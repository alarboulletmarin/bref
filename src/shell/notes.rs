//! Notes: opening, previewing, creating, saving, renaming with their title, links and wikilinks.

use super::*;

impl Shell {
    /// Copie toute la note dans le presse-papiers.
    /// Copie la note ; avec `block`, seulement le bloc de code où est le curseur
    /// s'il y en a un.
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

    /// Charge la note dans l'éditeur ; faux si le fichier est illisible.
    pub(super) fn load_note(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        // Déjà dans l'autre pane : deux éditeurs sur un même fichier s'écraseraient l'un l'autre.
        // La saisie y passe ; un simple aperçu ne la déplace pas.
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
        // Une pièce jointe (PDF, vidéo, archive…) s'ouvre dans l'application du système ; la note reste là.
        if vault::is_attachment(path) {
            // La plateforme de test n'ouvre rien : elle note ce qu'on lui aurait demandé.
            #[cfg(test)]
            self.opened.push(path.to_path_buf());
            #[cfg(not(test))]
            cx.open_with_system(path);
            return false;
        }
        if vault::is_table(path) {
            return self.open_table(path, cx);
        }
        // Une image prend la place de la note, qui reste chargée dessous.
        if vault::is_image(path) {
            let svg = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
            let open = self.pane().file.as_ref().filter(|file| file.path == path).map(|file| file.kind.clone());
            let theme = self.theme;
            let kind = match open {
                _ if !svg => FileKind::Image,
                // Un schéma s'ouvre dans son canevas, prêt à être repris.
                Some(FileKind::Drawing(canvas)) => FileKind::Drawing(canvas),
                open => match fs::read_to_string(path).ok().and_then(|svg| Diagram::from_svg(&svg)) {
                    Some(diagram) => {
                        let canvas = cx.new(|cx| Canvas::new(diagram, theme, cx));
                        cx.subscribe(&canvas, Self::on_canvas_event).detach();
                        FileKind::Drawing(canvas)
                    }
                    // Un SVG d'ailleurs s'ouvre dans la visionneuse : on le zoome, on s'y déplace.
                    None => match open {
                        Some(FileKind::Svg(viewer)) => FileKind::Svg(viewer),
                        _ => {
                            let source = viewer::Source::File(path.to_path_buf(), None);
                            FileKind::Svg(cx.new(|cx| viewer::Viewer::new(source, theme, cx)))
                        }
                    },
                },
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
                // Une note relue garde sa vue ; une autre s'ouvre en tableau si elle en est un.
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
                self.say(Tone::Failed, format!("{what} {} : {e}", path.display()), cx);
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

    /// Affiche la note sans la compter comme ouverte.
    pub(super) fn preview_note(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.pane_mut().peek = true;
        // La note déjà chargée, cachée par une image : il suffit de retirer l'image.
        if self.pane().path.as_deref() == Some(path) {
            if self.pane().file.is_some() {
                self.set_file(None, cx);
                cx.notify();
            }
        } else if self.load_note(path, cx) {
            self.pane_mut().preview = true;
        }
    }

    /// L'aperçu devient une note ouverte.
    pub(super) fn keep_preview(&mut self) {
        self.pane_mut().peek = false;
        if std::mem::take(&mut self.pane_mut().preview)
            && let Some(path) = self.pane().path.clone()
        {
            self.touch(&path);
        }
    }

    /// Nouvelle note, rangée dans le dossier sélectionné quand l'arbre est affiché.
    pub(super) fn new_note_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_note_in(self.nav.target_dir(), window, cx);
    }

    /// Nouvelle note rangée dans `dir` ; `None` : à la racine du coffre.
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

    /// Avant de quitter la note affichée : elle est enregistrée et, si son titre
    /// l'a renommée, les liens vers son ancien nom suivent. Attendre ce moment
    /// évite de réécrire les liens à chaque titre intermédiaire pendant la frappe.
    pub(super) fn leave(&mut self, cx: &mut Context<Self>) {
        self.mark_place(cx);
        self.flush(cx);
        if let (Some(old), Some(path)) = (self.pane_mut().origin.take(), &self.pane_mut().path) {
            let new = vault::stem(path);
            self.relink(&old, &new, cx);
        }
        self.drop_if_empty(cx);
    }

    /// Une note créée ici puis vidée ne laisse pas de fichier vide derrière elle, ni dans les
    /// récentes. Seul un fichier vide sur le disque est retiré : rien d'écrit ne se perd.
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

    /// Les `[[liens]]` vers `old` visent désormais `new` dans toutes les notes ;
    /// vrai si la note affichée a été réécrite. Sans effet si une autre note
    /// s'appelle encore `old` : les liens sont peut-être pour elle.
    // ponytail: réécriture synchrone, limitée aux notes que l'index dit liées à
    // `old` ; passer en tâche de fond si une note est liée depuis des centaines d'autres.
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
        // La note de l'autre pane est relue ici ; celle du pane actif, l'appelant en décide
        // (il est peut-être en train de la quitter).
        let showing = |pane: &Pane| pane.path.as_ref().is_some_and(|path| changed.contains(path));
        for pane in 0..self.panes.len() {
            if pane != self.active && showing(&self.panes[pane]) {
                self.in_pane(pane, |this| this.reload(cx));
            }
        }
        showing(&self.panes[self.active])
    }

    /// Relit depuis le disque la note affichée, réécrite en dehors de l'éditeur.
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.pane().path.clone() {
            // Relire n'est pas ouvrir : l'aperçu reste un aperçu, la page liste reste affichée.
            let (preview, listing) = (self.pane_mut().preview, self.pane_mut().listing.take());
            self.load_note(&path, cx);
            (self.pane_mut().preview, self.pane_mut().listing) = (preview, listing);
        }
    }

