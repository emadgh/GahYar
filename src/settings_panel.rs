//! Categorized settings sharing the application's theme and live calendar state.
use crate::*;
use std::sync::atomic::AtomicUsize;

pub const WIDTH: i32 = 740;
pub const HEIGHT: i32 = 600;
static PAGE: AtomicUsize = AtomicUsize::new(0);
static FOCUS: AtomicIsize = AtomicIsize::new(-1);
const TITLES: [&str; 5] = [
    "ظاهر برنامه",
    "تقویم و مناسبت‌ها",
    "آیکون کنار ساعت",
    "رفتار برنامه",
    "مدیریت برنامه",
];
const DESCRIPTIONS: [&str; 5] = [
    "رنگ‌ها و اندازه؛ نتیجه را همان لحظه در تقویم ببینید.",
    "تقویم اصلی و اطلاعاتی که کنار آن نمایش داده می‌شوند.",
    "نمایش شماره روز و تاریخ در نوار وظیفهٔ ویندوز.",
    "اجرای برنامه و دریافت نسخه‌های جدید.",
    "نصب، حذف و بازگرداندن تنظیمات برنامه.",
];

#[derive(Clone, Copy, Debug)]
enum Action {
    Theme,
    Primary,
    Accent,
    Scale,
    ThemeReset,
    Calendar,
    Direction,
    Layout,
    Jalali,
    Gregorian,
    Hijri,
    Subtitles,
    Events,
    TrayContent,
    Digits,
    TrayText,
    TrayBackground,
    Tooltip,
    Autostart,
    AutoUpdate,
    CheckUpdate,
    Install,
    Uninstall,
    Reset,
}
struct Row {
    title: &'static str,
    detail: String,
    options: Vec<String>,
    selected: Option<usize>,
    action: Action,
    enabled: bool,
    toggle: bool,
    swatch: Option<[u8; 3]>,
}
fn choice(
    title: &'static str,
    detail: &str,
    options: &[&str],
    selected: usize,
    action: Action,
) -> Row {
    Row {
        title,
        detail: detail.into(),
        options: options.iter().map(|v| v.to_string()).collect(),
        selected: Some(selected),
        action,
        enabled: true,
        toggle: false,
        swatch: None,
    }
}
fn toggle(title: &'static str, detail: &str, value: bool, action: Action) -> Row {
    let mut r = choice(title, detail, &[""], value as usize, action);
    r.toggle = true;
    r
}
fn button(title: &'static str, detail: &str, text: &str, action: Action) -> Row {
    let mut r = choice(title, detail, &[text], 0, action);
    r.selected = None;
    r
}
fn rows(app: &AppState, page: usize) -> Vec<Row> {
    let s = &app.settings;
    let colors = theme::ThemeColors::from_settings(s);
    match page {
        0 => {
            let mut primary = button(
                "رنگ اصلی (Primary)",
                "پس‌زمینه و سطوح برنامه",
                &theme::hex(colors.primary),
                Action::Primary,
            );
            primary.swatch = Some(colors.primary);
            let mut accent = button(
                "رنگ تأکیدی (Accent)",
                "انتخاب‌ها و دکمه‌های فعال",
                &theme::hex(colors.accent),
                Action::Accent,
            );
            accent.swatch = Some(colors.accent);
            vec![
                choice(
                    "پوسته",
                    if colors.is_custom() {
                        "رنگ‌های سفارشی فعال‌اند"
                    } else {
                        "رنگ‌های پیش‌فرض پوسته"
                    },
                    &["تیره", "روشن"],
                    (s.theme == Theme::Light) as usize,
                    Action::Theme,
                ),
                primary,
                accent,
                choice(
                    "اندازهٔ رابط",
                    "مقیاس تقویم و تنظیمات",
                    &["۸۰٪", "۹۰٪", "۱۰۰٪", "۱۱۰٪", "۱۲۵٪"],
                    [80, 90, 100, 110, 125]
                        .iter()
                        .position(|v| *v == s.ui_scale)
                        .unwrap_or(2),
                    Action::Scale,
                ),
                button(
                    "رنگ‌های پیش‌فرض",
                    "فقط رنگ‌های همین پوسته بازنشانی می‌شوند",
                    "بازگردانی رنگ‌ها",
                    Action::ThemeReset,
                ),
            ]
        }
        1 => vec![
            choice(
                "حالت نمایش",
                "تقویم کامل یا نمای جمع‌وجور روزانه",
                &["تقویم کامل", "روزانه"],
                s.compact_day as usize,
                Action::Layout,
            ),
            choice(
                "تقویم اصلی",
                "این تقویم همیشه نمایش داده می‌شود",
                &["شمسی", "میلادی", "قمری"],
                match s.main_calendar {
                    CalendarKind::Jalali => 0,
                    CalendarKind::Gregorian => 1,
                    CalendarKind::Hijri => 2,
                },
                Action::Calendar,
            ),
            choice(
                "چیدمان تقویم",
                "جهت نمایش روزهای هفته",
                &["راست به چپ", "چپ به راست"],
                (!s.calendar_rtl) as usize,
                Action::Direction,
            ),
            toggle(
                "تاریخ شمسی",
                "نمایش به‌عنوان تقویم جانبی",
                s.show_jalali || s.main_calendar == CalendarKind::Jalali,
                Action::Jalali,
            ),
            toggle(
                "تاریخ میلادی",
                "نمایش به‌عنوان تقویم جانبی",
                s.show_gregorian || s.main_calendar == CalendarKind::Gregorian,
                Action::Gregorian,
            ),
            toggle(
                "تاریخ قمری",
                "نمایش به‌عنوان تقویم جانبی",
                s.show_hijri || s.main_calendar == CalendarKind::Hijri,
                Action::Hijri,
            ),
            toggle(
                "عنوان تقویم‌های جانبی",
                "نام ماه کنار تاریخ‌های جانبی",
                s.show_subtitles,
                Action::Subtitles,
            ),
            toggle(
                "مناسبت‌ها",
                "رویدادهای مربوط به روز انتخاب‌شده",
                s.show_events,
                Action::Events,
            ),
        ]
        .into_iter()
        .map(|mut r| {
            let is_main = matches!(
                (r.action, s.main_calendar),
                (Action::Jalali, CalendarKind::Jalali)
                    | (Action::Gregorian, CalendarKind::Gregorian)
                    | (Action::Hijri, CalendarKind::Hijri)
            );
            if is_main {
                r.enabled = false;
                r.detail = "تقویم اصلی؛ همیشه نمایش داده می‌شود".into();
            }
            r
        })
        .collect(),
        2 => vec![
            choice(
                "محتوای آیکون",
                "آیکون برنامه یا شمارهٔ روز شمسی",
                &["آیکون برنامه", "شماره روز"],
                s.tray_day_icon as usize,
                Action::TrayContent,
            ),
            choice(
                "نوع ارقام",
                "زبان اعداد روی آیکون",
                &["فارسی", "انگلیسی"],
                s.tray_english_digits as usize,
                Action::Digits,
            ),
            choice(
                "رنگ متن",
                "رنگ ارقام روی آیکون",
                &["سفید", "مشکی"],
                (!s.tray_text_white) as usize,
                Action::TrayText,
            ),
            choice(
                "زمینهٔ آیکون",
                "رنگی از رنگ تأکیدی (Accent) استفاده می‌کند",
                &["شفاف", "رنگی (Accent)"],
                s.tray_accent_background as usize,
                Action::TrayBackground,
            ),
            toggle(
                "تاریخ کامل هنگام مکث",
                "نمایش تاریخ با قرار دادن ماوس روی آیکون",
                s.show_tray_date,
                Action::Tooltip,
            ),
        ]
        .into_iter()
        .map(|mut r| {
            if matches!(
                r.action,
                Action::Digits | Action::TrayText | Action::TrayBackground
            ) && !s.tray_day_icon
            {
                r.enabled = false;
            }
            r
        })
        .collect(),
        3 => vec![
            toggle(
                "اجرا همراه ویندوز",
                "شروع خودکار پس از ورود به ویندوز",
                s.autostart,
                Action::Autostart,
            ),
            toggle(
                "بروزرسانی خودکار",
                "دریافت و نصب خودکار نسخه‌های جدید",
                s.auto_update,
                Action::AutoUpdate,
            ),
            button(
                "بررسی نسخهٔ جدید",
                "بررسی دستی بروزرسانی برنامه",
                "بررسی بروزرسانی",
                Action::CheckUpdate,
            ),
        ],
        _ => {
            let installation = installation_state();
            let mut install = button(
                "نصب برنامه",
                "نصب در پوشهٔ Program Files",
                match installation {
                    InstallationState::NotInstalled => "نصب برنامه",
                    InstallationState::UpdateAvailable => "بروزرسانی نصب",
                    _ => "نصب شده",
                },
                Action::Install,
            );
            install.enabled = matches!(
                installation,
                InstallationState::NotInstalled | InstallationState::UpdateAvailable
            );
            let mut uninstall = button(
                "حذف برنامه",
                "حذف نسخهٔ نصب‌شده از ویندوز",
                "حذف برنامه",
                Action::Uninstall,
            );
            uninstall.enabled = installation != InstallationState::NotInstalled;
            vec![
                install,
                uninstall,
                button(
                    "بازنشانی تنظیمات",
                    "بازگردانی همهٔ تنظیمات به حالت اولیه",
                    "بازنشانی…",
                    Action::Reset,
                ),
            ]
        }
    }
}
fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}
fn row_top(row: usize) -> i32 {
    106 + row as i32
        * if PAGE.load(Ordering::SeqCst) == 1 {
            54
        } else {
            62
        }
}
fn control_rect(row: usize, index: usize, count: usize, toggle: bool) -> RECT {
    let top = row_top(row);
    if toggle {
        return rect(38, top + 15, 80, top + 39);
    }
    let width = 240 / count as i32;
    let right = 278 - index as i32 * width;
    rect(right - width + 2, top + 12, right - 2, top + 44)
}
fn contains(r: RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}
pub fn select_page(page: usize) {
    PAGE.store(page.min(4), Ordering::SeqCst);
    FOCUS.store(-1, Ordering::SeqCst);
    settings_window::reveal(rect(0, 0, WIDTH, 88));
    settings_window::repaint_all();
}

