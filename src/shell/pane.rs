//! Panes and tabs: what each part of the window shows, which one is active, splitting and closing.
//!
//! A `Pane` is what one tab holds (a note, or a file in its place). `Shell::panes` lists every
//! tab of the window, shown or not, so what must reach every open note (saving, renames, the
//! watcher, the theme) loops over it. A `Group` is a pane as the user sees it: its tabs, in
//! order, and the one it shows.

use super::*;

/// Width below which two panes do not fit next to the panel: it folds.
pub(super) const MIN_PANE: Pixels = px(300.);

/// Share of the width the left pane can take.
pub(super) const PANE_RATIO: std::ops::RangeInclusive<f32> = 0.2..=0.8;

/// What one tab holds: a note and its editor, what is shown in its place (image, diagram,
/// table, list page), and the note's kanban view.
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

/// A pane of the window: its tabs, left to right (indices in `Shell::panes`), and the one it shows.
pub(super) struct Group {
    pub(super) tabs: Vec<usize>,
    pub(super) shown: usize,
    /// The tab bar, scrolled sideways when its tabs do not fit; `seen` is the tab last brought
    /// into view, so that it is scrolled to once and the wheel stays free afterwards.
    pub(super) scroll: gpui::ScrollHandle,
    pub(super) seen: Option<usize>,
}

impl Group {
    pub(super) fn of(tab: usize) -> Self {
        Group { tabs: vec![tab], shown: tab, scroll: Default::default(), seen: None }
    }
}

/// Tabs closed, most recent last, that « reopen » brings back; the oldest are forgotten.
pub(super) const CLOSED_KEPT: usize = 20;

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

