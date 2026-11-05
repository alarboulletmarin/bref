//! A note laid out on A4 pages for print: one SVG per page, with its headings, lists, quotes,
//! code, tables, pictures and figures, in Bref Light on white whatever the window's theme.
//! Pure functions: the pictures and the embedded notes come from the caller (`find`).
//!
//! Each source line is a line of the page, as in the editor. The text stays text (`<text>`),
//! so the PDF made of these pages can be searched and copied.

use std::fmt::Write as _;
use std::sync::LazyLock;

use base64::Engine as _;
use ttf_parser::Face;
use unicode_width::UnicodeWidthChar;

use crate::lang::tr;
use crate::logic::diagram::Diagram;
use crate::logic::markdown::{self as md, Kind, Link};
use crate::ui::{figure, svg, theme};

/// A4, in points.
pub const PAGE: (f32, f32) = (595.28, 841.89);
/// 20 mm.
const MARGIN: f32 = 56.69;
const WIDTH: f32 = PAGE.0 - 2. * MARGIN;
/// Room for the text; the page number sits in the bottom margin.
const HEIGHT: f32 = PAGE.1 - 2. * MARGIN;
const BODY: f32 = 10.5;
const LEAD: f32 = 1.5;
const MONO: f32 = 8.5;
/// A CSS pixel, in points.
const PX: f32 = 0.75;
/// The sans, mono and serif of the app, measured here and named in the SVG.
const SANS: &str = "IBM Plex Sans";
const MONO_FONT: &str = "IBM Plex Mono";
const SERIF: &str = "IBM Plex Serif";

/// What a picture or an embed of the note stands for, as the caller found it.
pub enum Found {
    /// A PNG, JPEG or GIF file, as read, that can be drawn (see `picture`).
    Picture(Vec<u8>),
    /// The text of an SVG file.
    Svg(String),
    /// The text of the embedded part of a note.
    Section(String),
    /// Not found, or not readable.
    Missing,
}

/// A page: its SVG, and its web links (x, y, width, height in points from the top left, address).
pub struct Page {
    pub svg: String,
    pub links: Vec<([f32; 4], String)>,
}

/// The pages of a note, and what could not be printed in it (pictures, embeds, figures).
pub struct Printed {
    pub pages: Vec<Page>,
    pub missing: Vec<String>,
}

/// Lays out `text` on pages. `find` is asked for each picture and embed, by the path or name written.
pub fn print(text: &str, find: &dyn Fn(&str) -> Found) -> Printed {
    let ink = Ink::new();
    let mut out = Rows { rows: Vec::new(), missing: Vec::new(), group: 0, gap: 0., boxed: 0 };
    out.note(&text[md::front_matter(text)..], find, &ink, 0);
    if out.boxed > 0 {
        out.missing.push(format!("{} emoji ({BOX})", out.boxed));
    }
    Printed { pages: paginate(&out.rows, &ink), missing: out.missing }
}

/// The print colours: Bref Light's text, accent and code tints, on white.
struct Ink {
    text: String,
    soft: String,
    accent: String,
    code_bg: String,
    line: String,
    marks: Vec<String>,
    string: String,
    number: String,
    look: figure::Look,
    rgb: [u8; 3],
}

