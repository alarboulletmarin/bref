//! `@` dates, links on bare addresses, and list kinds.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // `@` propose des jours : celui qu'on choisit devient un lien vers sa note du jour.
    // Une adresse e-mail n'ouvre pas la liste.
    let tomorrow = date_name(markdown::day_of(markdown::day_number(today()) + 1));
    j.load(cx, "", 0);
    cx.simulate_input("voir @dem");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), format!("voir [[{tomorrow}]]"));
    j.load(cx, "", 0);
    cx.simulate_input("@2026-11-02");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "[[2026-11-02]]");
    j.load(cx, "", 0);
    cx.simulate_input("a@dem");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "a@dem\n");
    // Sur une adresse nue, Ctrl+K lui fait une place pour son texte.
    j.load(cx, "voir https://a.b/c ici", 9);
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input("la doc");
    assert_eq!(text(cx), "voir [la doc](https://a.b/c) ici");
    // Clic droit dans la note : son menu ; Échap le referme. Ses entrées font ce que fait
    // le clavier, et celles d'un lien agissent sur le lien visé.
    let view = cx.update(|window, _| window.viewport_size());
    cx.simulate_mouse_down(point(view.width / 2., view.height / 2.), MouseButton::Right, gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, _| s.menu.as_ref().is_some_and(|m| m.text == Some(None))));
    cx.simulate_keystrokes("escape");
    assert!(shell.read_with(cx, |s, _| s.menu.is_none()));
    cx.simulate_mouse_down(point(view.width / 2., view.height / 2.), MouseButton::Right, gpui::Modifiers::none());
    cx.write_to_clipboard(ClipboardItem::new_string(" !".into()));
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Paste, None, window, cx));
    cx.run_until_parked();
    // Collé là où le clic a posé le curseur.
    assert!(text(cx).contains(" !") && text(cx).replace(" !", "") == "voir [la doc](https://a.b/c) ici");
    let link = Link::Url("https://a.b/c".into());
    shell.update_in(cx, |s, window, cx| {
        s.menu = Some(nav::Menu { at: point(px(0.), px(0.)), target: None, text: Some(Some(link)), commented: false, block: false });
        s.menu_do(nav::Do::CopyAddress, None, window, cx)
    });
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("https://a.b/c"));
    // Sans sélection, et une adresse dans le presse-papiers : il ne manque que le texte.
    j.load(cx, "", 0);
    cx.write_to_clipboard(ClipboardItem::new_string("https://a.b".into()));
    cx.simulate_keystrokes("secondary-k");
    cx.simulate_input("ici");
    assert_eq!(text(cx), "[ici](https://a.b)");
    // Des lignes choisies deviennent une liste : au clavier, de genre en genre ; par le menu de la
    // note et par la palette, au genre voulu ; la sélection reste sur son texte, un geste s'annule d'un trait.
    j.load(cx, "un\ndeux\ntrois\n\nfin", 0);
    cx.simulate_keystrokes("shift-down shift-down shift-end");
    for want in ["- un\n- deux\n- trois\n\nfin", "1. un\n2. deux\n3. trois\n\nfin", "- [ ] un\n- [ ] deux\n- [ ] trois\n\nfin", "un\ndeux\ntrois\n\nfin"] {
        cx.simulate_keystrokes("secondary-shift-b");
        assert_eq!(text(cx), want);
    }
    assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).selected() == "un\ndeux\ntrois"));
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Numbered, None, window, cx));
    assert_eq!(text(cx), "1. un\n2. deux\n3. trois\n\nfin");
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("tâches");
    cx.simulate_keystrokes("down enter");
    assert_eq!(text(cx), "- [ ] un\n- [ ] deux\n- [ ] trois\n\nfin");
    // Encore le même genre : retour au texte. Annuler rend la liste d'avant.
    shell.update_in(cx, |s, window, cx| s.menu_do(nav::Do::Tasks, None, window, cx));
    assert_eq!(text(cx), "un\ndeux\ntrois\n\nfin");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "- [ ] un\n- [ ] deux\n- [ ] trois\n\nfin");
    // Sans sélection : la ligne du curseur seule, et un titre ou un bloc de code ne bouge pas.
    j.load(cx, "# T\n\n```\na\n```\nb", 0);
    cx.simulate_keystrokes("secondary-shift-b");
    assert_eq!(text(cx), "# T\n\n```\na\n```\nb");
    cx.simulate_keystrokes("secondary-end secondary-shift-b");
    assert_eq!(text(cx), "# T\n\n```\na\n```\n- b");
}
