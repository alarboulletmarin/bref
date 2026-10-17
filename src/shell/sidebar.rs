//! The side panel as the shell drives it: rail, tree, recent notes, tags, graph, calendar, list pages, context menu and file operations.

use std::{
    collections::HashSet,
    hash::{BuildHasher, BuildHasherDefault, DefaultHasher, Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
};
use gpui::{
    Action, ClickEvent, Context, CursorStyle, Div, Focusable, MouseButton,
    MouseDownEvent, Pixels, ScrollStrategy, Stateful, Window, div, point, prelude::*, px, svg, uniform_list,
};
use crate::{
    ui::widgets::{button, tip}, views::graph,
    logic::markdown::{self, Date},
    views::palette::{Palette, PaletteEvent, Setting},
    lang::tr,
    platform::vault,
    logic::note::Note,
};

use super::*;
use crate::views::nav::*;

impl Shell {
    /// Affiche le panneau dans ce mode et lui donne le focus. Redemander le mode
    /// affiché replie le panneau (au clavier : seulement s'il a déjà le focus).
    pub(super) fn show_nav(&mut self, mode: Mode, toggle: bool, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        if self.graph_stale && self.nav.panel != Panel::Rail && self.nav.mode == Mode::Graph {
            self.graph_stale = false;
            let notes: Vec<Note> = self.notes.iter().filter(|n| !self.is_template(n)).cloned().collect();
            self.graph.update(cx, |graph, cx| graph.set_notes(&notes, &vault::pictures(&self.images), cx));
        }
    }

    /// Un nœud du graphe est sélectionné : aperçu de la note si elle a la place.
    pub(super) fn select_from_graph(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.nav.panel == Panel::Split {
            self.preview_note(path, cx);
        }
        self.nav.sel = Some(path.to_path_buf());
        cx.notify();
    }

    pub(super) fn toggle_full(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.panel = match self.nav.panel {
            Panel::Full => Panel::Split,
            _ => Panel::Full,
        };
        self.settle_nav(window, cx);
    }

    /// Après un changement de panneau : le focus quitte ce qui n'est plus
    /// affiché, et la disposition est mémorisée.
    pub(super) fn settle_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav.dragging = false;
        let editor = self.pane().editor.focus_handle(cx);
        match self.nav.panel {
            Panel::Rail if self.nav.focus.contains_focused(window, cx) => window.focus(&editor),
            Panel::Full if editor.is_focused(window) => window.focus(&self.nav.focus),
            _ => {}
        }
        self.nav.save();
        cx.notify();
    }

    /// Largeur du panneau ouvert à côté de la note : celle qu'on lui a donnée, tant que la note
    /// (`NOTE_MIN`) et ce qui est réservé à sa droite gardent leur place.
    pub(super) fn split_width(&self) -> Pixels {
        let room = self.nav.total - RAIL - DIVIDER - self.nav.aside;
        // Le mois ne se lit plus en dessous de 170 px : ses jours se touchent.
        let floor = px(if self.nav.mode == Mode::Calendar { 170. } else { 120. });
        match if self.nav.mode == Mode::Graph { self.nav.graph_w } else { self.nav.list_w } {
            w if w == px(0.) => room / 2.,
            w => w.min(room - NOTE_MIN).max(floor),
        }
    }

    /// Le séparateur suit le pointeur ; aux extrémités, un côté se replie.
    pub(super) fn drag_nav(&mut self, x: Pixels, cx: &mut Context<Self>) {
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

    /// Empreinte de tout ce dont les lignes du panneau dépendent : les recalculer
    /// coûte dix fois plus cher que de vérifier qu'elles sont encore justes.
    pub(super) fn rows_source(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let open = self.nav.open.iter().map(|dir| BuildHasherDefault::<DefaultHasher>::default().hash_one(dir)).fold(0u64, u64::wrapping_add);
        (self.nav.mode, self.nav.panel == Panel::Rail, &self.vault, &self.dirs, &self.images, open).hash(&mut hasher);
        if self.nav.mode == Mode::Links {
            self.nav.anchor.hash(&mut hasher);
        }
        if self.nav.mode == Mode::Recent {
            self.recent.hash(&mut hasher);
        }
        for note in &self.notes {
            (&note.path, &note.name).hash(&mut hasher);
            if self.nav.mode == Mode::Tags {
                note.tags.hash(&mut hasher);
            }
            if self.nav.mode == Mode::Links {
                (&note.links, &note.aliases).hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    pub(super) fn nav_rows(&self) -> Vec<Row> {
        let Some(root) = &self.vault else {
            return Vec::new();
        };
        match self.nav.mode {
            Mode::Tree => tree_rows(root, &self.notes, &self.dirs, &self.images, &self.nav.open),
            Mode::Graph | Mode::Calendar => Vec::new(),
            Mode::Tags => tag_rows(&self.notes, &self.nav.open),
            Mode::Links => (self.nav.anchor.iter())
                .flat_map(|target| graph::backlinks(&self.notes, target))
                .map(|n| Row { path: n.path.clone(), name: n.name.clone(), depth: 0, dir: None })
                .collect(),
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

    pub(super) fn selected_row(&self) -> Option<usize> {
        let sel = self.nav.sel.as_ref()?;
        self.nav.rows.iter().position(|r| r.path == *sel)
    }

    /// Sélectionne la ligne ; une note s'affiche en aperçu si elle a la place.
    pub(super) fn nav_select(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let (path, is_note) = (row.path.clone(), row.dir.is_none());
        self.nav.marked.clear();
        self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        if is_note && self.nav.panel == Panel::Split {
            self.preview_note(&path, cx);
        }
        self.nav.sel = Some(path);
        self.nav.reveal = false;
        cx.notify();
    }

    /// Ctrl+clic ajoute la ligne à la sélection ou l'en retire ; Maj+clic
    /// sélectionne tout depuis la dernière ligne choisie.
    pub(super) fn nav_mark(&mut self, ix: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(path) = self.nav.rows.get(ix).map(|r| r.path.clone()) else {
            return;
        };
        let marked = &mut self.nav.marked;
        if extend {
            let from = self.nav.sel.as_ref().and_then(|sel| self.nav.rows.iter().position(|r| r.path == *sel)).unwrap_or(ix);
            *marked = self.nav.rows[from.min(ix)..=from.max(ix)].iter().map(|r| r.path.clone()).collect();
            self.nav.sel.get_or_insert(path);
        } else {
            // La ligne déjà sélectionnée fait partie du lot.
            if marked.is_empty() {
                marked.extend(self.nav.sel.take());
            }
            if !marked.remove(&path) {
                marked.insert(path.clone());
            }
            // Retirée, elle laisse sa place de ligne choisie à une autre du lot.
            let other = self.nav.rows.iter().map(|r| &r.path).find(|p| marked.contains(*p)).cloned();
            self.nav.sel = if marked.contains(&path) { Some(path) } else { other.or(Some(path)) };
        }
        cx.notify();
    }

    /// Ce qu'une action sur `target` vise : toute la sélection multiple s'il en
    /// fait partie, dans l'ordre du panneau, sans ce qu'un dossier visé contient déjà.
    pub(super) fn targets(&self, target: &Path) -> Vec<PathBuf> {
        let marked = &self.nav.marked;
        if !marked.contains(target) {
            return vec![target.to_path_buf()];
        }
        let mut seen = HashSet::new();
        let rows = self.nav.rows.iter().map(|r| &r.path);
        rows.filter(|p| marked.contains(*p) && !p.ancestors().skip(1).any(|a| marked.contains(a)) && seen.insert(*p))
            .cloned()
            .collect()
    }

    /// Calendrier : avance le jour choisi de `days` jours, ou de `months` mois.
    pub(super) fn day_step(&mut self, days: i64, months: i32, cx: &mut Context<Self>) {
        let moved = markdown::day_of(markdown::day_number(self.nav.day) + days);
        self.nav.day = markdown::add_months(moved, months);
        cx.notify();
    }

    /// Ouvre la note du jour `day`, ou la crée : celle que `@` et Ctrl+J désignent.
    pub(super) fn open_day(&mut self, day: Date, cx: &mut Context<Self>) {
        self.nav.day = day;
        self.open_wiki(&crate::lang::date_name(day), cx);
        cx.notify();
    }

    /// Le mois du jour choisi : une case par jour, marquée s'il a sa note. Les flèches du
    /// clavier changent de jour, Page haut et bas de mois, Entrée ouvre la note.
    pub(super) fn render_calendar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let (year, month, chosen) = self.nav.day;
        // La semaine commence le lundi en français, le dimanche en anglais.
        let monday = tr("sunday", "monday") == "monday";
        let (lead, days) = markdown::month(year, month, monday);
        let names: HashSet<&str> = self.notes.iter().map(|n| n.name.as_str()).collect();
        let (today, open) = (crate::platform::clock::today(), self.pane().path.as_deref().map(vault::stem));
        let (en, fr) = markdown::MONTHS[month as usize - 1];
        let step = |id: &'static str, icon: &'static str, by: i32, cx: &mut Context<Self>| {
            button(id, icon, false, t).on_click(cx.listener(move |this, _, _, cx| this.day_step(0, by, cx)))
        };
        let head = div()
            .flex()
            .items_center()
            .child(div().flex_1().min_w_0().truncate().pl_1().child(format!("{} {year}", tr(en, fr))))
            .child(step("month-prev", "chevron-left.svg", -1, cx))
            .child(step("month-next", "chevron-right.svg", 1, cx));
        // Sept colonnes égales, qui rétrécissent avec le panneau au lieu de passer sous la note.
        let cell = || div().h(px(30.)).flex().items_center().justify_center();
        let initials = if monday { tr("MTWTFSS", "LMMJVSD") } else { tr("SMTWTFS", "DLMMJVS") };
        let week = initials.chars().map(|c| cell().text_color(t.soft).text_size(px(11.)).child(c.to_string()));
        let blanks = (0..lead).map(|_| cell().into_any_element());
        let cells = (1..=days).map(|day| {
            let date = (year, month, day);
            let name = crate::lang::date_name(date);
            let has = names.contains(name.as_str());
            cell()
                .id(("day", day as usize))
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(if has { t.text } else { t.soft })
                .when(has, |d| d.font_weight(gpui::FontWeight::BOLD))
                .when(open.as_deref() == Some(name.as_str()), |d| d.text_color(t.accent))
                .when(date == today, |d| d.border_1().border_color(t.accent.opacity(0.6)))
                .when(day == chosen, |d| d.bg(t.selection))
                .hover(|s| s.bg(t.code_bg))
                .child(day.to_string())
                .on_click(cx.listener(move |this, _, _, cx| this.open_day(date, cx)))
                .into_any_element()
        });
        div()
            .flex_1()
            .min_h_0()
            .px_2()
            .text_size(px(13.))
            .flex()
            .flex_col()
            .gap_1()
            .child(head)
            .child(div().debug_selector(|| "calendar".into()).w_full().max_w(px(224.)).grid().grid_cols(7).children(week).children(blanks).children(cells))
    }

    pub(super) fn nav_step(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.nav.mode == Mode::Calendar {
            return self.day_step(if down { 7 } else { -7 }, 0, cx);
        }
        let last = self.nav.rows.len().saturating_sub(1);
        let ix = match self.selected_row() {
            Some(ix) if down => (ix + 1).min(last),
            Some(ix) => ix.saturating_sub(1),
            None => 0,
        };
        self.nav_select(ix, cx);
    }

    /// Entrée ou double-clic : ouvre la note pour de bon, ou bascule le dossier.
    pub(super) fn nav_activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.nav.rows.get(ix) else {
            return;
        };
        let path = row.path.clone();
        self.nav.marked.clear();
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
    pub(super) fn open_from_nav(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.open_note(path, cx);
        if self.nav.panel == Panel::Full {
            self.nav.panel = Panel::Split;
        }
        // Un tableau prend le focus pour qu'on y écrive ; une image s'affiche à la place
        // de la note, et le panneau garde la main.
        match self.shown_sheet() {
            Some(sheet) => window.focus(&sheet.focus_handle(cx)),
            None if self.pane().file.is_none() => window.focus(&self.pane().editor.focus_handle(cx)),
            None => {}
        }
        self.settle_nav(window, cx);
    }

    /// Gauche/droite dans l'arbre : replier ou déplier, sinon remonter ou descendre.
    pub(super) fn nav_fold(&mut self, unfold: bool, cx: &mut Context<Self>) {
        if self.nav.mode == Mode::Calendar {
            return self.day_step(if unfold { 1 } else { -1 }, 0, cx);
        }
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

    /// Replie tous les dossiers de l'arbre ; s'ils le sont déjà, les déplie tous.
    pub(super) fn fold_all(&mut self) {
        if self.all_folded() {
            self.nav.open = self.dirs.iter().cloned().collect();
        } else {
            self.nav.open.clear();
        }
    }

    /// Aucun dossier du coffre n'est déplié. `open` garde aussi les parents du
    /// coffre lui-même, d'où le test dossier par dossier.
    pub(super) fn all_folded(&self) -> bool {
        !self.dirs.iter().any(|d| self.nav.open.contains(d))
    }

    /// Demande un nom dans un champ de saisie, puis appelle `then` avec ce nom nettoyé.
    pub(crate) fn ask(
        &mut self,
        label: impl Into<String>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let theme = self.theme;
        let prompt = cx.new(|cx| Palette::prompt(label, text, theme, cx));
        cx.subscribe_in(&prompt, window, move |this, _, event, window, cx| {
            this.palette = None;
            window.focus(&this.nav.focus);
            if let PaletteEvent::Submit(text) = event
                && let Some(name) = vault::clean_name(text)
            {
                then(this, name, window, cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&prompt.focus_handle(cx));
        self.palette = Some(prompt);
        cx.notify();
    }

    /// Grille des icônes, ouverte depuis le menu : un clic donne l'icône, la croix la retire,
    /// un clic ailleurs referme.
    // ponytail: à la souris seulement ; une liste dans la palette si le clavier la réclame.
    pub(super) fn render_icons(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let (at, path) = self.icon_pick.clone()?;
        let t = self.theme;
        let mine = self.icons.get(&path).cloned();
        let pick = |i: usize, file: &'static str, name: Option<&'static str>, cx: &mut Context<Self>| {
            let path = path.clone();
            button(("icon", i), file, name.is_some() && name == mine.as_deref(), t).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.icon_pick = None;
                    this.set_icon(path.clone(), name, cx)
                }),
            )
        };
        let mut cells = vec![pick(0, "close.svg", None, cx)];
        cells.extend(crate::ui::assets::ICON_SET.iter().enumerate().map(|(i, (file, _))| pick(i + 1, file, Some(&file[2..file.len() - 4]), cx)));
        // Huit par rangée ; la grille reste dans la fenêtre, comme le menu.
        let view = window.viewport_size();
        let (width, height) = (px(8. * 30. - 2. + 12.), px(((cells.len() + 7) / 8) as f32 * 30. - 2. + 12.));
        let at = point(
            (at.x - self.nav.left).min(view.width - self.nav.left * 2. - width - px(8.)),
            (at.y - self.nav.left).min(view.height - self.nav.left * 2. - height - px(8.)),
        );
        let close = cx.listener(|this: &mut Self, _: &MouseDownEvent, _, cx| {
            this.icon_pick = None;
            cx.notify();
        });
        Some(
            div().absolute().inset_0().occlude().on_mouse_down(MouseButton::Left, close).child(
                div()
                    .absolute()
                    .left(at.x)
                    .top(at.y)
                    .w(width)
                    .p(px(5.))
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(8.))
                    .shadow_lg()
                    .flex()
                    .flex_wrap()
                    .gap(px(2.))
                    .children(cells),
            ),
        )
    }

    pub(super) fn render_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let t = self.theme;
        let focused = self.nav.focus.is_focused(window);
        let tree = self.nav.mode == Mode::Tree;
        let marked = &self.nav.marked;
        // Glisser une ligne de la sélection multiple emporte tout le lot.
        let lot = marked.iter().next().map(|any| self.targets(any)).unwrap_or_default();
        range
            .filter_map(|ix| {
                let row = self.nav.rows.get(ix)?;
                let selected = match marked.is_empty() {
                    true => self.nav.sel.as_ref() == Some(&row.path),
                    false => marked.contains(&row.path),
                };
                let current = self.pane().path.as_ref() == Some(&row.path);
                // Dans la liste des récents, le dossier de la note sert de détail.
                let detail = match (&self.vault, row.path.parent()) {
                    (Some(root), Some(dir)) if !tree => dir
                        .strip_prefix(root)
                        .map(|d| d.display().to_string())
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                // Sous un tag, le nombre de notes qui le portent.
                let detail = match row.dir {
                    Some(_) if !tree => {
                        let tag = &row.name[1..];
                        self.notes.iter().filter(|n| n.tags.iter().any(|t| t == tag)).count().to_string()
                    }
                    _ => detail,
                };
                // Sous les rétroliens, la ligne de la note qui porte le lien.
                let links = self.nav.mode == Mode::Links;
                let detail = match self.nav.anchor.as_ref().filter(|_| links) {
                    Some(target) => {
                        let note = |path: &Path| self.notes.iter().find(|n| n.path == path);
                        let line = note(target).zip(note(&row.path)).and_then(|(aimed, from)| graph::link_line(&from.body, aimed));
                        line.map(|line| line.trim().to_string()).unwrap_or_default()
                    }
                    None => detail,
                };
                let foldable = !matches!(self.nav.mode, Mode::Recent | Mode::Links);
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
                        .when(foldable, |d| {
                            d.child(div().size(px(14.)).flex_none().when(row.dir.is_some(), |d| {
                                d.child(svg().path(chevron).size(px(14.)).text_color(t.soft))
                            }))
                        })
                        .children(self.icons.get(&row.path).and_then(|name| crate::ui::assets::ICON_SET.iter().find(|(file, _)| file[2..file.len() - 4] == *name)).map(
                            |(file, _)| svg().path(*file).size(px(14.)).flex_none().text_color(if current { t.accent } else { t.text }),
                        ))
                        .child(
                            div()
                                .truncate()
                                // Le nom passe d'abord, en entier tant qu'il tient ; la ligne du lien
                                // prend ce qui reste.
                                .map(|d| if links { d.flex_none().max_w(gpui::relative(0.7)) } else { d.flex_1() })
                                .when(current, |d| d.text_color(t.accent))
                                .child(row.name.clone()),
                        )
                        .child(
                            div()
                                .map(|d| if links { d.flex_1().min_w_0() } else { d.flex_none().max_w(px(110.)) })
                                .truncate()
                                .text_size(px(11.5))
                                .text_color(t.soft)
                                .child(detail),
                        );
                let (path, name) = (row.path.clone(), row.name.clone());
                // Déposer sur une note range dans le dossier de cette note.
                let into = match row.dir {
                    Some(_) => Some(path.clone()),
                    None => path.parent().map(Path::to_path_buf),
                };
                let (target, drop_into) = (path.clone(), into.clone());
                let (paths, name) = match marked.contains(&path) {
                    true => (lot.clone(), format!("{} {}", lot.len(), tr("items", "éléments"))),
                    false => (vec![path], name),
                };
                let selector = format!("nav-row-{}", row.name);
                Some(
                    div()
                        .id(ix)
                        .debug_selector(move || selector.clone())
                        .w_full()
                        .h(ROW)
                        .px_1()
                        .cursor_pointer()
                        .child(line)
                        .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                            let held = e.modifiers();
                            if held.secondary() || held.shift {
                                return this.nav_mark(ix, held.shift, cx);
                            }
                            let folder = this.nav.rows.get(ix).is_some_and(|r| r.dir.is_some());
                            if folder || e.click_count() >= 2 {
                                this.nav_activate(ix, window, cx)
                            } else {
                                this.nav_select(ix, cx)
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                // Hors de la sélection multiple, le clic droit la remplace.
                                if !this.nav.marked.contains(&target) {
                                    this.nav.marked.clear();
                                    this.nav.sel = Some(target.clone());
                                }
                                // Un tag n'a qu'une entrée : la liste de ses notes.
                                if this.vault.as_ref().is_some_and(|root| target.starts_with(root)) || tag_of(&target).is_some() {
                                    this.menu = Some(Menu { at: e.position, target: Some(target.clone()), text: None, commented: false, block: false });
                                }
                                window.focus(&this.nav.focus);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        // Des fichiers venus du système : ils se rangent dans le dossier de la ligne, ou dans celui de
                        // la note, et la ligne se teinte pour le dire.
                        .when(tree, |d| {
                            let aim_into = drop_into.clone();
                            d.drag_over::<gpui::ExternalPaths>(move |style, _, _, _| style.bg(t.selection))
                                .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                                    let Some(dir) = &aim_into else { return };
                                    let mine = matches!(&this.drop_aim, Some(super::DropAim { aim: super::Aim::Folder(d), .. }) if d == dir);
                                    if e.bounds.contains(&e.event.position) {
                                        let aim = super::DropAim { aim: super::Aim::Folder(dir.clone()), items: e.drag(cx).paths().len(), pointer: e.event.position };
                                        this.aim_drop(Some(aim), cx);
                                    } else if mine {
                                        this.aim_drop(None, cx);
                                    }
                                }))
                                .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                                    if let Some(dir) = &drop_into {
                                        this.drop_paths(paths.paths().to_vec(), super::Aim::Folder(dir.clone()), cx);
                                    }
                                }))
                        })
                        .when(tree, |d| {
                            d.on_drag(Dragged { paths, name, theme: t }, |dragged, _, _, cx| {
                                cx.new(|_| dragged.clone())
                            })
                            .when(row.dir.is_some(), |d| {
                                d.drag_over::<Dragged>(move |style, _, _, _| style.bg(t.selection))
                            })
                            .on_drop(cx.listener(move |this, dragged: &Dragged, _, cx| {
                                if let Some(into) = &into {
                                    this.move_all(&dragged.paths, into, cx)
                                }
                            }))
                        }),
                )
            })
            .collect()
    }

    /// Rail, puis panneau et séparateur s'ils sont dépliés.
    pub(super) fn render_nav(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let (mode, panel) = (self.nav.mode, self.nav.panel);
        let unfolded = panel != Panel::Rail;
        if !self.pane().preview {
            self.nav.anchor = self.pane().path.clone();
        }
        let source = self.rows_source();
        if source != self.nav.rows_from {
            self.nav.rows = if unfolded { self.nav_rows() } else { Vec::new() };
            self.nav.rows_from = source;
        }
        let (current, selected) = (self.pane().path.clone(), self.nav.sel.clone());
        self.graph.update(cx, |graph, _| graph.sync(t, current.as_deref(), selected.as_deref()));
        if std::mem::take(&mut self.nav.reveal)
            && let Some(ix) = self.selected_row()
        {
            self.nav.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }

        let travel = |id: &'static str, icon: &'static str, on: bool| {
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .w(px(17.))
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .when(on, |d| d.cursor_pointer().hover(|s| s.bg(t.border)).active(|s| s.bg(t.selection)))
                .child(svg().path(icon).size(px(14.)).flex_none().text_color(t.soft).when(!on, |s| s.opacity(0.35)))
        };
        let mode_button = |id, icon, m: Mode, label: &'static str, keys: String| {
            button(id, icon, unfolded && mode == m, t)
                .tooltip(tip(label, keys, t))
                .on_click(cx.listener(move |this, _, window, cx| this.show_nav(m, true, window, cx)))
        };
        let keys = |action: &dyn Action| crate::ui::keys::of(cx, action);
        let rail = div()
            .id("rail")
            .flex_none()
            .w(RAIL)
            .h_full()
            // Une fenêtre basse : le rail défile, ses derniers boutons restent atteignables.
            .overflow_y_scroll()
            .py_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .when(!unfolded, |d| d.border_r_1().border_color(t.border))
            // Le logo de l'app, en tête du rail sur tous les systèmes.
            .child(div().debug_selector(|| "rail-logo".into()).mb_1().child(crate::ui::assets::logo(t)))
            .child(
                button("nav-commands", "menu.svg", false, t)
                    .debug_selector(|| "nav-commands".into())
                    .tooltip(tip(tr("Commands", "Commandes"), keys(&crate::actions::ToggleCommands), t))
                    .on_click(cx.listener(|this, _, window, cx| this.open_commands(window, cx))),
            )
            // Précédent et suivant, côte à côte ; estompés quand il n'y a nulle part où aller.
            .child(
                div()
                    .flex()
                    .gap(px(2.))
                    .child(
                        travel("nav-back", "chevron-left.svg", self.history.can_back())
                            .tooltip(tip(tr("Back", "Précédent"), keys(&crate::actions::GoBack), t))
                            .on_click(cx.listener(|this, _, window, cx| this.travel(false, window, cx))),
                    )
                    .child(
                        travel("nav-forward", "chevron-right.svg", self.history.can_forward())
                            .tooltip(tip(tr("Forward", "Suivant"), keys(&crate::actions::GoForward), t))
                            .on_click(cx.listener(|this, _, window, cx| this.travel(true, window, cx))),
                    ),
            )
            .child(mode_button("nav-tree", "tree.svg", Mode::Tree, tr("Vault tree", "Arbre du coffre"), keys(&ShowTree)))
            .child(mode_button("nav-recent", "clock.svg", Mode::Recent, tr("Recent notes", "Notes récentes"), keys(&ShowRecent)))
            .child(mode_button("nav-graph", "graph.svg", Mode::Graph, tr("Graph of the notes", "Graphe des notes"), keys(&ShowGraph)))
            .child(mode_button("nav-tags", "tag.svg", Mode::Tags, "Tags", keys(&ShowTags)))
            .child(mode_button("nav-links", "backlink.svg", Mode::Links, tr("Backlinks", "Rétroliens"), keys(&ShowLinks)))
            .child(mode_button("nav-calendar", "i-calendar.svg", Mode::Calendar, tr("Calendar", "Calendrier"), keys(&ShowCalendar)))
            .child(div().w(px(16.)).h(px(1.)).my_1().bg(t.border))
            .child(
                button("nav-search", "search.svg", false, t)
                    .tooltip(tip(tr("Find or create a note", "Chercher ou créer une note"), keys(&crate::actions::OpenPalette), t))
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette("", window, cx))),
            )
            .child(
                button("nav-new", "plus.svg", false, t)
                    .tooltip(tip(tr("New note", "Nouvelle note"), keys(&crate::actions::NewNote), t))
                    .on_click(cx.listener(|this, _, window, cx| this.new_note_here(window, cx))),
            )
            .child(div().flex_1().w_full().when(!self.nav.native_bar, |d| d.map(crate::ui::widgets::drag_window)))
            // Corbeille du coffre (ce qu'elle contient, pour le reprendre) et sauvegarde.
            .child(
                button("nav-trash", "trash.svg", false, t)
                    .tooltip(tip(tr("Trash: restore a note", "Corbeille : restaurer une note"), String::new(), t))
                    .on_click(cx.listener(|this, _, window, cx| this.open_trash(window, cx))),
            )
            .child(
                button("nav-backup", "backup.svg", false, t)
                    .tooltip(tip(tr("Back up the vault", "Sauvegarder le coffre"), String::new(), t))
                    .on_click(cx.listener(|this, _, window, cx| this.choose_backup(window, cx))),
            )
            .child(button("nav-theme", "theme.svg", false, t).tooltip(tip(tr("Theme", "Thème"), String::new(), t)).on_click(cx.listener(
                |this, _, window, cx| this.choose_setting(Setting::Theme, window, cx),
            )))
            .child(
                button("nav-help", "help.svg", false, t)
                    .tooltip(tip(tr("Shortcuts and help", "Raccourcis et aide"), keys(&crate::actions::ToggleHelp), t))
                    .on_click(cx.listener(|this, _, window, cx| this.set_help(true, window, cx))),
            );

        let wrapper = div().h_full().flex().min_w_0();
        if !unfolded {
            return wrapper.flex_none().child(rail);
        }

        let full = panel == Panel::Full;
        let folded = self.all_folded();
        let title = match (mode, &self.vault) {
            (Mode::Tree, Some(root)) => vault::stem(root),
            (Mode::Graph, _) => tr("Graph", "Graphe").to_string(),
            (Mode::Tags, _) => "Tags".to_string(),
            (Mode::Calendar, _) => tr("Calendar", "Calendrier").to_string(),
            (Mode::Links, _) => {
                let name = self.nav.anchor.as_deref().map(vault::stem).unwrap_or_default();
                format!("{} {name}", tr("Links to", "Liens vers"))
            }
            _ => tr("Recent", "Récents").to_string(),
        };
        let header = div()
            .flex_none()
            .h(px(44.))
            .pl_2()
            .pr_1()
            .flex()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(t.soft)
                    .when(!self.nav.native_bar, |d| d.map(crate::ui::widgets::drag_window))
                    .child(title),
            )
            .when(mode == Mode::Graph, |d| {
                d.child(button("nav-center", "target.svg", false, t).tooltip(tip(tr("Show the whole graph", "Voir tout le graphe"), String::new(), t)).on_click(cx.listener(
                    |this, _, _, cx| this.graph.update(cx, |graph, cx| graph.recenter(cx)),
                )))
            })
            .when(mode == Mode::Tree, |d| {
                // Une note à la racine du coffre, quelle que soit la sélection.
                d.child(button("nav-file", "file-plus.svg", false, t).tooltip(tip(tr("New note at the root", "Nouvelle note à la racine"), String::new(), t)).on_click(cx.listener(
                    |this, _, window, cx| this.new_note_in(None, window, cx),
                )))
                .child(button("nav-folder", "folder-plus.svg", false, t).tooltip(tip(tr("New folder", "Nouveau dossier"), keys(&NewFolder), t)).on_click(cx.listener(
                    |this, _, window, cx| this.menu_do(Do::NewFolder, this.nav.sel.clone(), window, cx),
                )))
                // Tout replier ; si tout l'est déjà, tout déplier.
                .child(button("nav-fold", if folded { "unfold.svg" } else { "fold.svg" }, false, t).tooltip(tip(if folded { tr("Unfold every folder", "Déplier tous les dossiers") } else { tr("Fold every folder", "Replier tous les dossiers") }, String::new(), t)).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.fold_all();
                        cx.notify();
                    }),
                ))
            })
            .child(
                button("nav-full", if full { "shrink.svg" } else { "expand.svg" }, false, t)
                    .tooltip(tip(if full { tr("Back beside the note", "Revenir à côté de la note") } else { tr("Panel on the whole window", "Panneau en pleine fenêtre") }, keys(&ToggleFull), t))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_full(window, cx))),
            );
        let list = uniform_list("nav-rows", self.nav.rows.len(), cx.processor(Self::render_rows))
            .track_scroll(self.nav.scroll.clone())
            .flex_1()
            .min_h_0()
            // Hors de toute ligne : déposer range à la racine, le clic droit vise le coffre.
            .when(mode == Mode::Tree, |d| {
                // Des fichiers du système lâchés hors de toute ligne : à la racine du coffre.
                d.on_drag_move(cx.listener(|this, e: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                    let root = matches!(&this.drop_aim, Some(super::DropAim { aim: super::Aim::Root, .. }));
                    if !e.bounds.contains(&e.event.position) {
                        if root {
                            this.aim_drop(None, cx);
                        }
                    } else if this.drop_aim.is_none() || root {
                        let aim = super::DropAim { aim: super::Aim::Root, items: e.drag(cx).paths().len(), pointer: e.event.position };
                        this.aim_drop(Some(aim), cx);
                    }
                }))
                .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                    this.drop_paths(paths.paths().to_vec(), super::Aim::Root, cx);
                }))
                .on_drop(cx.listener(|this, dragged: &Dragged, _, cx| {
                    if let Some(root) = this.vault.clone() {
                        this.move_all(&dragged.paths, &root, cx)
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        this.menu = Some(Menu { at: e.position, target: None, text: None, commented: false, block: false });
                        cx.notify();
                    }),
                )
            });
        let width = self.split_width();
        let content = div()
            .debug_selector(|| "nav-panel".into())
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
            .on_action(cx.listener(|this, _: &PrevMonth, _, cx| this.day_step(0, -1, cx)))
            .on_action(cx.listener(|this, _: &NextMonth, _, cx| this.day_step(0, 1, cx)))
            .on_action(cx.listener(|this, _: &Open, window, cx| {
                if this.nav.mode == Mode::Calendar {
                    return this.open_day(this.nav.day, cx);
                }
                if let Some(ix) = this.selected_row() {
                    this.nav_activate(ix, window, cx)
                }
            }))
            .when(!matches!(mode, Mode::Graph | Mode::Calendar), |d| {
                d.on_action(cx.listener(|this, _: &Rename, window, cx| {
                    this.menu_do(Do::Rename, this.nav.sel.clone(), window, cx)
                }))
                .on_action(cx.listener(|this, _: &Duplicate, window, cx| {
                    this.menu_do(Do::Duplicate, this.nav.sel.clone(), window, cx)
                }))
                .on_action(cx.listener(|this, _: &Trash, window, cx| {
                    this.menu_do(Do::Trash, this.nav.sel.clone(), window, cx)
                }))
            })
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if this.menu.take().is_some() || this.icon_pick.take().is_some() {
                    return cx.notify();
                }
                if this.nav.panel == Panel::Full {
                    this.nav.panel = Panel::Split;
                }
                window.focus(&this.pane().editor.focus_handle(cx));
                this.settle_nav(window, cx);
            }))
            .child(header)
            .map(|d| match mode {
                Mode::Graph => d.child(div().flex_1().min_h_0().child(self.graph.clone())),
                Mode::Calendar => d.child(self.render_calendar(cx)),
                _ => d.child(list),
            });
        // Poignée de 5 px autour d'un trait de 1 px ; double-clic : largeur d'origine.
        let divider = div()
            .id("nav-divider")
            .flex_none()
            .w(DIVIDER)
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .group("nav-divider")
            .child(
                div()
                    .w(px(1.))
                    .h_full()
                    .bg(if self.nav.dragging { t.accent } else { t.border })
                    .group_hover("nav-divider", |s| s.bg(t.accent)),
            )
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
