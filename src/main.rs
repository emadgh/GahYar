#![windows_subsystem = "windows"]

mod calendar;
mod events;
mod settings;
mod update;

use std::mem::{size_of, zeroed};
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
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::{FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource};
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const APP_NAME: &str = "گاه‌یار";
const WEBSITE_URL: &str = "https://emadghasemi.ir";
const APP_ICON_ID: usize = 1;
const MAIN_CLASS: &str = "GahYarMain";
const ABOUT_CLASS: &str = "GahYarAbout";
const BASE_WIDTH: i32 = 430;
const BASE_HEIGHT_CALENDAR: i32 = 517;
const BASE_EVENTS_HEIGHT: i32 = 136;
const BASE_FOOTER_HEIGHT: i32 = 30;
const BASE_UPDATE_HEIGHT: i32 = 42;
const BASE_SETTINGS_HEIGHT: i32 = 658;
const BASE_ABOUT_WIDTH: i32 = 380;
const BASE_ABOUT_HEIGHT: i32 = 250;

const GRID_LEFT: i32 = 18;
const GRID_TOP: i32 = 159;
const CELL_WIDTH: i32 = 56;
const CELL_HEIGHT: i32 = 54;
const GRID_WIDTH: i32 = CELL_WIDTH * 7;
const GRID_BOTTOM_PADDING: i32 = 10;
const EVENTS_TOP: i32 = 503;

const WM_TRAY: u32 = WM_APP + 1;
const WM_SHOW_EXISTING: u32 = WM_APP + 2;
const WM_UPDATE_STATUS: u32 = WM_APP + 3;
const WM_APPLY_UPDATE: u32 = WM_APP + 4;
const TRAY_ID: u32 = 1;
const CMD_OPEN: usize = 1001;
const CMD_SETTINGS: usize = 1002;
const CMD_ABOUT: usize = 1003;
const CMD_EXIT: usize = 1005;
const DATE_REFRESH_TIMER_ID: usize = 1;
const CF_UNICODETEXT_FORMAT: u32 = 13;
const INSTANCE_MUTEX_NAME: &str = "Local\\GahYar.SingleInstance";
const FONT_RESOURCE_ID: usize = 101;
const RCDATA_RESOURCE_TYPE: usize = 10;

static STATE: OnceLock<Mutex<AppState>> = OnceLock::new();
static EXITING: AtomicBool = AtomicBool::new(false);
static ABOUT_HWND: AtomicIsize = AtomicIsize::new(0);
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
        }
    }

    fn base_height(&self) -> i32 {
        let update_height = if update::banner_visible() { BASE_UPDATE_HEIGHT } else { 0 };
        (if self.view == ViewMode::Settings {
            BASE_SETTINGS_HEIGHT + BASE_FOOTER_HEIGHT
        } else {
            BASE_HEIGHT_CALENDAR
                + if self.settings.show_events { BASE_EVENTS_HEIGHT } else { 0 }
                + BASE_FOOTER_HEIGHT
        }) + update_height
    }

    fn scale(&self) -> u32 { self.settings.ui_scale }

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

fn main_month_heading(app: &AppState) -> String {
    match app.settings.main_calendar {
        CalendarKind::Gregorian => format!("{} {}", month_name(CalendarKind::Gregorian, app.month), app.year),
        _ => format!("{} {}", month_name(app.settings.main_calendar, app.month), persian_digits(app.year)),
    }
}

