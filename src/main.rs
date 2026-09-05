#![windows_subsystem = "windows"]

mod calendar;
mod events;
mod settings;
mod update;

use std::ffi::c_void;
use std::fs;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use calendar::{
    CalendarKind, Date, WEEKDAYS_SHORT, add_day, add_month, convert, days_in_month,
    first_weekday_saturday, from_gregorian, month_name, month_range_text, month_range_text_fa,
    to_gregorian,
};
use events::{CalendarEvent, EventStore};
use settings::{Settings, Theme, set_autostart};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
};
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::{
    FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource,
};
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const APP_NAME: &str = "گاه‌یار";
const WEBSITE_URL: &str = "https://emadghasemi.ir";
const GITHUB_URL: &str = "https://github.com/emadgh/GahYar";
const APP_ICON_ID: usize = 1;
const MAIN_CLASS: &str = "GahYarMain";
const ABOUT_CLASS: &str = "GahYarAbout";
const CONFIRM_CLASS: &str = "GahYarConfirm";
const BASE_WIDTH: i32 = 430;
const BASE_HEIGHT_CALENDAR: i32 = 517;
const BASE_EVENTS_HEIGHT: i32 = 136;
const BASE_FOOTER_HEIGHT: i32 = 30;
const BASE_UPDATE_HEIGHT: i32 = 42;
const BASE_SETTINGS_HEIGHT: i32 = 878;
const BASE_HEIGHT_COMPACT: i32 = 126;
const BASE_ABOUT_WIDTH: i32 = 380;
const BASE_ABOUT_HEIGHT: i32 = 300;
const BASE_CONFIRM_WIDTH: i32 = 430;
const BASE_CONFIRM_HEIGHT: i32 = 350;

const GRID_LEFT: i32 = 18;
const GRID_TOP: i32 = 159;
const GRID_CONTENT_TOP_PADDING: i32 = 4;
const CELL_WIDTH: i32 = 56;
const CELL_HEIGHT: i32 = 54;
const GRID_WIDTH: i32 = CELL_WIDTH * 7;
const GRID_BOTTOM_PADDING: i32 = 10;
const EVENTS_TOP: i32 = 503;
const COMPACT_EVENTS_TOP: i32 = 116;

const WM_TRAY: u32 = WM_APP + 1;
const WM_SHOW_EXISTING: u32 = WM_APP + 2;
const WM_UPDATE_STATUS: u32 = WM_APP + 3;
const WM_APPLY_UPDATE: u32 = WM_APP + 4;
const WM_MOUSE_LEAVE: u32 = 0x02A3;
const TRAY_ID: u32 = 1;
const CMD_OPEN: usize = 1001;
const CMD_SETTINGS: usize = 1002;
const CMD_ABOUT: usize = 1003;
const CMD_UPDATE: usize = 1004;
const CMD_EXIT: usize = 1005;
const DATE_REFRESH_TIMER_ID: usize = 1;
const UPDATE_CHECK_TIMER_ID: usize = 2;
const UPDATE_CHECK_INTERVAL_MS: u32 = 6 * 60 * 60 * 1000;
const CF_UNICODETEXT_FORMAT: u32 = 13;
const INSTANCE_MUTEX_NAME: &str = "Local\\GahYar.SingleInstance";
const FONT_RESOURCE_ID: usize = 101;
const RCDATA_RESOURCE_TYPE: usize = 10;

static STATE: OnceLock<Mutex<AppState>> = OnceLock::new();
static EXITING: AtomicBool = AtomicBool::new(false);
static MANUAL_UPDATE_REQUEST: AtomicBool = AtomicBool::new(false);
static ABOUT_HWND: AtomicIsize = AtomicIsize::new(0);
static CONFIRM_HWND: AtomicIsize = AtomicIsize::new(0);
static CUSTOM_TRAY_ICON: AtomicIsize = AtomicIsize::new(0);
static FONT_RESOURCE_HANDLE: AtomicIsize = AtomicIsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Calendar,
    Settings,
}

struct AppState {
    settings: Settings,
    events: EventStore,
    today_gregorian: Date,
    year: i32,
    month: u32,
    selected_day: Option<u32>,
    view: ViewMode,
    event_scroll: usize,
    hovered_cell: Option<i32>,
}

struct ConfirmDialogState {
    title: String,
    message: String,
    reminder: Option<String>,
    accept_label: String,
    destructive: bool,
    accepted: bool,
}

impl AppState {
    fn new() -> Self {
        let mut local: SYSTEMTIME = unsafe { zeroed() };
        unsafe { GetLocalTime(&mut local) };
        let today_gregorian = Date::new(local.wYear as i32, local.wMonth as u32, local.wDay as u32);
        let settings = Settings::load();
        let today_main = from_gregorian(settings.main_calendar, today_gregorian);
        Self {
            settings,
            events: EventStore::load(),
            today_gregorian,
            year: today_main.year,
            month: today_main.month,
            selected_day: Some(today_main.day),
            view: ViewMode::Calendar,
            event_scroll: 0,
            hovered_cell: None,
        }
    }

    fn base_height(&self) -> i32 {
        let update_height = if !self.settings.auto_update && update::banner_visible() {
            BASE_UPDATE_HEIGHT
        } else {
            0
        };
        (if self.view == ViewMode::Settings {
            BASE_SETTINGS_HEIGHT + BASE_FOOTER_HEIGHT
        } else {
            (if self.settings.compact_day {
                BASE_HEIGHT_COMPACT
            } else {
                BASE_HEIGHT_CALENDAR
            }) + if self.settings.show_events {
                BASE_EVENTS_HEIGHT
            } else {
                0
            } + BASE_FOOTER_HEIGHT
        }) + update_height
    }

    fn scale(&self) -> u32 {
        self.settings.ui_scale
    }

    fn today_main(&self) -> Date {
        from_gregorian(self.settings.main_calendar, self.today_gregorian)
    }

    fn refresh_today(&mut self) -> bool {
        let mut local: SYSTEMTIME = unsafe { zeroed() };
        unsafe { GetLocalTime(&mut local) };
        let today = Date::new(local.wYear as i32, local.wMonth as u32, local.wDay as u32);
        if today == self.today_gregorian {
            return false;
        }
        self.today_gregorian = today;
        true
    }

    fn set_main_calendar(&mut self, next: CalendarKind) {
        let anchor_day = self.selected_day.unwrap_or(1).min(days_in_month(
            self.settings.main_calendar,
            self.year,
            self.month,
        ));
        let anchor_g = to_gregorian(
            self.settings.main_calendar,
            Date::new(self.year, self.month, anchor_day),
        );
        self.settings.main_calendar = next;
        let converted = from_gregorian(next, anchor_g);
        self.year = converted.year;
        self.month = converted.month;
        self.selected_day = Some(converted.day);
        self.event_scroll = 0;
    }
}

fn state() -> &'static Mutex<AppState> {
    STATE.get_or_init(|| Mutex::new(AppState::new()))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn taskbar_created_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe {
        let name = wide("TaskbarCreated");
        RegisterWindowMessageW(name.as_ptr())
    })
}

fn persian_digits(value: impl ToString) -> String {
    value
        .to_string()
        .chars()
        .map(|character| match character {
            '0' => '۰',
            '1' => '۱',
            '2' => '۲',
            '3' => '۳',
            '4' => '۴',
            '5' => '۵',
            '6' => '۶',
            '7' => '۷',
            '8' => '۸',
            '9' => '۹',
            other => other,
        })
        .collect()
}

fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | (green as u32) << 8 | (blue as u32) << 16
}

fn scaled(value: i32, scale: u32) -> i32 {
    ((value as i64 * scale as i64 + 50) / 100) as i32
}

fn unscaled(value: i32, scale: u32) -> i32 {
    ((value as i64 * 100 + scale as i64 / 2) / scale as i64) as i32
}

fn scaled_rect(rect: RECT, scale: u32) -> RECT {
    RECT {
        left: scaled(rect.left, scale),
        top: scaled(rect.top, scale),
        right: scaled(rect.right, scale),
        bottom: scaled(rect.bottom, scale),
    }
}

fn calendar_column(column: i32, rtl: bool) -> i32 {
    if rtl { 6 - column } else { column }
}

#[derive(Clone, Copy)]
struct Palette {
    background: COLORREF,
    surface: COLORREF,
    surface_alt: COLORREF,
    calendar_panel: COLORREF,
    text: COLORREF,
    muted: COLORREF,
    faint: COLORREF,
    accent: COLORREF,
    accent_text: COLORREF,
    holiday: COLORREF,
    border: COLORREF,
    event: COLORREF,
    selected: COLORREF,
}

impl Palette {
    fn from_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self {
                background: rgb(30, 31, 33),
                surface: rgb(43, 45, 48),
                surface_alt: rgb(53, 56, 60),
                calendar_panel: rgb(32, 35, 39),
                text: rgb(240, 241, 243),
                muted: rgb(176, 183, 192),
                faint: rgb(91, 97, 104),
                accent: rgb(248, 211, 88),
                accent_text: rgb(24, 24, 24),
                holiday: rgb(255, 104, 104),
                border: rgb(70, 70, 70),
                event: rgb(126, 180, 255),
                selected: rgb(64, 58, 38),
            },
            Theme::Light => Self {
                background: rgb(231, 233, 236),
                surface: rgb(250, 250, 250),
                surface_alt: rgb(241, 243, 246),
                calendar_panel: rgb(245, 246, 248),
                text: rgb(27, 31, 36),
                muted: rgb(91, 98, 107),
                faint: rgb(176, 181, 188),
                accent: rgb(230, 181, 43),
                accent_text: rgb(29, 25, 12),
                holiday: rgb(206, 49, 49),
                border: rgb(211, 214, 219),
                event: rgb(32, 100, 191),
                selected: rgb(255, 246, 205),
            },
        }
    }
}

unsafe fn create_font(height: i32, weight: i32, family: &str) -> HFONT {
    let family = wide(family);
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32 | FF_DONTCARE as u32,
            family.as_ptr(),
        )
    }
}

unsafe fn fill_rect_color(hdc: HDC, rect: RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(hdc, &rect, brush);
        DeleteObject(brush as HGDIOBJ);
    }
}

unsafe fn draw_round_fill(hdc: HDC, rect: RECT, color: COLORREF, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let old_brush = SelectObject(hdc, brush as HGDIOBJ);
        let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN) as HGDIOBJ);
        RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius,
            radius,
        );
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        DeleteObject(brush as HGDIOBJ);
    }
}

unsafe fn draw_round_outline(hdc: HDC, rect: RECT, color: COLORREF, radius: i32, width: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width.max(1), color);
        let old_pen = SelectObject(hdc, pen as HGDIOBJ);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH) as HGDIOBJ);
        RoundRect(
            hdc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius,
            radius,
        );
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(pen as HGDIOBJ);
    }
}

unsafe fn draw_text(
    hdc: HDC,
    text: &str,
    mut rect: RECT,
    color: COLORREF,
    font: HFONT,
    format: u32,
) {
    unsafe {
        let old_font = SelectObject(hdc, font as HGDIOBJ);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, color);
        let mut content = wide(text);
        DrawTextW(hdc, content.as_mut_ptr(), -1, &mut rect, format);
        SelectObject(hdc, old_font);
    }
}

struct Fonts {
    tiny: HFONT,
    small: HFONT,
    regular: HFONT,
    medium: HFONT,
    day: HFONT,
    title: HFONT,
    icon: HFONT,
}

impl Fonts {
    unsafe fn create(scale: u32) -> Self {
        unsafe {
            Self {
                tiny: create_font(-scaled(11, scale), FW_NORMAL as i32, "Vazirmatn"),
                small: create_font(-scaled(13, scale), FW_NORMAL as i32, "Vazirmatn"),
                regular: create_font(-scaled(15, scale), FW_NORMAL as i32, "Vazirmatn"),
                medium: create_font(-scaled(16, scale), FW_SEMIBOLD as i32, "Vazirmatn"),
                day: create_font(-scaled(22, scale), FW_SEMIBOLD as i32, "Vazirmatn"),
                title: create_font(-scaled(23, scale), FW_BOLD as i32, "Vazirmatn"),
                icon: create_font(-scaled(20, scale), FW_NORMAL as i32, "Segoe UI Symbol"),
            }
        }
    }

    unsafe fn destroy(self) {
        unsafe {
            DeleteObject(self.tiny as HGDIOBJ);
            DeleteObject(self.small as HGDIOBJ);
            DeleteObject(self.regular as HGDIOBJ);
            DeleteObject(self.medium as HGDIOBJ);
            DeleteObject(self.day as HGDIOBJ);
            DeleteObject(self.title as HGDIOBJ);
            DeleteObject(self.icon as HGDIOBJ);
        }
    }
}

