//! Sharing through the real menu, clipboard and native save-dialog boundary.

use super::*;

fn settle(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn state(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> (Option<PathBuf>, bool, String) {
    shell.read_with(cx, |s, cx| (s.pane().path.clone(), s.pane().dirty, s.pane().editor.read(cx).text().to_string()))
}

fn choose(cx: &mut gpui::VisualTestContext, index: usize) {
    cx.simulate_keystrokes("secondary-alt-e home");
    for _ in 0..index { cx.simulate_keystrokes("down"); }
    cx.simulate_keystrokes("enter");
}

#[gpui::test]
fn sharing_notes_uses_the_whole_live_document(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "sharing-note");
    let source = root.join("Été.md");
    fs::write(&source, "# Été\n\n**Bonjour** 😀").unwrap();
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&source, window, cx));
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("\n- [x] nouveau");
    cx.simulate_keystrokes("shift-home");
    let before = state(&shell, cx);
    assert!(before.1);
    let disk = fs::read_to_string(&source).unwrap();
    let plain = "Été\n\nBonjour 😀\n- [x] nouveau";
    // The button opens the shared menu; it never copies just the selected passage.
    settle(cx);
    let button = cx.debug_bounds("copy-all").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    settle(cx);
    assert_eq!(shell.read_with(cx, |s, cx| s.menu_groups(cx).concat().len()), 5);
    cx.simulate_keystrokes("enter");
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), before.2);
    choose(cx, 1);
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), plain);
    for (index, name, expected) in [(2, "copie été.md", before.2.as_str()), (3, "copie été.txt", plain)] {
        choose(cx, index);
        assert!(cx.did_prompt_for_new_path());
        let destination = root.join(name);
        cx.simulate_new_path_selection(|_| Some(destination.clone()));
        settle(cx);
        assert_eq!(fs::read_to_string(&destination).unwrap(), expected);
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, name)));
        assert_eq!(state(&shell, cx), before);
        assert_eq!(fs::read_to_string(&source).unwrap(), disk);
    }
    // The existing PDF exporter joins the same document-aware menu.
    choose(cx, 4);
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(root.join("copie.pdf")));
    settle(cx);
    assert!(fs::read(root.join("copie.pdf")).unwrap().starts_with(b"%PDF-"));
    assert_eq!(state(&shell, cx), before);
    assert_eq!(fs::read_to_string(&source).unwrap(), disk);
    // Cancel, an occupied file, a directory and a missing parent leave source and destinations intact.
    choose(cx, 2);
    cx.simulate_new_path_selection(|_| None);
    settle(cx);
    for destination in [source.clone(), root.clone(), root.join("absent/copy.md")] {
        choose(cx, 2);
        cx.simulate_new_path_selection(|_| Some(destination.clone()));
        settle(cx);
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, &destination.display().to_string())));
        assert_eq!(state(&shell, cx), before);
        assert_eq!(fs::read_to_string(&source).unwrap(), disk);
    }
    // Existing code-block shortcut keeps its original scope.
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("\n```\nlet x = 1;\n```");
    cx.simulate_keystrokes("up secondary-shift-c");
    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap().trim(), "let x = 1;");
}

#[gpui::test]
fn sharing_empty_drafts_and_pending_dialogs(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "sharing-draft");
    shell.update_in(cx, |s, window, cx| { s.add_pane(Side::Right, window, cx); });
    for text in ["", "# 未保存 😀\n\ntexte\nmultiligne"] {
        shell.update_in(cx, |s, window, cx| s.focus_group(0, window, cx));
        shell.update_in(cx, |s, window, cx| s.new_note_in(Some(root.clone()), window, cx));
        if !text.is_empty() { cx.simulate_input(text); }
        let before = state(&shell, cx);
        assert!(before.0.is_none());
        choose(cx, 0);
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), text);
        choose(cx, 3);
        // Another tab can become active while the OS dialog is pending.
        shell.update_in(cx, |s, window, cx| s.focus_group(1, window, cx));
        let destination = root.join(if text.is_empty() { "empty.txt" } else { "draft.txt" });
        cx.simulate_new_path_selection(|_| Some(destination.clone()));
        settle(cx);
        assert_eq!(fs::read_to_string(destination).unwrap(), crate::logic::sharing::plain_text(text));
        assert!(shell.read_with(cx, |s, cx| s.panes.iter().any(|p| (p.path.clone(), p.dirty, p.editor.read(cx).text().to_string()) == before)));
    }
}

