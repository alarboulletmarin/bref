//! Palette (Ctrl+P): note search, `#tag` filter, creation, vault.
//! Also serves as a plain input field (folder name, new name).

use std::{path::PathBuf, sync::Arc};

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, Context, ElementInputHandler,
    EventEmitter, FocusHandle, Focusable, MouseButton, Pixels, Window,
    actions, canvas, div, ease_out_quint, prelude::*, px, svg,
};

use crate::{
    ui::theme::Theme,
    ui::line::{self, Line},
    ui::widgets::keycaps,
    logic::markdown::ListStyle,
    logic::rank::{Words, fuzzy},
    lang::{fold, tr},
};

/// Height of a row of the list.
const ROW: f32 = 38.;

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
    /// Fades when something opens.
    Motion,
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
            Setting::Motion => tr("Animations…", "Animations…"),
        }
    }
}

actions!(palette, [Prev, Next, Confirm, ConfirmAside, Dismiss, RemoveChoice, ChoiceUp, ChoiceDown]);

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
    /// Whole days since the file was written; `None` when unknown.
    pub days: Option<u64>,
    /// Text of the note, for full-text search.
    pub body: Arc<str>,
    /// What a choice says beside its name, in place of its folder (the vault switcher: its notes).
    pub said: String,
    /// The choice is there but cannot be used as it is (a vault whose folder is gone): drawn faded.
    pub faded: bool,
}

pub enum PaletteEvent {
    Open(PathBuf),
    /// An app command: the name of its action.
    Run(&'static str),
    Create(String),
    Setting(Setting),
    /// The chosen lines of the note, as a list or as text.
    Restyle(ListStyle),
    /// A note born from a template, a template inserted, or the note saved as a template.
    Template(TemplateAct),
    /// Turns the new-version check on or off.
    ToggleUpdates,
    /// Checks for a new version right now.
    CheckUpdate,
    /// Undoes the last vault-wide replacement.
    UndoSwap,
    /// In a list that can be edited (the vault switcher): take the choice at this index off the list,
    /// or move it one place up (`true`) or down.
    Remove(usize),
    Shift(usize, bool),
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
    Create,
    Updates,
    Check,
    Install,
    UndoSwap,
    Setting(Setting),
    Restyle(ListStyle),
    Template(TemplateAct),
    /// An app command (`commands`), by its index.
    Command(usize),
}

pub struct Palette {
    /// The vault's folder: what the rows' folders are shown from.
    root: PathBuf,
    /// Submitted with Ctrl+Enter: the choice opens in the other pane.
    pub aside: bool,
    focus: FocusHandle,
    query: Line,
    entries: Vec<Entry>,
    /// The name of each entry in lowercase, done once: the list is rebuilt on every keystroke.
    names: Vec<String>,
    /// The words typed, as the notes' text is searched for them: kept for the excerpt of each row found.
    words: Words,
    items: Vec<Item>,
    selected: usize,
    /// Input field: its label. No note is offered then.
    prompt: Option<String>,
    /// Choice list: `entries` are the options, without creation or commands.
    choices: bool,
    /// Search in the vault's text: one found line per choice, no note by name nor command.
    /// The new-version check is on: `None` outside the main palette.
    updates: Option<bool>,
    /// Number of the new version the app can install by itself.
    installable: Option<String>,
    /// A note is shown: its lines can become a list.
    lists: bool,
    /// A vault replacement can be undone.
    undo_swap: bool,
    /// A choice list whose choices can be removed and moved (the vault switcher); the last one stays put.
    editable: bool,
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

impl Palette {
    /// `entries`: the notes, most recent first.
    /// The notes' folders are shown from `root`.
    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = root;
        self
    }

