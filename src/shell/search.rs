//! The vault search of the side panel (Ctrl+Shift+F): the matches of a search, note by note, found in the
//! background; replacing in the whole vault, in one note, or all but the dismissed matches; and undoing it.

use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
    ops::Range,
};

use super::*;
use editor::{Find, FindCase, FindRegex, FindReplace, FindSwitch, FindWord, ReplaceAll, ReplaceInVault};
use field::{Field, FieldEvent};
use gpui::{HighlightStyle, ScrollStrategy, StyledText, Task, UniformListScrollHandle, uniform_list};
use markdown::{Hit, Swap};

/// The notes where the search was found, each with its matches.
pub(super) struct Group {
    path: PathBuf,
    name: String,
    body: Arc<str>,
    hits: Vec<Hit>,
}

/// A row of the results: a note's header, or one of its matches.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Row {
    Note(usize),
    Hit(usize, usize),
}

pub(super) struct VaultFind {
    query: Entity<Field>,
    with: Entity<Field>,
    /// The replace line shows.
    replace: bool,
    case: bool,
    word: bool,
    regex: bool,
    groups: Vec<Group>,
    /// Notes whose matches are hidden under their header.
    folded: HashSet<PathBuf>,
    /// Matches set aside (note, byte where they start): not listed, not replaced.
    // ponytail: known by their byte, an edit above one in its note moves it; keyed by the line's text if it misleads.
    dismissed: HashSet<(PathBuf, usize)>,
    /// Selected row, moved by the arrows of the search field.
    sel: usize,
    scroll: UniformListScrollHandle,
    /// The search running in the background; dropped, it stops.
    busy: Option<Task<()>>,
    /// The search, and the notes (`seen`), the results were computed for.
    asked: Option<Swap>,
    seen: u64,
    _fields: [gpui::Subscription; 2],
}

impl VaultFind {
    fn swap(&self, cx: &App) -> Swap {
        let (query, with) = (self.query.read(cx).line.text.clone(), self.with.read(cx).line.text.clone());
        Swap { query, with, case: self.case, word: self.word, regex: self.regex }
    }

    fn kept<'a>(&'a self, group: &'a Group) -> impl Iterator<Item = (usize, &'a Hit)> {
        group.hits.iter().enumerate().filter(|(_, hit)| !self.dismissed.contains(&(group.path.clone(), hit.at.start)))
    }

    /// The rows as listed: each note that still has a match, then its matches unless folded.
    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (g, group) in self.groups.iter().enumerate() {
            let kept: Vec<usize> = self.kept(group).map(|(h, _)| h).collect();
            if kept.is_empty() {
                continue;
            }
            rows.push(Row::Note(g));
            if !self.folded.contains(&group.path) {
                rows.extend(kept.into_iter().map(|h| Row::Hit(g, h)));
            }
        }
        rows
    }

    /// Matches and notes listed.
    fn counts(&self) -> (usize, usize) {
        let each = self.groups.iter().map(|group| self.kept(group).count()).filter(|&n| n > 0);
        each.fold((0, 0), |(hits, notes), n| (hits + n, notes + 1))
    }

    /// The bytes of the dismissed matches of a note: they stay as they are.
    fn skipped(&self, path: &Path) -> Vec<usize> {
        self.dismissed.iter().filter(|(at, _)| at == path).map(|(_, start)| *start).collect()
    }
}

