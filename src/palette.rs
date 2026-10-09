//! Palette (Ctrl+P) : recherche de notes, filtre par `#tag`, création, coffre.
//! Sert aussi de simple champ de saisie (nom d'un dossier, nouveau nom).

use std::{ops::Range, path::PathBuf, sync::Arc};

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, Bounds, Context, ElementInputHandler, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, MouseButton, Pixels, Point, UTF16Selection, Window,
    actions, canvas, div, ease_out_quint, prelude::*, px,
};

use crate::{Theme, sheet::Pick, tr};

fn vault_label() -> &'static str {
    tr("Change vault…", "Changer de coffre…")
}

fn diagram_label() -> &'static str {
    tr("New diagram", "Nouveau schéma")
}

fn folder_label() -> &'static str {
    tr("New folder", "Nouveau dossier")
}

fn import_label() -> &'static str {
    tr("Import a diagram (Excalidraw, draw.io)…", "Importer un schéma (Excalidraw, draw.io)…")
}

fn updates_label(on: bool) -> &'static str {
    if on {
        tr("Stop checking for updates", "Ne plus chercher les mises à jour")
    } else {
        tr("Check for updates every day", "Chercher les mises à jour chaque jour")
    }
}

fn check_label() -> &'static str {
    tr("Check for updates now", "Chercher une mise à jour maintenant")
}

fn install_label(version: &str) -> String {
    format!("{} {version}", tr("Update Bref to", "Mettre à jour Bref vers"))
}

fn backup_label() -> &'static str {
    tr("Back up the vault…", "Sauvegarder le coffre…")
}

fn trash_label() -> &'static str {
    tr("Restore from the trash…", "Restaurer depuis la corbeille…")
}

fn today_label() -> &'static str {
    tr("Today's note", "Note du jour")
}

fn outline_label() -> &'static str {
    tr("Outline of the note", "Plan de la note")
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

#[derive(Default)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub tags: Vec<String>,
    /// Texte de la note, et le même en minuscules, pour la recherche plein texte.
    pub body: Arc<str>,
    pub lower: String,
}

pub enum PaletteEvent {
    Open(PathBuf),
    Create(String),
    ChangeVault,
    Help,
    NewDiagram,
    NewFolder,
    ImportDiagram,
    Setting(Setting),
    /// Réglage ou geste du tableau affiché.
    Table(Pick),
    /// Active ou coupe la recherche de nouvelle version.
    ToggleUpdates,
    /// Cherche tout de suite une nouvelle version.
    CheckUpdate,
    /// Liste des titres de la note.
    Outline,
    /// Ligne choisie dans la recherche du coffre : la note, le rang de la ligne, le texte cherché.
    OpenAt(PathBuf, usize, String),
    /// Ouvre ou crée la note du jour.
    Today,
    /// Liste la corbeille du coffre, pour en restaurer un élément.
    Trash,
    /// Archive le coffre dans un dossier à choisir.
    Backup,
    /// Installe la nouvelle version.
    InstallUpdate,
    /// Texte validé dans un champ de saisie, ou choix validé dans une liste.
    Submit(String),
    /// Choix survolé dans une liste : à appliquer en aperçu.
    Preview(String),
    Dismiss,
}

#[derive(Clone, Copy)]
enum Item {
    Note(usize),
    /// Note dont le texte, et non le nom, répond à la recherche.
    Text(usize),
    /// Recherche du coffre : une ligne d'une note (son rang) où figure le texte cherché.
    Line(usize, usize),
    Create,
    Vault,
    Help,
    Diagram,
    Folder,
    Import,
    Updates,
    Check,
    Install,
    Outline,
    Today,
    Trash,
    Backup,
    Setting(Setting),
    Table(Pick),
}

