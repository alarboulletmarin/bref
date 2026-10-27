//! A section of another note shown in place: `![[Note#Heading]]` alone on a line (what it points at is
//! `logic/embed.rs`). The section is laid out by a read-only editor of its own, framed under the line,
//! and follows its note: the file is looked at again from time to time.

use std::time::{Instant, SystemTime};

use super::*;
use crate::logic::embed::{self, Anchor, Target};

/// Inside the frame: the bar on its left, the space after it and on its right, the strip of its label, the
/// space under the section.
const BAR: Pixels = px(3.);
const PAD: Pixels = px(12.);
const HEAD: Pixels = px(24.);
const FOOT: Pixels = px(8.);
/// Between the line and the frame.
const GAP: Pixels = px(4.);
/// How many embeds deep a section goes: the note's embed is the first.
const DEPTH: usize = 3;
/// How often a note shown in an embed is looked at again.
const LOOK: Duration = Duration::from_millis(500);

/// An embed laid out: what its frame shows under the line.
pub(super) struct EmbedRow {
    pub(super) target: Target,
    /// The section's editor; none when the frame only says why there is no section.
    pub(super) child: Option<Entity<Editor>>,
    /// Why there is no section: a circular embed, a missing note or heading, too deep.
    pub(super) message: Option<String>,
    /// Height of the section.
    pub(super) inner: Pixels,
    /// What the section's editor is kept under.
    key: Option<(PathBuf, Anchor)>,
}

impl EmbedRow {
    /// Room under the line: the frame, with its strip and its margins.
    pub(super) fn height(&self) -> Pixels {
        GAP + HEAD + self.inner + FOOT
    }
}

/// What was read of a file: when it was looked at, its date and size, its text.
type Read = (Instant, Option<(SystemTime, u64)>, Rc<str>);

/// The notes an editor can show in embeds, and what it keeps of them.
#[derive(Default)]
pub(super) struct Embeds {
    /// Name of each note (lowercase, aliases too) → its file.
    pub(super) sources: Rc<HashMap<String, PathBuf>>,
    /// The note shown, and the embeds above it when this editor is itself an embed: a note
    /// already in the chain would loop.
    pub(super) trail: Vec<String>,
    /// What was read of each file: when it was looked at, its date, its text.
    texts: HashMap<PathBuf, Read>,
    /// One editor per embedded section, kept while the line stays.
    slots: HashMap<(PathBuf, Anchor), Slot>,
    /// A note changed since the last layout.
    stale: bool,
    /// The notes must be looked at again now, not after the usual delay.
    pub(super) recheck: bool,
}

struct Slot {
    child: Entity<Editor>,
    /// The text last given to it.
    text: Rc<str>,
    seen: u64,
}

impl Embeds {
    /// The text of the file, read again only if its date changed since it was last looked at.
    fn text(&mut self, path: &Path) -> Option<Rc<str>> {
        let now = Instant::now();
        if let Some((looked, _, text)) = self.texts.get(path)
            && now.duration_since(*looked) < LOOK
        {
            return Some(text.clone());
        }
        self.look(path, now)
    }

    /// Looks at the file's date, and reads it if it is not the one that was known.
    fn look(&mut self, path: &Path, now: Instant) -> Option<Rc<str>> {
        let stamp = fs::metadata(path).and_then(|m| Ok((m.modified()?, m.len()))).ok();
        if let Some((looked, known, text)) = self.texts.get_mut(path)
            && *known == stamp
        {
            *looked = now;
            return Some(text.clone());
        }
        let text: Rc<str> = fs::read_to_string(path).ok()?.into();
        // ponytail: the notes read are kept to see them change; forgotten all at once when there are many.
        if self.texts.len() > 64 {
            self.texts.clear();
        }
        self.stale |= self.texts.insert(path.to_path_buf(), (now, stamp, text.clone())).is_some();
        Some(text)
    }

    /// Looks again at the notes that are shown, those not looked at for a while (all of them if `force`): true if
    /// one changed, and the layout must be redone.
    pub(super) fn poll(&mut self) -> bool {
        let (now, force) = (Instant::now(), std::mem::take(&mut self.recheck));
        let due: Vec<PathBuf> = self.texts.iter().filter(|(_, (looked, ..))| force || now.duration_since(*looked) >= LOOK).map(|(p, _)| p.clone()).collect();
        for path in due {
            self.look(&path, now);
        }
        std::mem::take(&mut self.stale)
    }

    /// The file `name` stands for, in the vault.
    fn path_of(&self, name: &str) -> Option<PathBuf> {
        self.sources.get(&name.to_lowercase()).cloned()
    }

    /// The text of what the embed points at, or why there is none.
    fn section(&mut self, target: &Target) -> Result<(PathBuf, Rc<str>), String> {
        let path = self.path_of(&target.note).ok_or_else(|| tr("note not found", "note introuvable").to_string())?;
        let text = self.text(&path).ok_or_else(|| tr("note not found", "note introuvable").to_string())?;
        let shown = embed::section(&text, &target.anchor).ok_or_else(|| match &target.anchor {
            Anchor::Block(_) => tr("block not found", "bloc introuvable").to_string(),
            _ => tr("heading not found", "titre introuvable").to_string(),
        })?;
        Ok((path, shown.into()))
    }