#[gpui::test]
fn sharing_follows_diagrams_images_and_tables(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "sharing-views");
    fs::write(root.join("Note.md"), "# Hidden\n").unwrap();
    let mut diagram = Diagram::default();
    diagram.add_shape(diagram::Form::Rect, 20., 20., 120., 60.);
    let original = diagram.to_svg(diagram::COLORS[0]);
    fs::write(root.join("Drawing.svg"), &original).unwrap();
    let foreign = r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120"><rect width="240" height="120" fill="#3875c9"/></svg>"##;
    fs::write(root.join("Foreign.svg"), foreign).unwrap();
    let png = figure::png(foreign, 1., 0xffffff).unwrap();
    fs::write(root.join("Picture.png"), &png).unwrap();
    fs::write(root.join("Data.csv"), "Name,Count\nAlpha,1\nBeta,2\n").unwrap();
    shell.update(cx, |s, cx| { s.rescan(&root, cx); s.open_note(&root.join("Note.md"), cx); });
    for (name, entries) in [("Drawing.svg", 3), ("Foreign.svg", 2), ("Picture.png", 1), ("Data.csv", 2)] {
        shell.update(cx, |s, cx| { s.open_note(&root.join(name), cx); });
        settle(cx);
        let hidden = state(&shell, cx);
        cx.simulate_keystrokes("secondary-alt-e");
        assert_eq!(shell.read_with(cx, |s, cx| s.menu_groups(cx).concat().len()), entries);
        cx.simulate_keystrokes("escape");
        if name == "Drawing.svg" {
            let canvas = shell.read_with(cx, |s, _| s.pane().drawing().unwrap().clone());
            diagram.add_shape(diagram::Form::Ellipse, 200., 30., 80., 80.);
            canvas.update(cx, |c, cx| { assert!(c.replace(diagram.clone(), cx)); });
            shell.update(cx, |s, _| s.pane_mut().canvas_dirty = true);
            choose(cx, 0);
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert_eq!(Diagram::from_svg(&copied).unwrap().to_svg(diagram::COLORS[0]), diagram.to_svg(diagram::COLORS[0]));
            choose(cx, 1);
            cx.simulate_new_path_selection(|_| Some(root.join("drawing-copy.svg")));
            settle(cx);
            assert_eq!(fs::read_to_string(root.join("drawing-copy.svg")).unwrap(), copied);
            assert!(shell.read_with(cx, |s, _| s.pane().canvas_dirty));
            assert_eq!(fs::read_to_string(root.join(name)).unwrap(), original);
            choose(cx, 2);
            settle(cx);
            assert!(root.join("Drawing.png").exists());
        } else if name == "Foreign.svg" {
            choose(cx, 0);
            assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), foreign);
            choose(cx, 1);
            cx.simulate_new_path_selection(|_| Some(root.join("foreign-copy.svg")));
            settle(cx);
            assert_eq!(fs::read_to_string(root.join("foreign-copy.svg")).unwrap(), foreign);
        } else if name == "Picture.png" {
            choose(cx, 0);
            cx.simulate_new_path_selection(|_| Some(root.join("picture-copy.png")));
            settle(cx);
            assert_eq!(fs::read(root.join("picture-copy.png")).unwrap(), png);
        } else {
            // Type an edit, then copy every existing format from the menu, before autosave.
            let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());
            shell.update_in(cx, |s, window, cx| s.focus_pane(s.active, window, cx));
            sheet.update(cx, |s, cx| { s.set_view((1, 0), (0., 0.), cx); });
            cx.simulate_input("Updated");
            cx.simulate_keystrokes("enter");
            let undo = sheet.read_with(cx, |s, _| s.undo_len());
            let disk = fs::read(root.join(name)).unwrap();
            for format in 0..4 {
                choose(cx, 0);
                cx.simulate_keystrokes("home");
                for _ in 0..format { cx.simulate_keystrokes("down"); }
                cx.simulate_keystrokes("enter");
                let copied = cx.read_from_clipboard().unwrap().text().unwrap();
                assert!(copied.contains("Updated") && copied.contains("Beta"), "{format}: {copied}");
                let before: std::collections::HashSet<_> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).collect();
                choose(cx, 1);
                cx.simulate_keystrokes("home");
                for _ in 0..format { cx.simulate_keystrokes("down"); }
                cx.simulate_keystrokes("enter");
                settle(cx);
                let added: Vec<_> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).filter(|p| !before.contains(p)).collect();
                assert_eq!(added.len(), 1, "format {format}");
                assert_eq!(fs::read_to_string(&added[0]).unwrap(), copied);
                assert_eq!(fs::read(root.join(name)).unwrap(), disk);
                assert_eq!(sheet.read_with(cx, |s, _| s.undo_len()), undo);
            }
            // A multi-cell selection retains the sheet's existing scope.
            sheet.update(cx, |s, cx| s.pick("row", 1, cx));
            choose(cx, 0);
            cx.simulate_keystrokes("home enter");
            let copied = cx.read_from_clipboard().unwrap().text().unwrap();
            assert!(copied.contains("Updated") && !copied.contains("Beta"));
        }
        assert_eq!(state(&shell, cx), hidden);
    }
}

