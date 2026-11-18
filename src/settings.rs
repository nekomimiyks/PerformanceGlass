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
    pub language: Option<crate::i18n::Language>,
    pub taskbar_metric: Option<usize>,
    pub cpu_permission: crate::cpu_sensor::Permission,
    pub range: usize,
    pub visible: [bool; 6],
    pub topmost: bool,
    pub tray_enabled: bool,
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
            language: None,
            taskbar_metric: Some(0),
            cpu_permission: crate::cpu_sensor::Permission::Ask,
            range: 0,
            visible: [true; 6],
            topmost: false,
            tray_enabled: false,
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
        Self::from_text(&fs::read_to_string(path()).unwrap_or_default())
    }
    fn from_text(text: &str) -> Self {
        let mut s = Self::default();
        {
            for line in text.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    match key {
                        "language" => s.language = crate::i18n::Language::from_tag(value),
                        "cpu_permission" => {
                            s.cpu_permission = crate::cpu_sensor::Permission::parse(value)
                        }
                        "taskbar_metric" => {
                            s.taskbar_metric = if value == "off" {
                                None
                            } else {
                                Some(
                                    crate::taskbar_graph::METRICS
                                        .iter()
                                        .position(|m| m.key == value)
                                        .unwrap_or(0),
                                )
                            };
                        }
                        "range" => s.range = value.parse::<usize>().unwrap_or(0).min(4),
                        "visible" => {
                            for (i, c) in value.chars().take(6).enumerate() {
                                s.visible[i] = c != '0';
                            }
                        }
                        "topmost" => s.topmost = value == "1",
                        "tray_enabled" => s.tray_enabled = value == "1",
                        "frameless" => s.frameless = value == "1",
                        "width" => s.width = value.parse::<i32>().unwrap_or(960).clamp(1, 7680),
                        "height" => s.height = value.parse::<i32>().unwrap_or(480).clamp(12, 4320),
                        "normal_width" => {
                            s.normal_width = value.parse::<i32>().unwrap_or(960).clamp(1, 7680)
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
        let content = self.to_text();
        // Write only on user changes and exit, never during sampling.
        let tmp = p.with_extension("tmp");
        if fs::write(&tmp, content).is_ok() {
            let _ = fs::rename(tmp, p);
        }
    }
    fn to_text(&self) -> String {
        let visible: String = self
            .visible
            .iter()
            .map(|v| if *v { '1' } else { '0' })
            .collect();
        format!(
            "language={}\nrange={}\nvisible={}\ntopmost={}\nframeless={}\nwidth={}\nheight={}\nnormal_width={}\nnormal_height={}\nx={}\ny={}\ntaskbar_metric={}\ncpu_permission={}\ntray_enabled={}\n",
            self.language.map(|v| v.tag()).unwrap_or("auto"), self.range, visible, self.topmost as u8, self.frameless as u8, self.width, self.height,self.normal_width,self.normal_height,self.x,self.y,
            self.taskbar_metric.map_or("off", |index| crate::taskbar_graph::METRICS[index].key),
            self.cpu_permission.key(), self.tray_enabled as u8
        )
    }
}
pub fn startup_enabled() -> bool {
    if crate::packaged_startup::is_packaged() {
        return crate::packaged_startup::enabled();
    }
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
pub fn set_startup(
    enabled: bool,
    owner: windows_sys::Win32::Foundation::HWND,
) -> Result<(), String> {
    if crate::packaged_startup::is_packaged() {
        return crate::packaged_startup::set(enabled, owner);
    }
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
            return Err(format!(
                "{} ({status})",
                crate::i18n::tr("自動起動の設定を開けません")
            ));
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
            Err(format!(
                "{} ({status})",
                crate::i18n::tr("自動起動を変更できません")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_width_survives_roundtrip_at_high_dpi() {
        let settings = Settings {
            width: 70, // 280 physical pixels at 400% scaling.
            normal_width: 140,
            ..Settings::default()
        };
        let restored = Settings::from_text(&settings.to_text());
        assert_eq!(restored.width, 70);
        assert_eq!(restored.normal_width, 140);
    }
    #[test]
    fn taskbar_choice_position_and_sensor_permission_survive_restart() {
        use crate::cpu_sensor::Permission;
        for (metric, permission) in [(Some(15), Permission::Allow), (None, Permission::Deny)] {
            let settings = Settings {
                x: -450,
                y: 180,
                taskbar_metric: metric,
                cpu_permission: permission,
                tray_enabled: true,
                ..Settings::default()
            };
            let loaded = Settings::from_text(&settings.to_text());
            assert_eq!((loaded.x, loaded.y), (-450, 180));
            assert_eq!(loaded.taskbar_metric, metric);
            assert_eq!(loaded.cpu_permission, permission);
            assert!(loaded.tray_enabled);
        }
        let previous = Settings::from_text("width=300\n");
        assert_eq!(previous.taskbar_metric, Some(0));
        assert_eq!(previous.cpu_permission, Permission::Ask);
        assert!(!previous.tray_enabled);
    }
    #[test]
    fn language_setting_roundtrip_and_old_settings() {
        for preference in std::iter::once(None).chain(crate::i18n::LANGUAGES.map(Some)) {
            let settings = Settings {
                language: preference,
                range: 3,
                topmost: true,
                ..Settings::default()
            };
            let loaded = Settings::from_text(&settings.to_text());
            assert_eq!(loaded.language, preference);
            assert_eq!(loaded.range, 3);
            assert!(loaded.topmost);
        }
        assert_eq!(Settings::from_text("range=2\n").language, None);
        assert_eq!(Settings::from_text("language=invalid\n").language, None);
    }
}
