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
    let open = shell.read_with(cx, |s, _| (s.active, s.layout.write(), s.panes.iter().map(|pane| pane.path.clone()).collect::<Vec<_>>()));
    assert_eq!(open, (0, "row(0.4 0, 0.6 1)".to_string(), vec![Some(left.clone()), Some(right.clone())]));
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

/// A long note at three widths: the text scrolls behind the band at the bottom of the note, which
/// holds the counter and the formatting bar or its button; going down line by line, the cursor
/// stays above the band, never under them.
#[gpui::test]
fn bottom_band_keeps_the_text_clear(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-band-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let lines: String = (0..200).map(|i| format!("Ligne {i} du texte\n")).collect();
    let note = root.join("Longue.md");
    fs::write(&note, format!("# Longue\n\n{lines}")).unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), vec![note.clone()], window, cx));
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    for width in [420., 860., 1400.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.simulate_keystrokes("secondary-home");
        settle(cx);
        for step in 0..45 {
            cx.simulate_keystrokes("down");
            settle(cx);
            let band = cx.debug_bounds("note-band").expect("the note has a band at its bottom");
            let counts = cx.debug_bounds("counts").unwrap();
            assert!(counts.top() >= band.top(), "{width} {step}: the counter sits in the band {counts:?} {band:?}");
            let (folded, caret) = shell.read_with(cx, |s, cx| {
                let editor = s.pane().editor.read(cx);
                (editor.bar_folded(), editor.point_of(editor.caret()).unwrap())
            });
            let control = cx.debug_bounds(if folded { "format-toggle" } else { "format-bar" }).unwrap();
            assert!(control.top() >= band.top(), "{width} {step}: the bar or its button sits in the band {control:?} {band:?}");
            assert!(caret.y + px(10.) <= band.top(), "{width} {step}: the cursor stays above the band {caret:?} {band:?}");
        }
    }
    let _ = fs::remove_dir_all(&root);
}

/// The new-version banner is a row of the window under the panes: it never covers the formatting bar (inline, or
/// folded and open with its colour badges), the counter or the note, whatever the width, the panel or the version.
#[gpui::test]
fn update_banner_stays_clear_of_the_formatting_bar(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-banner-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let note = root.join("Note.md");
    fs::write(&note, "un mot ici\n").unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), vec![note.clone()], window, cx));
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    let release = |version: &str| update::Release { version: version.into(), asset: Some("https://x/y".into()), sha256: None };
    let mut checked = 0;
    for panel in [Panel::Rail, Panel::Split] {
      for second_pane in [false, true] {
        if second_pane {
            cx.simulate_keystrokes("secondary-\\");
        }
        for width in [1400., 900., 860., 560., 420., 320., 240.] {
            for (version, updating) in [(None, false), (Some("0.4.0"), false), (Some("100.200.300"), false), (Some("0.4.0"), true)] {
                shell.update(cx, |s, cx| {
                    s.nav.panel = panel;
                    s.nav.list_w = px(600.);
                    s.update = version.map(release);
                    s.updating = updating;
                    cx.notify();
                });
                cx.simulate_resize(size(px(width), px(600.)));
                settle(cx);
                // Folded, the bar is open as well: its buttons and colour badges are the highest thing in the footer.
                let folded = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).bar_folded());
                if folded {
                    let at = cx.debug_bounds("format-toggle").unwrap().center();
                    cx.simulate_click(at, gpui::Modifiers::none());
                    settle(cx);
                    let at = cx.debug_bounds("format-colors").unwrap().center();
                    cx.simulate_click(at, gpui::Modifiers::none());
                    settle(cx);
                }
                let what = format!("{panel:?} two panes {second_pane} {width} {version:?} updating {updating} folded {folded}");
                // The pane with input: a window too small for two shows it alone.
                let group = shell.read_with(cx, |s, _| s.group());
                let window = cx.debug_bounds(leak(format!("pane-{group}"))).unwrap();
                let mut footer = vec![cx.debug_bounds("counts").unwrap(), cx.debug_bounds(if folded { "format-toggle" } else { "format-bar" }).unwrap()];
                if folded {
                    footer.extend(["format-bar", "format-swatches"].map(|name| cx.debug_bounds(name).unwrap()));
                }
                if version.is_some() {
                    let banner = cx.debug_bounds("update-banner").expect("the banner is drawn");
                    for part in &footer {
                        assert!(!banner.intersects(part), "{what}: the banner covers {part:?}, banner {banner:?}");
                    }
                    assert!(banner.top() >= window.bottom(), "{what}: the banner is under the note {banner:?} {window:?}");
                    assert!(banner.left() >= px(0.) && banner.right() <= px(width), "{what}: the banner fits the window {banner:?}");
                    assert!(banner.size.width > px(0.) && banner.size.height > px(20.), "{what}: {banner:?}");
                }
                checked += 1;
                // Close what was opened: the next round starts from a folded bar.
                if folded {
                    cx.simulate_keystrokes("escape");
                    settle(cx);
                }
            }
        }
        if second_pane {
            cx.dispatch_action(ClosePane);
            cx.run_until_parked();
        }
      }
    }
    assert_eq!(checked, 2 * 2 * 7 * 4);
    let _ = fs::remove_dir_all(&root);
}