fn main_date_heading(app: &AppState) -> String {
    const WEEKDAYS: [&str; 7] = [
        "شنبه",
        "یکشنبه",
        "دوشنبه",
        "سه‌شنبه",
        "چهارشنبه",
        "پنجشنبه",
        "جمعه",
    ];
    let today = app.today_main();
    let day = app.selected_day.unwrap_or_else(|| {
        if app.year == today.year && app.month == today.month {
            today.day
        } else {
            1
        }
    });
    let weekday =
        (first_weekday_saturday(app.settings.main_calendar, app.year, app.month) + day as i32 - 1)
            .rem_euclid(7) as usize;
    let (day_text, year_text) = match app.settings.main_calendar {
        CalendarKind::Gregorian => (day.to_string(), app.year.to_string()),
        CalendarKind::Jalali | CalendarKind::Hijri => {
            (persian_digits(day), persian_digits(app.year))
        }
    };
    format!(
        "{}، {} {} {}",
        WEEKDAYS[weekday],
        day_text,
        month_name(app.settings.main_calendar, app.month),
        year_text
    )
}

fn secondary_ranges(app: &AppState) -> Vec<String> {
    if !app.settings.show_subtitles {
        return Vec::new();
    }
    let mut values = Vec::new();
    let main = app.settings.main_calendar;
    if main != CalendarKind::Gregorian && app.settings.show_gregorian {
        values.push(month_range_text(
            main,
            app.year,
            app.month,
            CalendarKind::Gregorian,
        ));
    }
    if main != CalendarKind::Jalali && app.settings.show_jalali {
        values.push(month_range_text_fa(
            main,
            app.year,
            app.month,
            CalendarKind::Jalali,
            |year| persian_digits(year),
        ));
    }
    if main != CalendarKind::Hijri && app.settings.show_hijri {
        values.push(month_range_text_fa(
            main,
            app.year,
            app.month,
            CalendarKind::Hijri,
            |year| persian_digits(year),
        ));
    }
    values.truncate(2);
    values
}

fn adjacent_date(kind: CalendarKind, year: i32, month: u32, cell_index: i32) -> (Date, bool) {
    let first = first_weekday_saturday(kind, year, month);
    let current_days = days_in_month(kind, year, month) as i32;
    let relative = cell_index - first + 1;
    if relative >= 1 && relative <= current_days {
        return (Date::new(year, month, relative as u32), true);
    }
    if relative < 1 {
        let mut previous_year = year;
        let mut previous_month = month;
        add_month(kind, &mut previous_year, &mut previous_month, -1);
        let day = days_in_month(kind, previous_year, previous_month) as i32 + relative;
        return (Date::new(previous_year, previous_month, day as u32), false);
    }
    let mut next_year = year;
    let mut next_month = month;
    add_month(kind, &mut next_year, &mut next_month, 1);
    (
        Date::new(next_year, next_month, (relative - current_days) as u32),
        false,
    )
}

fn secondary_dates(app: &AppState, primary: Date) -> Vec<(CalendarKind, Date)> {
    let mut values = Vec::new();
    let main = app.settings.main_calendar;
    for kind in [
        CalendarKind::Jalali,
        CalendarKind::Gregorian,
        CalendarKind::Hijri,
    ] {
        let enabled = match kind {
            CalendarKind::Jalali => app.settings.show_jalali,
            CalendarKind::Gregorian => app.settings.show_gregorian,
            CalendarKind::Hijri => app.settings.show_hijri,
        };
        if enabled && kind != main {
            values.push((kind, convert(primary, main, kind)));
        }
    }
    values
}

fn event_items_for_view<'a>(app: &'a AppState) -> Vec<(u32, &'a CalendarEvent)> {
    let mut items = Vec::new();
    let main = app.settings.main_calendar;
    let first = app.selected_day.unwrap_or(1);
    let last = app
        .selected_day
        .unwrap_or(days_in_month(main, app.year, app.month));
    for day in first..=last {
        let primary = Date::new(app.year, app.month, day);
        let jalali = convert(primary, main, CalendarKind::Jalali);
        for event in app
            .events
            .events_for_day(jalali.year, jalali.month, jalali.day)
        {
            items.push((day, event));
        }
    }
    items
}

fn events_top(app: &AppState) -> i32 {
    if app.settings.compact_day {
        COMPACT_EVENTS_TOP
    } else {
        EVENTS_TOP
    }
}

fn move_calendar_day(app: &mut AppState, delta: i32) {
    let current = app.selected_day.unwrap_or_else(|| {
        let today = app.today_main();
        if app.year == today.year && app.month == today.month {
            today.day
        } else {
            1
        }
    });
    let next = add_day(
        app.settings.main_calendar,
        Date::new(app.year, app.month, current),
        delta,
    );
    app.year = next.year;
    app.month = next.month;
    app.selected_day = Some(next.day);
    app.event_scroll = 0;
}

unsafe fn paint_main(hwnd: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = zeroed();
        let window_hdc = BeginPaint(hwnd, &mut ps);
        if window_hdc.is_null() {
            return;
        }

        let app = state().lock().unwrap();
        let scale = app.scale();
        let palette = Palette::from_theme(app.settings.theme);
        let fonts = Fonts::create(scale);
        let width = scaled(BASE_WIDTH, scale);
        let height = scaled(app.base_height(), scale);
        let hdc = CreateCompatibleDC(window_hdc);
        let bitmap = CreateCompatibleBitmap(window_hdc, width, height);
        if hdc.is_null() || bitmap.is_null() {
            if !hdc.is_null() {
                DeleteDC(hdc);
            }
            if !bitmap.is_null() {
                DeleteObject(bitmap as HGDIOBJ);
            }
            drop(app);
            fonts.destroy();
            EndPaint(hwnd, &ps);
            return;
        }
        let old_bitmap = SelectObject(hdc, bitmap as HGDIOBJ);
        fill_rect_color(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            palette.background,
        );
        draw_round_fill(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            palette.surface,
            scaled(18, scale),
        );

        match app.view {
            ViewMode::Calendar => paint_calendar(hdc, &app, &palette, &fonts),
            ViewMode::Settings => paint_settings(hdc, &app, &palette, &fonts),
        }

        draw_round_outline(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width - 1,
                bottom: height - 1,
            },
            palette.border,
            scaled(18, scale),
            1,
        );
        BitBlt(window_hdc, 0, 0, width, height, hdc, 0, 0, SRCCOPY);
        SelectObject(hdc, old_bitmap);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(hdc);
        drop(app);
        fonts.destroy();
        EndPaint(hwnd, &ps);
    }
}

unsafe fn paint_calendar(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts) {
    unsafe {
        let scale = app.scale();
        let sr = |rect| scaled_rect(rect, scale);

        // Navigation, title and utility buttons.
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 14,
                top: 12,
                right: 66,
                bottom: 44,
            }),
            palette.surface_alt,
            scaled(12, scale),
        );
        draw_text(
            hdc,
            "‹",
            sr(RECT {
                left: 14,
                top: 8,
                right: 66,
                bottom: 44,
            }),
            palette.accent,
            fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 364,
                top: 12,
                right: 416,
                bottom: 44,
            }),
            palette.surface_alt,
            scaled(12, scale),
        );
        draw_text(
            hdc,
            "›",
            sr(RECT {
                left: 364,
                top: 8,
                right: 416,
                bottom: 44,
            }),
            palette.accent,
            fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );

        draw_text(
            hdc,
            &main_date_heading(app),
            sr(RECT {
                left: 72,
                top: 7,
                right: 358,
                bottom: 43,
            }),
            palette.accent,
            fonts.medium,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );

        let compact = app.settings.compact_day;
        if !compact {
            let ranges = secondary_ranges(app);
            if let Some(first) = ranges.first() {
                draw_text(
                    hdc,
                    first,
                    sr(RECT {
                        left: 58,
                        top: 44,
                        right: 372,
                        bottom: 62,
                    }),
                    palette.text,
                    fonts.small,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                );
            }
            if let Some(second) = ranges.get(1) {
                draw_text(
                    hdc,
                    second,
                    sr(RECT {
                        left: 58,
                        top: 62,
                        right: 372,
                        bottom: 80,
                    }),
                    palette.muted,
                    fonts.small,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                );
            }
        }

        draw_round_fill(
            hdc,
            sr(RECT {
                left: 16,
                top: 57,
                right: 50,
                bottom: 89,
            }),
            palette.surface_alt,
            scaled(11, scale),
        );
        draw_text(
            hdc,
            "ⓘ",
            sr(RECT {
                left: 16,
                top: 56,
                right: 50,
                bottom: 88,
            }),
            palette.muted,
            fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 380,
                top: 57,
                right: 414,
                bottom: 89,
            }),
            palette.surface_alt,
            scaled(11, scale),
        );
        draw_text(
            hdc,
            "⚙",
            sr(RECT {
                left: 380,
                top: 55,
                right: 414,
                bottom: 89,
            }),
            palette.muted,
            fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if !compact {
            let today = app.today_main();
            let today_is_active = app.year == today.year
                && app.month == today.month
                && app.selected_day == Some(today.day);
            if !today_is_active {
                draw_round_fill(
                    hdc,
                    sr(RECT {
                        left: 174,
                        top: 83,
                        right: 256,
                        bottom: 108,
                    }),
                    palette.accent,
                    scaled(12, scale),
                );
                draw_text(
                    hdc,
                    "برو به امروز",
                    sr(RECT {
                        left: 174,
                        top: 83,
                        right: 256,
                        bottom: 108,
                    }),
                    palette.accent_text,
                    fonts.tiny,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                );
            }
        }

        if !compact {
            // Weekday header.
            draw_round_fill(
                hdc,
                sr(RECT {
                    left: GRID_LEFT,
                    top: 116,
                    right: GRID_LEFT + GRID_WIDTH,
                    bottom: 150,
                }),
                palette.accent,
                scaled(10, scale),
            );
            for (index, weekday) in WEEKDAYS_SHORT.iter().enumerate() {
                let visual_column = calendar_column(index as i32, app.settings.calendar_rtl);
                let left = GRID_LEFT + visual_column * CELL_WIDTH;
                draw_text(
                    hdc,
                    weekday,
                    sr(RECT {
                        left,
                        top: 116,
                        right: left + CELL_WIDTH,
                        bottom: 150,
                    }),
                    palette.accent_text,
                    fonts.medium,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                );
            }

            // Calendar grid.
            draw_round_fill(
                hdc,
                sr(RECT {
                    left: GRID_LEFT,
                    top: GRID_TOP,
                    right: GRID_LEFT + GRID_WIDTH,
                    bottom: GRID_TOP + CELL_HEIGHT * 6 + GRID_BOTTOM_PADDING,
                }),
                palette.calendar_panel,
                scaled(14, scale),
            );

            for cell in 0..42 {
                let row = cell / 7;
                let column = cell % 7;
                let visual_column = calendar_column(column, app.settings.calendar_rtl);
                let (primary, in_current_month) =
                    adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
                let gregorian = to_gregorian(app.settings.main_calendar, primary);
                let jalali = from_gregorian(CalendarKind::Jalali, gregorian);
                let is_today = gregorian == app.today_gregorian;
                let is_selected = in_current_month && app.selected_day == Some(primary.day);
                let is_friday = column == 6;
                let official_holiday =
                    app.events
                        .is_official_holiday(jalali.year, jalali.month, jalali.day);
                let has_events = !app
                    .events
                    .events_for_day(jalali.year, jalali.month, jalali.day)
                    .is_empty();

                let left = GRID_LEFT + visual_column * CELL_WIDTH;
                let top = GRID_TOP + GRID_CONTENT_TOP_PADDING + row * CELL_HEIGHT;
                let cell_rect = sr(RECT {
                    left: left + 3,
                    top: top + 3,
                    right: left + CELL_WIDTH - 3,
                    bottom: top + CELL_HEIGHT - 3,
                });
                if is_selected {
                    draw_round_fill(hdc, cell_rect, palette.selected, scaled(9, scale));
                    draw_round_outline(
                        hdc,
                        cell_rect,
                        palette.accent,
                        scaled(9, scale),
                        scaled(2, scale),
                    );
                } else if is_today {
                    draw_round_outline(
                        hdc,
                        cell_rect,
                        palette.accent,
                        scaled(9, scale),
                        scaled(2, scale),
                    );
                }

                let primary_color = if !in_current_month {
                    palette.faint
                } else if is_friday || official_holiday {
                    palette.holiday
                } else {
                    palette.text
                };
                draw_text(
                    hdc,
                    &secondary_day_text(app.settings.main_calendar, primary.day),
                    sr(RECT {
                        left: left + 5,
                        top: top + 4,
                        right: left + CELL_WIDTH - 5,
                        bottom: top + 31,
                    }),
                    primary_color,
                    fonts.day,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                );

                let secondary = secondary_dates(app, primary);
                let secondary_color = if in_current_month {
                    palette.muted
                } else {
                    palette.faint
                };
                if secondary.len() == 1 {
                    let (kind, date) = secondary[0];
                    draw_text(
                        hdc,
                        &secondary_day_text(kind, date.day),
                        sr(RECT {
                            left: left + 3,
                            top: top + 32,
                            right: left + CELL_WIDTH - 3,
                            bottom: top + 49,
                        }),
                        secondary_color,
                        fonts.tiny,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                    );
                } else if secondary.len() >= 2 {
                    let (first_kind, first_date) = secondary[0];
                    let (second_kind, second_date) = secondary[1];
                    draw_text(
                        hdc,
                        &secondary_day_text(first_kind, first_date.day),
                        sr(RECT {
                            left: left + 3,
                            top: top + 32,
                            right: left + CELL_WIDTH / 2 + 1,
                            bottom: top + 49,
                        }),
                        secondary_color,
                        fonts.tiny,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                    );
                    draw_text(
                        hdc,
                        &secondary_day_text(second_kind, second_date.day),
                        sr(RECT {
                            left: left + CELL_WIDTH / 2 - 1,
                            top: top + 32,
                            right: left + CELL_WIDTH - 3,
                            bottom: top + 49,
                        }),
                        secondary_color,
                        fonts.tiny,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                    );
                }

                if has_events {
                    let dot_color = if official_holiday {
                        palette.holiday
                    } else {
                        palette.event
                    };
                    let dot_rect = sr(RECT {
                        left: left + CELL_WIDTH / 2 - 2,
                        top: top + 49,
                        right: left + CELL_WIDTH / 2 + 3,
                        bottom: top + 53,
                    });
                    draw_round_fill(hdc, dot_rect, dot_color, scaled(3, scale));
                }
            }
        }

        paint_day_tooltip(hdc, app, palette, fonts);

        if app.settings.show_events {
            paint_events(hdc, app, palette, fonts);
        }
        paint_footer(hdc, app, palette, fonts);
    }
}

