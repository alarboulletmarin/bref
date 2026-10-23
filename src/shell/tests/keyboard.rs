//! What the mouse does, the keys do too: context menus, the kanban board, the icon grid, buttons.

use super::*;

/// Shift+F10 and the Menu key open the menu of the selected row or of the cursor; arrows, Enter and
/// Esc walk it, and the focus goes back where it was.
#[gpui::test]
fn context_menus_walk_with_the_keys(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "menus");
    fs::write(root.join("Alpha.md"), "# Alpha\n").unwrap();
    fs::write(root.join("Beta.md"), "# Beta\n\nmot").unwrap();
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
    assert_eq!(menu(cx), Some((Some(12), Some("Mettre à la corbeille"))));
    cx.simulate_keystrokes("home down");
    assert_eq!(menu(cx), Some((Some(1), Some("Ouvrir à côté"))));
    cx.simulate_keystrokes("escape");
    assert!(menu(cx).is_none());
    assert!(cx.update(|window, cx| shell.read(cx).nav.focus.is_focused(window)));
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
    assert_eq!(shell.read_with(cx, |s, _| s.icon_pick.as_ref().map(|p| p.1.clone())), Some(root.join("Beta.md")));
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("secondary-shift-p");
    cx.simulate_input("dossier de la note");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.pane().listing.is_some()));
}
