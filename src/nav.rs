//! Navigation : un rail d'icônes toujours visible, et un panneau (arbre du
//! coffre, notes récentes ou graphe) qui partage la fenêtre avec la note.

use std::{
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
};

use gpui::{
    Context, CursorStyle, Div, FocusHandle, Focusable, MouseButton, MouseDownEvent, Pixels, ScrollStrategy,
    Stateful, UniformListScrollHandle, Window, actions, div, prelude::*, px, svg, uniform_list,
};

use crate::{
    Shell, Theme, tr,
    vault::{self, Note},
};

actions!(nav, [ShowTree, ShowRecent, ShowGraph, ToggleFull, Prev, Next, Fold, Unfold, Open, Close]);

const RAIL: Pixels = px(40.);
const ROW: Pixels = px(26.);
const LIST_W: Pixels = px(260.);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Tree,
    Recent,
    Graph,
}

/// Place du panneau : replié sur son rail, à côté de la note, ou seul.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Panel {
    Rail,
    Split,
    Full,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    /// Dossier, déplié ou non ; `None` pour une note.
    pub dir: Option<bool>,
}

pub struct Nav {
    pub focus: FocusHandle,
    pub mode: Mode,
    pub panel: Panel,
    list_w: Pixels,
    /// Largeur du graphe ; zéro : la moitié de la fenêtre.
    graph_w: Pixels,
    pub dragging: bool,
    /// Ligne sélectionnée (note ou dossier).
    pub sel: Option<PathBuf>,
    /// Dossiers dépliés.
    open: HashSet<PathBuf>,
    /// Lignes de la dernière frame.
    rows: Vec<Row>,
    scroll: UniformListScrollHandle,
    /// Amener la sélection à l'écran à la prochaine frame.
    reveal: bool,
    // Bord gauche et largeur du contenu de la fenêtre, à la dernière frame.
    pub left: Pixels,
    pub total: Pixels,
}

impl Nav {
    /// Reprend le panneau tel qu'il était à la fermeture de l'app.
    pub fn new(focus: FocusHandle) -> Self {
        let saved = vault::load_layout();
        let mut words = saved.split_whitespace();
        Self {
            focus,
            mode: match words.next() {
                Some("recent") => Mode::Recent,
                Some("graph") => Mode::Graph,
                _ => Mode::Tree,
            },
            panel: match words.next() {
                Some("split") => Panel::Split,
                Some("full") => Panel::Full,
                _ => Panel::Rail,
            },
            list_w: words.next().and_then(|w| w.parse().ok()).map_or(LIST_W, px),
            graph_w: words.next().and_then(|w| w.parse().ok()).map_or(px(0.), px),
            dragging: false,
            sel: None,
            open: HashSet::new(),
            rows: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            reveal: false,
            left: px(0.),
            total: px(0.),
        }
    }

    fn save(&self) {
        let mode = match self.mode {
            Mode::Tree => "tree",
            Mode::Recent => "recent",
            Mode::Graph => "graph",
        };
        let panel = match self.panel {
            Panel::Rail => "rail",
            Panel::Split => "split",
            Panel::Full => "full",
        };
        let (list, graph) = (f32::from(self.list_w), f32::from(self.graph_w));
        vault::save_layout(&format!("{mode} {panel} {list} {graph}\n"));
    }

    /// Sélectionne la note et déplie les dossiers qui y mènent.
    pub fn reveal(&mut self, path: &Path) {
        self.sel = Some(path.to_path_buf());
        self.open.extend(path.ancestors().skip(1).map(Path::to_path_buf));
        self.reveal = true;
    }

    /// Largeur du panneau à côté de la note, dans le mode affiché.
    fn width(&mut self) -> &mut Pixels {
        match self.mode {
            Mode::Graph => &mut self.graph_w,
            _ => &mut self.list_w,
        }
    }

    /// Dossier où créer une note depuis l'arbre : celui de la sélection.
    pub fn target_dir(&self) -> Option<PathBuf> {
        if self.panel == Panel::Rail || self.mode != Mode::Tree {
            return None;
        }
        let sel = self.sel.as_ref()?;
        if sel.extension().is_some_and(|e| e == "md") {
            sel.parent().map(Path::to_path_buf)
        } else {
            Some(sel.clone())
        }
    }
}

