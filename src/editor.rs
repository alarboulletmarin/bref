//! Éditeur Markdown stylé en direct : le texte reste du Markdown brut, seul le
//! rendu change (tailles, graisses, couleurs).

use std::{
    borrow::Cow,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    ops::Range,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    App, BorderStyle, Bounds, ClipboardEntry, ClipboardItem, Context, Corners, CursorStyle,
    ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, Font, FontStyle, FontWeight, GlobalElementId, Hitbox, HitboxBehavior, Hsla, Image,
    ImageFormat, ImgResourceLoader, InspectorElementId, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderImage, Resource, Rgba, ScrollWheelEvent,
    SharedString, Size, StrikethroughStyle, Style, TextAlign, TextRun, TransformationMatrix,
    UTF16Selection, UnderlineStyle, Window, WrappedLine, actions, div, fill, font, point,
    prelude::*, px, quad, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    MOD, Theme, figure, mono, sans, tr, vault,
    markdown::{self as md, Enter, Kind, Link},
};

actions!(
    editor,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        Home,
        End,
        DocStart,
        DocEnd,
        PageUp,
        PageDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectHome,
        SelectEnd,
        SelectDocStart,
        SelectDocEnd,
        SelectAll,
        Newline,
        Indent,
        Outdent,
        ToggleTask,
        Bold,
        Italic,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Cancel,
    ]
);

const TOP: Pixels = px(20.);
const MAX_WIDTH: Pixels = px(720.);
/// Côté du bouton de copie, à droite de l'ouverture d'un bloc de code.
const COPY_SIZE: Pixels = px(22.);
/// Corps des formules, par rapport au texte : la police mathématique paraît petite à taille égale.
const MATH_SCALE: f32 = 1.2;
/// Espace entre une ligne et l'image qu'elle désigne.
const IMAGE_GAP: Pixels = px(8.);

#[derive(Clone, Copy, PartialEq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

pub enum EditorEvent {
    Changed,
    Open(Link),
}

/// Une ligne logique mise en page (elle peut occuper plusieurs lignes visuelles).
struct Row {
    start: usize,
    len: usize,
    y: Pixels,
    pad: Pixels,
    lh: Pixels,
    height: Pixels,
    kind: Kind,
    line: Option<WrappedLine>,
    /// Signes affichés à la place de leur écriture ASCII : (octet de début,
    /// longueur dans le texte, longueur affichée).
    subs: Vec<(usize, usize, usize)>,
    /// Image désignée par la ligne, ou figure (diagramme, formule) qu'elle
    /// termine, dessinée dessous, et sa taille à l'écran.
    image: Option<(Arc<RenderImage>, Size<Pixels>)>,
    /// La ligne ouvre un bloc de code : elle porte le bouton de copie.
    opens: bool,
}

impl Row {
    fn pos(&self, i: usize) -> Point<Pixels> {
        self.line
            .as_ref()
            .and_then(|l| l.position_for_index(self.shown(i.min(self.len)), self.lh))
            .unwrap_or_default()
    }

    /// Octet du texte → octet du texte affiché.
    fn shown(&self, i: usize) -> usize {
        let mut shift = 0isize;
        for &(at, src, dst) in &self.subs {
            if i <= at {
                break;
            }
            // À l'intérieur d'un signe : on se place juste après lui.
            if i < at + src {
                return (at as isize + shift) as usize + dst;
            }
            shift += dst as isize - src as isize;
        }
        (i as isize + shift) as usize
    }

    /// Octet du texte affiché → octet du texte.
    fn source(&self, i: usize) -> usize {
        let mut shift = 0isize;
        for &(at, src, dst) in &self.subs {
            let start = (at as isize + shift) as usize;
            if i <= start {
                break;
            }
            if i < start + dst {
                return at + src;
            }
            shift += dst as isize - src as isize;
        }
        (i as isize - shift) as usize
    }

    fn image_height(&self) -> Pixels {
        self.image.as_ref().map_or(px(0.), |(_, s)| s.height + IMAGE_GAP)
    }

    fn index_at(&self, p: Point<Pixels>) -> usize {
        match &self.line {
            Some(l) => match l.closest_index_for_position(p, self.lh) {
                Ok(i) | Err(i) => self.source(i),
            },
            None => 0,
        }
    }

    fn visual_rows(&self) -> i32 {
        (f32::from(self.height - self.pad - self.image_height()) / f32::from(self.lh)).round() as i32
    }
}

/// Figure prête à dessiner (`None` : source invalide) et sa taille.
type Drawing = Option<(Arc<Image>, Size<Pixels>)>;

/// La figure de clé `key`, reprise de la frame précédente ou dessinée par `make` ;
/// si `lazy`, seulement reprise.
fn cached(
    old: &mut HashMap<u64, Drawing>,
    kept: &mut HashMap<u64, Drawing>,
    key: u64,
    lazy: bool,
    make: impl FnOnce() -> Option<figure::Figure>,
) -> Drawing {
    let drawing = match old.remove(&key).or_else(|| kept.get(&key).cloned()) {
        Some(drawing) => drawing,
        None if lazy => return None,
        None => make().map(|(svg, w, h)| {
            let image = Image::from_bytes(ImageFormat::Svg, svg.into_bytes());
            (Arc::new(image), size(px(w), px(h)))
        }),
    };
    kept.insert(key, drawing.clone());
    drawing
}

