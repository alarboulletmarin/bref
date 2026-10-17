//! Finding and replacing in a note or the vault, and counting words and characters.

use super::*;

/// Spans of `text` equal to `query`, without overlap. `case`: case matters;
/// `word`: the span touches no letter nor digit. The text is the file's: Markdown
/// marks are searched like the rest.
pub fn find(text: &str, query: &str, case: bool, word: bool) -> Vec<Range<usize>> {
    let fold = |c: char| -> Vec<char> { if case { vec![c] } else { c.to_lowercase().collect() } };
    let query: Vec<char> = query.chars().flat_map(fold).collect();
        // Length of the span that starts `rest` and equals the search, if there is one.
    let matched = |rest: &str| -> Option<usize> {
        let mut k = 0;
        for (i, c) in rest.char_indices() {
            if k == query.len() {
                return Some(i);
            }
            for folded in fold(c) {
                if query.get(k) != Some(&folded) {
                    return None;
                }
                k += 1;
            }
        }
        (k == query.len()).then_some(rest.len())
    };
    let inside = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut found = Vec::new();
    let mut at = 0;
    while !query.is_empty() && at < text.len() {
        let step = text[at..].chars().next().map_or(1, char::len_utf8);
        match matched(&text[at..]) {
            Some(len) if !word || !(inside(text[..at].chars().next_back()) || inside(text[at + len..].chars().next())) => {
                found.push(at..at + len);
                at += len.max(step);
            }
            _ => at += step,
        }
    }
    found
}

/// The spans of `text` that `query` names, and the text that replaces each. With `regex`,
/// `query` is a regular expression (`^` and `$` apply to each line) and `with` can
/// reuse its groups (`$1`, `${name}`); a badly written expression finds nothing.
pub fn replacements(text: &str, query: &str, with: &str, case: bool, word: bool, regex: bool) -> Vec<(Range<usize>, String)> {
    if !regex {
        return find(text, query, case, word).into_iter().map(|hit| (hit, with.to_string())).collect();
    }
    let pattern = if word { format!(r"\b(?:{query})\b") } else { query.to_string() };
    let Ok(pattern) = regex::RegexBuilder::new(&pattern).case_insensitive(!case).multi_line(true).build() else {
        return Vec::new();
    };
    let found = pattern.captures_iter(text).filter(|groups| !groups[0].is_empty());
    found
        .map(|groups| {
            let mut put = String::new();
            groups.expand(with, &mut put);
            (groups.get(0).map_or(0..0, |all| all.range()), put)
        })
        .collect()
}

/// What is searched and what replaces it, with the find bar's settings.
#[derive(Clone, PartialEq, Debug)]
pub struct Swap {
    pub query: String,
    pub with: String,
    pub case: bool,
    pub word: bool,
    pub regex: bool,
}

/// `text` once all its matches are replaced, and their number; `None` if there are none.
pub fn swap(text: &str, swap: &Swap) -> Option<(String, usize)> {
    let hits = replacements(text, &swap.query, &swap.with, swap.case, swap.word, swap.regex);
    let (mut put, mut done) = (String::with_capacity(text.len()), 0);
    for (hit, with) in &hits {
        put.push_str(&text[done..hit.start]);
        put.push_str(with);
        done = hit.end;
    }
    put.push_str(&text[done..]);
    (!hits.is_empty()).then_some((put, hits.len()))
}

/// Number of words and characters of the text, as written: Markdown marks
/// count as characters, line endings do not.
pub fn counts(text: &str) -> (usize, usize) {
    (text.unicode_words().count(), text.graphemes(true).filter(|g| !matches!(*g, "\n" | "\r\n")).count())
}

/// The counts of `new`, knowing those of `old` (`was`): only the lines that differ
/// are recounted. No word nor character spans a line ending: the lines'
/// counts add up.
pub fn recount(old: &str, was: (usize, usize), new: &str) -> (usize, usize) {
    let (a, b) = (old.as_bytes(), new.as_bytes());
        // By blocks first (a memory comparison), then byte by byte in the block that differs.
    const BLOCK: usize = 4096;
    let blocks = a.as_chunks::<BLOCK>().0.iter().zip(b.as_chunks::<BLOCK>().0).take_while(|(x, y)| x == y).count() * BLOCK;
    let same = blocks + a[blocks..].iter().zip(&b[blocks..]).take_while(|(x, y)| x == y).count();
        // The common start stops after a line ending, the common end starts after another.
    let head = a[..same].iter().rposition(|&c| c == b'\n').map_or(0, |i| i + 1);
    let (a, b) = (&a[head..], &b[head..]);
    let blocks = a.rchunks_exact(BLOCK).zip(b.rchunks_exact(BLOCK)).take_while(|(x, y)| x == y).count() * BLOCK;
    let (x, y) = (&a[..a.len() - blocks], &b[..b.len() - blocks]);
    let tail = blocks + x.iter().rev().zip(y.iter().rev()).take_while(|(x, y)| x == y).count();
    let tail = a[a.len() - tail..].iter().position(|&c| c == b'\n').map_or(0, |i| tail - i - 1);
    let (gone, come) = (counts(&old[head..old.len() - tail]), counts(&new[head..new.len() - tail]));
    (was.0 + come.0 - gone.0, was.1 + come.1 - gone.1)
}
