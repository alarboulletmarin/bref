//! Palette (Ctrl+P): note search, `#tag` filter, creation, vault.
//! Also serves as a plain input field (folder name, new name).

use std::{ops::Range, path::PathBuf, sync::Arc};

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, Bounds, Context, ElementInputHandler, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, MouseButton, Pixels, Point, UTF16Selection, Window,
    actions, canvas, div, ease_out_quint, prelude::*, px, svg,
};

use crate::{
    ui::theme::Theme,
    ui::line::{self, Line},
    views::editor::TableOp,
    logic::markdown::ListStyle,
    views::sheet::Pick,
    lang::tr,
};

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

pub fn sync_label() -> &'static str {
    tr("Sync the vault (git)", "Synchroniser le coffre (git)")
}

pub fn clone_label() -> &'static str {
    tr("Clone a vault from a git address…", "Cloner un coffre depuis une adresse git…")
}

fn backup_label() -> &'static str {
    tr("Back up the vault…", "Sauvegarder le coffre…")
}

fn capture_label() -> &'static str {
    tr("Set up the quick capture shortcut", "Créer le raccourci de capture rapide")
}

fn terminal_label() -> &'static str {
    tr("Open a terminal in the vault", "Ouvrir un terminal dans le coffre")
}

fn trash_label() -> &'static str {
    tr("Restore from the trash…", "Restaurer depuis la corbeille…")
}

pub fn back_label() -> &'static str {
    tr("Back: what was shown before", "Précédent : ce qui était affiché avant")
}

pub fn forward_label() -> &'static str {
    tr("Forward: what was shown after", "Suivant : ce qui était affiché après")
}

fn today_label() -> &'static str {
    tr("Today's note", "Note du jour")
}

fn board_label() -> &'static str {
    tr("New kanban board", "Nouveau tableau kanban")
}

fn undo_swap_label() -> &'static str {
    tr("Undo the replacement in the vault", "Annuler le remplacement dans le coffre")
}

fn list_label(style: ListStyle) -> &'static str {
    match style {
        ListStyle::Bullet => tr("Turn into a bulleted list", "Transformer en liste à puces"),
        ListStyle::Ordered => tr("Turn into a numbered list", "Transformer en liste numérotée"),
        ListStyle::Task => tr("Turn into a task list", "Transformer en liste de tâches"),
        ListStyle::Text => tr("Turn into plain text", "Transformer en texte simple"),
    }
}

fn comments_label() -> &'static str {
    tr("Comments of the note", "Commentaires de la note")
}

fn outline_label() -> &'static str {
    tr("Outline of the note", "Plan de la note")
}

fn help_label() -> &'static str {
    tr("Keyboard shortcuts, about", "Raccourcis clavier, à propos")
}

/// Appearance setting offered by the palette.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Setting {
    Theme,
    Font,
    Mono,
    /// Width of the text column.
    Page,
    /// Placement of tables wider than the text.
    Wide,
    /// Formatting bar at the bottom of the note.
    Bar,
}

impl Setting {
    pub fn label(self) -> &'static str {
        match self {
            Setting::Theme => tr("Theme…", "Thème…"),
            Setting::Font => tr("Font…", "Police…"),
            Setting::Mono => tr("Code font…", "Police du code…"),
            Setting::Page => tr("Page width…", "Largeur de page…"),
            Setting::Wide => tr("Wide tables…", "Grands tableaux…"),
            Setting::Bar => tr("Formatting bar…", "Barre de mise en forme…"),
        }
    }
}

actions!(palette, [Prev, Next, Confirm, ConfirmAside, Dismiss]);

/// What templates allow: a note born from one, a template inserted, the open note saved as a template.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TemplateAct {
    New,
    Insert,
    Save,
}

impl TemplateAct {
    fn label(self) -> &'static str {
        match self {
            TemplateAct::New => tr("New note from a template…", "Nouvelle note depuis un modèle…"),
            TemplateAct::Insert => tr("Insert a template…", "Insérer un modèle…"),
            TemplateAct::Save => tr("Make a template of this note…", "Faire un modèle de cette note…"),
        }
    }
}

