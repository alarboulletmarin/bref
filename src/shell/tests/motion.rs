//! Motion: short fades when the panel, the format bar or a folder opens; `motion=off` makes them instant.

use super::*;

#[gpui::test]
fn fades_never_move_or_delay_anything(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "motion");
    fs::create_dir_all(root.join("Dossier")).unwrap();
    fs::write(root.join("Dossier").join("Dedans.md"), "# Dedans\n").unwrap();
    fs::write(root.join("Alpha.md"), "# Alpha\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    let open = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.left_open());

    // Two toggles faster than the fade end where they began, three on the other side.
    let before = open(cx);
    for gestures in 1..=3 {
        shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Tree, true, window, cx));
        if gestures == 2 {
            cx.run_until_parked();
            assert_eq!(open(cx), before, "two quick toggles: back where it was");
        }
    }
    cx.run_until_parked();
    assert_ne!(open(cx), before, "three quick toggles: the other side");
    if !open(cx) {
        shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Tree, true, window, cx));
        cx.run_until_parked();
    }
    assert!(open(cx));

    // The panel is the same size with fades on and off: a fade is opacity only.
    let panel = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        cx.debug_bounds("nav-panel").expect("panel")
    };
    let on = panel(cx);
    assert!(shell.read_with(cx, |s, _| s.theme.motion), "fades are on by default");
    let choose = |cx: &mut gpui::VisualTestContext, keys: &str| {
        shell.update_in(cx, |s, window, cx| s.choose_setting(Setting::Motion, window, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
    };
    choose(cx, "down enter");
    assert!(shell.read_with(cx, |s, _| s.prefs.motion == "off" && !s.theme.motion));
    assert!(vault::load_settings().contains("motion=off\n"));
    assert_eq!(panel(cx), on);
    // Escape takes the previewed choice back; the first choice turns them on again.
    choose(cx, "up escape");
    assert!(shell.read_with(cx, |s, _| !s.theme.motion));
    choose(cx, "up enter");
    assert!(shell.read_with(cx, |s, _| s.theme.motion && s.prefs.motion == "on"));

    // A folder unfolded: its rows (not itself, not the others) fade for `FADE`; a second unfolding
    // before the end takes over, and nothing is left fading afterwards.
    let (dir, other) = (root.join("Dossier"), root.join("Alpha.md"));
    let fading = |cx: &mut gpui::VisualTestContext, path: &Path| shell.read_with(cx, |s, _| s.nav.fading(path));
    shell.update(cx, |s, _| s.nav.unfold(dir.clone()));
    assert_eq!(fading(cx, &dir.join("Dedans.md")), Some(0));
    assert_eq!((fading(cx, &dir), fading(cx, &other)), (None, None));
    shell.update(cx, |s, _| s.nav.unfold(dir.clone()));
    assert_eq!(fading(cx, &dir.join("Dedans.md")), Some(1));
    std::thread::sleep(crate::ui::widgets::FADE + std::time::Duration::from_millis(20));
    assert_eq!(fading(cx, &dir.join("Dedans.md")), None);
    cx.run_until_parked();
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_setting_reads_absent_empty_and_unknown() {
    for (text, motion) in [("", true), ("motion=\n", true), ("motion=banana\n", true), ("motion=on\n", true), ("motion=off\n", false)] {
        let prefs = crate::logic::prefs::Prefs::parse(text);
        assert_eq!(crate::ui::theme::Theme::of(&prefs, gpui::WindowAppearance::Light).motion, motion, "{text:?}");
    }
}
