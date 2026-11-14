//! The vault health page: what `logic::health` finds in the vault, by kind, each row with its fixes.
//! It opens from the palette in a tab of its own and scans only while it is shown, off the UI thread,
//! from the index already in memory. Nothing is deleted: what goes, goes to `.trash`, and a rename
//! goes through the panel's rename, which rewrites the links.

use std::collections::HashSet;

use gpui::{AnyElement, Context, FocusHandle, Window, div, prelude::*};

use crate::{
    lang::tr,
    logic::health::{self, File, FileKind, Finding, Kind},
    platform::vault,
    ui::widgets::{group_header, page_head, pill},
};

use super::*;

/// Rows shown per kind: past that, the page says how many more there are.
// ponytail: a cap instead of the list page's virtual rows; a vault with thousands of orphans shows
// the first ones, which is where a cleanup starts anyway.
const SHOWN: usize = 200;

/// The page as a tab holds it.
pub(super) struct HealthPage {
    /// The last scan's findings; `None` until the first one ends.
    pub(super) found: Option<Vec<Finding>>,
    /// Kinds folded by the user.
    pub(super) folded: HashSet<Kind>,
    pub(super) focus: FocusHandle,
    pub(super) scroll: gpui::ScrollHandle,
}

/// The page's title, in its tab and over it.
pub(super) fn title() -> &'static str {
    tr("Vault health", "Santé du coffre")
}

/// The title of a group of findings.
fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::BrokenLink => tr("Broken links", "Liens cassés"),
        Kind::TemplateLink => tr("Links to a template", "Liens vers un modèle"),
        Kind::SameName => tr("Same name in several notes", "Même nom dans plusieurs notes"),
        Kind::Conflict => tr("Sync conflict copies", "Copies de conflit de synchronisation"),
        Kind::BadFrontMatter => tr("Unreadable front matter", "En-tête illisible"),
        Kind::Empty => tr("Empty notes", "Notes vides"),
        Kind::Unused => tr("Unused pictures and attachments", "Images et pièces jointes inutilisées"),
        Kind::Heavy => tr("Heavy pictures", "Images lourdes"),
        Kind::Orphan => tr("Orphan notes (no link in or out)", "Notes orphelines (aucun lien entrant ni sortant)"),
    }
}

/// A size in megabytes, one decimal: `7.2 MB`.
fn megabytes(bytes: u64) -> String {
    format!("{:.1} {}", bytes as f64 / (1024. * 1024.), tr("MB", "Mo"))
}

/// What a fix button does.
#[derive(Clone)]
enum Fix {
    /// Open the note or file, at a line of it.
    Open(PathBuf, usize),
    /// Create the note a broken link names.
    Create(String),
    /// Point `[[old]]` of this note at `new`.
    Relink(PathBuf, String, String),
    /// Take the brackets off `[[target]]` on this line.
    Unbracket(PathBuf, usize, String),
    Trash(PathBuf),
    Rename(PathBuf),
    /// The copy and its original side by side.
    Compare(PathBuf, PathBuf),
    /// Keep the copy: the original goes to the trash and the copy takes its name.
    KeepCopy(PathBuf, PathBuf),
    /// The folder a file is in, in the system's file manager.
    Folder(PathBuf),
    /// The note, selected in the graph.
    Graph(PathBuf),
}

impl Shell {
    /// Opens the health page, or brings back the tab that shows it, and scans the vault.
    pub(super) fn open_health(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        // What is being typed counts: it is saved before the scan reads the index.
        self.flush(cx);
        match self.panes.iter().position(|pane| pane.health().is_some()) {
            Some(tab) => self.focus_pane(tab, window, cx),
            None => {
                self.fresh_tab(window, cx);
                let page = HealthPage { found: None, folded: HashSet::new(), focus: cx.focus_handle(), scroll: Default::default() };
                window.focus(&page.focus);
                self.pane_mut().peek = false;
                self.pane_mut().set_cover(Cover::Health(page));
            }
        }
        self.health_stale = true;
        self.refresh_health(cx);
        cx.notify();
    }

