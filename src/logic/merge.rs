//! Three texts of one note: `base` as the app last read or wrote it, `ours` as typed since, and
//! `theirs` as another program (an agent, a sync tool) wrote it in the meantime. What `theirs`
//! changed is turned into replacements of `ours`, so the editor applies them without losing a
//! keystroke. Lines are the unit, as in git.

use std::ops::Range;

/// A changed block: lines `base` of the base became lines `side` of the other text.
#[derive(Debug, Clone, PartialEq)]
struct Hunk {
    base: Range<usize>,
    side: Range<usize>,
}

/// What `theirs` changed, as byte replacements in `ours`, in order. `None` when both changed the
/// same lines differently, or inserted different lines at the same place: the caller keeps both.
pub fn onto(base: &str, ours: &str, theirs: &str) -> Option<Vec<(Range<usize>, String)>> {
    let (b, o, t) = (lines(base), lines(ours), lines(theirs));
    let (mine, others) = (hunks(&b, &o), hunks(&b, &t));
    let mut edits = Vec::new();
    for hunk in others {
        let same = |h: &&Hunk| h.base == hunk.base && o[h.side.clone()] == t[hunk.side.clone()];
        if mine.iter().any(|h| same(&h)) {
            // The same change on both sides: it is already in `ours`.
            continue;
        }
        let empty = |r: &Range<usize>| r.start == r.end;
        let clash = mine.iter().any(|h| {
            (hunk.base.start < h.base.end && h.base.start < hunk.base.end)
                || (empty(&hunk.base) && empty(&h.base) && hunk.base.start == h.base.start)
        });
        if clash {
            return None;
        }
        // Where the block's edges land in `ours`. An insertion of ours at the start comes before
        // their block, one at the end comes after it.
        let shift = |at: usize, start: bool| {
            mine.iter()
                .filter(|h| h.base.end < at || h.base.end == at && (start || !empty(&h.base)))
                .fold(at as isize, |n, h| n + h.side.len() as isize - h.base.len() as isize) as usize
        };
        let start = shift(hunk.base.start, true);
        let end = if empty(&hunk.base) { start } else { shift(hunk.base.end, false) };
        let byte = |line: usize| o[..line].iter().map(|l| l.len()).sum::<usize>();
        edits.push((byte(start)..byte(end), t[hunk.side.clone()].concat()));
    }
    Some(edits)
}

fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Above this many cells, the middle that differs is one block: coarser, never wrong.
// ponytail: quadratic longest common subsequence on what is left once the common start and end
// are cut (a few lines when someone types); swap for Myers' diff if big rewrites must merge finely.
const CELLS: usize = 1_000_000;

