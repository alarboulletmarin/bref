//! The outline, status messages and links.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Plan de la note : Ctrl+Maj+O liste les titres, celui de la section en cours
    // présélectionné. Le titre parcouru reçoit le curseur en aperçu (deux titres de même nom
    // restent distincts, celui du bloc de code n'en est pas un), Échap le rend, Entrée l'y laisse.
    let plan = "# Un\n\ntexte\n\n## Deux\n\n```\n# code\n```\n\n## Deux\n\nfin\n";
    j.load(cx, plan, plan.find("texte").unwrap());
    let caret = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).position());
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
    // Messages d'état : un clic retire celui qu'il vise, les autres partent seuls après quatre
    // secondes ; celui d'une opération en cours reste, jusqu'au message qui en donne l'issue.
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

    // Liens : une adresse collée sur une sélection en fait un lien ; Ctrl+K sur un lien
    // sélectionne son adresse, qu'on retape ; Ctrl+K sur une sélection attend l'adresse.
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
