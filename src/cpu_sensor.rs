use crate::{i18n, ui::wide};
use std::{
    fs::OpenOptions,
    io::Read,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::Command,
    ptr::{null, null_mut},
    sync::Mutex,
    time::{Duration, Instant},
};
use windows::Win32::System::Com::CoCreateGuid;
use windows_sys::Win32::{
    Foundation::*,
    Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE},
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*},
    UI::{Controls::*, Shell::*, WindowsAndMessaging::*},
};

const HELPER: &str = "cpu-sensor\\PerformanceGlass.CpuSensor.exe";
const INSTALLER_SHA256: [u8; 32] = [
    0x1f, 0x51, 0x9a, 0x22, 0xe4, 0x71, 0x87, 0xf7, 0x0a, 0x13, 0x79, 0xa4, 0x8c, 0xa6, 0x04, 0x98,
    0x1c, 0x4f, 0xcf, 0x69, 0x4f, 0x4e, 0x65, 0xb7, 0x34, 0xaa, 0xa7, 0x4a, 0x9f, 0xba, 0x30, 0x32,
];
static LATEST: Mutex<Option<(Instant, f32)>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Ask,
    Allow,
    Deny,
}

impl Permission {
    pub fn parse(value: &str) -> Self {
        match value {
            "allow" => Self::Allow,
            "deny" => Self::Deny,
            _ => Self::Ask,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

fn executable() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(HELPER))
}

pub fn available() -> bool {
    executable().is_some_and(|exe| exe.is_file())
}

fn installer_path() -> Option<PathBuf> {
    Some(executable()?.with_file_name("PawnIO_setup.exe"))
}

pub fn installer_available() -> bool {
    installer_path().is_some_and(|path| path.is_file())
}

unsafe fn launch_elevated(
    hwnd: HWND,
    exe: &Path,
    args: Option<&str>,
    show: i32,
) -> std::io::Result<OwnedHandle> {
    let exe: Vec<_> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let verb = wide("runas");
    let args = args.map(wide);
    let mut shell: SHELLEXECUTEINFOW = zeroed();
    shell.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    shell.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
    shell.hwnd = hwnd;
    shell.lpVerb = verb.as_ptr();
    shell.lpFile = exe.as_ptr();
    shell.lpParameters = args.as_ref().map_or(null(), |v| v.as_ptr());
    shell.nShow = show;
    if ShellExecuteExW(&mut shell) == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if shell.hProcess.is_null() {
        return Err(std::io::Error::other("Process handle missing"));
    }
    Ok(OwnedHandle::from_raw_handle(shell.hProcess))
}

fn official_installer(bytes: &[u8]) -> bool {
    let Ok(length) = u32::try_from(bytes.len()) else {
        return false;
    };
    let mut hash = [0; 32];
    let status = unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            null(),
            0,
            bytes.as_ptr(),
            length,
            hash.as_mut_ptr(),
            hash.len() as u32,
        )
    };
    status >= 0 && hash == INSTALLER_SHA256
}

pub unsafe fn install_driver(hwnd: HWND) -> std::io::Result<OwnedHandle> {
    let path = installer_path().ok_or(std::io::ErrorKind::NotFound)?;
    // Keep the verified file open without write/delete sharing through process creation.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)?;
    let mut bytes = Vec::new();
    (&file).take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if !official_installer(&bytes) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            label(
                "同梱版の SHA-256 と一致しません。",
                "The bundled installer SHA-256 does not match.",
            ),
        ));
    }
    launch_elevated(hwnd, &path, None, SW_SHOWNORMAL)
}

pub unsafe fn installer_exit(process: &OwnedHandle) -> std::io::Result<Option<u32>> {
    let handle = process.as_raw_handle();
    match WaitForSingleObject(handle, 0) {
        WAIT_TIMEOUT => Ok(None),
        WAIT_OBJECT_0 => {
            let mut code = 0;
            if GetExitCodeProcess(handle, &mut code) != 0 {
                return Ok(Some(code));
            }
            Err(std::io::Error::last_os_error())
        }
        _ => Err(std::io::Error::last_os_error()),
    }
}

pub fn label(ja: &'static str, en: &'static str) -> &'static str {
    if i18n::language() == i18n::Language::Ja {
        ja
    } else {
        en
    }
}

