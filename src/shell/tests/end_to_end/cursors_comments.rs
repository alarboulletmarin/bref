//! Multiple cursors, comments and their panel, the badge, narrow windows.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let shell = &j.shell;
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Multiple cursors: Ctrl+D takes the word, then its next occurrences; typing,
    // deleting and moving apply to all, a single Ctrl+Z undoes it all,
    // Escape returns to one cursor. Alt+click adds one more.
    let cursors = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).cursors());
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
    // One undo step reverts the gesture for the three cursors at once, and only it.
    cx.simulate_keystrokes("secondary-z");
    assert_eq!((text(cx), cursors(cx)), ("e chats\ne chiens\ne rats".to_string(), 1));
    j.load(cx, "un chat\nun chien\nun rat", 0);
    cx.simulate_keystrokes("secondary-d secondary-d escape");
    assert_eq!(cursors(cx), 1);
    cx.simulate_input("X");
    // The last cursor added stays: the one of the second occurrence.
    assert_eq!(text(cx), "un chat\nX chien\nun rat");
    j.load(cx, "ab\ncd", 0);
    let middle = cx.update(|window, _| window.viewport_size());
    cx.simulate_mouse_down(point(middle.width / 2., middle.height / 2.), MouseButton::Left, gpui::Modifiers { alt: true, ..Default::default() });
    cx.simulate_mouse_up(point(middle.width / 2., middle.height / 2.), MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_input("X");
    assert_eq!((text(cx).matches('X').count(), cursors(cx)), (2, 2));
    cx.simulate_keystrokes("escape");
    // Comments: Ctrl+Shift+M comments the selection (without one, the line, after its bullet),
    // the palette lists them and leads there, the same shortcut on a comment resolves it.
    j.load(cx, "- un mot ici\nfin", 5);
    cx.simulate_keystrokes("shift-right shift-right shift-right secondary-shift-m");
    cx.simulate_input("à revoir");
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\nfin");
    cx.simulate_keystrokes("down secondary-shift-m");
    cx.simulate_input("ok");
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\n{==fin==}{>>ok<<}");
    // The panel, right of the note: a click on a card leads into its comment,
    // its check mark resolves it; closed, it comes back when the palette lists comments.
    cx.run_until_parked();
    let view = cx.update(|window, _| window.viewport_size());
    cx.simulate_click(point(view.width - px(150.), px(125.)), gpui::Modifiers::none());
    assert!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).position() == 17));
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.resolve_comment(34, cx)));
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\nfin");
    cx.simulate_keystrokes("secondary-z");
    // The column marks the comment under the caret, and none elsewhere.
    let seen = |cx: &mut gpui::VisualTestContext, at: usize| {
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.place_cursor(at, cx)));
        cx.run_until_parked();
        shell.read_with(cx, |s, _| s.comments_seen)
    };
    assert_eq!((seen(cx, 40), seen(cx, 0), seen(cx, 8)), (Some(1), None, Some(0)));
    // Resolved by the keys (the editor keeps Tab, so the shortcut is the way), and undone.
    cx.simulate_keystrokes("secondary-shift-m");
    cx.run_until_parked();
    assert_eq!(text(cx), "- un mot ici\n{==fin==}{>>ok<<}");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\n{==fin==}{>>ok<<}");
    // The corner button, left of the copy one, closes the panel (and reopens it).
    cx.run_until_parked();
    let corner = cx.update(|window, _| window.viewport_size());
    // Two comments: the badge is at the button's corner, inside the window.
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
    assert!(shell.read_with(cx, |s, cx| s.palette.is_none() && !s.comments_shut && s.pane().editor.read(cx).position() == 5));
    cx.simulate_keystrokes("secondary-shift-m");
    assert_eq!(text(cx), "- un mot ici\n{==fin==}{>>ok<<}");
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), "- un {==mot==}{>>à revoir<<} ici\n{==fin==}{>>ok<<}");
    // The badge at its limits: ten comments, twelve, a hundred and twenty. It only grows to the
    // left (its right edge does not move), stays in the window and away from the copy button,
    // and its height does not change.
    let kept = text(cx);
    let many = |n: usize| format!("# T\n\n{}\n", (0..n).map(|i| format!("mot{{=={i}==}}{{>>c{i}<<}}")).collect::<Vec<_>>().join(" "));
    let mut seen = Vec::new();
    for n in [1, 9, 10, 12, 120] {
        j.load(cx, &many(n), 0);
        let (badge, toggle) = (cx.debug_bounds("comments-badge").unwrap(), cx.debug_bounds("comments-toggle").unwrap());
        let window = cx.update(|window, _| window.viewport_size());
        assert!(badge.right() <= window.width - px(47.), "{n}: the badge touches the copy button {badge:?}");
        assert!(badge.left() >= toggle.left() - px(12.) && badge.top() >= toggle.top() - px(4.) && badge.size.height == px(15.), "{n}: {badge:?} {toggle:?}");
        seen.push(badge);
    }
    // One digit: round; "9+": wider, at the same right edge, likewise for 10, 12 and 120.
    assert!(seen[0].size.width == seen[1].size.width && seen[2].size.width > seen[1].size.width, "{seen:?}");
    assert!(seen.iter().all(|b| b.right() == seen[0].right()) && seen[2..].iter().all(|b| b.size.width == seen[2].size.width), "{seen:?}");
    j.load(cx, &kept, 0);
    // Narrow window or panel too wide: nothing slips under its neighbor or out of the
    // window. The note keeps 200 px, the panel then the comments column give way to it.
    let before = shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel, s.nav.list_w));
    let whole = cx.update(|window, _| window.viewport_size());
    let lay = |cx: &mut gpui::VisualTestContext, width: f32, mode: Mode, panel: Panel, list: f32| {
        cx.simulate_resize(size(px(width), px(600.)));
        shell.update(cx, |s, cx| {
            s.nav.show_alone(mode);
            (s.nav.panel, s.nav.list_w) = (panel, px(list));
            cx.notify();
        });
        cx.run_until_parked();
    };
    let within = |cx: &mut gpui::VisualTestContext, part: &'static str, zone: &'static str| {
        let (part_at, zone_at) = (cx.debug_bounds(part).unwrap_or_else(|| panic!("{part} absent")), cx.debug_bounds(zone).unwrap());
        assert!(part_at.left() >= zone_at.left() && part_at.right() <= zone_at.right(), "{part} overflows {zone}: {part_at:?} outside {zone_at:?}");
    };
    // A 600 px panel in an 860 window: it gives back room to the note and the comments.
    lay(cx, 860., Mode::Tree, Panel::Split, 600.);
    let (panel, pane, said) = (cx.debug_bounds("nav-panel").unwrap(), cx.debug_bounds("pane-0").unwrap(), cx.debug_bounds("comments").unwrap());
    assert!(pane.size.width >= px(199.) && said.size.width == px(270.) && said.right() <= px(860.), "{pane:?} {said:?}");
    assert!(panel.right() <= pane.left() && pane.right() <= said.left(), "{panel:?} {pane:?} {said:?}");
    // 420 px with the panel: no more room for the comments, the note keeps its own.
    lay(cx, 420., Mode::Tree, Panel::Split, 600.);
    let pane = cx.debug_bounds("pane-0").unwrap();
    assert!(pane.size.width >= px(199.) && pane.right() == px(420. - 8.), "{pane:?}");
    // Panel collapsed: the column comes back, narrower; search and the counter fit in the note.
    lay(cx, 420., Mode::Tree, Panel::Rail, 600.);
    let (pane, said) = (cx.debug_bounds("pane-0").unwrap(), cx.debug_bounds("comments").unwrap());
    assert!(pane.size.width >= px(199.) && said.size.width == px(172.) && said.right() <= px(420. - 8.), "{pane:?} {said:?}");
    cx.simulate_keystrokes("secondary-f");
    cx.run_until_parked();
    within(cx, "find-bar", "pane-0");
    within(cx, "counts", "pane-0");
    cx.simulate_keystrokes("escape");
    // The calendar does not go below 170 px, and its month fits in the panel.
    lay(cx, 420., Mode::Calendar, Panel::Split, 120.);
    assert_eq!(cx.debug_bounds("nav-panel").unwrap().size.width, px(170.));
    within(cx, "calendar", "nav-panel");
    cx.simulate_resize(whole);
    shell.update(cx, |s, cx| {
        s.nav.show_alone(before.0);
        (s.nav.panel, s.nav.list_w) = (before.1, before.2);
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
