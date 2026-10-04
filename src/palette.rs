//! Palette (Ctrl+P) : recherche de notes, filtre par `#tag`, création, coffre.

use std::{ops::Range, path::PathBuf};

use gpui::{
    App, Bounds, Context, ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, MouseButton, Pixels, Point, UTF16Selection, Window, actions, canvas, div,
    prelude::*, px,
};

use crate::{Theme, tr};

fn vault_label() -> &'static str {
    tr("Change vault…", "Changer de coffre…")
}

fn help_label() -> &'static str {
    tr("Keyboard shortcuts", "Raccourcis clavier")
}

actions!(palette, [Prev, Next, Confirm, Dismiss, DeleteChar]);


pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub tags: Vec<String>,
}

pub enum PaletteEvent {
    Open(PathBuf),
    Create(String),
    ChangeVault,
    Help,
    Dismiss,
}

#[derive(Clone, Copy)]
enum Item {
    Note(usize),
    Create,
    Vault,
    Help,
}

pub struct Palette {
    focus: FocusHandle,
    query: String,
    entries: Vec<Entry>,
    items: Vec<Item>,
    selected: usize,
    theme: Theme,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Score de correspondance : sous-chaîne d'abord, sinon sous-séquence.
pub fn fuzzy(query: &str, name: &str) -> Option<i32> {
    if let Some(i) = name.find(query) {
        return Some(1000 - i as i32);
    }
    let mut chars = name.chars();
    for c in query.chars() {
        chars.find(|&x| x == c)?;
    }
    Some(-(name.len() as i32))
}

impl Palette {
    /// `entries` : les notes, les plus récentes d'abord.
    pub fn new(entries: Vec<Entry>, query: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus: cx.focus_handle(),
            query: query.to_string(),
            entries,
            items: Vec::new(),
            selected: 0,
            theme,
        };
        this.refresh();
        this
    }

    fn refresh(&mut self) {
        let q = self.query.trim().to_lowercase();
        let mut items: Vec<Item> = if let Some(tag) = q.strip_prefix('#') {
            (0..self.entries.len())
                .filter(|&i| self.entries[i].tags.iter().any(|t| t.starts_with(tag)))
                .map(Item::Note)
                .collect()
        } else {
            let mut scored: Vec<(i32, usize)> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| Some((fuzzy(&q, &e.name.to_lowercase())?, i)))
                .collect();
            // Tri stable : à score égal, l'ordre « plus récent d'abord » est conservé.
            scored.sort_by_key(|(score, _)| -score);
            scored.into_iter().map(|(_, i)| Item::Note(i)).collect()
        };
        items.truncate(8);
        let exact = self.entries.iter().any(|e| e.name.to_lowercase() == q);
        if !q.is_empty() && !q.starts_with('#') && !exact {
            items.push(Item::Create);
        }
        for (label, item) in [(vault_label(), Item::Vault), (help_label(), Item::Help)] {
            if fuzzy(&q, &label.to_lowercase()).is_some() {
                items.push(item);
            }
        }
        self.items = items;
        self.selected = 0;
    }

    fn step(&mut self, by: usize, cx: &mut Context<Self>) {
        if !self.items.is_empty() {
            self.selected = (self.selected + by) % self.items.len();
            cx.notify();
        }
    }

    fn confirm(&mut self, index: usize, cx: &mut Context<Self>) {
        cx.emit(match self.items.get(index) {
            Some(Item::Note(i)) => PaletteEvent::Open(self.entries[*i].path.clone()),
            Some(Item::Create) => PaletteEvent::Create(self.query.trim().to_string()),
            Some(Item::Vault) => PaletteEvent::ChangeVault,
            Some(Item::Help) => PaletteEvent::Help,
            None => PaletteEvent::Dismiss,
        });
    }

    fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        self.query.push_str(text);
        self.refresh();
        cx.notify();
    }
}

// ponytail: saisie réduite à « ajouter à la fin » (pas de curseur mobile ni de
// composition IME dans la palette) ; suffisant pour une requête de recherche.
impl EntityInputHandler for Palette {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.query.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typed(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.typed(text, cx);
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let focus = self.focus.clone();
        let entity = cx.entity();
        let input = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
            },
        )
        .size_0();

        let rows = self.items.iter().enumerate().map(|(i, item)| {
            let (label, detail) = match item {
                Item::Note(n) => {
                    let e = &self.entries[*n];
                    let tags: Vec<String> = e.tags.iter().take(4).map(|t| format!("#{t}")).collect();
                    (e.name.clone(), tags.join(" "))
                }
                Item::Create => (
                    format!("{} « {} »", tr("Create", "Créer"), self.query.trim()),
                    tr("new note", "nouvelle note").into(),
                ),
                Item::Vault => (vault_label().to_string(), String::new()),
                Item::Help => (help_label().to_string(), "F1".into()),
            };
            div()
                .mx_1()
                .px_3()
                .py_1p5()
                .rounded(px(6.))
                .flex()
                .justify_between()
                .gap_3()
                .when(i == self.selected, |d| d.bg(t.selection))
                .child(div().truncate().child(label))
                .child(div().flex_none().text_color(t.dim).text_size(px(12.)).child(detail))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.confirm(i, cx)),
                )
        });

        div()
            .absolute()
            .inset_0()
            .occlude()
            .key_context("Palette")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Next, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &Prev, _, cx| this.step(this.items.len().max(1) - 1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(this.selected, cx)))
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.emit(PaletteEvent::Dismiss)))
            .on_action(cx.listener(|this, _: &DeleteChar, _, cx| {
                this.query.pop();
                this.refresh();
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Dismiss)))
            .flex()
            .justify_center()
            .items_start()
            .child(
                div()
                    .mt(px(72.))
                    .w(px(520.))
                    .max_w_full()
                    .pb_1()
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(10.))
                    .shadow_lg()
                    .text_size(px(14.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .px_4()
                            .py_3()
                            .mb_1()
                            .border_b_1()
                            .border_color(t.border)
                            .flex()
                            .items_center()
                            .text_size(px(15.))
                            .child(input)
                            .when(self.query.is_empty(), |d| {
                                d.child(div().text_color(t.dim).child(tr(
                                    "Search or create a note, #tag…",
                                    "Chercher ou créer une note, #tag…",
                                )))
                            })
                            .when(!self.query.is_empty(), |d| d.child(self.query.clone()))
                            .child(div().w(px(2.)).h(px(18.)).bg(t.accent)),
                    )
                    .children(rows),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn ranks_matches() {
        assert!(fuzzy("cou", "courses") > fuzzy("cou", "les courses"));
        assert!(fuzzy("crs", "courses").is_some());
        assert!(fuzzy("crs", "courses") < fuzzy("ours", "courses"));
        assert_eq!(fuzzy("xyz", "courses"), None);
        assert!(fuzzy("", "courses").is_some());
    }
}
