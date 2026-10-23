//! Folders, renaming, drag and drop, multiple selection, trash, outside changes.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Files. Ctrl+Shift+N shows the tree and creates a folder next to the selection.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("cou");
    cx.simulate_keystrokes("enter secondary-shift-n");
    cx.simulate_input("Archives: 2026");
    cx.simulate_keystrokes("enter");
    let archives = root.join("Archives 2026");
    assert!(archives.is_dir());
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.sel.clone())), (Mode::Tree, Some(archives.clone())));

    // F2 renames: the title of a note matching its name follows, the recents list too.
    // An icon, chosen in the menu grid, follows the note being renamed; it is
    // remembered in a file of the vault.
    shell.update_in(cx, |s, window, cx| {
        s.menu = Some(nav::Menu { at: point(px(300.), px(200.)), target: Some(root.join("Test.md")), text: None, commented: false, block: false, ..Default::default() });
        s.menu_do(nav::Do::Icon, Some(root.join("Test.md")), window, cx)
    });
    cx.run_until_parked();
    let grid = shell.read_with(cx, |s, _| s.icon_pick.as_ref().unwrap().at - point(s.nav.left, s.nav.left));
    cx.simulate_click(grid + point(px(5. + 30. + 14.), px(5. + 14.)), gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.icon_pick.is_none() && s.icons.get(&root.join("Test.md")).map(String::as_str) == Some("star")));
    shell.update(cx, |s, _| s.nav.sel = Some(root.join("Test.md")));
    cx.simulate_keystrokes("f2");
    cx.simulate_input("s");
    cx.simulate_keystrokes("enter");
    let renamed = fs::read_to_string(root.join("Tests.md")).unwrap();
    assert!(renamed.starts_with("# Tests\n1. un") && !root.join("Test.md").exists());
    assert!(shell.read_with(cx, |s, _| s.recent.contains(&root.join("Tests.md"))));
    assert_eq!(fs::read_to_string(root.join(".bref-icons")).unwrap(), "star\tTests.md\n");
    shell.update(cx, |s, cx| s.set_icon(root.join("Tests.md"), None, cx));
    assert_eq!(fs::read_to_string(root.join(".bref-icons")).unwrap(), "");
    // Links follow, in the file as in the shown note.
    assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[Tests]]");
    assert!(fs::read_to_string(root.join("Courses.md")).unwrap().ends_with("[[Tests]]"));

    // Changing the title renames the note too: links only follow once the
    // note is left, not at each intermediate title.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("tests");
    // The note opens on its body: go up to the title to change it.
    cx.simulate_keystrokes("enter secondary-home end");
    for letter in ["X", "Y"] {
        cx.simulate_input(letter);
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
    }
    assert!(root.join("TestsXY.md").is_file() && !root.join("TestsX.md").exists());
    assert!(fs::read_to_string(root.join("Courses.md")).unwrap().ends_with("[[Tests]]"));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("cou");
    cx.simulate_keystrokes("enter secondary-e");
    assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[TestsXY]]");

    // Drag and drop: a note, then a whole folder, moved into another folder.
    shell.update(cx, |s, cx| {
        s.move_into(&root.join("TestsXY.md"), &archives, cx);
        s.move_into(&root.join("Projets"), &archives, cx);
        // A folder does not go into itself.
        s.move_into(&archives, &archives.join("Projets"), cx);
    });
    assert!(archives.join("TestsXY.md").is_file() && archives.join("Projets/Plan.md").is_file());
    assert!(shell.read_with(cx, |s, _| {
        s.dirs.contains(&archives.join("Projets"))
            && s.recent.contains(&archives.join("Projets/Plan.md"))
            && s.notes.iter().all(|n| n.path.is_file())
    }));

    // Multiple selection: Ctrl+click adds a row, Shift+click extends from the
    // chosen row; duplicate, copy paths and trash apply to the batch.
    cx.run_until_parked();
    let mark = |cx: &mut gpui::VisualTestContext, from: &str, to: &str, extend: bool| {
        shell.update(cx, |s, cx| {
            let row = |s: &Shell, name: &str| s.nav.rows.iter().position(|r| r.path == root.join(name)).unwrap();
            s.nav.marked.clear();
            s.nav.sel = Some(root.join(from));
            s.nav_mark(row(s, to), extend, cx);
        });
    };
    mark(cx, "Courses.md", "Idées.md", false);
    cx.simulate_keystrokes("secondary-d");
    assert!(root.join("Courses 2.md").is_file() && root.join("Idées 2.md").is_file());
    assert_eq!(fs::read_to_string(root.join("Courses 2.md")).unwrap(), text(cx));
    cx.run_until_parked();
    mark(cx, "Courses.md", "Idées.md", true);
    assert_eq!(shell.read_with(cx, |s, _| s.nav.marked.len()), 3);
    mark(cx, "Courses 2.md", "Idées 2.md", false);
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::CopyRelative, s.nav.sel.clone(), window, cx));
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("Courses 2.md\nIdées 2.md"));
    cx.simulate_keystrokes("delete");
    assert!(root.join(".trash/Courses 2.md").is_file() && root.join(".trash/Idées 2.md").is_file());
    assert!(root.join("Courses.md").is_file() && shell.read_with(cx, |s, _| s.nav.marked.is_empty()));

    // Delete moves to the vault's trash, destroying nothing; the open note gives
    // way to an empty note.
    shell.update(cx, |s, _| s.nav.sel = Some(archives.clone()));
    cx.simulate_keystrokes("delete");
    assert!(root.join(".trash/Archives 2026/Projets/Plan.md").is_file() && !archives.exists());
    assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[TestsXY]]");
    shell.update(cx, |s, _| s.nav.sel = Some(root.join("Courses.md")));
    cx.simulate_keystrokes("delete");
    assert!(root.join(".trash/Courses.md").is_file());
    assert_eq!(text(cx), "");
    let left = shell.read_with(cx, |s, _| {
        assert!(s.dirs.is_empty() && s.pane().path.is_none() && s.recent.iter().all(|p| p.is_file()));
        s.notes.iter().map(|n| n.name.clone()).collect::<Vec<_>>()
    });
    assert_eq!(left, ["Idées"]);

    // What another program changes in the vault shows up without doing anything: a
    // note added, then the shown note rewritten elsewhere.
    fs::write(root.join("Ailleurs.md"), "# Ailleurs\n").unwrap();
    fs::write(root.join("Idées.md"), "# Idées\n\nréécrite\n").unwrap();
    shell.update(cx, |s, cx| s.open_note(&root.join("Idées.md"), cx));
    fs::write(root.join("Idées.md"), "# Idées\n\nréécrite ailleurs\n").unwrap();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Ailleurs")));
    assert_eq!(text(cx), "# Idées\n\nréécrite ailleurs\n");

    // A `#` title added at the top of a note that had none: the file takes
    // its name. With the title untouched, a freely named note keeps its own.
    fs::write(root.join("brouillon.md"), "texte\n").unwrap();
    shell.update(cx, |s, cx| s.open_note(&root.join("brouillon.md"), cx));
    cx.update(|window, cx| window.focus(&shell.read(cx).pane().editor.focus_handle(cx)));
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("suite");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("brouillon.md")).unwrap(), "texte\nsuite");
    cx.simulate_keystrokes("secondary-home");
    cx.simulate_input("# Titre");
    cx.simulate_keystrokes("enter");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("Titre.md")).unwrap(), "# Titre\ntexte\nsuite");
    assert!(!root.join("brouillon.md").exists());
}
