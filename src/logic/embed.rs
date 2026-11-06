//! Embedded sections: `![[Note]]`, `![[Note#Heading]]` or `![[Note#^id]]` alone on a line shows that
//! part of another note in place. This is the pure part: reading the line, and finding in the text of
//! the note what the line points at (pure functions).

use std::ops::Range;

use super::markdown as md;

/// What an embed points at in its note.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// The whole note, without its front matter.
    Whole,
    /// A heading, and what is under it up to the next heading of the same level or above.
    Heading(String),
    /// A block marked with `^id` at its end.
    Block(String),
}

/// An embed: the note's name and what of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub note: String,
    pub anchor: Anchor,
}

impl Target {
    /// How the frame names its source: `Note`, `Note › Heading`, `Note › ^id`.
    pub fn label(&self) -> String {
        match &self.anchor {
            Anchor::Whole => self.note.clone(),
            Anchor::Heading(h) => format!("{} › {h}", self.note),
            Anchor::Block(id) => format!("{} › ^{id}", self.note),
        }
    }

    /// The target as it is written between the brackets.
    #[cfg(test)]
    pub fn written(&self) -> String {
        match &self.anchor {
            Anchor::Whole => self.note.clone(),
            Anchor::Heading(h) => format!("{}#{h}", self.note),
            Anchor::Block(id) => format!("{}#^{id}", self.note),
        }
    }
}

/// The embed a line is made of, if it is only that: `![[Note#Heading]]`, spaces around it allowed.
/// An alias after `|` is left out. Whether `Note` is a note or a picture is for the caller to say.
pub fn parse(line: &str) -> Option<Target> {
    let inner = line.trim().strip_prefix("![[")?.strip_suffix("]]")?;
    link_target(inner)
}

/// What `[[…]]` points at, from what is between the brackets: `Note`, `Note#Heading`, `Note#^id`, an alias
/// after `|` left out. `None` for an empty name or a nested bracket.
pub fn link_target(inner: &str) -> Option<Target> {
    if inner.contains(['[', ']']) {
        return None;
    }
    let link = md::WikiLink::parse(inner);
    let (note, anchor) = (link.target(), link.anchor().unwrap_or(""));
    if note.is_empty() {
        return None;
    }
    let anchor = match anchor.strip_prefix('^') {
        Some(id) if !id.is_empty() => Anchor::Block(id.to_string()),
        _ if anchor.is_empty() => Anchor::Whole,
        _ => Anchor::Heading(anchor.to_string()),
    };
    Some(Target { note: note.to_string(), anchor })
}

