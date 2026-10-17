//! SVGs from elsewhere (draw.io, Mermaid, browsers…) made readable by resvg, then
//! rasterized at a bounded size. Pure functions, no UI dependency.
//!
//! resvg ignores the HTML of a `<foreignObject>` (the text of draw.io and Mermaid.js) and the
//! CSS function `light-dark()`: `readable` replaces the first with SVG text, wrapped
//! as a browser would, and the second with the theme color.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use resvg::{tiny_skia, usvg};
use usvg::roxmltree::{self, Node};

/// Maximum side of an image sent to the GPU: 8,192 px work everywhere.
pub const MAX_SIDE: f32 = 8192.;
/// Maximum number of pixels of an image: 16 million, i.e. 64 MB in memory.
pub const MAX_PIXELS: f32 = 16e6;

/// System fonts and the app's; a missing font (Helvetica, Arial on
/// Linux…) is replaced by the app's text font, never by whichever comes first.
pub fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
        let mut db = usvg::fontdb::Database::new();
        // The app's fonts first: for an equal name, they come before the system's, which are
        // reread from disk for each piece of text (ten times slower).
        crate::ui::assets::FONT_FILES.iter().for_each(|data| db.load_font_data(data.to_vec()));
        db.load_system_fonts();
        db.set_sans_serif_family(crate::ui::assets::sans());
        db.set_serif_family(crate::ui::assets::sans());
        db.set_monospace_family(crate::ui::assets::mono());
        Arc::new(db)
    });
    DB.clone()
}

fn options() -> usvg::Options<'static> {
    usvg::Options { fontdb: fonts(), ..Default::default() }
}

/// The SVG tree, ready to draw; `None` if it is unreadable.
pub fn parse(svg: &str) -> Option<usvg::Tree> {
    usvg::Tree::from_str(svg, &options()).ok()
}

/// The scale at which to rasterize a `w`×`h` image to show it `want` times larger
/// than life, brought under `MAX_SIDE` and `MAX_PIXELS`.
pub fn scale_for(w: f32, h: f32, want: f32) -> f32 {
    let (w, h) = (w.max(1.), h.max(1.));
    want.min(MAX_SIDE / w.max(h)).min((MAX_PIXELS / (w * h)).sqrt())
}

/// The area `x, y, w, h` (in pixels of the final image) of `tree` drawn at scale `scale`,
/// on background `bg`, as a 24-bit BMP: gpui reads it without decompressing.
pub fn bmp(tree: &usvg::Tree, scale: f32, (x, y, w, h): (u32, u32, u32, u32), bg: [u8; 3]) -> Option<Vec<u8>> {
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    pixmap.fill(tiny_skia::Color::from_rgba8(bg[0], bg[1], bg[2], 255));
    let at = tiny_skia::Transform::from_translate(-(x as f32), -(y as f32)).pre_scale(scale, scale);
    resvg::render(tree, at, &mut pixmap.as_mut());
    // Rows top to bottom (negative height), each padded to a multiple of 4 bytes.
    let row = (w as usize * 3).div_ceil(4) * 4;
    let size = 54 + row * h as usize;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for line in pixmap.data().chunks_exact(w as usize * 4) {
        // Opaque background: colors are not premultiplied.
        line.as_chunks::<4>().0.iter().for_each(|p| out.extend_from_slice(&[p[2], p[1], p[0]]));
        out.resize(out.len() + row - w as usize * 3, 0);
    }
    Some(out)
}

/// The SVG where `light-dark()` takes the theme color (`dark`), `var(--x, default)` its default,
/// and where each `<foreignObject>` becomes SVG text.
pub fn readable(svg: &str, dark: bool) -> String {
    let svg = resolve(&resolve(svg, "light-dark(", |args| args.get(dark as usize).copied()), "var(", |args| args.get(1).copied());
    if !svg.contains("<foreignObject") {
        return svg;
    }
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let Ok(doc) = roxmltree::Document::parse_with_options(&svg, options) else { return svg };
    let mut edits: Vec<(std::ops::Range<usize>, String)> = doc
        .descendants()
        .filter(|n| n.has_tag_name("foreignObject"))
        // A `foreignObject` inside another is replaced along with it.
        .filter(|n| !n.ancestors().skip(1).any(|a| a.has_tag_name("foreignObject")))
        .map(|n| (n.range(), foreign(n)))
        .collect();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut out = svg.clone();
    for (range, text) in edits {
        out.replace_range(range, &text);
    }
    out
}

