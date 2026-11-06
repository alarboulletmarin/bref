//! Notes: opening, previewing, creating, saving, renaming with their title, links and wikilinks.

use crate::platform::config;
use super::*;

impl Shell {
    /// Copies the whole note to the clipboard.
    /// Copies the note; with `block`, only the code block the cursor is in,
    /// if there is one.
    pub(super) fn copy_all(&mut self, block: bool, cx: &mut Context<Self>) {
        if self.pane().file().is_some() || self.pane().tool().is_some() || self.pane().listing().is_some() || self.pane().figure().is_some() { return; }
        let editor = self.pane().editor.read(cx);
        let code = editor.code_at_cursor().filter(|_| block);
        let text = code.unwrap_or(editor.text()).to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Loads the note into the editor; false if the file is unreadable.
    pub(super) fn load_note(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        // Already in the other pane: two editors on the same file would overwrite each other.
        // Input moves there; a mere preview does not move it.
        if let Some(other) = self.pane_showing(path).filter(|other| *other != self.active) {
            if !self.pane().peek {
                self.active = other;
                if self.pane().path.as_deref() == Some(path) {
                    self.set_file(None, cx);
                    self.pane_mut().take_listing();
                }
                self.grab = true;
                cx.notify();
            }
            return false;
        }
        self.pane_mut().take_listing();
        // An attachment (PDF, video, archive…) opens in the system's application; the note stays.
        if vault::is_attachment(path) {
            // The test platform opens nothing: it records what it would have been asked.
            #[cfg(test)]
            self.opened.push(path.to_path_buf());
            #[cfg(not(test))]
            cx.open_with_system(path);
            return false;
        }
        if vault::is_table(path) {
            return self.open_table(path, cx);
        }
        // A picture takes the note's place; the note stays loaded underneath.
        if vault::is_image(path) {
            let svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
            let open = self.pane().file().filter(|file| file.path == path).map(|file| file.kind.clone());
            let theme = self.theme;
            let kind = match open {
                _ if !svg => FileKind::Image,
                // A diagram opens in its canvas, ready to be edited again.
                Some(FileKind::Drawing(canvas)) => FileKind::Drawing(canvas),
                open => {
                    let text = fs::read_to_string(path).ok();
                    match text.as_deref().and_then(Diagram::from_svg) {
                        Some(diagram) => {
                            let canvas = cx.new(|cx| Canvas::new(diagram, theme, cx));
                            cx.subscribe(&canvas, Self::on_canvas_event).detach();
                            FileKind::Drawing(canvas)
                        }
                        // An SVG from elsewhere opens in the viewer: zoom in, move around. So does a
                        // diagram of a newer Bref, which this one could only damage by saving it.
                        None => match open {
                            Some(FileKind::Svg(viewer)) => FileKind::Svg(viewer),
                            _ => {
                                if text.as_deref().is_some_and(Diagram::newer) {
                                    let notice = tr("Made by a newer Bref: shown read-only", "Fait par un Bref plus récent : en lecture seule");
                                    self.say(Tone::Info, notice, cx);
                                }
                                let source = viewer::Source::File(path.to_path_buf(), None);
                                FileKind::Svg(cx.new(|cx| viewer::Viewer::new(source, theme, cx)))
                            }
                        },
                    }
                }
            };
            self.set_file(Some(FileView { path: path.to_path_buf(), kind }), cx);
            cx.notify();
            return false;
        }
        if !self.leave(cx) {
            return false;
        }
        self.set_file(None, cx);
        cx.notify();
        match fs::read_to_string(path) {
            Ok(text) => {
                self.pane_mut().synced = vault::stem(path) == vault::title_of(&text);
                self.pane_mut().fresh = false;
                self.pane_mut().h1 = vault::h1_of(&text);
                self.pane_mut().origin = Some(vault::stem(path));
                // A reloaded note keeps its view; another one opens as a board if it is one.
                let same = self.pane_mut().path.as_deref() == Some(path);
                self.pane_mut().board.reloaded(same, kanban::is_board(&text));
                self.pane_mut().path = Some(path.to_path_buf());
                self.pane_mut().base = text.clone();
                self.calm();
                let cursor = markdown::body_start(&text);
                self.pane().editor.update(cx, |e, cx| e.load(text, cursor, cx));
                self.push_dirs(cx);
                self.nav.reveal(path);
                true
            }
            Err(e) => {
                let what = tr("Cannot open", "Impossible d'ouvrir");
                self.say(Tone::Failed, format!("{what} {} : {}", path.display(), crate::lang::io_reason(&e)), cx);
                false
            }
        }
    }

    pub(super) fn open_note(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.pane_mut().peek = false;
        if self.load_note(path, cx) {
            self.pane_mut().preview = false;
            self.touch(path);
        }
    }

    /// Shows the note without counting it as opened.
    pub(super) fn preview_note(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        // Already open: its tab is shown, input stays where it is, and the history does not count it.
        if let Some(tab) = self.pane_showing(path) {
            self.active = tab;
            self.show_active(cx);
            self.pane_mut().peek = true;
            return cx.notify();
        }
        // One preview tab per pane: each note browsed takes the place of the last one.
        let group = self.group();
        match self.groups[group].tabs.iter().copied().find(|&tab| self.panes[tab].preview) {
            Some(tab) => {
                self.active = tab;
                self.show_active(cx);
            }
            None => self.fresh_tab(window, cx),
        }
        self.pane_mut().peek = true;
        // The note already loaded, hidden by a picture: removing the picture is enough.
        if self.pane().path.as_deref() == Some(path) {
            if self.pane().file().is_some() {
                self.set_file(None, cx);
                cx.notify();
            }
        } else if self.load_note(path, cx) {
            self.pane_mut().preview = true;
        }
    }

    /// The preview becomes an opened note.
    pub(super) fn keep_preview(&mut self) {
        self.pane_mut().peek = false;
        if std::mem::take(&mut self.pane_mut().preview)
            && let Some(path) = self.pane().path.clone()
        {
            self.touch(&path);
        }
    }

    /// New note, filed in the selected folder when the tree is shown.
    pub(super) fn new_note_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_note_in(self.nav.target_dir(), window, cx);
    }