fn focus_regions(app: &AppState) -> Vec<RECT> {
    let mut result = vec![rect(20, 20, 52, 52)];
    for i in 0..5 {
        let top = if i == 4 { 506 } else { 94 + i * 58 };
        result.push(rect(558, top, 728, top + 46));
    }
    for (i, r) in rows(app, PAGE.load(Ordering::SeqCst)).iter().enumerate() {
        if r.enabled {
            for j in 0..r.options.len() {
                result.push(control_rect(i, j, r.options.len(), r.toggle));
            }
        }
    }
    result
}
pub fn keyboard(hwnd: HWND, key: usize) -> bool {
    let regions = focus_regions(&state().lock().unwrap());
    if key == 9 {
        let backwards =
            unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) < 0 };
        let current = FOCUS.load(Ordering::SeqCst);
        let next = if current < 0 && backwards {
            regions.len() as isize - 1
        } else {
            (current + if backwards { -1 } else { 1 }).rem_euclid(regions.len() as isize)
        };
        FOCUS.store(next, Ordering::SeqCst);
        settings_window::reveal(regions[next as usize]);
        settings_window::repaint_all();
        return true;
    }
    if key == 13 || key == 32 {
        if let Some(r) = regions.get(FOCUS.load(Ordering::SeqCst) as usize) {
            let scale = settings_window::display_scale();
            click(
                hwnd,
                scaled((r.left + r.right) / 2, scale),
                scaled((r.top + r.bottom) / 2, scale),
            );
        }
        return true;
    }
    false
}
pub unsafe fn paint(hdc: HDC, app: &AppState, p: &Palette, f: &Fonts, scale: u32) {
    unsafe {
        let boxfill =
            |r: RECT, color| draw_round_fill(hdc, scaled_rect(r, scale), color, scaled(8, scale));
        let text = |value: &str, r: RECT, font, color, flags| {
            draw_text(
                hdc,
                value,
                scaled_rect(r, scale),
                color,
                font,
                flags | DT_VCENTER | DT_SINGLELINE,
            )
        };
        let rtl = DT_RIGHT | DT_RTLREADING;
        let page = PAGE.load(Ordering::SeqCst);
        fill_rect_color(
            hdc,
            scaled_rect(rect(546, 0, WIDTH, HEIGHT), scale),
            p.calendar_panel,
        );
        fill_rect_color(hdc, scaled_rect(rect(546, 0, 547, HEIGHT), scale), p.border);
        text("تنظیمات", rect(568, 20, 718, 64), f.title, p.text, rtl);
        for (i, title) in TITLES.iter().enumerate() {
            let top = if i == 4 { 506 } else { 94 + i as i32 * 58 };
            if page == i {
                boxfill(rect(558, top, 728, top + 46), p.selected);
                boxfill(rect(725, top + 8, 729, top + 38), p.accent);
            }
            text(title, rect(568, top, 714, top + 46), f.small, p.text, rtl);
        }
        boxfill(rect(20, 20, 52, 52), p.surface_alt);
        text("×", rect(20, 20, 52, 52), f.medium, p.text, DT_CENTER);
        text(TITLES[page], rect(68, 18, 520, 55), f.title, p.text, rtl);
        text(
            DESCRIPTIONS[page],
            rect(24, 60, 520, 88),
            f.small,
            p.muted,
            rtl,
        );
        for (i, row) in rows(app, page).iter().enumerate() {
            let top = row_top(i);
            boxfill(rect(24, top, 522, top + 50), p.calendar_panel);
            let fg = if row.enabled { p.text } else { p.muted };
            text(
                row.title,
                rect(290, top + 3, 508, top + 29),
                f.small,
                fg,
                rtl,
            );
            text(
                &row.detail,
                rect(290, top + 28, 508, top + 51),
                f.tiny,
                p.muted,
                rtl,
            );
            if row.toggle {
                let r = control_rect(i, 0, 1, true);
                let on = row.selected == Some(1);
                boxfill(
                    r,
                    if on && row.enabled {
                        p.accent
                    } else {
                        p.surface_alt
                    },
                );
                let x = if on { r.right - 21 } else { r.left + 3 };
                boxfill(
                    rect(x, r.top + 3, x + 18, r.bottom - 3),
                    if on && row.enabled {
                        p.accent_text
                    } else {
                        p.muted
                    },
                );
            } else {
                for (j, label) in row.options.iter().enumerate() {
                    let r = control_rect(i, j, row.options.len(), false);
                    let active = row.selected == Some(j) && row.enabled;
                    boxfill(r, if active { p.accent } else { p.surface_alt });
                    let mut tr = r;
                    if let Some(color) = row.swatch {
                        boxfill(
                            rect(r.left + 8, r.top + 7, r.left + 28, r.bottom - 7),
                            rgb(color[0], color[1], color[2]),
                        );
                        tr.left += 32;
                    }
                    text(
                        label,
                        tr,
                        f.small,
                        if active { p.accent_text } else { fg },
                        DT_CENTER,
                    );
                }
            }
        }
        if page == 2 {
            let (icon, owned) = selected_tray_icon(app);
            DrawIconEx(
                hdc,
                scaled(246, scale),
                scaled(456, scale),
                icon,
                scaled(32, scale),
                scaled(32, scale),
                0,
                null_mut(),
                DI_NORMAL,
            );
            text(
                "پیش‌نمایش آیکون",
                rect(24, 498, 520, 526),
                f.small,
                p.muted,
                DT_CENTER,
            );
            if owned {
                DestroyIcon(icon);
            }
        }
        text(
            "تغییرات خودکار ذخیره می‌شوند",
            rect(24, 558, 520, 584),
            f.tiny,
            p.muted,
            rtl,
        );
        if let Some(r) = focus_regions(app).get(FOCUS.load(Ordering::SeqCst) as usize) {
            draw_round_outline(
                hdc,
                scaled_rect(
                    rect(r.left - 2, r.top - 2, r.right + 2, r.bottom + 2),
                    scale,
                ),
                p.text,
                scaled(8, scale),
                1,
            );
        }
    }
}
pub fn click(hwnd: HWND, x: i32, y: i32) {
    let scale = settings_window::display_scale();
    let (x, y) = (unscaled(x, scale), unscaled(y, scale));
    let focus = focus_regions(&state().lock().unwrap())
        .iter()
        .position(|r| contains(*r, x, y));
    FOCUS.store(focus.map(|i| i as isize).unwrap_or(-1), Ordering::SeqCst);
    if contains(rect(20, 20, 52, 52), x, y) {
        settings_window::close();
        return;
    }
    for i in 0..5 {
        let top = if i == 4 { 506 } else { 94 + i as i32 * 58 };
        if contains(rect(558, top, 728, top + 46), x, y) {
            select_page(i);
            return;
        }
    }
    let hit = {
        let app = state().lock().unwrap();
        rows(&app, PAGE.load(Ordering::SeqCst))
            .iter()
            .enumerate()
            .find_map(|(i, r)| {
                if !r.enabled {
                    return None;
                }
                (0..r.options.len())
                    .find(|j| contains(control_rect(i, *j, r.options.len(), r.toggle), x, y))
                    .map(|j| (r.action, j))
            })
    };
    if let Some((action, index)) = hit {
        apply(hwnd, action, index);
    }
}
fn apply(hwnd: HWND, action: Action, index: usize) {
    let main = settings_window::owner();
    match action {
        Action::Primary | Action::Accent => {
            color_picker::open(
                hwnd,
                if matches!(action, Action::Primary) {
                    color_picker::Target::Primary
                } else {
                    color_picker::Target::Accent
                },
            );
            return;
        }
        Action::CheckUpdate => {
            request_manual_update(main);
            return;
        }
        Action::Install | Action::Uninstall => {
            let exit = unsafe {
                if matches!(action, Action::Install) {
                    request_install(main)
                } else {
                    request_uninstall(main)
                }
            };
            if exit {
                EXITING.store(true, Ordering::SeqCst);
                unsafe {
                    DestroyWindow(main);
                }
            }
            settings_window::repaint_all();
            return;
        }
        Action::Reset => {
            let accepted = unsafe {
                confirm_action(
                    hwnd,
                    "بازنشانی تنظیمات",
                    "همهٔ تنظیمات و رنگ‌ها به حالت اولیه بازگردند؟",
                    Some("رنگ‌های سفارشی و انتخاب‌های فعلی بازنشانی می‌شوند."),
                    "بازنشانی",
                    true,
                )
            };
            if !accepted {
                return;
            }
        }
        _ => {}
    }
    let result = {
        let mut app = state().lock().unwrap();
        let previous = app.settings.clone();
        match action {
            Action::Theme => {
                let t = if index == 0 {
                    Theme::Dark
                } else {
                    Theme::Light
                };
                if t != app.settings.theme {
                    theme::ThemeColors::preset(t).apply(&mut app.settings);
                }
            }
            Action::ThemeReset => {
                let t = app.settings.theme;
                theme::ThemeColors::preset(t).apply(&mut app.settings);
            }
            Action::Scale => app.settings.ui_scale = [80, 90, 100, 110, 125][index],
            Action::Calendar => app.set_main_calendar(
                [
                    CalendarKind::Jalali,
                    CalendarKind::Gregorian,
                    CalendarKind::Hijri,
                ][index],
            ),
            Action::Direction => app.settings.calendar_rtl = index == 0,
            Action::Layout => {
                app.settings.compact_day = index == 1;
                app.hovered_cell = None;
                if app.settings.compact_day && app.selected_day.is_none() {
                    let today = app.today_main();
                    app.selected_day =
                        Some(if app.year == today.year && app.month == today.month {
                            today.day
                        } else {
                            1
                        });
                }
                app.event_scroll = 0;
            }
            Action::Jalali => app.settings.show_jalali = !app.settings.show_jalali,
            Action::Gregorian => app.settings.show_gregorian = !app.settings.show_gregorian,
            Action::Hijri => app.settings.show_hijri = !app.settings.show_hijri,
            Action::Subtitles => app.settings.show_subtitles = !app.settings.show_subtitles,
            Action::Events => app.settings.show_events = !app.settings.show_events,
            Action::TrayContent => app.settings.tray_day_icon = index == 1,
            Action::Digits => app.settings.tray_english_digits = index == 1,
            Action::TrayText => app.settings.tray_text_white = index == 0,
            Action::TrayBackground => app.settings.tray_accent_background = index == 1,
            Action::Tooltip => app.settings.show_tray_date = !app.settings.show_tray_date,
            Action::Autostart => {
                let next = !app.settings.autostart;
                if !set_autostart(next) {
                    drop(app);
                    color_picker::report_error(hwnd, "تغییر اجرای خودکار ممکن نشد.");
                    return;
                }
                app.settings.autostart = next;
            }
            Action::AutoUpdate => app.settings.auto_update = !app.settings.auto_update,
            Action::Reset => {
                let defaults = Settings::default();
                if app.settings.autostart != defaults.autostart
                    && !set_autostart(defaults.autostart)
                {
                    drop(app);
                    color_picker::report_error(hwnd, "بازگردانی اجرای خودکار ممکن نشد.");
                    return;
                }
                app.set_main_calendar(defaults.main_calendar);
                app.settings = defaults;
            }
            _ => {}
        }
        let saved = app.settings.try_save();
        if saved.is_err() {
            app.set_main_calendar(previous.main_calendar);
            if previous.autostart != app.settings.autostart {
                let _ = set_autostart(previous.autostart);
            }
            app.settings = previous;
        }
        saved
    };
    if let Err(error) = result {
        color_picker::report_error(hwnd, &format!("ذخیرهٔ تنظیمات ممکن نشد.\n{error}"));
    }
    unsafe {
        resize_main_window(main, true);
        refresh_tray_icon(main);
        refresh_tray_tooltip(main);
    }
    settings_window::repaint_all();
    if state().lock().unwrap().settings.auto_update
        && matches!(action, Action::AutoUpdate | Action::Reset)
        && matches!(update::status(), update::UpdateStatus::Available(_))
    {
        update::start_download(main, WM_UPDATE_STATUS, WM_APPLY_UPDATE);
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    fn fixture() -> AppState {
        AppState {
            settings: Settings {
                tray_day_icon: true,
                tray_english_digits: true,
                ..Settings::default()
            },
            events: EventStore::load(),
            today_gregorian: Date::new(2026, 9, 7),
            year: 1405,
            month: 6,
            selected_day: Some(16),
            event_scroll: 0,
            hovered_cell: None,
        }
    }

    // Off-screen rendering uses the same GDI painter as the real window without touching user settings.
    #[test]
    #[ignore = "manual visual QA; exports the actual settings painter"]
    fn render_settings_previews() {
        unsafe {
            install_embedded_font(GetModuleHandleW(null()));
            let mut app = fixture();
            std::fs::create_dir_all("target/settings-previews").unwrap();
            let dc = CreateCompatibleDC(null_mut());
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: WIDTH,
                    biHeight: HEIGHT,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..zeroed()
                },
                ..zeroed()
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            assert!(!bitmap.is_null());
            let old = SelectObject(dc, bitmap as HGDIOBJ);
            for variant in 0..3 {
                app.settings.theme = if variant == 1 {
                    Theme::Light
                } else {
                    Theme::Dark
                };
                app.settings.primary = if variant == 2 {
                    Some([26, 38, 54])
                } else {
                    None
                };
                app.settings.accent = if variant == 2 {
                    Some([53, 179, 156])
                } else {
                    None
                };
                let palette =
                    Palette::from_colors(theme::ThemeColors::from_settings(&app.settings));
                let fonts = Fonts::create(100);
                for page in 0..5 {
                    PAGE.store(page, Ordering::SeqCst);
                    fill_rect_color(dc, rect(0, 0, WIDTH, HEIGHT), palette.surface);
                    paint(dc, &app, &palette, &fonts, 100);
                    GdiFlush();
                    let pixels = std::slice::from_raw_parts(
                        bits as *const u8,
                        (WIDTH * HEIGHT * 4) as usize,
                    );
                    let mut output = Vec::new();
                    output.extend_from_slice(b"BM");
                    output.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
                    output.extend_from_slice(&[0; 4]);
                    output.extend_from_slice(&54u32.to_le_bytes());
                    output.extend_from_slice(std::slice::from_raw_parts(
                        &info.bmiHeader as *const _ as *const u8,
                        40,
                    ));
                    output.extend_from_slice(pixels);
                    std::fs::write(
                        format!("target/settings-previews/theme-{variant}-page-{page}.bmp"),
                        output,
                    )
                    .unwrap();
                }
                fonts.destroy();
            }
            SelectObject(dc, old);
            DeleteObject(bitmap as HGDIOBJ);
            DeleteDC(dc);
            uninstall_embedded_font();
        }
    }
}
