//! Viewer: an SVG from elsewhere (draw.io, Mermaid.js, Inkscape…) or a Mermaid
//! diagram from the note, large, panned and zoomed. It is drawn in tiles at the zoom's resolution, off the
//! UI thread: a diagram 6,000 px high stays sharp without taking everything at once.

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use gpui::{
    App, Asset, Bounds, ContentMask, Context, Corners, CursorStyle, FocusHandle, Focusable, Image, ImageFormat,
    ImgResourceLoader, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, RenderImage, Resource, Rgba,
    ScrollWheelEvent, Window, canvas, div, fill, point, prelude::*, px,
};
use resvg::usvg;

use crate::{
    ui::theme::Theme,
    views::canvas::{Cancel, Down, Left, Right, Up, ZoomFit, ZoomIn, ZoomOut},
    ui::widgets::{button, tip},
    ui::figure, ui::svg, lang::tr,
};

/// Side of a tile, in screen pixels.
const TILE: u32 = 512;
/// Most tiles kept in memory (4 MB each); the oldest go.
const KEPT: usize = 64;
/// Most tiles drawn at once: zooming fast does not pile up hundreds of them.
const FLYING: usize = 8;
/// Allowed zoom: from 5% to 800%.
const ZOOM: (f32, f32) = (0.05, 8.);

/// The SVG read, made readable and parsed, in a theme's colors.
pub struct Doc {
    tree: usvg::Tree,
    id: u64,
}

impl Doc {
    fn size(&self) -> (f32, f32) {
        (self.tree.size().width(), self.tree.size().height())
    }
}

/// What the viewer shows.
#[derive(Clone, Hash, PartialEq)]
pub enum Source {
    /// An SVG file, as it is on disk (modification date).
    File(PathBuf, Option<SystemTime>),
    /// The source of a Mermaid diagram.
    Mermaid(String),
    /// A LaTeX formula of the note: its source, and whether it is display math.
    Formula(String, bool),
    /// A picture (PNG, JPEG, GIF, WebP…): drawn whole by the graphics card, not in tiles.
    Raster(PathBuf),
}

/// Em size of a formula in the viewer, in pixels: sharp when zoomed in.
const FORMULA: f32 = 24.;

/// What to read, in the theme's colors.
#[derive(Clone, Hash, PartialEq)]
struct Load {
    source: Source,
    look: figure::Look,
}

impl Asset for Load {
    type Source = Self;
    type Output = Option<Arc<Doc>>;

    // The future must not borrow `cx`: an `async fn` would.
    #[allow(clippy::manual_async_fn)]
    fn load(load: Self, _: &mut App) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            let svg = match &load.source {
                Source::File(path, _) => svg::readable(&std::fs::read_to_string(path).ok()?, load.look.dark),
                Source::Mermaid(text) => figure::mermaid(text, &load.look)?,
                Source::Formula(text, display) => {
                    let ink = |i: usize| u8::from_str_radix(load.look.text.get(i..i + 2)?, 16).ok();
                    figure::latex(text, *display, [ink(1)?, ink(3)?, ink(5)?], FORMULA)?.0
                }
                Source::Raster(_) => return None,
            };
            let tree = svg::parse(&svg)?;
            let mut hasher = DefaultHasher::new();
            load.hash(&mut hasher);
            Some(Arc::new(Doc { tree, id: hasher.finish() }))
        }
    }
}

/// A tile: the cell `x, y` of the `TILE`-pixel grid of the document drawn at scale
/// `2^(level/4)`. `level` at `None`: the whole document, small, under the tiles that are loading.
#[derive(Clone)]
struct Tile {
    doc: Arc<Doc>,
    level: Option<i32>,
    x: u32,
    y: u32,
    bg: [u8; 3],
}

impl Hash for Tile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.doc.id, self.level, self.x, self.y, self.bg).hash(state);
    }
}

