//! Panes: what each half of the window shows, which one is active, splitting and closing.

use super::*;

/// Width below which two panes do not fit next to the panel: it folds.
pub(super) const MIN_PANE: Pixels = px(300.);

/// Share of the width the left pane can take.
pub(super) const PANE_RATIO: std::ops::RangeInclusive<f32> = 0.2..=0.8;

/// What is open in one half of the window: a note and its editor, what is shown
/// in its place (image, diagram, table, list page), and the note's kanban view.
pub(super) struct Pane {
    /// Holds the focus as soon as one of the pane's views has it: that is what makes it active. It
    /// holds it itself when the pane only shows an image.
    pub(super) zone: FocusHandle,
    pub(super) editor: Entity<Editor>,
    /// File shown in place of the note, picked in the tree or the graph (`Shell::set_file`).
    pub(super) file: Option<FileView>,
    /// Mermaid diagram of the note, viewed large in its place.
    pub(super) figure: Option<Entity<viewer::Viewer>>,
    /// File of the open note; `None` as long as a new note is not saved.
    pub(super) path: Option<PathBuf>,
    /// The file name follows the note's title (first line).
    pub(super) synced: bool,
    /// `# …` title of the note when it was loaded: if it changes, or if it
    /// appears, the file takes its name.
    pub(super) h1: Option<String>,
    /// Name of the note when it was loaded; `None` for a new note.
    /// If its title changes, links to that name follow when it is left.
    pub(super) origin: Option<String>,
    /// The note shown is only a preview: it counts as open only
    /// once you enter it (focus or keystroke).
    pub(super) preview: bool,
    /// Folder to save the new note in; the vault by default.
    pub(super) new_dir: Option<PathBuf>,
    /// The note was created in this pane: left empty, its file does not stay in the vault.
    pub(super) fresh: bool,
    pub(super) dirty: bool,
    /// The note shown as a kanban board, when it is one.
    pub(super) board: kanban::Board,
    /// List page shown in place of the note.
    pub(super) listing: Option<nav::Listing>,
    /// What is shown is only a preview (arrows in the tree, graph): the history
    /// ignores it until it is opened for good.
    pub(super) peek: bool,
}

/// A file of the vault shown in place of the note's text, which stays loaded underneath.
pub(super) struct FileView {
    pub(super) path: PathBuf,
    pub(super) kind: FileKind,
}

/// What shows the file.
#[derive(Clone)]
pub(super) enum FileKind {
    /// A picture, drawn as is.
    Image,
    /// A Bref diagram, open in its canvas.
    Drawing(Entity<Canvas>),
    /// An SVG from elsewhere, in the zoomable viewer.
    Svg(Entity<viewer::Viewer>),
    /// A CSV or TSV table, in its grid.
    Sheet(Entity<Sheet>),
}

impl Pane {
    /// Path of the file shown in place of the note.
    pub(super) fn file_path(&self) -> Option<&PathBuf> {
        self.file.as_ref().map(|file| &file.path)
    }

    pub(super) fn drawing(&self) -> Option<&Entity<Canvas>> {
        match &self.file {
            Some(FileView { kind: FileKind::Drawing(canvas), .. }) => Some(canvas),
            _ => None,
        }
    }

    pub(super) fn viewer(&self) -> Option<&Entity<viewer::Viewer>> {
        match &self.file {
            Some(FileView { kind: FileKind::Svg(viewer), .. }) => Some(viewer),
            _ => None,
        }
    }

    pub(super) fn sheet(&self) -> Option<&Entity<Sheet>> {
        match &self.file {
            Some(FileView { kind: FileKind::Sheet(sheet), .. }) => Some(sheet),
            _ => None,
        }
    }
}

impl Shell {
    /// An empty pane: a new note in its editor.
    pub(super) fn new_pane(theme: Theme, window: &mut Window, cx: &mut Context<Self>) -> Pane {
        let editor = cx.new(|cx| Editor::new(theme, cx));
        cx.subscribe_in(&editor, window, Self::on_editor_event).detach();
        Pane {
            zone: cx.focus_handle(),
            editor,
            file: None,
            figure: None,
            path: None,
            synced: true,
            h1: None,
            origin: None,
            preview: false,
            new_dir: None,
            fresh: false,
            dirty: false,
            board: kanban::Board::new(cx),
            listing: None,
            peek: false,
        }
    }

    /// The pane already showing this file, as a note (even hidden by an image) or in its place.
    pub(super) fn pane_showing(&self, path: &Path) -> Option<usize> {
        self.panes.iter().position(|pane| pane.path.as_deref() == Some(path) || pane.file_path().map(PathBuf::as_path) == Some(path))
    }

