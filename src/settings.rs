use std::{
    fs,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::System::Registry::*;
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[derive(Clone)]
pub struct Settings {
    pub range: usize,
    pub visible: [bool; 6],
    pub topmost: bool,
    pub frameless: bool,
    pub width: i32,
    pub height: i32,
    pub normal_width: i32,
    pub normal_height: i32,
    pub x: i32,
    pub y: i32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            range: 0,
            visible: [true; 6],
            topmost: false,
            frameless: false,
            width: 960,
            height: 480,
            normal_width: 960,
            normal_height: 480,
            x: i32::MIN,
            y: i32::MIN,
        }
    }
}
fn path() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
        .join("PerformanceGlass")
        .join("settings.txt")
}
impl Settings {
    pub fn load() -> Self {
        let mut s = Self::default();
        if let Ok(text) = fs::read_to_string(path()) {
            for line in text.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    match key {
                        "range" => s.range = value.parse::<usize>().unwrap_or(0).min(4),
                        "visible" => {
                            for (i, c) in value.chars().take(6).enumerate() {
                                s.visible[i] = c != '0';
                            }
                        }
                        "topmost" => s.topmost = value == "1",
                        "frameless" => s.frameless = value == "1",
                        "width" => s.width = value.parse::<i32>().unwrap_or(960).clamp(180, 7680),
                        "height" => s.height = value.parse::<i32>().unwrap_or(480).clamp(12, 4320),
                        "normal_width" => {
                            s.normal_width = value.parse::<i32>().unwrap_or(960).clamp(180, 7680)
                        }
                        "normal_height" => {
                            s.normal_height = value.parse::<i32>().unwrap_or(480).clamp(280, 4320)
                        }
                        "x" => s.x = value.parse().unwrap_or(i32::MIN),
                        "y" => s.y = value.parse().unwrap_or(i32::MIN),
                        _ => {}
                    }
                }
            }
        }
        s
    }
    pub fn save(&self) {
        let p = path();
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let visible: String = self
            .visible
            .iter()
            .map(|v| if *v { '1' } else { '0' })
            .collect();
        let content = format!(
            "range={}\nvisible={}\ntopmost={}\nframeless={}\nwidth={}\nheight={}\nnormal_width={}\nnormal_height={}\nx={}\ny={}\n",
            self.range, visible, self.topmost as u8, self.frameless as u8, self.width, self.height,self.normal_width,self.normal_height,self.x,self.y
        );
        // Write only on user changes and exit, never during sampling.
        let tmp = p.with_extension("tmp");
        if fs::write(&tmp, content).is_ok() {
            let _ = fs::rename(tmp, p);
        }
    }
}
pub fn startup_enabled() -> bool {
    let mut size = 0;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide("PerformanceGlass").as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut size,
        ) == 0
    }
}
pub fn set_startup(enabled: bool) -> Result<(), String> {
    unsafe {
        let mut key = null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        );
        if status != 0 {
            return Err(format!("自動起動の設定を開けません ({status})"));
        }
        let status = if enabled {
            match std::env::current_exe() {
                Ok(exe) => {
                    let command = wide(&format!("\"{}\" --tray", exe.display()));
                    RegSetValueExW(
                        key,
                        wide("PerformanceGlass").as_ptr(),
                        0,
                        REG_SZ,
                        command.as_ptr() as _,
                        (command.len() * 2) as u32,
                    )
                }
                Err(e) => {
                    RegCloseKey(key);
                    return Err(e.to_string());
                }
            }
        } else {
            RegDeleteValueW(key, wide("PerformanceGlass").as_ptr())
        };
        RegCloseKey(key);
        if status == 0 || (!enabled && status == 2) {
            Ok(())
        } else {
            Err(format!("自動起動を変更できません ({status})"))
        }
    }
}