impl PartialEq for Tile {
    fn eq(&self, other: &Self) -> bool {
        (self.doc.id, self.level, self.x, self.y, self.bg) == (other.doc.id, other.level, other.x, other.y, other.bg)
    }
}

/// The scale of a tile level: four levels per doubling.
fn scale_of(level: i32) -> f32 {
    2f32.powf(level as f32 / 4.)
}

/// The scale of the preview: 1,024 px on the longest side, or less.
fn overview(doc: &Doc) -> f32 {
    let (w, h) = doc.size();
    svg::scale_for(w, h, (1024. / w.max(h).max(1.)).min(1.))
}

impl Asset for Tile {
    type Source = Self;
    type Output = Option<Arc<Image>>;

    // The future must not borrow `cx`: an `async fn` would.
    #[allow(clippy::manual_async_fn)]
    fn load(tile: Self, _: &mut App) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            let (w, h) = tile.doc.size();
            let (scale, region) = match tile.level {
                Some(level) => {
                    let scale = scale_of(level);
                    let (x, y) = (tile.x * TILE, tile.y * TILE);
                    let right = (w * scale).ceil() as u32;
                    let bottom = (h * scale).ceil() as u32;
                    (scale, (x, y, TILE.min(right.saturating_sub(x)), TILE.min(bottom.saturating_sub(y))))
                }
                None => {
                    let scale = overview(&tile.doc);
                    (scale, (0, 0, (w * scale).ceil() as u32, (h * scale).ceil() as u32))
                }
            };
            let bytes = svg::bmp(&tile.doc.tree, scale, region, tile.bg)?;
            Some(Arc::new(Image::from_bytes(ImageFormat::Bmp, bytes)))
        }
    }
}

pub struct Viewer {
    focus: FocusHandle,
    source: Source,
    theme: Theme,
    /// The document shown, and what was read to get it; it stays on screen while
    /// the next one is prepared (other theme, modified file).
    doc: Option<(Load, Arc<Doc>)>,
    /// A picture (`Source::Raster`) once loaded.
    picture: Option<Arc<RenderImage>>,
    /// The file could not be read.
    failed: bool,
    /// Reading in progress.
    waiting: Option<Load>,
    /// Offset (px) of the document's origin in the view, and pixels per unit.
    pan: (f32, f32),
    zoom: f32,
    /// Fit the document on the next frame.
    fit: bool,
    /// Drag in progress: last pointer position.
    drag: Option<Point<Pixels>>,
    bounds: Bounds<Pixels>,
    /// Requested tiles, oldest first, and those being drawn.
    tiles: VecDeque<Tile>,
    flying: Vec<Tile>,
}