fn hunks(base: &[&str], side: &[&str]) -> Vec<Hunk> {
    let head = base.iter().zip(side).take_while(|(a, b)| a == b).count();
    let tail = base[head..].iter().rev().zip(side[head..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (b, s) = (&base[head..base.len() - tail], &side[head..side.len() - tail]);
    if b.is_empty() && s.is_empty() {
        return Vec::new();
    }
    if b.len() * s.len() > CELLS || b.is_empty() || s.is_empty() {
        return vec![Hunk { base: head..head + b.len(), side: head..head + s.len() }];
    }
    // Length of the longest common subsequence of b[i..] and s[j..].
    let w = s.len() + 1;
    let mut lcs = vec![0u32; (b.len() + 1) * w];
    for i in (0..b.len()).rev() {
        for j in (0..s.len()).rev() {
            lcs[i * w + j] = if b[i] == s[j] { lcs[(i + 1) * w + j + 1] + 1 } else { lcs[(i + 1) * w + j].max(lcs[i * w + j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    let mut open: Option<(usize, usize)> = None;
    while i < b.len() || j < s.len() {
        if i < b.len() && j < s.len() && b[i] == s[j] {
            if let Some((bi, sj)) = open.take() {
                out.push(Hunk { base: head + bi..head + i, side: head + sj..head + j });
            }
            (i, j) = (i + 1, j + 1);
            continue;
        }
        open.get_or_insert((i, j));
        if j < s.len() && (i == b.len() || lcs[i * w + j + 1] >= lcs[(i + 1) * w + j]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if let Some((bi, sj)) = open {
        out.push(Hunk { base: head + bi..head + i, side: head + sj..head + j });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ours` with what `theirs` changed put in, or `None`.
    fn merged(base: &str, ours: &str, theirs: &str) -> Option<String> {
        let mut text = ours.to_string();
        for (range, with) in onto(base, ours, theirs)?.into_iter().rev() {
            text.replace_range(range, &with);
        }
        Some(text)
    }

    #[test]
    fn changes_on_different_lines_are_both_kept() {
        let base = "# Note\n\nfirst\nsecond\nthird\n";
        // An agent appends while the user types on the first lines.
        assert_eq!(merged(base, "# Note\n\nfirst typed\nsecond\nthird\n", "# Note\n\nfirst\nsecond\nthird\n\n## Added\nby the agent\n").unwrap(), "# Note\n\nfirst typed\nsecond\nthird\n\n## Added\nby the agent\n");
        // The agent edits above where the user types.
        assert_eq!(merged(base, "# Note\n\nfirst\nsecond\nthird!\n", "# Note\n\nFIRST\nsecond\nthird\n").unwrap(), "# Note\n\nFIRST\nsecond\nthird!\n");
        // Neighbouring lines, one each: lines are independent.
        assert_eq!(merged(base, "# Note\n\nfirst\nsecond 2\nthird\n", "# Note\n\nfirst\nsecond\nthird 3\n").unwrap(), "# Note\n\nfirst\nsecond 2\nthird 3\n");
        // Lines removed by the agent, lines added by the user elsewhere.
        assert_eq!(merged(base, "# Note\nnew\n\nfirst\nsecond\nthird\n", "# Note\n\nthird\n").unwrap(), "# Note\nnew\n\nthird\n");
    }

    #[test]
    fn the_same_lines_changed_twice_clash() {
        let base = "a\nb\nc\n";
        assert_eq!(merged(base, "a\nB mine\nc\n", "a\nB theirs\nc\n"), None);
        // Two insertions at the same place: their order cannot be chosen.
        assert_eq!(merged(base, "a\nmine\nb\nc\n", "a\ntheirs\nb\nc\n"), None);
        // A line deleted on one side, changed on the other.
        assert_eq!(merged(base, "a\nc\n", "a\nb!\nc\n"), None);
        // Typing at the end of a last line without newline while the agent appends to it.
        assert_eq!(merged("a\nb", "a\nbX", "a\nb\nc\n"), None);
    }

    #[test]
    fn edges_and_identical_changes() {
        // Nothing changed elsewhere; or only elsewhere; or the same change on both sides.
        assert_eq!(merged("a\n", "a typed\n", "a\n").unwrap(), "a typed\n");
        assert_eq!(merged("a\n", "a\n", "b\n").unwrap(), "b\n");
        assert_eq!(merged("a\nb\n", "a\nx\n", "a\nx\n").unwrap(), "a\nx\n");
        // Empty texts.
        assert_eq!(merged("", "typed\n", "").unwrap(), "typed\n");
        assert_eq!(merged("", "", "theirs\n").unwrap(), "theirs\n");
        assert_eq!(merged("", "mine\n", "theirs\n"), None);
        assert_eq!(merged("a\n", "a\n", "").unwrap(), "");
        // An insertion of ours right before their changed block, one of theirs right after ours.
        assert_eq!(merged("a\nb\nc\n", "a\nmine\nb\nc\n", "a\nB\nc\n").unwrap(), "a\nmine\nB\nc\n");
        assert_eq!(merged("a\nb\nc\n", "a\nB\nc\n", "a\nb\ntheirs\nc\n").unwrap(), "a\nB\ntheirs\nc\n");
        // CRLF, accents, CJK, emoji: lines are compared byte for byte.
        assert_eq!(merged("é\r\n日本\r\n", "é typed\r\n日本\r\n", "é\r\n日本 🙂\r\n").unwrap(), "é typed\r\n日本 🙂\r\n");
    }

    #[test]
    fn a_long_note_merges_and_a_huge_rewrite_stays_whole() {
        let base: String = (0..5_000).map(|n| format!("line {n}\n")).collect();
        let ours = base.replace("line 10\n", "line 10 typed\n");
        let theirs = base.replace("line 4000\n", "line 4000 agent\n");
        assert_eq!(merged(&base, &ours, &theirs).unwrap(), base.replace("line 10\n", "line 10 typed\n").replace("line 4000\n", "line 4000 agent\n"));
        // Every line rewritten on one side, one on the other: too big to align finely, so one clash.
        let all: String = (0..2_000).map(|n| format!("other {n}\n")).collect();
        let start = std::time::Instant::now();
        assert_eq!(merged(&base, &ours, &all), None);
        assert!(start.elapsed().as_millis() < 500);
    }

    #[test]
    fn random_edits_apart_always_merge() {
        // Each side changes lines of its own half: the result has both, whatever the changes.
        let mut seed = 7u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..300 {
            let base: Vec<String> = (0..20).map(|n| format!("{n}\n")).collect();
            let edit = |lines: &mut Vec<String>, from: usize, r: u64| {
                let at = from + (r % 8) as usize;
                match r % 3 {
                    0 => lines[at] = format!("changed {r}\n"),
                    1 => lines[at] = String::new(),
                    _ => lines[at].insert_str(0, &format!("new {r}\n")),
                }
            };
            let (mut ours, mut theirs) = (base.clone(), base.clone());
            let (a, b) = (next(), next());
            edit(&mut ours, 1, a);
            edit(&mut theirs, 11, b);
            let mut both = base.clone();
            edit(&mut both, 1, a);
            edit(&mut both, 11, b);
            assert_eq!(merged(&base.concat(), &ours.concat(), &theirs.concat()).unwrap(), both.concat());
        }
    }
}
