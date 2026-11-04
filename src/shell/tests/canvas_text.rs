//! Writing in a diagram shape: arrows, Backspace and Delete go by displayed
//! character, so a joined emoji, a flag, a skin tone or a decomposed accent
//! never leaves a fragment behind.

use super::*;
use crate::logic::diagram::Diagram;
use gpui::{Modifiers, MouseButton};

#[gpui::test]
fn label_editing_goes_by_displayed_character(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "canvas-text");
    let file = root.join("Schéma.svg");
    fs::write(&file, "<svg><metadata id=\"bref-diagram\">\nversion 1\n</metadata></svg>\n").unwrap();
    let canvas = shell.update_in(cx, |s, window, cx| {
        s.load_note(&file, cx);
        let canvas = s.pane().drawing().cloned().unwrap();
        window.focus(&canvas.focus_handle(cx));
        canvas
    });
    cx.run_until_parked();
    cx.simulate_input("r");
    let (from, to) = canvas.read_with(cx, |c, _| (c.spot(0., 0.), c.spot(120., 60.)));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    let label = |cx: &mut gpui::VisualTestContext| canvas.read_with(cx, |c, _| c.diagram().shapes[0].text.clone());

    // The reported case: one Backspace takes the whole joined emoji.
    cx.simulate_keystrokes("enter");
    cx.simulate_input("A👩\u{200d}💻");
    cx.simulate_keystrokes("backspace");
    assert_eq!(label(cx), "A");
    // Two flags side by side are two characters: one Backspace takes one.
    cx.simulate_input("🇫🇷🇯🇵");
    cx.simulate_keystrokes("backspace");
    assert_eq!(label(cx), "A🇫🇷");
    // Empty text: nothing to move over or erase, no panic.
    cx.simulate_keystrokes("backspace backspace backspace left right delete");
    assert_eq!(label(cx), "");

    let clusters = ["👩\u{200d}💻", "👨\u{200d}👩\u{200d}👧\u{200d}👦", "🇫🇷", "👍🏽", "e\u{301}", "é", "家", "a"];
    for cluster in clusters {
        // Next to a newline, at the end, at the start, beside CJK.
        cx.simulate_input(&format!("{cluster}\n{cluster}x"));
        cx.simulate_keystrokes("backspace backspace");
        assert_eq!(label(cx), format!("{cluster}\n"), "{cluster}");
        cx.simulate_keystrokes("left left delete");
        assert_eq!(label(cx), "\n", "{cluster}");
        cx.simulate_keystrokes("right backspace");
        assert_eq!(label(cx), "", "{cluster}");
        cx.simulate_input(&format!("家{cluster}家"));
        cx.simulate_keystrokes("left left backspace");
        assert_eq!(label(cx), format!("{cluster}家"), "{cluster}");
        cx.simulate_keystrokes("right right backspace backspace");
        assert_eq!(label(cx), "", "{cluster}");
    }

    // What is left is saved whole, and undo and redo bring it back as it was.
    cx.simulate_input("Café 👩\u{200d}💻🇫🇷!");
    cx.simulate_keystrokes("backspace backspace escape");
    cx.run_until_parked();
    let saved = || Diagram::from_svg(&fs::read_to_string(&file).unwrap()).unwrap().shapes[0].text.clone();
    assert_eq!((label(cx), saved()), ("Café 👩\u{200d}💻".into(), "Café 👩\u{200d}💻".into()));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(label(cx), "");
    cx.simulate_keystrokes("secondary-shift-z");
    assert_eq!(label(cx), "Café 👩\u{200d}💻");
}
