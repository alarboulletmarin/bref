//! The window around the notes: badge, format bar, saved shortcuts and panes, first launch, quick capture, empty page.

use super::*;
use crate::shell::render::badge_label;

#[test]
fn badge_shows_the_count_up_to_nine() {
    assert_eq!(["1", "9", "9+", "9+"], [badge_label(1), badge_label(9), badge_label(10), badge_label(120)]);
    assert_eq!(badge_label(0), "0");
}

#[test]
fn badge_text_reads_on_every_theme() {
    // The number is in the background colour, on the accent badge: it reads like a link's text.
    for (name, _) in THEMES {
        let prefs = Prefs { theme: name.to_string(), ..Prefs::parse("") };
        let t = Theme::of(&prefs, gpui::WindowAppearance::Light);
        assert!(contrast(t.accent, t.bg) >= 4.5, "{name}: {} on the badge", contrast(t.accent, t.bg));
    }
}

#[test]
fn folded_bar_button_hides_what_it_covers() {
    // The folded bar's button sits over text: opaque, closed or open, and its sign is readable, on every theme.
    for (name, _) in THEMES {
        let prefs = Prefs { theme: name.to_string(), ..Prefs::parse("") };
        let t = Theme::of(&prefs, gpui::WindowAppearance::Light);
        for open in [false, true] {
            let fill = editor::bar_fill(&t, open);
            assert!(fill.a == 1., "{name} {open}: translucent background {fill:?}");
            let sign = if open { t.accent } else { t.soft };
            assert!(contrast(sign, fill) >= 3., "{name} {open}: sign {} on the background", contrast(sign, fill));
        }
    }
}

/// Changed shortcuts are read again at launch; a faulty line in the file is set aside
/// and reported, the others apply.
#[gpui::test]
fn keys_survive_a_restart(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-keys-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    vault::save_file("keys", "nav::ShowTags=secondary-shift-y\nno::Such=f8\n");
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.changes.clone()), keys::Changes::from([("nav::ShowTags".to_string(), "secondary-shift-y".to_string())]));
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "no::Such=f8")));
    cx.simulate_keystrokes("secondary-t");
    assert_ne!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tags);
    cx.simulate_keystrokes("secondary-shift-y");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.mode), nav::Mode::Tags);
    // A file from before this version, or empty: the original shortcuts.
    let _ = fs::remove_dir_all(&root);
}

/// The second pane comes back at launch, with its width; a vanished file, or a config
/// from before panes, leaves a single pane.
#[gpui::test]
fn panes_come_back(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-panes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let (left, right) = (root.join("Gauche.md"), root.join("Droite.md"));
    fs::write(&left, "# Gauche\n").unwrap();
    fs::write(&right, "# Droite\n").unwrap();
    vault::save_file("panes", &format!("0.4\n{}\n", right.display()));
    cx.update(bind_keys);
    cx.update(init_fonts);
    // The right note is the last opened: the first pane takes the next one.
    let recent = vec![right.clone(), left.clone()];
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), recent, window, cx));
    cx.run_until_parked();
    let open = shell.read_with(cx, |s, _| (s.active, s.pane_ratio, s.panes.iter().map(|pane| pane.path.clone()).collect::<Vec<_>>()));
    assert_eq!(open, (0, 0.4, vec![Some(left.clone()), Some(right.clone())]));
    cx.simulate_input("x");
    assert_eq!(shell.read_with(cx, |s, cx| s.panes[0].editor.read(cx).text().to_string()), "# Gauche\nx");
    let _ = fs::remove_dir_all(&root);
}

/// Without a vault, the welcome screen creates one with a click, in `Notes` of the home folder.
#[gpui::test]
fn starts_in_a_new_vault(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-start-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(None, Vec::new(), window, cx));
    cx.run_until_parked();
    let at = cx.debug_bounds("welcome-create").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    let notes = root.join(".config").join("Notes");
    assert!(notes.is_dir());
    assert_eq!(shell.read_with(cx, |s, _| s.vault.clone()), Some(notes.clone()));
    // On Windows, the path is shown in full.
    if !cfg!(windows) {
        assert_eq!(vault::shown(&notes), "~/Notes");
    }
    let _ = fs::remove_dir_all(&root);
}

/// A note opens on its body: the first key does not break the front matter and does not
/// rename the note after its title, which would rewrite the other notes' links.
#[gpui::test]
fn opens_on_the_body(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-body-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let plan = root.join("Plan.md");
    fs::write(&plan, "---\ntags: [a]\n---\n# Plan\n\nÉtapes\n").unwrap();
    fs::write(root.join("Accueil.md"), "# Accueil\n\n[[Plan]]\n").unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (_shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), vec![plan.clone()], window, cx));
    cx.run_until_parked();
    cx.simulate_input("x");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&plan).unwrap(), "---\ntags: [a]\n---\n# Plan\n\nxÉtapes\n");
    assert_eq!(fs::read_to_string(root.join("Accueil.md")).unwrap(), "# Accueil\n\n[[Plan]]\n");

    // A new note written then emptied leaves nothing behind when leaving.
    cx.simulate_keystrokes("secondary-n");
    cx.simulate_input("Brouillon");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(root.join("Brouillon.md").is_file());
    cx.simulate_keystrokes("secondary-a backspace");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    let mut notes: Vec<_> = fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).filter(|n| n.to_string_lossy().ends_with(".md")).collect();
    notes.sort();
    assert_eq!(notes, ["Accueil.md", "Plan.md"]);
    let _ = fs::remove_dir_all(&root);
}

