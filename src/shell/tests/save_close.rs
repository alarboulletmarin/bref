//! Failed closes keep the owning tab, its live view and the way to retry.

use super::*;

fn open(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext, path: &Path) {
    shell.update_in(cx, |s, window, cx| s.open_from_nav(path, window, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn failed_close_of_an_inactive_tab_shows_it_in_its_pane(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "close-inactive");
    let alpha = root.join("Alpha.md");
    let beta = root.join("Beta.md");
    fs::write(&alpha, "# Alpha\n\noriginal").unwrap();
    fs::write(&beta, "# Beta\n").unwrap();
    open(&shell, cx, &alpha);
    fs::create_dir(root.join(".Alpha.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input(" ajout");
    let (tab, editor) = shell.read_with(cx, |s, _| (s.active, s.pane().editor.clone()));
    open(&shell, cx, &beta);
    cx.simulate_input("autre ajout");
    // Middle-click on an inactive tab must bring its failed editor into view.
    let at = cx.debug_bounds(leak(format!("tab-{tab}"))).unwrap().center();
    cx.simulate_event(gpui::MouseDownEvent { button: MouseButton::Middle, position: at, modifiers: gpui::Modifiers::none(), click_count: 1, first_mouse: false });
    cx.simulate_event(gpui::MouseUpEvent { button: MouseButton::Middle, position: at, modifiers: gpui::Modifiers::none(), click_count: 1 });
    assert_eq!(shell.read_with(cx, |s, _| (s.active, s.groups[0].shown)), (tab, tab));
    assert!(editor.read_with(cx, |e, _| e.text().ends_with(" ajout")));
    // Another dirty note can still close successfully while Alpha cannot save.
    open(&shell, cx, &beta);
    cx.simulate_keystrokes("secondary-w");
    assert!(fs::read_to_string(&beta).unwrap().contains("autre ajout"));
    assert!(shell.read_with(cx, |s, _| s.panes.iter().any(|p| p.editor == editor && p.dirty)));

    // The same failure in the other pane keeps that pane and moves input to it.
    shell.update_in(cx, |s, window, cx| { s.add_pane(Side::Right, window, cx); });
    let before = shell.read_with(cx, |s, _| s.panes.len());
    shell.update_in(cx, |s, window, cx| s.close_tab(tab, window, cx));
    shell.read_with(cx, |s, _| {
        assert_eq!(s.panes.len(), before);
        assert_eq!(s.pane().editor, editor);
        assert_eq!(s.groups[s.group()].shown, s.active);
    });
    shell.update_in(cx, |_, window, cx| assert!(editor.focus_handle(cx).is_focused(window)));
    fs::remove_dir(root.join(".Alpha.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-w");
    assert!(fs::read_to_string(alpha).unwrap().ends_with(" ajout"));
    let _ = fs::remove_dir_all(root);
}

#[gpui::test]
fn failed_close_preserves_empty_files_drafts_and_title_renames(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "close-note-limits");
    let anchor = root.join("Anchor.md");
    fs::write(&anchor, "# Anchor\n").unwrap();
    open(&shell, cx, &anchor);
    // An existing empty file is retained, even after a successful close.
    let empty = root.join("Empty.md");
    fs::write(&empty, "").unwrap();
    open(&shell, cx, &empty);
    cx.simulate_keystrokes("secondary-w");
    assert!(empty.exists());
    // An empty unnamed draft needs no write.
    cx.simulate_keystrokes("secondary-n secondary-w");
    assert_eq!(shell.read_with(cx, |s, _| s.panes.len()), 1);
    // An unnamed draft whose destination vanished stays unnamed with all its text.
    let dir = root.join("Gone");
    fs::create_dir(&dir).unwrap();
    shell.update_in(cx, |s, window, cx| s.new_note_in(Some(dir.clone()), window, cx));
    fs::remove_dir(&dir).unwrap();
    cx.simulate_input("# Brouillon\n\ntexte");
    cx.simulate_keystrokes("secondary-w");
    assert!(shell.read_with(cx, |s, cx| s.pane().path.is_none() && s.pane().dirty && s.pane().editor.read(cx).text().contains("texte")));
    fs::create_dir(&dir).unwrap();
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(fs::read_to_string(dir.join("Brouillon.md")).unwrap(), "# Brouillon\n\ntexte");
    // A failed title rename leaves the old file and links intact; a retry follows the title.
    let alpha = root.join("Alpha.md");
    let link = root.join("Link.md");
    fs::write(&alpha, "# Alpha\n\noriginal").unwrap();
    fs::write(&link, "[[Alpha]]").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    open(&shell, cx, &alpha);
    fs::create_dir(root.join(".Renamed.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-a");
    cx.simulate_input("# Renamed\n\nchangé");
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(fs::read_to_string(&alpha).unwrap(), "# Alpha\n\noriginal");
    assert_eq!(fs::read_to_string(&link).unwrap(), "[[Alpha]]");
    assert!(shell.read_with(cx, |s, _| s.pane().dirty && s.pane().path.as_ref() == Some(&alpha)));
    fs::remove_dir(root.join(".Renamed.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-w secondary-alt-t");
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(root.join("Renamed.md")));
    assert_eq!(fs::read_to_string(link).unwrap(), "[[Renamed]]");
    let _ = fs::remove_dir_all(root);
}

#[gpui::test]
fn failed_close_keeps_csv_and_tsv_edits_until_the_final_write(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "close-sheets");
    let anchor = root.join("Anchor.md");
    fs::write(&anchor, "# Anchor\n").unwrap();
    open(&shell, cx, &anchor);
    for (name, original) in [("data.csv", "a,b\n1,2\n"), ("data.tsv", "a\tb\n1\t2\n")] {
        let path = root.join(name);
        fs::write(&path, original).unwrap();
        open(&shell, cx, &path);
        let sheet = shell.read_with(cx, |s, _| s.shown_sheet().unwrap());
        let block = root.join(format!(".{name}.tmp"));
        fs::create_dir(&block).unwrap();
        // Close while a cell is still being typed: flush commits that input before saving.
        cx.simulate_input("modifié");
        cx.simulate_keystrokes("secondary-w");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(shell.read_with(cx, |s, _| s.shown_sheet()), Some(sheet.clone()));
        assert!(sheet.read_with(cx, |s, _| s.is_dirty() && s.table().row(0).join("|").contains("modifié")));
        assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, name)));
        // A background retry also fails without losing the data or the undo stack.
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert!(sheet.read_with(cx, |s, _| s.is_dirty()));
        cx.simulate_keystrokes("secondary-z secondary-shift-z");
        assert!(sheet.read_with(cx, |s, _| s.table().row(0).join("|").contains("modifié")));
        fs::remove_dir(&block).unwrap();
        // Close with a scheduled background save still pending. The final write wins.
        cx.simulate_keystrokes("secondary-w");
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert!(fs::read_to_string(&path).unwrap().contains("modifié"));
        assert!(shell.read_with(cx, |s, _| s.panes.iter().all(|p| p.sheet() != Some(&sheet))));
        cx.simulate_keystrokes("secondary-alt-t");
        assert!(shell.read_with(cx, |s, cx| s.shown_sheet().unwrap().read(cx).table().row(0).join("|").contains("modifié")));
        cx.simulate_keystrokes("secondary-w");
    }
    let _ = fs::remove_dir_all(root);
}

#[gpui::test]
fn failed_canvas_close_keeps_the_live_diagram_through_rescan(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "close-canvas");
    let anchor = root.join("Anchor.md");
    fs::write(&anchor, "# Anchor\n").unwrap();
    open(&shell, cx, &anchor);
    let path = root.join("Drawing.svg");
    fs::write(&path, Diagram::blank().to_svg(diagram::COLORS[0])).unwrap();
    open(&shell, cx, &path);
    let canvas = shell.read_with(cx, |s, _| s.pane().drawing().cloned().unwrap());
    let original = fs::read_to_string(&path).unwrap();
    fs::create_dir(root.join(".Drawing.tmp")).unwrap();
    cx.simulate_input("r");
    let (from, to) = canvas.read_with(cx, |c, _| (c.spot(0., 0.), c.spot(120., 60.)));
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shapes.len()), 1);
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(shell.read_with(cx, |s, _| s.pane().canvas_dirty && s.pane().drawing() == Some(&canvas) && s.told(Tone::Failed, "Drawing.svg")));
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    assert_eq!(shell.read_with(cx, |s, _| s.toasts.iter().filter(|t| t.tone == Tone::Failed && t.text.contains("Drawing.svg")).count()), 1);
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shapes.len()), 1);
    cx.simulate_keystrokes("secondary-z secondary-shift-z");
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shapes.len()), 1);
    fs::remove_dir(root.join(".Drawing.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-w secondary-alt-t");
    assert_eq!(Diagram::from_svg(&fs::read_to_string(path).unwrap()).unwrap().shapes.len(), 1);
    assert!(!shell.read_with(cx, |s, _| s.pane().canvas_dirty));
    let _ = fs::remove_dir_all(root);
}