    /// New note filed in `dir`; `None`: at the vault root.
    pub(super) fn new_note_in(&mut self, dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.fresh_tab(window, cx);
        self.new_note(String::new(), cx);
        self.pane_mut().new_dir = dir;
        self.nav.give_note_room();
        window.focus(&self.pane().editor.focus_handle(cx));
        self.settle_nav(window, cx);
    }

    pub(super) fn new_note(&mut self, text: String, cx: &mut Context<Self>) {
        self.pane_mut().peek = false;
        if !self.leave(cx) {
            return;
        }
        self.pane_mut().take_listing();
        self.set_file(None, cx);
        self.pane_mut().path = None;
        self.pane_mut().origin = None;
        self.pane_mut().synced = true;
        self.pane_mut().preview = false;
        self.pane_mut().new_dir = None;
        self.pane_mut().fresh = true;
        self.pane_mut().dirty = !text.is_empty();
        self.pane_mut().board.reloaded(false, kanban::is_board(&text));
        let cursor = text.len();
        self.pane().editor.update(cx, |e, cx| e.load(text, cursor, cx));
        self.push_dirs(cx);
        self.flush(cx);
        cx.notify();
    }

    /// Before leaving the shown note: it is saved and, if its title
    /// renamed it, links to its old name follow. Waiting for this moment
    /// avoids rewriting the links at every intermediate title while typing.
    /// False means the current view must stay alive, with its dirty state and undo history.
    pub(super) fn leave(&mut self, cx: &mut Context<Self>) -> bool {
        self.mark_place(cx);
        if !self.flush_one(cx) {
            return false;
        }
        self.flush(cx);
        self.follow_title(cx);
        self.drop_if_empty(cx);
        true
    }

    /// The links to the note follow the name its title gave it since it was opened.
    pub(super) fn follow_title(&mut self, cx: &mut Context<Self>) {
        if let (Some(old), Some(path)) = (self.pane_mut().origin.take(), &self.pane_mut().path) {
            let new = vault::stem(path);
            self.relink(&old, &new, cx);
            // Still open in its tab: a later title follows from this name.
            self.pane_mut().origin = Some(new);
        }
    }

