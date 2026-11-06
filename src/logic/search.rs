//! Finding and replacing in a note or the vault, and counting words and characters.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

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
pub fn replacements(text: &str, query: &str, with: &str, FindOptions { case, word, regex }: FindOptions) -> Vec<(Range<usize>, String)> {
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
pub fn broken(query: &str, FindOptions { word, regex, .. }: FindOptions) -> bool {
    let pattern = if word { format!(r"\b(?:{query})\b") } else { query.to_string() };
    regex && !query.is_empty() && regex::Regex::new(&pattern).is_err()
}

/// The find bar's settings.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct FindOptions {
    pub case: bool,
    pub word: bool,
    pub regex: bool,
}

/// One of the settings, as the toggles name it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FindOption {
    Case,
    Word,
    Regex,
}

impl FindOptions {
    pub fn flip(&mut self, option: FindOption) {
        let on = match option {
            FindOption::Case => &mut self.case,
            FindOption::Word => &mut self.word,
            FindOption::Regex => &mut self.regex,
        };
        *on ^= true;
    }

    pub fn is(&self, option: FindOption) -> bool {
        match option {
            FindOption::Case => self.case,
            FindOption::Word => self.word,
            FindOption::Regex => self.regex,
        }
    }
}

/// What is searched and what replaces it, with the find bar's settings.
#[derive(Clone, PartialEq, Debug)]
pub struct Swap {
    pub query: String,
    pub with: String,
    pub options: FindOptions,
}

/// `text` once its matches are replaced, except those that start at a byte of `skip` (dismissed from the
/// vault search), and how many were; `None` if none was.
pub fn swap_except(text: &str, swap: &Swap, skip: &[usize]) -> Option<(String, usize)> {
    let hits = replacements(text, &swap.query, &swap.with, swap.options);
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
    for (at, _) in replacements(text, &swap.query, "", swap.options) {
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
    use crate::logic::{dates::*, markdown::*};

    fn swap(query: &str, with: &str, regex: bool) -> Swap {
        Swap { query: query.into(), with: with.into(), options: FindOptions { regex, ..Default::default() } }
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
        let put = |query: &str, with: &str, word, regex| replacements("Le chat, le chien.\nle rat", query, with, FindOptions { word, regex, ..Default::default() });
        assert_eq!(put("le", "un", true, false), [(0..2, "un".to_string()), (9..11, "un".into()), (19..21, "un".into())]);
        assert_eq!(put(r"le (\w+)", "$1 (le)", false, true), [(0..7, "chat (le)".to_string()), (9..17, "chien (le)".into()), (19..25, "rat (le)".into())]);
        assert_eq!(put("^le", "x", false, true).len(), 2);
        assert_eq!(put("ch|ra", "", true, true), []);
        assert!(put("(", "", false, true).is_empty() && put("x*", "", false, true).is_empty());
        assert_eq!(replacements("A a", "a", "b", FindOptions { case: true, regex: true, ..Default::default() }), [(2..3, "b".to_string())]);
        // A regular expression that does not compile is said; text is never one, nor an empty search.
        let opts = |word, regex| FindOptions { word, regex, ..Default::default() };
        assert!(broken("(", opts(false, true)) && broken("[a", opts(true, true)) && !broken("a*", opts(false, true)) && !broken("(", opts(false, false)) && !broken("", opts(false, true)));
        // Replacing with what contains the search does not loop; `$1` without a group is empty.
        assert_eq!(put("le", "lele", true, false).len(), 3);
        assert_eq!(put("le", "${1}x", true, true)[0].1, "x");
        let all = Swap { query: "le".into(), with: "un".into(), options: FindOptions { word: true, ..Default::default() } };
        assert_eq!(swap_except("Le chat, le chien.", &all, &[]), Some(("un chat, un chien.".into(), 2)));
        assert_eq!(swap_except("rien", &all, &[]), None);
                // Comment names: by content, numbered when the first cells are alike, by position when empty.
        let grid = |rows: &[&[&str]]| rows.iter().map(|r| r.iter().map(|c| c.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
        let rows = grid(&[&["Nom", "", "Âge"], &["---", "---", "---"], &["Léa", "", "1"], &["Léa", "", "2"], &["", "", ""]]);
        let (names, cols) = scopes(&rows);
        let written = |list: &[Scope]| list.iter().map(Scope::written).collect::<Vec<_>>();
        assert_eq!(written(&names), ["Row \"Nom\"", "Row \"Léa\" (1)", "Row \"Léa\" (2)", "Row 4"]);
        assert_eq!(written(&cols), ["Column \"Nom\"", "Column 2", "Column \"Âge\""]);
        // What a scope writes, `parse` reads back: also for texts that look like the syntax, except `": ` and
        // `" (1): ` inside a name, where the first one wins (a name is cut at the first).
        for text in ["", "a", "Léa", "a\" (", "日本 😀", " x ", "\\|", "[[x]]", "(1)", "3"] {
            for nth in [None, Some(1), Some(12)] {
                for scope in [Scope::Row(Label::Text(text.into(), nth)), Scope::Column(Label::Text(text.into(), nth))] {
                    let line = format!("{}tail: x", scope.opening().trim_start_matches("{>>"));
                    assert_eq!(Scope::parse(&line), Some((scope.clone(), "tail: x")), "{text:?} {nth:?}");
                    assert_eq!(scope.said(&line), Some("tail: x"));
                }
            }
        }
        for place in [Scope::Row(Label::Place(3)), Scope::Column(Label::Place(120)), Scope::Table] {
            let line = format!("{}x", place.opening().trim_start_matches("{>>"));
            assert_eq!(Scope::parse(&line), Some((place.clone(), "x")), "{place}");
        }
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
        let scopes: Vec<_> = scopes.into_iter().map(|(scope, said)| (scope.map(|s| s.to_string()), said)).collect();
        assert_eq!(scopes, [("Cell A1", "1"), ("Cell B2", "2"), ("Row x (2)", "trois"), ("Column 2", "quatre"), ("Table", "cinq")].map(|(s, w)| (Some(s.to_string()), w)).into_iter().chain([(None, "libre")]).collect::<Vec<_>>());
        assert_eq!(Scope::parse("Row \"a\": b\": c"), Some((Scope::Row(Label::Text("a".into(), None)), "b\": c")));
        assert_eq!(Scope::parse("Column \"\": x"), Some((Scope::Column(Label::Text("".into(), None)), "x")));
        assert_eq!((Scope::parse("Row x: y"), Scope::parse("Rowing: y"), Scope::parse("Table:x")), (None, None, None));
        // The comment written next to its parser is read back whole.
        let commented = comment_on("a b", "x|y");
        assert_eq!(comments(&commented), [(0..commented.len(), 3..6, 12..15)]);
        assert_eq!(comments(&comment_on("a", "")).len(), 1);
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
        assert_eq!(index("```\n#b [[z]]").1, ["z"]);
        let (tags, wikis) = index("#A\n```\n#b [[z]]\n```\n#a #c [[X]] [[x|alias]] [[Y]]");
        assert_eq!(tags, vec!["a", "c"]);
        assert_eq!(wikis, vec!["x", "y"]);
    }
}
