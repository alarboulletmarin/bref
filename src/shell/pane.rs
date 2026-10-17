//! Panes: what each half of the window shows, which one is active, splitting and closing.

use super::*;

/// Largeur en dessous de laquelle deux panes ne tiennent pas à côté du panneau : il se replie.
pub(super) const MIN_PANE: Pixels = px(300.);

/// Part de la largeur que peut prendre le pane de gauche.
pub(super) const PANE_RATIO: std::ops::RangeInclusive<f32> = 0.2..=0.8;

/// Ce qui est ouvert dans une moitié de la fenêtre : une note et son éditeur, ce qui s'affiche
/// à sa place (image, schéma, tableau, page liste), et la vue kanban de la note.
pub(super) struct Pane {
    /// Contient le focus dès qu'une des vues du pane l'a : c'est ce qui le rend actif. Il le
    /// porte lui-même quand le pane ne montre qu'une image.
    pub(super) zone: FocusHandle,
    pub(super) editor: Entity<Editor>,
    /// File shown in place of the note, picked in the tree or the graph (`Shell::set_file`).
    pub(super) file: Option<FileView>,
    /// Diagramme Mermaid de la note, vu en grand à sa place.
    pub(super) figure: Option<Entity<viewer::Viewer>>,
    /// Fichier de la note ouverte ; `None` tant qu'une nouvelle note n'est pas enregistrée.
    pub(super) path: Option<PathBuf>,
    /// Le nom du fichier suit le titre (première ligne) de la note.
    pub(super) synced: bool,
    /// Titre `# …` de la note quand elle a été chargée : s'il change, ou s'il
    /// apparaît, le fichier prend son nom.
    pub(super) h1: Option<String>,
    /// Nom de la note quand elle a été chargée ; `None` pour une nouvelle note.
    /// Si son titre change, les liens vers ce nom suivent quand on la quitte.
    pub(super) origin: Option<String>,
    /// La note affichée n'est qu'un aperçu : elle ne compte comme ouverte que
    /// lorsqu'on y entre (focus ou frappe).
    pub(super) preview: bool,
    /// Dossier où enregistrer la nouvelle note ; le coffre par défaut.
    pub(super) new_dir: Option<PathBuf>,
    /// La note a été créée dans ce pane : laissée vide, son fichier ne reste pas dans le coffre.
    pub(super) fresh: bool,
    pub(super) dirty: bool,
    /// The note shown as a kanban board, when it is one.
    pub(super) board: kanban::Board,
    /// Page liste affichée à la place de la note.
    pub(super) listing: Option<nav::Listing>,
    /// Ce qui est affiché n'est qu'un aperçu (flèches dans l'arbre, graphe) : l'historique
    /// l'ignore tant qu'il n'est pas ouvert pour de bon.
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
    /// Un pane vide : une note neuve dans son éditeur.
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

    /// Le pane qui montre déjà ce fichier, comme note (même cachée par une image) ou à sa place.
    pub(super) fn pane_showing(&self, path: &Path) -> Option<usize> {
        self.panes.iter().position(|pane| pane.path.as_deref() == Some(path) || pane.file_path().map(PathBuf::as_path) == Some(path))
    }

    /// Donne la saisie au pane : à sa grille, à sa liste, à sa note, sinon au pane lui-même.
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

    /// Ouvre le second pane, sur une note neuve, et lui donne la saisie.
    pub(super) fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Trop étroit pour deux panes : le panneau se replie sur son rail.
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

    /// Ctrl+\ : un second pane, ouvert sur la palette pour choisir ce qui y va. Une note ne
    /// s'affiche pas deux fois, il ne peut donc pas reprendre celle qu'on quitte.
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

    /// Ouvre le fichier dans l'autre pane, créé au besoin ; déjà affiché, il reçoit la saisie.
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

    /// Un fichier de l'arbre lâché sur un pane : il s'y ouvre (un dossier : sa page liste), ou
    /// dans un second pane s'il est lâché sur la moitié droite du seul pane.
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

    /// Ferme le pane qui a la saisie ; l'autre reprend toute la largeur. Le dernier reste.
    pub(super) fn close_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() < 2 {
            return;
        }
        self.leave(cx);
        self.panes.remove(self.active);
        self.focus_pane(0, window, cx);
    }

    /// La disposition des panes, telle qu'elle s'enregistre : la part du pane de gauche, puis le
    /// fichier du second pane (rien s'il n'y en a qu'un).
    pub(super) fn panes_state(&self) -> String {
        let aside = self.panes.get(1).and_then(|pane| pane.file_path().or(pane.path.as_ref()));
        format!("{}\n{}\n", self.pane_ratio, aside.map(|path| path.display().to_string()).unwrap_or_default())
    }

    /// Fait `act` dans le pane `pane`, comme s'il était l'actif, puis revient à l'actif.
    pub(super) fn in_pane<R>(&mut self, pane: usize, act: impl FnOnce(&mut Self) -> R) -> R {
        let active = std::mem::replace(&mut self.active, pane);
        let done = act(self);
        // Le pane a pu être fermé entre-temps.
        self.active = active.min(self.panes.len() - 1);
        done
    }

    /// Fait `act` dans chaque pane, tour à tour.
    pub(super) fn each_pane(&mut self, mut act: impl FnMut(&mut Self)) {
        for pane in 0..self.panes.len() {
            self.in_pane(pane, &mut act);
        }
    }

    /// Le tableau affiché, s'il y en a un.
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
            // Ce que `track` aurait retenu au rendu suivant, tant que le tableau existait encore.
            if !self.pane().peek {
                self.mark_place(cx);
            }
            self.flush_sheet(cx);
        }
        self.pane_mut().file = file;
    }
}