pub fn temperature() -> f32 {
    LATEST
        .lock()
        .unwrap()
        .filter(|(at, _)| at.elapsed() <= Duration::from_secs(3))
        .map_or(f32::NAN, |(_, value)| value)
}

pub enum Poll {
    Running,
    AdminRequired,
    MissingDriver,
    Unsupported,
    Failed,
}

pub struct Reader {
    pipe: HANDLE,
    pid: u32,
    connected: bool,
    last_packet: Instant,
}

impl Reader {
    pub unsafe fn start(hwnd: HWND, elevated: bool) -> std::io::Result<Self> {
        let exe = executable()
            .filter(|path| path.is_file())
            .ok_or(std::io::ErrorKind::NotFound)?;
        let guid = CoCreateGuid().map_err(std::io::Error::other)?;
        let name = format!("PerformanceGlass-{:032x}", guid.to_u128());
        let pipe_path = wide(&format!(r"\\.\pipe\{name}"));
        let pipe = CreateNamedPipeW(
            pipe_path.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            64,
            0,
            null(),
        );
        if pipe == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let mut reader = Self {
            pipe,
            pid: 0,
            connected: false,
            last_packet: Instant::now(),
        };
        reader.pid = if elevated {
            let process = launch_elevated(hwnd, &exe, Some(&name), SW_HIDE)?;
            GetProcessId(process.as_raw_handle())
        } else {
            Command::new(exe)
                .arg(name)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()?
                .id()
        };
        reader.last_packet = Instant::now();
        Ok(reader)
    }

    pub unsafe fn poll(&mut self) -> Poll {
        if self.last_packet.elapsed() > Duration::from_secs(12) {
            return Poll::Failed;
        }
        if !self.connected {
            if ConnectNamedPipe(self.pipe, null_mut()) == 0
                && GetLastError() != ERROR_PIPE_CONNECTED
            {
                return Poll::Running;
            }
            let mut pid = 0;
            if GetNamedPipeClientProcessId(self.pipe, &mut pid) == 0 || pid != self.pid {
                return Poll::Failed;
            }
            self.connected = true;
        }

        // The helper sends one little-endian i32 per sample. Negative sentinels are statuses.
        let mut packet = [0u8; 4];
        for _ in 0..8 {
            let mut count = 0;
            if ReadFile(self.pipe, packet.as_mut_ptr(), 4, &mut count, null_mut()) == 0 {
                return if GetLastError() == ERROR_NO_DATA {
                    Poll::Running
                } else {
                    Poll::Failed
                };
            }
            if count != 4 {
                return Poll::Failed;
            }
            self.last_packet = Instant::now();
            match i32::from_le_bytes(packet) {
                -20_001 => return Poll::AdminRequired,
                -20_002 => return Poll::MissingDriver,
                -20_003 => return Poll::Unsupported,
                -20_004 => *LATEST.lock().unwrap() = None,
                value @ -5000..=15000 => {
                    *LATEST.lock().unwrap() = Some((self.last_packet, value as f32 / 100.));
                }
                _ => return Poll::Failed,
            }
        }
        Poll::Running
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.pipe) };
        *LATEST.lock().unwrap() = None;
    }
}

pub unsafe fn confirm(hwnd: HWND) -> (bool, bool) {
    let title = wide("Performance Glass");
    let heading = wide(label(
        "CPU温度の取得に管理者権限を許可しますか？",
        "Allow administrator access to read CPU temperature?",
    ));
    let info = wide(label(
        "CPU温度を読み取る補助プログラムを起動します。この後、Windowsのユーザーアカウント制御が表示されます。",
        "The CPU temperature helper will start. Windows User Account Control will ask for permission.",
    ));
    let remember = wide(label("今後この選択を覚える", "Remember this choice"));
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: hwnd,
        dwCommonButtons: TDCBF_YES_BUTTON | TDCBF_NO_BUTTON,
        nDefaultButton: IDNO,
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: heading.as_ptr(),
        pszContent: info.as_ptr(),
        pszVerificationText: remember.as_ptr(),
        ..zeroed()
    };
    let mut answer = 0;
    let mut checked = 0;
    if TaskDialogIndirect(&config, &mut answer, null_mut(), &mut checked) < 0 {
        return (false, false);
    }
    (
        answer == IDYES,
        checked != 0 && matches!(answer, IDYES | IDNO),
    )
}