/// Where the app draws the window buttons (Linux), they float over the top right corner: the panel header, on the whole window,
/// keeps its four buttons clear of them, and a narrow note starts under them, whatever the panel, the width or the language.
#[gpui::test]
fn window_buttons_leave_the_header_and_the_title_clear(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-pill-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    let note = root.join("Titre.md");
    fs::write(&note, "# Un titre très très très long qui passe à la ligne dans une fenêtre étroite\n\nTexte\n").unwrap();
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = Shell::new(Some(root.clone()), vec![note.clone()], window, cx);
        shell.client = true;
        shell
    });
    cx.run_until_parked();
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    let top = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).top_margin());
    // The panel on the whole window: its four buttons end where the window buttons begin, from a phone width to a very wide window.
    shell.update(cx, |s, cx| {
        s.nav.show_alone(nav::Mode::Tree);
        s.nav.panel = nav::Panel::Full;
        cx.notify();
    });
    for width in [320., 420., 860., 1600.] {
        cx.simulate_resize(size(px(width), px(600.)));
        settle(cx);
        let pill = cx.debug_bounds("window-pill").expect("the window buttons are drawn");
        let mut last = px(0.);
        for button in ["nav-file", "nav-folder", "nav-fold", "nav-full"] {
            let bounds = cx.debug_bounds(button).unwrap_or_else(|| panic!("{width}: {button} absent"));
            assert!(bounds.right() <= pill.left(), "{width}: {button} {bounds:?} under the window buttons {pill:?}");
            assert!(bounds.left() >= last, "{width}: {button} overlaps the one before");
            last = bounds.right();
        }
    }
    // The panel beside the note: a note narrowed by the panel (120, 260, 600 px) starts under the window buttons; in a window wide
    // enough for the page, the note starts under the handle strip.
    shell.update(cx, |s, cx| {
        s.nav.panel = nav::Panel::Split;
        cx.notify();
    });
    let pill_bottom = |cx: &mut gpui::VisualTestContext| cx.debug_bounds("window-pill").unwrap().bottom();
    for (window, panel) in [(860., 120.), (860., 260.), (860., 600.), (420., 120.), (1600., 260.), (1600., 600.)] {
        cx.simulate_resize(size(px(window), px(600.)));
        shell.update(cx, |s, cx| {
            s.nav.list_w = px(panel);
            cx.notify();
        });
        settle(cx);
        let (pane, pill, top) = (cx.debug_bounds("pane-0").unwrap(), pill_bottom(cx), top(cx));
        // Narrow: the pane, not the page measure, sets the width of the text.
        let narrow = pane.size.width < editor::MAX_WIDTH + px(48.);
        if narrow {
            assert!(pane.top() + top >= pill, "{window}/{panel}: the text starts at {top:?}, the window buttons end at {pill:?}");
        } else {
            assert_eq!(top, px(40.), "{window}/{panel}: a note wide enough starts under the window's handle strip");
        }
    }
    // No panel, the page is as wide as the window lets it: the default window starts under the handle strip, a narrow one gives room.
    shell.update(cx, |s, cx| {
        s.nav.panel = nav::Panel::Rail;
        cx.notify();
    });
    for (window, narrow) in [(860., false), (1600., false), (420., true), (320., true)] {
        cx.simulate_resize(size(px(window), px(600.)));
        settle(cx);
        let (pane, pill, top) = (cx.debug_bounds("pane-0").unwrap(), pill_bottom(cx), top(cx));
        // Wide enough: the text starts right under the handle strip, which is as high as the window buttons.
        assert!(pane.top() + top >= pill && (narrow || top == px(40.)), "{window}: the text starts at {top:?}, the window buttons end at {pill:?}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// Where the app draws the window buttons, no tab and no comment column goes under them: in a window of 420, 860
/// and 1 600 px, with eight tabs of long titles and a note that has comments, the tabs' row and the column's header
/// end before the buttons or start below them.
#[gpui::test]
fn window_buttons_have_the_top_row_to_themselves(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-pill-row-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let notes: Vec<_> = (0..8).map(|i| root.join(format!("Une note au titre très très long numéro {i}.md"))).collect();
    for note in &notes {
        fs::write(note, "# Titre\n\nUn {==mot==}{>>remarque<<} et du texte.\n").unwrap();
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
    let mut seen = 0;
    for width in [420., 860., 1600.] {
        cx.simulate_resize(size(px(width), px(600.)));
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        let pill = cx.debug_bounds("window-pill").expect("the window buttons are drawn");
        // The tabs scroll inside their row, which ends where the buttons begin.
        let row = cx.debug_bounds("tabs-row-0").expect("the tabs' row is drawn");
        assert!(row.right() <= pill.left(), "{width}: the tabs' row {row:?} runs under the window buttons {pill:?}");
        assert!(row.size.width > px(0.), "{width}: no room left for a tab");
        if let Some(shut) = cx.debug_bounds("comments-shut") {
            assert!(shut.top() >= pill.bottom() || shut.right() <= pill.left(), "{width}: the comments' header {shut:?} under the window buttons {pill:?}");
            seen += 1;
        }
    }
    assert!(seen > 0, "the comments column was never open");
    let _ = fs::remove_dir_all(&root);
}

/// Where the app draws its title bar, there is always a place to grab the window: in every layout, the top row of each pane
/// (tab bar past the last tab, or, without tab bar, a strip over the empty top of the note), the panel's header past its
/// title and the empty part of the rail. The text never starts under the strip.
#[gpui::test]
fn the_window_can_always_be_grabbed_at_the_top(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-grab-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (one, two) = (root.join("Une.md"), root.join("Deux.md"));
    fs::write(&one, "# Titre\n\nUn {==mot==}{>>remarque<<} et du texte.\n").unwrap();
    fs::write(&two, "# Autre\n\nTexte\n").unwrap();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = Shell::new(Some(root.clone()), vec![one.clone()], window, cx);
        shell.client = true;
        shell
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(860.), px(720.)));
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    };
    // The handles of the top row: bounds of the bare strip or of the bar's free part, with the widest one named.
    let handles = |cx: &mut gpui::VisualTestContext, group: usize| {
        let names = [["drag-strip-0", "tabs-free-0"], ["drag-strip-1", "tabs-free-1"]][group];
        names.map(|name| cx.debug_bounds(name)).into_iter().flatten().collect::<Vec<_>>()
    };
    let wide = px(200.);
    let (mut seen, mut report) = (0, Vec::new());
    let mut check = |what: &str, groups: usize, cx: &mut gpui::VisualTestContext| {
        settle(cx);
        for group in 0..groups {
            let pane = cx.debug_bounds(["pane-0", "pane-1"][group]).unwrap();
            // Bounds of an element that is no longer drawn stay in the table: only what lies inside the pane counts.
            let found: Vec<_> = handles(cx, group).into_iter().filter(|b| b.left() >= pane.left() - px(1.) && b.right() <= pane.right() + px(1.)).collect();
            let best = found.iter().map(|b| b.size.width).fold(px(0.), |a, b| a.max(b));
            assert!(best >= wide, "{what}: pane {group} has no handle of {wide:?} at its top ({found:?})");
            for handle in &found {
                assert!(handle.top() <= pane.top() + px(1.) && handle.bottom() <= pane.top() + px(41.), "{what}: {handle:?} not at the top of {pane:?}");
            }
            report.push(format!("{what} pane {group}: {best:?}"));
            seen += 1;
        }
    };
    let panel = |cx: &mut gpui::VisualTestContext, panel| {
        shell.update(cx, |s, cx| {
            s.nav.panel = panel;
            cx.notify();
        });
    };
    // One pane, one tab: no tab bar, the strip; the text starts under it.
    panel(cx, nav::Panel::Rail);
    check("one tab, rail", 1, cx);
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).top_margin()), px(40.), "the text starts under the strip");
    let rail = cx.debug_bounds("rail-free").expect("the rail has an empty part");
    assert!(rail.size.height > px(20.), "{rail:?}");
    // Panel open beside the note (the header's free part past the title), with the comment column open.
    panel(cx, nav::Panel::Split);
    check("one tab, panel", 1, cx);
    let (head, title) = (cx.debug_bounds("tool-head-tree").unwrap(), cx.debug_bounds("tool-title-tree").unwrap());
    assert!(head.right() > title.right() + px(40.) || head.size.width < title.size.width + px(40.), "{head:?} {title:?}");
    assert!(cx.debug_bounds("comments").is_some(), "the comments column is open");
    // Two tabs: the bar, free past the last tab.
    shell.update_in(cx, |s, window, cx| {
        s.add_tab(window, cx);
        s.open_note(&two, cx);
    });
    check("two tabs, panel", 1, cx);
    panel(cx, nav::Panel::Rail);
    check("two tabs, rail", 1, cx);
    // Two panes of one tab each.
    shell.update_in(cx, |s, window, cx| {
        s.close_tab(1, window, cx);
        s.open_aside(&two, window, cx);
    });
    check("two panes", 2, cx);
    panel(cx, nav::Panel::Split);
    check("two panes, panel", 2, cx);
    assert_eq!(seen, 8, "{report:?}");
    let _ = fs::remove_dir_all(&root);
}

