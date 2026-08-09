use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::os::windows::process::CommandExt;

use crate::calendar::CalendarKind;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "GahYar";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn key(self) -> &'static str {
        match self { Self::Dark => "dark", Self::Light => "light" }
    }

    pub fn from_key(value: &str) -> Self {
        if value == "light" { Self::Light } else { Self::Dark }
    }

    pub fn title(self) -> &'static str {
        match self { Self::Dark => "تیره", Self::Light => "روشن" }
    }

    pub fn toggle(self) -> Self {
        match self { Self::Dark => Self::Light, Self::Light => Self::Dark }
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub theme: Theme,
    pub ui_scale: u32,
    pub main_calendar: CalendarKind,
    pub calendar_rtl: bool,
    pub show_jalali: bool,
    pub show_gregorian: bool,
    pub show_hijri: bool,
    pub show_subtitles: bool,
    pub show_events: bool,
    pub show_tray_date: bool,
    pub auto_update: bool,
    pub tray_day_icon: bool,
    pub taskbar_widget: bool,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            ui_scale: 90,
            main_calendar: CalendarKind::Jalali,
            calendar_rtl: true,
            show_jalali: true,
            show_gregorian: true,
            show_hijri: true,
            show_subtitles: true,
            show_events: true,
            show_tray_date: true,
            auto_update: true,
            tray_day_icon: false,
            taskbar_widget: false,
            autostart: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut settings = Self::default();
        if let Ok(text) = fs::read_to_string(settings_path()) {
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else { continue; };
                match key.trim() {
                    "theme" => settings.theme = Theme::from_key(value.trim()),
                    "ui_scale" => settings.ui_scale = value.trim().parse::<u32>().unwrap_or(90).clamp(80, 125),
                    "main_calendar" => settings.main_calendar = CalendarKind::from_key(value.trim()),
                    "calendar_rtl" => settings.calendar_rtl = parse_bool(value),
                    "show_jalali" => settings.show_jalali = parse_bool(value),
                    "show_gregorian" => settings.show_gregorian = parse_bool(value),
                    "show_hijri" => settings.show_hijri = parse_bool(value),
                    "show_subtitles" => settings.show_subtitles = parse_bool(value),
                    "show_events" => settings.show_events = parse_bool(value),
                    "show_tray_date" => settings.show_tray_date = parse_bool(value),
                    "auto_update" => settings.auto_update = parse_bool(value),
                    "tray_day_icon" => settings.tray_day_icon = parse_bool(value),
                    // Obsolete since 2.4.2: retain the key only for settings-file compatibility.
                    "taskbar_widget" => settings.taskbar_widget = false,
                    "autostart" => settings.autostart = parse_bool(value),
                    _ => {}
                }
            }
        }
        // The registry is the source of truth, so stale settings cannot display a wrong state.
        settings.autostart = is_autostart_enabled();
        settings.taskbar_widget = false;
        settings
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
        let text = format!(
            "theme={}\nui_scale={}\nmain_calendar={}\ncalendar_rtl={}\nshow_jalali={}\nshow_gregorian={}\nshow_hijri={}\nshow_subtitles={}\nshow_events={}\nshow_tray_date={}\nauto_update={}\ntray_day_icon={}\ntaskbar_widget={}\nautostart={}\n",
            self.theme.key(),
            self.ui_scale,
            self.main_calendar.key(),
            self.calendar_rtl,
            self.show_jalali,
            self.show_gregorian,
            self.show_hijri,
            self.show_subtitles,
            self.show_events,
            self.show_tray_date,
            self.auto_update,
            self.tray_day_icon,
            self.taskbar_widget,
            self.autostart,
        );
        let _ = fs::write(path, text);
    }

    pub fn smaller(&mut self) {
        self.ui_scale = match self.ui_scale {
            0..=80 => 80,
            81..=90 => 80,
            91..=100 => 90,
            101..=110 => 100,
            _ => 110,
        };
    }

    pub fn larger(&mut self) {
        self.ui_scale = match self.ui_scale {
            0..=80 => 90,
            81..=90 => 100,
            91..=100 => 110,
            101..=110 => 125,
            _ => 125,
        };
    }
}

pub fn set_autostart(enabled: bool) -> bool {
    if !enabled && !is_autostart_enabled() {
        return true;
    }

    let mut command = Command::new("reg.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    if enabled {
        let Ok(executable) = std::env::current_exe() else { return false; };
        let value = format!("\"{}\"", executable.display());
        command.args([
            "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &value, "/f",
        ]);
    } else {
        command.args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
    }

    command.status().map(|status| status.success()).unwrap_or(false)
}

fn is_autostart_enabled() -> bool {
    Command::new("reg.exe")
        .args(["query", RUN_KEY, "/v", RUN_VALUE])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "1" | "yes" | "on")
}

fn settings_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("GahYar").join("settings.ini")
}