#[gpui::test]
fn failed_close_button_and_window_close_keep_the_failed_document(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "close-buttons");
    let alpha = root.join("Alpha.md");
    let beta = root.join("Beta.md");
    fs::write(&alpha, "# Alpha\n").unwrap();
    fs::write(&beta, "# Beta\n").unwrap();
    open(&shell, cx, &alpha);
    fs::create_dir(root.join(".Alpha.tmp")).unwrap();
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input("à garder");
    let (tab, editor) = shell.read_with(cx, |s, _| (s.active, s.pane().editor.clone()));
    // Hover exposes the close cross even while the tab is dirty.
    shell.update(cx, |s, cx| { s.tab_hover = Some(tab); s.client = true; cx.notify(); });
    open(&shell, cx, &beta);
    let cross = cx.debug_bounds(leak(format!("tab-close-{tab}"))).unwrap().center();
    cx.simulate_click(cross, gpui::Modifiers::none());
    assert_eq!(shell.read_with(cx, |s, _| s.pane().editor.clone()), editor);
    assert!(shell.read_with(cx, |s, _| s.pane().dirty));
    // Native window close uses the same save result, including an inactive failed tab.
    open(&shell, cx, &beta);
    assert!(!cx.simulate_close());
    assert_eq!(shell.read_with(cx, |s, _| s.pane().editor.clone()), editor);
    cx.simulate_keystrokes("secondary-q");
    assert!(shell.read_with(cx, |s, cx| s.pane().dirty && s.pane().editor.read(cx).text().contains("à garder")));
    // Linux window controls used to remove the window directly.
    cx.run_until_parked();
    let close = cx.debug_bounds("close").unwrap().center();
    cx.simulate_click(close, gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.pane().dirty));
    // Switching vaults also closes tabs and must respect the failed write.
    let other = root.join("Other vault");
    fs::create_dir(&other).unwrap();
    shell.update_in(cx, |s, window, cx| s.set_vault(other, window, cx));
    assert_eq!(shell.read_with(cx, |s, _| s.vault.clone()), Some(root.clone()));
    assert_eq!(shell.read_with(cx, |s, _| s.pane().editor.clone()), editor);
    fs::remove_dir(root.join(".Alpha.tmp")).unwrap();
    assert!(cx.simulate_close());
    assert!(fs::read_to_string(alpha).unwrap().contains("à garder"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[gpui::test]
fn failed_close_in_a_read_only_or_vanished_directory(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (root, shell, cx, _config) = start(cx, "close-directory");
    let anchor = root.join("Anchor.md");
    fs::write(&anchor, "# Anchor\n").unwrap();
    open(&shell, cx, &anchor);
    let dir = root.join("Dossier éphémère");
    fs::create_dir(&dir).unwrap();
    let path = dir.join("Note.md");
    fs::write(&path, "original").unwrap();
    open(&shell, cx, &path);
    cx.simulate_keystrokes("secondary-end");
    cx.simulate_input(" changé");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(dir.join("probe"), "").is_err() {
        cx.simulate_keystrokes("secondary-w");
        assert!(shell.read_with(cx, |s, _| s.pane().dirty && s.pane().path.as_ref() == Some(&path)));
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    } else {
        eprintln!("read-only directory check skipped: permissions are not enforced for this user");
    }
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    // Move the entire directory away: the original file remains safe elsewhere.
    let moved = root.join("Moved");
    fs::rename(&dir, &moved).unwrap();
    cx.simulate_keystrokes("secondary-w");
    assert!(shell.read_with(cx, |s, cx| s.pane().dirty && s.pane().editor.read(cx).text() == "original changé"));
    assert_eq!(fs::read_to_string(moved.join("Note.md")).unwrap(), "original");
    fs::rename(&moved, &dir).unwrap();
    cx.simulate_keystrokes("secondary-w");
    assert_eq!(fs::read_to_string(path).unwrap(), "original changé");
    let _ = fs::remove_dir_all(root);
}