/// Lignes visibles de l'arbre : dossiers d'abord, puis notes, par ordre
/// alphabétique ; le contenu des dossiers repliés est omis.
// ponytail: retrié à chaque frame où le panneau est visible (quelques ms pour des
// milliers de notes) ; mettre en cache si l'arbre devient très gros.
pub fn tree_rows(root: &Path, notes: &[Note], open: &HashSet<PathBuf>) -> Vec<Row> {
    let key = |note: &Note| -> Vec<(bool, String)> {
        let rel = note.path.strip_prefix(root).unwrap_or(&note.path);
        let last = rel.components().count().saturating_sub(1);
        rel.components()
            .enumerate()
            .map(|(i, c)| (i == last, c.as_os_str().to_string_lossy().to_lowercase()))
            .collect()
    };
    let mut sorted: Vec<&Note> = notes.iter().collect();
    sorted.sort_by_cached_key(|n| key(n));

    let mut rows = Vec::new();
    let mut prev: Vec<PathBuf> = Vec::new();
    for note in sorted {
        let rel = note.path.strip_prefix(root).unwrap_or(&note.path);
        let mut dirs = Vec::new();
        let mut dir = root.to_path_buf();
        for part in rel.parent().into_iter().flat_map(Path::components) {
            dir.push(part);
            dirs.push(dir.clone());
        }
        let mut visible = true;
        for (depth, dir) in dirs.iter().enumerate() {
            if !visible {
                break;
            }
            let unfolded = open.contains(dir);
            if prev.get(depth) != Some(dir) {
                rows.push(Row {
                    path: dir.clone(),
                    name: vault::stem(dir),
                    depth,
                    dir: Some(unfolded),
                });
            }
            visible = unfolded;
        }
        if visible {
            rows.push(Row {
                path: note.path.clone(),
                name: note.name.clone(),
                depth: dirs.len(),
                dir: None,
            });
        }
        prev = dirs;
    }
    rows
}

/// Bouton icône du rail et des en-têtes de panneau.
fn button(id: &'static str, icon: &'static str, active: bool, t: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|s| s.bg(t.border))
        .when(active, |d| d.bg(t.selection))
        .child(
            svg()
                .path(icon)
                .size(px(16.))
                .flex_none()
                .text_color(if active { t.accent } else { t.dim }),
        )
}

impl Shell {
    /// Affiche le panneau dans ce mode et lui donne le focus. Redemander le mode
    /// affiché replie le panneau (au clavier : seulement s'il a déjà le focus).
    pub fn show_nav(&mut self, mode: Mode, toggle: bool, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.nav.panel != Panel::Rail && self.nav.mode == mode;
        if shown && (toggle || self.nav.focus.contains_focused(window, cx)) {
            self.nav.panel = Panel::Rail;
        } else {
            self.nav.mode = mode;
            if self.nav.panel == Panel::Rail {
                self.nav.panel = Panel::Split;
            }
            self.nav.reveal = true;
            window.focus(&self.nav.focus);
            self.refresh_graph(cx);
        }
        self.settle_nav(window, cx);
    }