pub struct Editor {
    focus: FocusHandle,
    content: String,
    sel: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    selecting: bool,
    goal_x: Option<Pixels>,
    undo: Vec<(String, Range<usize>)>,
    redo: Vec<(String, Range<usize>)>,
    last_edit: Option<Instant>,
    notes: Vec<String>,
    /// Dossiers où chercher les images : celui de la note, puis le coffre.
    dirs: Vec<PathBuf>,
    /// Début de la ligne dont le bloc de code vient d'être copié.
    copied: Option<usize>,
    /// Début de la ligne dont le bouton de copie est survolé.
    hover: Option<usize>,
    /// Zones des boutons de copie, par début de ligne : elles portent le curseur en main.
    copy_hitboxes: Vec<(usize, Hitbox)>,
    /// Diagrammes et formules de la dernière frame, par empreinte de leur source.
    figures: HashMap<u64, Drawing>,
    ac_index: usize,
    ac_dismissed: Option<usize>,
    theme: Theme,
    // Mise en page de la dernière frame.
    rows: Vec<Row>,
    origin: Point<Pixels>,
    width: Pixels,
    viewport: Bounds<Pixels>,
    scroll_y: Pixels,
    reveal: bool,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Editor {
    pub fn new(theme: Theme, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            content: String::new(),
            sel: 0..0,
            reversed: false,
            marked: None,
            selecting: false,
            goal_x: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            notes: Vec::new(),
            dirs: Vec::new(),
            copied: None,
            hover: None,
            copy_hitboxes: Vec::new(),
            figures: HashMap::new(),
            ac_index: 0,
            ac_dismissed: None,
            theme,
            rows: Vec::new(),
            origin: Point::default(),
            width: MAX_WIDTH,
            viewport: Bounds::default(),
            scroll_y: px(0.),
            reveal: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    /// Remplace tout le contenu (changement de note) ; curseur à l'octet `cursor`.
    pub fn load(&mut self, text: String, cursor: usize, cx: &mut Context<Self>) {
        let cursor = cursor.min(text.len());
        self.content = text;
        self.sel = cursor..cursor;
        self.reversed = false;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.scroll_y = px(0.);
        self.reveal = true;
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Noms des notes du coffre, pour compléter les `[[wikiliens]]`.
    pub fn set_notes(&mut self, notes: Vec<String>) {
        self.notes = notes;
    }

    pub fn set_dirs(&mut self, dirs: Vec<PathBuf>) {
        self.dirs = dirs;
    }

    /// Contenu du bloc de code où se trouve le curseur.
    pub fn code_at_cursor(&self) -> Option<&str> {
        md::code_block(&self.content, self.cursor()).map(|r| &self.content[r])
    }

    /// Centre du premier bouton « Copier » affiché.
    #[cfg(test)]
    pub fn copy_button(&self) -> Option<Point<Pixels>> {
        Some(self.copy_bounds(self.rows.iter().find(|r| r.opens)?).center())
    }

    /// Nombre de signes et taille de l'image affichés sur la ligne qui commence par `prefix`.
    #[cfg(test)]
    pub fn decorations(&self, prefix: &str) -> (usize, Option<(f32, f32)>) {
        let row = self.rows.iter().find(|r| self.content[r.start..].starts_with(prefix)).unwrap();
        let image = row.image.as_ref().map(|(_, s)| (s.width.into(), s.height.into()));
        (row.subs.len(), image)
    }

    fn cursor(&self) -> usize {
        if self.reversed { self.sel.start } else { self.sel.end }
    }

    fn clamp(&self, mut i: usize) -> usize {
        i = i.min(self.content.len());
        while !self.content.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn line_range(&self, at: usize) -> Range<usize> {
        let start = self.content[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = self.content[at..].find('\n').map_or(self.content.len(), |i| at + i);
        start..end
    }

    fn fences_before(&self, at: usize) -> usize {
        self.content[..at].lines().filter(|l| md::is_fence(l)).count()
    }

    /// Un ``` sans clôture n'ouvre pas de bloc, comme dans `layout`.
    fn in_code(&self, line_start: usize) -> bool {
        self.fences_before(line_start) % 2 == 1
            && self.content[line_start..].lines().any(md::is_fence)
    }

    fn move_to(&mut self, to: usize, cx: &mut Context<Self>) {
        self.sel = to..to;
        self.reversed = false;
        self.goal_x = None;
        self.reveal = true;
        cx.notify();
    }

    fn select_to(&mut self, to: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.sel.start = to
        } else {
            self.sel.end = to
        }
        if self.sel.end < self.sel.start {
            self.reversed = !self.reversed;
            self.sel = self.sel.end..self.sel.start;
        }
        self.goal_x = None;
        self.reveal = true;
        cx.notify();
    }

    // ponytail: l'annulation garde des copies entières du texte (200 max) ;
    // passer à un journal d'opérations si les notes dépassent le Mo.
    fn push_undo(&mut self, boundary: bool) {
        let stale = self
            .last_edit
            .is_none_or(|t| t.elapsed() > Duration::from_millis(600));
        if boundary || stale {
            self.undo.push((self.content.clone(), self.sel.clone()));
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some(Instant::now());
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.marked = None;
        self.copied = None;
        self.ac_index = 0;
        self.goal_x = None;
        self.reveal = true;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Remplace `range` par `text` en conservant la sélection au mieux.
    fn splice(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.push_undo(text.contains('\n') || text == " ");
        self.content.replace_range(range.clone(), text);
        let map = |p: usize| {
            if p <= range.start {
                p
            } else if p >= range.end {
                p + text.len() - range.len()
            } else {
                range.start + text.len()
            }
        };
        self.sel = map(self.sel.start)..map(self.sel.end);
        self.changed(cx);
    }

    /// Remplace `range` par `text` et place le curseur juste après.
    fn edit(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let end = range.start + text.len();
        self.splice(range, text, cx);
        self.sel = end..end;
        self.reversed = false;
    }

    fn renumber(&mut self, cx: &mut Context<Self>) {
        if let Some((text, cursor)) = md::renumber(&self.content, self.cursor()) {
            self.content = text;
            self.sel = cursor..cursor;
            self.reversed = false;
            self.changed(cx);
        }
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some((text, sel)) = from.pop() else {
            return;
        };
        to.push((std::mem::replace(&mut self.content, text), self.sel.clone()));
        self.sel = sel;
        self.reversed = false;
        self.last_edit = None;
        self.changed(cx);
    }

    // ----- Déplacements -----

    fn row_at(&self, i: usize) -> Option<usize> {
        self.rows
            .partition_point(|r| r.start <= i)
            .checked_sub(1)
    }

    fn vertical(&self, from: usize, down: bool, gx: Pixels) -> usize {
        let Some(r) = self.row_at(from) else {
            return from;
        };
        let row = &self.rows[r];
        let v = (f32::from(row.pos(from - row.start).y) / f32::from(row.lh)).round() as i32;
        let (r, v) = if down {
            if v + 1 < row.visual_rows() {
                (r, v + 1)
            } else if r + 1 < self.rows.len() {
                (r + 1, 0)
            } else {
                return self.content.len();
            }
        } else if v > 0 {
            (r, v - 1)
        } else if r > 0 {
            (r - 1, self.rows[r - 1].visual_rows() - 1)
        } else {
            return 0;
        };
        let row = &self.rows[r];
        self.clamp(row.start + row.index_at(point(gx, row.lh * (v as f32 + 0.5))))
    }

    fn target(&self, m: Motion, gx: Pixels) -> usize {
        let c = self.cursor();
        let lr = self.line_range(c);
        let line = &self.content[lr.clone()];
        let col = c - lr.start;
        let page = (f32::from(self.viewport.size.height) / 26.) as usize;
        match m {
            Motion::Left if col == 0 => c.saturating_sub(1),
            Motion::Left => {
                lr.start + line[..col].grapheme_indices(true).next_back().map_or(0, |(i, _)| i)
            }
            Motion::Right if c == lr.end => (c + 1).min(self.content.len()),
            Motion::Right => c + line[col..].graphemes(true).next().map_or(0, str::len),
            Motion::WordLeft => self.content[..c]
                .split_word_bound_indices()
                .rev()
                .find(|(_, w)| !w.trim().is_empty())
                .map_or(0, |(i, _)| i),
            Motion::WordRight => self.content[c..]
                .split_word_bound_indices()
                .find(|(_, w)| !w.trim().is_empty())
                .map_or(self.content.len(), |(i, w)| c + i + w.len()),
            Motion::Home => lr.start,
            Motion::End => lr.end,
            Motion::DocStart => 0,
            Motion::DocEnd => self.content.len(),
            Motion::Up => self.vertical(c, false, gx),
            Motion::Down => self.vertical(c, true, gx),
            Motion::PageUp => (0..page).fold(c, |c, _| self.vertical(c, false, gx)),
            Motion::PageDown => (0..page).fold(c, |c, _| self.vertical(c, true, gx)),
        }
    }

    fn go(&mut self, m: Motion, select: bool, cx: &mut Context<Self>) {
        if !select
            && matches!(m, Motion::Up | Motion::Down)
            && let Some((_, items)) = self.completion()
        {
            let n = items.len();
            let i = self.ac_index.min(n - 1);
            self.ac_index = if m == Motion::Down { (i + 1) % n } else { (i + n - 1) % n };
            cx.notify();
            return;
        }
        let c = self.cursor();
        let gx = self
            .goal_x
            .unwrap_or_else(|| self.row_at(c).map_or(px(0.), |r| {
                let row = &self.rows[r];
                row.pos(c.saturating_sub(row.start)).x
            }));
        let to = match m {
            Motion::Left if !select && !self.sel.is_empty() => self.sel.start,
            Motion::Right if !select && !self.sel.is_empty() => self.sel.end,
            _ => self.target(m, gx),
        };
        if select {
            self.select_to(to, cx)
        } else {
            self.move_to(to, cx)
        }
        if matches!(m, Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown) {
            self.goal_x = Some(gx);
        }
    }

    fn delete(&mut self, m: Motion, cx: &mut Context<Self>) {
        let range = if self.sel.is_empty() {
            let (c, t) = (self.cursor(), self.target(m, px(0.)));
            c.min(t)..c.max(t)
        } else {
            self.sel.clone()
        };
        self.edit(range, "", cx);
        self.renumber(cx);
    }

    // ----- Édition Markdown -----

    fn newline(&mut self, cx: &mut Context<Self>) {
        if self.accept_completion(cx) {
            return;
        }
        if !self.sel.is_empty() {
            return self.edit(self.sel.clone(), "\n", cx);
        }
        let c = self.cursor();
        let lr = self.line_range(c);
        let line = self.content[lr.clone()].to_string();
        let in_code = self.in_code(lr.start);
        // Ouverture d'un bloc de code : on pose aussi la clôture.
        if md::is_fence(&line)
            && !in_code
            && c == lr.end
            && self.fences_before(self.content.len()) % 2 == 1
        {
            self.edit(c..c, "\n\n```", cx);
            return self.move_to(c + 1, cx);
        }
        match md::on_enter(&line, c - lr.start, in_code) {
            Enter::Insert(text) => self.edit(c..c, &text, cx),
            Enter::Clear => self.edit(lr, "", cx),
        }
        self.renumber(cx);
    }

    fn shift_lines(&mut self, indent: bool, cx: &mut Context<Self>) {
        if indent && self.accept_completion(cx) {
            return;
        }
        let first = self.line_range(self.sel.start);
        if indent && self.sel.is_empty() {
            let line = &self.content[first.clone()];
            let (kind, _) = md::classify(line, self.in_code(first.start));
            if !matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_)) {
                return self.edit(self.sel.clone(), md::INDENT, cx);
            }
        }
        let block = first.start..self.line_range(self.sel.end).end;
        let sel = self.sel.clone();
        let mut deltas = Vec::new();
        let shifted: Vec<String> = self.content[block.clone()]
            .split('\n')
            .map(|l| {
                let new = if indent {
                    format!("{}{l}", md::INDENT)
                } else if let Some(rest) = l.strip_prefix('\t') {
                    rest.to_string()
                } else {
                    let n = l.bytes().take(md::INDENT.len()).take_while(|&b| b == b' ').count();
                    l[n..].to_string()
                };
                deltas.push(new.len() as isize - l.len() as isize);
                new
            })
            .collect();
        self.push_undo(true);
        self.content.replace_range(block.clone(), &shifted.join("\n"));
        let total: isize = deltas.iter().sum();
        let start = (sel.start as isize + deltas[0]).max(block.start as isize) as usize;
        let end = (sel.end as isize + total).max(start as isize) as usize;
        self.sel = start..end;
        self.changed(cx);
        self.renumber(cx);
    }

    fn toggle_task(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.cursor());
        let line = &self.content[lr.clone()];
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        let at = lr.start + marker;
        match kind {
            Kind::Task(done) => self.splice(at - 3..at - 2, if done { " " } else { "x" }, cx),
            Kind::Bullet => {
                self.splice(at..at, "[ ] ", cx);
                if self.sel.end == at {
                    self.move_to(at + 4, cx);
                }
            }
            _ => {}
        }
    }

    fn wrap(&mut self, delimiter: &str, cx: &mut Context<Self>) {
        let sel = self.sel.clone();
        let n = delimiter.len();
        self.splice(sel.end..sel.end, delimiter, cx);
        self.splice(sel.start..sel.start, delimiter, cx);
        self.sel = sel.start + n..sel.end + n;
    }

    // ----- Complétion des wikiliens -----

    /// Début de la requête après `[[` et notes correspondantes.
    fn completion(&self) -> Option<(usize, Vec<&str>)> {
        if !self.sel.is_empty() {
            return None;
        }
        let c = self.cursor();
        let before = &self.content[self.line_range(c).start..c];
        let open = before.rfind("[[")?;
        let query = &before[open + 2..];
        let start = c - query.len();
        if query.contains("]]") || query.contains('|') || self.ac_dismissed == Some(start) {
            return None;
        }
        let query = query.to_lowercase();
        let items: Vec<&str> = self
            .notes
            .iter()
            .filter(|n| n.to_lowercase().contains(&query))
            .map(String::as_str)
            .take(6)
            .collect();
        (!items.is_empty()).then_some((start, items))
    }

    fn accept_completion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, items)) = self.completion() else {
            return false;
        };
        let name = items[self.ac_index.min(items.len() - 1)].to_string();
        let c = self.cursor();
        let closed = self.content[c..].starts_with("]]");
        self.edit(start..c, &format!("{name}]]"), cx);
        if closed {
            let end = self.cursor();
            self.splice(end..end + 2, "", cx);
        }
        true
    }

