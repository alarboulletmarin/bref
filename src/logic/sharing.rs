//! Readable text copies of Bref's Markdown dialect. No linked files are expanded.

use super::markdown as md;

/// Strip presentation, keep code, list/task markers and destinations. Tables become
/// tab-separated rows. Omit front matter and comments, keeping the annotated text.
/// Unknown syntax stays literal; relative links and embeds remain references.
pub fn plain_text(text: &str) -> String {
    let mut out = String::new();
    let mut fence: Option<(char, usize)> = None;
    let mut html_comment = false;
    for raw in text[md::front_matter(text)..].split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|c| *c == marker).count();
        if let Some((open, n)) = fence {
            if marker == open && count >= n && trimmed[count..].trim().is_empty() {
                fence = None;
            } else {
                out.push_str(line);
                if raw.ends_with('\n') { out.push('\n'); }
            }
            continue;
        }
        if !html_comment && matches!(marker, '`' | '~') && count >= 3 {
            fence = Some((marker, count));
            continue;
        }
        let clean = without_comments(line, &mut html_comment);
        if clean.is_empty() && !line.is_empty() { continue; }
        let (kind, start) = md::classify(&clean, false);
        let visible = match kind {
            md::Kind::Heading(_) | md::Kind::Quote => inline(&clean[start..]),
            md::Kind::Rule => String::new(),
            md::Kind::Table => {
                let cells = table_cells(&clean);
                if md::is_dashes(&cells) { continue; }
                cells.iter().map(|s| inline(s)).collect::<Vec<_>>().join("\t")
            }
            // List indentation and completion state carry meaning in a text copy.
            _ => inline(&clean),
        };
        out.push_str(&visible);
        if raw.ends_with('\n') { out.push('\n'); }
    }
    out
}

fn table_cells(line: &str) -> Vec<String> {
    let mut bars = Vec::new();
    let mut at = 0;
    while at < line.len() {
        let rest = &line[at..];
        if rest.starts_with('`') && let Some((end, _)) = code(rest) {
            at += end;
            continue;
        }
        if rest.starts_with("[[") && let Some(end) = rest.find("]]") {
            at += end + 2;
            continue;
        }
        let c = rest.chars().next().unwrap();
        at += c.len_utf8();
        if c == '\\' {
            at += line[at..].chars().next().map_or(0, char::len_utf8);
        } else if c == '|' { bars.push(at - 1); }
    }
    if bars.last().is_some_and(|last| !line[last + 1..].trim().is_empty()) { bars.push(line.len()); }
    bars.windows(2).map(|pair| line[pair[0] + 1..pair[1]].trim().to_string()).collect()
}

fn without_comments(line: &str, html: &mut bool) -> String {
    let mut out = String::new();
    let mut at = 0;
    let comments = md::comments(line);
    let mut comments = comments.iter().peekable();
    while at < line.len() {
        while comments.peek().is_some_and(|(r, ..)| r.start < at) { comments.next(); }
        let rest = &line[at..];
        if *html {
            let Some(end) = rest.find("-->") else { break };
            at += end + 3;
            *html = false;
        } else if rest.starts_with("<!--") {
            *html = true;
            at += 4;
        } else if rest.starts_with('`') && let Some((end, _)) = code(rest) {
            out.push_str(&rest[..end]);
            at += end;
        } else if let Some((whole, noted, _)) = comments.peek().filter(|(r, ..)| r.start == at) {
            out.push_str(&line[noted.clone()]);
            at = whole.end;
            comments.next();
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            at += c.len_utf8();
        }
    }
    out
}

fn code(text: &str) -> Option<(usize, &str)> {
    let n = text.bytes().take_while(|b| *b == b'`').count();
    if n == 0 { return None; }
    let end = text[n..].find(&text[..n])? + n;
    Some((end + n, &text[n..end]))
}

fn styles(text: &str) -> String {
    let mut flags = vec![0; text.len()];
    md::emphasis(text, 0, &mut flags);
    text.char_indices().filter_map(|(i, c)| (flags[i] & md::DIM == 0).then_some(c)).collect()
}

fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut flags = vec![0; text.len()];
    md::emphasis(text, 0, &mut flags);
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        if rest.starts_with('\\') && let Some(c) = rest[1..].chars().next().filter(|c| c.is_ascii_punctuation()) {
            out.push(c);
            at += 1 + c.len_utf8();
            continue;
        }
        if rest.starts_with('`') && let Some((end, inside)) = code(rest) {
            out.push_str(inside);
            at += end;
            continue;
        }
        let image = rest.starts_with("![");
        let link = if image { &rest[1..] } else { rest };
        if let Some(inner) = link.strip_prefix("[[").and_then(|r| r.find("]]").map(|end| (&r[..end], end + 4))) {
            let (target, label) = inner.0.split_once('|').map_or((inner.0, inner.0), |(t, l)| (t, l));
            if image { out.push_str("[embed: "); }
            out.push_str(&styles(label));
            if label != target { out.push_str(&format!(" ({target})")); }
            if image { out.push(']'); }
            at += inner.1 + usize::from(image);
            continue;
        }
        if link.starts_with('[') && let Some(mid) = link.find("](") {
            let from = mid + 2;
            let mut depth = 1;
            let end = link[from..].char_indices().find_map(|(i, c)| {
                match c { '(' => depth += 1, ')' => depth -= 1, _ => {} }
                (depth == 0).then_some(from + i)
            });
            if let Some(end) = end {
                if image { out.push_str("[image: "); }
                let label = styles(&link[1..mid]);
                let target = &link[from..end];
                out.push_str(&label);
                if label.is_empty() { out.push_str(target); }
                else if label != target { out.push_str(&format!(" ({target})")); }
                if image { out.push(']'); }
                at += end + 1 + usize::from(image);
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        if flags[at] & md::DIM == 0 { out.push(c); }
        at += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharing_plain_text_rules() {
        for (source, expected) in [
            ("", ""),
            ("# Été 😀\n\n**bold** _word_ ~~old~~ ==bright== <u>under</u>", "Été 😀\n\nbold word old bright under"),
            ("- [ ] do\n  - [x] done\n7. seven\n> quote", "- [ ] do\n  - [x] done\n7. seven\nquote"),
            ("[site](https://example.org/a(b)) [[Note#Title|Alias]]", "site (https://example.org/a(b)) Alias (Note#Title)"),
            ("![été](../été.png) ![[Note#Part]] ![](x.png)", "[image: été (../été.png)] [embed: Note#Part] [image: x.png]"),
            ("| Name | Count |\n| :--- | ---: |\n| **é** | `a|b` |\n", "Name\tCount\né\ta|b\n"),
            ("---\nprivate: yes\n---\n# Title\n<!-- hidden\nmore -->body {==kept==}{>>secret<<} {>>aside<<}", "Title\nbody kept "),
            ("```rust\n# **literal** <!-- keep -->\n```\n~~~\n[[literal]]\n~~~\n", "# **literal** <!-- keep -->\n[[literal]]\n"),
            ("````\n```\ntext\n````\n", "```\ntext\n"),
            ("```\n未閉じ 😀", "未閉じ 😀"),
            ("`**code**` ``a`b`` \\*literal\\* snake_case", "**code** a`b *literal* snake_case"),
            ("<!--\n```\nsecret\n```\n-->visible", "visible"),
            ("**[site](url)** and *`code`*", "site (url) and code"),
            ("[relative](../a.md)\r\nplain\r\n", "relative (../a.md)\nplain\n"),
        ] {
            assert_eq!(plain_text(source), expected, "{source:?}");
        }
    }
}
