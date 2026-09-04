use std::sync::{Mutex, OnceLock};

use update_via_github::UpdateConfig;
pub use update_via_github::UpdateInfo;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

const REPOSITORY: &str = "emadgh/GahYar";
const ASSET_NAME: &str = "GahYar.exe";
const MAX_DOWNLOAD_SIZE: usize = 50 * 1024 * 1024;

#[derive(Clone, Debug)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading,
    Failed(UpdateInfo),
}

static STATUS: OnceLock<Mutex<UpdateStatus>> = OnceLock::new();

fn status_cell() -> &'static Mutex<UpdateStatus> {
    STATUS.get_or_init(|| Mutex::new(UpdateStatus::Idle))
}

fn config() -> UpdateConfig {
    UpdateConfig::new(REPOSITORY, ASSET_NAME, env!("CARGO_PKG_VERSION"))
        .with_app_name("GahYar")
        .with_max_download_size(MAX_DOWNLOAD_SIZE)
}

pub fn status() -> UpdateStatus {
    status_cell().lock().unwrap().clone()
}

pub fn banner_visible() -> bool {
    matches!(
        status(),
        UpdateStatus::Available(_) | UpdateStatus::Downloading | UpdateStatus::Failed(_)
    )
}

pub fn start_check(hwnd: HWND, notify_message: u32) -> bool {
    {
        let mut status = status_cell().lock().unwrap();
        if matches!(*status, UpdateStatus::Checking | UpdateStatus::Downloading) {
            return false;
        }
        *status = UpdateStatus::Checking;
    }

    let hwnd = hwnd as isize;
    std::thread::spawn(move || {
        let next = match update_via_github::check_latest_release(&config()) {
            Ok(Some(info)) => UpdateStatus::Available(info),
            Ok(None) => UpdateStatus::UpToDate,
            Err(_) => UpdateStatus::Idle,
        };
        *status_cell().lock().unwrap() = next;
        unsafe {
            PostMessageW(hwnd as HWND, notify_message, 0, 0);
        }
    });
    true
}

pub fn start_download(hwnd: HWND, notify_message: u32, apply_message: u32) -> bool {
    let info = match status() {
        UpdateStatus::Available(info) | UpdateStatus::Failed(info) => info,
        _ => return false,
    };

    *status_cell().lock().unwrap() = UpdateStatus::Downloading;
    unsafe {
        PostMessageW(hwnd, notify_message, 0, 0);
    }

    let hwnd = hwnd as isize;
    std::thread::spawn(move || {
        let updater_config = config();
        let result = update_via_github::download_update(&updater_config, &info, |_, _| {})
            .and_then(|source| update_via_github::apply_update(&updater_config, &source));
        if result.is_ok() {
            unsafe {
                PostMessageW(hwnd as HWND, apply_message, 0, 0);
            }
        } else {
            *status_cell().lock().unwrap() = UpdateStatus::Failed(info);
            unsafe {
                PostMessageW(hwnd as HWND, notify_message, 0, 0);
            }
        }
    });
    true
}
