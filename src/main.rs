#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod history;
mod label_layout;
mod layout;
mod metrics;
mod packaged_startup;
mod sensors;
mod settings;
mod ui;
mod window_style;

use history::History;
use std::{
    fs::File,
    io::Write,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|s| s == "--probe") {
        let count = args
            .get(i + 1)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10)
            .clamp(2, 3600);
        let output = args
            .get(i + 2)
            .cloned()
            .unwrap_or_else(|| "probe.csv".into());
        probe(count, &output);
        return;
    }
    let data = Arc::new(Mutex::new(History::new()));
    let demo = args.iter().any(|s| s == "--demo");
    if demo {
        let end = metrics::now();
        let mut h = data.lock().unwrap();
        for n in 0..=43200 {
            let t = n as f32;
            let wave = |speed: f32| (t * speed).sin().abs();
            h.push(history::Sample {
                time: end - 43200. + n as f64,
                values: [
                    12. + wave(0.033) * 18. + if n % 103 < 9 { 35. } else { 0. },
                    42. + wave(0.002) * 3.,
                    wave(0.067).powf(16.) * 38.,
                    wave(0.026) * 3.6,
                    wave(0.013) * 1.3,
                    8. + wave(0.018).powf(4.) * 45.,
                ],
                ghz: 3.82,
                memory_used: 13.4,
                memory_total: 32.,
                gpu_memory: 4.2,
                disk_io: [25.8, 6.4],
                temperatures: [62., 54.],
                traffic: [[2.8, 0.8], [0.9, 0.4]],
                connected: [true, true],
            });
        }
    }
    let stop = Arc::new((Mutex::new(false), Condvar::new()));
    let worker_data = data.clone();
    let worker_stop = stop.clone();
    // All PDH work runs on this thread so collection never blocks window messages.
    let worker = std::thread::spawn(move || {
        if demo {
            return;
        }
        let mut collector = metrics::Collector::new();
        let mut deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let (lock, cv) = &*worker_stop;
            let stopped = lock.lock().unwrap();
            let (stopped, _) = cv
                .wait_timeout_while(
                    stopped,
                    deadline.saturating_duration_since(Instant::now()),
                    |v| !*v,
                )
                .unwrap();
            if *stopped {
                break;
            }
            drop(stopped);
            let sample = collector.sample();
            worker_data.lock().unwrap().push(sample);
            deadline += Duration::from_secs(1);
            if deadline < Instant::now() {
                deadline = Instant::now() + Duration::from_secs(1);
            }
        }
    });
    let result = ui::run(data, &args);
    *stop.0.lock().unwrap() = true;
    stop.1.notify_one();
    let _ = worker.join();
    if let Err(error) = result {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
                std::ptr::null_mut(),
                ui::wide(&error).as_ptr(),
                ui::wide("Performance Glass").as_ptr(),
                windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}

fn probe(count: usize, path: &str) {
    let Ok(mut file) = File::create(path) else {
        return;
    };
    let mut collector = metrics::Collector::new();
    let _ = writeln!(file, "# {}", collector.diagnostics());
    let start = Instant::now();
    let cpu_start = process_cpu();
    let _ = writeln!(file,"time,cpu_percent,memory_percent,disk_percent,ethernet_mbps,wifi_mbps,gpu_percent,cpu_ghz,memory_used_gib,memory_total_gib,disk_read_mbs,disk_write_mbs,cpu_c,gpu_c,collection_ms,working_set_mib,gpu_memory_gib");
    for _ in 0..count {
        std::thread::sleep(Duration::from_secs(1));
        let at = Instant::now();
        let s = collector.sample();
        let ms = at.elapsed().as_secs_f64() * 1000.;
        let _ = writeln!(
            file,
            "{:.3},{:.3},{:.3},{:.3},{:.5},{:.5},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.1},{:.1},{:.3},{:.3},{:.3}",
            s.time,
            s.values[0],
            s.values[1],
            s.values[2],
            s.values[3],
            s.values[4],
            s.values[5],
            s.ghz,
            s.memory_used,
            s.memory_total,
            s.disk_io[0],s.disk_io[1],s.temperatures[0],s.temperatures[1],
            ms,
            working_set(), s.gpu_memory
        );
    }
    let elapsed = start.elapsed().as_secs_f64();
    let cpu = process_cpu() - cpu_start;
    let _ = writeln!(
        file,
        "# elapsed_s={elapsed:.3},process_cpu_s={cpu:.5},one_core_percent={:.4}",
        100. * cpu / elapsed
    );
}
pub fn process_cpu() -> f64 {
    use windows_sys::Win32::{Foundation::FILETIME, System::Threading::*};
    unsafe {
        let mut c: FILETIME = std::mem::zeroed();
        let mut e = c;
        let mut k = c;
        let mut u = c;
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) == 0 {
            return 0.;
        }
        let ticks = |v: FILETIME| ((v.dwHighDateTime as u64) << 32) | v.dwLowDateTime as u64;
        (ticks(k) + ticks(u)) as f64 / 10_000_000.
    }
}
pub fn working_set() -> f64 {
    use windows_sys::Win32::System::{ProcessStatus::*, Threading::*};
    unsafe {
        let mut m: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut m,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ) == 0
        {
            return 0.;
        }
        m.WorkingSetSize as f64 / 1_048_576.
    }
}
