use super::*;
use gpui::{Bounds, Modifiers, Pixels, VisualTestContext};

/// `Ctrl` over a wikilink shows the linked note in a card under the link, without opening it: from a heading for
/// `[[Note#Heading]]`, through an alias, in a table cell, for the note itself. The card stays while the pointer
/// is on the link or on the card, `Ctrl` released or not, and goes when it leaves both, on a scroll, on typing
/// or on Escape. Its title opens the note; a missing note offers « Create »; a link inside the card asks for no
/// card of its own. From the keyboard, the palette's command previews the link under the cursor.
#[gpui::test]
fn ctrl_over_a_link_previews_the_note(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "link-preview");
    fs::write(root.join("Cible.md"), "# Cible\n\nPremière ligne de la cible\n\n## Partie\n\nDans la partie, [[Source]]\n").unwrap();
    fs::write(root.join("Nommée.md"), "---\naliases: [Surnom]\n---\n# Nommée\n\nLe texte de la note nommée\n").unwrap();
    let source = "# Source\n\nVoir [[Cible]] puis [[Cible#Partie]] et [[Absente]].\n\nMoi : [[Source]], [[Surnom]], [[Cible#Rien]].\n\n| a | b |\n| --- | --- |\n| x [[Nommée]] | y |\n";
    fs::write(root.join("Source.md"), source).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    shell.update_in(cx, |s, window, cx| {
        s.open(&root.join("Source.md"), window, cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    settle(cx);
    let (none, ctrl) = (Modifiers::none(), Modifiers::secondary_key());
    let editor = shell.read_with(cx, |s, _| s.pane().editor.clone());
    // A point inside the `n`th `needle` of the note, in the window.
    let spot = |cx: &mut VisualTestContext, needle: &str, n: usize| {
        let at = editor.read_with(cx, |e, _| e.text().match_indices(needle).nth(n).unwrap().0) + 3;
        editor.read_with(cx, |e, _| e.point_of(at).unwrap())
    };
    let seen = |cx: &mut VisualTestContext| shell.read_with(cx, |s, cx| s.peek_seen(cx));
    let card = |cx: &mut VisualTestContext| cx.debug_bounds("peek");
    let window = |cx: &mut VisualTestContext| cx.update(|window, _| Bounds::new(point(px(0.), px(0.)), window.viewport_size()));
    let inside = |inner: Bounds<Pixels>, outer: Bounds<Pixels>| inner.left() >= outer.left() - px(0.5) && inner.right() <= outer.right() + px(0.5) && inner.top() >= outer.top() - px(0.5) && inner.bottom() <= outer.bottom() + px(0.5);

    // Without `Ctrl`, reading never pops anything up.
    let link = spot(cx, "[[Cible]]", 0);
    cx.simulate_mouse_move(link, None, none);
    settle(cx);
    assert_eq!(seen(cx), None);
    // With it: the note under its title, its first lines, the card under the link and in the window.
    cx.simulate_mouse_move(link, None, ctrl);
    settle(cx);
    let (target, text) = seen(cx).expect("a card");
    assert_eq!(target, "Cible");
    assert_eq!(text.as_deref(), Some("Première ligne de la cible\n\n## Partie\n\nDans la partie, [[Source]]"));
    let shown = card(cx).expect("the card is drawn");
    assert!(shown.top() >= link.y && shown.top() <= link.y + px(30.), "{shown:?} under {link:?}");
    assert!(inside(shown, window(cx)), "{shown:?}");
    let title = cx.debug_bounds("peek-title").expect("its title");
    assert!(inside(title, shown), "{title:?} in {shown:?}");
    // A short note: the card is no taller than it needs.
    assert!(shown.size.height < px(300.), "{shown:?}");
    // `Ctrl` let go, along the link, then into the card: it stays.
    cx.simulate_modifiers_change(none);
    cx.simulate_mouse_move(link + point(px(10.), px(0.)), None, none);
    cx.simulate_mouse_move(shown.center(), None, none);
    settle(cx);
    assert!(seen(cx).is_some());
    // A link inside the card, with `Ctrl`: no card on the card, the first one stays.
    let reader = shell.read_with(cx, |s, _| s.peek_reader()).expect("a reader");
    let inner = reader.read_with(cx, |e, _| e.text().find("[[Source]]").unwrap()) + 3;
    if let Some(at) = reader.read_with(cx, |e, _| e.point_of(inner)).filter(|at| shown.contains(at)) {
        cx.simulate_mouse_move(at, None, ctrl);
        settle(cx);
        assert_eq!(seen(cx).map(|s| s.0).as_deref(), Some("Cible"));
    }
    // Text typed in the card changes nothing.
    cx.simulate_mouse_down(shown.center(), MouseButton::Left, none);
    cx.simulate_mouse_up(shown.center(), MouseButton::Left, none);
    assert!(cx.update(|window, cx| reader.focus_handle(cx).is_focused(window)), "a click in the card gives it the keys");
    let before = reader.read_with(cx, |e, _| e.text().to_string());
    // Selecting and copying work.
    cx.simulate_keystrokes("secondary-a secondary-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some(before.as_str()));
    cx.simulate_input("zz");
    // The keys that edit, paste or search reach nothing; at the end of the text either, where a cursor put
    // after a refused text would point out of it.
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("collé".into()));
    cx.simulate_keystrokes("secondary-end enter backspace tab secondary-v secondary-z secondary-f x secondary-b");
    settle(cx);
    assert_eq!(reader.read_with(cx, |e, _| e.text().to_string()), before);
    assert!(!fs::read_to_string(root.join("Cible.md")).unwrap().contains("zz"));
    assert!(seen(cx).is_some(), "the card stays while its text is read");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(seen(cx), None);
    // A narrow, a middle and a wide window, the text at 11, 16 and 24 px: the card is in the window, against the
    // link (under or above it), never wider than the room.
    for (width, zoom) in [(420., "secondary-- secondary-- secondary-- secondary-- secondary--"), (860., "secondary-0"), (1600., "secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-= secondary-=")] {
        cx.simulate_keystrokes("secondary-0");
        cx.simulate_keystrokes(zoom);
        cx.simulate_resize(size(px(width), px(720.)));
        settle(cx);
        { let at = spot(cx, "[[Cible]]", 0); cx.simulate_mouse_move(at, None, ctrl); }
        settle(cx);
        let at = editor.read_with(cx, |e, _| e.point_of(e.text().find("[[Cible]]").unwrap() + 3).unwrap());
        let shown = card(cx).expect("the card is drawn");
        assert!(seen(cx).is_some(), "{width}");
        assert!(inside(shown, window(cx)), "{width}: {shown:?}");
        assert!(shown.top() > at.y || shown.bottom() < at.y, "{width}: {shown:?} over the link at {at:?}");
        assert!((shown.top() - at.y).abs() < px(40.) || (at.y - shown.bottom()).abs() < px(40.), "{width}: {shown:?} far from {at:?}");
        cx.simulate_keystrokes("escape");
        settle(cx);
    }
    cx.simulate_keystrokes("secondary-0");
    cx.simulate_resize(size(px(1000.), px(720.)));
    settle(cx);
    { let at = spot(cx, "[[Cible]]", 0); cx.simulate_mouse_move(at, None, ctrl); }
    settle(cx);
    assert!(seen(cx).is_some());
    // Out of the link and the card: it goes.
    { let at = spot(cx, "Moi", 0); cx.simulate_mouse_move(at, None, none); }
    settle(cx);
    assert_eq!(seen(cx), None);

    // To a heading: from the heading; a heading that is not there: the whole note, said so.
    { let at = spot(cx, "[[Cible#Partie]]", 0); cx.simulate_mouse_move(at, None, ctrl); }
    settle(cx);
    let (target, text) = seen(cx).expect("a card");
    assert_eq!((target.as_str(), text.as_deref()), ("Cible#Partie", Some("## Partie\n\nDans la partie, [[Source]]")));
    // The card covers the lines under the link: it is closed before pointing at another one.
        cx.simulate_keystrokes("escape");

    { let at = spot(cx, "[[Cible#Rien]]", 0); cx.simulate_mouse_move(at, None, ctrl); }
    settle(cx);
    let got = seen(cx);
    assert!(got.clone().and_then(|s| s.1).is_some_and(|t| t.starts_with("Première ligne")), "{got:?} {:?}", editor.read_with(cx, |e, _| e.text().to_string()));
    // The note itself; an alias; a link in a table cell.
    for (needle, shows) in [("[[Source]]", "Voir [[Cible]]"), ("[[Surnom]]", "Le texte de la note nommée"), ("[[Nommée]]", "Le texte de la note nommée")] {
        cx.simulate_keystrokes("escape");
        { let at = spot(cx, needle, 0); cx.simulate_mouse_move(at, None, ctrl); }
        settle(cx);
        let text = seen(cx).and_then(|s| s.1).unwrap_or_else(|| panic!("{needle}: no card"));
        assert!(text.starts_with(shows), "{needle}: {text:?}");
        assert!(inside(card(cx).unwrap(), window(cx)));
    }
    // Escape closes it.
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert_eq!(seen(cx), None);

    // The note changes on disk while it is previewed: the card follows.
    { let at = spot(cx, "[[Cible]]", 0); cx.simulate_mouse_move(at, None, ctrl); }
    settle(cx);
    fs::write(root.join("Cible.md"), "# Cible\n\nRécrite ailleurs\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    settle(cx);
    assert_eq!(seen(cx).and_then(|s| s.1).as_deref(), Some("Récrite ailleurs"));
    // A scroll closes it, as typing does.
    let at = spot(cx, "Moi", 0);
    cx.simulate_event(gpui::ScrollWheelEvent { position: at, delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-5.))), ..Default::default() });
    settle(cx);
    assert_eq!(seen(cx), None);

    // Another note loaded in the pane (a tab changed): the card goes.
    { let at = spot(cx, "[[Cible]]", 0); cx.simulate_mouse_move(at, None, ctrl); }
    settle(cx);
    assert!(seen(cx).is_some());
    let text = editor.read_with(cx, |e, _| e.text().to_string());
    editor.update(cx, |e, cx| e.load(text.clone(), 0, cx));
    settle(cx);
    assert_eq!(seen(cx), None);
    // `Ctrl + click` on the link still opens it, the card open or not.
    let link = spot(cx, "[[Cible]]", 0);
    cx.simulate_mouse_move(link, None, ctrl);
    settle(cx);
    assert!(seen(cx).is_some());
    cx.simulate_mouse_down(link, MouseButton::Left, ctrl);
    cx.simulate_mouse_up(link, MouseButton::Left, ctrl);
    settle(cx);
    assert_eq!(seen(cx), None);
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(root.join("Cible.md")));
    cx.simulate_keystrokes("alt-left");
    settle(cx);
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(root.join("Source.md")));

    // The title opens the note there, without creating anything.
    let link = spot(cx, "[[Cible#Partie]]", 0);
    cx.simulate_mouse_move(link, None, ctrl);
    settle(cx);
    let title = cx.debug_bounds("peek-title").unwrap();
    cx.simulate_mouse_down(title.center(), MouseButton::Left, none);
    settle(cx);
    assert_eq!(seen(cx), None);
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(root.join("Cible.md")));

    // A missing note: the card says so, and « Create » makes it.
    shell.update_in(cx, |s, window, cx| s.open(&root.join("Source.md"), window, cx));
    settle(cx);
    { let at = spot(cx, "[[Absente]]", 0); cx.simulate_mouse_move(at, None, ctrl); }
    settle(cx);
    assert_eq!(seen(cx), Some(("Absente".into(), None)));
    let create = cx.debug_bounds("peek-create").expect("« Create »");
    assert!(inside(create, card(cx).unwrap()));
    cx.simulate_mouse_down(create.center(), MouseButton::Left, none);
    settle(cx);
    assert_eq!(seen(cx), None);
    assert!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().starts_with("# Absente")));

    // From the keyboard: the link under the cursor; the pointer moving leaves it, typing closes it.
    shell.update_in(cx, |s, window, cx| {
        s.open(&root.join("Source.md"), window, cx);
        window.focus(&s.pane().editor.focus_handle(cx));
    });
    settle(cx);
    let at = editor.read_with(cx, |e, _| e.text().find("[[Cible]]").unwrap() + 4);
    editor.update(cx, |e, cx| e.jump(at, false, cx));
    settle(cx);
    cx.dispatch_action(crate::actions::PreviewLink);
    settle(cx);
    assert_eq!(seen(cx).map(|s| s.0).as_deref(), Some("Cible"));
    { let at = spot(cx, "Moi", 0); cx.simulate_mouse_move(at, None, none); }
    settle(cx);
    assert!(seen(cx).is_some());
    cx.simulate_input("a");
    settle(cx);
    assert_eq!(seen(cx), None);
    // No link under the cursor: nothing opens.
    editor.update(cx, |e, cx| e.jump(2, false, cx));
    cx.dispatch_action(crate::actions::PreviewLink);
    settle(cx);
    assert_eq!(seen(cx), None);

    // The backlinks of the panel, with `Ctrl`: the note that links here, in the card beside its row.
    shell.update_in(cx, |s, window, cx| {
        s.open(&root.join("Cible.md"), window, cx);
        s.show_panel(crate::views::nav::Mode::Links, crate::views::nav::Panel::Split, cx);
    });
    settle(cx);
    let row = cx.debug_bounds("nav-row-Source").expect("the backlink row");
    cx.simulate_mouse_move(row.center(), None, none);
    settle(cx);
    assert_eq!(seen(cx), None);
    cx.simulate_mouse_move(row.center() + point(px(4.), px(0.)), None, ctrl);
    settle(cx);
    assert_eq!(seen(cx).map(|s| s.0).as_deref(), Some("Source"));
    let shown = card(cx).unwrap();
    assert!(inside(shown, window(cx)) && shown.top() >= row.bottom() - px(0.5), "{shown:?} under {row:?}");
    cx.simulate_mouse_move(point(row.center().x, row.top() - px(6.)), None, none);
    settle(cx);
    assert_eq!(seen(cx), None);
}
