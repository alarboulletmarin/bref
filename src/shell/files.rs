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
    /// Ctrl+Maj+N : nouveau dossier dans l'arbre, à côté de la sélection.
    pub(super) fn new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        if self.nav.panel == Panel::Rail || self.nav.mode != Mode::Tree {
            self.show_nav(Mode::Tree, false, window, cx);
        }
        self.menu_do(Do::NewFolder, self.nav.sel.clone(), window, cx);
    }

    pub(super) fn fail(&mut self, what: &str, e: std::io::Error, cx: &mut Context<Self>) {
        self.say(Tone::Failed, format!("{what} : {e}"), cx);
    }

    /// Range dans le dossier `dir` tout ce qui a été glissé.
    pub(super) fn move_all(&mut self, from: &[PathBuf], dir: &Path, cx: &mut Context<Self>) {
        from.iter().for_each(|path| self.move_into(path, dir, cx));
        self.nav.marked.clear();
    }

    /// Range la note ou le dossier `from` dans le dossier `dir`.
    pub(super) fn move_into(&mut self, from: &Path, dir: &Path, cx: &mut Context<Self>) {
        // Déjà dans ce dossier, ou dossier déposé dans lui-même : rien à faire.
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
            Err(e) => self.fail(tr("Not moved", "Déplacement impossible"), e, cx),
        }
    }

    /// `from` est devenu `to` sur le disque : tout ce que l'app en sait suit.
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
        // Les icônes suivent leur note ou leur dossier, et ce qu'il contient.
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

    /// Met à la corbeille du coffre (`.trash`) la note ou le dossier.
    pub(super) fn trash(&mut self, root: &Path, path: &Path, cx: &mut Context<Self>) {
        self.flush(cx);
        if let Err(e) = vault::trash(root, path) {
            return self.fail(tr("Not moved to the trash", "Mise à la corbeille impossible"), e, cx);
        }
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

    /// Après un déplacement ou une suppression : config, complétion et graphe à jour.
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
