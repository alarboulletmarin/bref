//! Diagrams: creating, importing, saving and exporting them.

use super::*;

impl Shell {
    /// Le schéma est enregistré à chaque changement : rien n'attend en mémoire.
    // ponytail: écriture synchrone à chaque geste ou frappe (quelques Ko) ;
    // la différer comme celle des notes si de gros schémas font attendre.
    pub(super) fn on_canvas_event(&mut self, canvas: Entity<Canvas>, event: &CanvasEvent, cx: &mut Context<Self>) {
        let shows = |pane: &Pane| pane.drawing() == Some(&canvas);
        if let Some(pane) = self.panes.iter().position(shows) {
            self.in_pane(pane, |this| this.canvas_event(canvas, event, cx));
        }
    }

    pub(super) fn canvas_event(&mut self, canvas: Entity<Canvas>, event: &CanvasEvent, cx: &mut Context<Self>) {
        let Some(path) = self.pane().file_path().filter(|_| self.pane().drawing() == Some(&canvas)) else {
            return;
        };
        let result = match event {
            CanvasEvent::Changed => vault::write(path, &canvas.read(cx).diagram().to_svg(diagram::COLORS[0])),
            CanvasEvent::Export => return self.export_png(path.clone(), &canvas, cx),
        };
        match result {
            Ok(()) => self.calm(),
            Err(e) => self.say(Tone::Failed, format!("{} : {e}", tr("Diagram not saved", "Schéma non enregistré")), cx),
        }
        cx.notify();
    }

    /// Image PNG du schéma, à côté de lui : encre sombre sur fond blanc, pour
    /// qu'elle se colle partout. Elle est dessinée hors du thread UI.
    pub(super) fn export_png(&mut self, file: PathBuf, canvas: &Entity<Canvas>, cx: &mut Context<Self>) {
        let svg = canvas.read(cx).diagram().to_svg(0x1e1e1e);
        let canvas = canvas.downgrade();
        cx.spawn(async move |this, cx| {
            let drawn = cx.background_executor().spawn(async move {
                let bytes = figure::png(&svg, 2.).ok_or_else(|| std::io::Error::other("PNG"))?;
                let path = vault::free_path(file.parent().unwrap_or(Path::new("")), &vault::stem(&file), "png");
                fs::write(&path, bytes).map(|_| path)
            });
            let saved = drawn.await;
            this.update(cx, |this, cx| {
                match saved {
                    Ok(path) => {
                        this.images.push(path);
                        this.push_names(cx);
                        this.graph_stale = true;
                        this.refresh_graph(cx);
                        canvas.update(cx, |canvas, cx| canvas.exported(cx)).ok();
                    }
                    Err(e) => this.say(Tone::Failed, format!("{} : {e}", tr("Picture not saved", "Image non enregistrée")), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Dossier d'un nouveau schéma : celui sélectionné dans l'arbre, sinon celui de la note.
    pub(super) fn diagram_dir(&self) -> Option<PathBuf> {
        let beside = self.pane().path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        self.nav.target_dir().or(beside).or(self.vault.clone())
    }

    pub(super) fn new_diagram(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dir) = self.diagram_dir().filter(|_| self.vault.is_some()) {
            self.add_diagram(&dir, tr("Diagram", "Schéma"), Diagram::default(), window, cx);
        }
    }

    /// Demande un fichier Excalidraw ou draw.io, et en fait un schéma du coffre.
    pub(super) fn import_diagram(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr("Import", "Importer").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(file) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.import_file(&file, window, cx)).ok();
            }
        })
        .detach();
    }

    pub(super) fn import_file(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let drawio = file.extension().is_some_and(|e| e == "drawio" || e == "xml");
        let read = fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|text| match drawio {
            true => import::drawio(&text),
            false => import::excalidraw(&text),
        });
        match (read, self.diagram_dir()) {
            (Ok(diagram), Some(dir)) => self.add_diagram(&dir, &vault::stem(file), diagram, window, cx),
            (Err(e), _) => self.say(Tone::Failed, format!("{} : {e}", tr("Import failed", "Import impossible")), cx),
            _ => {}
        }
        cx.notify();
    }

    /// Enregistre `diagram` dans `dir` sous un nom libre, et l'ouvre.
    pub(super) fn add_diagram(&mut self, dir: &Path, name: &str, diagram: Diagram, window: &mut Window, cx: &mut Context<Self>) {
        let path = vault::free_path(dir, name, "svg");
        if let Err(e) = vault::write(&path, &diagram.to_svg(diagram::COLORS[0])) {
            return self.say(Tone::Failed, format!("{} : {e}", tr("Diagram not saved", "Schéma non enregistré")), cx);
        }
        self.images.push(path.clone());
        self.push_names(cx);
        self.graph_stale = true;
        self.refresh_graph(cx);
        self.nav.reveal(&path);
        self.load_note(&path, cx);
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        if let Some(canvas) = self.pane().drawing() {
            window.focus(&canvas.focus_handle(cx));
        } else if let Some(viewer) = self.pane().viewer() {
            window.focus(&viewer.focus_handle(cx));
        }
        self.settle_nav(window, cx);
    }
}