unsafe fn paint_day_tooltip(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts) {
    let Some(cell) = app.hovered_cell else {
        return;
    };
    let (primary, _) = adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
    let jalali = convert(primary, app.settings.main_calendar, CalendarKind::Jalali);
    let events = app
        .events
        .events_for_day(jalali.year, jalali.month, jalali.day);
    if events.is_empty() {
        return;
    }
    let text = events
        .iter()
        .take(3)
        .map(|event| format!("• {}", event.title))
        .collect::<Vec<_>>()
        .join("\n");
    let row = cell / 7;
    let logical_column = cell % 7;
    let visual_column = calendar_column(logical_column, app.settings.calendar_rtl);
    let width = 286;
    let height = 34 + events.len().min(3) as i32 * 22;
    let center = GRID_LEFT + visual_column * CELL_WIDTH + CELL_WIDTH / 2;
    let left = (center - width / 2).clamp(12, BASE_WIDTH - width - 12);
    let top = if row <= 2 {
        GRID_TOP + (row + 1) * CELL_HEIGHT + 4
    } else {
        GRID_TOP + row * CELL_HEIGHT - height - 4
    };
    unsafe {
        let scale = app.scale();
        let rect = scaled_rect(
            RECT {
                left,
                top,
                right: left + width,
                bottom: top + height,
            },
            scale,
        );
        draw_round_fill(hdc, rect, palette.surface_alt, scaled(10, scale));
        draw_round_outline(hdc, rect, palette.accent, scaled(10, scale), 1);
        draw_text(
            hdc,
            &text,
            scaled_rect(
                RECT {
                    left: left + 12,
                    top: top + 7,
                    right: left + width - 12,
                    bottom: top + height - 7,
                },
                scale,
            ),
            palette.text,
            fonts.small,
            DT_RIGHT | DT_VCENTER | DT_WORDBREAK | DT_RTLREADING,
        );
    }
}

unsafe fn update_day_hover(hwnd: HWND, x: i32, y: i32) {
    let changed = {
        let mut app = state().lock().unwrap();
        let x = unscaled(x, app.scale());
        let y = unscaled(y, app.scale());
        let next = if app.view == ViewMode::Calendar
            && (GRID_LEFT..GRID_LEFT + GRID_WIDTH).contains(&x)
            && (GRID_TOP..GRID_TOP + CELL_HEIGHT * 6).contains(&y)
        {
            let visual_column = (x - GRID_LEFT) / CELL_WIDTH;
            let column = calendar_column(visual_column, app.settings.calendar_rtl);
            let row = (y - GRID_TOP) / CELL_HEIGHT;
            let cell = row * 7 + column;
            let (primary, _) = adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
            let jalali = convert(primary, app.settings.main_calendar, CalendarKind::Jalali);
            if app
                .events
                .events_for_day(jalali.year, jalali.month, jalali.day)
                .is_empty()
            {
                None
            } else {
                Some(cell)
            }
        } else {
            None
        };
        let changed = app.hovered_cell != next;
        app.hovered_cell = next;
        changed
    };
    if changed {
        unsafe {
            InvalidateRect(hwnd, null(), 0);
        }
    }
}

fn secondary_day_text(kind: CalendarKind, day: u32) -> String {
    match kind {
        CalendarKind::Gregorian => day.to_string(),
        CalendarKind::Jalali | CalendarKind::Hijri => persian_digits(day),
    }
}

unsafe fn paint_events(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts) {
    unsafe {
        let scale = app.scale();
        let sr = |rect| scaled_rect(rect, scale);
        let top = events_top(app);
        let bottom = top + BASE_EVENTS_HEIGHT - 8;
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 18,
                top,
                right: 412,
                bottom,
            }),
            palette.surface_alt,
            scaled(13, scale),
        );

        let section_title = if let Some(day) = app.selected_day {
            format!("مناسبت‌های روز {}", persian_digits(day))
        } else {
            "مناسبت‌های این ماه".to_string()
        };
        draw_text(
            hdc,
            &section_title,
            sr(RECT {
                left: 30,
                top: top + 8,
                right: 400,
                bottom: top + 34,
            }),
            palette.text,
            fonts.medium,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );

        let items = event_items_for_view(app);
        if items.is_empty() {
            let message = if app.events.source_year
                == convert(
                    Date::new(app.year, app.month, 1),
                    app.settings.main_calendar,
                    CalendarKind::Jalali,
                )
                .year
            {
                "برای این بازه مناسبتی ثبت نشده است."
            } else {
                "فایل رویداد پیوست فقط اطلاعات سال ۱۴۰۵ را دارد."
            };
            draw_text(
                hdc,
                message,
                sr(RECT {
                    left: 30,
                    top: top + 39,
                    right: 398,
                    bottom: bottom - 8,
                }),
                palette.muted,
                fonts.small,
                DT_RIGHT | DT_VCENTER | DT_WORDBREAK | DT_RTLREADING,
            );
            return;
        }

        let item_count = items.len();
        let max_scroll = item_count.saturating_sub(3);
        let start = app.event_scroll.min(max_scroll);
        let mut y = top + 38;
        for (day, event) in items.into_iter().skip(start).take(3) {
            let item_bottom = y + 28;
            let badge_color = if event.official_holiday {
                palette.holiday
            } else {
                palette.event
            };
            draw_round_fill(
                hdc,
                sr(RECT {
                    left: 356,
                    top: y + 2,
                    right: 399,
                    bottom: item_bottom,
                }),
                palette.calendar_panel,
                scaled(10, scale),
            );
            draw_text(
                hdc,
                &persian_digits(day),
                sr(RECT {
                    left: 356,
                    top: y + 2,
                    right: 399,
                    bottom: item_bottom,
                }),
                badge_color,
                fonts.small,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
            );
            draw_text(
                hdc,
                &event.title,
                sr(RECT {
                    left: 30,
                    top: y,
                    right: 348,
                    bottom: item_bottom,
                }),
                if event.official_holiday {
                    palette.holiday
                } else {
                    palette.text
                },
                fonts.small,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_RTLREADING,
            );
            y += 30;
        }

        if item_count > 3 {
            let track_top = top + 40;
            let track_bottom = bottom - 10;
            let track_height = track_bottom - track_top;
            let thumb_height = ((track_height * 3) / item_count as i32).max(18);
            let max_offset = track_height - thumb_height;
            let thumb_offset = if max_scroll == 0 {
                0
            } else {
                max_offset * start as i32 / max_scroll as i32
            };
            draw_round_fill(
                hdc,
                sr(RECT {
                    left: 22,
                    top: track_top,
                    right: 27,
                    bottom: track_bottom,
                }),
                palette.border,
                scaled(3, scale),
            );
            draw_round_fill(
                hdc,
                sr(RECT {
                    left: 21,
                    top: track_top + thumb_offset,
                    right: 28,
                    bottom: track_top + thumb_offset + thumb_height,
                }),
                palette.accent,
                scaled(4, scale),
            );
        }
    }
}

unsafe fn paint_footer(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts) {
    unsafe {
        let scale = app.scale();
        let top = app.base_height() - BASE_FOOTER_HEIGHT;
        paint_update_banner(hdc, app, palette, fonts, top);
        draw_text(
            hdc,
            "عماد قاسمی - emadghasemi.ir",
            scaled_rect(
                RECT {
                    left: 184,
                    top,
                    right: BASE_WIDTH - 12,
                    bottom: top + BASE_FOOTER_HEIGHT - 3,
                },
                scale,
            ),
            palette.accent,
            fonts.tiny,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_text(
            hdc,
            "|",
            scaled_rect(
                RECT {
                    left: 172,
                    top,
                    right: 184,
                    bottom: top + BASE_FOOTER_HEIGHT - 3,
                },
                scale,
            ),
            palette.muted,
            fonts.tiny,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            &format!("گاه‌یار نسخه {}", persian_digits(env!("CARGO_PKG_VERSION"))),
            scaled_rect(
                RECT {
                    left: 10,
                    top,
                    right: 172,
                    bottom: top + BASE_FOOTER_HEIGHT - 3,
                },
                scale,
            ),
            palette.accent,
            fonts.tiny,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
    }
}

unsafe fn paint_update_banner(
    hdc: HDC,
    app: &AppState,
    palette: &Palette,
    fonts: &Fonts,
    footer_top: i32,
) {
    if app.settings.auto_update {
        return;
    }
    let status = update::status();
    let (text, color) = match status {
        update::UpdateStatus::Available(info) => (
            format!(
                "نسخه جدید {} منتشر شده — برای بروزرسانی کلیک کنید",
                persian_digits(info.version)
            ),
            palette.accent,
        ),
        update::UpdateStatus::Downloading => {
            ("در حال دریافت و نصب نسخه جدید…".to_string(), palette.event)
        }
        update::UpdateStatus::Failed(_) => (
            "بروزرسانی خودکار ناموفق بود — دانلود دستی".to_string(),
            palette.holiday,
        ),
        _ => return,
    };
    unsafe {
        let scale = app.scale();
        let top = footer_top - BASE_UPDATE_HEIGHT;
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 18,
                    top: top + 4,
                    right: BASE_WIDTH - 18,
                    bottom: footer_top - 3,
                },
                scale,
            ),
            palette.surface_alt,
            scaled(11, scale),
        );
        draw_text(
            hdc,
            &text,
            scaled_rect(
                RECT {
                    left: 28,
                    top: top + 4,
                    right: BASE_WIDTH - 28,
                    bottom: footer_top - 3,
                },
                scale,
            ),
            color,
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
    }
}

