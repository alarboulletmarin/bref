//! Graphe du coffre : un nœud par note, une arête par `[[lien]]` vers une note
//! existante. On s'y déplace à la souris (glisser, molette) ou au clavier, et
//! un nœud se déplace en le tirant : ses voisins suivent.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use gpui::{
    App, BorderStyle, Bounds, ContentMask, Context, CursorStyle, EventEmitter, FocusHandle,
    Focusable, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, PathBuilder, Pixels, Point,
    ScrollWheelEvent, SharedString, TextRun, Window, actions, canvas, div, font, point, prelude::*,
    px, quad, size,
};

use crate::{
    Theme,
    nav::{Fold, Next, Open, Prev, Unfold},
    sans,
    vault::{self, Note},
};

actions!(graph, [Cycle]);

/// Pixels par unité de disposition en deçà desquels les noms sont masqués.
const LABEL_ZOOM: f32 = 30.;
/// Frames pendant lesquelles les nœuds entraînés finissent de se poser après un glisser.
const SETTLE: u32 = 60;

pub enum GraphEvent {
    /// Un nœud est sélectionné : la note peut s'afficher en aperçu.
    Select(PathBuf),
    Open(PathBuf),
}

/// Nom d'une image dans le graphe et dans les liens : son nom de fichier.
pub fn image_name(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Arêtes `(i, j)`, `i < j`, sans doublon : les wikiliens qui visent une note du
/// coffre, et les images affichées. Les images sont numérotées après les notes.
pub fn edges(notes: &[Note], images: &[PathBuf]) -> Vec<(usize, usize)> {
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (i, note) in notes.iter().enumerate() {
        by_name.entry(note.name.to_lowercase()).or_insert(i);
    }
    for (i, image) in images.iter().enumerate() {
        by_name.entry(image_name(image).to_lowercase()).or_insert(notes.len() + i);
    }
    let mut edges: Vec<(usize, usize)> = notes
        .iter()
        .enumerate()
        .flat_map(|(i, note)| note.links.iter().filter_map(move |link| Some((i, link))))
        .filter_map(|(i, link)| {
            let j = *by_name.get(link)?;
            (i != j).then_some((i.min(j), i.max(j)))
        })
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges
}

/// Position de départ du nœud `i` : une spirale, identique d'un lancement à l'autre.
fn spiral(i: usize) -> (f32, f32) {
    let (r, a) = ((i as f32 + 0.5).sqrt(), i as f32 * 2.399_963);
    (r * a.cos(), r * a.sin())
}

/// Disposition par forces (Fruchterman-Reingold) : les nœuds proches se
/// repoussent, les arêtes les rapprochent, une légère gravité ramène vers le
/// centre les notes sans lien. L'unité est la longueur idéale d'une arête.
/// `seed` reprend des positions connues.
// ponytail: répulsion en O(n²) par itération, correcte jusqu'à ~1500 notes ;
// passer à Barnes-Hut si les coffres grossissent.
pub fn layout(seed: &[Option<(f32, f32)>], edges: &[(usize, usize)]) -> Vec<(f32, f32)> {
    let n = seed.len();
    let mut pos: Vec<(f32, f32)> =
        seed.iter().enumerate().map(|(i, s)| s.unwrap_or_else(|| spiral(i))).collect();
    let iterations = (40_000_000 / (n * n).max(1)).clamp(30, 300);
    let start = 0.5 + (n as f32).sqrt() * 0.1;
    let mut push = vec![(0f32, 0f32); n];
    for step in 0..iterations {
        push.fill((0., 0.));
        for i in 0..n {
            for j in i + 1..n {
                let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                let d2 = (dx * dx + dy * dy).max(0.01);
                // Portée limitée : sans elle, les notes isolées partent très loin.
                if d2 > 9. {
                    continue;
                }
                let (fx, fy) = (dx / d2, dy / d2);
                push[i] = (push[i].0 + fx, push[i].1 + fy);
                push[j] = (push[j].0 - fx, push[j].1 - fy);
            }
        }
        for &(i, j) in edges {
            let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
            let d = (dx * dx + dy * dy).sqrt();
            let (fx, fy) = (dx * d, dy * d);
            push[i] = (push[i].0 - fx, push[i].1 - fy);
            push[j] = (push[j].0 + fx, push[j].1 + fy);
        }
        // Le pas maximal décroît : les nœuds se figent peu à peu.
        let heat = start * (1. - step as f32 / iterations as f32) + 0.01;
        for (p, f) in pos.iter_mut().zip(&push) {
            let (fx, fy) = (f.0 - p.0 * 0.05, f.1 - p.1 * 0.05);
            let len = (fx * fx + fy * fy).sqrt().max(1e-6);
            let by = len.min(heat) / len;
            *p = (p.0 + fx * by, p.1 + fy * by);
        }
    }
    pos
}

/// Un pas du glisser : le déplacement du nœud tenu se propage le long des liens.
/// Chaque nœud lié suit la moyenne des déplacements de ses voisins et revient
/// un peu vers sa place `rest` ; plus on s'éloigne du nœud tenu, moins ça bouge.
// ponytail: pas de répulsion pendant le glisser, un nœud entraîné peut en
// recouvrir un autre ; relancer `layout` au lâcher si cela gêne.
fn follow(pos: &mut [(f32, f32)], rest: &[(f32, f32)], near: &[Vec<usize>], held: usize) {
    let shift: Vec<(f32, f32)> = pos.iter().zip(rest).map(|(p, r)| (p.0 - r.0, p.1 - r.1)).collect();
    for (i, links) in near.iter().enumerate() {
        if i == held || links.is_empty() {
            continue;
        }
        let n = links.len() as f32;
        let mean = links.iter().fold((0., 0.), |a, &j| (a.0 + shift[j].0 / n, a.1 + shift[j].1 / n));
        let ease = |own: f32, mean: f32| own + 0.12 * (mean - own) - 0.012 * own;
        pos[i] = (rest[i].0 + ease(shift[i].0, mean.0), rest[i].1 + ease(shift[i].1, mean.1));
    }
}

pub struct Graph {
    focus: FocusHandle,
    theme: Theme,
    nodes: Vec<(PathBuf, SharedString)>,
    edges: Vec<(usize, usize)>,
    near: Vec<Vec<usize>>,
    pos: Vec<(f32, f32)>,
    generation: usize,
    /// Décalage (px) du centre du graphe par rapport au centre de la vue.
    pan: (f32, f32),
    /// Pixels par unité de disposition.
    zoom: f32,
    /// Recadrer sur tout le graphe à la prochaine frame.
    fit: bool,
    hover: Option<usize>,
    selected: Option<usize>,
    current: Option<usize>,
    /// Dernière position du pointeur pendant un glisser du fond.
    grab: Option<Point<Pixels>>,
    /// Nœud tenu à la souris.
    drag: Option<usize>,
    /// Glisser en cours ou qui se pose : (nœud tiré, frames restantes), et la
    /// place des nœuds avant qu'il commence.
    pull: Option<(usize, u32)>,
    rest: Vec<(f32, f32)>,
    /// Cadrage (décalage, zoom) vers lequel la vue glisse.
    aim: Option<((f32, f32), f32)>,
    /// Parcours des voisins avec Tab : (nœud de départ, rang du voisin).
    cycle: Option<(usize, usize)>,
    bounds: Bounds<Pixels>,
}

impl EventEmitter<GraphEvent> for Graph {}

impl Focusable for Graph {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Graph {
    pub fn new(focus: FocusHandle, theme: Theme) -> Self {
        Self {
            focus,
            theme,
            nodes: Vec::new(),
            edges: Vec::new(),
            near: Vec::new(),
            pos: Vec::new(),
            generation: 0,
            pan: (0., 0.),
            zoom: 60.,
            fit: true,
            hover: None,
            selected: None,
            current: None,
            grab: None,
            drag: None,
            pull: None,
            rest: Vec::new(),
            aim: None,
            cycle: None,
            bounds: Bounds::default(),
        }
    }

    /// Reconstruit le graphe ; les notes déjà placées gardent leur position de
    /// départ, et la disposition se calcule hors du thread UI.
    /// Les nœuds du graphe : les notes, puis les images que des notes affichent.
    pub fn set_notes(&mut self, notes: &[Note], images: &[PathBuf], cx: &mut Context<Self>) {
        let nodes: Vec<(PathBuf, SharedString)> = notes
            .iter()
            .map(|n| (n.path.clone(), n.name.clone().into()))
            .chain(images.iter().map(|p| (p.clone(), image_name(p).into())))
            .collect();
        let known: HashMap<&Path, (f32, f32)> =
            self.nodes.iter().zip(&self.pos).map(|((path, _), p)| (path.as_path(), *p)).collect();
        let seed: Vec<Option<(f32, f32)>> =
            nodes.iter().map(|(path, _)| known.get(path.as_path()).copied()).collect();
        drop(known);
        let first = self.nodes.is_empty();
        self.edges = edges(notes, images);
        self.near = vec![Vec::new(); nodes.len()];
        for &(i, j) in &self.edges {
            self.near[i].push(j);
            self.near[j].push(i);
        }
        self.pos = seed.iter().enumerate().map(|(i, s)| s.unwrap_or_else(|| spiral(i))).collect();
        self.nodes = nodes;
        (self.hover, self.cycle, self.drag, self.pull) = (None, None, None, None);
        self.generation += 1;
        let (generation, edges) = (self.generation, self.edges.clone());
        cx.spawn(async move |this, cx| {
            let pos = cx.background_executor().spawn(async move { layout(&seed, &edges) }).await;
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.pos = pos;
                    this.fit |= first;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Thème, note ouverte et note sélectionnée, tels que la fenêtre les connaît.
    pub fn sync(&mut self, theme: Theme, current: Option<&Path>, selected: Option<&Path>) {
        let find = |path: Option<&Path>| self.nodes.iter().position(|(p, _)| Some(p.as_path()) == path);
        (self.current, self.selected) = (find(current), find(selected));
        self.theme = theme;
    }

    /// Nombre de nœuds et d'arêtes.
    #[cfg(test)]
    pub fn size(&self) -> (usize, usize) {
        (self.nodes.len(), self.edges.len())
    }

    /// La vue glisse jusqu'à cadrer tout le graphe.
    pub fn recenter(&mut self, cx: &mut Context<Self>) {
        self.aim = self.framing();
        cx.notify();
    }

    /// Position à l'écran du nœud de cette note.
    #[cfg(test)]
    pub fn spot(&self, path: &Path) -> Point<Pixels> {
        self.screen(self.nodes.iter().position(|(p, _)| p == path).unwrap())
    }

    fn screen(&self, i: usize) -> Point<Pixels> {
        let c = self.bounds.center();
        point(
            c.x + px(self.pan.0 + self.pos[i].0 * self.zoom),
            c.y + px(self.pan.1 + self.pos[i].1 * self.zoom),
        )
    }

    fn radius(&self, i: usize) -> f32 {
        let grown = if self.hover == Some(i) || self.drag == Some(i) { 1.5 } else { 0. };
        (3. + 1.5 * (self.near[i].len() as f32).sqrt()) * (self.zoom / 40.).clamp(0.6, 1.5) + grown
    }

    /// Décalage et zoom qui montrent tout le graphe.
    fn framing(&self) -> Option<((f32, f32), f32)> {
        let (w, h) = (f32::from(self.bounds.size.width), f32::from(self.bounds.size.height));
        if self.pos.is_empty() || w <= 0. || h <= 0. {
            return None;
        }
        let (mut min, mut max) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for p in &self.pos {
            min = (min.0.min(p.0), min.1.min(p.1));
            max = (max.0.max(p.0), max.1.max(p.1));
        }
        // Une unité de marge de chaque côté, pour les noms.
        let zoom = (w / (max.0 - min.0 + 2.)).min(h / (max.1 - min.1 + 2.)).clamp(6., 80.);
        Some(((-(min.0 + max.0) / 2. * zoom, -(min.1 + max.1) / 2. * zoom), zoom))
    }

    /// Avance les animations d'une frame : glissement de la vue, nœuds entraînés.
    fn animate(&mut self, window: &mut Window) {
        if let Some((pan, zoom)) = self.aim {
            let toward = |from: f32, to: f32| from + (to - from) * 0.2;
            self.pan = (toward(self.pan.0, pan.0), toward(self.pan.1, pan.1));
            self.zoom = toward(self.zoom, zoom);
            if (self.zoom - zoom).abs() < 0.05 && (self.pan.0 - pan.0).hypot(self.pan.1 - pan.1) < 0.5 {
                (self.pan, self.zoom, self.aim) = (pan, zoom, None);
            }
        }
        if let Some((held, left)) = self.pull {
            follow(&mut self.pos, &self.rest, &self.near, held);
            let left = if self.drag.is_some() { SETTLE } else { left - 1 };
            self.pull = (left > 0).then_some((held, left));
        }
        if self.aim.is_some() || self.pull.is_some() {
            window.request_animation_frame();
        }
    }

    fn node_at(&self, at: Point<Pixels>) -> Option<usize> {
        (0..self.pos.len())
            .map(|i| {
                let p = self.screen(i);
                let (dx, dy) = (f32::from(p.x - at.x), f32::from(p.y - at.y));
                (i, (dx * dx + dy * dy).sqrt() - self.radius(i))
            })
            .filter(|(_, d)| *d < 5.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn select(&mut self, i: usize, cx: &mut Context<Self>) {
        self.selected = Some(i);
        // Le nœud choisi au clavier peut être hors champ : on le ramène au centre.
        if !self.bounds.inset(px(24.)).contains(&self.screen(i)) {
            self.pan = (-self.pos[i].0 * self.zoom, -self.pos[i].1 * self.zoom);
        }
        cx.emit(GraphEvent::Select(self.nodes[i].0.clone()));
        cx.notify();
    }

    /// Flèches : le nœud le plus proche dans cette direction.
    fn step(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        self.cycle = None;
        let Some(from) = self.selected.or(self.current) else {
            // Rien de sélectionné : on part du nœud le plus proche du centre de la vue.
            let c = self.bounds.center();
            let far = |i: usize| {
                let p = self.screen(i);
                f32::from(p.x - c.x).powi(2) + f32::from(p.y - c.y).powi(2)
            };
            let nearest = (0..self.pos.len()).min_by(|&a, &b| far(a).total_cmp(&far(b)));
            return nearest.map_or((), |i| self.select(i, cx));
        };
        let o = self.pos[from];
        let best = (0..self.pos.len())
            .filter(|&i| i != from)
            .filter_map(|i| {
                let (x, y) = (self.pos[i].0 - o.0, self.pos[i].1 - o.1);
                let (along, across) = (x * dx + y * dy, (x * dy - y * dx).abs());
                (along > 0. && across <= along * 1.5).then_some((i, along + across * 2.))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = best {
            self.select(i, cx);
        }
    }

    /// Tab : parcourt les notes liées à celle d'où l'on est parti.
    fn cycle(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.selected.or(self.current) else {
            return;
        };
        let (from, rank) = match self.cycle {
            Some((from, rank)) if self.near[from].get(rank) == Some(&selected) => (from, rank + 1),
            _ => (selected, 0),
        };
        if let Some(&next) = self.near[from].get(rank % self.near[from].len().max(1)) {
            self.cycle = Some((from, rank % self.near[from].len()));
            self.select(next, cx);
        }
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        (self.cycle, self.aim) = (None, None);
        match self.node_at(e.position) {
            Some(i) if e.click_count >= 2 => cx.emit(GraphEvent::Open(self.nodes[i].0.clone())),
            Some(i) => {
                (self.drag, self.pull) = (Some(i), None);
                self.select(i, cx)
            }
            None => self.grab = Some(e.position),
        }
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(last) = self.grab
            && e.pressed_button == Some(MouseButton::Left)
        {
            self.pan.0 += f32::from(e.position.x - last.x);
            self.pan.1 += f32::from(e.position.y - last.y);
            self.grab = Some(e.position);
            return cx.notify();
        }
        // Le nœud tenu suit le pointeur ; ses voisins sont entraînés à chaque frame.
        if let Some(i) = self.drag
            && e.pressed_button == Some(MouseButton::Left)
        {
            let c = self.bounds.center();
            let at = (
                (f32::from(e.position.x - c.x) - self.pan.0) / self.zoom,
                (f32::from(e.position.y - c.y) - self.pan.1) / self.zoom,
            );
            if self.pull.is_none() {
                self.rest = self.pos.clone();
                self.pull = Some((i, SETTLE));
            }
            self.pos[i] = at;
            return cx.notify();
        }
        (self.grab, self.drag) = (None, None);
        let hover = self.node_at(e.position);
        if hover != self.hover {
            self.hover = hover;
            cx.notify();
        }
    }

    /// La molette zoome autour du pointeur.
    fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let zoom = (self.zoom * (f32::from(e.delta.pixel_delta(px(26.)).y) * 0.004).exp()).clamp(4., 240.);
        let c = self.bounds.center();
        let (mx, my) = (f32::from(e.position.x - c.x), f32::from(e.position.y - c.y));
        let k = zoom / self.zoom;
        self.pan = (mx - (mx - self.pan.0) * k, my - (my - self.pan.1) * k);
        (self.zoom, self.aim) = (zoom, None);
        cx.notify();
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        self.bounds = bounds;
        if self.fit
            && let Some((pan, zoom)) = self.framing()
        {
            (self.pan, self.zoom, self.fit) = (pan, zoom, false);
        }
        self.animate(window);
        let t = self.theme;
        let focus = self.hover.or(self.selected);
        let lit = |i: usize| focus.is_none_or(|f| f == i || self.near[f].contains(&i));
        let fade = |color: Hsla, on: bool| if on { color } else { t.bg.blend(color.opacity(0.4)) };

        let mut plain = PathBuilder::stroke(px(1.));
        let mut strong = PathBuilder::stroke(px(1.5));
        let (mut any_plain, mut any_strong) = (false, false);
        for &(i, j) in &self.edges {
            let (a, b) = (self.screen(i), self.screen(j));
            if !bounds.contains(&a) && !bounds.contains(&b) {
                continue;
            }
            let line = if focus == Some(i) || focus == Some(j) {
                any_strong = true;
                &mut strong
            } else {
                any_plain = true;
                &mut plain
            };
            line.move_to(a);
            line.line_to(b);
        }
        let edge_color = t.dim.opacity(if focus.is_none() { 0.5 } else { 0.3 });
        if any_plain && let Ok(path) = plain.build() {
            window.paint_path(path, edge_color);
        }
        if any_strong && let Ok(path) = strong.build() {
            window.paint_path(path, t.accent.opacity(0.8));
        }

        let disc = |c: Point<Pixels>, r: f32| Bounds::new(point(c.x - px(r), c.y - px(r)), size(px(r * 2.), px(r * 2.)));
        for i in 0..self.pos.len() {
            let (c, r) = (self.screen(i), self.radius(i));
            if !bounds.dilate(px(r)).contains(&c) {
                continue;
            }
            let on = lit(i);
            let color = if self.current == Some(i) { t.accent } else { t.bg.blend(t.text.opacity(0.7)) };
            if self.selected == Some(i) {
                let ring = disc(c, r + 4.);
                window.paint_quad(quad(ring, px(r + 4.), t.selection, px(1.5), t.accent, BorderStyle::default()));
            }
            // Une image : un carré, pour la distinguer d'une note.
            let corner = if vault::is_image(&self.nodes[i].0) { r * 0.3 } else { r };
            window.paint_quad(quad(disc(c, r), px(corner), fade(color, on), px(0.), t.bg, BorderStyle::default()));

            let named = self.zoom >= LABEL_ZOOM || focus.is_some_and(|_| on) || self.current == Some(i);
            if !named {
                continue;
            }
            let name = self.nodes[i].1.clone();
            let run = TextRun {
                len: name.len(),
                font: font(sans()),
                color: if focus == Some(i) || self.current == Some(i) { t.text } else { fade(t.dim, on) },
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let label = window.text_system().shape_line(name, px(11.5), &[run], None);
            let origin = point(c.x - label.width / 2., c.y + px(r + 3.));
            label.paint(origin, px(15.), window, cx).ok();
        }
    }
}

impl Render for Graph {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let drawing = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    this.update(cx, |graph, cx| graph.paint(bounds, window, cx))
                })
            },
        )
        .size_full();
        div()
            .size_full()
            .track_focus(&self.focus)
            .when(self.hover.is_some(), |d| d.cursor_pointer())
            .when(self.drag.is_some(), |d| d.cursor(CursorStyle::ClosedHand))
            .on_action(cx.listener(|this, _: &Prev, _, cx| this.step(0., -1., cx)))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.step(0., 1., cx)))
            .on_action(cx.listener(|this, _: &Fold, _, cx| this.step(-1., 0., cx)))
            .on_action(cx.listener(|this, _: &Unfold, _, cx| this.step(1., 0., cx)))
            .on_action(cx.listener(|this, _: &Cycle, _, cx| this.cycle(cx)))
            .on_action(cx.listener(|this, _: &Open, _, cx| {
                if let Some(i) = this.selected {
                    cx.emit(GraphEvent::Open(this.nodes[i].0.clone()))
                }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    (this.grab, this.drag) = (None, None);
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(drawing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn note(name: &str, links: &[&str]) -> Note {
        Note {
            name: name.into(),
            path: PathBuf::from(format!("/v/{name}.md")),
            tags: Vec::new(),
            links: links.iter().map(|l| l.to_string()).collect(),
            mtime: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn resolves_edges() {
        let notes = [note("A", &["b", "a", "absente"]), note("B", &["a", "c"]), note("c", &[])];
        // Sans doublon, sans boucle, sans lien vers une note inexistante.
        assert_eq!(edges(&notes, &[]), [(0, 1), (1, 2)]);
        // Une image affichée par une note lui est reliée ; elle vient après les notes.
        let notes = [note("a", &["plan.png"]), note("b", &[])];
        assert_eq!(edges(&notes, &[PathBuf::from("/v/img/Plan.PNG")]), [(0, 2)]);
    }

    #[test]
    fn lays_out_linked_notes_closer() {
        let dist = |p: &[(f32, f32)], i: usize, j: usize| (p[i].0 - p[j].0).hypot(p[i].1 - p[j].1);
        let pos = layout(&[None; 6], &[(0, 1), (1, 2)]);
        assert!(pos.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
        assert!(dist(&pos, 0, 1) < dist(&pos, 0, 4));
        assert!(dist(&pos, 1, 2) < dist(&pos, 2, 5));
        // Aucun nœud ne se superpose à un autre, et le résultat est reproductible.
        assert!((0..6).all(|i| (i + 1..6).all(|j| dist(&pos, i, j) > 0.3)));
        assert_eq!(pos, layout(&[None; 6], &[(0, 1), (1, 2)]));
    }

    #[test]
    fn dragged_node_pulls_its_neighbours() {
        // Chaîne 0-1-2, et 3 sans lien. Le nœud 0 est tiré de 5 unités vers la droite.
        let rest = [(0., 0.), (1., 0.), (2., 0.), (0., 2.)];
        let near = [vec![1], vec![0, 2], vec![1], vec![]];
        let mut pos = rest;
        pos[0] = (5., 0.);
        for _ in 0..SETTLE {
            follow(&mut pos, &rest, &near, 0);
        }
        let moved = |i: usize| pos[i].0 - rest[i].0;
        // Il reste sous le pointeur ; ses voisins suivent, de moins en moins loin.
        assert_eq!((pos[0], pos[3]), ((5., 0.), rest[3]));
        assert!(moved(1) > 3. && moved(1) < 5. && moved(2) > 2. && moved(2) < moved(1));
    }
}
