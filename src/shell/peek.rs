//! The card that previews a linked note without opening it: asked by a note's editor (`Ctrl` over a
//! wikilink, or the palette for the link under the cursor), drawn over the window under the link by a
//! read-only editor, closed when the pointer leaves the link and the card, on a click elsewhere, a scroll or Escape.

use std::io::Read;

use gpui::{MouseDownEvent, Subscription, WeakEntity};

use super::*;
use crate::logic::embed::{self, Anchor, Target};
use crate::logic::{files, table};
use crate::views::editor::Peek as Asked;

/// Lines of the note's text the card shows at most before it scrolls, at the note's text size.
const LINES: f32 = 40.;
/// Height of the card's head (the note's title), at scale 1.
const HEAD: f32 = 34.;
/// Room kept between the card and the window's edges, at scale 1.
const MARGIN: f32 = 12.;
/// Rows and columns of a CSV the card shows, its header included.
const TABLE_ROWS: usize = 21;
const TABLE_COLS: usize = 12;
/// What is read of a CSV for its first rows.
const TABLE_BYTES: u64 = 64 * 1024;

pub(super) struct Peek {
    /// The editor that asked, told when the card goes; and its pane, where a link followed from the card opens.
    from: WeakEntity<Editor>,
    pane: usize,
    target: Target,
    /// The link's zone in the window: the card stays while the pointer is in it or on the card.
    at: Bounds<Pixels>,
    /// Asked from the keyboard: the pointer moving does not close it.
    keys: bool,
    /// The text version of the note it came from: another note loaded there (a tab changed) closes it.
    revision: u64,
    shown: Shown,
    _watch: Vec<Subscription>,
}

enum Shown {
    /// What the reader shows: the file it was read from and its text, and what the head says beside the title
    /// (a heading that is not there).
    Text { reader: Entity<Editor>, path: PathBuf, text: String, note: Option<&'static str> },
    /// No note bears the name: the card offers to create it.
    Missing,
}

/// What the card shows for `target`: the file it comes from, the text the reader lays out, and a remark.
enum Found {
    Text(PathBuf, String, Option<&'static str>),
    Missing,
    /// A folder, an attachment: the card has nothing to show for them.
    Nothing,
}

/// Where the card goes for a link at `link`, `want` big, in a window `room` big: under the link, against it,
/// or above it when there is more room there; moved left to stay in the window, and shortened to the room left.
pub(super) fn place(link: Bounds<Pixels>, want: Size<Pixels>, room: Size<Pixels>, margin: Pixels) -> Bounds<Pixels> {
    let below = room.height - margin - link.bottom();
    let above = link.top() - margin;
    let (top, height) = match want.height <= below || below >= above {
        true => (link.bottom(), want.height.min(below)),
        false => {
            let height = want.height.min(above);
            (link.top() - height, height)
        }
    };
    let width = want.width.min(room.width - margin * 2.).max(px(0.));
    let left = link.left().min(room.width - margin - width).max(margin);
    Bounds::new(point(left, top), size(width, height.max(px(0.))))
}

impl Shell {
    /// What a card for `target` shows.
    fn peek_find(&self, target: &Target) -> Found {
        if let Some(path) = self.wiki_path(&target.note) {
            let Ok(text) = fs::read_to_string(&path) else { return Found::Missing };
            let (shown, found) = embed::peek_text(&text, &target.anchor);
            let note = match (&target.anchor, found) {
                (_, true) => None,
                (Anchor::Block(_), false) => Some(tr("block not found", "bloc introuvable")),
                _ => Some(tr("heading not found", "titre introuvable")),
            };
            return Found::Text(path, shown, note);
        }
        if self.wiki_dir(&target.note).is_some() {
            return Found::Nothing;
        }
        match self.wiki_file(&target.note) {
            // A picture or a diagram: the reader draws it as a note draws `![[…]]`.
            Some(file) if files::is_image(&file) => {
                let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                Found::Text(file, format!("![[{name}]]"), None)
            }
            Some(file) if files::is_table(&file) => {
                let mut bytes = Vec::new();
                let read = fs::File::open(&file).and_then(|f| f.take(TABLE_BYTES).read_to_end(&mut bytes));
                match read {
                    Ok(_) => Found::Text(file, table::head_markdown(&bytes, TABLE_ROWS, TABLE_COLS), None),
                    Err(_) => Found::Missing,
                }
            }
            Some(_) => Found::Nothing,
            None => Found::Missing,
        }
    }

