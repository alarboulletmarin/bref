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
        shell.read_with(cx, |s, _| (s.nav.right.clone(), if s.left_open() { s.dock_tools(false) } else { vec![] }, if s.right_open() { s.dock_tools(true) } else { vec![] }))
    };
    let bounds = |cx: &mut gpui::VisualTestContext, name: &str| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        cx.debug_bounds(leak(name.to_string())).unwrap_or_else(|| panic!("{name}"))
    };

    // The backlinks go to the right dock; the left dock shows the tree in their place.
    cx.dispatch_action(MoveTool);
    cx.run_until_parked();
    assert_eq!(places(cx), (vec![Mode::Links], vec![Mode::Tree], vec![Mode::Links]));
    let (left, pane, right) = (bounds(cx, "nav-panel"), bounds(cx, "pane-0"), bounds(cx, "dock-right"));
    assert!(left.right() <= pane.left() + px(0.5) && pane.right() <= right.left() + px(0.5) && right.right() <= px(1600.5), "{left:?} {pane:?} {right:?}");
    // No line between the zones: each resize handle is the 8 px of ground between its dock and the note, found
    // by the pointer though nothing is drawn there at rest.
    let (grip, dock_grip) = (bounds(cx, "nav-divider"), bounds(cx, "dock-divider"));
    assert_eq!((grip.size.width, grip.left(), grip.right()), (px(8.), left.right(), pane.left()), "{grip:?} {left:?}");
    assert_eq!((dock_grip.size.width, dock_grip.left(), dock_grip.right()), (px(8.), pane.right(), right.left()), "{dock_grip:?} {right:?}");
    // Both lists are drawn at once, each from its own rows: a click in the right one opens what
    // links here, and the right dock is the one the keys reach afterwards.
    let alpha = bounds(cx, "nav-row-Alpha");
    assert!(alpha.left() >= right.left(), "the backlink is in the right dock: {alpha:?}");
    cx.simulate_click(alpha.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| (s.pane().path.clone(), s.nav.mode)), (Some(note("Alpha")), Mode::Links));
    let beta = bounds(cx, "nav-row-Beta");
    assert!(beta.right() <= pane.left(), "the tree is still on the left: {beta:?}");
    cx.simulate_click(beta.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(shell.read_with(cx, |s, _| (s.pane().path.clone(), s.nav.mode)), (Some(note("Beta")), Mode::Tree));

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
    assert_eq!(shell.read_with(cx, |s, _| (s.tool_tab(Mode::Graph), s.left_open(), s.dock_tools(false))), (None, true, vec![Mode::Graph]));

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
    assert_eq!(places(cx), (vec![], vec![Mode::Links], vec![]));
}

