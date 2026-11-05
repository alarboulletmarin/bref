//! The displayed document, the tree selection and the note used by backlinks are independent.

use super::*;

const FILES: [&str; 6] = ["Note.md", "Drawing.svg", "Foreign.svg", "Picture.png", "Data.csv", "Data.tsv"];

fn fixtures(root: &Path) {
    fs::write(root.join("Note.md"), "# Note\n\n![](Foreign.svg)\n").unwrap();
    fs::write(root.join("Other.md"), "# Other\n\n[[Note]]\n").unwrap();
    let mut diagram = Diagram::default();
    diagram.add_shape(diagram::Form::Rect, 20., 20., 120., 60.);
    fs::write(root.join("Drawing.svg"), diagram.to_svg(diagram::COLORS[0])).unwrap();
    fs::write(root.join("Foreign.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120"><rect width="240" height="120" fill="#3875c9"/></svg>"##).unwrap();
    let svg = fs::read_to_string(root.join("Foreign.svg")).unwrap();
    let png = crate::ui::figure::png(&svg, 1., 0xffffff).unwrap();
    fs::write(root.join("Picture.png"), png).unwrap();
    fs::write(root.join("Data.csv"), "Name,Count\nAlpha,1\n").unwrap();
    fs::write(root.join("Data.tsv"), "Name\tCount\nBeta\t2\n").unwrap();
}

fn frames(cx: &mut gpui::VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn current(cx: &mut gpui::VisualTestContext, name: &str) {
    frames(cx);
    // GPUI 0.2.2 keeps old debug selectors between frames: compare the bounds of a stable
    // selector, rather than treating an old selector's presence as a row still being current.
    let row = if name.ends_with(".md") { name.trim_end_matches(".md") } else { name };
    let expected = cx.debug_bounds(leak(format!("nav-row-{row}"))).unwrap();
    let current = cx.debug_bounds("nav-current").unwrap();
    assert!((current.top() - expected.top()).abs() < px(0.5) && (current.bottom() - expected.bottom()).abs() < px(0.5), "{name}: {current:?} instead of {expected:?}");
}

/// Tree opening and tab switching mark every supported view, even with a Markdown editor underneath.
#[gpui::test]
fn the_current_file_follows_mixed_views(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "current-mixed");
    fixtures(&root);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&root.join("Note.md"), window, cx);
        s.show_nav(Mode::Tree, false, window, cx);
    });
    current(cx, "Note.md");
    // Load into this tab, as history and note links can do: the Markdown path stays underneath.
    for file in &FILES[1..] {
        shell.update(cx, |s, cx| s.open_note(&root.join(file), cx));
        frames(cx);
        assert_eq!(shell.read_with(cx, |s, _| s.pane().file_path().cloned()), Some(root.join(file)), "view stays shown");
        current(cx, file);
        assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(root.join("Note.md")));
    }
    for file in FILES {
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&root.join(file), window, cx));
        current(cx, file);
    }
    cx.simulate_keystrokes("ctrl-tab");
    current(cx, "Note.md");
    cx.simulate_keystrokes("ctrl-shift-tab");
    current(cx, "Data.tsv");
}

/// Graph nodes use the displayed file; tables have no node, and backlinks stay on the last note.
#[gpui::test]
fn the_current_file_in_the_graph_is_not_the_backlink_anchor(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "current-graph");
    fixtures(&root);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&root.join("Note.md"), window, cx);
        s.show_nav(Mode::Graph, false, window, cx);
    });
    frames(cx);
    for file in FILES {
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&root.join(file), window, cx));
        frames(cx);
        // Selection is independent of the document. Do not preview the selected row here.
        shell.update(cx, |s, cx| { s.nav.sel = Some(root.join("Other.md")); cx.notify(); });
        frames(cx);
        shell.read_with(cx, |s, cx| {
            let expected = (!file.ends_with(".csv") && !file.ends_with(".tsv")).then(|| root.join(file));
            assert_eq!(s.graph.read(cx).indicated(), (expected.as_deref(), Some(root.join("Other.md").as_path())), "{file}");
            assert_eq!(s.nav.anchor, Some(root.join("Note.md")), "backlinks while showing {file}");
        });
    }
    shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Links, false, window, cx));
    frames(cx);
    assert_eq!(shell.read_with(cx, |s, _| s.nav.rows.iter().map(|r| r.path.clone()).collect::<Vec<_>>()), vec![root.join("Other.md")]);
}

