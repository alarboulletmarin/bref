//! What the app says when something goes wrong or nothing is there: failures that stay and name the file, the trash with its undo.

use super::*;

/// A failure stays 12 s (a success goes after 4), Esc or a click takes it away, and it says which file and why, in words.
#[gpui::test]
fn failures_stay_and_name_the_file(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "failures");
    let toasts = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.toasts.iter().map(|t| (t.tone, t.text.clone())).collect::<Vec<_>>());
    let secs = |cx: &mut gpui::VisualTestContext, n: u64| {
        cx.executor().advance_clock(Duration::from_secs(n));
        cx.run_until_parked();
    };
    shell.update(cx, |s, cx| {
        s.say(Tone::Done, "fait", cx);
        s.say(Tone::Failed, "raté", cx);
    });
    secs(cx, 5);
    assert_eq!(toasts(cx), [(Tone::Failed, "raté".to_string())], "the success went at 4 s, the failure stays");
    secs(cx, 5);
    assert_eq!(toasts(cx).len(), 1, "still there at 10 s");
    secs(cx, 3);
    assert!(toasts(cx).is_empty(), "gone after 12 s");
    // A long message is given time to be read.
    shell.update(cx, |s, cx| s.say(Tone::Done, "x".repeat(200), cx));
    secs(cx, 5);
    assert_eq!(toasts(cx).len(), 1, "200 characters are not read in 5 s");
    secs(cx, 10);
    assert!(toasts(cx).is_empty());
    // Esc takes the failures away, and only them; the note keeps the focus and its text.
    cx.simulate_input("abc");
    shell.update(cx, |s, cx| {
        s.say(Tone::Failed, "raté", cx);
        s.say(Tone::Info, "info", cx);
    });
    cx.simulate_keystrokes("escape");
    assert_eq!(toasts(cx), [(Tone::Info, "info".to_string())]);
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string()), "abc");
    // The reason is in words, whatever the system says, and the file is named.
    let ghost = root.join("Fantôme.md");
    shell.update(cx, |s, cx| s.trash(&root, &ghost, cx));
    let said = toasts(cx).pop().unwrap();
    assert_eq!(said.0, Tone::Failed);
    assert!(said.1.contains("Fantôme.md") && said.1.ends_with("introuvable"), "{said:?}");
    assert!(!said.1.contains("os error"), "{said:?}");
    let _ = fs::remove_dir_all(&root);
}

/// The trash says so, and its message undoes it: the note goes back where it was, a name taken meanwhile is refused.
#[gpui::test]
fn the_trash_says_so_and_can_be_undone(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "trash");
    fs::create_dir(root.join("Projets")).unwrap();
    let note = root.join("Projets/Plan.md");
    fs::write(&note, "# Plan\n").unwrap();
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..2 {
            cx.executor().advance_clock(Duration::from_secs(3));
            cx.run_until_parked();
        }
    };
    settle(cx);
    shell.update(cx, |s, cx| s.trash(&root, &note, cx));
    assert!(!note.exists() && root.join(".trash/Plan.md").is_file());
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "Mis à la corbeille : « Plan.md »")));
    // « Annuler » is on the message: a click puts the note back in its folder.
    cx.run_until_parked();
    let at = cx.debug_bounds("toast-undo").expect("the message offers to undo").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&note).unwrap(), "# Plan\n");
    assert!(!root.join(".trash/Plan.md").exists());
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "Remis en place : « Plan.md »") && !s.told(Tone::Done, "Mis à la corbeille")));
    // Trashed, then a note takes its name: the undo refuses, nothing is overwritten, both stay.
    shell.update(cx, |s, cx| s.trash(&root, &note, cx));
    fs::write(&note, "# Autre\n").unwrap();
    let (from, to) = (root.join(".trash/Plan.md"), note.clone());
    shell.update(cx, |s, cx| s.put_back(&from, &to, cx));
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "Remise en place impossible « Plan.md » : ce nom est déjà pris")));
    assert_eq!(fs::read_to_string(&note).unwrap(), "# Autre\n");
    assert_eq!(fs::read_to_string(&from).unwrap(), "# Plan\n");
    // Its folder gone meanwhile: it is made again.
    fs::remove_file(&note).unwrap();
    fs::remove_dir(root.join("Projets")).unwrap();
    shell.update(cx, |s, cx| s.put_back(&from, &to, cx));
    assert_eq!(fs::read_to_string(&note).unwrap(), "# Plan\n");
    // A folder goes to the trash and comes back whole; a name with accents, spaces and a bracket.
    fs::create_dir(root.join("Été [1]")).unwrap();
    fs::write(root.join("Été [1]/a b.md"), "# a b\n").unwrap();
    let dir = root.join("Été [1]");
    shell.update(cx, |s, cx| s.trash(&root, &dir, cx));
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Done, "« Été [1] »")));
    cx.run_until_parked();
    let at = cx.debug_bounds("toast-undo").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(fs::read_to_string(dir.join("a b.md")).unwrap(), "# a b\n");
    let _ = fs::remove_dir_all(&root);
}

