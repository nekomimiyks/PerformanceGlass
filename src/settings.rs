use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::atomic::{AtomicUsize, Ordering},
};
use windows_sys::Win32::System::Registry::*;
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const BACKUP_HEADER: &str = "Performance Glass settings backup v1";
const MAX_BACKUP_SIZE: u64 = 16 * 1024;
static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    let mut temp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(file) => break file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                temp.set_extension(format!(
                    "tmp-{}-{}",
                    std::process::id(),
                    NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
                ));
            }
            Err(error) => return Err(error),
        }
    };
    let written = file
        .write_all(content.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = written.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[derive(Clone)]
pub struct Settings {
    pub language: Option<crate::i18n::Language>,
    pub taskbar_metric: Option<usize>,
    pub cpu_permission: crate::cpu_sensor::Permission,
    pub cpu_temperature: bool,
    pub cpu_helper_enabled: bool,
    pub auto_update: bool,
    pub last_update_notice: Option<crate::updates::Version>,
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
    pub window_dpi: Option<u32>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: None,
            taskbar_metric: Some(0),
            cpu_permission: crate::cpu_sensor::Permission::Ask,
            cpu_temperature: true,
            cpu_helper_enabled: false,
            auto_update: false,
            last_update_notice: None,
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
            window_dpi: None,
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
                        "cpu_temperature" => s.cpu_temperature = value != "0",
                        "cpu_helper_enabled" => s.cpu_helper_enabled = value == "1",
                        "auto_update" => s.auto_update = value == "1",
                        "last_update_notice" => {
                            s.last_update_notice = crate::updates::Version::parse(value)
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
                        "window_dpi" => {
                            s.window_dpi = value.parse().ok().filter(|dpi| (48..=960).contains(dpi))
                        }
                        _ => {}
                    }
                }
            }
        }
        s
    }
    pub fn save(&self) -> io::Result<()> {
        let p = path();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(&p, &self.to_text())
    }
    pub fn export_backup(&self, file: &Path) -> io::Result<()> {
        if !file
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pgbackup"))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let normalized = Self::from_text(&self.preferences_text());
        write_atomic(
            file,
            &format!("{BACKUP_HEADER}\n{}", normalized.preferences_text()),
        )
    }
    pub fn read_backup(&self, file: &Path) -> io::Result<Self> {
        let mut text = String::new();
        fs::File::open(file)?
            .take(MAX_BACKUP_SIZE + 1)
            .read_to_string(&mut text)?;
        if text.len() as u64 > MAX_BACKUP_SIZE {
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.parse_backup(&text)
    }
    fn parse_backup(&self, text: &str) -> io::Result<Self> {
        let Some((header, body)) = text.split_once('\n') else {
            return Err(io::ErrorKind::InvalidData.into());
        };
        if header.trim_end_matches('\r') != BACKUP_HEADER {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let mut settings = Self::from_text(body);
        if !body.lines().eq(settings.preferences_text().lines()) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        // Keep this installation's elevation and update-check preferences.
        settings.cpu_permission = self.cpu_permission;
        settings.cpu_temperature = self.cpu_temperature;
        settings.cpu_helper_enabled = self.cpu_helper_enabled;
        settings.auto_update = self.auto_update;
        settings.last_update_notice = self.last_update_notice;
        Ok(settings)
    }
    fn to_text(&self) -> String {
        format!(
            "{}cpu_permission={}\ncpu_temperature={}\ncpu_helper_enabled={}\nauto_update={}\nlast_update_notice={}\n",
            self.preferences_text(),
            self.cpu_permission.key(),
            self.cpu_temperature as u8,
            self.cpu_helper_enabled as u8,
            self.auto_update as u8,
            self.last_update_notice
                .map(|version| version.to_string())
                .unwrap_or_default()
        )
    }
    fn preferences_text(&self) -> String {
        let visible: String = self
            .visible
            .iter()
            .map(|v| if *v { '1' } else { '0' })
            .collect();
        format!(
            "language={}\nrange={}\nvisible={}\ntopmost={}\nframeless={}\nwidth={}\nheight={}\nnormal_width={}\nnormal_height={}\nx={}\ny={}\nwindow_dpi={}\ntaskbar_metric={}\ntray_enabled={}\n",
            self.language.map(|v| v.tag()).unwrap_or("auto"), self.range, visible, self.topmost as u8, self.frameless as u8, self.width, self.height,self.normal_width,self.normal_height,self.x,self.y,
            self.window_dpi.unwrap_or(0),
            self.taskbar_metric.map_or("off", |index| crate::taskbar_graph::METRICS[index].key),
            self.tray_enabled as u8
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
    fn backup_roundtrip_rejects_corruption_and_never_imports_elevation() {
        let exported = Settings {
            range: 4,
            tray_enabled: true,
            x: -200,
            cpu_permission: crate::cpu_sensor::Permission::Allow,
            ..Settings::default()
        };
        let backup = format!("{BACKUP_HEADER}\n{}", exported.preferences_text());
        let local = Settings::default();
        let restored = local.parse_backup(&backup.replace('\n', "\r\n")).unwrap();
        assert_eq!(restored.preferences_text(), exported.preferences_text());
        assert_eq!(restored.cpu_permission, local.cpu_permission);
        for invalid in [
            backup.replace("range=4", "range=9"),
            backup.replace("visible=111111\n", ""),
            format!("{backup}cpu_permission=allow\n"),
            backup.replace("backup v1", "backup v2"),
        ] {
            assert!(local.parse_backup(&invalid).is_err());
        }
    }
    #[test]
    fn compact_width_survives_roundtrip_at_high_dpi() {
        let settings = Settings {
            width: 70, // 280 physical pixels at 400% scaling.
            normal_width: 140,
            window_dpi: Some(384),
            ..Settings::default()
        };
        let restored = Settings::from_text(&settings.to_text());
        assert_eq!(restored.width, 70);
        assert_eq!(restored.normal_width, 140);
        assert_eq!(restored.window_dpi, Some(384));
    }
    #[test]
    fn update_preferences_are_local_and_disabled_in_older_settings() {
        let old = Settings::from_text("range=2\n");
        assert!(!old.auto_update);
        assert_eq!(old.last_update_notice, None);
        let opted_in = Settings::from_text("auto_update=1\nlast_update_notice=1.2.3");
        let loaded = Settings::from_text(&opted_in.to_text());
        assert!(loaded.auto_update);
        assert_eq!(
            loaded.last_update_notice,
            crate::updates::Version::parse("1.2.3")
        );
        let backup = format!("{BACKUP_HEADER}\n{}", opted_in.preferences_text());
        assert!(!old.parse_backup(&backup).unwrap().auto_update);
        assert!(opted_in.parse_backup(&backup).unwrap().auto_update);
    }
    #[test]
    fn cpu_temperature_defaults_to_read_only_even_with_a_remembered_grant() {
        assert!(Settings::default().cpu_temperature);
        assert!(!Settings::default().cpu_helper_enabled);
        let legacy = Settings::from_text("cpu_permission=allow\n");
        assert!(legacy.cpu_temperature);
        assert!(!legacy.cpu_helper_enabled);
        let disabled = Settings::from_text("cpu_temperature=0\ncpu_permission=allow\n");
        assert!(!Settings::from_text(&disabled.to_text()).cpu_temperature);
        let enabled = Settings::from_text("cpu_helper_enabled=1\n");
        assert!(Settings::from_text(&enabled.to_text()).cpu_helper_enabled);
        let backup = format!("{BACKUP_HEADER}\n{}", enabled.preferences_text());
        assert!(!legacy.parse_backup(&backup).unwrap().cpu_helper_enabled);
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
