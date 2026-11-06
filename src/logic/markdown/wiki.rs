//! The inside of a wikilink, `target#anchor|alias`: one reader for the whole app, and the one rewriter
//! of the links of a note.

use super::{CODE, inline, walk};

/// What is between the brackets of `[[…]]`, cut at the first `|` (the alias) and then at the first `#`
/// (the anchor). Every part is as written, spaces included; `target` and `anchor` trim them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WikiLink<'a> {
    /// Everything before the alias, `name#anchor`.
    pub to: &'a str,
    pub name: &'a str,
    pub anchor: Option<&'a str>,
    pub alias: Option<&'a str>,
}

impl<'a> WikiLink<'a> {
    pub fn parse(inner: &'a str) -> Self {
        let (to, alias) = inner.split_once('|').map_or((inner, None), |(t, a)| (t, Some(a)));
        let (name, anchor) = to.split_once('#').map_or((to, None), |(n, a)| (n, Some(a)));
        WikiLink { to, name, anchor, alias }
    }

    /// The note's name, without the spaces around it.
    pub fn target(&self) -> &'a str {
        self.name.trim()
    }

    /// The heading or `^id` after `#`, without the spaces around it.
    pub fn anchor(&self) -> Option<&'a str> {
        self.anchor.map(str::trim)
    }

    /// The link written again with another name and anchor (as written, `None` for none): the spaces around the
    /// name and the alias are kept.
    pub fn with(&self, name: &str, anchor: Option<&str>) -> String {
        let lead = &self.name[..self.name.len() - self.name.trim_start().len()];
        let tail = &self.name[self.name.trim_end().len()..];
        let mut out = format!("{lead}{name}{tail}");
        for (mark, part) in [('#', anchor), ('|', self.alias)] {
            if let Some(part) = part {
                out.push(mark);
                out.push_str(part);
            }
        }
        out
    }
}

/// Rewrites the wikilinks of `text` (`[[…]]`, `![[…]]`) by what `change` says for each: the new inside
/// of the brackets, `None` to leave it. Links in code blocks and in inline code are left alone, whoever
/// asks. Returns the new text and how many links changed.
pub fn rewrite(text: &str, change: impl Fn(WikiLink) -> Option<String>) -> (String, usize) {
    let (mut out, mut count) = (String::with_capacity(text.len()), 0);
    for l in walk(text) {
        let line = l.text;
        let (mut done, mut from) = (0, 0);
        if l.outside() && line.contains("[[") {
            let mut flags = vec![0u16; line.len()];
            inline(line, 0, &mut flags);
            while let Some(open) = line[from..].find("[[").map(|i| i + from) {
                let Some(close) = line[open..].find("]]").map(|i| i + open) else { break };
                from = close + 2;
                let inner = &line[open + 2..close];
                if flags[open] & CODE != 0 {
                    continue;
                }
                if let Some(new) = change(WikiLink::parse(inner)).filter(|new| new != inner) {
                    out.push_str(&line[done..open + 2]);
                    out.push_str(&new);
                    done = close;
                    count += 1;
                }
            }
        }
        out.push_str(&line[done..]);
        out.push_str(&text[l.at + line.len()..l.end]);
    }
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_target_anchor_and_alias() {
        let w = WikiLink::parse(" Note # Un titre | alias ");
        assert_eq!((w.target(), w.anchor(), w.alias), ("Note", Some("Un titre"), Some(" alias ")));
        assert_eq!(WikiLink::parse("Note").anchor, None);
        assert_eq!(WikiLink::parse("a|b#c").anchor, None);
        assert_eq!(WikiLink::parse("a#b|c").to, "a#b");
        assert_eq!(WikiLink::parse("").target(), "");
        assert_eq!(WikiLink::parse("#^id").target(), "");
        assert_eq!(WikiLink::parse("日本#é|😀").anchor(), Some("é"));
        assert_eq!(WikiLink::parse(" a #b|c").with("Z", Some("y")), " Z #y|c");
        assert_eq!(WikiLink::parse("a").with("Z", None), "Z");
    }

    #[test]
    fn rewrites_outside_code_only() {
        let up = |w: WikiLink| (w.target() == "a").then(|| w.with("b", w.anchor));
        let text = "[[a]] `[[a]]` ![[ a #x|y]]\n```\n[[a]]\n```\n[[a]] [[";
        assert_eq!(rewrite(text, up), ("[[b]] `[[a]]` ![[ b #x|y]]\n```\n[[a]]\n```\n[[b]] [[".to_string(), 3));
        assert_eq!(rewrite("[[a]]\n", up), ("[[b]]\n".to_string(), 1));
        assert_eq!(rewrite("[[b]]", up), ("[[b]]".to_string(), 0));
        // A stray fence opens nothing.
        assert_eq!(rewrite("```\n[[a]]", up), ("```\n[[b]]".to_string(), 1));
    }
}
