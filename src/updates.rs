use std::{
    ffi::c_void,
    fmt::{self, Write as _},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    mem::size_of_val,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::PathBuf,
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};
use windows::{
    core::PCWSTR,
    Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{AttachmentServices, IAttachmentExecute},
    },
};
use windows_sys::Win32::{
    Foundation::HWND,
    Networking::WinHttp::*,
    Security::Cryptography::{
        BCryptGenRandom, BCryptHash, BCRYPT_SHA256_ALG_HANDLE, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    },
    Storage::FileSystem::FILE_SHARE_READ,
    UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
};

const RELEASES_PATH: &str = "/nekomimiyks/PerformanceGlass/releases/";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_INSTALLER_SIZE: usize = 32 * 1024 * 1024;
const REDIRECTS: [u32; 5] = [301, 302, 303, 307, 308];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u32, u32, u32);

impl Version {
    pub fn parse(value: &str) -> Option<Self> {
        let mut segments = value.split('.');
        let mut next = || {
            let part = segments.next()?;
            if part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            part.parse::<u32>().ok()
        };
        let version = Self(next()?, next()?, next()?);
        segments.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

fn release_version(location: &str) -> Option<Version> {
    let path = location
        .strip_prefix("https://github.com")
        .unwrap_or(location);
    let tag = path.strip_prefix(RELEASES_PATH)?.strip_prefix("tag/v")?;
    Version::parse(tag)
}

struct InternetHandle(*mut c_void);

impl InternetHandle {
    fn new(raw: *mut c_void) -> Option<Self> {
        (!raw.is_null()).then(|| Self(raw))
    }
}

impl Drop for InternetHandle {
    fn drop(&mut self) {
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

struct Response {
    request: InternetHandle,
    _connection: InternetHandle,
    _session: InternetHandle,
    status: u32,
}

fn request(host: &str, path: &str, verb: &str) -> Option<Response> {
    use crate::ui::wide;
    unsafe {
        let session = InternetHandle::new(WinHttpOpen(
            wide("PerformanceGlass").as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        if WinHttpSetTimeouts(session.0, 4000, 4000, 4000, 4000) == 0 {
            return None;
        }
        let connection =
            InternetHandle::new(WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0))?;
        let request = InternetHandle::new(WinHttpOpenRequest(
            connection.0,
            wide(verb).as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        // Handle redirects explicitly so that downloads stay on approved HTTPS hosts.
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
        if WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&policy as *const u32).cast(),
            size_of_val(&policy) as u32,
        ) == 0
            || WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, null_mut()) == 0
        {
            return None;
        }

        let mut status = 0u32;
        let mut size = size_of_val(&status) as u32;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            null_mut(),
        ) == 0
        {
            return None;
        }
        Some(Response {
            request,
            _connection: connection,
            _session: session,
            status,
        })
    }
}

fn redirect(request: &InternetHandle) -> Option<String> {
    unsafe {
        let mut location = [0u16; 4096];
        let mut bytes = size_of_val(&location) as u32;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_LOCATION,
            null(),
            location.as_mut_ptr().cast(),
            &mut bytes,
            null_mut(),
        ) == 0
        {
            return None;
        }
        let end = location.iter().position(|&c| c == 0)?;
        String::from_utf16(&location[..end]).ok()
    }
}

fn latest_release() -> Option<Version> {
    let response = request("github.com", &format!("{RELEASES_PATH}latest"), "HEAD")?;
    if !REDIRECTS.contains(&response.status) {
        return None;
    }
    release_version(&redirect(&response.request)?)
}

fn download(path: &str, max_size: usize) -> Option<Vec<u8>> {
    let mut host = "github.com".to_owned();
    let mut path = path.to_owned();
    let start = Instant::now();
    for _ in 0..3 {
        let response = request(&host, &path, "GET")?;
        if REDIRECTS.contains(&response.status) {
            let location = redirect(&response.request)?;
            let (next_host, next_path) = location.strip_prefix("https://")?.split_once('/')?;
            if !matches!(
                next_host,
                "release-assets.githubusercontent.com" | "objects.githubusercontent.com"
            ) {
                return None;
            }
            host = next_host.to_owned();
            path = format!("/{next_path}");
            continue;
        }
        if response.status != 200 {
            return None;
        }
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            if start.elapsed() > Duration::from_secs(120)
                || unsafe {
                    WinHttpReadData(
                        response.request.0,
                        buffer.as_mut_ptr().cast(),
                        buffer.len() as u32,
                        &mut read,
                    )
                } == 0
            {
                return None;
            }
            let read = read as usize;
            if read == 0 {
                return Some(bytes);
            }
            if bytes.len() + read > max_size {
                return None;
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
    }
    None
}

fn checksum(contents: &[u8], filename: &str) -> Option<[u8; 32]> {
    let contents = std::str::from_utf8(contents)
        .ok()?
        .trim_start_matches('\u{feff}');
    let mut hashes = contents.lines().filter_map(|line| {
        let mut parts = line.split_ascii_whitespace();
        let hex = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        if name != filename || parts.next().is_some() || hex.len() != 64 || !hex.is_ascii() {
            return None;
        }
        let mut hash = [0u8; 32];
        for (index, byte) in hash.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
        }
        Some(hash)
    });
    let hash = hashes.next()?;
    hashes.next().is_none().then_some(hash)
}

fn sha256(bytes: &[u8]) -> Option<[u8; 32]> {
    let mut hash = [0u8; 32];
    let status = unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            null(),
            0,
            bytes.as_ptr(),
            u32::try_from(bytes.len()).ok()?,
            hash.as_mut_ptr(),
            hash.len() as u32,
        )
    };
    (status >= 0).then_some(hash)
}