/// The saved layout (file `panes`): the left pane's share, then each pane's tabs and the position
/// of the one it shows. Reads the two older formats too: a share then the second pane's file
/// (empty: one pane), where the first pane showed the most recent note.
pub(super) fn read_panes(text: &str) -> (Option<f32>, Vec<(Vec<PathBuf>, usize)>) {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    let ratio = |s: &str| s.trim().parse::<f32>().ok().filter(|ratio| PANE_RATIO.contains(ratio));
    let Some(share) = first.strip_prefix("v2 ") else {
        // Older: the first pane is left to the recent notes, the second is the line after.
        let aside = lines.next().map(str::trim).filter(|line| !line.is_empty());
        return (ratio(first), aside.map(|path| vec![(Vec::new(), 0), (vec![PathBuf::from(path)], 0)]).unwrap_or_default());
    };
    let groups = lines
        .map(|line| {
            let tabs: Vec<&str> = line.split('\t').filter(|tab| !tab.is_empty()).collect();
            let shown = tabs.iter().position(|tab| tab.starts_with('*')).unwrap_or(0);
            (tabs.iter().map(|tab| PathBuf::from(tab.trim_start_matches('*'))).collect(), shown)
        })
        .collect();
    (ratio(share), groups)
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
        self.show_active(cx);
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

    /// The active tab has a note under what it shows (a file opened alone in its tab has none).
    pub(super) fn has_note(&self, cx: &App) -> bool {
        self.pane().path.is_some() || !self.pane().editor.read(cx).text().is_empty()
    }

    /// Input is somewhere in tab `tab`: its note, its file, its list or its figure.
    pub(super) fn tab_has_focus(&self, tab: usize, window: &Window, cx: &App) -> bool {
        let pane = &self.panes[tab];
        let file = match pane.file.as_ref().map(|file| &file.kind) {
            Some(FileKind::Sheet(sheet)) => sheet.focus_handle(cx).is_focused(window),
            Some(FileKind::Drawing(canvas)) => canvas.focus_handle(cx).is_focused(window),
            Some(FileKind::Svg(viewer)) => viewer.focus_handle(cx).is_focused(window),
            _ => false,
        };
        file || pane.zone.is_focused(window)
            || pane.editor.focus_handle(cx).is_focused(window)
            || pane.figure.as_ref().is_some_and(|figure| figure.focus_handle(cx).is_focused(window))
            || pane.listing.as_ref().is_some_and(|listing| listing.filter.focus_handle(cx).is_focused(window))
    }

    /// The active tab becomes the one its pane shows (code that sets `active` itself, as the
    /// note already open elsewhere does, is followed at the next render).
    pub(super) fn show_active(&mut self, cx: &mut Context<Self>) {
        let group = self.group();
        self.show_tab(group, self.active, cx);
    }

    /// `group` shows `tab`; the tab it showed before lets go of what it laid out for the screen
    /// (`Editor::rest`), so that hidden tabs hold their text and little else. Its note is left: a
    /// title changed there renames the links to it now.
    pub(super) fn show_tab(&mut self, group: usize, tab: usize, cx: &mut Context<Self>) {
        let was = std::mem::replace(&mut self.groups[group].shown, tab);
        if was != tab && was < self.panes.len() {
            self.panes[was].editor.update(cx, |editor, _| editor.rest());
            self.in_pane(was, |this| this.follow_title(cx));
        }
    }

    /// The group the tab belongs to.
    pub(super) fn group_of(&self, tab: usize) -> usize {
        self.groups.iter().position(|group| group.tabs.contains(&tab)).unwrap_or(0)
    }

    /// The group of the active tab.
    pub(super) fn group(&self) -> usize {
        self.group_of(self.active)
    }

    /// Gives input to the tab group `group` shows.
    pub(super) fn focus_group(&mut self, group: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(shown) = self.groups.get(group).map(|group| group.shown) {
            self.focus_pane(shown, window, cx);
        }
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
        self.groups.push(Group::of(self.active));
        self.push_names_to(self.active..self.active + 1, cx);
        self.push_dirs(cx);
        window.focus(&self.pane().editor.focus_handle(cx));
        cx.notify();
    }

    /// A new tab in the active pane, right of the one shown, on a new note; it gets input.
    pub(super) fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let group = self.group();
        let pane = Self::new_pane(self.theme, window, cx);
        self.panes.push(pane);
        let tab = self.panes.len() - 1;
        let at = self.groups[group].tabs.iter().position(|t| *t == self.active).map_or(0, |at| at + 1);
        self.groups[group].tabs.insert(at, tab);
        self.show_tab(group, tab, cx);
        self.active = tab;
        self.push_names_to(self.active..self.active + 1, cx);
        self.push_dirs(cx);
        cx.notify();
    }

    /// The active tab holds nothing worth keeping (a preview, or a new note left or made empty):
    /// what opens next takes its place instead of a tab of its own, and an emptied note's file goes.
    pub(super) fn spare_tab(&self, cx: &App) -> bool {
        let pane = self.pane();
        pane.preview || (pane.path.is_none() || pane.fresh) && pane.file.is_none() && pane.listing.is_none() && pane.editor.read(cx).text().is_empty()
    }

    /// A tab for what is about to open: the active one if it is spare, otherwise a new one.
    pub(super) fn fresh_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.spare_tab(cx) {
            self.add_tab(window, cx);
        }
    }

    /// Opens a file the way an editor does: the tab that already shows it comes to the front,
    /// otherwise it opens in a tab of its own, right of the shown one; input goes there.
    pub(super) fn open(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match self.pane_showing(path) {
            Some(tab) => {
                self.active = tab;
                // The note under a picture or a list page comes back to the front.
                if self.pane().path.as_deref() == Some(path) {
                    self.set_file(None, cx);
                    self.pane_mut().listing = None;
                }
                // Coming back to it counts as an opening: the recent notes say so.
                (self.pane_mut().peek, self.pane_mut().preview) = (false, false);
                self.touch(path);
            }
            None => {
                let spare = self.spare_tab(cx);
                if !spare {
                    self.add_tab(window, cx);
                }
                self.open_note(path, cx);
                // Nothing to show (an attachment, opened by the system; an unreadable file): the
                // new tab goes away.
                if !spare && self.pane().path.is_none() && self.pane().file.is_none() {
                    return self.close_tab(self.active, window, cx);
                }
            }
        }
        self.focus_pane(self.active, window, cx);
    }

    /// The next (or previous) tab of the active pane, round the end.
    pub(super) fn cycle_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = &self.groups[self.group()].tabs;
        let Some(at) = tabs.iter().position(|t| *t == self.active) else { return };
        let next = if forward { (at + 1) % tabs.len() } else { (at + tabs.len() - 1) % tabs.len() };
        let tab = tabs[next];
        self.focus_pane(tab, window, cx);
    }

    /// Moves the active tab one place left or right in its bar.
    pub(super) fn move_tab(&mut self, right: bool, cx: &mut Context<Self>) {
        let group = self.group();
        let tabs = &mut self.groups[group].tabs;
        let Some(at) = tabs.iter().position(|t| *t == self.active) else { return };
        let to = if right { at + 1 } else { at.wrapping_sub(1) };
        if to < tabs.len() {
            tabs.swap(at, to);
            cx.notify();
        }
    }

    /// Takes the tab out of the window: the indices after it shift down, a group left without a
    /// tab goes away, and a group that showed it shows its neighbour (the next one, else the last).
    pub(super) fn remove_tab(&mut self, tab: usize) {
        let (group, was_active) = (self.group_of(tab), self.active == tab);
        self.panes.remove(tab);
        let fix = |t: usize| if t > tab { t - 1 } else { t };
        for each in &mut self.groups {
            if let Some(at) = each.tabs.iter().position(|t| *t == tab) {
                each.tabs.remove(at);
                if each.shown == tab && !each.tabs.is_empty() {
                    each.shown = each.tabs[at.min(each.tabs.len() - 1)];
                }
            }
            each.tabs.iter_mut().for_each(|t| *t = fix(*t));
            each.shown = fix(each.shown);
            each.seen = None;
        }
        self.groups.retain(|each| !each.tabs.is_empty());
        self.active = match was_active {
            // Its group shows the neighbour now; gone with it, the pane that took its place does.
            true => self.groups[group.min(self.groups.len() - 1)].shown,
            false => fix(self.active),
        };
    }

    /// Ctrl+\: a second pane, opened on the palette to choose what goes there. A note is not
    /// shown twice, so it cannot take the one being left.
    pub(super) fn split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        if self.groups.len() > 1 {
            return self.focus_group(1, window, cx);
        }
        self.add_pane(window, cx);
        self.open_palette("", window, cx);
    }

    /// Opens the file in the other pane, created if needed; already shown, it receives input.
    pub(super) fn open_aside(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.pane_showing(path) {
            return self.focus_pane(pane, window, cx);
        }
        match self.groups.len() {
            1 => self.add_pane(window, cx),
            _ => self.active = self.groups[1 - self.group()].shown,
        }
        self.open_from_nav(path, window, cx);
    }

    /// A file from the tree dropped on a pane: it opens there (a folder: its list page), or
    /// in a second pane if it is dropped on the right half of the only pane.
    pub(super) fn drop_on_pane(&mut self, group: usize, aside: bool, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.groups.get(group).map(|group| group.shown) else { return };
        if path.is_dir() {
            match aside {
                true => self.add_pane(window, cx),
                false => self.active = shown,
            }
            self.open_listing(path.to_path_buf(), window, cx);
        } else if aside {
            self.open_aside(path, window, cx);
        } else {
            self.active = shown;
            self.open_from_nav(path, window, cx);
        }
    }

    /// Ctrl+W: closes the tab that has input, what it holds saved first; the pane goes with
    /// its last tab, and the other takes the whole width. The last tab of the last pane stays.
    pub(super) fn close_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() < 2 {
            return;
        }
        self.leave(cx);
        if let Some(path) = self.pane().file_path().or(self.pane().path.as_ref()).cloned() {
            self.closed.retain(|p| *p != path);
            self.closed.push(path);
            if self.closed.len() > CLOSED_KEPT {
                self.closed.remove(0);
            }
        }
        self.remove_tab(self.active);
        self.focus_pane(self.active, window, cx);
    }

    /// Ctrl+Alt+T: the last closed tab comes back, in a new tab of the active pane; what was
    /// deleted or moved since is skipped.
    pub(super) fn reopen_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(path) = self.closed.pop() {
            if path.exists() {
                return self.open_from_nav(&path, window, cx);
            }
        }
    }

    /// Opens the tabs saved last time (`read_panes`), those whose file is still in the vault; a
    /// first pane left without any shows the most recent note, as before tabs.
    pub(super) fn restore_panes(&mut self, saved: Vec<(Vec<PathBuf>, usize)>, root: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let mut seen = std::collections::HashSet::new();
        let mut groups: Vec<(Vec<PathBuf>, usize)> = saved
            .into_iter()
            .map(|(tabs, shown)| {
                let want = tabs.get(shown).cloned();
                // A file open twice (edited by hand) keeps its first tab: a note is open once.
                let tabs: Vec<PathBuf> = tabs.into_iter().filter(|p| p.is_file() && p.starts_with(root) && seen.insert(p.clone())).collect();
                let shown = want.and_then(|want| tabs.iter().position(|p| *p == want)).unwrap_or(0);
                (tabs, shown)
            })
            .collect();
        if groups.is_empty() {
            groups.push((Vec::new(), 0));
        }
        if groups[0].0.is_empty() {
            groups[0].0.extend(self.recent.iter().find(|p| !seen.contains(*p)).cloned());
        }
        for (n, (tabs, shown)) in groups.into_iter().enumerate() {
            if n > 0 {
                if tabs.is_empty() {
                    continue;
                }
                self.add_pane(window, cx);
            }
            let mut opened = Vec::new();
            for (i, path) in tabs.iter().enumerate() {
                if i > 0 {
                    self.add_tab(window, cx);
                }
                self.open_note(path, cx);
                opened.push(self.active);
            }
            if let Some(&tab) = opened.get(shown) {
                self.active = tab;
                self.show_active(cx);
            }
        }
        self.focus_group(0, window, cx);
    }

    /// The pane layout, as it is saved: a version line with the left pane's share, then one line
    /// per pane, its tabs separated by tabulations, the shown one marked with `*`. New notes not
    /// saved yet have no file and are left out.
    pub(super) fn panes_state(&self) -> String {
        let mut state = format!("v2 {}\n", self.pane_ratio);
        for group in &self.groups {
            let tabs: Vec<String> = group
                .tabs
                .iter()
                .filter_map(|&tab| {
                    let pane = &self.panes[tab];
                    let path = pane.file_path().or(pane.path.as_ref())?;
                    Some(format!("{}{}", if tab == group.shown { "*" } else { "" }, path.display()))
                })
                .collect();
            state.push_str(&tabs.join("\t"));
            state.push('\n');
        }
        state
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_panes_read_in_every_format() {
        assert_eq!(read_panes(""), (None, vec![]));
        assert_eq!(read_panes("0.4\n\n"), (Some(0.4), vec![]));
        assert_eq!(read_panes("0.4\n/v/B.md\n"), (Some(0.4), vec![(vec![], 0), (vec![PathBuf::from("/v/B.md")], 0)]));
        // A share out of range is dropped, the tabs stay.
        assert_eq!(read_panes("v2 7\n/v/A.md\t*/v/B c.md\n*/v/D.md\n"), (None, vec![(vec![PathBuf::from("/v/A.md"), PathBuf::from("/v/B c.md")], 1), (vec![PathBuf::from("/v/D.md")], 0)]));
        // No mark: the first tab is shown; an empty line is a pane whose notes were all new.
        assert_eq!(read_panes("v2 0.5\n/v/A.md\n\n"), (Some(0.5), vec![(vec![PathBuf::from("/v/A.md")], 0), (vec![], 0)]));
    }
}