    /// Scans the vault for the health page, off the UI thread, if the page is open and the index
    /// changed. One scan at a time: a change during a scan starts another when it ends.
    pub(super) fn refresh_health(&mut self, cx: &mut Context<Self>) {
        if !self.health_stale || self.health_running || !self.panes.iter().any(|pane| pane.health().is_some()) {
            return;
        }
        let Some(root) = self.vault.clone() else { return };
        (self.health_stale, self.health_running) = (false, true);
        let (notes, images, dirs) = (self.notes.clone(), self.images.clone(), self.dirs.clone());
        let scanned = root.clone();
        cx.spawn(async move |this, cx| {
            let (found, broken) = cx.background_executor().spawn(async move { scan(&scanned, &notes, &images, &dirs) }).await;
            this.update(cx, |this, cx| {
                this.health_running = false;
                if this.vault.as_ref() != Some(&root) {
                    return;
                }
                // Looking at the page is seeing its broken links: the palette's dot goes out. The count is
                // `health_dot`'s own, taken on the same snapshot, so the two always compare like with like.
                this.health_seen = Some(broken);
                for pane in &mut this.panes {
                    if let Some(page) = pane.health_mut() {
                        page.found = Some(found.clone());
                    }
                }
                this.refresh_health(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The palette's command shows a dot when there are more broken links than the page last showed.
    /// The first time it is asked, what is there is taken as seen.
    pub(super) fn health_dot(&mut self) -> bool {
        let Some(root) = self.vault.clone() else { return false };
        let broken = broken_count(&root, &self.notes, &self.images, &self.dirs);
        let seen = *self.health_seen.get_or_insert(broken);
        // Fewer than seen (fixed elsewhere): the next new one counts from there.
        self.health_seen = Some(seen.min(broken));
        broken > seen
    }

    /// The findings of the page in the active tab, for the tests.
    #[cfg(test)]
    pub(super) fn health_found(&self) -> Option<&[Finding]> {
        self.pane().health()?.found.as_deref()
    }

    /// Opens a note or a file; a note at line `row`.
    fn open_row(&mut self, path: &Path, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.open(path, window, cx);
        if row > 0 && self.pane().path.as_deref() == Some(path) {
            let at = self.pane().editor.read(cx).text().split_inclusive('\n').take(row).map(str::len).sum();
            self.pane().editor.update(cx, |e, cx| e.jump(at, true, cx));
        }
    }

    /// Rewrites one note with `change`. A note open in a tab changes in its editor, as one undo step,
    /// and is saved; any other is written on disk. False if nothing changed.
    fn fix_note(&mut self, path: &Path, change: impl Fn(&str) -> Option<String>, cx: &mut Context<Self>) -> bool {
        if let Some(tab) = self.panes.iter().position(|pane| pane.path.as_deref() == Some(path)) {
            let editor = self.panes[tab].editor.clone();
            let Some(text) = change(editor.read(cx).text()) else { return false };
            editor.update(cx, |e, cx| e.rewrite(&text, cx));
            return self.in_pane(tab, |this| {
                this.pane_mut().dirty = true;
                this.flush_one(cx)
            });
        }
        let Some(text) = fs::read_to_string(path).ok().and_then(|old| change(&old)) else { return false };
        if let Err(e) = vault::write(path, &text) {
            self.fail(tr("Not rewritten", "Non réécrite"), path, e, cx);
            return false;
        }
        self.edit_note_text(path, text.into());
        true
    }

    /// Relinks every broken link that has a single close name: each note once.
    fn fix_all(&mut self, cx: &mut Context<Self>) {
        let Some(found) = self.pane().health().and_then(|page| page.found.clone()) else { return };
        // Only what was rewritten counts: a note that could not be changed adds nothing to the message.
        let (mut links, mut notes) = (0, 0);
        for (path, pairs) in health::safe_relinks(&found) {
            if self.fix_note(&path, |text| health::apply_relinks(text, &pairs), cx) {
                (links, notes) = (links + pairs.len(), notes + 1);
            }
        }
        self.graph_changed(cx);
        self.say(Tone::Done, format!("{} {links} · {} {notes}", tr("Links relinked:", "Liens corrigés :"), tr("notes rewritten:", "notes réécrites :")), cx);
    }

    fn apply_fix(&mut self, fix: Fix, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else { return };
        match fix {
            Fix::Open(path, row) => self.open_row(&path, row, window, cx),
            Fix::Create(name) => self.open_wiki(&name, window, cx),
            Fix::Relink(path, old, new) => {
                if self.fix_note(&path, |text| markdown::relink(text, &old, &new), cx) {
                    self.graph_changed(cx);
                }
            }
            Fix::Unbracket(path, row, target) => {
                if self.fix_note(&path, |text| health::unbracket(text, row, &target), cx) {
                    self.graph_changed(cx);
                }
            }
            Fix::Trash(path) => self.trash(&root, &path, cx),
            Fix::Rename(path) => self.menu_do(nav::Do::Rename, Some(path), window, cx),
            Fix::Compare(original, copy) => {
                self.open(&original, window, cx);
                self.open_aside(&copy, window, cx);
            }
            Fix::KeepCopy(original, copy) => {
                self.trash(&root, &original, cx);
                if !original.exists() {
                    // The trash's « Undo » would put the original back where the copy now stands: the
                    // name is taken, so it could only fail. Its message goes, and says where the original is.
                    self.toasts.retain(|t| t.undo.as_ref().is_none_or(|(_, to)| *to != original));
                    self.flush(cx);
                    match vault::rename(&copy, &original) {
                        Ok(()) => {
                            self.relocate(&copy, &original, true, cx);
                            let name = original.file_name().unwrap_or_default().to_string_lossy().into_owned();
                            let said = tr("Copy kept; the original is in the trash, to restore under another name if needed:", "Copie gardée ; l'original est dans la corbeille, à restaurer sous un autre nom au besoin :");
                            self.say(Tone::Done, format!("{said} « {name} »"), cx);
                        }
                        Err(e) => self.fail(tr("Not renamed", "Renommage impossible"), &copy, e, cx),
                    }
                }
            }
            Fix::Folder(path) => {
                if let Some(dir) = path.parent() {
                    // The test platform opens nothing: it records what it would have been asked.
                    #[cfg(test)]
                    self.opened.push(dir.to_path_buf());
                    #[cfg(not(test))]
                    cx.open_with_system(dir);
                }
            }
            Fix::Graph(path) => {
                self.nav.set_sel(Some(path));
                self.show_nav(Mode::Graph, false, window, cx);
            }
        }
        cx.notify();
    }

    fn fold_health(&mut self, kind: Kind, cx: &mut Context<Self>) {
        if let Some(page) = self.pane_mut().health_mut()
            && !page.folded.remove(&kind)
        {
            page.folded.insert(kind);
        }
        cx.notify();
    }

    /// The path as the vault shows it: from its root, `/` between folders.
    fn shown_path(&self, path: &Path) -> String {
        let rel = self.vault.as_deref().and_then(|root| path.strip_prefix(root).ok()).unwrap_or(path);
        rel.to_string_lossy().replace('\\', "/")
    }

    /// What a row says, and its fixes.
    fn row_of(&self, finding: &Finding) -> (String, Vec<(String, Fix)>) {
        let open = || tr("Open", "Ouvrir").to_string();
        let trash = || tr("Move to the trash", "Mettre à la corbeille").to_string();
        let name = |path: &Path| vault::stem(path);
        match finding {
            Finding::BrokenLink { note, row, target, closest } => {
                let mut fixes = vec![(open(), Fix::Open(note.clone(), *row)), (format!("{} « {target} »", tr("Create", "Créer")), Fix::Create(target.clone()))];
                fixes.extend(closest.iter().take(3).map(|near| (format!("{} « {near} »", tr("Relink to", "Relier à")), Fix::Relink(note.clone(), target.clone(), near.clone()))));
                fixes.push((tr("Remove the brackets", "Retirer les crochets").to_string(), Fix::Unbracket(note.clone(), *row, target.clone())));
                (format!("{} · [[{target}]] · {} {}", name(note), tr("line", "ligne"), row + 1), fixes)
            }
            Finding::TemplateLink { note, row, target, template } => (
                format!("{} · [[{target}]] · {} {}", name(note), tr("line", "ligne"), row + 1),
                vec![(open(), Fix::Open(note.clone(), *row)), (tr("Open the template", "Ouvrir le modèle").to_string(), Fix::Open(template.clone(), 0))],
            ),
            Finding::SameName { name, notes } => {
                let mut fixes = Vec::new();
                for path in notes {
                    let shown = self.shown_path(path);
                    fixes.push((format!("{} {shown}", tr("Open", "Ouvrir")), Fix::Open(path.clone(), 0)));
                    fixes.push((format!("{} {shown}", tr("Rename", "Renommer")), Fix::Rename(path.clone())));
                }
                (format!("« {name} » · {}", notes.iter().map(|p| self.shown_path(p)).collect::<Vec<_>>().join(", ")), fixes)
            }
            Finding::Conflict { copy, original: Some(original) } => (
                self.shown_path(copy),
                vec![
                    (tr("Compare side by side", "Comparer côte à côte").to_string(), Fix::Compare(original.clone(), copy.clone())),
                    (tr("Keep this copy", "Garder cette copie").to_string(), Fix::KeepCopy(original.clone(), copy.clone())),
                    (tr("Keep the original", "Garder l'original").to_string(), Fix::Trash(copy.clone())),
                ],
            ),
            Finding::Conflict { copy, original: None } => (self.shown_path(copy), vec![(open(), Fix::Open(copy.clone(), 0)), (trash(), Fix::Trash(copy.clone()))]),
            Finding::BadFrontMatter { note, row } => (format!("{} · {} {}", name(note), tr("line", "ligne"), row + 1), vec![(open(), Fix::Open(note.clone(), *row))]),
            Finding::Empty { note } => (self.shown_path(note), vec![(open(), Fix::Open(note.clone(), 0)), (trash(), Fix::Trash(note.clone()))]),
            Finding::Unused { path } => (self.shown_path(path), vec![(open(), Fix::Open(path.clone(), 0)), (trash(), Fix::Trash(path.clone()))]),
            Finding::Heavy { path, bytes } => (
                format!("{} · {}", self.shown_path(path), megabytes(*bytes)),
                vec![(open(), Fix::Open(path.clone(), 0)), (tr("Show the folder", "Montrer le dossier").to_string(), Fix::Folder(path.clone()))],
            ),
            Finding::Orphan { note } => (
                self.shown_path(note),
                vec![(open(), Fix::Open(note.clone(), 0)), (tr("Show in the graph", "Montrer dans le graphe").to_string(), Fix::Graph(note.clone()))],
            ),
        }
    }

    /// The page: its head ("12 to look at", the "Fix all" button), then the findings by kind, each kind
    /// folded or not, each row with its fixes. Debug selectors: `health-kind-{kind}`, `health-row-{kind}-{i}`,
    /// `health-fix-{kind}-{i}-{j}`, `health-fix-all`.
    pub(super) fn render_health(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        let Some(page) = self.pane().health() else {
            return div().into_any_element();
        };
        let found = page.found.as_deref().unwrap_or_default();
        let summary = match &page.found {
            None => tr("Looking…", "Recherche…").to_string(),
            Some(_) if found.is_empty() => tr("nothing to fix", "rien à corriger").to_string(),
            Some(_) => format!("{} {}", found.len(), tr("to look at", "à examiner")),
        };
        let safe = health::safe_relinks(found).iter().map(|(_, pairs)| pairs.len()).sum::<usize>();
        let fix_all = pill(t, true)
            .id("health-fix-all")
            .debug_selector(|| "health-fix-all".into())
            .h(t.px(26.))
            .cursor_pointer()
            .child(format!("{} ({safe})", tr("Fix all: relink to the only close name", "Tout corriger : relier au seul nom proche")))
            .on_click(cx.listener(|this, _, _, cx| this.fix_all(cx)));
        let mut body: Vec<AnyElement> = Vec::new();
        for (kind, count) in health::counts(found) {
            let key = format!("{kind:?}");
            let folded = page.folded.contains(&kind);
            body.push(
                group_header(("health-kind", kind as usize), kind_name(kind), count, folded, t)
                    .debug_selector({
                        let key = key.clone();
                        move || format!("health-kind-{key}")
                    })
                    .mt_3()
                    .on_click(cx.listener(move |this, _, _, cx| this.fold_health(kind, cx)))
                    .into_any_element(),
            );
            if folded {
                continue;
            }
            for (i, finding) in found.iter().filter(|f| f.kind() == kind).take(SHOWN).enumerate() {
                let (label, fixes) = self.row_of(finding);
                let buttons = fixes.into_iter().enumerate().map(|(j, (said, fix))| {
                    let key = key.clone();
                    pill(t, false)
                        .id(("health-fix", (kind as usize) * 1_000_000 + i * 100 + j))
                        .debug_selector(move || format!("health-fix-{key}-{i}-{j}"))
                        .h(t.px(22.))
                        .flex_none()
                        .border_1()
                        .border_color(t.border)
                        .text_size(t.px(12.))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.hover).text_color(t.text))
                        .child(said)
                        .on_click(cx.listener(move |this, _, window, cx| this.apply_fix(fix.clone(), window, cx)))
                });
                let key = key.clone();
                body.push(
                    div()
                        .debug_selector(move || format!("health-row-{key}-{i}"))
                        .pl(t.px(22.))
                        .py(t.px(3.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .rounded(t.small())
                        .hover(|s| s.bg(t.hover))
                        .child(div().min_w_0().max_w_full().truncate().text_color(t.text).child(label))
                        .children(buttons)
                        .into_any_element(),
                );
            }
            if count > SHOWN {
                body.push(div().pl(t.px(22.)).py_1().text_color(t.soft).child(format!("… {} {}", count - SHOWN, tr("more", "de plus"))).into_any_element());
            }
        }
        let empty = page.found.is_some() && found.is_empty();
        div()
            .id("health")
            .track_focus(&page.focus)
            .size_full()
            .pt(t.px(48.))
            .px_6()
            .pb_4()
            .text_size(t.px(13.))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&page.scroll)
            .child(page_head(t, "health", 22., title().to_string(), summary).pb_2())
            .child(div().text_color(t.soft).pb_2().child(tr(
                "Nothing is deleted: what you remove goes to the vault's trash, and a rename rewrites the links.",
                "Rien n'est supprimé : ce que vous retirez va dans la corbeille du coffre, et un renommage réécrit les liens.",
            )))
            .when(safe > 0, |d| d.child(div().flex().child(fix_all)))
            .children(body)
            .when(empty, |d| {
                d.child(div().debug_selector(|| "health-empty".into()).py_6().text_center().text_color(t.soft).child(tr("Nothing to fix: the vault is in good health", "Rien à corriger : le coffre est en bonne santé")))
            })
            .into_any_element()
    }
}

/// What the scan needs to know of a file that is not a note. A table, or an SVG that is a Bref diagram
/// (`read`: its text is read to tell), is a document of its own, never unused.
fn kind_of(path: &Path, read: bool) -> FileKind {
    let diagram = || {
        path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"))
            && fs::read_to_string(path).is_ok_and(|text| Diagram::from_svg(&text).is_some() || Diagram::newer(&text))
    };
    if vault::is_table(path) || (read && diagram()) {
        FileKind::Document
    } else if vault::is_image(path) {
        FileKind::Picture
    } else {
        FileKind::Attachment
    }
}

/// The templates folder answers no link, as `wiki_dir` says.
fn link_dirs(root: &Path, dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter().filter(|dir| !vault::in_templates(root, &dir.join("x"))).cloned().collect()
}

/// The broken links the palette's dot counts, from the index alone: no file is read.
fn broken_count(root: &Path, notes: &[Note], images: &[PathBuf], dirs: &[PathBuf]) -> usize {
    let files: Vec<File> = images.iter().map(|path| File { path: path.clone(), bytes: 0, kind: kind_of(path, false) }).collect();
    let template = |n: &Note| vault::in_templates(root, &n.path);
    health::broken_count(&health::Vault { notes, files: &files, dirs: &link_dirs(root, dirs), template: &template })
}

/// The scan, as run off the UI thread: the sizes of the files are read here, the rest is the index. With
/// it, the dot's count of the same snapshot.
fn scan(root: &Path, notes: &[Note], images: &[PathBuf], dirs: &[PathBuf]) -> (Vec<Finding>, usize) {
    let files: Vec<File> = images
        .iter()
        .map(|path| File { path: path.clone(), bytes: fs::metadata(path).map_or(0, |m| m.len()), kind: kind_of(path, true) })
        .collect();
    let template = |n: &Note| vault::in_templates(root, &n.path);
    let found = health::scan(&health::Vault { notes, files: &files, dirs: &link_dirs(root, dirs), template: &template });
    (found, broken_count(root, notes, images, dirs))
}
