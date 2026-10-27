//! Tabs: several files open in a pane, one shown, reached and closed from the keys, kept at restart.

use super::*;

/// The tabs of pane `group`, by file, and the index of the one shown.
fn tabs(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext, group: usize) -> (Vec<Option<PathBuf>>, usize) {
    shell.read_with(cx, |s, _| {
        let g = &s.groups[group];
        (g.tabs.iter().map(|&t| s.panes[t].path.clone()).collect(), g.tabs.iter().position(|&t| t == g.shown).unwrap())
    })
}

#[gpui::test]
fn tabs_open_switch_close_and_come_back(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "tabs");
    let note = |name: &str| root.join(format!("{name}.md"));
    for name in ["Alpha", "Beta", "Gamma"] {
        fs::write(note(name), format!("# {name}\n\n")).unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Alpha"), window, cx));
    cx.simulate_keystrokes("secondary-e");
    cx.run_until_parked();
    // One tab: no bar, the pane looks as it always did (its top, measured, says so).
    let top = cx.debug_bounds("pane-0").unwrap().top();

    // As in an editor: a click in the tree opens the note in a tab of its own, right of the shown
    // one, and shows it; so does the palette.
    let click = |cx: &mut gpui::VisualTestContext, name: &str| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let at = cx.debug_bounds(leak(name.to_string())).unwrap_or_else(|| panic!("{name}")).center();
        cx.simulate_click(at, gpui::Modifiers::none());
        cx.run_until_parked();
    };
    click(cx, "nav-row-Beta");
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("gamma");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let all = |names: &[&str]| names.iter().map(|n| Some(note(n))).collect::<Vec<_>>();
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Beta", "Gamma"]), 2));
    // A click on a note open in a tab out of sight brings that tab to the front, and the palette
    // too: no second tab.
    click(cx, "nav-row-Alpha");
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Beta", "Gamma"]), 0));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("gamma");
    cx.simulate_keystrokes("enter");
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Beta", "Gamma"]), 2));
    // The bar sits over the note, inside the pane, and never over its text.
    let (bar, pane) = (cx.debug_bounds("tabs-0").unwrap(), cx.debug_bounds("pane-0").unwrap());
    assert!(bar.bottom() <= pane.top() + px(0.5) && bar.left() >= pane.left() - px(0.5) && bar.right() <= pane.right() + px(0.5), "{bar:?} over {pane:?}");
    assert!((bar.top() - top).abs() < px(0.5) && (pane.top() - top - px(34.)).abs() < px(1.), "the bar takes 34 px at the top: {bar:?} {pane:?}");
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_labels(0, cx)), vec!["Alpha", "Beta", "Gamma"]);

    // Ctrl+Tab goes round; Ctrl+Shift+Tab back.
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(tabs(&shell, cx, 0).1, 0);
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(tabs(&shell, cx, 0).1, 2);
    // Moving the shown tab left, twice: it reaches the start and stays there.
    cx.simulate_keystrokes("ctrl-shift-pageup ctrl-shift-pageup ctrl-shift-pageup");
    assert_eq!(tabs(&shell, cx, 0), (all(&["Gamma", "Alpha", "Beta"]), 0));

    // A note already in a tab is not opened twice: its tab is shown.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Beta"), window, cx));
    cx.run_until_parked();
    assert_eq!(tabs(&shell, cx, 0), (all(&["Gamma", "Alpha", "Beta"]), 2));
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("écrit");
    // Ctrl+W closes it, saved first; the neighbour is shown and has the keys.
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(fs::read_to_string(note("Beta")).unwrap(), "# Beta\n\nécrit");
    assert_eq!(tabs(&shell, cx, 0), (all(&["Gamma", "Alpha"]), 1));
    cx.simulate_input("!");
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string()), "# Alpha\n\n!");
    // Ctrl+Alt+T brings it back.
    cx.simulate_keystrokes("secondary-alt-t");
    assert_eq!(tabs(&shell, cx, 0), (all(&["Gamma", "Alpha", "Beta"]), 2));

    // A new note opens in a tab of its own; left empty then closed, no empty file stays in the vault.
    let before = fs::read_dir(&root).unwrap().count();
    cx.simulate_keystrokes("secondary-n");
    assert_eq!(tabs(&shell, cx, 0).0.len(), 4);
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_labels(0, cx)[3].clone()), "Nouvelle note");
    cx.simulate_input("# Brouillon");
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_labels(0, cx)[3].clone()), "Brouillon");
    cx.simulate_keystrokes("secondary-a backspace secondary-w");
    assert_eq!(tabs(&shell, cx, 0).0.len(), 3);
    assert_eq!(fs::read_dir(&root).unwrap().count(), before);

    // Closing a tab that is not shown, with the middle button: the shown one keeps the keys.
    let gamma = shell.read_with(cx, |s, _| s.groups[0].tabs[0]);
    let at = cx.debug_bounds(leak(format!("tab-{gamma}"))).unwrap().center();
    cx.simulate_event(gpui::MouseDownEvent { button: MouseButton::Middle, position: at, modifiers: gpui::Modifiers::none(), click_count: 1, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { button: MouseButton::Middle, position: at, modifiers: gpui::Modifiers::none(), click_count: 1 });
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Beta"]), 1));
    // Down to one tab: the bar goes, and Ctrl+W on the last tab of the last pane does nothing.
    cx.simulate_keystrokes("secondary-w");
    cx.run_until_parked();
    assert!((cx.debug_bounds("pane-0").unwrap().top() - top).abs() < px(0.5));
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha"]), 0));

    // Two notes of the same name in two folders: each tab says its folder.
    fs::create_dir(root.join("Ici")).unwrap();
    fs::create_dir(root.join("Là")).unwrap();
    fs::write(root.join("Ici/Plan.md"), "# Plan\n").unwrap();
    fs::write(root.join("Là/Plan.md"), "# Plan\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.open(&root.join("Ici/Plan.md"), window, cx);
        s.open(&root.join("Là/Plan.md"), window, cx);
    });
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_labels(0, cx)), vec!["Alpha", "Plan · Ici", "Plan · Là"]);

    // Thirty tabs: the bar scrolls, and the shown tab is always brought into view.
    for n in 0..30 {
        fs::write(root.join(format!("Note {n:02} au nom assez long pour remplir.md")), "x").unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    for n in 0..30 {
        let path = root.join(format!("Note {n:02} au nom assez long pour remplir.md"));
        shell.update_in(cx, |s, window, cx| s.open(&path, window, cx));
    }
    cx.run_until_parked();
    cx.run_until_parked();
    let bar = cx.debug_bounds("tabs-0").unwrap();
    for (keys, which) in [("", 32), ("ctrl-tab", 0), ("ctrl-shift-tab", 32)] {
        if !keys.is_empty() {
            cx.simulate_keystrokes(keys);
        }
        cx.run_until_parked();
        let tab = shell.read_with(cx, |s, _| s.groups[0].tabs[which]);
        let shown = cx.debug_bounds(leak(format!("tab-{tab}"))).unwrap();
        assert!(shown.left() >= bar.left() - px(0.5) && shown.right() <= bar.right() + px(0.5), "tab {which} {shown:?} out of {bar:?}");
    }

    // The tabs come back at the next start, in their order, the shown one shown; a file gone
    // since is dropped without a word.
    let saved = vault::load_file("panes");
    assert!(saved.starts_with("v2 0.5\n") && saved.contains(&format!("{}\t{}", note("Alpha").display(), root.join("Ici/Plan.md").display())), "{saved}");
    fs::remove_file(root.join("Ici/Plan.md")).unwrap();
    let recent = vec![note("Gamma")];
    let (again, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), recent, window, cx));
    cx2.run_until_parked();
    let (open, shown) = tabs(&again, cx2, 0);
    assert_eq!(open.len(), 32);
    assert_eq!((open[0].clone(), open[1].clone(), shown), (Some(note("Alpha")), Some(root.join("Là/Plan.md")), 31));
    // At startup the bar has no places yet: the shown tab, the last one, is brought into view all the same.
    let bar = cx2.debug_bounds("tabs-0").unwrap();
    let tab = again.read_with(cx2, |s, _| s.groups[0].tabs[31]);
    let last = cx2.debug_bounds(leak(format!("tab-{tab}"))).unwrap();
    assert!(last.right() <= bar.right() + px(0.5), "{last:?} out of {bar:?}");
    let _ = fs::remove_dir_all(&root);
}

