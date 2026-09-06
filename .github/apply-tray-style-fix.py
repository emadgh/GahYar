from pathlib import Path
import subprocess


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise RuntimeError(f"{path}: expected {expected} occurrence(s), found {count}: {old[:80]!r}")
    text = text.replace(old, new)
    with p.open("w", encoding="utf-8", newline="\n") as f:
        f.write(text)


# Settings model: add a persisted three-state tray appearance setting.
replace_exact(
    "src/settings.rs",
    "#[derive(Clone, Debug)]\npub struct Settings {",
    '''#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
pub struct Settings {''',
)
replace_exact(
    "src/settings.rs",
    "    pub tray_english_digits: bool,\n    pub compact_day: bool,",
    "    pub tray_english_digits: bool,\n    pub tray_icon_style: TrayIconStyle,\n    pub compact_day: bool,",
)
replace_exact(
    "src/settings.rs",
    "            tray_english_digits: false,\n            compact_day: false,",
    "            tray_english_digits: false,\n            tray_icon_style: TrayIconStyle::YellowBlack,\n            compact_day: false,",
)
replace_exact(
    "src/settings.rs",
    '                    "tray_english_digits" => settings.tray_english_digits = parse_bool(value),\n                    "compact_day" => settings.compact_day = parse_bool(value),',
    '                    "tray_english_digits" => settings.tray_english_digits = parse_bool(value),\n                    "tray_icon_style" => settings.tray_icon_style = TrayIconStyle::from_key(value.trim()),\n                    "compact_day" => settings.compact_day = parse_bool(value),',
)
replace_exact(
    "src/settings.rs",
    '            "theme={}\\nui_scale={}\\nmain_calendar={}\\ncalendar_rtl={}\\nshow_jalali={}\\nshow_gregorian={}\\nshow_hijri={}\\nshow_subtitles={}\\nshow_events={}\\nshow_tray_date={}\\nauto_update={}\\ntray_day_icon={}\\ntray_english_digits={}\\ncompact_day={}\\nautostart={}\\n",',
    '            "theme={}\\nui_scale={}\\nmain_calendar={}\\ncalendar_rtl={}\\nshow_jalali={}\\nshow_gregorian={}\\nshow_hijri={}\\nshow_subtitles={}\\nshow_events={}\\nshow_tray_date={}\\nauto_update={}\\ntray_day_icon={}\\ntray_english_digits={}\\ntray_icon_style={}\\ncompact_day={}\\nautostart={}\\n",',
)
replace_exact(
    "src/settings.rs",
    "            self.tray_english_digits,\n            self.compact_day,",
    "            self.tray_english_digits,\n            self.tray_icon_style.key(),\n            self.compact_day,",
)

# Main UI/settings layout.
replace_exact(
    "src/main.rs",
    "use settings::{Settings, Theme, set_autostart};",
    "use settings::{Settings, Theme, TrayIconStyle, set_autostart};",
)
replace_exact("src/main.rs", "const BASE_SETTINGS_HEIGHT: i32 = 878;", "const BASE_SETTINGS_HEIGHT: i32 = 922;")
replace_exact(
    "src/main.rs",
    '''        paint_toggle_row(
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
        );''',
    '''        paint_value_row(
            hdc,
            app,
            palette,
            fonts,
            658,
            "ظاهر شماره در System Tray",
            app.settings.tray_icon_style.title(),
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            702,
            "نمایش روزانه (بدون تقویم)",
            app.settings.compact_day,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            746,
            "اجرا همراه با ویندوز",
            app.settings.autostart,
        );''',
)
replace_exact("src/main.rs", "                top: 754,\n                right: 406,\n                bottom: 794,", "                top: 798,\n                right: 406,\n                bottom: 838,", 2)
replace_exact("src/main.rs", "                left: 24,\n                top: 754,\n                right: 212,\n                bottom: 794,", "                left: 24,\n                top: 798,\n                right: 212,\n                bottom: 838,", 2)
replace_exact("src/main.rs", "                left: 24,\n                top: 801,\n                right: 406,\n                bottom: 843,", "                left: 24,\n                top: 845,\n                right: 406,\n                bottom: 887,", 2)

replace_exact(
    "src/main.rs",
    '''                    } else if (657..=700).contains(&y) {
                        app.settings.compact_day = !app.settings.compact_day;''',
    '''                    } else if (657..=700).contains(&y) {
                        app.settings.tray_icon_style = app.settings.tray_icon_style.next();
                        app.settings.save();
                        refresh_tray_visual = true;
                    } else if (701..=744).contains(&y) {
                        app.settings.compact_day = !app.settings.compact_day;''',
)
replace_exact("src/main.rs", "(701..=748).contains(&y)", "(745..=792).contains(&y)")
replace_exact("src/main.rs", "(750..=798).contains(&y)", "(794..=842).contains(&y)")
replace_exact("src/main.rs", "(799..=854).contains(&y)", "(843..=898).contains(&y)")

