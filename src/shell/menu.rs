//! The context menu of the tree and the note, and what each entry does.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use gpui::{
    Action, Animation, AnimationExt, ClipboardItem, Context, Focusable, MouseButton,
    MouseDownEvent, Window, div, ease_out_quint, prelude::*, px,
};
use crate::{
    views::editor, views::graph,
    logic::markdown::Link,
    lang::tr,
    platform::vault,
};
use super::*;
use crate::ui::assets::ICON_SET;
use crate::views::nav::*;

impl Shell {
    /// Runs a context menu entry on `target` (`None`: the vault).
    pub(super) fn menu_do(&mut self, what: Do, target: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let menu = self.menu.take();
        if let Some(back) = menu.as_ref().and_then(|menu| menu.back.as_ref()) {
            window.focus(back);
        }
        // Without a menu (from the palette), the icon grid opens near the top of the note.
        let at = menu.as_ref().map_or(point(px(80.), px(80.)), |menu| menu.at);
        let link = menu.and_then(|menu| menu.text).flatten();
        cx.notify();
        if let (Do::Icon, Some(path)) = (what, &target) {
            // On the item's icon if it has one, else on "no icon".
            let chosen = self.icons.get(path).and_then(|mine| ICON_SET.iter().position(|(file, _)| file[2..file.len() - 4] == *mine)).map_or(0, |i| i + 1);
            self.icon_pick = Some(IconPick { at, path: path.clone(), chosen, back: window.focused(cx) });
            window.focus(&self.menu_focus);
            return;
        }
        // Note menu: the editor's gestures, as with the keyboard, and the targeted link.
        let action: Option<Box<dyn Action>> = match (what, link) {
            (Do::Cut, _) => Some(Box::new(editor::Cut)),
            (Do::Copy, _) => Some(Box::new(editor::Copy)),
            (Do::Paste, _) => Some(Box::new(editor::Paste)),
            (Do::Bold, _) => Some(Box::new(editor::Bold)),
            (Do::Italic, _) => Some(Box::new(editor::Italic)),
            (Do::Underline, _) => Some(Box::new(editor::Underline)),
            (Do::Strike, _) => Some(Box::new(editor::Strike)),
            (Do::Highlight, _) => Some(Box::new(editor::Highlight)),
            (Do::Code, _) => Some(Box::new(editor::InlineCode)),
            (Do::Link, _) => Some(Box::new(editor::InsertLink)),
            (Do::Comment, _) => Some(Box::new(editor::Comment)),
            (Do::Bullets, _) => Some(Box::new(editor::ListBullet)),
            (Do::Numbered, _) => Some(Box::new(editor::ListOrdered)),
            (Do::Tasks, _) => Some(Box::new(editor::ListTask)),
            (Do::Plain, _) => Some(Box::new(editor::ListText)),
            (Do::Table(op), _) => Some(op.action()),
            (Do::Block(op), _) => Some(op.action()),
            (Do::OpenLink, Some(link)) => return self.follow(&link, window, cx),
            (Do::CopyAddress, Some(Link::Url(url))) => return cx.write_to_clipboard(ClipboardItem::new_string(url)),
            _ => None,
        };
        if let Some(action) = action {
            window.focus(&self.pane().editor.focus_handle(cx));
            return window.dispatch_action(action, cx);
        }
        let Some(root) = self.vault.clone() else {
            return;
        };
        if let Do::List = what {
            return self.open_listing(target.unwrap_or(root), window, cx);
        }
        // A tag is neither a note nor a folder: nothing to rename or trash.
        if target.as_ref().is_some_and(|t| !t.starts_with(&root)) {
            return;
        }
        let is_dir = target.as_ref().is_none_or(|t| self.dirs.contains(t));
        // Targeted folder: the target itself, or the one containing the note.
        let dir = match &target {
            Some(t) if is_dir => t.clone(),
            Some(t) => t.parent().map_or(root.clone(), Path::to_path_buf),
            None => root.clone(),
        };
        // Trash, copies and duplication apply to the whole multiple selection.
        let all = target.as_deref().map_or_else(|| vec![root.clone()], |t| self.targets(t));
        let copy = |lines: Vec<String>, cx: &mut Context<Self>| {
            cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
        };
        match (what, target) {
            (Do::MoveTo, Some(_)) => self.ask_move(all, window, cx),
            (Do::Open, Some(path)) => self.open_from_nav(&path, window, cx),
            (Do::OpenAside, Some(path)) => self.open_aside(&path, window, cx),
            (Do::NewNote, _) => {
                self.nav.open.insert(dir.clone());
                self.nav.sel = Some(dir);
                self.new_note_here(window, cx);
            }
            (Do::FromTemplate, _) => {
                self.nav.open.insert(dir.clone());
                self.nav.sel = Some(dir);
                self.open_templates(crate::views::palette::TemplateAct::New, window, cx);
            }
            (Do::NewDiagram, _) => {
                self.nav.open.insert(dir.clone());
                self.nav.sel = Some(dir);
                self.new_diagram(window, cx);
            }
            (Do::NewFolder, _) => {
                // The folder is created in the selection's: the field says so.
                let label = match dir.file_name().filter(|_| dir != root) {
                    Some(name) => format!("{} « {} »", tr("Folder name, in", "Nom du dossier, dans"), name.to_string_lossy()),
                    None => tr("Folder name", "Nom du dossier").to_string(),
                };
                self.ask(label, "", window, cx, move |this, name, _, cx| {
                    let new = dir.join(name);
                    match fs::create_dir(&new) {
                        Ok(()) => {
                            this.nav.open.insert(dir.clone());
                            this.dirs.push(new.clone());
                            this.nav.reveal(&new);
                        }
                        Err(e) => this.fail(tr("Folder not created", "Dossier non créé"), &new, e, cx),
                    }
                });
            }
            (Do::Rename, Some(path)) => {
                let label = tr("Rename: new name", "Renommer : nouveau nom");
                let current = if is_dir { path.file_name().unwrap_or_default().to_string_lossy().into_owned() } else { vault::stem(&path) };
                self.ask(label, &current.clone(), window, cx, move |this, name, _, cx| {
                    if name == current {
                        return;
                    }
                    this.flush(cx);
                    let image = vault::is_image(&path) || vault::is_attachment(&path);
                    let renamed = if is_dir {
                        let to = path.with_file_name(&name);
                        vault::rename(&path, &to).map(|()| to)
                    } else if image || vault::is_table(&path) {
                        // An image or a table keeps its extension.
                        let extension = path.extension().unwrap_or_default().to_string_lossy();
                        let to = path.with_file_name(format!("{name}.{extension}"));
                        vault::rename(&path, &to).map(|()| to)
                    } else {
                        vault::rename_note(&path, &name)
                    };
                    match renamed {
                        Ok(to) => {
                            // The title of a renamed note may have changed: reread it.
                            this.relocate(&path, &to, !is_dir, cx);
                            // Links follow: by note name, or by file name for an image.
                            let (old, new) = if image {
                                (graph::image_name(&path), graph::image_name(&to))
                            } else {
                                (current.clone(), name)
                            };
                            if !is_dir && this.relink(&old, &new, cx) {
                                this.reload(cx);
                            }
                        }
                        Err(e) => this.fail(tr("Not renamed", "Renommage impossible"), &path, e, cx),
                    }
                });
            }
            (Do::Trash, Some(_)) => {
                all.iter().for_each(|path| self.trash(&root, path, cx));
                self.nav.marked.clear();
            }
            (Do::CopyLink, Some(_)) => {
                let files = all.iter().filter(|p| !self.dirs.contains(p));
                let link = |path: &PathBuf| match (vault::is_image(path), vault::is_table(path)) {
                    (true, _) => format!("![[{}]]", graph::image_name(path)),
                    // A table has no link: its file name.
                    (_, true) => graph::image_name(path),
                    // An attachment is targeted by its file name, extension included.
                    _ if vault::is_attachment(path) => format!("[[{}]]", graph::image_name(path)),
                    _ => format!("[[{}]]", vault::stem(path)),
                };
                copy(files.map(link).collect(), cx);
            }
            (Do::CopyPath, _) => copy(all.iter().map(|p| p.display().to_string()).collect(), cx),
            (Do::CopyRelative, _) => {
                let relative = |p: &PathBuf| p.strip_prefix(&root).unwrap_or(p).display().to_string();
                copy(all.iter().map(relative).collect(), cx);
            }
            (Do::Reveal, _) => cx.reveal_path(&all[0]),
            // ponytail: only files are duplicated; copying a whole folder
            // needs a recursive walk, to write if the need arises.
            (Do::Duplicate, Some(_)) => {
                // The copy starts from what is on screen, not from a lagging file.
                self.flush(cx);
                let mut last = None;
                for path in all.iter().filter(|p| p.is_file()) {
                    let extension = path.extension().unwrap_or_default().to_string_lossy();
                    let twin = vault::free_path(path.parent().unwrap_or(&root), &vault::stem(path), &extension);
                    match fs::copy(path, &twin) {
                        Ok(_) => last = Some(twin),
                        Err(e) => return self.fail(tr("Not duplicated", "Duplication impossible"), path, e, cx),
                    }
                }
                if let Some(twin) = last {
                    let (notes, dirs, images) = vault::rescan(&root, &self.notes);
                    self.sync(notes, dirs, images, cx);
                    self.nav.marked.clear();
                    self.nav.reveal(&twin);
                }
            }
            _ => {}
        }
    }