/// The id a line ends with, `text ^id`: its range in the line. The id is letters, digits and `-`, and
/// stands alone at the end.
fn block_mark(line: &str) -> Option<Range<usize>> {
    let end = line.trim_end().len();
    let caret = line[..end].rfind('^')?;
    let id = &line[caret + 1..end];
    let alone = caret == 0 || line[..caret].ends_with([' ', '\t']);
    (alone && !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then_some(caret..end)
}

/// Lines of `text` outside code blocks: (start, text).
fn lines(text: &str) -> Vec<(usize, &str)> {
    md::walk(text).filter(md::Line::outside).map(|l| (l.at, l.text)).collect()
}

/// The ids of the blocks of the text, in order, to offer after `^`.
pub fn block_ids(text: &str) -> Vec<&str> {
    lines(&text[md::front_matter(text)..]).into_iter().filter_map(|(_, l)| block_mark(l).map(|r| &l[r.start + 1..r.end])).collect()
}

/// Byte range of what `anchor` shows in `text`, and whether the last line carries the `^id`
/// to leave out. `None` if the heading or the block is not there.
fn find(text: &str, anchor: &Anchor) -> Option<Range<usize>> {
    let meta = md::front_matter(text);
    match anchor {
        Anchor::Whole => Some(meta..text.len()),
        Anchor::Heading(wanted) => {
            let heads = md::headings(text);
            let k = heads.iter().position(|(_, label, _)| label.trim().eq_ignore_ascii_case(wanted.trim()) || label.trim().to_lowercase() == wanted.trim().to_lowercase())?;
            let (level, _, start) = heads[k];
            let end = heads[k + 1..].iter().find(|(l, ..)| *l <= level).map_or(text.len(), |(.., at)| *at);
            Some(start..end)
        }
        Anchor::Block(id) => {
            let all = lines(&text[meta..]);
            let at = all.iter().position(|(_, l)| block_mark(l).is_some_and(|r| &l[r.start + 1..r.end] == id))?;
            // A mark alone on its line is that of the block just above.
            let alone = block_mark(all[at].1).is_some_and(|r| all[at].1[..r.start].trim().is_empty());
            let last = if alone { at.checked_sub(1)? } else { at };
            let mut first = last;
            while first > 0 && !all[first - 1].1.trim().is_empty() && all[first - 1].0 + all[first - 1].1.len() + 1 == all[first].0 {
                first -= 1;
            }
            if all[last].1.trim().is_empty() {
                return None;
            }
            Some(meta + all[first].0..meta + all[at].0 + all[at].1.len())
        }
    }
}

/// What the embed shows of `text`: the section, the front matter and the blank lines around it
/// and the `^id` of a block left out. `None` if the heading or the block is not there.
pub fn section(text: &str, anchor: &Anchor) -> Option<String> {
    let range = find(text, anchor)?;
    let mut shown = text[range].trim_matches('\n').trim_end().to_string();
    if let Anchor::Block(_) = anchor {
        // The mark goes, with the line it was alone on.
        let last_start = shown.rfind('\n').map_or(0, |i| i + 1);
        if let Some(mark) = block_mark(&shown[last_start..]) {
            shown.truncate(last_start + mark.start);
            shown = shown.trim_end().to_string();
        }
    }
    (!shown.trim().is_empty()).then_some(shown)
}

/// The byte where the embed's target starts in `text`: the heading, the block, or the top of the note
/// after its front matter. To open the note there.
pub fn position(text: &str, anchor: &Anchor) -> Option<usize> {
    find(text, anchor).map(|r| r.start)
}

/// The one heading that was renamed between two versions of a note (the same headings but one,
/// which reads differently, at the same level): its old and new label. `None` for anything else.
pub fn renamed_heading(before: &str, after: &str) -> Option<(String, String)> {
    let (old, new) = (md::headings(before), md::headings(after));
    let changed: Vec<_> = old.iter().zip(&new).filter(|(a, b)| a.0 != b.0 || a.1 != b.1).collect();
    let [(was, now)] = changed.as_slice() else { return None };
    if old.len() != new.len() || was.0 != now.0 || now.1.trim().is_empty() || was.1.trim().is_empty() {
        return None;
    }
    Some((was.1.trim().to_string(), now.1.trim().to_string()))
}

/// Writes `![[note#old]]` and `[[note#old]]` as `…#new`, for every link of `text` that points at heading
/// `old` of note `note`: the new text, and how many. (A renamed note is `markdown::relink`.)
pub fn rename_heading(text: &str, note: &str, old: &str, new: &str) -> (String, usize) {
    md::rewrite_wikilinks(text, |w| {
        let hit = w.target().eq_ignore_ascii_case(note) && w.anchor().is_some_and(|a| a.eq_ignore_ascii_case(old) && !a.starts_with('^'));
        hit.then(|| w.with(w.target(), Some(new)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(note: &str, anchor: Anchor) -> Option<Target> {
        Some(Target { note: note.into(), anchor })
    }

    #[test]
    fn reads_an_embed_line() {
        assert_eq!(parse("![[Note]]"), target("Note", Anchor::Whole));
        assert_eq!(parse("  ![[Note#Un titre]]  "), target("Note", Anchor::Heading("Un titre".into())));
        assert_eq!(parse("![[ Note # Un titre | alias ]]"), target("Note", Anchor::Heading("Un titre".into())));
        assert_eq!(parse("![[Note#^abc-1]]"), target("Note", Anchor::Block("abc-1".into())));
        assert_eq!(parse("![[Note#^]]"), target("Note", Anchor::Heading("^".into())));
        assert_eq!(parse("![[Dossier/Note]]"), target("Dossier/Note", Anchor::Whole));
        for line in ["[[Note]]", "texte ![[Note]]", "![[Note]] texte", "![[]]", "![[#Titre]]", "![[a]] ![[b]]", "![](a.png)", "", "![[a[b]]", "- ![[Note]]"] {
            assert_eq!(parse(line), None, "{line:?}");
        }
        let t = parse("![[Note#Titre]]").unwrap();
        assert_eq!((t.label(), t.written()), ("Note › Titre".into(), "Note#Titre".into()));
    }

    const NOTE: &str = "---\ntags: [a]\n---\n# Titre\n\nIntro.\n\n## Un\n\ntexte un ^u1\n\n### Sous-un\n\nsous.\n\n## Deux\n\n```\n# pas un titre\n```\n\ndeux.\n\n# Autre\n\nfin ^fin-1\n";

    #[test]
    fn shows_a_section() {
        let s = |a: Anchor| section(NOTE, &a);
        // The heading and what is under it, up to the next heading of the same level or above.
        assert_eq!(s(Anchor::Heading("Un".into())).unwrap(), "## Un\n\ntexte un ^u1\n\n### Sous-un\n\nsous.");
        assert_eq!(s(Anchor::Heading("un".into())).unwrap(), s(Anchor::Heading(" UN ".into())).unwrap());
        assert_eq!(s(Anchor::Heading("Sous-un".into())).unwrap(), "### Sous-un\n\nsous.");
        assert_eq!(s(Anchor::Heading("Deux".into())).unwrap(), "## Deux\n\n```\n# pas un titre\n```\n\ndeux.");
        assert_eq!(s(Anchor::Heading("Titre".into())).unwrap().lines().last(), Some("deux."));
        assert_eq!(s(Anchor::Heading("Autre".into())).unwrap(), "# Autre\n\nfin ^fin-1");
        // Without its front matter, the whole note.
        assert!(s(Anchor::Whole).unwrap().starts_with("# Titre") && s(Anchor::Whole).unwrap().ends_with("fin ^fin-1"));
        // A block, without its mark.
        assert_eq!(s(Anchor::Block("u1".into())).unwrap(), "texte un");
        assert_eq!(s(Anchor::Block("fin-1".into())).unwrap(), "fin");
        for missing in [Anchor::Heading("Absent".into()), Anchor::Heading("pas un titre".into()), Anchor::Block("zzz".into())] {
            assert_eq!(s(missing), None);
        }
        assert_eq!(section("", &Anchor::Whole), None);
        assert_eq!(section("---\na: b\n---\n", &Anchor::Whole), None);
        assert_eq!(block_ids(NOTE), ["u1", "fin-1"]);
        assert_eq!(position(NOTE, &Anchor::Heading("Un".into())), Some(NOTE.find("## Un").unwrap()));
        assert_eq!(position(NOTE, &Anchor::Whole), Some(NOTE.find("# Titre").unwrap()));
    }

    #[test]
    fn finds_blocks() {
        // A block is the lines around its mark; a mark alone on its line is that of the block above.
        let text = "a\nb ^x\nc\n\nlist\n- un\n- deux\n^liste\n\nfin ^y";
        assert_eq!(section(text, &Anchor::Block("x".into())).unwrap(), "a\nb");
        assert_eq!(section(text, &Anchor::Block("liste".into())).unwrap(), "list\n- un\n- deux");
        assert_eq!(section(text, &Anchor::Block("y".into())).unwrap(), "fin");
        for text in ["a^b", "a ^", "a ^b c", "^a\n", "x ^é"] {
            assert_eq!(section(text, &Anchor::Block("b".into())), None, "{text:?}");
        }
        assert_eq!(section("```\ncode ^x\n```\n", &Anchor::Block("x".into())), None);
        assert_eq!(block_ids("un ^a\n```\nx ^b\n```\n^c"), ["a", "c"]);
        // Line endings and accents.
        assert_eq!(section("# É\r\n\r\ntexte ^é1\r\n", &Anchor::Heading("é".into())).unwrap(), "# É\r\n\r\ntexte ^é1");
    }

    #[test]
    fn rewrites_links_when_a_heading_is_renamed() {
        let (out, n) = rename_heading("![[N#Un]] [[N#un|a]] [[N#Deux]] [[M#Un]] ![[N#^Un]] [[N]]", "N", "Un", "Premier");
        assert_eq!((out.as_str(), n), ("![[N#Premier]] [[N#Premier|a]] [[N#Deux]] [[M#Un]] ![[N#^Un]] [[N]]", 2));
        assert_eq!(rename_heading("rien", "N", "a", "b"), ("rien".to_string(), 0));
        // Code is left alone, spaces around the name are kept, the alias too.
        let text = "`[[N#Un]]` ![[ N #Un|x]]\n```\n![[N#Un]]\n```\n";
        assert_eq!(rename_heading(text, "N", "Un", "Z"), ("`[[N#Un]]` ![[ N #Z|x]]\n```\n![[N#Un]]\n```\n".to_string(), 1));
    }
}
