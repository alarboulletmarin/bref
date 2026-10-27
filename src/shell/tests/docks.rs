//! Docks: the side panel's tools on the left or the right of the panes, or as a tab.

use super::*;
use crate::actions::{MoveTool, ToolAsTab};

#[gpui::test]
fn tools_move_between_docks_and_tabs(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "docks");
    let note = |name: &str| root.join(format!("{name}.md"));
    fs::write(note("Alpha"), "# Alpha\n\n[[Beta]]\n").unwrap();
    fs::write(note("Beta"), "# Beta\n\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(1600.), px(900.)));
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&note("Beta"), window, cx);
        s.show_nav(Mode::Links, false, window, cx);
    });
    cx.run_until_parked();
    let places = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, _| (s.nav.right.clone(), s.left_open().then(|| s.nav.left_mode()), s.right_open().then(|| s.nav.right_mode())))
    };
    let bounds = |cx: &mut gpui::VisualTestContext, name: &str| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        cx.debug_bounds(leak(name.to_string())).unwrap_or_else(|| panic!("{name}"))
    };

    // The backlinks go to the right dock; the left dock shows the tree in their place.
    cx.dispatch_action(MoveTool);
    cx.run_until_parked();
    assert_eq!(places(cx), (vec![Mode::Links], Some(Mode::Tree), Some(Mode::Links)));
    let (left, pane, right) = (bounds(cx, "nav-panel"), bounds(cx, "pane-0"), bounds(cx, "dock-right"));
    assert!(left.right() <= pane.left() + px(0.5) && pane.right() <= right.left() + px(0.5) && right.right() <= px(1600.5), "{left:?} {pane:?} {right:?}");
    // Both lists are drawn at once, each from its own rows: a click in the right one opens what
    // links here, and the right dock is the one the keys reach afterwards.
    let alpha = bounds(cx, "nav-row-Alpha");
    assert!(alpha.left() >= right.left(), "the backlink is in the right dock: {alpha:?}");
    cx.simulate_click(alpha.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| (s.pane().path.clone(), s.nav.right_active)), (Some(note("Alpha")), true));
    let beta = bounds(cx, "nav-row-Beta");
    assert!(beta.right() <= pane.left(), "the tree is still on the left: {beta:?}");
    cx.simulate_click(beta.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| (s.pane().path.clone(), s.nav.right_active)), (Some(note("Beta")), false));

    // The graph as a tab: its dock lets it go; closing the tab gives it back.
    shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Graph, false, window, cx));
    cx.dispatch_action(ToolAsTab);
    cx.run_until_parked();
    let tab = shell.read_with(cx, |s, _| s.tool_tab(Mode::Graph)).expect("a tab shows the graph");
    assert_eq!(shell.read_with(cx, |s, cx| (s.active, s.tab_name(tab, cx), s.left_open())), (tab, "Graphe".into(), false));
    assert!(shell.read_with(cx, |s, _| s.tool_shown(Mode::Graph)));
    // The rail reaches the tab, not a dock.
    shell.update_in(cx, |s, window, cx| {
        s.focus_pane(0, window, cx);
        s.show_nav(Mode::Graph, true, window, cx);
    });
    assert_eq!(shell.read_with(cx, |s, _| s.active), tab);
    cx.simulate_keystrokes("secondary-w");
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| (s.tool_tab(Mode::Graph), s.left_open(), s.nav.left_mode())), (None, true, Mode::Graph));

    // Restart: the backlinks on the right, the graph in its tab.
    cx.dispatch_action(ToolAsTab);
    cx.run_until_parked();
    let (again, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx2.run_until_parked();
    let back = again.read_with(cx2, |s, _| (s.nav.right.clone(), s.right_open(), s.tool_tab(Mode::Graph).is_some()));
    assert_eq!(back, (vec![Mode::Links], true, true));

    // A narrow window: the right dock hides before the panes get under their minimum, and comes
    // back with the room.
    // (The left dock is empty, its graph in a tab: at 700 px the right one still fits.)
    cx.simulate_resize(size(px(700.), px(720.)));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.right_width()) >= px(160.));
    let (pane, right) = (bounds(cx, "pane-0"), bounds(cx, "dock-right"));
    assert!(pane.size.width >= pane::MIN_PANE - px(1.) && pane.right() <= right.left() + px(0.5), "{pane:?} {right:?}");
    cx.simulate_resize(size(px(480.), px(720.)));
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| s.right_width()), px(0.));
    cx.simulate_resize(size(px(1600.), px(900.)));
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.right_width()) >= px(160.));

    // Back to the left: the right dock, empty, closes.
    shell.update_in(cx, |s, window, cx| s.show_nav(Mode::Links, false, window, cx));
    cx.dispatch_action(MoveTool);
    cx.run_until_parked();
    assert_eq!(places(cx), (vec![], Some(Mode::Links), None));
}

#[gpui::test]
fn the_layout_file_old_new_and_odd(cx: &mut TestAppContext) {
    let (root, _shell, cx, _config) = start(cx, "docks-file");
    let read = |cx: &mut gpui::VisualTestContext, text: &str| {
        vault::save_layout(text);
        let (shell, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
        cx2.run_until_parked();
        shell.read_with(cx2, |s, _| (s.nav.left_mode(), s.nav.right.clone(), s.nav.right_open, s.nav.right_mode()))
    };
    // Before docks: everything on the left, as it was.
    assert_eq!(read(cx, "tags split 300 0\n"), (Mode::Tags, vec![], false, Mode::Tree));
    // A tool on both sides keeps the left; an unknown one is dropped; one shown on the right that
    // does not live there is not shown.
    assert_eq!(read(cx, "graph split 300 0 right=links,bogus,graph,links show=tags width=250\n"), (Mode::Graph, vec![Mode::Links], false, Mode::Links));
    assert_eq!(read(cx, "tree rail 260 0 width=x right=calendar show=calendar\n"), (Mode::Tree, vec![Mode::Calendar], true, Mode::Calendar));
    // Empty or absent: the defaults.
    assert_eq!(read(cx, ""), (Mode::Tree, vec![], false, Mode::Tree));
}