    /// Met le graphe à jour s'il est affiché et que les notes ou leurs liens ont changé.
    pub fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        if self.graph_stale && self.nav.panel != Panel::Rail && self.nav.mode == Mode::Graph {
            self.graph_stale = false;
            self.graph.update(cx, |graph, cx| graph.set_notes(&self.notes, cx));
        }
    }

    /// Un nœud du graphe est sélectionné : aperçu de la note si elle a la place.
    pub fn select_from_graph(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.nav.panel == Panel::Split {
            self.preview_note(path, cx);
        }
        self.nav.sel = Some(path.to_path_buf());
        cx.notify();
    }

    pub fn toggle_full(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.panel = match self.nav.panel {
            Panel::Full => Panel::Split,
            _ => Panel::Full,
        };
        self.settle_nav(window, cx);
    }

    /// Après un changement de panneau : le focus quitte ce qui n'est plus
    /// affiché, et la disposition est mémorisée.
    pub fn settle_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.dragging = false;
        let editor = self.editor.focus_handle(cx);
        match self.nav.panel {
            Panel::Rail if self.nav.focus.contains_focused(window, cx) => window.focus(&editor),
            Panel::Full if editor.is_focused(window) => window.focus(&self.nav.focus),
            _ => {}
        }
        self.nav.save();
        cx.notify();
    }

    /// Le séparateur suit le pointeur ; aux extrémités, un côté se replie.
    pub fn drag_nav(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let x = x - self.nav.left - RAIL;
        let room = self.nav.total - RAIL;
        self.nav.panel = if x < px(100.) {
            Panel::Rail
        } else if x > room - px(140.) {
            Panel::Full
        } else {
            *self.nav.width() = x;
            Panel::Split
        };
        cx.notify();
    }

    fn nav_rows(&self) -> Vec<Row> {
        let Some(root) = &self.vault else {
            return Vec::new();
        };
        match self.nav.mode {
            Mode::Tree => tree_rows(root, &self.notes, &self.nav.open),
            Mode::Graph => Vec::new(),
            Mode::Recent => self
                .by_recency()
                .into_iter()
                .map(|n| Row {
                    path: n.path.clone(),
                    name: n.name.clone(),
                    depth: 0,
                    dir: None,
                })
                .collect(),
        }
    }

    fn selected_row(&self) -> Option<usize> {
        let sel = self.nav.sel.as_ref()?;
        self.nav.rows.iter().position(|r| r.path == *sel)
    }

    /// Sélectionne la ligne ; une note s'affiche en aperçu si elle a la place.
    fn nav_select(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let (path, is_note) = (row.path.clone(), row.dir.is_none());
        self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        if is_note && self.nav.panel == Panel::Split {
            self.preview_note(&path, cx);
        }
        self.nav.sel = Some(path);
        self.nav.reveal = false;
        cx.notify();
    }

    fn nav_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let last = self.nav.rows.len().saturating_sub(1);
        let ix = match self.selected_row() {
            Some(ix) if down => (ix + 1).min(last),
            Some(ix) => ix.saturating_sub(1),
            None => 0,
        };
        self.nav_select(ix, cx);
    }

    /// Entrée ou double-clic : ouvre la note pour de bon, ou bascule le dossier.
    fn nav_activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let path = row.path.clone();
        if row.dir.is_some() {
            if !self.nav.open.remove(&path) {
                self.nav.open.insert(path.clone());
            }
            self.nav.sel = Some(path);
            cx.notify();
        } else {
            self.open_from_nav(&path, window, cx);
        }
    }

    /// Ouvre la note et y place le focus ; la note reprend sa place si le
    /// panneau occupait toute la fenêtre.
    pub fn open_from_nav(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.open_note(path, cx);
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        window.focus(&self.editor.focus_handle(cx));
        self.settle_nav(window, cx);
    }

    /// Gauche/droite dans l'arbre : replier ou déplier, sinon remonter ou descendre.
    fn nav_fold(&mut self, unfold: bool, cx: &mut Context<Self>) {
        let Some(ix) = self.selected_row() else {
            return self.nav_step(true, cx);
        };
        let row = self.nav.rows[ix].clone();
        match (row.dir, unfold) {
            (Some(false), true) => {
                self.nav.open.insert(row.path);
            }
            (Some(true), false) => {
                self.nav.open.remove(&row.path);
            }
            (_, true) => return self.nav_step(true, cx),
            (_, false) => {
                let parent = row.path.parent();
                if let Some(up) = self.nav.rows.iter().position(|r| Some(r.path.as_path()) == parent) {
                    return self.nav_select(up, cx);
                }
            }
        }
        cx.notify();
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let t = self.theme;
        let focused = self.nav.focus.is_focused(window);
        let tree = self.nav.mode == Mode::Tree;
        range
            .filter_map(|ix| {
                let row = self.nav.rows.get(ix)?;
                let selected = self.nav.sel.as_ref() == Some(&row.path);
                let current = self.path.as_ref() == Some(&row.path);
                // Dans la liste des récents, le dossier de la note sert de détail.
                let detail = match (&self.vault, row.path.parent()) {
                    (Some(root), Some(dir)) if !tree => dir
                        .strip_prefix(root)
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                let chevron = match row.dir {
                    Some(true) => "chevron-down.svg",
                    Some(false) => "chevron-right.svg",
                    None => "",
                };
                let line = div()
                    .size_full()
                    .pr_2()
                        .pl(px(8. + 14. * row.depth as f32))
                        .flex()
                        .items_center()
                        .gap_1()
                        .rounded(px(5.))
                        .text_size(px(13.))
                        .when(!selected, |d| d.hover(|s| s.bg(t.code_bg)))
                        .when(selected, |d| d.bg(if focused { t.selection } else { t.border }))
                        .when(tree, |d| {
                            d.child(div().size(px(14.)).flex_none().when(row.dir.is_some(), |d| {
                                d.child(svg().path(chevron).size(px(14.)).text_color(t.dim))
                            }))
                        })
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .when(current, |d| d.text_color(t.accent))
                                .child(row.name.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .max_w(px(110.))
                                .truncate()
                                .text_size(px(11.5))
                                .text_color(t.dim)
                                .child(detail),
                        );
                Some(
                    div()
                        .id(ix)
                        .w_full()
                        .h(ROW)
                        .px_1()
                        .child(line)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                let folder = this.nav.rows.get(ix).is_some_and(|r| r.dir.is_some());
                                if folder || e.click_count >= 2 {
                                    this.nav_activate(ix, window, cx)
                                } else {
                                    this.nav_select(ix, cx)
                                }
                            }),
                        ),
                )
            })
            .collect()
    }

    /// Rail, puis panneau et séparateur s'ils sont dépliés.
    pub fn render_nav(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let (mode, panel) = (self.nav.mode, self.nav.panel);
        let unfolded = panel != Panel::Rail;
        self.nav.rows = if unfolded { self.nav_rows() } else { Vec::new() };
        let (current, selected) = (self.path.clone(), self.nav.sel.clone());
        self.graph.update(cx, |graph, _| graph.sync(t, current.as_deref(), selected.as_deref()));
        if std::mem::take(&mut self.nav.reveal)
            && let Some(ix) = self.selected_row()
        {
            self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }

        let mode_button = |id, icon, m: Mode| {
            button(id, icon, unfolded && mode == m, t)
                .on_click(cx.listener(move |this, _, window, cx| this.show_nav(m, true, window, cx)))
        };
        let rail = div()
            .flex_none()
            .w(RAIL)
            .h_full()
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .when(!unfolded, |d| d.border_r_1().border_color(t.border))
            .child(mode_button("nav-tree", "tree.svg", Mode::Tree))
            .child(mode_button("nav-recent", "clock.svg", Mode::Recent))
            .child(mode_button("nav-graph", "graph.svg", Mode::Graph))
            .child(div().w(px(16.)).h(px(1.)).my_1().bg(t.border))
            .child(
                button("nav-search", "search.svg", false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette("", window, cx))),
            )
            .child(
                button("nav-new", "plus.svg", false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.new_note_here(window, cx))),
            )
            .child(div().flex_1())
            .child(
                button("nav-help", "help.svg", false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.set_help(true, window, cx))),
            );

        let wrapper = div().h_full().flex().min_w_0();
        if !unfolded {
            return wrapper.flex_none().child(rail);
        }

        let full = panel == Panel::Full;
        let title = match (mode, &self.vault) {
            (Mode::Tree, Some(root)) => vault::stem(root),
            (Mode::Graph, _) => tr("Graph", "Graphe").to_string(),
            _ => tr("Recent", "Récents").to_string(),
        };
        let header = div()
            .flex_none()
            .h(px(44.))
            .pl_2()
            .pr_1()
            .flex()
            .items_center()
            .child(div().flex_1().truncate().text_size(px(12.)).text_color(t.dim).child(title))
            .when(mode == Mode::Graph, |d| {
                d.child(button("nav-center", "target.svg", false, t).on_click(cx.listener(
                    |this, _, _, cx| this.graph.update(cx, |graph, cx| graph.recenter(cx)),
                )))
            })
            .when(mode == Mode::Tree, |d| {
                d.child(button("nav-fold", "fold.svg", false, t).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.nav.open.clear();
                        cx.notify();
                    },
                )))
            })
            .child(
                button("nav-full", if full { "shrink.svg" } else { "expand.svg" }, false, t)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_full(window, cx))),
            );
        let list = uniform_list("nav-rows", self.nav.rows.len(), cx.processor(Self::render_rows))
            .track_scroll(self.nav.scroll.clone())
            .flex_1()
            .min_h_0();
        let room = self.nav.total - RAIL;
        let width = match *self.nav.width() {
            w if w == px(0.) => room / 2.,
            w => w.min(room - px(200.)).max(px(120.)),
        };
        let content = div()
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .map(|d| if full { d.flex_1() } else { d.flex_none().w(width) })
            .key_context("Nav")
            .track_focus(&self.nav.focus)
            .on_action(cx.listener(|this, _: &Prev, _, cx| this.nav_step(false, cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.nav_step(true, cx)))
            .on_action(cx.listener(|this, _: &Fold, _, cx| this.nav_fold(false, cx)))
            .on_action(cx.listener(|this, _: &Unfold, _, cx| this.nav_fold(true, cx)))
            .on_action(cx.listener(|this, _: &Open, window, cx| {
                if let Some(ix) = this.selected_row() {
                    this.nav_activate(ix, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if this.nav.panel == Panel::Full {
                    this.nav.panel = Panel::Split;
                }
                window.focus(&this.editor.focus_handle(cx));
                this.settle_nav(window, cx);
            }))
            .child(header)
            .map(|d| match mode {
                Mode::Graph => d.child(div().flex_1().min_h_0().child(self.graph.clone())),
                _ => d.child(list),
            });
        // Poignée de 5 px autour d'un trait de 1 px ; double-clic : largeur d'origine.
        let divider = div()
            .id("nav-divider")
            .flex_none()
            .w(px(5.))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .child(div().w(px(1.)).h_full().bg(t.border))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    if e.click_count >= 2 {
                        *this.nav.width() = if this.nav.mode == Mode::Graph { px(0.) } else { LIST_W };
                        this.nav.panel = Panel::Split;
                        this.settle_nav(window, cx);
                    } else {
                        this.nav.dragging = true;
                    }
                    cx.stop_propagation();
                }),
            );
        wrapper
            .map(|d| if full { d.flex_1() } else { d.flex_none() })
            .child(rail)
            .child(content)
            .child(divider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn builds_tree_rows() {
        let root = Path::new("/v");
        let note = |rel: &str| Note {
            name: vault::stem(Path::new(rel)),
            path: root.join(rel),
            tags: Vec::new(),
            links: Vec::new(),
            mtime: SystemTime::UNIX_EPOCH,
        };
        let notes = [note("b.md"), note("Z/x.md"), note("a/c/d.md"), note("a/B.md"), note("A.md")];
        let shown = |open: &[&str]| -> Vec<String> {
            let open = open.iter().map(|d| root.join(d)).collect();
            tree_rows(root, &notes, &open)
                .iter()
                .map(|r| format!("{}{}{}", "  ".repeat(r.depth), r.name, if r.dir.is_some() { "/" } else { "" }))
                .collect()
        };
        // Dossiers d'abord, sans tenir compte de la casse ; tout est replié.
        assert_eq!(shown(&[]), ["a/", "Z/", "A", "b"]);
        assert_eq!(shown(&["a"]), ["a/", "  c/", "  B", "Z/", "A", "b"]);
        // Un dossier déplié dans un dossier replié reste caché.
        assert_eq!(shown(&["a/c", "Z"]), ["a/", "Z/", "  x", "A", "b"]);
        assert_eq!(shown(&["a", "a/c"]), ["a/", "  c/", "    d", "  B", "Z/", "A", "b"]);
    }
}