fn secondary_ranges(app: &AppState) -> Vec<String> {
    if !app.settings.show_subtitles { return Vec::new(); }
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

unsafe fn paint_main(hwnd: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = zeroed();
        let window_hdc = BeginPaint(hwnd, &mut ps);
        if window_hdc.is_null() { return; }

        let app = state().lock().unwrap();
        let scale = app.scale();
        let palette = Palette::from_theme(app.settings.theme);
        let fonts = Fonts::create(scale);
        let width = scaled(BASE_WIDTH, scale);
        let height = scaled(app.base_height(), scale);
        let hdc = CreateCompatibleDC(window_hdc);
        let bitmap = CreateCompatibleBitmap(window_hdc, width, height);
        if hdc.is_null() || bitmap.is_null() {
            if !hdc.is_null() { DeleteDC(hdc); }
            if !bitmap.is_null() { DeleteObject(bitmap as HGDIOBJ); }
            drop(app);
            fonts.destroy();
            EndPaint(hwnd, &ps);
            return;
        }
        let old_bitmap = SelectObject(hdc, bitmap as HGDIOBJ);
        fill_rect_color(hdc, RECT { left: 0, top: 0, right: width, bottom: height }, palette.background);
        draw_round_fill(hdc, RECT { left: 0, top: 0, right: width, bottom: height }, palette.surface, scaled(18, scale));

        match app.view {
            ViewMode::Calendar => paint_calendar(hdc, &app, &palette, &fonts),
            ViewMode::Settings => paint_settings(hdc, &app, &palette, &fonts),
        }

        draw_round_outline(
            hdc,
            RECT { left: 0, top: 0, right: width - 1, bottom: height - 1 },
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
        draw_round_fill(hdc, sr(RECT { left: 14, top: 12, right: 66, bottom: 44 }), palette.surface_alt, scaled(12, scale));
        draw_text(hdc, "‹", sr(RECT { left: 14, top: 8, right: 66, bottom: 44 }), palette.accent, fonts.icon, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        draw_round_fill(hdc, sr(RECT { left: 364, top: 12, right: 416, bottom: 44 }), palette.surface_alt, scaled(12, scale));
        draw_text(hdc, "›", sr(RECT { left: 364, top: 8, right: 416, bottom: 44 }), palette.accent, fonts.icon, DT_CENTER | DT_VCENTER | DT_SINGLELINE);

        draw_text(
            hdc,
            &main_month_heading(app),
            sr(RECT { left: 72, top: 7, right: 358, bottom: 43 }),
            palette.accent,
            fonts.title,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );

        let ranges = secondary_ranges(app);
        if let Some(first) = ranges.first() {
            draw_text(hdc, first, sr(RECT { left: 58, top: 44, right: 372, bottom: 62 }), palette.text, fonts.small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        }
        if let Some(second) = ranges.get(1) {
            draw_text(hdc, second, sr(RECT { left: 58, top: 62, right: 372, bottom: 80 }), palette.muted, fonts.small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        }

        draw_round_fill(hdc, sr(RECT { left: 16, top: 57, right: 50, bottom: 89 }), palette.surface_alt, scaled(11, scale));
        draw_text(hdc, "ⓘ", sr(RECT { left: 16, top: 56, right: 50, bottom: 88 }), palette.muted, fonts.icon, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        draw_round_fill(hdc, sr(RECT { left: 380, top: 57, right: 414, bottom: 89 }), palette.surface_alt, scaled(11, scale));
        draw_text(hdc, "⚙", sr(RECT { left: 380, top: 55, right: 414, bottom: 89 }), palette.muted, fonts.icon, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        draw_round_fill(hdc, sr(RECT { left: 174, top: 83, right: 256, bottom: 108 }), palette.accent, scaled(12, scale));
        draw_text(hdc, "برو به امروز", sr(RECT { left: 174, top: 83, right: 256, bottom: 108 }), palette.accent_text, fonts.tiny, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);

        // Weekday header.
        draw_round_fill(hdc, sr(RECT { left: GRID_LEFT, top: 116, right: GRID_LEFT + GRID_WIDTH, bottom: 150 }), palette.accent, scaled(10, scale));
        for (index, weekday) in WEEKDAYS_SHORT.iter().enumerate() {
            let visual_column = calendar_column(index as i32, app.settings.calendar_rtl);
            let left = GRID_LEFT + visual_column * CELL_WIDTH;
            draw_text(
                hdc,
                weekday,
                sr(RECT { left, top: 116, right: left + CELL_WIDTH, bottom: 150 }),
                palette.accent_text,
                fonts.medium,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
            );
        }

        // Calendar grid.
        draw_round_fill(
            hdc,
            sr(RECT { left: GRID_LEFT, top: GRID_TOP, right: GRID_LEFT + GRID_WIDTH, bottom: GRID_TOP + CELL_HEIGHT * 6 + GRID_BOTTOM_PADDING }),
            palette.calendar_panel,
            scaled(14, scale),
        );

        for cell in 0..42 {
            let row = cell / 7;
            let column = cell % 7;
            let visual_column = calendar_column(column, app.settings.calendar_rtl);
            let (primary, in_current_month) = adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
            let gregorian = to_gregorian(app.settings.main_calendar, primary);
            let jalali = from_gregorian(CalendarKind::Jalali, gregorian);
            let is_today = gregorian == app.today_gregorian;
            let is_selected = in_current_month && app.selected_day == Some(primary.day);
            let is_friday = column == 6;
            let official_holiday = app.events.is_official_holiday(jalali.year, jalali.month, jalali.day);
            let has_events = !app.events.events_for_day(jalali.year, jalali.month, jalali.day).is_empty();

            let left = GRID_LEFT + visual_column * CELL_WIDTH;
            let top = GRID_TOP + row * CELL_HEIGHT;
            let cell_rect = sr(RECT { left: left + 3, top: top + 3, right: left + CELL_WIDTH - 3, bottom: top + CELL_HEIGHT - 3 });
            if is_selected {
                draw_round_fill(hdc, cell_rect, palette.selected, scaled(9, scale));
                draw_round_outline(hdc, cell_rect, palette.accent, scaled(9, scale), scaled(2, scale));
            } else if is_today {
                draw_round_outline(hdc, cell_rect, palette.accent, scaled(9, scale), scaled(2, scale));
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
                sr(RECT { left: left + 5, top: top + 4, right: left + CELL_WIDTH - 5, bottom: top + 31 }),
                primary_color,
                fonts.day,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
            );

            let secondary = secondary_dates(app, primary);
            let secondary_color = if in_current_month { palette.muted } else { palette.faint };
            if secondary.len() == 1 {
                let (kind, date) = secondary[0];
                draw_text(
                    hdc,
                    &secondary_day_text(kind, date.day),
                    sr(RECT { left: left + 3, top: top + 32, right: left + CELL_WIDTH - 3, bottom: top + 49 }),
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
                    sr(RECT { left: left + 3, top: top + 32, right: left + CELL_WIDTH / 2 + 1, bottom: top + 49 }),
                    secondary_color,
                    fonts.tiny,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                );
                draw_text(
                    hdc,
                    &secondary_day_text(second_kind, second_date.day),
                    sr(RECT { left: left + CELL_WIDTH / 2 - 1, top: top + 32, right: left + CELL_WIDTH - 3, bottom: top + 49 }),
                    secondary_color,
                    fonts.tiny,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
                );
            }

            if has_events {
                let dot_color = if official_holiday { palette.holiday } else { palette.event };
                let dot_rect = sr(RECT { left: left + CELL_WIDTH / 2 - 2, top: top + 49, right: left + CELL_WIDTH / 2 + 3, bottom: top + 53 });
                draw_round_fill(hdc, dot_rect, dot_color, scaled(3, scale));
            }
        }

        if app.settings.show_events {
            paint_events(hdc, app, palette, fonts);
        }
        paint_footer(hdc, app, palette, fonts);
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
        let bottom = EVENTS_TOP + BASE_EVENTS_HEIGHT - 8;
        draw_round_fill(hdc, sr(RECT { left: 18, top: EVENTS_TOP, right: 412, bottom }), palette.surface_alt, scaled(13, scale));

        let section_title = if let Some(day) = app.selected_day {
            format!("مناسبت‌های روز {}", persian_digits(day))
        } else {
            "مناسبت‌های این ماه".to_string()
        };
        draw_text(hdc, &section_title, sr(RECT { left: 30, top: EVENTS_TOP + 8, right: 400, bottom: EVENTS_TOP + 34 }), palette.text, fonts.medium, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);

        let items = event_items_for_view(app);
        if items.is_empty() {
            let message = if app.events.source_year == convert(Date::new(app.year, app.month, 1), app.settings.main_calendar, CalendarKind::Jalali).year {
                "برای این بازه مناسبتی ثبت نشده است."
            } else {
                "فایل رویداد پیوست فقط اطلاعات سال ۱۴۰۵ را دارد."
            };
            draw_text(hdc, message, sr(RECT { left: 30, top: EVENTS_TOP + 39, right: 398, bottom: bottom - 8 }), palette.muted, fonts.small, DT_RIGHT | DT_VCENTER | DT_WORDBREAK | DT_RTLREADING);
            return;
        }

        let item_count = items.len();
        let max_scroll = item_count.saturating_sub(3);
        let start = app.event_scroll.min(max_scroll);
        let mut y = EVENTS_TOP + 38;
        for (day, event) in items.into_iter().skip(start).take(3) {
            let item_bottom = y + 28;
            let badge_color = if event.official_holiday { palette.holiday } else { palette.event };
            draw_round_fill(hdc, sr(RECT { left: 356, top: y + 2, right: 399, bottom: item_bottom }), palette.calendar_panel, scaled(10, scale));
            draw_text(hdc, &persian_digits(day), sr(RECT { left: 356, top: y + 2, right: 399, bottom: item_bottom }), badge_color, fonts.small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
            draw_text(
                hdc,
                &event.title,
                sr(RECT { left: 30, top: y, right: 348, bottom: item_bottom }),
                if event.official_holiday { palette.holiday } else { palette.text },
                fonts.small,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_RTLREADING,
            );
            y += 30;
        }

        if item_count > 3 {
            let track_top = EVENTS_TOP + 40;
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
                sr(RECT { left: 22, top: track_top, right: 27, bottom: track_bottom }),
                palette.border,
                scaled(3, scale),
            );
            draw_round_fill(
                hdc,
                sr(RECT { left: 21, top: track_top + thumb_offset, right: 28, bottom: track_top + thumb_offset + thumb_height }),
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
            "نوشته شده توسط عماد قاسمی — emadghasemi.ir",
            scaled_rect(RECT { left: 16, top, right: BASE_WIDTH - 16, bottom: top + BASE_FOOTER_HEIGHT - 3 }, scale),
            palette.accent,
            fonts.tiny,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
    }
}

unsafe fn paint_update_banner(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts, footer_top: i32) {
    let status = update::status();
    let (text, color) = match status {
        update::UpdateStatus::Available(info) => (
            format!("نسخه جدید {} منتشر شده — برای بروزرسانی کلیک کنید", persian_digits(info.version)),
            palette.accent,
        ),
        update::UpdateStatus::Downloading => ("در حال دریافت و نصب نسخه جدید…".to_string(), palette.event),
        update::UpdateStatus::Failed(_) => ("بروزرسانی خودکار ناموفق بود — دانلود دستی".to_string(), palette.holiday),
        _ => return,
    };
    unsafe {
        let scale = app.scale();
        let top = footer_top - BASE_UPDATE_HEIGHT;
        draw_round_fill(
            hdc,
            scaled_rect(RECT { left: 18, top: top + 4, right: BASE_WIDTH - 18, bottom: footer_top - 3 }, scale),
            palette.surface_alt,
            scaled(11, scale),
        );
        draw_text(
            hdc,
            &text,
            scaled_rect(RECT { left: 28, top: top + 4, right: BASE_WIDTH - 28, bottom: footer_top - 3 }, scale),
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
        draw_text(hdc, "تنظیمات", sr(RECT { left: 70, top: 10, right: 360, bottom: 48 }), palette.accent, fonts.title, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_round_fill(hdc, sr(RECT { left: 370, top: 12, right: 416, bottom: 46 }), palette.surface_alt, scaled(11, scale));
        draw_text(hdc, "›", sr(RECT { left: 370, top: 8, right: 416, bottom: 46 }), palette.accent, fonts.icon, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        draw_text(hdc, "تقویم اصلی همیشه نمایش داده می‌شود؛ موارد زیر برای نمایش تقویم‌های جانبی هستند.", sr(RECT { left: 26, top: 48, right: 404, bottom: 76 }), palette.muted, fonts.tiny, DT_RIGHT | DT_VCENTER | DT_WORDBREAK | DT_RTLREADING);

        paint_value_row(hdc, app, palette, fonts, 78, "پوسته", app.settings.theme.title());
        paint_scale_row(hdc, app, palette, fonts, 122);
        paint_value_row(hdc, app, palette, fonts, 166, "تقویم اصلی", app.settings.main_calendar.title());
        paint_toggle_row(hdc, app, palette, fonts, 218, "چیدمان تقویم از راست به چپ", app.settings.calendar_rtl);
        paint_toggle_row(hdc, app, palette, fonts, 262, "نمایش تاریخ شمسی", app.settings.show_jalali);
        paint_toggle_row(hdc, app, palette, fonts, 306, "نمایش تاریخ میلادی", app.settings.show_gregorian);
        paint_toggle_row(hdc, app, palette, fonts, 350, "نمایش تاریخ قمری", app.settings.show_hijri);
        paint_toggle_row(hdc, app, palette, fonts, 394, "نمایش عنوان تقویم‌های جانبی", app.settings.show_subtitles);
        paint_toggle_row(hdc, app, palette, fonts, 438, "نمایش بخش مناسبت‌ها", app.settings.show_events);
        paint_toggle_row(hdc, app, palette, fonts, 482, "نمایش تاریخ کامل در Tooltip", app.settings.show_tray_date);
        paint_toggle_row(hdc, app, palette, fonts, 526, "اجرا همراه با ویندوز", app.settings.autostart);

        draw_round_fill(hdc, sr(RECT { left: 24, top: 581, right: 406, bottom: 623 }), palette.accent, scaled(12, scale));
        draw_text(hdc, "بازنشانی تنظیمات", sr(RECT { left: 24, top: 581, right: 406, bottom: 623 }), palette.accent_text, fonts.medium, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        paint_footer(hdc, app, palette, fonts);
    }
}

unsafe fn paint_value_row(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts, top: i32, label: &str, value: &str) {
    unsafe {
        let scale = app.scale();
        draw_round_fill(hdc, scaled_rect(RECT { left: 24, top, right: 406, bottom: top + 40 }, scale), palette.surface_alt, scaled(10, scale));
        draw_text(hdc, label, scaled_rect(RECT { left: 170, top, right: 390, bottom: top + 40 }, scale), palette.text, fonts.regular, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_round_fill(hdc, scaled_rect(RECT { left: 34, top: top + 6, right: 148, bottom: top + 34 }, scale), palette.calendar_panel, scaled(11, scale));
        draw_text(hdc, value, scaled_rect(RECT { left: 34, top: top + 6, right: 148, bottom: top + 34 }, scale), palette.accent, fonts.small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
    }
}

unsafe fn paint_scale_row(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts, top: i32) {
    unsafe {
        let scale = app.scale();
        draw_round_fill(hdc, scaled_rect(RECT { left: 24, top, right: 406, bottom: top + 40 }, scale), palette.surface_alt, scaled(10, scale));
        draw_text(hdc, "مقیاس رابط کاربری", scaled_rect(RECT { left: 170, top, right: 390, bottom: top + 40 }, scale), palette.text, fonts.regular, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_round_fill(hdc, scaled_rect(RECT { left: 30, top: top + 6, right: 62, bottom: top + 34 }, scale), palette.calendar_panel, scaled(9, scale));
        draw_text(hdc, "−", scaled_rect(RECT { left: 30, top: top + 4, right: 62, bottom: top + 34 }, scale), palette.accent, fonts.icon, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        draw_text(hdc, &format!("٪{}", persian_digits(app.settings.ui_scale)), scaled_rect(RECT { left: 64, top: top + 5, right: 116, bottom: top + 35 }, scale), palette.text, fonts.small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_round_fill(hdc, scaled_rect(RECT { left: 118, top: top + 6, right: 150, bottom: top + 34 }, scale), palette.calendar_panel, scaled(9, scale));
        draw_text(hdc, "+", scaled_rect(RECT { left: 118, top: top + 5, right: 150, bottom: top + 34 }, scale), palette.accent, fonts.icon, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    }
}

unsafe fn paint_toggle_row(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts, top: i32, label: &str, enabled: bool) {
    unsafe {
        let scale = app.scale();
        draw_round_fill(hdc, scaled_rect(RECT { left: 24, top, right: 406, bottom: top + 40 }, scale), palette.surface_alt, scaled(10, scale));
        draw_text(hdc, label, scaled_rect(RECT { left: 102, top, right: 390, bottom: top + 40 }, scale), palette.text, fonts.regular, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        let track = scaled_rect(RECT { left: 35, top: top + 9, right: 83, bottom: top + 31 }, scale);
        draw_round_fill(hdc, track, if enabled { palette.accent } else { palette.faint }, scaled(11, scale));
        let knob_left = if enabled { 61 } else { 37 };
        draw_round_fill(hdc, scaled_rect(RECT { left: knob_left, top: top + 11, right: knob_left + 18, bottom: top + 29 }, scale), if enabled { palette.accent_text } else { palette.surface }, scaled(9, scale));
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
    let mut refresh_tooltip = false;
    let mut start_update = false;
    let mut update_release_url = None;
    let mut resize = false;
    {
        let mut app = state().lock().unwrap();
        let x = unscaled(x, app.scale());
        let y = unscaled(y, app.scale());
        let footer_top = app.base_height() - BASE_FOOTER_HEIGHT;
        if y >= footer_top {
            open_website_link = true;
        } else if update::banner_visible() && y >= footer_top - BASE_UPDATE_HEIGHT {
            match update::status() {
                update::UpdateStatus::Available(_) => start_update = true,
                update::UpdateStatus::Failed(info) => update_release_url = Some(info.release_url),
                _ => {}
            }
        } else { match app.view {
            ViewMode::Calendar => {
                if y >= 8 && y <= 48 && x <= 75 {
                    let kind = app.settings.main_calendar;
                    let (mut year, mut month) = (app.year, app.month);
                    add_month(kind, &mut year, &mut month, if app.settings.calendar_rtl { 1 } else { -1 });
                    app.year = year;
                    app.month = month;
                    app.selected_day = None;
                    app.event_scroll = 0;
                } else if y >= 8 && y <= 48 && x >= 355 {
                    let kind = app.settings.main_calendar;
                    let (mut year, mut month) = (app.year, app.month);
                    add_month(kind, &mut year, &mut month, if app.settings.calendar_rtl { -1 } else { 1 });
                    app.year = year;
                    app.month = month;
                    app.selected_day = None;
                    app.event_scroll = 0;
                } else if (54..=94).contains(&y) && x <= 62 {
                    show_about_dialog = true;
                } else if (54..=94).contains(&y) && x >= 368 {
                    app.view = ViewMode::Settings;
                    resize = true;
                } else if (78..=113).contains(&y) && (160..=270).contains(&x) {
                    let today = app.today_main();
                    app.year = today.year;
                    app.month = today.month;
                    app.selected_day = Some(today.day);
                    app.event_scroll = 0;
                } else if (GRID_TOP..GRID_TOP + CELL_HEIGHT * 6).contains(&y)
                    && (GRID_LEFT..GRID_LEFT + GRID_WIDTH).contains(&x)
                {
                    let visual_column = (x - GRID_LEFT) / CELL_WIDTH;
                    let column = calendar_column(visual_column, app.settings.calendar_rtl);
                    let row = (y - GRID_TOP) / CELL_HEIGHT;
                    let cell = row * 7 + column;
                    let (clicked, in_current) = adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
                    if !in_current {
                        app.year = clicked.year;
                        app.month = clicked.month;
                    }
                    app.selected_day = if in_current && app.selected_day == Some(clicked.day) { None } else { Some(clicked.day) };
                    app.event_scroll = 0;
                } else if app.settings.show_events
                    && (16..=36).contains(&x)
                    && (EVENTS_TOP + 36..=EVENTS_TOP + BASE_EVENTS_HEIGHT - 8).contains(&y)
                {
                    let count = event_items_for_view(&app).len();
                    let max_scroll = count.saturating_sub(3);
                    if max_scroll > 0 {
                        let track_top = EVENTS_TOP + 40;
                        let track_bottom = EVENTS_TOP + BASE_EVENTS_HEIGHT - 18;
                        let relative = (y - track_top).clamp(0, track_bottom - track_top);
                        app.event_scroll = (relative as usize * max_scroll / (track_bottom - track_top) as usize).min(max_scroll);
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
                    if x <= 68 { app.settings.smaller(); }
                    else if (112..=160).contains(&x) { app.settings.larger(); }
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
                } else if (524..=570).contains(&y) {
                    let next = !app.settings.autostart;
                    if set_autostart(next) {
                        app.settings.autostart = next;
                        app.settings.save();
                    }
                } else if (574..=634).contains(&y) {
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
                }
            }
        }}
    }
    if resize { unsafe { resize_main_window(hwnd, true); } }
    unsafe { InvalidateRect(hwnd, null(), 0); }
    if show_about_dialog { unsafe { show_about(hwnd); } }
    if open_website_link { unsafe { open_website(hwnd); } }
    if refresh_tooltip { unsafe { refresh_tray_tooltip(hwnd); } }
    if start_update { update::start_download(hwnd, WM_UPDATE_STATUS, WM_APPLY_UPDATE); }
    if let Some(url) = update_release_url { unsafe { open_url(hwnd, &url); } }
}

unsafe fn install_embedded_font(instance: HINSTANCE) {
    unsafe {
        let resource = FindResourceW(
            instance,
            FONT_RESOURCE_ID as *const u16,
            RCDATA_RESOURCE_TYPE as *const u16,
        );
        if resource.is_null() { return; }
        let size = SizeofResource(instance, resource);
        let loaded = LoadResource(instance, resource);
        if size == 0 || loaded.is_null() { return; }
        let data = LockResource(loaded);
        if data.is_null() { return; }
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
        unsafe { RemoveFontMemResourceEx(handle); }
    }
}

fn clipboard_date(kind: CalendarKind, date: Date) -> String {
    let value = format!("{:04}/{:02}/{:02}", date.year, date.month, date.day);
    if kind == CalendarKind::Gregorian { value } else { persian_digits(value) }
}

unsafe fn copy_text_to_clipboard(hwnd: HWND, text: &str) -> bool {
    unsafe {
        if OpenClipboard(hwnd) == 0 { return false; }
        if EmptyClipboard() == 0 { CloseClipboard(); return false; }
        let content = wide(text);
        let byte_len = content.len() * size_of::<u16>();
        let memory = GlobalAlloc(GMEM_MOVEABLE, byte_len);
        if memory.is_null() { CloseClipboard(); return false; }
        let target = GlobalLock(memory) as *mut u16;
        if target.is_null() { GlobalFree(memory); CloseClipboard(); return false; }
        std::ptr::copy_nonoverlapping(content.as_ptr(), target, content.len());
        GlobalUnlock(memory);
        let accepted = !SetClipboardData(CF_UNICODETEXT_FORMAT, memory as HANDLE).is_null();
        if !accepted { GlobalFree(memory); }
        CloseClipboard();
        accepted
    }
}

unsafe fn copy_date_at_point(hwnd: HWND, x: i32, y: i32) {
    let text = {
        let app = state().lock().unwrap();
        if app.view != ViewMode::Calendar { return; }
        let x = unscaled(x, app.scale());
        let y = unscaled(y, app.scale());
        if !(GRID_LEFT..GRID_LEFT + GRID_WIDTH).contains(&x)
            || !(GRID_TOP..GRID_TOP + CELL_HEIGHT * 6).contains(&y) { return; }
        let visual_column = (x - GRID_LEFT) / CELL_WIDTH;
        let column = calendar_column(visual_column, app.settings.calendar_rtl);
        let row = (y - GRID_TOP) / CELL_HEIGHT;
        let (date, _) = adjacent_date(app.settings.main_calendar, app.year, app.month, row * 7 + column);
        clipboard_date(app.settings.main_calendar, date)
    };
    unsafe { copy_text_to_clipboard(hwnd, &text); }
}

unsafe fn load_app_icon(instance: HINSTANCE) -> HICON {
    unsafe { LoadIconW(instance, APP_ICON_ID as *const u16) }
}

unsafe fn open_website(hwnd: HWND) {
    unsafe { open_url(hwnd, WEBSITE_URL); }
}

unsafe fn open_url(hwnd: HWND, address: &str) {
    unsafe {
        let operation = wide("open");
        let url = wide(address);
        ShellExecuteW(hwnd, operation.as_ptr(), url.as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

unsafe fn add_tray_icon(hwnd: HWND) {
    unsafe {
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        data.hIcon = load_app_icon(GetModuleHandleW(null()));
        let tip = wide(&tray_tooltip(&state().lock().unwrap()));
        let count = tip.len().min(data.szTip.len());
        data.szTip[..count].copy_from_slice(&tip[..count]);
        Shell_NotifyIconW(NIM_ADD, &data);
    }
}

fn tray_tooltip(app: &AppState) -> String {
    if !app.settings.show_tray_date { return APP_NAME.to_string(); }
    const WEEKDAYS: [&str; 7] = ["شنبه", "یکشنبه", "دوشنبه", "سه‌شنبه", "چهارشنبه", "پنجشنبه", "جمعه"];
    let jalali = from_gregorian(CalendarKind::Jalali, app.today_gregorian);
    let gregorian = app.today_gregorian;
    let weekday = (first_weekday_saturday(CalendarKind::Jalali, jalali.year, jalali.month)
        + jalali.day as i32 - 1).rem_euclid(7) as usize;
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

unsafe fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let mut data: NOTIFYICONDATAW = zeroed();
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = TRAY_ID;
        Shell_NotifyIconW(NIM_DELETE, &data);
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
        let y = if preserve_bottom { rect.bottom - height } else { rect.top };
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
        if IsWindowVisible(hwnd) != 0 { ShowWindow(hwnd, SW_HIDE); } else { show_popup(hwnd); }
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() { return; }
        let open = wide("باز کردن گاه‌یار");
        let settings_text = wide("تنظیمات");
        let about = wide("درباره گاه‌یار");
        let exit = wide("خروج");
        AppendMenuW(menu, MF_STRING, CMD_OPEN, open.as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_SETTINGS, settings_text.as_ptr());
        AppendMenuW(menu, MF_STRING, CMD_ABOUT, about.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, CMD_EXIT, exit.as_ptr());
        let mut point: POINT = zeroed();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, point.x, point.y, 0, hwnd, null());
        DestroyMenu(menu);
        match command as usize {
            CMD_OPEN => show_popup(hwnd),
            CMD_SETTINGS => {
                state().lock().unwrap().view = ViewMode::Settings;
                resize_main_window(hwnd, true);
                show_popup(hwnd);
            }
            CMD_ABOUT => show_about(hwnd),
            CMD_EXIT => {
                EXITING.store(true, Ordering::SeqCst);
                DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn main_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            unsafe { add_tray_icon(hwnd); SetTimer(hwnd, DATE_REFRESH_TIMER_ID, 60_000, None); resize_main_window(hwnd, false); }
            update::start_check(hwnd, WM_UPDATE_STATUS);
            0
        }
        WM_PAINT => { unsafe { paint_main(hwnd); } 0 }
        WM_ERASEBKGND => 1,
        WM_UPDATE_STATUS => {
            unsafe { resize_main_window(hwnd, true); InvalidateRect(hwnd, null(), 0); }
            0
        }
        WM_APPLY_UPDATE => {
            EXITING.store(true, Ordering::SeqCst);
            unsafe { DestroyWindow(hwnd); }
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = point_from_lparam(lparam);
            unsafe { handle_main_click(hwnd, x, y); }
            0
        }
        WM_LBUTTONDBLCLK => {
            let (x, y) = point_from_lparam(lparam);
            unsafe { copy_date_at_point(hwnd, x, y); }
            0
        }
        WM_TIMER if wparam == DATE_REFRESH_TIMER_ID => {
            let changed = state().lock().unwrap().refresh_today();
            if changed { unsafe { refresh_tray_tooltip(hwnd); InvalidateRect(hwnd, null(), 0); } }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) & 0xffff) as i16 as i32;
            let mut app = state().lock().unwrap();
            if app.view == ViewMode::Calendar && app.settings.show_events {
                let count = event_items_for_view(&app).len();
                let max_scroll = count.saturating_sub(3);
                if delta < 0 { app.event_scroll = (app.event_scroll + 1).min(max_scroll); }
                else { app.event_scroll = app.event_scroll.saturating_sub(1); }
            }
            drop(app);
            unsafe { InvalidateRect(hwnd, null(), 0); }
            0
        }
        WM_KEYDOWN => {
            match wparam as u32 {
                0x1B => {
                    let mut app = state().lock().unwrap();
                    if app.view == ViewMode::Settings {
                        app.view = ViewMode::Calendar;
                        drop(app);
                        unsafe { resize_main_window(hwnd, true); InvalidateRect(hwnd, null(), 0); }
                    } else {
                        drop(app);
                        unsafe { ShowWindow(hwnd, SW_HIDE); }
                    }
                }
                0x74 => {
                    state().lock().unwrap().events = EventStore::load();
                    unsafe { InvalidateRect(hwnd, null(), 0); }
                }
                0x25 | 0x27 => {
                    let left = wparam as u32 == 0x25;
                    let mut app = state().lock().unwrap();
                    if app.view == ViewMode::Calendar {
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
                    drop(app);
                    unsafe { InvalidateRect(hwnd, null(), 0); }
                }
                _ => {}
            }
            0
        }
        WM_ACTIVATE => {
            if (wparam as u32 & 0xffff) == WA_INACTIVE
                && !EXITING.load(Ordering::SeqCst)
                && ABOUT_HWND.load(Ordering::SeqCst) == 0
            {
                unsafe { ShowWindow(hwnd, SW_HIDE); }
            }
            0
        }
        WM_CLOSE => { unsafe { ShowWindow(hwnd, SW_HIDE); } 0 }
        WM_SHOW_EXISTING => {
            state().lock().unwrap().view = ViewMode::Calendar;
            unsafe { resize_main_window(hwnd, true); show_popup(hwnd); }
            0
        }
        WM_TRAY => {
            match lparam as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => unsafe { toggle_popup(hwnd); },
                WM_RBUTTONUP | WM_CONTEXTMENU => unsafe { show_tray_menu(hwnd); },
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            unsafe { KillTimer(hwnd, DATE_REFRESH_TIMER_ID); remove_tray_icon(hwnd); PostQuitMessage(0); }
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
            x, y, width, height,
            owner, null_mut(), instance, null(),
        );
        if hwnd.is_null() { return; }
        ABOUT_HWND.store(hwnd as isize, Ordering::SeqCst);
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, scaled(18, scale), scaled(18, scale));
        SetWindowRgn(hwnd, region, 1);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }
}

unsafe fn paint_about(hwnd: HWND) {
    unsafe {
        let mut ps: PAINTSTRUCT = zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        if hdc.is_null() { return; }
        let app = state().lock().unwrap();
        let scale = app.scale();
        let palette = Palette::from_theme(app.settings.theme);
        let fonts = Fonts::create(scale);
        let width = scaled(BASE_ABOUT_WIDTH, scale);
        let height = scaled(BASE_ABOUT_HEIGHT, scale);
        fill_rect_color(hdc, RECT { left: 0, top: 0, right: width, bottom: height }, palette.surface);
        draw_round_fill(hdc, RECT { left: 0, top: 0, right: width, bottom: height }, palette.surface, scaled(18, scale));
        draw_text(hdc, "گاه‌یار،", scaled_rect(RECT { left: 50, top: 14, right: 330, bottom: 52 }, scale), palette.accent, fonts.title, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_text(hdc, "نوشته شده توسط عماد قاسمی", scaled_rect(RECT { left: 34, top: 58, right: 346, bottom: 94 }, scale), palette.text, fonts.regular, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_text(hdc, "emadghasemi.ir", scaled_rect(RECT { left: 34, top: 96, right: 346, bottom: 132 }, scale), palette.accent, fonts.medium, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        draw_text(hdc, &format!("نسخه {}", persian_digits(env!("CARGO_PKG_VERSION"))), scaled_rect(RECT { left: 34, top: 136, right: 346, bottom: 172 }, scale), palette.muted, fonts.small, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_round_fill(hdc, scaled_rect(RECT { left: 110, top: 190, right: 270, bottom: 228 }, scale), palette.accent, scaled(12, scale));
        draw_text(hdc, "بستن", scaled_rect(RECT { left: 110, top: 190, right: 270, bottom: 228 }, scale), palette.accent_text, fonts.medium, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        draw_round_outline(hdc, RECT { left: 0, top: 0, right: width - 1, bottom: height - 1 }, palette.border, scaled(18, scale), 1);
        drop(app);
        fonts.destroy();
        EndPaint(hwnd, &ps);
    }
}

unsafe extern "system" fn about_window_proc(hwnd: HWND, msg: u32, _wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => { unsafe { paint_about(hwnd); } 0 }
        WM_LBUTTONUP => {
            let (x, y) = point_from_lparam(lparam);
            let scale = state().lock().unwrap().scale();
            let x = unscaled(x, scale);
            let y = unscaled(y, scale);
            if (30..=350).contains(&x) && (102..=170).contains(&y) {
                unsafe { open_website(hwnd); }
            } else if (100..=280).contains(&x) && (184..=236).contains(&y) {
                unsafe { DestroyWindow(hwnd); }
            }
            0
        }
        WM_KEYDOWN if _wparam as u32 == 0x1B => { unsafe { DestroyWindow(hwnd); } 0 }
        WM_CLOSE => { unsafe { DestroyWindow(hwnd); } 0 }
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
        RegisterClassW(&main_class) != 0 && RegisterClassW(&about_class) != 0
    }
}

unsafe fn activate_existing_instance() {
    let class_name = wide(MAIN_CLASS);
    for _ in 0..20 {
        let hwnd = unsafe { FindWindowW(class_name.as_ptr(), null()) };
        if !hwnd.is_null() {
            unsafe { PostMessageW(hwnd, WM_SHOW_EXISTING, 0, 0); }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn main() {
    unsafe {
        let mutex_name = wide(INSTANCE_MUTEX_NAME);
        let instance_mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if instance_mutex.is_null() { return; }
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
            CW_USEDEFAULT, CW_USEDEFAULT, width, height,
            null_mut(), null_mut(), instance, null(),
        );
        if hwnd.is_null() { return; }

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
    use super::calendar_column;

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
}