/// A figure or a picture that cannot be read says so under its source, in words; a good one says nothing.
#[gpui::test]
fn unreadable_figures_and_pictures_say_so(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "unreadable");
    fs::write(root.join("cassée.png"), "ceci n'est pas une image").unwrap();
    fs::write(root.join("vide.png"), "").unwrap();
    let good = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20"/></svg>"#;
    fs::write(root.join("bonne.svg"), good).unwrap();
    fs::write(root.join("cassé.svg"), "<svg pas du svg").unwrap();
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    let said = |cx: &mut gpui::VisualTestContext, text: &str| {
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load(text.to_string(), text.len(), cx)));
        for _ in 0..6 {
            cx.executor().advance_clock(Duration::from_millis(500));
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        shell.read_with(cx, |s, cx| s.pane().editor.read(cx).unreadable())
    };
    let mermaid = "```mermaid\nsequenceDiagram\nA->>B: salut\n```\n";
    assert!(said(cx, mermaid).is_empty(), "a good diagram says nothing");
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).figure_count()), (1, 0), "and it is drawn");
    assert_eq!(said(cx, "```mermaid\nceci n'est pas un diagramme\n```\n"), ["Figure illisible"]);
    assert_eq!(said(cx, "avant\n\n```mermaid\n\n```\n\naprès"), ["Figure illisible"], "an empty diagram too");
    assert_eq!(said(cx, "$$\n\\frac{1}{\n$$\n"), ["Formule illisible"]);
    assert!(said(cx, "$$\n\\frac{1}{2}\n$$\n").is_empty());
    assert!(said(cx, "```rust\nfn main() {}\n```\n").is_empty(), "code is not a figure");
    assert_eq!(said(cx, "![](cassée.png)\n\n![](cassé.svg)\n\n![](bonne.svg)"), ["Image illisible", "Figure illisible"]);
    assert_eq!(said(cx, "![](vide.png)"), ["Image illisible"], "a file of 0 bytes");
    assert!(said(cx, "![](absente.png)").is_empty(), "an image that is not there is just text");
    // Two bad diagrams and a good one, in the order of the note; the cursor in the bad one waits until it leaves.
    let three = "```mermaid\nfaux\n```\n\n```mermaid\nsequenceDiagram\nA->>B: x\n```\n\n```mermaid\nfaux aussi\n```\n";
    assert_eq!(said(cx, three), ["Figure illisible", "Figure illisible"]);
    let _ = fs::remove_dir_all(&root);
}

/// A picture opened alone that cannot be read says so, instead of an empty pane for ever.
#[gpui::test]
fn a_corrupt_picture_opened_alone_says_so(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "corrupt");
    // A PNG of one pixel.
    let png: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50, 0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82];
    fs::write(root.join("bonne.png"), png).unwrap();
    fs::write(root.join("cassée.png"), "ceci n'est pas une image").unwrap();
    fs::write(root.join("vide.png"), "").unwrap();
    let open = |cx: &mut gpui::VisualTestContext, name: &str| {
        let path = root.join(name);
        // As when it is picked in the panel: the focus is there, not in the note, which would take over again.
        shell.update_in(cx, |s, window, cx| {
            s.open_note(&path, cx);
            window.focus(&s.nav.focus);
        });
        for _ in 0..6 {
            cx.executor().advance_clock(Duration::from_millis(500));
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        assert!(shell.read_with(cx, |s, _| s.pane().file_path().is_some()), "{name} is shown alone");
        cx.debug_bounds("image-said").is_some()
    };
    assert!(!open(cx, "bonne.png"), "a good picture says nothing");
    assert!(open(cx, "cassée.png"), "a corrupt one says so");
    assert!(open(cx, "vide.png"), "and a file of 0 bytes");
    let _ = fs::remove_dir_all(&root);
}