    /// A row menu's entry on the file of the active pane; "Show as a list" lists its folder.
    pub(super) fn file_do(&mut self, what: Do, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.pane().path.clone() else { return };
        let target = if matches!(what, Do::List) { path.parent().map(Path::to_path_buf) } else { Some(path) };
        self.menu_do(what, target, window, cx)
    }

    /// The folders of the vault in a list, the vault first; the one chosen receives `items`. A folder is
    /// not offered for itself, nor what is inside it.
    fn ask_move(&mut self, items: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else { return };
        let mut dirs: Vec<PathBuf> = self.dirs.iter().filter(|d| !items.iter().any(|item| d.starts_with(item))).cloned().collect();
        dirs.sort();
        dirs.insert(0, root.clone());
        let names = dirs.iter().map(|d| match d.strip_prefix(&root) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
            _ => format!("{} ({})", vault::shown(&root), tr("the vault", "le coffre")),
        }).collect();
        let back = window.focused(cx);
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(tr("Move to", "Déplacer vers"), names, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            if matches!(event, PaletteEvent::Preview(_)) {
                return;
            }
            let chosen = palette.read(cx).chosen().and_then(|i| dirs.get(i)).cloned().filter(|_| matches!(event, PaletteEvent::Submit(_)));
            this.palette = None;
            if let Some(back) = &back {
                window.focus(back);
            }
            if let Some(dir) = chosen {
                this.move_all(&items, &dir, cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Opens `menu`; it takes the keys until it closes.
    pub(super) fn show_menu(&mut self, menu: Menu, window: &mut Window, cx: &mut Context<Self>) {
        let back = self.menu.take().and_then(|old| old.back).or_else(|| window.focused(cx));
        self.menu = Some(Menu { back, ..menu });
        window.focus(&self.menu_focus);
        cx.notify();
    }

    /// Closes the menu, if one is open, and gives the focus back to what had it.
    pub(super) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(back) = self.menu.take().and_then(|menu| menu.back) {
            window.focus(&back);
        }
        cx.notify();
    }

    /// Shift+F10 or the Menu key: the menu of the selected row in the panel, or of the cursor in the note.
    pub(super) fn menu_from_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav.focus.is_focused(window) {
            let Some(target) = self.nav.sel.clone() else { return };
            let at = self.nav.sel_at.get().map_or(point(px(60.), px(60.)), |row| row.bottom_left() + point(px(24.), px(0.)));
            if self.vault.as_ref().is_some_and(|root| target.starts_with(root)) || tag_of(&target).is_some() {
                self.show_menu(Menu { at, target: Some(target), chosen: Some(0), ..Default::default() }, window, cx);
            }
        } else if self.pane().editor.focus_handle(cx).is_focused(window) {
            self.pane().editor.clone().update(cx, |editor, cx| editor.menu_at_cursor(cx));
        }
    }

    /// Arrows, Home, End, Enter and Escape in the open menu.
    fn menu_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let entries: Vec<Do> = self.menu_groups(cx).into_iter().flatten().map(|(_, what)| what).collect();
        let (Some(menu), n) = (self.menu.as_mut(), entries.len()) else { return false };
        let at = menu.chosen;
        menu.chosen = match key {
            "down" => Some(at.map_or(0, |i| (i + 1) % n)),
            "up" => Some(at.map_or(n - 1, |i| (i + n - 1) % n)),
            "home" | "pageup" => Some(0),
            "end" | "pagedown" => Some(n - 1),
            "enter" | "space" => {
                if let Some(what) = at.and_then(|i| entries.get(i)) {
                    let target = menu.target.clone();
                    self.menu_do(*what, target, window, cx);
                }
                return true;
            }
            "escape" => {
                self.close_menu(window, cx);
                return true;
            }
            _ => return false,
        };
        cx.notify();
        true
    }

