//! The outline, status messages and links.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Note outline: Ctrl+Shift+O lists the headings, the current section's one
    // preselected. The browsed heading gets the cursor as a preview (two headings with the same name
    // stay distinct, the one in the code block is not one), Escape gives it back, Enter leaves it there.
    let plan = "# Un\n\ntexte\n\n## Deux\n\n```\n# code\n```\n\n## Deux\n\nfin\n";
    j.load(cx, plan, plan.find("texte").unwrap());
    let caret = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).position());
    cx.simulate_keystrokes("secondary-shift-o down");
    assert_eq!(caret(cx), plan.find("## Deux").unwrap());
    cx.simulate_keystrokes("down");
    assert_eq!(caret(cx), plan.rfind("## Deux").unwrap());
    cx.simulate_keystrokes("down");
    assert_eq!(caret(cx), 0);
    cx.simulate_keystrokes("escape");
    assert_eq!(caret(cx), plan.find("texte").unwrap());
    cx.simulate_keystrokes("secondary-shift-o");
    cx.simulate_input("deux");
    cx.simulate_keystrokes("enter");
    assert!(caret(cx) == plan.find("## Deux").unwrap() && shell.read_with(cx, |s, _| s.palette.is_none()));
    j.load(cx, "sans titre", 0);
    cx.simulate_keystrokes("secondary-shift-o");
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Info, "titres")));
    // Status messages: a click removes the one it targets, the others go by themselves after four
    // seconds; the one of an operation in progress stays, until the message that gives its outcome.
    shell.update(cx, |s, cx| s.say(Tone::Busy, "En cours", cx));
    let view = cx.update(|window, _| window.viewport_size());
    cx.simulate_click(point(view.width - px(60.), view.height - px(70.)), gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Info, "titres") && !s.told(Tone::Busy, "")));
    shell.update(cx, |s, cx| s.say(Tone::Busy, "En cours", cx));
    cx.executor().advance_clock(TOAST);
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.toasts.len() == 1 && s.told(Tone::Busy, "En cours")));
    shell.update(cx, |s, cx| s.say(Tone::Done, "Fini", cx));
    assert!(shell.read_with(cx, |s, _| s.toasts.len() == 1 && s.told(Tone::Done, "Fini")));
    cx.executor().advance_clock(TOAST);
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.toasts.is_empty()));

    // Links: an address pasted on a selection makes it a link; Ctrl+K on a link
    // selects its address, to retype; Ctrl+K on a selection waits for the address.
    j.load(cx, "voir la doc ici", 5);
    cx.simulate_keystrokes("shift-right shift-right shift-right shift-right shift-right shift-right");
    cx.write_to_clipboard(ClipboardItem::new_string("https://a.b/c\n".into()));
    cx.simulate_keystrokes("secondary-v");
    assert_eq!(text(cx), "voir [la doc](https://a.b/c) ici");
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input("https://x.y");
    assert_eq!(text(cx), "voir [la doc](https://x.y) ici");
    cx.simulate_keystrokes("secondary-z secondary-z");
    assert_eq!(text(cx), "voir la doc ici");
    j.load(cx, "mot", 0);
    cx.write_to_clipboard(ClipboardItem::new_string("rien".into()));
    cx.simulate_keystrokes("shift-right shift-right shift-right secondary-k");
    cx.simulate_input("https://z.fr");
    assert_eq!(text(cx), "[mot](https://z.fr)");
}
