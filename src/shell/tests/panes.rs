//! Panes: the window split in any number of panes, side by side and one above the other,
//! reached and fed from the keys, resized by their borders, kept at restart.

use super::*;

/// The layout as saved, and the group that has input.
fn state(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> (String, usize) {
    shell.read_with(cx, |s, _| (s.layout.write(), s.group()))
}

/// The bounds of each pane in the window, by group.
fn areas(shell: &Entity<Shell>, cx: &mut gpui::VisualTestContext) -> Vec<Bounds<Pixels>> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let count = shell.read_with(cx, |s, _| s.groups.len());
    (0..count).map(|g| cx.debug_bounds(leak(format!("pane-{g}"))).unwrap_or_else(|| panic!("pane-{g}"))).collect()
}

/// No two panes overlap, and each keeps its minimum size.
fn apart(areas: &[Bounds<Pixels>]) {
    for (i, a) in areas.iter().enumerate() {
        assert!(a.size.width >= pane::MIN_PANE - px(40.) && a.size.height >= pane::MIN_PANE_H - px(40.), "pane {i} too small: {a:?}");
        for b in &areas[i + 1..] {
            let overlap = a.left() < b.right() - px(1.) && b.left() < a.right() - px(1.) && a.top() < b.bottom() - px(1.) && b.top() < a.bottom() - px(1.);
            assert!(!overlap, "{a:?} overlaps {b:?}");
        }
    }
}

#[gpui::test]
fn panes_split_move_resize_and_come_back(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "panes");
    let note = |name: &str| root.join(format!("{name}.md"));
    for name in ["Alpha", "Beta", "Gamma", "Delta"] {
        fs::write(note(name), format!("# {name}\n\n")).unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(1400.), px(900.)));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Alpha"), window, cx));
    cx.run_until_parked();
    let choose = |cx: &mut gpui::VisualTestContext, name: &str| {
        cx.simulate_input(name);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
    };

    // Ctrl+\ splits to the right, Ctrl+Alt+\ below: each new pane asks what it shows.
    cx.simulate_keystrokes("secondary-\\");
    choose(cx, "beta");
    assert_eq!(state(&shell, cx), ("row(0.5 0, 0.5 1)".into(), 1));
    cx.simulate_keystrokes("secondary-alt-\\");
    choose(cx, "gamma");
    assert_eq!(state(&shell, cx), ("row(0.5 0, 0.5 col(0.5 1, 0.5 2))".into(), 2));
    let three = areas(&shell, cx);
    apart(&three);
    assert!(three[1].bottom() <= three[2].top() + px(0.5) && (three[1].left() - three[2].left()).abs() < px(0.5), "{three:?}");
    assert!(three[0].right() <= three[1].left() + px(0.5), "{three:?}");

    // Alt+Shift+arrows go from pane to pane; nothing beyond the edge.
    cx.simulate_keystrokes("alt-shift-up");
    assert_eq!(state(&shell, cx).1, 1);
    cx.simulate_keystrokes("alt-shift-left");
    assert_eq!(state(&shell, cx).1, 0);
    cx.simulate_keystrokes("alt-shift-left");
    assert_eq!(state(&shell, cx).1, 0);

    // A tab sent down splits its pane when there is none there; alone, it stays.
    cx.simulate_keystrokes("secondary-p");
    choose(cx, "delta");
    assert_eq!(shell.read_with(cx, |s, _| s.groups[0].tabs.len()), 2);
    cx.dispatch_action(TabToDown);
    cx.run_until_parked();
    assert_eq!(state(&shell, cx), ("row(0.5 col(0.5 0, 0.5 3), 0.5 col(0.5 1, 0.5 2))".into(), 3));
    cx.dispatch_action(TabToDown);
    cx.run_until_parked();
    assert_eq!(state(&shell, cx).0, "row(0.5 col(0.5 0, 0.5 3), 0.5 col(0.5 1, 0.5 2))");
    // Sent right, it joins the pane level with it as its tab; its own pane, emptied, goes.
    cx.dispatch_action(TabToRight);
    cx.run_until_parked();
    assert_eq!(state(&shell, cx), ("row(0.5 0, 0.5 col(0.5 1, 0.5 2))".into(), 2));
    assert_eq!(shell.read_with(cx, |s, _| s.groups[2].tabs.len()), 2);

    // The border between the columns follows the pointer, and stops where a pane would get
    // narrower than its minimum.
    let border = cx.debug_bounds("pane-border--0").unwrap().center();
    cx.simulate_mouse_down(border, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(point(px(0.), border.y), MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(point(px(0.), border.y), MouseButton::Left, gpui::Modifiers::none());
    let dragged = areas(&shell, cx);
    apart(&dragged);
    assert!((dragged[0].size.width - pane::MIN_PANE).abs() < px(8.), "{dragged:?}");
    // A double click on it shares the columns evenly again.
    let border = cx.debug_bounds("pane-border--0").unwrap().center();
    cx.simulate_event(gpui::MouseDownEvent { position: border, modifiers: gpui::Modifiers::none(), button: MouseButton::Left, click_count: 2, first_mouse: false });
    cx.simulate_mouse_up(border, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(state(&shell, cx).0, "row(0.5 0, 0.5 col(0.5 1, 0.5 2))");

    // Alt+Shift+Enter: the pane on the whole window, then back.
    cx.simulate_keystrokes("alt-shift-enter");
    assert_eq!(shell.read_with(cx, |s, _| s.zoomed), Some(2));
    let alone = areas(&shell, cx)[2];
    assert!(alone.size.width > px(1000.) && alone.size.height > px(800.), "{alone:?}");
    cx.simulate_keystrokes("alt-shift-enter");
    assert_eq!(shell.read_with(cx, |s, _| s.zoomed), None);

    // A window too small for every pane shows the active one alone; bigger again, all come back.
    // (At 420 px each column keeps 190 px: still enough.)
    cx.simulate_resize(size(px(420.), px(720.)));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.alone()), None);
    cx.simulate_resize(size(px(320.), px(720.)));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.alone()), Some(2));
    let alone = areas(&shell, cx)[2];
    assert!(alone.size.width > px(270.) && alone.size.height > px(650.), "{alone:?}");
    cx.simulate_resize(size(px(1400.), px(900.)));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.alone()), None);
    apart(&areas(&shell, cx));

    // The tree comes back at the next start, each pane on what it showed.
    let (again, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx2.run_until_parked();
    let back = again.read_with(cx2, |s, _| (s.layout.write(), s.groups.iter().map(|g| s.panes[g.shown].path.clone()).collect::<Vec<_>>()));
    assert_eq!(back.0, "row(0.5 0, 0.5 col(0.5 1, 0.5 2))");
    assert_eq!(back.1[0], Some(note("Alpha")));
    assert_eq!(back.1[2], Some(note("Delta")));

    // No room: a pane is not split below its minimum, and the message says why.
    cx.simulate_resize(size(px(900.), px(560.)));
    let before = state(&shell, cx).0;
    cx.simulate_keystrokes("secondary-alt-\\");
    cx.run_until_parked();
    assert_eq!(state(&shell, cx).0, before);
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Info, "Pas de place")));
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("secondary-\\");
    cx.run_until_parked();
    assert_eq!(state(&shell, cx).0, before);

    // Closing a pane's last tab gives its room to its neighbour, down to one pane.
    while shell.read_with(cx, |s, _| s.groups.len()) > 1 {
        cx.simulate_keystrokes("secondary-w");
        cx.run_until_parked();
    }
    assert_eq!(state(&shell, cx).0, "0");
}