impl Focusable for Viewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Viewer {
    pub fn new(source: Source, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            source,
            theme,
            doc: None,
            picture: None,
            failed: false,
            waiting: None,
            pan: (0., 0.),
            zoom: 1.,
            fit: true,
            drag: None,
            bounds: Bounds::default(),
            tiles: VecDeque::new(),
            flying: Vec::new(),
        }
    }

    /// The theme, as the window knows it.
    pub fn sync(&mut self, theme: Theme) {
        self.theme = theme;
    }

    #[cfg(test)]
    pub fn state(&self) -> (bool, bool, f32, (f32, f32)) {
        (self.doc.is_some() || self.picture.is_some(), self.failed, self.zoom, self.pan)
    }

    /// Corner of the viewer in the window.
    #[cfg(test)]
    pub fn origin(&self) -> Point<Pixels> {
        self.bounds.origin
    }

    fn bg(&self) -> [u8; 3] {
        let c = Rgba::from(self.theme.bg);
        [c.r, c.g, c.b].map(|v| (v * 255.).round() as u8)
    }

    /// Zooms around this point of the view (pixels from its corner).
    fn zoom_at(&mut self, zoom: f32, (mx, my): (f32, f32), cx: &mut Context<Self>) {
        let zoom = zoom.clamp(ZOOM.0, ZOOM.1);
        let k = zoom / self.zoom;
        self.pan = (mx - (mx - self.pan.0) * k, my - (my - self.pan.1) * k);
        self.zoom = zoom;
        cx.notify();
    }

    fn zoom_by(&mut self, by: f32, cx: &mut Context<Self>) {
        let middle = (f32::from(self.bounds.size.width) / 2., f32::from(self.bounds.size.height) / 2.);
        self.zoom_at(self.zoom * by, middle, cx);
    }

    fn nudge(&mut self, dx: f32, dy: f32, cx: &mut Context<Self>) {
        self.pan = (self.pan.0 + dx * 80., self.pan.1 + dy * 80.);
        cx.notify();
    }

    /// The wheel moves the view; with Ctrl (Cmd), it zooms around the pointer.
    fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = e.delta.pixel_delta(px(26.));
        if e.modifiers.secondary() {
            let o = self.bounds.origin;
            let at = (f32::from(e.position.x - o.x), f32::from(e.position.y - o.y));
            self.zoom_at(self.zoom * (f32::from(delta.y) * 0.004).exp(), at, cx);
        } else {
            self.nudge(f32::from(delta.x) / 80., f32::from(delta.y) / 80., cx);
        }
    }

    /// Fits the document: to its width (a long diagram is read going down), at 100% at most,
    /// centered if it fits entirely.
    fn frame(&mut self, (w, h): (f32, f32)) {
        let (bw, bh) = (f32::from(self.bounds.size.width), f32::from(self.bounds.size.height));
        let margin = 24.;
        self.zoom = ((bw - 2. * margin) / w.max(1.)).clamp(ZOOM.0, 1.);
        let top = if h * self.zoom + 2. * margin <= bh { (bh - h * self.zoom) / 2. } else { margin };
        self.pan = ((bw - w * self.zoom) / 2., top);
    }

    /// The document on screen, read in the background.
    fn document(&mut self, window: &mut Window, cx: &mut App) -> Option<Arc<Doc>> {
        // A file changed on disk is reread.
        if let Source::File(path, stamp) = &mut self.source {
            *stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        }
        let load = Load { source: self.source.clone(), look: figure::Look::of(&self.theme) };
        // One read at a time: the one for a theme already left finishes before the next starts.
        if let Some(running) = self.waiting.clone().filter(|running| *running != load) {
            if window.use_asset::<Load>(&running, cx).is_none() {
                return self.doc.as_ref().map(|(_, doc)| doc.clone());
            }
            cx.remove_asset::<Load>(&running);
            self.waiting = None;
        }
        if self.doc.as_ref().is_none_or(|(have, _)| *have != load) {
            match window.use_asset::<Load>(&load, cx) {
                Some(Some(doc)) => {
                    if let Some((old, _)) = self.doc.take() {
                        cx.remove_asset::<Load>(&old);
                    }
                    self.doc = Some((load, doc));
                    (self.failed, self.waiting) = (false, None);
                }
                Some(None) => {
                    cx.remove_asset::<Load>(&load);
                    self.failed = self.doc.is_none();
                }
                None => self.waiting = Some(load),
            }
        }
        self.doc.as_ref().map(|(_, doc)| doc.clone())
    }

    /// The picture on screen, read by gpui in the background (animated GIF: its first frame).
    fn picture(&mut self, window: &mut Window, cx: &mut App) -> Option<Arc<RenderImage>> {
        let Source::Raster(path) = &self.source else { return None };
        match window.use_asset::<ImgResourceLoader>(&Resource::Path(path.clone().into()), cx) {
            Some(Ok(image)) => self.picture = Some(image),
            Some(Err(_)) => self.failed = self.picture.is_none(),
            None => {}
        }
        self.picture.clone()
    }

    /// A tile's image, if it is ready; keeps it among the last requested. A tile
    /// never requested waits its turn if `FLYING` are already in progress.
    fn tile(&mut self, tile: Tile, window: &mut Window, cx: &mut App) -> Option<Arc<gpui::RenderImage>> {
        match self.tiles.iter().position(|t| *t == tile) {
            Some(i) => {
                let t = self.tiles.remove(i).unwrap();
                self.tiles.push_back(t);
            }
            None if self.flying.len() >= FLYING => return None,
            None => {
                self.tiles.push_back(tile.clone());
                self.flying.push(tile.clone());
            }
        }
        let image = window.use_asset::<Tile>(&tile, cx)??;
        image.use_render_image(window, cx)
    }

    /// Tiles in progress that have finished make room for the next ones.
    fn landed(&mut self, window: &mut Window, cx: &mut App) {
        let flying = std::mem::take(&mut self.flying);
        self.flying = flying.into_iter().filter(|t| window.use_asset::<Tile>(t, cx).is_none()).collect();
    }

    /// Frees the tiles beyond the last `KEPT`: their image and their room on the graphics card.
    fn forget(&mut self, window: &mut Window, cx: &mut App) {
        while self.tiles.len() > KEPT {
            let tile = self.tiles.pop_front().unwrap();
            self.flying.retain(|t| *t != tile);
            if let Some(Some(image)) = window.get_asset::<Tile>(&tile, cx) {
                if let Some(render) = image.clone().get_render_image(window, cx) {
                    window.drop_image(render).ok();
                }
                image.remove_asset(cx);
            }
            cx.remove_asset::<Tile>(&tile);
        }
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        self.bounds = bounds;
        let t = self.theme;
        let picture = matches!(self.source, Source::Raster(_)).then(|| self.picture(window, cx));
        let doc = match picture {
            Some(_) => None,
            None => match self.document(window, cx) {
                Some(doc) => Some(doc),
                None => return,
            },
        };
        self.landed(window, cx);
        let (w, h) = match (&doc, &picture) {
            (Some(doc), _) => doc.size(),
            (None, Some(Some(image))) => {
                let size = image.size(0);
                (size.width.0 as f32, size.height.0 as f32)
            }
            _ => return,
        };
        if std::mem::take(&mut self.fit) {
            self.frame((w, h));
        }
        let o = bounds.origin;
        let (pan, zoom) = (self.pan, self.zoom);
        let at = move |x: f32, y: f32| point(o.x + px(pan.0 + x * zoom), o.y + px(pan.1 + y * zoom));
        let whole = Bounds::from_corners(at(0., 0.), at(w, h));
        window.paint_quad(fill(whole, t.bg).border_widths(px(1.)).border_color(t.border));
        let Some(doc) = doc else {
            if let Some(Some(image)) = picture {
                window.paint_image(whole, Corners::default(), image, 0, false).ok();
            }
            return;
        };
        let bg = self.bg();
        let base = Tile { doc: doc.clone(), level: None, x: 0, y: 0, bg };
        if let Some(image) = self.tile(base, window, cx) {
            window.paint_image(whole, Corners::default(), image, 0, false).ok();
        }
        // The tile level just fine enough for the screen: a tile weighs only its size, the
        // whole document is never rasterized at that level.
        let level = ((zoom * window.scale_factor()).log2() * 4.).ceil() as i32;
        let scale = scale_of(level);
        // The part of the document the view shows, in document units, then in cells.
        let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let unit = |v: f32, pan: f32| (v - pan) / zoom;
        let cell = |u: f32| (u * scale / TILE as f32).floor().max(0.) as u32;
        let (cols, rows) = ((w * scale / TILE as f32).ceil() as u32, (h * scale / TILE as f32).ceil() as u32);
        let (x0, x1) = (cell(unit(0., pan.0)), cell(unit(bw, pan.0)).min(cols.saturating_sub(1)));
        let (y0, y1) = (cell(unit(0., pan.1)), cell(unit(bh, pan.1)).min(rows.saturating_sub(1)));
        let side = TILE as f32 / scale;
        for y in y0..=y1.max(y0) {
            for x in x0..=x1.max(x0) {
                if x >= cols || y >= rows {
                    continue;
                }
                let tile = Tile { doc: doc.clone(), level: Some(level), x, y, bg };
                if let Some(image) = self.tile(tile, window, cx) {
                    let (ux, uy) = (x as f32 * side, y as f32 * side);
                    let cell = Bounds::from_corners(at(ux, uy), at((ux + side).min(w), (uy + side).min(h)));
                    window.paint_image(cell, Corners::default(), image, 0, false).ok();
                }
            }
        }
        self.forget(window, cx);
    }
}