    /// Lays out the embed of `target` at `width`: its editor is made, or given its text again, and measured.
    pub(super) fn row(&mut self, target: &Target, width: Pixels, host: &Editor, frame: u64, window: &mut Window, cx: &mut App) -> EmbedRow {
        let message = |why: String| EmbedRow { target: target.clone(), child: None, message: Some(why), inner: px(0.), key: None };
        let name = target.note.to_lowercase();
        if self.trail.contains(&name) {
            return message(tr("circular embed", "incorporation circulaire").to_string());
        }
        if self.trail.len() > DEPTH {
            return message(tr("embedded too deep", "incorporation trop profonde").to_string());
        }
        let (path, text) = match self.section(target) {
            Ok(found) => found,
            Err(why) => return message(why),
        };
        let key = (path.clone(), target.anchor.clone());
        let slot = match self.slots.remove(&key) {
            Some(slot) => slot,
            None => {
                let (theme, trail, sources) = (host.theme, self.trail.iter().cloned().chain([name]).collect::<Vec<_>>(), self.sources.clone());
                let child = cx.new(|cx| {
                    let mut child = Editor::new(theme, cx);
                    child.readonly = true;
                    child.embeds.trail = trail;
                    child.embeds.sources = sources;
                    child
                });
                Slot { child, text: "".into(), seen: 0 }
            }
        };
        let (dirs, images, theme) = (vec![path.parent().map(Path::to_path_buf), host.dirs.last().cloned()].into_iter().flatten().collect::<Vec<_>>(), host.images.clone(), host.theme);
        let mut slot = slot;
        let inner = slot.child.update(cx, |child, cx| {
            if slot.text != text || child.dirs != dirs || child.images != images || child.theme != theme {
                child.theme = theme;
                child.images = images;
                child.dirs = dirs;
                child.load(text.to_string(), 0, cx);
                slot.text = text.clone();
            }
            child.fit(width - BAR - PAD * 2., window, cx)
        });
        slot.seen = frame;
        let child = slot.child.clone();
        self.slots.insert(key.clone(), slot);
        EmbedRow { target: target.clone(), child: Some(child), message: None, inner, key: Some(key) }
    }

    /// The editors of embeds that are no longer on a line go: the lines laid out again this time, and the ones
    /// that were kept as they were, `rows`, hold theirs.
    pub(super) fn sweep(&mut self, rows: &[Row], frame: u64) {
        let kept: Vec<&(PathBuf, Anchor)> = rows.iter().filter_map(|r| r.embed.as_ref()?.key.as_ref()).collect();
        self.slots.retain(|key, slot| slot.seen == frame || kept.contains(&key));
    }

    fn has_slots(&self) -> bool {
        !self.slots.is_empty()
    }

    /// Whether some embedded editor still waits for a picture or a diagram.
    pub(super) fn loading(&self, cx: &App) -> bool {
        self.slots.values().any(|slot| slot.child.read(cx).loading)
    }
}

impl Editor {
    /// Tells the editor the notes it can show in embeds (name in lowercase, aliases too → file) and
    /// the name of the one it shows, which an embed may not lead back to.
    pub fn set_sources(&mut self, sources: Rc<HashMap<String, PathBuf>>, me: Option<String>) {
        self.embeds.sources = sources;
        self.embeds.trail = me.into_iter().map(|m| m.to_lowercase()).collect();
        self.embeds.recheck = true;
        self.epoch += 1;
        self.laid_out = None;
    }

    /// The name of the note shown changed: an embed may not lead back to it.
    pub fn set_trail(&mut self, me: Option<String>) {
        self.embeds.trail = me.into_iter().map(|m| m.to_lowercase()).collect();
        self.epoch += 1;
        self.laid_out = None;
    }

    /// A note may have changed on disk: its embeds are looked at again.
    pub fn refresh_embeds(&mut self, cx: &mut Context<Self>) {
        if self.embeds.has_slots() {
            self.embeds.recheck = true;
            cx.notify();
        }
    }

