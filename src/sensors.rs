//! Read-only sensors. No driver installation, ACPI temperature
//! guesses, or synthetic CPU temperature. Unsupported sensors stay NaN.
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
    sync::{mpsc, Arc, Mutex},
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
    provider_check: Option<Instant>,
    cpu: Option<CpuReader>,
}

struct CpuReader {
    latest: Arc<Mutex<Option<(Instant, f32)>>>,
    // Dropping the sender wakes an idle worker. The collector never joins WMI.
    _stop: mpsc::Sender<()>,
}

impl CpuReader {
    fn start(namespace: &'static str) -> std::io::Result<Self> {
        let (stop, stop_rx) = mpsc::channel();
        let latest = Arc::new(Mutex::new(None));
        let output = Arc::clone(&latest);
        std::thread::Builder::new()
            .name("cpu-temperature".into())
            .spawn(move || {
                let mut wmi = CpuWmi {
                    provider: None,
                    com_initialized: false,
                };
                let mut namespace = Some(namespace);
                let mut probe_after = Instant::now();
                loop {
                    if !matches!(stop_rx.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                        break;
                    }
                    if wmi.provider.is_none() && Instant::now() >= probe_after {
                        if let Some(name) = namespace.take().or_else(running_exporter) {
                            unsafe {
                                wmi.connect(name);
                            }
                        }
                        probe_after = Instant::now() + Duration::from_secs(60);
                    }
                    let value = unsafe { wmi.cpu_temperature() };
                    *output.lock().unwrap() = Some((Instant::now(), value));
                    let wait = if wmi.provider.is_some() {
                        Duration::from_secs(1)
                    } else {
                        probe_after.saturating_duration_since(Instant::now())
                    };
                    if !matches!(
                        stop_rx.recv_timeout(wait),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        break;
                    }
                }
            })?;
        Ok(Self {
            latest,
            _stop: stop,
        })
    }

    fn temperature(&self) -> f32 {
        self.latest
            .lock()
            .unwrap()
            .filter(|(at, _)| at.elapsed() <= Duration::from_secs(3))
            .map_or(f32::NAN, |(_, value)| value)
    }
}

impl Sensors {
    pub fn new() -> Self {
        let mut s = Self {
            adapters: Vec::new(),
            last_enumeration: Instant::now(),
            provider_check: None,
            cpu: None,
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
        let integrated = crate::cpu_sensor::temperature();
        if integrated.is_finite() {
            return [integrated, gpu];
        }
        if self.cpu.is_none()
            && self
                .provider_check
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.provider_check = Some(Instant::now());
            if let Some(namespace) = running_exporter() {
                self.cpu = CpuReader::start(namespace).ok();
            }
        }
        [
            self.cpu.as_ref().map_or(f32::NAN, CpuReader::temperature),
            gpu,
        ]
    }
}

struct CpuWmi {
    provider: Option<IWbemServices>,
    com_initialized: bool,
}

impl CpuWmi {
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
            let status = rows.Next(10, &mut item, &mut count);
            if status.is_err() {
                self.provider = None;
                return f32::NAN;
            }
            if count == 0 && status.0 == WBEM_S_TIMEDOUT.0 {
                continue;
            }
            if count == 0 {
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
impl Drop for CpuWmi {
    fn drop(&mut self) {
        unsafe {
            self.provider = None;
            if self.com_initialized {
                CoUninitialize();
            }
        }
    }
}

impl Drop for Sensors {
    fn drop(&mut self) {
        unsafe {
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