impl Render for Viewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        // A picture is read from here: gpui draws this view again once it is ready.
        self.picture(window, cx);
        let this = cx.entity();
        let drawing = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    this.update(cx, |viewer, cx| viewer.paint(bounds, window, cx))
                })
            },
        )
        .size_full();
        let percent = format!("{} %", (self.zoom * 100.).round());
        let zoom = div()
            .absolute()
            .bottom_3()
            .right_3()
            .p(px(2.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .occlude()
            .child(button("v-zoom-out", "minimize.svg", false, t).tooltip(tip(tr("Zoom out", "Dézoomer"), "-".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(0.8, cx))))
            .child(
                div()
                    .id("v-zoom-fit")
                    .w(px(52.))
                    .text_center()
                    .text_size(px(12.))
                    .text_color(t.soft)
                    .cursor_pointer()
                    .child(percent)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.fit = true;
                        cx.notify();
                    })),
            )
            .child(button("v-zoom-in", "plus.svg", false, t).tooltip(tip(tr("Zoom in", "Zoomer"), "+".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(1.25, cx))));
        let note = |text: &'static str| div().absolute().inset_0().flex().items_center().justify_center().text_color(t.soft).child(text);
        let shown = self.doc.is_some() || self.picture.is_some();
        let status = match (shown, self.failed) {
            (false, true) => Some(note(tr("This drawing cannot be read", "Ce dessin ne peut pas être lu"))),
            (false, false) => Some(note(tr("Drawing…", "Dessin en cours…"))),
            _ => None,
        };
        div()
            .id("viewer")
            .size_full()
            .relative()
            .key_context("Canvas")
            .track_focus(&self.focus)
            .bg(t.bg)
            .cursor(if self.drag.is_some() { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom_by(1.25, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom_by(0.8, cx)))
            .on_action(cx.listener(|this, _: &ZoomFit, _, cx| {
                this.fit = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Left, _, cx| this.nudge(1., 0., cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.nudge(-1., 0., cx)))
            .on_action(cx.listener(|this, _: &Up, _, cx| this.nudge(0., 1., cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.nudge(0., -1., cx)))
            // Escape: the window decides (back to the note).
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.propagate()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    this.drag = Some(e.position);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                let Some(last) = this.drag else { return };
                if !e.dragging() {
                    this.drag = None;
                    return cx.notify();
                }
                this.pan.0 += f32::from(e.position.x - last.x);
                this.pan.1 += f32::from(e.position.y - last.y);
                this.drag = Some(e.position);
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag = None;
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(Self::scroll))
            // As in the canvas: + - 0 without modifier.
            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
                let k = &e.keystroke;
                if k.modifiers.secondary() || k.modifiers.alt {
                    return;
                }
                match k.key_char.as_deref() {
                    Some("+" | "=") => this.zoom_by(1.25, cx),
                    Some("-") => this.zoom_by(0.8, cx),
                    Some("0") => {
                        this.fit = true;
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(drawing)
            .children(status)
            .child(zoom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_double_every_four() {
        assert_eq!(scale_of(0), 1.);
        assert_eq!(scale_of(4), 2.);
        assert!((scale_of(-4) - 0.5).abs() < 1e-6);
    }
}