/// Atelier: the notes and the panel are sheets on the ground, 8 px of it between two sheets and at the window's edges
/// (top, right, bottom), none between the rail and the panel. In every width, with the panel folded and open, with one
/// pane and two, and at the smallest and largest zoom.
#[gpui::test]
fn sheets_lie_on_the_ground_with_eight_pixels_between(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "ground");
    let (one, two) = (root.join("Une.md"), root.join("Deux.md"));
    fs::write(&one, "# Titre\n\nTexte\n").unwrap();
    fs::write(&two, "# Autre\n\nTexte\n").unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&one, cx);
    });
    let gap = nav::GAP;
    assert_eq!(gap, px(8.));
    let mut seen = 0;
    let mut check = |what: &str, width: f32, panels: bool, panes: usize, cx: &mut gpui::VisualTestContext| {
        cx.simulate_resize(size(px(width), px(720.)));
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        let sheets: Vec<_> = (0..panes).map(|i| cx.debug_bounds(["pane-0", "pane-1"][i]).unwrap_or_else(|| panic!("{what}: pane {i}"))).collect();
        for (i, sheet) in sheets.iter().enumerate() {
            assert_eq!((sheet.top(), sheet.bottom()), (gap, px(720.) - gap), "{what}: pane {i} {sheet:?}");
        }
        assert_eq!(sheets.last().unwrap().right(), px(width) - gap, "{what}: right edge {sheets:?}");
        for pair in sheets.windows(2) {
            assert_eq!(pair[1].left() - pair[0].right(), gap, "{what}: between the panes {pair:?}");
        }
        let rail = cx.debug_bounds("rail-free").expect("the rail is drawn");
        if panels {
            let panel = cx.debug_bounds("nav-panel").expect("the panel is drawn");
            assert_eq!(panel.left(), rail.right(), "{what}: the panel touches the rail's side {panel:?} {rail:?}");
            assert_eq!((panel.top(), panel.bottom()), (gap, px(720.) - gap), "{what}: panel {panel:?}");
            assert_eq!(sheets[0].left() - panel.right(), gap, "{what}: between the panel and the note {panel:?} {sheets:?}");
        } else {
            assert_eq!(sheets[0].left() - rail.right(), px(0.), "{what}: the note touches the rail's side");
        }
        seen += 1;
    };
    for width in [860., 1600., 420.] {
        shell.update(cx, |s, cx| {
            s.nav.panel = nav::Panel::Rail;
            cx.notify();
        });
        check(&format!("{width} folded"), width, false, 1, cx);
        shell.update(cx, |s, cx| {
            s.nav.panel = nav::Panel::Split;
            cx.notify();
        });
        check(&format!("{width} open"), width, true, 1, cx);
    }
    shell.update_in(cx, |s, window, cx| s.open_aside(&two, window, cx));
    shell.update(cx, |s, cx| {
        s.nav.panel = nav::Panel::Split;
        cx.notify();
    });
    for width in [860., 1600.] {
        check(&format!("{width} two panes"), width, true, 2, cx);
    }
    for keys in ["secondary-- ".repeat(5), "secondary-= ".repeat(21)] {
        cx.simulate_keystrokes(keys.trim_end());
        cx.run_until_parked();
        check("zoomed", 1600., true, 2, cx);
    }
    assert_eq!(seen, 10);
    let _ = fs::remove_dir_all(&root);
}