/// A vault that is gone (the config points to a missing folder) is said on the welcome page, narrow or wide, until another is opened.
#[gpui::test]
fn a_missing_vault_is_said_on_the_welcome_page(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-lost-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(None, Vec::new(), window, cx));
    cx.run_until_parked();
    assert!(!shell.read_with(cx, |s, _| s.lost.is_some()), "nothing is said at the very first launch");
    // A long path with spaces and accents, on a window as narrow as it gets.
    let gone = root.join("Disque externe é/Mes notes très très très longues/Coffre");
    shell.update(cx, |s, cx| {
        s.lost = Some(gone);
        cx.notify();
    });
    for width in [360., 860., 1600.] {
        cx.simulate_resize(size(px(width), px(640.)));
        cx.run_until_parked();
        let (said, page) = (cx.debug_bounds("welcome-lost").expect("the page says the vault is gone"), cx.debug_bounds("welcome-create").unwrap());
        assert!(said.left() >= px(0.) && said.right() <= px(width), "{width}: {said:?}");
        assert!(said.bottom() <= page.top(), "{width}: the message {said:?} sits above the buttons {page:?}");
    }
    // Opening a vault takes it away.
    let at = cx.debug_bounds("welcome-create").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.vault.is_some() && s.lost.is_none()));
    let _ = fs::remove_dir_all(&root);
}