#[gpui::test]
fn tabs_dragged_into_place(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "tab-drag");
    let note = |name: &str| root.join(format!("{name}.md"));
    for name in ["Alpha", "Beta", "Gamma"] {
        fs::write(note(name), format!("# {name}\n\n")).unwrap();
    }
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(1400.), px(900.)));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note("Alpha"), window, cx));
    for name in ["beta", "gamma"] {
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input(name);
        cx.simulate_keystrokes("enter");
    }
    cx.run_until_parked();
    let names = |cx: &mut gpui::VisualTestContext, group: usize| {
        shell.read_with(cx, |s, _| s.groups[group].tabs.iter().map(|&t| vault::stem(s.panes[t].path.as_ref().unwrap())).collect::<Vec<_>>())
    };
    assert_eq!(names(cx, 0), ["Alpha", "Beta", "Gamma"]);
    let none = gpui::Modifiers::none();
    let tab = |cx: &mut gpui::VisualTestContext, name: &str| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let id = shell.read_with(cx, |s, _| s.pane_showing(&note(name)).unwrap());
        cx.debug_bounds(leak(format!("tab-{id}"))).unwrap()
    };
    // Takes the tab `name` and holds it at `to`, not let go yet.
    let hold = |cx: &mut gpui::VisualTestContext, name: &str, to: Point<Pixels>| {
        let from = tab(cx, name).center();
        cx.simulate_mouse_down(from, MouseButton::Left, none);
        cx.simulate_mouse_move(from + point(px(12.), px(2.)), MouseButton::Left, none);
        cx.simulate_mouse_move(to, MouseButton::Left, none);
        cx.run_until_parked();
    };
    let aims = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.tab_aim, s.pane_aim));

    // Along the bar: on the right half of the last tab, it goes last, a line showing where.
    let last = tab(cx, "Gamma");
    let to = point(last.right() - px(8.), last.center().y);
    hold(cx, "Alpha", to);
    assert_eq!(aims(cx), (Some((0, 3)), None));
    let line = cx.debug_bounds("tab-aim").unwrap();
    assert!((line.right() - last.right()).abs() <= px(1.5) && line.size.width < px(3.), "{line:?} {last:?}");
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(names(cx, 0), ["Beta", "Gamma", "Alpha"]);
    assert_eq!(aims(cx), (None, None));
    // And back first, on the left half of the first.
    let first = tab(cx, "Beta");
    let to = point(first.left() + px(8.), first.center().y);
    hold(cx, "Alpha", to);
    assert_eq!(aims(cx), (Some((0, 0)), None));
    cx.simulate_mouse_up(to, MouseButton::Left, none);
    assert_eq!(names(cx, 0), ["Alpha", "Beta", "Gamma"]);
    // Dropped on its own place: nothing moves.
    let own = tab(cx, "Beta").center();
    hold(cx, "Beta", own + point(px(14.), px(0.)));
    cx.simulate_mouse_up(own, MouseButton::Left, none);
    assert_eq!(names(cx, 0), ["Alpha", "Beta", "Gamma"]);

    // On the middle of its own pane: nothing to aim at. Near its right edge: the right half is
    // tinted, and the tab takes a new pane there.
    let area = cx.debug_bounds("pane-0").unwrap();
    hold(cx, "Gamma", area.center());
    assert_eq!(aims(cx), (None, None));
    let edge = point(area.right() - px(20.), area.center().y);
    cx.simulate_mouse_move(edge, MouseButton::Left, none);
    assert_eq!(aims(cx), (None, Some((0, Zone::Edge(Side::Right)))));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let tint = cx.debug_bounds("pane-tint").unwrap();
    assert!((tint.left() - area.center().x).abs() < px(1.) && (tint.right() - area.right()).abs() < px(1.), "{tint:?} {area:?}");
    cx.simulate_mouse_up(edge, MouseButton::Left, none);
    assert_eq!(state(&shell, cx), ("row(0.5 0, 0.5 1)".into(), 1));
    assert_eq!((names(cx, 0), names(cx, 1)), (vec!["Alpha".to_string(), "Beta".into()], vec!["Gamma".to_string()]));

    // On the middle of another pane: it joins it, right of the tab shown.
    let other = cx.debug_bounds("pane-1").unwrap().center();
    hold(cx, "Beta", other);
    assert_eq!(aims(cx), (None, Some((1, Zone::Center))));
    cx.simulate_mouse_up(other, MouseButton::Left, none);
    assert_eq!((names(cx, 0), names(cx, 1)), (vec!["Alpha".to_string()], vec!["Gamma".to_string(), "Beta".into()]));

    // Escape halfway: the drag stops, nothing moves, and letting go does nothing.
    let below = { let a = cx.debug_bounds("pane-0").unwrap(); point(a.center().x, a.bottom() - px(10.)) };
    hold(cx, "Beta", below);
    assert_eq!(aims(cx), (None, Some((0, Zone::Edge(Side::Down)))));
    cx.simulate_keystrokes("escape");
    assert_eq!(aims(cx), (None, None));
    cx.simulate_mouse_up(below, MouseButton::Left, none);
    assert_eq!(state(&shell, cx).0, "row(0.5 0, 0.5 1)");
    assert_eq!(names(cx, 1), ["Gamma", "Beta"]);

    // A pane too small to split: its edge is not aimed at, and a drop there changes nothing.
    cx.simulate_resize(size(px(760.), px(720.)));
    let a = cx.debug_bounds("pane-1").unwrap();
    let edge = point(a.left() + px(10.), a.center().y);
    hold(cx, "Beta", edge);
    assert_eq!(aims(cx), (None, None));
    cx.simulate_mouse_up(edge, MouseButton::Left, none);
    assert_eq!(state(&shell, cx).0, "row(0.5 0, 0.5 1)");
    // Out of every pane and bar (over the rail): nothing either.
    let rail = point(px(10.), px(300.));
    hold(cx, "Beta", rail);
    assert_eq!(aims(cx), (None, None));
    cx.simulate_mouse_up(rail, MouseButton::Left, none);
    assert_eq!((names(cx, 0), names(cx, 1)), (vec!["Alpha".to_string()], vec!["Gamma".to_string(), "Beta".into()]));
}
