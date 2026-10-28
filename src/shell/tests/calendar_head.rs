use super::*;

/// The calendar in the panel keeps its serif month title above the grid and every day inside the sheet:
/// months that start on a Monday / a Sunday, a leap February, a 6-week month, today on the first and
/// the last cell; at 420, 860 and 1,600 px and at zoom 11, 16 and 32.
#[gpui::test]
fn the_calendar_stays_inside_its_sheet(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-calendar-head-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let note = root.join("Note.md");
    fs::write(&note, "# Note\n").unwrap();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = Shell::new(Some(root.clone()), vec![note.clone()], window, cx);
        shell.client = true;
        shell
    });
    cx.run_until_parked();
    shell.update(cx, |s, _| {
        s.nav.show_alone(Mode::Calendar);
        s.nav.panel = Panel::Split;
    });
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..4 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    // (month, first day, last day): Sep 2025 starts on a Monday, Feb 2026 on a Sunday (28 days, 4 rows
    // in a Monday week), Feb 2024 has 29 days, Mar 2025 spans six weeks from a Saturday.
    for (day, first, last) in [((2025, 9, 15), 1, 30), ((2026, 2, 10), 1, 28), ((2024, 2, 29), 1, 29), ((2025, 3, 31), 1, 31)] {
        shell.update(cx, |s, _| s.nav.day = day);
        for (keys, zoom) in [("secondary-- ".repeat(5), 11.), ("secondary-0 ".into(), 16.), ("secondary-= ".repeat(16), 32.)] {
            cx.simulate_keystrokes("secondary-0");
            cx.simulate_keystrokes(keys.trim_end());
            assert_eq!(shell.read_with(cx, |s, _| s.theme.size), zoom);
            for width in [420., 860., 1600.] {
                cx.simulate_resize(size(px(width), px(700.)));
                settle(cx);
                let what = format!("{day:?} at {width} px, zoom {zoom}");
                let (sheet, title, grid) = (cx.debug_bounds("nav-panel").unwrap(), cx.debug_bounds("month-title").unwrap(), cx.debug_bounds("calendar").unwrap());
                assert!(title.left() >= sheet.left() && title.right() <= sheet.right() + px(0.5), "{what}: title {title:?} leaves the sheet {sheet:?}");
                assert!(title.bottom() <= grid.top(), "{what}: title {title:?} overlaps the grid {grid:?}");
                for d in [first, last] {
                    let cell = cx.debug_bounds(Box::leak(format!("day-{}-{}-{d}", day.0, day.1).into_boxed_str())).unwrap();
                    assert!(cell.left() >= sheet.left() && cell.right() <= sheet.right() + px(0.5), "{what}: day {d} {cell:?} leaves the sheet");
                    assert!(cell.top() >= grid.top() && cell.bottom() <= grid.bottom() + px(0.5), "{what}: day {d} {cell:?} leaves the grid {grid:?}");
                }
            }
        }
    }
    fs::remove_dir_all(&root).ok();
}
