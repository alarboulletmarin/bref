//! The side panel's tools placed: in the left dock (the panel as it always was), in the right
//! dock, or as a tab of a pane (the graph, the calendar). Each dock shows one tool at a time.

use super::*;
use crate::views::nav::{DIVIDER, RAIL};

impl Shell {
    /// The tab that shows the tool, if it is open as one.
    pub(super) fn tool_tab(&self, mode: Mode) -> Option<usize> {
        self.panes.iter().position(|pane| pane.tool == Some(mode))
    }

    /// The left dock is open on a tool that lives there (not moved right, not open as a tab).
    pub(super) fn left_open(&self) -> bool {
        let mode = self.nav.left_mode();
        self.nav.panel != Panel::Rail && !self.nav.right.contains(&mode) && self.tool_tab(mode).is_none()
    }

    pub(super) fn right_open(&self) -> bool {
        let mode = self.nav.right_mode();
        self.nav.right_open && self.nav.right.contains(&mode) && self.nav.panel != Panel::Full && self.tool_tab(mode).is_none()
    }

    /// The tool is in sight: in its dock, or as a tab.
    pub(super) fn tool_shown(&self, mode: Mode) -> bool {
        self.tool_tab(mode).is_some() || self.left_open() && self.nav.left_mode() == mode || self.right_open() && self.nav.right_mode() == mode
    }

    /// The tool comes into its dock's view, which comes into `Nav`, and that dock opens.
    pub(super) fn put_tool(&mut self, mode: Mode) {
        let right = self.nav.right.contains(&mode);
        self.nav.activate(right);
        if self.nav.mode != mode {
            (self.nav.mode, self.nav.rows_from) = (mode, 0);
        }
        match right {
            true => self.nav.right_open = true,
            false if self.nav.panel == Panel::Rail => self.nav.panel = Panel::Split,
            false => {}
        }
    }

    /// Width of the right dock: the one it was given, as long as the panes keep `MIN_PANE`; none
    /// (the dock hides) when even 160 px would take it from them.
    pub(super) fn right_width(&self) -> Pixels {
        let left = if self.left_open() && self.nav.panel == Panel::Split { self.split_width() + DIVIDER } else { px(0.) };
        let room = self.nav.total - self.theme.px(RAIL) - left - self.nav.aside - DIVIDER - pane::MIN_PANE;
        match room < px(160.) {
            true => px(0.),
            false => self.nav.right_w.clamp(px(160.), room),
        }
    }

    /// The right dock's border follows the pointer.
    pub(super) fn drag_right(&mut self, x: Pixels, cx: &mut Context<Self>) {
        self.nav.right_w = (self.nav.left + self.nav.total - x).max(px(160.));
        cx.notify();
    }

    /// The tool of the active dock goes to the other dock, and is shown there; the dock it leaves
    /// shows another of its tools, or folds.
    pub(super) fn move_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (mode, right) = (self.nav.mode, self.nav.right_active);
        match right {
            true => {
                self.nav.right.retain(|m| *m != mode);
                match self.nav.right.first() {
                    Some(&next) => (self.nav.mode, self.nav.rows_from) = (next, 0),
                    None => self.nav.right_open = false,
                }
            }
            false => {
                self.nav.right.push(mode);
                match Mode::ALL.into_iter().find(|m| !self.nav.right.contains(m)) {
                    Some(next) => (self.nav.mode, self.nav.rows_from) = (next, 0),
                    None => self.nav.panel = Panel::Rail,
                }
            }
        }
        self.put_tool(mode);
        self.nav.reveal = true;
        window.focus(&self.nav.focus);
        self.refresh_graph(cx);
        self.settle_nav(window, cx);
    }

    /// The tool of the active dock (the graph, the calendar) opens as a tab of the active pane;
    /// closing the tab gives it back to its dock.
    pub(super) fn tool_as_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // From the palette, with a list in the dock: the graph.
        let mode = Some(self.nav.mode).filter(|m| m.tabs()).unwrap_or(Mode::Graph);
        if self.tool_tab(mode).is_none() {
            self.fresh_tab(window, cx);
            self.pane_mut().tool = Some(mode);
            self.refresh_graph(cx);
        }
        let tab = self.tool_tab(mode).unwrap_or(self.active);
        self.focus_pane(tab, window, cx);
        self.settle_nav(window, cx);
    }
}
