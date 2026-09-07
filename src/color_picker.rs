//! Inline Win32 settings controls using rust-colorpicker's configurable native picker.
use crate::{AppState, Fonts, Palette, Settings, theme::ThemeColors};
use rust_colorpicker::{
    Color, ColorPicker, PickerChrome, PickerEvent, PickerFont, PickerLabels, PickerTheme,
};
use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

static OPEN: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Accent,
    Primary,
}

const CONTROLS: [(Target, i32, i32, &str); 2] = [
    (Target::Primary, 24, 209, "رنگ اصلی"),
    (Target::Accent, 221, 406, "رنگ تأکیدی"),
];

pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}

pub fn hit_test(x: i32, y: i32) -> Option<Target> {
    if !(122..162).contains(&y) {
        return None;
    }
    CONTROLS
        .iter()
        .find(|(_, left, right, _)| (*left..*right).contains(&x))
        .map(|v| v.0)
}

fn unpack(color: COLORREF) -> [u8; 3] {
    let color = Color::from_colorref(color);
    [color.r, color.g, color.b]
}

fn packed(color: [u8; 3]) -> COLORREF {
    Color::rgb(color[0], color[1], color[2]).to_colorref()
}

fn picker_theme(palette: &Palette) -> PickerTheme {
    let color = Color::from_colorref;
    PickerTheme {
        background: color(palette.background),
        surface: color(palette.surface),
        surface_alt: color(palette.surface_alt),
        text: color(palette.text),
        muted: color(palette.muted),
        border: color(palette.border),
        accent: color(palette.accent),
        accent_text: color(palette.accent_text),
        checker_light: color(palette.surface_alt),
        checker_dark: color(palette.calendar_panel),
    }
}

fn set_target_color(settings: &mut Settings, target: Target, color: [u8; 3]) {
    let mut colors = ThemeColors::from_settings(settings);
    match target {
        Target::Primary => colors.primary = color,
        Target::Accent => colors.accent = color,
    }
    colors.apply(settings);
}

fn repaint_host(hwnd: HWND) {
    unsafe {
        crate::settings_window::repaint_all();
        let main = crate::settings_window::owner();
        if !main.is_null() && IsWindow(main) != 0 {
            crate::refresh_tray_icon(main);
        }
        if !hwnd.is_null() && IsWindow(hwnd) != 0 {
            InvalidateRect(hwnd, null(), 0);
        }
    }
}

