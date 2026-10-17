//! The vault: choosing it, scanning and watching it, icons, backup, trash, terminal.

use super::*;

impl Shell {
    /// Relit le coffre tout de suite, sans attendre la surveillance : ce que la fusion a écrit
    /// s'affiche, chaque note gardant son curseur et son défilement.
    pub(super) fn rescan(&mut self, root: &Path, cx: &mut Context<Self>) {
        let views: Vec<(usize, f32)> = self.panes.iter().map(|pane| pane.editor.read(cx).view()).collect();
        let (notes, dirs, images) = vault::rescan(root, &self.notes);
        self.sync(notes, dirs, images, cx);
        for (pane, (at, scroll)) in self.panes.iter().zip(views) {
            if pane.editor.read(cx).view().0 != at {
                pane.editor.update(cx, |editor, cx| editor.set_view(at, scroll, cx));
            }
        }
    }

    /// Suit ce que d'autres programmes changent dans le coffre : notes ajoutées,
    /// renommées, supprimées, ou modifiées pendant qu'elles sont affichées.
    ///
    /// Le système signale les changements (inotify, FSEvents…) : la relecture suit
    /// dans la fraction de seconde. Une relecture de contrôle a lieu quand même toutes
    /// les 30 s, au cas où un événement se perdrait. Sans surveillance possible (limite
    /// d'inotify atteinte, dossier réseau), l'empreinte du coffre est recalculée toutes
    /// les 2 s.
    // ponytail: l'empreinte (noms et dates, sans lire les fichiers) est calculée hors
    // du thread UI et ne prend jamais plus de 0,5 % d'un cœur : un gros coffre est
    // donc regardé moins souvent (5 000 notes : toutes les 5 s, ou 100 s en contrôle).
    // Les tests n'installent pas de surveillance : leur horloge est simulée.
    pub(super) fn watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let changed = Arc::new(vault::Signal::default());
            let mut watched: Option<(PathBuf, Option<notify::RecommendedWatcher>)> = None;
            let mut last = None;
            let mut pause = Duration::from_secs(2);
            // Prochain contrôle de la surveillance.
            let mut next_check = Instant::now();
            loop {
                let live = watched.as_ref().is_some_and(|(_, w)| w.is_some());
                // Avec la surveillance, la tâche dort jusqu'au signal du système ; elle se réveille
                // quand même toutes les 2 s pour voir si on a changé de coffre.
                let notified = if live {
                    changed.wait(cx.background_executor().timer(Duration::from_secs(2))).await
                } else {
                    cx.background_executor().timer(pause).await;
                    false
                };
                let Ok(state) = this.update(cx, |this, _| this.vault.clone().map(|root| (root, this.save_gen)))
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
                        // Un enregistrement ou une synchronisation touche souvent plusieurs
                        // fichiers d'affilée : on laisse la rafale se terminer.
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
                // Seules les notes dont la date a changé sont relues.
                let Ok(known) = this.update(cx, |this, _| this.notes.clone()) else {
                    return;
                };
                let scan_root = root.clone();
                let (notes, dirs, images) = cx
                    .background_executor()
                    .spawn(async move { vault::rescan(&scan_root, &known) })
                    .await;
                let mut applied = false;
                this.update(cx, |this, cx| {
                    // Une frappe ou un enregistrement pendant la lecture : on réessaie au prochain tour.
                    if this.vault.as_ref() == Some(&root) && this.save_gen == generation && !this.panes.iter().any(|pane| pane.dirty) {
                        this.sync(notes, dirs, images, cx);
                        last = Some((root, print));
                        applied = true;
                    }
                })
                .ok();
                if !applied {
                    // Avec une surveillance, le prochain tour est celui du contrôle : le rapprocher.
                    next_check = Instant::now() + Duration::from_secs(1);
                }
            }
        })
        .detach();
    }

    /// Aligne l'app sur le coffre tel qu'il vient d'être relu.
    pub(super) fn sync(
        &mut self,
        notes: Vec<Note>,
        mut dirs: Vec<PathBuf>,
        mut images: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        // Mêmes notes, mêmes tags, mêmes liens ? Chaque note relue se compare à celle qu'on avait
        // sous son chemin, sans rien copier ni trier.
        let had: std::collections::HashMap<&Path, &Note> = self.notes.iter().map(|n| (n.path.as_path(), n)).collect();
        let same = notes.len() == had.len() && notes.iter().all(|n| had.get(n.path.as_path()).is_some_and(|o| o.tags == n.tags && o.links == n.links));
        dirs.sort();
        self.dirs.sort();
        images.sort();
        self.images.sort();
        if !same || dirs != self.dirs || images != self.images {
            self.notes = notes;
            self.dirs = dirs;
            self.images = images;
            self.push_names(cx);
            self.graph_stale = true;
            self.refresh_graph(cx);
            cx.notify();
        } else {
            // Ni nom, ni tag, ni lien n'ont changé, mais le texte peut avoir : la recherche le suit.
            self.notes = notes;
        }
        self.each_pane(|this| {
            // Le tableau affiché a été réécrit ailleurs : il est relu s'il n'attend rien ici.
            if let Some(sheet) = this.pane().sheet() {
                sheet.update(cx, |sheet, cx| sheet.reload_if_changed(cx));
            }
            // La note affichée a été réécrite ailleurs : on la relit. Sans modification
            // en attente ici (l'appelant s'en assure), rien n'est perdu.
            if let Some(path) = &this.pane().path
                && let Ok(text) = fs::read_to_string(path)
                && text != this.pane().editor.read(cx).text()
            {
                this.reload(cx);
            }
        });
    }

    pub(super) fn set_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.each_pane(|this| this.leave(cx));
        // Un autre coffre : un seul pane, sur une note neuve.
        self.panes.truncate(1);
        self.active = 0;
        self.vault = Some(root.clone());
        self.icons = vault::load_icons(&root);
        self.notes.clear();
        self.dirs.clear();
        self.images.clear();
        self.recent.clear();
        self.new_note(String::new(), cx);
        vault::save_config(&root, &[]);
        window.focus(&self.pane().editor.focus_handle(cx));
        // L'index (noms, tags) se construit hors du thread UI.
        cx.spawn(async move |this, cx| {
            let scan_root = root.clone();
            let (notes, dirs, images) = cx
                .background_executor()
                .spawn(async move { vault::scan(&scan_root) })
                .await;
            this.update(cx, |this, cx| {
                if this.vault.as_ref() == Some(&root) {
                    this.notes = notes;
                    this.dirs = dirs;
                    this.images = images;
                    this.push_names(cx);
                    this.graph_stale = true;
                    this.refresh_graph(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Crée le dossier du coffre s'il n'existe pas, puis l'ouvre : un dossier déjà là est
    /// ouvert tel quel, rien n'y est écrit.
    pub(super) fn create_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match fs::create_dir_all(&root) {
            Ok(()) => self.set_vault(root, window, cx),
            Err(e) => {
                let what = tr("Cannot create the vault", "Impossible de créer le coffre");
                self.say(Tone::Failed, format!("{what} {} : {e}", root.display()), cx);
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
                this.update_in(cx, |this, window, cx| this.set_vault(root, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    pub(super) fn push_names(&mut self, cx: &mut Context<Self>) {
        // Les alias se proposent après `[[` comme des noms.
        let notes: Vec<&Note> = self.notes.iter().filter(|n| !self.is_template(n)).collect();
        let names: Vec<String> = notes.iter().map(|n| n.name.clone()).chain(notes.iter().flat_map(|n| n.aliases.iter().cloned())).collect();
        let pictures = vault::pictures(&self.images);
        for pane in &self.panes {
            pane.editor.update(cx, |e, _| {
                e.set_notes(names.clone());
                e.set_images(&pictures);
            });
        }
    }

    /// Indique à l'éditeur où chercher les images : à côté de la note, puis dans le coffre.
    pub(super) fn push_dirs(&mut self, cx: &mut Context<Self>) {
        let note_dir = self.pane().path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let dirs = note_dir.into_iter().chain(self.vault.clone()).collect();
        self.pane().editor.update(cx, |e, _| e.set_dirs(dirs));
    }

    /// Place la note en tête des récentes et mémorise la liste.
    pub(super) fn touch(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(200);
        if let Some(root) = &self.vault {
            vault::save_config(root, &self.recent);
        }
    }

    /// Les notes, de la plus récemment ouverte à la plus ancienne ; celles jamais
    /// ouvertes suivent, par date de modification.
    pub(super) fn by_recency(&self) -> Vec<&Note> {
        // Le rang de chaque récente, trouvé d'un coup : pas de liste relue pour chaque note.
        let rank: std::collections::HashMap<&Path, usize> = self.recent.iter().enumerate().rev().map(|(i, p)| (p.as_path(), i)).collect();
        let mut notes: Vec<&Note> = self.notes.iter().collect();
        notes.sort_by_cached_key(|n| rank.get(n.path.as_path()).copied().unwrap_or(usize::MAX));
        notes
    }

    /// Donne l'icône `icon` à la note, au fichier ou au dossier `path` ; `None` la lui retire.
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
            self.say(Tone::Failed, format!("{} : {e}", tr("Icons not saved", "Icônes non enregistrées")), cx);
        }
        cx.notify();
    }

    /// Ouvre le terminal du système dans le coffre, ou dit qu'il n'en a pas trouvé.
    pub(super) fn open_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        if !vault::open_terminal(&root) {
            let hint = tr("No terminal found: name yours in $TERMINAL", "Aucun terminal trouvé : nommer le vôtre dans $TERMINAL");
            self.say(Tone::Failed, hint, cx);
        }
    }

    /// Sauvegarde du coffre : demande où la ranger.
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

    /// Écrit dans `dir`, en tâche de fond, l'archive datée de tout le coffre, puis dit où elle est.
    pub(super) fn backup_to(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        self.flush(cx);
        self.say(Tone::Busy, tr("Backing up the vault…", "Sauvegarde du coffre…"), cx);
        let date = date_name(today());
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { vault::backup(&root, &dir, &date) }).await;
            this.update(cx, |this, cx| match done {
                Ok(to) => this.say(Tone::Done, format!("{} {}", tr("Backup saved:", "Sauvegarde enregistrée :"), to.display()), cx),
                Err(e) => this.say(Tone::Failed, format!("{} : {e}", tr("Backup failed", "Sauvegarde impossible")), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Corbeille du coffre : la liste de ce qu'elle contient ; Entrée remet l'élément choisi à
    /// la racine du coffre, où le suivi des fichiers le retrouve.
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
            match chosen.map(|path| vault::restore(&root, path)) {
                Some(Ok(to)) => {
                    let name = to.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    this.say(Tone::Done, format!("{} {name}", tr("Restored:", "Restauré :")), cx);
                }
                Some(Err(e)) => this.say(Tone::Failed, format!("{} : {e}", tr("Not restored", "Restauration impossible")), cx),
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
