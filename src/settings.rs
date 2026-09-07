use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use crate::calendar::CalendarKind;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "GahYar";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn from_key(value: &str) -> Self {
        if value == "light" {
            Self::Light
        } else {
            Self::Dark
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Dark => "تیره",
            Self::Light => "روشن",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayIconStyle {
    TransparentWhite,
    TransparentBlack,
    YellowBlack,
}

impl TrayIconStyle {
    pub fn key(self) -> &'static str {
        match self {
            Self::TransparentWhite => "transparent_white",
            Self::TransparentBlack => "transparent_black",
            Self::YellowBlack => "yellow_black",
        }
    }

    pub fn from_key(value: &str) -> Self {
        match value {
            "transparent_white" => Self::TransparentWhite,
            "transparent_black" => Self::TransparentBlack,
            _ => Self::YellowBlack,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::TransparentWhite => "متن سفید",
            Self::TransparentBlack => "متن مشکی",
            Self::YellowBlack => "زرد + مشکی",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::TransparentWhite => Self::TransparentBlack,
            Self::TransparentBlack => Self::YellowBlack,
            Self::YellowBlack => Self::TransparentWhite,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub theme: Theme,
    pub primary: Option<[u8; 3]>,
    pub accent: Option<[u8; 3]>,
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
    pub tray_english_digits: bool,
    pub tray_text_white: bool,
    pub tray_accent_background: bool,
    pub compact_day: bool,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            primary: None,
            accent: None,
            ui_scale: 100,
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
            tray_english_digits: false,
            tray_text_white: false,
            tray_accent_background: true,
            compact_day: false,
            autostart: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let text = fs::read_to_string(settings_path()).unwrap_or_default();
        let mut settings = Self::from_text(&text);
        // The registry is the source of truth for the startup setting.
        settings.autostart = is_autostart_enabled();
        settings
    }

    fn from_text(text: &str) -> Self {
        let mut settings = Self::default();
        // Legacy appearance is a fallback; explicit new keys always take precedence.
        if let Some(style) = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(key, _)| key.trim() == "tray_icon_style")
            .map(|(_, value)| TrayIconStyle::from_key(value.trim()))
            .last()
        {
            settings.tray_text_white = style == TrayIconStyle::TransparentWhite;
            settings.tray_accent_background = style == TrayIconStyle::YellowBlack;
        }
        {
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                match key.trim() {
                    "theme" => settings.theme = Theme::from_key(value.trim()),
                    "primary" => settings.primary = crate::theme::parse_hex(value),
                    "accent" => settings.accent = crate::theme::parse_hex(value),
                    "ui_scale" => {
                        settings.ui_scale =
                            value.trim().parse::<u32>().unwrap_or(100).clamp(80, 125)
                    }
                    "main_calendar" => {
                        settings.main_calendar = CalendarKind::from_key(value.trim())
                    }
                    "calendar_rtl" => settings.calendar_rtl = parse_bool(value),
                    "show_jalali" => settings.show_jalali = parse_bool(value),
                    "show_gregorian" => settings.show_gregorian = parse_bool(value),
                    "show_hijri" => settings.show_hijri = parse_bool(value),
                    "show_subtitles" => settings.show_subtitles = parse_bool(value),
                    "show_events" => settings.show_events = parse_bool(value),
                    "show_tray_date" => settings.show_tray_date = parse_bool(value),
                    "auto_update" => settings.auto_update = parse_bool(value),
                    "tray_day_icon" => settings.tray_day_icon = parse_bool(value),
                    "tray_english_digits" => settings.tray_english_digits = parse_bool(value),
                    "tray_text_white" => settings.tray_text_white = parse_bool(value),
                    "tray_accent_background" => settings.tray_accent_background = parse_bool(value),
                    "compact_day" => settings.compact_day = parse_bool(value),
                    "autostart" => settings.autostart = parse_bool(value),
                    _ => {}
                }
            }
        }
        settings
    }

    pub fn save(&self) {
        let _ = self.try_save();
    }

    pub fn try_save(&self) -> std::io::Result<()> {
        self.save_to(&settings_path())
    }

    fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temporary, self.to_text())?;
        let wide = |path: &std::path::Path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<u16>>()
        };
        if unsafe {
            MoveFileExW(
                wide(&temporary).as_ptr(),
                wide(path).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            let _ = fs::remove_file(temporary);
            return Err(error);
        }
        Ok(())
    }

    fn to_text(&self) -> String {
        format!(
            "theme={}\nprimary={}\naccent={}\nui_scale={}\nmain_calendar={}\ncalendar_rtl={}\nshow_jalali={}\nshow_gregorian={}\nshow_hijri={}\nshow_subtitles={}\nshow_events={}\nshow_tray_date={}\nauto_update={}\ntray_day_icon={}\ntray_english_digits={}\ntray_text_white={}\ntray_accent_background={}\ncompact_day={}\nautostart={}\n",
            self.theme.key(),
            self.primary.map(crate::theme::hex).unwrap_or_default(),
            self.accent.map(crate::theme::hex).unwrap_or_default(),
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
            self.tray_english_digits,
            self.tray_text_white,
            self.tray_accent_background,
            self.compact_day,
            self.autostart,
        )
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
        let Ok(executable) = std::env::current_exe() else {
            return false;
        };
        let value = format!("\"{}\"", executable.display());
        command.args([
            "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &value, "/f",
        ]);
    } else {
        command.args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
    }

    command
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("GahYar").join("settings.ini")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeColors;

    #[test]
    fn tray_appearance_migration_and_independent_choices_round_trip() {
        for (legacy, white, colored) in [
            ("transparent_white", true, false),
            ("transparent_black", false, false),
            ("yellow_black", false, true),
        ] {
            let settings = Settings::from_text(&format!("tray_icon_style={legacy}\n"));
            assert_eq!(
                (settings.tray_text_white, settings.tray_accent_background),
                (white, colored)
            );
        }
        for white in [false, true] {
            for colored in [false, true] {
                let settings = Settings {
                    tray_text_white: white,
                    tray_accent_background: colored,
                    ..Settings::default()
                };
                let restored = Settings::from_text(&settings.to_text());
                assert_eq!(
                    (restored.tray_text_white, restored.tray_accent_background),
                    (white, colored)
                );
            }
        }
        for text in [
            "tray_text_white=true\ntray_accent_background=true\ntray_icon_style=transparent_black",
            "tray_icon_style=transparent_black\ntray_text_white=true\ntray_accent_background=true",
        ] {
            let s = Settings::from_text(text);
            assert!(s.tray_text_white && s.tray_accent_background);
        }
    }

    #[test]
    fn settings_migrate_and_reject_invalid_colors() {
        let legacy = Settings::from_text("theme=light\nui_scale=110\n");
        assert_eq!(
            ThemeColors::from_settings(&legacy),
            ThemeColors::preset(Theme::Light)
        );
        assert_eq!(legacy.ui_scale, 110);
        let invalid = Settings::from_text("accent=#123\ntheme=light\nprimary=invalid\n");
        assert_eq!(
            ThemeColors::from_settings(&invalid),
            ThemeColors::preset(Theme::Light)
        );
        let reordered = Settings::from_text("accent=#010203\nprimary=#040506\ntheme=light\n");
        assert_eq!(reordered.accent, Some([1, 2, 3]));
        assert_eq!(reordered.primary, Some([4, 5, 6]));
    }

    #[test]
    fn custom_colors_survive_file_replacement() {
        let path =
            std::env::temp_dir().join(format!("gahyar-theme-test-{}.ini", std::process::id()));
        let mut settings = Settings::default();
        settings.save_to(&path).unwrap();
        let colors = ThemeColors {
            theme: Theme::Light,
            primary: [18, 25, 42],
            accent: [100, 0, 240],
        };
        colors.apply(&mut settings);
        settings.save_to(&path).unwrap();
        let loaded = Settings::from_text(&fs::read_to_string(&path).unwrap());
        assert_eq!(ThemeColors::from_settings(&loaded), colors);
        assert_eq!(loaded.main_calendar, settings.main_calendar);
        fs::remove_file(path).unwrap();
    }
}