fn mark_download(path: &[u16], source: &str) -> windows::core::Result<()> {
    let source = crate::ui::wide(source);
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let result = (|| {
            let attachment: IAttachmentExecute =
                CoCreateInstance(&AttachmentServices, None, CLSCTX_INPROC_SERVER)?;
            attachment.SetLocalPath(PCWSTR(path.as_ptr()))?;
            attachment.SetSource(PCWSTR(source.as_ptr()))?;
            attachment.Save()
        })();
        CoUninitialize();
        result
    }
}

pub struct Installer {
    path: PathBuf,
    lock: Option<File>,
    launched: bool,
}

impl Installer {
    fn prepare(version: Version) -> Option<Self> {
        let filename = format!("PerformanceGlass-v{version}-Windows-x64-Setup.exe");
        let directory = format!("{RELEASES_PATH}download/v{version}/");
        let asset = format!("{directory}{filename}");
        let expected = checksum(
            &download(&format!("{directory}SHA256SUMS.txt"), 4096)?,
            &filename,
        )?;
        let bytes = download(&asset, MAX_INSTALLER_SIZE)?;
        if sha256(&bytes)? != expected {
            return None;
        }
        let mut random = [0u8; 16];
        if unsafe {
            BCryptGenRandom(
                null_mut(),
                random.as_mut_ptr(),
                random.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        } < 0
        {
            return None;
        }
        let mut name = String::from("PerformanceGlass-update-");
        for byte in random {
            let _ = write!(name, "{byte:02x}");
        }
        let temporary = std::env::temp_dir().join(name);
        fs::create_dir(&temporary).ok()?;
        let mut installer = Self {
            path: temporary.join(filename),
            lock: None,
            launched: false,
        };
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&installer.path)
                .ok()?;
            file.write_all(&bytes).ok()?;
        }
        drop(bytes);
        mark_download(
            &installer.wide_path(),
            &format!("https://github.com{asset}"),
        )
        .ok()?;
        // Keep the verified file open without write or delete sharing through process creation.
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&installer.path)
            .ok()?;
        let mut stored = Vec::new();
        (&file)
            .take(MAX_INSTALLER_SIZE as u64 + 1)
            .read_to_end(&mut stored)
            .ok()?;
        if sha256(&stored)? != expected {
            return None;
        }
        installer.lock = Some(file);
        Some(installer)
    }

    fn wide_path(&self) -> Vec<u16> {
        self.path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub unsafe fn launch(&mut self, hwnd: HWND) -> bool {
        let result = ShellExecuteW(
            hwnd,
            crate::ui::wide("open").as_ptr(),
            self.wide_path().as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
        self.launched = result as isize > 32;
        self.launched
    }
}

impl Drop for Installer {
    fn drop(&mut self) {
        self.lock.take();
        if !self.launched {
            let _ = fs::remove_file(&self.path);
            if let Some(parent) = self.path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
    }
}

#[derive(Default)]
pub struct Check {
    next: Option<Instant>,
    pending: Option<Receiver<Option<Version>>>,
    download: Option<Receiver<Option<Installer>>>,
}

impl Check {
    pub fn start_download(&mut self, version: Version) -> bool {
        if self.download.is_some() || !Self::is_newer(version) {
            return false;
        }
        let (sender, receiver) = mpsc::channel();
        if std::thread::Builder::new()
            .name("update-download".into())
            .spawn(move || {
                let _ = sender.send(Installer::prepare(version));
            })
            .is_err()
        {
            return false;
        }
        self.download = Some(receiver);
        true
    }

    pub fn is_downloading(&self) -> bool {
        self.download.is_some()
    }

    pub fn is_newer(version: Version) -> bool {
        Version::parse(env!("CARGO_PKG_VERSION")).is_some_and(|current| version > current)
    }

    pub fn take_download(&mut self) -> Option<Result<Installer, ()>> {
        let receiver = self.download.as_ref()?;
        let result = match receiver.try_recv() {
            Ok(installer) => installer,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => None,
        };
        self.download = None;
        Some(result.ok_or(()))
    }

    pub fn poll(&mut self, last_notice: Option<Version>) -> Option<Version> {
        if let Some(receiver) = &self.pending {
            let result = match receiver.try_recv() {
                Ok(version) => version,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => None,
            };
            self.pending = None;
            return result.filter(|&latest| Self::is_newer(latest) && Some(latest) > last_notice);
        }
        let now = Instant::now();
        if self.next.is_none_or(|next| now >= next) {
            self.next = Some(now + CHECK_INTERVAL);
            let (sender, receiver) = mpsc::channel();
            if std::thread::Builder::new()
                .name("update-check".into())
                .spawn(move || {
                    let _ = sender.send(latest_release());
                })
                .is_ok()
            {
                self.pending = Some(receiver);
            }
        }
        None
    }

    pub fn timer_ms(&self) -> u32 {
        if self.pending.is_some() || self.is_downloading() {
            return 1000;
        }
        self.next
            .map_or(CHECK_INTERVAL, |next| {
                next.saturating_duration_since(Instant::now())
            })
            .as_millis()
            .max(1) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_stable_tags_of_the_official_repository() {
        let canonical = "https://github.com/nekomimiyks/PerformanceGlass/releases/tag/v0.1.20";
        assert_eq!(release_version(canonical), Some(Version(0, 1, 20)));
        assert!(Version::parse("0.2.9") < Version::parse("0.2.10"));
        for invalid in [
            canonical.replace("github.com", "github.com.example.com"),
            canonical.replace("https://", "http://"),
            canonical.replace("/nekomimiyks/", "/someone-else/"),
            format!("{canonical}-preview"),
            format!("{canonical}#page"),
            format!("{canonical}.1"),
        ] {
            assert_eq!(release_version(&invalid), None, "{invalid}");
        }
    }

    #[test]
    fn checksum_must_identify_the_exact_installer() {
        let hash = "0123456789abcdef".repeat(4);
        let entry = format!("{hash}  Setup.exe\r\n");
        assert_eq!(
            checksum(entry.as_bytes(), "Setup.exe"),
            Some(
                [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
                    .repeat(4)
                    .try_into()
                    .unwrap()
            )
        );
        assert_eq!(checksum(entry.as_bytes(), "Elsewhere.exe"), None);
        let duplicate = format!("{entry}{entry}");
        assert_eq!(checksum(duplicate.as_bytes(), "Setup.exe"), None);
        let invalid = format!("{}  Setup.exe", "x".repeat(64));
        assert_eq!(checksum(invalid.as_bytes(), "Setup.exe"), None);
    }

    #[test]
    #[ignore = "requires an internet connection to GitHub"]
    fn published_github_installer_can_be_verified_and_prepared() {
        let version = latest_release().expect("GitHub latest release redirect");
        let installer =
            Installer::prepare(version).expect("download and validate published installer");
        assert!(installer.path.is_file());
        let path = installer.path.clone();
        drop(installer);
        assert!(!path.exists());
    }
}
