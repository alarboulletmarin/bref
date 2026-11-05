use super::*;
use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent};

/// Column widths of a CSV: the edge of a header dragged (narrowest, widest, Escape, release), double-clicked
/// to fit, widened and narrowed by the keyboard, reset; widths that follow moved and deleted columns and
/// their undo, kept for the file when it is reopened and renamed, never a change of the file.
#[gpui::test]
fn the_columns_take_the_width_the_user_gives_them(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "sheet-widths");
    let long = "x".repeat(80);
    let csv = root.join("codes.csv");
    let original = format!("code,description,n\nA1,{long},3\nB2,é 漢字 🙂,4\n");
    fs::write(&csv, &original).unwrap();
    let open = |cx: &mut gpui::VisualTestContext, path: &Path| {
        shell.update(cx, |s, cx| {
            let (notes, dirs, images) = vault::scan(&root);
            s.sync(notes, dirs, images, cx);
            s.open_note(path, cx);
        });
        cx.simulate_resize(size(px(1200.), px(700.)));
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().expect("the table is shown"));
        shell.update_in(cx, |s, window, cx| window.focus(&s.shown_sheet().unwrap().focus_handle(cx)));
        sheet
    };
    let sheet = open(cx, &csv);
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    };
    let col = |cx: &mut gpui::VisualTestContext, c: usize| cx.debug_bounds(leak(format!("sheet-col-{c}"))).unwrap_or_else(|| panic!("no column {c}"));
    let widths = |cx: &mut gpui::VisualTestContext| sheet.read_with(cx, |s, _| s.col_widths());
    let px_of = |cx: &mut gpui::VisualTestContext, chars: f32| sheet.read_with(cx, |s, _| s.px_of(chars));
    let tables = || fs::read_to_string(root.join(".config/bref/tables")).unwrap_or_default();
    let (auto, _) = widths(cx);
    assert!(auto[1] <= px_of(cx, 40.) + 1., "the long column is cut at 40 characters: {auto:?}");

    // Drag the right edge of the first column 60 px to the right: it grows by 60, the next one moves by 60.
    let (a, b) = (col(cx, 0), col(cx, 1));
    let edge = point(a.right(), a.center().y);
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge + point(px(60.), px(0.)), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(cx.debug_bounds("sheet-width").is_some(), "the width shows in a pill while dragging");
    cx.simulate_mouse_up(edge + point(px(60.), px(0.)), MouseButton::Left, Modifiers::none());
    // The pill goes with the drag (GPUI keeps old debug bounds: the drag state says it).
    assert!(!sheet.read_with(cx, |s, _| s.resizing()), "released");
    draw(cx);
    let (a2, b2) = (col(cx, 0), col(cx, 1));
    let char_px = px_of(cx, 1.) - px_of(cx, 0.);
    assert!((f32::from(a2.size.width - a.size.width) - 60.).abs() <= char_px / 10. + 0.5, "{a:?} → {a2:?}");
    assert!((f32::from(b2.left() - b.left()) - f32::from(a2.size.width - a.size.width)).abs() < 0.5, "the next column follows");
    assert!(a2.right() <= b2.left() + px(0.5) && a2.top() == b2.top(), "no overlap: {a2:?} {b2:?}");
    assert_eq!(b2.size.width, b.size.width);
    let set = widths(cx).1[0].expect("the width is chosen");
    assert!(tables().contains(&format!("codes.csv\t\t\t\t{set}")), "kept without pinning the format: {:?}", tables());
    // Not a change of the file: nothing to save, nothing to undo.
    assert!(!sheet.read_with(cx, |s, _| s.is_dirty()));
    assert_eq!(sheet.read_with(cx, |s, _| s.undo_len()), 0);

    // Escape mid-drag puts the width back; a drag back to the start keeps it; 1 px moves at most a tenth.
    let edge = point(col(cx, 0).right(), a.center().y);
    let before = widths(cx);
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge + point(px(200.), px(0.)), MouseButton::Left, Modifiers::none());
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(edge + point(px(200.), px(0.)), MouseButton::Left, Modifiers::none());
    assert_eq!(widths(cx), before, "Escape puts the width back");
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge + point(px(90.), px(0.)), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(edge, MouseButton::Left, Modifiers::none());
    assert_eq!(widths(cx), before, "back to the start, the same width");
    // Double-click on the edge of the long column: it fits its 80 characters, beyond the cap of 40.
    let b = col(cx, 1);
    let edge = point(b.right() - px(1.), b.center().y);
    for count in [1, 2] {
        cx.simulate_event(MouseDownEvent { position: edge, button: MouseButton::Left, modifiers: Modifiers::none(), click_count: count, first_mouse: false });
        cx.simulate_event(MouseUpEvent { position: edge, button: MouseButton::Left, modifiers: Modifiers::none(), click_count: count });
    }
    let fitted = widths(cx).0[1];
    assert!((fitted - px_of(cx, 80. * 0.55 / 0.6)).abs() < 1., "fits its text: {fitted}");
    assert_eq!(sheet.read_with(cx, |s, _| s.table().cell(0, 1).into_owned()), "description", "the header was not edited");
    // A double-click in a header cell, away from the edge, still edits it.
    draw(cx);
    let c = col(cx, 2).center();
    cx.simulate_event(MouseDownEvent { position: c, button: MouseButton::Left, modifiers: Modifiers::none(), click_count: 1, first_mouse: false });
    cx.simulate_event(MouseUpEvent { position: c, button: MouseButton::Left, modifiers: Modifiers::none(), click_count: 1 });
    cx.simulate_event(MouseDownEvent { position: c, button: MouseButton::Left, modifiers: Modifiers::none(), click_count: 2, first_mouse: false });
    cx.simulate_input("count");
    cx.simulate_keystrokes("escape");
    assert_eq!(widths(cx).1[2], None, "a click in the cell sets no width");

    // To the narrowest and past it, then the widest and past it.
    draw(cx);
    let edge = point(col(cx, 0).right(), a.center().y);
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(edge - point(px(5000.), px(0.)), MouseButton::Left, Modifiers::none());
    assert_eq!(widths(cx).0[0], px_of(cx, 3.), "never narrower than three characters");
    cx.simulate_mouse_move(edge + point(px(9000.), px(0.)), MouseButton::Left, Modifiers::none());
    // Released outside the window: the drag ends where it is.
    cx.simulate_event(MouseUpEvent { position: point(px(5000.), px(-50.)), button: MouseButton::Left, modifiers: Modifiers::none(), click_count: 1 });
    let widest = widths(cx).0[0];
    assert!(widest <= 4000. && widest > 3900., "the widest is bounded: {widest}");
    // The horizontal scroll reaches the new right edge.
    sheet.update(cx, |s, _| s.scroll_by(100_000., 0.));
    let (scroll, _) = sheet.read_with(cx, |s, _| s.view_state());
    assert!(scroll.0 > 3000., "the scroll range follows the widths: {scroll:?}");
    sheet.update(cx, |s, _| s.scroll_by(-100_000., 0.));
    draw(cx);

    // Keyboard: one character wider, then narrower; a hundred times narrower stops at the narrowest.
    cx.simulate_keystrokes("secondary-home");
    let start = widths(cx).0[0];
    cx.simulate_keystrokes("alt-,");
    assert!((start - widths(cx).0[0] - char_px).abs() < char_px / 10. + 0.01);
    cx.simulate_keystrokes("alt-.");
    assert!((widths(cx).0[0] - start).abs() < char_px / 10. + 0.01);
    cx.simulate_keystrokes(&["alt-,"; 10].join(" "));
    assert!((start - widths(cx).0[0] - 10. * char_px).abs() < 1.);
    cx.simulate_keystrokes(&["alt-."; 10].join(" "));
    assert!((widths(cx).0[0] - start).abs() < 1.);
    cx.simulate_keystrokes("right right");
    cx.simulate_keystrokes(&["alt-,"; 100].join(" "));
    assert_eq!(widths(cx).0[2], px_of(cx, 3.), "a hundred times narrower: the narrowest");
    // Two columns selected: both a character wider (the first, at the widest, does not move).
    cx.simulate_keystrokes("secondary-home right shift-right");
    let before = widths(cx).0;
    cx.simulate_keystrokes("alt-.");
    let after = widths(cx).0;
    assert!((1..3).all(|c| (after[c] - before[c] - char_px).abs() < char_px / 10. + 0.01) && after[0] == before[0], "{before:?} → {after:?}");

    // Columns moved and deleted: the widths go with them, and come back with an undo.
    let set = widths(cx).1;
    cx.simulate_keystrokes("secondary-home");
    sheet.update(cx, |s, cx| s.table_op(editor::TableOp::MoveColRight, cx));
    assert_eq!(widths(cx).1, vec![set[1], set[0], set[2]]);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(widths(cx).1, set);
    cx.simulate_keystrokes("secondary-home");
    sheet.update(cx, |s, cx| s.table_op(editor::TableOp::DeleteCol, cx));
    assert_eq!(widths(cx).1, vec![set[1], set[2]]);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(widths(cx).1, set, "undoing a deletion brings the width back");
    cx.simulate_keystrokes("secondary-home");
    sheet.update(cx, |s, cx| s.table_op(editor::TableOp::ColLeft, cx));
    assert_eq!(widths(cx).1, vec![None, set[0], set[1], set[2]], "a new column is automatic");
    cx.simulate_keystrokes("secondary-z");
    // A sort changes no width; an edit does not widen a chosen column.
    sheet.update(cx, |s, cx| s.table_op(editor::TableOp::SortDesc, cx));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(widths(cx).1, set);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&csv).unwrap(), original, "the file has its bytes back");

    // Zoom: the widths keep their characters.
    let chars = widths(cx).1;
    cx.simulate_keystrokes("secondary-= secondary-= secondary-=");
    draw(cx);
    assert_eq!(widths(cx).1, chars);
    assert_eq!(widths(cx).0[1], px_of(cx, chars[1].unwrap()));
    cx.simulate_keystrokes("secondary-0");

    // Closed and opened again: the same widths. Renamed in the app: still them.
    let note = root.join("Note.md");
    fs::write(&note, "# Note\n").unwrap();
    shell.update(cx, |s, cx| s.open_note(&note, cx));
    cx.run_until_parked();
    let sheet = open(cx, &csv);
    assert_eq!(sheet.read_with(cx, |s, _| s.col_widths().1), chars);
    let renamed = root.join("renamed.csv");
    vault::rename(&csv, &renamed).unwrap();
    shell.update(cx, |s, cx| s.relocate(&csv, &renamed, false, cx));
    shell.update(cx, |s, cx| s.open_note(&note, cx));
    cx.run_until_parked();
    let sheet = open(cx, &renamed);
    assert_eq!(sheet.read_with(cx, |s, _| s.col_widths().1), chars, "the widths follow a rename");
    assert!(!tables().contains("codes.csv"));
    // To the trash and put back: the widths come back with the file.
    let trashed = shell.update(cx, |s, cx| {
        s.trash(&root, &renamed, cx);
        vault::trashed(&root)[0].clone()
    });
    assert!(tables().contains(".trash"), "{:?}", tables());
    shell.update(cx, |s, cx| s.put_back(&trashed, &renamed, cx));
    let sheet = open(cx, &renamed);
    assert_eq!(sheet.read_with(cx, |s, _| s.col_widths().1), chars, "the widths come back from the trash");
    // Edited elsewhere with one column less, then one more: the widths that still have a column apply.
    fs::write(&renamed, "code,description\nA1,x\n").unwrap();
    sheet.update(cx, |s, cx| s.reload_if_changed(cx));
    assert_eq!(sheet.read_with(cx, |s, _| s.col_widths().1), chars[..2]);
    fs::write(&renamed, "code,description,n,more\nA1,x,1,2\n").unwrap();
    sheet.update(cx, |s, cx| s.reload_if_changed(cx));
    assert_eq!(sheet.read_with(cx, |s, _| s.col_widths().1), vec![chars[0], chars[1], None, None]);

    // Reset from the palette: automatic widths again, nothing kept.
    cx.simulate_keystrokes("f10");
    cx.simulate_input("remettre les largeurs");
    cx.simulate_keystrokes("enter");
    let (now, chosen) = sheet.read_with(cx, |s, _| s.col_widths());
    assert_eq!(chosen, vec![None; 4]);
    assert_eq!(now[0], auto[0], "automatic again");
    assert!(!tables().contains("renamed.csv"), "{:?}", tables());
    fs::remove_dir_all(&root).ok();
}
