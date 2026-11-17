//! Store startup tasks use Windows' package registration rather than HKCU Run.
use std::{
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, AtomicI32, Ordering},
        mpsc::{sync_channel, SyncSender},
        Mutex, OnceLock,
    },
};
use windows::{
    core::HSTRING,
    ApplicationModel::{StartupTask, StartupTaskState},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentPackageFullName(length: *mut u32, name: *mut u16) -> i32;
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static QUERY_STATUS: AtomicI32 = AtomicI32::new(0);
static ERROR: Mutex<Option<String>> = Mutex::new(None);
static WORKER: OnceLock<Result<SyncSender<Option<bool>>, String>> = OnceLock::new();

pub fn is_packaged() -> bool {
    static VALUE: OnceLock<bool> = OnceLock::new();
    *VALUE.get_or_init(|| {
        let mut length = 0;
        matches!(
            unsafe { GetCurrentPackageFullName(&mut length, null_mut()) },
            0 | 122
        )
    })
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn worker() -> Result<&'static SyncSender<Option<bool>>, String> {
    WORKER.get_or_init(|| {
        let (sender, receiver) = sync_channel(2);
        std::thread::Builder::new().name("store-startup".into()).stack_size(128 * 1024)
            .spawn(move || {
                let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
                while let Ok(change) = receiver.recv() {
                    let result = initialized.clone().and_then(|_| update(change));
                    QUERY_STATUS.store(if result.is_ok() { 1 } else { -1 }, Ordering::Relaxed);
                    match result {
                        Ok(enabled) => ENABLED.store(enabled, Ordering::Relaxed),
                        Err(error) if change.is_some() => {
                            *ERROR.lock().unwrap() = Some(format!("{}\n{error}", crate::i18n::tr("自動起動を変更できません。Windowsの「設定 → アプリ → スタートアップ」も確認してください。")));
                        }
                        Err(_) => {}
                    }
                }
                if initialized.is_ok() { unsafe { RoUninitialize(); } }
            }).map_err(|e| e.to_string())?;
        Ok(sender)
    }).as_ref().map_err(Clone::clone)
}

fn update(change: Option<bool>) -> windows::core::Result<bool> {
    let task = StartupTask::GetAsync(&HSTRING::from("PerformanceGlassStartup"))?.get()?;
    let state = match change {
        Some(true) => task.RequestEnableAsync()?.get()?,
        Some(false) => {
            task.Disable()?;
            task.State()?
        }
        None => task.State()?,
    };
    let enabled = matches!(
        state,
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy
    );
    if change == Some(true) && !enabled {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070005_u32 as i32,
        )));
    }
    Ok(enabled)
}

pub fn refresh() {
    if is_packaged() {
        if let Ok(sender) = worker() {
            let _ = sender.try_send(None);
        }
    }
}

pub fn set(enabled: bool) -> Result<(), String> {
    worker()?.try_send(Some(enabled)).map_err(|_| {
        crate::i18n::tr("自動起動の設定処理中です。少し待ってから再度お試しください。").into()
    })
}

pub fn take_error() -> Option<String> {
    ERROR.lock().unwrap().take()
}

pub fn query_succeeded() -> bool {
    QUERY_STATUS.load(Ordering::Relaxed) == 1
}
