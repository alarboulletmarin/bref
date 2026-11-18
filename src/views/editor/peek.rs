//! The card that previews a linked note (the shell draws it): the link under the pointer with `Ctrl`, or under
//! the cursor, and where it is on screen; and the read-only editor the card shows the note in.

use super::*;
use crate::logic::embed::{self, Target};

/// A link to preview: what it points at, and its zone in the window (the card goes under it, and closes
/// when the pointer leaves it for anything but the card).
#[derive(Clone, Debug, PartialEq)]
pub struct Peek {
    pub target: Target,
    pub at: Bounds<Pixels>,
}

impl Editor {
    /// A reader for the card that previews a note from `host`: the same theme and vault, nothing in it can be
    /// changed, and it previews no link of its own.
    pub fn reader(host: &Entity<Editor>, dirs: Vec<PathBuf>, cx: &mut App) -> Entity<Editor> {
        let (theme, vault) = { let h = host.read(cx); (h.theme, h.vault.clone()) };
        cx.new(|cx| {
            let mut e = Editor::new(theme, cx);
            e.reader = true;
            e.vault = vault;
            e.dirs = dirs;
            e
        })
    }

    /// The wikilink at the pointer `at`, its bytes in the text and what it points at.
    fn link_under(&self, at: Point<Pixels>) -> Option<(Range<usize>, Target)> {
        if self.stale() || self.rows.is_empty() {
            return None;
        }
        let i = self.index_at(at);
        let found = self.link_around(i, false)?;
        self.link_bounds(&found.0)?.contains(&at).then_some(found)
    }

    /// The wikilink holding byte `i` (or ending there, with `edge`): its bytes and target.
    fn link_around(&self, i: usize, edge: bool) -> Option<(Range<usize>, Target)> {
        let lr = self.line_range(self.clamp(i));
        if self.in_code(lr.start) {
            return None;
        }
        let (line, col) = (&self.content[lr.clone()], i - lr.start);
        let (r, _) = md::links(line).into_iter().find(|(r, link)| matches!(link, md::Link::Wiki(_)) && (r.contains(&col) || edge && r.end == col))?;
        let target = embed::link_target(&line[r.start + 2..r.end - 2])?;
        Some((lr.start + r.start..lr.start + r.end, target))
    }

    /// The zone of bytes `range` in the window: from its start to its end on one visual line, the page's width
    /// when it wraps.
    fn link_bounds(&self, range: &Range<usize>) -> Option<Bounds<Pixels>> {
        let row = &self.rows[self.row_at(range.start)?];
        let base = self.origin + point(px(0.), row.y + row.pad);
        let (a, b) = (row.pos(range.start - row.start), row.pos((range.end - row.start).min(row.len)));
        Some(match a.y == b.y {
            true => Bounds::from_corners(base + a, base + point(b.x, b.y + row.lh)),
            false => Bounds::from_corners(base + point(px(0.), a.y), base + point(self.width, b.y + row.lh)),
        })
    }

    /// The pointer moved, or `Ctrl` changed: over a link with `Ctrl` held, the card is asked for, once per link.
    /// Closing it is the shell's: it knows where the card is.
    pub(super) fn aim_peek(&mut self, at: Point<Pixels>, held: gpui::Modifiers, cx: &mut Context<Self>) {
        if self.reader || self.readonly || !held.secondary() {
            return;
        }
        let Some((range, target)) = self.link_under(at) else { return };
        if self.peeked.as_ref() == Some(&range) {
            return;
        }
        let Some(at) = self.link_bounds(&range) else { return };
        self.peeked = Some(range);
        cx.emit(EditorEvent::Peek(Peek { target, at }));
    }

    /// The link under the cursor (or just before it), to preview from the keyboard.
    pub fn peek_at_cursor(&mut self) -> Option<Peek> {
        if self.stale() {
            return None;
        }
        let (range, target) = self.link_around(self.cursor(), true)?;
        let at = self.link_bounds(&range)?;
        self.peeked = Some(range);
        Some(Peek { target, at })
    }

    /// Gives the reader its text. Its cursor waits on an empty line after it, so that no line of the text shows
    /// as typed; the card stays at its top.
    pub fn show_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = format!("{text}\n");
        let end = text.len();
        self.load(text, end, cx);
        self.reveal = false;
    }

    /// The card went: the same link may ask for it again.
    pub fn forget_peek(&mut self) {
        self.peeked = None;
    }

    /// How tall the text is with its top margin (the empty line under it is the bottom one), as last laid out;
    /// before that, a guess from its lines.
    pub fn content_height(&self) -> Pixels {
        match self.rows.is_empty() {
            true => px(self.theme.size * 1.65) * self.content.split('\n').count() as f32 + TOP,
            false => self.total_y + TOP,
        }
    }
}