pub struct Palette {
    focus: FocusHandle,
    query: String,
    entries: Vec<Entry>,
    items: Vec<Item>,
    selected: usize,
    /// Champ de saisie : son libellé. Aucune note n'est alors proposée.
    prompt: Option<String>,
    /// Liste de choix : `entries` sont les options, sans création ni commande.
    choices: bool,
    /// Recherche dans le texte du coffre : une ligne trouvée par choix, ni note par son nom ni commande.
    lines: bool,
    /// La recherche de nouvelle version est active : `None` hors de la palette principale.
    updates: Option<bool>,
    /// Numéro de la nouvelle version que l'app sait installer seule.
    installable: Option<String>,
    /// Un tableau est affiché : ses réglages se cherchent ici.
    table: bool,
    theme: Theme,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Tous les mots figurent dans le texte (déjà en minuscules).
fn contains_all(lower: &str, words: &[&str]) -> bool {
    words.iter().all(|w| lower.contains(w))
}

/// La ligne de la note où figure `word` (en minuscules), avec sa casse d'origine,
/// recentrée sur le mot et raccourcie.
fn snippet(body: &str, lower: &str, word: &str) -> String {
    let Some(at) = lower.find(word) else {
        return String::new();
    };
    snippet_at(body, lower, lower[..at].matches('\n').count(), word)
}

/// La même, pour la ligne de rang `row`.
fn snippet_at(body: &str, lower: &str, row: usize, word: &str) -> String {
    let (Some(shown), Some(low)) = (body.lines().nth(row), lower.lines().nth(row)) else {
        return String::new();
    };
    let col = low.find(word).map_or(0, |i| low[..i].chars().count());
    let chars: Vec<char> = shown.chars().collect();
    let start = col.saturating_sub(24);
    let part: String = chars.iter().skip(start).take(80).collect();
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        part.trim(),
        if start + 80 < chars.len() { "…" } else { "" }
    )
}