#[gpui::test]
fn sharing_menu_fits_themes_and_narrow_windows(cx: &mut TestAppContext) {
    let (_root, shell, cx, _config) = start(cx, "sharing-layout");
    for name in ["Bref Light", "Bref Dark", "Nord"] {
        shell.update_in(cx, |s, window, cx| { s.prefs.theme = name.into(); s.restyle(window, cx); });
        for width in [240., 320., 420., 860., 1600.] {
            cx.simulate_resize(size(px(width), px(720.)));
            settle(cx);
            let trigger = cx.debug_bounds("copy-all").unwrap();
            cx.simulate_click(trigger.center(), gpui::Modifiers::none());
            settle(cx);
            assert!(shell.read_with(cx, |s, _| s.menu.as_ref().is_some_and(|m| m.share.is_some())));
            let menu = cx.debug_bounds("menu").unwrap();
            assert!(menu.left() >= px(0.) && menu.right() <= px(width) + px(0.5), "{name} {width}: {menu:?}");
            assert!(menu.top() >= px(0.) && menu.bottom() <= px(720.));
            cx.simulate_keystrokes("up home down end escape");
            assert!(shell.read_with(cx, |s, _| s.menu.is_none()));
        }
    }
}

/// English labels and keyboard sharing follow the active pane.
#[gpui::test]
fn sharing_keys_target_the_active_pane(cx: &mut TestAppContext) {
    crate::lang::in_english(|| {
        let (root, shell, cx, _config) = start(cx, "sharing-panes");
        fs::write(root.join("Left.md"), "# Left\n").unwrap();
        fs::write(root.join("Right.md"), "# Right\n").unwrap();
        cx.simulate_resize(size(px(1400.), px(800.)));
        shell.update_in(cx, |s, window, cx| {
            s.open_from_nav(&root.join("Left.md"), window, cx);
            assert!(s.add_pane(Side::Right, window, cx));
            s.open_from_nav(&root.join("Right.md"), window, cx);
            s.focus_group(0, window, cx);
        });
        choose(cx, 0);
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "# Left\n");
        shell.update_in(cx, |s, window, cx| s.focus_group(1, window, cx));
        choose(cx, 0);
        assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "# Right\n");
        cx.simulate_keystrokes("secondary-alt-e");
        assert_eq!(shell.read_with(cx, |s, cx| s.menu_groups(cx).concat()[0].0), "Copy note as Markdown");
        cx.simulate_keystrokes("escape");
    });
}