old_tray = '''unsafe fn create_tray_day_icon(day: u32, english_digits: bool) -> HICON {
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
'''
new_tray = '''unsafe fn create_tray_day_icon(
    day: u32,
    english_digits: bool,
    style: TrayIconStyle,
) -> HICON {
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
            if style == TrayIconStyle::YellowBlack {
                rgb(248, 211, 88)
            } else {
                rgb(0, 0, 0)
            },
        );

        // White pixels in the monochrome mask are transparent; black pixels are opaque.
        // The yellow mode keeps the rounded tile. Transparent modes only mark glyphs opaque.
        let old_mask_bitmap = SelectObject(mask_dc, mask as HGDIOBJ);
        PatBlt(mask_dc, 0, 0, 32, 32, WHITENESS);
        if style == TrayIconStyle::YellowBlack {
            let old_mask_brush = SelectObject(mask_dc, GetStockObject(BLACK_BRUSH) as HGDIOBJ);
            let old_mask_pen = SelectObject(mask_dc, GetStockObject(BLACK_PEN) as HGDIOBJ);
            RoundRect(mask_dc, 1, 1, 31, 31, 5, 5);
            SelectObject(mask_dc, old_mask_pen);
            SelectObject(mask_dc, old_mask_brush);
        }

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
        let text_rect = if english_digits {
            RECT {
                left: 2,
                top: 2,
                right: 30,
                bottom: 31,
            }
        } else {
            RECT {
                left: 0,
                top: 2,
                right: 32,
                bottom: 32,
            }
        };
        let font = if english_digits {
            create_font(-22, FW_BOLD as i32, "Segoe UI")
        } else {
            create_font(-24, FW_BOLD as i32, "Vazirmatn")
        };
        if english_digits {
            // Segoe UI plus slight negative tracking keeps two-digit days compact and centered.
            SetTextCharacterExtra(color_dc, -2);
            SetTextCharacterExtra(mask_dc, -2);
        }
        let text_color = match style {
            TrayIconStyle::TransparentWhite => rgb(255, 255, 255),
            TrayIconStyle::TransparentBlack | TrayIconStyle::YellowBlack => rgb(18, 18, 18),
        };
        draw_text(
            color_dc,
            &day_text,
            text_rect,
            text_color,
            font,
            text_format,
        );
        if style != TrayIconStyle::YellowBlack {
            draw_text(
                mask_dc,
                &day_text,
                text_rect,
                rgb(0, 0, 0),
                font,
                text_format,
            );
        }
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
'''
replace_exact("src/main.rs", old_tray, new_tray)
replace_exact(
    "src/main.rs",
    "            let icon = create_tray_day_icon(today.day, app.settings.tray_english_digits);",
    "            let icon = create_tray_day_icon(\n                today.day,\n                app.settings.tray_english_digits,\n                app.settings.tray_icon_style,\n            );",
)

# Patch release metadata.
replace_exact("Cargo.toml", 'version = "2.8.0"', 'version = "2.8.1"')
replace_exact("Cargo.lock", 'name = "gahyar"\nversion = "2.8.0"', 'name = "gahyar"\nversion = "2.8.1"')
replace_exact("app.rc", "FILEVERSION 2,8,0,0", "FILEVERSION 2,8,1,0")
replace_exact("app.rc", "PRODUCTVERSION 2,8,0,0", "PRODUCTVERSION 2,8,1,0")
replace_exact("app.rc", 'VALUE "FileVersion", "2.8.0"', 'VALUE "FileVersion", "2.8.1"')
replace_exact("app.rc", 'VALUE "ProductVersion", "2.8.0"', 'VALUE "ProductVersion", "2.8.1"')

notes = '''# گاه‌یار ۲.۸.۱

## اصلاحات System Tray

- اصلاح فاصله بین ارقام انگلیسی و حاشیه‌های آیکن برای نمایش فشرده‌تر و متعادل‌تر اعداد دو رقمی
- استفاده از فونت Segoe UI با tracking فشرده برای ارقام انگلیسی، بدون تغییر نمایش ارقام فارسی
- افزودن گزینه سه‌حالته «ظاهر شماره در System Tray» به تنظیمات:
  - بدون زمینه با متن سفید
  - بدون زمینه با متن مشکی
  - زمینه زرد با متن مشکی
- حفظ حالت «زمینه زرد + متن مشکی» به‌عنوان مقدار پیش‌فرض برای سازگاری با تنظیمات قبلی
- اعمال تغییر ظاهر System Tray بلافاصله و بدون نیاز به اجرای مجدد برنامه

## دریافت

فایل `GahYar.exe` را از بخش Assets همین Release دریافت و اجرا کنید. برنامه قابل‌حمل است و به نصب نیاز ندارد.
'''
with Path("RELEASE_NOTES.md").open("w", encoding="utf-8", newline="\n") as f:
    f.write(notes)

# Remove this temporary migration mechanism from the branch before the actual source commit.
Path(".github/workflows/apply-tray-style-fix.yml").unlink(missing_ok=True)
Path(".github/apply-tray-style-fix.py").unlink(missing_ok=True)

subprocess.run(["git", "config", "user.name", "github-actions[bot]"], check=True)
subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], check=True)
subprocess.run(["git", "add", "-A"], check=True)
subprocess.run(["git", "commit", "-m", "Fix tray digit spacing and add appearance modes"], check=True)
subprocess.run(["git", "push"], check=True)