/// An import says how many items it could not draw, in the singular and the plural, and nothing when all came.
#[gpui::test]
fn an_import_counts_what_it_leaves_out(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "import");
    let import = |cx: &mut gpui::VisualTestContext, name: &str, json: &str| {
        let file = root.join(".config").join(name);
        fs::write(&file, json).unwrap();
        shell.update_in(cx, |s, window, cx| {
            s.toasts.clear();
            s.import_file(&file, window, cx);
        });
        cx.run_until_parked();
        shell.read_with(cx, |s, _| s.toasts.iter().map(|t| (t.tone, t.text.clone())).collect::<Vec<_>>())
    };
    let ellipse = r#"{"id":"a","type":"ellipse","x":0,"y":0,"width":80,"height":60}"#;
    let stroke = |n: usize| (0..n).map(|i| format!(r#"{{"id":"s{i}","type":"freedraw"}}"#)).collect::<Vec<_>>().join(",");
    assert!(import(cx, "tout.excalidraw", &format!(r#"{{"elements":[{ellipse}]}}"#)).is_empty(), "nothing to say when everything came");
    assert_eq!(import(cx, "un.excalidraw", &format!(r#"{{"elements":[{ellipse},{}]}}"#, stroke(1))), [(Tone::Info, "1 élément non importé".to_string())]);
    assert_eq!(import(cx, "trois.excalidraw", &format!(r#"{{"elements":[{ellipse},{}]}}"#, stroke(3))), [(Tone::Info, "3 éléments non importés".to_string())]);
    // Only what cannot be drawn: an empty diagram, and the count says why it is empty.
    assert_eq!(import(cx, "traits.excalidraw", &format!(r#"{{"elements":[{}]}}"#, stroke(12))), [(Tone::Info, "12 éléments non importés".to_string())]);
    let broken = import(cx, "cassé.excalidraw", "pas du json");
    assert!(matches!(broken.as_slice(), [(Tone::Failed, said)] if said.starts_with("Import impossible")), "{broken:?}");
    let _ = fs::remove_dir_all(&root);
}

/// Frames until the layout settles.
fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

/// A list page of 300 notes builds only the rows near the window; scrolled to the end and back,
/// the row at the middle of the page is drawn at every step.
#[gpui::test]
fn a_long_list_page_draws_the_rows_in_view(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "long-list");
    let dir = root.join("Long");
    fs::create_dir(&dir).unwrap();
    for i in 0..300 {
        fs::write(dir.join(format!("Note {i:03}.md")), format!("---\nrang: {i}\n---\n# Note {i}\n")).unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open_listing(dir.clone(), window, cx));
    settle(cx);
    // gpui keeps the debug bounds of earlier frames: the row under the middle must be there, at its
    // place in this frame. Above it, the rows left out are a space (the first child after the titles).
    let drawn = |cx: &mut gpui::VisualTestContext| {
        let (view, first, row) = shell.read_with(cx, |s, _| {
            let scroll = &s.pane().listing.as_ref().unwrap().scroll;
            let first = scroll.bounds_for_item(3).unwrap();
            (scroll.bounds(), first + point(px(0.), scroll.offset().y), s.theme.px(crate::views::nav::ROW))
        });
        let i = (((view.center().y - first.top()) / row) as usize).min(299);
        let at = cx.debug_bounds(leak(format!("list-cell-{i}-1"))).unwrap_or_else(|| panic!("row {i} is drawn"));
        assert!(at.top() <= view.center().y && at.bottom() >= view.center().y, "row {i} at the middle of {view:?}: {at:?}");
        (view.center(), i, first.size.height)
    };
    let (centre, _, skipped) = drawn(cx);
    assert_eq!(skipped, px(0.), "nothing left out above the first row");
    let wheel = |cx: &mut gpui::VisualTestContext, dy: f32| {
        cx.simulate_event(gpui::ScrollWheelEvent { position: centre, delta: gpui::ScrollDelta::Pixels(point(px(0.), px(dy))), ..Default::default() });
        cx.run_until_parked();
    };
    let mut end = (0, px(0.));
    for _ in 0..40 {
        wheel(cx, -300.);
        let (_, i, skipped) = drawn(cx);
        end = (i, skipped);
    }
    assert!(end.0 > 250 && end.1 > px(0.), "at the end, the rows above are left out: {end:?}");
    for _ in 0..40 {
        wheel(cx, 300.);
        drawn(cx);
    }
    assert!(cx.debug_bounds("list-cell-0-1").is_some(), "back at the first row");
    let _ = fs::remove_dir_all(&root);
}

/// An empty panel says so, in each of its lists, inside the panel; the wide and the narrow window alike.
#[gpui::test]
fn an_empty_panel_says_so(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "empty-panel");
    shell.update(cx, |s, cx| {
        s.nav.panel = nav::Panel::Split;
        cx.notify();
    });
    for width in [420., 860., 1600.] {
        cx.simulate_resize(size(px(width), px(640.)));
        for mode in [nav::Mode::Tree, nav::Mode::Recent, nav::Mode::Tags, nav::Mode::Links] {
            shell.update(cx, |s, cx| {
                s.nav.show_alone(mode);
                cx.notify();
            });
            settle(cx);
            // The bounds of the text itself: a long French line wraps instead of running over the note.
            let said = cx.debug_bounds("nav-empty").unwrap_or_else(|| panic!("{width} {mode:?}: the panel says nothing"));
            let panel = cx.debug_bounds("nav-panel").unwrap();
            assert!(said.left() >= panel.left() && said.right() <= panel.right() && said.bottom() <= panel.bottom(), "{width} {mode:?}: {said:?} in {panel:?}");
            assert!(said.size.width < panel.size.width, "{width} {mode:?}: the text has no margin in {panel:?}");
        }
    }
    let _ = fs::remove_dir_all(&root);
}

/// A list that has something in it says nothing.
#[gpui::test]
fn a_panel_with_notes_says_nothing(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-full-panel-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Une.md"), "# Une\n\n#tag [[Une]]\n").unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), vec![], window, cx)
    });
    shell.update(cx, |s, cx| {
        s.nav.panel = nav::Panel::Split;
        s.nav.show_alone(nav::Mode::Tree);
        cx.notify();
    });
    settle(cx);
    assert!(cx.debug_bounds("nav-panel").is_some());
    assert!(cx.debug_bounds("nav-empty").is_none(), "a vault with a note has a tree");
    let _ = fs::remove_dir_all(&root);
}

/// The palette says when nothing matches; a result list says nothing.
#[gpui::test]
fn the_palette_says_when_nothing_matches(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "palette-empty");
    fs::write(root.join("Courses.md"), "# Courses\n\nlait\n").unwrap();
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.load("# Courses\n\nlait\n".into(), 0, cx)));
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    // Nothing typed says nothing; a command search that matches nothing is said, inside the palette.
    cx.simulate_keystrokes("secondary-p");
    cx.run_until_parked();
    assert!(cx.debug_bounds("palette-empty").is_none(), "nothing typed, nothing said");
    cx.simulate_input(">qqqqzzzz");
    cx.run_until_parked();
    let panel = cx.debug_bounds("palette-panel").unwrap();
    let said = cx.debug_bounds("palette-empty").expect("no result is said");
    assert!(said.left() >= panel.left() && said.right() <= panel.right(), "{said:?} in {panel:?}");
    cx.simulate_keystrokes("escape");
    let _ = fs::remove_dir_all(&root);
}

/// A list page with no row, empty or filtered to nothing, says so.
#[gpui::test]
fn an_empty_list_page_says_so(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "empty-list");
    fs::create_dir(root.join("Vide")).unwrap();
    let dir = root.join("Vide");
    shell.update_in(cx, |s, window, cx| s.open_listing(dir, window, cx));
    settle(cx);
    assert!(cx.debug_bounds("list-empty").is_some(), "a folder with no note");
    let _ = fs::remove_dir_all(&root);
}

/// A board with no column says so and offers the way to add one, instead of a lone « + ».
#[gpui::test]
fn an_empty_board_says_so(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "empty-board");
    let board = root.join("Kanban vide.md");
    fs::write(&board, "---\nkanban: true\n---\n# Kanban vide").unwrap();
    shell.update(cx, |s, cx| {
        s.open_note(&board, cx);
    });
    settle(cx);
    let said = cx.debug_bounds("board-empty").expect("the empty board says so");
    let pane = cx.debug_bounds("pane-0").unwrap();
    assert!(said.left() >= pane.left() && said.right() <= pane.right(), "{said:?} in {pane:?}");
    // Its button opens the field for the first column.
    assert!(shell.read_with(cx, |s, _| s.pane().board.field.is_none()));
    cx.simulate_click(gpui::point(said.center().x, said.center().y + px(14.)), gpui::Modifiers::none());
    settle(cx);
    assert!(shell.read_with(cx, |s, _| s.pane().board.field.is_some()), "a click on « Add a column » asks for its name");
    let _ = fs::remove_dir_all(&root);
}

/// A list page whose filter keeps no row says so (a fresh page: the other test left a folder with no note).
#[gpui::test]
fn a_filter_that_keeps_nothing_says_so(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "filtered-list");
    fs::create_dir(root.join("Livres")).unwrap();
    fs::write(root.join("Livres/Dune.md"), "---\nauteur: Herbert\n---\n# Dune\n").unwrap();
    for _ in 0..2 {
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
    let dir = root.join("Livres");
    shell.update_in(cx, |s, window, cx| s.open_listing(dir, window, cx));
    settle(cx);
    assert!(cx.debug_bounds("list-empty").is_none(), "one note, one row");
    cx.simulate_input("zzzz");
    settle(cx);
    assert!(cx.debug_bounds("list-empty").is_some(), "the filter keeps no row");
    let _ = fs::remove_dir_all(&root);
}

/// A vault that cannot be read says so instead of looking empty, at every width, and offers another vault; given
/// back, the notes come on the next scan. A folder inside that cannot be read is marked, the rest opens.
#[cfg(unix)]
#[gpui::test]
fn an_unreadable_vault_or_folder_says_so(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (root, shell, cx, _config) = start(cx, "unreadable-vault");
    let mode = |path: &Path, m: u32| fs::set_permissions(path, fs::Permissions::from_mode(m)).unwrap();
    // A failed assertion gives the rights back, so the next run can clear the folder.
    struct GiveBack(PathBuf);
    impl Drop for GiveBack {
        fn drop(&mut self) {
            for dir in [self.0.clone(), self.0.join("Interdit"), self.0.join("Comptes rendus des réunions de l'année")] {
                let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o755));
            }
        }
    }
    let _give_back = GiveBack(root.clone());
    fs::write(root.join("Note.md"), "# Note\n").unwrap();
    fs::create_dir(root.join("Interdit")).unwrap();
    fs::write(root.join("Interdit/Cachée.md"), "x").unwrap();
    mode(&root, 0o000);
    if fs::read_dir(&root).is_ok() {
        mode(&root, 0o755);
        return eprintln!("permissions are not enforced here (root?): skipped");
    }
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let page = |cx: &mut gpui::VisualTestContext| ["empty-key-0", "empty-key-1", "empty-key-2"].map(|key| cx.debug_bounds(key).unwrap().origin);
    let readable = page(cx);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_keystrokes("secondary-e");
    let reason = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.unreadable.clone());
    assert_eq!(reason(cx), [(root.clone(), std::io::ErrorKind::PermissionDenied)]);
    assert!(shell.read_with(cx, |s, _| s.nav.rows.is_empty()));
    // The empty page offers no note to write there: neither a new one nor the day's.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // (Bounds of a previous frame stay known: the keys that remain are measured.) The day's line is gone, the
    // keys come up; « shortcuts » takes the place of « new note ».
    let closed = page(cx);
    assert!(closed[0].y < readable[0].y, "{closed:?} after {readable:?}");
    assert_eq!(closed[2].x - closed[0].x, readable[1].x - readable[0].x);
    // The message stays inside the panel, a narrow window as a wide one.
    for width in [420., 860., 1600.] {
        cx.simulate_resize(size(px(width), px(720.)));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let (said, panel) = (cx.debug_bounds("nav-unreadable").expect("the panel says it"), cx.debug_bounds("nav-panel").unwrap());
        assert!(said.left() >= panel.left() && said.right() <= panel.right() && said.bottom() <= panel.bottom(), "{width}: {said:?} in {panel:?}");
        let other = cx.debug_bounds("nav-other-vault").expect("another vault is offered");
        // Centring rounds to the half pixel.
        assert!(other.top() >= said.top() && other.bottom() <= said.bottom() + px(1.), "{width}: {other:?} in {said:?}");
    }
    // Given back: the next scan reads it, the message goes.
    mode(&root, 0o755);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    assert!(reason(cx).is_empty());
    assert!(shell.read_with(cx, |s, _| s.nav.rows.iter().any(|r| r.name == "Note")));
    // A folder inside: the vault opens, the folder is there, marked; taken away while open, given back.
    let long = "Comptes rendus des réunions de l'année";
    fs::create_dir(root.join(long)).unwrap();
    mode(&root.join("Interdit"), 0o000);
    mode(&root.join(long), 0o000);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    let mut closed = reason(cx);
    closed.sort();
    assert_eq!(closed, [(root.join(long), std::io::ErrorKind::PermissionDenied), (root.join("Interdit"), std::io::ErrorKind::PermissionDenied)]);
    assert!(shell.read_with(cx, |s, _| s.nav.rows.iter().any(|r| r.name == "Interdit") && s.nav.rows.iter().any(|r| r.name == "Note")));
    // The reason is said in full at every width: the name gives way, not the reason.
    let mut full = None;
    for width in [1600., 860., 420.] {
        cx.simulate_resize(size(px(width), px(720.)));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let panel = cx.debug_bounds("nav-panel").unwrap();
        for name in ["Interdit", long] {
            let (row, why) = (cx.debug_bounds(leak(format!("nav-row-{name}"))).expect("the folder's row"), cx.debug_bounds(leak(format!("nav-why-{name}"))).unwrap());
            assert!(row.right() <= panel.right() && why.right() <= row.right(), "{width}: {why:?} in {row:?}");
            // In full where the panel has room for it, beside a long name too; cut last, in a panel of 175 px.
            let full = *full.get_or_insert(why.size.width);
            assert!(width < 860. || why.size.width == full, "{width} {name}: the reason is cut to {:?} from {full:?}", why.size.width);
        }
    }
    mode(&root.join("Interdit"), 0o755);
    mode(&root.join(long), 0o755);
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    assert!(reason(cx).is_empty());
    let _ = fs::remove_dir_all(&root);
}
