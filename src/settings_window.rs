//! A modeless settings window beside the calendar; both use the same Settings.
use crate::*;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::AtomicU32;
use windows_sys::Win32::UI::Controls::SetScrollInfo;

static HWND_VALUE: AtomicIsize = AtomicIsize::new(0);
static OWNER: AtomicIsize = AtomicIsize::new(0);
static SCROLL: AtomicI32 = AtomicI32::new(0);
static DISPLAY_SCALE: AtomicU32 = AtomicU32::new(100);
const CLASS: &str = "GahYarSettings";

pub fn display_scale() -> u32 {
    DISPLAY_SCALE.load(Ordering::SeqCst)
}

fn fit_scale(requested: u32, work_width: i32, calendar_width: i32, scrollbar: i32) -> u32 {
    let available = (work_width - calendar_width - 24 - scrollbar).max(1);
    requested.min((available * 100 / settings_panel::WIDTH).max(1) as u32)
}

pub fn owner() -> HWND {
    OWNER.load(Ordering::SeqCst) as HWND
}
fn window() -> HWND {
    HWND_VALUE.load(Ordering::SeqCst) as HWND
}
pub fn is_open() -> bool {
    !window().is_null()
}

// Defer until Windows has finished moving activation between the owned windows.
fn belongs_to_popup(main: HWND, foreground: HWND) -> bool {
    foreground == main
        || (!foreground.is_null() && unsafe { GetAncestor(foreground, GA_ROOTOWNER) } == main)
}

pub fn dismiss_if_outside(main: HWND) {
    unsafe {
        if EXITING.load(Ordering::SeqCst)
            || color_picker::is_open()
            || CONFIRM_HWND.load(Ordering::SeqCst) != 0
            || ABOUT_HWND.load(Ordering::SeqCst) != 0
        {
            return;
        }
        let foreground = GetForegroundWindow();
        if belongs_to_popup(main, foreground) {
            return;
        }
        close();
        ShowWindow(main, SW_HIDE);
    }
}

pub fn close() {
    let hwnd = window();
    if !hwnd.is_null() {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
}

pub fn show(main: HWND) {
    unsafe {
        if is_open() {
            reposition();
            SetForegroundWindow(window());
            return;
        }
        let instance = GetModuleHandleW(null());
        let class = wide(CLASS);
        let definition = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: load_app_icon(instance),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&definition) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return;
        }
        OWNER.store(main as isize, Ordering::SeqCst);
        SCROLL.store(0, Ordering::SeqCst);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class.as_ptr(),
            wide("تنظیمات گاه‌یار").as_ptr(),
            WS_POPUP | WS_VSCROLL,
            0,
            0,
            1,
            1,
            main,
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            OWNER.store(0, Ordering::SeqCst);
            return;
        }
        reposition();
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
    }
}

// Keep the pair within the selected monitor, preferring the left of the calendar.
fn positions(work: RECT, calendar: RECT, width: i32, height: i32) -> (i32, i32, i32) {
    let gap = 8;
    let calendar_width = calendar.right - calendar.left;
    let minimum = work.left + gap;
    let maximum = work.right - gap;
    let mut calendar_x = calendar
        .left
        .clamp(minimum, (maximum - calendar_width).max(minimum));
    let settings_x = if calendar_x - gap - width >= minimum {
        calendar_x - gap - width
    } else if calendar_x + calendar_width + gap + width <= maximum {
        calendar_x + calendar_width + gap
    } else {
        calendar_x = (maximum - calendar_width).max(minimum);
        (calendar_x - gap - width).max(minimum)
    };
    let y = ((calendar.top + calendar.bottom - height) / 2).clamp(
        work.top + gap,
        (work.bottom - gap - height).max(work.top + gap),
    );
    (calendar_x, settings_x, y)
}

pub fn reposition() {
    let hwnd = window();
    let main = owner();
    if hwnd.is_null() || main.is_null() {
        return;
    }
    unsafe {
        let requested_scale = state().lock().unwrap().scale();
        let mut calendar: RECT = zeroed();
        GetWindowRect(main, &mut calendar);
        let monitor = MonitorFromWindow(main, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return;
        }
        let scale = fit_scale(
            requested_scale,
            info.rcWork.right - info.rcWork.left,
            calendar.right - calendar.left,
            GetSystemMetrics(SM_CXVSCROLL),
        );
        DISPLAY_SCALE.store(scale, Ordering::SeqCst);
        let content_height = scaled(settings_panel::HEIGHT, scale);
        let height = content_height.min((info.rcWork.bottom - info.rcWork.top - 16).max(100));
        let width = scaled(settings_panel::WIDTH, scale)
            + if content_height > height {
                GetSystemMetrics(SM_CXVSCROLL)
            } else {
                0
            };
        let (main_x, x, y) = positions(info.rcWork, calendar, width, height);
        SetWindowPos(
            main,
            null_mut(),
            main_x,
            calendar.top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
        set_scroll(SCROLL.load(Ordering::SeqCst));
        InvalidateRect(hwnd, null(), 0);
    }
}

fn set_scroll(position: i32) {
    let hwnd = window();
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let total = scaled(settings_panel::HEIGHT, display_scale());
        let mut client: RECT = zeroed();
        GetClientRect(hwnd, &mut client);
        let position = position.clamp(0, (total - client.bottom).max(0));
        SCROLL.store(position, Ordering::SeqCst);
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
            nMin: 0,
            nMax: total - 1,
            nPage: client.bottom as u32,
            nPos: position,
            ..zeroed()
        };
        SetScrollInfo(hwnd, SB_VERT, &info, 1);
        InvalidateRect(hwnd, null(), 0);
    }
}

