//! What the mouse does, the keys do too: context menus, the kanban board, the icon grid, buttons.

use super::*;

/// Shift+F10 and the Menu key open the menu of the selected row or of the cursor; arrows, Enter and
/// Esc walk it, and the focus goes back where it was.
#[gpui::test]
fn context_menus_walk_with_the_keys(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "menus");
    fs::write(root.join("Alpha.md"), "# Alpha\n").unwrap();
    fs::write(root.join("Beta.md"), "# Beta\n\nmot").unwrap();
    fs::create_dir(root.join("Dossier")).unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Beta.md"), cx);
    });
    cx.run_until_parked();
    let menu = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.menu.as_ref().map(|m| (m.chosen, s.menu_groups(cx).concat().get(m.chosen.unwrap_or(0)).map(|e| e.0))));
    let focus_in_menu = |cx: &mut gpui::VisualTestContext| cx.update(|window, cx| shell.read(cx).menu_focus.is_focused(window));

    // The panel: the selected row's menu opens under it, on its first entry.
    cx.simulate_keystrokes("secondary-e");
    shell.update(cx, |s, cx| {
        s.nav.sel = Some(root.join("Alpha.md"));
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("shift-f10");
    assert_eq!(menu(cx), Some((Some(0), Some("Ouvrir"))));
    assert!(focus_in_menu(cx));
    let (at, row) = shell.read_with(cx, |s, _| (s.menu.as_ref().unwrap().at, s.nav.sel_at.get().unwrap()));
    assert!(at.y >= row.bottom() && at.y < row.bottom() + px(2.) && at.x > row.left(), "under the row: {at:?} {row:?}");
    // Up from the first goes round to the last; Esc closes and the tree has the keys again.
    cx.simulate_keystrokes("up");
    assert_eq!(menu(cx), Some((Some(13), Some("Mettre à la corbeille"))));
    cx.simulate_keystrokes("home down");
    assert_eq!(menu(cx), Some((Some(1), Some("Ouvrir à côté"))));
    cx.simulate_keystrokes("escape");
    assert!(menu(cx).is_none());
    assert!(cx.update(|window, cx| shell.read(cx).nav.focus.is_focused(window)));
    // "Move to…" lists the folders, the vault first; the chosen one receives the note, as a drag would.
    cx.simulate_keystrokes("menu up up up");
    assert_eq!(menu(cx), Some((Some(11), Some("Déplacer vers…"))));
    cx.simulate_keystrokes("enter");
    cx.simulate_input("doss");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(root.join("Dossier/Alpha.md").is_file() && !root.join("Alpha.md").exists());
    assert!(cx.update(|window, cx| shell.read(cx).nav.focus.is_focused(window)), "the keys are back in the tree");
    // A folder is not offered to itself.
    shell.update(cx, |s, cx| {
        s.nav.sel = Some(root.join("Dossier"));
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("menu");
    let entries = shell.read_with(cx, |s, cx| s.menu_groups(cx).concat().iter().map(|e| e.0).collect::<Vec<_>>());
    let at = entries.iter().position(|e| *e == "Déplacer vers…").unwrap();
    cx.simulate_keystrokes("down ".repeat(at).trim_end());
    cx.simulate_keystrokes("enter");
    cx.simulate_input("doss");
    assert_eq!(shell.read_with(cx, |s, cx| s.palette.as_ref().map(|p| p.read(cx).listed())), Some(0), "Dossier is not a choice for itself");
    cx.simulate_keystrokes("escape");
    fs::rename(root.join("Dossier/Alpha.md"), root.join("Alpha.md")).unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.nav.sel = Some(root.join("Alpha.md"));
        cx.notify();
    });
    cx.run_until_parked();
    // The Menu key, then Enter on "Open to the side": a second pane with Alpha.
    cx.simulate_keystrokes("menu down enter");
    cx.run_until_parked();
    assert!(menu(cx).is_none());
    assert_eq!(shell.read_with(cx, |s, _| s.panes.iter().map(|p| p.path.clone()).collect::<Vec<_>>()), [Some(root.join("Beta.md")), Some(root.join("Alpha.md"))]);

    // The note: the menu of the cursor; Bold on the selected word.
    cx.simulate_keystrokes("secondary-1 secondary-end shift-home");
    cx.simulate_keystrokes("shift-f10");
    assert_eq!(menu(cx), Some((Some(0), Some("Couper"))));
    cx.simulate_keystrokes("down down down");
    assert_eq!(menu(cx), Some((Some(3), Some("Gras"))));
    cx.simulate_keystrokes("enter");
    assert!(menu(cx).is_none());
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string()), "# Beta\n\n**mot**");
    // Esc in the note's menu gives the keys back to the note, its selection kept: typing replaces it.
    cx.simulate_keystrokes("menu escape");
    cx.simulate_input("!");
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string()), "# Beta\n\n**!**");

    // The command palette has a second key, the usual one; the row menu's entries are there, for the open note.
    cx.simulate_keystrokes("secondary-shift-p");
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    cx.simulate_input("chemin relatif");
    cx.simulate_keystrokes("enter");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("Beta.md"));
    cx.simulate_keystrokes("secondary-shift-p");
    cx.simulate_input("lien de la note");
    cx.simulate_keystrokes("enter");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("[[Beta]]"));
    cx.simulate_keystrokes("secondary-shift-p");
    cx.simulate_input("icône de la note");
    cx.simulate_keystrokes("enter");
    assert_eq!(shell.read_with(cx, |s, _| s.icon_pick.as_ref().map(|p| (p.path.clone(), p.chosen))), Some((root.join("Beta.md"), 0)));
    // The grid: arrows (eight a row, the last row stops short), Enter gives the icon; it opens again on it.
    let icon = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.icons.get(&root.join("Beta.md")).cloned());
    let last = crate::ui::assets::ICON_SET.len();
    cx.simulate_keystrokes("end down");
    assert_eq!(shell.read_with(cx, |s, _| s.icon_pick.as_ref().unwrap().chosen), last);
    cx.simulate_keystrokes("right down left enter");
    let name = |i: usize| crate::ui::assets::ICON_SET[i - 1].0[2..].trim_end_matches(".svg").to_string();
    assert_eq!(icon(cx), Some(name(7)));
    assert!(shell.read_with(cx, |s, _| s.icon_pick.is_none()));
    cx.simulate_keystrokes("secondary-shift-p");
    cx.simulate_input("icône de la note");
    cx.simulate_keystrokes("enter");
    assert_eq!(shell.read_with(cx, |s, _| s.icon_pick.as_ref().unwrap().chosen), 7);
    cx.simulate_keystrokes("home space");
    assert_eq!(icon(cx), None);
    // Esc closes it and the note has the keys again.
    cx.simulate_keystrokes("secondary-shift-p");
    cx.simulate_input("icône de la note");
    cx.simulate_keystrokes("enter escape");
    assert!(shell.read_with(cx, |s, _| s.icon_pick.is_none()));
    assert!(cx.update(|window, cx| shell.read(cx).pane().editor.focus_handle(cx).is_focused(window)));
    cx.simulate_keystrokes("secondary-shift-p");
    cx.simulate_input("afficher le dossier");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().listing.is_some()));
}

