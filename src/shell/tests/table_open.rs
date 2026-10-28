use super::*;

fn window<'a>(cx: &'a mut TestAppContext, tag: &str, files: &[(&str, &[u8])]) -> (PathBuf, Entity<Shell>, &'a mut gpui::VisualTestContext, std::sync::MutexGuard<'static, ()>) {
    let root = std::env::temp_dir().join(format!("bref-table-open-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    fs::write(root.join("Note.md"), "# Note\n\n[table](data.csv)\n").unwrap();
    for (name, bytes) in files {
        fs::write(root.join(name), bytes).unwrap();
    }
    let note = root.join("Note.md");
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = Shell::new(Some(root.clone()), vec![note], window, cx);
        shell.client = true;
        shell
    });
    cx.run_until_parked();
    (root, shell, cx, config)
}

fn frames(cx: &mut gpui::VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn open(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext, root: &Path, name: &str) {
    let (notes, dirs, images) = vault::scan(root);
    shell.update(cx, |s, cx| {
        s.sync(notes, dirs, images, cx);
        s.open_note(&root.join(name), cx);
    });
    frames(cx);
}

fn shown(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> bool {
    shell.read_with(cx, |s, _| s.shown_sheet().is_some())
}

/// A CSV with nothing in it opens in the table view like any other: an empty grid to write in.
#[gpui::test]
fn an_empty_csv_opens_as_a_table(cx: &mut TestAppContext) {
    let files: [(&str, &[u8]); 5] = [
        ("zero.csv", b""),
        ("newline.csv", b"\n"),
        ("header.csv", b"a,b,c\n"),
        ("bom.csv", b"\xff\xfe"),
        ("bom8.csv", b"\xef\xbb\xbf"),
    ];
    let (root, shell, cx, _config) = window(cx, "empty", &files);
    for (name, _) in files {
        open(&shell, cx, &root, name);
        assert!(shown(&shell, cx), "{name} is not shown as a table");
        let (rows, cols) = shell.read_with(cx, |s, cx| {
            let sheet = s.shown_sheet().unwrap();
            let table = sheet.read(cx).table();
            (table.rows(), table.cols())
        });
        assert!(rows >= 1, "{name}: {rows} x {cols}");
        assert!(shell.read_with(cx, |s, _| s.toasts.is_empty()), "{name}: a message is left");
    }
    // Emptied from outside while open: the table stays, empty.
    fs::write(root.join("header.csv"), b"x,y\n1,2\n").unwrap();
    open(&shell, cx, &root, "header.csv");
    fs::write(root.join("header.csv"), b"").unwrap();
    shell.update(cx, |s, cx| s.shown_sheet().unwrap().update(cx, |sheet, cx| sheet.reload_if_changed(cx)));
    frames(cx);
    assert!(shown(&shell, cx), "emptied from outside: the table is gone");
}

/// A table opened while the note's editor has the focus stays on screen, whatever the way in.
#[gpui::test]
fn a_table_opened_from_the_editor_stays(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = window(cx, "focus", &[("data.csv", b"a,b\n1,2\n")]);
    let focus_editor = |cx: &mut gpui::VisualTestContext| {
        shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
        frames(cx);
    };
    for how in ["open_note", "palette", "link"] {
        shell.update(cx, |s, cx| s.open_note(&root.join("Note.md"), cx));
        focus_editor(cx);
        assert!(!shown(&shell, cx));
        match how {
            "open_note" => open(&shell, cx, &root, "data.csv"),
            "palette" => {
                cx.simulate_keystrokes("secondary-p");
                cx.simulate_input("data.csv");
                cx.simulate_keystrokes("enter");
                frames(cx);
            }
            _ => {
                shell.update_in(cx, |s, window, cx| s.follow(&markdown::Link::Wiki("data.csv".into()), window, cx));
                frames(cx);
            }
        }
        assert!(shown(&shell, cx), "{how}: the note replaced the table at the next frame");
        // Typing goes to the table.
        cx.simulate_keystrokes("x enter");
        frames(cx);
        let text = shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).table().row(0).join("|"));
        assert!(text.contains('x'), "{how}: typing did not reach the table: {text}");
        assert!(!fs::read_to_string(root.join("Note.md")).unwrap().contains('x'), "{how}: typing went to the note");
    }
}

/// With two panes, a table opened in the second one while its editor has the focus stays there, and the
/// first pane keeps its note.
#[gpui::test]
fn a_table_opened_in_the_second_pane_stays(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = window(cx, "two", &[("data.csv", b"a,b\n1,2\n"), ("Other.md", b"# Other\n")]);
    cx.simulate_keystrokes("secondary-\\");
    cx.simulate_input("Other");
    cx.simulate_keystrokes("enter");
    frames(cx);
    assert_eq!(shell.read_with(cx, |s, _| s.active), 1);
    shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
    frames(cx);
    open(&shell, cx, &root, "data.csv");
    shell.update(cx, |s, cx| s.open_note(&root.join("data.csv"), cx));
    frames(cx);
    assert!(shown(&shell, cx), "the note replaced the table in the second pane");
    assert_eq!(shell.read_with(cx, |s, _| s.active), 1);
    assert!(shell.read_with(cx, |s, _| s.panes[0].sheet().is_none() && s.panes[0].path.as_deref() == Some(root.join("Note.md").as_path())));
    cx.simulate_keystrokes("x enter");
    frames(cx);
    assert!(shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).table().row(0).join("|").contains('x')));
}
