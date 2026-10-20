//! The label of a transition, `trigger [guard] / effect`, read from the text of an arrow (pure). The
//! guard, between brackets, is drawn in italics; the rest is as typed. What does not fit the grammar
//! is drawn as typed, never refused.

use std::ops::Range;

use super::richtext::{self, Span};

/// The `[guard]`, brackets included: the first pair that begins the line or follows a space or a
/// `)`, closes, is not empty, and comes before the `/` of the effect.
pub fn guard(line: &str) -> Option<Range<usize>> {
    let limit = line.find('/').unwrap_or(line.len());
    let mut from = 0;
    while let Some(at) = line[from..limit].find('[').map(|i| i + from) {
        let before = line[..at].chars().next_back();
        if before.is_none_or(|c| c.is_whitespace() || c == ')') {
            let mut depth = 0;
            for (i, c) in line[at..].char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            let end = at + i + 1;
                            // Not empty, and closed before the effect begins.
                            return (!line[at + 1..end - 1].trim().is_empty() && end <= limit).then_some(at..end);
                        }
                    }
                    _ => {}
                }
            }
            return None;
        }
        from = at + 1;
    }
    None
}

/// The runs of one line of a transition's label.
pub fn runs(line: &str) -> Vec<Span> {
    let Some(g) = guard(line) else { return richtext::spans(line, false) };
    let mut guarded = richtext::spans(&line[g.clone()], false);
    for span in &mut guarded {
        span.style.italic = true;
    }
    let mut out = richtext::spans(&line[..g.start], false);
    out.extend(guarded);
    out.extend(richtext::spans(&line[g.end..], false));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guarded(line: &str) -> Option<&str> {
        guard(line).map(|r| &line[r])
    }

    #[test]
    fn finds_the_guard_of_a_transition() {
        assert_eq!(guarded("timer [ok] / go()"), Some("[ok]"));
        assert_eq!(guarded("[x > 0]"), Some("[x > 0]"));
        assert_eq!(guarded("pay(a) [paid]"), Some("[paid]"));
        assert_eq!(guarded("e [list[0] > 1] / f"), Some("[list[0] > 1]"));
        assert_eq!(guarded("日本 [条件] / 動作 😀"), Some("[条件]"));
        // Not a guard: an index, an effect's brackets, nothing closed, nothing in it, no brackets, nothing at all.
        for no in ["a[0]", "a / b [c]", "[unclosed", "[]", "[  ]", "timer", "", "/", "] [", "a/[b]"] {
            assert_eq!(guarded(no), None, "{no:?}");
        }
    }

    #[test]
    fn draws_only_the_guard_in_italics() {
        let spans = runs("timer [ok] / go()");
        let drawn: Vec<(String, bool)> = spans.iter().map(|s| (s.text.clone(), s.style.italic)).collect();
        assert_eq!(drawn, [("timer ".to_string(), false), ("[ok]".to_string(), true), (" / go()".to_string(), false)]);
        // With no guard, the line as the marks of notes read it; the text is never lost.
        for line in ["", "x", "*a* [g] **b**", "[a] [b]", "日本語 [😀] é | \\"] {
            let text: String = runs(line).iter().map(|s| s.text.as_str()).collect();
            let plain: String = richtext::spans(line, false).iter().map(|s| s.text.as_str()).collect();
            assert_eq!(text, plain, "{line:?}");
        }
    }
}
