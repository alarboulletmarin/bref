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
        div()
            .px(t.px(10.))
            .py(t.px(5.))
            .max_w(t.px(220.))
            .truncate()
            .rounded(t.large())
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow(t.lifted())
            .text_size(t.px(13.))
            .text_color(t.text)
            .child(self.name.clone())
    }
}

impl Shell {
    /// The tab that shows the tool, if it is open as one.
    pub(super) fn tool_tab(&self, mode: Mode) -> Option<usize> {
        self.panes.iter().position(|pane| pane.tool == Some(mode))
    }

    /// The tools drawn in the left dock (or the right one), top to bottom: those shown there that
    /// live there and are not open as a tab.
    pub(super) fn dock_tools(&self, right: bool) -> Vec<Mode> {
        let shown = if right { &self.nav.right_shown } else { &self.nav.left_shown };
        shown.iter().copied().filter(|m| self.nav.right.contains(m) == right && self.tool_tab(*m).is_none()).collect()
    }

    pub(super) fn left_open(&self) -> bool {
        self.nav.panel != Panel::Rail && !self.dock_tools(false).is_empty()
    }

    pub(super) fn right_open(&self) -> bool {
        self.nav.panel != Panel::Full && !self.dock_tools(true).is_empty()
    }

    /// The tool is drawn in a dock that is open.
    fn docked(&self, mode: Mode) -> bool {
        self.left_open() && self.dock_tools(false).contains(&mode) || self.right_open() && self.dock_tools(true).contains(&mode)
    }

    /// The tool is in sight: in its dock, or as a tab.
    pub(super) fn tool_shown(&self, mode: Mode) -> bool {
        self.tool_tab(mode).is_some() || self.docked(mode)
    }

    /// The tool's view comes into `Nav` and its dock opens on it: alone, as the panel always did,
    /// unless it is already shown there with others.
    pub(super) fn put_tool(&mut self, mode: Mode) {
        let right = self.nav.right.contains(&mode);
        if !self.docked(mode) {
            match right {
                true => self.nav.right_shown = vec![mode],
                false => self.nav.left_shown = vec![mode],
            }
        }
        if !right && self.nav.panel == Panel::Rail {
            self.nav.panel = Panel::Split;
        }
        self.nav.folded.remove(&mode);
        self.nav.activate(mode);
    }

    /// Shift+click on the rail: the tool joins the others of its dock, at the bottom, or leaves
    /// them if it is shown.
    pub(super) fn stack_tool(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool_tab(mode).is_some() {
            return self.show_nav(mode, false, window, cx);
        }
        let right = self.nav.right.contains(&mode);
        match self.docked(mode) {
            true => self.hide_tool(mode, window, cx),
            false => {
                // A folded left dock opens on this tool alone.
                let fresh = !right && !self.left_open();
                let stack = if right { &mut self.nav.right_shown } else { &mut self.nav.left_shown };
                if fresh {
                    stack.clear();
                }
                stack.retain(|m| *m != mode);
                stack.push(mode);
                if !right && self.nav.panel == Panel::Rail {
                    self.nav.panel = Panel::Split;
                }
                self.nav.folded.remove(&mode);
                self.nav.activate(mode);
                self.nav.reveal = true;
                window.focus(&self.nav.focus);
                self.refresh_graph(cx);
                self.settle_nav(window, cx);
            }
        }
    }

    /// The tool leaves its dock's stack; the last one of the left dock folds the panel onto its
    /// rail instead, so that it comes back the same.
    pub(super) fn hide_tool(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let right = self.nav.right.contains(&mode);
        let had_focus = self.nav.focus.contains_focused(window, cx);
        match right {
            true => self.nav.right_shown.retain(|m| *m != mode),
            false if self.dock_tools(false) == [mode] => self.nav.panel = Panel::Rail,
            false => self.nav.left_shown.retain(|m| *m != mode),
        }
        // The keys go to another tool in sight, or back to the note.
        match self.dock_tools(right).first().or(self.dock_tools(!right).first()).copied().filter(|m| self.docked(*m)) {
            Some(next) if had_focus => self.nav.activate(next),
            _ if had_focus => window.focus(&self.pane().editor.focus_handle(cx)),
            _ => {}
        }
        self.settle_nav(window, cx);
    }

    /// A tool stacked in a dock folds down to its header, or unfolds.
    pub(super) fn fold_tool(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if !self.nav.folded.remove(&mode) {
            self.nav.folded.insert(mode);
        }
        self.nav.save();
        cx.notify();
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

    /// The active tool goes to the bottom of the other dock's stack.
    pub(super) fn move_tool(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.nav.mode;
        let right = !self.nav.right.contains(&mode);
        let at = self.dock_tools(right).len();
        self.place_tool(mode, right, at, window, cx);
    }

    /// Puts `mode` at place `at` of the stack of the right dock (`right`) or the left one, by a
    /// drag of its header or the keys; it is shown there and gets the keys. The left dock, left
    /// without tools, folds onto its rail.
    pub(super) fn place_tool(&mut self, mode: Mode, right: bool, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        // The place counts the tools drawn; the tool itself, if it was above, leaves a gap.
        let drawn = self.dock_tools(right);
        let at = match drawn.iter().position(|m| *m == mode) {
            Some(was) if was < at => at - 1,
            _ => at,
        };
        let before = drawn.iter().filter(|m| **m != mode).nth(at).copied();
        let left_drawn = self.left_open();
        self.nav.left_shown.retain(|m| *m != mode);
        self.nav.right_shown.retain(|m| *m != mode);
        self.nav.right.retain(|m| *m != mode);
        let stack = match right {
            true => {
                self.nav.right.push(mode);
                &mut self.nav.right_shown
            }
            false => {
                // A folded left dock opens on this tool alone.
                if !left_drawn {
                    self.nav.left_shown.clear();
                }
                &mut self.nav.left_shown
            }
        };
        let at = before.and_then(|b| stack.iter().position(|m| *m == b)).unwrap_or(stack.len());
        stack.insert(at, mode);
        match (right, self.dock_tools(false).is_empty()) {
            (false, _) => self.nav.panel = if self.nav.panel == Panel::Full { Panel::Full } else { Panel::Split },
            // Its last tool gone, the left dock shows another one that lives there; none left, it folds.
            (true, true) => match Mode::ALL.into_iter().find(|m| !self.nav.right.contains(m) && self.tool_tab(*m).is_none()) {
                Some(other) => self.nav.left_shown = vec![other],
                None => self.nav.panel = Panel::Rail,
            },
            (true, false) => {}
        }
        self.nav.folded.remove(&mode);
        self.nav.activate(mode);
        self.nav.reveal = true;
        window.focus(&self.nav.focus);
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
