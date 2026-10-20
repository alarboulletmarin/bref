//! Styled text of a diagram: the marks that notes already use (`**bold**`, `*italic*`, `_italic_`,
//! `<u>underline</u>`, `~~strike~~`, backticks) read into runs of text, the marks removed. There is
//! one grammar: `markdown::emphasis`. Links and the rest of what notes read stay plain text.

use super::markdown::{self as md, BOLD, CODE, DIM, ITALIC, STRIKE, UNDER};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub under: bool,
    pub strike: bool,
    pub code: bool,
}

/// A run of text drawn in one style.
#[derive(Clone, PartialEq, Debug)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    #[cfg(test)]
    pub fn is_plain(&self) -> bool {
        self.style == Style::default()
    }
}

/// The text as typed, in one run, bold or not.
pub fn plain(text: &str, bold: bool) -> Vec<Span> {
    vec![Span { text: text.to_string(), style: Style { bold, ..Style::default() } }]
}

/// Length of the mark that starts `rest`.
fn mark(rest: &str) -> Option<usize> {
    ["**", "~~", "<u>", "</u>"].iter().find(|m| rest.starts_with(**m)).map(|m| m.len()).or_else(|| rest.starts_with(['`', '*', '_']).then_some(1))
}

/// The runs of one line of text. `bold` is the style the line already has (a class name).
/// A mark that closes nothing is text, as in a note.
pub fn spans(line: &str, bold: bool) -> Vec<Span> {
    let mut flags = vec![0u16; line.len()];
    md::emphasis(line, 0, &mut flags);
    let mut out: Vec<Span> = Vec::new();
    let mut at = 0;
    while at < line.len() {
        let f = flags[at];
        // Only the marks of the styles above are hidden: `==` and `<mark>` stay visible text.
        if f & DIM != 0
            && let Some(n) = mark(&line[at..])
        {
            at += n;
            continue;
        }
        let c = line[at..].chars().next().map_or(1, char::len_utf8);
        let style = Style { bold: bold || f & BOLD != 0, italic: f & ITALIC != 0, under: f & UNDER != 0, strike: f & STRIKE != 0, code: f & CODE != 0 };
        match out.last_mut() {
            Some(last) if last.style == style => last.text.push_str(&line[at..at + c]),
            _ => out.push(Span { text: line[at..at + c].to_string(), style }),
        }
        at += c;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(line: &str) -> Vec<(String, &'static str)> {
        let name = |s: Style| match s {
            Style { bold: true, italic: true, .. } => "bold italic",
            Style { bold: true, .. } => "bold",
            Style { italic: true, .. } => "italic",
            Style { under: true, .. } => "under",
            Style { strike: true, .. } => "strike",
            Style { code: true, .. } => "code",
            _ => "",
        };
        spans(line, false).into_iter().map(|s| (s.text, name(s.style))).collect()
    }

    #[test]
    fn reads_the_marks_of_notes() {
        let one = |t: &str, s: &'static str| vec![(t.to_string(), s)];
        assert_eq!(runs("*abstract()*"), one("abstract()", "italic"));
        assert_eq!(runs("_abstract()_"), one("abstract()", "italic"));
        assert_eq!(runs("<u>count</u>"), one("count", "under"));
        assert_eq!(runs("**Name**"), one("Name", "bold"));
        assert_eq!(runs("~~old~~"), one("old", "strike"));
        assert_eq!(runs("`List<T>`"), one("List<T>", "code"));
        assert_eq!(runs("***both***"), one("both", "bold italic"));
        assert_eq!(runs("+ <u>count</u>: int"), vec![("+ ".into(), ""), ("count".into(), "under"), (": int".into(), "")]);
        // Bold already: the class name's.
        assert!(spans("Name", true)[0].style.bold && spans("*Name*", true)[0].style.italic);
    }

    #[test]
    fn leaves_what_is_not_a_mark() {
        // Nothing closed, a star between spaces, a lone underscore in a word, an address, a comparison.
        for plain in ["", "a", "**a", "a * b * c", "snake_case_name", "abc==", "a == b", "*", "**", "<u>open", "</u>", "[[Note]]", "a | b \\ c", "日本語 😀", "{==c==}"] {
            let got = spans(plain, false);
            let text: String = got.iter().map(|s| s.text.as_str()).collect();
            assert!(got.iter().all(Span::is_plain) && text == plain, "{plain:?} -> {got:?}");
        }
        // Marks of a style we do not draw (`==`) stay in the text.
        assert_eq!(runs("==a=="), vec![("==a==".to_string(), "")]);
    }

    #[test]
    fn keeps_every_character_of_the_text() {
        // Whatever the marks do, the visible text is the line minus its marks, never a lost letter.
        for line in ["**é*日*😀**", "a*b*c", "x`y`z", "<u>*a*</u>", "~~**b**~~", "* a *b*", "Ünï `cödé` **gras** _it_"] {
            let text: String = spans(line, false).iter().map(|s| s.text.as_str()).collect();
            let letters = |t: &str| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
            assert_eq!(letters(&text), letters(&line.replace("<u>", "").replace("</u>", "")), "{line:?}");
        }
    }
}