    /// A note created here then emptied leaves no empty file behind, nor in the
    /// recent list. Only an empty file on disk is removed: nothing written is lost.
    pub(super) fn drop_if_empty(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.pane_mut().fresh) || !self.pane_mut().editor.read(cx).text().trim().is_empty() {
            return;
        }
        let Some(path) = self.pane().path.clone() else {
            return;
        };
        if fs::read_to_string(&path).is_ok_and(|text| text.trim().is_empty()) && fs::remove_file(&path).is_ok() {
            self.forget_notes(|p| *p == path);
            self.recent.retain(|p| *p != path);
            if let Some(root) = &self.vault {
                config::save_config(root, &self.recent);
            }
            self.history.remove(&path);
            self.pane_mut().path = None;
            self.index_changed(cx);
        }
    }

    /// `[[links]]` to `old` now point to `new` in every note;
    /// true if the shown note was rewritten. No effect if another note
    /// is still called `old`: the links may be meant for it.
    // ponytail: synchronous rewrite, limited to the notes the index says link to
    // `old`; move to a background task if a note is linked from hundreds of others.
    pub(super) fn relink(&mut self, old: &str, new: &str, cx: &mut Context<Self>) -> bool {
        let changed = vault::relink(self.vault.as_deref(), &mut self.notes, old, new);
        if changed.is_empty() {
            return false;
        }
        // The links moved, the names did not.
        self.graph_changed(cx);
        // The other pane's note is reloaded here; the active pane's is up to the caller
        // (it may be leaving it).
        let showing = |pane: &Pane| pane.path.as_ref().is_some_and(|path| changed.contains(path));
        for pane in 0..self.panes.len() {
            if pane != self.active && showing(&self.panes[pane]) {
                self.in_pane(pane, |this| this.reload(cx));
            }
        }
        showing(&self.panes[self.active])
    }

    /// A heading of the note was renamed (the same headings but one, which reads differently): the links
    /// and embeds of the other notes that point at it, `[[Note#old]]` and `![[Note#old]]`, now point at the new one.
    // ponytail: synchronous rewrite, limited to the notes the index says link to this one, like `relink`;
    // typing a heading rewrites them at each save, which is as often as the note is written.
    fn follow_heading(&mut self, path: &Path, before: &str, after: &str, cx: &mut Context<Self>) {
        let Some((was, now)) = embed::renamed_heading(before, after) else { return };
        let rewritten = vault::follow_heading(self.vault.as_deref(), &mut self.notes, path, &was, &now);
        for pane in 0..self.panes.len() {
            if pane != self.active && self.panes[pane].path.as_ref().is_some_and(|p| rewritten.contains(p)) {
                self.in_pane(pane, |this| this.reload(cx));
            }
        }
        // A note shown here that embeds the renamed section follows too.
        for pane in &self.panes {
            pane.editor.update(cx, |e, cx| e.refresh_embeds(cx));
        }
    }

    /// Reloads the shown note from disk, rewritten outside the editor.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.pane().path.clone() {
            // Reloading is not opening: a preview stays a preview, the list page stays shown.
            // Nor does it move the reader: an agent writing at the end leaves cursor and scroll
            // where they were.
            let (preview, listing) = (self.pane_mut().preview, self.pane_mut().take_listing());
            let (at, scroll) = self.pane().editor.read(cx).view();
            if self.load_note(&path, cx) {
                self.pane().editor.update(cx, |e, cx| e.set_view(at, scroll, cx));
            }
            self.pane_mut().preview = preview;
            if let Some(listing) = listing {
                self.pane_mut().set_cover(Cover::Listing(listing));
            }
        }
    }

    /// Brings the disk up to date: each pane writes what is waiting in it.
    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        self.each_pane(|this| { this.flush_one(cx); });
        // What was just written may be shown in an embed, in this window.
        for pane in &self.panes {
            pane.editor.update(cx, |e, cx| e.refresh_embeds(cx));
        }
    }

    /// Another program (an agent, a sync tool) wrote the note while it was typed in: its changes
    /// join the typed text as one undo step. When both changed the same lines, its version is
    /// written next to the note, as a sync conflict is. False if that copy could not be written:
    /// the note is then not saved, so that nothing is lost.
    fn take_outside(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        let Ok(disk) = fs::read_to_string(path) else { return true };
        if disk == self.pane().base {
            return true;
        }
        let ours = self.pane().editor.read(cx).text().to_string();
        if disk != ours {
            match logic::merge::onto(&self.pane().base, &ours, &disk) {
                Some(edits) => self.pane().editor.update(cx, |e, cx| e.absorb(&edits, cx)),
                None => {
                    let date = date_name(today());
                    let copy = git::free_conflict_path(path, &date, "");
                    if let Err(e) = vault::write(&copy, &disk) {
                        let what = tr("Changed here and elsewhere, and the other version could not be kept:", "Modifiée ici et ailleurs, et l'autre version n'a pas pu être gardée :");
                        self.say(Tone::Failed, format!("{what} « {} » : {}", path.display(), crate::lang::io_reason(&e)), cx);
                        return false;
                    }
                    let what = tr("Changed here and elsewhere on the same lines: the other version is in", "Modifiée ici et ailleurs sur les mêmes lignes : l'autre version est dans");
                    self.say(Tone::Info, format!("{what} {}", vault::stem(&copy)), cx);
                }
            }
        }
        self.pane_mut().base = disk;
        true
    }

    /// Writes the pane's note to disk if it changed.
    // ponytail: synchronous write on the UI thread (a few KB, < 1 ms);
    // move to a background task if notes get very large.
    pub(super) fn flush_one(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.flush_sheet(cx) || !self.flush_canvas(cx) {
            return false;
        }
        let Some(root) = self.vault.clone() else {
            return !self.pane().dirty;
        };
        if !self.pane().dirty {
            return true;
        }
        if let Some(path) = self.pane().path.clone()
            && !self.take_outside(&path, cx)
        {
            return false;
        }
        let content = self.pane().editor.read(cx).text().to_string();
        if self.pane().path.is_none() && content.trim().is_empty() {
            self.pane_mut().dirty = false;
            return true;
        }
        // A `#` title added or changed at the top: the file now bears its name.
        if vault::h1_of(&content).is_some_and(|h1| self.pane().h1.as_ref() != Some(&h1)) {
            self.pane_mut().synced = true;
        }
        let dir = self.pane().new_dir.as_deref().unwrap_or(&root);
        match vault::save(dir, self.pane().path.as_deref(), self.pane().synced, &content) {
            Ok(path) => {
                self.pane_mut().dirty = false;
                self.pane_mut().base = content.clone();
                self.calm();
                let old = self.pane().path.clone();
                let fresh = Note::from_text(vault::stem(&path), path.clone(), SystemTime::now(), Arc::from(content), vault::in_templates(&root, &path));
                // The text as it was last saved, to see which heading was renamed. Only a rename
                // pushes the names, and only a new link or alias refreshes the graph: a save per
                // keystroke pause must stay cheap, so this does not go through `index_changed`.
                let before = self.put_note(old.as_ref(), fresh);
                if old.as_ref() != Some(&path) {
                    // Renamed by its title, the note keeps its icon.
                    if let Some(icon) = old.as_ref().and_then(|old| self.icons.remove(old)) {
                        self.icons.insert(path.clone(), icon);
                        self.save_icons(cx);
                    }
                    if let Some(old) = &old {
                        self.history.rename(old, &path);
                    }
                    self.recent.retain(|p| Some(p) != old.as_ref());
                    self.touch(&path);
                    self.nav.reveal(&path);
                    self.pane_mut().path = Some(path.clone());
                    self.push_names(cx);
                    self.push_dirs(cx);
                }
                if let Some(before) = before {
                    self.follow_heading(&path, &before, &self.notes[0].body.clone(), cx);
                }
                self.refresh_graph(cx);
            }
            Err(e) => {
                let name = self.pane().path.as_deref().map_or_else(|| vault::title_of(&content), vault::stem);
                self.say(Tone::Failed, format!("{} « {name} » : {}", tr("Note not saved", "Note non enregistrée"), crate::lang::io_reason(&e)), cx)
            }
        }
        cx.notify();
        !self.pane().dirty
    }

    pub(super) fn on_editor_event(
        &mut self,
        editor: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.panes.iter().position(|pane| pane.editor == *editor) else { return };
        // A gesture made in a note (followed link, menu, replacement) applies to its pane;
        // typing, though, may come from elsewhere (a board card, a list cell).
        if !matches!(event, EditorEvent::Changed) {
            self.active = pane;
        }
        match event {
            EditorEvent::Changed => {
                self.in_pane(pane, |this| {
                    // The board key was just written into the text: stay there, the
                    // corner button shows the board when wanted.
                    let board = kanban::is_board(this.pane().editor.read(cx).text());
                    this.pane_mut().board.typed(board);
                    this.keep_preview();
                    this.pane_mut().dirty = true;
                });
                self.save_gen += 1;
                let generation = self.save_gen;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(400)).await;
                    this.update(cx, |this, cx| {
                        if this.save_gen == generation {
                            this.flush(cx);
                        }
                    })
                    .ok();
                })
                .detach();
            }
            EditorEvent::Open(link) => self.follow(link, window, cx),
            EditorEvent::OpenAside(link) => match link {
                Link::Wiki(name) => match self.wiki_path(name) {
                    Some(path) => self.open_aside(&path, window, cx),
                    None => self.follow(link, window, cx),
                },
                _ => self.follow(link, window, cx),
            },
            EditorEvent::OpenAt(target, aside) => self.open_at(target, *aside, window, cx),
            EditorEvent::Swap(swap) => self.replace_in_vault(swap.clone(), window, cx),
            EditorEvent::Menu(at, link, commented, keys) => {
                let chosen = keys.then_some(0);
                self.show_menu(nav::Menu { at: *at, text: Some(link.clone()), commented: *commented, chosen, ..Default::default() }, window, cx);
            }
            EditorEvent::BlockMenu(at) => {
                self.show_menu(nav::Menu { at: *at, text: Some(None), block: true, ..Default::default() }, window, cx);
            }
            // The diagram, formula or picture takes the note's place; Escape returns to it.
            EditorEvent::View(source) => self.show_figure(source.clone(), window, cx),
        }
    }

    /// Follows a link in the note: the address in the browser, the tag in the palette, the note.
    pub(super) fn follow(&mut self, link: &Link, window: &mut Window, cx: &mut Context<Self>) {
        match link {
            Link::Url(url) => cx.open_url(url),
            Link::Tag(tag) => self.open_palette(&format!("#{tag}"), window, cx),
            // A folder reads as a list page, when no note bears its name.
            Link::Wiki(name) if self.wiki_path(name).is_none() && self.wiki_file(name).is_none() => match self.wiki_dir(name) {
                Some(dir) => self.open_listing(dir, window, cx),
                None => self.open_wiki(name, window, cx),
            },
            Link::Wiki(name) => self.open_wiki(name, window, cx),
        }
    }

    /// Opens the note of an embed or of a link to a heading, scrolled to what it points at: the heading,
    /// the block (a note comes back from there with Alt+Left, like after any link).
    pub(super) fn open_at(&mut self, target: &embed::Target, aside: bool, window: &mut Window, cx: &mut Context<Self>) {
        match (aside, self.wiki_path(&target.note)) {
            (true, Some(path)) => self.open_aside(&path, window, cx),
            _ => self.open_wiki(&target.note, window, cx),
        }
        let anchor = target.anchor.clone();
        let at = self.pane().editor.read(cx).text();
        let at = embed::position(at, &anchor).filter(|_| anchor != embed::Anchor::Whole);
        if let Some(at) = at {
            self.pane().editor.update(cx, |e, cx| e.jump(at, true, cx));
        }
    }

    /// The vault file (picture, table, attachment) that `[[name.ext]]` points to, case-insensitive.
    pub(super) fn wiki_file(&self, name: &str) -> Option<PathBuf> {
        let wanted = name.to_lowercase();
        self.images.iter().find(|path| path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == wanted)).cloned()
    }

    /// The vault folder that `[[Name]]` points to, except the templates folder.
    pub(super) fn wiki_dir(&self, name: &str) -> Option<PathBuf> {
        let (root, wanted) = (self.vault.as_ref()?, name.to_lowercase());
        self.dirs.iter().find(|dir| !vault::in_templates(root, &dir.join("x")) && dir.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == wanted)).cloned()
    }

    /// Daily note: the note named after the local date, wherever it is in the
    /// vault; created with that date as title if it does not exist yet.
    pub(super) fn open_today(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let name = date_name(today());
        // No daily note yet, and a `Daily` or `Journal` template: it is born from it.
        if self.wiki_path(&name).is_none()
            && let Some(template) = self.daily_template()
            && self.create_from(&template, &name, &root, window, cx).is_some()
        {
            return;
        }
        self.open_wiki(&name, window, cx);
    }

    /// The note a `[[link]]` points to: by its name first, otherwise by an alias in its
    /// front matter.
    pub(super) fn wiki_path(&self, name: &str) -> Option<PathBuf> {
        // A template answers no link: `[[Réunion]]` points to the note, not its template.
        logic::wiki::resolve(&self.notes, name, |n| self.is_template(n)).map(|note| note.path.clone())
    }

    pub(super) fn open_wiki(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.wiki_path(name).or_else(|| self.wiki_file(name)) {
            Some(path) => self.open(&path, window, cx),
            None => {
                self.fresh_tab(window, cx);
                self.new_note(vault::new_note_text(name), cx);
                window.focus(&self.pane().editor.focus_handle(cx));
            }
        }
    }
}
