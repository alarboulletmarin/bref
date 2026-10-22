//! Notes: opening, previewing, creating, saving, renaming with their title, links and wikilinks.

use super::*;

impl Shell {
    /// Copies the whole note to the clipboard.
    /// Copies the note; with `block`, only the code block the cursor is in,
    /// if there is one.
    pub(super) fn copy_all(&mut self, block: bool, cx: &mut Context<Self>) {
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
                    self.pane_mut().listing = None;
                }
                self.grab = true;
                cx.notify();
            }
            return false;
        }
        self.pane_mut().listing = None;
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
            let open = self.pane().file.as_ref().filter(|file| file.path == path).map(|file| file.kind.clone());
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
        self.set_file(None, cx);
        self.leave(cx);
        cx.notify();
        match fs::read_to_string(path) {
            Ok(text) => {
                self.pane_mut().synced = vault::stem(path) == vault::title_of(&text);
                self.pane_mut().fresh = false;
                self.pane_mut().h1 = vault::h1_of(&text);
                self.pane_mut().origin = Some(vault::stem(path));
                // A reloaded note keeps its view; another one opens as a board if it is one.
                self.pane_mut().board.text &= self.pane_mut().path.as_deref() == Some(path);
                self.pane_mut().board.was = kanban::is_board(&text);
                self.pane_mut().path = Some(path.to_path_buf());
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
    pub(super) fn preview_note(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.pane_mut().peek = true;
        // The note already loaded, hidden by a picture: removing the picture is enough.
        if self.pane().path.as_deref() == Some(path) {
            if self.pane().file.is_some() {
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
        self.new_note(String::new(), cx);
        self.pane_mut().new_dir = dir;
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        window.focus(&self.pane().editor.focus_handle(cx));
        self.settle_nav(window, cx);
    }

    pub(super) fn new_note(&mut self, text: String, cx: &mut Context<Self>) {
        self.pane_mut().peek = false;
        self.leave(cx);
        self.pane_mut().listing = None;
        self.set_file(None, cx);
        self.pane_mut().path = None;
        self.pane_mut().origin = None;
        self.pane_mut().synced = true;
        self.pane_mut().preview = false;
        self.pane_mut().new_dir = None;
        self.pane_mut().fresh = true;
        self.pane_mut().dirty = !text.is_empty();
        (self.pane_mut().board.text, self.pane_mut().board.was) = (false, kanban::is_board(&text));
        let cursor = text.len();
        self.pane().editor.update(cx, |e, cx| e.load(text, cursor, cx));
        self.push_dirs(cx);
        self.flush(cx);
        cx.notify();
    }

    /// Before leaving the shown note: it is saved and, if its title
    /// renamed it, links to its old name follow. Waiting for this moment
    /// avoids rewriting the links at every intermediate title while typing.
    pub(super) fn leave(&mut self, cx: &mut Context<Self>) {
        self.mark_place(cx);
        self.flush(cx);
        if let (Some(old), Some(path)) = (self.pane_mut().origin.take(), &self.pane_mut().path) {
            let new = vault::stem(path);
            self.relink(&old, &new, cx);
        }
        self.drop_if_empty(cx);
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
            self.notes.retain(|n| n.path != path);
            self.recent.retain(|p| *p != path);
            if let Some(root) = &self.vault {
                vault::save_config(root, &self.recent);
            }
            self.history.remove(&path);
            self.pane_mut().path = None;
            self.graph_stale = true;
            self.push_names(cx);
            self.refresh_graph(cx);
        }
    }

    /// `[[links]]` to `old` now point to `new` in every note;
    /// true if the shown note was rewritten. No effect if another note
    /// is still called `old`: the links may be meant for it.
    // ponytail: synchronous rewrite, limited to the notes the index says link to
    // `old`; move to a background task if a note is linked from hundreds of others.
    pub(super) fn relink(&mut self, old: &str, new: &str, cx: &mut Context<Self>) -> bool {
        let key = old.to_lowercase();
        if key == new.to_lowercase() || self.notes.iter().any(|n| n.name.to_lowercase() == key) {
            return false;
        }
        let mut changed = Vec::new();
        for note in self.notes.iter_mut().filter(|n| n.links.contains(&key)) {
            let rewritten = fs::read_to_string(&note.path)
                .ok()
                .and_then(|text| markdown::relink(&text, old, new))
                .filter(|text| vault::write(&note.path, text).is_ok());
            if let Some(text) = rewritten {
                (note.tags, note.links) = Note::index(&text);
                changed.push(note.path.clone());
                self.graph_stale = true;
            }
        }
        self.refresh_graph(cx);
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
        let (old, new): (Vec<_>, Vec<_>) = (markdown::headings(before), markdown::headings(after));
        let changed: Vec<_> = old.iter().zip(&new).filter(|(a, b)| a.0 != b.0 || a.1 != b.1).collect();
        let [(was, now)] = changed.as_slice() else { return };
        if old.len() != new.len() || was.0 != now.0 || now.1.trim().is_empty() || was.1.trim().is_empty() {
            return;
        }
        let (name, key) = (vault::stem(path), vault::stem(path).to_lowercase());
        let mut rewritten = Vec::new();
        for note in self.notes.iter_mut().filter(|n| n.path != path && n.links.contains(&key)) {
            let fresh = fs::read_to_string(&note.path)
                .ok()
                .map(|text| embed::rename_heading(&text, &name, was.1.trim(), now.1.trim()))
                .filter(|(_, count)| *count > 0)
                .filter(|(text, _)| vault::write(&note.path, text).is_ok());
            if let Some((text, _)) = fresh {
                (note.tags, note.links) = Note::index(&text);
                rewritten.push(note.path.clone());
            }
        }
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
            let (preview, listing) = (self.pane_mut().preview, self.pane_mut().listing.take());
            self.load_note(&path, cx);
            (self.pane_mut().preview, self.pane_mut().listing) = (preview, listing);
        }
    }

    /// Brings the disk up to date: each pane writes what is waiting in it.
    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        self.each_pane(|this| this.flush_one(cx));
        // What was just written may be shown in an embed, in this window.
        for pane in &self.panes {
            pane.editor.update(cx, |e, cx| e.refresh_embeds(cx));
        }
    }

    /// Writes the pane's note to disk if it changed.
    // ponytail: synchronous write on the UI thread (a few KB, < 1 ms);
    // move to a background task if notes get very large.
    pub(super) fn flush_one(&mut self, cx: &mut Context<Self>) {
        self.flush_sheet(cx);
        let Some(root) = self.vault.clone() else {
            return;
        };
        if !self.pane().dirty {
            return;
        }
        let content = self.pane().editor.read(cx).text().to_string();
        if self.pane().path.is_none() && content.trim().is_empty() {
            self.pane_mut().dirty = false;
            return;
        }
        // A `#` title added or changed at the top: the file now bears its name.
        if vault::h1_of(&content).is_some_and(|h1| self.pane().h1.as_ref() != Some(&h1)) {
            self.pane_mut().synced = true;
        }
        let dir = self.pane().new_dir.as_deref().unwrap_or(&root);
        match vault::save(dir, self.pane().path.as_deref(), self.pane().synced, &content) {
            Ok(path) => {
                self.pane_mut().dirty = false;
                self.calm();
                let (tags, links) = markdown::index(&content);
                // A template has neither tags nor links.
                let (tags, links) = if vault::in_templates(&root, &path) { (Vec::new(), Vec::new()) } else { (tags, links) };
                let old = self.pane().path.clone();
                // The graph only changes if the note is new, renamed or linked differently.
                let known = self.notes.iter().find(|n| Some(&n.path) == old.as_ref());
                if known.is_none_or(|n| n.path != path || n.links[..] != links[..]) {
                    self.graph_stale = true;
                }
                // The text as it was last saved, to see which heading was renamed.
                let before = self.notes.iter().find(|n| Some(&n.path) == old.as_ref()).map(|n| n.body.clone());
                self.notes.retain(|n| Some(&n.path) != old.as_ref() && n.path != path);
                let body: Arc<str> = Arc::from(content);
                self.notes.insert(
                    0,
                    Note {
                        name: vault::stem(&path),
                        tags: tags.into(),
                        links: links.into(),
                        aliases: markdown::aliases(&body).into(),
                        mtime: SystemTime::now(),
                        path: path.clone(),
                        lower: logic::note::lowered(&body),
                        body,
                    },
                );
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
                    this.pane_mut().board.text |= board && !this.pane_mut().board.was;
                    this.pane_mut().board.was = board;
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
            EditorEvent::Menu(at, link, commented) => {
                self.menu = Some(nav::Menu { at: *at, target: None, text: Some(link.clone()), commented: *commented, block: false });
                cx.notify();
            }
            EditorEvent::BlockMenu(at) => {
                self.menu = Some(nav::Menu { at: *at, target: None, text: Some(None), commented: false, block: true });
                cx.notify();
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
                None => self.open_wiki(name, cx),
            },
            Link::Wiki(name) => self.open_wiki(name, cx),
        }
    }

    /// Opens the note of an embed or of a link to a heading, scrolled to what it points at: the heading,
    /// the block (a note comes back from there with Alt+Left, like after any link).
    pub(super) fn open_at(&mut self, target: &embed::Target, aside: bool, window: &mut Window, cx: &mut Context<Self>) {
        match (aside, self.wiki_path(&target.note)) {
            (true, Some(path)) => self.open_aside(&path, window, cx),
            _ => self.open_wiki(&target.note, cx),
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
    pub(super) fn open_today(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let name = date_name(today());
        // No daily note yet, and a `Daily` or `Journal` template: it is born from it.
        if self.wiki_path(&name).is_none()
            && let Some(template) = self.daily_template()
            && self.create_from(&template, &name, &root, cx).is_some()
        {
            return;
        }
        self.open_wiki(&name, cx);
    }

    /// The note a `[[link]]` points to: by its name first, otherwise by an alias in its
    /// front matter.
    pub(super) fn wiki_path(&self, name: &str) -> Option<PathBuf> {
        let wanted = name.to_lowercase();
        // A template answers no link: `[[Réunion]]` points to the note, not its template.
        let mut notes = self.notes.iter().filter(|n| !self.is_template(n));
        let named = notes.clone().find(|n| n.name.to_lowercase() == wanted);
        named.or_else(|| notes.find(|n| n.answers(&wanted))).map(|note| note.path.clone())
    }

    pub(super) fn open_wiki(&mut self, name: &str, cx: &mut Context<Self>) {
        match self.wiki_path(name).or_else(|| self.wiki_file(name)) {
            Some(path) => self.open_note(&path, cx),
            None => self.new_note(format!("# {name}\n\n"), cx),
        }
    }
}