impl Shell {
    /// Ctrl+Shift+F: the panel shows the vault search, on the search of the note's bar when it has one; the
    /// keys go to its search field, all selected.
    pub(super) fn open_vault_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.flush(cx);
        let seed = self.pane().editor.read(cx).search(cx).filter(|swap| !swap.query.is_empty());
        if self.vault_find.is_none() {
            self.vault_find = Some(self.new_vault_find(window, cx));
        }
        let find = self.vault_find.as_mut().unwrap();
        let query = seed.as_ref().map_or_else(|| find.query.read(cx).line.text.clone(), |seed| seed.query.clone());
        find.query.update(cx, |field, cx| field.set(&query, cx));
        if let Some(seed) = seed {
            find.with.update(cx, |field, cx| field.set(&seed.with, cx));
            (find.case, find.word, find.regex) = (seed.case, seed.word, seed.regex);
        }
        window.focus(&find.query.focus_handle(cx));
        self.put_tool(Mode::Search);
        self.vault_search(cx);
        self.settle_nav(window, cx);
    }

    fn new_vault_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> VaultFind {
        let theme = self.theme;
        let (query, with) = (cx.new(|cx| Field::new("", theme, cx).scrolling()), cx.new(|cx| Field::new("", theme, cx).scrolling()));
        // Typed: searched again. Enter in the search opens the selected match; in the replacement, replaces in
        // the vault after the count. Escape gives the keys back to the note.
        let on = |replacing: bool| {
            move |this: &mut Self, _: &Entity<Field>, event: &FieldEvent, window: &mut Window, cx: &mut Context<Self>| match event {
                FieldEvent::Changed if replacing => cx.notify(),
                FieldEvent::Changed => this.vault_search(cx),
                FieldEvent::Done if replacing => this.vault_replace(None, window, cx),
                FieldEvent::Done => this.vault_open_selected(window, cx),
                FieldEvent::Cancel => window.focus(&this.pane().editor.focus_handle(cx)),
            }
        };
        let fields = [cx.subscribe_in(&query, window, on(false)), cx.subscribe_in(&with, window, on(true))];
        VaultFind {
            query,
            with,
            replace: false,
            case: false,
            word: false,
            regex: false,
            groups: Vec::new(),
            folded: HashSet::new(),
            dismissed: HashSet::new(),
            sel: 0,
            scroll: UniformListScrollHandle::new(),
            busy: None,
            asked: None,
            seen: 0,
            _fields: fields,
        }
    }

    /// What the notes are: a change (saved, renamed, deleted, from outside too) searches again.
    fn notes_seen(&self) -> u64 {
        let mut hash = std::hash::DefaultHasher::new();
        for note in &self.notes {
            (&note.path, Arc::as_ptr(&note.body) as *const u8 as usize, note.body.len()).hash(&mut hash);
        }
        hash.finish()
    }

    /// Searches the notes in the background if the search or the notes changed; the results of the previous
    /// search stay until then.
    pub(super) fn vault_search(&mut self, cx: &mut Context<Self>) {
        let seen = self.notes_seen();
        let Some(find) = &mut self.vault_find else { return };
        let swap = find.swap(cx);
        let same = find.asked.as_ref().is_some_and(|asked| (&asked.query, asked.case, asked.word, asked.regex) == (&swap.query, swap.case, swap.word, swap.regex));
        if same && find.seen == seen {
            return;
        }
        (find.asked, find.seen) = (Some(swap.clone()), seen);
        if !same {
            find.dismissed.clear();
            find.sel = 0;
        }
        if swap.query.is_empty() || markdown::broken(&swap.query, swap.word, swap.regex) {
            (find.groups, find.busy) = (Vec::new(), None);
            return cx.notify();
        }
        let mut notes: Vec<(PathBuf, String, Arc<str>)> = self.notes.iter().map(|note| (note.path.clone(), note.name.clone(), note.body.clone())).collect();
        // Sorted in the background too: the keystroke only copies the list.
        let work = cx.background_executor().spawn(async move {
            notes.sort_by_cached_key(|(path, name, _)| (name.to_lowercase(), path.clone()));
            let found = notes.into_iter().map(|(path, name, body)| (markdown::hits(&body, &swap), path, name, body));
            found.filter(|(hits, ..)| !hits.is_empty()).map(|(hits, path, name, body)| Group { path, name, body, hits }).collect::<Vec<_>>()
        });
        find.busy = Some(cx.spawn(async move |this, cx| {
            let groups = work.await;
            this.update(cx, |this, cx| {
                if let Some(find) = &mut this.vault_find {
                    find.groups = groups;
                    find.busy = None;
                    find.sel = find.sel.min(find.rows().len().saturating_sub(1));
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Up and down in the search field: the selected row moves, and stays in sight.
    fn vault_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(find) = &mut self.vault_find else { return };
        let last = find.rows().len().saturating_sub(1);
        find.sel = if forward { (find.sel + 1).min(last) } else { find.sel.saturating_sub(1) };
        find.scroll.scroll_to_item(find.sel, ScrollStrategy::Center);
        cx.notify();
    }

    /// Enter: a match opens its note on it; a note's header folds or unfolds it.
    fn vault_open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.vault_find.as_ref().and_then(|find| find.rows().get(find.sel).copied()) else { return };
        self.vault_open(row, window, cx);
    }

    fn vault_open(&mut self, row: Row, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = &mut self.vault_find else { return };
        match row {
            Row::Note(g) => {
                let path = find.groups[g].path.clone();
                if !find.folded.remove(&path) {
                    find.folded.insert(path);
                }
                cx.notify();
            }
            Row::Hit(g, h) => {
                let (path, at, swap) = (find.groups[g].path.clone(), find.groups[g].hits[h].at.start, find.swap(cx));
                if !path.is_file() {
                    return self.say(Tone::Failed, format!("{} {}", tr("This note is no longer there:", "Cette note n'est plus là :"), vault::shown(&path)), cx);
                }
                self.open(&path, window, cx);
                self.pane().editor.clone().update(cx, |editor, cx| editor.show_match(&swap, at, window, cx));
            }
        }
    }

    /// A match set aside: neither listed nor replaced.
    fn vault_dismiss(&mut self, g: usize, h: usize, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.vault_find {
            let key = (find.groups[g].path.clone(), find.groups[g].hits[h].at.start);
            find.dismissed.insert(key);
            find.sel = find.sel.min(find.rows().len().saturating_sub(1));
        }
        cx.notify();
    }

    /// Replaces from the panel: in one note at once (`only`), or in the whole vault after the count is accepted;
    /// the dismissed matches stay.
    fn vault_replace(&mut self, only: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = &self.vault_find else { return };
        let swap = find.swap(cx);
        if swap.query.is_empty() {
            return;
        }
        match only {
            Some(path) => {
                let skip = find.skipped(&path);
                self.flush(cx);
                let found = self.notes.iter().find(|note| note.path == path).and_then(|note| markdown::swap_except(&note.body, &swap, &skip));
                if let Some((text, count)) = found {
                    self.swap_notes(vec![(path, text, count)], cx);
                }
            }
            None => self.replace_in_vault(swap, window, cx),
        }
    }

    /// Replace in the whole vault: counts first, and writes only once the count
    /// is accepted. Each rewritten note is kept as it was, to undo everything.
    pub(super) fn replace_in_vault(&mut self, swap: markdown::Swap, window: &mut Window, cx: &mut Context<Self>) {
        // The open note is saved first: the replacement starts from what is on screen.
        self.flush(cx);
        // The matches set aside in the panel's results of this same search stay as they are.
        let panel = self.vault_find.as_ref().filter(|find| find.asked.as_ref().is_some_and(|asked| (&asked.query, asked.case, asked.word, asked.regex) == (&swap.query, swap.case, swap.word, swap.regex)));
        let skip = |path: &Path| panel.map_or_else(Vec::new, |find| find.skipped(path));
        let found: Vec<(PathBuf, String, usize)> = (self.notes.iter())
            .filter_map(|note| markdown::swap_except(&note.body, &swap, &skip(&note.path)).map(|(text, count)| (note.path.clone(), text, count)))
            .collect();
        let count: usize = found.iter().map(|(.., count)| count).sum();
        if count == 0 {
            return self.say(Tone::Info, tr("Nothing to replace in the vault", "Rien à remplacer dans le coffre"), cx);
        }
        let title = format!("{} {count} · notes : {}", tr("Matches:", "Passages :"), found.len());
        let options = vec![tr("Replace them all", "Tout remplacer").to_string(), tr("Cancel", "Annuler").to_string()];
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(&title, options, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let agreed = matches!(event, PaletteEvent::Submit(_)) && palette.read(cx).chosen() == Some(0);
            this.palette = None;
            window.focus(&this.pane().editor.focus_handle(cx));
            if agreed {
                this.swap_notes(found.clone(), cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Writes the new text of each note, and keeps the old one.
    pub(super) fn swap_notes(&mut self, found: Vec<(PathBuf, String, usize)>, cx: &mut Context<Self>) {
        self.swapped.clear();
        let mut count = 0;
        for (path, text, hits) in found {
            let Some(note) = self.notes.iter_mut().find(|note| note.path == path) else {
                continue;
            };
            if let Err(e) = vault::write(&path, &text) {
                self.say(Tone::Failed, format!("{} {} : {}", tr("Not rewritten:", "Non réécrite :"), path.display(), crate::lang::io_reason(&e)), cx);
                continue;
            }
            count += hits;
            (note.tags, note.links) = Note::index(&text);
            self.swapped.push((path, note.set_body(Arc::from(text.as_str())), text));
        }
        self.after_swap(cx);
        let done = format!("{} {count} · notes : {} ({})", tr("Replaced:", "Remplacés :"), self.swapped.len(), tr("the palette can undo it", "la palette peut l'annuler"));
        self.say(Tone::Done, done, cx);
    }

    /// Undoes the last vault replacement: each note gets its previous text back,
    /// except those changed since, left as they are.
    pub(super) fn undo_swap(&mut self, cx: &mut Context<Self>) {
        self.flush(cx);
        let (mut back, mut kept) = (0, 0);
        for (path, old, new) in std::mem::take(&mut self.swapped) {
            let same = fs::read_to_string(&path).is_ok_and(|now| now == new);
            if same && vault::write(&path, &old).is_ok() {
                if let Some(note) = self.notes.iter_mut().find(|note| note.path == path) {
                    (note.tags, note.links) = Note::index(&old);
                    note.set_body(old);
                }
                back += 1;
            } else {
                kept += 1;
            }
        }
        self.after_swap(cx);
        let mut done = format!("{} {back}", tr("Notes restored:", "Notes rétablies :"));
        if kept > 0 {
            done += &format!(" · {} {kept}", tr("changed since, left as they are:", "modifiées depuis, laissées telles quelles :"));
        }
        self.say(if back + kept == 0 { Tone::Info } else { Tone::Done }, if back + kept == 0 { tr("No replacement to undo", "Aucun remplacement à annuler").to_string() } else { done }, cx);
    }

    /// After notes are rewritten: the open one is reread, the graph follows.
    pub(super) fn after_swap(&mut self, cx: &mut Context<Self>) {
        self.graph_stale = true;
        self.refresh_graph(cx);
        self.each_pane(|this| this.reload(cx));
        cx.notify();
    }
}

impl Shell {
    /// The vault search for a test: its search, the matches and notes listed, the rows, the selected row.
    #[cfg(test)]
    pub(super) fn vault_state(&self, cx: &App) -> Option<(String, usize, usize, usize, usize)> {
        let find = self.vault_find.as_ref()?;
        let (hits, notes) = find.counts();
        Some((find.query.read(cx).line.text.clone(), hits, notes, find.rows().len(), find.sel))
    }

    /// The two fields of the vault search, for a test.
    #[cfg(test)]
    pub(super) fn vault_fields(&self) -> Option<(Entity<Field>, Entity<Field>)> {
        self.vault_find.as_ref().map(|find| (find.query.clone(), find.with.clone()))
    }

    /// Case, whole words, regular expression: switched, searched again.
    fn vault_flip(&mut self, which: usize, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.vault_find {
            match which {
                0 => find.case ^= true,
                1 => find.word ^= true,
                _ => find.regex ^= true,
            }
        }
        self.vault_search(cx);
    }

    /// The replace line, shown or hidden; the keys go to the field that shows.
    fn vault_flip_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.vault_find {
            find.replace ^= true;
            let ready = find.replace && !find.query.read(cx).line.text.is_empty();
            window.focus(&if ready { &find.with } else { &find.query }.focus_handle(cx));
        }
        cx.notify();
    }

    /// Tab: from the search to the replacement and back, when it shows.
    fn vault_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = self.vault_find.as_ref().filter(|find| find.replace) {
            let to = if find.with.focus_handle(cx).is_focused(window) { &find.query } else { &find.with };
            window.focus(&to.focus_handle(cx));
        }
    }

    /// The panel in search mode: the search and its settings on top, the count, then the matches note by note.
    pub(super) fn render_vault_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.vault_find.is_none() {
            self.vault_find = Some(self.new_vault_find(window, cx));
        }
        // The notes changed since (saved, renamed, deleted elsewhere): searched again.
        self.vault_search(cx);
        let t = self.theme;
        let find = self.vault_find.as_ref().unwrap();
        let keys = |action: &dyn gpui::Action| keys::of(cx, action);
        let field = |id: &'static str, field: &Entity<Field>, hint: &'static str| {
            let (on, empty) = (field.focus_handle(cx).is_focused(window), field.read(cx).line.text.is_empty());
            ui::widgets::field_frame(id, field.clone(), on, empty, hint, t)
        };
        let flip = |which: usize| cx.listener(move |this, _: &gpui::ClickEvent, _, cx| this.vault_flip(which, cx));
        let swap = find.swap(cx);
        let (hits, notes) = find.counts();
        let said = if swap.query.is_empty() {
            tr("Search the text of every note", "Chercher dans le texte de toutes les notes").to_string()
        } else if markdown::broken(&swap.query, swap.word, swap.regex) {
            tr("Invalid expression", "Expression invalide").to_string()
        } else if find.busy.is_some() && find.groups.is_empty() {
            tr("Searching…", "Recherche…").to_string()
        } else if hits == 0 {
            tr("No results", "Aucun résultat").to_string()
        } else {
            let (matches, in_notes) = match (hits, notes) {
                (1, _) => (tr("match", "passage"), tr("note", "note")),
                (_, 1) => (tr("matches", "passages"), tr("note", "note")),
                _ => (tr("matches", "passages"), tr("notes", "notes")),
            };
            format!("{hits} {matches} · {notes} {in_notes}")
        };
        let chevron = ui::widgets::button("vault-chevron", if find.replace { "chevron-down.svg" } else { "chevron-right.svg" }, false, t)
            .debug_selector(|| "vault-chevron".into())
            .tooltip(ui::widgets::tip(tr("Replace", "Remplacer"), keys(&FindReplace), t))
            .on_click(cx.listener(|this, _, window, cx| this.vault_flip_replace(window, cx)));
        // The field has its own line, the panel's full width; the chevron and the settings are under it.
        let query = div().flex().child(field("vault-query", &find.query, tr("Search the vault", "Chercher dans le coffre")));
        let first = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(chevron)
            .child(ui::widgets::toggle("vault-case", "Aa", find.case, tr("Match case", "Respecter la casse"), keys(&FindCase), t).on_click(flip(0)))
            .child(ui::widgets::toggle("vault-word", "\u{201c}ab\u{201d}", find.word, tr("Whole words", "Mots entiers"), keys(&FindWord), t).on_click(flip(1)))
            .child(ui::widgets::toggle("vault-regex", ".*", find.regex, tr("Regular expression", "Expression régulière"), keys(&FindRegex), t).on_click(flip(2)));
        let second = find.replace.then(|| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .child(field("vault-with", &find.with, tr("Replace with", "Remplacer par")))
                .child(
                    ui::widgets::button("vault-replace-all", "replace-all.svg", false, t)
                        .debug_selector(|| "vault-replace-all".into())
                        .tooltip(ui::widgets::tip(tr("Replace in every note, after showing the count", "Remplacer dans toutes les notes, après en avoir montré le compte"), keys(&ReplaceAll), t))
                        .on_click(cx.listener(|this, _, window, cx| this.vault_replace(None, window, cx))),
                )
        });
        let head = div()
            .flex_none()
            .px_2()
            .pt_1()
            .pb_1p5()
            .flex()
            .flex_col()
            .gap_1p5()
            .text_size(t.px(13.))
            .text_color(t.text)
            .child(query)
            .child(first)
            .children(second)
            // One line, cut with an ellipsis: wrapped, the text is measured at several widths in a row and a
            // later unbounded measure reuses the narrowest result (one line per character), which made this
            // header taller than the panel in a 144 px panel and left the list no height.
            .child(div().debug_selector(|| "vault-count".into()).px_1().truncate().text_size(t.px(12.)).text_color(t.soft).child(said));
        let rows = find.rows().len();
        let list = uniform_list("vault-rows", rows, cx.processor(Self::render_vault_rows)).track_scroll(find.scroll.clone()).flex_1().min_h_0().px_1();
        div()
            .debug_selector(|| "vault-find".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            // The keys of the note's find bar: settings, Tab, replace all; the arrows walk the results.
            .key_context("Find")
            .on_action(cx.listener(|this, _: &Find, window, cx| this.open_vault_find(window, cx)))
            .on_action(cx.listener(|this, _: &FindReplace, window, cx| this.vault_flip_replace(window, cx)))
            .on_action(cx.listener(|this, _: &FindCase, _, cx| this.vault_flip(0, cx)))
            .on_action(cx.listener(|this, _: &FindWord, _, cx| this.vault_flip(1, cx)))
            .on_action(cx.listener(|this, _: &FindRegex, _, cx| this.vault_flip(2, cx)))
            .on_action(cx.listener(|this, _: &FindSwitch, window, cx| this.vault_switch(window, cx)))
            .on_action(cx.listener(|this, _: &palette::Prev, _, cx| this.vault_step(false, cx)))
            .on_action(cx.listener(|this, _: &palette::Next, _, cx| this.vault_step(true, cx)))
            .on_action(cx.listener(|this, _: &ReplaceAll, window, cx| this.vault_replace(None, window, cx)))
            .on_action(cx.listener(|this, _: &ReplaceInVault, window, cx| this.vault_replace(None, window, cx)))
            .child(head)
            .child(list)
            .into_any_element()
    }

    /// The rows in sight: a note's header (folds it; with the replace line, a button replaces in it), or a match:
    /// its line number, its line with the match marked, a button that sets it aside.
    fn render_vault_rows(&mut self, range: Range<usize>, _: &mut Window, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let t = self.theme;
        let Some(find) = &self.vault_find else { return Vec::new() };
        // ponytail: the rows are listed again for each frame, linear in the matches; kept if a search of a
        // common letter over thousands of notes ever makes scrolling slow.
        let rows = find.rows();
        // How many characters the line has room for, so that the match shows: what is ahead of it gives way.
        // ponytail: an average character width (6.5 px at 12.5 px); measured text if a script runs much wider.
        let room = ((f32::from(self.split_width()) - 90. * t.ui) / (6.5 * t.ui)).max(0.) as usize;
        let mut out = Vec::new();
        for i in range {
            let Some(&row) = rows.get(i) else { break };
            let chosen = i == find.sel;
            let element = match row {
                Row::Note(g) => {
                    let group = &find.groups[g];
                    let path = group.path.clone();
                    ui::widgets::group_header(("vault-note", i), group.name.clone(), find.kept(group).count(), find.folded.contains(&group.path), t)
                        .debug_selector(move || format!("vault-row-{i}"))
                        .when(chosen, |d| d.bg(t.code_bg))
                        .when(find.replace, |d| {
                            d.child(
                                ui::widgets::button(("vault-note-replace", i), "replace-all.svg", false, t)
                                    .debug_selector(move || format!("vault-note-replace-{i}"))
                                    .tooltip(ui::widgets::tip(tr("Replace in this note", "Remplacer dans cette note"), String::new(), t))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.vault_replace(Some(path.clone()), window, cx);
                                    })),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| this.vault_open(row, window, cx)))
                        .into_any_element()
                }
                Row::Hit(g, h) => {
                    let (group, hit) = (&find.groups[g], &find.groups[g].hits[h]);
                    let shown = group.body[hit.at.start..hit.at.end.min(hit.span.end)].chars().count();
                    let (text, at) = markdown::excerpt(&group.body, hit, room.saturating_sub(shown + 3).clamp(3, 40));
                    let marked = StyledText::new(text).with_highlights([(at, HighlightStyle { background_color: Some(t.selection), ..Default::default() })]);
                    div()
                        .id(("vault-hit", i))
                        .debug_selector(move || format!("vault-row-{i}"))
                        .group("vault-hit")
                        .w_full()
                        .overflow_hidden()
                        .h(t.px(nav::ROW))
                        .pl(t.px(22.))
                        .pr_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(t.small())
                        .cursor_pointer()
                        .text_size(t.px(12.5))
                        .hover(|s| s.bg(t.hover))
                        .when(chosen, |d| d.bg(t.code_bg))
                        .child(div().flex_none().min_w(t.px(22.)).font_family(mono()).text_size(t.px(11.)).text_color(t.soft).child(hit.line.to_string()))
                        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_color(t.text).child(marked))
                        .child(
                            ui::widgets::button(("vault-dismiss", i), "close.svg", false, t)
                                .debug_selector(move || format!("vault-dismiss-{i}"))
                                .when(!chosen, |d| d.opacity(0.).group_hover("vault-hit", |s| s.opacity(1.)))
                                .tooltip(ui::widgets::tip(tr("Set this match aside", "Écarter ce passage"), String::new(), t))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.vault_dismiss(g, h, cx);
                                })),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| this.vault_open(row, window, cx)))
                        .into_any_element()
                }
            };
            out.push(element);
        }
        out
    }
}
