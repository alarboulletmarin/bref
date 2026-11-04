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
    assert!((bar.top() - top).abs() < px(0.5) && (pane.top() - top - px(34. + 6.)).abs() < px(1.), "the bar takes 34 px at the top, then 6 of ground: {bar:?} {pane:?}");
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
    assert!(saved.starts_with("v3 0\n") && saved.contains(&format!("{}\t{}", note("Alpha").display(), root.join("Ici/Plan.md").display())), "{saved}");
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

/// The bar is a row of pills on the ground: twelve tabs with long titles (one without a space), at 420, 860 and
/// 1,600 px and at zoom 11, 16 and 32, each pill 30 px high (scaled), no wider than 200, in the middle of the bar and
/// of the window buttons, and the row stops before the buttons.
#[gpui::test]
fn tabs_are_pills_that_stay_clear_of_the_window_buttons(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-tab-pills-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let notes: Vec<PathBuf> = (0..12)
        .map(|i| match i {
            3 => root.join(format!("{}.md", "Z".repeat(80))),
            _ => root.join(format!("Une note au titre vraiment très long, numéro {i} ✨ 日本語.md")),
        })
        .collect();
    for note in &notes {
        fs::write(note, "# Titre\n").unwrap();
    }
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = Shell::new(Some(root.clone()), vec![notes[0].clone()], window, cx);
        shell.client = true;
        shell
    });
    cx.run_until_parked();
    shell.update_in(cx, |s, window, cx| {
        for note in &notes[1..] {
            s.add_tab(window, cx);
            s.open_note(note, cx);
        }
    });
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..4 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    for (keys, zoom) in [("secondary-- ".repeat(5), 11.), ("secondary-0 ".into(), 16.), ("secondary-= ".repeat(16), 32.)] {
        cx.simulate_keystrokes(keys.trim_end());
        assert_eq!(shell.read_with(cx, |s, _| s.theme.size), zoom);
        let scale = if zoom < 16. { 0.85 } else if zoom > 16. { 1.5 } else { 1. };
        for width in [420., 860., 1600.] {
            cx.simulate_resize(size(px(width), px(600.)));
            settle(cx);
            let what = format!("{width} px at zoom {zoom}");
            let (bar, row, buttons) = (cx.debug_bounds("tabs-0").unwrap(), cx.debug_bounds("tabs-row-0").unwrap(), cx.debug_bounds("window-pill").unwrap());
            assert!(row.right() <= buttons.left() + px(0.5), "{what}: the row {row:?} runs under the buttons {buttons:?}");
            assert!(row.size.width > px(0.), "{what}: no room for a tab");
            let shown = shell.read_with(cx, |s, _| s.groups[0].shown);
            let mut drawn = 0;
            for tab in 0..12 {
                // Tabs scrolled out of the row do not count: only the ones that show at least in part.
                let Some(pill) = cx.debug_bounds(leak(format!("tab-{tab}"))).filter(|b| b.right() > row.left() && b.left() < row.right()) else { continue };
                drawn += 1;
                assert!((f32::from(pill.size.height) - 30. * scale).abs() < 1., "{what}: tab {tab} is {pill:?} high");
                assert!(f32::from(pill.size.width) <= 200. * scale + 1., "{what}: tab {tab} is {pill:?} wide");
                assert!(pill.top() >= bar.top() && pill.bottom() <= bar.bottom(), "{what}: tab {tab} {pill:?} out of the bar {bar:?}");
            }
            assert!(drawn >= 1, "{what}: no tab in view");
            let pill = cx.debug_bounds(leak(format!("tab-{shown}"))).expect("the tab shown is drawn");
            assert!(pill.size.width >= px(60.), "{what}: the tab shown is squeezed to {pill:?}");
            // The buttons and the pills share their middle (at zoom 16: the buttons do not scale).
            if zoom == 16. {
                assert!((pill.center().y - buttons.center().y).abs() < px(1.), "{what}: pill {pill:?} and buttons {buttons:?} are not level");
            }
        }
    }
    let _ = fs::remove_dir_all(&root);
}

