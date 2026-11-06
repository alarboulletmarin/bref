//! The side panel's tools placed: stacked in the left dock (the panel as it always was), stacked
//! in the right dock, or as a tab of a pane (the graph, the calendar).

use super::*;
use crate::views::nav::{DIVIDER, RAIL};

/// Where a dragged tool would land: the right dock or the left one, at that place in its stack.
pub(super) type ToolAim = (bool, usize);

/// A tool's header being dragged: a small card with its name follows the pointer.
#[derive(Clone)]
pub(super) struct ToolDragged {
    pub mode: Mode,
    pub name: String,
    pub theme: Theme,
}

impl Render for ToolDragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        crate::ui::widgets::floating(t)
            .px(t.px(10.))
            .py(t.px(5.))
            .max_w(t.px(220.))
            .truncate()
            .rounded(t.tab())
            .text_size(t.font_body())
            .text_color(t.text)
            .child(self.name.clone())
    }
}

impl Shell {
    /// The tab that shows the tool, if it is open as one.
    pub(super) fn tool_tab(&self, mode: Mode) -> Option<usize> {
        self.panes.iter().position(|pane| pane.tool() == Some(mode))
    }

    /// The tools open as a tab of some pane.
    fn tabbed(&self) -> Vec<Mode> {
        self.panes.iter().filter_map(|pane| pane.tool()).collect()
    }

    pub(super) fn dock_tools(&self, right: bool) -> Vec<Mode> {
        self.nav.dock_tools(right, &self.tabbed())
    }

    pub(super) fn left_open(&self) -> bool {
        self.nav.left_open(&self.tabbed())
    }

    pub(super) fn right_open(&self) -> bool {
        self.nav.right_open(&self.tabbed())
    }

    /// The tool is in sight: in its dock, or as a tab.
    pub(super) fn tool_shown(&self, mode: Mode) -> bool {
        let tabs = self.tabbed();
        tabs.contains(&mode) || self.nav.docked(mode, &tabs)
    }

    /// The tool's view comes into `Nav` and its dock opens on it.
    pub(super) fn put_tool(&mut self, mode: Mode) {
        let tabs = self.tabbed();
        self.nav.put_tool(mode, &tabs);
    }

    /// Shift+click on the rail: the tool joins the others of its dock, at the bottom, or leaves
    /// them if it is shown.
    pub(super) fn stack_tool(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = self.tabbed();
        if tabs.contains(&mode) {
            return self.show_nav(mode, false, window, cx);
        }
        match self.nav.docked(mode, &tabs) {
            true => self.hide_tool(mode, window, cx),
            false => {
                self.nav.stack_tool(mode, &tabs);
                window.focus(&self.nav.focus());
                self.refresh_graph(cx);
                self.settle_nav(window, cx);
            }
        }
    }

    /// The tool leaves its dock's stack (see `Nav::hide_tool`).
    pub(super) fn hide_tool(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = self.tabbed();
        let right = self.nav.right().contains(&mode);
        let had_focus = self.nav.focus().contains_focused(window, cx);
        self.nav.hide_tool(mode, &tabs);
        // The keys go to another tool in sight, or back to the note.
        match self.nav.next_docked(right, &tabs) {
            Some(next) if had_focus => self.nav.activate(next),
            _ if had_focus => window.focus(&self.pane().editor.focus_handle(cx)),
            _ => {}
        }
        self.settle_nav(window, cx);
    }

    /// A tool stacked in a dock folds down to its header, or unfolds.
    pub(super) fn fold_tool(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.nav.toggle_fold(mode);
        self.nav.save();
        cx.notify();
    }

    /// Width of the right dock: the one it was given, as long as the panes keep `MIN_PANE`; none
    /// (the dock hides) when even 160 px would take it from them.
    pub(super) fn right_width(&self) -> Pixels {
        let left = if self.left_open() && self.nav.panel() == Panel::Split { self.split_width() + DIVIDER } else { px(0.) };
        let room = self.nav.total() - self.theme.px(RAIL) - left - self.nav.aside() - DIVIDER - pane::MIN_PANE;
        match room < px(160.) {
            true => px(0.),
            false => self.nav.right_w().clamp(px(160.), room),
        }
    }

    /// The right dock's border follows the pointer.
    pub(super) fn drag_right(&mut self, x: Pixels, cx: &mut Context<Self>) {
        self.nav.drag_right(x);
        cx.notify();
    }

    /// The active tool goes to the bottom of the other dock's stack.
    pub(super) fn move_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.nav.mode();
        let right = !self.nav.right().contains(&mode);
        let at = self.dock_tools(right).len();
        self.place_tool(mode, right, at, window, cx);
    }

    /// Puts `mode` at place `at` of the stack of the right dock (`right`) or the left one, by a
    /// drag of its header or the keys (see `Nav::place_tool`); it gets the keys.
    pub(super) fn place_tool(&mut self, mode: Mode, right: bool, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = self.tabbed();
        self.nav.place_tool(mode, right, at, &tabs);
        window.focus(&self.nav.focus());
        self.refresh_graph(cx);
        self.settle_nav(window, cx);
    }

    /// Follows a tool's header dragged over a dock: the place it would take there.
    pub(super) fn aim_tool(&mut self, aim: Option<ToolAim>, cx: &mut Context<Self>) {
        if aim != self.tool_aim {
            self.tool_aim = aim;
            cx.notify();
        }
    }

    /// The tool of the active dock (the graph, the calendar) opens as a tab of the active pane;
    /// closing the tab gives it back to its dock.
    pub(super) fn tool_as_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // From the palette, with a list in the dock: the graph.
        let mode = Some(self.nav.mode()).filter(|m| m.tabs()).unwrap_or(Mode::Graph);
        if self.tool_tab(mode).is_none() {
            self.fresh_tab(window, cx);
            self.pane_mut().set_tool(mode);
            self.refresh_graph(cx);
        }
        let tab = self.tool_tab(mode).unwrap_or(self.active);
        self.focus_pane(tab, window, cx);
        self.settle_nav(window, cx);
    }
}
