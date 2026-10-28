//! The tab bar of a pane: one row over what it shows, as soon as it holds two tabs.

use super::*;

/// Height of the tab bar, at scale 1: a row of the side panel.
const TAB_BAR: f32 = 34.;

/// A tab being dragged: a small card with its name follows the pointer.
#[derive(Clone)]
pub(super) struct TabDragged {
    pub tab: usize,
    name: String,
    theme: Theme,
}

impl Render for TabDragged {
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
    /// What a tab is called: its file, its list page, or a new note's first line.
    pub(super) fn tab_name(&self, tab: usize, cx: &App) -> String {
        let pane = &self.panes[tab];
        match pane.tool {
            Some(Mode::Graph) => return tr("Graph", "Graphe").to_string(),
            Some(_) => return tr("Calendar", "Calendrier").to_string(),
            None => {}
        }
        if let Some(listing) = &pane.listing {
            return match nav::tag_of(&listing.of) {
                Some(tag) => format!("#{tag}"),
                None => vault::stem(&listing.of),
            };
        }
        if let Some(path) = pane.file_path().or(pane.path.as_ref()) {
            return vault::stem(path);
        }
        let first = pane.editor.read(cx).text().lines().next().unwrap_or_default().trim_start_matches('#').trim();
        match first.is_empty() {
            true => tr("New note", "Nouvelle note").to_string(),
            false => first.chars().take(80).collect(),
        }
    }

    /// The labels of a pane's tabs, in order; two tabs of the same name (two folders) say their folder.
    pub(super) fn tab_labels(&self, group: usize, cx: &App) -> Vec<String> {
        let tabs = &self.groups[group].tabs;
        let names: Vec<String> = tabs.iter().map(|&tab| self.tab_name(tab, cx)).collect();
        tabs.iter()
            .zip(&names)
            .map(|(&tab, name)| {
                let pane = &self.panes[tab];
                let twin = names.iter().filter(|other| *other == name).count() > 1;
                let folder = pane.file_path().or(pane.path.as_ref()).and_then(|p| p.parent()).and_then(|p| p.file_name());
                match folder.filter(|_| twin) {
                    Some(folder) => format!("{name} · {}", folder.to_string_lossy()),
                    None => name.clone(),
                }
            })
            .collect()
    }

    /// Where a dragged tab would land in a bar; the pane under it is not aimed at meanwhile.
    fn aim_tab(&mut self, aim: Option<(usize, usize)>, cx: &mut Context<Self>) {
        if aim != self.tab_aim {
            self.tab_aim = aim;
            if aim.is_some() {
                self.pane_aim = None;
            }
            cx.notify();
        }
    }

    /// Ctrl+W or the cross of a tab: that tab closes; input stays where it was unless it was there.
    pub(super) fn close_tab(&mut self, tab: usize, window: &mut Window, cx: &mut Context<Self>) {
        let keep = (self.active != tab).then_some(self.active);
        self.active = tab;
        self.close_pane(window, cx);
        if let Some(keep) = keep {
            self.focus_pane(if keep > tab { keep - 1 } else { keep }, window, cx);
        }
    }

