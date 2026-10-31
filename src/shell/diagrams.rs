//! Diagrams: creating, importing, saving and exporting them.

use super::*;

impl Shell {
    /// The diagram is saved on every change: nothing waits in memory.
    // ponytail: synchronous write on every gesture or keystroke (a few KB);
    // defer it like the notes' if big diagrams make things wait.
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
            CanvasEvent::Template(i) => {
                self.template = Some(*i);
                return cx.notify();
            }
        };
        match result {
            Ok(()) => self.calm(),
            Err(e) => self.say(Tone::Failed, format!("{} : {}", tr("Diagram not saved", "Schéma non enregistré"), crate::lang::io_reason(&e)), cx),
        }
        cx.notify();
    }

    /// PNG image of the diagram, next to it: dark ink on a white background, so
    /// that it pastes anywhere. It is drawn off the UI thread.
    pub(super) fn export_png(&mut self, file: PathBuf, canvas: &Entity<Canvas>, cx: &mut Context<Self>) {
        let svg = canvas.read(cx).diagram().to_svg(0x1e1e1e);
        let canvas = canvas.downgrade();
        cx.spawn(async move |this, cx| {
            let drawn = cx.background_executor().spawn(async move {
                let bytes = figure::png(&svg, 2., 0xffffff).ok_or_else(|| std::io::Error::other("PNG"))?;
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
                    Err(e) => this.say(Tone::Failed, format!("{} : {}", tr("Picture not saved", "Image non enregistrée"), crate::lang::io_reason(&e)), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Folder of a new diagram: the one selected in the tree, otherwise the note's.
    pub(super) fn diagram_dir(&self) -> Option<PathBuf> {
        let beside = self.pane().path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        self.nav.target_dir().or(beside).or(self.vault.clone())
    }

    pub(super) fn new_diagram(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dir) = self.diagram_dir().filter(|_| self.vault.is_some()) {
            self.add_diagram(&dir, tr("Diagram", "Schéma"), Diagram::blank(), window, cx);
        }
    }

    /// A new diagram that starts from one of the library's, and opens.
    pub(super) fn new_diagram_from(&mut self, template: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(template) = crate::logic::library::templates().into_iter().nth(template) else { return };
        if let Some(dir) = self.diagram_dir().filter(|_| self.vault.is_some()) {
            self.add_diagram(&dir, template.name, (template.make)(), window, cx);
        }
    }

    /// Asks for an Excalidraw or draw.io file, and makes a vault diagram of it.
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
        let read = fs::read_to_string(file).map_err(|e| crate::lang::io_reason(&e)).and_then(|text| match drawio {
            true => import::drawio(&text),
            false => import::excalidraw(&text),
        });
        match (read, self.diagram_dir()) {
            (Ok(imported), Some(dir)) => {
                self.add_diagram(&dir, &vault::stem(file), imported.diagram, window, cx);
                // Said after the diagram is shown: the message must not be taken for its failure.
                if imported.skipped > 0 {
                    let said = if imported.skipped == 1 { tr("item not imported", "élément non importé") } else { tr("items not imported", "éléments non importés") };
                    self.say(Tone::Info, format!("{} {said}", imported.skipped), cx);
                }
            }
            (Err(e), _) => self.say(Tone::Failed, format!("{} : {}", tr("Import failed", "Import impossible"), e), cx),
            _ => {}
        }
        cx.notify();
    }

    /// Shows a diagram, a formula or a picture of the note in its place.
    pub(super) fn show_figure(&mut self, source: viewer::Source, window: &mut Window, cx: &mut Context<Self>) {
        let theme = self.theme;
        let figure = cx.new(|cx| viewer::Viewer::new(source, theme, cx));
        cx.subscribe_in(&figure, window, |this, _, event, window, cx| match event {
            viewer::ViewerEvent::ToCanvas(text) => this.mermaid_to_canvas(text, window, cx),
        })
        .detach();
        window.focus(&figure.focus_handle(cx));
        self.pane_mut().figure = Some(figure);
        cx.notify();
    }

    /// A Mermaid diagram of a note becomes a canvas diagram, next to the note, to carry on by hand. The note is
    /// not touched; what could not be read is counted.
    pub(super) fn mermaid_to_canvas(&mut self, source: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(made), Some(dir)) = (logic::mermaid::to_diagram(source), self.diagram_dir().filter(|_| self.vault.is_some())) else { return };
        let name = self.pane().path.as_deref().map(vault::stem).unwrap_or_default();
        let name = if name.is_empty() { tr("Diagram", "Schéma").to_string() } else { format!("{name} - {}", tr("diagram", "schéma")) };
        self.pane_mut().figure = None;
        self.add_diagram(&dir, &name, made.diagram, window, cx);
        if made.skipped > 0 {
            let lines = format!("{} {}", made.skipped, if made.skipped == 1 { tr("line was not understood", "ligne n'a pas été comprise") } else { tr("lines were not understood", "lignes n'ont pas été comprises") });
            self.say(Tone::Info, lines, cx);
        }
    }

    /// Saves `diagram` in `dir` under a free name, and opens it.
    pub(super) fn add_diagram(&mut self, dir: &Path, name: &str, diagram: Diagram, window: &mut Window, cx: &mut Context<Self>) {
        let path = vault::free_path(dir, name, "svg");
        if let Err(e) = vault::write(&path, &diagram.to_svg(diagram::COLORS[0])) {
            return self.say(Tone::Failed, format!("{} : {}", tr("Diagram not saved", "Schéma non enregistré"), crate::lang::io_reason(&e)), cx);
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