/// `text` where each call `name…(a, b)` is replaced by the argument `pick` chooses (or
/// left as is if it chooses none).
fn resolve<'a>(text: &'a str, name: &str, pick: impl Fn(&[&'a str]) -> Option<&'a str> + Copy) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(name) {
        out.push_str(&rest[..at]);
        let inner = &rest[at + name.len()..];
        // End of the call and top-level commas; `rgb(…)` can be inside.
        let (mut depth, mut commas, mut end) = (0, Vec::new(), None);
        for (i, c) in inner.char_indices() {
            match c {
                '(' => depth += 1,
                ')' if depth == 0 => {
                    end = Some(i);
                    break;
                }
                ')' => depth -= 1,
                ',' if depth == 0 => commas.push(i),
                _ => {}
            }
        }
        let Some(end) = end else {
            out.push_str(&rest[at..]);
            return out;
        };
        let mut bounds = vec![0];
        commas.iter().for_each(|&c| bounds.extend([c, c + 1]));
        bounds.push(end);
        let args: Vec<&str> = bounds.chunks(2).map(|b| inner[b[0]..b[1]].trim()).collect();
        match pick(&args) {
            // The chosen argument can itself contain another.
            Some(arg) => out.push_str(&resolve(arg, name, pick)),
            None => out.push_str(&rest[at..at + name.len() + end + 1]),
        }
        rest = &inner[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Style of a piece of text.
#[derive(Clone, PartialEq, Debug)]
struct Style {
    size: f32,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: String,
    family: String,
}

/// Layout of the text block, read from its `div`s.
struct Block {
    /// Origin of the box and its width (`None`: nothing wraps).
    x: f32,
    y: f32,
    width: Option<f32>,
    height: f32,
    /// Placement of the block in the box: 0 at the start, 0.5 in the middle, 1 at the end.
    across: f32,
    down: f32,
    /// Alignment of the lines with each other, likewise.
    align: f32,
    wrap: bool,
    line_height: f32,
}

/// A piece of text and its style; `None`: line break.
type Piece = Option<(String, Style)>;

fn declarations(style: &str) -> impl Iterator<Item = (String, &str)> {
    style.split(';').filter_map(|d| {
        let (k, v) = d.split_once(':')?;
        Some((k.trim().to_ascii_lowercase(), v.trim()))
    })
}

fn css(node: Node, key: &str) -> Option<String> {
    declarations(node.attribute("style")?).filter(|(k, _)| k == key).last().map(|(_, v)| v.to_string())
}

/// A CSS length in pixels (`12px`, `12pt`, `12`); `base` for `em` and `%`.
fn length(value: &str, base: f32) -> Option<f32> {
    let value = value.trim();
    let (number, unit) = value.find(|c: char| c.is_ascii_alphabetic() || c == '%').map_or((value, ""), |i| value.split_at(i));
    let n: f32 = number.trim().parse().ok()?;
    Some(match unit {
        "pt" => n * 4. / 3.,
        "em" | "rem" => n * base,
        "%" => n * base / 100.,
        _ => n,
    })
}

/// Place on an axis of a `flex-start`, `center`, `right`… value (0, 0.5 or 1).
fn place(value: &str) -> f32 {
    let value = value.trim_start_matches("unsafe").trim();
    match value {
        "center" | "middle" => 0.5,
        "flex-end" | "end" | "right" | "bottom" => 1.,
        _ => 0.,
    }
}

/// The SVG text that replaces the `foreignObject` `node`.
fn foreign(node: Node) -> String {
    let attr = |name: &str| node.attribute(name).and_then(|v| (!v.contains('%')).then(|| length(v, 0.)).flatten());
    let mut block = Block {
        x: attr("x").unwrap_or(0.),
        y: attr("y").unwrap_or(0.),
        width: attr("width"),
        height: attr("height").unwrap_or(0.),
        across: 0.,
        down: 0.,
        align: 0.,
        wrap: true,
        line_height: 1.2,
    };
    // draw.io places its text through the margins of a flexible `div`, inside a
    // `foreignObject` the size of the drawing; Mermaid.js, through the size of the `foreignObject`.
    for div in node.descendants().filter(|n| n.is_element()) {
        for (key, value) in div.attribute("style").map(declarations).into_iter().flatten() {
            match key.as_str() {
                "padding-top" => block.y += length(value, 0.).unwrap_or(0.),
                "margin-left" => block.x += length(value, 0.).unwrap_or(0.),
                "width" if div.parent().is_some_and(|p| p == node) => block.width = length(value, 0.),
                "height" if div.parent().is_some_and(|p| p == node) => block.height = length(value, 0.).unwrap_or(block.height),
                "max-width" => block.width = length(value, 0.).or(block.width),
                "align-items" => block.down = place(value),
                "justify-content" => block.across = place(value),
                "text-align" => block.align = place(value),
                "white-space" => block.wrap = !value.starts_with("nowrap") && !value.starts_with("pre"),
                "line-height" => block.line_height = value.parse().ok().or_else(|| Some(length(value, 16.)? / 16.)).unwrap_or(1.2),
                _ => {}
            }
        }
    }
    // draw.io: the block is centered (or set, or pinned) on `padding-top`, outside any box.
    let drawio = node.descendants().any(|n| css(n, "padding-top").is_some());
    let style = Style {
        size: 16.,
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        color: "#000000".into(),
        family: "sans-serif".into(),
    };
    let mut pieces = Vec::new();
    walk(node, &style, &mut pieces);
    if pieces.iter().all(|p| p.as_ref().is_none_or(|(t, _)| t.trim().is_empty())) {
        return String::new();
    }
    let lines = wrap(&pieces, block.width.filter(|_| block.wrap));
    let lh = |line: &Line| line.iter().map(|(_, s, _)| s.size).fold(0f32, f32::max).max(1.) * block.line_height;
    let total: f32 = lines.iter().map(lh).sum();
    let widest = lines.iter().map(|l| l.iter().map(|(_, _, w)| w).sum::<f32>()).fold(0f32, f32::max);
    let room = block.width.unwrap_or(widest);
    let left = block.x + (room - widest) * block.across;
    let mut top = if drawio { block.y - total * block.down } else { block.y + (block.height - total).max(0.) * block.down };
    let mut out = String::new();
    for line in &lines {
        let height = lh(line);
        let width: f32 = line.iter().map(|(_, _, w)| w).sum();
        let size = line.iter().map(|(_, s, _)| s.size).fold(0f32, f32::max).max(1.);
        // Baseline: half the leading, then the cap height and a bit more.
        let base = top + (height - size) / 2. + size * 0.8;
        let x = left + (widest - width) * block.align;
        out.push_str(&format!(r#"<text x="{x:.2}" y="{base:.2}" xml:space="preserve">"#));
        // One `tspan` per run of words of the same style: resvg shapes each separately.
        for run in line.chunk_by(|a, b| a.1 == b.1) {
            out.push_str(&tspan(&run.iter().map(|(t, ..)| t.as_str()).collect::<String>(), &run[0].1));
        }
        out.push_str("</text>");
        top += height;
    }
    // A `switch` only draws its first child: all the lines form one.
    format!("<g>{out}</g>")
}

fn tspan(text: &str, s: &Style) -> String {
    let mut deco = Vec::new();
    s.underline.then(|| deco.push("underline"));
    s.strike.then(|| deco.push("line-through"));
    format!(
        r#"<tspan font-family="{}" font-size="{}" font-weight="{}" font-style="{}" fill="{}"{}>{}</tspan>"#,
        escape(&s.family),
        s.size,
        if s.bold { "bold" } else { "normal" },
        if s.italic { "italic" } else { "normal" },
        escape(&s.color),
        if deco.is_empty() { String::new() } else { format!(r#" text-decoration="{}""#, deco.join(" ")) },
        escape(text),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The pieces of text under `node`, in order, with their style.
fn walk(node: Node, style: &Style, out: &mut Vec<Piece>) {
    for child in node.children() {
        if child.is_text() {
            let text = child.text().unwrap_or("");
            // Whitespace collapses, as in a web page.
            let mut collapsed = String::with_capacity(text.len());
            for c in text.chars() {
                let blank = c.is_whitespace();
                if !(blank && collapsed.ends_with(' ')) {
                    collapsed.push(if blank { ' ' } else { c });
                }
            }
            if !collapsed.is_empty() {
                out.push(Some((collapsed, style.clone())));
            }
            continue;
        }
        if !child.is_element() {
            continue;
        }
        let tag = child.tag_name().name().to_ascii_lowercase();
        let mut s = style.clone();
        match tag.as_str() {
            "br" => {
                out.push(None);
                continue;
            }
            "style" | "script" | "img" | "svg" => continue,
            "b" | "strong" | "th" => s.bold = true,
            "i" | "em" => s.italic = true,
            "u" | "ins" => s.underline = true,
            "s" | "strike" | "del" => s.strike = true,
            "code" | "pre" | "tt" | "kbd" => s.family = "monospace".into(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                s.bold = true;
                s.size *= [2., 1.5, 1.17, 1., 0.83, 0.67][tag.as_bytes()[1] as usize - b'1' as usize];
            }
            "font" => {
                if let Some(color) = child.attribute("color") {
                    s.color = color.into();
                }
                if let Some(face) = child.attribute("face") {
                    s.family = face.into();
                }
                if let Some(n) = child.attribute("size").and_then(|v| v.parse::<usize>().ok()) {
                    s.size = [10., 13., 16., 18., 24., 32., 48.][n.clamp(1, 7) - 1];
                }
            }
            _ => {}
        }
        for (key, value) in child.attribute("style").map(declarations).into_iter().flatten() {
            match key.as_str() {
                "font-size" => s.size = length(value, style.size).filter(|n| *n > 0.).unwrap_or(s.size),
                "font-weight" => s.bold = value == "bold" || value == "bolder" || value.parse::<u32>().is_ok_and(|w| w >= 600),
                "font-style" => s.italic = value != "normal",
                "font-family" => s.family = value.replace('"', "'"),
                "color" => s.color = value.into(),
                "text-decoration" | "text-decoration-line" => {
                    s.underline |= value.contains("underline");
                    s.strike |= value.contains("line-through");
                }
                _ => {}
            }
        }
        // draw.io's text is in a `div` with `font-size: 0`: only the one it contains counts.
        let block = matches!(tag.as_str(), "div" | "p" | "li" | "tr" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" | "table");
        let inline_block = css(child, "display").is_some_and(|d| d.starts_with("inline"));
        let breaks = block && !inline_block;
        if breaks && out.last().is_some_and(|p| p.is_some()) {
            out.push(None);
        }
        if tag == "li" {
            out.push(Some(("• ".into(), s.clone())));
        }
        walk(child, &s, out);
        if tag == "td" || tag == "th" {
            out.push(Some(("  ".into(), s.clone())));
        }
        if breaks && out.last().is_some_and(|p| p.is_some()) {
            out.push(None);
        }
    }
}

/// A line: its pieces, their style and their width.
type Line = Vec<(String, Style, f32)>;

/// The pieces split into lines of at most `width` pixels (`None`: no limit).
fn wrap(pieces: &[Piece], width: Option<f32>) -> Vec<Line> {
    // Words and whitespace apart: only break at whitespace.
    let mut words: Vec<Option<(String, Style)>> = Vec::new();
    for piece in pieces {
        match piece {
            None => words.push(None),
            Some((text, style)) => {
                let mut word = String::new();
                for c in text.chars() {
                    if (c == ' ') != word.starts_with(' ') && !word.is_empty() {
                        words.push(Some((std::mem::take(&mut word), style.clone())));
                    }
                    word.push(c);
                }
                if !word.is_empty() {
                    words.push(Some((word, style.clone())));
                }
            }
        }
    }
    let widths = measure(words.iter().flatten());
    let mut lines: Vec<Line> = vec![Vec::new()];
    let mut i = 0;
    for word in &words {
        let Some((text, style)) = word else {
            lines.push(Vec::new());
            continue;
        };
        let w = widths[i];
        i += 1;
        let line = lines.last_mut().unwrap();
        let blank = text.starts_with(' ');
        // Whitespace at the start of a line disappears, as in a web page.
        if blank && line.is_empty() {
            continue;
        }
        let used: f32 = line.iter().map(|(_, _, w)| w).sum();
        if !blank && width.is_some_and(|room| used + w > room + 0.5) && line.iter().any(|(t, ..)| !t.starts_with(' ')) {
            // The whitespace before the word stays at the end of the line: it does not count.
            while line.last().is_some_and(|(t, ..)| t.starts_with(' ')) {
                line.pop();
            }
            lines.push(Vec::new());
        }
        lines.last_mut().unwrap().push((text.clone(), style.clone(), w));
    }
    for line in &mut lines {
        while line.last().is_some_and(|(t, ..)| t.starts_with(' ')) {
            line.pop();
        }
    }
    // Empty lines at the end: nothing to show.
    while lines.len() > 1 && lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// The width of each piece of text in its style: the sum of the widths of its
/// characters, measured by resvg itself, once and for all (at 100 px, then scaled).
// ponytail: without kerning, a few tenths of a pixel off per word; measure whole
// words if a line breaks badly. Shaping a word with resvg: 0.2 ms, too much
// for the thousands of words of a big diagram.
fn measure<'a>(words: impl Iterator<Item = &'a (String, Style)>) -> Vec<f32> {
    type Key = (char, String, bool, bool);
    static WIDTHS: LazyLock<Mutex<HashMap<Key, f32>>> = LazyLock::new(Default::default);
    let words: Vec<_> = words.collect();
    let key = |c: char, s: &Style| (c, s.family.clone(), s.bold, s.italic);
    let mut widths = WIDTHS.lock().unwrap_or_else(|e| e.into_inner());
    let mut missing: Vec<Key> = words.iter().flat_map(|(t, s)| t.chars().map(|c| key(c, s))).filter(|k| !widths.contains_key(k)).collect();
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        // Each character followed by a marker: the marker's position gives its width.
        let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
        for (n, (c, family, bold, italic)) in missing.iter().enumerate() {
            let style = Style { size: 100., bold: *bold, italic: *italic, underline: false, strike: false, color: "#000".into(), family: family.clone() };
            svg.push_str(&format!(r#"<text id="m{n}" x="0" y="100" xml:space="preserve">{}</text>"#, tspan(&format!("{c}|"), &style)));
        }
        svg.push_str("</svg>");
        let mut found = vec![None; missing.len()];
        if let Some(tree) = parse(&svg) {
            for node in tree.root().children() {
                if let usvg::Node::Text(text) = node
                    && let Some(n) = text.id().strip_prefix('m').and_then(|n| n.parse::<usize>().ok())
                    && let Some(last) = text.layouted().iter().flat_map(|s| &s.positioned_glyphs).last()
                {
                    found[n] = Some(last.transform().tx / 100.);
                }
            }
        }
        // Without a font, an estimate: half an em.
        for (k, w) in missing.into_iter().zip(found) {
            widths.insert(k, w.unwrap_or(0.55));
        }
    }
    words.iter().map(|(t, s)| t.chars().map(|c| widths[&key(c, s)]).sum::<f32>() * s.size).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_theme_color() {
        let s = r#"<rect style="fill: light-dark(rgb(1, 2, 3), #121212); stroke: light-dark(#000, var(--x, #fff))"/>"#;
        assert_eq!(readable(s, false), r#"<rect style="fill: rgb(1, 2, 3); stroke: #000"/>"#);
        assert_eq!(readable(s, true), r#"<rect style="fill: #121212; stroke: #fff"/>"#);
        // Unclosed call, no argument, nested: nothing breaks.
        assert_eq!(resolve("a light-dark(#1", "light-dark(", |a| a.first().copied()), "a light-dark(#1");
        assert_eq!(resolve("var(--x)", "var(", |a| a.get(1).copied()), "var(--x)");
        assert_eq!(resolve("light-dark(light-dark(a, b), c)", "light-dark(", |a| a.first().copied()), "a");
    }

    fn drawio(inner: &str, wrap: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200"><switch><foreignObject width="100%" height="100%" requiredFeatures="http://www.w3.org/TR/SVG11/feature#Extensibility"><div xmlns="http://www.w3.org/1999/xhtml" style="display: flex; align-items: unsafe center; justify-content: unsafe center; width: 118px; height: 1px; padding-top: 100px; margin-left: 41px;"><div style="box-sizing: border-box; font-size: 0; text-align: center;"><div style="display: inline-block; font-size: 12px; font-family: Helvetica; color: light-dark(#000000, #ffffff); line-height: 1.2; white-space: {wrap};">{inner}</div></div></div></foreignObject><text x="100" y="104">Text is not SVG - cannot display</text></switch></svg>"#
        )
    }

    /// The text lines of the SVG: (x, y, text).
    fn lines(svg: &str) -> Vec<(f32, f32, String)> {
        let doc = roxmltree::Document::parse(svg).unwrap();
        doc.descendants()
            .filter(|n| n.has_tag_name("text"))
            .map(|n| {
                let num = |a| n.attribute(a).unwrap().parse().unwrap();
                (num("x"), num("y"), n.descendants().filter(|t| t.is_text()).filter_map(|t| t.text()).collect())
            })
            .collect()
    }

    #[test]
    fn turns_html_into_text() {
        let svg = readable(&drawio("<b>Titre</b><br />Un texte assez long pour passer à la ligne dans la boîte", "normal"), true);
        assert!(!svg.contains("foreignObject") && !svg.contains("light-dark"));
        let found = lines(&svg);
        assert_eq!(found[0].2, "Titre");
        assert!(found.len() >= 3, "{found:?}");
        // Centered on `padding-top` (100): as many lines above as below.
        let (first, last) = (found[0].1, found[found.len() - 1].1);
        assert!(first < 100. && last > 100. && ((first + last) / 2. - 100.).abs() < 8., "{first} {last}");
        // Each line fits in the box's 118 px, centered on its middle (100).
        let tree = parse(&svg).unwrap();
        for node in tree.root().children() {
            if let usvg::Node::Text(t) = node {
                let b = t.bounding_box();
                assert!(b.width() <= 120. && ((b.left() + b.right()) / 2. - 100.).abs() < 3., "{:?}", b);
            }
        }
        // Bold and dark theme color, without the "Text is not SVG" fallback.
        assert!(svg.contains(r#"font-weight="bold" font-style="normal" fill="\#ffffff">Titre"#.replace('\\', "").as_str()));
        assert_eq!(found.iter().filter(|l| l.2.contains("not SVG")).count(), 1, "le repli reste, mais `switch` prend le premier");
    }

    #[test]
    fn keeps_odd_text() {
        // Nothing to write: the `foreignObject` disappears without leaving anything.
        assert!(lines(&readable(&drawio("", "normal"), false)).iter().all(|l| l.2.contains("not SVG")));
        assert!(lines(&readable(&drawio("  <br/> ", "normal"), false)).iter().all(|l| l.2.contains("not SVG")));
        // A word longer than the box overflows on its own line; nothing wraps with `nowrap`.
        let long = "x".repeat(80);
        assert_eq!(lines(&readable(&drawio(&format!("a {long} b"), "normal"), false)).len() - 1, 3);
        assert_eq!(lines(&readable(&drawio("un deux trois quatre cinq six sept huit", "nowrap"), false)).len() - 1, 1);
        // Characters to escape, CJK, emojis, accents: kept as is.
        let odd = "a &lt; b &amp; \"c\" 漢字 🎉 é | \\";
        let found = lines(&readable(&drawio(odd, "nowrap"), false));
        assert_eq!(found[0].2, "a < b & \"c\" 漢字 🎉 é | \\");
        // Lists, paragraphs, tables: one line each.
        let found = lines(&readable(&drawio("<p>un</p><p>deux</p><ul><li>trois</li></ul>", "nowrap"), false));
        assert_eq!(found.iter().map(|l| l.2.as_str()).collect::<Vec<_>>()[..3], ["un", "deux", "• trois"]);
    }

    #[test]
    fn places_mermaid_labels() {
        // Mermaid.js: the size of the `foreignObject`, in a translated group.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100"><g transform="translate(50, 20)"><foreignObject x="-40" y="-12" width="80" height="24"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; text-align: center;"><span class="nodeLabel"><p>Début</p></span></div></foreignObject></g></svg>"#;
        let out = readable(svg, false);
        let found = lines(&out);
        assert_eq!(found.len(), 1);
        assert!(found[0].1 > -12. && found[0].1 < 12., "{found:?}");
        assert!(out.contains("translate(50, 20)"));
    }

    #[test]
    fn rasterizes_within_bounds() {
        assert_eq!(scale_for(100., 100., 2.), 2.);
        // 6,000 px high at scale 2 would exceed 8,192 px.
        assert!(scale_for(1262., 6253., 2.) * 6253. <= MAX_SIDE);
        assert!(scale_for(20000., 20000., 2.).powi(2) * 4e8 <= MAX_PIXELS * 1.001);
        assert!(scale_for(0., 0., 2.) > 0.);
        let tree = parse(r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="2"><rect x="2" width="2" height="2" fill="#ff0000"/></svg>"##).unwrap();
        let bmp = bmp(&tree, 1., (0, 0, 4, 2), [0, 0, 255]).unwrap();
        // 54-byte header, 12-byte rows: blue background on the left, red on the right (in BGR).
        assert_eq!(bmp.len(), 54 + 2 * 12);
        assert_eq!(&bmp[54..60], &[255, 0, 0, 255, 0, 0]);
        assert_eq!(&bmp[60..66], &[0, 0, 255, 0, 0, 255]);
        assert!(super::bmp(&tree, 1., (0, 0, 0, 0), [0; 3]).is_none());
    }
}