    /// The card's width: about 30 characters of the note's text more than a line of it, within the window.
    fn peek_width(&self, window: &Window) -> Pixels {
        let t = self.theme;
        px((t.size * 30.).max(320.)).min(window.viewport_size().width - t.px(MARGIN) * 2.)
    }

    /// Opens the card for the link `asked` of the note in `from` (pane `pane`); `keys`: asked from the keyboard.
    pub(super) fn open_peek(&mut self, from: &Entity<Editor>, pane: usize, asked: Asked, keys: bool, window: &mut Window, cx: &mut Context<Self>) {
        let shown = match self.peek_find(&asked.target) {
            Found::Nothing => {
                self.close_peek(window, cx);
                return from.update(cx, |e, _| e.forget_peek());
            }
            Found::Missing => Shown::Missing,
            Found::Text(path, text, note) => {
                let dirs = path.parent().map(Path::to_path_buf).into_iter().chain(from.read(cx).lookups().0.last().cloned()).collect();
                let reader = Editor::reader(from, dirs, cx);
                // Laid out when drawn: the card takes the height of its text then (`content_height`).
                reader.update(cx, |e, cx| e.show_text(&text, cx));
                Shown::Text { reader, path, text, note }
            }
        };
        let mut watch = Vec::new();
        if let Shown::Text { reader, .. } = &shown {
            // A picture of it loaded: the card takes its new height.
            watch.push(cx.observe(reader, |_, _, cx| cx.notify()));
            watch.push(cx.subscribe_in(reader, window, Self::on_reader_event));
        }
        // The card of another link goes first; the editor that asked for this one keeps the link it asked for.
        if let Some(old) = self.peek.take() {
            let forget = old.from != from.downgrade();
            Self::let_go(old, forget, window, cx);
        }
        let revision = from.read(cx).revision();
        self.peek = Some(Peek { from: from.downgrade(), pane, target: asked.target, at: asked.at, keys, revision, shown, _watch: watch });
        cx.notify();
    }

    /// The palette's "Preview link under the cursor".
    pub(super) fn peek_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.pane().editor.clone();
        match editor.update(cx, |e, _| e.peek_at_cursor()) {
            Some(asked) => self.open_peek(&editor, self.active, asked, true, window, cx),
            None => self.say(Tone::Info, tr("No link under the cursor", "Aucun lien sous le curseur"), cx),
        }
    }

    /// The card goes. Its text clicked (to select in it), the keys go back to the note it came from.
    /// The pointer moved over a backlink row of the panel: with `Ctrl`, the card previews its note.
    // ponytail: `Ctrl` pressed while the pointer rests on a row shows nothing until it moves; follow the
    // modifiers in the panel if it is missed.
    pub(super) fn peek_row(&mut self, path: &Path, held: gpui::Modifiers, window: &mut Window, cx: &mut Context<Self>) {
        let target = Target { note: vault::stem(path), anchor: Anchor::Whole };
        let Some(at) = self.row_zones.borrow().get(path).copied() else { return };
        if !held.secondary() || self.peek.as_ref().is_some_and(|p| p.target == target && p.at == at) {
            return;
        }
        let editor = self.pane().editor.clone();
        self.open_peek(&editor, self.active, Asked { target, at }, false, window, cx);
    }

    pub(super) fn close_peek(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(peek) = self.peek.take() {
            Self::let_go(peek, true, window, cx);
        }
    }

    /// What the card held goes; its editor is told, unless it already asked for the next one (`forget` false).
    fn let_go(peek: Peek, forget: bool, window: &mut Window, cx: &mut Context<Self>) {
        let held = match &peek.shown {
            Shown::Text { reader, .. } => reader.focus_handle(cx).contains_focused(window, cx),
            Shown::Missing => false,
        };
        if let Some(from) = peek.from.upgrade() {
            if forget {
                from.update(cx, |e, _| e.forget_peek());
            }
            if held {
                window.focus(&from.focus_handle(cx));
            }
        }
        cx.notify();
    }

