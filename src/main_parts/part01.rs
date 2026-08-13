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
            &main_date_heading(app),
            sr(RECT { left: 72, top: 7, right: 358, bottom: 43 }),
            palette.accent,
            fonts.medium,
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
        let today = app.today_main();
        let today_is_active = app.year == today.year
            && app.month == today.month
            && app.selected_day == Some(today.day);
        if !today_is_active {
            draw_round_fill(hdc, sr(RECT { left: 174, top: 83, right: 256, bottom: 108 }), palette.accent, scaled(12, scale));
            draw_text(hdc, "برو به امروز", sr(RECT { left: 174, top: 83, right: 256, bottom: 108 }), palette.accent_text, fonts.tiny, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);
        }

        if app.settings.daily_view {
            if app.settings.show_events {
                paint_events(hdc, app, palette, fonts);
            }
            paint_footer(hdc, app, palette, fonts);
            return;
        }

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
            let top = GRID_TOP + GRID_CONTENT_TOP_PADDING + row * CELL_HEIGHT;
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

        paint_day_tooltip(hdc, app, palette, fonts);

        if app.settings.show_events {
            paint_events(hdc, app, palette, fonts);
        }
        paint_footer(hdc, app, palette, fonts);
    }
}

unsafe fn paint_day_tooltip(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts) {
    let Some(cell) = app.hovered_cell else { return; };
    let (primary, _) = adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
    let jalali = convert(primary, app.settings.main_calendar, CalendarKind::Jalali);
    let events = app.events.events_for_day(jalali.year, jalali.month, jalali.day);
    if events.is_empty() { return; }
    let text = events.iter().take(3).map(|event| format!("• {}", event.title)).collect::<Vec<_>>().join("\n");
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
        let rect = scaled_rect(RECT { left, top, right: left + width, bottom: top + height }, scale);
        draw_round_fill(hdc, rect, palette.surface_alt, scaled(10, scale));
        draw_round_outline(hdc, rect, palette.accent, scaled(10, scale), 1);
        draw_text(
            hdc,
            &text,
            scaled_rect(RECT { left: left + 12, top: top + 7, right: left + width - 12, bottom: top + height - 7 }, scale),
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
            && !app.settings.daily_view
            && (GRID_LEFT..GRID_LEFT + GRID_WIDTH).contains(&x)
            && (GRID_TOP..GRID_TOP + CELL_HEIGHT * 6).contains(&y)
        {
            let visual_column = (x - GRID_LEFT) / CELL_WIDTH;
            let column = calendar_column(visual_column, app.settings.calendar_rtl);
            let row = (y - GRID_TOP) / CELL_HEIGHT;
            let cell = row * 7 + column;
            let (primary, _) = adjacent_date(app.settings.main_calendar, app.year, app.month, cell);
            let jalali = convert(primary, app.settings.main_calendar, CalendarKind::Jalali);
            if app.events.events_for_day(jalali.year, jalali.month, jalali.day).is_empty() { None } else { Some(cell) }
        } else {
            None
        };
        let changed = app.hovered_cell != next;
        app.hovered_cell = next;
        changed
    };
    if changed { unsafe { InvalidateRect(hwnd, null(), 0); } }
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
        let events_top = app.events_top();
        let bottom = events_top + BASE_EVENTS_HEIGHT - 8;
        draw_round_fill(hdc, sr(RECT { left: 18, top: events_top, right: 412, bottom }), palette.surface_alt, scaled(13, scale));

        let section_title = if let Some(day) = app.selected_day {
            format!("مناسبت‌های روز {}", persian_digits(day))
        } else {
            "مناسبت‌های این ماه".to_string()
        };
        draw_text(hdc, &section_title, sr(RECT { left: 30, top: events_top + 8, right: 400, bottom: events_top + 34 }), palette.text, fonts.medium, DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING);

        let items = event_items_for_view(app);
        if items.is_empty() {
            let message = if app.events.source_year == convert(Date::new(app.year, app.month, 1), app.settings.main_calendar, CalendarKind::Jalali).year {
                "برای این بازه مناسبتی ثبت نشده است."
            } else {
                "فایل رویداد پیوست فقط اطلاعات سال ۱۴۰۵ را دارد."
            };
            draw_text(hdc, message, sr(RECT { left: 30, top: events_top + 39, right: 398, bottom: bottom - 8 }), palette.muted, fonts.small, DT_RIGHT | DT_VCENTER | DT_WORDBREAK | DT_RTLREADING);
            return;
        }

        let item_count = items.len();
        let max_scroll = item_count.saturating_sub(3);
        let start = app.event_scroll.min(max_scroll);
        let mut y = events_top + 38;
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
            let track_top = events_top + 40;
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
            "عماد قاسمی - emadghasemi.ir",
            scaled_rect(RECT { left: 184, top, right: BASE_WIDTH - 12, bottom: top + BASE_FOOTER_HEIGHT - 3 }, scale),
            palette.accent,
            fonts.tiny,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
        draw_text(
            hdc,
            "|",
            scaled_rect(RECT { left: 172, top, right: 184, bottom: top + BASE_FOOTER_HEIGHT - 3 }, scale),
            palette.muted,
            fonts.tiny,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        draw_text(
            hdc,
            &format!("گاه‌یار نسخه {}", persian_digits(env!("CARGO_PKG_VERSION"))),
            scaled_rect(RECT { left: 10, top, right: 172, bottom: top + BASE_FOOTER_HEIGHT - 3 }, scale),
            palette.accent,
            fonts.tiny,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_RTLREADING,
        );
    }
}

unsafe fn paint_update_banner(hdc: HDC, app: &AppState, palette: &Palette, fonts: &Fonts, footer_top: i32) {
    if app.settings.auto_update { return; }
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
