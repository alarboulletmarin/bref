//! Back and forward, shortcuts shown in menus, two panes.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
        // Back / forward: the history of what was shown, as in a browser.
    let (hist_a, hist_b, hist_csv) = (root.join("Hist A.md"), root.join("Hist B.md"), root.join("hist.csv"));
    let long: String = (0..200).map(|i| format!("ligne {i}\n")).collect();
    fs::write(&hist_a, format!("# Hist A\n\n{long}\n[[Hist B]]\n")).unwrap();
    fs::write(&hist_b, "# Hist B\n").unwrap();
    fs::write(&hist_csv, "a,b\n1,2\n3,4\n5,6\n").unwrap();
    j.settle(cx);
    let (back, forward) = if cfg!(target_os = "macos") { ("cmd-[", "cmd-]") } else { ("alt-left", "alt-right") };
    let seen = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.pane().file_path().cloned(), s.pane().path.clone()));
    let on_note = |path: &Path| (None, Some(path.to_path_buf()));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_a, window, cx));
        // Far into the note: the cursor and scroll will be found again on return.
    cx.simulate_keystrokes("secondary-end up up up");
    let place = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view());
    assert!(place.0 > 1000 && place.1 > 0., "{place:?}");
        // Follow a link, come back, go again.
    shell.update_in(cx, |s, window, cx| s.follow(&Link::Wiki("Hist B".into()), window, cx));
    assert_eq!(seen(cx), on_note(&hist_b));
    assert!(shell.read_with(cx, |s, _| s.history.can_back() && !s.history.can_forward()));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view()), place);
    cx.simulate_input("x");
    assert!(text(cx).contains("x"), "input is in the note found again");
    cx.simulate_keystrokes("backspace");
    assert!(shell.read_with(cx, |s, _| s.history.can_forward()));
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx), on_note(&hist_b));
        // The rail button.
    let at = cx.debug_bounds("nav-back").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(seen(cx), on_note(&hist_a));
        // Opening something else after going back: "forward" has nowhere to go. A table counts.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_csv, window, cx));
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    assert!(!shell.read_with(cx, |s, _| s.history.can_forward()));
    cx.simulate_keystrokes("down right");
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    assert_eq!(shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).whereabouts().0), (1, 1), "the cell is found again");
        // The mouse's back and forward buttons.
    let middle = gpui::point(px(500.), px(400.));
    let navigate = |cx: &mut gpui::VisualTestContext, to| {
        cx.simulate_mouse_down(middle, MouseButton::Navigate(to), gpui::Modifiers::none());
        cx.simulate_mouse_up(middle, MouseButton::Navigate(to), gpui::Modifiers::none());
    };
    navigate(cx, gpui::NavigationDirection::Back);
    assert_eq!(seen(cx), on_note(&hist_a));
        // A preview does not enter the history: "back" returns to where we came from, and
        // "forward" still leads to the table.
    shell.update_in(cx, |s, window, cx| {
        window.focus(&s.nav.focus);
        s.preview_note(&hist_b, window, cx);
    });
    assert_eq!(seen(cx), on_note(&hist_b));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    navigate(cx, gpui::NavigationDirection::Forward);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
        // A moved note is followed.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&hist_b, window, cx));
    let shelf = root.join("Étagère");
    fs::create_dir_all(&shelf).unwrap();
    shell.update(cx, |s, cx| s.move_into(&hist_b, &shelf, cx));
    let moved = shelf.join("Hist B.md");
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx).0, Some(hist_csv.clone()));
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx), on_note(&moved));
        // A file put in the trash leaves the history; a file deleted by another
        // program is skipped.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Trash, Some(hist_csv.clone()), window, cx));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));
    fs::remove_file(&moved).unwrap();
    cx.simulate_keystrokes(forward);
    assert_eq!(seen(cx), on_note(&hist_a));
    assert!(!shell.read_with(cx, |s, _| s.history.can_forward()));
        // From a new note, not yet saved: "back" returns to the last one.
    shell.update_in(cx, |s, window, cx| s.new_note_in(None, window, cx));
    assert_eq!(seen(cx), (None, None));
    cx.simulate_keystrokes(back);
    assert_eq!(seen(cx), on_note(&hist_a));

        // Menus show, next to each action, the shortcut that does the same thing:
        // the one in force, and nothing for an action that has none.
    let keys = |cx: &mut gpui::VisualTestContext, what: nav::Do| cx.update(|_, cx| what.keys(cx));
    assert_eq!(keys(cx, nav::Do::Rename), "F2");
    assert_eq!(keys(cx, nav::Do::Duplicate), format!("{MOD}+D"));
    assert_eq!(keys(cx, nav::Do::Bold), format!("{MOD}+B"));
    assert_eq!(keys(cx, nav::Do::OpenLink), format!("{MOD}+clic"));
    assert_eq!(keys(cx, nav::Do::Icon), "");
    shell.update(cx, |s, cx| s.key_set("nav::Rename", "f6".into(), &[], cx));
    assert_eq!(keys(cx, nav::Do::Rename), "F6");
    shell.update(cx, |s, cx| s.key_set("nav::Rename", String::new(), &[], cx));
    assert_eq!(keys(cx, nav::Do::Rename), "");
    shell.update(cx, |s, cx| s.key_reset(None, cx));
    assert_eq!(keys(cx, nav::Do::Rename), "F2");

        // Two panes side by side.
    let pane = |name: &str| root.join(format!("Pane {name}.md"));
    for name in ["A", "B", "D"] {
        fs::write(pane(name), format!("# Pane {name}\n\n")).unwrap();
    }
    fs::write(pane("C"), "# Pane C\n\n[[Pane A]]\n").unwrap();
    j.settle(cx);
    let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.active, s.panes.iter().map(|pane| pane.path.clone()).collect::<Vec<_>>()));
    let both = |active: usize, left: &str, right: &str| (active, vec![Some(pane(left)), Some(pane(right))]);
    let pane_text = |cx: &mut gpui::VisualTestContext, pane: usize| shell.read_with(cx, |s, cx| s.panes[pane].editor.read(cx).text().to_string());
        // The tabs opened by the chapters before go: the panes start from a single tab.
    shell.update_in(cx, |s, window, cx| {
        while s.panes.len() > 1 {
            s.close_tab(0, window, cx);
        }
    });
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&pane("A"), window, cx));
    assert_eq!(open(cx), (0, vec![Some(pane("A"))]));
        // From the keyboard: the second pane opens on the palette, to choose what goes there.
    cx.simulate_keystrokes("secondary-\\");
    assert_eq!(open(cx), (1, vec![Some(pane("A")), None]));
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    cx.simulate_input("pane b");
    cx.simulate_keystrokes("enter");
    assert_eq!(open(cx), both(1, "A", "B"));
        // Each pane is written and saved; Ctrl+1 and Ctrl+2 switch from one to the other.
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("droite");
    cx.simulate_keystrokes("secondary-1 secondary-end");
    assert_eq!(open(cx).0, 0);
    cx.simulate_input("gauche");
    assert_eq!((pane_text(cx, 0), pane_text(cx, 1)), ("# Pane A\n\ngauche".to_string(), "# Pane B\n\ndroite".to_string()));
    j.settle(cx);
    assert_eq!(fs::read_to_string(pane("A")).unwrap(), "# Pane A\n\ngauche");
    assert_eq!(fs::read_to_string(pane("B")).unwrap(), "# Pane B\n\ndroite");
        // A note already shown does not open twice: input goes to its pane.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&pane("B"), window, cx));
    assert_eq!(open(cx), both(1, "A", "B"));
    cx.simulate_input("!");
    assert_eq!(pane_text(cx, 1), "# Pane B\n\ndroite!");
        // From here, what each pane shows: the pane with input, then the note shown on each side.
    let shown = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, _| (s.group(), s.groups.iter().map(|g| s.panes[g.shown].path.clone()).collect::<Vec<_>>()))
    };
    let sides = |active: usize, names: &[&str]| (active, names.iter().map(|n| Some(pane(n))).collect::<Vec<_>>());
        // Ctrl+Enter in the palette: the chosen note opens in the other pane, in a tab of its own.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("pane c");
    cx.simulate_keystrokes("secondary-enter");
    assert_eq!(shown(cx), sides(0, &["C", "B"]));
    assert_eq!(shell.read_with(cx, |s, _| s.groups[0].tabs.len()), 2);
        // Ctrl+Shift+click on a link: the linked note opens beside.
    fs::write(pane("E"), "# Pane E\n\n").unwrap();
    fs::write(pane("C"), "# Pane C\n\n[[Pane E]]\n").unwrap();
    j.settle(cx);
    let link = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).point_of("# Pane C\n\n[[Pa".len()).unwrap());
    cx.simulate_click(link, gpui::Modifiers { shift: true, ..gpui::Modifiers::secondary_key() });
    assert_eq!(shown(cx), sides(1, &["C", "E"]));
        // « Ouvrir à côté » in the tree menu.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::OpenAside, Some(pane("D")), window, cx));
    assert_eq!(shown(cx), sides(0, &["D", "E"]));
        // Moving a note open in a tab out of sight: its tab follows.
    shell.update(cx, |s, cx| s.move_into(&pane("A"), &shelf, cx));
    assert!(shell.read_with(cx, |s, _| s.pane_showing(&shelf.join("Pane A.md")).is_some() && s.pane_showing(&pane("A")).is_none()));
        // Dragging a note from the tree onto a pane opens it there; the targeted half is tinted.
    shell.update(cx, |s, cx| {
        (s.nav.mode, s.nav.panel) = (nav::Mode::Tree, nav::Panel::Split);
        cx.notify();
    });
    cx.run_until_parked();
    let row = cx.debug_bounds("nav-row-Pane B").unwrap().center();
    let right = cx.debug_bounds("pane-1").unwrap().center();
    cx.simulate_mouse_down(row, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(row + gpui::point(px(12.), px(4.)), Some(MouseButton::Left), gpui::Modifiers::none());
    cx.simulate_mouse_move(right, Some(MouseButton::Left), gpui::Modifiers::none());
    assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), Some((1, false)));
    cx.simulate_mouse_up(right, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(shown(cx), sides(1, &["D", "B"]));
    assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), None);
        // Ctrl+W closes the tab that has input, saved first; the pane goes with its last tab.
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("?");
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(shown(cx), sides(1, &["D", "E"]));
    assert_eq!(fs::read_to_string(pane("B")).unwrap(), "# Pane B\n\ndroite!?");
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(shown(cx), sides(0, &["D"]));
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("seul");
    assert_eq!(text(cx), "# Pane D\n\nseul");
        // Down to one tab, the last one does not close.
    shell.update_in(cx, |s, window, cx| {
        while s.panes.len() > 1 {
            let other = s.groups[0].tabs.iter().copied().find(|&t| t != s.active).unwrap();
            s.close_tab(other, window, cx);
        }
    });
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(shown(cx), sides(0, &["D"]));
    assert_eq!(shell.read_with(cx, |s, _| s.panes.len()), 1, "the last tab does not close");
        // With a single pane, dropping a note on the right half opens the second.
    let whole = cx.debug_bounds("pane-0").unwrap();
    let half = gpui::point(whole.right() - px(40.), whole.center().y);
    cx.simulate_mouse_down(row, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(row + gpui::point(px(12.), px(4.)), Some(MouseButton::Left), gpui::Modifiers::none());
    cx.simulate_mouse_move(half, Some(MouseButton::Left), gpui::Modifiers::none());
    assert_eq!(shell.read_with(cx, |s, _| s.pane_aim), Some((0, true)));
    cx.simulate_mouse_up(half, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(shown(cx), sides(1, &["D", "B"]));
        // The layout is remembered: the width, and what each pane shows.
    assert_eq!(vault::load_file("panes"), format!("v2 0.5\n*{}\n*{}\n", pane("D").display(), pane("B").display()));
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(vault::load_file("panes"), format!("v2 0.5\n*{}\n", pane("D").display()));
}
