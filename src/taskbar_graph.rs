use crate::{
    history::{History, Sample},
    i18n::tr,
};
use std::{mem::size_of, ptr::null_mut};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HWND,
        System::Com::*,
        UI::{
            Shell::{ITaskbarList3, TaskbarList},
            WindowsAndMessaging::HICON as ShellIcon,
        },
    },
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO},
};

const WIDTH: usize = 32;
const INSET: usize = 3;
const POINTS: usize = WIDTH - 2 * INSET;
pub const MENU_BASE: usize = 500;

pub struct Taskbar(Option<ITaskbarList3>);

impl Taskbar {
    pub unsafe fn new() -> Option<Self> {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().ok()?;
        let api = CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER)
            .ok()
            .filter(|api| api.HrInit().is_ok());
        Some(Self(api))
    }

    pub unsafe fn set_graph(&self, hwnd: HWND, icon: HICON, metric: Option<Metric>) {
        if let Some(api) = &self.0 {
            let label = metric.map(Metric::label).unwrap_or_default();
            let label = crate::ui::wide(&label);
            let _ = api.SetOverlayIcon(hwnd, ShellIcon(icon), PCWSTR(label.as_ptr()));
        }
    }
}

impl Drop for Taskbar {
    fn drop(&mut self) {
        self.0.take();
        unsafe { CoUninitialize() };
    }
}

#[derive(Clone, Copy)]
pub struct Metric {
    pub key: &'static str,
    group: &'static str,
    field: &'static str,
    color: usize,
    percent: bool,
    read: fn(&Sample) -> f32,
}

impl Metric {
    pub fn label(self) -> String {
        if self.field.is_empty() {
            tr(self.group).to_owned()
        } else {
            format!("{} · {}", tr(self.group), tr(self.field))
        }
    }

    pub fn available(self, sample: &Sample) -> bool {
        (self.read)(sample).is_finite()
    }

    pub fn color(self) -> usize {
        self.color
    }
}

macro_rules! metric {
    ($key:literal, $group:literal, $field:literal, $color:literal, $percent:literal, $read:expr) => {
        Metric {
            key: $key,
            group: $group,
            field: $field,
            color: $color,
            percent: $percent,
            read: $read,
        }
    };
}

