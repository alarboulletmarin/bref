//! The vault: choosing it, scanning and watching it, icons, backup, trash, terminal.

use crate::platform::{config, signal, terminal, backup};
use super::*;

impl Shell {
    /// Rereads the vault right away, without waiting for the watcher: what the merge wrote
    /// shows, each note keeping its cursor and scroll (`reload`).
    pub(super) fn rescan(&mut self, root: &Path, cx: &mut Context<Self>) {
        let (notes, dirs, images, unreadable) = vault::rescan_full(root, &self.notes);
        self.unreadable = unreadable;
        self.sync(notes, dirs, images, cx);
    }

    /// Opens the window's door to `bref mcp` (`platform::link`): an agent asks it to show a file of
    /// the vault. No door (no port to be had): the agent still writes, it just cannot show.
    pub(super) fn listen(&mut self, window: &Window, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let signal = Arc::new(signal::Signal::default());
        if let Err(e) = platform::link::open(asked.clone(), signal.clone()) {
            eprintln!("bref: agent door closed: {e}");
            let why = crate::lang::io_reason(&e);
            self.say(Tone::Failed, format!("{} {why}", tr("An agent cannot make Bref show a note:", "Un agent ne peut pas faire afficher une note :")), cx);
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            loop {
                signal.wait(cx.background_executor().timer(Duration::from_secs(3600))).await;
                let each: Vec<platform::link::Asked> = std::mem::take(&mut *asked.lock().unwrap_or_else(|e| e.into_inner()));
                for ask in each {
                    let Ok(answer) = this.update_in(cx, |this, window, cx| this.show_asked(&ask.path, window, cx)) else {
                        return;
                    };
                    let _ = ask.answer.send(answer);
                }
            }
        })
        .detach();
    }

    /// Shows a file an agent asked for (`path` absolute), in a tab of its own, and brings the
    /// window to the front.
    pub(super) fn show_asked(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(root) = self.vault.clone() else {
            return Err("Bref has no vault open".into());
        };
        // The agent and the window may write the vault's path differently (a link, `/private` on macOS).
        let (Ok(canon_root), Ok(canon)) = (root.canonicalize(), path.canonicalize()) else {
            return Err(format!("no file at {}", path.display()));
        };
        let Ok(inside) = canon.strip_prefix(&canon_root) else {
            return Err(format!("Bref has another vault open: {}", root.display()));
        };
        self.open(&root.join(inside), window, cx);
        window.activate_window();
        Ok(())
    }

    /// Follows what other programs change in the vault: notes added,
    /// renamed, deleted, or modified while they are shown.
    ///
    /// The system reports changes (inotify, FSEvents…): the reread follows
    /// within a fraction of a second. A check reread still happens every
    /// 30 s, in case an event gets lost. Without a possible watcher (inotify
    /// limit reached, network folder), the vault's fingerprint is recomputed every
    /// 2 s.
    // ponytail: the fingerprint (names and dates, without reading the files) is computed off
    // the UI thread and never takes more than 0.5 % of a core: a big vault is
    // thus looked at less often (5,000 notes: every 5 s, or 100 s for the check).
    // Tests install no watcher: their clock is simulated.
    pub(super) fn watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let changed = Arc::new(signal::Signal::default());
            let mut watched: Option<(PathBuf, Option<notify::RecommendedWatcher>)> = None;
            let mut last = None;
            let mut pause = Duration::from_secs(2);
            // Next watcher check.
            let mut next_check = Instant::now();
            loop {
                let live = watched.as_ref().is_some_and(|(_, w)| w.is_some());
                // With a watcher, the task sleeps until the system's signal; it still wakes up
                // every 2 s to see whether the vault changed.
                let notified = if live {
                    changed.wait(cx.background_executor().timer(Duration::from_secs(2))).await
                } else {
                    cx.background_executor().timer(pause).await;
                    false
                };
                // A vault still being read for the first time is left alone: on a slow disk that read
                // outlasts the pause, and a second one beside it would cost its time and its memory twice.
                let Ok(state) = this.update(cx, |this, _| this.vault.clone().filter(|_| !this.scanning).map(|root| (root, this.save_gen)))
                else {
                    return;
                };
                let Some((root, generation)) = state else {
                    continue;
                };
                if watched.as_ref().map(|(r, _)| r) != Some(&root) {
                    let watcher = if cfg!(test) { None } else { vault::watch(&root, changed.clone()) };
                    watched = Some((root.clone(), watcher));
                    next_check = Instant::now();
                }
                let live = watched.as_ref().is_some_and(|(_, w)| w.is_some());
                if live {
                    if !notified && Instant::now() < next_check {
                        continue;
                    }
                    if notified {
                        // A save or a sync often touches several
                        // files in a row: let the burst finish.
                        cx.background_executor().timer(Duration::from_millis(250)).await;
                        changed.take();
                    }
                }
                let print_root = root.clone();
                let (took, print) = cx
                    .background_executor()
                    .spawn(async move {
                        let start = Instant::now();
                        let print = vault::fingerprint(&print_root);
                        (start.elapsed(), print)
                    })
                    .await;
                pause = Duration::from_secs(2).max(took * 200);
                next_check = Instant::now() + Duration::from_secs(30).max(took * 200);
                if last == Some((root.clone(), print)) {
                    continue;
                }
                // Only notes whose date changed are reread.
                let Ok(known) = this.update(cx, |this, _| this.notes.clone()) else {
                    return;
                };
                let scan_root = root.clone();
                let (notes, dirs, images, unreadable) = cx
                    .background_executor()
                    .spawn(async move { vault::rescan_full(&scan_root, &known) })
                    .await;
                let mut applied = false;
                this.update(cx, |this, cx| {
                    // A keystroke or a save during the read: try again on the next round.
                    if this.vault.as_ref() == Some(&root) && this.save_gen == generation && !this.panes.iter().any(|pane| pane.dirty) {
                        this.unreadable = unreadable;
                        this.sync(notes, dirs, images, cx);
                        last = Some((root, print));
                        applied = true;
                    }
                })
                .ok();
                if !applied {
                    // With a watcher, the next round is the check: bring it closer.
                    next_check = Instant::now() + Duration::from_secs(1);
                }
            }
        })
        .detach();
    }

    /// Aligns the app with the vault as just reread.
    pub(super) fn sync(
        &mut self,
        notes: Vec<Note>,
        mut dirs: Vec<PathBuf>,
        mut images: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        // Same notes, same tags, links and aliases? Each reread note is compared with the one we had
        // under its path, without copying or sorting anything.
        let had: std::collections::HashMap<&Path, &Note> = self.notes.iter().map(|n| (n.path.as_path(), n)).collect();
        let same = notes.len() == had.len() && notes.iter().all(|n| had.get(n.path.as_path()).is_some_and(|o| o.tags == n.tags && o.links == n.links && o.aliases == n.aliases));
        dirs.sort();
        self.dirs.sort();
        images.sort();
        self.images.sort();
        if !same || dirs != self.dirs || images != self.images {
            self.replace_notes(notes, true);
            self.dirs = dirs;
            self.images = images;
            self.index_changed(cx);
            cx.notify();
        } else {
            // No name, tag or link changed, but the text may have: search follows it.
            self.replace_notes(notes, false);
        }
        self.each_pane(|this| {
            // The shown table was rewritten elsewhere: it is reread if nothing is pending here.
            if let Some(sheet) = this.pane().sheet() {
                sheet.update(cx, |sheet, cx| sheet.reload_if_changed(cx));
            }
            // A failed canvas write still holds edits that must not be replaced by the disk.
            if !this.pane().canvas_dirty
                && let Some(canvas) = this.pane().drawing().cloned()
                && let Some(fresh) = this.pane().file_path().and_then(|path| fs::read_to_string(path).ok()).as_deref().and_then(Diagram::from_svg)
                && fresh.to_svg(diagram::COLORS[0]) != canvas.read(cx).diagram().to_svg(diagram::COLORS[0])
            {
                canvas.update(cx, |canvas, cx| canvas.replace(fresh, cx));
            }
            // The shown note was rewritten elsewhere: reread it. With no modification
            // pending here (the caller makes sure of it), nothing is lost.
            if let Some(path) = &this.pane().path
                && let Ok(text) = fs::read_to_string(path)
                && text != this.pane().editor.read(cx).text()
            {
                this.reload(cx);
            }
        });
    }

    pub(super) fn set_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if !self.may_close_window(window, cx) {
            return;
        }
        // Another vault: a single pane, on a new note.
        self.panes.truncate(1);
        self.active = 0;
        self.groups = vec![Group::of(0)];
        (self.layout, self.zoomed) = (Node::default(), None);
        self.closed.clear();
        self.vault = Some(root.clone());
        self.lost = None;
        self.remember_vault(&root);
        self.icons = vault::load_icons(&root);
        self.replace_notes(Vec::new(), true);
        self.dirs.clear();
        self.images.clear();
        self.recent.clear();
        self.new_note(String::new(), cx);
        config::save_config(&root, &[]);
        window.focus(&self.pane().editor.focus_handle(cx));
        // The index (names, tags) is built off the UI thread.
        self.scanning = true;
        cx.spawn(async move |this, cx| {
            let scan_root = root.clone();
            let (notes, dirs, images, unreadable) = cx
                .background_executor()
                .spawn(async move { vault::rescan_full(&scan_root, &[]) })
                .await;
            this.update(cx, |this, cx| {
                if this.vault.as_ref() == Some(&root) {
                    this.scanning = false;
                    this.unreadable = unreadable;
                    this.replace_notes(notes, true);
                    this.dirs = dirs;
                    this.images = images;
                    this.count_notes();
                    this.index_changed(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Creates the vault folder if it does not exist, then opens it: a folder already there is
    /// opened as is, nothing is written to it.
    pub(super) fn create_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match fs::create_dir_all(&root) {
            Ok(()) => self.switch_to(root, window, cx),
            Err(e) => {
                let what = tr("Cannot create the vault", "Impossible de créer le coffre");
                self.say(Tone::Failed, format!("{what} {} : {}", root.display(), crate::lang::io_reason(&e)), cx);
            }
        }
    }

    pub(super) fn choose_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Use this vault", "Choisir ce coffre").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(root) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.switch_to(root, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Everything that must follow a change of the index (`notes`, `images`): the names offered
    /// by the editors, and the graph. Call it after the change, never set `graph_stale` by hand.
    pub(super) fn index_changed(&mut self, cx: &mut Context<Self>) {
        self.push_names(cx);
        self.graph_changed(cx);
    }

    /// Only the graph follows: for a rewrite of note text, which leaves the names as they are
    /// (`index_changed` when a name, an alias or an image may have changed).
    pub(super) fn graph_changed(&mut self, cx: &mut Context<Self>) {
        self.graph_stale = true;
        self.refresh_graph(cx);
    }

    // The only places `self.notes` is written (the call to `index_changed` or `graph_changed`
    // stays with the caller, once per batch): each marks the graph stale when it can change it.

    /// The whole index replaced; `graph_changed` is false when only the text moved.
    pub(super) fn replace_notes(&mut self, notes: Vec<Note>, graph_changed: bool) {
        self.notes = notes;
        self.graph_stale |= graph_changed;
    }

    /// The text of the note at `path` rewritten in the index (not on disk); its previous text,
    /// or `None` if the index does not know the note.
    pub(super) fn edit_note_text(&mut self, path: &Path, text: Arc<str>) -> Option<Arc<str>> {
        let template = self.vault.as_ref().is_some_and(|r| vault::in_templates(r, path));
        let note = self.notes.iter_mut().find(|note| note.path == path)?;
        self.graph_stale = true;
        Some(note.set_text(text, template))
    }

    /// `[[old]]` links rewritten to `new` in the notes that link to it (on disk and in the index). The
    /// caller follows with `graph_changed` if some note was rewritten.
    pub(super) fn relink_notes(&mut self, old: &str, new: &str) -> vault::Relinked {
        vault::relink(self.vault.as_deref(), &mut self.notes, old, new)
    }

    /// The same for a renamed heading (`[[Note#old]]` → `[[Note#new]]`); the graph links notes, not
    /// headings, so it stays as it is. Returns the notes rewritten.
    pub(super) fn follow_heading_notes(&mut self, path: &Path, was: &str, now: &str) -> Vec<PathBuf> {
        vault::follow_heading(self.vault.as_deref(), &mut self.notes, path, was, now)
    }

    /// The notes `gone` says leave the index.
    pub(super) fn forget_notes(&mut self, gone: impl Fn(&PathBuf) -> bool) {
        self.notes.retain(|n| !gone(&n.path));
        self.graph_stale = true;
    }

    /// Every note path moved by `shift` (a note or folder was renamed).
    pub(super) fn move_notes(&mut self, shift: impl Fn(&PathBuf) -> PathBuf) {
        for note in &mut self.notes {
            note.path = shift(&note.path);
            note.name = vault::stem(&note.path);
        }
        self.graph_stale = true;
    }

    /// `fresh` (just saved) becomes the first note, in place of the one at `old` and of any at
    /// its own path. The graph is marked stale only if the note is new, renamed or linked
    /// differently. Returns the text of the replaced note, as it was last saved.
    pub(super) fn put_note(&mut self, old: Option<&PathBuf>, fresh: Note) -> Option<Arc<str>> {
        let known = self.notes.iter().find(|n| Some(&n.path) == old);
        if known.is_none_or(|n| n.path != fresh.path || n.links != fresh.links || n.aliases != fresh.aliases) {
            self.graph_stale = true;
        }
        let before = known.map(|n| n.body.clone());
        self.notes.retain(|n| Some(&n.path) != old && n.path != fresh.path);
        self.notes.insert(0, fresh);
        before
    }

    pub(super) fn push_names(&mut self, cx: &mut Context<Self>) {
        self.push_names_to(0..self.panes.len(), cx);
    }

    /// The same, to some tabs only: a new tab gets them without the others laying out again.
    pub(super) fn push_names_to(&mut self, tabs: std::ops::Range<usize>, cx: &mut Context<Self>) {
        // Aliases are offered after `[[` like names.
        let notes: Vec<&Note> = self.notes.iter().filter(|n| !self.is_template(n)).collect();
        let names: Vec<String> = notes.iter().map(|n| n.name.clone()).chain(notes.iter().flat_map(|n| n.aliases.iter().cloned())).collect();
        let pictures = vault::pictures(&self.images);
        // Notes and their aliases, for the sections embedded from them.
        let sources = logic::wiki::by_key(notes.iter().copied()).into_iter().map(|(key, i)| (key.as_str().to_string(), notes[i].path.clone())).collect();
        let lookups = std::rc::Rc::new(crate::views::editor::VaultLookups::new(names, &pictures, sources));
        let me = |pane: &Pane| pane.path.as_deref().and_then(Path::file_stem).map(|s| s.to_string_lossy().to_string());
        for pane in &self.panes[tabs] {
            let me = me(pane);
            pane.editor.update(cx, |e, _| e.set_vault(lookups.clone(), me));
        }
    }

    /// Tells the editor where to look for images: next to the note, then in the vault.
    pub(super) fn push_dirs(&mut self, cx: &mut Context<Self>) {
        let note_dir = self.pane().path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let dirs = note_dir.into_iter().chain(self.vault.clone()).collect();
        let me = self.pane().path.as_deref().and_then(Path::file_stem).map(|s| s.to_string_lossy().to_string());
        self.pane().editor.update(cx, |e, _| {
            e.set_dirs(dirs);
            e.set_trail(me);
        });
    }

    /// Puts the note at the head of the recent ones and saves the list.
    pub(super) fn touch(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(200);
        if let Some(root) = &self.vault {
            config::save_config(root, &self.recent);
        }
    }

    /// The notes, from the most recently opened to the oldest; those never
    /// opened follow, by modification date.
    pub(super) fn by_recency(&self) -> Vec<&Note> {
        // The rank of each recent note, found in one go: no list reread for each note.
        let rank: std::collections::HashMap<&Path, usize> = self.recent.iter().enumerate().rev().map(|(i, p)| (p.as_path(), i)).collect();
        let mut notes: Vec<&Note> = self.notes.iter().collect();
        notes.sort_by_cached_key(|n| rank.get(n.path.as_path()).copied().unwrap_or(usize::MAX));
        notes
    }

    /// Gives icon `icon` to the note, file or folder `path`; `None` removes it.
    pub(super) fn set_icon(&mut self, path: PathBuf, icon: Option<&str>, cx: &mut Context<Self>) {
        match icon {
            Some(icon) => self.icons.insert(path, icon.to_string()),
            None => self.icons.remove(&path),
        };
        self.save_icons(cx);
    }

    pub(super) fn save_icons(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = &self.vault
            && let Err(e) = vault::save_icons(root, &self.icons)
        {
            self.say(Tone::Failed, format!("{} : {}", tr("Icons not saved", "Icônes non enregistrées"), crate::lang::io_reason(&e)), cx);
        }
        cx.notify();
    }

    /// Opens the system terminal in the vault, or says it found none.
    pub(super) fn open_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        if !terminal::open_terminal(&root) {
            let hint = tr("No terminal found: name yours in $TERMINAL", "Aucun terminal trouvé : nommer le vôtre dans $TERMINAL");
            self.say(Tone::Failed, hint, cx);
        }
    }

    /// Vault backup: asks where to put it.
    pub(super) fn choose_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Save the backup here", "Enregistrer la sauvegarde ici").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(dir) = paths.into_iter().next()
            {
                this.update(cx, |this, cx| this.backup_to(dir, cx)).ok();
            }
        })
        .detach();
    }

    /// Writes into `dir`, in the background, the dated archive of the whole vault, then says where it is.
    pub(super) fn backup_to(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        self.flush(cx);
        self.say(Tone::Busy, tr("Backing up the vault…", "Sauvegarde du coffre…"), cx);
        let date = date_name(today());
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { backup::backup(&root, &dir, &date) }).await;
            this.update(cx, |this, cx| match done {
                Ok(to) => this.say(Tone::Done, format!("{} {}", tr("Backup saved:", "Sauvegarde enregistrée :"), to.display()), cx),
                Err(e) => this.say(Tone::Failed, format!("{} : {}", tr("Backup failed", "Sauvegarde impossible"), crate::lang::io_reason(&e)), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Vault trash: the list of what it holds; Enter puts the chosen item back at
    /// the vault root, where the file watcher finds it.
    pub(super) fn open_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let found = vault::trashed(&root);
        if found.is_empty() {
            return self.say(Tone::Info, tr("The trash is empty", "La corbeille est vide"), cx);
        }
        let names = found.iter().map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(tr("Restore from the trash", "Restaurer depuis la corbeille"), names, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let chosen = palette.read(cx).chosen().and_then(|i| found.get(i)).filter(|_| matches!(event, PaletteEvent::Submit(_)));
            match chosen.map(|path| vault::restore(&root, path).inspect(|to| crate::views::sheet::move_tables(path, to))) {
                Some(Ok(to)) => {
                    let name = to.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    this.say(Tone::Done, format!("{} {name}", tr("Restored:", "Restauré :")), cx);
                }
                Some(Err(e)) => this.say(Tone::Failed, format!("{} : {}", tr("Not restored", "Restauration impossible"), crate::lang::io_reason(&e)), cx),
                None => {}
            }
            this.palette = None;
            window.focus(&this.pane().editor.focus_handle(cx));
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    /// `Shell::notes` changes only in this file (`replace_notes`, `put_note`…), which keep the graph
    /// in step: nowhere else writes it or lends it out mutably.
    #[test]
    fn notes_are_written_only_in_the_library() {
        let writes = ["&mut self.notes", "self.notes =", "self.notes.push", "self.notes.insert", "self.notes.retain", "self.notes.sort", "self.notes.iter_mut", "self.notes.clear", "self.notes.extend", "self.notes.remove"];
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shell");
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") || path.file_name().is_some_and(|n| n == "library.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for write in writes {
                assert!(!text.contains(write), "{} writes the index with `{write}`: go through library.rs", path.display());
            }
        }
    }
}