/// The interface follows the note's zoom, from 85 % to 150 % of its size at 16: the rail's buttons, the panel's
/// rows, the menus and the palette grow with it, their text with them.
#[gpui::test]
fn interface_follows_the_zoom(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "scale");
    fs::write(root.join("Alpha.md"), "# Alpha\n\nUn {==mot==}{>>remarque<<} et beaucoup de mots pour un long compteur.\n").unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Alpha.md"), cx);
    });
    cx.simulate_resize(size(px(1400.), px(900.)));
    cx.simulate_keystrokes("secondary-e");
    cx.run_until_parked();
    // Sizes of a button, a row of the tree, the row's menu and an entry of the command palette.
    let measure = |cx: &mut gpui::VisualTestContext| {
        let button = cx.debug_bounds("nav-file").unwrap().size.width;
        let row = cx.debug_bounds("nav-row-Alpha").unwrap().size.height;
        cx.simulate_keystrokes("shift-f10");
        cx.run_until_parked();
        let menu = cx.debug_bounds("menu").unwrap().size;
        cx.simulate_keystrokes("escape f10");
        cx.simulate_input("note du jour");
        cx.run_until_parked();
        let command = cx.debug_bounds("command-app::Today").unwrap().size.height;
        cx.simulate_keystrokes("escape secondary-e");
        cx.run_until_parked();
        [f32::from(button), f32::from(row), f32::from(menu.width), f32::from(menu.height), f32::from(command)]
    };
    let base = measure(cx);
    for (keys, size, scale) in [("secondary-= ".repeat(8), 24., 1.5), ("secondary-= ".repeat(6), 30., 1.5), ("secondary-0 ".into(), 16., 1.), ("secondary-- ".repeat(5), 11., 0.85)] {
        cx.simulate_keystrokes(keys.trim_end());
        cx.run_until_parked();
        assert_eq!(shell.read_with(cx, |s, _| s.theme.size), size);
        let now = measure(cx);
        // The note's foot: counter, comments button and copy button side by side, none over another.
        let foot = ["counts", "comments-toggle", "copy-all"].map(|name| cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} at {size}")));
        assert!(foot[0].right() <= foot[1].left() && foot[1].right() <= foot[2].left(), "at {size}: {foot:?}");
        for (i, (got, was)) in now.iter().zip(base).enumerate() {
            assert!((got / was - scale).abs() < 0.04, "at {size}: measure {i} is {got}, {was} at 16 (×{scale} expected)");
        }
    }
}