    /// Gives input to the pane: to its grid, to its list, to its note, otherwise to the pane itself.
    pub(super) fn focus_pane(&mut self, pane: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active = pane.min(self.panes.len() - 1);
        if let Some(figure) = &self.pane().figure {
            window.focus(&figure.focus_handle(cx));
            return cx.notify();
        }
        match (&self.pane().listing, self.pane().file.as_ref().map(|file| &file.kind)) {
            (_, Some(FileKind::Sheet(sheet))) => window.focus(&sheet.focus_handle(cx)),
            (Some(listing), _) => window.focus(&listing.filter.focus_handle(cx)),
            (None, Some(FileKind::Drawing(canvas))) => window.focus(&canvas.focus_handle(cx)),
            (None, Some(FileKind::Svg(viewer))) => window.focus(&viewer.focus_handle(cx)),
            (None, Some(FileKind::Image)) => window.focus(&self.pane().zone),
            (None, None) => window.focus(&self.pane().editor.focus_handle(cx)),
        }
        cx.notify();
    }

    /// Opens the second pane, on a new note, and gives it input.
    pub(super) fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Too narrow for two panes: the panel folds onto its rail.
        let width = self.pane_box.get().size.width;
        if self.nav.panel == Panel::Split && width > px(0.) && width < MIN_PANE * 2. {
            self.nav.panel = Panel::Rail;
            self.nav.save();
        }
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        let pane = Self::new_pane(self.theme, window, cx);
        self.panes.push(pane);
        self.active = self.panes.len() - 1;
        self.push_names(cx);
        self.push_dirs(cx);
        window.focus(&self.pane().editor.focus_handle(cx));
        cx.notify();
    }

    /// Ctrl+\: a second pane, opened on the palette to choose what goes there. A note is not
    /// shown twice, so it cannot take the one being left.
    pub(super) fn split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        if self.panes.len() > 1 {
            return self.focus_pane(1, window, cx);
        }
        self.add_pane(window, cx);
        self.open_palette("", window, cx);
    }

    /// Opens the file in the other pane, created if needed; already shown, it receives input.
    pub(super) fn open_aside(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.pane_showing(path) {
            return self.focus_pane(pane, window, cx);
        }
        match self.panes.len() {
            1 => self.add_pane(window, cx),
            _ => self.active = 1 - self.active,
        }
        self.open_from_nav(path, window, cx);
    }

    /// A file from the tree dropped on a pane: it opens there (a folder: its list page), or
    /// in a second pane if it is dropped on the right half of the only pane.
    pub(super) fn drop_on_pane(&mut self, pane: usize, aside: bool, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if path.is_dir() {
            match aside {
                true => self.add_pane(window, cx),
                false => self.active = pane.min(self.panes.len() - 1),
            }
            self.open_listing(path.to_path_buf(), window, cx);
        } else if aside {
            self.open_aside(path, window, cx);
        } else {
            self.active = pane.min(self.panes.len() - 1);
            self.open_from_nav(path, window, cx);
        }
    }

    /// Closes the pane that has input; the other takes the whole width. The last one stays.
    pub(super) fn close_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() < 2 {
            return;
        }
        self.leave(cx);
        self.panes.remove(self.active);
        self.focus_pane(0, window, cx);
    }

    /// The pane layout, as it is saved: the left pane's share, then the
    /// second pane's file (nothing if there is only one).
    pub(super) fn panes_state(&self) -> String {
        let aside = self.panes.get(1).and_then(|pane| pane.file_path().or(pane.path.as_ref()));
        format!("{}\n{}\n", self.pane_ratio, aside.map(|path| path.display().to_string()).unwrap_or_default())
    }

    /// Does `act` in pane `pane`, as if it were the active one, then goes back to the active one.
    pub(super) fn in_pane<R>(&mut self, pane: usize, act: impl FnOnce(&mut Self) -> R) -> R {
        let active = std::mem::replace(&mut self.active, pane);
        let done = act(self);
        // The pane may have been closed in the meantime.
        self.active = active.min(self.panes.len() - 1);
        done
    }

    /// Does `act` in each pane, in turn.
    pub(super) fn each_pane(&mut self, mut act: impl FnMut(&mut Self)) {
        for pane in 0..self.panes.len() {
            self.in_pane(pane, &mut act);
        }
    }

    /// The table shown, if there is one.
    pub(super) fn shown_sheet(&self) -> Option<Entity<Sheet>> {
        self.pane().sheet().cloned()
    }

    pub(super) fn flush_sheet(&mut self, cx: &mut Context<Self>) {
        if let Some(sheet) = self.pane().sheet() {
            sheet.update(cx, |sheet, cx| sheet.flush(cx));
        }
    }

    /// Shows `file` in the active pane in place of its note, or the note again (`None`). A table
    /// that is left keeps its place in the history and is saved first.
    pub(super) fn set_file(&mut self, file: Option<FileView>, cx: &mut Context<Self>) {
        let leaving = self.pane().file.as_ref().is_some_and(|old| file.as_ref().is_none_or(|new| new.path != old.path));
        if leaving && self.pane().sheet().is_some() {
            // What `track` would have kept on the next render, while the table still existed.
            if !self.pane().peek {
                self.mark_place(cx);
            }
            self.flush_sheet(cx);
        }
        self.pane_mut().file = file;
    }
}
