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

/// The search is a regular expression that does not compile: the bar says so rather than finding nothing.
pub fn broken(query: &str, word: bool, regex: bool) -> bool {
    let pattern = if word { format!(r"\b(?:{query})\b") } else { query.to_string() };
    regex && !query.is_empty() && regex::Regex::new(&pattern).is_err()
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

/// `text` once its matches are replaced, except those that start at a byte of `skip` (dismissed from the
/// vault search), and how many were; `None` if none was.
pub fn swap_except(text: &str, swap: &Swap, skip: &[usize]) -> Option<(String, usize)> {
    let hits = replacements(text, &swap.query, &swap.with, swap.case, swap.word, swap.regex);
    let hits: Vec<_> = hits.into_iter().filter(|(hit, _)| !skip.contains(&hit.start)).collect();
    let (mut put, mut done) = (String::with_capacity(text.len()), 0);
    for (hit, with) in &hits {
        put.push_str(&text[done..hit.start]);
        put.push_str(with);
        done = hit.end;
    }
    put.push_str(&text[done..]);
    (!hits.is_empty()).then_some((put, hits.len()))
}

/// A match of the vault search: its line (from 1), that line (without its ending) and the match, in bytes of
/// the note.
#[derive(Clone, PartialEq, Debug)]
pub struct Hit {
    pub line: usize,
    pub span: Range<usize>,
    pub at: Range<usize>,
}

/// The matches of `swap` in `text`, each with its line.
pub fn hits(text: &str, swap: &Swap) -> Vec<Hit> {
    let (mut line, mut from) = (1, 0);
    let mut out = Vec::new();
    for (at, _) in replacements(text, &swap.query, "", swap.case, swap.word, swap.regex) {
        // Lines counted from the previous match on: one pass over the note.
        line += text[from..at.start].matches('\n').count();
        from = at.start;
        let start = text[..at.start].rfind('\n').map_or(0, |i| i + 1);
        let end = text[at.start..].find('\n').map_or(text.len(), |i| at.start + i);
        out.push(Hit { line, span: start..end, at });
    }
    out
}

/// The line of `hit` as the vault search shows it: from its first letter, or cut to `before` characters ahead
/// of the match (« … » says so); the match's place in it. A match over several lines is shown on its first.
pub fn excerpt(text: &str, hit: &Hit, before: usize) -> (String, Range<usize>) {
    let ahead = text[hit.span.start..hit.at.start].trim_start();
    let cut = ahead.char_indices().rev().nth(before.saturating_sub(1)).filter(|_| ahead.chars().count() > before).map(|(i, _)| i);
    let ahead = match cut {
        Some(i) => format!("…{}", &ahead[i..]),
        None => ahead.to_string(),
    };
    let end = hit.at.end.min(hit.span.end);
    let shown = format!("{ahead}{}", &text[hit.at.start..hit.span.end]);
    (shown, ahead.len()..ahead.len() + end - hit.at.start)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn swap(query: &str, with: &str, regex: bool) -> Swap {
        Swap { query: query.into(), with: with.into(), case: false, word: false, regex }
    }

    #[test]
    fn lists_each_match_with_its_line() {
        let text = "# Été\n\nUn été chaud.\n\n| a | été |\nfin";
        let found = hits(text, &swap("été", "", false));
        assert_eq!(found.iter().map(|h| h.line).collect::<Vec<_>>(), [1, 3, 5]);
        for hit in &found {
            assert_eq!(text[hit.at.clone()].to_lowercase(), "été");
            assert!(hit.span.start <= hit.at.start && hit.at.end <= hit.span.end && !text[hit.span.clone()].contains('\n'));
        }
        // Two on a line, the last line without a line ending, none, an empty match.
        assert_eq!(hits("a a\nb", &swap("a", "", false)).iter().map(|h| (h.line, h.span.clone())).collect::<Vec<_>>(), [(1, 0..3), (1, 0..3)]);
        assert_eq!(hits("x\nfin", &swap("fin", "", false))[0].span, 2..5);
        assert!(hits("abc", &swap("z", "", false)).is_empty() && hits("abc", &swap("^", "", true)).is_empty());
        // A match over two lines keeps to its first line.
        let over = hits("un\ndeux", &swap("n\nd", "", true));
        assert_eq!((over[0].line, over[0].span.clone(), over[0].at.clone()), (1, 0..2, 1..4));
    }

    #[test]
    fn an_excerpt_shows_the_match_in_its_line() {
        let text = "  - un très long début de ligne avant le mot cherché ici";
        let hit = &hits(text, &swap("cherché", "", false))[0];
        let (shown, at) = excerpt(text, hit, 12);
        assert_eq!(&shown[at.clone()], "cherché");
        assert!(shown.starts_with('…') && shown.ends_with(" ici"), "{shown}");
        assert_eq!(shown.chars().take_while(|&c| c != 'c').count(), 13, "{shown}");
        // Near the start: the line from its first letter, its indentation dropped.
        let (shown, at) = excerpt("\t\tbonjour", &hits("\t\tbonjour", &swap("jour", "", false))[0], 12);
        assert_eq!((shown.as_str(), at), ("bonjour", 3..7));
        // CJK and emoji: cut on a character, never inside one.
        let text = "東京都の🎉とても長い文章の中に検索語があります";
        let hit = &hits(text, &swap("検索語", "", false))[0];
        let (shown, at) = excerpt(text, hit, 3);
        assert_eq!((shown.as_str(), &shown[at]), ("…の中に検索語があります", "検索語"));
        // A match over two lines: shown up to the end of the first.
        let (shown, at) = excerpt("un\ndeux", &hits("un\ndeux", &swap("n\nd", "", true))[0], 12);
        assert_eq!((shown.as_str(), at), ("un", 1..2));
    }

    #[test]
    fn replaces_all_but_the_dismissed() {
        let text = "a b a b a";
        assert_eq!(swap_except(text, &swap("a", "x", false), &[]), Some(("x b x b x".into(), 3)));
        assert_eq!(swap_except(text, &swap("a", "x", false), &[4]), Some(("x b a b x".into(), 2)));
        assert_eq!(swap_except(text, &swap("a", "x", false), &[0, 4, 8]), None);
        assert_eq!(swap_except(text, &swap("(a) (b)", "$2$1", true), &[]), Some(("ba ba a".into(), 2)));
    }
}