#[derive(Default)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub tags: Arc<[String]>,
    /// Text of the note, and the same in lowercase, for full-text search.
    pub body: Arc<str>,
    pub lower: Arc<str>,
}

pub enum PaletteEvent {
    Open(PathBuf),
    /// An app command: the name of its action.
    Run(&'static str),
    Create(String),
    ChangeVault,
    Help,
    NewDiagram,
    NewFolder,
    ImportDiagram,
    Setting(Setting),
    /// Setting or gesture of the shown table.
    Table(Pick),
    /// The chosen lines of the note, as a list or as text.
    Restyle(ListStyle),
    /// A gesture on the table under the cursor.
    Grid(TableOp),
    /// A note born from a template, a template inserted, or the note saved as a template.
    Template(TemplateAct),
    /// Turns the new-version check on or off.
    ToggleUpdates,
    /// Checks for a new version right now.
    CheckUpdate,
    /// List of the note's headings.
    Outline,
    /// List of the note's comments.
    Comments,
    /// Undoes the last vault-wide replacement.
    UndoSwap,
    /// Creates a note that opens as a kanban board.
    NewBoard,
    /// Line chosen in the vault search: the note, the line's index, the searched text.
    OpenAt(PathBuf, usize, String),
    /// Opens or creates the daily note.
    Today,
    /// Back (`false`) or forward (`true`) in what has been shown.
    Travel(bool),
    /// Lists the vault's trash, to restore an item from it.
    Trash,
    /// Archives the vault into a folder to choose.
    Backup,
    Sync,
    CloneVault,
    /// Opens the system terminal in the vault folder.
    Terminal,
    /// Binds capture (`bref --capture`) to a desktop shortcut.
    CaptureKey,
    /// Installs the new version.
    InstallUpdate,
    /// Text submitted in an input field, or choice submitted in a list.
    Submit(String),
    /// Choice hovered in a list: to apply as a preview.
    Preview(String),
    Dismiss,
}

#[derive(Clone, Copy)]
enum Item {
    Note(usize),
    /// Note whose text, not name, matches the search.
    Text(usize),
    /// Vault search: a line of a note (its index) where the searched text appears.
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
    Comments,
    UndoSwap,
    Board,
    Today,
    Back,
    Forward,
    Trash,
    Backup,
    Sync,
    CloneVault,
    Terminal,
    CaptureKey,
    Setting(Setting),
    Table(Pick),
    Restyle(ListStyle),
    Grid(TableOp),
    Template(TemplateAct),
    /// An app command (`commands`), by its index.
    Command(usize),
}

pub struct Palette {
    /// Submitted with Ctrl+Enter: the choice opens in the other pane.
    pub aside: bool,
    focus: FocusHandle,
    query: Line,
    entries: Vec<Entry>,
    /// The name of each entry in lowercase, done once: the list is rebuilt on every keystroke.
    names: Vec<String>,
    items: Vec<Item>,
    selected: usize,
    /// Input field: its label. No note is offered then.
    prompt: Option<String>,
    /// Choice list: `entries` are the options, without creation or commands.
    choices: bool,
    /// Search in the vault's text: one found line per choice, no note by name nor command.
    lines: bool,
    /// The new-version check is on: `None` outside the main palette.
    updates: Option<bool>,
    /// Number of the new version the app can install by itself.
    installable: Option<String>,
    /// A table is shown: its settings are searched here.
    table: bool,
    /// A note is shown: its lines can become a list.
    lists: bool,
    /// The cursor is in a Markdown table: its rows and columns are managed from here.
    cells: bool,
    /// A vault replacement can be undone.
    undo_swap: bool,
    /// The commands the view can do, the view's own first (the first `here`): the
    /// palette finds them by what they do, and offers the `here` ones without typing anything.
    commands: Vec<crate::logic::commands::Command>,
    here: usize,
    /// Only the commands (F10, or ">" typed first), and the name of what is being looked at.
    commands_only: bool,
    context: &'static str,
    /// Margin above the frame.
    top: Pixels,
    /// Scroll of the input line, and the x of its cursor at the last frame:
    /// a text longer than the frame slides under it so the cursor stays in view.
    scroll: gpui::ScrollHandle,
    caret: std::rc::Rc<std::cell::Cell<Option<Pixels>>>,
    theme: Theme,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// All the words appear in the text (already lowercase).
fn contains_all(lower: &str, words: &[&str]) -> bool {
    words.iter().all(|w| lower.contains(w))
}

/// The note's line where `word` (lowercase) appears, with its original case,
/// recentred on the word and shortened.
fn snippet(body: &str, lower: &str, word: &str) -> String {
    let Some(at) = lower.find(word) else {
        return String::new();
    };
    snippet_at(body, lower, lower[..at].matches('\n').count(), word)
}

/// The same, for the line at index `row`.
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

/// Index of each line of the text (lowercase) where `query` appears, once per line.
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

/// The text in lowercase, without its accents: « thème » is found by typing « theme ».
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// A command matches the search when its label contains it, or when each typed word
/// starts one of its words (`new fol`: "New folder"). No subsequence, unlike
/// notes: `plan` would bring up « Annuler le remplacement ».
pub fn command(query: &str, label: &str) -> bool {
    let (query, label) = (fold(query), fold(label));
    if label.contains(query.trim()) {
        return true;
    }
    let words: Vec<&str> = label.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    query.split_whitespace().all(|typed| words.iter().any(|word| word.starts_with(typed)))
}

/// Match score: substring first, otherwise subsequence.
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
    /// `entries`: the notes, most recent first.
    pub fn new(entries: Vec<Entry>, query: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus: cx.focus_handle(),
            query: Line::new(query),
            names: entries.iter().map(|e| e.name.to_lowercase()).collect(),
            entries,
            items: Vec::new(),
            selected: 0,
            aside: false,
            prompt: None,
            choices: false,
            lines: false,
            updates: None,
            installable: None,
            table: false,
            lists: false,
            cells: false,
            undo_swap: false,
            commands: Vec::new(),
            here: 0,
            commands_only: false,
            context: "",
            top: px(72.),
            scroll: gpui::ScrollHandle::new(),
            caret: Default::default(),
            theme,
        };
        this.refresh();
        this
    }