impl Ink {
    fn new() -> Self {
        let t = theme::Theme::of(&crate::logic::prefs::Prefs::parse("theme=Bref Light"), gpui::WindowAppearance::Light);
        let rgb = |c: gpui::Hsla| {
            let c = gpui::Rgba::from(c);
            [c.r, c.g, c.b].map(|v| (v * 255.).round() as u8)
        };
        let hex = |c: gpui::Hsla| {
            let [r, g, b] = rgb(c);
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let mut look = figure::Look::of(&t);
        look.font = SANS.into();
        look.bg = "#ffffff".into();
        Ink {
            text: hex(t.text),
            soft: hex(t.soft),
            accent: hex(t.accent),
            code_bg: hex(t.code_bg),
            line: hex(t.line),
            marks: (0..md::MARKS.len()).map(|i| hex(t.highlight(i))).collect(),
            string: hex(t.string),
            number: hex(t.number),
            look,
            rgb: rgb(t.text),
        }
    }
}

// ----- Measuring: the advances of the app's fonts, with no shaping (no kerning).

static FACES: LazyLock<Vec<Face<'static>>> =
    LazyLock::new(|| crate::ui::assets::FONT_FILES.iter().filter_map(|data| Face::parse(data, 0).ok()).collect());

/// Font of a piece of text: the base (body or heading) and the inline styles.
#[derive(Clone, Copy, PartialEq)]
struct Font {
    serif: bool,
    size: f32,
}

/// Index in `FONT_FILES` of the face that draws `style` over `font`.
fn face(style: u16, font: Font) -> usize {
    let (bold, italic) = (style & md::BOLD != 0, style & md::ITALIC != 0);
    match () {
        _ if style & md::CODE != 0 => 4 + italic as usize + 2 * bold as usize,
        _ if font.serif => 8 + italic as usize,
        _ => italic as usize + 2 * bold as usize,
    }
}

/// Size of `style` over `font`: inline code a little smaller.
fn size(style: u16, font: Font) -> f32 {
    if style & md::CODE != 0 && !font.serif && font.size > MONO { font.size * 0.9 } else { font.size }
}

/// Advance of `c`, in points.
// ponytail: a character missing from the app's fonts (CJK, emoji) is measured at one em, or half
// of it when narrow; the system font that draws it may differ a little. Ask fontdb for the real face if it shows.
fn advance(c: char, style: u16, font: Font) -> f32 {
    let size = size(style, font);
    let wide = match c.width() {
        Some(0) => return 0.,
        Some(2) => true,
        _ => false,
    };
    let Some(f) = FACES.get(face(style, font)) else { return size * 0.6 };
    match f.glyph_index(c).and_then(|g| f.glyph_hor_advance(g)) {
        Some(a) => a as f32 / f.units_per_em() as f32 * size,
        None if wide => size,
        None => size * 0.6,
    }
}

// ----- Inline text: the characters shown, with their styles.

/// A character as printed: its style flags (see `markdown`), its web link, its width.
#[derive(Clone)]
struct Glyph {
    c: char,
    style: u16,
    link: Option<usize>,
    w: f32,
    /// Colour of its highlight (see `markdown::MARKS`).
    mark: u8,
}

/// A line of text and the web addresses its glyphs point at.
#[derive(Default)]
struct Inline {
    glyphs: Vec<Glyph>,
    urls: Vec<String>,
    /// Emoji drawn as a box (see `tame`).
    boxed: usize,
}

/// What an emoji becomes on paper: the replacement character, which the app's font has.
const BOX: char = '\u{FFFD}';

/// A character of the emoji ranges that the app's text font does not draw.
fn is_emoji(c: char) -> bool {
    let symbol = matches!(c, '\u{2300}'..='\u{23FF}' | '\u{2600}'..='\u{27BF}' | '\u{2B00}'..='\u{2BFF}');
    matches!(c, '\u{1F000}'..='\u{1FAFF}') || symbol && FACES.first().is_some_and(|f| f.glyph_index(c).is_none())
}

/// Colour emoji cannot go in a PDF font (their font cannot be subset, the whole export would fail):
/// each emoji, with its joiners, skin tone and variants, becomes a box. The number of boxes.
fn tame(glyphs: &mut Vec<Glyph>, font: Font) -> usize {
    let (mut boxed, mut in_emoji, mut joined) = (0, false, false);
    let mut out = Vec::with_capacity(glyphs.len());
    for g in glyphs.drain(..) {
        match g.c {
            // Invisible on their own, and drawn by the emoji font after one.
            '\u{FE0E}' | '\u{FE0F}' | '\u{20E3}' | '\u{E0020}'..='\u{E007F}' => {}
            '\u{200D}' if in_emoji => joined = true,
            '\u{1F3FB}'..='\u{1F3FF}' if in_emoji => {}
            c if is_emoji(c) => {
                if !(in_emoji && joined) {
                    boxed += 1;
                    out.push(Glyph { c: BOX, w: advance(BOX, g.style, font), ..g });
                }
                (in_emoji, joined) = (true, false);
            }
            _ => {
                (in_emoji, joined) = (false, false);
                out.push(g);
            }
        }
    }
    *glyphs = out;
    boxed
}

const SHOWN: u16 = md::BOLD | md::ITALIC | md::CODE | md::STRIKE | md::LINK | md::TAG | md::HIGHLIGHT | md::UNDER | md::KEYWORD | md::STRING | md::NUMBER;

/// The printed characters of `line` from byte `from`: the marks of the styles, the address of a
/// `[text](address)` and the text of a comment are left out, as on screen when the cursor is elsewhere.
fn inline(line: &str, from: usize, font: Font) -> Inline {
    let flags = md::flags(line, from);
    let mut hidden: Vec<bool> = flags.iter().map(|f| f & md::DIM != 0).collect();
    let mut link = vec![None; line.len()];
    let mut out = Inline::default();
    for (r, l) in md::links(line).into_iter().filter(|(r, _)| r.start >= from) {
        match l {
            // `[[Note|alias]]` shows its alias.
            Link::Wiki(_) => {
                if let Some(bar) = line[r.start + 2..r.end - 2].find('|') {
                    hidden[r.start + 2..=r.start + 2 + bar].fill(true);
                }
            }
            Link::Url(url) => {
                link[r].fill(Some(out.urls.len()));
                out.urls.push(url);
            }
            Link::Tag(_) => {}
        }
    }
    let mut mark = vec![0u8; line.len()];
    for (r, color) in md::highlights(&line[from..], false) {
        mark[from + r.start..from + r.end].fill(color as u8);
    }
    let symbols = md::symbols(line, from, &flags);
    let mut i = from;
    while i < line.len() {
        let shown = out.glyphs.len();
        let next = if let Some((_, len, sign)) = symbols.iter().find(|s| s.0 == i) {
            out.push_str(sign, flags[i] & SHOWN, link[i], font);
            i + len
        } else {
            let c = line[i..].chars().next().unwrap_or(' ');
            if !hidden[i] {
                let c = if c == '\t' { ' ' } else { c };
                out.glyphs.push(Glyph { c, style: flags[i] & SHOWN, link: link[i], w: advance(c, flags[i] & SHOWN, font), mark: 0 });
            }
            i + c.len_utf8()
        };
        out.glyphs[shown..].iter_mut().for_each(|g| g.mark = mark[i]);
        i = next;
    }
    out.boxed = tame(&mut out.glyphs, font);
    out
}

impl Inline {
    fn push_str(&mut self, text: &str, style: u16, link: Option<usize>, font: Font) {
        self.glyphs.extend(text.chars().map(|c| Glyph { c, style, link, w: advance(c, style, font), mark: 0 }));
    }

    fn plain(text: &str, style: u16, font: Font) -> Self {
        let mut out = Inline::default();
        out.push_str(text, style, None, font);
        out.boxed = tame(&mut out.glyphs, font);
        out
    }

