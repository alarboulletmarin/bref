//! The side panel: tree, new note in a folder, divider, recent notes, graph.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    // Tree: Ctrl+E opens it and gives it focus; the arrows show a preview (or the tab of a note
    // already open), which does not count as an opening.
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.simulate_keystrokes("secondary-e up");
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Tree, Panel::Split));
    assert_eq!(text(cx), "# Courses\n\n- lait #maison\n[[Test]]");
    // Courses is open in a tab already: that tab is shown, without counting as an opening.
    assert!(shell.read_with(cx, |s, _| !s.pane().preview && s.recent[0] == root.join("Idées.md")));
    // The folder expands with Right; Enter opens the note and hands control back to the editor.
    cx.simulate_keystrokes("up right down");
    // The collapse button collapses the whole tree, then expands it entirely.
    let open = |s: &mut Shell| {
        s.fold_all();
        s.nav.open.len()
    };
    assert_eq!(shell.update(cx, |s, _| (open(s), open(s))), (0, 1));
    assert_eq!(text(cx), "# Plan\n\n[[Courses]]\n");
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| !s.pane().preview && s.recent[0] == root.join("Projets/Plan.md")));
    assert!(cx.update(|window, cx| shell.read(cx).pane().editor.focus_handle(cx).is_focused(window)));

    // New note from the tree: filed in the selection's folder.
    cx.simulate_keystrokes("secondary-n");
    cx.simulate_input("# Sous");
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(root.join("Projets/Sous.md").is_file());

    // Separator: it sets the width, and collapses the panel at the end of its travel.
    let drag = |cx: &mut gpui::VisualTestContext, from: f32, to: f32| {
        let at = |x: f32| point(px(x), px(300.));
        cx.simulate_mouse_down(at(from), MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(at(to), MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(at(to), MouseButton::Left, gpui::Modifiers::none());
    };
    drag(cx, 302., 450.);
    assert_eq!(vault::load_layout(), "tree split 410 0\n");
    drag(cx, 452., 60.);
    assert_eq!(shell.read_with(cx, |s, _| s.nav.panel), Panel::Rail);

    // Recent notes, full screen (Ctrl+M), then Escape gives back the room and focus to the note.
    cx.simulate_keystrokes("secondary-shift-t secondary-m");
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Recent, Panel::Full));
    cx.simulate_keystrokes("down escape");
    assert_eq!(shell.read_with(cx, |s, _| s.nav.panel), Panel::Split);
    assert_eq!(text(cx), "# Sous");
    // Asking again for the shown mode: first focus, then collapsing to the rail.
    cx.simulate_keystrokes("secondary-shift-t secondary-shift-t");
    assert_eq!(vault::load_layout(), "recent rail 410 0\n");

    // Graph: one edge per resolved wikilink. Tab walks the notes linked to the
    // open note, as a preview; Enter opens the selected one.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("cou");
    cx.simulate_keystrokes("enter secondary-g");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| (s.nav.mode, s.nav.panel)), (Mode::Graph, Panel::Split));
    assert_eq!(shell.read_with(cx, |s, cx| s.graph.read(cx).size()), (5, 2));
    // A node dragged with the mouse follows the pointer.
    let spot = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, cx| s.graph.read(cx).spot(&root.join("Courses.md")))
    };
    let from = spot(cx);
    let to = from + point(px(40.), px(30.));
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    assert!((spot(cx).x - to.x).abs() < px(1.) && (spot(cx).y - to.y).abs() < px(1.));
    let mut linked = Vec::new();
    for _ in 0..2 {
        cx.simulate_keystrokes("tab");
        linked.push(shell.read_with(cx, |s, _| vault::stem(s.pane().path.as_ref().unwrap())));
        // A preview, or the tab of a note already open: neither counts as an opening.
        assert!(shell.read_with(cx, |s, _| s.recent[0] == root.join("Courses.md")));
    }
    linked.sort();
    assert_eq!(linked, ["Plan", "Test"]);
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| !s.pane().preview && s.recent[0] == *s.pane().path.as_ref().unwrap()));
    assert!(cx.update(|window, cx| shell.read(cx).pane().editor.focus_handle(cx).is_focused(window)));
    // Full screen: the selection no longer moves the note; Enter gives it its place back.
    cx.simulate_keystrokes("secondary-g secondary-m tab");
    let open = shell.read_with(cx, |s, _| s.pane().path.clone());
    assert!(shell.read_with(cx, |s, _| s.nav.panel == Panel::Full && s.nav.sel != s.pane().path));
    cx.simulate_keystrokes("enter");
    assert!(shell.read_with(cx, |s, _| s.nav.panel == Panel::Split && s.pane().path != open));
}