/// The arrows in the tree preview notes in a single tab, in italics, that the next note browsed
/// replaces; a tab left empty is used by what opens next.
#[gpui::test]
fn the_tree_previews_in_a_single_tab(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "tabs-preview");
    let note = |name: &str| root.join(format!("{name}.md"));
    for name in ["Alpha", "Beta", "Gamma"] {
        fs::write(note(name), format!("# {name}\n\n")).unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Alpha"), window, cx));
    let all = |names: &[&str]| names.iter().map(|n| Some(note(n))).collect::<Vec<_>>();
    let preview = |shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.pane().preview);
    let browse = |name: &str, cx: &mut gpui::VisualTestContext| shell.update_in(cx, |s, window, cx| s.preview_note(&note(name), window, cx));

    // Beta, then Gamma, browsed from the tree: one preview tab, the opened note stays where it was.
    shell.update_in(cx, |s, window, _| window.focus(&s.nav.focus));
    browse("Beta", cx);
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Beta"]), 1));
    assert!(preview(&shell, cx));
    browse("Gamma", cx);
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Gamma"]), 1));
    // A note already open: its tab is shown, the preview tab stays for the next one.
    browse("Alpha", cx);
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Gamma"]), 0));
    browse("Beta", cx);
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Beta"]), 1));
    // Opened for good, it keeps its tab and loses its italics; the next preview gets a new one.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Beta"), window, cx));
    assert!(!preview(&shell, cx));
    browse("Gamma", cx);
    assert_eq!(tabs(&shell, cx, 0), (all(&["Alpha", "Beta", "Gamma"]), 2));

    // Ctrl+N twice: the second finds the first new note still empty and uses its tab.
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Gamma"), window, cx));
    cx.simulate_keystrokes("secondary-n secondary-n");
    assert_eq!(tabs(&shell, cx, 0).0.len(), 4);
    // A link to a note that does not exist yet: the new note takes that empty tab too.
    shell.update_in(cx, |s, window, cx| s.open_wiki("Delta", window, cx));
    assert_eq!(tabs(&shell, cx, 0).0.len(), 4);
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_labels(0, cx)[3].clone()), "Delta");
    // An attachment opens in the system's application: no tab is left behind for it.
    fs::write(root.join("rapport.pdf"), "%PDF").unwrap();
    shell.update_in(cx, |s, window, cx| s.open(&root.join("rapport.pdf"), window, cx));
    assert_eq!(tabs(&shell, cx, 0).0.len(), 4);
    assert_eq!(shell.read_with(cx, |s, _| s.opened.last().cloned()), Some(root.join("rapport.pdf")));
    let _ = fs::remove_dir_all(&root);
}