pub unsafe fn paint_row(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts) {
    let colors = ThemeColors::from_settings(&app.settings);
    let scale = app.scale();
    for (target, left, right, label) in CONTROLS {
        let value = match target {
            Target::Primary => colors.primary,
            Target::Accent => colors.accent,
        };
        let rect = |left, top, right, bottom| {
            crate::scaled_rect(
                RECT {
                    left,
                    top,
                    right,
                    bottom,
                },
                scale,
            )
        };
        unsafe {
            crate::draw_round_fill(
                hdc,
                rect(left, 122, right, 162),
                palette.surface_alt,
                crate::scaled(10, scale),
            );
            crate::draw_round_fill(
                hdc,
                rect(left + 12, 131, left + 40, 153),
                packed(value),
                crate::scaled(6, scale),
            );
            crate::draw_round_outline(
                hdc,
                rect(left + 12, 131, left + 40, 153),
                palette.muted,
                crate::scaled(6, scale),
                1,
            );
            crate::draw_text(
                hdc,
                label,
                rect(left + 48, 122, right - 12, 162),
                palette.text,
                fonts.regular,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
}

pub fn save_colors(settings: &mut Settings, colors: ThemeColors) -> std::io::Result<()> {
    let previous = settings.clone();
    colors.apply(settings);
    if let Err(error) = settings.try_save() {
        *settings = previous;
        return Err(error);
    }
    Ok(())
}

pub fn report_error(hwnd: HWND, message: &str) {
    unsafe {
        MessageBoxW(
            hwnd,
            crate::wide(message).as_ptr(),
            crate::wide("گاه‌یار").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn open(hwnd: HWND, target: Target) {
    if OPEN.swap(true, Ordering::SeqCst) {
        return;
    }

    let (initial, palette) = {
        let app = crate::state().lock().unwrap();
        let colors = ThemeColors::from_settings(&app.settings);
        let initial = match target {
            Target::Primary => colors.primary,
            Target::Accent => colors.accent,
        };
        (initial, Palette::from_colors(colors))
    };

    let initial_color = Color::rgb(initial[0], initial[1], initial[2]);
    let mut picker = ColorPicker::new();
    picker.set_show_alpha(false);
    picker.set_scale_percent(crate::settings_window::display_scale());
    picker.set_theme(picker_theme(&palette));
    picker.set_chrome(PickerChrome::Borderless);
    picker.set_labels(PickerLabels::persian());
    picker.set_font(PickerFont::new("Vazirmatn", 15, 16));
    picker.set_rtl(true);
    picker.set_corner_radius(12);

    let result = picker.pick_with_owner_live(hwnd, initial_color, |event| {
        let selected = match event {
            PickerEvent::Preview(color)
            | PickerEvent::Accepted(color)
            | PickerEvent::Cancelled(color) => unpack(color.to_colorref()),
        };
        {
            let mut app = crate::state().lock().unwrap();
            set_target_color(&mut app.settings, target, selected);
        }
        repaint_host(hwnd);
    });

    OPEN.store(false, Ordering::SeqCst);
    if unsafe { IsWindow(hwnd) } == 0 {
        return;
    }

    match result {
        Ok(Some(selected)) => {
            let selected = unpack(selected.to_colorref());
            let saved = {
                let mut app = crate::state().lock().unwrap();
                let mut colors = ThemeColors::from_settings(&app.settings);
                match target {
                    Target::Primary => colors.primary = selected,
                    Target::Accent => colors.accent = selected,
                }
                // Re-establish the pre-dialog value as the rollback point before
                // persisting the accepted preview.
                set_target_color(&mut app.settings, target, initial);
                save_colors(&mut app.settings, colors)
            };
            if let Err(error) = saved {
                report_error(hwnd, &format!("ذخیرهٔ رنگ ممکن نشد.\n{error}"));
            }
        }
        Ok(None) => {
            let mut app = crate::state().lock().unwrap();
            set_target_color(&mut app.settings, target, initial);
        }
        Err(error) => {
            {
                let mut app = crate::state().lock().unwrap();
                set_target_color(&mut app.settings, target, initial);
            }
            report_error(
                hwnd,
                &format!("باز کردن کالرپیکر ممکن نشد.\nWindows error: {error}"),
            );
        }
    }

    unsafe {
        SetForegroundWindow(hwnd);
    }
    repaint_host(hwnd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;

    #[test]
    fn picker_regions_do_not_overlap_presets_scale_or_each_other() {
        assert_eq!(hit_test(100, 120), None);
        assert_eq!(hit_test(100, 122), Some(Target::Primary));
        assert_eq!(hit_test(300, 142), Some(Target::Accent));
        assert_eq!(hit_test(215, 142), None);
        assert_eq!(hit_test(100, 162), None);
        assert_eq!(hit_test(100, 166), None);
        for scale in [80, 90, 100, 110, 125] {
            let x = crate::unscaled(crate::scaled(300, scale), scale);
            let y = crate::unscaled(crate::scaled(142, scale), scale);
            assert_eq!(hit_test(x, y), Some(Target::Accent));
        }
    }

    // Exercise the real package/Win32 modal dialog on this UI thread. The timer
    // only accepts/cancels a dialog owned by our temporary test window.
    #[test]
    #[ignore = "opens native Windows dialogs for integration QA"]
    fn native_package_accept_cancel_and_preserve_alpha() {
        use std::sync::atomic::AtomicUsize;
        static ACTION: AtomicUsize = AtomicUsize::new(0x0D);
        unsafe extern "system" fn visit(window: HWND, owner: LPARAM) -> i32 {
            if unsafe { GetWindow(window, GW_OWNER) } == owner as HWND {
                unsafe {
                    PostMessageW(window, WM_KEYDOWN, ACTION.load(Ordering::SeqCst), 0);
                }
            }
            1
        }
        unsafe extern "system" fn finish(owner: HWND, _: u32, _: usize, _: u32) {
            unsafe {
                EnumThreadWindows(
                    windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                    Some(visit),
                    owner as LPARAM,
                );
            }
        }
        unsafe {
            let owner = CreateWindowExW(
                0,
                crate::wide("STATIC").as_ptr(),
                crate::wide("GahYar picker integration test").as_ptr(),
                0,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            assert!(!owner.is_null());
            assert_ne!(SetTimer(owner, 1, 100, Some(finish)), 0);
            let mut picker = ColorPicker::new();
            picker.set_show_alpha(false);
            picker.set_scale_percent(100);
            picker.set_chrome(PickerChrome::Borderless);
            picker.set_labels(PickerLabels::persian());
            picker.set_font(PickerFont::new("Vazirmatn", 15, 16));
            picker.set_rtl(true);
            let initial = Color::rgba(18, 52, 86, 123);
            let accepted = picker.pick_with_owner(owner, initial);
            ACTION.store(0x1B, Ordering::SeqCst);
            let cancelled = picker.pick_with_owner(owner, initial);
            KillTimer(owner, 1);
            DestroyWindow(owner);
            assert_eq!(accepted, Ok(Some(initial)));
            assert_eq!(cancelled, Ok(None));
            assert!(!picker.show_alpha());
        }
    }

    #[test]
    fn windows_colorref_channel_order_round_trips() {
        for color in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [18, 52, 86]] {
            assert_eq!(unpack(packed(color)), color);
            assert_eq!(
                theme::parse_hex(&theme::hex(unpack(packed(color)))),
                Some(color)
            );
        }
        assert_eq!(packed([18, 52, 86]), 0x00563412);
    }
}
