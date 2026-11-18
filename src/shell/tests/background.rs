//! The window's background: opaque, translucent or blurred, chosen on the settings page. Only the ground lets the
//! desktop through, at the chosen opacity and never under the floor; the sheets stay opaque; kept across a restart.

use crate::platform::config;
use super::*;
use crate::shell::settings_page::Row;

#[gpui::test]
fn only_the_ground_lets_the_desktop_through(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "background");
    fs::write(root.join("Alpha.md"), "# Alpha\n\nText.\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open(&root.join("Alpha.md"), window, cx));
    cx.simulate_keystrokes("secondary-,");
    settle(cx);
    let alpha = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.theme.ground.a);
    let sheets = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| [s.theme.bg.a, s.theme.panel.a, s.theme.code_bg.a]);
    let click = |cx: &mut gpui::VisualTestContext, id: &str| {
        let at = cx.debug_bounds(leak(format!("settings-{id}"))).unwrap_or_else(|| panic!("{id} is drawn")).center();
        cx.simulate_click(at, gpui::Modifiers::none());
        settle(cx);
    };
    let saved = || config::load(config::File::Settings);

    // Opaque until chosen, nothing written for it.
    assert_eq!(alpha(cx), 1.);
    assert!(!saved().contains("background") && !saved().contains("opacity"));

    // Translucent: the ground at the default opacity, applied at once and saved; the sheets stay opaque.
    click(cx, "background-translucent");
    assert_eq!((alpha(cx), sheets(cx)), (Prefs::OPACITY, [1.; 3]));
    assert!(saved().contains("background=translucent\n") && !saved().contains("opacity"), "{}", saved());

    // Down 5 % at a time to the floor, where it stops; the value shown follows.
    let shown = |cx: &mut gpui::VisualTestContext| cx.debug_bounds("settings-opacity-value").is_some();
    for _ in 0..6 {
        click(cx, "opacity-less");
    }
    assert_eq!(alpha(cx), Prefs::FLOOR, "never under the floor");
    assert!(shown(cx) && saved().contains(&format!("opacity={}\n", Prefs::FLOOR)), "{}", saved());
    assert_eq!(sheets(cx), [1.; 3]);
    // Up to 100 % and no further; Default brings back 90 %.
    for _ in 0..8 {
        click(cx, "opacity-more");
    }
    assert_eq!(alpha(cx), 1.);
    click(cx, "opacity-reset");
    assert_eq!(alpha(cx), Prefs::OPACITY);
    assert!(!saved().contains("opacity"), "the default is not written: {}", saved());

    // The same with the keys: Left and Right on the opacity row.
    shell.update(cx, |s, _| s.settings.sel = Some(Row::Opacity));
    cx.simulate_keystrokes("left left");
    assert_eq!(alpha(cx), 0.8);
    cx.simulate_keystrokes("enter");
    assert_eq!(alpha(cx), Prefs::OPACITY, "Enter: the default");

    // Blurred: the same ground, and the system asked for its blur; where it has none, the page says so.
    click(cx, "background-blurred");
    assert!(shell.read_with(cx, |s, _| s.prefs.blurred()));
    assert_eq!(alpha(cx), Prefs::OPACITY);
    let warned = cx.debug_bounds("settings-background-no-blur").is_some();
    assert_eq!(warned, !crate::platform::offers_blur(), "the missing blur is said where the desktop has none");

    // Opaque again: the ground is whole. (gpui keeps the debug bounds of what is no longer drawn: the line about
    // blur leaving cannot be checked here.)
    click(cx, "background-opaque");
    assert_eq!(alpha(cx), 1.);
    assert!(saved().contains("background=opaque\n"));

    // A hand-written value under the floor, or unreadable, is brought back within bounds on opening the page.
    config::save(config::File::Settings, "background=translucent\nopacity=0.1\n").unwrap();
    cx.simulate_keystrokes("secondary-,");
    settle(cx);
    assert_eq!(alpha(cx), Prefs::FLOOR);
    config::save(config::File::Settings, "background=glass\nopacity=abc\n").unwrap();
    cx.simulate_keystrokes("secondary-,");
    settle(cx);
    assert_eq!(alpha(cx), 1., "an unknown background is opaque");

    // A restart reads it back (last: the second window shows the settings page too).
    config::save(config::File::Settings, "background=blurred\nopacity=0.8\n").unwrap();
    let (again, cx2) = cx.add_window_view({
        let root = root.to_path_buf();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx2.run_until_parked();
    assert_eq!(again.read_with(cx2, |s, _| (s.prefs.blurred(), s.theme.ground.a)), (true, 0.8));
}
