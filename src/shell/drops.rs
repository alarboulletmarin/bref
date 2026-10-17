//! Files dragged from the system onto the window: where they would go, and dropping them.

use super::*;

/// Where files from the system would go, while they are dragged over the window.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum Aim {
    /// In the pane's note, between two lines (the byte where it starts); `version`: the text seen then.
    Note { pane: usize, at: usize, version: u64 },
    /// In a vault folder.
    Folder(PathBuf),
    /// At the vault root.
    Root,
}

/// What is being dragged, and what the drop would do: enough to show the choice under the pointer.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct DropAim {
    pub aim: Aim,
    pub items: usize,
    pub pointer: Point<Pixels>,
}

impl Shell {
    /// What a drop would do, while it lasts: the label near the pointer, the line in the note.
    pub(super) fn aim_drop(&mut self, aim: Option<DropAim>, cx: &mut Context<Self>) {
        if aim == self.drop_aim {
            return;
        }
        let mark = |aim: &Option<DropAim>| match aim {
            Some(DropAim { aim: Aim::Note { pane, at, .. }, .. }) => Some((*pane, *at)),
            _ => None,
        };
        let (old, new) = (mark(&self.drop_aim), mark(&aim));
        if old != new {
            for (n, pane) in self.panes.iter().enumerate() {
                let at = new.filter(|(pane, _)| *pane == n).map(|(_, at)| at);
                pane.editor.update(cx, |editor, cx| editor.set_drop_mark(at, cx));
            }
        }
        self.drop_aim = aim;
        cx.notify();
    }

    /// Files and folders dropped on the window: copied into the vault (the original does not move, a
    /// taken name gets a number), then, on a note, named by one line each where the line marker was.
    pub(super) fn drop_paths(&mut self, paths: Vec<PathBuf>, aim: Aim, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        let dir = match &aim {
            Aim::Note { pane, .. } => {
                self.panes.get(*pane).and_then(|p| p.path.as_deref()).and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(|| root.clone())
            }
            Aim::Folder(dir) => dir.clone(),
            Aim::Root => root.clone(),
        };
        self.aim_drop(None, cx);
        if paths.is_empty() {
            return;
        }
        self.say(Tone::Busy, tr("Copying…", "Copie…"), cx);
        cx.spawn(async move |this, cx| {
            let copied = cx.background_executor().spawn({
                let dir = dir.clone();
                async move { vault::import(&paths, &dir) }
            });
            let (done, problems) = copied.await;
            this.update(cx, |this, cx| this.dropped(done, problems, &root, &dir, aim, cx)).ok();
        })
        .detach();
    }

    /// What the copy filed: the new files in the tree, their links in the note, a word about what
    /// was left out.
    pub(super) fn dropped(&mut self, done: Vec<vault::Imported>, problems: Vec<String>, root: &Path, dir: &Path, aim: Aim, cx: &mut Context<Self>) {
        self.toasts.retain(|t| t.tone != Tone::Busy);
        if done.is_empty() {
            let why = problems.first().cloned().unwrap_or_default();
            return self.say(Tone::Failed, format!("{} : {why}", tr("Nothing copied", "Rien n'a été copié")), cx);
        }
        self.rescan(root, cx);
        if let Some(first) = done.first() {
            self.nav.open.insert(dir.to_path_buf());
            self.nav.reveal(&first.to);
        }
        if let Aim::Note { pane, at, version } = aim {
            // One line per item: a picture is shown, another file is linked by its name.
            let lines: Vec<String> = done
                .iter()
                // A type the vault does not list (`.exe`…) is copied, but has no link that opens.
                .filter(|item| item.dir || vault::is_file_shown(&item.to) || item.to.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")))
                .map(|item| {
                    let name = item.to.file_name().unwrap_or_default().to_string_lossy();
                    match (item.dir, vault::is_image(&item.to)) {
                        (false, true) => format!("![[{name}]]"),
                        (false, false) if item.to.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")) => format!("[[{}]]", vault::stem(&item.to)),
                        _ => format!("[[{name}]]"),
                    }
                })
                .collect();
            if let Some(editor) = self.panes.get(pane).map(|p| p.editor.clone()).filter(|_| !lines.is_empty()) {
                // The text changed during the copy: the lines go where the cursor is.
                editor.update(cx, |editor, cx| {
                    let at = if editor.revision() == version { at } else { editor.caret_line_start() };
                    editor.insert_lines(at, &lines, cx);
                });
            }
        }
        let files: usize = done.iter().map(|d| d.files.max(1)).sum();
        let place = match dir.strip_prefix(root) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
            _ => tr("the vault", "le coffre").to_string(),
        };
        let mut said = format!("{} {} {} {place}", tr("Copied", "Copié"), files, tr(if files == 1 { "file into" } else { "files into" }, if files == 1 { "fichier dans" } else { "fichiers dans" }));
        if !problems.is_empty() {
            said.push_str(&format!(" · {} {}", problems.len(), tr("left out", "laissés de côté")));
        }
        self.say(if problems.is_empty() { Tone::Done } else { Tone::Info }, said, cx);
    }

