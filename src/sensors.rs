//! Read-only sensors. No driver installation, subprocess polling, ACPI temperature
//! guesses, or synthetic CPU temperature. Unsupported sensors stay NaN.
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
    time::{Duration, Instant},
};
use windows::{
    core::{w, BSTR, VARIANT},
    Win32::System::{Com::*, Rpc::*, Wmi::*},
};
use windows_sys::Wdk::Graphics::Direct3D::*;
use windows_sys::Win32::{Foundation::*, System::Diagnostics::ToolHelp::*};

pub struct Sensors {
    adapters: Vec<D3DKMT_ADAPTERINFO>,
    last_enumeration: Instant,
    provider: Option<IWbemServices>,
    com_initialized: bool,
    provider_check: Option<Instant>,
    cpu_check: Option<Instant>,
    cpu_value: f32,
}
impl Sensors {
    pub fn new() -> Self {
        let mut s = Self {
            adapters: Vec::new(),
            last_enumeration: Instant::now(),
            provider: None,
            com_initialized: false,
            provider_check: None,
            cpu_check: None,
            cpu_value: f32::NAN,
        };
        s.enumerate();
        s
    }
    fn enumerate(&mut self) {
        unsafe {
            for a in self.adapters.drain(..) {
                D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
                    hAdapter: a.hAdapter,
                });
            }
            let mut query = D3DKMT_ENUMADAPTERS2 {
                NumAdapters: 0,
                pAdapters: null_mut(),
            };
            if D3DKMTEnumAdapters2(&mut query) >= 0
                && query.NumAdapters > 0
                && query.NumAdapters <= 64
            {
                let mut adapters = vec![zeroed(); query.NumAdapters as usize];
                query.pAdapters = adapters.as_mut_ptr();
                if D3DKMTEnumAdapters2(&mut query) >= 0 {
                    adapters.truncate(query.NumAdapters as usize);
                    self.adapters = adapters;
                }
            }
        }
        self.last_enumeration = Instant::now();
    }
    pub fn read(&mut self) -> [f32; 2] {
        if self.last_enumeration.elapsed() >= Duration::from_secs(60) {
            self.enumerate();
        }
        let mut gpu = f32::NAN;
        unsafe {
            for a in &self.adapters {
                let mut value: D3DKMT_ADAPTER_PERFDATA = zeroed();
                let mut query = D3DKMT_QUERYADAPTERINFO {
                    hAdapter: a.hAdapter,
                    Type: KMTQAITYPE_ADAPTERPERFDATA,
                    pPrivateDriverData: &mut value as *mut _ as _,
                    PrivateDriverDataSize: size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
                };
                if D3DKMTQueryAdapterInfo(&mut query) >= 0
                    && value.Temperature > 0
                    && value.Temperature <= 1500
                {
                    let c = value.Temperature as f32 / 10.;
                    gpu = if gpu.is_nan() { c } else { gpu.max(c) };
                }
            }
        }
        // Only probe WMI if a real sensor exporter is already running. This
        // avoids starting the WMI provider/service on the normal lightweight path.
        if self.provider.is_none()
            && self
                .provider_check
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.provider_check = Some(Instant::now());
            if let Some(namespace) = running_exporter() {
                unsafe {
                    self.connect(namespace);
                }
            }
        }
        if self.provider.is_some()
            && self
                .cpu_check
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
        {
            self.cpu_check = Some(Instant::now());
            self.cpu_value = unsafe { self.cpu_temperature() };
        }
        if self.provider.is_none() {
            self.cpu_value = f32::NAN;
        }
        [self.cpu_value, gpu]
    }
    unsafe fn connect(&mut self, namespace: &str) {
        if !self.com_initialized {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                return;
            }
            self.com_initialized = true;
            let _ = CoInitializeSecurity(
                None,
                -1,
                None,
                None,
                RPC_C_AUTHN_LEVEL_DEFAULT,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
                None,
            );
        }
        let Ok(locator) =
            CoCreateInstance::<_, IWbemLocator>(&WbemLocator, None, CLSCTX_INPROC_SERVER)
        else {
            return;
        };
        let Ok(services) = locator.ConnectServer(
            &BSTR::from(namespace),
            &BSTR::new(),
            &BSTR::new(),
            &BSTR::new(),
            WBEM_FLAG_CONNECT_USE_MAX_WAIT.0,
            &BSTR::new(),
            None,
        ) else {
            return;
        };
        if CoSetProxyBlanket(
            &services,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
        .is_ok()
        {
            self.provider = Some(services);
        }
    }
    unsafe fn cpu_temperature(&mut self) -> f32 {
        let Some(svc) = &self.provider else {
            return f32::NAN;
        };
        let query=BSTR::from("SELECT Name, Value FROM Sensor WHERE SensorType = 'Temperature' AND (Identifier LIKE '/amdcpu/%' OR Identifier LIKE '/intelcpu/%')");
        let Ok(rows) = svc.ExecQuery(
            &BSTR::from("WQL"),
            &query,
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        ) else {
            self.provider = None;
            return f32::NAN;
        };
        let mut package = f32::NAN;
        let mut hottest = f32::NAN;
        let deadline = Instant::now() + Duration::from_millis(100);
        for _ in 0..128 {
            if Instant::now() >= deadline {
                break;
            }
            let mut item = [None];
            let mut count = 0;
            if rows.Next(10, &mut item, &mut count).is_err() || count == 0 {
                break;
            }
            let Some(item) = item[0].take() else {
                continue;
            };
            let mut value = VARIANT::default();
            let mut name = VARIANT::default();
            if item.Get(w!("Value"), 0, &mut value, None, None).is_err() {
                continue;
            }
            let Ok(c) = f64::try_from(&value) else {
                continue;
            };
            if !(0.0..=150.0).contains(&c) {
                continue;
            }
            let c = c as f32;
            hottest = if hottest.is_nan() { c } else { hottest.max(c) };
            if item.Get(w!("Name"), 0, &mut name, None, None).is_ok() {
                let name = BSTR::try_from(&name)
                    .map(|n| n.to_string().to_lowercase())
                    .unwrap_or_default();
                if name.contains("package") || name.contains("tctl") || name.contains("tdie") {
                    package = if package.is_nan() { c } else { package.max(c) };
                }
            }
        }
        if package.is_finite() {
            package
        } else {
            hottest
        }
    }
}
impl Drop for Sensors {
    fn drop(&mut self) {
        unsafe {
            self.provider = None;
            if self.com_initialized {
                CoUninitialize();
            }
            for a in self.adapters.drain(..) {
                D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER {
                    hAdapter: a.hAdapter,
                });
            }
        }
    }
}
fn running_exporter() -> Option<&'static str> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut result = None;
        let mut ok = Process32FirstW(snapshot, &mut entry);
        while ok != 0 {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("LibreHardwareMonitor.exe") {
                result = Some(r"ROOT\LibreHardwareMonitor");
                break;
            }
            if name.eq_ignore_ascii_case("OpenHardwareMonitor.exe") {
                result = Some(r"ROOT\OpenHardwareMonitor");
                break;
            }
            ok = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        result
    }
}