    /// The folder of the file in the vault, as a path; empty at the root.
    fn folder_of(&self, path: &std::path::Path) -> String {
        path.parent().and_then(|dir| dir.strip_prefix(&self.root).ok()).map(|dir| dir.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn new(entries: Vec<Entry>, query: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus: cx.focus_handle(),
            query: Line::new(query),
            names: entries.iter().map(|e| e.name.to_lowercase()).collect(),
            words: Words::default(),
            entries,
            items: Vec::new(),
            selected: 0,
            aside: false,
            root: PathBuf::new(),
            prompt: None,
            choices: false,
            updates: None,
            installable: None,
            lists: false,
            undo_swap: false,
            editable: false,
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

    pub fn with_lists(mut self, shown: bool) -> Self {
        self.lists = shown;
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

    /// A choice list of full entries (name, what it says, age, faded), `current` preselected. With `editable`,
    /// Shift+Delete takes a choice off and Alt+Up/Down move it, but for the last one, which is an action.
    pub fn choose_entries(label: &str, entries: Vec<Entry>, current: usize, editable: bool, theme: Theme, cx: &mut Context<Self>) -> Self {
        let mut this = Self { prompt: Some(label.into()), choices: true, editable, ..Self::new(entries, "", theme, cx) };
        this.refresh();
        this.selected = this.items.iter().position(|i| matches!(i, Item::Note(n) if *n == current)).unwrap_or(0);
        this
    }

    /// The edit asked on the selected choice of an editable list, if it is not the last (the action).
    fn edit_choice(&mut self, edit: impl Fn(usize) -> PaletteEvent, cx: &mut Context<Self>) {
        if let Some(at) = self.chosen().filter(|at| self.editable && at + 1 < self.entries.len()) {
            cx.emit(edit(at));
        }
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

    /// What is typed.
    #[cfg(test)]
    pub fn typed_text(&self) -> &str {
        &self.query.text
    }

    #[cfg(test)]
    pub fn listed(&self) -> usize {
        self.items.len()
    }

    /// What the empty list says, under the input: that nothing matches. An input field and an empty query have
    /// nothing to say.
    fn nothing_to_show(&self) -> Option<&'static str> {
        if !self.items.is_empty() || (self.prompt.is_some() && !self.choices) {
            return None;
        }
        (!self.query.text.trim().is_empty()).then(|| tr("No result", "Aucun résultat"))
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

    /// The label of the choice listed at `i`.
    #[cfg(test)]
    pub fn choice_at(&self, i: usize) -> Option<String> {
        self.items.get(i).and_then(|item| self.choice(item)).map(str::to_string)
    }

    /// The choice listed at `i` is drawn faded.
    #[cfg(test)]
    pub fn faded_at(&self, i: usize) -> bool {
        matches!(self.items.get(i), Some(Item::Note(n)) if self.entries[*n].faded)
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
                self.words = Words::of(&q);
                items.extend(
                    (0..self.entries.len())
                        .filter(|&i| !named[i] && self.words.all_in(&self.entries[i].body))
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
        // Offered even when nothing is typed: the vault and the help.
        let pinned = |c: &crate::logic::commands::Command| c.pinned && command(&q, c.label);
        items.extend((0..self.commands.len()).filter(|&i| pinned(&self.commands[i])).map(Item::Command));
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
        if !q.is_empty() && self.lists {
            items.extend([TemplateAct::Insert, TemplateAct::Save].into_iter().filter(|a| command(&q, a.label())).map(Item::Template));
        }
        if self.lists && !q.is_empty() {
            items.extend(
                [ListStyle::Bullet, ListStyle::Ordered, ListStyle::Task, ListStyle::Text]
                    .into_iter()
                    .filter(|&s| command(&q, list_label(s)))
                    .map(Item::Restyle),
            );
        }
        // The app's commands, by what they do (the pinned ones are above already).
        if !q.is_empty() {
            let found = (0..self.commands.len()).filter(|&i| !self.commands[i].pinned && command(&q, self.commands[i].label));
            items.extend(found.take(8).map(Item::Command));
        }
        // The settings do not clutter the list until searched for.
        for setting in [Setting::Font, Setting::Mono, Setting::Page, Setting::Wide, Setting::Bar, Setting::Motion] {
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
            Some(Item::Create) => PaletteEvent::Create(self.query.text.trim().to_string()),
            Some(&Item::Restyle(style)) => PaletteEvent::Restyle(style),
            Some(&Item::Command(n)) => PaletteEvent::Run(self.commands[n].action),
            Some(&Item::Template(act)) => PaletteEvent::Template(act),
            Some(Item::Updates) => PaletteEvent::ToggleUpdates,
            Some(Item::Check) => PaletteEvent::CheckUpdate,
            Some(Item::UndoSwap) => PaletteEvent::UndoSwap,
            Some(Item::Install) => PaletteEvent::InstallUpdate,
            Some(Item::Setting(setting)) => PaletteEvent::Setting(*setting),
            None => PaletteEvent::Dismiss,
        });
    }
}

impl crate::ui::line::LineInput for Palette {
    fn line(&self) -> Option<&Line> {
        Some(&self.query)
    }

    fn line_mut(&mut self) -> Option<&mut Line> {
        Some(&mut self.query)
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        self.refresh();
        self.preview(cx);
        cx.notify();
    }
}

crate::ui::line::line_input!(Palette);

impl Palette {
    /// A command: its icon, its label, its keys one by one; above, the title of its group
    /// when it opens it.
    fn command_row(&self, i: usize, n: usize, header: bool, cx: &Context<Self>) -> gpui::Div {
        let t = self.theme;
        let c = &self.commands[n];
        let on = i == self.selected;
        let title = header.then(|| {
            div().px_4().pt(t.px(10.)).pb(t.px(4.)).text_size(t.px(11.5)).text_color(t.soft).child(c.group.title(n < self.here))
        });
        let row = div()
            .id(("command", i))
            .debug_selector({
                let action = c.action;
                move || format!("command-{action}")
            })
            .mx(t.px(6.))
            .px_3()
            .h(t.px(ROW))
            .rounded(t.large())
            .flex()
            .items_center()
            .gap(t.px(11.))
            .cursor_pointer()
            .when(on, |d| d.bg(t.selection))
            .when(!on, |d| d.hover(|s| s.bg(t.hover)))
            .child(svg().path(c.icon).size(t.px(16.)).flex_none().text_color(if on { t.accent } else { t.soft }))
            .child(div().flex_1().min_w_0().truncate().child(c.label))
            .child(keycaps(&c.keys, t, on).debug_selector({
                let action = c.action;
                move || format!("command-keys-{action}")
            }).flex_none())
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
                let slide = if caret.x > frame.right() - t.px(2.) {
                    frame.right() - t.px(2.) - caret.x
                } else if caret.x < frame.left() {
                    frame.left() - caret.x
                } else {
                    t.px(0.)
                };
                seen.set(Some(caret.x));
                // Bounded to what can scroll: at the end, nothing more is requested.
                let mut offset = scroll.offset();
                let to = (offset.x + slide).max(-scroll.max_offset().width).min(t.px(0.));
                if (to - offset.x).abs() > t.px(0.5) {
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
                let grouped = self.query.text.trim().is_empty() && (self.commands_only || n < self.here);
                let group = self.commands[n].group;
                let same = |prev: Option<&Item>| matches!(prev, Some(&Item::Command(m)) if self.commands[m].group == group);
                let header = grouped && (i == first || !same(i.checked_sub(1).and_then(|p| self.items.get(p))));
                return self.command_row(i, n, header, cx);
            }
            let (label, detail) = match item {
                Item::Note(n) => {
                    let e = &self.entries[*n];
                    // Under `#tag`, the tags; otherwise the folder.
                    let detail = if !e.said.is_empty() {
                        e.said.clone()
                    } else if self.query.text.trim_start().starts_with('#') {
                        e.tags.iter().take(4).map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ")
                    } else {
                        self.folder_of(&e.path)
                    };
                    (e.name.clone(), detail)
                }
                Item::Text(n) => {
                    let e = &self.entries[*n];
                    (e.name.clone(), self.words.snippet(&e.body))
                }
                Item::Create => (
                    format!("{} « {} »", tr("Create", "Créer"), self.query.text.trim()),
                    tr("new note", "nouvelle note").into(),
                ),
                Item::Updates => (updates_label(self.updates.unwrap_or(true)).to_string(), String::new()),
                Item::Check => (check_label().to_string(), String::new()),
                &Item::Restyle(style) => (list_label(style).to_string(), String::new()),
                &Item::Command(n) => (self.commands[n].label.to_string(), self.commands[n].keys.clone()),
                &Item::Template(act) => (act.label().to_string(), String::new()),
                Item::UndoSwap => (undo_swap_label().to_string(), String::new()),
                Item::Install => (install_label(self.installable.as_deref().unwrap_or_default()), String::new()),
                Item::Setting(setting) => (setting.label().to_string(), String::new()),
            };
            let note = matches!(item, Item::Note(_));
            let faded = matches!(item, Item::Note(n) if self.entries[*n].faded);
            let age = match item {
                Item::Note(n) => self.entries[*n].days.map(crate::lang::age),
                _ => None,
            };
            let soft = |d: gpui::Div| d.text_size(t.font_small()).text_color(t.soft);
            div()
                .debug_selector(move || format!("palette-item-{i}"))
                .mx(t.px(6.))
                .px_3()
                .h(t.px(ROW))
                .rounded(t.large())
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .cursor_pointer()
                .when(i == self.selected, |d| d.bg(t.selection))
                .when(i != self.selected, |d| d.hover(|s| s.bg(t.hover)))
                // A note: its name, its folder after it, and on the right how long ago it was written. With a
                // text excerpt, the name keeps its place and the excerpt is shortened.
                .when(note, |d| {
                    d.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(t.px(10.))
                            .child(div().min_w_0().truncate().when(faded, |d| d.text_color(t.soft)).child(label.clone()))
                            .child(soft(div()).debug_selector(move || format!("palette-folder-{i}")).truncate().flex_none().max_w(gpui::relative(0.4)).child(detail.clone())),
                    )
                })
                .when(!note, |d| {
                    d.child(
                        div()
                            .truncate()
                            .when(matches!(item, Item::Text(_)), |d| d.flex_none().max_w(t.px(220.)))
                            .child(label),
                    )
                    .child(
                        div()
                            .truncate()
                            .when(!matches!(item, Item::Text(_)), |d| d.flex_none())
                            .text_color(if i == self.selected { t.text } else { t.soft })
                            .text_size(t.font_small())
                            .child(detail),
                    )
                })
                .children(age.map(|age| soft(div()).debug_selector(move || format!("palette-age-{i}")).flex_none().min_w(t.px(56.)).whitespace_nowrap().text_right().child(age)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| this.confirm(i, cx)),
                )
        });

        // The keys of the main palette, under the list; an input field and an empty list have none to show.
        let footer = ((self.prompt.is_none() || self.editable) && !self.items.is_empty()).then(|| {
            let hint = |keys: String, what: &'static str| div().flex().items_center().gap(t.px(6.)).child(keycaps(&keys, t, false)).child(what);
            let hints = if self.editable {
                vec![
                    hint(crate::ui::keys::of(cx, &Confirm), tr("switch", "changer")),
                    hint(crate::ui::keys::of(cx, &RemoveChoice), tr("take off the list (the folder stays)", "retirer de la liste (le dossier reste)")),
                    hint(crate::ui::keys::of(cx, &ChoiceUp), tr("move up", "monter")),
                    hint(crate::ui::keys::of(cx, &ChoiceDown), tr("move down", "descendre")),
                ]
            } else if self.commands_only {
                vec![hint(crate::ui::keys::of(cx, &Confirm), tr("run", "lancer"))]
            } else {
                vec![
                    hint(crate::ui::keys::of(cx, &Confirm), tr("open", "ouvrir")),
                    hint(crate::ui::keys::of(cx, &ConfirmAside), tr("in the other pane", "dans l'autre volet")),
                    hint(">".into(), tr("commands", "commandes")),
                ]
            };
            div()
                .debug_selector(|| "palette-footer".into())
                .mt(t.px(6.))
                .px(t.px(18.))
                .py(t.px(10.))
                .border_t_1()
                .border_color(t.code_bg)
                .flex()
                .flex_wrap()
                .gap_x(t.px(16.))
                .gap_y(t.px(4.))
                .text_size(t.font_small())
                .text_color(t.soft)
                .children(hints)
        });

        let root = div()
            .absolute()
            .inset_0()
            .occlude()
            // The keys of an input line serve the question being typed.
            .key_context(crate::ui::keys::context::PALETTE_LINE)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Next, _, cx| this.step(1, cx)))
            .on_action(cx.listener(|this, _: &Prev, _, cx| this.step(this.items.len().max(1) - 1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(this.selected, cx)))
            .on_action(cx.listener(|this, _: &ConfirmAside, _, cx| {
                this.aside = true;
                this.confirm(this.selected, cx)
            }))
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.emit(PaletteEvent::Dismiss)))
            .on_action(cx.listener(|this, _: &RemoveChoice, _, cx| this.edit_choice(PaletteEvent::Remove, cx)))
            .on_action(cx.listener(|this, _: &ChoiceUp, _, cx| this.edit_choice(|at| PaletteEvent::Shift(at, true), cx)))
            .on_action(cx.listener(|this, _: &ChoiceDown, _, cx| this.edit_choice(|at| PaletteEvent::Shift(at, false), cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Dismiss)));
        line::keys(root, cx, |this| Some(&mut this.query), |this, cx| {
            this.refresh();
            this.preview(cx);
        })
            .flex()
            .justify_center()
            .items_start()
            .child(
                crate::ui::widgets::floating(t)
                    .debug_selector(|| "palette-panel".into())
                    .mt(self.top)
                    .w(t.px(560.))
                    .max_w_full()
                    .pb(t.px(if footer.is_some() { 0. } else { 6. }))
                    .text_size(t.px(14.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .h(t.px(52.))
                            .px(t.px(18.))
                            .mb(t.px(6.))
                            .border_b_1()
                            .border_color(t.code_bg)
                            .flex()
                            .items_center()
                            .text_size(t.px(15.))
                            // Neither the prompt nor a long pasted address overflows the frame.
                            .overflow_hidden()
                            .child(input)
                            .when(self.commands_only, |d| d.child(svg().path("search.svg").size(t.px(16.)).flex_none().mr(t.px(10.)).text_color(t.soft)))
                            // The cursor first, the prompt after it: one writes in its place.
                            .child(div().id("palette-line").min_w_0().overflow_x_scroll().track_scroll(&self.scroll).flex().child(line.flex_none().pr(t.px(2.)).whitespace_nowrap()).child(follow))
                            .when(self.query.text.is_empty(), |d| {
                                d.child(div().min_w_0().truncate().text_color(t.soft).child(self.prompt.clone().unwrap_or_else(|| match self.commands_only {
                                    true => tr("Search a command…", "Rechercher une commande…").into(),
                                    false => tr("Search or create a note, #tag…", "Chercher ou créer une note, #tag…").into(),
                                })))
                            })
                            .when(!self.query.text.is_empty(), |d| {
                                d.children(self.prompt.clone().map(|label| {
                                    div().ml_auto().pl_3().flex_none().max_w(gpui::relative(0.45)).truncate().text_size(t.font_small()).text_color(t.soft).child(label)
                                }))
                            })
                            // What the commands target first: the note, a table, a diagram.
                            .when(self.commands_only && !self.context.is_empty(), |d| {
                                d.child(div().flex_1()).child(
                                    div()
                                        .debug_selector(|| "palette-context".into())
                                        .flex_none()
                                        .px(t.px(7.))
                                        .py(t.px(1.))
                                        .rounded(t.small())
                                        .bg(t.code_bg)
                                        .text_size(t.px(11.5))
                                        .text_color(t.soft)
                                        .child(self.context),
                                )
                            }),
                    )
                    .children(rows)
                    .children(footer)
                    .children(self.nothing_to_show().map(|said| {
                        div().debug_selector(|| "palette-empty".into()).px_4().py_3().text_center().text_size(t.font_body()).text_color(t.soft).child(said)
                    }))
                    // The panel slides into place when opening.
                    .with_animation(
                        "palette-in",
                        Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                        move |panel, delta| panel.opacity(delta).mt(top - t.px(8.) * (1. - delta)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn finds_commands_by_words() {
        assert!(command("folder", "New folder"));
        assert!(command("new fol", "New folder"));
        assert!(command("theme", "Thème…"));
        // Folded like the rest of the app: œ is oe, a typographic apostrophe a straight one.
        assert!(command("coeur", "Cœur"));
        assert!(command("l'aide", "Ouvrir l’aide"));
        assert!(command("l’aide", "Ouvrir l'aide"));
        assert!(command("", "Change vault…"));
        // No more subsequence: scattered letters do not bring up an unrelated command.
        assert!(!command("plan", "Annuler le remplacement dans le coffre"));
        assert!(!command("nvf", "New folder"));
    }
}