    /// Where files dragged at `at` would go in the note of pane `pane`: only if a note is written there.
    pub(super) fn note_aim(&self, pane: usize, at: Point<Pixels>, items: usize, cx: &App) -> Option<DropAim> {
        let shown = self.panes.get(pane)?;
        let editor = shown.editor.read(cx);
        let board = kanban::is_board(editor.text()) && !shown.board.text;
        let writes = shown.file.is_none() && shown.listing.is_none() && !board && self.vault.is_some();
        writes.then(|| DropAim { aim: Aim::Note { pane, at: editor.drop_point(at), version: editor.revision() }, items, pointer: at })
    }

    /// The label that follows the pointer while files are dragged over the window.
    pub(super) fn render_drop_chip(&self, view: Size<Pixels>) -> Option<impl IntoElement + use<>> {
        let (aim, t) = (self.drop_aim.as_ref()?, self.theme);
        let name = |path: &Path| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let (verb, place) = match &aim.aim {
            Aim::Note { pane, .. } => {
                let title = self.panes.get(*pane).and_then(|p| p.path.as_deref()).map(vault::stem).unwrap_or_else(|| tr("New note", "Nouvelle note").to_string());
                (tr("Insert in", "Insérer dans"), title)
            }
            Aim::Folder(dir) => (tr("Copy into", "Copier dans"), name(dir)),
            Aim::Root => (tr("Copy into", "Copier dans"), tr("the vault", "le coffre").to_string()),
        };
        let count = format!("{} {}", aim.items, tr(if aim.items == 1 { "item" } else { "items" }, if aim.items == 1 { "élément" } else { "éléments" }));
        // Near an edge, the label moves to the other side of the pointer: it stays in the window.
        let (left, above) = (aim.pointer.x > view.width * 0.6, aim.pointer.y > view.height - px(70.));
        Some(
            div()
                .debug_selector(|| "drop-chip".into())
                .absolute()
                .map(|d| if left { d.right(view.width - aim.pointer.x + px(14.)) } else { d.left(aim.pointer.x + px(14.)) })
                .map(|d| if above { d.bottom(view.height - aim.pointer.y + px(14.)) } else { d.top(aim.pointer.y + px(18.)) })
                .max_w(px(420.))
                .truncate()
                .px_2()
                .py_1()
                .rounded(px(8.))
                .bg(t.panel)
                .border_1()
                .border_color(t.accent.opacity(0.6))
                .text_size(px(12.))
                .text_color(t.text)
                .flex()
                .gap_1()
                .child(div().text_color(t.accent).child(verb))
                .child(format!("« {place} » · {count}")),
        )
    }
}