/// Rang de chaque ligne du texte (en minuscules) où figure `query`, une fois par ligne.
fn rows_with(lower: &str, query: &str) -> Vec<usize> {
    let (mut rows, mut row, mut seen) = (Vec::new(), 0, 0);
    for (at, _) in lower.match_indices(query) {
        row += lower[seen..at].matches('\n').count();
        seen = at;
        if rows.last() != Some(&row) {
            rows.push(row);
        }
    }
    rows
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
            lines: false,
            updates: None,
            installable: None,
            table: false,
            theme,
        };
        this.refresh();
        this
    }

    /// Propose d'activer ou de couper la recherche de nouvelle version, selon son état.
    pub fn with_updates(mut self, on: bool, installable: Option<String>) -> Self {
        self.updates = Some(on);
        self.installable = installable;
        self.refresh();
        self
    }

    pub fn with_table(mut self, shown: bool) -> Self {
        self.table = shown;
        self.refresh();
        self
    }

    /// Champ de saisie prérempli avec `text` ; Entrée émet `Submit`.
    pub fn prompt(label: impl Into<String>, text: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self {
            prompt: Some(label.into()),
            ..Self::new(Vec::new(), text, theme, cx)
        }
    }

    /// Liste de choix filtrable (thème, police), `current` présélectionné :
    /// chaque déplacement émet `Preview`, Entrée émet `Submit`.
    pub fn choose(
        label: &str,
        options: Vec<String>,
        current: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let entries =
            options.into_iter().map(|name| Entry { name, ..Entry::default() }).collect();
        let mut this = Self { prompt: Some(label.into()), choices: true, ..Self::new(entries, "", theme, cx) };
        this.refresh();
        this.selected = this.items.iter().position(|i| this.choice(i) == Some(current)).unwrap_or(0);
        this
    }

    /// Recherche dans le texte du coffre (comme `Ctrl+Maj+F` de Zed) : chaque ligne où figure
    /// le texte tapé, sans tenir compte de la casse ; Entrée ouvre la note à cette ligne.
    pub fn search(entries: Vec<Entry>, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self { lines: true, ..Self::new(entries, "", theme, cx) }
    }

    /// Présélectionne le choix de rang `index` (deux choix peuvent porter le même nom).
    pub fn select(mut self, index: usize) -> Self {
        self.selected = index.min(self.items.len().saturating_sub(1));
        self
    }

    /// Rang, dans la liste donnée à `choose`, du choix sélectionné.
    pub fn chosen(&self) -> Option<usize> {
        match self.items.get(self.selected) {
            Some(Item::Note(i)) if self.choices => Some(*i),
            _ => None,
        }
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
        if self.lines {
            // ponytail: les 200 premières lignes trouvées, notes récentes d'abord, cherchées à
            // chaque frappe dans tout le texte du coffre ; une recherche en tâche de fond et une
            // liste complète si les coffres grossissent. Une lettre seule ramènerait tout.
            let found = (0..self.entries.len()).filter(|_| q.chars().count() >= 2);
            let found = found.flat_map(|i| rows_with(&self.entries[i].lower, &q).into_iter().map(move |row| Item::Line(i, row)));
            self.items = found.take(200).collect();
            self.selected = 0;
            return;
        }
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
            let mut named = vec![false; self.entries.len()];
            for &(_, i) in &scored {
                named[i] = true;
            }
            let mut items: Vec<Item> = scored.into_iter().map(|(_, i)| Item::Note(i)).collect();
            // Après les noms : les notes dont le texte contient tous les mots, les plus récentes d'abord.
            // Une lettre seule les ramènerait presque toutes.
            if !self.choices && q.chars().count() >= 2 {
                let words: Vec<&str> = q.split_whitespace().collect();
                items.extend(
                    (0..self.entries.len())
                        .filter(|&i| !named[i] && contains_all(&self.entries[i].lower, &words))
                        .take(8)
                        .map(Item::Text),
                );
            }
            items
        };
        if self.choices {
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
        for (label, item) in [(diagram_label(), Item::Diagram), (folder_label(), Item::Folder), (import_label(), Item::Import)] {
            if !q.is_empty() && fuzzy(&q, &label.to_lowercase()).is_some() {
                items.push(item);
            }
        }
        for (label, item) in [(outline_label(), Item::Outline), (today_label(), Item::Today), (trash_label(), Item::Trash), (backup_label(), Item::Backup)] {
            if !q.is_empty() && fuzzy(&q, &label.to_lowercase()).is_some() {
                items.push(item);
            }
        }
        if let Some(version) = &self.installable
            && !q.is_empty()
            && fuzzy(&q, &install_label(version).to_lowercase()).is_some()
        {
            items.push(Item::Install);
        }
        if let Some(on) = self.updates
            && !q.is_empty()
            && fuzzy(&q, &updates_label(on).to_lowercase()).is_some()
        {
            items.push(Item::Updates);
        }
        if self.updates.is_some() && !q.is_empty() && fuzzy(&q, &check_label().to_lowercase()).is_some() {
            items.push(Item::Check);
        }
        if self.table && !q.is_empty() {
            items.extend(
                Pick::ALL.into_iter().filter(|p| fuzzy(&q, &p.label().to_lowercase()).is_some()).map(Item::Table),
            );
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
            Some(Item::Note(i) | Item::Text(i)) => PaletteEvent::Open(self.entries[*i].path.clone()),
            Some(&Item::Line(i, row)) => PaletteEvent::OpenAt(self.entries[i].path.clone(), row, self.query.trim().to_string()),
            Some(Item::Create) => PaletteEvent::Create(self.query.trim().to_string()),
            Some(Item::Vault) => PaletteEvent::ChangeVault,
            Some(Item::Help) => PaletteEvent::Help,
            Some(Item::Diagram) => PaletteEvent::NewDiagram,
            Some(Item::Folder) => PaletteEvent::NewFolder,
            Some(&Item::Table(pick)) => PaletteEvent::Table(pick),
            Some(Item::Import) => PaletteEvent::ImportDiagram,
            Some(Item::Updates) => PaletteEvent::ToggleUpdates,
            Some(Item::Check) => PaletteEvent::CheckUpdate,
            Some(Item::Outline) => PaletteEvent::Outline,
            Some(Item::Today) => PaletteEvent::Today,
            Some(Item::Trash) => PaletteEvent::Trash,
            Some(Item::Backup) => PaletteEvent::Backup,
            Some(Item::Install) => PaletteEvent::InstallUpdate,
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

        // Une liste de choix montre 14 lignes à la fois, autour du choix sélectionné.
        let first = self.selected.saturating_sub(6).min(self.items.len().saturating_sub(14));
        let rows = self.items.iter().enumerate().skip(first).take(14).map(|(i, item)| {
            let (label, detail) = match item {
                Item::Note(n) => {
                    let e = &self.entries[*n];
                    let tags: Vec<String> = e.tags.iter().take(4).map(|t| format!("#{t}")).collect();
                    (e.name.clone(), tags.join(" "))
                }
                Item::Text(n) => {
                    let e = &self.entries[*n];
                    let word = self.query.trim().to_lowercase();
                    let word = word.split_whitespace().next().unwrap_or_default().to_string();
                    (e.name.clone(), snippet(&e.body, &e.lower, &word))
                }
                Item::Line(n, row) => {
                    let e = &self.entries[*n];
                    (e.name.clone(), snippet_at(&e.body, &e.lower, *row, &self.query.trim().to_lowercase()))
                }
                Item::Create => (
                    format!("{} « {} »", tr("Create", "Créer"), self.query.trim()),
                    tr("new note", "nouvelle note").into(),
                ),
                Item::Vault => (vault_label().to_string(), String::new()),
                Item::Help => (help_label().to_string(), "F1".into()),
                Item::Diagram => (diagram_label().to_string(), format!("{}+Shift+D", crate::MOD)),
                Item::Folder => (folder_label().to_string(), format!("{}+Shift+N", crate::MOD)),
                Item::Import => (import_label().to_string(), String::new()),
                Item::Updates => (updates_label(self.updates.unwrap_or(true)).to_string(), String::new()),
                Item::Check => (check_label().to_string(), String::new()),
                Item::Outline => (outline_label().to_string(), format!("{}+Shift+O", crate::MOD)),
                Item::Today => (today_label().to_string(), format!("{}+J", crate::MOD)),
                Item::Trash => (trash_label().to_string(), String::new()),
                Item::Backup => (backup_label().to_string(), String::new()),
                Item::Install => (install_label(self.installable.as_deref().unwrap_or_default()), String::new()),
                Item::Setting(setting) => (setting.label().to_string(), String::new()),
                Item::Table(pick) => (pick.label().to_string(), String::new()),
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
                // Avec un extrait de texte, le nom garde sa place et l'extrait se raccourcit.
                .child(
                    div()
                        .truncate()
                        .when(matches!(item, Item::Text(_) | Item::Line(..)), |d| d.flex_none().max_w(px(220.)))
                        .child(label),
                )
                .child(
                    div()
                        .truncate()
                        .when(!matches!(item, Item::Text(_) | Item::Line(..)), |d| d.flex_none())
                        .text_color(t.dim)
                        .text_size(px(12.))
                        .child(detail),
                )
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
                                d.child(div().text_color(t.dim).child(self.prompt.clone().unwrap_or_else(|| match self.lines {
                                    true => tr("Search in the text of the vault…", "Chercher dans le texte du coffre…").into(),
                                    false => tr("Search or create a note, #tag…", "Chercher ou créer une note, #tag…").into(),
                                })))
                            })
                            .when(!self.query.is_empty(), |d| d.child(self.query.clone()))
                            .child(div().w(px(2.)).h(px(18.)).bg(t.accent))
                            .when(!self.query.is_empty(), |d| {
                                d.children(self.prompt.clone().map(|label| {
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
    use super::{contains_all, fuzzy, rows_with, snippet, snippet_at};

    #[test]
    fn finds_text_with_all_the_words() {
        let lower = "# courses\n\n- lait et pâtes au citron\n";
        assert!(contains_all(lower, &["pâtes", "citron"]));
        assert!(!contains_all(lower, &["pâtes", "beurre"]));
    }

    #[test]
    fn lists_each_line_once() {
        let body = "Lait, lait\n\nrien\nau LAIT cru\n";
        let lower = body.to_lowercase();
        assert_eq!(rows_with(&lower, "lait"), [0, 3]);
        assert!(rows_with(&lower, "absent").is_empty());
        assert_eq!(snippet_at(body, &lower, 3, "lait"), "au LAIT cru");
    }

    #[test]
    fn snippet_keeps_the_case_and_trims() {
        let body = "# Courses\n\n- Lait et Pâtes au Citron\n";
        assert_eq!(snippet(body, &body.to_lowercase(), "pâtes"), "- Lait et Pâtes au Citron");
        let long = format!("{}Citron{}", "a".repeat(100), "b".repeat(100));
        let cut = snippet(&long, &long.to_lowercase(), "citron");
        assert!(cut.starts_with('…') && cut.ends_with('…') && cut.contains("Citron"));
    }

    #[test]
    fn ranks_matches() {
        assert!(fuzzy("cou", "courses") > fuzzy("cou", "les courses"));
        assert!(fuzzy("crs", "courses").is_some());
        assert!(fuzzy("crs", "courses") < fuzzy("ours", "courses"));
        assert_eq!(fuzzy("xyz", "courses"), None);
        assert!(fuzzy("", "courses").is_some());
    }
}
