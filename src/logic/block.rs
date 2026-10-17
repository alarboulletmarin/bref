//! A note's blocks, like Notion's: what a handle takes, moves, duplicates, deletes
//! or transforms. A block is made of whole lines: a heading, a line of text, a list item
//! with what is indented further under it, a quote or a callout (its consecutive `>` lines),
//! a code or formula block, a table, a rule, an image. Blank lines
//! separate blocks without being one, and the front matter is not one. A moving block takes one
//! of the spaces around it, and gives it back where it lands: paragraphs separated by a blank line
//! stay so, list items stay tight. Pure functions.

use crate::logic::markdown::{self as md, Kind};
use std::ops::Range;

/// What a block can become.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Turn {
    Text,
    Heading(u8),
    Bullet,
    Ordered,
    Task,
    Quote,
    Code,
}

/// Start and end (without the newline) of each line.
fn lines(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in text.split('\n') {
        out.push((at, at + line.len()));
        at += line.len() + 1;
    }
    out
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn is_item(line: &str) -> bool {
    matches!(md::classify(line, false).0, Kind::Bullet | Kind::Ordered | Kind::Task(_))
}

/// The index of the last line of the block starting at line `i`.
fn extent(text: &str, lines: &[(usize, usize)], i: usize) -> usize {
    let at = |k: usize| &text[lines[k].0..lines[k].1];
    let last = lines.len() - 1;
    let line = at(i);
    // A block opened and never closed runs to the end of the note.
    if md::is_fence(line) {
        return (i + 1..=last).find(|&k| md::is_fence(at(k))).unwrap_or(last);
    }
    if line.trim() == "$$" {
        return (i + 1..=last).find(|&k| at(k).trim() == "$$").unwrap_or(last);
    }
    let run = |keep: &dyn Fn(&str) -> bool| (i + 1..=last).take_while(|&k| keep(at(k))).last().unwrap_or(i);
    match md::classify(line, false).0 {
        Kind::Table => run(&md::is_table_line),
        Kind::Quote => run(&|l: &str| md::classify(l, false).0 == Kind::Quote),
        Kind::Bullet | Kind::Ordered | Kind::Task(_) => {
            let own = indent(line);
            run(&|l: &str| !l.trim().is_empty() && indent(l) > own)
        }
        _ => i,
    }
}

/// Top-level blocks, in order: from the start of their first line to the end of their
/// last, without the newline after it.
pub fn blocks(text: &str) -> Vec<Range<usize>> {
    let lines = lines(text);
    let front = md::front_matter(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (start, end) = lines[i];
        if start < front || text[start..end].trim().is_empty() {
            i += 1;
            continue;
        }
        let last = extent(text, &lines, i);
        out.push(start..lines[last].1);
        i = last + 1;
    }
    out
}

/// The block of the line starting at `line`: a list item is its own (with its sub-items),
/// a line of a code block, a table or a quote gives the whole block; a blank line,
/// nothing. `top`: the top-level blocks (`blocks`).
pub fn at(text: &str, top: &[Range<usize>], line: usize) -> Option<Range<usize>> {
    let i = top.partition_point(|b| b.start <= line).checked_sub(1)?;
    let block = top[i].clone();
    if line > block.end {
        return None;
    }
    let first = &text[block.start..text[block.start..].find('\n').map_or(text.len(), |n| block.start + n)];
    let end = text[line..].find('\n').map_or(text.len(), |n| line + n);
    if line == block.start || !is_item(first) || !is_item(&text[line..end]) {
        return Some(block);
    }
    // A sub-item: it and what is indented further under it, without leaving the block.
    let own = indent(&text[line..end]);
    let mut stop = end;
    for (start, end) in lines(&text[..block.end]).into_iter().filter(|(s, _)| *s > line) {
        let l = &text[start..end];
        if l.trim().is_empty() || indent(l) <= own {
            break;
        }
        stop = end;
    }
    Some(line..stop)
}

/// Where a block can be dropped: before each top-level block, before each item of a list,
/// and at the end (`text.len()`).
pub fn targets(text: &str, top: &[Range<usize>]) -> Vec<usize> {
    let mut out = Vec::new();
    for block in top {
        out.push(block.start);
        let first = text[block.start..block.end].split('\n').next().unwrap_or_default();
        if is_item(first) {
            let mut at = block.start;
            for line in text[block.start..block.end].split('\n') {
                if at != block.start && is_item(line) {
                    out.push(at);
                }
                at += line.len() + 1;
            }
        }
    }
    out.push(text.len());
    out
}

/// The space after the block up to the next block (blank lines and newlines), and where
/// that next one starts; `None` for the last block.
fn after(text: &str, block: &Range<usize>) -> (Range<usize>, bool) {
    let rest = &text[block.end..];
    let blank = rest.len() - rest.trim_start_matches(['\n', ' ', '\t']).len();
    // The space stops at the start of the next line that has text.
    let gap = &rest[..blank];
    let to = block.end + gap.rfind('\n').map_or(0, |n| n + 1);
    (block.end..to, to < text.len() && blank < rest.len())
}

/// The space before the block from the end of the previous block.
fn before(text: &str, block: &Range<usize>) -> Range<usize> {
    let head = &text[..block.start];
    let kept = head.trim_end_matches(['\n', ' ', '\t']).len();
    // It starts at the newline that ends the last written line.
    let from = head[kept..].find('\n').map_or(block.start, |n| kept + n);
    from.max(md::front_matter(text).saturating_sub(1)).min(block.start)..block.start
}

/// The first line of a block.
fn head(piece: &str) -> &str {
    piece.split('\n').next().unwrap_or_default()
}

/// The text without the block nor the smaller of the two spaces around it (the one after, on
/// a tie): the last item of a list leaves with the newline that ties it to the previous item,
/// and the blank line separating the list from what follows stays in place. Also returns the space removed and
/// where the block was in the new text.
fn take(text: &str, block: &Range<usize>) -> Option<(String, String, usize)> {
    let (gap, more) = after(text, block);
    let before = before(text, block);
    let lines = |r: &Range<usize>| text[r.clone()].matches('\n').count();
    let mut out = text.to_string();
    if more && (before.is_empty() || lines(&gap) <= lines(&before)) {
        out.replace_range(block.start..gap.end, "");
        return Some((out, text[gap].to_string(), block.start));
    }
    if before.is_empty() {
        return None;
    }
    out.replace_range(before.start..block.end, "");
    Some((out, text[before.clone()].to_string(), before.start))
}

/// The space to put between two neighbouring blocks `a` and `b`: two list items stay tight; an
/// item next to something else takes the space `there` of the place it lands; otherwise `kept`, the space
/// the block had, keeps the note's way of writing.
fn spacing(a: &str, b: &str, there: &str, kept: &str) -> String {
    let gap = match (is_item(head(a)), is_item(head(b))) {
        (true, true) => "\n",
        (false, false) => kept,
        _ => there,
    };
    if gap.contains('\n') { gap.to_string() } else { "\n\n".to_string() }
}

/// The block placed before the block starting at `to` (in the original text), or at the end
/// (`text.len()`): the new text and where the block is. `None` when nothing would move.
pub fn moved(text: &str, block: Range<usize>, to: usize) -> Option<(String, Range<usize>)> {
    let (gap, _) = after(text, &block);
    if (block.start..=gap.end).contains(&to) || (to == text.len() && gap.end >= text.len()) {
        return None;
    }
    let piece = &text[block.clone()];
    let (mut rest, kept, _) = take(text, &block)?;
    let removed = text.len() - rest.len();
    if to >= text.len() {
        let end = rest.trim_end_matches(['\n', ' ', '\t']).len();
        // The space of this place: the one before the last block.
        let last = blocks(&rest[..end]).last().cloned().unwrap_or(0..0);
        let there = rest[before(&rest, &last)].to_string();
        let sep = spacing(&rest[last], piece, &there, &kept);
        rest.insert_str(end, &format!("{sep}{piece}"));
        let at = end + sep.len();
        return Some((rest, at..at + piece.len()));
    }
    // The space that originally precedes the block we drop before: the one of this place.
    let there_start = text[..to].trim_end_matches(['\n', ' ', '\t']).len();
    let there = &text[there_start..to];
    let target = text[to..].split('\n').next().unwrap_or_default();
    let sep = spacing(piece, target, there, &kept);
    let at = if to > block.start { to - removed } else { to };
    rest.insert_str(at, &format!("{piece}{sep}"));
    Some((rest, at..at + piece.len()))
}

/// The block doubled right after itself: an item sticks to its double, another block keeps the space
/// around it. The new text and where the copy is.
pub fn duplicated(text: &str, block: Range<usize>) -> (String, Range<usize>) {
    let piece = &text[block.clone()];
    let (gap, more) = after(text, &block);
    let kept = if more { &text[gap] } else { &text[before(text, &block)] };
    let sep = spacing(piece, piece, kept, kept);
    let mut out = text.to_string();
    out.insert_str(block.end, &format!("{sep}{piece}"));
    let at = block.end + sep.len();
    (out, at..at + piece.len())
}

/// The text without the block nor one of the spaces around it, and where to put the cursor.
pub fn removed(text: &str, block: Range<usize>) -> (String, usize) {
    match take(text, &block) {
        Some((out, _, at)) => {
            let at = at.min(out.len());
            (out, at)
        }
        // The only block of the note: it goes, what surrounds it stays.
        None => {
            let mut out = text.to_string();
            out.replace_range(block.clone(), "");
            (out, block.start)
        }
    }
}

/// The content of a line, without its heading, list or quote marker.
fn bare(line: &str) -> &str {
    let (kind, marker) = md::classify(line, false);
    match kind {
        Kind::Heading(_) | Kind::Quote | Kind::Bullet | Kind::Ordered | Kind::Task(_) => &line[marker.min(line.len())..],
        _ => line.trim_start(),
    }
}

/// The lines of block `piece` in the form `turn`. A code block loses its fences before
/// becoming something else; a list is born from each line, numbered from 1.
pub fn turned(piece: &str, turn: Turn) -> String {
    let lines: Vec<&str> = piece.split('\n').collect();
    let fenced = lines.len() >= 2 && md::is_fence(lines[0]) && md::is_fence(lines[lines.len() - 1]);
    let inner: Vec<&str> = if fenced { lines[1..lines.len() - 1].to_vec() } else { lines.iter().map(|l| bare(l)).collect() };
    match turn {
        Turn::Code if fenced => piece.to_string(),
        Turn::Code => format!("```\n{}\n```", lines.iter().map(|l| bare(l)).collect::<Vec<_>>().join("\n")),
        Turn::Text => inner.join("\n"),
        Turn::Heading(n) => {
            let hashes = "#".repeat(n as usize);
            inner.iter().map(|l| if l.trim().is_empty() { l.to_string() } else { format!("{hashes} {l}") }).collect::<Vec<_>>().join("\n")
        }
        Turn::Quote => inner.iter().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n"),
        Turn::Bullet | Turn::Ordered | Turn::Task => {
            let style = match turn {
                Turn::Bullet => md::ListStyle::Bullet,
                Turn::Ordered => md::ListStyle::Ordered,
                _ => md::ListStyle::Task,
            };
            // A list already of this kind stays so: `restyle` would turn it back into text.
            let source = if fenced { inner.join("\n") } else { piece.to_string() };
            let lines: Vec<&str> = source.split('\n').collect();
            let plain: String = match lines.iter().any(|l| matches!(md::classify(l, false).0, Kind::Heading(_) | Kind::Quote)) {
                true => lines.iter().map(|l| bare(l)).collect::<Vec<_>>().join("\n"),
                false => source.clone(),
            };
            let (out, _) = md::restyle(&plain, style);
            if out.split('\n').all(|l| l.trim().is_empty() || md::classify(l, false).0 == Kind::Para) && plain != out {
                plain
            } else {
                out
            }
        }
    }
}

/// The smallest replacement that turns `old` into `new`: the range in `old` and its text.
pub fn diff(old: &str, new: &str) -> (Range<usize>, String) {
    let mut start = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let room = old.len().min(new.len()) - start;
    let mut tail = old.bytes().rev().zip(new.bytes().rev()).take(room).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - tail) || !new.is_char_boundary(new.len() - tail) {
        tail -= 1;
    }
    (start..old.len() - tail, new[start..new.len() - tail].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named<'a>(text: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
        ranges.iter().map(|r| &text[r.clone()]).collect()
    }

    const NOTE: &str = "# Titre\n\nUn paragraphe.\n\n- un\n    - sous\n- deux\n\n```\ncode\n\nencore\n```\n\n| a | b |\n| - | - |\n\n> cité\n> encore\n\n---\n";

    #[test]
    fn cuts_a_note_into_blocks() {
        let top = blocks(NOTE);
        assert_eq!(
            named(NOTE, &top),
            ["# Titre", "Un paragraphe.", "- un\n    - sous", "- deux", "```\ncode\n\nencore\n```", "| a | b |\n| - | - |", "> cité\n> encore", "---"]
        );
        // The front matter is not a block; a code block never closed runs to the end.
        let front = "---\ntags: a\n---\ntexte\n```\nouvert\n\nfin";
        assert_eq!(named(front, &blocks(front)), ["texte", "```\nouvert\n\nfin"]);
    }

    #[test]
    fn finds_the_block_of_a_line() {
        let top = blocks(NOTE);
        let line = |needle: &str| NOTE.find(needle).unwrap();
        let of = |needle: &str| at(NOTE, &top, line(needle)).map(|r| &NOTE[r]);
        assert_eq!(of("- un"), Some("- un\n    - sous"));
        assert_eq!(of("    - sous"), Some("    - sous"));
        assert_eq!(of("- deux"), Some("- deux"));
        assert_eq!(of("encore\n```"), Some("```\ncode\n\nencore\n```"));
        assert_eq!(of("| - |"), Some("| a | b |\n| - | - |"));
        assert_eq!(at(NOTE, &top, line("\n\nUn") + 1), None, "une ligne vide est hors des blocs");
        // A list's items are as many places to drop a block, besides the blocks and the end.
        let targets = targets(NOTE, &top);
        assert!(targets.contains(&line("- deux")) && targets.contains(&line("    - sous")) && targets.contains(&NOTE.len()));
    }

    #[test]
    fn moves_blocks_with_their_spacing() {
        let text = "A\n\nB\n\nC\n";
        let (a, c) = (0..1, 6..7);
        assert_eq!(moved(text, a.clone(), 6).unwrap(), ("B\n\nA\n\nC\n".to_string(), 3..4));
        assert_eq!(moved(text, a, text.len()).unwrap().0, "B\n\nC\n\nA\n");
        assert_eq!(moved(text, c.clone(), 0).unwrap(), ("C\n\nA\n\nB\n".to_string(), 0..1));
        // Right before itself, or right after: nothing moves.
        assert_eq!(moved(text, 3..4, 3), None);
        assert_eq!(moved(text, 3..4, 6), None);
        assert_eq!(moved(text, c, text.len()), None);
        // No final newline, and list items that stay tight.
        let list = "- a\n- b\n- c";
        assert_eq!(moved(list, 0..3, list.len()).unwrap().0, "- b\n- c\n- a");
        assert_eq!(moved(list, 8..11, 0).unwrap().0, "- c\n- a\n- b");
        // The only block has nowhere to go.
        assert_eq!(moved("seul\n", 0..4, 5), None);
        // The last item of a list, moved to the top: the list stays tight, and the blank line that
        // separates it from the callout stays in place.
        let list = "Titre\n\n- a\n- b\n- c\n\n> note\n";
        let c = list.find("- c").unwrap();
        assert_eq!(moved(list, c..c + 3, list.find("- a").unwrap()).unwrap().0, "Titre\n\n- c\n- a\n- b\n\n> note\n");
        // An item taken out of its list takes the space of the place it lands.
        let a = list.find("- a").unwrap();
        assert_eq!(moved(list, a..a + 3, list.find("> note").unwrap()).unwrap().0, "Titre\n\n- b\n- c\n\n- a\n\n> note\n");
        assert_eq!(moved(list, a..a + 3, list.len()).unwrap().0, "Titre\n\n- b\n- c\n\n> note\n\n- a\n");
    }

    #[test]
    fn duplicates_and_removes() {
        let text = "A\n\nB\n";
        assert_eq!(duplicated(text, 0..1), ("A\n\nA\n\nB\n".to_string(), 3..4));
        assert_eq!(duplicated(text, 3..4), ("A\n\nB\n\nB\n".to_string(), 6..7));
        assert_eq!(duplicated("- a\n- b", 0..3).0, "- a\n- a\n- b");
        assert_eq!(duplicated("- a\n- b\n\nP", 4..7).0, "- a\n- b\n- b\n\nP");
        assert_eq!(removed(text, 0..1), ("B\n".to_string(), 0));
        assert_eq!(removed(text, 3..4), ("A\n".to_string(), 1));
        assert_eq!(removed("seul", 0..4), (String::new(), 0));
    }

    #[test]
    fn turns_blocks_into_others() {
        assert_eq!(turned("Texte", Turn::Heading(2)), "## Texte");
        assert_eq!(turned("# Titre", Turn::Text), "Titre");
        assert_eq!(turned("# Titre", Turn::Bullet), "- Titre");
        assert_eq!(turned("un\ndeux", Turn::Ordered), "1. un\n2. deux");
        assert_eq!(turned("- un", Turn::Bullet), "- un", "une liste à puces le reste");
        assert_eq!(turned("- un", Turn::Task), "- [ ] un");
        assert_eq!(turned("- [x] fait", Turn::Text), "fait");
        assert_eq!(turned("> cité", Turn::Text), "cité");
        assert_eq!(turned("ligne", Turn::Quote), "> ligne");
        assert_eq!(turned("a\nb", Turn::Code), "```\na\nb\n```");
        assert_eq!(turned("```\na\n```", Turn::Text), "a");
        assert_eq!(turned("```\na\n```", Turn::Code), "```\na\n```");
    }

    #[test]
    fn diffs_to_the_smallest_change() {
        assert_eq!(diff("abcdef", "abXYef"), (2..4, "XY".to_string()));
        assert_eq!(diff("été", "étté"), (3..3, "t".to_string()));
        assert_eq!(diff("aé", "aè"), (1..3, "è".to_string()));
        assert_eq!(diff("same", "same"), (4..4, String::new()));
    }
}
