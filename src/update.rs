use std::ffi::c_void;
use std::fs;
use std::mem::size_of;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;

const REPOSITORY: &str = "emadgh/GahYar";
const API_HOST: &str = "api.github.com";
const MAX_DOWNLOAD_SIZE: usize = 50 * 1024 * 1024;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub release_url: String,
    pub download_url: String,
}

#[derive(Clone, Debug)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading,
    Failed(UpdateInfo),
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

struct InternetHandle(*mut c_void);

impl Drop for InternetHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0); }
        }
    }
}

static STATUS: OnceLock<Mutex<UpdateStatus>> = OnceLock::new();

fn status_cell() -> &'static Mutex<UpdateStatus> {
    STATUS.get_or_init(|| Mutex::new(UpdateStatus::Idle))
}

pub fn status() -> UpdateStatus {
    status_cell().lock().unwrap().clone()
}

pub fn banner_visible() -> bool {
    matches!(status(), UpdateStatus::Available(_) | UpdateStatus::Downloading | UpdateStatus::Failed(_))
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
        let next = match check_latest_release() {
            Ok(Some(info)) => UpdateStatus::Available(info),
            Ok(None) => UpdateStatus::UpToDate,
            Err(()) => UpdateStatus::Idle,
        };
        *status_cell().lock().unwrap() = next;
        unsafe { PostMessageW(hwnd as HWND, notify_message, 0, 0); }
    });
    true
}

pub fn start_download(hwnd: HWND, notify_message: u32, apply_message: u32) -> bool {
    let info = match status() {
        UpdateStatus::Available(info) | UpdateStatus::Failed(info) => info,
        _ => return false,
    };
    *status_cell().lock().unwrap() = UpdateStatus::Downloading;
    unsafe { PostMessageW(hwnd, notify_message, 0, 0); }
    let hwnd = hwnd as isize;
    std::thread::spawn(move || {
        if download_and_prepare(&info).is_ok() {
            unsafe { PostMessageW(hwnd as HWND, apply_message, 0, 0); }
        } else {
            *status_cell().lock().unwrap() = UpdateStatus::Failed(info);
            unsafe { PostMessageW(hwnd as HWND, notify_message, 0, 0); }
        }
    });
    true
}

fn check_latest_release() -> Result<Option<UpdateInfo>, ()> {
    let path = format!("/repos/{REPOSITORY}/releases/latest");
    let body = http_get(API_HOST, &path)?;
    let release: GithubRelease = serde_json::from_slice(&body).map_err(|_| ())?;
    if !is_newer(&release.tag_name, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    let asset = release.assets.into_iter().find(|asset| asset.name.eq_ignore_ascii_case("GahYar.exe")).ok_or(())?;
    Ok(Some(UpdateInfo {
        version: release.tag_name.trim_start_matches(['v', 'V']).to_string(),
        release_url: release.html_url,
        download_url: asset.browser_download_url,
    }))
}

fn download_and_prepare(info: &UpdateInfo) -> Result<(), ()> {
    let (host, path) = split_https_url(&info.download_url).ok_or(())?;
    let bytes = http_get(host, path)?;
    if bytes.len() < 100_000 || !bytes.starts_with(b"MZ") { return Err(()); }

    let current_exe = std::env::current_exe().map_err(|_| ())?;
    let safe_version = info.version.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '.').collect::<String>();
    let temp_dir = std::env::temp_dir();
    let source = temp_dir.join(format!("GahYar-update-{}-{safe_version}.exe", std::process::id()));
    let script = temp_dir.join(format!("GahYar-updater-{}.ps1", std::process::id()));
    fs::write(&source, bytes).map_err(|_| ())?;
    fs::write(&script, updater_script()).map_err(|_| ())?;
    launch_updater(&script, &source, &current_exe)
}

fn launch_updater(script: &Path, source: &Path, destination: &Path) -> Result<(), ()> {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(script)
        .arg("-TargetPid").arg(std::process::id().to_string())
        .arg("-Source").arg(source)
        .arg("-Destination").arg(destination)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|_| ())
}

fn updater_script() -> &'static str {
    r#"param([int]$TargetPid, [string]$Source, [string]$Destination)
Wait-Process -Id $TargetPid -ErrorAction SilentlyContinue
for ($attempt = 0; $attempt -lt 20; $attempt++) {
    try {
        Copy-Item -LiteralPath $Source -Destination $Destination -Force -ErrorAction Stop
        Start-Process -FilePath $Destination
        Remove-Item -LiteralPath $Source -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
        exit 0
    } catch {
        Start-Sleep -Milliseconds 500
    }
}
"#
}

fn split_https_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let slash = rest.find('/')?;
    Some((&rest[..slash], &rest[slash..]))
}

fn version_tuple(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.trim_start_matches(['v', 'V']).split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch_text = parts.next().unwrap_or("0");
    let patch = patch_text.split(|ch: char| !ch.is_ascii_digit()).next()?.parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(remote: &str, current: &str) -> bool {
    matches!((version_tuple(remote), version_tuple(current)), (Some(remote), Some(current)) if remote > current)
}

fn http_get(host: &str, path: &str) -> Result<Vec<u8>, ()> {
    unsafe {
        let agent = wide("GahYar Update Checker");
        let session = InternetHandle(WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0));
        if session.0.is_null() { return Err(()); }
        let host_wide = wide(host);
        let connection = InternetHandle(WinHttpConnect(session.0, host_wide.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0));
        if connection.0.is_null() { return Err(()); }
        let verb = wide("GET");
        let path_wide = wide(path);
        let request = InternetHandle(WinHttpOpenRequest(
            connection.0, verb.as_ptr(), path_wide.as_ptr(), null(), null(), null(),
            WINHTTP_FLAG_SECURE | WINHTTP_FLAG_REFRESH,
        ));
        if request.0.is_null() { return Err(()); }
        if WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, null_mut()) == 0 { return Err(()); }

        let mut status_code = 0u32;
        let mut status_size = size_of::<u32>() as u32;
        let mut index = 0u32;
        if WinHttpQueryHeaders(
            request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, null(),
            &mut status_code as *mut u32 as *mut c_void, &mut status_size, &mut index,
        ) == 0 || !(200..300).contains(&status_code) { return Err(()); }

        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            if WinHttpQueryDataAvailable(request.0, &mut available) == 0 { return Err(()); }
            if available == 0 { break; }
            if body.len().saturating_add(available as usize) > MAX_DOWNLOAD_SIZE { return Err(()); }
            let start = body.len();
            body.resize(start + available as usize, 0);
            let mut read = 0u32;
            if WinHttpReadData(request.0, body[start..].as_mut_ptr() as *mut c_void, available, &mut read) == 0 {
                return Err(());
            }
            body.truncate(start + read as usize);
        }
        Ok(body)
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions() {
        assert!(is_newer("v2.2.1", "2.2.0"));
        assert!(is_newer("3.0.0", "2.9.9"));
        assert!(!is_newer("v2.2.0", "2.2.0"));
        assert!(!is_newer("v2.1.9", "2.2.0"));
    }
}