pub const METRICS: [Metric; 17] = [
    metric!("cpu", "CPU", "使用率", 0, true, |s| s.values[0]),
    metric!("cpu_clock", "CPU", "速度", 0, false, |s| s.ghz),
    metric!("cpu_temp", "CPU", "温度", 0, false, |s| s.temperatures[0]),
    metric!("memory", "メモリ", "使用率", 1, true, |s| s.values[1]),
    metric!(
        "memory_used",
        "メモリ",
        "使用量",
        1,
        false,
        |s| if s.memory_total > 0. {
            s.memory_used
        } else {
            f32::NAN
        }
    ),
    metric!("disk", "ディスク", "稼働率", 2, true, |s| s.values
        [2]),
    metric!("disk_read", "ディスク", "読み込み", 2, false, |s| s
        .disk_io[0]),
    metric!(
        "disk_write",
        "ディスク",
        "書き込み",
        2,
        false,
        |s| s.disk_io[1]
    ),
    metric!("ethernet_rx", "Ethernet", "受信", 3, false, |s| s.traffic
        [0][0]),
    metric!("ethernet_tx", "Ethernet", "送信", 3, false, |s| s.traffic
        [0][1]),
    metric!("ethernet", "Ethernet", "送受信合計", 3, false, |s| s
        .values[3]),
    metric!("wifi_rx", "Wi-Fi", "受信", 4, false, |s| s.traffic[1][0]),
    metric!("wifi_tx", "Wi-Fi", "送信", 4, false, |s| s.traffic[1][1]),
    metric!("wifi", "Wi-Fi", "送受信合計", 4, false, |s| s.values
        [4]),
    metric!("gpu", "GPU", "使用率", 5, true, |s| s.values[5]),
    metric!("gpu_memory", "GPUメモリ", "", 5, false, |s| s.gpu_memory),
    metric!("gpu_temp", "GPU", "温度", 5, false, |s| s.temperatures[1]),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Frame([u8; POINTS]);

impl Frame {
    pub fn from_history(history: &History, metric: Metric) -> Option<Self> {
        let latest = history.samples.back()?;
        if !metric.available(latest) {
            return None;
        }
        let mut values = [f32::NAN; POINTS];
        for sample in history.samples.iter().rev() {
            let age = (latest.time - sample.time).round() as usize;
            if age >= POINTS {
                break;
            }
            let slot = &mut values[POINTS - 1 - age];
            if slot.is_nan() {
                *slot = (metric.read)(sample);
            }
        }
        let ceiling = if metric.percent {
            100.
        } else {
            let peak = values.iter().copied().fold(0f32, f32::max);
            if peak > 0. {
                2f32.powf(peak.log2().ceil())
            } else {
                1.
            }
        };
        Some(Self(values.map(|v| {
            if v.is_finite() {
                (INSET as f32 + (POINTS - 1) as f32 * (1. - (v / ceiling).clamp(0., 1.))).round()
                    as u8
            } else {
                u8::MAX
            }
        })))
    }

    pub unsafe fn create_icon(self, bg: u32, line: u32) -> HICON {
        fn argb(color: u32) -> u32 {
            0xff00_0000 | ((color & 0xff) << 16) | (color & 0xff00) | ((color >> 16) & 0xff)
        }
        let mut bitmap: BITMAPINFO = std::mem::zeroed();
        bitmap.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bitmap.bmiHeader.biWidth = WIDTH as i32;
        bitmap.bmiHeader.biHeight = -(WIDTH as i32);
        bitmap.bmiHeader.biPlanes = 1;
        bitmap.bmiHeader.biBitCount = 32;
        let mut bits = null_mut();
        let color = CreateDIBSection(
            null_mut(),
            &bitmap,
            DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        );
        if color.is_null() {
            return null_mut();
        }
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), WIDTH * WIDTH);
        pixels.fill(argb(bg));
        let line = argb(line);
        for (index, y) in self
            .0
            .into_iter()
            .enumerate()
            .filter(|(_, y)| *y != u8::MAX)
        {
            let x = index + INSET;
            let last = if index > 0 && self.0[index - 1] != u8::MAX {
                self.0[index - 1]
            } else {
                y
            };
            for row in last.min(y) as usize..=last.max(y) as usize {
                pixels[row * WIDTH + x] = line;
                pixels[row * WIDTH + x + 1] = line;
            }
            pixels[(y as usize + 1) * WIDTH + x] = line;
        }
        let mask_bits = [0u8; WIDTH * WIDTH / 8];
        let mask = CreateBitmap(WIDTH as i32, WIDTH as i32, 1, 1, mask_bits.as_ptr().cast());
        let icon = if mask.is_null() {
            null_mut()
        } else {
            let icon = CreateIconIndirect(&ICONINFO {
                fIcon: 1,
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: color,
            });
            DeleteObject(mask);
            icon
        };
        DeleteObject(color);
        icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_graph_uses_fixed_scale_and_leaves_gaps() {
        let mut history = History::new();
        for (time, cpu) in [(1., 0.), (3., 100.)] {
            let mut sample = Sample {
                time,
                ..Sample::default()
            };
            sample.values[0] = cpu;
            history.push(sample);
        }
        let Frame(rows) = Frame::from_history(&history, METRICS[0]).unwrap();
        assert_eq!(
            &rows[POINTS - 3..],
            &[(WIDTH - INSET - 1) as u8, u8::MAX, INSET as u8]
        );
        assert!(Frame::from_history(&history, METRICS[2]).is_none());
    }
}
