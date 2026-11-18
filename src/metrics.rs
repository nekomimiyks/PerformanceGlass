use crate::history::Sample;
use std::{
    collections::HashMap,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    NetworkManagement::IpHelper::*,
    System::{Performance::*, Registry::*, SystemInformation::*},
};

const PDH_FMT_NOCAP100: u32 = 0x800;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn now() -> f64 {
    static CLOCK: OnceLock<(Instant, f64)> = OnceLock::new();
    let (origin, epoch) = CLOCK.get_or_init(|| {
        (
            Instant::now(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
        )
    });
    epoch + origin.elapsed().as_secs_f64()
}
unsafe fn string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
}
pub fn registry_dword(root: HKEY, path: &str, key: &str) -> Option<u32> {
    let mut value = 0_u32;
    let mut size = 4;
    let result = unsafe {
        RegGetValueW(
            root,
            wide(path).as_ptr(),
            wide(key).as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut value as *mut _ as _,
            &mut size,
        )
    };
    (result == 0).then_some(value)
}

// PDH buffers require 8-byte alignment. Reuse allocation across samples.
struct Counter {
    handle: isize,
    buffer: Vec<u64>,
    status: u32,
}
impl Counter {
    unsafe fn new(query: isize, path: &str) -> Self {
        let mut handle = 0;
        let status = if query != 0 {
            PdhAddEnglishCounterW(query, wide(path).as_ptr(), 0, &mut handle)
        } else {
            u32::MAX
        };
        Self {
            handle,
            buffer: Vec::new(),
            status,
        }
    }
    unsafe fn scalar(&self) -> f32 {
        let mut value: PDH_FMT_COUNTERVALUE = zeroed();
        if self.handle == 0
            || PdhGetFormattedCounterValue(
                self.handle,
                PDH_FMT_DOUBLE | PDH_FMT_NOCAP100,
                null_mut(),
                &mut value,
            ) != 0
            || value.CStatus > 1
        {
            return f32::NAN;
        }
        value.Anonymous.doubleValue as f32
    }
    unsafe fn array(&mut self, mut visit: impl FnMut(&str, f32)) -> bool {
        if self.handle == 0 {
            return false;
        }
        let mut bytes = (self.buffer.len() * 8) as u32;
        let had_buffer = bytes != 0;
        let mut count = 0;
        let flags = PDH_FMT_DOUBLE | PDH_FMT_NOCAP100;
        let mut status = PdhGetFormattedCounterArrayW(
            self.handle,
            flags,
            &mut bytes,
            &mut count,
            if bytes == 0 {
                null_mut()
            } else {
                self.buffer.as_mut_ptr() as _
            },
        );
        if had_buffer && matches!(status, PDH_MORE_DATA | PDH_INVALID_ARGUMENT) {
            // An undersized nonempty buffer does not return a reliable size.
            bytes = 0;
            status = PdhGetFormattedCounterArrayW(
                self.handle,
                flags,
                &mut bytes,
                &mut count,
                null_mut(),
            );
        }
        if status == PDH_MORE_DATA && bytes > 0 {
            self.buffer.resize((bytes as usize).div_ceil(8), 0);
            status = PdhGetFormattedCounterArrayW(
                self.handle,
                flags,
                &mut bytes,
                &mut count,
                self.buffer.as_mut_ptr() as _,
            );
        }
        if status != 0 {
            return false;
        }
        if count == 0 {
            return true;
        }
        for item in std::slice::from_raw_parts(
            self.buffer.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
            count as usize,
        ) {
            if item.FmtValue.CStatus <= 1 {
                let v = item.FmtValue.Anonymous.doubleValue as f32;
                if v.is_finite() {
                    visit(&string(item.szName), v);
                }
            }
        }
        true
    }
}