/// Palette and history use the same current marker as the tree, in every supported view.
#[gpui::test]
fn the_current_file_follows_the_palette_and_history(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "current-travel");
    fixtures(&root);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Tree, false, window, cx));
    for file in FILES {
        shell.update_in(cx, |s, window, cx| s.open_from_nav(&root.join("Other.md"), window, cx));
        frames(cx);
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input(file.trim_end_matches(".md"));
        cx.simulate_keystrokes("enter");
        current(cx, file);
        let (back, forward) = if cfg!(target_os = "macos") { ("cmd-[", "cmd-]") } else { ("alt-left", "alt-right") };
        cx.simulate_keystrokes(back);
        current(cx, "Other.md");
        cx.simulate_keystrokes(forward);
        current(cx, file);
    }
}

/// Keyboard focus and a multiple selection do not replace the marker of the displayed file.
#[gpui::test]
fn the_current_file_is_separate_from_tree_selection(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "current-selection");
    fixtures(&root);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&root.join("Note.md"), window, cx));
    frames(cx);
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&root.join("Foreign.svg"), window, cx);
        s.show_nav(Mode::Tree, false, window, cx);
    });
    current(cx, "Foreign.svg");
    shell.update(cx, |s, cx| { s.nav.sel = None; cx.notify(); });
    let mut held = gpui::Modifiers::none();
    if cfg!(target_os = "macos") { held.platform = true; } else { held.control = true; }
    for name in ["Note", "Other"] {
        let at = cx.debug_bounds(leak(format!("nav-row-{name}"))).unwrap().center();
        cx.simulate_click(at, held);
        current(cx, "Foreign.svg");
    }
    shell.read_with(cx, |s, _| {
        assert_eq!(s.nav.sel, Some(root.join("Other.md")));
        assert_eq!(s.nav.marked.len(), 2);
        assert!(s.nav.marked.contains(&root.join("Note.md")) && s.nav.marked.contains(&root.join("Other.md")));
        assert_eq!(s.nav.anchor, Some(root.join("Note.md")));
    });
    // Keyboard preview keeps input in the tree while marking the file it displays.
    shell.update_in(cx, |s, window, cx| {
        s.nav.marked.clear();
        s.nav.sel = Some(root.join("Drawing.svg"));
        window.focus(&s.nav.focus);
        cx.notify();
    });
    cx.simulate_keystrokes("down");
    current(cx, "Foreign.svg");
    assert!(shell.update_in(cx, |s, window, _| s.nav.focus.is_focused(window)));
    cx.simulate_keystrokes("enter");
    current(cx, "Foreign.svg");
}

/// Each pane supplies its shown tab's path, including after restoring a layout with an SVG active.
#[gpui::test]
fn the_current_file_follows_panes_and_restart(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "current-restore");
    fixtures(&root);
    cx.simulate_resize(size(px(1400.), px(900.)));
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&root.join("Note.md"), window, cx);
        s.open_from_nav(&root.join("Foreign.svg"), window, cx);
        s.show_nav(Mode::Tree, false, window, cx);
    });
    current(cx, "Foreign.svg");
    shell.update_in(cx, |s, window, cx| {
        assert!(s.add_pane(Side::Right, window, cx));
        s.open_from_nav(&root.join("Data.csv"), window, cx);
    });
    current(cx, "Data.csv");
    shell.update_in(cx, |s, window, cx| s.focus_group(0, window, cx));
    current(cx, "Foreign.svg");
    shell.update_in(cx, |s, window, cx| s.focus_group(1, window, cx));
    current(cx, "Data.csv");
    shell.update_in(cx, |s, window, cx| s.focus_group(0, window, cx));
    current(cx, "Foreign.svg");
    let state = shell.read_with(cx, |s, _| s.panes_state());
    let recent = shell.read_with(cx, |s, _| s.recent.clone());
    let (again, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), recent, window, cx));
    current(cx2, "Foreign.svg");
    assert_eq!(again.read_with(cx2, |s, _| s.panes_state()), state);
    again.update_in(cx2, |s, window, cx| s.focus_group(1, window, cx));
    current(cx2, "Data.csv");
}