unsafe fn paint_settings(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts) {
    unsafe {
        let scale = app.scale();
        let sr = |rect| scaled_rect(rect, scale);
        draw_text(
            hdc,
            "تنظیمات",
            sr(RECT {
                left: 70,
                top: 10,
                right: 360,
                bottom: 48,
            }),
            palette.accent,
            fonts.title,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 370,
                top: 12,
                right: 416,
                bottom: 46,
            }),
            palette.surface_alt,
            scaled(11, scale),
        );
        draw_text(
            hdc,
            "›",
            sr(RECT {
                left: 370,
                top: 8,
                right: 416,
                bottom: 46,
            }),
            palette.accent,
            fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            "تقویم اصلی همیشه نمایش داده می‌شود؛ موارد زیر برای نمایش تقویم‌های جانبی هستند.",
            sr(RECT {
                left: 26,
                top: 48,
                right: 404,
                bottom: 76,
            }),
            palette.muted,
            fonts.tiny,
            DT_RIGHT | DT_VCENTER | DT_WORDBREAK | DT_RTLREADING,
        );

        paint_value_row(
            hdc,
            app,
            palette,
            fonts,
            78,
            "پوسته",
            app.settings.theme.title(),
        );
        paint_scale_row(hdc, app, palette, fonts, 122);
        paint_value_row(
            hdc,
            app,
            palette,
            fonts,
            166,
            "تقویم اصلی",
            app.settings.main_calendar.title(),
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            218,
            "چیدمان تقویم از راست به چپ",
            app.settings.calendar_rtl,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            262,
            "نمایش تاریخ شمسی",
            app.settings.show_jalali,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            306,
            "نمایش تاریخ میلادی",
            app.settings.show_gregorian,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            350,
            "نمایش تاریخ قمری",
            app.settings.show_hijri,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            394,
            "نمایش عنوان تقویم‌های جانبی",
            app.settings.show_subtitles,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            438,
            "نمایش بخش مناسبت‌ها",
            app.settings.show_events,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            482,
            "نمایش تاریخ کامل در Tooltip",
            app.settings.show_tray_date,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            526,
            "بروزرسانی خودکار",
            app.settings.auto_update,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            570,
            "نمایش شماره روز روی آیکن Tray",
            app.settings.tray_day_icon,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            614,
            "نمایش شماره انگلیسی در System Tray",
            app.settings.tray_english_digits,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            658,
            "نمایش روزانه (بدون تقویم)",
            app.settings.compact_day,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            702,
            "اجرا همراه با ویندوز",
            app.settings.autostart,
        );

        let installation = installation_state();
        let installed = installation != InstallationState::NotInstalled;
        let install_label = match installation {
            InstallationState::NotInstalled => "نصب در Program Files",
            InstallationState::InstalledCurrent => "نصب شده",
            InstallationState::UpdateAvailable => "بروزرسانی نسخه نصب‌شده",
            InstallationState::InstalledOtherUpToDate => "نسخه نصب‌شده بروز است",
        };
        let install_color = match installation {
            InstallationState::NotInstalled | InstallationState::UpdateAvailable => palette.accent,
            InstallationState::InstalledCurrent | InstallationState::InstalledOtherUpToDate => {
                palette.surface_alt
            }
        };
        let install_text_color = match installation {
            InstallationState::NotInstalled | InstallationState::UpdateAvailable => {
                palette.accent_text
            }
            InstallationState::InstalledCurrent | InstallationState::InstalledOtherUpToDate => {
                palette.muted
            }
        };
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 218,
                top: 754,
                right: 406,
                bottom: 794,
            }),
            install_color,
            scaled(12, scale),
        );
        draw_text(
            hdc,
            install_label,
            sr(RECT {
                left: 218,
                top: 754,
                right: 406,
                bottom: 794,
            }),
            install_text_color,
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 24,
                top: 754,
                right: 212,
                bottom: 794,
            }),
            if installed {
                palette.holiday
            } else {
                palette.surface_alt
            },
            scaled(12, scale),
        );
        draw_text(
            hdc,
            "حذف برنامه",
            sr(RECT {
                left: 24,
                top: 754,
                right: 212,
                bottom: 794,
            }),
            if installed {
                palette.accent_text
            } else {
                palette.faint
            },
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_fill(
            hdc,
            sr(RECT {
                left: 24,
                top: 801,
                right: 406,
                bottom: 843,
            }),
            palette.accent,
            scaled(12, scale),
        );
        draw_text(
            hdc,
            "بازنشانی تنظیمات",
            sr(RECT {
                left: 24,
                top: 801,
                right: 406,
                bottom: 843,
            }),
            palette.accent_text,
            fonts.medium,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        paint_footer(hdc, app, palette, fonts);
    }
}

unsafe fn paint_value_row(
    hdc: HDC,
    app: &AppState,
    palette: &Palette,
    fonts: &Fonts,
    top: i32,
    label: &str,
    value: &str,
) {
    unsafe {
        let scale = app.scale();
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 24,
                    top,
                    right: 406,
                    bottom: top + 40,
                },
                scale,
            ),
            palette.surface_alt,
            scaled(10, scale),
        );
        draw_text(
            hdc,
            label,
            scaled_rect(
                RECT {
                    left: 170,
                    top,
                    right: 390,
                    bottom: top + 40,
                },
                scale,
            ),
            palette.text,
            fonts.regular,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 34,
                    top: top + 6,
                    right: 148,
                    bottom: top + 34,
                },
                scale,
            ),
            palette.calendar_panel,
            scaled(11, scale),
        );
        draw_text(
            hdc,
            value,
            scaled_rect(
                RECT {
                    left: 34,
                    top: top + 6,
                    right: 148,
                    bottom: top + 34,
                },
                scale,
            ),
            palette.accent,
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
    }
}

unsafe fn paint_scale_row(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts, top: i32) {
    unsafe {
        let scale = app.scale();
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 24,
                    top,
                    right: 406,
                    bottom: top + 40,
                },
                scale,
            ),
            palette.surface_alt,
            scaled(10, scale),
        );
        draw_text(
            hdc,
            "مقیاس رابط کاربری",
            scaled_rect(
                RECT {
                    left: 170,
                    top,
                    right: 390,
                    bottom: top + 40,
                },
                scale,
            ),
            palette.text,
            fonts.regular,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 30,
                    top: top + 6,
                    right: 62,
                    bottom: top + 34,
                },
                scale,
            ),
            palette.calendar_panel,
            scaled(9, scale),
        );
        draw_text(
            hdc,
            "−",
            scaled_rect(
                RECT {
                    left: 30,
                    top: top + 4,
                    right: 62,
                    bottom: top + 34,
                },
                scale,
            ),
            palette.accent,
            fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            &format!("٪{}", persian_digits(app.settings.ui_scale)),
            scaled_rect(
                RECT {
                    left: 64,
                    top: top + 5,
                    right: 116,
                    bottom: top + 35,
                },
                scale,
            ),
            palette.text,
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 118,
                    top: top + 6,
                    right: 150,
                    bottom: top + 34,
                },
                scale,
            ),
            palette.calendar_panel,
            scaled(9, scale),
        );
        draw_text(
            hdc,
            "+",
            scaled_rect(
                RECT {
                    left: 118,
                    top: top + 5,
                    right: 150,
                    bottom: top + 34,
                },
                scale,
            ),
            palette.accent,
            fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

unsafe fn paint_toggle_row(
    hdc: HDC,
    app: &AppState,
    palette: &Palette,
    fonts: &Fonts,
    top: i32,
    label: &str,
    enabled: bool,
) {
    unsafe {
        let scale = app.scale();
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 24,
                    top,
                    right: 406,
                    bottom: top + 40,
                },
                scale,
            ),
            palette.surface_alt,
            scaled(10, scale),
        );
        draw_text(
            hdc,
            label,
            scaled_rect(
                RECT {
                    left: 102,
                    top,
                    right: 390,
                    bottom: top + 40,
                },
                scale,
            ),
            palette.text,
            fonts.regular,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        let track = scaled_rect(
            RECT {
                left: 35,
                top: top + 9,
                right: 83,
                bottom: top + 31,
            },
            scale,
        );
        draw_round_fill(
            hdc,
            track,
            if enabled {
                palette.accent
            } else {
                palette.faint
            },
            scaled(11, scale),
        );
        let knob_left = if enabled { 61 } else { 37 };
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: knob_left,
                    top: top + 11,
                    right: knob_left + 18,
                    bottom: top + 29,
                },
                scale,
            ),
            if enabled {
                palette.accent_text
            } else {
                palette.surface
            },
            scaled(9, scale),
        );
    }
}

fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam as u32 & 0xffff) as i16 as i32;
    let y = ((lparam as u32 >> 16) & 0xffff) as i16 as i32;
    (x, y)
}