    /// The tab bar of `group`, if it has more than one tab. The tab shown is underlined, in the
    /// accent in the active pane; one that is not saved yet has a dot in place of its cross.
    pub(super) fn render_tabs(&mut self, group: usize, client: bool, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let t = self.theme;
        let tabs = self.groups[group].tabs.clone();
        if tabs.len() < 2 {
            return None;
        }
        let shown = self.groups[group].shown;
        // Brought into view once when it changes, so the wheel stays free afterwards. gpui scrolls with
        // the places of the last frame: a bar not drawn yet (at startup) asks for one more frame.
        if self.groups[group].seen != Some(shown)
            && let Some(at) = tabs.iter().position(|tab| *tab == shown)
        {
            let scroll = &self.groups[group].scroll;
            scroll.scroll_to_item(at);
            if scroll.bounds_for_item(at).is_some() {
                self.groups[group].seen = Some(shown);
            } else {
                let me = cx.entity();
                cx.defer(move |cx| me.update(cx, |_, cx| cx.notify()));
            }
        }
        let active = self.group() == group;
        let labels = self.tab_labels(group, cx);
        // Each tab is a box and its text; the cross and the dot are drawn only where they show (the
        // tab shown, the one under the pointer), so that a long bar stays cheap to draw.
        // ponytail: every tab is laid out at each frame (about 20 µs a tab: 2 ms for a hundred, measured
        // by `hundred_tabs`); draw only those in view, or cache the bar as its own view, if that matters.
        let hovered = self.tab_hover;
        // While a tab is dragged over this bar: a line where it would land.
        let aim = self.tab_aim.filter(|(over, _)| *over == group).map(|(_, at)| at);
        let count = tabs.len();
        let line = move || div().absolute().top_0().bottom_0().w(t.px(2.)).bg(t.accent).debug_selector(|| "tab-aim".into());
        let cells = tabs.iter().zip(labels).enumerate().map(|(at, (&tab, label))| {
            let pane = &self.panes[tab];
            let (on, over) = (tab == shown, hovered == Some(tab));
            let slot = div().size(t.px(16.)).flex_none().flex().items_center().justify_center();
            let slot = match (over || on && !pane.dirty, pane.dirty) {
                (true, _) => slot.child(
                    div()
                        .id(("tab-close", tab))
                        .debug_selector(move || format!("tab-close-{tab}"))
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.small())
                        .hover(|s| s.bg(t.hover))
                        .child(svg().path("close.svg").size(t.px(12.)).text_color(t.soft))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| this.close_tab(tab, window, cx))),
                ),
                // Not saved yet: a dot, which turns into the cross under the pointer.
                (false, true) => slot.child(div().size(t.px(6.)).rounded_full().bg(t.soft)),
                (false, false) => slot,
            };
            div()
                .id(("tab", tab))
                .debug_selector(move || format!("tab-{tab}"))
                .relative()
                .flex_none()
                .max_w(t.px(200.))
                .h_full()
                .pl(t.px(12.))
                .pr(t.px(6.))
                .flex()
                .items_center()
                .gap(t.px(6.))
                .border_r_1()
                .border_color(t.border)
                .cursor_pointer()
                .text_color(if on || over { t.text } else { t.soft })
                .when(pane.preview, |d| d.italic())
                .child(div().min_w_0().truncate().child(label.clone()))
                .child(slot)
                .when(aim == Some(at), |d| d.child(line().left_0()))
                .when(aim == Some(count) && at + 1 == count, |d| d.child(line().right_0()))
                // Taken, it follows the pointer; over a tab, the half under the pointer says on
                // which side of it it would land.
                .on_drag(TabDragged { tab, name: label, theme: t }, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<TabDragged>, _, cx| {
                    let p = e.event.position;
                    if e.bounds.contains(&p) {
                        this.aim_tab(Some((group, at + usize::from(p.x > e.bounds.center().x))), cx);
                    }
                }))
                // Under the tab shown: the accent in the active pane, quieter in the other.
                .when(on, |d| d.child(div().absolute().left_0().right_0().bottom_0().h(t.px(2.)).bg(if active { t.accent } else { t.soft })))
                .on_hover(cx.listener(move |this, hover: &bool, _, cx| {
                    let now = if *hover { Some(tab) } else { this.tab_hover.filter(|t| *t != tab) };
                    if now != this.tab_hover {
                        this.tab_hover = now;
                        cx.notify();
                    }
                }))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| this.focus_pane(tab, window, cx)))
                .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| this.close_tab(tab, window, cx)))
        });
        // The window buttons sit over the top right corner: the right pane's bar stops before them,
        // so that no tab scrolls under them.
        let corner = client && group == self.corner_group();
        Some(
            div()
                .id(("tabs", group))
                .debug_selector(move || format!("tabs-{group}"))
                .flex_none()
                .w_full()
                .h(t.px(TAB_BAR))
                .flex()
                .border_b_1()
                .border_color(t.border)
                .text_size(t.px(13.))
                .when(corner, |d| d.pr(t.px(render::PILL_ROOM)))
                // Past the last tab, it lands at the end; out of the bar, nowhere in it.
                .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<TabDragged>, _, cx| {
                    let mine = this.tab_aim.is_some_and(|(over, _)| over == group);
                    match e.bounds.contains(&e.event.position) {
                        true if !mine => this.aim_tab(Some((group, count)), cx),
                        false if mine => this.aim_tab(None, cx),
                        _ => {}
                    }
                }))
                .on_drop(cx.listener(move |this, dragged: &TabDragged, window, cx| {
                    if let Some((_, at)) = this.tab_aim.take().filter(|(over, _)| *over == group) {
                        this.place_tab(dragged.tab, pane::Place::Bar(group, at), window, cx);
                    }
                }))
                .child(
                    div()
                        .id(("tabs-row", group))
                        .debug_selector(move || format!("tabs-row-{group}"))
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .overflow_x_scroll()
                        .track_scroll(&self.groups[group].scroll)
                        .children(cells)
                        // Past the last tab: a handle to move the window, where the app draws its title bar.
                        .child(div().debug_selector(move || format!("tabs-free-{group}")).flex_1().h_full().when(client, |d| d.map(crate::ui::widgets::drag_window))),
                ),
        )
    }
}
