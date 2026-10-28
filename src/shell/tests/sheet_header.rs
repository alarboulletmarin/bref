use super::*;

/// The table's head (serif name, "N rows · M columns", three pills, copy, export) never overlaps itself and
/// stays above the grid: a long name without a space, a header alone, one cell, a file of 300 columns,
/// at 420, 860 and 1,600 px and at zoom 11, 16 and 32.
#[gpui::test]
fn the_table_head_stays_above_the_grid(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-sheet-header-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let note = root.join("Note.md");
    fs::write(&note, "# Note\n").unwrap();
    let long = root.join(format!("{}.csv", "Contacts".repeat(16)));
    fs::write(&long, "a,b\n1,2\n").unwrap();
    let empty = root.join("vide.csv");
    fs::write(&empty, "a,b,c\n").unwrap();
    let one = root.join("une.csv");
    fs::write(&one, "x\n").unwrap();
    let wide = root.join("large.csv");
    fs::write(&wide, format!("{}\n{}\n", (0..300).map(|i| format!("c{i}")).collect::<Vec<_>>().join(","), (0..300).map(|i| i.to_string()).collect::<Vec<_>>().join(","))).unwrap();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = Shell::new(Some(root.clone()), vec![note.clone()], window, cx);
        shell.client = true;
        shell
    });
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..4 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    let names = ["sheet-table-title", "sheet-table-summary", "sheet-delimiter", "sheet-encoding", "sheet-header", "sheet-copy", "sheet-export"];
    for (keys, zoom) in [("secondary-- ".repeat(5), 11.), ("secondary-0 ".into(), 16.), ("secondary-= ".repeat(16), 32.)] {
        cx.simulate_keystrokes(keys.trim_end());
        assert_eq!(shell.read_with(cx, |s, _| s.theme.size), zoom);
        for file in [&long, &empty, &one, &wide] {
            shell.update_in(cx, |s, window, cx| {
                let (notes, dirs, images) = vault::scan(&root);
                s.sync(notes, dirs, images, cx);
                s.open_note(file, cx);
                // The note's editor keeps the keys otherwise, and takes the table's place at the next frame.
                window.focus(&s.shown_sheet().unwrap().focus_handle(cx));
            });
            assert!(shell.read_with(cx, |s, _| s.shown_sheet().is_some()), "{} is not shown as a table", file.display());
            for width in [420., 860., 1600.] {
                cx.simulate_resize(size(px(width), px(700.)));
                settle(cx);
                let what = format!("{} at {width} px, zoom {zoom}", file.file_name().unwrap().to_string_lossy().chars().take(12).collect::<String>());
                let sheet = cx.debug_bounds("pane-0").unwrap();
                let found: Vec<_> = names.iter().map(|n| (*n, cx.debug_bounds(n).unwrap_or_else(|| panic!("{what}: no {n}")))).collect();
                for (n, b) in &found {
                    assert!(b.left() >= sheet.left() && b.right() <= sheet.right() + px(0.5), "{what}: {n} {b:?} leaves the sheet {sheet:?}");
                }
                for (i, (a, ab)) in found.iter().enumerate() {
                    for (b, bb) in &found[i + 1..] {
                        assert!(!ab.intersects(bb), "{what}: {a} {ab:?} overlaps {b} {bb:?}");
                    }
                }
                let head = cx.debug_bounds("sheet-table-header").unwrap();
                let body = cx.debug_bounds("sheet-body").unwrap();
                assert!(head.bottom() <= body.top() && body.bottom() <= sheet.bottom(), "{what}: head {head:?}, grid {body:?}, sheet {sheet:?}");
                assert!(body.size.height > px(60.), "{what}: grid {body:?} squeezed");
            }
        }
    }
    fs::remove_dir_all(&root).ok();
}
