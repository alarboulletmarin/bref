//! Palette (Ctrl+P) : recherche de notes, filtre par `#tag`, création, coffre.
//! Sert aussi de simple champ de saisie (nom d'un dossier, nouveau nom).

use std::{ops::Range, path::PathBuf};

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, Bounds, Context, ElementInputHandler, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, MouseButton, Pixels, Point, UTF16Selection, Window,
    actions, canvas, div, ease_out_quint, prelude::*, px,
};

use crate::{Theme, tr};

fn vault_label() -> &'static str {
    tr("Change vault…", "Changer de coffre…")
}

fn diagram_label() -> &'static str {
    tr("New diagram", "Nouveau schéma")
}

fn import_label() -> &'static str {
    tr("Import a diagram (Excalidraw, draw.io)…", "Importer un schéma (Excalidraw, draw.io)…")
}

fn help_label() -> &'static str {
    tr("Keyboard shortcuts, about", "Raccourcis clavier, à propos")
}

/// Réglage d'apparence proposé par la palette.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Setting {
    Theme,
    Font,
    Mono,
}

impl Setting {
    pub fn label(self) -> &'static str {
        match self {
            Setting::Theme => tr("Theme…", "Thème…"),
            Setting::Font => tr("Font…", "Police…"),
            Setting::Mono => tr("Code font…", "Police du code…"),
        }
    }
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
    NewDiagram,
    ImportDiagram,
    Setting(Setting),
    /// Texte validé dans un champ de saisie, ou choix validé dans une liste.
    Submit(String),
    /// Choix survolé dans une liste : à appliquer en aperçu.
    Preview(String),
    Dismiss,
}

#[derive(Clone, Copy)]
enum Item {
    Note(usize),
    Create,
    Vault,
    Help,
    Diagram,
    Import,
    Setting(Setting),
}

pub struct Palette {
    focus: FocusHandle,
    query: String,
    entries: Vec<Entry>,
    items: Vec<Item>,
    selected: usize,
    /// Champ de saisie : son libellé. Aucune note n'est alors proposée.
    prompt: Option<&'static str>,
    /// Liste de choix : `entries` sont les options, sans création ni commande.
    choices: bool,
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
            prompt: None,
            choices: false,
            theme,
        };
        this.refresh();
        this
    }

    /// Champ de saisie prérempli avec `text` ; Entrée émet `Submit`.
    pub fn prompt(label: &'static str, text: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self {
            prompt: Some(label),
            ..Self::new(Vec::new(), text, theme, cx)
        }
    }

    /// Liste de choix filtrable (thème, police), `current` présélectionné :
    /// chaque déplacement émet `Preview`, Entrée émet `Submit`.
    pub fn choose(
        label: &'static str,
        options: Vec<String>,
        current: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let entries =
            options.into_iter().map(|name| Entry { name, path: PathBuf::new(), tags: Vec::new() }).collect();
        let mut this = Self { prompt: Some(label), choices: true, ..Self::new(entries, "", theme, cx) };
        this.refresh();
        this.selected = this.items.iter().position(|i| this.choice(i) == Some(current)).unwrap_or(0);
        this
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    fn choice(&self, item: &Item) -> Option<&str> {
        match item {
            Item::Note(i) if self.choices => Some(&self.entries[*i].name),
            _ => None,
        }
    }

    /// Dans une liste de choix, le choix sélectionné s'applique en aperçu.
    fn preview(&self, cx: &mut Context<Self>) {
        if let Some(name) = self.items.get(self.selected).and_then(|i| self.choice(i)) {
            cx.emit(PaletteEvent::Preview(name.to_string()));
        }
    }

    fn refresh(&mut self) {
        if self.prompt.is_some() && !self.choices {
            return self.items.clear();
        }
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
        if self.choices {
            items.truncate(14);
            self.items = items;
            self.selected = 0;
            return;
        }
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
        // Comme les réglages : proposés seulement quand on les cherche.
        for (label, item) in [(diagram_label(), Item::Diagram), (import_label(), Item::Import)] {
            if !q.is_empty() && fuzzy(&q, &label.to_lowercase()).is_some() {
                items.push(item);
            }
        }
        // Les réglages n'encombrent pas la liste tant qu'on ne les cherche pas.
        for setting in [Setting::Theme, Setting::Font, Setting::Mono] {
            if !q.is_empty() && fuzzy(&q, &setting.label().to_lowercase()).is_some() {
                items.push(Item::Setting(setting));
            }
        }
        self.items = items;
        self.selected = 0;
    }

    fn step(&mut self, by: usize, cx: &mut Context<Self>) {
        if !self.items.is_empty() {
            self.selected = (self.selected + by) % self.items.len();
            self.preview(cx);
            cx.notify();
        }
    }

    fn confirm(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.choices {
            return cx.emit(match self.items.get(index).and_then(|i| self.choice(i)) {
                Some(name) => PaletteEvent::Submit(name.to_string()),
                None => PaletteEvent::Dismiss,
            });
        }
        if self.prompt.is_some() {
            return cx.emit(PaletteEvent::Submit(self.query.trim().to_string()));
        }
        cx.emit(match self.items.get(index) {
            Some(Item::Note(i)) => PaletteEvent::Open(self.entries[*i].path.clone()),
            Some(Item::Create) => PaletteEvent::Create(self.query.trim().to_string()),
            Some(Item::Vault) => PaletteEvent::ChangeVault,
            Some(Item::Help) => PaletteEvent::Help,
            Some(Item::Diagram) => PaletteEvent::NewDiagram,
            Some(Item::Import) => PaletteEvent::ImportDiagram,
            Some(Item::Setting(setting)) => PaletteEvent::Setting(*setting),
            None => PaletteEvent::Dismiss,
        });
    }

    fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        self.query.push_str(text);
        self.refresh();
        self.preview(cx);
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
                Item::Diagram => (diagram_label().to_string(), format!("{}+Shift+D", crate::MOD)),
                Item::Import => (import_label().to_string(), String::new()),
                Item::Setting(setting) => (setting.label().to_string(), String::new()),
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
                .when(i != self.selected, |d| d.hover(|s| s.bg(t.code_bg)))
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
                this.preview(cx);
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
                                d.child(div().text_color(t.dim).child(self.prompt.unwrap_or(tr(
                                    "Search or create a note, #tag…",
                                    "Chercher ou créer une note, #tag…",
                                ))))
                            })
                            .when(!self.query.is_empty(), |d| d.child(self.query.clone()))
                            .child(div().w(px(2.)).h(px(18.)).bg(t.accent))
                            .when(!self.query.is_empty(), |d| {
                                d.children(self.prompt.map(|label| {
                                    div().ml_auto().pl_3().text_size(px(12.)).text_color(t.dim).child(label)
                                }))
                            }),
                    )
                    .children(rows)
                    // Le panneau glisse en place à l'ouverture.
                    .with_animation(
                        "palette-in",
                        Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                        |panel, delta| panel.opacity(delta).mt(px(64. + 8. * delta)),
                    ),
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