    /// The pointer moved over the window (not over the card, which hides it): out of the link, the card goes.
    pub(super) fn peek_pointer(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.peek.as_ref().is_some_and(|p| !p.keys && !p.at.dilate(px(1.)).contains(&at)) {
            self.close_peek(window, cx);
        }
    }

    /// Before drawing: the note the card came from changed or is gone (another tab loaded in its pane), the card goes.
    pub(super) fn peek_still(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let moved = self.peek.as_ref().is_some_and(|p| p.from.upgrade().is_none_or(|from| from.read(cx).revision() != p.revision));
        if moved {
            self.close_peek(window, cx);
        }
    }

    /// The note `editor` changed: its links moved, the card goes.
    pub(super) fn peek_typed(&mut self, editor: &Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) {
        if self.peek.as_ref().is_some_and(|p| p.from == editor.downgrade()) {
            self.close_peek(window, cx);
        }
    }

    /// A note may have changed on disk: the card shows it again if its file did.
    pub(super) fn refresh_peek(&mut self, cx: &mut Context<Self>) {
        let Some(peek) = &self.peek else { return };
        let Shown::Text { path, text, .. } = &peek.shown else { return };
        if !files::is_image(path) && let Found::Text(again, new, note) = self.peek_find(&peek.target) && again == *path && new != *text {
            let peek = self.peek.as_mut().expect("checked above");
            if let Shown::Text { reader, text, note: said, .. } = &mut peek.shown {
                reader.update(cx, |e, cx| e.show_text(&new, cx));
                (*text, *said) = (new, note);
            }
        }
    }

    /// A link clicked in the card opens, in the pane of the note the card came from; the card goes.
    fn on_reader_event(&mut self, _: &Entity<Editor>, event: &EditorEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(event, EditorEvent::Open(_) | EditorEvent::OpenAside(_) | EditorEvent::OpenAt(..)) {
            return;
        }
        let pane = self.peek.as_ref().map(|p| p.pane);
        self.close_peek(window, cx);
        if let Some(pane) = pane.filter(|&p| p < self.panes.len()) {
            self.active = pane;
        }
        self.follow_event(event, window, cx);
    }

    /// The card's title clicked: the note opens there, at what the link points at (Shift: in the other pane);
    /// a missing note is created.
    fn peek_open(&mut self, aside: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(peek) = &self.peek else { return };
        let (target, pane) = (peek.target.clone(), peek.pane);
        self.close_peek(window, cx);
        if pane < self.panes.len() {
            self.active = pane;
        }
        self.open_at(&target, aside, window, cx);
    }

