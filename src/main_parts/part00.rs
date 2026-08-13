use std::fs;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use calendar::{
    add_month, convert, days_in_month, first_weekday_saturday, from_gregorian, month_name,
    month_range_text, month_range_text_fa, to_gregorian, CalendarKind, Date, WEEKDAYS_SHORT,
};
use events::{CalendarEvent, EventStore};
use settings::{set_autostart, Settings, Theme};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
};
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::{FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource};
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, SetFocus, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE,
};
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
const BASE_HEIGHT_DAILY: i32 = 130;
const BASE_EVENTS_HEIGHT: i32 = 136;
const BASE_FOOTER_HEIGHT: i32 = 30;
const BASE_UPDATE_HEIGHT: i32 = 42;
const BASE_SETTINGS_HEIGHT: i32 = 834;
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
const DAILY_EVENTS_TOP: i32 = 116;

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
        let update_height = if !self.settings.auto_update && update::banner_visible() { BASE_UPDATE_HEIGHT } else { 0 };
        (if self.view == ViewMode::Settings {
            BASE_SETTINGS_HEIGHT + BASE_FOOTER_HEIGHT
        } else {
            let content_height = if self.settings.daily_view { BASE_HEIGHT_DAILY } else { BASE_HEIGHT_CALENDAR };
            content_height
                + if self.settings.show_events { BASE_EVENTS_HEIGHT } else { 0 }
                + BASE_FOOTER_HEIGHT
        }) + update_height
    }

    fn scale(&self) -> u32 { self.settings.ui_scale }

    fn events_top(&self) -> i32 {
        if self.settings.daily_view { DAILY_EVENTS_TOP } else { EVENTS_TOP }
    }

    fn ensure_daily_selection(&mut self) {
        if self.selected_day.is_none() {
            let today = self.today_main();
            self.selected_day = Some(if today.year == self.year && today.month == self.month { today.day } else { 1 });
        }
    }

    fn step_day(&mut self, delta: i32) {
        self.ensure_daily_selection();
        let mut date = Date::new(
            self.year,
            self.month,
            self.selected_day.unwrap_or(1).min(days_in_month(self.settings.main_calendar, self.year, self.month)),
        );
        if delta >= 0 {
            if date.day < days_in_month(self.settings.main_calendar, date.year, date.month) {
                date.day += 1;
            } else {
                date.day = 1;
                add_month(self.settings.main_calendar, &mut date.year, &mut date.month, 1);
            }
        } else if date.day > 1 {
            date.day -= 1;
        } else {
            add_month(self.settings.main_calendar, &mut date.year, &mut date.month, -1);
            date.day = days_in_month(self.settings.main_calendar, date.year, date.month);
        }
        self.year = date.year;
        self.month = date.month;
        self.selected_day = Some(date.day);
        self.event_scroll = 0;
        self.hovered_cell = None;
    }

    fn today_main(&self) -> Date {
        from_gregorian(self.settings.main_calendar, self.today_gregorian)
    }

    fn refresh_today(&mut self) -> bool {
        let mut local: SYSTEMTIME = unsafe { zeroed() };
        unsafe { GetLocalTime(&mut local) };
        let today = Date::new(local.wYear as i32, local.wMonth as u32, local.wDay as u32);
        if today == self.today_gregorian { return false; }
        self.today_gregorian = today;
        true
    }

    fn set_main_calendar(&mut self, next: CalendarKind) {
        let anchor_day = self.selected_day.unwrap_or(1).min(days_in_month(self.settings.main_calendar, self.year, self.month));
        let anchor_g = to_gregorian(self.settings.main_calendar, Date::new(self.year, self.month, anchor_day));
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
    value.to_string().chars().map(|character| match character {
        '0' => '۰', '1' => '۱', '2' => '۲', '3' => '۳', '4' => '۴',
        '5' => '۵', '6' => '۶', '7' => '۷', '8' => '۸', '9' => '۹',
        other => other,
    }).collect()
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
            height, 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32 | FF_DONTCARE as u32,
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
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
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
        RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, radius, radius);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(pen as HGDIOBJ);
    }
}

unsafe fn draw_text(hdc: HDC, text: &str, mut rect: RECT, color: COLORREF, font: HFONT, format: u32) {
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
    const WEEKDAYS: [&str; 7] = ["شنبه", "یکشنبه", "دوشنبه", "سه‌شنبه", "چهارشنبه", "پنجشنبه", "جمعه"];
    let today = app.today_main();
    let day = app.selected_day.unwrap_or_else(|| {
        if app.year == today.year && app.month == today.month { today.day } else { 1 }
    });
    let weekday = (first_weekday_saturday(app.settings.main_calendar, app.year, app.month)
        + day as i32 - 1).rem_euclid(7) as usize;
    let (day_text, year_text) = match app.settings.main_calendar {
        CalendarKind::Gregorian => (day.to_string(), app.year.to_string()),
        CalendarKind::Jalali | CalendarKind::Hijri => (persian_digits(day), persian_digits(app.year)),
    };
    format!("{}، {} {} {}", WEEKDAYS[weekday], day_text, month_name(app.settings.main_calendar, app.month), year_text)
}

fn secondary_ranges(app: &AppState) -> Vec<String> {
    if !app.settings.show_subtitles || app.settings.daily_view { return Vec::new(); }
    let mut values = Vec::new();
    let main = app.settings.main_calendar;
    if main != CalendarKind::Gregorian && app.settings.show_gregorian {
        values.push(month_range_text(main, app.year, app.month, CalendarKind::Gregorian));
    }
    if main != CalendarKind::Jalali && app.settings.show_jalali {
        values.push(month_range_text_fa(main, app.year, app.month, CalendarKind::Jalali, |year| persian_digits(year)));
    }
    if main != CalendarKind::Hijri && app.settings.show_hijri {
        values.push(month_range_text_fa(main, app.year, app.month, CalendarKind::Hijri, |year| persian_digits(year)));
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
    (Date::new(next_year, next_month, (relative - current_days) as u32), false)
}

fn secondary_dates(app: &AppState, primary: Date) -> Vec<(CalendarKind, Date)> {
    let mut values = Vec::new();
    let main = app.settings.main_calendar;
    for kind in [CalendarKind::Jalali, CalendarKind::Gregorian, CalendarKind::Hijri] {
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
    let last = app.selected_day.unwrap_or(days_in_month(main, app.year, app.month));
    for day in first..=last {
        let primary = Date::new(app.year, app.month, day);
        let jalali = convert(primary, main, CalendarKind::Jalali);
        for event in app.events.events_for_day(jalali.year, jalali.month, jalali.day) {
            items.push((day, event));
        }
    }
    items
}
