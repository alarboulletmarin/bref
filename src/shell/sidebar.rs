//! The side panel as the shell drives it: rail, tree, recent notes, tags, graph, calendar, list pages, context menu and file operations.

use std::{
    collections::HashSet,
    hash::{BuildHasher, BuildHasherDefault, DefaultHasher, Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
};
use gpui::{
    Action, ClickEvent, Context, CursorStyle, Div, Focusable, MouseButton,
    MouseDownEvent, Pixels, ScrollStrategy, Stateful, Window, div, point, prelude::*, px, svg, uniform_list,
};
use crate::{
    ui::widgets::{button, tip}, views::graph,
    logic::markdown::{self, Date},
    views::palette::{Palette, PaletteEvent, Setting},
    lang::tr,
    platform::vault,
    logic::note::Note,
};

use super::*;
use crate::ui::assets::ICON_SET;
use crate::views::nav::*;

impl Shell {
    /// Shows the panel in this mode and gives it focus. Asking again for the mode
    /// shown collapses the panel (from the keyboard: only if it already has focus).
    pub(super) fn show_nav(&mut self, mode: Mode, toggle: bool, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.nav.panel != Panel::Rail && self.nav.mode == mode;
        if shown && (toggle || self.nav.focus.contains_focused(window, cx)) {
            self.nav.panel = Panel::Rail;
        } else {
            self.nav.mode = mode;
            if self.nav.panel == Panel::Rail {
                self.nav.panel = Panel::Split;
            }
            self.nav.reveal = true;
            window.focus(&self.nav.focus);
            self.refresh_graph(cx);
        }
        self.settle_nav(window, cx);
    }

    /// Updates the graph if it is shown and the notes or their links changed.
    pub(super) fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        if self.graph_stale && self.nav.panel != Panel::Rail && self.nav.mode == Mode::Graph {
            self.graph_stale = false;
            let notes: Vec<Note> = self.notes.iter().filter(|n| !self.is_template(n)).cloned().collect();
            self.graph.update(cx, |graph, cx| graph.set_notes(&notes, &vault::pictures(&self.images), cx));
        }
    }

    /// A graph node is selected: preview of the note if it has room.
    pub(super) fn select_from_graph(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav.panel == Panel::Split {
            self.preview_note(path, window, cx);
        }
        self.nav.sel = Some(path.to_path_buf());
        cx.notify();
    }

    pub(super) fn toggle_full(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.panel = match self.nav.panel {
            Panel::Full => Panel::Split,
            _ => Panel::Full,
        };
        self.settle_nav(window, cx);
    }

    /// After a panel change: focus leaves what is no longer
    /// shown, and the layout is saved.
    pub(super) fn settle_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.dragging = false;
        let editor = self.pane().editor.focus_handle(cx);
        match self.nav.panel {
            Panel::Rail if self.nav.focus.contains_focused(window, cx) => window.focus(&editor),
            Panel::Full if editor.is_focused(window) => window.focus(&self.nav.focus),
            _ => {}
        }
        self.nav.save();
        cx.notify();
    }

    /// Width of the panel open beside the note: the one it was given, as long as the note
    /// (`NOTE_MIN`) and what is reserved to its right keep their room.
    pub(super) fn split_width(&self) -> Pixels {
        let room = self.nav.total - self.theme.px(RAIL) - DIVIDER - self.nav.aside;
        // The month can no longer be read below 170 px: its days touch.
        let floor = px(if self.nav.mode == Mode::Calendar { 170. } else { 120. });
        match if self.nav.mode == Mode::Graph { self.nav.graph_w } else { self.nav.list_w } {
            w if w == px(0.) => room / 2.,
            w => w.min(room - NOTE_MIN).max(floor),
        }
    }

    /// The separator follows the pointer; at the ends, one side collapses.
    pub(super) fn drag_nav(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let x = x - self.nav.left - self.theme.px(RAIL);
        let room = self.nav.total - self.theme.px(RAIL);
        self.nav.panel = if x < px(100.) {
            Panel::Rail
        } else if x > room - px(140.) {
            Panel::Full
        } else {
            *self.nav.width() = x;
            Panel::Split
        };
        cx.notify();
    }

    /// Hash of everything the panel's rows depend on: recomputing them
    /// costs ten times more than checking they are still right.
    pub(super) fn rows_source(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let open = self.nav.open.iter().map(|dir| BuildHasherDefault::<DefaultHasher>::default().hash_one(dir)).fold(0u64, u64::wrapping_add);
        (self.nav.mode, self.nav.panel == Panel::Rail, &self.vault, &self.dirs, &self.images, open).hash(&mut hasher);
        if self.nav.mode == Mode::Links {
            self.nav.anchor.hash(&mut hasher);
        }
        if self.nav.mode == Mode::Recent {
            self.recent.hash(&mut hasher);
        }
        for note in &self.notes {
            (&note.path, &note.name).hash(&mut hasher);
            if self.nav.mode == Mode::Tags {
                note.tags.hash(&mut hasher);
            }
            if self.nav.mode == Mode::Links {
                (&note.links, &note.aliases).hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    pub(super) fn nav_rows(&self) -> Vec<Row> {
        let Some(root) = &self.vault else {
            return Vec::new();
        };
        match self.nav.mode {
            Mode::Tree => tree_rows(root, &self.notes, &self.dirs, &self.images, &self.nav.open),
            Mode::Graph | Mode::Calendar | Mode::Search => Vec::new(),
            Mode::Tags => tag_rows(&self.notes, &self.nav.open),
            Mode::Links => (self.nav.anchor.iter())
                .flat_map(|target| graph::backlinks(&self.notes, target))
                .map(|n| Row { path: n.path.clone(), name: n.name.clone(), depth: 0, dir: None })
                .collect(),
            Mode::Recent => self
                .by_recency()
                .into_iter()
                .map(|n| Row {
                    path: n.path.clone(),
                    name: n.name.clone(),
                    depth: 0,
                    dir: None,
                })
                .collect(),
        }
    }

    pub(super) fn selected_row(&self) -> Option<usize> {
        let sel = self.nav.sel.as_ref()?;
        self.nav.rows.iter().position(|r| r.path == *sel)
    }

    /// Selects the row; a note shows as a preview if it has room.
    pub(super) fn nav_select(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let (path, is_note) = (row.path.clone(), row.dir.is_none());
        self.nav.marked.clear();
        self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        if is_note && self.nav.panel == Panel::Split {
            self.preview_note(&path, window, cx);
        }
        self.nav.sel = Some(path);
        self.nav.reveal = false;
        cx.notify();
    }

    /// Ctrl+click adds the row to the selection or removes it; Shift+click
    /// selects everything since the last chosen row.
    pub(super) fn nav_mark(&mut self, ix: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(path) = self.nav.rows.get(ix).map(|r| r.path.clone()) else {
            return;
        };
        let marked = &mut self.nav.marked;
        if extend {
            let from = self.nav.sel.as_ref().and_then(|sel| self.nav.rows.iter().position(|r| r.path == *sel)).unwrap_or(ix);
            *marked = self.nav.rows[from.min(ix)..=from.max(ix)].iter().map(|r| r.path.clone()).collect();
            self.nav.sel.get_or_insert(path);
        } else {
            // The row already selected is part of the batch.
            if marked.is_empty() {
                marked.extend(self.nav.sel.take());
            }
            if !marked.remove(&path) {
                marked.insert(path.clone());
            }
            // Removed, it leaves its chosen-row place to another of the batch.
            let other = self.nav.rows.iter().map(|r| &r.path).find(|p| marked.contains(*p)).cloned();
            self.nav.sel = if marked.contains(&path) { Some(path) } else { other.or(Some(path)) };
        }
        cx.notify();
    }

    /// What an action on `target` aims at: the whole multiple selection if it
    /// is part of it, in panel order, without what a targeted folder already contains.
    pub(super) fn targets(&self, target: &Path) -> Vec<PathBuf> {
        let marked = &self.nav.marked;
        if !marked.contains(target) {
            return vec![target.to_path_buf()];
        }
        let mut seen = HashSet::new();
        let rows = self.nav.rows.iter().map(|r| &r.path);
        rows.filter(|p| marked.contains(*p) && !p.ancestors().skip(1).any(|a| marked.contains(a)) && seen.insert(*p))
            .cloned()
            .collect()
    }

    /// Calendar: moves the chosen day by `days` days, or by `months` months.
    pub(super) fn day_step(&mut self, days: i64, months: i32, cx: &mut Context<Self>) {
        let moved = markdown::day_of(markdown::day_number(self.nav.day) + days);
        self.nav.day = markdown::add_months(moved, months);
        cx.notify();
    }

    /// Opens the daily note for `day`, or creates it: the one `@` and Ctrl+J point to.
    pub(super) fn open_day(&mut self, day: Date, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.day = day;
        self.open_wiki(&crate::lang::date_name(day), window, cx);
        cx.notify();
    }

    /// The month of the chosen day: one cell per day, marked if it has its note. The keyboard
    /// arrows change day, Page Up and Down change month, Enter opens the note.
    pub(super) fn render_calendar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let (year, month, chosen) = self.nav.day;
        // The week starts on Monday in French, on Sunday in English.
        let monday = tr("sunday", "monday") == "monday";
        let (lead, days) = markdown::month(year, month, monday);
        let names: HashSet<&str> = self.notes.iter().map(|n| n.name.as_str()).collect();
        let (today, open) = (crate::platform::clock::today(), self.pane().path.as_deref().map(vault::stem));
        let (en, fr) = markdown::MONTHS[month as usize - 1];
        let step = |id: &'static str, icon: &'static str, by: i32, cx: &mut Context<Self>| {
            button(id, icon, false, t).on_click(cx.listener(move |this, _, _, cx| this.day_step(0, by, cx)))
        };
        let head = div()
            .flex()
            .items_center()
            .child(div().flex_1().min_w_0().truncate().pl_1().child(format!("{} {year}", tr(en, fr))))
            .child(step("month-prev", "chevron-left.svg", -1, cx))
            .child(step("month-next", "chevron-right.svg", 1, cx));
        // Seven equal columns, which shrink with the panel instead of going under the note.
        let cell = || div().h(t.px(30.)).flex().items_center().justify_center();
        let initials = if monday { tr("MTWTFSS", "LMMJVSD") } else { tr("SMTWTFS", "DLMMJVS") };
        let week = initials.chars().map(|c| cell().text_color(t.soft).text_size(t.px(11.)).child(c.to_string()));
        let blanks = (0..lead).map(|_| cell().into_any_element());
        let cells = (1..=days).map(|day| {
            let date = (year, month, day);
            let name = crate::lang::date_name(date);
            let has = names.contains(name.as_str());
            cell()
                .id(("day", day as usize))
                .rounded(t.px(6.))
                .cursor_pointer()
                .text_color(if has || day == chosen { t.text } else { t.soft })
                .when(has, |d| d.font_weight(gpui::FontWeight::BOLD))
                .when(open.as_deref() == Some(name.as_str()), |d| d.text_color(t.accent))
                .when(date == today, |d| d.border_1().border_color(t.accent.opacity(0.6)))
                .when(day == chosen, |d| d.bg(t.selection))
                .hover(|s| s.bg(t.code_bg))
                .child(day.to_string())
                .on_click(cx.listener(move |this, _, window, cx| this.open_day(date, window, cx)))
                .into_any_element()
        });
        div()
            .flex_1()
            .min_h_0()
            .px_2()
            .text_size(t.px(13.))
            .flex()
            .flex_col()
            .gap_1()
            .child(head)
            .child(div().debug_selector(|| "calendar".into()).w_full().max_w(t.px(224.)).grid().grid_cols(7).children(week).children(blanks).children(cells))
    }

    pub(super) fn nav_step(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav.mode == Mode::Calendar {
            return self.day_step(if down { 7 } else { -7 }, 0, cx);
        }
        let last = self.nav.rows.len().saturating_sub(1);
        let ix = match self.selected_row() {
            Some(ix) if down => (ix + 1).min(last),
            Some(ix) => ix.saturating_sub(1),
            None => 0,
        };
        self.nav_select(ix, window, cx);
    }

    /// Enter or double-click: opens the note for good, or toggles the folder.
    pub(super) fn nav_activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let path = row.path.clone();
        self.nav.marked.clear();
        if row.dir.is_some() {
            if !self.nav.open.remove(&path) {
                self.nav.open.insert(path.clone());
            }
            self.nav.sel = Some(path);
            cx.notify();
        } else {
            self.open_from_nav(&path, window, cx);
        }
    }

    /// Opens the note and puts focus in it; the note gets its place back if the
    /// panel took the whole window.
    pub(super) fn open_from_nav(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.open(path, window, cx);
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        // A table takes focus so one can write in it; an image shows in place
        // of the note, and the panel keeps control.
        match self.shown_sheet() {
            Some(sheet) => window.focus(&sheet.focus_handle(cx)),
            None if self.pane().file.is_none() => window.focus(&self.pane().editor.focus_handle(cx)),
            None => {}
        }
        self.settle_nav(window, cx);
    }

    /// Left/right in the tree: collapse or expand, otherwise go up or down.
    pub(super) fn nav_fold(&mut self, unfold: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav.mode == Mode::Calendar {
            return self.day_step(if unfold { 1 } else { -1 }, 0, cx);
        }
        let Some(ix) = self.selected_row() else {
            return self.nav_step(true, window, cx);
        };
        let row = self.nav.rows[ix].clone();
        match (row.dir, unfold) {
            (Some(false), true) => {
                self.nav.open.insert(row.path);
            }
            (Some(true), false) => {
                self.nav.open.remove(&row.path);
            }
            (_, true) => return self.nav_step(true, window, cx),
            (_, false) => {
                let parent = row.path.parent();
                if let Some(up) = self.nav.rows.iter().position(|r| Some(r.path.as_path()) == parent) {
                    return self.nav_select(up, window, cx);
                }
            }
        }
        cx.notify();
    }

    /// Collapses all the tree's folders; if they already are, expands them all.
    pub(super) fn fold_all(&mut self) {
        if self.all_folded() {
            self.nav.open = self.dirs.iter().cloned().collect();
        } else {
            self.nav.open.clear();
        }
    }

    /// No vault folder is expanded. `open` also keeps the parents of the
    /// vault itself, hence the folder-by-folder test.
    pub(super) fn all_folded(&self) -> bool {
        !self.dirs.iter().any(|d| self.nav.open.contains(d))
    }

    /// Asks for a name in an input field, then calls `then` with that name cleaned up.
    pub(crate) fn ask(
        &mut self,
        label: impl Into<String>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let theme = self.theme;
        let prompt = cx.new(|cx| Palette::prompt(label, text, theme, cx));
        cx.subscribe_in(&prompt, window, move |this, _, event, window, cx| {
            this.palette = None;
            window.focus(&this.nav.focus);
            if let PaletteEvent::Submit(text) = event
                && let Some(name) = vault::clean_name(text)
            {
                then(this, name, window, cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&prompt.focus_handle(cx));
        self.palette = Some(prompt);
        cx.notify();
    }

    /// Closes the icon grid, if it is open, and gives the focus back.
    pub(super) fn close_icons(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(back) = self.icon_pick.take().and_then(|pick| pick.back) {
            window.focus(&back);
        }
        cx.notify();
    }

    /// Gives cell `i` of the grid (0: no icon) to the item, and closes the grid.
    fn pick_icon(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.icon_pick.as_ref().map(|pick| pick.path.clone()) else { return };
        self.close_icons(window, cx);
        let name = i.checked_sub(1).and_then(|i| ICON_SET.get(i)).map(|(file, _)| &file[2..file.len() - 4]);
        self.set_icon(path, name, cx)
    }

    /// Arrows (eight cells a row), Home, End, Enter and Space in the icon grid.
    fn icons_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let n = ICON_SET.len() + 1;
        let Some(pick) = self.icon_pick.as_mut() else { return false };
        let at = pick.chosen;
        pick.chosen = match key {
            "right" => (at + 1) % n,
            "left" => (at + n - 1) % n,
            "down" => (at + 8).min(n - 1),
            "up" => at.saturating_sub(8),
            "home" => 0,
            "end" => n - 1,
            "enter" | "space" => {
                self.pick_icon(at, window, cx);
                return true;
            }
            _ => return false,
        };
        cx.notify();
        true
    }

    /// Icon grid, opened from the menu: a click or Enter gives the icon, the cross removes it,
    /// a click elsewhere or Esc closes it.
    pub(super) fn render_icons(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let IconPick { at, path, chosen, .. } = self.icon_pick.as_ref()?;
        let (at, chosen) = (*at, *chosen);
        let t = self.theme;
        let mine = self.icons.get(path).cloned();
        let pick = |i: usize, file: &'static str, name: Option<&'static str>, cx: &mut Context<Self>| {
            button(("icon", i), file, name.is_some() && name == mine.as_deref(), t)
                .when(i == chosen, |d| d.border_2().border_color(t.accent))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.pick_icon(i, window, cx)
                    }),
                )
        };
        let mut cells = vec![pick(0, "close.svg", None, cx)];
        cells.extend(ICON_SET.iter().enumerate().map(|(i, (file, _))| pick(i + 1, file, Some(&file[2..file.len() - 4]), cx)));
        // Eight per row; the grid stays in the window, like the menu.
        let view = window.viewport_size();
        let (width, height) = (px(8. * 30. - 2. + 12.), px(cells.len().div_ceil(8) as f32 * 30. - 2. + 12.));
        let at = point(
            (at.x - self.nav.left).min(view.width - self.nav.left * 2. - width - t.px(8.)),
            (at.y - self.nav.left).min(view.height - self.nav.left * 2. - height - t.px(8.)),
        );
        let close = cx.listener(|this: &mut Self, _: &MouseDownEvent, window, cx| this.close_icons(window, cx));
        Some(
            div().absolute().inset_0().occlude().on_mouse_down(MouseButton::Left, close).child(
                div()
                    .debug_selector(|| "icons".into())
                    .track_focus(&self.menu_focus)
                    .key_context("Menu")
                    .on_action(cx.listener(|this, _: &CloseHelp, window, cx| this.close_icons(window, cx)))
                    .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                        if !e.keystroke.modifiers.modified() && this.icons_key(&e.keystroke.key, window, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .absolute()
                    .left(at.x)
                    .top(at.y)
                    .w(width)
                    .p(t.px(5.))
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .rounded(t.px(8.))
                    .shadow_lg()
                    .flex()
                    .flex_wrap()
                    .gap(t.px(2.))
                    .children(cells),
            ),
        )
    }

    pub(super) fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let t = self.theme;
        let focused = self.nav.focus.is_focused(window);
        let tree = self.nav.mode == Mode::Tree;
        let marked = &self.nav.marked;
        // Dragging a row of the multiple selection carries the whole batch.
        let lot = marked.iter().next().map(|any| self.targets(any)).unwrap_or_default();
        range
            .filter_map(|ix| {
                let row = self.nav.rows.get(ix)?;
                let selected = match marked.is_empty() {
                    true => self.nav.sel.as_ref() == Some(&row.path),
                    false => marked.contains(&row.path),
                };
                let current = self.pane().path.as_ref() == Some(&row.path);
                // In the recent list, the note's folder serves as detail.
                let detail = match (&self.vault, row.path.parent()) {
                    (Some(root), Some(dir)) if !tree => dir
                        .strip_prefix(root)
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                // Under a tag, the number of notes that carry it.
                let detail = match row.dir {
                    Some(_) if !tree => {
                        let tag = &row.name[1..];
                        self.notes.iter().filter(|n| n.tags.iter().any(|t| t == tag)).count().to_string()
                    }
                    _ => detail,
                };
                // Under backlinks, the line of the note that carries the link.
                let links = self.nav.mode == Mode::Links;
                let detail = match self.nav.anchor.as_ref().filter(|_| links) {
                    Some(target) => {
                        let note = |path: &Path| self.notes.iter().find(|n| n.path == path);
                        let line = note(target).zip(note(&row.path)).and_then(|(aimed, from)| graph::link_line(&from.body, aimed));
                        // An embedded section is said to be one.
                        line.map(|line| match embed::parse(line) {
                            Some(_) => format!("{} · {}", tr("embedded", "incorporé"), line.trim()),
                            None => line.trim().to_string(),
                        })
                        .unwrap_or_default()
                    }
                    None => detail,
                };
                // A folder of the tree that cannot be read says so and why, instead of looking empty.
                let closed = self.unreadable.iter().find(|(dir, _)| tree && *dir == row.path);
                let detail = match closed {
                    Some((_, why)) => format!("{} {}", tr("unreadable:", "illisible :"), crate::lang::io_reason(&std::io::Error::from(*why))),
                    None => detail,
                };
                let wide = links || closed.is_some();
                let said = closed.map(|_| detail.clone());
                let foldable = !matches!(self.nav.mode, Mode::Recent | Mode::Links);
                let chevron = match row.dir {
                    Some(true) => "chevron-down.svg",
                    Some(false) => "chevron-right.svg",
                    None => "",
                };
                let line = div()
                    .size_full()
                    .pr_2()
                        .pl(px(8. + 14. * row.depth as f32))
                        .flex()
                        .items_center()
                        .gap_1()
                        .rounded(t.px(5.))
                        .text_size(t.px(13.))
                        .when(!selected, |d| d.hover(|s| s.bg(t.code_bg)))
                        .when(selected, |d| {
                            let at = self.nav.sel_at.clone();
                            d.bg(if focused { t.selection } else { t.selection.opacity(0.5) })
                                .relative()
                                .child(gpui::canvas(move |bounds, _, _| at.set(Some(bounds)), |_, _, _, _| {}).absolute().size_full())
                        })
                        .when(foldable, |d| {
                            d.child(div().size(t.px(14.)).flex_none().when(row.dir.is_some(), |d| {
                                d.child(svg().path(chevron).size(t.px(14.)).text_color(if selected { t.text } else { t.soft }))
                            }))
                        })
                        .children(self.icons.get(&row.path).and_then(|name| ICON_SET.iter().find(|(file, _)| file[2..file.len() - 4] == *name)).map(
                            |(file, _)| svg().path(*file).size(t.px(14.)).flex_none().text_color(if current { t.accent } else { t.text }),
                        ))
                        .child(
                            div()
                                .truncate()
                                // The name goes first, in full as long as it fits; the link's line, or the
                                // reason a folder cannot be read, takes what is left.
                                .map(|d| match (closed.is_some(), wide) {
                                    // The reason a folder cannot be read is said in full; its name gives way.
                                    (true, _) => d.flex_1().min_w(t.px(40.)),
                                    (_, true) => d.flex_none().max_w(gpui::relative(0.7)),
                                    _ => d.flex_1(),
                                })
                                .when(current, |d| d.text_color(t.accent))
                                .child(row.name.clone()),
                        )
                        .child(
                            div()
                                .debug_selector({
                                    let why = format!("nav-why-{}", row.name);
                                    move || why.clone()
                                })
                                .map(|d| match (closed.is_some(), wide) {
                                    // ponytail: in a panel too narrow for it, the reason is cut last, in full in the row's tooltip.
                                    (true, _) => d.flex_initial().min_w_0(),
                                    (_, true) => d.flex_1().min_w_0(),
                                    _ => d.flex_none().max_w(t.px(110.)),
                                })
                                .truncate()
                                .text_size(t.px(11.5))
                                .text_color(if selected { t.text } else { t.soft })
                                .child(detail),
                        );
                let (path, name) = (row.path.clone(), row.name.clone());
                // Dropping on a note files into that note's folder.
                let into = match row.dir {
                    Some(_) => Some(path.clone()),
                    None => path.parent().map(Path::to_path_buf),
                };
                let (target, drop_into) = (path.clone(), into.clone());
                let (paths, name) = match marked.contains(&path) {
                    true => (lot.clone(), format!("{} {}", lot.len(), tr("items", "éléments"))),
                    false => (vec![path], name),
                };
                let selector = format!("nav-row-{}", row.name);
                Some(
                    div()
                        .id(ix)
                        .debug_selector(move || selector.clone())
                        .w_full()
                        .h(t.px(ROW))
                        .px_1()
                        .cursor_pointer()
                        .child(line)
                        .when_some(said, |d, said| d.tooltip(tip(said, String::new(), t)))
                        .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                            let held = e.modifiers();
                            if held.secondary() || held.shift {
                                return this.nav_mark(ix, held.shift, cx);
                            }
                            // A click opens the note, in its tab; a second click finds it there.
                            this.nav_activate(ix, window, cx)
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                // Outside the multiple selection, right-click replaces it.
                                if !this.nav.marked.contains(&target) {
                                    this.nav.marked.clear();
                                    this.nav.sel = Some(target.clone());
                                }
                                // A tag has only one entry: the list of its notes.
                                window.focus(&this.nav.focus);
                                if this.vault.as_ref().is_some_and(|root| target.starts_with(root)) || tag_of(&target).is_some() {
                                    this.show_menu(Menu { at: e.position, target: Some(target.clone()), ..Default::default() }, window, cx);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        // Files coming from the system: they go into the row's folder, or into the
                        // note's, and the row tints to say so.
                        .when(tree, |d| {
                            let aim_into = drop_into.clone();
                            d.drag_over::<gpui::ExternalPaths>(move |style, _, _, _| style.bg(t.selection))
                                .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                                    let Some(dir) = &aim_into else { return };
                                    let mine = matches!(&this.drop_aim, Some(super::DropAim { aim: super::Aim::Folder(d), .. }) if d == dir);
                                    if e.bounds.contains(&e.event.position) {
                                        let aim = super::DropAim { aim: super::Aim::Folder(dir.clone()), items: e.drag(cx).paths().len(), pointer: e.event.position };
                                        this.aim_drop(Some(aim), cx);
                                    } else if mine {
                                        this.aim_drop(None, cx);
                                    }
                                }))
                                .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                                    if let Some(dir) = &drop_into {
                                        this.drop_paths(paths.paths().to_vec(), super::Aim::Folder(dir.clone()), cx);
                                    }
                                }))
                        })
                        .when(tree, |d| {
                            d.on_drag(Dragged { paths, name, theme: t }, |dragged, _, _, cx| {
                                cx.new(|_| dragged.clone())
                            })
                            .when(row.dir.is_some(), |d| {
                                d.drag_over::<Dragged>(move |style, _, _, _| style.bg(t.selection))
                            })
                            .on_drop(cx.listener(move |this, dragged: &Dragged, _, cx| {
                                if let Some(into) = &into {
                                    this.move_all(&dragged.paths, into, cx)
                                }
                            }))
                        }),
                )
            })
            .collect()
    }

    /// Rail, then panel and separator if they are expanded.
    pub(super) fn render_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let (mode, panel) = (self.nav.mode, self.nav.panel);
        let unfolded = panel != Panel::Rail;
        if !self.pane().preview {
            self.nav.anchor = self.pane().path.clone();
        }
        let source = self.rows_source();
        if source != self.nav.rows_from {
            self.nav.rows = if unfolded { self.nav_rows() } else { Vec::new() };
            self.nav.rows_from = source;
        }
        let (current, selected) = (self.pane().path.clone(), self.nav.sel.clone());
        self.graph.update(cx, |graph, _| graph.sync(t, current.as_deref(), selected.as_deref()));
        if std::mem::take(&mut self.nav.reveal)
            && let Some(ix) = self.selected_row()
        {
            self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }

        let travel = |id: &'static str, icon: &'static str, on: bool| {
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .w(t.px(28.))
                .h(t.px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(5.))
                .when(on, |d| d.cursor_pointer().hover(|s| s.bg(t.border)).active(|s| s.bg(t.selection)))
                .child(svg().path(icon).size(t.px(14.)).flex_none().text_color(t.soft).when(!on, |s| s.opacity(0.35)))
        };
        let mode_button = |id, icon, m: Mode, label: &'static str, keys: String| {
            button(id, icon, unfolded && mode == m, t)
                .tooltip(tip(label, keys, t))
                .on_click(cx.listener(move |this, _, window, cx| this.show_nav(m, true, window, cx)))
        };
        let keys = |action: &dyn Action| crate::ui::keys::of(cx, action);
        let rail = div()
            .id("rail")
            .flex_none()
            .w(t.px(RAIL))
            .h_full()
            // A short window: the rail scrolls, its last buttons stay reachable.
            .overflow_y_scroll()
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .when(!unfolded, |d| d.border_r_1().border_color(t.border))
            // The app logo, at the top of the rail on every system.
            .child(div().debug_selector(|| "rail-logo".into()).mb_1().child(crate::ui::assets::logo(t)))
            .child(
                button("nav-commands", "menu.svg", false, t)
                    .debug_selector(|| "nav-commands".into())
                    .tooltip(tip(tr("Commands", "Commandes"), keys(&crate::actions::ToggleCommands), t))
                    .on_click(cx.listener(|this, _, window, cx| this.open_commands(window, cx))),
            )
            // Back and forward, one above the other (side by side, two 24 px targets do not fit the rail);
            // dimmed when there is nowhere to go.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        travel("nav-back", "chevron-left.svg", self.history.can_back())
                            .tooltip(tip(tr("Back", "Précédent"), keys(&crate::actions::GoBack), t))
                            .on_click(cx.listener(|this, _, window, cx| this.travel(false, window, cx))),
                    )
                    .child(
                        travel("nav-forward", "chevron-right.svg", self.history.can_forward())
                            .tooltip(tip(tr("Forward", "Suivant"), keys(&crate::actions::GoForward), t))
                            .on_click(cx.listener(|this, _, window, cx| this.travel(true, window, cx))),
                    ),
            )
            .child(mode_button("nav-tree", "tree.svg", Mode::Tree, tr("Vault tree", "Arbre du coffre"), keys(&ShowTree)))
            .child(mode_button("nav-recent", "clock.svg", Mode::Recent, tr("Recent notes", "Notes récentes"), keys(&ShowRecent)))
            .child(mode_button("nav-graph", "graph.svg", Mode::Graph, tr("Graph of the notes", "Graphe des notes"), keys(&ShowGraph)))
            .child(mode_button("nav-tags", "tag.svg", Mode::Tags, "Tags", keys(&ShowTags)))
            .child(mode_button("nav-links", "backlink.svg", Mode::Links, tr("Backlinks", "Rétroliens"), keys(&ShowLinks)))
            .child(mode_button("nav-calendar", "i-calendar.svg", Mode::Calendar, tr("Calendar", "Calendrier"), keys(&ShowCalendar)))
            .child(
                button("nav-find", "find-text.svg", unfolded && mode == Mode::Search, t)
                    .debug_selector(|| "nav-find".into())
                    .tooltip(tip(tr("Search the text of the vault", "Chercher dans le texte du coffre"), keys(&crate::actions::SearchVault), t))
                    .on_click(cx.listener(|this, _, window, cx| match this.nav.panel != Panel::Rail && this.nav.mode == Mode::Search {
                        true => this.show_nav(Mode::Search, true, window, cx),
                        false => this.open_vault_find(window, cx),
                    })),
            )
            .child(div().w(t.px(16.)).h(t.px(1.)).my_1().bg(t.border))
            .child(
                button("nav-search", "search.svg", false, t)
                    .tooltip(tip(tr("Find or create a note", "Chercher ou créer une note"), keys(&crate::actions::OpenPalette), t))
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette("", window, cx))),
            )
            .child(
                button("nav-new", "plus.svg", false, t)
                    .tooltip(tip(tr("New note", "Nouvelle note"), keys(&crate::actions::NewNote), t))
                    .on_click(cx.listener(|this, _, window, cx| this.new_note_here(window, cx))),
            )
            .child(div().flex_1().w_full().when(!self.nav.native_bar, |d| d.map(crate::ui::widgets::drag_window)))
            // Vault trash (what it holds, to take it back) and backup.
            .child(
                button("nav-trash", "trash.svg", false, t)
                    .tooltip(tip(tr("Trash: restore a note", "Corbeille : restaurer une note"), String::new(), t))
                    .on_click(cx.listener(|this, _, window, cx| this.open_trash(window, cx))),
            )
            // A vault kept in git: its sync, turning while it runs, with a mark when the last one failed.
            .when(self.vault.as_ref().is_some_and(|root| root.join(".git").exists()), |d| {
                let (busy, broken) = (self.syncing, self.sync_broken);
                let said = if broken {
                    tr("Sync the vault (the last sync failed)", "Synchroniser le coffre (la dernière synchronisation a échoué)")
                } else {
                    tr("Sync the vault with git", "Synchroniser le coffre par git")
                };
                let mut sync = button("nav-sync", if busy { "spinner.svg" } else { "sync.svg" }, busy, t)
                    .debug_selector(|| "nav-sync".into())
                    .relative()
                    .tooltip(tip(said, keys(&crate::actions::SyncVault), t))
                    .on_click(cx.listener(|this, _, window, cx| this.git_sync(window, cx)));
                if broken && !busy {
                    sync = sync.child(div().debug_selector(|| "nav-sync-broken".into()).absolute().top(t.px(3.)).right(t.px(3.)).size(t.px(7.)).rounded_full().bg(t.hue(crate::ui::theme::RED, t.panel)));
                }
                d.child(sync)
            })
            .child(
                button("nav-backup", "backup.svg", false, t)
                    .tooltip(tip(tr("Back up the vault", "Sauvegarder le coffre"), String::new(), t))
                    .on_click(cx.listener(|this, _, window, cx| this.choose_backup(window, cx))),
            )
            .child(button("nav-theme", "theme.svg", false, t).tooltip(tip(tr("Choose a theme…", "Choisir un thème…"), String::new(), t)).on_click(cx.listener(
                |this, _, window, cx| this.choose_setting(Setting::Theme, window, cx),
            )))
            .child(
                button("nav-help", "help.svg", false, t)
                    .tooltip(tip(tr("Shortcuts and help", "Raccourcis et aide"), keys(&crate::actions::ToggleHelp), t))
                    .on_click(cx.listener(|this, _, window, cx| this.set_help(true, window, cx))),
            );

        let wrapper = div().h_full().flex().min_w_0();
        if !unfolded {
            return wrapper.flex_none().child(rail);
        }

        let full = panel == Panel::Full;
        let folded = self.all_folded();
        let title = match (mode, &self.vault) {
            (Mode::Tree, Some(root)) => vault::stem(root),
            (Mode::Graph, _) => tr("Graph", "Graphe").to_string(),
            (Mode::Tags, _) => "Tags".to_string(),
            (Mode::Calendar, _) => tr("Calendar", "Calendrier").to_string(),
            (Mode::Search, _) => tr("Search", "Recherche").to_string(),
            (Mode::Links, _) => {
                let name = self.nav.anchor.as_deref().map(vault::stem).unwrap_or_default();
                format!("{} {name}", tr("Links to", "Liens vers"))
            }
            _ => tr("Recent", "Récents").to_string(),
        };
        let header = div()
            .flex_none()
            .h(t.px(44.))
            .pl_2()
            // Over the whole window, the window buttons float over the right end of the header.
            .pr(if full && !self.nav.native_bar { t.px(super::render::PILL_ROOM) } else { t.px(4.) })
            // A narrow panel at a large scale: its buttons are cut at its edge, never drawn over the note
            // (they are in the palette too).
            .overflow_hidden()
            .flex()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .truncate()
                    .text_size(t.px(12.))
                    .text_color(t.soft)
                    .when(!self.nav.native_bar, |d| d.map(crate::ui::widgets::drag_window))
                    .child(title),
            )
            .when(mode == Mode::Graph, |d| {
                d.child(button("nav-center", "target.svg", false, t).tooltip(tip(tr("Show the whole graph", "Voir tout le graphe"), String::new(), t)).on_click(cx.listener(
                    |this, _, _, cx| this.graph.update(cx, |graph, cx| graph.recenter(cx)),
                )))
            })
            .when(mode == Mode::Tree, |d| {
                // A note at the vault root, whatever the selection.
                d.child(button("nav-file", "file-plus.svg", false, t).debug_selector(|| "nav-file".into()).tooltip(tip(tr("New note at the root", "Nouvelle note à la racine"), String::new(), t)).on_click(cx.listener(
                    |this, _, window, cx| this.new_note_in(None, window, cx),
                )))
                .child(button("nav-folder", "folder-plus.svg", false, t).debug_selector(|| "nav-folder".into()).tooltip(tip(tr("New folder", "Nouveau dossier"), keys(&NewFolder), t)).on_click(cx.listener(
                    |this, _, window, cx| this.menu_do(Do::NewFolder, this.nav.sel.clone(), window, cx),
                )))
                // Collapse all; if all already is, expand all.
                .child(button("nav-fold", if folded { "unfold.svg" } else { "fold.svg" }, false, t).debug_selector(|| "nav-fold".into()).tooltip(tip(if folded { tr("Unfold every folder", "Déplier tous les dossiers") } else { tr("Fold every folder", "Replier tous les dossiers") }, String::new(), t)).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.fold_all();
                        cx.notify();
                    }),
                ))
            })
            .child(
                button("nav-full", if full { "shrink.svg" } else { "expand.svg" }, false, t)
                    .debug_selector(|| "nav-full".into())
                    .tooltip(tip(if full { tr("Back beside the note", "Revenir à côté de la note") } else { tr("Panel on the whole window", "Panneau en pleine fenêtre") }, keys(&ToggleFull), t))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_full(window, cx))),
            );
        // A vault that cannot be read is not an empty one: whatever the mode, the panel says so, why and
        // where, and offers another vault rather than a note that could not be written.
        let unreadable = self.vault.as_ref().and_then(|root| self.unreadable.iter().find(|(dir, _)| dir == root).map(|(_, why)| (root.clone(), *why)));
        let nothing = self.nav.rows.is_empty().then(|| {
            if let Some((root, why)) = &unreadable {
                return div().absolute().inset_0().flex().items_center().justify_center().px_4().text_center().text_size(t.px(13.)).text_color(t.soft).child(
                    div()
                        .debug_selector(|| "nav-unreadable".into())
                        .max_w_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(format!(
                            "{} {}\n« {} »",
                            tr("This vault cannot be read:", "Ce coffre ne peut pas être lu :"),
                            crate::lang::io_reason(&std::io::Error::from(*why)),
                            vault::shown(root)
                        ))
                        .child(
                            div()
                                .id("nav-other-vault")
                                .debug_selector(|| "nav-other-vault".into())
                                .px_2()
                                .py_1()
                                .rounded(t.px(6.))
                                .cursor_pointer()
                                .text_color(t.accent)
                                .hover(|s| s.bg(t.border))
                                .child(tr("Open another vault…", "Ouvrir un autre coffre…"))
                                .on_click(cx.listener(|this, _, window, cx| this.choose_vault(window, cx))),
                        ),
                );
            }
            let said = match mode {
                Mode::Tree => match keys(&crate::actions::NewNote) {
                    key if key.is_empty() => tr("Nothing here yet", "Rien ici pour l'instant").to_string(),
                    key => format!("{}\n{} {key}", tr("Nothing here yet", "Rien ici pour l'instant"), tr("New note:", "Nouvelle note :")),
                },
                Mode::Tags => tr("No tag yet: write #tag in a note", "Aucun tag pour l'instant : écrivez #tag dans une note").to_string(),
                Mode::Links if self.nav.anchor.is_none() => tr("Open a note to see what links to it", "Ouvrez une note pour voir ce qui renvoie vers elle").to_string(),
                Mode::Links => tr("No note links here yet", "Aucune note ne renvoie ici pour l'instant").to_string(),
                _ => tr("No recent note", "Aucune note récente").to_string(),
            };
            div().absolute().inset_0().flex().items_center().justify_center().px_4().text_center().text_size(t.px(13.)).text_color(t.soft).child(div().debug_selector(|| "nav-empty".into()).max_w_full().child(said))
        });
        // Set again by the selected row if it is drawn.
        self.nav.sel_at.set(None);
        let list = uniform_list("nav-rows", self.nav.rows.len(), cx.processor(Self::render_rows))
            .track_scroll(self.nav.scroll.clone())
            .flex_1()
            .min_h_0()
            // Outside any row: a drop goes to the root, right-click targets the vault.
            .when(mode == Mode::Tree, |d| {
                // System files dropped outside any row: at the vault root.
                d.on_drag_move(cx.listener(|this, e: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    let root = matches!(&this.drop_aim, Some(super::DropAim { aim: super::Aim::Root, .. }));
                    if !e.bounds.contains(&e.event.position) {
                        if root {
                            this.aim_drop(None, cx);
                        }
                    } else if this.drop_aim.is_none() || root {
                        let aim = super::DropAim { aim: super::Aim::Root, items: e.drag(cx).paths().len(), pointer: e.event.position };
                        this.aim_drop(Some(aim), cx);
                    }
                }))
                .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                    this.drop_paths(paths.paths().to_vec(), super::Aim::Root, cx);
                }))
                .on_drop(cx.listener(|this, dragged: &Dragged, _, cx| {
                    if let Some(root) = this.vault.clone() {
                        this.move_all(&dragged.paths, &root, cx)
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, e: &MouseDownEvent, window, cx| {
                        this.show_menu(Menu { at: e.position, ..Default::default() }, window, cx);
                    }),
                )
            });
        let width = self.split_width();
        let content = div()
            .debug_selector(|| "nav-panel".into())
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .map(|d| if full { d.flex_1() } else { d.flex_none().w(width) })
            .key_context("Nav")
            .track_focus(&self.nav.focus)
            .on_action(cx.listener(|this, _: &Prev, window, cx| this.nav_step(false, window, cx)))
            .on_action(cx.listener(|this, _: &Next, window, cx| this.nav_step(true, window, cx)))
            .on_action(cx.listener(|this, _: &Fold, window, cx| this.nav_fold(false, window, cx)))
            .on_action(cx.listener(|this, _: &Unfold, window, cx| this.nav_fold(true, window, cx)))
            .on_action(cx.listener(|this, _: &PrevMonth, _, cx| this.day_step(0, -1, cx)))
            .on_action(cx.listener(|this, _: &NextMonth, _, cx| this.day_step(0, 1, cx)))
            .on_action(cx.listener(|this, _: &Open, window, cx| {
                if this.nav.mode == Mode::Calendar {
                    return this.open_day(this.nav.day, window, cx);
                }
                if let Some(ix) = this.selected_row() {
                    this.nav_activate(ix, window, cx)
                }
            }))
            .when(!matches!(mode, Mode::Graph | Mode::Calendar), |d| {
                d.on_action(cx.listener(|this, _: &Rename, window, cx| {
                    this.menu_do(Do::Rename, this.nav.sel.clone(), window, cx)
                }))
                .on_action(cx.listener(|this, _: &Duplicate, window, cx| {
                    this.menu_do(Do::Duplicate, this.nav.sel.clone(), window, cx)
                }))
                .on_action(cx.listener(|this, _: &Trash, window, cx| {
                    this.menu_do(Do::Trash, this.nav.sel.clone(), window, cx)
                }))
            })
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if this.menu.is_some() {
                    return this.close_menu(window, cx);
                }
                if this.icon_pick.is_some() {
                    return this.close_icons(window, cx);
                }
                if this.nav.panel == Panel::Full {
                    this.nav.panel = Panel::Split;
                }
                window.focus(&this.pane().editor.focus_handle(cx));
                this.settle_nav(window, cx);
            }))
            .child(header)
            .map(|d| match mode {
                Mode::Graph => d.child(div().flex_1().min_h_0().child(self.graph.clone())),
                Mode::Calendar => d.child(self.render_calendar(cx)),
                Mode::Search => d.child(self.render_vault_find(window, cx)),
                _ => d.child(div().flex_1().min_h_0().relative().flex().flex_col().child(list).children(nothing)),
            });
        // 5 px handle around a 1 px stroke; double-click: original width.
        let divider = div()
            .id("nav-divider")
            .flex_none()
            .w(DIVIDER)
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .group("nav-divider")
            .child(
                div()
                    .w(t.px(1.))
                    .h_full()
                    .bg(if self.nav.dragging { t.accent } else { t.border })
                    .group_hover("nav-divider", |s| s.bg(t.accent)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    if e.click_count >= 2 {
                        *this.nav.width() = if this.nav.mode == Mode::Graph { px(0.) } else { LIST_W };
                        this.nav.panel = Panel::Split;
                        this.settle_nav(window, cx);
                    } else {
                        this.nav.dragging = true;
                    }
                    cx.stop_propagation();
                }),
            );
        wrapper
            .map(|d| if full { d.flex_1() } else { d.flex_none() })
            .child(rail)
            .child(content)
            .child(divider)
    }
}