    /// Met le disque à jour : chaque pane écrit ce qui y attend.
    pub(super) fn flush(&mut self, cx: &mut Context<Self>) {
        self.each_pane(|this| this.flush_one(cx));
    }

    /// Écrit la note du pane sur disque si elle a changé.
    // ponytail: écriture synchrone sur le thread UI (quelques Ko, < 1 ms) ;
    // passer en tâche de fond si les notes deviennent très grosses.
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
        // Un titre `#` ajouté ou modifié en tête : le fichier porte désormais son nom.
        if vault::h1_of(&content).is_some_and(|h1| self.pane().h1.as_ref() != Some(&h1)) {
            self.pane_mut().synced = true;
        }
        let dir = self.pane().new_dir.as_deref().unwrap_or(&root);
        match vault::save(dir, self.pane().path.as_deref(), self.pane().synced, &content) {
            Ok(path) => {
                self.pane_mut().dirty = false;
                self.calm();
                let (tags, links) = markdown::index(&content);
                // Un modèle n'a ni tags ni liens.
                let (tags, links) = if vault::in_templates(&root, &path) { (Vec::new(), Vec::new()) } else { (tags, links) };
                let old = self.pane().path.clone();
                // Le graphe ne change que si la note est nouvelle, renommée ou liée autrement.
                let known = self.notes.iter().find(|n| Some(&n.path) == old.as_ref());
                if known.is_none_or(|n| n.path != path || n.links[..] != links[..]) {
                    self.graph_stale = true;
                }
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
                    // Renommée par son titre, la note garde son icône.
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
                    self.pane_mut().path = Some(path);
                    self.push_names(cx);
                    self.push_dirs(cx);
                }
                self.refresh_graph(cx);
            }
            Err(e) => {
                self.say(Tone::Failed, format!("{} : {e}", tr("Note not saved", "Note non enregistrée")), cx)
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
        // Un geste fait dans une note (lien suivi, menu, remplacement) vaut pour son pane ; la
        // frappe, elle, peut venir d'ailleurs (une carte du tableau, une cellule de la liste).
        if !matches!(event, EditorEvent::Changed) {
            self.active = pane;
        }
        match event {
            EditorEvent::Changed => {
                self.in_pane(pane, |this| {
                    // La clé du tableau vient d'être écrite dans le texte : on y reste, le
                    // bouton du coin montre le tableau quand on le veut.
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
            EditorEvent::Swap(swap) => self.replace_in_vault(swap.clone(), window, cx),
            EditorEvent::Menu(at, link, commented) => {
                self.menu = Some(nav::Menu { at: *at, target: None, text: Some(link.clone()), commented: *commented, block: false });
                cx.notify();
            }
            EditorEvent::BlockMenu(at) => {
                self.menu = Some(nav::Menu { at: *at, target: None, text: Some(None), commented: false, block: true });
                cx.notify();
            }
            // Le diagramme prend la place de la note, comme une image ; Échap y ramène.
            EditorEvent::View(source) => {
                let (source, theme) = (viewer::Source::Mermaid(source.clone()), self.theme);
                let figure = cx.new(|cx| viewer::Viewer::new(source, theme, cx));
                window.focus(&figure.focus_handle(cx));
                self.pane_mut().figure = Some(figure);
                cx.notify();
            }
        }
    }

    /// Suit un lien de la note : l'adresse dans le navigateur, le tag dans la palette, la note.
    pub(super) fn follow(&mut self, link: &Link, window: &mut Window, cx: &mut Context<Self>) {
        match link {
            Link::Url(url) => cx.open_url(url),
            Link::Tag(tag) => self.open_palette(&format!("#{tag}"), window, cx),
            // Un dossier se lit comme une page liste, quand aucune note ne porte son nom.
            Link::Wiki(name) if self.wiki_path(name).is_none() && self.wiki_file(name).is_none() => match self.wiki_dir(name) {
                Some(dir) => self.open_listing(dir, window, cx),
                None => self.open_wiki(name, cx),
            },
            Link::Wiki(name) => self.open_wiki(name, cx),
        }
    }

    /// Le fichier du coffre (image, tableau, pièce jointe) que désigne `[[nom.ext]]`, sans tenir compte de la casse.
    pub(super) fn wiki_file(&self, name: &str) -> Option<PathBuf> {
        let wanted = name.to_lowercase();
        self.images.iter().find(|path| path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == wanted)).cloned()
    }

    /// Le dossier du coffre que désigne `[[Nom]]`, sauf celui des modèles.
    pub(super) fn wiki_dir(&self, name: &str) -> Option<PathBuf> {
        let (root, wanted) = (self.vault.as_ref()?, name.to_lowercase());
        self.dirs.iter().find(|dir| !vault::in_templates(root, &dir.join("x")) && dir.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == wanted)).cloned()
    }

    /// Note du jour : la note qui porte la date locale pour nom, où qu'elle soit dans le
    /// coffre ; créée avec cette date pour titre si elle n'existe pas encore.
    pub(super) fn open_today(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let name = date_name(today());
        // Pas encore de note du jour, et un modèle `Daily` ou `Journal` : elle en naît.
        if self.wiki_path(&name).is_none()
            && let Some(template) = self.daily_template()
            && self.create_from(&template, &name, &root, cx).is_some()
        {
            return;
        }
        self.open_wiki(&name, cx);
    }

    /// La note que désigne un `[[lien]]` : par son nom d'abord, sinon par un alias de son
    /// en-tête YAML.
    pub(super) fn wiki_path(&self, name: &str) -> Option<PathBuf> {
        let wanted = name.to_lowercase();
        // Un modèle ne répond à aucun lien : `[[Réunion]]` vise la note, pas son modèle.
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
