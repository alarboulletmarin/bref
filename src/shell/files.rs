//! File operations from the side panel: new folder, move, rename, trash, and what follows on disk.

use std::path::{Path, PathBuf};
use gpui::{Context, Window};
use crate::{
    lang::tr,
    platform::vault,
};
use super::*;
use crate::views::nav::*;

impl Shell {
    /// Ctrl+Shift+N: new folder in the tree, next to the selection.
    pub(super) fn new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        if self.nav.panel == Panel::Rail || self.nav.mode != Mode::Tree {
            self.show_nav(Mode::Tree, false, window, cx);
        }
        self.menu_do(Do::NewFolder, self.nav.sel.clone(), window, cx);
    }

    /// A disk operation on `path` failed: what was tried, on which file, and why.
    pub(super) fn fail(&mut self, what: &str, path: &Path, e: std::io::Error, cx: &mut Context<Self>) {
        let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
        self.say(Tone::Failed, format!("{what} « {name} » : {}", crate::lang::io_reason(&e)), cx);
    }

    /// Moves everything that was dragged into the folder `dir`.
    pub(super) fn move_all(&mut self, from: &[PathBuf], dir: &Path, cx: &mut Context<Self>) {
        from.iter().for_each(|path| self.move_into(path, dir, cx));
        self.nav.marked.clear();
    }

    /// Moves the note or folder `from` into the folder `dir`.
    pub(super) fn move_into(&mut self, from: &Path, dir: &Path, cx: &mut Context<Self>) {
        // Already in this folder, or folder dropped into itself: nothing to do.
        if from.parent() == Some(dir) || dir.starts_with(from) {
            return;
        }
        let Some(name) = from.file_name() else {
            return;
        };
        let to = dir.join(name);
        self.flush(cx);
        match vault::rename(from, &to) {
            Ok(()) => {
                self.nav.open.insert(dir.to_path_buf());
                self.relocate(from, &to, false, cx);
            }
            Err(e) => self.fail(tr("Not moved", "Déplacement impossible"), from, e, cx),
        }
    }

    /// `from` became `to` on disk: everything the app knows about it follows.
    pub(super) fn relocate(&mut self, from: &Path, to: &Path, reload: bool, cx: &mut Context<Self>) {
        self.history.rename(from, to);
        let shift = |p: &PathBuf| match p.strip_prefix(from) {
            Ok(rest) if rest.as_os_str().is_empty() => to.to_path_buf(),
            Ok(rest) => to.join(rest),
            Err(_) => p.clone(),
        };
        for note in &mut self.notes {
            note.path = shift(&note.path);
            note.name = vault::stem(&note.path);
        }
        self.dirs = self.dirs.iter().map(shift).collect();
        self.images = self.images.iter().map(shift).collect();
        self.recent = self.recent.iter().map(shift).collect();
        // Icons follow their note or folder, and what it contains.
        if self.icons.keys().any(|p| p.starts_with(from)) {
            self.icons = self.icons.drain().map(|(path, icon)| (shift(&path), icon)).collect();
            self.save_icons(cx);
        }
        self.nav.open = self.nav.open.iter().map(shift).collect();
        self.each_pane(|this| {
            if let Some(file) = &mut this.pane_mut().file {
                file.path = shift(&file.path);
                if let super::FileKind::Sheet(sheet) = &file.kind {
                    let moved = file.path.clone();
                    sheet.update(cx, |sheet, _| sheet.moved(moved));
                }
            }
            this.pane_mut().new_dir = this.pane_mut().new_dir.as_ref().map(shift);
            this.pane_mut().path = this.pane_mut().path.as_ref().map(shift);
            if reload && this.pane().path.as_deref() == Some(to) {
                this.reload(cx);
            }
        });
        self.nav.reveal(to);
        self.files_changed(cx);
    }

    /// Moves the note or folder to the vault's trash (`.trash`).
    pub(super) fn trash(&mut self, root: &Path, path: &Path, cx: &mut Context<Self>) {
        self.flush(cx);
        let trashed = match vault::trash(root, path) {
            Ok(trashed) => trashed,
            Err(e) => return self.fail(tr("Not moved to the trash", "Mise à la corbeille impossible"), path, e, cx),
        };
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        self.say_undoable(Tone::Done, format!("{} « {name} »", tr("Moved to the trash:", "Mis à la corbeille :")), Some((trashed, path.to_path_buf())), cx);
        self.history.remove(path);
        let gone = |p: &PathBuf| p.starts_with(path);
        self.notes.retain(|n| !gone(&n.path));
        self.dirs.retain(|d| !gone(d));
        self.images.retain(|p| !gone(p));
        self.recent.retain(|p| !gone(p));
        self.nav.sel = None;
        self.each_pane(|this| {
            if this.pane().file_path().is_some_and(gone) {
                this.set_file(None, cx);
            }
            if this.pane().path.as_ref().is_some_and(gone) {
                this.pane_mut().path = None;
                this.new_note(String::new(), cx);
            }
        });
        self.files_changed(cx);
    }

    /// « Undo » of the trash: `from` (in the trash) goes back to `to`, in its folder, which is made again if it is gone. The
    /// watcher finds it; nothing is overwritten (`vault::rename` refuses a name that is taken).
    pub(super) fn put_back(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        let made = to.parent().map_or(Ok(()), std::fs::create_dir_all);
        match made.and_then(|()| vault::rename(from, to)) {
            Ok(()) => {
                self.say(Tone::Done, format!("{} « {} »", tr("Put back:", "Remis en place :"), to.file_name().unwrap_or_default().to_string_lossy()), cx);
                self.files_changed(cx);
            }
            Err(e) => self.fail(tr("Not put back", "Remise en place impossible"), to, e, cx),
        }
    }

    /// After a move or a deletion: config, completion and graph up to date.
    pub(super) fn files_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = &self.vault {
            vault::save_config(root, &self.recent);
        }
        self.calm();
        self.push_names(cx);
        self.graph_stale = true;
        self.refresh_graph(cx);
        cx.notify();
    }
}