/// The kanban board without the mouse: arrows go, Alt+arrows carry, Space ticks, Enter writes, Delete
/// removes, the board's undo brings it back; twelve columns scroll to the spot, and to a held card's edge.
#[gpui::test]
fn the_board_from_the_keys(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "board-keys");
    let mut text = "---\nkanban: true\n---\n# Projet\n\n## C0\n- [ ] A\n- [ ] B\n".to_string();
    for c in 1..12 {
        text.push_str(&format!("\n## C{c}\n"));
    }
    text.push_str("- [ ] Z\n");
    fs::write(root.join("Projet.md"), &text).unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Projet.md"), cx);
    });
    cx.run_until_parked();
    let note = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    let at = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.pane().board.at);
    let cards = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| kanban::columns(s.pane().editor.read(cx).text()).iter().map(|c| c.cards.iter().map(|k| format!("{}{}", k.text, if k.done { "✓" } else { "" })).collect::<Vec<_>>()).collect::<Vec<_>>());
    assert!(cx.update(|window, cx| shell.read(cx).pane().board.focus.is_focused(window)));

    // The first arrow lands on the first card; Alt+Down carries it under B, Alt+Right into C1.
    cx.simulate_keystrokes("down");
    assert_eq!(at(cx), Some((0, 1)));
    cx.simulate_keystrokes("alt-down");
    assert_eq!((cards(cx)[0].clone(), at(cx)), (vec!["B".to_string(), "A".to_string()], Some((0, 2))));
    cx.simulate_keystrokes("alt-right");
    assert_eq!((cards(cx)[1].clone(), at(cx)), (vec!["A".to_string()], Some((1, 1))));
    // Space ticks it; Enter writes it in place, Enter again confirms.
    cx.simulate_keystrokes("space");
    assert_eq!(cards(cx)[1], ["A✓"]);
    cx.simulate_keystrokes("enter");
    cx.simulate_input("!");
    cx.simulate_keystrokes("enter");
    assert_eq!(cards(cx)[1], ["A!✓"]);
    // A space typed in a card being written is a space, not a tick.
    cx.simulate_keystrokes("enter");
    cx.simulate_input(" b");
    cx.simulate_keystrokes("enter");
    assert_eq!(cards(cx)[1], ["A! b✓"]);
    // Delete removes the card; the keys stay in the column, on "Add a card"; Enter there opens a new card.
    cx.simulate_keystrokes("delete");
    assert_eq!((cards(cx)[1].clone(), at(cx)), (Vec::<String>::new(), Some((1, 1))));
    cx.simulate_keystrokes("enter");
    cx.simulate_input("N");
    cx.simulate_keystrokes("enter escape");
    assert_eq!(cards(cx)[1], ["N"]);
    // On a title, Delete removes the column with its cards; undo brings it back, whole.
    let before = note(cx);
    cx.simulate_keystrokes("up up delete");
    assert_eq!(cards(cx).len(), 11);
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(note(cx), before);

    // Twelve columns: the last one is out of an 860 px window; the arrows bring it in.
    cx.simulate_resize(gpui::size(px(860.), px(720.)));
    cx.run_until_parked();
    let width = cx.update(|window, _| window.viewport_size().width);
    assert!(cx.debug_bounds("column-title-11").is_none_or(|b| b.right() > width), "the last column starts out of view");
    cx.simulate_keystrokes("right right right right right right right right right right");
    cx.run_until_parked();
    assert_eq!(at(cx).map(|a| a.0), Some(11));
    let last = cx.debug_bounds("column-title-11").expect("drawn");
    assert!(last.left() >= px(0.) && last.right() <= width, "{last:?} in a window {width:?} wide");
    // One more: "Add a column", then nothing further; Enter there opens the field of a new column.
    cx.simulate_keystrokes("right right enter");
    assert_eq!(shell.read_with(cx, |s, _| s.pane().board.field.as_ref().map(|f| f.0)), Some(kanban::Slot::NewColumn));
    cx.simulate_keystrokes("escape");

    // Back to the first column, a card held near the right edge scrolls the board toward it.
    cx.simulate_keystrokes("left left left left left left left left left left left left");
    cx.run_until_parked();
    let offset = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.pane().board.scroll.offset().x);
    let start = offset(cx);
    let card = cx.debug_bounds("card-0-0").unwrap();
    let (left, none) = (gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_down(card.center(), left, none);
    cx.simulate_mouse_move(card.center() + point(px(8.), px(0.)), left, none);
    for x in [10., 6., 2.] {
        cx.simulate_mouse_move(point(width - px(x), card.center().y), left, none);
    }
    assert!(offset(cx) < start, "{:?} then {:?}", start, offset(cx));
    cx.simulate_mouse_up(point(width - px(2.), card.center().y), left, none);
}

