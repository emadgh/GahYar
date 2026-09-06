from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"Expected text not found in {path}: {old[:80]!r}")
    p.write_text(text.replace(old, new, 1), encoding="utf-8")

# Tray font sizing/weight correction.
replace_once(
    "src/main.rs",
    '''        let font = if english_digits {\n            create_font(-22, FW_BOLD as i32, "Segoe UI")\n        } else {\n            create_font(-24, FW_BOLD as i32, "Vazirmatn")\n        };''',
    '''        let transparent_style = style != TrayIconStyle::YellowBlack;\n        let font = if english_digits {\n            // English digits use a regular weight. Transparent modes get a larger glyph.\n            create_font(\n                if transparent_style { -26 } else { -22 },\n                FW_NORMAL as i32,\n                "Segoe UI",\n            )\n        } else {\n            // Persian digits stay bold. Transparent modes get a larger glyph.\n            create_font(\n                if transparent_style { -28 } else { -24 },\n                FW_BOLD as i32,\n                "Vazirmatn",\n            )\n        };''',
)

# Patch version metadata to 2.8.2.
replace_once("Cargo.toml", 'version = "2.8.1"', 'version = "2.8.2"')
replace_once("Cargo.lock", 'name = "gahyar"\nversion = "2.8.1"', 'name = "gahyar"\nversion = "2.8.2"')
replace_once("app.rc", "FILEVERSION 2,8,1,0", "FILEVERSION 2,8,2,0")
replace_once("app.rc", "PRODUCTVERSION 2,8,1,0", "PRODUCTVERSION 2,8,2,0")
replace_once("app.rc", 'VALUE "FileVersion", "2.8.1"', 'VALUE "FileVersion", "2.8.2"')
replace_once("app.rc", 'VALUE "ProductVersion", "2.8.1"', 'VALUE "ProductVersion", "2.8.2"')

Path("RELEASE_NOTES.md").write_text(
    """# گاه‌یار ۲.۸.۲\n\n## اصلاحات System Tray\n\n- بزرگ‌تر شدن اندازه عدد روز در دو حالت بدون پس‌زمینه برای خوانایی بهتر\n- نمایش ارقام فارسی با وزن Bold\n- نمایش ارقام انگلیسی با وزن Regular\n- حفظ اندازه جمع‌وجورتر در حالت پس‌زمینه زرد\n\n## دریافت\n\nفایل `GahYar.exe` را از بخش Assets همین Release دریافت و اجرا کنید.\n""",
    encoding="utf-8",
)

# Remove one-shot migration files from the resulting branch commit.
Path(".github/workflows/apply-tray-font-fix.yml").unlink(missing_ok=True)
Path(".github/scripts/apply_tray_font_fix.py").unlink(missing_ok=True)
