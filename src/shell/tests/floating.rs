use super::*;
use gpui::{Bounds, Pixels};

fn bounds_of(cx: &mut gpui::VisualTestContext, name: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} absent"))
}

/// The palette lists notes with their folder and age on 38 px rows that stay inside the floating panel, and
/// ends on a line of keys; a context menu has 30 px items. Names that are very long, CJK, emoji or in a deep
/// folder; no result, a tag filter and many; 420, 860 and 1,600 px at zoom 11, 16 and 32.
#[gpui::test]
fn the_palette_and_the_menu_keep_their_rows(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "floating");
    let deep = root.join("Un dossier au nom assez long pour ne pas tenir").join("et un sous-dossier");
    fs::create_dir_all(&deep).unwrap();
    let long = "x".repeat(120);
    for (dir, name) in [(&root, "Liste".to_string()), (&root, long.clone()), (&root, "列表 日本語 のノート".into()), (&root, "Liste 🎉 émoji".into()), (&deep, "Liste profonde".into())] {
        fs::write(dir.join(format!("{name}.md")), format!("# {name}\n\n#etiquette\n")).unwrap();
    }
    for i in 0..30 {
        fs::write(root.join(format!("Liste {i}.md")), "# n\n").unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(Duration::from_millis(400));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    };
    for (keys, zoom) in [("secondary-- ".repeat(5), 11.), ("secondary-0 ".into(), 16.), ("secondary-= ".repeat(16), 32.)] {
        cx.simulate_keystrokes("secondary-0");
        cx.simulate_keystrokes(keys.trim_end());
        assert_eq!(shell.read_with(cx, |s, _| s.theme.size), zoom);
        let t = shell.read_with(cx, |s, _| s.theme);
        for width in [420., 860., 1600.] {
            cx.simulate_resize(size(px(width), px(720.)));
            for query in ["list", "#etiquette", ">qqqqzzzz", "xxxxxxxx"] {
                cx.simulate_keystrokes("secondary-p");
                cx.simulate_input(query);
                settle(cx);
                let panel = bounds_of(cx, "palette-panel");
                assert!(panel.left() >= px(0.) && panel.right() <= px(width) + px(0.5), "{query}: {panel:?} in {width}");
                // The rows of the list, folder before age, none over its neighbour.
                // Bounds of rows from an earlier, longer list stay behind: only the listed ones are live.
                let listed = shell.read_with(cx, |s, cx| s.palette.as_ref().map_or(0, |p| p.read(cx).listed())).min(14);
                let mut lowest = px(0.);
                for i in 0..listed {
                    let row = bounds_of(cx, leak(format!("palette-item-{i}")));
                    lowest = lowest.max(row.bottom());
                    assert!((row.size.height - t.px(38.)).abs() < px(0.5), "{query} row {i}: {row:?}");
                    assert!(row.left() >= panel.left() && row.right() <= panel.right() + px(0.5), "{query} row {i}: {row:?} in {panel:?}");
                    if let Some(folder) = cx.debug_bounds(leak(format!("palette-folder-{i}"))) {
                        assert!(folder.left() >= row.left() && folder.right() <= row.right() + px(0.5), "{query} folder {i}: {folder:?} in {row:?}");
                        if let Some(age) = cx.debug_bounds(leak(format!("palette-age-{i}"))) {
                            assert!(folder.right() <= age.left() + px(0.5), "{query} {i}: {folder:?} over {age:?}");
                            assert!(age.right() <= row.right() + px(0.5), "{query} age {i}: {age:?} in {row:?}");
                        }
                    }
                }
                // The line of keys is under the rows, inside the panel; an empty list has none.
                match (listed > 0).then(|| bounds_of(cx, "palette-footer")) {
                    Some(foot) => {
                        assert!(foot.left() >= panel.left() && foot.right() <= panel.right() + px(0.5) && foot.bottom() <= panel.bottom() + px(0.5), "{query}: {foot:?} in {panel:?}");
                        assert!(foot.top() >= lowest - px(0.5), "{query}: {foot:?} over the rows (down to {lowest:?})");
                    }
                    None => {}
                }
                cx.simulate_keystrokes("escape");
                settle(cx);
            }
            // A context menu on a note: 30 px items, the whole menu in the window.
            let note = root.join("Liste.md");
            shell.update(cx, |s, cx| {
                s.menu = Some(nav::Menu { at: point(px(width - 20.), px(500.)), target: Some(note.clone()), text: None, commented: false, block: false, ..Default::default() });
                cx.notify();
            });
            settle(cx);
            let menu = bounds_of(cx, "menu");
            // At zoom 32 in 420 px the menu is wider and taller than the window (as before this step): it
            // can only be anchored at the margin then.
            if menu.size.width <= px(width) {
                assert!(menu.left() >= px(0.) && menu.right() <= px(width) + px(0.5), "{menu:?} in {width}");
            }
            if menu.size.height <= px(720.) {
                assert!(menu.top() >= px(0.) && menu.bottom() <= px(720.) + px(0.5), "{menu:?} in {width}");
            }
            let entry = bounds_of(cx, leak(format!("menu-entry-{}", tr("Rename", "Renommer"))));
            assert!((entry.size.height - t.px(30.)).abs() < px(0.5), "{entry:?}");
            shell.update(cx, |s, cx| {
                s.menu = None;
                cx.notify();
            });
            settle(cx);
        }
    }
    let _ = fs::remove_dir_all(&root);
}
