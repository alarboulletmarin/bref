//! List pages: a folder or a tag shown as a table of its notes, one column per front-matter key.

use std::{
    fs,
    path::PathBuf,
};
use gpui::{
    App, Context, Focusable, MouseButton, Window, div, prelude::*, px,
};
use crate::{
    views::kanban::{Field, FieldEvent},
    ui::line::Line,
    logic::markdown::{self},
    lang::tr,
    platform::vault,
    logic::note::Note,
};
use super::*;
use crate::views::nav::*;

/// A note of a list page: its path and its cells.
type ListedNote = (PathBuf, Vec<String>);

impl Shell {
    /// Opens the list page of a folder, or of a tag (`#tag`); typing goes to its filter.
    pub(super) fn open_listing(&mut self, of: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let theme = self.theme;
        let filter = cx.new(|cx| Field::new("", theme, cx));
        cx.observe(&filter, |_, _, cx| cx.notify()).detach();
        // Escape clears the filter.
        cx.subscribe(&filter, |_, filter, event: &FieldEvent, cx| {
            if let FieldEvent::Cancel = event {
                filter.update(cx, |filter, cx| {
                    filter.line = Line::default();
                    cx.notify();
                });
            }
        })
        .detach();
        window.focus(&filter.focus_handle(cx));
        self.pane_mut().peek = false;
        self.pane_mut().listing = Some(Listing { of, by: 0, back: false, filter, edit: None });
        cx.notify();
    }

    /// List page: the columns ("Note", then the front-matter keys) and, sorted, the notes of the
    /// folder or tag that the filter keeps, with their cells.
    pub(super) fn listed(&self, cx: &App) -> Option<(Vec<String>, Vec<ListedNote>)> {
        let Listing { of, by, back, filter, .. } = self.pane().listing.as_ref()?;
        let tag = tag_of(of);
        let inside: Vec<&Note> = self.notes.iter().filter(|note| tag.map_or_else(|| note.path.starts_with(of), |tag| note.tags.iter().any(|t| t == tag))).collect();
        let bodies: Vec<&str> = inside.iter().map(|note| &*note.body).collect();
        let (mut keys, cells) = markdown::list_table(&bodies);
        keys.insert(0, "Note".to_string());
        let named = |(note, mut cells): (&&Note, Vec<String>)| {
            cells.insert(0, note.name.clone());
            (note.path.clone(), cells)
        };
        let filter = &filter.read(cx).line.text;
        let mut rows: Vec<(PathBuf, Vec<String>)> = inside.iter().zip(cells).map(named).filter(|(_, cells)| markdown::row_kept(&keys, cells, filter)).collect();
        let by = (*by).min(keys.len() - 1);
        rows.sort_by(|a, b| markdown::cell_order(&a.1[by], &b.1[by]).then_with(|| markdown::cell_order(&a.1[0], &b.1[0])));
        if *back {
            rows.reverse();
        }
        Some((keys, rows))
    }

    /// Sorts the list page by this column; asking again reverses the order.
    pub(super) fn sort_listing(&mut self, col: usize, cx: &mut Context<Self>) {
        if let Some(Listing { by, back, .. }) = &mut self.pane_mut().listing {
            (*back, *by) = (*by == col && !*back, col);
        }
        cx.notify();
    }

    /// Opens a list page cell: the value of key `key` of this note, to rewrite.
    /// The one being written is confirmed first.
    pub(super) fn list_write(&mut self, path: PathBuf, key: String, window: &mut Window, cx: &mut Context<Self>) {
        self.list_commit(window, cx);
        let Some(note) = self.notes.iter().find(|note| note.path == path) else {
            return;
        };
        let (value, theme) = (markdown::front_values(&note.body, &key).join(", "), self.theme);
        let field = cx.new(|cx| Field::new(&value, theme, cx));
        cx.subscribe_in(&field, window, |this, _, event: &FieldEvent, window, cx| match event {
            FieldEvent::Done => this.list_commit(window, cx),
            FieldEvent::Cancel => this.list_close(window, cx),
        })
        .detach();
        window.focus(&field.focus_handle(cx));
        if let Some(listing) = &mut self.pane_mut().listing {
            listing.edit = Some((path, key, field));
        }
        cx.notify();
    }