    /// Offers to turn the new-version check on or off, depending on its state.
    pub fn with_updates(mut self, on: bool, installable: Option<String>) -> Self {
        self.updates = Some(on);
        self.installable = installable;
        self.refresh();
        self
    }

    /// Offers to undo the last vault replacement, if there is one.
    pub fn with_commands(mut self, commands: Vec<crate::logic::commands::Command>, here: usize, context: &'static str) -> Self {
        (self.commands, self.here, self.context) = (commands, here, context);
        self.refresh();
        self
    }

    pub fn with_undo_swap(mut self, possible: bool) -> Self {
        self.undo_swap = possible;
        self.refresh();
        self
    }

    pub fn with_cells(mut self, here: bool) -> Self {
        self.cells = here;
        self.refresh();
        self
    }

    pub fn with_lists(mut self, shown: bool) -> Self {
        self.lists = shown;
        self.refresh();
        self
    }

    pub fn with_table(mut self, shown: bool) -> Self {
        self.table = shown;
        self.refresh();
        self
    }

    /// Input field prefilled with `text`; Enter emits `Submit`.
    pub fn prompt(label: impl Into<String>, text: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            prompt: Some(label.into()),
            ..Self::new(Vec::new(), text, theme, cx)
        };
        // `new` listed the palette's commands: an input field offers none.
        this.refresh();
        this
    }

    /// Stuck to the top of its container: for a window that is only this field.
    pub fn at_top(mut self) -> Self {
        self.top = px(8.);
        self
    }

    /// Filterable choice list (theme, font), `current` preselected:
    /// each move emits `Preview`, Enter emits `Submit`.
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

    /// Search in the vault's text (`Ctrl+Shift+F`): each line where
    /// the typed text appears, ignoring case; Enter opens the note at that line.
    pub fn search(entries: Vec<Entry>, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self { lines: true, ..Self::new(entries, "", theme, cx) }
    }

    /// Preselects the choice at index `index` (two choices can bear the same name).
    pub fn select(mut self, index: usize) -> Self {
        self.selected = index.min(self.items.len().saturating_sub(1));
        self
    }

    /// Index, in the list given to `choose`, of the selected choice.
    /// The input's cursor is inside its frame, at the last frame drawn.
    #[cfg(test)]
    pub fn caret_inside(&self) -> Option<bool> {
        let (frame, caret) = (self.scroll.bounds(), self.caret.get()?);
        Some(caret >= frame.left() && caret <= frame.right())
    }

    /// Number of lines offered under the input.
    /// Only the commands, and the name of what is being looked at.
    #[cfg(test)]
    pub fn mode(&self) -> (bool, &'static str) {
        (self.commands_only, self.context)
    }

    /// The commands offered, by the name of their action.
    #[cfg(test)]
    pub fn runs(&self) -> Vec<&'static str> {
        self.items.iter().filter_map(|item| if let &Item::Command(n) = item { Some(self.commands[n].action) } else { None }).collect()
    }

    #[cfg(test)]
    pub fn listed(&self) -> usize {
        self.items.len()
    }

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

    /// In a choice list, the selected choice is applied as a preview.
    fn preview(&self, cx: &mut Context<Self>) {
        if let Some(name) = self.items.get(self.selected).and_then(|i| self.choice(i)) {
            cx.emit(PaletteEvent::Preview(name.to_string()));
        }
    }

    fn refresh(&mut self) {
        if self.prompt.is_some() && !self.choices {
            return self.items.clear();
        }
        let q = self.query.text.trim().to_lowercase();
        if self.lines {
            // ponytail: the first 200 lines found, recent notes first, searched on
            // every keystroke in the whole vault text; a background search and a
            // full list if vaults grow. A single letter would bring up everything.
            let found = (0..self.entries.len()).filter(|_| q.chars().count() >= 2);
            let found = found.flat_map(|i| rows_with(&self.entries[i].lower, &q).into_iter().map(move |row| Item::Line(i, row)));
            self.items = found.take(200).collect();
            self.selected = 0;
            return;
        }
        // ">" first: the palette shows only the commands, and the ">"
        // is cleared from the field.
        if !self.commands_only && !self.choices && self.prompt.is_none() && self.query.text.starts_with('>') {
            self.query = Line::new(self.query.text[1..].trim_start());
            self.commands_only = true;
        }
        if self.commands_only {
            let q = self.query.text.trim().to_lowercase();
            let found = (0..self.commands.len()).filter(|&i| q.is_empty() || command(&q, self.commands[i].label) || command(&q, &self.commands[i].keys));
            self.items = found.map(Item::Command).collect();
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
                .names
                .iter()
                .enumerate()
                .filter_map(|(i, name)| Some((fuzzy(&q, name)?, i)))
                .collect();
            // Stable sort: at equal score, the "most recent first" order is kept.
            scored.sort_by_key(|(score, _)| -score);
            let mut named = vec![false; self.entries.len()];
            for &(_, i) in &scored {
                named[i] = true;
            }
            let mut items: Vec<Item> = scored.into_iter().map(|(_, i)| Item::Note(i)).collect();
            // After the names: the notes whose text contains all the words, most recent first.
            // A single letter would bring up nearly all of them.
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
        // Without typing anything, on a table or a diagram: what can be done there, under the recent notes.
        if q.is_empty() && self.context != "Note" {
            items.extend((0..self.here.min(self.commands.len()).min(6)).map(Item::Command));
        }
        let exact = self.names.contains(&q);
        if !q.is_empty() && !q.starts_with('#') && !exact {
            items.push(Item::Create);
        }
        for (label, item) in [(vault_label(), Item::Vault), (help_label(), Item::Help)] {
            if command(&q, label) {
                items.push(item);
            }
        }
        // Like the settings: offered only when searched for.
        for (label, item) in [(diagram_label(), Item::Diagram), (folder_label(), Item::Folder), (import_label(), Item::Import)] {
            if !q.is_empty() && command(&q, label) {
                items.push(item);
            }
        }
        for (label, item) in [(outline_label(), Item::Outline), (comments_label(), Item::Comments), (board_label(), Item::Board), (today_label(), Item::Today), (back_label(), Item::Back), (forward_label(), Item::Forward), (trash_label(), Item::Trash), (backup_label(), Item::Backup), (sync_label(), Item::Sync), (clone_label(), Item::CloneVault), (terminal_label(), Item::Terminal), (capture_label(), Item::CaptureKey)] {
            if !q.is_empty() && command(&q, label) {
                items.push(item);
            }
        }
        if self.undo_swap && !q.is_empty() && command(&q, undo_swap_label()) {
            items.push(Item::UndoSwap);
        }
        if let Some(version) = &self.installable
            && !q.is_empty()
            && command(&q, &install_label(version))
        {
            items.push(Item::Install);
        }
        if let Some(on) = self.updates
            && !q.is_empty()
            && command(&q, updates_label(on))
        {
            items.push(Item::Updates);
        }
        if self.updates.is_some() && !q.is_empty() && command(&q, check_label()) {
            items.push(Item::Check);
        }
        if self.table && !q.is_empty() {
            items.extend(
                Pick::ALL.into_iter().filter(|p| command(&q, p.label())).map(Item::Table),
            );
        }
        if (self.cells || self.table) && !q.is_empty() {
            let shown = |op: &TableOp| (self.cells || op.in_sheet()) && command(&q, op.label());
            items.extend(TableOp::ALL.into_iter().filter(shown).map(Item::Grid));
        }
        if !q.is_empty() {
            let acts = [TemplateAct::New, TemplateAct::Insert, TemplateAct::Save];
            items.extend(acts.into_iter().filter(|a| (*a == TemplateAct::New || self.lists) && command(&q, a.label())).map(Item::Template));
        }
        if self.lists && !q.is_empty() {
            items.extend(
                [ListStyle::Bullet, ListStyle::Ordered, ListStyle::Task, ListStyle::Text]
                    .into_iter()
                    .filter(|&s| command(&q, list_label(s)))
                    .map(Item::Restyle),
            );
        }
        // The app's commands, by what they do; those the palette already offers under another
        // name (table gestures, outline, daily note…) are not there twice.
        if !q.is_empty() {
            let doubled = |name: &str| {
                name.starts_with("editor::Table")
                    || name.starts_with("sheet::Pick")
                    || matches!(
                        name,
                        "app::Outline" | "app::Today" | "app::OpenVault" | "app::GoBack" | "app::GoForward" | "app::SyncVault" | "app::ToggleHelp"
                            | "app::NewDiagram" | "nav::NewFolder" | "app::NewFromTemplate" | "app::ChooseTheme" | "editor::ListBullet"
                            | "editor::ListOrdered" | "editor::ListTask"
                    )
            };
            let found = (0..self.commands.len()).filter(|&i| !doubled(self.commands[i].action) && command(&q, self.commands[i].label));
            items.extend(found.take(8).map(Item::Command));
        }
        // The settings do not clutter the list until searched for.
        for setting in [Setting::Theme, Setting::Font, Setting::Mono, Setting::Page, Setting::Wide, Setting::Bar] {
            if !q.is_empty() && command(&q, setting.label()) {
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
            return cx.emit(PaletteEvent::Submit(self.query.text.trim().to_string()));
        }
        cx.emit(match self.items.get(index) {
            Some(Item::Note(i) | Item::Text(i)) => PaletteEvent::Open(self.entries[*i].path.clone()),
            Some(&Item::Line(i, row)) => PaletteEvent::OpenAt(self.entries[i].path.clone(), row, self.query.text.trim().to_string()),
            Some(Item::Create) => PaletteEvent::Create(self.query.text.trim().to_string()),
            Some(Item::Vault) => PaletteEvent::ChangeVault,
            Some(Item::Help) => PaletteEvent::Help,
            Some(Item::Diagram) => PaletteEvent::NewDiagram,
            Some(Item::Folder) => PaletteEvent::NewFolder,
            Some(&Item::Table(pick)) => PaletteEvent::Table(pick),
            Some(&Item::Restyle(style)) => PaletteEvent::Restyle(style),
            Some(&Item::Grid(op)) => PaletteEvent::Grid(op),
            Some(&Item::Command(n)) => PaletteEvent::Run(self.commands[n].action),
            Some(&Item::Template(act)) => PaletteEvent::Template(act),
            Some(Item::Import) => PaletteEvent::ImportDiagram,
            Some(Item::Updates) => PaletteEvent::ToggleUpdates,
            Some(Item::Check) => PaletteEvent::CheckUpdate,
            Some(Item::Outline) => PaletteEvent::Outline,
            Some(Item::Comments) => PaletteEvent::Comments,
            Some(Item::UndoSwap) => PaletteEvent::UndoSwap,
            Some(Item::Board) => PaletteEvent::NewBoard,
            Some(Item::Today) => PaletteEvent::Today,
            Some(Item::Back) => PaletteEvent::Travel(false),
            Some(Item::Forward) => PaletteEvent::Travel(true),
            Some(Item::Trash) => PaletteEvent::Trash,
            Some(Item::Backup) => PaletteEvent::Backup,
            Some(Item::Sync) => PaletteEvent::Sync,
            Some(Item::CloneVault) => PaletteEvent::CloneVault,
            Some(Item::Terminal) => PaletteEvent::Terminal,
            Some(Item::CaptureKey) => PaletteEvent::CaptureKey,
            Some(Item::Install) => PaletteEvent::InstallUpdate,
            Some(Item::Setting(setting)) => PaletteEvent::Setting(*setting),
            None => PaletteEvent::Dismiss,
        });
    }

    fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        self.query.insert(text);
        self.refresh();
        self.preview(cx);
        cx.notify();
    }
}