    /// The entries of the open menu, by group; a line separates the groups.
    pub(super) fn menu_groups(&self, cx: &App) -> Vec<Vec<(&'static str, Do)>> {
        let Some(menu) = self.menu.as_ref() else { return Vec::new() };
        let is_dir = menu.target.as_ref().is_none_or(|p| self.dirs.contains(p));
        let tree = self.nav.mode == Mode::Tree;
        let many = menu.target.as_ref().is_some_and(|p| self.nav.marked.contains(p)) && self.nav.marked.len() > 1;
        // Groups of entries; a line separates those that have any.
        let mut groups: Vec<Vec<(&'static str, Do)>> = vec![Vec::new(); 7];
        if menu.block {
            // A block's handle: what it can become, move or duplicate it, copy it, delete it.
            use editor::BlockOp;
            groups[0].extend(BlockOp::ALL.into_iter().filter(|op| matches!(op, BlockOp::Turn(_))).map(|op| (op.label(), Do::Block(op))));
            groups[1].extend([BlockOp::Duplicate, BlockOp::Up, BlockOp::Down].map(|op| (op.label(), Do::Block(op))));
            groups[2].push((tr("Copy", "Copier"), Do::Copy));
            groups[2].push((tr("Cut", "Couper"), Do::Cut));
            groups[2].push((tr("Comment", "Commenter"), Do::Comment));
            groups[3].push((BlockOp::Delete.label(), Do::Block(BlockOp::Delete)));
        } else if let Some(link) = &menu.text {
            let web = matches!(link, Some(Link::Url(_)));
            if link.is_some() {
                groups[0].push((tr("Open the link", "Ouvrir le lien"), Do::OpenLink));
            }
            if web {
                groups[0].push((tr("Copy the address", "Copier l'adresse"), Do::CopyAddress));
            }
            groups[1].push((tr("Cut", "Couper"), Do::Cut));
            groups[1].push((tr("Copy", "Copier"), Do::Copy));
            groups[1].push((tr("Paste", "Coller"), Do::Paste));
            if self.pane().editor.read(cx).in_table() {
                // In a table: its rows, its columns, then the table.
                use editor::TableOp::*;
                for (group, ops) in [
                    (2, [RowAbove, RowBelow, DuplicateRow, MoveRowUp, MoveRowDown, DeleteRow].as_slice()),
                    (3, [ColLeft, ColRight, DuplicateCol, MoveColLeft, MoveColRight, DeleteCol].as_slice()),
                    (4, [SortAsc, SortDesc, Transpose, DeleteTable].as_slice()),
                ] {
                    groups[group].extend(ops.iter().map(|&op| (op.label(), Do::Table(op))));
                }
                groups[5].push((tr("Comment", "Commenter"), Do::Comment));
            } else {
                groups[2].push((tr("Bold", "Gras"), Do::Bold));
                groups[2].push((tr("Italic", "Italique"), Do::Italic));
                groups[2].push((tr("Underline", "Souligné"), Do::Underline));
                groups[2].push((tr("Strikethrough", "Barré"), Do::Strike));
                groups[2].push((tr("Highlight", "Surligné"), Do::Highlight));
                groups[2].push((tr("Code", "Code"), Do::Code));
                groups[2].push((if web { tr("Edit the link", "Modifier le lien") } else { tr("Make a link", "Faire un lien") }, Do::Link));
                groups[2].push((if menu.commented { tr("Resolve the comment", "Résoudre le commentaire") } else { tr("Comment", "Commenter") }, Do::Comment));
                // The targeted lines, from one kind of list to another.
                groups[3].push((tr("Bulleted list", "Liste à puces"), Do::Bullets));
                groups[3].push((tr("Numbered list", "Liste numérotée"), Do::Numbered));
                groups[3].push((tr("Task list", "Liste de tâches"), Do::Tasks));
                groups[3].push((tr("Plain text", "Texte simple"), Do::Plain));
            }
        } else if menu.target.as_deref().and_then(tag_of).is_some() {
            // A tag is neither a note nor a folder: only its notes can be listed.
            groups[0].push((tr("Show as a list", "Afficher en liste"), Do::List));
        } else {
            if !is_dir && !many {
                groups[0].push((tr("Open", "Ouvrir"), Do::Open));
                groups[0].push((tr("Open to the side", "Ouvrir à côté"), Do::OpenAside));
            }
            if tree && !many {
                groups[1].push((tr("New note here", "Nouvelle note ici"), Do::NewNote));
                if self.vault.as_ref().is_some_and(|root| vault::template_dir(root, &self.dirs).is_some()) {
                    groups[1].push((tr("New note from a template", "Nouvelle note depuis un modèle"), Do::FromTemplate));
                }
                groups[1].push((tr("New diagram here", "Nouveau schéma ici"), Do::NewDiagram));
                groups[1].push((tr("New folder", "Nouveau dossier"), Do::NewFolder));
            }
            if !is_dir || many {
                groups[2].push((tr("Copy the [[link]]", "Copier le [[lien]]"), Do::CopyLink));
            }
            groups[2].push((tr("Copy path", "Copier le chemin"), Do::CopyPath));
            if menu.target.is_some() {
                groups[2].push((tr("Copy relative path", "Copier le chemin relatif"), Do::CopyRelative));
            }
            if !many {
                groups[2].push((reveal_label(), Do::Reveal));
            }
            if is_dir && !many && tree {
                groups[0].push((tr("Show as a list", "Afficher en liste"), Do::List));
            }
            if menu.target.is_some() {
                if !is_dir || many {
                    groups[3].push((tr("Duplicate", "Dupliquer"), Do::Duplicate));
                }
                if !many {
                    groups[3].push((tr("Rename", "Renommer"), Do::Rename));
                    groups[3].push((tr("Move to…", "Déplacer vers…"), Do::MoveTo));
                    groups[3].push((tr("Icon…", "Icône…"), Do::Icon));
                }
                groups[3].push((tr("Move to the trash", "Mettre à la corbeille"), Do::Trash));
            }
        }
        groups.retain(|g| !g.is_empty());
        groups
    }

    /// Context menu, over the whole window: a click elsewhere closes it.
    pub(super) fn render_menu(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let menu = self.menu.as_ref()?;
        let t = self.theme;
        let groups = self.menu_groups(cx);
        let chosen = menu.chosen;
        let at = menu.at;
        let target = menu.target.clone();
        let entry = |i: usize, (label, what): (&'static str, Do), cx: &mut Context<Self>| {
            let target = target.clone();
            let on = chosen == Some(i);
            div()
                .id(label)
                .debug_selector(move || format!("menu-entry-{label}"))
                .h(px(28.))
                .mx_1()
                .px_2()
                .flex()
                .items_center()
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(t.selection))
                .when(on, |d| d.bg(t.selection))
                .when(matches!(what, Do::Trash), |d| d.text_color(t.accent))
                // The whole label, then its shortcut, dimmer, pinned to the right: the
                // menu takes the width they need.
                .child(div().flex_1().whitespace_nowrap().child(label))
                .child(keycaps(&what.keys(cx), t, on).debug_selector(move || format!("menu-keys-{label}")).flex_none().ml_4())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.menu_do(what, target.clone(), window, cx)
                    }),
                )
                .into_any_element()
        };
        let mut rows = Vec::new();
        let mut i = 0;
        for (g, group) in groups.into_iter().enumerate() {
            if g > 0 {
                rows.push(div().h(px(1.)).my(px(4.)).bg(t.border).into_any_element());
            }
            for item in group {
                rows.push(entry(i, item, cx));
                i += 1;
            }
        }
        let close = || cx.listener(|this: &mut Self, _: &MouseDownEvent, window, cx| this.close_menu(window, cx));
        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(MouseButton::Left, close())
                .on_mouse_down(MouseButton::Right, close())
                // The menu is as wide as its longest label with its shortcut, whatever
                // the font, and stays in the window, even opened near an edge.
                .child(gpui::anchored().position(at).snap_to_window_with_margin(px(8.)).child(
                    div()
                        .debug_selector(|| "menu".into())
                        .track_focus(&self.menu_focus)
                        .key_context("Menu")
                        // Escape is the window's `CloseHelp` before it is a key.
                        .on_action(cx.listener(|this, _: &CloseHelp, window, cx| this.close_menu(window, cx)))
                        .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                            if e.keystroke.modifiers.modified() {
                                return;
                            }
                            if this.menu_key(&e.keystroke.key, window, cx) {
                                cx.stop_propagation();
                            }
                        }))
                        .min_w(px(230.))
                        .py(px(5.))
                        .bg(t.panel)
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(8.))
                        .shadow_lg()
                        .text_size(px(13.))
                        .children(rows)
                        .with_animation(
                            "menu-in",
                            Animation::new(Duration::from_millis(110)).with_easing(ease_out_quint()),
                            |menu, delta| menu.opacity(delta),
                        ),
                )),
        )
    }
}