    pub(super) fn render_peek(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let peek = self.peek.as_ref()?;
        let t = self.theme;
        let room = window.viewport_size();
        let body = match &peek.shown {
            Shown::Text { reader, .. } => reader.read(cx).content_height().min(px(t.size * 1.65 * LINES)),
            Shown::Missing => t.px(76.),
        };
        // A framed window keeps its shadow's margin around the sheet: the card stays on the sheet.
        let margin = t.px(MARGIN) + if self.frame_of(window).1 { super::render::SHADOW } else { px(0.) };
        let card = place(peek.at, size(self.peek_width(window), t.px(HEAD + 1.) + body), room, margin);
        let label = match &peek.target.anchor {
            Anchor::Whole => None,
            _ => Some(format!("› {}", peek.target.label().split_once(" › ").map_or("", |(_, a)| a))),
        };
        let remark = match &peek.shown {
            Shown::Text { note, .. } => *note,
            Shown::Missing => None,
        };
        let title = match &peek.shown {
            Shown::Text { path, .. } if files::is_image(path) || files::is_table(path) => {
                path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            }
            Shown::Text { path, .. } => vault::stem(path),
            Shown::Missing => peek.target.note.clone(),
        };
        let open = cx.listener(|this, e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            this.peek_open(e.modifiers.shift, window, cx)
        });
        let head = div()
            .id("peek-title")
            .debug_selector(|| "peek-title".into())
            .h(t.px(HEAD))
            .flex_none()
            .px(t.px(14.))
            .flex()
            .items_center()
            .gap(t.px(6.))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, open)
            .child(
                // The note's title in the note's font, as it heads the note.
                div().font_family(t.sans).font_weight(gpui::FontWeight::SEMIBOLD).text_size(t.px(14.)).text_color(t.text)
                    .min_w_0().truncate().hover(|s| s.text_color(t.accent)).child(title),
            )
            .children(label.map(|l| div().flex_none().text_size(t.font_small()).text_color(t.soft).child(l)))
            .children(remark.map(|r| div().ml_auto().flex_none().text_size(t.font_small()).text_color(t.soft).child(r)));
        let body = match &peek.shown {
            Shown::Text { reader, .. } => div().flex_1().min_h_0().child(reader.clone()).into_any_element(),
            Shown::Missing => div()
                .flex_1()
                .px(t.px(14.))
                .flex()
                .items_center()
                .justify_between()
                .gap(t.px(8.))
                .text_size(t.font_body())
                .text_color(t.soft)
                .child(div().min_w_0().truncate().child(tr("No note bears this name", "Aucune note ne porte ce nom")))
                .child(
                    crate::ui::widgets::text_button("peek-create", tr("Create", "Créer"), t)
                        .debug_selector(|| "peek-create".into())
                        .flex_none()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.peek_open(false, window, cx)
                        })),
                )
                .into_any_element(),
        };
        let sheet = crate::ui::widgets::floating(t)
            .id("peek")
            .debug_selector(|| "peek".into())
            .occlude()
            .w(card.size.width)
            .h(card.size.height)
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(head)
            .child(div().h(px(1.)).flex_none().bg(t.border))
            .child(body);
        Some(gpui::anchored().position(card.origin).child(crate::ui::widgets::fade_in(sheet, "peek-in", &t)))
    }
}

/// What the tests see of the card.
#[cfg(test)]
impl Shell {
    /// The card's target as written in the link, and the text its reader shows (none for a missing note).
    pub(super) fn peek_seen(&self, cx: &App) -> Option<(String, Option<String>)> {
        let peek = self.peek.as_ref()?;
        let text = match &peek.shown {
            Shown::Text { reader, .. } => Some(reader.read(cx).text().trim_end_matches('\n').to_string()),
            Shown::Missing => None,
        };
        Some((peek.target.written(), text))
    }

    /// The card's reader.
    pub(super) fn peek_reader(&self) -> Option<Entity<Editor>> {
        match &self.peek.as_ref()?.shown {
            Shown::Text { reader, .. } => Some(reader.clone()),
            Shown::Missing => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    #[test]
    fn the_card_goes_under_the_link_or_above_it_and_stays_in_the_window() {
        let room = size(px(1000.), px(800.));
        let want = size(px(400.), px(300.));
        let m = px(8.);
        // Room under the link: against its bottom, from its left.
        assert_eq!(place(b(100., 100., 50., 20.), want, room, m), b(100., 120., 400., 300.));
        // Near the bottom: above it, its bottom against the link's top.
        assert_eq!(place(b(100., 700., 50., 20.), want, room, m), b(100., 400., 400., 300.));
        // Near the right edge: moved left, inside the margin.
        assert_eq!(place(b(900., 100., 50., 20.), want, room, m), b(592., 120., 400., 300.));
        // A window too low both ways: on the larger side, shortened to it.
        let low = size(px(1000.), px(300.));
        assert_eq!(place(b(100., 100., 50., 20.), want, low, m), b(100., 120., 400., 172.));
        assert_eq!(place(b(100., 200., 50., 20.), want, low, m), b(100., 8., 400., 192.));
        // A window narrower than the card: as wide as the room, at the margin.
        assert_eq!(place(b(10., 100., 50., 20.), want, size(px(300.), px(800.)), m), b(8., 120., 284., 300.));
    }
}