/// Capture (`bref --capture`): a line typed, Enter, it is in the daily note
/// and the window has closed. Escape closes without writing anything.
#[gpui::test]
fn capture_window(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-capture-window-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let note = root.join(format!("{}.md", date_name(today())));
    cx.update(bind_keys);
    cx.update(init_fonts);
    let closed = std::rc::Rc::new(std::cell::Cell::new(0));
    cx.update(|cx| {
        let closed = closed.clone();
        cx.on_window_closed(move |_| closed.set(closed.get() + 1)).detach();
    });
    {
        let (capture, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
        cx.run_until_parked();
        // An input field suggests nothing, even before the first keystroke.
        assert_eq!(capture.read_with(cx, |c, cx| c.field.read(cx).listed()), 0);
        cx.simulate_input("rappeler le plombier");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
    }
    assert_eq!(fs::read_to_string(&note).unwrap(), format!("# {}\n\n- rappeler le plombier\n", date_name(today())));
    assert_eq!(closed.get(), 1);
    {
        let (_, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
        cx.run_until_parked();
        cx.simulate_input("à oublier");
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    }
    assert!(!fs::read_to_string(&note).unwrap().contains("oublier"));
    assert_eq!(closed.get(), 2);
    // A very long line: nothing is cut or refused. The text scrolls under the cursor,
    // which stays in the frame, while typing as when going back to the start of the line.
    let long = "mot ".repeat(250);
    {
        let (capture, cx) = cx.add_window_view(|window, cx| Capture::new(root.clone(), window, cx));
        cx.run_until_parked();
        let caret_inside = |cx: &mut gpui::VisualTestContext| capture.read_with(cx, |c, cx| c.field.read(cx).caret_inside());
        cx.simulate_input(&long);
        cx.run_until_parked();
        assert_eq!(caret_inside(cx), Some(true), "the cursor follows the typing");
        cx.simulate_keystrokes("home");
        cx.run_until_parked();
        assert_eq!(caret_inside(cx), Some(true), "the cursor comes back into view at the start");
        cx.simulate_keystrokes("end enter");
        cx.run_until_parked();
    }
    assert!(fs::read_to_string(&note).unwrap().ends_with(&format!("- {}\n", long.trim())));
    assert_eq!(closed.get(), 3);
    let _ = fs::remove_dir_all(&root);
}

/// The empty page: today's date, the useful keys and the notes to revisit, under the line
/// where one writes. Everything fits in a narrow window, and disappears on the first keystroke.
#[gpui::test]
fn empty_page(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let week = date_name(markdown::day_of(markdown::day_number(today()) - 7));
    let (daily, old) = (root.join(format!("{week}.md")), root.join("Ancienne.md"));
    fs::write(&daily, format!("# {week}\n")).unwrap();
    fs::write(&old, "# Ancienne\n").unwrap();
    let long_ago = std::time::SystemTime::now() - Duration::from_secs(100 * 86_400);
    fs::File::options().write(true).open(&old).unwrap().set_modified(long_ago).unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.pane().path.clone());
    let text_of = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    assert_eq!(open(cx), None);

    // The logo is in the rail, whatever the system.
    assert!(cx.debug_bounds("rail-logo").is_some());
    // The notes to revisit: the daily note from a week ago, then an old note.
    assert_eq!(shell.read_with(cx, |s, _| s.recalled().iter().map(|r| r.name.clone()).collect::<Vec<_>>()), [week.clone(), "Ancienne".to_string()]);
    // In the narrowest window, nothing overflows the pane.
    cx.simulate_resize(size(px(360.), px(640.)));
    cx.run_until_parked();
    let pane = cx.debug_bounds("pane-0").unwrap();
    assert!(pane.size.width < px(360.), "{pane:?}");
    for part in ["empty-today", "empty-key-0", "empty-key-1", "empty-key-2", "empty-recall-0", "empty-recall-1"] {
        let bounds = cx.debug_bounds(part).unwrap_or_else(|| panic!("{part} absent"));
        assert!(bounds.left() >= pane.left() && bounds.right() <= pane.right(), "{part} overflows: {bounds:?} outside {pane:?}");
    }
    cx.simulate_resize(size(px(860.), px(720.)));
    cx.run_until_parked();

    // A click on a note to revisit opens it.
    let at = cx.debug_bounds("empty-recall-1").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(open(cx), Some(old.clone()));
    // A note with text does not show the empty page: where the date was, the click
    // lands in the note. (gpui keeps the bounds of a vanished element: we click.)
    cx.run_until_parked();
    let at = cx.debug_bounds("empty-today").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(open(cx), Some(old.clone()));

    // A key of the empty page can be clicked: here the palette.
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    let at = cx.debug_bounds("empty-key-0").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.palette.is_some()));
    cx.simulate_keystrokes("escape");

    // The date opens the daily note.
    let at = cx.debug_bounds("empty-today").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(open(cx), Some(root.join(format!("{}.md", date_name(today())))));

    // The first keystroke clears everything: the page belongs to what is written.
    cx.simulate_keystrokes("secondary-n");
    cx.run_until_parked();
    cx.simulate_input("x");
    cx.run_until_parked();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(text_of(cx), "x");
    let _ = fs::remove_dir_all(&root);
}