/// Every small target is at least 24 × 24 px: the window buttons, back and forward, the note's copy button, the
/// update banner's cross, "Reset all" and "default" in the help.
#[gpui::test]
fn small_targets_are_24_px(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "targets");
    fs::write(root.join("Alpha.md"), "# Alpha\n").unwrap();
    fs::write(root.join("keys"), "").unwrap();
    shell.update(cx, |s, cx| {
        s.client = true;
        s.rescan(&root, cx);
        s.open_note(&root.join("Alpha.md"), cx);
        s.update = Some(update::Release { version: "9.9.9".into(), ..Default::default() });
    });
    cx.simulate_resize(size(px(1400.), px(900.)));
    cx.run_until_parked();
    let mut seen = Vec::new();
    for name in ["minimize", "maximize", "close", "nav-back", "nav-forward", "copy-all", "update-close"] {
        let b = cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} is drawn"));
        seen.push((name, b.size));
    }
    // The help: "Reset all", and the "default" of a changed shortcut.
    shell.update(cx, |s, cx| s.key_set("app::OpenVault", "secondary-shift-k".into(), &[], cx));
    cx.simulate_keystrokes("f1");
    cx.run_until_parked();
    for name in ["keys-reset", "key-reset-app::OpenVault"] {
        let b = cx.debug_bounds(name).unwrap_or_else(|| panic!("{name} is drawn"));
        seen.push((name, b.size));
    }
    let small: Vec<_> = seen.iter().filter(|(_, s)| s.width < px(24.) || s.height < px(24.)).collect();
    assert!(small.is_empty(), "under 24 px: {small:?}");
}