    /// Closes the open cell without writing anything; typing goes back to the filter.
    pub(super) fn list_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(listing) = &mut self.pane_mut().listing
            && listing.edit.take().is_some()
        {
            window.focus(&listing.filter.focus_handle(cx));
            cx.notify();
        }
    }

    /// Confirms the open cell: its key is rewritten in the note's front matter, and nothing else.
    pub(super) fn list_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, key, field)) = self.pane_mut().listing.as_mut().and_then(|listing| listing.edit.clone()) else {
            return;
        };
        self.list_close(window, cx);
        // The open note is saved first: it is the file being rewritten.
        self.flush(cx);
        let value = field.read(cx).line.text.clone();
        let Ok(old) = fs::read_to_string(&path) else {
            return self.say(super::Tone::Failed, format!("{} {}", tr("Cannot read", "Impossible de lire"), path.display()), cx);
        };
        let new = markdown::set_front(&old, &key, &value);
        if new == old {
            return;
        }
        if let Err(e) = vault::write(&path, &new) {
            return self.say(super::Tone::Failed, format!("{} {} : {}", tr("Not rewritten:", "Non réécrite :"), path.display(), crate::lang::io_reason(&e)), cx);
        }
        if let Some(note) = self.notes.iter_mut().find(|note| note.path == path) {
            (note.tags, note.links) = crate::logic::note::Note::index(&new);
            note.aliases = markdown::aliases(&new).into();
            note.set_body(new.as_str().into());
        }
        self.graph_stale = true;
        self.refresh_graph(cx);
        // The note shown under the list is reloaded: it just changed on disk.
        self.each_pane(|this| {
            if this.pane().path.as_ref() == Some(&path) {
                this.reload(cx);
            }
        });
        cx.notify();
    }

    /// The list page, in place of the note: typing filters the rows (`key:text` for a
    /// single column), a click on a column title sorts, a click on a note's name
    /// opens it, a click on another cell rewrites it (Enter confirms, Escape gives up).
    // ponytail: every row is drawn; go through the virtualized grid of
    // `sheet.rs` for a folder of thousands of notes. A note's name is not changed here.
    pub(super) fn render_listing(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let (keys, rows) = self.listed(cx).unwrap_or_default();
        let Some(Listing { of, by, back, filter, edit }) = self.pane().listing.as_ref() else {
            return div().into_any_element();
        };
        let (by, back, count) = (*by, *back, rows.len());
        // The columns share the window width, between two bounds; beyond that, the
        // page scrolls.
        let wide = |first: bool| div().flex_1().min_w(px(if first { 130. } else { 80. })).max_w(px(if first { 260. } else { 200. })).px_2();
        let cell = |first: bool| wide(first).truncate();
        let head = keys.iter().enumerate().map(|(i, key)| {
            let arrow = if i != by { "" } else if back { " ↓" } else { " ↑" };
            cell(i == 0)
                .id(("list-head", i))
                .cursor_pointer()
                .text_color(if i == by { t.accent } else { t.soft })
                .child(format!("{key}{arrow}"))
                .on_click(cx.listener(move |this, _, _, cx| this.sort_listing(i, cx)))
        });
        let mut lines = Vec::new();
        for (i, (path, cells)) in rows.into_iter().enumerate() {
            let mut shown = Vec::new();
            for (c, text) in cells.into_iter().enumerate() {
                let writing = edit.as_ref().filter(|(open, key, _)| *open == path && *key == keys[c] && c > 0);
                let path = path.clone();
                let one = match (c, writing) {
                    // The cell being written shows all its text: it wraps, its row grows.
                    (_, Some((_, _, field))) => wide(false).py(px(3.)).rounded(px(4.)).border_1().border_color(t.accent).child(field.clone()).into_any_element(),
                    (0, _) => cell(true)
                        .id(("list-name", i))
                        .cursor_pointer()
                        .hover(|s| s.text_color(t.accent))
                        .child(text)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_note(&path, cx);
                            window.focus(&this.pane().editor.focus_handle(cx));
                        }))
                        .into_any_element(),
                    _ => {
                        let key = keys[c].clone();
                        cell(false)
                            .id(("list-cell", i * 1000 + c))
                            .debug_selector(|| format!("list-cell-{i}-{c}"))
                            .h(ROW)
                            .flex()
                            .items_center()
                            .rounded(px(4.))
                            .cursor_text()
                            .text_color(t.soft)
                            .hover(|s| s.bg(t.bg))
                            .child(div().flex_1().min_w_0().truncate().child(text))
                            .on_click(cx.listener(move |this, _, window, cx| this.list_write(path.clone(), key.clone(), window, cx)))
                            .into_any_element()
                    }
                };
                shown.push(one);
            }
            lines.push(div().min_h(ROW).flex().items_center().rounded(px(5.)).hover(|s| s.bg(t.code_bg)).children(shown));
        }
        let asked = !filter.read(cx).line.text.is_empty();
        let hint = div().absolute().left(px(6.)).top_0().text_color(t.soft).child(tr("Filter… (key:text for one column)", "Filtrer… (clé:texte pour une colonne)"));
        let filter_box = div()
            .id("list-filter")
            .w(px(280.))
            .max_w_full()
            .px_2()
            .py_1()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border)
            .relative()
            .whitespace_nowrap()
            .overflow_hidden()
            .child(div().relative().child(filter.clone()).when(!asked, |d| d.child(hint)));
        let title = if tag_of(of).is_some() { of.display().to_string() } else { vault::stem(of) };
        div()
            .id("listing")
            .size_full()
            .pt(px(48.))
            .px_6()
            .pb_4()
            .text_size(px(13.))
            .flex()
            .flex_col()
            .overflow_scroll()
            // A click elsewhere confirms the cell being written.
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.list_commit(window, cx)))
            .child(
                div()
                    .pb_3()
                    .px_2()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(div().text_size(px(20.)).font_weight(gpui::FontWeight::BOLD).child(title))
                    .child(div().text_color(t.soft).child(count.to_string())),
            )
            .child(div().pb_3().px_2().child(filter_box))
            .child(div().h(ROW).flex().items_center().border_b_1().border_color(t.border).children(head))
            .children(lines)
            // Nothing to list: the filter keeps no row, or the folder or tag holds no note.
            .when(count == 0, |d| {
                let said = if asked { tr("No note matches the filter", "Aucune note ne correspond au filtre") } else { tr("No notes here yet", "Aucune note ici pour l'instant") };
                d.child(div().debug_selector(|| "list-empty".into()).py_6().text_center().text_color(t.soft).child(said))
            })
            .into_any_element()
    }
}