/// A failed atomic write must leave the actual editor alive, including its undo history.
#[gpui::test]
fn failed_save_keeps_the_tab_and_its_edits(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "tabs-save-failure");
    let alpha = root.join("Alpha.md");
    let beta = root.join("Beta.md");
    let original = "# Alpha\n\noriginal";
    fs::write(&alpha, original).unwrap();
    fs::write(&beta, "# Beta\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&alpha, window, cx);
        s.open_from_nav(&beta, window, cx);
        s.open_from_nav(&alpha, window, cx);
    });
    fs::create_dir(root.join(".Alpha.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input(" UNSAVED CHANGES");
    let editor = shell.read_with(cx, |s, _| s.pane().editor.clone());
    assert!(shell.read_with(cx, |s, _| s.pane().dirty));
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(fs::read_to_string(&alpha).unwrap(), original);
    shell.read_with(cx, |s, cx| {
        assert!(s.panes.iter().any(|p| p.editor == editor), "the editor holding unsaved text was discarded");
        assert_eq!(s.pane().editor, editor);
        assert!(s.pane().dirty);
        assert_eq!(editor.read(cx).text(), format!("{original} UNSAVED CHANGES"));
        assert!(s.toasts.iter().any(|t| t.tone == Tone::Failed && t.text.contains("Alpha")));
        assert!(s.closed.is_empty());
    });
    cx.simulate_keystrokes("secondary-z");
    assert_ne!(editor.read_with(cx, |e, _| e.text().to_string()), format!("{original} UNSAVED CHANGES"));
    cx.simulate_keystrokes("secondary-shift-z");
    fs::remove_dir(root.join(".Alpha.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(fs::read_to_string(&alpha).unwrap(), format!("{original} UNSAVED CHANGES"));
    assert!(!shell.read_with(cx, |s, _| s.panes.iter().any(|p| p.editor == editor)));
    cx.simulate_keystrokes("secondary-alt-t");
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string()), format!("{original} UNSAVED CHANGES"));
    let _ = fs::remove_dir_all(&root);
}

/// Right click on a tab, or the palette: close it, the others, the pane or every file. Each tab is
/// saved first, one that cannot be stays with its edits, no file is deleted, and an emptied window
/// is left (and comes back at restart) on a new empty note.
#[gpui::test]
fn tabs_close_in_bulk_from_their_menu(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "tabs-bulk");
    let note = |name: &str| root.join(format!("{name}.md"));
    for name in ["Alpha", "Beta", "Gamma", "Delta"] {
        fs::write(note(name), format!("# {name}\n\n")).unwrap();
    }
    let csv = root.join("Données.csv");
    fs::write(&csv, "a,b\n1,2\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    let open = |cx: &mut gpui::VisualTestContext, path: &Path| {
        shell.update_in(cx, |s, window, cx| s.open(path, window, cx));
        cx.run_until_parked();
    };
    for name in ["Alpha", "Beta", "Gamma"] {
        open(cx, &note(name));
    }
    open(cx, &csv);
    shell.update_in(cx, |s, window, cx| s.tool_as_tab(window, cx));
    cx.run_until_parked();
    let names = |cx: &mut gpui::VisualTestContext, group: usize| shell.read_with(cx, |s, cx| s.tab_labels(group, cx));
    assert_eq!(names(cx, 0), vec!["Alpha", "Beta", "Gamma", "Données", "Graphe"]);
    let right_click = |cx: &mut gpui::VisualTestContext, tab: usize| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let at = cx.debug_bounds(leak(format!("tab-{tab}"))).unwrap().center();
        cx.simulate_event(gpui::MouseDownEvent { button: MouseButton::Right, position: at, modifiers: gpui::Modifiers::none(), click_count: 1, first_mouse: false });
        cx.simulate_event(gpui::MouseUpEvent { button: MouseButton::Right, position: at, modifiers: gpui::Modifiers::none(), click_count: 1 });
        cx.run_until_parked();
    };
    let menu = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.menu_groups(cx).concat().iter().map(|(label, _)| *label).collect::<Vec<_>>());

    // The menu of an inactive tab: it opens on that tab without showing it; Escape closes it, nothing else.
    let beta = shell.read_with(cx, |s, _| s.groups[0].tabs[1]);
    let shown = shell.read_with(cx, |s, _| s.active);
    right_click(cx, beta);
    assert_eq!(menu(cx), vec!["Fermer", "Fermer les autres onglets", "Fermer tous les onglets de ce volet"]);
    assert_eq!(shell.read_with(cx, |s, _| s.active), shown);
    cx.simulate_keystrokes("escape");
    assert!(shell.read_with(cx, |s, _| s.menu.is_none()));
    assert_eq!(names(cx, 0).len(), 5);

    // "Close other tabs" keeps the clicked tab, not the shown one; it gets shown, with input.
    right_click(cx, beta);
    let at = cx.debug_bounds("menu-entry-Fermer les autres onglets").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(names(cx, 0), vec!["Beta"]);
    shell.update_in(cx, |s, window, cx| {
        assert_eq!(s.pane().path.as_deref(), Some(note("Beta").as_path()));
        assert!(s.pane().editor.focus_handle(cx).is_focused(window));
    });
    assert!(csv.is_file() && note("Alpha").is_file() && note("Gamma").is_file(), "closing never deletes");
    // The closed files can come back, the last closed first (the graph is a tool, not a file).
    cx.simulate_keystrokes("secondary-alt-t");
    assert_eq!(names(cx, 0), vec!["Beta", "Données"]);

    // Two panes: closing every tab of one, from the keys of its menu, leaves the other as it was.
    shell.update_in(cx, |s, window, cx| { s.add_pane(Side::Right, window, cx); });
    open(cx, &note("Delta"));
    let first = shell.read_with(cx, |s, _| s.groups[0].tabs[0]);
    right_click(cx, first);
    cx.simulate_keystrokes("down down down enter");
    cx.run_until_parked();
    shell.read_with(cx, |s, _| {
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.panes[s.groups[0].tabs[0]].path.as_deref(), Some(note("Delta").as_path()));
        assert_eq!(s.active, s.groups[0].tabs[0]);
    });

    // Close all files with a failed save among them: that tab stays, with its text; the others close.
    open(cx, &note("Alpha"));
    fs::create_dir(root.join(".Alpha.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("non enregistré");
    let editor = shell.read_with(cx, |s, _| s.pane().editor.clone());
    // A draft with text is saved as a note; an empty one leaves nothing.
    cx.simulate_keystrokes("secondary-n");
    cx.simulate_input("# Brouillon\n\ntexte");
    cx.simulate_keystrokes("secondary-n");
    shell.update_in(cx, |s, window, cx| { s.add_pane(Side::Right, window, cx); });
    open(cx, &note("Gamma"));
    fs::create_dir(root.join(".Gamma.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("aussi");
    let gamma = shell.read_with(cx, |s, _| s.pane().editor.clone());
    let files = |root: &Path| fs::read_dir(root).unwrap().map(|e| e.unwrap().file_name()).filter(|n| !n.to_string_lossy().starts_with('.')).collect::<std::collections::BTreeSet<_>>();
    let before = files(&root);
    cx.dispatch_action(CloseAllFiles);
    cx.run_until_parked();
    shell.update_in(cx, |s, window, cx| {
        // Both failed tabs stay, each in its pane; input goes to the first.
        assert_eq!((s.panes.len(), s.groups.len()), (2, 2), "{:?}", s.panes.iter().map(|p| p.path.clone()).collect::<Vec<_>>());
        assert!(s.panes.iter().any(|p| p.editor == gamma && p.dirty));
        assert_eq!(s.pane().editor, editor);
        assert!(editor.read(cx).text().ends_with("non enregistré"));
        assert!(editor.focus_handle(cx).is_focused(window));
        assert!(s.toasts.iter().any(|t| t.tone == Tone::Failed && t.text.contains("Alpha")));
    });
    assert!(root.join("Brouillon.md").is_file());
    assert_eq!(files(&root), before, "closing neither deletes nor adds a file");

    // Once it can be saved, closing all leaves a single new empty note, at restart too.
    fs::remove_dir(root.join(".Alpha.tmp")).unwrap();
    fs::remove_dir(root.join(".Gamma.tmp")).unwrap();
    cx.dispatch_action(CloseAllFiles);
    cx.run_until_parked();
    shell.read_with(cx, |s, cx| {
        assert_eq!((s.panes.len(), s.groups.len()), (1, 1));
        assert!(s.pane().path.is_none() && s.pane().file.is_none() && s.pane().tool.is_none());
        assert!(s.pane().editor.read(cx).text().is_empty());
    });
    assert!(fs::read_to_string(note("Alpha")).unwrap().ends_with("non enregistré"));
    assert!(fs::read_to_string(note("Gamma")).unwrap().ends_with("aussi"));
    assert_eq!(files(&root), before);
    // Closing all again changes nothing.
    cx.dispatch_action(CloseAllFiles);
    assert_eq!(shell.read_with(cx, |s, _| s.panes.len()), 1);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let recent = vec![note("Gamma")];
    let (again, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), recent, window, cx));
    cx2.run_until_parked();
    again.read_with(cx2, |s, cx| {
        assert_eq!(s.panes.len(), 1);
        assert!(s.pane().path.is_none() && s.pane().editor.read(cx).text().is_empty());
    });
    // And Ctrl+Alt+T still brings back what was closed.
    cx.simulate_keystrokes("secondary-alt-t");
    assert!(shell.read_with(cx, |s, _| s.pane().path.is_some()));

    // Thirty tabs of long, twin names: the one kept is the one asked for.
    for n in 0..30 {
        fs::create_dir_all(root.join(format!("d{n}"))).unwrap();
        fs::write(root.join(format!("d{n}/Un nom de note très long, répété partout.md")), "x").unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    for n in 0..30 {
        open(cx, &root.join(format!("d{n}/Un nom de note très long, répété partout.md")));
    }
    let kept = shell.read_with(cx, |s, _| s.groups[0].tabs[15]);
    let path = shell.read_with(cx, |s, _| s.panes[kept].path.clone());
    shell.update_in(cx, |s, window, cx| s.close_other_tabs(kept, window, cx));
    shell.read_with(cx, |s, _| assert_eq!((s.panes.len(), s.pane().path.clone()), (1, path)));
    let _ = fs::remove_dir_all(&root);
}

/// Closing in bulk over a diagram, a picture and a table whose cell is still typed, its save
/// pending: each final write lands before its tab goes, and a diagram that cannot be written stays.
#[gpui::test]
fn bulk_close_writes_pending_tables_and_diagrams(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "tabs-bulk-kinds");
    let anchor = root.join("Ancre.md");
    fs::write(&anchor, "# Ancre\n").unwrap();
    let image = root.join("Photo.png");
    fs::write(&image, super::enlarge::png(4, 4)).unwrap();
    let csv = root.join("Données.csv");
    fs::write(&csv, "a,b\n1,2\n").unwrap();
    let drawing = root.join("Schéma.svg");
    fs::write(&drawing, Diagram::blank().to_svg(diagram::COLORS[0])).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    let open = |cx: &mut gpui::VisualTestContext, path: &Path| {
        shell.update_in(cx, |s, window, cx| s.open(path, window, cx));
        cx.run_until_parked();
    };
    for path in [&anchor, &image] {
        open(cx, path);
    }
    // A shape drawn in the diagram, which cannot be written (it saves as soon as it is drawn).
    let block = root.join(".Schéma.tmp");
    fs::create_dir(&block).unwrap();
    open(cx, &drawing);
    let canvas = shell.read_with(cx, |s, _| s.pane().drawing().cloned().unwrap());
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.simulate_input("r");
        let (from, to) = canvas.read_with(cx, |c, _| (c.spot(0., 0.), c.spot(120., 60.)));
        cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    };
    draw(cx);
    assert!(shell.read_with(cx, |s, _| s.pane().canvas_dirty));
    // A cell still being typed: its background save is only scheduled.
    open(cx, &csv);
    cx.simulate_input("modifié");
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_labels(0, cx)), vec!["Ancre", "Photo", "Schéma", "Données"]);

    // The diagram stays, live, and the others close, the table saved.
    cx.dispatch_action(CloseAllFiles);
    cx.run_until_parked();
    assert!(fs::read_to_string(&csv).unwrap().contains("modifié"));
    shell.read_with(cx, |s, _| {
        assert_eq!(s.panes.len(), 1);
        assert_eq!(s.pane().drawing(), Some(&canvas));
        assert!(s.pane().canvas_dirty && s.told(Tone::Failed, "Schéma.svg"));
    });
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shapes.len()), 1);

    // Writable again: it is saved, the window is left on an empty note, every file still there.
    fs::remove_dir(&block).unwrap();
    cx.dispatch_action(CloseAllFiles);
    cx.run_until_parked();
    assert_eq!(Diagram::from_svg(&fs::read_to_string(&drawing).unwrap()).unwrap().shapes.len(), 1);
    shell.read_with(cx, |s, cx| {
        assert_eq!(s.panes.len(), 1);
        assert!(s.pane().path.is_none() && s.pane().file.is_none() && s.pane().editor.read(cx).text().is_empty());
    });
    assert!(anchor.is_file() && image.is_file() && csv.is_file() && drawing.is_file());
    // They come back the last closed first, the picture too; the first one takes the empty note's place.
    cx.simulate_keystrokes("secondary-alt-t secondary-alt-t secondary-alt-t");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, cx| s.tab_labels(0, cx)), vec!["Schéma", "Données", "Photo"]);
    assert!(shell.read_with(cx, |s, _| matches!(s.pane().file.as_ref().map(|f| &f.kind), Some(FileKind::Image))));
    let _ = fs::remove_dir_all(&root);
}
