//! Line-by-line Markdown analysis: pure functions, no dependency on the UI.


mod dates;
mod front;
mod lists;
mod search;
mod tables;

pub use dates::*;
pub use front::*;
pub use lists::*;
pub use search::*;
pub use tables::*;

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Para,
    Heading(u8),
    Bullet,
    Ordered,
    Task(bool),
    Quote,
    Fence,
    Code,
    Rule,
    Table,
        /// Line of the front matter block: neither heading nor rule, whatever it contains.
    Meta,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Link {
    Wiki(String),
    Url(String),
    Tag(String),
}

// Inline styles, one word of flags per byte of text.
pub const BOLD: u16 = 1;
pub const ITALIC: u16 = 2;
pub const CODE: u16 = 4;
pub const STRIKE: u16 = 8;
pub const DIM: u16 = 16;
pub const LINK: u16 = 32;
pub const TAG: u16 = 64;
pub const MARK: u16 = 128;
// Highlight `==text==` and underline `<u>text</u>` (Markdown has no underline: the HTML tag serves).
pub const HIGHLIGHT: u16 = 2048;
pub const UNDER: u16 = 4096;
// Text that steps back but is still read (front matter, the address of a link, comments): `DIM` for the
// rest of the code, drawn readable.
pub const QUIET: u16 = 8192;
// Code block coloring.
pub const KEYWORD: u16 = 256;
pub const STRING: u16 = 512;
pub const NUMBER: u16 = 1024;

pub const INDENT: &str = "    ";

fn indent_len(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

pub fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

/// The line is a table line: `classify(line, false)` calls it `Kind::Table`, without the rest of the work.
pub fn is_table_line(line: &str) -> bool {
    line[indent_len(line)..].starts_with('|')
}

/// Number of ``` fence lines and of lines reduced to `$$` in the text, without analysing it line by line:
/// we look for the patterns, which are rare, then look at the line where each one falls.
pub fn count_fences_and_dollars(text: &str) -> (usize, usize) {
    let line_start = |at: usize| text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| text[at..].find('\n').map_or(text.len(), |i| at + i);
    let (mut fences, mut last) = (0, usize::MAX);
    for (at, _) in text.match_indices("```") {
        let start = line_start(at);
        if start != last && text[start..at].chars().all(char::is_whitespace) {
            fences += 1;
            last = start;
        }
    }
    let (mut dollars, mut last) = (0, usize::MAX);
    for (at, _) in text.match_indices("$$") {
        let start = line_start(at);
        if start != last && text[start..line_end(at)].trim() == "$$" {
            dollars += 1;
            last = start;
        }
    }
    (fences, dollars)
}

/// Kind of the line and length (in bytes) of its marker, indentation included.
pub fn classify(line: &str, in_code: bool) -> (Kind, usize) {
    let indent = indent_len(line);
    let rest = &line[indent..];
    if rest.starts_with("```") {
        return (Kind::Fence, line.len());
    }
    if in_code {
        return (Kind::Code, 0);
    }
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        return (Kind::Heading(hashes as u8), indent + hashes + 1);
    }
    let t = rest.trim_end();
    if t.len() >= 3 && ['-', '*', '_'].iter().any(|&c| t.chars().all(|x| x == c)) {
        return (Kind::Rule, line.len());
    }
    if rest.starts_with('|') {
        return (Kind::Table, 0);
    }
    if rest == ">" || rest.starts_with("> ") {
        return (Kind::Quote, indent + rest.len().min(2));
    }
    let b = rest.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' {
        let after = &rest[2..];
        if after.starts_with("[ ] ") {
            return (Kind::Task(false), indent + 6);
        }
        if after.starts_with("[x] ") || after.starts_with("[X] ") {
            return (Kind::Task(true), indent + 6);
        }
        return (Kind::Bullet, indent + 2);
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && (rest[digits..].starts_with(". ") || rest[digits..].starts_with(") "))
    {
        return (Kind::Ordered, indent + digits + 2);
    }
    (Kind::Para, 0)
}

/// An address that can be opened: a known protocol, then something, without a space.
pub fn is_url(text: &str) -> bool {
    let known = ["http://", "https://", "mailto:", "file://"].iter().any(|p| text.len() > p.len() && text.starts_with(p));
    known && !text.contains(char::is_whitespace)
}

/// If `rest` starts with a `[text](address)` link: the range of the text and that of the address.
// ponytail: only addresses with a protocol, without `)`; a link to a vault file
// is written `[[note]]`. Count parentheses if addresses containing them show up.
pub fn web_link(rest: &str) -> Option<(Range<usize>, Range<usize>)> {
    let close = rest.find("](")?;
    let end = close + 2 + rest[close + 2..].find(')')?;
    let (text, url) = (1..close, close + 2..end);
    let ok = rest.starts_with('[') && !rest[text.clone()].contains(['[', ']']) && is_url(&rest[url.clone()]);
    ok.then_some((text, url))
}