    fn width(&self) -> f32 {
        self.glyphs.iter().map(|g| g.w).sum()
    }
}

/// Where `glyphs` go to the next line to fit in `width`: after a space or around a wide
/// character, otherwise anywhere (a long word, an address); `anywhere` for code, where spaces stay.
fn wrap(glyphs: &[Glyph], width: f32, anywhere: bool) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let (mut start, mut x, mut cut) = (0, 0., None);
    let mut i = 0;
    while i < glyphs.len() {
        let g = &glyphs[i];
        // A space may hang past the edge: the line breaks after it.
        if x + g.w > width && i > start && (anywhere || g.c != ' ') {
            let end = cut.filter(|&c| c > start && !anywhere).unwrap_or(i);
            lines.push(start..end);
            start = end;
            // The spaces a line breaks on are not carried to the next one.
            while !anywhere && start < glyphs.len() && glyphs[start].c == ' ' {
                start += 1;
            }
            (x, cut, i) = (0., None, start);
            continue;
        }
        x += g.w;
        let wide = g.c.width() == Some(2);
        if g.c == ' ' || wide {
            cut = Some(i + 1);
        }
        if wide || glyphs.get(i + 1).is_some_and(|n| n.c.width() == Some(2)) {
            cut = Some(i + 1);
        }
        i += 1;
    }
    if start < glyphs.len() || lines.is_empty() {
        lines.push(start..glyphs.len());
    }
    lines
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Height of a line of `font`, and the distance from its top to its baseline.
fn metrics(font: Font) -> (f32, f32) {
    let lh = font.size * LEAD;
    (lh, (lh - font.size * 1.3) / 2. + font.size * 1.025)
}

/// Draws the glyphs of one printed line at `x`, its top at `top`, in `color`: their backgrounds
/// (inline code, highlight), then their text, piece by piece of the same style.
fn draw_line(svg: &mut String, links: &mut Vec<([f32; 4], String)>, text: &Inline, glyphs: &[Glyph], (x, top): (f32, f32), font: Font, color: &str, ink: &Ink) {
    let (lh, base) = metrics(font);
    let mut at = x;
    let mut i = 0;
    while i < glyphs.len() {
        // A character the app's font lacks is drawn on its own: the font found for it would
        // otherwise draw the whole piece.
        let own = |g: &Glyph| g.c == ' ' || FACES.get(face(g.style, font)).is_some_and(|f| f.glyph_index(g.c).is_some());
        let (style, link, mark, has) = (glyphs[i].style, glyphs[i].link, glyphs[i].mark, own(&glyphs[i]));
        let n = glyphs[i..].iter().take_while(|g| g.style == style && g.link == link && g.mark == mark && own(g) == has).count();
        let piece = &glyphs[i..i + n];
        let w: f32 = piece.iter().map(|g| g.w).sum();
        let shown: String = piece.iter().map(|g| g.c).collect();
        let fill = if style & md::CODE != 0 && style & (md::KEYWORD | md::STRING | md::NUMBER) == 0 && font.size > MONO {
            Some(&ink.code_bg)
        } else if style & md::HIGHLIGHT != 0 {
            Some(&ink.marks[(mark as usize).min(ink.marks.len() - 1)])
        } else {
            None
        };
        if let Some(fill) = fill.filter(|_| !shown.trim().is_empty()) {
            let _ = write!(svg, r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="2" fill="{fill}"/>"#, at - 1., top + (lh - font.size * 1.3) / 2., w + 2., font.size * 1.3);
        }
        if !shown.trim().is_empty() {
            let family = if style & md::CODE != 0 { MONO_FONT } else if font.serif { SERIF } else { SANS };
            let weight = if font.serif && style & md::CODE == 0 { "600" } else if style & md::BOLD != 0 { "700" } else { "400" };
            let fill = match () {
                _ if style & md::KEYWORD != 0 => &ink.accent,
                _ if style & md::STRING != 0 => &ink.string,
                _ if style & md::NUMBER != 0 => &ink.number,
                _ if style & (md::LINK | md::TAG) != 0 => &ink.accent,
                _ => color,
            };
            let mut deco = Vec::new();
            if style & md::STRIKE != 0 {
                deco.push("line-through");
            }
            if style & md::UNDER != 0 || link.is_some() {
                deco.push("underline");
            }
            let _ = write!(
                svg,
                r#"<text x="{at:.2}" y="{:.2}" font-family="{family}" font-size="{:.2}" font-weight="{weight}" font-style="{}" fill="{fill}"{}>{}</text>"#,
                top + base,
                size(style, font),
                if style & md::ITALIC != 0 { "italic" } else { "normal" },
                if deco.is_empty() { String::new() } else { format!(r#" text-decoration="{}""#, deco.join(" ")) },
                escape(&shown)
            );
        }
        if let Some(l) = link {
            links.push(([at, top, w, lh], text.urls[l].clone()));
        }
        at += w;
        i += n;
    }
}

// ----- Rows: the unit of pagination. A page break never splits a row.

/// A frame drawn behind consecutive rows of the same group, page by page.
#[derive(Clone, Copy, PartialEq)]
enum Frame {
    /// A code block: a tinted rounded sheet.
    Code,
    /// A quote, a callout or an embed: a bar on the left.
    Bar,
}

struct Row {
    /// Space above, dropped at the top of a page.
    before: f32,
    h: f32,
    /// SVG, from the row's top left.
    svg: String,
    links: Vec<([f32; 4], String)>,
    /// Goes on the same page as the next row (a heading).
    keep: bool,
    frame: Option<(usize, Frame)>,
}

struct Rows {
    rows: Vec<Row>,
    missing: Vec<String>,
    /// Number of the current frame group.
    group: usize,
    /// Space waiting for the next row (blank lines).
    gap: f32,
    /// Emoji drawn as a box.
    boxed: usize,
}

/// Text rows of a source line: its glyphs from x `indent`, wrapped at the page width.
fn text_rows(text: &Inline, indent: f32, font: Font, color: &str, ink: &Ink, anywhere: bool) -> Vec<(String, Vec<([f32; 4], String)>)> {
    wrap(&text.glyphs, WIDTH - indent, anywhere)
        .into_iter()
        .map(|r| {
            let (mut svg, mut links) = (String::new(), Vec::new());
            draw_line(&mut svg, &mut links, text, &text.glyphs[r], (indent, 0.), font, color, ink);
            (svg, links)
        })
        .collect()
}

impl Rows {
    fn push(&mut self, mut row: Row) {
        row.before += std::mem::take(&mut self.gap);
        self.rows.push(row);
    }

    /// Rows of one wrapped source line, the first with `before` and `lead` (a bullet) drawn in it.
    fn lines(&mut self, text: &Inline, indent: f32, font: Font, color: &str, ink: &Ink, before: f32, lead: &str, frame: Option<(usize, Frame)>, keep: bool) {
        let (lh, _) = metrics(font);
        self.boxed += text.boxed;
        for (i, (mut svg, links)) in text_rows(text, indent, font, color, ink, false).into_iter().enumerate() {
            if i == 0 {
                svg.insert_str(0, lead);
            }
            self.push(Row { before: if i == 0 { before } else { 0. }, h: lh, svg, links, keep, frame });
        }
    }

    fn missing(&mut self, what: String, ink: &Ink) {
        let font = Font { serif: false, size: BODY };
        let said = format!("[{} : {what}]", tr("Not printed", "Non imprimé"));
        let text = Inline::plain(&said, md::ITALIC, font);
        self.lines(&text, 0., font, &ink.soft, ink, 0., "", None, false);
        self.missing.push(what);
    }

    /// A picture or a figure: an SVG or a raster, `w`×`h` points at most, scaled to fit the page.
    fn picture(&mut self, data: &[u8], mime: &str, (w, h): (f32, f32), indent: f32) {
        let scale = ((WIDTH - indent) / w).min(HEIGHT * 0.9 / h).min(1.);
        let (w, h) = (w * scale, h * scale);
        let data = base64::engine::general_purpose::STANDARD.encode(data);
        let svg = format!(r#"<image x="{indent:.2}" y="0" width="{w:.2}" height="{h:.2}" preserveAspectRatio="xMidYMid meet" xlink:href="data:{mime};base64,{data}"/>"#);
        self.push(Row { before: 4., h: h + 4., svg, links: Vec::new(), keep: false, frame: None });
    }

    /// An SVG figure, at its own size (CSS pixels).
    fn figure(&mut self, svg: &str, what: &str, indent: f32, ink: &Ink) {
        match svg::parse(svg).map(|t| (t.size().width() * PX, t.size().height() * PX)) {
            Some(size) => self.picture(svg.as_bytes(), "image/svg+xml", size, indent),
            None => self.missing(what.to_string(), ink),
        }
    }

    /// The picture or the embed written `path` on a line.
    fn embed(&mut self, path: &str, find: &dyn Fn(&str) -> Found, ink: &Ink, depth: usize) {
        match find(path) {
            Found::Picture(bytes) => match mime(&bytes) {
                Some(mime) => match raster_size(&bytes) {
                    Some(size) => self.picture(&bytes, mime, size, 0.),
                    None => self.missing(path.to_string(), ink),
                },
                None => self.missing(format!("{path} ({})", tr("format not supported", "format non pris en charge")), ink),
            },
            Found::Svg(text) => {
                let drawn = Diagram::from_svg(&text).map_or_else(|| svg::readable(&text, false), |d| d.to_svg(u32::from_be_bytes([0, ink.rgb[0], ink.rgb[1], ink.rgb[2]])));
                self.figure(&drawn, path, 0., ink);
            }
            // ponytail: the embedded rows are moved 12 points right after being laid out at full width,
            // so a long line can reach into the right margin; lay them out narrower if it shows.
            Found::Section(text) if depth == 0 => {
                self.group += 1;
                let start = self.rows.len();
                self.note(&text, find, ink, depth + 1);
                let group = self.group;
                for row in &mut self.rows[start..] {
                    row.frame = row.frame.or(Some((group, Frame::Bar)));
                    row.svg = format!(r#"<g transform="translate(12 0)">{}</g>"#, row.svg);
                    row.links.iter_mut().for_each(|l| l.0[0] += 12.);
                }
                self.group += 1;
            }
            // An embed inside an embed is left as its link.
            Found::Section(_) => {
                let font = Font { serif: false, size: BODY };
                let text = Inline::plain(&format!("[[{path}]]"), md::LINK, font);
                self.lines(&text, 0., font, &ink.text, ink, 0., "", None, false);
            }
            Found::Missing => self.missing(path.to_string(), ink),
        }
    }

    /// The rows of a note's text.
    fn note(&mut self, text: &str, find: &dyn Fn(&str) -> Found, ink: &Ink, depth: usize) {
        let body = Font { serif: false, size: BODY };
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        let mut quoted = false;
        while i < lines.len() {
            let line = lines[i];
            i += 1;
            let (kind, marker) = md::classify(line, false);
            if kind != Kind::Quote && quoted {
                quoted = false;
                self.group += 1;
            }
            if line.trim().is_empty() {
                self.gap += BODY * 0.6;
                continue;
            }
            // A fenced block, if it is closed: code, or a Mermaid diagram.
            if kind == Kind::Fence
                && let Some(len) = lines[i..].iter().position(|l| md::classify(l, true).0 == Kind::Fence)
            {
                let lang = line.trim_start()[3..].trim().to_lowercase();
                let block = &lines[i..i + len];
                i += len + 1;
                if lang == "mermaid" {
                    match figure::mermaid(&block.join("\n"), &ink.look) {
                        Ok(svg) => self.figure(&svg, "Mermaid", 0., ink),
                        Err(e) => self.missing(format!("Mermaid ({e})"), ink),
                    }
                } else {
                    self.code(block, &lang, ink);
                }
                continue;
            }
            // A formula over several lines, between two `$$` lines.
            if line.trim() == "$$"
                && let Some(len) = lines[i..].iter().position(|l| l.trim() == "$$")
            {
                let source = lines[i..i + len].join("\n");
                i += len + 1;
                self.formula(&source, ink);
                continue;
            }
            if kind == Kind::Table {
                let len = lines[i..].iter().take_while(|l| md::classify(l, false).0 == Kind::Table).count();
                self.table(&lines[i - 1..i + len], ink);
                i += len;
                continue;
            }
            if kind == Kind::Rule {
                let svg = format!(r#"<line x1="0" y1="6" x2="{WIDTH:.2}" y2="6" stroke="{}" stroke-width="0.75"/>"#, ink.line);
                self.push(Row { before: 4., h: 12., svg, links: Vec::new(), keep: false, frame: None });
                continue;
            }
            if let Some((range, path)) = md::image(line) {
                let rest = format!("{}{}", &line[..range.start], &line[range.end..]);
                if !rest.trim().is_empty() {
                    self.lines(&inline(&rest, 0, body), 0., body, &ink.text, ink, 0., "", None, false);
                }
                self.embed(&path, find, ink, depth);
                continue;
            }
            if let Some((range, source, true)) = md::math(line)
                && line[..range.start].trim().is_empty()
                && line[range.end..].trim().is_empty()
            {
                self.formula(source, ink);
                continue;
            }
            match kind {
                Kind::Heading(level) => {
                    let scale = [1.94, 1.44, 1.19, 1.06, 1., 1.][(level as usize - 1).min(5)];
                    let font = Font { serif: true, size: BODY * scale };
                    let before = if self.rows.is_empty() { 0. } else { font.size * 0.8 };
                    self.lines(&inline(line, marker, font), 0., font, &ink.text, ink, before, "", None, true);
                }
                Kind::Quote => {
                    if !quoted {
                        quoted = true;
                        self.group += 1;
                    }
                    let mut text = inline(line, marker, body);
                    // `> [!NOTE] Title`: the type, in bold, in place of its brackets.
                    if let Some(r) = md::callout(line) {
                        let label = line[r.start + 2..r.end - 1].to_lowercase();
                        let mut label: Vec<char> = label.chars().collect();
                        if let Some(c) = label.first_mut() {
                            *c = c.to_ascii_uppercase();
                        }
                        let mut head = Inline::plain(&label.into_iter().collect::<String>(), md::BOLD, body);
                        let after = inline(line, r.end, body);
                        head.glyphs.extend(after.glyphs.into_iter().map(|mut g| {
                            g.link = g.link.map(|l| l + head.urls.len());
                            g
                        }));
                        head.urls.extend(after.urls);
                        text = head;
                    }
                    let frame = Some((self.group, Frame::Bar));
                    self.lines(&text, 12., body, &ink.soft, ink, 0., "", frame, false);
                }
                Kind::Bullet | Kind::Ordered | Kind::Task(_) => {
                    let spaces = line.len() - line.trim_start_matches([' ', '\t']).len();
                    let cols: usize = line.chars().take_while(|c| matches!(c, ' ' | '\t')).map(|c| if c == '\t' { 4 } else { 1 }).sum();
                    let x = cols as f32 / 4. * 16.;
                    let (lh, base) = metrics(body);
                    let lead = match kind {
                        Kind::Task(done) => {
                            let (s, y) = (8., (lh - 8.) / 2.);
                            let check = if done {
                                format!(r#"<path d="M{:.2} {:.2} l2.2 2.4 l4 -5" fill="none" stroke="{}" stroke-width="1.3"/>"#, x + 1.8, y + 4.2, ink.accent)
                            } else {
                                String::new()
                            };
                            format!(r#"<rect x="{x:.2}" y="{y:.2}" width="{s}" height="{s}" rx="1.5" fill="none" stroke="{}" stroke-width="0.9"/>{check}"#, ink.soft)
                        }
                        Kind::Ordered => {
                            let number = line[spaces..marker].trim_end();
                            format!(r#"<text x="{x:.2}" y="{base:.2}" font-family="{SANS}" font-size="{BODY}" fill="{}">{}</text>"#, ink.soft, escape(number))
                        }
                        _ => format!(r#"<circle cx="{:.2}" cy="{:.2}" r="1.8" fill="{}"/>"#, x + 4., lh / 2., ink.soft),
                    };
                    self.lines(&inline(line, marker, body), x + 16., body, &ink.text, ink, 0., &lead, None, false);
                }
                _ => self.lines(&inline(line, marker, body), 0., body, &ink.text, ink, 0., "", None, false),
            }
        }
    }

    /// A display formula, centred.
    fn formula(&mut self, source: &str, ink: &Ink) {
        match figure::latex(source, true, ink.rgb, BODY / PX) {
            Some((svg, w, _)) => {
                let indent = ((WIDTH - w * PX) / 2.).max(0.);
                self.figure(&svg, source, indent, ink)
            }
            None => self.missing(format!("$${source}$$"), ink),
        }
    }

    /// A code block: its lines in mono, cut anywhere to fit, coloured as in the editor, on a tinted sheet.
    fn code(&mut self, block: &[&str], lang: &str, ink: &Ink) {
        let font = Font { serif: false, size: MONO };
        let (lh, _) = metrics(font);
        self.group += 1;
        let frame = Some((self.group, Frame::Code));
        let pad = |before| Row { before, h: 6., svg: String::new(), links: Vec::new(), keep: false, frame };
        self.push(pad(4.));
        for line in block {
            let mut flags = vec![0u16; line.len()];
            md::code(line, lang, &mut flags);
            let mut text = Inline::default();
            for (i, c) in line.char_indices() {
                let style = md::CODE | (flags[i] & (md::KEYWORD | md::STRING | md::NUMBER));
                let c = if c == '\t' { ' ' } else { c };
                let n = if line[i..].starts_with('\t') { 4 } else { 1 };
                (0..n).for_each(|_| text.glyphs.push(Glyph { c, style, link: None, w: advance(c, style, font), mark: 0 }));
            }
            self.boxed += tame(&mut text.glyphs, font);
            for (svg, _) in text_rows(&text, 8., font, &ink.text, ink, true) {
                self.push(Row { before: 0., h: lh, svg, links: Vec::new(), keep: false, frame });
            }
        }
        self.push(pad(0.));
        self.group += 1;
        self.gap = 4.;
    }

    /// A table: columns as wide as their text, shared out when they do not fit; its cells wrap.
    // ponytail: a row taller than a page runs off it, and the header is not repeated on the next page.
    fn table(&mut self, lines: &[&str], ink: &Ink) {
        let font = Font { serif: false, size: BODY * 0.9 };
        let bold = Font { serif: false, size: font.size };
        let (rows, _) = md::table(&lines.join("\n"));
        let rows: Vec<&Vec<String>> = rows.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, r)| r).collect();
        let cells: Vec<Vec<Inline>> = rows
            .iter()
            .enumerate()
            .map(|(r, row)| {
                row.iter()
                    .map(|cell| {
                        let mut text = inline(&cell.replace("\\|", "|"), 0, font);
                        self.boxed += text.boxed;
                        if r == 0 {
                            text.glyphs.iter_mut().for_each(|g| {
                                g.style |= md::BOLD;
                                g.w = advance(g.c, g.style, bold);
                            });
                        }
                        text
                    })
                    .collect()
            })
            .collect();
        let pad = 4.;
        let cols = cells.first().map_or(0, Vec::len);
        let natural: Vec<f32> = (0..cols).map(|c| cells.iter().map(|row| row[c].width()).fold(0., f32::max) + 2. * pad).collect();
        let widths = share(&natural, WIDTH);
        let (lh, _) = metrics(font);
        for (r, row) in cells.iter().enumerate() {
            let wrapped: Vec<Vec<std::ops::Range<usize>>> = row.iter().enumerate().map(|(c, t)| wrap(&t.glyphs, widths[c] - 2. * pad, false)).collect();
            let h = wrapped.iter().map(Vec::len).max().unwrap_or(1) as f32 * lh + 2. * pad;
            let (mut svg, mut links) = (String::new(), Vec::new());
            let total: f32 = widths.iter().sum();
            if r == 0 {
                let _ = write!(svg, r#"<rect x="0" y="0" width="{total:.2}" height="{h:.2}" fill="{}"/>"#, ink.code_bg);
                let _ = write!(svg, r#"<line x1="0" y1="0" x2="{total:.2}" y2="0" stroke="{}" stroke-width="0.6"/>"#, ink.line);
            }
            let _ = write!(svg, r#"<line x1="0" y1="{h:.2}" x2="{total:.2}" y2="{h:.2}" stroke="{}" stroke-width="0.6"/>"#, ink.line);
            let mut x = 0.;
            for (c, text) in row.iter().enumerate() {
                let _ = write!(svg, r#"<line x1="{x:.2}" y1="0" x2="{x:.2}" y2="{h:.2}" stroke="{}" stroke-width="0.6"/>"#, ink.line);
                for (l, range) in wrapped[c].iter().enumerate() {
                    draw_line(&mut svg, &mut links, text, &text.glyphs[range.clone()], (x + pad, pad + l as f32 * lh), font, &ink.text, ink);
                }
                x += widths[c];
            }
            let _ = write!(svg, r#"<line x1="{x:.2}" y1="0" x2="{x:.2}" y2="{h:.2}" stroke="{}" stroke-width="0.6"/>"#, ink.line);
            self.push(Row { before: if r == 0 { 4. } else { 0. }, h, svg, links, keep: r == 0, frame: None });
        }
        self.gap = 4.;
    }
}

/// Widths of columns that want `natural` within `room`: all of it if it fits; otherwise the narrow
/// ones keep theirs and the others share what is left equally.
fn share(natural: &[f32], room: f32) -> Vec<f32> {
    if natural.iter().sum::<f32>() <= room {
        return natural.to_vec();
    }
    let mut order: Vec<usize> = (0..natural.len()).collect();
    order.sort_by(|a, b| natural[*a].total_cmp(&natural[*b]));
    let (mut left, mut widths) = (room, vec![0.; natural.len()]);
    for (n, &c) in order.iter().enumerate() {
        widths[c] = natural[c].min(left / (natural.len() - n) as f32);
        left -= widths[c];
    }
    widths
}

/// The MIME type of a picture the PDF can hold.
fn mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xff, 0xd8, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        _ => None,
    }
}

/// An SVG holding only the raster picture `bytes`, if the PDF can hold its format.
pub fn picture(bytes: &[u8]) -> Option<String> {
    let (mime, data) = (mime(bytes)?, base64::engine::general_purpose::STANDARD.encode(bytes));
    Some(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="data:{mime};base64,{data}"/></svg>"#))
}

/// Size of a raster picture, in points (a pixel for a CSS pixel).
fn raster_size(bytes: &[u8]) -> Option<(f32, f32)> {
    let b = svg::parse(&picture(bytes)?)?.root().abs_bounding_box();
    (b.width() > 0. && b.height() > 0.).then(|| (b.width() * PX, b.height() * PX))
}

// ----- Pages.

/// The rows placed on pages: a row that does not fit starts a page, and a heading goes with what
/// follows it. Each page has its number at the bottom.
fn paginate(rows: &[Row], ink: &Ink) -> Vec<Page> {
    // (top of the row on its page, page) for each row.
    let mut placed = Vec::with_capacity(rows.len());
    let (mut page, mut y) = (0, 0.);
    for (i, row) in rows.iter().enumerate() {
        let before = if y == 0. { 0. } else { row.before };
        // A heading takes with it the first line of what follows (and the heading under it).
        let mut need = before + row.h;
        let mut j = i;
        while rows[j].keep && j + 1 < rows.len() {
            j += 1;
            need += rows[j].before + rows[j].h;
        }
        if y > 0. && y + need > HEIGHT {
            (page, y) = (page + 1, 0.);
        }
        let before = if y == 0. { 0. } else { row.before };
        placed.push((y + before, page));
        y += before + row.h;
    }
    let count = page + 1;
    let mut pages: Vec<Page> = (0..count).map(|_| Page { svg: String::new(), links: Vec::new() }).collect();
    // Frames, per page: the extent of each run of rows of the same group.
    let mut i = 0;
    while i < rows.len() {
        let Some((group, frame)) = rows[i].frame else {
            i += 1;
            continue;
        };
        let (top, page) = placed[i];
        let mut j = i;
        while j + 1 < rows.len() && rows[j + 1].frame.is_some_and(|f| f.0 == group) && placed[j + 1].1 == page {
            j += 1;
        }
        let bottom = placed[j].0 + rows[j].h;
        let (x, y, h) = (MARGIN, MARGIN + top, bottom - top);
        let svg = &mut pages[page].svg;
        match frame {
            Frame::Code => {
                let _ = write!(svg, r#"<rect x="{x:.2}" y="{y:.2}" width="{WIDTH:.2}" height="{h:.2}" rx="4" fill="{}"/>"#, ink.code_bg);
            }
            Frame::Bar => {
                let _ = write!(svg, r#"<rect x="{x:.2}" y="{y:.2}" width="2.5" height="{h:.2}" rx="1.25" fill="{}"/>"#, ink.line);
            }
        }
        i = j + 1;
    }
    for (row, (top, page)) in rows.iter().zip(&placed) {
        let p = &mut pages[*page];
        let _ = write!(p.svg, r#"<g transform="translate({MARGIN:.2} {:.2})">{}</g>"#, MARGIN + top, row.svg);
        p.links.extend(row.links.iter().map(|([x, y, w, h], url)| ([x + MARGIN, y + MARGIN + top, *w, *h], url.clone())));
    }
    for (n, p) in pages.iter_mut().enumerate() {
        let number = if count > 1 { format!(r#"<text x="{:.2}" y="{:.2}" font-family="{SANS}" font-size="8" fill="{}" text-anchor="middle">{} / {count}</text>"#, PAGE.0 / 2., PAGE.1 - MARGIN / 2., ink.soft, n + 1) } else { String::new() };
        p.svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" xml:space="preserve" width="{0:.2}" height="{1:.2}" viewBox="0 0 {0:.2} {1:.2}"><rect width="{0:.2}" height="{1:.2}" fill="#ffffff"/>{2}{number}</svg>"##,
            PAGE.0, PAGE.1, p.svg
        );
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Found {
        Found::Missing
    }

    /// The text of a page, as a PDF reader would extract it: the content of its `<text>` elements.
    fn words(page: &Page) -> String {
        let mut out = String::new();
        let mut rest = page.svg.as_str();
        while let Some(i) = rest.find("<text ") {
            let open = i + rest[i..].find('>').unwrap() + 1;
            let close = open + rest[open..].find("</text>").unwrap();
            out.push_str(&rest[open..close]);
            out.push(' ');
            rest = &rest[close..];
        }
        out
    }

    #[test]
    fn an_empty_note_is_one_blank_page() {
        let printed = print("", &none);
        assert_eq!(printed.pages.len(), 1);
        assert!(printed.missing.is_empty());
        assert!(svg::parse(&printed.pages[0].svg).is_some());
    }

    #[test]
    fn marks_are_left_out_and_links_kept() {
        let printed = print("# Titre\n\n**gras** et *ital* `code` [site](https://ex.fr) [[Note|alias]] -> fin", &none);
        let text = words(&printed.pages[0]);
        assert!(text.contains("Titre"), "{text}");
        assert!(!text.contains("**") && !text.contains("](") && !text.contains("[["), "{text}");
        assert!(text.contains("alias") && !text.contains("Note|"), "{text}");
        assert!(text.contains('→'), "{text}");
        assert_eq!(printed.pages[0].links.len(), 1);
        assert_eq!(printed.pages[0].links[0].1, "https://ex.fr");
    }

    #[test]
    fn a_long_note_goes_over_pages_numbered() {
        let text: String = (0..200).map(|i| format!("Ligne {i}\n")).collect();
        let printed = print(&text, &none);
        assert!(printed.pages.len() >= 3, "{}", printed.pages.len());
        let last = printed.pages.len();
        assert!(words(&printed.pages[last - 1]).contains(&format!("{last} / {last}")));
        // Nothing lost, nothing twice.
        let all: String = printed.pages.iter().map(words).collect();
        assert!(all.contains("Ligne 0 ") && all.contains("Ligne 199 "));
        assert_eq!(all.matches("Ligne 150 ").count(), 1);
    }

    #[test]
    fn a_heading_is_never_last_on_a_page() {
        for pad in 0..60 {
            let mut text: String = (0..pad).map(|i| format!("Ligne {i}\n")).collect();
            text.push_str("## Titre\nSuite\n");
            let printed = print(&text, &none);
            for page in &printed.pages {
                let w = words(page);
                if w.contains("Titre") {
                    assert!(w.contains("Suite"), "pad {pad}");
                }
            }
        }
    }

    #[test]
    fn long_words_and_code_wrap_inside_the_page() {
        let word = "a".repeat(400);
        let printed = print(&format!("{word}\n```\n{word}\n```\n"), &none);
        let svg = &printed.pages[0].svg;
        // Every piece of text starts within the page's text column.
        for x in svg.split("<text x=\"").skip(1).map(|s| s[..s.find('"').unwrap()].parse::<f32>().unwrap()) {
            assert!(x <= WIDTH, "{x}");
        }
        assert!(words(&printed.pages[0]).matches('a').count() >= 800);
        let lines = wrap(&Inline::plain(&word, 0, Font { serif: false, size: BODY }).glyphs, WIDTH, false);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|r| !r.is_empty()));
    }

    #[test]
    fn wrapping_breaks_on_spaces_and_cjk() {
        let font = Font { serif: false, size: BODY };
        let text = Inline::plain("mot mot mot mot", 0, font);
        let w = text.glyphs[..7].iter().map(|g| g.w).sum::<f32>() + 1.;
        let lines = wrap(&text.glyphs, w, false);
        let shown: Vec<String> = lines.iter().map(|r| text.glyphs[r.clone()].iter().map(|g| g.c).collect()).collect();
        assert_eq!(shown, ["mot mot ", "mot mot"]);
        let cjk = Inline::plain("日本語の文章日本語の文章", 0, font);
        let lines = wrap(&cjk.glyphs, BODY * 5., false);
        assert!(lines.len() >= 2 && lines.iter().all(|r| r.len() <= 5), "{lines:?}");
    }

    #[test]
    fn wide_tables_share_the_width() {
        assert_eq!(share(&[10., 20.], 100.), vec![10., 20.]);
        let w = share(&[10., 300., 300.], 210.);
        assert_eq!(w[0], 10.);
        assert!((w.iter().sum::<f32>() - 210.).abs() < 0.01 && (w[1] - w[2]).abs() < 0.01);
        let row = format!("| {} |", vec!["cellule"; 30].join(" | "));
        // 30 columns: each is narrow, its words cut, nothing leaves the text column.
        let printed = print(&format!("{row}\n|{}\n{row}\n", "---|".repeat(30)), &none);
        let svg = &printed.pages[0].svg;
        assert_eq!(words(&printed.pages[0]).matches('c').count(), 60);
        for x in svg.split("<text x=\"").skip(1).map(|s| s[..s.find('"').unwrap()].parse::<f32>().unwrap()) {
            assert!(x <= WIDTH, "{x}");
        }
    }

    #[test]
    fn missing_pictures_and_embeds_are_reported() {
        let printed = print("![](absent.png)\n![[Ailleurs#Titre]]\n", &none);
        assert_eq!(printed.missing, ["absent.png", "Ailleurs#Titre"]);
        assert!(words(&printed.pages[0]).contains("absent.png"));
        let webp = print("![](p.webp)", &|_| Found::Picture(b"RIFF0000WEBP".to_vec()));
        assert_eq!(webp.missing.len(), 1);
    }

    #[test]
    fn embeds_print_their_section_once() {
        let find = |p: &str| if p == "Autre" { Found::Section("Texte incorporé\n![[Autre]]".into()) } else { Found::Missing };
        let printed = print("Avant\n![[Autre]]\nAprès", &find);
        let text = words(&printed.pages[0]);
        assert_eq!(text.matches("Texte incorporé").count(), 1, "{text}");
        assert!(text.contains("[[Autre]]") && text.contains("Après"));
    }

    #[test]
    fn front_matter_is_not_printed() {
        let printed = print("---\ntags: [a]\n---\n# Titre\n", &none);
        let text = words(&printed.pages[0]);
        assert!(!text.contains("tags") && text.contains("Titre"), "{text}");
    }

    #[test]
    fn figures_and_pictures_become_images() {
        let printed = print("```mermaid\ngraph TD\nA-->B\n```\n$$\nx^2\n$$\n", &none);
        assert!(printed.missing.is_empty(), "{:?}", printed.missing);
        assert_eq!(printed.pages[0].svg.matches("<image ").count(), 2);
        // A 2×1 PNG.
        let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGPYEqwFRAAJcgJjO/BPNwAAAABJRU5ErkJggg==").unwrap();
        assert_eq!(raster_size(&png), Some((2. * PX, PX)));
        let printed = print("![](p.png)", &|_| Found::Picture(png.clone()));
        assert!(printed.missing.is_empty() && printed.pages[0].svg.contains("data:image/png;base64"));
    }

    #[test]
    fn emoji_become_boxes_and_are_counted() {
        let font = Font { serif: false, size: BODY };
        let shown = |s: &str| {
            let mut t = Inline::plain(s, 0, font);
            let n = t.boxed;
            t.boxed = tame(&mut t.glyphs, font);
            (t.glyphs.iter().map(|g| g.c).collect::<String>(), n)
        };
        assert_eq!(shown("a🎉b"), ("a�b".into(), 1));
        // Joined, toned, flagged, keycap, variant: one box per emoji as seen.
        assert_eq!(shown("👩‍💻 👍🏽 ❤️ #️⃣"), ("� � � #".into(), 3));
        assert_eq!(shown("é 漢 → ✓ 1+1"), ("é 漢 → ✓ 1+1".into(), 0));
        assert!(FACES[0].glyph_index(BOX).is_some(), "the box is in the app's font");
        let printed = print("# 🎉\n```\n🎉🎉\n```\n| 🎉 |\n|---|\n", &none);
        assert_eq!(printed.missing, ["4 emoji (�)"]);
    }

    #[test]
    fn highlights_keep_their_colour() {
        let printed = print("==a== <mark class=\"green\">b</mark>", &none);
        let ink = Ink::new();
        assert!(printed.pages[0].svg.contains(&ink.marks[0]) && printed.pages[0].svg.contains(&ink.marks[1]));
        assert_ne!(ink.marks[0], ink.marks[1]);
    }

    #[test]
    fn every_page_is_readable_svg() {
        let text = "# Été 日本 👩‍💻\n- [ ] tâche\n- [x] faite\n1. un\n> [!note] Attention <&>\n| a | b |\n|---|---|\n| 1 | 2 |\n---\n";
        for page in print(text, &none).pages {
            assert!(svg::parse(&page.svg).is_some(), "{}", page.svg);
        }
    }
}