/// Small clarity fixes: the help's table of contents, the sync button of a git vault, the theme's default that
/// says it follows the system, the line of the active pane.
#[gpui::test]
fn clear_at_a_glance(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "clarity");
    fs::write(root.join("Alpha.md"), "# Alpha\n").unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Alpha.md"), cx);
    });
    cx.simulate_resize(size(px(1400.), px(900.)));
    cx.run_until_parked();

    // The help: a link per section, which scrolls the list to it.
    cx.simulate_keystrokes("f1");
    cx.run_until_parked();
    let scroll = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.help.scroll.offset().y);
    assert!(cx.debug_bounds("help-contents").is_some());
    let last = shell.read_with(cx, |s, cx| s.help_rows(cx).len()) - 1;
    let link = cx.debug_bounds(leak(format!("help-to-{last}"))).expect("a link to the last section");
    assert!(link.size.height >= px(24.));
    cx.simulate_click(link.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(scroll(cx) < px(0.), "the list scrolled to the last section");
    // Sections far from the window are empty boxes; the ones in view, after the jump and after wheel steps up, are drawn.
    let drawn = |cx: &mut gpui::VisualTestContext| {
        let (view, boxes, rows) = shell.read_with(cx, |s, cx| {
            let boxes: Vec<_> = (0..s.help_rows(cx).len()).map(|i| s.help.scroll.bounds_for_item(i).unwrap()).collect();
            (s.help.scroll.bounds(), boxes, s.help_rows(cx))
        });
        let mid = rows.iter().zip(&boxes).filter(|(_, b)| b.top() < view.center().y && b.bottom() > view.center().y).collect::<Vec<_>>();
        assert!(!mid.is_empty(), "a section is at the middle of the list");
        for (section, _) in mid {
            let action = section.1.iter().find_map(|row| row.action).expect("a section with a shortcut");
            assert!(cx.debug_bounds(leak(format!("key-{action}"))).is_some(), "{} is drawn at the middle of the list", section.0);
        }
        view.center()
    };
    let centre = drawn(cx);
    for _ in 0..60 {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: centre,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(300.))),
            ..Default::default()
        });
        cx.run_until_parked();
        drawn(cx);
    }
    assert_eq!(scroll(cx), px(0.), "back at the top");
    cx.simulate_keystrokes("escape escape");

    // The sync button: only in a git vault; a mark while the last sync failed.
    assert!(cx.debug_bounds("nav-sync").is_none());
    fs::create_dir(root.join(".git")).unwrap();
    shell.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!(cx.debug_bounds("nav-sync").is_some() && cx.debug_bounds("nav-sync-broken").is_none());
    shell.update(cx, |s, cx| {
        s.sync_broken = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("nav-sync-broken").is_some());

    // The theme's default says what it is.
    cx.dispatch_action(ChooseTheme);
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, cx| s.palette.as_ref().and_then(|p| p.read(cx).choice_at(0))), Some("Par défaut : Bref, clair ou sombre comme le système".to_string()));
    cx.simulate_keystrokes("escape");

    // Two panes: a ring around the active one's sheet.
    cx.simulate_keystrokes("secondary-\\");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    let ring = cx.debug_bounds("pane-active").expect("the active pane is marked");
    let active = shell.read_with(cx, |s, _| s.active);
    let pane = cx.debug_bounds(leak(format!("pane-{active}"))).unwrap();
    assert_eq!(ring, pane, "the ring runs along the sheet's edge");
}

