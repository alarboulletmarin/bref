//! Multiple cursors, comments and their panel, the badge, narrow windows.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Plusieurs curseurs : Ctrl+D prend le mot, puis ses occurrences suivantes ; la frappe,
    // l'effacement et les déplacements valent pour tous, un seul Ctrl+Z défait le tout,
    // Échap revient à un curseur. Alt+clic en pose un de plus.
    let cursors = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).cursors());
    j.load(cx, "un chat\nun chien\nun rat", 0);
    cx.simulate_keystrokes("secondary-d secondary-d secondary-d");
    assert_eq!(cursors(cx), 3);
    cx.simulate_keystrokes("secondary-d");
    assert_eq!(cursors(cx), 3);
    cx.simulate_input("le");
    assert_eq!(text(cx), "le chat\nle chien\nle rat");
    cx.simulate_keystrokes("end");
    cx.simulate_input("s !");
    cx.simulate_keystrokes("backspace backspace home delete");
    assert_eq!((text(cx), cursors(cx)), ("e chats\ne chiens\ne rats".to_string(), 3));
    cx.simulate_keystrokes("shift-end secondary-b");
    assert_eq!(text(cx), "**e chats**\n**e chiens**\n**e rats**");
    // Une étape d'annulation défait le geste pour les trois curseurs à la fois, et lui seul.
    cx.simulate_keystrokes("secondary-z");
    assert_eq!((text(cx), cursors(cx)), ("e chats\ne chiens\ne rats".to_string(), 1));
    j.load(cx, "un chat\nun chien\nun rat", 0);
    cx.simulate_keystrokes("secondary-d secondary-d escape");
    assert_eq!(cursors(cx), 1);
    cx.simulate_input("X");
    // Le dernier curseur posé reste : celui de la deuxième occurrence.
    assert_eq!(text(cx), "un chat\nX chien\nun rat");
    j.load(cx, "ab\ncd", 0);
    let middle = cx.update(|window, _| window.viewport_size());
    cx.simulate_mouse_down(point(middle.width / 2., middle.height / 2.), MouseButton::Left, gpui::Modifiers { alt: true, ..Default::default() });
    cx.simulate_mouse_up(point(middle.width / 2., middle.height / 2.), MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_input("X");
    assert_eq!((text(cx).matches('X').count(), cursors(cx)), (2, 2));
    cx.simulate_keystrokes("escape");
    // Commentaires : Ctrl+Maj+M commente la sélection (sans elle, la ligne, après sa puce),
    // la palette les liste et y mène, le même raccourci sur un commentaire le résout.
    j.load(cx, "- un mot ici\nfin", 5);
    cx.simulate_keystrokes("shift-right shift-right shift-right secondary-shift-m");
    cx.simulate_input("à revoir");
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\nfin");
    cx.simulate_keystrokes("down secondary-shift-m");
    cx.simulate_input("ok");
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\n{==fin==}{>>ok<<}");
    // Le panneau, à droite de la note : un clic sur une carte mène dans son commentaire,
    // sa coche le résout ; refermé, il revient quand la palette liste les commentaires.
    cx.run_until_parked();
    let view = cx.update(|window, _| window.viewport_size());
    cx.simulate_click(point(view.width - px(150.), px(125.)), gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, cx| s.editor.read(cx).position() == 17));
    shell.update(cx, |s, cx| s.editor.update(cx, |e, cx| e.resolve_comment(34, cx)));
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\nfin");
    cx.simulate_keystrokes("secondary-z");
    // Le bouton du coin, à gauche de la copie, referme le panneau (et le rouvre).
    cx.run_until_parked();
    let corner = cx.update(|window, _| window.viewport_size());
    // Deux commentaires : la pastille est au coin du bouton, dans la fenêtre.
    let badge = cx.debug_bounds("comments-badge").unwrap();
    assert!(badge.right() <= corner.width && badge.size.width >= px(15.), "{badge:?}");
    let closed = shell.read_with(cx, |s, _| s.comments_shut);
    cx.simulate_click(point(corner.width - px(270. + 65.), corner.height - px(31.)), gpui::Modifiers::none());
    assert!(!closed && shell.read_with(cx, |s, _| s.comments_shut));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("commentaires");
    cx.simulate_keystrokes("down enter");
    cx.simulate_input("revoir");
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, cx| s.palette.is_none() && !s.comments_shut && s.editor.read(cx).position() == 5));
    cx.simulate_keystrokes("secondary-shift-m");
    assert_eq!(text(cx), "- un mot ici\n{==fin==}{>>ok<<}");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\n{==fin==}{>>ok<<}");
    // La pastille aux limites : dix commentaires, douze, cent vingt. Elle ne grossit que vers la
    // gauche (son bord droit ne bouge pas), reste dans la fenêtre et loin du bouton de copie,
    // et sa hauteur ne change pas.
    let kept = text(cx);
    let many = |n: usize| format!("# T\n\n{}\n", (0..n).map(|i| format!("mot{{=={i}==}}{{>>c{i}<<}}")).collect::<Vec<_>>().join(" "));
    let mut seen = Vec::new();
    for n in [1, 9, 10, 12, 120] {
        j.load(cx, &many(n), 0);
        let (badge, toggle) = (cx.debug_bounds("comments-badge").unwrap(), cx.debug_bounds("comments-toggle").unwrap());
        let window = cx.update(|window, _| window.viewport_size());
        assert!(badge.right() <= window.width - px(47.), "{n} : la pastille touche le bouton de copie {badge:?}");
        assert!(badge.left() >= toggle.left() - px(12.) && badge.top() >= toggle.top() - px(4.) && badge.size.height == px(15.), "{n} : {badge:?} {toggle:?}");
        seen.push(badge);
    }
    // Un chiffre : ronde ; « 9+ » : plus large, au même bord droit, pareil pour 10, 12 et 120.
    assert!(seen[0].size.width == seen[1].size.width && seen[2].size.width > seen[1].size.width, "{seen:?}");
    assert!(seen.iter().all(|b| b.right() == seen[0].right()) && seen[2..].iter().all(|b| b.size.width == seen[2].size.width), "{seen:?}");
    j.load(cx, &kept, 0);
    // Fenêtre étroite ou panneau trop large : rien ne passe sous son voisin ni hors de la
    // fenêtre. La note garde 200 px, le panneau puis la colonne des commentaires lui cèdent.
    let before = shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel, s.nav.list_w));
    let whole = cx.update(|window, _| window.viewport_size());
    let lay = |cx: &mut gpui::VisualTestContext, width: f32, mode: Mode, panel: Panel, list: f32| {
        cx.simulate_resize(size(px(width), px(600.)));
        shell.update(cx, |s, cx| {
            (s.nav.mode, s.nav.panel, s.nav.list_w) = (mode, panel, px(list));
            cx.notify();
        });
        cx.run_until_parked();
    };
    let within = |cx: &mut gpui::VisualTestContext, part: &'static str, zone: &'static str| {
        let (part_at, zone_at) = (cx.debug_bounds(part).unwrap_or_else(|| panic!("{part} absent")), cx.debug_bounds(zone).unwrap());
        assert!(part_at.left() >= zone_at.left() && part_at.right() <= zone_at.right(), "{part} dépasse de {zone} : {part_at:?} hors de {zone_at:?}");
    };
    // Un panneau de 600 px dans une fenêtre de 860 : il rend la place de la note et des commentaires.
    lay(cx, 860., Mode::Tree, Panel::Split, 600.);
    let (panel, pane, said) = (cx.debug_bounds("nav-panel").unwrap(), cx.debug_bounds("pane-0").unwrap(), cx.debug_bounds("comments").unwrap());
    assert!(pane.size.width >= px(199.) && said.size.width == px(270.) && said.right() <= px(860.), "{pane:?} {said:?}");
    assert!(panel.right() <= pane.left() && pane.right() <= said.left(), "{panel:?} {pane:?} {said:?}");
    // 420 px avec le panneau : plus de place pour les commentaires, la note garde la sienne.
    lay(cx, 420., Mode::Tree, Panel::Split, 600.);
    let pane = cx.debug_bounds("pane-0").unwrap();
    assert!(pane.size.width >= px(199.) && pane.right() == px(420.), "{pane:?}");
    // Panneau replié : la colonne revient, plus étroite ; la recherche et le compteur tiennent dans la note.
    lay(cx, 420., Mode::Tree, Panel::Rail, 600.);
    let (pane, said) = (cx.debug_bounds("pane-0").unwrap(), cx.debug_bounds("comments").unwrap());
    assert!(pane.size.width >= px(199.) && said.size.width == px(180.) && said.right() <= px(420.), "{pane:?} {said:?}");
    cx.simulate_keystrokes("secondary-f");
    cx.run_until_parked();
    within(cx, "find-bar", "pane-0");
    within(cx, "counts", "pane-0");
    cx.simulate_keystrokes("escape");
    // Le calendrier ne descend pas sous 170 px, et son mois tient dans le panneau.
    lay(cx, 420., Mode::Calendar, Panel::Split, 120.);
    assert_eq!(cx.debug_bounds("nav-panel").unwrap().size.width, px(170.));
    within(cx, "calendar", "nav-panel");
    cx.simulate_resize(whole);
    shell.update(cx, |s, cx| {
        (s.nav.mode, s.nav.panel, s.nav.list_w) = before;
        cx.notify();
    });
    cx.run_until_parked();
    j.load(cx, "rien", 0);
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("commentaires");
    cx.simulate_keystrokes("down enter");
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Info, "commentaires")));
    shell.update(cx, |s, _| s.toasts.clear());
}