/// The icon buttons: Tab goes from one to the next, Enter or Space presses it; a click presses it
/// without taking the keys from the note.
#[gpui::test]
fn buttons_from_the_keys(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "buttons");
    fs::write(root.join("Note.md"), "# Note\n").unwrap();
    fs::create_dir(root.join("Dossier")).unwrap();
    fs::write(root.join("Dossier/Autre.md"), "# Autre\n").unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Note.md"), cx);
    });
    cx.run_until_parked();
    // gpui presses a button when the key comes up; the test platform only sends it down.
    let press = |cx: &mut gpui::VisualTestContext, key: &str| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        cx.simulate_keystrokes(key);
        cx.simulate_event(gpui::KeyUpEvent { keystroke: gpui::Keystroke::parse(key).unwrap() });
        cx.run_until_parked();
    };
    let on_button = |cx: &mut gpui::VisualTestContext| cx.update(|w, cx| {
        let s = shell.read(cx);
        w.focused(cx).is_some() && !s.nav.focus.is_focused(w) && !s.pane().editor.focus_handle(cx).is_focused(w)
    });
    let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.nav.open.len());
    // From the tree, Tab goes along the panel's buttons as they are drawn: new note, new folder, fold all.
    cx.simulate_keystrokes("secondary-e tab");
    assert!(on_button(cx), "a button has the keys");
    cx.simulate_keystrokes("tab tab");
    let unfolded = open(cx);
    // Enter presses it (and does not open the tree's selected note); the keys stay on it, Space presses it again.
    press(cx, "enter");
    let folded = open(cx);
    assert_ne!(folded, unfolded);
    assert!(on_button(cx));
    press(cx, "space");
    assert_ne!(open(cx), folded);
    // Shift+Tab goes back to "New folder", which asks for its name.
    cx.simulate_keystrokes("shift-tab");
    press(cx, "enter");
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    cx.simulate_keystrokes("escape");
    // A click on a button presses it and leaves the keys to the note.
    cx.update(|w, cx| w.focus(&shell.read(cx).pane().editor.focus_handle(cx)));
    let (fold, before) = (cx.debug_bounds("nav-fold").expect("drawn"), open(cx));
    cx.simulate_click(fold.center(), gpui::Modifiers::none());
    assert_ne!(open(cx), before, "the click pressed it");
    assert!(cx.update(|w, cx| shell.read(cx).pane().editor.focus_handle(cx).is_focused(w)), "the click left the keys in the note");
}