/// Wikilinks, `[text](address)` links, URLs and tags of the line, with their byte range.
pub fn links(line: &str) -> Vec<(Range<usize>, Link)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let rest = &line[i..];
        if rest.starts_with("[[")
            && let Some(e) = rest.find("]]")
        {
            let target = rest[2..e].split(['|', '#']).next().unwrap_or("").trim();
            if !target.is_empty() {
                out.push((i..i + e + 2, Link::Wiki(target.to_string())));
            }
            i += e + 2;
            continue;
        }
                // `![…](…)` is an image, not a link.
        if b[i] == b'[' && (i == 0 || b[i - 1] != b'!')
            && let Some((_, url)) = web_link(rest)
        {
            out.push((i..i + url.end + 1, Link::Url(rest[url.clone()].to_string())));
            i += url.end + 1;
            continue;
        }
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ')')
                .unwrap_or(rest.len());
            out.push((i..i + end, Link::Url(rest[..end].to_string())));
            i += end;
            continue;
        }
        if b[i] == b'#' && (i == 0 || b[i - 1].is_ascii_whitespace()) {
            let n: usize = rest[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .map(char::len_utf8)
                .sum();
            let tag = &rest[1..1 + n];
            if tag.chars().any(|c| !c.is_ascii_digit()) {
                out.push((i..i + 1 + n, Link::Tag(tag.to_lowercase())));
                i += 1 + n;
                continue;
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// The `[ ]` and `[x]` of the line from byte `from`, alone between spaces, `|` or
/// the line's edges: not `[[ ]]`, not `[x](link)`, not `a[ ]`.
fn boxes(line: &str, from: usize) -> impl Iterator<Item = Range<usize>> + '_ {
    let b = line.as_bytes();
    (from..b.len().saturating_sub(2)).filter_map(move |i| {
        let token = b[i] == b'[' && matches!(b[i + 1], b' ' | b'x' | b'X') && b[i + 2] == b']';
        let open = i == 0 || matches!(b[i - 1], b' ' | b'\t' | b'|');
        let shut = i + 3 == b.len() || matches!(b[i + 3], b' ' | b'\t' | b'|');
        (token && open && shut).then_some(i..i + 3)
    })
}

/// The line's checkboxes, outside inline code.
pub fn checkboxes(line: &str, from: usize) -> Vec<Range<usize>> {
    let mut flags = vec![0u16; line.len()];
    inline(line, from, &mut flags);
    boxes(line, from).filter(|r| flags[r.start] & CODE == 0).collect()
}

/// Highlight colors: the name (the class of `<mark class="name">`), the first one is that of `==`.
pub const MARKS: [&str; 5] = ["yellow", "green", "blue", "pink", "purple"];

/// `<mark>` or `<mark class="name">` at the start of `rest` (name of a known color), or `</mark>` when
/// `closing`: its length.
fn mark_tag(rest: &str, closing: bool) -> Option<usize> {
    if closing {
        return rest.starts_with("</mark>").then_some(7);
    }
    let end = rest.strip_prefix("<mark")?.find('>')? + 6;
    let tag = &rest[..end];
    (tag == "<mark>" || MARKS.iter().any(|name| tag == format!("<mark class=\"{name}\">"))).then_some(end)
}

/// The number (in `MARKS`) of an opening `<mark>`'s color, 0 for `<mark>` and for `==`.
pub fn mark_color(open: &str) -> usize {
    MARKS.iter().position(|name| open == format!("<mark class=\"{name}\">")).unwrap_or(0)
}

/// How a highlight of color `color` is written: `(opening, closing)`.
pub fn mark_signs(color: usize) -> (String, &'static str) {
    match color {
        0 => ("==".into(), "=="),
        _ => (format!("<mark class=\"{}\">", MARKS[color.min(MARKS.len() - 1)]), "</mark>"),
    }
}

/// Fills `flags` with the inline styles of `line` from byte `from`: all of them.
pub fn inline(line: &str, from: usize, flags: &mut [u16]) {
    emphasis(line, from, flags);
    let b = line.as_bytes();
    // A `[ ]` or `[x]` checkbox: colored, bold once checked.
    for r in boxes(line, from) {
        if flags[r.start] & CODE == 0 {
            let checked = if b[r.start + 1] == b' ' { 0 } else { BOLD };
            flags[r.clone()].iter_mut().for_each(|f| *f |= MARK | checked);
        }
    }
    links_and_comments(line, from, flags);
}

/// Fills `flags` with the styles made by marks alone (`**`, `*`, `_`, `~~`, `==`, `<u>`, `<mark>`, backticks):
/// the marks are `DIM`. Texts that are not notes (a diagram's) read only this part.
pub fn emphasis(line: &str, from: usize, flags: &mut [u16]) {
    let b = line.as_bytes();
    let alnum = |i: Option<usize>| i.and_then(|i| b.get(i)).is_some_and(u8::is_ascii_alphanumeric);
    let mut active = 0u16;
        // The current highlight opened with `<mark>` (otherwise with `==`): it closes the same way.
    let mut tagged = false;
    let mut i = from;
    while i < b.len() {
        let rest = &line[i..];
        if b[i] == b'`'
            && let Some(end) = rest[1..].find('`')
        {
            flags[i] |= DIM;
            flags[i + 1 + end] |= DIM;
            flags[i + 1..i + 1 + end].iter_mut().for_each(|f| *f |= CODE);
            i += end + 2;
            continue;
        }
        let (n, flag) = if rest.starts_with("**") {
            (2, BOLD)
        } else if rest.starts_with("~~") {
            (2, STRIKE)
        } else if let Some(n) = if active & HIGHLIGHT != 0 { mark_tag(rest, true).filter(|_| tagged) } else { mark_tag(rest, false) } {
            (n, HIGHLIGHT)
        } else if rest.starts_with("==") && !(tagged && active & HIGHLIGHT != 0) {
            (2, HIGHLIGHT)
        } else if rest.starts_with("<u>") && active & UNDER == 0 {
            (3, UNDER)
        } else if rest.starts_with("</u>") && active & UNDER != 0 {
            (4, UNDER)
        } else if b[i] == b'*' || b[i] == b'_' {
            (1, ITALIC)
        } else {
            (0, 0)
        };
        if flag != 0 {
            let underscore = b[i] == b'_';
                        // `==`: not stuck to a word (`abc==` of an address), nor the `{==` `==}` of a comment.
            let sound = |at: usize| b.get(at.wrapping_sub(1)) != Some(&b'{') && b.get(at + 2) != Some(&b'}');
            let ok = if flag == UNDER {
                n == 4 || rest[n..].contains("</u>")
            } else if flag == HIGHLIGHT && rest.starts_with('<') {
                                // `<mark>`: an opening one that has its closing one, or the closing one of the one that opened.
                active & flag != 0 || rest[n..].contains("</mark>")
            } else if flag == HIGHLIGHT {
                if active & flag == 0 {
                    !alnum(i.checked_sub(1))
                        && sound(i)
                        && rest[2..].chars().next().is_some_and(|c| !c.is_whitespace() && c != '=')
                        && rest[2..].match_indices("==").any(|(j, _)| sound(i + 2 + j) && !alnum(Some(i + 4 + j)))
                } else {
                    sound(i) && !alnum(Some(i + 2))
                }
            } else if active & flag == 0 {
                rest[n..].chars().next().is_some_and(|c| !c.is_whitespace())
                    && rest[n..].contains(&rest[..n])
                    && !(underscore && alnum(i.checked_sub(1)))
            } else {
                !(underscore && alnum(Some(i + 1)))
            };
            if ok {
                flags[i..i + n].iter_mut().for_each(|f| *f |= DIM | active);
                if flag == HIGHLIGHT && active & flag == 0 {
                    tagged = rest.starts_with('<');
                }
                active ^= flag;
                i += n;
                continue;
            }
        }
        let n = rest.chars().next().map_or(1, char::len_utf8);
        flags[i..i + n].iter_mut().for_each(|f| *f |= active);
        i += n;
    }
}

/// Links, tags and comments: what notes add to the styles of the marks.
fn links_and_comments(line: &str, from: usize, flags: &mut [u16]) {
    for (r, link) in links(line) {
        if r.start < from {
            continue;
        }
        match link {
            Link::Tag(_) => flags[r].iter_mut().for_each(|f| *f |= TAG),
                        // `[text](address)`: the text is the link, the rest fades.
            Link::Url(_) => match web_link(&line[r.start..]) {
                Some((text, address)) => {
                    flags[r.clone()].iter_mut().for_each(|f| *f |= DIM);
                    flags[r.start + text.start..r.start + text.end].iter_mut().for_each(|f| *f = *f & !DIM | LINK);
                    flags[r.start + address.start..r.start + address.end].iter_mut().for_each(|f| *f |= QUIET);
                }
                None => flags[r].iter_mut().for_each(|f| *f |= LINK),
            },
            Link::Wiki(_) => {
                flags[r.start..r.start + 2].iter_mut().for_each(|f| *f |= DIM);
                flags[r.end - 2..r.end].iter_mut().for_each(|f| *f |= DIM);
                flags[r.start + 2..r.end - 2].iter_mut().for_each(|f| *f |= LINK);
            }
        }
    }
        // A comment: its text keeps its styles (the editor highlights it), the rest fades.
    for (whole, noted, said) in comments(line) {
        if whole.start < from || flags[whole.start] & CODE != 0 {
            continue;
        }
        flags[whole.start..noted.start].fill(DIM);
        flags[noted.end..whole.end].fill(DIM);
        flags[said].fill(DIM | ITALIC | QUIET);
    }
}

/// The highlighted spans of `line` (`==text==` or `<mark>`), without their marks, with their color (see
/// `MARKS`), for the editor to paint: the whole line after its bullet, or each cell for a table line.
pub fn highlights(line: &str, table: bool) -> Vec<(Range<usize>, usize)> {
    let mut found = Vec::new();
    if !line.contains("==") && !line.contains("<mark") {
        return found;
    }
    let pieces: Vec<Range<usize>> = if table { cells(line) } else { std::iter::once(classify(line, false).1..line.len()).collect() };
    for piece in pieces {
        let f = flags(&line[piece.clone()], 0);
        let mut i = 0;
        while i < f.len() {
            if f[i] & HIGHLIGHT == 0 {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < f.len() && f[j] & HIGHLIGHT != 0 {
                j += 1;
            }
                        // The closing mark, and the marks of another style before it, do not carry the background.
            let mut end = j;
            while end > i && f[end - 1] & DIM != 0 {
                end -= 1;
            }
            if end > i {
                let before = &line[piece.start..piece.start + i];
                let color = before.rfind("<mark").filter(|_| before.ends_with('>')).map_or(0, |m| mark_color(&before[m..]));
                found.push((piece.start + i..piece.start + end, color));
            }
            i = j;
        }
    }
    found
}

/// The styles the formatting bar applies, in the order of its buttons.
pub const STYLES: [u16; 6] = [BOLD, ITALIC, UNDER, STRIKE, HIGHLIGHT, CODE];

/// The ways a style is written: `(opening, closing)`, the first is the one applied.
pub fn delimiters(flag: u16) -> &'static [(&'static str, &'static str)] {
    match flag {
        BOLD => &[("**", "**")],
        ITALIC => &[("*", "*"), ("_", "_")],
        STRIKE => &[("~~", "~~")],
        HIGHLIGHT => &[("==", "==")],
        UNDER => &[("<u>", "</u>")],
        CODE => &[("`", "`")],
        _ => &[],
    }
}

/// The inline styles of `line` (see `inline`), one word per byte.
pub fn flags(line: &str, from: usize) -> Vec<u16> {
    let mut flags = vec![0u16; line.len()];
    inline(line, from.min(line.len()), &mut flags);
    flags
}

/// The span of style `flag` that holds `sel` (bytes of `line`, `flags` taken from `flags`): the
/// ranges of its opening and closing marks, enough to remove it. `None` if `sel` is not
/// entirely inside such a span. A lone cursor counts when it touches the span, or
/// when it is between the opening and closing marks.
// ponytail: removes the whole span, without splitting it around a smaller selection;
// a selection straddling two spans of the same style removes neither (the style is applied again).
pub fn unmark(line: &str, flags: &[u16], sel: Range<usize>, flag: u16) -> Option<(Range<usize>, Range<usize>)> {
    let has = |i: usize| flags.get(i).is_some_and(|f| f & flag != 0);
    let pivot = if sel.is_empty() {
        let at = sel.start;
        let touching = [Some(at), at.checked_sub(1)].into_iter().flatten().find(|&i| has(i) && flags[i] & DIM == 0);
        match touching {
            Some(i) => i,
            None => {
                                // `**|**`: an empty span, the cursor between its marks.
                return delimiters(flag).iter().find_map(|(open, close)| {
                    (line[..at].ends_with(open) && line[at..].starts_with(close)).then(|| (at - open.len()..at, at..at + close.len()))
                });
            }
        }
    } else {
        let mut text = sel.clone().filter(|&i| flags[i] & DIM == 0).peekable();
        text.peek()?;
        if !text.all(has) {
            return None;
        }
        sel.clone().find(|&i| flags[i] & DIM == 0)?
    };
    let (mut s, mut e) = (pivot, pivot + 1);
    while s > 0 && has(s - 1) {
        s -= 1;
    }
    while e < line.len() && has(e) {
        e += 1;
    }
    let mut signs: Vec<(String, &str)> = delimiters(flag).iter().map(|(open, close)| (open.to_string(), *close)).collect();
        // A color highlight is written `<mark class="…">…</mark>`.
    if flag == HIGHLIGHT
        && let Some(m) = line[..s].rfind("<mark")
        && mark_tag(&line[m..], false) == Some(s - m)
    {
        signs.push((line[m..s].to_string(), "</mark>"));
    }
    signs.iter().find_map(|(open, close)| {
                // The closing mark carries the style of the bytes it closes: it is in the span, or just after.
        let inside = e >= close.len() && line[..e].ends_with(close) && flags[e - close.len()..e].iter().all(|f| f & DIM != 0);
        [inside.then(|| e - close.len()), Some(e)].into_iter().flatten().find_map(|end| {
            (line[..s].ends_with(open.as_str()) && line[end..].starts_with(close)).then(|| (s - open.len()..s, end..end + close.len()))
        })
    })
}

const KEYWORDS: &[&str] = &[
    "False", "None", "Self", "True", "and", "as", "async", "await", "break", "case", "catch",
    "class", "const", "continue", "def", "default", "defer", "do", "done", "elif", "else", "end",
    "enum", "except", "export", "extends", "false", "fi", "final", "finally", "fn", "for", "from",
    "func", "function", "go", "if", "impl", "import", "in", "interface", "is", "lambda", "let",
    "loop", "match", "mod", "mut", "namespace", "new", "nil", "not", "null", "or", "package",
    "private", "protected", "pub", "public", "return", "self", "static", "struct", "super",
    "switch", "then", "this", "throw", "trait", "true", "try", "type", "use", "using", "var",
    "void", "where", "while", "with", "yield",
];

/// Colors a line of code: comments, strings, numbers and keywords.
// ponytail: a single line-by-line lexer for all languages, with no state from one
// line to the next (`/* */` comments and multi-line strings ignored).
// Move to a grammar per language (tree-sitter) if coloring becomes wrong.
pub fn code(line: &str, lang: &str, flags: &mut [u16]) {
    let comment = match lang {
        "py" | "python" | "sh" | "bash" | "zsh" | "fish" | "shell" | "yaml" | "yml" | "toml"
        | "rb" | "ruby" | "r" | "perl" | "make" | "makefile" | "dockerfile" | "conf" | "ini"
        | "nix" | "elixir" => "#",
        "sql" | "lua" | "haskell" | "hs" | "elm" | "ada" => "--",
        _ => "//",
    };
    let rust = matches!(lang, "rs" | "rust");
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        if rest.starts_with(comment) {
            flags[i..].fill(DIM | ITALIC | QUIET);
            return;
        }
        let c = rest.chars().next().unwrap();
        let (n, flag) = if matches!(c, '"' | '\'' | '`') {
            let mut end = None;
            let mut escaped = false;
            for (j, x) in rest.char_indices().skip(1) {
                if !escaped && x == c {
                    end = Some(j + 1);
                    break;
                }
                escaped = !escaped && x == '\\';
            }
            match end {
                                // In Rust, `'a` is a lifetime: only `'x'` or `'\n'` is a character.
                Some(n) if rust && c == '\'' && rest[..n].chars().count() > 3 && !rest[1..].starts_with('\\') => {
                    (1, 0)
                }
                Some(n) => (n, STRING),
                None if c == '"' => (rest.len(), STRING),
                None => (1, 0),
            }
        } else if c.is_alphabetic() || c == '_' {
            let n = rest.find(|x: char| !x.is_alphanumeric() && x != '_').unwrap_or(rest.len());
            (n, if KEYWORDS.contains(&&rest[..n]) { KEYWORD } else { 0 })
        } else if c.is_ascii_digit() {
            let n = rest
                .find(|x: char| !x.is_ascii_alphanumeric() && x != '.' && x != '_')
                .unwrap_or(rest.len());
            (n, NUMBER)
        } else {
            (c.len_utf8(), 0)
        };
        flags[i..i + n].fill(flag);
        i += n;
    }
}

/// Content (fences excluded) of the code block that contains byte `at`. A
/// ``` without a closing fence opens no block.
pub fn code_block(text: &str, at: usize) -> Option<Range<usize>> {
    let mut open = None;
    let mut offset = 0;
    for line in text.split('\n') {
        if is_fence(line) {
            match open.take() {
                Some(start) if (start..offset).contains(&at) => return Some(start..offset - 1),
                Some(_) => {}
                None => open = Some(offset + line.len() + 1),
            }
        }
        offset += line.len() + 1;
    }
    None
}

const SYMBOLS: &[(&str, &str)] = &[
    ("<->", "↔"),
    ("<=>", "⇔"),
    ("->", "→"),
    ("<-", "←"),
    ("=>", "⇒"),
    ("!=", "≠"),
    ("<=", "≤"),
    (">=", "≥"),
];

/// Symbols to display in place of their ASCII spelling (`->` becomes `→`), outside
/// code and links: (start byte, replaced length, symbol). The text does not change.
pub fn symbols(line: &str, from: usize, flags: &[u16]) -> Vec<(usize, usize, &'static str)> {
    let b = line.as_bytes();
        // A symbol stuck to another operator (`-->`, `!==`, `<--`) stays as is.
    let glued = |i: Option<usize>| i.and_then(|i| b.get(i)).is_some_and(|c| b"-=<>!".contains(c));
    let mut out = Vec::new();
    let mut i = from;
    while i < b.len() {
        let found = SYMBOLS.iter().find(|(ascii, _)| {
            b[i..].starts_with(ascii.as_bytes())
                && flags[i..i + ascii.len()].iter().all(|f| f & (CODE | LINK) == 0)
                && !glued(i.checked_sub(1))
                && !glued(Some(i + ascii.len()))
        });
        match found {
            Some((ascii, sign)) => {
                out.push((i, ascii.len(), *sign));
                i += ascii.len();
            }
            None => i += 1,
        }
    }
    out
}

/// First image of the line, `![alt](path)` or `![[path]]`: its range
/// and its path.
pub fn image(line: &str) -> Option<(Range<usize>, String)> {
    let start = line.find("![")?;
    let rest = &line[start + 2..];
    let (len, path) = if let Some(inner) = rest.strip_prefix('[') {
        let end = inner.find("]]")?;
        (end + 5, inner[..end].split('|').next()?)
    } else {
        let open = rest.find("](")?;
        let end = open + rest[open..].find(')')?;
        (end + 3, rest[open + 2..end].split(" \"").next()?)
    };
    let path = path.trim().replace("%20", " ");
    (!path.is_empty()).then(|| (start..start + len, path))
}

/// First formula of the line, `$$…$$` or `$…$`: its range, its source, and
/// whether it is display math (`$$`).
pub fn math(line: &str) -> Option<(Range<usize>, &str, bool)> {
    let mut from = 0;
    while let Some(i) = line[from..].find('$') {
        let start = from + i;
        let n = if line[start..].starts_with("$$") { 2 } else { 1 };
        let body = &line[start + n..];
        if let Some(len) = body.find(&"$$"[..n])
            && len > 0
                        // Like pandoc: a `$` stuck to the formula, and not followed by a digit,
                        // to leave "5 $ et 10 $" or "$5 et $10" alone.
            && (n == 2
                || !(body.starts_with(' ')
                    || body[..len].ends_with(' ')
                    || body[len + 1..].starts_with(|c: char| c.is_ascii_digit())))
        {
            return Some((start..start + len + 2 * n, body[..len].trim(), n == 2));
        }
        from = start + n;
    }
    None
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Tags and wikilinks (lowercase) of a whole note, outside code blocks, deduplicated.
/// Displayed images appear in it by their file name.
pub fn index(text: &str) -> (Vec<String>, Vec<String>) {
        // Front matter tags (`tags: [a, b]`) count as the text's, with or without `#`.
    let mut tags: Vec<String> = Vec::new();
    for tag in front_values(text, "tags") {
        let tag = tag.trim_start_matches('#').to_string();
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    let mut wikis = Vec::new();
    let mut in_code = false;
    for line in text[front_matter(text)..].lines() {
        if is_fence(line) {
            in_code = !in_code;
        } else if !in_code {
            for (_, link) in links(line) {
                let (list, item) = match link {
                    Link::Tag(t) => (&mut tags, t),
                    Link::Wiki(w) => (&mut wikis, w.to_lowercase()),
                    Link::Url(_) => continue,
                };
                if !list.contains(&item) {
                    list.push(item);
                }
            }
                        // A displayed image counts as a link to its file.
            if let Some((_, path)) = image(line) {
                let name = file_name(&path).to_lowercase();
                if !wikis.contains(&name) {
                    wikis.push(name);
                }
            }
        }
    }
    (tags, wikis)
}

/// The text where wikilinks to `old` target `new` (alias and anchor kept,
/// code blocks and front matter left as is); `None` if no link changes.
pub fn relink(text: &str, old: &str, new: &str) -> Option<String> {
    let old = old.to_lowercase();
    let meta = front_matter(text);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..meta]);
    let mut in_code = false;
    for line in text[meta..].split_inclusive('\n') {
        let mut done = 0;
        if is_fence(line) {
            in_code = !in_code;
        } else if !in_code {
            for (range, link) in links(line) {
                if matches!(link, Link::Wiki(target) if target.to_lowercase() == old) {
                                        // `[[target|alias]]` or `[[target#anchor]]`: only the target changes.
                    let inner = &line[range.start + 2..range.end - 2];
                    let rest = inner.find(['|', '#']).map_or("", |i| &inner[i..]);
                    out.push_str(&line[done..range.start + 2]);
                    out.push_str(new);
                    out.push_str(rest);
                    done = range.end - 2;
                }
            }
        }
        out.push_str(&line[done..]);
    }
    let out = format!("{}{}", &out[..meta], reembed(&out[meta..], &old, new));
    (out != text).then_some(out)
}

/// The text where `![…](folder/old)` images name the file `new`.
fn reembed(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    for line in text.split_inclusive('\n') {
        if is_fence(line) {
            in_code = !in_code;
        } else if !in_code
            && let Some((range, path)) = image(line)
            && !line[range.clone()].starts_with("![[")
            && file_name(&path).to_lowercase() == old
        {
                        // The name as written: with its spaces, or as `%20`.
            let name = file_name(&path);
            let spot = [name.to_string(), name.replace(' ', "%20")]
                .into_iter()
                .find_map(|written| Some((line[range.clone()].rfind(&written)?, written.len())));
            if let Some((at, len)) = spot {
                out.push_str(&line[..range.start + at]);
                out.push_str(&new.replace(' ', "%20"));
                out.push_str(&line[range.start + at + len..]);
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

// ----- Callouts and tables -----

/// Kind of callout opened by a `> [!NOTE]` quote (GitHub syntax).
pub fn callout(line: &str) -> Option<Range<usize>> {
    let open = line.find('>')? + 1;
    let open = open + indent_len(&line[open..]);
    let close = line[open..].strip_prefix("[!")?.find(']')?;
    Some(open..open + close + 3)
}

// ----- Gestures on a table: pure functions on the `table` grid (the header, the dash line, the
// body). A "visible" rank does not count the dash line: 0 is the header, 1 the first body row.
// Each gesture returns the changed grid, always rectangular and with its dashes; `None` when nothing is left.

/// Headings of the text: level (1 to 6), label, start of their line. A code block line
/// is not a heading, nor is a front matter line; as on screen, a ``` without a
/// closing fence opens no block.
pub fn headings(text: &str) -> Vec<(u8, &str, usize)> {
    let meta = front_matter(text);
    let fences = text[meta..].lines().filter(|line| is_fence(line)).count();
    let (mut seen, mut in_code, mut at, mut found) = (0, false, meta, Vec::new());
    for line in text[meta..].split('\n') {
        if is_fence(line) {
            seen += 1;
            in_code = !in_code && seen < fences;
        } else if !in_code
            && let (Kind::Heading(_), marker) = classify(line, false)
        {
            let level = line.trim_start().bytes().take_while(|&b| b == b'#').count() as u8;
            found.push((level, line[marker..].trim(), at));
        }
        at += line.len() + 1;
    }
    found
}

/// Comments of the line, written in CriticMarkup `{==text==}{>>comment<<}`: the range
/// of the whole, that of the commented text and that of the comment. A comment on its own,
/// `{>>comment<<}`, has an empty commented text, at its start.
pub fn comments(line: &str) -> Vec<(Range<usize>, Range<usize>, Range<usize>)> {
    let (mut found, mut from) = (Vec::new(), 0);
    while let Some(open) = line[from..].find("{==").map(|i| i + from) {
        let Some(mid) = line[open + 3..].find("==}{>>").map(|i| i + open + 3) else { break };
        let Some(end) = line[mid + 6..].find("<<}").map(|i| i + mid + 6) else { break };
        found.push((open..end + 3, open + 3..mid, mid + 6..end));
        from = end + 3;
    }
    let full = found.len();
    from = 0;
    while let Some(open) = line[from..].find("{>>").map(|i| i + from) {
        let Some(end) = line[open + 3..].find("<<}").map(|i| i + open + 3) else { break };
        if !found[..full].iter().any(|(whole, ..)| whole.contains(&open)) {
            found.push((open..end + 3, open..open, open + 3..end));
        }
        from = end + 3;
    }
    found.sort_by_key(|(whole, ..)| whole.start);
    found
}

/// A comment of the note, as its panel lists it.
#[derive(Debug, PartialEq)]
pub struct Note<'a> {
    /// The commented text; empty for a comment on its own.
    pub noted: &'a str,
    /// What is said, without the name of the row or column it starts with, if any.
    pub said: &'a str,
    /// The byte where the whole starts, and the one where the comment starts.
    pub at: usize,
    pub inside: usize,
    /// The byte where the whole ends.
    pub end: usize,
    /// What it is on, in a table: `Cell C2`, `Header B`, `Row Léa`, `Column Âge`, `Table`; empty elsewhere.
    pub scope: String,
}

/// Name of column `c` as a spreadsheet writes it: `A`, `B`… `Z`, `AA`.
pub fn column_letters(c: usize) -> String {
    let (mut n, mut name) = (c + 1, String::new());
    while n > 0 {
        name.insert(0, (b'A' + ((n - 1) % 26) as u8) as char);
        n = (n - 1) / 26;
    }
    name
}

/// A comment line under a table starts with what it is about (see `scopes`): its name as the
/// panel says it, and what follows.
pub fn split_scope(said: &str) -> Option<(String, &str)> {
    if let Some(rest) = said.strip_prefix("Table: ") {
        return Some(("Table".into(), rest));
    }
    let (kind, tail) = said.strip_prefix("Row ").map(|t| ("Row", t)).or_else(|| said.strip_prefix("Column ").map(|t| ("Column", t)))?;
    let digits = |t: &str| t.bytes().take_while(u8::is_ascii_digit).count();
    let Some(quoted) = tail.strip_prefix('"') else {
        let n = digits(tail);
        return tail[n..].strip_prefix(": ").filter(|_| n > 0).map(|rest| (format!("{kind} {}", &tail[..n]), rest));
    };
    let plain = quoted.find("\": ").map(|i| (i, quoted[i + 3..].len(), String::new()));
    let numbered = quoted.match_indices("\" (").find_map(|(i, _)| {
        let n = digits(&quoted[i + 3..]);
        let rest = quoted[i + 3 + n..].strip_prefix("): ")?;
        (n > 0).then(|| (i, rest.len(), format!(" ({})", &quoted[i + 3..i + 3 + n])))
    });
    let (i, left, number) = [plain, numbered].into_iter().flatten().min_by_key(|(i, ..)| *i)?;
    Some((format!("{kind} {}{number}", &quoted[..i]), &quoted[quoted.len() - left..]))
}

/// All the note's comments, outside code blocks.
pub fn all_comments(text: &str) -> Vec<Note<'_>> {
        // Almost no note has any: a single search says so, without reading the lines.
    if !text.contains("{==") && !text.contains("{>>") {
        return Vec::new();
    }
    let fences = text.lines().filter(|line| is_fence(line)).count();
    let (mut seen, mut in_code, mut at, mut found) = (0, false, 0, Vec::new());
    // Line of the table we are in (the header is 0, the dashes 1).
    let mut row: Option<usize> = None;
    for line in text.split('\n') {
        if is_fence(line) {
            seen += 1;
            in_code = !in_code && seen < fences;
            row = None;
        } else if !in_code {
            row = line.trim_start().starts_with('|').then(|| row.map_or(0, |r| r + 1));
            let cells = row.map(|_| cells(line));
            for (whole, noted, said) in comments(line) {
                let mut note = Note { noted: &line[noted.clone()], said: &line[said.clone()], at: at + whole.start, inside: at + said.start, end: at + whole.end, scope: String::new() };
                if let Some(cells) = &cells {
                    let c = cells.iter().rposition(|cell| cell.start <= whole.start).unwrap_or(0);
                    note.scope = match row {
                        Some(0) => format!("Header {}", column_letters(c)),
                        Some(r) => format!("Cell {}{}", column_letters(c), r.saturating_sub(1)),
                        None => String::new(),
                    };
                } else if noted.start == whole.start && let Some((scope, rest)) = split_scope(note.said) {
                    note.scope = scope;
                    note.said = rest;
                }
                found.push(note);
            }
        }
        at += line.len() + 1;
    }
    found
}

#[cfg(test)]
mod tests {
    #[test]
    fn sets_a_front_matter_key() {
        let note = "---\nauteur: Herbert\ntags: [sf, roman]\nlu:\n  - 2020\n  - 2023\n---\n# Dune\n\nauteur: pas ici\n";
                // Only one key changes, the rest of the file is the same to the byte.
        assert_eq!(set_front(note, "auteur", " Frank Herbert "), note.replacen("auteur: Herbert", "auteur: Frank Herbert", 1));
        assert_eq!(set_front(note, "auteur", "Herbert"), note);
                // A list stays a list, inline.
        assert_eq!(set_front(note, "tags", "sf,classique"), note.replace("[sf, roman]", "[sf, classique]"));
        assert_eq!(set_front(note, "lu", "2024"), note.replace("lu:\n  - 2020\n  - 2023\n", "lu: [2024]\n"));
                // Missing key: added at the end of the front matter, created if there is none; empty: nothing.
        assert_eq!(set_front(note, "note", "9"), note.replace("---\n# Dune", "note: 9\n---\n# Dune"));
        assert_eq!(set_front("# Emma\n", "note", "10"), "---\nnote: 10\n---\n# Emma\n");
        assert_eq!(set_front("# Emma\n", "note", " "), "# Emma\n");
                // Value emptied: the key goes, and the front matter with its last key.
        assert_eq!(set_front(note, "lu", ""), note.replace("lu:\n  - 2020\n  - 2023\n", ""));
        assert_eq!(set_front("---\nnote: 9\n---\n# Emma\n", "note", ""), "# Emma\n");
        assert_eq!(set_front("---\r\nnote: 9\r\n---\r\n# Emma", "note", "8"), "---\r\nnote: 8\r\n---\r\n# Emma");

        let keys = ["Note".to_string(), "auteur".into(), "note".into()];
        let row = ["Dune".to_string(), "Herbert".into(), "9".into()];
        assert!(row_kept(&keys, &row, "") && row_kept(&keys, &row, "herb") && row_kept(&keys, &row, " Auteur: HER "));
        assert!(!row_kept(&keys, &row, "note:herb") && !row_kept(&keys, &row, "austen") && row_kept(&keys, &row, "note:9"));
    }

    use super::*;

    #[test]
    fn classifies_lines() {
        assert_eq!(classify("# Titre", false), (Kind::Heading(1), 2));
        assert_eq!(classify("### T", false), (Kind::Heading(3), 4));
        assert_eq!(classify("#tag", false), (Kind::Para, 0));
        assert_eq!(classify("- a", false), (Kind::Bullet, 2));
        assert_eq!(classify("    * a", false), (Kind::Bullet, 6));
        assert_eq!(classify("- [ ] a", false), (Kind::Task(false), 6));
        assert_eq!(classify("- [x] a", false), (Kind::Task(true), 6));
        assert_eq!(classify("12. a", false), (Kind::Ordered, 4));
        assert_eq!(classify("> q", false), (Kind::Quote, 2));
        assert_eq!(classify("---", false), (Kind::Rule, 3));
        assert_eq!(classify("```rust", false), (Kind::Fence, 7));
        assert_eq!(classify("- a", true), (Kind::Code, 0));
        assert_eq!(classify("texte", false), (Kind::Para, 0));
    }

    #[test]
    fn finds_links() {
        let l = links("voir [[Ma note|alias]] et #projet/x sur https://a.b/c).");
        assert_eq!(l[0].1, Link::Wiki("Ma note".into()));
        assert_eq!(l[1].1, Link::Tag("projet/x".into()));
        assert_eq!(l[2].1, Link::Url("https://a.b/c".into()));
        assert!(links("a#b et #123").is_empty());
                // List page: front matter keys make the columns, in order of appearance.
        let (dune, emma) = ("---\nauteur: Herbert\nnote: 9\ntags: [sf, désert]\n---\n# Dune", "---\nnote: 10\nlu: oui\n---\n");
        assert_eq!(front_keys(dune), ["auteur", "note", "tags"]);
        let (keys, rows) = list_table(&[dune, emma, "# Sans en-tête"]);
        assert_eq!(keys, ["auteur", "note", "tags", "lu"]);
        assert_eq!(rows, [vec!["Herbert", "9", "sf, désert", ""], vec!["", "10", "", "oui"], vec![""; 4]]);
        use std::cmp::Ordering::{Greater, Less};
        assert_eq!((cell_order("9", "10"), cell_order("b", "A"), cell_order("", "z"), cell_order("1,5", "1.25")), (Less, Greater, Greater, Greater));
                // Replacements: the text as is, or a regular expression and its groups.
        let put = |query: &str, with: &str, word, regex| replacements("Le chat, le chien.\nle rat", query, with, false, word, regex);
        assert_eq!(put("le", "un", true, false), [(0..2, "un".to_string()), (9..11, "un".into()), (19..21, "un".into())]);
        assert_eq!(put(r"le (\w+)", "$1 (le)", false, true), [(0..7, "chat (le)".to_string()), (9..17, "chien (le)".into()), (19..25, "rat (le)".into())]);
        assert_eq!(put("^le", "x", false, true).len(), 2);
        assert_eq!(put("ch|ra", "", true, true), []);
        assert!(put("(", "", false, true).is_empty() && put("x*", "", false, true).is_empty());
        assert_eq!(replacements("A a", "a", "b", true, false, true), [(2..3, "b".to_string())]);
        // A regular expression that does not compile is said; text is never one, nor an empty search.
        assert!(broken("(", false, true) && broken("[a", true, true) && !broken("a*", false, true) && !broken("(", false, false) && !broken("", false, true));
        // Replacing with what contains the search does not loop; `$1` without a group is empty.
        assert_eq!(put("le", "lele", true, false).len(), 3);
        assert_eq!(put("le", "${1}x", true, true)[0].1, "x");
        let all = Swap { query: "le".into(), with: "un".into(), case: false, word: true, regex: false };
        assert_eq!(swap_except("Le chat, le chien.", &all, &[]), Some(("un chat, un chien.".into(), 2)));
        assert_eq!(swap_except("rien", &all, &[]), None);
                // Comment names: by content, numbered when the first cells are alike, by position when empty.
        let grid = |rows: &[&[&str]]| rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
        let rows = grid(&[&["Nom", "", "Âge"], &["---", "---", "---"], &["Léa", "", "1"], &["Léa", "", "2"], &["", "", ""]]);
        let (names, cols) = scopes(&rows);
        assert_eq!(names, ["Row \"Nom\"", "Row \"Léa\" (1)", "Row \"Léa\" (2)", "Row 4"]);
        assert_eq!(cols, ["Column \"Nom\"", "Column 2", "Column \"Âge\""]);
                // A line follows its row when it is renamed or moved up, goes with it when it is deleted, and the others stay.
        let old = (names.clone(), cols.clone());
        let after = grid(&[&["Nom", "", "Âge"], &["---", "---", "---"], &["Léa", "", "2"], &["", "", ""]]);
        let new = scopes(&after);
        let lines = ["{>>Row \"Léa\" (1): a<<}", "{>>Row \"Léa\" (2): b<<}", "{>>Column 2: c<<}", "{>>Table: d<<}", "{>>libre<<}", "  {>>Row 4: e<<}"];
        let moved = [Some(0), None, Some(1), Some(2)];
        let kept = [Some(0), Some(1), Some(2)];
        assert_eq!(
            retarget(&lines, &old, &new, (&moved, &kept), false),
            [None, Some("{>>Row \"Léa\": b<<}".to_string()), Some(lines[2].to_string()), Some(lines[3].to_string()), Some(lines[4].to_string()), Some("  {>>Row 3: e<<}".to_string())]
        );
        assert_eq!(retarget(&lines, &old, &new, (&[None; 4], &[None; 3]), true), [None, None, None, None, Some(lines[4].to_string()), None]);
                // Comments: the text, the comment, and where the whole starts; not in code.
        let line = "un {==mot **fort**==}{>>à revoir<<} et {==deux==}{>><<} {==seul==}";
        assert_eq!(comments(line), [(3..36, 6..18, 24..33), (40..56, 43..47, 53..53)]);
        let note = format!("# T\n{line}\n```\n{line}\n```");
        let found: Vec<_> = all_comments(&note).into_iter().map(|n| (n.noted, n.said, n.at, n.inside)).collect();
        assert_eq!(found, [("mot **fort**", "à revoir", 7, 28), ("deux", "", 44, 57)]);
                // A comment on its own has nothing commented, and a full one is not counted twice.
        assert_eq!(comments("a {>>seul<<} b {==x==}{>>y<<}"), [(2..12, 2..2, 5..9), (15..29, 18..19, 25..26)]);
        assert_eq!(comments("{>>pas fini"), []);
                // Where a comment is: in a cell, a header, or a line under a table named by what it is about.
        let table = "| a | b |\n| - | - |\n| {==x==}{>>1<<} | y |\n| u | {==v==}{>>2<<} |\n\n{>>Row \"x\" (2): trois<<}\n\n{>>Column 2: quatre<<}\n\n{>>Table: cinq<<}\n\n{>>libre<<}";
        let scopes: Vec<_> = all_comments(table).into_iter().map(|n| (n.scope, n.said)).collect();
        assert_eq!(scopes, [("Cell A1".into(), "1"), ("Cell B2".into(), "2"), ("Row x (2)".into(), "trois"), ("Column 2".into(), "quatre"), ("Table".into(), "cinq"), (String::new(), "libre")]);
        assert_eq!(split_scope("Row \"a\": b\": c"), Some(("Row a".into(), "b\": c")));
        assert_eq!(split_scope("Column \"\": x"), Some(("Column ".into(), "x")));
        assert_eq!((split_scope("Row x: y"), split_scope("Rowing: y"), split_scope("Table:x")), (None, None, None));
        assert_eq!((column_letters(0), column_letters(25), column_letters(26), column_letters(701), column_letters(702)), ("A".into(), "Z".into(), "AA".into(), "ZZ".into(), "AAA".into()));
        let mut flags = vec![0u16; line.len()];
        inline(line, 0, &mut flags);
        assert!(flags[3..6].iter().all(|f| *f == DIM) && flags[6] == 0 && flags[12] == BOLD);
        assert!(flags[18..24].iter().all(|f| *f == DIM) && flags[24..33].iter().all(|f| *f == DIM | ITALIC | QUIET) && flags[36] == 0);
                // Dates after `@`. October 9, 2026 is a Friday.
        let today = (2026, 10, 9);
        assert_eq!((day_number((1970, 1, 1)), day_number((2000, 3, 1)), day_of(day_number((2024, 2, 29)))), (0, 11_017, (2024, 2, 29)));
        assert_eq!(day_of(day_number((2026, 12, 31)) + 1), (2027, 1, 1));
                // October 2026 starts on a Thursday and has 31 days; February 2024 has 29.
        assert_eq!((month(2026, 10, true), month(2026, 10, false), month(2024, 2, true)), ((3, 31), (4, 31), (3, 29)));
        assert_eq!((add_months((2026, 1, 31), 1), add_months((2026, 12, 5), 1), add_months((2026, 1, 5), -1)), ((2026, 2, 28), (2027, 1, 5), (2025, 12, 5)));
        let found = |q: &str| dates(q, today).into_iter().map(|(_, fr, date)| (fr, date)).collect::<Vec<_>>();
        assert_eq!(found("").len(), 10);
        assert_eq!(found("dem"), [("demain", (2026, 10, 10))]);
        assert_eq!(found("Aujourd’"), [("aujourd'hui", today)]);
        assert_eq!(found("yes"), [("hier", (2026, 10, 8))]);
        assert_eq!(found("vendredi"), [("vendredi", (2026, 10, 16))]);
        assert_eq!(found("mon"), [("lundi", (2026, 10, 12))]);
        assert_eq!(found("t").len(), 4);
        assert_eq!(found("2026-11-02"), [("", (2026, 11, 2))]);
        assert!(found("2026-02-30").is_empty() && found("2026-1-2").is_empty() && found("zz").is_empty());
                // `[text](address)`: a single link, over its whole range; neither an image nor a path.
        let l = links("lire [la doc](https://a.b/c#d) et ![img](https://a.b/i.png)");
        assert_eq!(l[0], (5..30, Link::Url("https://a.b/c#d".into())));
        assert_eq!(l[1], (41..58, Link::Url("https://a.b/i.png".into())));
        assert_eq!(l.len(), 2);
        assert!(links("[x](fichier.md) [y]( ) [a [b](c)").is_empty());
        assert_eq!(web_link("[la doc](mailto:a@b.c) suite"), Some((1..7, 9..21)));
        assert!(is_url("https://a.b") && !is_url("https://") && !is_url("https://a b") && !is_url("a.b"));
        let mut flags = vec![0u16; 31];
        inline("lire [la doc](https://a.b/c#d).", 0, &mut flags);
        assert!(flags[5] == DIM && flags[6..12].iter().all(|f| *f == LINK) && flags[12..14] == [DIM; 2]
            && flags[14..29].iter().all(|f| *f == DIM | QUIET) && flags[29] == DIM && flags[30] == 0);
        let (tags, wikis) = index("#A\n```\n#b [[z]]\n```\n#a #c [[X]] [[x|alias]] [[Y]]");
        assert_eq!(tags, vec!["a", "c"]);
        assert_eq!(wikis, vec!["x", "y"]);
    }

    #[test]
    fn relinks_renamed_notes() {
        let text = "[[Test]] et [[test|alias]], [[ Test#titre]]\n```\n[[Test]]\n```\n[[Testé]] [[Test]]";
        let want = "[[Essai]] et [[Essai|alias]], [[Essai#titre]]\n```\n[[Test]]\n```\n[[Testé]] [[Essai]]";
        assert_eq!(relink(text, "Test", "Essai").as_deref(), Some(want));
        assert_eq!(relink(text, "Autre", "Essai"), None);
                // A renamed image: both spellings follow, the rest of the line too.
        let text = "![a](img/Le%20chat.png \"t\") ![[le chat.png|300]] le chat.png";
        let want = "![a](img/Un%20chien.png \"t\") ![[Un chien.png|300]] le chat.png";
        assert_eq!(relink(text, "Le chat.png", "Un chien.png").as_deref(), Some(want));
        assert_eq!(index("![x](img/Le%20chat.png)\n![[B.svg]]").1, ["le chat.png", "b.svg"]);
    }

    #[test]
    fn styles_inline() {
        let line = "a **b** `c*` snake_case_x *é*";
        let mut f = vec![0u16; line.len()];
        inline(line, 0, &mut f);
        assert_eq!(f[2], DIM);
        assert_eq!(f[4], BOLD);
        assert_eq!(f[5], DIM | BOLD);
        assert_eq!(f[9], CODE);
        assert_eq!(f[10], CODE);
        assert!(f[13..25].iter().all(|&x| x == 0));
        assert_eq!(f[27], ITALIC);
        assert_eq!(f[28], ITALIC);
    }

    #[test]
    fn styles_highlight_and_underline() {
        let on = |line: &str| flags(line, 0);
                // `==text==`: marks dimmed, text highlighted; `<u>text</u>`: underlined.
        let f = on("a ==b c== d");
        assert!(f[2..4].iter().all(|x| *x == DIM) && f[4..7].iter().all(|x| *x == HIGHLIGHT) && f[7..9].iter().all(|x| *x == DIM | HIGHLIGHT) && f[9] == 0 && f[0] == 0);
        let f = on("a <u>b</u> c");
        assert!(f[2..5].iter().all(|x| *x == DIM) && f[5] == UNDER && f[6..10].iter().all(|x| *x == DIM | UNDER) && f[10] == 0);
                // Not an operator `==`, nor a single `==`, nor a `<u>` never closed, nor a lone `</u>`.
        for line in ["a == b == c", "a ==b", "x==y==z", "====", "a === b ===", "<u>ouvert", "fermé</u>", "base64 abc== et def== fin", "?q==a&r==b"] {
            assert!(on(line).iter().all(|x| x & (HIGHLIGHT | UNDER) == 0 && *x & DIM == 0), "{line}: {:?}", on(line));
        }
                // The `{==`…`==}` of a comment stay a comment, even next to a highlight.
        let line = "==a== puis {==mot==}{>>note<<} et ==b==";
        let f = on(line);
        assert!(f[2] == HIGHLIGHT && f[line.find("mot").unwrap()] == 0 && f[line.find("b==").unwrap()] == HIGHLIGHT, "{f:?}");
        let f = on("==a {==mot==}{>>n<<}");
        assert!(f.iter().all(|x| x & HIGHLIGHT == 0), "an opener without a clean closer: {f:?}");
                // In inline code, nothing changes; with other styles, flags add up.
        assert!(on("`==a==` `<u>b</u>`").iter().all(|x| x & (HIGHLIGHT | UNDER) == 0));
        let f = on("**==a <u>é</u>==**");
        assert_eq!(f[4] & (BOLD | HIGHLIGHT), BOLD | HIGHLIGHT);
        assert_eq!(f[9] & (BOLD | HIGHLIGHT | UNDER), BOLD | HIGHLIGHT | UNDER);
    }

    #[test]
    fn highlights_in_color() {
        let on = |line: &str| flags(line, 0);
                // `<mark>` and `<mark class="color">` highlight like `==`, marks dimmed; an unknown color is not a color.
        for line in ["a <mark>b c</mark> d", "a <mark class=\"green\">b c</mark> d"] {
            let f = on(line);
            let at = line.find("b c").unwrap();
            assert!(f[at..at + 3].iter().all(|x| *x == HIGHLIGHT) && f[at - 1] == DIM && f[0] == 0 && *f.last().unwrap() == 0, "{line} {f:?}");
        }
        for line in ["<mark class=\"rouge\">x</mark>", "<mark>jamais fermé", "</mark> seul", "<markdown>x</mark>", "<mark class=\"green>x</mark>"] {
            assert!(on(line).iter().all(|x| x & HIGHLIGHT == 0), "{line}");
        }
                // A highlight opened by `==` does not close with `</mark>`, nor the reverse.
        assert!(on("==a</mark> b==")[2..].iter().take(8).all(|x| x & HIGHLIGHT != 0));
        assert!(on("<mark>a== b</mark>").iter().skip(6).take(5).all(|x| x & HIGHLIGHT != 0));
                // The colors read for painting, and the marks written.
        let line = "a ==b== <mark class=\"blue\">c</mark> <mark>d</mark> <mark class=\"purple\">é</mark>";
        let got: Vec<_> = highlights(line, false).into_iter().map(|(r, color)| (&line[r], color)).collect();
        assert_eq!(got, [("b", 0), ("c", 2), ("d", 0), ("é", 4)]);
        assert_eq!(mark_signs(0), ("==".to_string(), "=="));
        assert_eq!(mark_signs(3), ("<mark class=\"pink\">".to_string(), "</mark>"));
        assert_eq!(mark_signs(99).0, "<mark class=\"purple\">");
        for color in 0..MARKS.len() {
            let (open, close) = mark_signs(color);
            let line = format!("un {open}mot{close} ici");
            let at = line.find("mot").unwrap();
            let cut = unmark(&line, &flags(&line, 0), at..at + 3, HIGHLIGHT);
            assert_eq!(cut, Some((3..at, at + 3..at + 3 + close.len())), "{line}");
            assert_eq!(highlights(&line, false), [(at..at + 3, color)], "{line}");
        }
    }

    #[test]
    fn finds_the_highlighted_text() {
        fn found(line: &str, table: bool) -> Vec<&str> {
            highlights(line, table).into_iter().map(|(r, _)| &line[r]).collect()
        }
        assert_eq!(found("a ==b c== d ==e==", false), ["b c", "e"]);
        assert_eq!(found("- ==puce== et", false), ["puce"]);
        assert_eq!(found("==a **b** c== d", false), ["a **b** c"]);
        assert_eq!(found("**==a==**", false), ["a"]);
        assert_eq!(found("| ==Titre== | autre | ==é🙂== |", true), ["Titre", "é🙂"]);
                // Nothing to paint: no `==`, a single one, an address, a comment, code.
        for line in ["", "a b", "a == b", "abc== et def==", "{==mot==}{>>n<<}", "`==a==`", "==a"] {
            assert!(found(line, false).is_empty(), "{line}");
        }
    }

    #[test]
    fn finds_the_marks_to_remove() {
        let cut = |line: &str, sel: Range<usize>, flag: u16| unmark(line, &flags(line, 0), sel, flag);
                // A cursor in the span, at its edge, or between its marks; outside everything, nothing.
        assert_eq!(cut("a **bc** d", 5..5, BOLD), Some((2..4, 6..8)));
        assert_eq!(cut("a **bc** d", 4..4, BOLD), Some((2..4, 6..8)));
        assert_eq!(cut("a **bc** d", 6..6, BOLD), Some((2..4, 6..8)));
        assert_eq!(cut("a ****", 4..4, BOLD), Some((2..4, 4..6)));
        assert_eq!(cut("a **bc** d", 0..0, BOLD), None);
        assert_eq!(cut("a **bc** d", 9..9, BOLD), None);
        assert_eq!(cut("a **bc** d", 8..8, BOLD), None);
                // A selection: entirely in the span (marks included or not), otherwise nothing.
        assert_eq!(cut("a **bc** d", 4..6, BOLD), Some((2..4, 6..8)));
        assert_eq!(cut("a **bc** d", 2..8, BOLD), Some((2..4, 6..8)));
        assert_eq!(cut("a **bc** d", 4..5, BOLD), Some((2..4, 6..8)));
        assert_eq!(cut("a **bc** d", 0..6, BOLD), None);
        assert_eq!(cut("a **bc** d", 4..10, BOLD), None);
        assert_eq!(cut("a **bc** d", 2..4, BOLD), None);
                // Each style has its marks; another style is not removed.
        assert_eq!(cut("~~x~~", 2..3, STRIKE), Some((0..2, 3..5)));
        assert_eq!(cut("==x==", 2..2, HIGHLIGHT), Some((0..2, 3..5)));
        assert_eq!(cut("<u>x</u>", 3..4, UNDER), Some((0..3, 4..8)));
        assert_eq!(cut("a `x` b", 3..3, CODE), Some((2..3, 4..5)));
        assert_eq!(cut("*x* et _y_", 1..1, ITALIC), Some((0..1, 2..3)));
        assert_eq!(cut("*x* et _y_", 8..8, ITALIC), Some((7..8, 9..10)));
        assert_eq!(cut("**x**", 2..3, ITALIC), None);
                // Nested styles: bold around italic, and italic inside bold.
        assert_eq!(cut("**a *b* c**", 3..3, BOLD), Some((0..2, 9..11)));
        assert_eq!(cut("**a *b* c**", 5..5, BOLD), Some((0..2, 9..11)));
        assert_eq!(cut("**a *b* c**", 5..6, ITALIC), Some((4..5, 6..7)));
        assert_eq!(cut("<u>**é**</u>", 5..7, BOLD), Some((3..5, 7..9)));
        assert_eq!(cut("<u>**é**</u>", 5..7, UNDER), Some((0..3, 9..13)));
                // Non-ASCII text around: bytes, not characters.
        let line = "日本 ==語 é🙂== fin";
        let at = line.find("é").unwrap();
        assert_eq!(cut(line, at..at, HIGHLIGHT), Some((line.find("==").unwrap()..line.find("==").unwrap() + 2, line.rfind("==").unwrap()..line.rfind("==").unwrap() + 2)));
                // An unclosed span is not one.
        assert_eq!(cut("a **bc d", 5..5, BOLD), None);
        assert_eq!(cut("", 0..0, BOLD), None);
    }

    #[test]
    fn colors_code() {
        let line = r#"let x = f("a // b", 42); // fin"#;
        let mut f = vec![0u16; line.len()];
        code(line, "rust", &mut f);
        assert_eq!((f[0], f[4], f[8]), (KEYWORD, 0, 0));
        assert!(f[10..18].iter().all(|&x| x == STRING));
        assert_eq!((f[20], f[25]), (NUMBER, DIM | ITALIC | QUIET));
                // A lifetime is not a string; `#` comments in Python, not in Rust.
        let line = "fn f<'a>(x: &'a str) # '\\n'";
        let mut f = vec![0u16; line.len()];
        code(line, "rust", &mut f);
        assert!(f[3..21].iter().all(|&x| x == 0) && f[23] == STRING);
        code(line, "python", &mut f);
        assert_eq!(f[21], DIM | ITALIC | QUIET);
    }

    #[test]
    fn finds_code_blocks() {
        let text = "a\n```rust\nun\ndeux\n```\nb\n```\nouvert";
        assert_eq!(code_block(text, 10).map(|r| &text[r]), Some("un\ndeux"));
        assert_eq!(code_block(text, 16).map(|r| &text[r]), Some("un\ndeux"));
                // Outside a block, on a fence, or after a ``` never closed.
        assert_eq!((code_block(text, 0), code_block(text, 3), code_block(text, 30)), (None, None, None));
    }

    #[test]
    fn finds_symbols_and_images() {
        let line = "a -> b != c `x -> y` <-> --> !== [[a->b]]";
        let mut f = vec![0u16; line.len()];
        inline(line, 0, &mut f);
        assert_eq!(symbols(line, 0, &f), vec![(2, 2, "→"), (7, 2, "≠"), (21, 3, "↔")]);
        assert_eq!(image("voir ![un chat](img/le%20chat.png \"titre\") ici"), Some((5..42, "img/le chat.png".into())));
        assert_eq!(image("![[photo.jpg|300]]"), Some((0..18, "photo.jpg".into())));
        assert_eq!((image("![]()"), image("[lien](a.png)")), (None, None));
        assert_eq!(math("soit $x^2$ et $y$"), Some((5..10, "x^2", false)));
        assert_eq!(math("$$ \\frac{1}{2} $$"), Some((0..17, "\\frac{1}{2}", true)));
        assert_eq!((math("$5 et $10"), math("5 $ et 10 $"), math("prix : 3$")), (None, None, None));
    }

    #[test]
    fn continues_lists() {
        assert_eq!(on_enter("- a", 3, false), Enter::Insert("\n- ".into()));
        assert_eq!(on_enter("  9. a", 6, false), Enter::Insert("\n  10. ".into()));
        assert_eq!(on_enter("- [x] a", 7, false), Enter::Insert("\n- [ ] ".into()));
        assert_eq!(on_enter("> a", 3, false), Enter::Insert("\n> ".into()));
        assert_eq!(on_enter("- ", 2, false), Enter::Clear);
        assert_eq!(on_enter("  x", 3, false), Enter::Insert("\n  ".into()));
        assert_eq!(on_enter("- a", 3, true), Enter::Insert("\n".into()));
    }

    #[test]
    fn opens_on_the_body() {
        let front = "---\ntags: [a]\n---\n";
        assert_eq!(body_start(&format!("{front}# Titre\n\nCorps\n")), format!("{front}# Titre\n\n").len());
        assert_eq!(body_start("# Titre\nCorps"), "# Titre\n".len());
                // Nothing after the title: the end of the note.
        assert_eq!(body_start("# Titre\n\n"), "# Titre\n\n".len());
        assert_eq!(body_start("# Titre"), "# Titre".len());
        assert_eq!(body_start(""), 0);
        assert_eq!(body_start(&format!("{front}seul")), format!("{front}seul").len());
    }

    #[test]
    fn reads_the_front_matter() {
        let block = "---\ntags: [a]\n# pas un titre\n---\n";
        let text = format!("{block}# Titre\n[[Vieux]] #tag\n");
        assert_eq!(front_matter(&text), block.len());
        assert_eq!(front_matter("---\r\na: 1\r\n...\r\nsuite"), "---\r\na: 1\r\n...\r\n".len());
        assert_eq!(front_matter("---\na: 1\n---"), "---\na: 1\n---".len());
                // Not on line 1, or never closed: ordinary text (the `---` line stays a rule).
        for plain in ["", "---", "---\na: 1\n", "\n---\na\n---\n", "# T\n---\na\n---\n", "----\na\n---\n"] {
            assert_eq!(front_matter(plain), 0, "{plain:?}");
        }
                // No heading, tag or link to rewrite in the block; the rest of the note reads as before.
        assert_eq!(headings(&text), [(1, "Titre", block.len())]);
        let linked = "---\nvoir: \"[[Vieux]]\" #non\n---\n[[Vieux]] #oui\n".to_string();
        assert_eq!(index(&linked), (vec!["oui".to_string()], vec!["vieux".to_string()]));
        assert_eq!(relink(&linked, "vieux", "Neuf").unwrap(), "---\nvoir: \"[[Vieux]]\" #non\n---\n[[Neuf]] #oui\n");
        assert_eq!(relink("---\nvoir: \"[[Vieux]]\"\n---\nrien\n", "vieux", "Neuf"), None);
                // Front matter tags and aliases, in their three spellings.
        let inline = "---\ntags: [projet, \"#idée\", projet]\naliases: Bref app, 'Bref'\ntagsx: [non]\n---\n#corps #projet\n";
        assert_eq!(index(inline).0, ["projet", "idée", "corps"]);
        assert_eq!(aliases(inline), ["Bref app", "Bref"]);
        let dashes = "---\naliases:\n  - \"Un nom\"\n  - Autre\ntags:\n- a\n---\n";
        assert_eq!((aliases(dashes), index(dashes).0), (vec!["Un nom".to_string(), "Autre".to_string()], vec!["a".to_string()]));
        assert!(aliases("aliases: [hors bloc]\n").is_empty() && aliases("---\naliases:\n---\n").is_empty());
    }

    #[test]
    fn lists_headings() {
        let text = "# Un\ntexte #pas\n\n### Trois ##\n```\n# code\n```\n| # | t |\n#serré\n## Deux\n```\n# après un bloc jamais fermé\n";
        let third = text.find("###").unwrap();
        assert_eq!(
            headings(text),
            [(1, "Un", 0), (3, "Trois ##", third), (2, "Deux", text.find("## Deux").unwrap()), (1, "après un bloc jamais fermé", text.rfind("# ").unwrap())]
        );
        assert!(headings("```\n# a\n```\n```\n# b\n```").is_empty());
    }

    #[test]
    fn finds_passages() {
        let text = "Été, été. L'ÉTÉ d'**été**\nétés";
        assert_eq!(find(text, "été", false, false).len(), 5);
        assert_eq!(find(text, "été", true, false).len(), 3);
        assert_eq!(find(text, "été", false, true).len(), 4);
        let bold = find(text, "**été**", true, true);
        assert_eq!((bold.len(), bold[0].clone()), (1, text.find("**").unwrap()..text.find('\n').unwrap()));
        assert_eq!(find("aaaa", "aa", true, false), [0..2, 2..4]);
        assert!(find(text, "", false, false).is_empty() && find("", "a", false, false).is_empty());
        // A letter that grows when lowercased ("İ": two characters) shifts nothing.
        let found = find("İstanbul", "stan", false, false);
        assert_eq!((found.len(), found[0].clone()), (1, "İ".len().."İstan".len()));
    }

    #[test]
    fn counts_words_and_characters() {
        assert_eq!(counts(""), (0, 0));
        assert_eq!(counts("# Été\n\nl'idée, **déjà** 2 fois\n"), (5, 28));
        assert_eq!(counts("👨‍👩‍👧 ok"), (1, 4));
                // Recounting only the changed lines gives the counts of the whole text.
        let texts = ["", "a", "un mot\n", "un mot\ndeux\n", "un mot\r\ndeux\r\n", "un mot\ndeux mots\n", "un\n\ndeux\n", "été\nl'idée 👨‍👩‍👧\nfin", "un mot\ndeux\ntrois\n", "x\r", "x\r\n", "\n", "\n\n", "deux\n"];
        for old in texts {
            for new in texts {
                assert_eq!(recount(old, counts(old), new), counts(new), "{old:?} -> {new:?}");
            }
        }
                // A long text, compared by blocks: a keystroke at the start, in the middle, at the end, and a line removed.
        let long: String = (0..2000).map(|i| format!("ligne {i} d'été\n")).collect();
        for at in [0, 7, long.len() / 2, long.len() - 1, long.len()] {
            let typed = format!("{}mot {}", &long[..at], &long[at..]);
            assert_eq!(recount(&long, counts(&long), &typed), counts(&typed), "keystroke at {at}");
            assert_eq!(recount(&typed, counts(&typed), &long), counts(&long), "indent at {at}");
        }
                // Random edits (always the same): insert, delete, replace, anywhere.
        let bits = ["a", " ", "\n", "\r\n", "\r", "é", "👨‍👩‍👧", "l'idée", "3.14", "mot mot\n", "", "\n\n"];
        let mut seed = 42u64;
        let mut next = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n.max(1)
        };
        let (mut text, mut known) = (String::new(), (0, 0));
        for step in 0..3000 {
            let cut = |t: &str, mut i: usize| {
                while !t.is_char_boundary(i) {
                    i -= 1;
                }
                i
            };
            let from = cut(&text, next(text.len() + 1));
            let to = cut(&text, (from + next(6)).min(text.len())).max(from);
            let fresh = format!("{}{}{}", &text[..from], bits[next(bits.len())], &text[to..]);
            known = recount(&text, known, &fresh);
            assert_eq!(known, counts(&fresh), "edit {step}: {text:?} -> {fresh:?}");
            text = fresh;
        }
    }

    #[test]
    fn fills_a_template() {
        let at = |t: &str| fill(t, "Plan", "2026-10-13", "09:05");
        assert_eq!(at("# {{title}}\n{{date}} {{time}}\n"), ("# Plan\n2026-10-13 09:05\n".to_string(), 24));
                // The cursor: at the first mark, which disappears; without a mark, at the end.
        assert_eq!(at("a {{cursor}}b {{cursor}}c"), ("a b c".to_string(), 2));
        assert_eq!(at("fin"), ("fin".to_string(), 3));
                // Case and spaces do not count; an unknown word, or a brace never closed, stays.
        assert_eq!(at("{{ Title }}|{{DATE}}|{{ time }}").0, "Plan|2026-10-13|09:05");
        assert_eq!(at("{{inconnu}} et {{title").0, "{{inconnu}} et {{title");
        assert_eq!(at("{{}} {{ }}").0, "{{}} {{ }}");
                // What is inserted is not reread, and Unicode goes through: title with braces, accents, emojis.
        assert_eq!(fill("{{title}}", "{{date}} é😀", "d", "t"), ("{{date}} é😀".to_string(), "{{date}} é😀".len()));
        assert_eq!(fill("é {{cursor}}😀 {{title}}", "日本", "", ""), ("é 😀 日本".to_string(), "é ".len()));
                // Empty, and many braces: `{{{title}}}` leaves one brace on each side.
        assert_eq!(at(""), (String::new(), 0));
        assert_eq!(at("{{{title}}}").0, "{{{title}}}");
        assert_eq!(at("{{title}}{{title}}").0, "PlanPlan");
    }

    #[test]
    fn makes_a_template_of_a_note() {
        assert_eq!(templatize("# Réunion\n\nOrdre du jour\n"), "# {{title}}\n\nOrdre du jour\n");
                // No title: one is added; the front matter stays at the top; a `#` in a code block is not a heading.
        assert_eq!(templatize("texte"), "# {{title}}\n\ntexte");
        assert_eq!(templatize("---\ntags: [a]\n---\n# Nom\nsuite"), "---\ntags: [a]\n---\n# {{title}}\nsuite");
        assert_eq!(templatize("---\ntags: [a]\n---\ntexte"), "---\ntags: [a]\n---\n# {{title}}\n\ntexte");
        assert_eq!(templatize("```\n# code\n```\n"), "# {{title}}\n\n```\n# code\n```\n");
        assert_eq!(templatize(""), "# {{title}}\n\n");
                // A `##` is not the title; without a final line break, the title is still replaced.
        assert_eq!(templatize("## Deux\n# Un"), "## Deux\n# {{title}}");
                // Filled afterwards, it gives back a note titled with the chosen name.
        assert_eq!(fill(&templatize("# Réunion\nx"), "Lundi", "", "").0, "# Lundi\nx");
    }

    #[test]
    fn restyles_lines_into_lists() {
        let to = |block: &str, style| restyle(block, style).0;
        assert_eq!(to("a\nb", ListStyle::Bullet), "- a\n- b");
                // Indents kept, numbering per level.
        assert_eq!(to("- a\n  - b\n  - c\n- d", ListStyle::Ordered), "1. a\n  1. b\n  2. c\n2. d");
        assert_eq!(to("1. a\n   2. b", ListStyle::Bullet), "- a\n   - b");
                // A checked box stays checked from one task to the other, not from one list to the other.
        assert_eq!(to("- [x] a\n- [ ] b", ListStyle::Ordered), "1. a\n2. b");
        assert_eq!(to("- a\n- [x] b", ListStyle::Task), "- [ ] a\n- [x] b");
                // A mixed selection takes the requested style; already in that style, it becomes text again.
        assert_eq!(to("- a\n1. b\ntexte", ListStyle::Bullet), "- a\n- b\n- texte");
        assert_eq!(to("- a\n- b", ListStyle::Bullet), "a\nb");
        assert_eq!(to("- [x] a\n- [ ] b", ListStyle::Task), "a\nb");
        assert_eq!(to("  - a\n  - b", ListStyle::Text), "a\nb");
                // Headings, tables, quotes, code and empty lines stay as they are.
        let block = "# T\n\n```\ncode\n```\n| a |\n> q\ntexte\n\nsuite";
        assert_eq!(to(block, ListStyle::Bullet), "# T\n\n```\ncode\n```\n| a |\n> q\n- texte\n\n- suite");
        assert_eq!(to(block, ListStyle::Ordered), "# T\n\n```\ncode\n```\n| a |\n> q\n1. texte\n\n2. suite");
        assert_eq!(to("# T\n```\n- a\n```", ListStyle::Ordered), "# T\n```\n- a\n```");
                // Four levels, and tabs for indent: one counter per level, which restarts when going back up.
        assert_eq!(to("- a\n\t- b\n\t\t- c\n\t\t\t- d\n\t\t- e\n- f", ListStyle::Ordered), "1. a\n\t1. b\n\t\t1. c\n\t\t\t1. d\n\t\t2. e\n2. f");
        assert_eq!(to("1. a\n    1. b\n        1. c\n            1. d", ListStyle::Task), "- [ ] a\n    - [ ] b\n        - [ ] c\n            - [ ] d");
                // A single line, empty or made of spaces; a one-character text.
        assert_eq!(to("", ListStyle::Bullet), "");
        assert_eq!(to("   ", ListStyle::Bullet), "   ");
        assert_eq!(to("é", ListStyle::Ordered), "1. é");
                // A round trip gives back the original list.
        let flat = "- a\n- b\n- c";
        assert_eq!(to(&to(flat, ListStyle::Ordered), ListStyle::Bullet), flat);
        assert_eq!(to(&to(flat, ListStyle::Task), ListStyle::Bullet), flat);
                // Start of the content before and after, to restore the cursor.
        assert_eq!(restyle("  - ab\n\ntexte", ListStyle::Ordered).1, vec![(4, 5), (0, 0), (0, 3)]);
    }

    #[test]
    fn renumbers_around_cursor() {
        let (t, c) = renumber("1. a\n1. b\n    - x\n5. c\n\n7. z", 9).unwrap();
        assert_eq!(t, "1. a\n2. b\n    - x\n3. c\n\n7. z");
        assert_eq!(c, 9);
        let (t, c) = renumber("9. a\n9. b", 9).unwrap();
        assert_eq!((t.as_str(), c), ("9. a\n10. b", 10));
        assert_eq!(renumber("1. a\n2. b", 0), None);
        assert_eq!(renumber("texte", 0), None);
    }
    #[test]
    fn reads_and_writes_tables() {
        assert_eq!(classify("| a | b |", false), (Kind::Table, 0));
        assert_eq!(callout("> [!NOTE] titre"), Some(2..9));
        assert_eq!(callout("> note"), None);
                // An escaped `|` or one inside a link does not separate; the final `|` is optional.
        let line = r"| a \| b | [[n|alias]] |  | fin";
        let found: Vec<&str> = cells(line).into_iter().map(|r| &line[r]).collect();
        assert_eq!(found, [r"a \| b", "[[n|alias]]", "", "fin"]);
        assert_eq!(cells("|   |")[0], 2..2);

                // Aligned columns, dashes added under the header, rows completed.
        let (rows, added) = table("| Nom | Âge |\n| Élodie |");
        assert!(added);
        assert_eq!(format_table(&rows, ""), "| Nom    | Âge |\n| ------ | --- |\n| Élodie |     |");
                // The requested alignment is kept; an already formatted table does not move.
        let text = "  | a   | b   |\n  | :-- | --: |\n  | 1   | 2   |";
        let (rows, added) = table(text);
        assert!(!added);
        assert_eq!(format_table(&rows, "  "), text);
        assert_eq!(format_table(&new_table(2, 2), ""), "|     |     |\n| --- | --- |\n|     |     |");
    }
        /// Several hundred random tables (fixed seed): varied
        /// cells (accents, ideographs, emojis, `\|`, aliased links, code, empty or
        /// very long), irregular rows. The written table reads back identically,
        /// keeps the same number of cells everywhere, and no longer moves once formatted.
    #[test]
    fn tables_survive_random_content() {
        const PIECES: &[&str] = &[
            "a", "é", "Élodie", "日本語", "😀", "x y", r"\|", "[[note|alias]]", "`code`", "**gras**", "12,5", "-", ":",
            "http://a.b/c?d=e", "", "", "très long texte de cellule qui ne tient sur aucune page raisonnable ",
        ];
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for case in 0..600 {
            let cols = 1 + next(6);
            let rows = 1 + next(7);
            let cell = |next: &mut dyn FnMut(usize) -> usize| {
                let n = next(4);
                (0..n).map(|_| PIECES[next(PIECES.len())]).collect::<Vec<_>>().join(if next(2) == 0 { "" } else { " " })
            };
                        // Irregular rows, without a final `|` one time in four.
            let lines: Vec<String> = (0..rows)
                .map(|_| {
                    let n = 1 + next(cols);
                    let body = (0..n).map(|_| format!(" {} ", cell(&mut next).trim())).collect::<Vec<_>>().join("|");
                    format!("|{body}{}", if next(4) == 0 { "" } else { "|" })
                })
                .collect();
            let (grid, _) = table(&lines.join("\n"));
            let width = grid[0].len();
            assert!(grid.iter().all(|row| row.len() == width), "case {case}: {grid:?}");
            let text = format_table(&grid, "");
                        // Same number of cells on each row, whatever the content.
            for line in text.lines() {
                assert_eq!(cells(line).len(), width, "case {case}: {line}");
            }
                        // Read back, the written table gives the same cells, and the same text.
            let (again, added) = table(&text);
            assert!(!added, "case {case}");
                        // The dash line reads back at its written width; the rest identically.
            let data = |g: &[Vec<String>]| g.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, r)| r.clone()).collect::<Vec<_>>();
            assert_eq!(data(&again), data(&grid), "case {case}");
            assert_eq!(format_table(&again, ""), text, "case {case}");
                        // Pasted from another table, it stays a grid of the same shape.
            let pasted = paste_grid(&text).unwrap();
            assert_eq!(pasted.len(), grid.len() - 1, "case {case}");
            assert!(pasted.iter().all(|row| row.len() == width), "case {case}");
        }
    }

        /// A test table written as Markdown, read back as a grid.
    fn grid(text: &str) -> Vec<Vec<String>> {
        table(text).0
    }

    #[test]
    fn edits_a_table_by_rows_and_columns() {
        let t = grid("| Nom | Âge |\n| :-- | --: |\n| Léa | 31 |\n| Max | 7 |");
        let flat = |g: &[Vec<String>]| g.iter().map(|r| r.join("|")).collect::<Vec<_>>().join("/");
        assert_eq!(visible_rows(&t), 3);
                // Insert: never before the header, never past the end.
        assert_eq!(flat(&insert_row(&t, 0)), "Nom|Âge/:--|--:/|/Léa|31/Max|7");
        assert_eq!(flat(&insert_row(&t, 2)), "Nom|Âge/:--|--:/Léa|31/|/Max|7");
        assert_eq!(flat(&insert_row(&t, 99)), "Nom|Âge/:--|--:/Léa|31/Max|7/|");
        assert_eq!(flat(&insert_col(&t, 1)), "Nom||Âge/:--|---|--:/Léa||31/Max||7");
        assert_eq!(flat(&insert_col(&t, 9)), "Nom|Âge|/:--|--:|---/Léa|31|/Max|7|");
                // Delete: the header goes with its row, the next one becomes it; nothing left: the table goes.
        assert_eq!(flat(&delete_rows(&t, 2..3).unwrap()), "Nom|Âge/:--|--:/Léa|31");
        assert_eq!(flat(&delete_rows(&t, 0..1).unwrap()), "Léa|31/:--|--:/Max|7");
        assert_eq!(flat(&delete_rows(&t, 1..3).unwrap()), "Nom|Âge/:--|--:");
        assert_eq!(delete_rows(&t, 0..3), None);
        assert_eq!(flat(&delete_cols(&t, 0..1).unwrap()), "Âge/--:/31/7");
        assert_eq!(delete_cols(&t, 0..2), None);
                // Move: alignment follows its column, the header goes to the row that takes its place.
        assert_eq!(flat(&move_col(&t, 0, 1)), "Âge|Nom/--:|:--/31|Léa/7|Max");
        assert_eq!(flat(&move_row(&t, 1, 2)), "Nom|Âge/:--|--:/Max|7/Léa|31");
        assert_eq!(flat(&move_row(&t, 2, 0)), "Max|7/:--|--:/Nom|Âge/Léa|31");
                // Duplicate.
        assert_eq!(flat(&duplicate_row(&t, 1)), "Nom|Âge/:--|--:/Léa|31/Léa|31/Max|7");
        assert_eq!(flat(&duplicate_row(&t, 0)), "Nom|Âge/:--|--:/Nom|Âge/Léa|31/Max|7");
        assert_eq!(flat(&duplicate_col(&t, 1)), "Nom|Âge|Âge/:--|--:|--:/Léa|31|31/Max|7|7");
                // Sort: the header stays, the sort is natural, empty cells stay at the end in both directions.
        let n = grid("| n |\n| - |\n| 10 |\n| |\n| 9 |\n| 2 |");
        assert_eq!(flat(&sort_rows(&n, 0, true)), "n/-/2/9/10/");
        assert_eq!(flat(&sort_rows(&n, 0, false)), "n/-/10/9/2/");
                // Transpose: the first row becomes the header.
        assert_eq!(flat(&transpose(&t)), "Nom|Léa|Max/---|---|---/Âge|31|7");
                // A single header, two body rows: everything stays a valid grid.
        for g in [insert_row(&t, 1), insert_col(&t, 0), move_col(&t, 1, 0), transpose(&t)] {
            assert!(g.len() >= 2 && is_dashes(&g[1]) && g.iter().all(|r| r.len() == g[0].len()), "{g:?}");
        }
    }

    #[test]
    fn table_edits_keep_a_valid_grid() {
        let mut seed = 0x9E3779B97F4A7C15u64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for case in 0..400 {
            let (cols, body) = (1 + next(5), next(6));
            let mut g = new_table(cols, 1 + body);
            for row in g.iter_mut().enumerate().filter(|(i, _)| *i != 1).map(|(_, r)| r) {
                row.iter_mut().for_each(|c| *c = ["", "a", "b c", "10", "9"][next(5)].to_string());
            }
                        // A random sequence of gestures: the grid stays rectangular, with its dashes, or disappears.
            for step in 0..12 {
                let (r, c) = (next(visible_rows(&g) + 1), next(g[0].len() + 1));
                let done = match next(11) {
                    0 => Some(insert_row(&g, r)),
                    1 => Some(insert_col(&g, c)),
                    2 => delete_rows(&g, r..r + 1 + next(2)),
                    3 => delete_cols(&g, c..c + 1 + next(2)),
                    4 => Some(move_row(&g, r, next(visible_rows(&g) + 1))),
                    5 => Some(move_col(&g, c, next(g[0].len() + 1))),
                    6 => Some(duplicate_row(&g, r)),
                    7 => Some(duplicate_col(&g, c)),
                    8 => Some(sort_rows(&g, c, next(2) == 0)),
                    9 => Some(transpose(&g)),
                    _ => Some(g.clone()),
                };
                let Some(next_grid) = done else { break };
                g = next_grid;
                assert!(g.len() >= 2 && is_dashes(&g[1]) && g.iter().all(|row| row.len() == g[0].len() && !row.is_empty()), "case {case} geste {step}: {g:?}");
                                // Written then read back, it does not change shape.
                let again = table(&format_table(&g, "")).0;
                assert_eq!((again.len(), again[0].len()), (g.len(), g[0].len()), "case {case} geste {step}");
            }
        }
    }

    #[test]
    fn pasted_text_stays_in_its_cell() {
        assert_eq!(cell_text("a|b"), r"a\|b");
        assert_eq!(cell_text(r"a\|b"), r"a\|b");
        assert_eq!(cell_text("une\r\n\r\n  deux  \n"), "une deux");
        assert_eq!(cell_text("\tx\t"), "x");
        assert_eq!(cell_text("a\tb"), "a b");
        assert_eq!(paste_grid("texte\nsur deux lignes"), None);
        assert_eq!(paste_grid("a|b"), None);
        assert_eq!(paste_grid("a\tb\n\nc\t\n"), Some(vec![vec!["a".into(), "b".into()], vec!["c".into(), String::new()]]));
        assert_eq!(paste_grid("1\t2\r\n3\t4"), Some(vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]]));
                // The dashes of a Markdown table are not data.
        assert_eq!(paste_grid("| a | b |\n| :-- | --: |\n| c | d |").unwrap().len(), 2);
        assert_eq!(paste_grid("| a | b |").unwrap(), vec![vec!["a".to_string(), "b".to_string()]]);
        assert_eq!(paste_grid("a\t\"x|y\""), Some(vec![vec!["a".into(), "\"x\\|y\"".into()]]));
    }

    #[test]
    fn copied_cells_are_a_markdown_table_that_pastes_back() {
        let g = |rows: &[&[&str]]| rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(grid_markdown(&[]), "");
        assert_eq!(grid_markdown(&g(&[&["a"]])), "| a   |\n| --- |");
        assert_eq!(grid_markdown(&g(&[&["Nom", "Âge"], &["Léa", "31"]])), "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |");
        let cases: Vec<Vec<Vec<String>>> = vec![
            g(&[&["a"], &["b"]]),
            g(&[&["", ""], &["", "x"]]),
            g(&[&["a|b", "c\\d"], &["l1\nl2", " pad "], &["😀", "日本語"], &["é", "[[x|y]]"]]),
            g(&[&["h1", "h2"]]),
            vec![(0..30).map(|i| format!("c{i}")).collect(), (0..30).map(|i| i.to_string()).collect()],
            std::iter::once(vec!["k".to_string(), "v".to_string()]).chain((0..1000).map(|i| vec![format!("n{i}"), i.to_string()])).collect(),
        ];
        for (n, cells) in cases.iter().enumerate() {
            let text = grid_markdown(cells);
            // Every line is a table line, in one piece, however the cells look.
            assert!(text.lines().all(|l| l.starts_with('|') && l.ends_with('|')), "case {n}: {text}");
            let back = paste_grid(&text).unwrap();
            assert_eq!(back.len(), cells.len(), "case {n}");
            let unescaped = |c: &str| c.replace("\\|", "|");
            for (row, want) in back.iter().zip(cells) {
                let want: Vec<String> = want.iter().map(|c| cell_text(c)).collect();
                assert_eq!(row.iter().map(|c| unescaped(c)).collect::<Vec<_>>(), want.iter().map(|c| unescaped(c)).collect::<Vec<_>>(), "case {n}");
            }
            // Outside a table, it is a table again.
            if cells.len() > 1 {
                assert!(paste_table(&text).is_some(), "case {n}");
            }
        }
    }

    #[test]
    fn pastes_tables_from_anywhere() {
        let md = "| a | b |\n|:--|--:|\n| longue | 2 |";
        assert_eq!(paste_table(md).unwrap(), "| a      | b   |\n| :----- | --: |\n| longue | 2   |");
                // Cells from a web page or spreadsheet: the first row is the header.
        assert_eq!(paste_table("Nom\tÂge\nLéa\t31").unwrap(), "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |");
                // No columns, unequal columns, a single line or indented code: text.
        assert_eq!(paste_table("a\nb"), None);
        assert_eq!(paste_table("a\tb\nc"), None);
        assert_eq!(paste_table("a\tb"), None);
        assert_eq!(paste_table("| a | b |"), None);
        assert_eq!(paste_table("\ta\n\tb"), None);
    }

    #[test]
    fn aligns_by_display_width_and_stays_compact_when_wide() {
                // An ideograph and an emoji take two columns.
        let (rows, _) = table("| a | b |\n| 日本 | 😀 |");
        assert_eq!(format_table(&rows, ""), "| a    | b   |\n| ---- | --- |\n| 日本 | 😀  |");
                // Too wide to stay aligned: a single space around each cell.
        let long = "x".repeat(120);
        let (rows, _) = table(&format!("| a | b |\n| {long} | c |"));
        assert_eq!(format_table(&rows, ""), format!("| a | b |\n| --- | --- |\n| {long} | c |"));
                // Rewritten, it no longer moves.
        let once = format_table(&rows, "");
        assert_eq!(format_table(&table(&once).0, ""), once);
        let (rows, _) = table(&format!("| a | b |\n|:-:|--:|\n| {long} | c |"));
        assert!(format_table(&rows, "").contains("| :-: | --: |"));
    }

    #[test]
    fn finds_checkboxes_anywhere() {
        fn found(line: &str) -> Vec<&str> {
            checkboxes(line, 0).into_iter().map(|r| &line[r]).collect()
        }
        assert_eq!(found("[ ] a [x] b [X]"), ["[ ]", "[x]", "[X]"]);
        assert_eq!(found("| [ ] | fait [x] | a |"), ["[ ]", "[x]"]);
        assert_eq!(found("|[ ]|"), ["[ ]"]);
                // No link, no stuck word, no code, no escape, no other content.
        assert!(found("[[ ]] [x](lien) a[ ] [ ]b `[ ]` \\[ ] [y] [  ]").is_empty());
                // The checkbox of a list task is the marker's, which is skipped.
        assert_eq!(checkboxes("- [ ] a [ ] b", 6).len(), 1);
                // A checked box is bold.
        let mut flags = vec![0u16; 7];
        inline("[x] [ ]", 0, &mut flags);
        assert!(flags[..3].iter().all(|f| f & (MARK | BOLD) == MARK | BOLD));
        assert!(flags[4..].iter().all(|f| f & MARK != 0 && f & BOLD == 0));
    }

        /// The shortcuts that avoid classifying each line say the same as `classify` and `is_fence`.
    #[test]
    fn shortcuts_agree_with_the_line_by_line_scan() {
        let lines = [
            "", "|", "| a |", "  | a |", "\t| a |", "- | x", "# | x", "|---|", "```", "  ```rust", "x ```", "``````", "$$", " $$ ",
            "a $$ b", "$$$$", "> | q", "~~~", "---", "|-", "1. | x",
        ];
        for text in [lines.join("\n"), lines.join("\n\n"), lines.iter().rev().cloned().collect::<Vec<_>>().join("\n"), "```\n```".to_string()] {
            let (fences, dollars) = (
                text.split('\n').filter(|l| is_fence(l)).count(),
                text.split('\n').filter(|l| l.trim() == "$$").count(),
            );
            assert_eq!(count_fences_and_dollars(&text), (fences, dollars), "{text:?}");
        }
        for l in lines {
            assert_eq!(is_table_line(l), classify(l, false).0 == Kind::Table, "{l:?}");
        }
    }
}