unsafe fn handle_main_click(hwnd: HWND, x: i32, y: i32) {
    let mut show_about_dialog = false;
    let mut open_website_link = false;
    let mut open_github_link = false;
    let mut refresh_tooltip = false;
    let mut refresh_tray_visual = false;
    let mut enable_auto_update = false;
    let mut install_requested = false;
    let mut uninstall_requested = false;
    let mut start_update = false;
    let mut update_release_url = None;
    let mut resize = false;
    {
        let mut app = state().lock().unwrap();
        let x = unscaled(x, app.scale());
        let y = unscaled(y, app.scale());
        let footer_top = app.base_height() - BASE_FOOTER_HEIGHT;
        if y >= footer_top {
            if x >= 178 {
                open_website_link = true;
            } else {
                open_github_link = true;
            }
        } else if !app.settings.auto_update
            && update::banner_visible()
            && y >= footer_top - BASE_UPDATE_HEIGHT
        {
            match update::status() {
                update::UpdateStatus::Available(_) => start_update = true,
                update::UpdateStatus::Failed(info) => update_release_url = Some(info.release_url),
                _ => {}
            }
        } else {
            match app.view {
                ViewMode::Calendar => {
                    if y >= 8 && y <= 48 && x <= 75 {
                        if app.settings.compact_day {
                            move_calendar_day(&mut app, -1);
                        } else {
                            let kind = app.settings.main_calendar;
                            let (mut year, mut month) = (app.year, app.month);
                            add_month(
                                kind,
                                &mut year,
                                &mut month,
                                if app.settings.calendar_rtl { 1 } else { -1 },
                            );
                            app.year = year;
                            app.month = month;
                            app.selected_day = None;
                            app.event_scroll = 0;
                        }
                    } else if y >= 8 && y <= 48 && x >= 355 {
                        if app.settings.compact_day {
                            move_calendar_day(&mut app, 1);
                        } else {
                            let kind = app.settings.main_calendar;
                            let (mut year, mut month) = (app.year, app.month);
                            add_month(
                                kind,
                                &mut year,
                                &mut month,
                                if app.settings.calendar_rtl { -1 } else { 1 },
                            );
                            app.year = year;
                            app.month = month;
                            app.selected_day = None;
                            app.event_scroll = 0;
                        }
                    } else if (54..=94).contains(&y) && x <= 62 {
                        show_about_dialog = true;
                    } else if (54..=94).contains(&y) && x >= 368 {
                        app.view = ViewMode::Settings;
                        resize = true;
                    } else if !app.settings.compact_day
                        && (78..=113).contains(&y)
                        && (160..=270).contains(&x)
                    {
                        let today = app.today_main();
                        let today_is_active = app.year == today.year
                            && app.month == today.month
                            && app.selected_day == Some(today.day);
                        if !today_is_active {
                            app.year = today.year;
                            app.month = today.month;
                            app.selected_day = Some(today.day);
                            app.event_scroll = 0;
                        }
                    } else if !app.settings.compact_day
                        && (GRID_TOP..GRID_TOP + CELL_HEIGHT * 6).contains(&y)
                        && (GRID_LEFT..GRID_LEFT + GRID_WIDTH).contains(&x)
                    {
                        let visual_column = (x - GRID_LEFT) / CELL_WIDTH;
                        let column = calendar_column(visual_column, app.settings.calendar_rtl);
                        let row = (y - GRID_TOP) / CELL_HEIGHT;
                        let cell = row * 7 + column;
                        let (clicked, in_current) =
                            adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
                        if !in_current {
                            app.year = clicked.year;
                            app.month = clicked.month;
                        }
                        app.selected_day = if in_current && app.selected_day == Some(clicked.day) {
                            None
                        } else {
                            Some(clicked.day)
                        };
                        app.event_scroll = 0;
                    } else if app.settings.show_events
                        && (16..=36).contains(&x)
                        && (events_top(&app) + 36..=events_top(&app) + BASE_EVENTS_HEIGHT - 8)
                            .contains(&y)
                    {
                        let count = event_items_for_view(&app).len();
                        let max_scroll = count.saturating_sub(3);
                        if max_scroll > 0 {
                            let event_top = events_top(&app);
                            let track_top = event_top + 40;
                            let track_bottom = event_top + BASE_EVENTS_HEIGHT - 18;
                            let relative = (y - track_top).clamp(0, track_bottom - track_top);
                            app.event_scroll = (relative as usize * max_scroll
                                / (track_bottom - track_top) as usize)
                                .min(max_scroll);
                        }
                    }
                }
                ViewMode::Settings => {
                    if y <= 54 && x >= 350 {
                        app.view = ViewMode::Calendar;
                        resize = true;
                    } else if (76..=120).contains(&y) {
                        app.settings.theme = app.settings.theme.toggle();
                        app.settings.save();
                    } else if (120..=164).contains(&y) {
                        if x <= 68 {
                            app.settings.smaller();
                        } else if (112..=160).contains(&x) {
                            app.settings.larger();
                        }
                        app.settings.save();
                        resize = true;
                    } else if (164..=210).contains(&y) {
                        let next = app.settings.main_calendar.next();
                        app.set_main_calendar(next);
                        app.settings.save();
                    } else if (216..=258).contains(&y) {
                        app.settings.calendar_rtl = !app.settings.calendar_rtl;
                        app.settings.save();
                    } else if (260..=302).contains(&y) {
                        app.settings.show_jalali = !app.settings.show_jalali;
                        app.settings.save();
                    } else if (304..=346).contains(&y) {
                        app.settings.show_gregorian = !app.settings.show_gregorian;
                        app.settings.save();
                    } else if (348..=390).contains(&y) {
                        app.settings.show_hijri = !app.settings.show_hijri;
                        app.settings.save();
                    } else if (392..=434).contains(&y) {
                        app.settings.show_subtitles = !app.settings.show_subtitles;
                        app.settings.save();
                    } else if (436..=478).contains(&y) {
                        app.settings.show_events = !app.settings.show_events;
                        app.settings.save();
                        resize = true;
                    } else if (480..=522).contains(&y) {
                        app.settings.show_tray_date = !app.settings.show_tray_date;
                        app.settings.save();
                        refresh_tooltip = true;
                    } else if (524..=568).contains(&y) {
                        app.settings.auto_update = !app.settings.auto_update;
                        enable_auto_update = app.settings.auto_update;
                        app.settings.save();
                        resize = true;
                    } else if (569..=612).contains(&y) {
                        app.settings.tray_day_icon = !app.settings.tray_day_icon;
                        app.settings.save();
                        refresh_tray_visual = true;
                    } else if (613..=656).contains(&y) {
                        app.settings.tray_english_digits = !app.settings.tray_english_digits;
                        app.settings.save();
                        refresh_tray_visual = true;
                    } else if (657..=700).contains(&y) {
                        app.settings.compact_day = !app.settings.compact_day;
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
                        app.settings.save();
                        resize = true;
                    } else if (701..=748).contains(&y) {
                        let next = !app.settings.autostart;
                        if set_autostart(next) {
                            app.settings.autostart = next;
                            app.settings.save();
                        }
                    } else if (750..=798).contains(&y) {
                        if x >= 215 {
                            if matches!(
                                installation_state(),
                                InstallationState::NotInstalled
                                    | InstallationState::UpdateAvailable
                            ) {
                                install_requested = true;
                            }
                        } else {
                            uninstall_requested = true;
                        }
                    } else if (799..=854).contains(&y) {
                        let default_settings = Settings::default();
                        let next_main = default_settings.main_calendar;
                        if next_main != app.settings.main_calendar {
                            app.set_main_calendar(next_main);
                        }
                        if app.settings.autostart && !default_settings.autostart {
                            let _ = set_autostart(false);
                        }
                        app.settings = default_settings;
                        app.settings.save();
                        resize = true;
                        refresh_tooltip = true;
                        refresh_tray_visual = true;
                        enable_auto_update = app.settings.auto_update;
                    }
                }
            }
        }
    }
    if resize {
        unsafe {
            resize_main_window(hwnd, true);
        }
    }
    unsafe {
        InvalidateRect(hwnd, null(), 0);
    }
    if show_about_dialog {
        unsafe {
            show_about(hwnd);
        }
    }
    if open_website_link {
        unsafe {
            open_website(hwnd);
        }
    }
    if open_github_link {
        unsafe {
            open_github(hwnd);
        }
    }
    if refresh_tooltip {
        unsafe {
            refresh_tray_tooltip(hwnd);
        }
    }
    if refresh_tray_visual {
        unsafe {
            refresh_tray_icon(hwnd);
        }
    }
    if install_requested && unsafe { request_install(hwnd) } {
        EXITING.store(true, Ordering::SeqCst);
        unsafe {
            DestroyWindow(hwnd);
        }
        return;
    }
    if uninstall_requested && unsafe { request_uninstall(hwnd) } {
        EXITING.store(true, Ordering::SeqCst);
        unsafe {
            DestroyWindow(hwnd);
        }
        return;
    }
    if enable_auto_update && matches!(update::status(), update::UpdateStatus::Available(_)) {
        update::start_download(hwnd, WM_UPDATE_STATUS, WM_APPLY_UPDATE);
    }
    if start_update {
        update::start_download(hwnd, WM_UPDATE_STATUS, WM_APPLY_UPDATE);
    }
    if let Some(url) = update_release_url {
        unsafe {
            open_url(hwnd, &url);
        }
    }
}

unsafe fn install_embedded_font(instance: HINSTANCE) {
    unsafe {
        let resource = FindResourceW(
            instance,
            FONT_RESOURCE_ID as *const u16,
            RCDATA_RESOURCE_TYPE as *const u16,
        );
        if resource.is_null() {
            return;
        }
        let size = SizeofResource(instance, resource);
        let loaded = LoadResource(instance, resource);
        if size == 0 || loaded.is_null() {
            return;
        }
        let data = LockResource(loaded);
        if data.is_null() {
            return;
        }
        let mut fonts_added = 0u32;
        let handle = AddFontMemResourceEx(data, size, null(), &mut fonts_added);
        if !handle.is_null() && fonts_added > 0 {
            FONT_RESOURCE_HANDLE.store(handle as isize, Ordering::SeqCst);
        }
    }
}

unsafe fn uninstall_embedded_font() {
    let handle = FONT_RESOURCE_HANDLE.swap(0, Ordering::SeqCst) as HANDLE;
    if !handle.is_null() {
        unsafe {
            RemoveFontMemResourceEx(handle);
        }
    }
}

fn clipboard_date(kind: CalendarKind, date: Date) -> String {
    let value = format!("{:04}/{:02}/{:02}", date.year, date.month, date.day);
    if kind == CalendarKind::Gregorian {
        value
    } else {
        persian_digits(value)
    }
}

unsafe fn copy_text_to_clipboard(hwnd: HWND, text: &str) -> bool {
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return false;
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return false;
        }
        let content = wide(text);
        let byte_len = content.len() * size_of::<u16>();
        let memory = GlobalAlloc(GMEM_MOVEABLE, byte_len);
        if memory.is_null() {
            CloseClipboard();
            return false;
        }
        let target = GlobalLock(memory) as *mut u16;
        if target.is_null() {
            GlobalFree(memory);
            CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(content.as_ptr(), target, content.len());
        GlobalUnlock(memory);
        let accepted = !SetClipboardData(CF_UNICODETEXT_FORMAT, memory as HANDLE).is_null();
        if !accepted {
            GlobalFree(memory);
        }
        CloseClipboard();
        accepted
    }
}

unsafe fn copy_date_at_point(hwnd: HWND, x: i32, y: i32) {
    let text = {
        let app = state().lock().unwrap();
        if app.view != ViewMode::Calendar {
            return;
        }
        let x = unscaled(x, app.scale());
        let y = unscaled(y, app.scale());
        if !(GRID_LEFT..GRID_LEFT + GRID_WIDTH).contains(&x)
            || !(GRID_TOP..GRID_TOP + CELL_HEIGHT * 6).contains(&y)
        {
            return;
        }
        let visual_column = (x - GRID_LEFT) / CELL_WIDTH;
        let column = calendar_column(visual_column, app.settings.calendar_rtl);
        let row = (y - GRID_TOP) / CELL_HEIGHT;
        let (date, _) = adjacent_date(
            app.settings.main_calendar,
            app.year,
            app.month,
            row * 7 + column,
        );
        clipboard_date(app.settings.main_calendar, date)
    };
    unsafe {
        copy_text_to_clipboard(hwnd, &text);
    }
}

unsafe fn load_app_icon(instance: HINSTANCE) -> HICON {
    unsafe { LoadIconW(instance, APP_ICON_ID as *const u16) }
}

unsafe fn create_tray_day_icon(day: u32, english_digits: bool) -> HICON {
    unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return null_mut();
        }
        let color_dc = CreateCompatibleDC(screen);
        let mask_dc = CreateCompatibleDC(screen);
        let color = CreateCompatibleBitmap(screen, 32, 32);
        let mask = CreateBitmap(32, 32, 1, 1, null());
        if color_dc.is_null() || mask_dc.is_null() || color.is_null() || mask.is_null() {
            if !color_dc.is_null() {
                DeleteDC(color_dc);
            }
            if !mask_dc.is_null() {
                DeleteDC(mask_dc);
            }
            if !color.is_null() {
                DeleteObject(color as HGDIOBJ);
            }
            if !mask.is_null() {
                DeleteObject(mask as HGDIOBJ);
            }
            ReleaseDC(null_mut(), screen);
            return null_mut();
        }

        let old_color_bitmap = SelectObject(color_dc, color as HGDIOBJ);
        fill_rect_color(
            color_dc,
            RECT {
                left: 0,
                top: 0,
                right: 32,
                bottom: 32,
            },
            rgb(248, 211, 88),
        );

        // A color icon's monochrome mask uses white pixels as transparent and black
        // pixels as opaque. Shape the mask independently so the taskbar can show
        // genuinely transparent corners on both light and dark themes.
        let old_mask_bitmap = SelectObject(mask_dc, mask as HGDIOBJ);
        PatBlt(mask_dc, 0, 0, 32, 32, WHITENESS);
        let old_mask_brush = SelectObject(mask_dc, GetStockObject(BLACK_BRUSH) as HGDIOBJ);
        let old_mask_pen = SelectObject(mask_dc, GetStockObject(BLACK_PEN) as HGDIOBJ);
        RoundRect(mask_dc, 1, 1, 31, 31, 5, 5);
        SelectObject(mask_dc, old_mask_pen);
        SelectObject(mask_dc, old_mask_brush);

        let day_text = if english_digits {
            day.to_string()
        } else {
            persian_digits(day)
        };
        let text_format = if english_digits {
            DT_CENTER | DT_VCENTER | DT_SINGLELINE
        } else {
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING
        };
        let font = create_font(-24, FW_BOLD as i32, "Vazirmatn");
        draw_text(
            color_dc,
            &day_text,
            RECT {
                left: 0,
                top: 2,
                right: 32,
                bottom: 32,
            },
            rgb(18, 18, 18),
            font,
            text_format,
        );
        DeleteObject(font as HGDIOBJ);
        SelectObject(color_dc, old_color_bitmap);
        SelectObject(mask_dc, old_mask_bitmap);
        DeleteDC(color_dc);
        DeleteDC(mask_dc);
        ReleaseDC(null_mut(), screen);

        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        DeleteObject(mask as HGDIOBJ);
        DeleteObject(color as HGDIOBJ);
        icon
    }
}
unsafe fn selected_tray_icon(app: &AppState) -> (HICON, bool) {
    unsafe {
        if app.settings.tray_day_icon {
            let today = from_gregorian(CalendarKind::Jalali, app.today_gregorian);
            let icon = create_tray_day_icon(today.day, app.settings.tray_english_digits);
            if !icon.is_null() {
                return (icon, true);
            }
        }
        (load_app_icon(GetModuleHandleW(null())), false)
    }
}

unsafe fn open_website(hwnd: HWND) {
    unsafe {
        open_url(hwnd, WEBSITE_URL);
    }
}

unsafe fn open_github(hwnd: HWND) {
    unsafe {
        open_url(hwnd, GITHUB_URL);
    }
}