/// The rail reads as three groups (navigation, views, actions) then the bottom ones: in a 900, 720 and
/// 480 px high window the buttons come in that order, none overlaps the next, and the groups are a
/// button apart (at 480 px the rail scrolls, so the layout is the same).
#[gpui::test]
fn rail_groups_keep_their_order_and_room(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!("bref-rail-groups-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let _config = isolated_config(&root);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (_shell, cx) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx.run_until_parked();
    let order = [
        "nav-commands", "nav-back", "nav-forward", "nav-tree", "nav-recent", "nav-graph", "nav-tags", "nav-links", "nav-calendar", "nav-find", "nav-search", "nav-new", "nav-trash", "nav-help",
    ];
    for height in [900., 720., 480.] {
        cx.simulate_resize(size(px(860.), px(height)));
        for _ in 0..3 {
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
        let b: Vec<_> = order.iter().map(|id| cx.debug_bounds(id).unwrap_or_else(|| panic!("{id} is drawn at {height}"))).collect();
        for (pair, ids) in b.windows(2).zip(order.windows(2)) {
            assert!(pair[0].bottom() <= pair[1].top(), "{} overlaps {} at {height}", ids[0], ids[1]);
        }
        let gap = |from: &str, to: &str| {
            let at = |id: &str| b[order.iter().position(|o| *o == id).unwrap()];
            at(to).top() - at(from).bottom()
        };
        for (from, to) in [("nav-commands", "nav-back"), ("nav-forward", "nav-tree"), ("nav-find", "nav-search")] {
            assert!(gap(from, to) >= px(24.), "{from} and {to} are not set apart at {height}");
        }
        assert!(gap("nav-tree", "nav-recent") < px(8.), "views stay one list at {height}");
    }
}
