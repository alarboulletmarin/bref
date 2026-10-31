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

    // Alt+Shift+arrows (Cmd+Alt on macOS) go from pane to pane; nothing beyond the edge.
    let mac = cfg!(target_os = "macos");
    let (up, left) = if mac { ("cmd-alt-up", "cmd-alt-left") } else { ("alt-shift-up", "alt-shift-left") };
    cx.simulate_keystrokes(up);
    assert_eq!(state(&shell, cx).1, 1);
    cx.simulate_keystrokes(left);
    assert_eq!(state(&shell, cx).1, 0);
    cx.simulate_keystrokes(left);
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

/// A note rewritten outside the app (an agent through `bref mcp`, a sync) is reread where the
/// reader is: the scroll and the cursor stay, the new text is there.
#[gpui::test]
fn an_outside_write_keeps_the_place(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "outside-write");
    let path = root.join("Long.md");
    let lines: String = (0..300).map(|i| format!("Ligne {i}\n\n")).collect();
    fs::write(&path, format!("# Long\n\n{lines}")).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(900.), px(700.)));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&path, window, cx));
    cx.run_until_parked();
    let at = fs::read_to_string(&path).unwrap().find("Ligne 150").unwrap();
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.set_view(at, 2400., cx)));
    cx.run_until_parked();
    let before = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view());

    // Appended at the end, then a line inserted at the top, as an agent's edits would.
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("\n## Écrit par un agent\n");
    fs::write(&path, &text).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    let (after, shown) = shell.read_with(cx, |s, cx| (s.pane().editor.read(cx).view(), s.pane().editor.read(cx).text().to_string()));
    assert!(shown.contains("Écrit par un agent"));
    assert_eq!(after, before, "the reader was moved");
    // A text that got shorter than the cursor: the cursor goes to the end, nothing panics.
    fs::write(&path, "# Long\n\nCourt é\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    let (at, _) = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view());
    assert_eq!(at, "# Long\n\nCourt é\n".len());
}

/// A diagram rewritten outside the app (an agent through `bref mcp`) is redrawn in its open canvas, without
/// closing it: the view stays where it was, and one step of undo goes back to what was shown.
#[gpui::test]
fn an_outside_write_redraws_the_open_diagram(cx: &mut TestAppContext) {
    use crate::logic::diagram::{self, Diagram, Form};
    let (root, shell, cx, _config) = start(cx, "outside-diagram");
    let path = root.join("Schéma.svg");
    let mut first = Diagram::default();
    first.add_shape(Form::Rect, 0., 0., 120., 60.);
    fs::write(&path, first.to_svg(diagram::COLORS[0])).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(900.), px(700.)));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&path, window, cx));
    cx.run_until_parked();
    let canvas = shell.read_with(cx, |s, _| s.pane().drawing().cloned()).expect("the diagram opens in its canvas");
    let view = canvas.read_with(cx, |c, _| c.spot(0., 0.));

    let mut second = first.clone();
    second.add_shape(Form::Ellipse, 300., 0., 120., 80.);
    fs::write(&path, second.to_svg(diagram::COLORS[0])).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shapes.len()), 2, "the canvas shows the diagram as it is now");
    assert_eq!(canvas.read_with(cx, |c, _| c.spot(0., 0.)), view, "the view moved");
    assert!(shell.read_with(cx, |s, _| s.pane().drawing() == Some(&canvas)), "the same canvas, not a new one");

    // Undo goes back to what was shown, and that is written.
    shell.update_in(cx, |s, window, cx| window.focus(&s.pane().drawing().unwrap().focus_handle(cx)));
    cx.simulate_keystrokes("secondary-z");
    cx.run_until_parked();
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shapes.len()), 1);
    assert_eq!(Diagram::from_svg(&fs::read_to_string(&path).unwrap()).unwrap().shapes.len(), 1);

    // The same text again, or a file that is no diagram any more: nothing changes.
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.run_until_parked();
    assert_eq!(canvas.read_with(cx, |c, _| c.diagram().shapes.len()), 1);
}

/// A note written by another program while it is typed in (an agent through `bref mcp`): both
/// changes stay, as one undo step; on the same lines, the other version goes next to the note.
#[gpui::test]
fn an_outside_write_while_typing_is_merged(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "outside-typing");
    let path = root.join("Plan.md");
    fs::write(&path, "# Plan\n\nPremière ligne\nDeuxième ligne\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&path, window, cx));
    cx.run_until_parked();
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().to_string());
    let typing_at = |cx: &mut gpui::VisualTestContext, needle: &str, typed: &str| {
        let at = text(cx).find(needle).unwrap() + needle.len();
        shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.set_view(at, 0., cx)));
        cx.simulate_input(typed);
    };
    let save = |cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
    };

    // The user types on the first line; before the save, the agent appends a section.
    typing_at(cx, "Première ligne", " tapée");
    fs::write(&path, "# Plan\n\nPremière ligne\nDeuxième ligne\n\n## Agent 🙂\n").unwrap();
    save(cx);
    let both = "# Plan\n\nPremière ligne tapée\nDeuxième ligne\n\n## Agent 🙂\n";
    assert_eq!(fs::read_to_string(&path).unwrap(), both);
    assert_eq!(text(cx), both);
    let cursor = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).view().0);
    assert_eq!(cursor, both.find(" tapée").unwrap() + " tapée".len(), "the cursor stays where the user types");
    // Undo takes back the agent's change in one step, and only it.
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.restore(false, cx)));
    assert_eq!(text(cx), "# Plan\n\nPremière ligne tapée\nDeuxième ligne\n");
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.restore(true, cx)));
    save(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), both);

    // Both change the same line: the note keeps what was typed, the agent's version is beside it.
    typing_at(cx, "Deuxième ligne", " ici");
    fs::write(&path, both.replace("Deuxième ligne", "Deuxième ligne agent")).unwrap();
    save(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), both.replace("Deuxième ligne", "Deuxième ligne ici"));
    let date = crate::lang::date_name(crate::platform::clock::today());
    let copy = root.join(format!("Plan (conflict {date}).md"));
    assert_eq!(fs::read_to_string(&copy).unwrap(), both.replace("Deuxième ligne", "Deuxième ligne agent"));
    let said = shell.read_with(cx, |s, _| s.toasts.iter().map(|t| (t.tone, t.text.clone())).collect::<Vec<_>>());
    assert!(said.iter().any(|(tone, text)| *tone == Tone::Info && text.ends_with(&format!("Plan (conflict {date})"))), "{said:?}");
    // Again: the copy gets a number, nothing is overwritten.
    typing_at(cx, "Deuxième ligne ici", "!");
    fs::write(&path, both.replace("Deuxième ligne", "encore")).unwrap();
    save(cx);
    assert_eq!(fs::read_to_string(root.join(format!("Plan (conflict {date}) 2.md"))).unwrap(), both.replace("Deuxième ligne", "encore"));
    assert_eq!(fs::read_to_string(&copy).unwrap(), both.replace("Deuxième ligne", "Deuxième ligne agent"));

    // The copy cannot be written (read-only folder): the note is not saved over the agent's text.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        typing_at(cx, "Première ligne tapée", "?");
        let theirs = text(cx).replace("Première ligne tapée?", "Première agent");
        fs::write(&path, &theirs).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();
        save(cx);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), theirs, "the agent's text was overwritten");
        let failed = shell.read_with(cx, |s, _| s.toasts.iter().any(|t| t.tone == Tone::Failed));
        assert!(failed);
    }
}