unsafe fn open_url(hwnd: HWND, address: &str) {
    unsafe {
        let operation = wide("open");
        let url = wide(address);
        ShellExecuteW(
            hwnd,
            operation.as_ptr(),
            url.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}

fn installed_executable_path() -> Option<PathBuf> {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .map(|path| path.join("GahYar").join("GahYar.exe"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InstallationState {
    NotInstalled,
    InstalledCurrent,
    UpdateAvailable,
    InstalledOtherUpToDate,
}

fn same_executable_path(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

fn executable_version(path: &Path) -> Option<(u16, u16, u16, u16)> {
    unsafe {
        let path_wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut ignored = 0u32;
        let size = GetFileVersionInfoSizeW(path_wide.as_ptr(), &mut ignored);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(
            path_wide.as_ptr(),
            0,
            size,
            data.as_mut_ptr() as *mut c_void,
        ) == 0
        {
            return None;
        }
        let root = wide("\\");
        let mut version_pointer: *mut c_void = null_mut();
        let mut version_size = 0u32;
        if VerQueryValueW(
            data.as_ptr() as *const c_void,
            root.as_ptr(),
            &mut version_pointer,
            &mut version_size,
        ) == 0
            || version_pointer.is_null()
            || version_size < size_of::<VS_FIXEDFILEINFO>() as u32
        {
            return None;
        }
        let info = &*(version_pointer as *const VS_FIXEDFILEINFO);
        if info.dwSignature != 0xFEEF04BD {
            return None;
        }
        Some((
            (info.dwFileVersionMS >> 16) as u16,
            info.dwFileVersionMS as u16,
            (info.dwFileVersionLS >> 16) as u16,
            info.dwFileVersionLS as u16,
        ))
    }
}

fn running_version_is_newer(current: &Path, installed: &Path) -> bool {
    matches!(
        (executable_version(current), executable_version(installed)),
        (Some(current), Some(installed)) if current > installed
    )
}

fn installation_state() -> InstallationState {
    let Some(installed) = installed_executable_path().filter(|path| path.is_file()) else {
        return InstallationState::NotInstalled;
    };
    let Some(current) = std::env::current_exe().ok() else {
        return InstallationState::InstalledOtherUpToDate;
    };
    if same_executable_path(&current, &installed) {
        InstallationState::InstalledCurrent
    } else if running_version_is_newer(&current, &installed) {
        InstallationState::UpdateAvailable
    } else {
        InstallationState::InstalledOtherUpToDate
    }
}

unsafe fn show_message(hwnd: HWND, title: &str, message: &str, flags: u32) -> i32 {
    unsafe {
        let title = wide(title);
        let message = wide(message);
        MessageBoxW(
            hwnd,
            message.as_ptr(),
            title.as_ptr(),
            flags | MB_RTLREADING | MB_RIGHT,
        )
    }
}

unsafe fn confirm_state(hwnd: HWND) -> Option<&'static mut ConfirmDialogState> {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ConfirmDialogState;
    unsafe { pointer.as_mut() }
}

unsafe fn paint_confirm(hwnd: HWND) {
    unsafe {
        let Some(dialog) = confirm_state(hwnd) else {
            return;
        };
        let mut ps: PAINTSTRUCT = zeroed();
        let window_hdc = BeginPaint(hwnd, &mut ps);
        if window_hdc.is_null() {
            return;
        }

        let app = state().lock().unwrap();
        let scale = app.scale();
        let palette = Palette::from_theme(app.settings.theme);
        let fonts = Fonts::create(scale);
        let width = scaled(BASE_CONFIRM_WIDTH, scale);
        let height = scaled(BASE_CONFIRM_HEIGHT, scale);
        let hdc = CreateCompatibleDC(window_hdc);
        let bitmap = CreateCompatibleBitmap(window_hdc, width, height);
        if hdc.is_null() || bitmap.is_null() {
            if !hdc.is_null() {
                DeleteDC(hdc);
            }
            if !bitmap.is_null() {
                DeleteObject(bitmap as HGDIOBJ);
            }
            drop(app);
            fonts.destroy();
            EndPaint(hwnd, &ps);
            return;
        }

        let old_bitmap = SelectObject(hdc, bitmap as HGDIOBJ);
        fill_rect_color(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            palette.background,
        );
        draw_round_fill(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            palette.surface,
            scaled(18, scale),
        );
        draw_text(
            hdc,
            &dialog.title,
            scaled_rect(
                RECT {
                    left: 28,
                    top: 20,
                    right: 402,
                    bottom: 58,
                },
                scale,
            ),
            palette.accent,
            fonts.title,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        let message_bottom = if dialog.reminder.is_some() { 188 } else { 276 };
        draw_text(
            hdc,
            &dialog.message,
            scaled_rect(
                RECT {
                    left: 28,
                    top: 68,
                    right: 402,
                    bottom: message_bottom,
                },
                scale,
            ),
            palette.text,
            fonts.regular,
            DT_RIGHT | DT_WORDBREAK | DT_RTLREADING,
        );
        if let Some(reminder) = dialog.reminder.as_deref() {
            let reminder_rect = scaled_rect(
                RECT {
                    left: 28,
                    top: 198,
                    right: 402,
                    bottom: 276,
                },
                scale,
            );
            draw_round_fill(hdc, reminder_rect, palette.selected, scaled(11, scale));
            draw_round_outline(hdc, reminder_rect, palette.accent, scaled(11, scale), 1);
            draw_text(
                hdc,
                reminder,
                scaled_rect(
                    RECT {
                        left: 40,
                        top: 204,
                        right: 390,
                        bottom: 270,
                    },
                    scale,
                ),
                palette.text,
                fonts.small,
                DT_RIGHT | DT_WORDBREAK | DT_RTLREADING,
            );
        }

        let cancel_rect = scaled_rect(
            RECT {
                left: 26,
                top: 292,
                right: 204,
                bottom: 332,
            },
            scale,
        );
        let accept_rect = scaled_rect(
            RECT {
                left: 226,
                top: 292,
                right: 404,
                bottom: 332,
            },
            scale,
        );
        draw_round_fill(hdc, cancel_rect, palette.surface_alt, scaled(12, scale));
        draw_round_outline(hdc, cancel_rect, palette.border, scaled(12, scale), 1);
        draw_text(
            hdc,
            "انصراف",
            cancel_rect,
            palette.text,
            fonts.medium,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        let accept_color = if dialog.destructive {
            palette.holiday
        } else {
            palette.accent
        };
        let accept_text_color = if dialog.destructive {
            rgb(255, 255, 255)
        } else {
            palette.accent_text
        };
        draw_round_fill(hdc, accept_rect, accept_color, scaled(12, scale));
        draw_text(
            hdc,
            &dialog.accept_label,
            accept_rect,
            accept_text_color,
            fonts.medium,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_outline(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width - 1,
                bottom: height - 1,
            },
            palette.border,
            scaled(18, scale),
            1,
        );

        BitBlt(window_hdc, 0, 0, width, height, hdc, 0, 0, SRCCOPY);
        SelectObject(hdc, old_bitmap);
        DeleteObject(bitmap as HGDIOBJ);
        DeleteDC(hdc);
        drop(app);
        fonts.destroy();
        EndPaint(hwnd, &ps);
    }
}

unsafe extern "system" fn confirm_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = lparam as *const CREATESTRUCTW;
            if create.is_null() {
                return 0;
            }
            let pointer = unsafe { (*create).lpCreateParams } as *mut ConfirmDialogState;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as isize);
            }
            CONFIRM_HWND.store(hwnd as isize, Ordering::SeqCst);
            1
        }
        WM_PAINT => {
            unsafe {
                paint_confirm(hwnd);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONUP => {
            let (x, y) = point_from_lparam(lparam);
            let scale = state().lock().unwrap().scale();
            let x = unscaled(x, scale);
            let y = unscaled(y, scale);
            if (226..=404).contains(&x) && (292..=332).contains(&y) {
                if let Some(dialog) = unsafe { confirm_state(hwnd) } {
                    dialog.accepted = true;
                }
                unsafe {
                    DestroyWindow(hwnd);
                }
            } else if (26..=204).contains(&x) && (292..=332).contains(&y) {
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_KEYDOWN => {
            match wparam as u32 {
                0x0D => {
                    if let Some(dialog) = unsafe { confirm_state(hwnd) } {
                        dialog.accepted = true;
                    }
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                }
                0x1B => unsafe {
                    DestroyWindow(hwnd);
                },
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_NCDESTROY => {
            CONFIRM_HWND.store(0, Ordering::SeqCst);
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn confirm_action(
    owner: HWND,
    title: &str,
    message: &str,
    reminder: Option<&str>,
    accept_label: &str,
    destructive: bool,
) -> bool {
    unsafe {
        let instance = GetModuleHandleW(null());
        let class_name = wide(CONFIRM_CLASS);
        let window_title = wide(title);
        let scale = state().lock().unwrap().scale();
        let width = scaled(BASE_CONFIRM_WIDTH, scale);
        let height = scaled(BASE_CONFIRM_HEIGHT, scale);
        let mut owner_rect: RECT = zeroed();
        GetWindowRect(owner, &mut owner_rect);
        let x = owner_rect.left + (owner_rect.right - owner_rect.left - width) / 2;
        let y = owner_rect.top + (owner_rect.bottom - owner_rect.top - height) / 2;
        let mut dialog = ConfirmDialogState {
            title: title.to_owned(),
            message: message.to_owned(),
            reminder: reminder.map(str::to_owned),
            accept_label: accept_label.to_owned(),
            destructive,
            accepted: false,
        };
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            owner,
            null_mut(),
            instance,
            &mut dialog as *mut ConfirmDialogState as *mut _,
        );
        if hwnd.is_null() {
            return false;
        }

        let region = CreateRoundRectRgn(
            0,
            0,
            width + 1,
            height + 1,
            scaled(18, scale),
            scaled(18, scale),
        );
        SetWindowRgn(hwnd, region, 1);
        EnableWindow(owner, 0);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);

        let mut message: MSG = zeroed();
        while IsWindow(hwnd) != 0 {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result <= 0 {
                if result == 0 {
                    PostQuitMessage(message.wParam as i32);
                }
                break;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        EnableWindow(owner, 1);
        SetForegroundWindow(owner);
        dialog.accepted
    }
}

fn quoted_argument(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

unsafe fn launch_elevated_script(hwnd: HWND, script: &Path, arguments: &str) -> bool {
    unsafe {
        let operation = wide("runas");
        let executable = wide("powershell.exe");
        let parameters = wide(&format!(
            "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File {} {}",
            quoted_argument(script),
            arguments,
        ));
        ShellExecuteW(
            hwnd,
            operation.as_ptr(),
            executable.as_ptr(),
            parameters.as_ptr(),
            null(),
            SW_SHOWNORMAL,
        ) as isize
            > 32
    }
}

fn install_script() -> &'static str {
    r#"param([int]$TargetPid, [string]$Source, [string]$Destination)
Wait-Process -Id $TargetPid -ErrorAction SilentlyContinue
$folder = Split-Path -Parent $Destination
$installed = $false
for ($attempt = 0; $attempt -lt 20; $attempt++) {
    try {
        New-Item -ItemType Directory -Path $folder -Force | Out-Null
        Copy-Item -LiteralPath $Source -Destination $Destination -Force -ErrorAction Stop
        $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
        $acl = Get-Acl -LiteralPath $folder
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule($identity, 'Modify', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.SetAccessRule($rule)
        Set-Acl -LiteralPath $folder -AclObject $acl
        $installed = $true
        break
    } catch {
        Start-Sleep -Milliseconds 500
    }
}
if ($installed) {
    $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    $runValue = Get-ItemProperty -Path $runKey -Name 'GahYar' -ErrorAction SilentlyContinue
    if ($null -ne $runValue) {
        Set-ItemProperty -Path $runKey -Name 'GahYar' -Value ('"' + $Destination + '"')
    }
    Start-Process -FilePath $Destination
} else {
    Start-Process -FilePath $Source
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
"#
}

fn uninstall_script() -> &'static str {
    r#"param([int]$TargetPid, [string]$Destination, [int]$OpenGithub)
Wait-Process -Id $TargetPid -ErrorAction SilentlyContinue
$removed = $false
for ($attempt = 0; $attempt -lt 20; $attempt++) {
    try {
        Remove-Item -LiteralPath $Destination -Force -ErrorAction Stop
        $removed = -not (Test-Path -LiteralPath $Destination)
        if ($removed) { break }
    } catch {
        Start-Sleep -Milliseconds 500
    }
}
$folder = Split-Path -Parent $Destination
if ($removed) {
    Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'GahYar' -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $folder -Force -ErrorAction SilentlyContinue
    if ($OpenGithub -eq 1) {
        Start-Process 'https://github.com/emadgh/GahYar'
    }
} elseif (Test-Path -LiteralPath $Destination) {
    Start-Process -FilePath $Destination
}
Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue
"#
}

unsafe fn request_install(hwnd: HWND) -> bool {
    unsafe {
        let Some(destination) = installed_executable_path() else {
            show_message(
                hwnd,
                "نصب گاه‌یار",
                "مسیر Program Files پیدا نشد.",
                MB_OK | MB_ICONERROR,
            );
            return false;
        };
        let Ok(source) = std::env::current_exe() else {
            show_message(
                hwnd,
                "نصب گاه‌یار",
                "مسیر فایل اجرایی فعلی پیدا نشد.",
                MB_OK | MB_ICONERROR,
            );
            return false;
        };
        let installation = if destination.is_file() {
            if same_executable_path(&source, &destination) {
                InstallationState::InstalledCurrent
            } else if running_version_is_newer(&source, &destination) {
                InstallationState::UpdateAvailable
            } else {
                InstallationState::InstalledOtherUpToDate
            }
        } else {
            InstallationState::NotInstalled
        };
        if matches!(
            installation,
            InstallationState::InstalledCurrent | InstallationState::InstalledOtherUpToDate
        ) {
            let message = if installation == InstallationState::InstalledCurrent {
                "گاه‌یار هم‌اکنون در Program Files نصب شده است."
            } else {
                "نسخهٔ نصب‌شده با نسخهٔ فعلی یکسان یا از آن جدیدتر است و نیازی به بروزرسانی ندارد."
            };
            show_message(hwnd, "نصب گاه‌یار", message, MB_OK | MB_ICONINFORMATION);
            return false;
        }
        let updating = installation == InstallationState::UpdateAvailable;
        let title = if updating {
            "بروزرسانی گاه‌یار"
        } else {
            "نصب گاه‌یار"
        };
        let message = if updating {
            "نسخه‌ای که اکنون اجرا کرده‌اید با نسخهٔ داخل Program Files متفاوت است.\n\nآیا فایل نصب‌شده با فایل فعلی جایگزین شود؟\n\nبرنامه بسته می‌شود و پس از بروزرسانی دوباره از مسیر نصب‌شده اجرا خواهد شد."
        } else {
            "آیا گاه‌یار در Program Files نصب شود؟\n\nبرنامه بسته می‌شود و پس از نصب دوباره اجرا خواهد شد.\n\nاگر مایل بودید، خوشحال می‌شویم گاه‌یار را به دوستانتان و در شبکه‌های اجتماعی پیشنهاد دهید."
        };
        if !confirm_action(
            hwnd,
            title,
            message,
            if updating {
                None
            } else {
                Some(
                    "یادآوری: گاه‌یار یک برنامهٔ پورتابل است و برای کارکردن نیازی به نصب در Program Files ندارد. انتقال آن به این پوشه فقط کمک می‌کند فایل برنامه اتفاقی حذف نشود و محل نگهداری مطمئن‌تری داشته باشد.",
                )
            },
            if updating {
                "بروزرسانی برنامه"
            } else {
                "نصب برنامه"
            },
            false,
        ) {
            return false;
        }
        let script =
            std::env::temp_dir().join(format!("GahYar-install-{}.ps1", std::process::id()));
        if fs::write(&script, install_script()).is_err() {
            show_message(
                hwnd,
                "نصب گاه‌یار",
                "ساخت فایل موقت نصب ناموفق بود.",
                MB_OK | MB_ICONERROR,
            );
            return false;
        }
        let arguments = format!(
            "-TargetPid {} -Source {} -Destination {}",
            std::process::id(),
            quoted_argument(&source),
            quoted_argument(&destination),
        );
        if launch_elevated_script(hwnd, &script, &arguments) {
            true
        } else {
            let _ = fs::remove_file(script);
            show_message(
                hwnd,
                title,
                if updating {
                    "بروزرسانی آغاز نشد یا اجازه دسترسی صادر نشد."
                } else {
                    "نصب آغاز نشد یا اجازه دسترسی صادر نشد."
                },
                MB_OK | MB_ICONWARNING,
            );
            false
        }
    }
}

unsafe fn request_uninstall(hwnd: HWND) -> bool {
    unsafe {
        let Some(destination) = installed_executable_path().filter(|path| path.is_file()) else {
            show_message(
                hwnd,
                "حذف گاه‌یار",
                "نسخه‌ای از گاه‌یار در Program Files نصب نشده است.",
                MB_OK | MB_ICONINFORMATION,
            );
            return false;
        };
        if !confirm_action(
            hwnd,
            "حذف گاه‌یار",
            "آیا مطمئن هستید که می‌خواهید گاه‌یار را حذف کنید؟\n\nاز اینکه این مدت از گاه‌یار استفاده کردید خوشحالیم. اگر مایل بودید، همیشه می‌توانید برنامه را دوباره از GitHub دانلود کنید.\n\nخوشحال می‌شویم بازخوردتان را بشنویم.",
            None,
            "حذف برنامه",
            true,
        ) {
            return false;
        }
        let open_github = confirm_action(
            hwnd,
            "بازخورد و دانلود مجدد",
            "آیا پس از حذف، صفحه GitHub برای دانلود دوباره یا ارسال بازخورد باز شود؟",
            None,
            "باز کردن GitHub",
            false,
        );
        let script =
            std::env::temp_dir().join(format!("GahYar-uninstall-{}.ps1", std::process::id()));
        if fs::write(&script, uninstall_script()).is_err() {
            show_message(
                hwnd,
                "حذف گاه‌یار",
                "ساخت فایل موقت حذف ناموفق بود.",
                MB_OK | MB_ICONERROR,
            );
            return false;
        }
        let arguments = format!(
            "-TargetPid {} -Destination {} -OpenGithub {}",
            std::process::id(),
            quoted_argument(&destination),
            if open_github { 1 } else { 0 },
        );
        if launch_elevated_script(hwnd, &script, &arguments) {
            true
        } else {
            let _ = fs::remove_file(script);
            show_message(
                hwnd,
                "حذف گاه‌یار",
                "حذف آغاز نشد یا اجازه دسترسی صادر نشد.",
                MB_OK | MB_ICONWARNING,
            );
            false
        }
    }
}

unsafe fn add_tray_icon(hwnd: HWND) {
    unsafe {
        let (icon, custom) = selected_tray_icon(&state().lock().unwrap());
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        data.hIcon = icon;
        let tip = wide(&tray_tooltip(&state().lock().unwrap()));
        let count = tip.len().min(data.szTip.len());
        data.szTip[..count].copy_from_slice(&tip[..count]);
        Shell_NotifyIconW(NIM_ADD, &data);
        let previous = CUSTOM_TRAY_ICON
            .swap(if custom { icon as isize } else { 0 }, Ordering::SeqCst)
            as HICON;
        if !previous.is_null() {
            DestroyIcon(previous);
        }
    }
}

fn tray_tooltip(app: &AppState) -> String {
    if !app.settings.show_tray_date {
        return APP_NAME.to_string();
    }
    const WEEKDAYS: [&str; 7] = [
        "شنبه",
        "یکشنبه",
        "دوشنبه",
        "سه‌شنبه",
        "چهارشنبه",
        "پنجشنبه",
        "جمعه",
    ];
    let jalali = from_gregorian(CalendarKind::Jalali, app.today_gregorian);
    let gregorian = app.today_gregorian;
    let weekday = (first_weekday_saturday(CalendarKind::Jalali, jalali.year, jalali.month)
        + jalali.day as i32
        - 1)
    .rem_euclid(7) as usize;
    format!(
        "گاه‌یار — {}، {} {} {} | {:04}/{:02}/{:02}",
        WEEKDAYS[weekday],
        persian_digits(jalali.day),
        month_name(CalendarKind::Jalali, jalali.month),
        persian_digits(jalali.year),
        gregorian.year,
        gregorian.month,
        gregorian.day,
    )
}

unsafe fn refresh_tray_tooltip(hwnd: HWND) {
    unsafe {
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_TIP;
        let tip = wide(&tray_tooltip(&state().lock().unwrap()));
        let count = tip.len().min(data.szTip.len());
        data.szTip[..count].copy_from_slice(&tip[..count]);
        Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

unsafe fn refresh_tray_icon(hwnd: HWND) {
    unsafe {
        let (icon, custom) = selected_tray_icon(&state().lock().unwrap());
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_ICON;
        data.hIcon = icon;
        Shell_NotifyIconW(NIM_MODIFY, &data);
        let previous = CUSTOM_TRAY_ICON
            .swap(if custom { icon as isize } else { 0 }, Ordering::SeqCst)
            as HICON;
        if !previous.is_null() {
            DestroyIcon(previous);
        }
    }
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        Shell_NotifyIconW(NIM_DELETE, &data);
        let custom = CUSTOM_TRAY_ICON.swap(0, Ordering::SeqCst) as HICON;
        if !custom.is_null() {
            DestroyIcon(custom);
        }
    }
}

unsafe fn resize_main_window(hwnd: HWND, preserve_bottom: bool) {
    unsafe {
        let app = state().lock().unwrap();
        let width = scaled(BASE_WIDTH, app.scale());
        let height = scaled(app.base_height(), app.scale());
        let radius = scaled(18, app.scale());
        drop(app);

        let mut rect: RECT = zeroed();
        GetWindowRect(hwnd, &mut rect);
        let x = rect.left;
        let y = if preserve_bottom {
            rect.bottom - height
        } else {
            rect.top
        };
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius);
        SetWindowRgn(hwnd, region, 1);
    }
}

unsafe fn show_popup(hwnd: HWND) {
    unsafe {
        let app = state().lock().unwrap();
        let width = scaled(BASE_WIDTH, app.scale());
        let height = scaled(app.base_height(), app.scale());
        let radius = scaled(18, app.scale());
        drop(app);

        let mut cursor: POINT = zeroed();
        GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        let work = info.rcWork;
        let mut x = cursor.x - width / 2;
        x = x.max(work.left + 8).min(work.right - width - 8);
        let y = work.bottom - height - 8;
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, radius, radius);
        SetWindowRgn(hwnd, region, 1);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn toggle_popup(hwnd: HWND) {
    unsafe {
        if IsWindowVisible(hwnd) != 0 {
            ShowWindow(hwnd, SW_HIDE);
        } else {
            show_popup(hwnd);
        }
    }
}

fn request_manual_update(hwnd: HWND) {
    MANUAL_UPDATE_REQUEST.store(true, Ordering::SeqCst);
    match update::status() {
        update::UpdateStatus::Available(_) | update::UpdateStatus::Failed(_) => {
            if update::start_download(hwnd, WM_UPDATE_STATUS, WM_APPLY_UPDATE) {
                MANUAL_UPDATE_REQUEST.store(false, Ordering::SeqCst);
            }
        }
        update::UpdateStatus::Checking | update::UpdateStatus::Downloading => {}
        _ => {
            update::start_check(hwnd, WM_UPDATE_STATUS);
        }
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        let open = wide("باز کردن گاه‌یار");
        let settings_text = wide("تنظیمات");
        let about = wide("درباره گاه‌یار");
        let update_text = wide("بررسی بروزرسانی");
        let exit = wide("خروج");
        AppendMenuW(menu, MF_STRING, CMD_OPEN, open.as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_SETTINGS, settings_text.as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_ABOUT, about.as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_UPDATE, update_text.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_EXIT, exit.as_ptr());
        let mut point: POINT = zeroed();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        );
        DestroyMenu(menu);
        match command as usize {
            CMD_OPEN => show_popup(hwnd),
            CMD_SETTINGS => {
                state().lock().unwrap().view = ViewMode::Settings;
                resize_main_window(hwnd, true);
                show_popup(hwnd);
            }
            CMD_ABOUT => show_about(hwnd),
            CMD_UPDATE => request_manual_update(hwnd),
            CMD_EXIT => {
                EXITING.store(true, Ordering::SeqCst);
                DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn main_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            unsafe {
                add_tray_icon(hwnd);
                SetTimer(hwnd, DATE_REFRESH_TIMER_ID, 60_000, None);
                SetTimer(hwnd, UPDATE_CHECK_TIMER_ID, UPDATE_CHECK_INTERVAL_MS, None);
                resize_main_window(hwnd, false);
            }
            update::start_check(hwnd, WM_UPDATE_STATUS);
            0
        }
        WM_PAINT => {
            unsafe {
                paint_main(hwnd);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_UPDATE_STATUS => {
            let status = update::status();
            let manual = MANUAL_UPDATE_REQUEST.load(Ordering::SeqCst);
            if matches!(status, update::UpdateStatus::Available(_))
                && (state().lock().unwrap().settings.auto_update || manual)
                && update::start_download(hwnd, WM_UPDATE_STATUS, WM_APPLY_UPDATE)
            {
                MANUAL_UPDATE_REQUEST.store(false, Ordering::SeqCst);
            } else if matches!(
                status,
                update::UpdateStatus::UpToDate
                    | update::UpdateStatus::Idle
                    | update::UpdateStatus::Failed(_)
            ) {
                MANUAL_UPDATE_REQUEST.store(false, Ordering::SeqCst);
            }
            unsafe {
                resize_main_window(hwnd, true);
                InvalidateRect(hwnd, null(), 0);
                let about = ABOUT_HWND.load(Ordering::SeqCst) as HWND;
                if !about.is_null() && IsWindow(about) != 0 {
                    InvalidateRect(about, null(), 0);
                }
            }
            0
        }
        WM_APPLY_UPDATE => {
            EXITING.store(true, Ordering::SeqCst);
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = point_from_lparam(lparam);
            unsafe {
                handle_main_click(hwnd, x, y);
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            let (x, y) = point_from_lparam(lparam);
            unsafe {
                copy_date_at_point(hwnd, x, y);
            }
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = point_from_lparam(lparam);
            unsafe {
                update_day_hover(hwnd, x, y);
                let mut tracking = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tracking);
            }
            0
        }
        WM_MOUSE_LEAVE => {
            let changed = {
                let mut app = state().lock().unwrap();
                let changed = app.hovered_cell.take().is_some();
                changed
            };
            if changed {
                unsafe {
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_TIMER if wparam == DATE_REFRESH_TIMER_ID => {
            let changed = state().lock().unwrap().refresh_today();
            if changed {
                unsafe {
                    refresh_tray_tooltip(hwnd);
                    refresh_tray_icon(hwnd);
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        WM_TIMER if wparam == UPDATE_CHECK_TIMER_ID => {
            update::start_check(hwnd, WM_UPDATE_STATUS);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xffff) as i16 as i32;
            let mut app = state().lock().unwrap();
            if app.view == ViewMode::Calendar && app.settings.show_events {
                let count = event_items_for_view(&app).len();
                let max_scroll = count.saturating_sub(3);
                if delta < 0 {
                    app.event_scroll = (app.event_scroll + 1).min(max_scroll);
                } else {
                    app.event_scroll = app.event_scroll.saturating_sub(1);
                }
            }
            drop(app);
            unsafe {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_KEYDOWN => {
            match wparam as u32 {
                0x1B => {
                    let mut app = state().lock().unwrap();
                    if app.view == ViewMode::Settings {
                        app.view = ViewMode::Calendar;
                        drop(app);
                        unsafe {
                            resize_main_window(hwnd, true);
                            InvalidateRect(hwnd, null(), 0);
                        }
                    } else {
                        drop(app);
                        unsafe {
                            ShowWindow(hwnd, SW_HIDE);
                        }
                    }
                }
                0x74 => {
                    state().lock().unwrap().events = EventStore::load();
                    unsafe {
                        InvalidateRect(hwnd, null(), 0);
                    }
                }
                0x25 | 0x27 => {
                    let left = wparam as u32 == 0x25;
                    let mut app = state().lock().unwrap();
                    if app.view == ViewMode::Calendar {
                        if app.settings.compact_day {
                            move_calendar_day(&mut app, if left { -1 } else { 1 });
                        } else {
                            let delta = match (left, app.settings.calendar_rtl) {
                                (true, true) | (false, false) => 1,
                                _ => -1,
                            };
                            let kind = app.settings.main_calendar;
                            let (mut year, mut month) = (app.year, app.month);
                            add_month(kind, &mut year, &mut month, delta);
                            app.year = year;
                            app.month = month;
                            app.selected_day = None;
                            app.event_scroll = 0;
                        }
                    }
                    drop(app);
                    unsafe {
                        InvalidateRect(hwnd, null(), 0);
                    }
                }
                _ => {}
            }
            0
        }
        WM_ACTIVATE => {
            if (wparam as u32 & 0xffff) == WA_INACTIVE
                && !EXITING.load(Ordering::SeqCst)
                && ABOUT_HWND.load(Ordering::SeqCst) == 0
                && CONFIRM_HWND.load(Ordering::SeqCst) == 0
            {
                unsafe {
                    ShowWindow(hwnd, SW_HIDE);
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
            0
        }
        WM_SHOW_EXISTING => {
            state().lock().unwrap().view = ViewMode::Calendar;
            unsafe {
                resize_main_window(hwnd, true);
                show_popup(hwnd);
            }
            0
        }
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => unsafe {
                    toggle_popup(hwnd);
                },
                WM_RBUTTONUP | WM_CONTEXTMENU => unsafe {
                    show_tray_menu(hwnd);
                },
                _ => {}
            }
            0
        }
        _ if msg == taskbar_created_message() => {
            unsafe {
                add_tray_icon(hwnd);
            }
            0
        }
        WM_DESTROY => {
            unsafe {
                KillTimer(hwnd, DATE_REFRESH_TIMER_ID);
                KillTimer(hwnd, UPDATE_CHECK_TIMER_ID);
                remove_tray_icon(hwnd);
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn show_about(owner: HWND) {
    unsafe {
        let existing = ABOUT_HWND.load(Ordering::SeqCst) as HWND;
        if !existing.is_null() && IsWindow(existing) != 0 {
            SetForegroundWindow(existing);
            return;
        }
        let instance = GetModuleHandleW(null());
        let class_name = wide(ABOUT_CLASS);
        let title = wide("درباره گاه‌یار");
        let scale = state().lock().unwrap().scale();
        let width = scaled(BASE_ABOUT_WIDTH, scale);
        let height = scaled(BASE_ABOUT_HEIGHT, scale);
        let mut owner_rect: RECT = zeroed();
        GetWindowRect(owner, &mut owner_rect);
        let x = owner_rect.left + (owner_rect.right - owner_rect.left - width) / 2;
        let y = owner_rect.top + (owner_rect.bottom - owner_rect.top - height) / 2;
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            owner,
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        ABOUT_HWND.store(hwnd as isize, Ordering::SeqCst);
        let region = CreateRoundRectRgn(
            0,
            0,
            width + 1,
            height + 1,
            scaled(18, scale),
            scaled(18, scale),
        );
        SetWindowRgn(hwnd, region, 1);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

unsafe fn paint_about(hwnd: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.is_null() {
            return;
        }
        let app = state().lock().unwrap();
        let scale = app.scale();
        let palette = Palette::from_theme(app.settings.theme);
        let fonts = Fonts::create(scale);
        let width = scaled(BASE_ABOUT_WIDTH, scale);
        let height = scaled(BASE_ABOUT_HEIGHT, scale);
        fill_rect_color(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            palette.surface,
        );
        draw_round_fill(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            palette.surface,
            scaled(18, scale),
        );
        draw_text(
            hdc,
            "گاه‌یار،",
            scaled_rect(
                RECT {
                    left: 50,
                    top: 14,
                    right: 330,
                    bottom: 52,
                },
                scale,
            ),
            palette.accent,
            fonts.title,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_text(
            hdc,
            "عماد قاسمی - emadghasemi.ir",
            scaled_rect(
                RECT {
                    left: 34,
                    top: 58,
                    right: 346,
                    bottom: 94,
                },
                scale,
            ),
            palette.accent,
            fonts.regular,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_text(
            hdc,
            "github.com/emadgh/GahYar",
            scaled_rect(
                RECT {
                    left: 34,
                    top: 96,
                    right: 346,
                    bottom: 132,
                },
                scale,
            ),
            palette.event,
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            &format!("نسخه {}", persian_digits(env!("CARGO_PKG_VERSION"))),
            scaled_rect(
                RECT {
                    left: 34,
                    top: 136,
                    right: 346,
                    bottom: 172,
                },
                scale,
            ),
            palette.muted,
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        let update_text = match update::status() {
            update::UpdateStatus::Checking => "در حال بررسی…",
            update::UpdateStatus::Downloading => "در حال بروزرسانی…",
            update::UpdateStatus::UpToDate => "برنامه بروز است",
            update::UpdateStatus::Available(_) => "دریافت نسخه جدید",
            update::UpdateStatus::Failed(_) => "تلاش دوباره برای بروزرسانی",
            _ => "بررسی بروزرسانی",
        };
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 70,
                    top: 184,
                    right: 310,
                    bottom: 224,
                },
                scale,
            ),
            palette.surface_alt,
            scaled(12, scale),
        );
        draw_text(
            hdc,
            update_text,
            scaled_rect(
                RECT {
                    left: 70,
                    top: 184,
                    right: 310,
                    bottom: 224,
                },
                scale,
            ),
            palette.accent,
            fonts.small,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_fill(
            hdc,
            scaled_rect(
                RECT {
                    left: 110,
                    top: 242,
                    right: 270,
                    bottom: 280,
                },
                scale,
            ),
            palette.accent,
            scaled(12, scale),
        );
        draw_text(
            hdc,
            "بستن",
            scaled_rect(
                RECT {
                    left: 110,
                    top: 242,
                    right: 270,
                    bottom: 280,
                },
                scale,
            ),
            palette.accent_text,
            fonts.medium,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_round_outline(
            hdc,
            RECT {
                left: 0,
                top: 0,
                right: width - 1,
                bottom: height - 1,
            },
            palette.border,
            scaled(18, scale),
            1,
        );
        drop(app);
        fonts.destroy();
        EndPaint(hwnd, &ps);
    }
}

unsafe extern "system" fn about_window_proc(
    hwnd: HWND,
    msg: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            unsafe {
                paint_about(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = point_from_lparam(lparam);
            let scale = state().lock().unwrap().scale();
            let x = unscaled(x, scale);
            let y = unscaled(y, scale);
            if (30..=350).contains(&x) && (54..=96).contains(&y) {
                unsafe {
                    open_website(hwnd);
                }
            } else if (30..=350).contains(&x) && (96..=136).contains(&y) {
                unsafe {
                    open_github(hwnd);
                }
            } else if (60..=320).contains(&x) && (178..=232).contains(&y) {
                let owner = unsafe { GetWindow(hwnd, GW_OWNER) };
                if !owner.is_null() {
                    request_manual_update(owner);
                }
                unsafe {
                    InvalidateRect(hwnd, null(), 0);
                }
            } else if (100..=280).contains(&x) && (236..=288).contains(&y) {
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_KEYDOWN if _wparam as u32 == 0x1B => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            ABOUT_HWND.store(0, Ordering::SeqCst);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, _wparam, lparam) },
    }
}

fn register_window_classes(instance: HINSTANCE) -> bool {
    unsafe {
        let main_class_name = wide(MAIN_CLASS);
        let about_class_name = wide(ABOUT_CLASS);
        let confirm_class_name = wide(CONFIRM_CLASS);
        let main_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW | CS_DBLCLKS,
            lpfnWndProc: Some(main_window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: load_app_icon(instance),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: main_class_name.as_ptr(),
        };
        let about_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
            lpfnWndProc: Some(about_window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: load_app_icon(instance),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: about_class_name.as_ptr(),
        };
        let confirm_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DROPSHADOW,
            lpfnWndProc: Some(confirm_window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: load_app_icon(instance),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: confirm_class_name.as_ptr(),
        };
        RegisterClassW(&main_class) != 0
            && RegisterClassW(&about_class) != 0
            && RegisterClassW(&confirm_class) != 0
    }
}

unsafe fn activate_existing_instance() {
    let class_name = wide(MAIN_CLASS);
    for _ in 0..20 {
        let hwnd = unsafe { FindWindowW(class_name.as_ptr(), null()) };
        if !hwnd.is_null() {
            unsafe {
                PostMessageW(hwnd, WM_SHOW_EXISTING, 0, 0);
            }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn main() {
    unsafe {
        let mutex_name = wide(INSTANCE_MUTEX_NAME);
        let instance_mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if instance_mutex.is_null() {
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            activate_existing_instance();
            CloseHandle(instance_mutex);
            return;
        }

        let instance = GetModuleHandleW(null());
        install_embedded_font(instance);
        if !register_window_classes(instance) {
            uninstall_embedded_font();
            CloseHandle(instance_mutex);
            return;
        }
        let class_name = wide(MAIN_CLASS);
        let window_title = wide(APP_NAME);
        let app = state().lock().unwrap();
        let width = scaled(BASE_WIDTH, app.scale());
        let height = scaled(app.base_height(), app.scale());
        drop(app);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }

        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        uninstall_embedded_font();
        CloseHandle(instance_mutex);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::{calendar_column, executable_version};

    #[test]
    fn calendar_columns_follow_selected_direction() {
        assert_eq!(calendar_column(0, true), 6);
        assert_eq!(calendar_column(6, true), 0);
        assert_eq!(calendar_column(0, false), 0);
        assert_eq!(calendar_column(6, false), 6);
        for column in 0..7 {
            assert_eq!(calendar_column(calendar_column(column, true), true), column);
        }
    }

    #[test]
    fn executable_version_resource_is_readable() {
        let executable = std::env::current_exe().expect("test executable path");
        let version = executable_version(&executable).expect("embedded file version");
        let expected = env!("CARGO_PKG_VERSION")
            .split('.')
            .map(|part| part.parse::<u16>().expect("numeric package version"))
            .collect::<Vec<_>>();
        assert_eq!(version, (expected[0], expected[1], expected[2], 0));
    }
}