pub fn repaint_all() {
    unsafe {
        for hwnd in [
            owner(),
            window(),
            ABOUT_HWND.load(Ordering::SeqCst) as HWND,
            CONFIRM_HWND.load(Ordering::SeqCst) as HWND,
        ] {
            if !hwnd.is_null() && IsWindow(hwnd) != 0 {
                InvalidateRect(hwnd, null(), 0);
            }
        }
    }
}

pub fn reveal(rect: RECT) {
    let scale = display_scale();
    let mut client: RECT = unsafe { zeroed() };
    unsafe {
        GetClientRect(window(), &mut client);
    }
    let position = SCROLL.load(Ordering::SeqCst);
    let top = scaled(rect.top, scale);
    let bottom = scaled(rect.bottom, scale);
    if top < position {
        set_scroll(top);
    } else if bottom > position + client.bottom {
        set_scroll(bottom - client.bottom);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            HWND_VALUE.store(hwnd as isize, Ordering::SeqCst);
            1
        }
        WM_PAINT => {
            unsafe {
                paint_main(hwnd, ViewMode::Settings, SCROLL.load(Ordering::SeqCst));
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_ACTIVATE => {
            if (wparam as u32 & 0xffff) == WA_INACTIVE {
                unsafe {
                    PostMessageW(owner(), WM_DISMISS_POPUPS, 0, 0);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = point_from_lparam(lparam);
            settings_panel::click(hwnd, x, y + SCROLL.load(Ordering::SeqCst));
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xffff) as i16 as i32;
            let scale = state().lock().unwrap().scale();
            set_scroll(SCROLL.load(Ordering::SeqCst) - delta * scaled(48, scale) / 120);
            0
        }
        WM_VSCROLL => {
            let mut info: SCROLLINFO = unsafe { zeroed() };
            info.cbSize = size_of::<SCROLLINFO>() as u32;
            info.fMask = SIF_ALL;
            unsafe {
                GetScrollInfo(hwnd, SB_VERT, &mut info);
            }
            let next = match (wparam & 0xffff) as i32 {
                SB_LINEUP => info.nPos - 32,
                SB_LINEDOWN => info.nPos + 32,
                SB_PAGEUP => info.nPos - info.nPage as i32,
                SB_PAGEDOWN => info.nPos + info.nPage as i32,
                SB_THUMBTRACK | SB_THUMBPOSITION => info.nTrackPos,
                SB_TOP => 0,
                SB_BOTTOM => info.nMax,
                _ => info.nPos,
            };
            set_scroll(next);
            0
        }
        WM_KEYDOWN if wparam == 0x1B => {
            close();
            0
        }
        WM_CLOSE => {
            close();
            0
        }
        WM_KEYDOWN if settings_panel::keyboard(hwnd, wparam) => 0,
        WM_NCDESTROY => {
            HWND_VALUE.store(0, Ordering::SeqCst);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_between_owned_windows_stays_inside_the_popup() {
        unsafe {
            let create = |parent| {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    wide("STATIC").as_ptr(),
                    wide("GahYar hidden activation test").as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    10,
                    10,
                    parent,
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                )
            };
            // Hidden test-only windows: no user state, focus, or visible desktop changes.
            let main = create(null_mut());
            let settings = create(main);
            let dialog = create(settings);
            let external = create(null_mut());
            assert!(
                !main.is_null() && !settings.is_null() && !dialog.is_null() && !external.is_null()
            );
            assert!(belongs_to_popup(main, main));
            assert!(belongs_to_popup(main, settings));
            assert!(belongs_to_popup(main, dialog));
            assert!(!belongs_to_popup(main, external));
            assert!(!belongs_to_popup(main, null_mut()));
            DestroyWindow(dialog);
            DestroyWindow(settings);
            DestroyWindow(main);
            DestroyWindow(external);
        }
    }

    #[test]
    fn centering_and_scaled_pair_fit_small_and_negative_monitors() {
        for screen_width in [1280, 1366, 1920, 2560] {
            for scale in [80, 100, 125] {
                let work = RECT {
                    left: -screen_width,
                    top: -200,
                    right: 0,
                    bottom: 840,
                };
                let cw = scaled(BASE_WIDTH, scale);
                let effective = fit_scale(scale, screen_width, cw, 17);
                let sw = scaled(settings_panel::WIDTH, effective) + 17;
                let sh = scaled(settings_panel::HEIGHT, effective);
                let calendar = RECT {
                    left: -cw - 8,
                    top: 0,
                    right: -8,
                    bottom: 600,
                };
                let (cx, sx, y) = positions(work, calendar, sw, sh);
                assert!(sx >= work.left && sx + sw <= work.right);
                assert!(sx + sw <= cx || cx + cw <= sx);
                let ideal = (calendar.top + calendar.bottom - sh) / 2;
                assert_eq!(y, ideal.clamp(work.top + 8, work.bottom - 8 - sh));
            }
        }
    }

    #[test]
    fn settings_and_calendar_fit_beside_each_other_on_either_monitor() {
        for work in [
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            RECT {
                left: -1920,
                top: -200,
                right: 0,
                bottom: 840,
            },
        ] {
            for left in [work.left + 8, work.left + 600, work.right - 438] {
                let calendar = RECT {
                    left,
                    top: work.bottom - 700,
                    right: left + 430,
                    bottom: work.bottom - 8,
                };
                let (main_x, settings_x, y) = positions(work, calendar, 447, 900);
                assert!(main_x >= work.left && main_x + 430 <= work.right);
                assert!(settings_x >= work.left && settings_x + 447 <= work.right);
                assert!(settings_x + 447 <= main_x || main_x + 430 <= settings_x);
                assert!(y >= work.top && y + 900 <= work.bottom);
            }
        }
    }
}