    // ----- Souris -----

    fn copy_bounds(&self, row: &Row) -> Bounds<Pixels> {
        let at = point(self.width - COPY_SIZE, row.y + row.pad + (row.lh - COPY_SIZE) / 2.);
        Bounds::new(self.origin + at, size(COPY_SIZE, COPY_SIZE))
    }

    /// Début de la ligne dont le bouton de copie est sous le pointeur.
    fn copy_at(&self, at: Point<Pixels>) -> Option<usize> {
        let row = self.rows.iter().find(|r| r.opens && self.copy_bounds(r).contains(&at))?;
        Some(row.start)
    }

    fn index_at(&self, pos: Point<Pixels>) -> usize {
        let y = pos.y - self.origin.y;
        let Some(row) = self.rows.iter().find(|r| y < r.y + r.height).or(self.rows.last()) else {
            return 0;
        };
        let local = point(pos.x - self.origin.x, (y - row.y - row.pad).max(px(0.)));
        self.clamp(row.start + row.index_at(local))
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        // Bouton de copie, à droite de l'ouverture d'un bloc de code.
        if let Some(start) = self.copy_at(e.position)
            && let Some(block) = md::code_block(&self.content, self.line_range(start).end + 1)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[block].to_string()));
            self.copied = Some(start);
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                this.update(cx, |this, cx| {
                    this.copied = None;
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return cx.notify();
        }
        let i = self.index_at(e.position);
        let lr = self.line_range(i);
        let line = &self.content[lr.clone()];
        let col = i - lr.start;
        if e.modifiers.secondary()
            && let Some((_, link)) = md::links(line).into_iter().find(|(r, _)| r.contains(&col))
        {
            return cx.emit(EditorEvent::Open(link));
        }
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        if matches!(kind, Kind::Task(_)) && col + 4 >= marker && col < marker {
            self.move_to(lr.end, cx);
            return self.toggle_task(cx);
        }
        match e.click_count {
            2 => {
                let word = line
                    .split_word_bound_indices()
                    .find(|(s, w)| (*s..=s + w.len()).contains(&col))
                    .map_or(col..col, |(s, w)| s..s + w.len());
                self.move_to(lr.start + word.start, cx);
                self.select_to(lr.start + word.end, cx);
            }
            3 => {
                self.move_to(lr.start, cx);
                self.select_to(lr.end, cx);
            }
            _ => {
                self.selecting = true;
                if e.modifiers.shift {
                    self.select_to(i, cx)
                } else {
                    self.move_to(i, cx)
                }
            }
        }
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.index_at(e.position), cx);
        }
        let hover = self.copy_at(e.position);
        if hover != self.hover {
            self.hover = hover;
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_y -= e.delta.pixel_delta(px(26.)).y;
        cx.notify();
    }

    // ----- Presse-papiers -----

    fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        if self.sel.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.content[self.sel.clone()].to_string(),
        ));
        if cut {
            self.edit(self.sel.clone(), "", cx);
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let image = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        });
        let text = match (image, self.dirs.first()) {
            // Une image collée est enregistrée à côté de la note, qui la désigne.
            (Some(image), Some(dir)) => {
                let extension = match image.format {
                    ImageFormat::Png => "png",
                    ImageFormat::Jpeg => "jpg",
                    ImageFormat::Webp => "webp",
                    ImageFormat::Gif => "gif",
                    ImageFormat::Svg => "svg",
                    ImageFormat::Bmp => "bmp",
                    ImageFormat::Tiff => "tiff",
                };
                match vault::save_image(dir, extension, &image.bytes) {
                    Ok(name) => format!("![]({name})"),
                    Err(_) => return,
                }
            }
            _ => match item.text() {
                Some(text) => text.replace("\r\n", "\n"),
                None => return,
            },
        };
        self.push_undo(true);
        self.edit(self.sel.clone(), &text, cx);
    }

    // ----- UTF-16 (méthodes de saisie) -----

    fn from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (i, ch) in self.content.char_indices() {
            if utf16 >= offset {
                return i;
            }
            utf16 += ch.len_utf16();
        }
        self.content.len()
    }

    fn to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].encode_utf16().count()
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.from_utf16(r.start)..self.from_utf16(r.end)
    }

    // ----- Mise en page et rendu -----

    // ponytail: toutes les lignes sont remises en forme à chaque frame (le cache
    // de gpui absorbe les lignes inchangées). Ne mettre en forme que le visible
    // si une note de plusieurs dizaines de milliers de lignes devient lente.
    fn layout(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let t = self.theme;
        let cursor = self.cursor();
        // ponytail: chaînes et nombres prennent la couleur d'accent décalée en teinte,
        // pour s'accorder à chaque thème sans palette dédiée ; donner ses couleurs de
        // code à chaque thème si l'une d'elles jure.
        let hue = |by: f32| Hsla { h: (t.accent.h + by).fract(), ..t.accent };
        // Un ``` sans clôture n'ouvre pas de bloc : sinon chaque ``` tapé ferait
        // basculer en code, donc remettre en forme, toute la suite de la note. S'il
        // y en a un de trop, c'est celui qu'on est en train de taper, sinon le dernier.
        let mut fences = self.content.split('\n').filter(|l| md::is_fence(l)).count();
        let typed = self.line_range(cursor);
        let orphan = (fences % 2 == 1 && md::is_fence(&self.content[typed.clone()]))
            .then_some(typed.start);
        fences -= orphan.is_some() as usize;
        // Une police sans `≠` le compose d'un `=` et d'une barre mal placée : autant
        // laisser `!=`.
        let text_system = window.text_system().clone();
        let has_unequal = text_system
            .advance(text_system.resolve_font(&font(sans())), px(16.), '≠')
            .is_ok();
        let mut lang = String::new();
        // Diagrammes Mermaid et formules LaTeX : dessinés une fois, repris ensuite.
        let rgb = |c: Hsla| {
            let c = Rgba::from(c);
            [c.r, c.g, c.b].map(|v| (v * 255.) as u8)
        };
        let hex = |c: Hsla| {
            let [r, g, b] = rgb(c);
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let look = figure::Look {
            dark: t.bg.l < 0.5,
            bg: hex(t.bg),
            fill: hex(t.code_bg),
            text: hex(t.text),
            line: hex(t.dim),
            font: sans().to_string(),
        };
        let key = |kind: u8, source: &str| {
            let mut hasher = DefaultHasher::new();
            (kind, source, &look.bg, &look.fill, &look.text, &look.line, &look.font, t.size.to_bits())
                .hash(&mut hasher);
            hasher.finish()
        };
        let mut old = std::mem::take(&mut self.figures);
        let mut kept = HashMap::new();
        // Bloc Mermaid ou `$$` en cours : son début et son source.
        let mut block: Option<(usize, String)> = None;
        let mut dollars = self.content.split('\n').filter(|l| l.trim() == "$$").count();
        let width = (bounds.size.width - px(48.)).min(MAX_WIDTH).max(px(120.));
        let marked = self.marked.clone();
        let mut rows = Vec::new();
        let mut y = px(0.);
        let mut offset = 0;
        let mut in_code = false;
        for line in self.content.split('\n') {
            let (mut kind, mut marker) = md::classify(line, in_code);
            let mut opens = false;
            let mut drawing = None;
            if kind == Kind::Fence && orphan != Some(offset) {
                opens = !in_code && fences > 1;
                in_code = opens;
                fences -= 1;
                if opens {
                    lang = line.trim_start()[3..].trim().to_lowercase();
                    block = (lang == "mermaid").then(|| (offset, String::new()));
                } else if let Some((start, source)) = block.take() {
                    // ponytail: un diagramme se dessine sur le thread UI (quelques dizaines
                    // de ms), donc seulement une fois le curseur sorti du bloc ; passer
                    // en tâche de fond si de gros diagrammes font attendre.
                    let editing = (start..=offset + line.len()).contains(&cursor);
                    drawing = cached(&mut old, &mut kept, key(0, &source), editing, || {
                        figure::mermaid(&source, &look)
                    });
                }
            }
            let code = matches!(kind, Kind::Code | Kind::Fence);
            // Formule sur plusieurs lignes, entre deux lignes `$$`.
            let mut math = false;
            if kind == Kind::Code {
                if let Some((_, source)) = &mut block {
                    source.push_str(line);
                    source.push('\n');
                }
            } else if !code && line.trim() == "$$" {
                math = true;
                dollars -= 1;
                match block.take() {
                    Some((_, source)) => {
                        drawing = cached(&mut old, &mut kept, key(1, &source), false, || {
                            figure::latex(&source, true, rgb(t.text), t.size * MATH_SCALE)
                        });
                    }
                    // Comme pour ``` : un `$$` sans clôture n'ouvre rien.
                    None if dollars > 0 => block = Some((offset, String::new())),
                    None => {}
                }
            } else if !code && let Some((_, source)) = &mut block {
                math = true;
                source.push_str(line);
                source.push('\n');
            }
            if math {
                (kind, marker) = (Kind::Para, 0);
            }
            let (font_size, pad) = match kind {
                Kind::Heading(1) => (px(27.), px(14.)),
                Kind::Heading(2) => (px(21.), px(10.)),
                Kind::Heading(_) => (px(17.5), px(6.)),
                Kind::Code | Kind::Fence => (px(14.), px(0.)),
                _ => (px(16.), px(0.)),
            };
            // Les tailles ci-dessus valent pour un texte courant de 16 px.
            let font_size = font_size * (t.size / 16.);
            let pad = if offset == 0 { px(0.) } else { pad };

            let mut flags = vec![0u16; line.len()];
            let list = matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_));
            flags[..marker].fill(if list { md::MARK } else { md::DIM });
            if math {
                flags.fill(if line.trim() == "$$" { md::DIM } else { md::CODE });
            } else if kind == Kind::Code {
                // Sans langage annoncé, en texte brut ou en Mermaid, le bloc reste tel quel.
                if !matches!(lang.as_str(), "" | "text" | "txt" | "plain" | "mermaid") {
                    md::code(line, &lang, &mut flags);
                }
            } else if !code && kind != Kind::Rule {
                md::inline(line, marker, &mut flags);
                // Formule sur la ligne : son source reste lisible, comme du code.
                if let Some((range, source, display)) = md::math(line)
                    && range.start >= marker
                    && flags[range.start] & md::CODE == 0
                {
                    flags[range].fill(md::CODE);
                    drawing = cached(&mut old, &mut kept, key(1 + display as u8, source), false, || {
                        figure::latex(source, display, rgb(t.text), t.size * MATH_SCALE)
                    });
                }
            }
            if kind == Kind::Task(true) {
                flags[marker..].iter_mut().for_each(|f| *f |= md::STRIKE | md::DIM);
            }
            // ponytail: une image par ligne, cherchée dans le dossier de la note puis à
            // la racine du coffre ; les images en ligne (http) ne sont pas chargées.
            let image = (!code).then(|| md::image(line)).flatten().and_then(|(range, path)| {
                flags[range].iter_mut().for_each(|f| *f |= md::DIM);
                let file = self.dirs.iter().map(|d| d.join(&path)).find(|p| p.is_file())?;
                // gpui 0.2 rend un SVG deux fois plus grand que nature, pour qu'il reste
                // net (`SMOOTH_SVG_SCALE_FACTOR`, qui n'est pas public).
                let svg = file.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
                let zoom = if svg { 2. } else { 1. };
                let image = window
                    .use_asset::<ImgResourceLoader>(&Resource::Path(file.into()), cx)?
                    .ok()?;
                let natural = |d: gpui::DevicePixels| d.0 as f32 / zoom;
                let (w, h) = (natural(image.size(0).width), natural(image.size(0).height));
                let scale = (f32::from(width) / w.max(1.)).min(1.);
                Some((image, size(px(w * scale), px(h * scale))))
            });
            let image = image.or_else(|| {
                let (image, s) = drawing?;
                let image = image.use_render_image(window, cx)?;
                let scale = (width / s.width).min(1.);
                Some((image, size(s.width * scale, s.height * scale)))
            });

            // Hors de la ligne du curseur, `->`, `!=`… s'affichent comme des signes ;
            // le texte, lui, ne change pas.
            let has_cursor = (offset..=offset + line.len()).contains(&cursor);
            let mut signs = if code || has_cursor || kind == Kind::Rule {
                Vec::new()
            } else {
                md::symbols(line, marker, &flags)
            };
            signs.retain(|(_, _, sign)| has_unequal || *sign != "≠");
            let mut shown = Cow::Borrowed(line);
            let mut subs = Vec::new();
            if !signs.is_empty() {
                let (mut text, mut styles, mut done) = (String::new(), Vec::new(), 0);
                for (at, len, sign) in signs {
                    text.push_str(&line[done..at]);
                    styles.extend_from_slice(&flags[done..at]);
                    text.push_str(sign);
                    styles.resize(text.len(), flags[at]);
                    subs.push((at, len, sign.len()));
                    done = at + len;
                }
                text.push_str(&line[done..]);
                styles.extend_from_slice(&flags[done..]);
                shown = Cow::Owned(text);
                flags = styles;
            }
            let is_marked = |i: usize| marked.as_ref().is_some_and(|m| m.contains(&(offset + i)));

            let mut runs: Vec<TextRun> = Vec::new();
            let mut i = 0;
            while i < shown.len() {
                let (f, m) = (flags[i], is_marked(i));
                let mut j = i + 1;
                while j < shown.len() && flags[j] == f && is_marked(j) == m {
                    j += 1;
                }
                let heading = matches!(kind, Kind::Heading(_));
                let color = if f & md::DIM != 0 {
                    t.dim
                } else if f & md::STRING != 0 {
                    hue(0.33)
                } else if f & md::NUMBER != 0 {
                    hue(0.6)
                } else if f & (md::LINK | md::TAG | md::MARK | md::KEYWORD) != 0 {
                    t.accent
                } else {
                    t.text
                };
                runs.push(TextRun {
                    len: j - i,
                    font: Font {
                        weight: if heading || f & md::BOLD != 0 {
                            FontWeight::BOLD
                        } else {
                            FontWeight::NORMAL
                        },
                        style: if f & md::ITALIC != 0 || kind == Kind::Quote {
                            FontStyle::Italic
                        } else {
                            FontStyle::Normal
                        },
                        ..font(if code || f & md::CODE != 0 { mono() } else { sans() })
                    },
                    color,
                    background_color: (f & md::CODE != 0).then_some(t.code_bg),
                    // Un seul trait pour tout le lien, crochets compris : gpui 0.2 laisse
                    // un bout de trait dans la marge si le soulignement change de style
                    // juste à un retour à la ligne.
                    underline: (m || f & md::LINK != 0).then_some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(if f & md::LINK != 0 { t.accent } else { color }.opacity(0.5)),
                        wavy: false,
                    }),
                    strikethrough: (f & md::STRIKE != 0).then_some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(color),
                    }),
                });
                i = j;
            }

            let lh = (font_size * 1.65).round();
            let shaped = window
                .text_system()
                .shape_text(shown.into_owned().into(), font_size, &runs, Some(width), None)
                .ok()
                .and_then(|lines| lines.into_iter().next());
            let text_height = shaped.as_ref().map_or(lh, |l| l.size(lh).height);
            let row = Row {
                start: offset,
                len: line.len(),
                y,
                pad,
                lh,
                height: pad + text_height,
                kind,
                line: shaped,
                subs,
                image,
                opens,
            };
            let height = row.height + row.image_height();
            rows.push(Row { height, ..row });
            y += height;
            offset += line.len() + 1;
        }
        self.rows = rows;
        self.figures = kept;

        if self.reveal {
            self.reveal = false;
            let c = self.cursor();
            if let Some(r) = self.row_at(c) {
                let row = &self.rows[r];
                let top = TOP + row.y + row.pad + row.pos(c - row.start).y;
                let margin = px(24.);
                if top - self.scroll_y < margin {
                    self.scroll_y = top - margin;
                } else if top + row.lh - self.scroll_y > bounds.size.height - margin {
                    self.scroll_y = top + row.lh - bounds.size.height + margin;
                }
            }
        }
        let max_scroll = (TOP + y - bounds.size.height * 0.4).max(px(0.));
        self.scroll_y = self.scroll_y.max(px(0.)).min(max_scroll);
        self.origin = point(
            bounds.left() + (bounds.size.width - width) / 2.,
            bounds.top() + TOP - self.scroll_y,
        );
        self.width = width;
        self.viewport = bounds;
        let opening = self.rows.iter().filter(|r| r.opens);
        self.copy_hitboxes = opening
            .map(|r| (r.start, window.insert_hitbox(self.copy_bounds(r), HitboxBehavior::Normal)))
            .collect();
    }

    fn paint(&self, focused: bool, window: &mut Window, cx: &mut App) {
        let (t, o, w) = (self.theme, self.origin, self.width);
        let cursor = self.cursor();
        let sel = &self.sel;
        let label = |text: &str, color, window: &mut Window| {
            let run = TextRun {
                len: text.len(),
                font: font(sans()),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(text.to_string()), px(14.), &[run], None)
        };
        for row in &self.rows {
            let top = o.y + row.y;
            if top + row.height < self.viewport.top() || top > self.viewport.bottom() {
                continue;
            }
            let text_top = top + row.pad;
            let end = row.start + row.len;
            let has_cursor = (row.start..=end).contains(&cursor);
            let block = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                Bounds::new(point(o.x + x, y), size(w, h))
            };
            match row.kind {
                Kind::Code | Kind::Fence => {
                    window.paint_quad(fill(block(px(-12.), top, w + px(24.), row.height), t.code_bg))
                }
                Kind::Quote => {
                    window.paint_quad(fill(block(px(-14.), top, px(3.), row.height), t.border))
                }
                Kind::Rule if !has_cursor => {
                    let mid = top + row.height / 2.;
                    window.paint_quad(fill(block(px(0.), mid, w, px(1.)), t.border));
                    continue;
                }
                _ => {}
            }
            if !sel.is_empty() && sel.start <= end && sel.end > row.start {
                let a = row.pos(sel.start.max(row.start) - row.start);
                let b = row.pos(sel.end.min(end) - row.start);
                let tail = if sel.end > end { px(6.) } else { px(0.) };
                let mut rect = |x: Pixels, y: Pixels, w: Pixels, h: Pixels| {
                    window.paint_quad(fill(block(x, text_top + y, w, h), t.selection))
                };
                if a.y == b.y {
                    rect(a.x, a.y, b.x - a.x + tail, row.lh);
                } else {
                    rect(a.x, a.y, w - a.x, row.lh);
                    rect(px(0.), a.y + row.lh, w, b.y - a.y - row.lh);
                    rect(px(0.), b.y, b.x + tail, row.lh);
                }
            }
            if let Some(line) = &row.line {
                line.paint(point(o.x, text_top), row.lh, TextAlign::Left, None, window, cx)
                    .ok();
            }
            if focused && sel.is_empty() && has_cursor {
                let p = row.pos(cursor - row.start);
                let caret = block(p.x, text_top + p.y + row.lh * 0.14, px(2.), row.lh * 0.72);
                window.paint_quad(fill(caret, t.accent));
            }
            if let Some((image, s)) = &row.image {
                let at = block(px(0.), top + row.height - s.height, s.width, s.height);
                window.paint_image(at, Corners::all(px(6.)), image.clone(), 0, false).ok();
            }
            // Bouton de copie du bloc : la même icône que pour copier la note.
            if row.opens {
                let bounds = self.copy_bounds(row);
                if self.hover == Some(row.start) {
                    window.paint_quad(fill(bounds, t.border).corner_radii(px(6.)));
                }
                let icon = if self.copied == Some(row.start) { "check.svg" } else { "copy.svg" };
                let inset = (COPY_SIZE - px(16.)) / 2.;
                let at = Bounds::new(bounds.origin + point(inset, inset), size(px(16.), px(16.)));
                window.paint_svg(at, icon.into(), TransformationMatrix::unit(), t.text, cx).ok();
                if let Some((_, hitbox)) = self.copy_hitboxes.iter().find(|(s, _)| *s == row.start) {
                    window.set_cursor_style(CursorStyle::PointingHand, hitbox);
                }
            }
        }
        if self.content.is_empty() {
            let hint = format!(
                "{}   {MOD}+P : notes   {MOD}+N : {}   F1 : {}",
                tr("Write here…", "Écris ici…"),
                tr("new note", "nouvelle note"),
                tr("shortcuts", "raccourcis"),
            );
            label(&hint, t.dim, window)
                .paint(point(o.x + px(8.), o.y + px(3.)), px(22.), window, cx)
                .ok();
        }
        if focused
            && let Some((_, items)) = self.completion()
            && let Some(r) = self.row_at(cursor)
        {
            let row = &self.rows[r];
            let p = row.pos(cursor - row.start);
            let item_h = px(28.);
            let at = point(o.x + p.x, o.y + row.y + row.pad + p.y + row.lh + px(4.));
            let panel = Bounds::new(at, size(px(280.), item_h * items.len() as f32 + px(8.)));
            window.paint_quad(quad(panel, px(8.), t.panel, px(1.), t.border, BorderStyle::default()));
            for (i, name) in items.iter().enumerate() {
                let y = at.y + px(4.) + item_h * i as f32;
                if i == self.ac_index.min(items.len() - 1) {
                    let hl = Bounds::new(point(at.x + px(4.), y), size(px(272.), item_h));
                    window.paint_quad(fill(hl, t.selection).corner_radii(px(5.)));
                }
                let name: String = name.chars().take(36).collect();
                label(&name, t.text, window)
                    .paint(point(at.x + px(12.), y + px(4.)), px(20.), window, cx)
                    .ok();
            }
        }
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.sel),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        self.edit(range, text, cx);
        if text != " " {
            return;
        }
        // `[] ` en début de ligne devient une case à cocher.
        let c = self.cursor();
        let lr = self.line_range(c);
        let typed = self.content[lr.start..c].trim_start();
        let task = match typed {
            "[] " | "[ ] " => "- [ ] ",
            "[x] " => "- [x] ",
            _ => return,
        };
        if !self.in_code(lr.start) {
            self.edit(c - typed.len()..c, task, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        let start = range.start;
        self.edit(range, text, cx);
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(r) = new_selected_utf16 {
            let to = |n: usize| -> usize {
                text.char_indices()
                    .scan(0, |u, (i, ch)| {
                        let at = (*u, i);
                        *u += ch.len_utf16();
                        Some(at)
                    })
                    .find(|(u, _)| *u >= n)
                    .map_or(text.len(), |(_, i)| i)
            };
            self.sel = start + to(r.start)..start + to(r.end);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let i = self.from_utf16(range_utf16.start);
        let row = &self.rows[self.row_at(i)?];
        let p = row.pos(i.saturating_sub(row.start));
        Some(Bounds::new(
            point(self.origin.x + p.x, self.origin.y + row.y + row.pad + p.y),
            size(px(2.), row.lh),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.to_utf16(self.index_at(point)))
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        macro_rules! motions {
            ($el:expr, $($action:ident => $motion:ident, $select:expr;)*) => {
                $el$(.on_action(cx.listener(|this, _: &$action, _, cx| {
                    this.go(Motion::$motion, $select, cx)
                })))*
            };
        }
        let el = div()
            .size_full()
            .key_context("Editor")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam);
        motions!(el,
            Left => Left, false; Right => Right, false; Up => Up, false; Down => Down, false;
            WordLeft => WordLeft, false; WordRight => WordRight, false;
            Home => Home, false; End => End, false;
            DocStart => DocStart, false; DocEnd => DocEnd, false;
            PageUp => PageUp, false; PageDown => PageDown, false;
            SelectLeft => Left, true; SelectRight => Right, true;
            SelectUp => Up, true; SelectDown => Down, true;
            SelectWordLeft => WordLeft, true; SelectWordRight => WordRight, true;
            SelectHome => Home, true; SelectEnd => End, true;
            SelectDocStart => DocStart, true; SelectDocEnd => DocEnd, true;
        )
        .on_action(cx.listener(|this, _: &Backspace, _, cx| this.delete(Motion::Left, cx)))
        .on_action(cx.listener(|this, _: &Delete, _, cx| this.delete(Motion::Right, cx)))
        .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| this.delete(Motion::WordLeft, cx)))
        .on_action(cx.listener(|this, _: &DeleteWordRight, _, cx| this.delete(Motion::WordRight, cx)))
        .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
            this.move_to(0, cx);
            this.select_to(this.content.len(), cx);
        }))
        .on_action(cx.listener(|this, _: &Newline, _, cx| this.newline(cx)))
        .on_action(cx.listener(|this, _: &Indent, _, cx| this.shift_lines(true, cx)))
        .on_action(cx.listener(|this, _: &Outdent, _, cx| this.shift_lines(false, cx)))
        .on_action(cx.listener(|this, _: &ToggleTask, _, cx| this.toggle_task(cx)))
        .on_action(cx.listener(|this, _: &Bold, _, cx| this.wrap("**", cx)))
        .on_action(cx.listener(|this, _: &Italic, _, cx| this.wrap("*", cx)))
        .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(false, cx)))
        .on_action(cx.listener(|this, _: &Cut, _, cx| this.copy(true, cx)))
        .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste(cx)))
        .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
        .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
        .on_action(cx.listener(|this, _: &Cancel, _, cx| {
            if let Some((start, _)) = this.completion() {
                this.ac_dismissed = Some(start);
                cx.notify();
            }
        }))
        .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
        .on_mouse_move(cx.listener(Self::mouse_move))
        .on_scroll_wheel(cx.listener(Self::scroll))
        .child(EditorElement(cx.entity()))
    }
}

struct EditorElement(Entity<Editor>);

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.update(cx, |editor, cx| editor.layout(bounds, window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.0.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.0.clone()), cx);
        let focused = focus.is_focused(window);
        self.0.update(cx, |editor, cx| editor.paint(focused, window, cx));
    }
}