/// A rescan after an external rename, move or deletion must not mark the hidden Markdown note.
#[gpui::test]
fn the_current_file_does_not_fall_back_after_external_changes(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "current-outside");
    fixtures(&root);
    fs::create_dir(root.join("Folder")).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&root.join("Note.md"), window, cx);
        s.show_nav(Mode::Graph, false, window, cx);
    });
    for (from, to) in [("Foreign.svg", Some("Renamed.svg")), ("Renamed.svg", Some("Folder/Moved.svg")), ("Folder/Moved.svg", None)] {
        shell.update(cx, |s, cx| s.open_note(&root.join(from), cx));
        frames(cx);
        assert_eq!(shell.read_with(cx, |s, cx| s.graph.read(cx).indicated().0.map(Path::to_path_buf)), Some(root.join(from)));
        match to {
            Some(to) => fs::rename(root.join(from), root.join(to)).unwrap(),
            None => fs::remove_file(root.join(from)).unwrap(),
        }
        shell.update(cx, |s, cx| s.rescan(&root, cx));
        frames(cx);
        shell.read_with(cx, |s, cx| {
            assert_eq!(s.graph.read(cx).indicated().0, None);
            assert_eq!(s.pane().displayed_path(), Some(&root.join(from)));
            assert!(!s.nav_rows().iter().any(|row| Some(&row.path) == s.pane().displayed_path()));
            assert_eq!(s.nav.anchor, Some(root.join("Note.md")));
        });
    }
}

/// Whatever changes the displayed document (tab, pane, file view), the tree selection moves to it;
/// between two changes it stays where the user puts it.
#[gpui::test]
fn the_tree_selection_follows_the_displayed_document(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "current-follow");
    fixtures(&root);
    cx.simulate_resize(size(px(1400.), px(900.)));
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| {
        s.show_nav(Mode::Tree, false, window, cx);
        s.open_from_nav(&root.join("Note.md"), window, cx);
        s.open_from_nav(&root.join("Data.csv"), window, cx);
        s.open_from_nav(&root.join("Foreign.svg"), window, cx);
    });
    frames(cx);
    let sel = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.nav.sel.clone());
    assert_eq!(sel(cx), Some(root.join("Foreign.svg")));
    // Tabs, both ways, round the end.
    for (keys, file) in [("ctrl-tab", "Note.md"), ("ctrl-tab", "Data.csv"), ("ctrl-shift-tab", "Note.md"), ("ctrl-shift-tab", "Foreign.svg")] {
        cx.simulate_keystrokes(keys);
        frames(cx);
        assert_eq!(sel(cx), Some(root.join(file)), "{keys} to {file}");
        current(cx, file);
    }
    // A picture loaded into the tab, then the note under it again.
    shell.update(cx, |s, cx| s.open_note(&root.join("Picture.png"), cx));
    frames(cx);
    assert_eq!(sel(cx), Some(root.join("Picture.png")));
    // The user moves the selection: it stays there while the document does not change.
    shell.update(cx, |s, cx| { s.nav.sel = Some(root.join("Other.md")); cx.notify(); });
    frames(cx);
    assert_eq!(sel(cx), Some(root.join("Other.md")));
    // Another pane, then back.
    shell.update_in(cx, |s, window, cx| {
        assert!(s.add_pane(Side::Right, window, cx));
        s.open_from_nav(&root.join("Data.tsv"), window, cx);
    });
    frames(cx);
    assert_eq!(sel(cx), Some(root.join("Data.tsv")));
    shell.update_in(cx, |s, window, cx| s.focus_group(0, window, cx));
    frames(cx);
    assert_eq!(sel(cx), Some(root.join("Picture.png")));
    // Only one row wears the selection.
    current(cx, "Picture.png");
}
