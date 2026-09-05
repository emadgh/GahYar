$ErrorActionPreference = 'Stop'
$path = 'src/main.rs'
$script:text = (Get-Content -LiteralPath $path -Raw -Encoding utf8).Replace("`r`n", "`n")

function Replace-Exact([string]$old, [string]$new, [int]$expected, [string]$label) {
    $old = $old.Replace("`r`n", "`n")
    $new = $new.Replace("`r`n", "`n")
    $count = 0
    $offset = 0
    while (($index = $script:text.IndexOf($old, $offset, [System.StringComparison]::Ordinal)) -ge 0) {
        $count++
        $offset = $index + $old.Length
    }
    if ($count -ne $expected) {
        throw "${label}: expected $expected match(es), found $count"
    }
    $script:text = $script:text.Replace($old, $new)
}

Replace-Exact 'const BASE_SETTINGS_HEIGHT: i32 = 834;' 'const BASE_SETTINGS_HEIGHT: i32 = 878;' 1 'settings height'

$oldRows = @'
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            614,
            "نمایش روزانه (بدون تقویم)",
            app.settings.compact_day,
        );
        paint_toggle_row(
            hdc,
            app,
            palette,
            fonts,
            658,
            "اجرا همراه با ویندوز",
            app.settings.autostart,
        );
'@
$newRows = @'
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
'@
Replace-Exact $oldRows $newRows 1 'settings rows'

$oldInstallRight = @'
                top: 710,
                right: 406,
                bottom: 750,
'@
$newInstallRight = @'
                top: 754,
                right: 406,
                bottom: 794,
'@
Replace-Exact $oldInstallRight $newInstallRight 2 'install button right'

$oldInstallLeft = @'
                left: 24,
                top: 710,
                right: 212,
                bottom: 750,
'@
$newInstallLeft = @'
                left: 24,
                top: 754,
                right: 212,
                bottom: 794,
'@
Replace-Exact $oldInstallLeft $newInstallLeft 2 'install button left'

$oldReset = @'
                left: 24,
                top: 757,
                right: 406,
                bottom: 799,
'@
$newReset = @'
                left: 24,
                top: 801,
                right: 406,
                bottom: 843,
'@
Replace-Exact $oldReset $newReset 2 'reset button'

$oldCompactClick = @'
                    } else if (613..=656).contains(&y) {
                        app.settings.compact_day = !app.settings.compact_day;
'@
$newCompactClick = @'
                    } else if (613..=656).contains(&y) {
                        app.settings.tray_english_digits = !app.settings.tray_english_digits;
                        app.settings.save();
                        refresh_tray_visual = true;
                    } else if (657..=700).contains(&y) {
                        app.settings.compact_day = !app.settings.compact_day;
'@
Replace-Exact $oldCompactClick $newCompactClick 1 'English digits click handler'
Replace-Exact '(657..=704).contains(&y)' '(701..=748).contains(&y)' 1 'autostart click range'
Replace-Exact '(706..=754).contains(&y)' '(750..=798).contains(&y)' 1 'install click range'
Replace-Exact '(755..=810).contains(&y)' '(799..=854).contains(&y)' 1 'reset click range'

Replace-Exact 'unsafe fn create_tray_day_icon(day: u32) -> HICON {' 'unsafe fn create_tray_day_icon(day: u32, english_digits: bool) -> HICON {' 1 'tray icon signature'

$oldTrayText = @'
        let font = create_font(-24, FW_BOLD as i32, "Vazirmatn");
        draw_text(
            color_dc,
            &persian_digits(day),
            RECT {
                left: 0,
                top: 2,
                right: 32,
                bottom: 32,
            },
            rgb(18, 18, 18),
            font,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
'@
$newTrayText = @'
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
'@
Replace-Exact $oldTrayText $newTrayText 1 'tray icon digits rendering'
Replace-Exact 'let icon = create_tray_day_icon(today.day);' 'let icon = create_tray_day_icon(today.day, app.settings.tray_english_digits);' 1 'tray icon call'

[System.IO.File]::WriteAllText((Join-Path $PWD $path), $script:text, [System.Text.UTF8Encoding]::new($false))

Remove-Item -LiteralPath '.github/workflows/apply-tray-english-digits.yml' -Force
Remove-Item -LiteralPath $PSCommandPath -Force

git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git add -A
git diff --cached --check
git commit -m 'Add English digits option for system tray'
git push