#[gpui::test]
fn tools_stacked_folded_and_dragged(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "docks-stack");
    let note = |name: &str| root.join(format!("{name}.md"));
    fs::write(note("Alpha"), "# Alpha\n\n[[Beta]] #projet\n").unwrap();
    fs::write(note("Beta"), "# Beta\n\n").unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    cx.simulate_resize(size(px(1600.), px(720.)));
    shell.update_in(cx, |s, window, cx| {
        s.open_from_nav(&note("Beta"), window, cx);
        s.show_nav(Mode::Tree, false, window, cx);
    });
    let bounds = |cx: &mut gpui::VisualTestContext, name: &str| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        cx.debug_bounds(leak(name.to_string())).unwrap_or_else(|| panic!("{name}"))
    };
    let tools = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| (s.dock_tools(false), if s.right_open() { s.dock_tools(true) } else { vec![] }));
    let none = gpui::Modifiers::none();
    let shift = gpui::Modifiers { shift: true, ..Default::default() };

    // Shift+click on the rail stacks the tools of the left dock; a plain click shows one alone.
    for id in ["nav-links", "nav-tags", "nav-recent", "nav-graph", "nav-calendar"] {
        let at = bounds(cx, id).center();
        cx.simulate_click(at, shift);
    }
    assert_eq!(tools(cx).0, [Mode::Tree, Mode::Links, Mode::Tags, Mode::Recent, Mode::Graph, Mode::Calendar]);
    // Six tools in 720 px: each keeps its header, one under the other, none over the next.
    let mut above = px(0.);
    for mode in tools(cx).0 {
        let b = bounds(cx, &format!("tool-{}", mode.name()));
        assert!(b.top() >= above - px(0.5) && b.size.height >= px(44.) - px(0.5), "{mode:?} {b:?}");
        above = b.bottom();
    }
    assert!(above <= px(720.5), "the stack fits the window: {above:?}");
    // Folded, a tool keeps only its header and leaves its room to the others.
    let tags = bounds(cx, "tool-tags").size.height;
    let at = bounds(cx, "tool-fold-tags").center();
    cx.simulate_click(at, none);
    let folded = bounds(cx, "tool-tags").size.height;
    assert!((folded - px(44.)).abs() < px(2.) && folded < tags, "{folded:?} {tags:?}");
    // Its cross hides it from the stack.
    let at = bounds(cx, "tool-close-recent").center();
    cx.simulate_click(at, none);
    assert_eq!(tools(cx).0, [Mode::Tree, Mode::Links, Mode::Tags, Mode::Graph, Mode::Calendar]);

    // Its title dragged onto the right edge, with the right dock closed: a strip takes it there.
    let hold = |cx: &mut gpui::VisualTestContext, title: &str, to: Point<Pixels>| {
        let from = bounds(cx, title).center();
        cx.simulate_mouse_down(from, MouseButton::Left, none);
        cx.simulate_mouse_move(from + point(px(12.), px(2.)), MouseButton::Left, none);
        cx.simulate_mouse_move(to, MouseButton::Left, none);
        cx.run_until_parked();
    };
    hold(cx, "tool-title-links", point(px(1000.), px(300.)));
    let strip = bounds(cx, "dock-right-drop");
    cx.simulate_mouse_move(strip.center(), MouseButton::Left, none);
    assert_eq!(shell.read_with(cx, |s, _| s.tool_aim), Some((true, 0)));
    cx.simulate_mouse_up(strip.center(), MouseButton::Left, none);
    assert_eq!(tools(cx), (vec![Mode::Tree, Mode::Tags, Mode::Graph, Mode::Calendar], vec![Mode::Links]));
    // Over the upper half of the right dock's only tool: above it, a line says so.
    let links = bounds(cx, "tool-links");
    let top = point(links.center().x, links.top() + px(60.));
    hold(cx, "tool-title-calendar", top);
    assert_eq!(shell.read_with(cx, |s, _| s.tool_aim), Some((true, 0)));
    assert!(bounds(cx, "tool-aim").top() <= links.top() + px(1.));
    cx.simulate_mouse_up(top, MouseButton::Left, none);
    assert_eq!(tools(cx), (vec![Mode::Tree, Mode::Tags, Mode::Graph], vec![Mode::Calendar, Mode::Links]));
    // Within its own dock: the tree goes under the graph.
    let graph = bounds(cx, "tool-graph");
    let below = point(graph.center().x, graph.bottom() - px(10.));
    hold(cx, "tool-title-tree", below);
    cx.simulate_mouse_up(below, MouseButton::Left, none);
    assert_eq!(tools(cx).0, [Mode::Tags, Mode::Graph, Mode::Tree]);
    // Escape halfway: nothing moves.
    hold(cx, "tool-title-tags", below);
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(below, MouseButton::Left, none);
    assert_eq!(tools(cx).0, [Mode::Tags, Mode::Graph, Mode::Tree]);

    // Restart: both stacks, in their order, the folded tool folded.
    let (again, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
    cx2.run_until_parked();
    let back = again.read_with(cx2, |s, _| (s.dock_tools(false), s.dock_tools(true), s.nav.folded.contains(&Mode::Tags)));
    assert_eq!(back, (vec![Mode::Tags, Mode::Graph, Mode::Tree], vec![Mode::Calendar, Mode::Links], true));

    // A plain click on the rail: that tool alone in its dock, as the panel always did.
    let at = bounds(cx, "nav-recent").center();
    cx.simulate_click(at, none);
    assert_eq!(tools(cx).0, [Mode::Recent]);
}

#[gpui::test]
fn the_layout_file_old_new_and_odd(cx: &mut TestAppContext) {
    let (root, _shell, cx, _config) = start(cx, "docks-file");
    let read = |cx: &mut gpui::VisualTestContext, text: &str| {
        vault::save_layout(text);
        let (shell, cx2) = cx.add_window_view(|window, cx| Shell::new(Some(root.clone()), Vec::new(), window, cx));
        cx2.run_until_parked();
        shell.read_with(cx2, |s, _| (s.nav.left_shown.clone(), s.nav.right.clone(), s.nav.right_shown.clone()))
    };
    use Mode::*;
    // Before docks: everything on the left, as it was.
    assert_eq!(read(cx, "tags split 300 0\n"), (vec![Tags], vec![], vec![]));
    // A tool on both sides keeps the left; an unknown one is dropped; one shown on the right that
    // does not live there is not shown.
    assert_eq!(read(cx, "graph split 300 0 right=links,bogus,graph,links show=tags width=250\n"), (vec![Graph], vec![Links], vec![]));
    assert_eq!(read(cx, "tree rail 260 0 width=x right=calendar show=calendar\n"), (vec![Tree], vec![Calendar], vec![Calendar]));
    assert_eq!(read(cx, "tree split 260 0 left=links,tree,links right=links,tags show=tags,recent fold=links,zzz\n"), (vec![Links, Tree], vec![Tags], vec![Tags]));
    // Empty or absent: the defaults.
    assert_eq!(read(cx, ""), (vec![Tree], vec![], vec![]));
}