// ponytail: input reduced to "append at the end" (no movable cursor nor
// IME composition in the palette); enough for a search query.
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
        Some(self.query.utf16())
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

impl Palette {
    /// A command: its icon, its label, its keys one by one; above, the title of its group
    /// when it opens it.
    fn command_row(&self, i: usize, n: usize, header: bool, cx: &Context<Self>) -> gpui::Div {
        let t = self.theme;
        let c = &self.commands[n];
        let on = i == self.selected;
        let caps = crate::logic::commands::keycaps(&c.keys).into_iter().map(|key| {
            div()
                .min_w(px(20.))
                .px(px(6.))
                .rounded(px(4.))
                .flex()
                .justify_center()
                .bg(if on { t.panel.opacity(0.7) } else { t.code_bg })
                .text_size(px(11.5))
                .text_color(t.soft)
                .child(key)
        });
        let title = header.then(|| {
            div().px_4().pt(px(10.)).pb(px(4.)).text_size(px(11.5)).text_color(t.soft).child(c.group.title(n < self.here))
        });
        let row = div()
            .id(("command", i))
            .debug_selector({
                let action = c.action;
                move || format!("command-{action}")
            })
            .mx_1()
            .px_3()
            .h(px(34.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(11.))
            .cursor_pointer()
            .when(on, |d| d.bg(t.selection))
            .when(!on, |d| d.hover(|s| s.bg(t.code_bg)))
            .child(svg().path(c.icon).size(px(16.)).flex_none().text_color(if on { t.accent } else { t.soft }))
            .child(div().flex_1().min_w_0().truncate().child(c.label))
            .child(div().flex_none().flex().gap(px(3.)).children(caps))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.confirm(i, cx)));
        div().flex().flex_col().children(title).child(row)
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (t, top) = (self.theme, self.top);
        let focus = self.focus.clone();
        let entity = cx.entity();
        let input = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
            },
        )
        .size_0();

        // The input line scrolls under its cursor: it stays in view whatever is typed or pasted.
        let (line, layout) = self.query.shown(t);
        let (scroll, seen, at, palette) = (self.scroll.clone(), self.caret.clone(), self.query.cursor(), cx.entity_id());
        let follow = canvas(
            |_, _, _| (),
            move |_, _, _, cx| {
                let Some(caret) = layout.position_for_index(at) else { return };
                let frame = scroll.bounds();
                // Enough to bring the cursor back into the frame; the next frame shows it.
                let slide = if caret.x > frame.right() - px(2.) {
                    frame.right() - px(2.) - caret.x
                } else if caret.x < frame.left() {
                    frame.left() - caret.x
                } else {
                    px(0.)
                };
                seen.set(Some(caret.x));
                // Bounded to what can scroll: at the end, nothing more is requested.
                let mut offset = scroll.offset();
                let to = (offset.x + slide).max(-scroll.max_offset().width).min(px(0.));
                if (to - offset.x).abs() > px(0.5) {
                    offset.x = to;
                    scroll.set_offset(offset);
                    // During drawing, `Window::refresh` has no effect: request the render afterwards.
                    cx.defer(move |cx| cx.notify(palette));
                }
            },
        )
        .absolute()
        .size_0();

        // A choice list shows 14 lines at a time, around the selected choice.
        let first = self.selected.saturating_sub(6).min(self.items.len().saturating_sub(14));
        let rows = self.items.iter().enumerate().skip(first).take(14).map(|(i, item)| {
            if let &Item::Command(n) = item {
                // Without typing anything, commands go by group, each under its title.
                let grouped = self.query.text.trim().is_empty();
                let group = self.commands[n].group;
                let same = |prev: Option<&Item>| matches!(prev, Some(&Item::Command(m)) if self.commands[m].group == group);
                let header = grouped && (i == first || !same(i.checked_sub(1).and_then(|p| self.items.get(p))));
                return self.command_row(i, n, header, cx);
            }
            let (label, detail) = match item {
                Item::Note(n) => {
                    let e = &self.entries[*n];
                    let tags: Vec<String> = e.tags.iter().take(4).map(|t| format!("#{t}")).collect();
                    (e.name.clone(), tags.join(" "))
                }
                Item::Text(n) => {
                    let e = &self.entries[*n];
                    let word = self.query.text.trim().to_lowercase();
                    let word = word.split_whitespace().next().unwrap_or_default().to_string();
                    (e.name.clone(), snippet(&e.body, &e.lower, &word))
                }
                Item::Line(n, row) => {
                    let e = &self.entries[*n];
                    (e.name.clone(), snippet_at(&e.body, &e.lower, *row, &self.query.text.trim().to_lowercase()))
                }
                Item::Create => (
                    format!("{} « {} »", tr("Create", "Créer"), self.query.text.trim()),
                    tr("new note", "nouvelle note").into(),
                ),
                Item::Vault => (vault_label().to_string(), crate::ui::keys::of(cx, &crate::actions::OpenVault)),
                Item::Help => (help_label().to_string(), crate::ui::keys::of(cx, &crate::actions::ToggleHelp)),
                Item::Diagram => (diagram_label().to_string(), crate::ui::keys::of(cx, &crate::actions::NewDiagram)),
                Item::Folder => (folder_label().to_string(), crate::ui::keys::of(cx, &crate::views::nav::NewFolder)),
                Item::Import => (import_label().to_string(), String::new()),
                Item::Updates => (updates_label(self.updates.unwrap_or(true)).to_string(), String::new()),
                Item::Check => (check_label().to_string(), String::new()),
                Item::Outline => (outline_label().to_string(), crate::ui::keys::of(cx, &crate::actions::Outline)),
                Item::Comments => (comments_label().to_string(), String::new()),
                &Item::Restyle(style) => (list_label(style).to_string(), String::new()),
                &Item::Grid(op) => (op.label().to_string(), crate::ui::keys::of(cx, op.action().as_ref())),
                &Item::Command(n) => (self.commands[n].label.to_string(), self.commands[n].keys.clone()),
                &Item::Template(act) => (act.label().to_string(), if act == TemplateAct::New { crate::ui::keys::of(cx, &crate::actions::NewFromTemplate) } else { String::new() }),
                Item::UndoSwap => (undo_swap_label().to_string(), String::new()),
                Item::Board => (board_label().to_string(), String::new()),
                Item::Today => (today_label().to_string(), crate::ui::keys::of(cx, &crate::actions::Today)),
                Item::Back => (back_label().to_string(), crate::ui::keys::of(cx, &crate::actions::GoBack)),
                Item::Forward => (forward_label().to_string(), crate::ui::keys::of(cx, &crate::actions::GoForward)),
                Item::Trash => (trash_label().to_string(), String::new()),
                Item::Backup => (backup_label().to_string(), String::new()),
                Item::Sync => (sync_label().to_string(), crate::ui::keys::of(cx, &crate::actions::SyncVault)),
                Item::CloneVault => (clone_label().to_string(), String::new()),
                Item::Terminal => (terminal_label().to_string(), String::new()),
                Item::CaptureKey => (capture_label().to_string(), String::new()),
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
                .cursor_pointer()
                .when(i == self.selected, |d| d.bg(t.selection))
                .when(i != self.selected, |d| d.hover(|s| s.bg(t.code_bg)))
                // With a text excerpt, the name keeps its place and the excerpt is shortened.
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
                        .text_color(t.soft)
                        .text_size(px(12.))
                        .child(detail),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.confirm(i, cx)),
                )
        });

        let root = div()
            .absolute()
            .inset_0()
            .occlude()
            // The keys of an input line serve the question being typed.
            .key_context("Palette Line")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Next, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &Prev, _, cx| this.step(this.items.len().max(1) - 1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(this.selected, cx)))
            .on_action(cx.listener(|this, _: &ConfirmAside, _, cx| {
                this.aside = true;
                this.confirm(this.selected, cx)
            }))
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.emit(PaletteEvent::Dismiss)))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Dismiss)));
        line::keys(root, cx, |this| Some(&mut this.query), |this, cx| {
            this.refresh();
            this.preview(cx);
        })
            .flex()
            .justify_center()
            .items_start()
            .child(
                div()
                    .debug_selector(|| "palette-panel".into())
                    .mt(self.top)
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
                            // Neither the prompt nor a long pasted address overflows the frame.
                            .overflow_hidden()
                            .child(input)
                            .when(self.commands_only, |d| d.child(svg().path("search.svg").size(px(16.)).flex_none().mr(px(10.)).text_color(t.soft)))
                            // The cursor first, the prompt after it: one writes in its place.
                            .child(div().id("palette-line").min_w_0().overflow_x_scroll().track_scroll(&self.scroll).flex().child(line.flex_none().pr(px(2.)).whitespace_nowrap()).child(follow))
                            .when(self.query.text.is_empty(), |d| {
                                d.child(div().min_w_0().truncate().text_color(t.soft).child(self.prompt.clone().unwrap_or_else(|| match (self.lines, self.commands_only) {
                                    (true, _) => tr("Search in the text of the vault…", "Chercher dans le texte du coffre…").into(),
                                    (_, true) => tr("Search a command…", "Rechercher une commande…").into(),
                                    _ => tr("Search or create a note, #tag…", "Chercher ou créer une note, #tag…").into(),
                                })))
                            })
                            .when(!self.query.text.is_empty(), |d| {
                                d.children(self.prompt.clone().map(|label| {
                                    div().ml_auto().pl_3().flex_none().max_w(gpui::relative(0.45)).truncate().text_size(px(12.)).text_color(t.soft).child(label)
                                }))
                            })
                            // What the commands target first: the note, a table, a diagram.
                            .when(self.commands_only && !self.context.is_empty(), |d| {
                                d.child(div().flex_1()).child(
                                    div()
                                        .debug_selector(|| "palette-context".into())
                                        .flex_none()
                                        .px(px(7.))
                                        .py(px(1.))
                                        .rounded(px(5.))
                                        .bg(t.code_bg)
                                        .text_size(px(11.5))
                                        .text_color(t.soft)
                                        .child(self.context),
                                )
                            }),
                    )
                    .children(rows)
                    // The panel slides into place when opening.
                    .with_animation(
                        "palette-in",
                        Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                        move |panel, delta| panel.opacity(delta).mt(top - px(8.) * (1. - delta)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{command, contains_all, fuzzy, rows_with, snippet, snippet_at};

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

    #[test]
    fn finds_commands_by_words() {
        assert!(command("folder", "New folder"));
        assert!(command("new fol", "New folder"));
        assert!(command("theme", "Thème…"));
        assert!(command("", "Change vault…"));
        // No more subsequence: scattered letters do not bring up an unrelated command.
        assert!(!command("plan", "Annuler le remplacement dans le coffre"));
        assert!(!command("nvf", "New folder"));
    }
}