    /// Lays out this embedded section at `width` and says how tall it is.
    pub(super) fn fit(&mut self, width: Pixels, window: &mut Window, cx: &mut App) -> Pixels {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(width + SIDE * 2., px(10_000.)));
        let key = LayoutKey { version: self.version, width: f32::from(bounds.size.width).to_bits(), size: self.theme.size.to_bits(), cursor: 0, marked: None };
        if self.laid_out.as_ref() != Some(&key) {
            self.laid_out = None;
            self.relayout(bounds, window, cx);
            self.laid_out = (!self.loading).then_some(key);
        }
        self.total_y
    }

    /// Draws the frame of an embed and its section, under its line: `top` is where the frame starts.
    pub(super) fn paint_embed(&self, row: &Row, embed: &EmbedRow, window: &mut Window, cx: &mut App) {
        let t = self.theme;
        let top = self.origin.y + row.y + row.height - embed.height() + GAP;
        let area = Bounds::new(point(self.origin.x, top), size(self.width, embed.height() - GAP));
        if area.bottom() < self.viewport.top() || area.top() > self.viewport.bottom() {
            return;
        }
        window.paint_quad(fill(area, t.code_bg.opacity(0.5)).corner_radii(px(4.)));
        window.paint_quad(fill(Bounds::new(area.origin, size(BAR, area.size.height)), t.accent));
        // The source's name, on the right of the strip; a click on it opens the note there.
        let label = embed.target.label();
        let run = |color| TextRun { len: label.len(), font: font(sans()), color, background_color: None, underline: None, strikethrough: None };
        let hot = self.embed_label(row, embed).is_some_and(|b| b.contains(&self.pointer));
        let text = window.text_system().shape_line(SharedString::from(label.clone()), px(12.), &[run(if hot { t.accent } else { t.soft })], None);
        text.paint(point(area.right() - PAD - text.width, area.top() + px(4.)), px(16.), window, cx).ok();
        if let Some(why) = &embed.message {
            let run = TextRun { len: why.len(), font: font(sans()), color: t.soft, background_color: None, underline: None, strikethrough: None };
            let line = window.text_system().shape_line(SharedString::from(why.clone()), px(13.), &[run], None);
            line.paint(point(area.left() + BAR + PAD, area.top() + px(3.)), px(18.), window, cx).ok();
            return;
        }
        let Some(child) = &embed.child else { return };
        let inner = point(area.left() + BAR + PAD, area.top() + HEAD);
        let clip = Bounds::new(inner, size(self.width - BAR - PAD * 2., embed.inner)).intersect(&self.viewport);
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            child.update(cx, |child, cx| {
                child.place_embedded(inner, clip, window);
                child.paint(false, window, cx);
            });
        });
    }

    /// The zone of the source's name, in the frame under `row`.
    pub(super) fn embed_label(&self, row: &Row, embed: &EmbedRow) -> Option<Bounds<Pixels>> {
        let top = self.origin.y + row.y + row.height - embed.height() + GAP;
        let width = px(7.) * embed.target.label().chars().count() as f32 + PAD;
        Some(Bounds::new(point(self.origin.x + self.width - width - PAD, top), size(width + PAD, HEAD)))
    }

    /// The embed whose frame holds `at`, and whether `at` is on its label.
    pub(super) fn embed_at(&self, at: Point<Pixels>) -> Option<(&Row, &EmbedRow, bool)> {
        self.rows.iter().find_map(|row| {
            let embed = row.embed.as_ref()?;
            let top = self.origin.y + row.y + row.height - embed.height() + GAP;
            let area = Bounds::new(point(self.origin.x, top), size(self.width, embed.height() - GAP));
            area.contains(&at).then(|| (row, &**embed, self.embed_label(row, embed).is_some_and(|b| b.contains(&at))))
        })
    }

    /// The embed on the line holding byte `at`.
    pub(super) fn embed_target_at(&self, at: usize) -> Option<Target> {
        let line = self.line_range(self.clamp(at));
        (!self.in_code(line.start)).then(|| embed::parse(&self.content[line])).flatten()
    }

    /// The note to open for an embed line, the pointer's or the cursor's.
    pub(super) fn open_embed(&mut self, target: Target, aside: bool, cx: &mut Context<Self>) {
        cx.emit(EditorEvent::OpenAt(target, aside));
    }

    /// `Alt+Enter` on the line of an embed: opens its note, at the section.
    pub(super) fn open_embed_here(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.embed_target_at(self.cursor()) {
            self.open_embed(target, false, cx);
        }
    }

    /// The embeds of the note, for the tests: what each is written as, and what its frame says if it has no section.
    #[cfg(test)]
    pub fn embeds_shown(&self) -> Vec<(String, Option<String>, f32)> {
        self.rows.iter().filter_map(|r| r.embed.as_ref()).map(|e| (e.target.written(), e.message.clone(), f32::from(e.inner))).collect()
    }

    /// For each embed, a point in its frame and a point on its source's name, for the tests.
    #[cfg(test)]
    pub fn embed_points(&self) -> Vec<(Point<Pixels>, Point<Pixels>)> {
        self.rows
            .iter()
            .filter_map(|row| {
                let embed = row.embed.as_ref()?;
                let top = self.origin.y + row.y + row.height - embed.height() + GAP;
                let body = Bounds::new(point(self.origin.x, top), size(self.width, embed.height() - GAP)).center();
                Some((body, self.embed_label(row, embed)?.center()))
            })
            .collect()
    }

    /// The editor of the first embedded section, for the tests.
    #[cfg(test)]
    pub fn first_child(&self, _: &App) -> Option<Entity<Editor>> {
        self.rows.iter().find_map(|r| r.embed.as_ref())?.child.clone()
    }

    /// What the first embedded section shows, line by line, for the tests.
    #[cfg(test)]
    pub fn first_section(&self, cx: &App) -> Option<String> {
        let child = self.rows.iter().find_map(|r| r.embed.as_ref())?.child.as_ref()?;
        Some(child.read(cx).text().to_string())
    }
}