pub struct Collector {
    query: isize,
    cpu: Counter,
    speed: Counter,
    disk: Counter,
    disk_read: Counter,
    disk_write: Counter,
    gpu: Counter,
    gpu_memory: Counter,
    sensors: crate::sensors::Sensors,
    base_mhz: f32,
    previous: HashMap<u64, (u64, u64, Instant)>,
    engines: HashMap<String, f32>,
    retry_at: Instant,
}
impl Collector {
    pub fn diagnostics(&self) -> String {
        format!(
            "query={},cpu=0x{:08x},speed=0x{:08x},disk=0x{:08x},gpu=0x{:08x},base_mhz={}",
            self.query,
            self.cpu.status,
            self.speed.status,
            self.disk.status,
            self.gpu.status,
            self.base_mhz
        )
    }
    pub fn new() -> Self {
        unsafe {
            let mut query = 0;
            PdhOpenQueryW(null(), 0, &mut query);
            let mut c = Self {
                query,
                cpu: Counter::new(query, r"\Processor Information(_Total)\% Processor Time"),
                speed: Counter::new(
                    query,
                    r"\Processor Information(_Total)\% Processor Performance",
                ),
                disk: Counter::new(query, r"\PhysicalDisk(*)\% Idle Time"),
                disk_read: Counter::new(query, r"\PhysicalDisk(_Total)\Disk Read Bytes/sec"),
                disk_write: Counter::new(query, r"\PhysicalDisk(_Total)\Disk Write Bytes/sec"),
                gpu: Counter::new(query, r"\GPU Engine(*)\Utilization Percentage"),
                gpu_memory: Counter::new(query, r"\GPU Adapter Memory(*)\Dedicated Usage"),
                sensors: crate::sensors::Sensors::new(),
                base_mhz: registry_dword(
                    HKEY_LOCAL_MACHINE,
                    r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
                    "~MHz",
                )
                .unwrap_or(0) as f32,
                previous: HashMap::new(),
                engines: HashMap::new(),
                retry_at: Instant::now(),
            };
            if c.cpu.handle == 0 {
                c.cpu = Counter::new(query, r"\Processor(_Total)\% Processor Time");
            }
            if query != 0 {
                PdhCollectQueryData(query);
            }
            c.network(&mut Sample::default());
            c
        }
    }
    pub fn sample(&mut self) -> Sample {
        let mut s = Sample {
            time: now(),
            ..Sample::default()
        };
        unsafe {
            // Retry counters absent during startup (e.g. display-driver reset), at most once a minute.
            if self.retry_at.elapsed().as_secs() >= 60 {
                if self.disk.handle == 0 {
                    self.disk = Counter::new(self.query, r"\PhysicalDisk(*)\% Idle Time");
                }
                if self.gpu.handle == 0 {
                    self.gpu = Counter::new(self.query, r"\GPU Engine(*)\Utilization Percentage");
                }
                if self.gpu_memory.handle == 0 {
                    self.gpu_memory =
                        Counter::new(self.query, r"\GPU Adapter Memory(*)\Dedicated Usage");
                }
                if self.disk_read.handle == 0 {
                    self.disk_read =
                        Counter::new(self.query, r"\PhysicalDisk(_Total)\Disk Read Bytes/sec");
                }
                if self.disk_write.handle == 0 {
                    self.disk_write =
                        Counter::new(self.query, r"\PhysicalDisk(_Total)\Disk Write Bytes/sec");
                }
                self.retry_at = Instant::now();
            }
            if self.query != 0 && PdhCollectQueryData(self.query) == 0 {
                s.values[0] = self.cpu.scalar().clamp(0., 100.);
                let performance = self.speed.scalar();
                if self.base_mhz > 0. {
                    s.ghz = performance * self.base_mhz / 100_000.;
                }
                let mut disk = f32::NAN;
                self.disk.array(|name, idle| {
                    if name != "_Total" {
                        let busy = (100. - idle).clamp(0., 100.);
                        disk = if disk.is_nan() { busy } else { disk.max(busy) };
                    }
                });
                s.values[2] = disk;
                s.disk_io = [
                    self.disk_read.scalar() / 1_000_000.,
                    self.disk_write.scalar() / 1_000_000.,
                ];
                self.engines.clear();
                let engines = &mut self.engines;
                self.gpu.array(|name, usage| {
                    // Sum processes sharing the same adapter + physical engine, then take
                    // the busiest engine. Summing different engines overstates GPU use.
                    if let Some(pos) = name.find("luid_") {
                        *engines.entry(name[pos..].to_owned()).or_default() += usage.max(0.);
                    }
                });
                s.values[5] = self
                    .engines
                    .values()
                    .copied()
                    .reduce(f32::max)
                    .unwrap_or(f32::NAN)
                    .clamp(0., 100.);
                // Adapter-wide instances count each physical GPU once. Process
                // memory counters can double-count shared resources.
                let mut bytes = 0.;
                let mut present = false;
                self.gpu_memory.array(|name, value| {
                    if name != "_Total" && value >= 0. {
                        bytes += value;
                        present = true;
                    }
                });
                if present {
                    s.gpu_memory = bytes / 1_073_741_824.;
                }
            }
            let mut mem: MEMORYSTATUSEX = zeroed();
            mem.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut mem) != 0 && mem.ullTotalPhys > 0 {
                s.memory_total = mem.ullTotalPhys as f32 / 1_073_741_824.;
                s.memory_used = (mem.ullTotalPhys - mem.ullAvailPhys) as f32 / 1_073_741_824.;
                s.values[1] = 100. * s.memory_used / s.memory_total;
            }
            self.network(&mut s);
            s.temperatures = self.sensors.read();
        }
        s
    }
    unsafe fn network(&mut self, s: &mut Sample) {
        let mut table = null_mut();
        if GetIfTable2(&mut table) != 0 || table.is_null() {
            return;
        }
        let rows =
            std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        let time = Instant::now();
        let mut present = [false; 2];
        let mut ready = [false; 2];
        let mut traffic = [[0_f32; 2]; 2];
        let mut seen = Vec::with_capacity(rows.len());
        for row in rows {
            let group = match row.Type {
                6 => 0,
                71 => 1,
                _ => continue,
            };
            // HardwareInterface bit excludes VPN/tunnels and Hyper-V virtual adapters.
            if row.InterfaceAndOperStatusFlags._bitfield & 1 == 0 {
                continue;
            }
            let id = row.InterfaceLuid.Value;
            seen.push(id);
            present[group] = true;
            s.connected[group] |= row.OperStatus == 1;
            if let Some((rx, tx, at)) = self
                .previous
                .insert(id, (row.InOctets, row.OutOctets, time))
            {
                let seconds = time.duration_since(at).as_secs_f32();
                if seconds > 0. && row.InOctets >= rx && row.OutOctets >= tx {
                    traffic[group][0] += (row.InOctets - rx) as f32 * 8. / seconds / 1_000_000.;
                    traffic[group][1] += (row.OutOctets - tx) as f32 * 8. / seconds / 1_000_000.;
                    ready[group] = true;
                }
            }
        }
        FreeMibTable(table as _);
        self.previous.retain(|id, _| seen.contains(id));
        for group in 0..2 {
            if present[group] && ready[group] {
                s.traffic[group] = traffic[group];
                // Disconnected interfaces have no plotted line; current card shows disconnected.
                if s.connected[group] {
                    s.values[3 + group] = traffic[group][0] + traffic[group][1];
                }
            }
        }
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        if self.query != 0 {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_windows_counters() {
        let mut c = Collector::new();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let s = c.sample();
        assert!(s.values[0].is_finite(), "CPU counter unavailable: {s:?}");
        assert!((0.0..=100.0).contains(&s.values[0]));
        assert!(s.memory_total > 0. && s.values[1].is_finite());
    }
}
