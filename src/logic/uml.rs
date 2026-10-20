//! UML class notation, read from the text of a box (pure). A class is a rectangle whose text is cut
//! into compartments by `---` lines: the name, with its stereotype; then the attributes; then the
//! operations; then any other. What is typed is the notation of the specification (§9.5 of UML 2.5.1),
//! and what cannot be read is kept and drawn as typed, never refused.
//!
//! ```text
//! «interface»          stereotype, then the name (bold, centered)
//! Shape
//! ---
//! + <u>count</u>: int  an attribute: visibility, name, type, multiplicity, default, properties
//! # static total: int  `static` is drawn underlined, `abstract` in italics
//! ---
//! + abstract area(): f32
//! ```

use super::richtext::{self, Span};

/// `+ - # ~`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Package,
}

impl Visibility {
    const ALL: [(Visibility, char); 4] = [(Visibility::Public, '+'), (Visibility::Private, '-'), (Visibility::Protected, '#'), (Visibility::Package, '~')];

    pub fn sign(self) -> char {
        Visibility::ALL.iter().find(|(v, _)| *v == self).map_or('+', |(_, c)| *c)
    }
}

/// An attribute or an operation: `[visibility] [static] [abstract] [/]name[(parameters)][: type[multiplicity]][ = default][ {properties}]`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Member {
    pub visibility: Option<Visibility>,
    pub is_static: bool,
    pub is_abstract: bool,
    /// `/name`: a derived attribute.
    pub derived: bool,
    pub name: String,
    /// `Some` for an operation: what is between the parentheses, as typed.
    pub params: Option<String>,
    /// The type of an attribute, the return type of an operation.
    pub ty: Option<String>,
    pub multiplicity: Option<String>,
    pub default: Option<String>,
    /// What is between braces at the end (`readOnly`, `ordered`…), without `static` and `abstract`.
    pub props: Vec<String>,
}

/// Index of the byte that closes the bracket opened at the start of `s`, brackets of its kind nested.
fn closing(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

impl Member {
    /// Reads a line, or `None` when it is not a member (it is then drawn as typed).
    pub fn parse(line: &str) -> Option<Member> {
        let mut rest = line.trim();
        let mut m = Member::default();
        if let Some(c) = rest.chars().next()
            && let Some((v, _)) = Visibility::ALL.iter().find(|(_, s)| *s == c)
        {
            m.visibility = Some(*v);
            rest = rest[c.len_utf8()..].trim_start();
        }
        // The keywords that become a style, before the name.
        loop {
            if let Some(after) = rest.strip_prefix("static ") {
                m.is_static = true;
                rest = after.trim_start();
            } else if let Some(after) = rest.strip_prefix("abstract ") {
                m.is_abstract = true;
                rest = after.trim_start();
            } else {
                break;
            }
        }
        if let Some(after) = rest.strip_prefix('/') {
            m.derived = true;
            rest = after;
        }
        let name_end = rest.find(['(', ':', '[', '=', '{']).unwrap_or(rest.len());
        m.name = rest[..name_end].trim().to_string();
        // A name is one word: a sentence is not a member.
        if m.name.is_empty() || m.name.contains(char::is_whitespace) {
            return None;
        }
        rest = rest[name_end..].trim_start();
        if rest.starts_with('(') {
            let end = closing(rest, '(', ')')?;
            m.params = Some(rest[1..end].to_string());
            rest = rest[end + 1..].trim_start();
        }
        if let Some(after) = rest.strip_prefix(':') {
            let end = Member::type_end(after);
            // A colon with no type is not notation (`Responsibilities:`).
            m.ty = Some(after[..end].trim().to_string()).filter(|t| !t.is_empty());
            m.ty.as_ref()?;
            rest = after[end..].trim_start();
        }
        // `[0..*]` is a multiplicity; `[]` is part of a type (`int[]`), which `type_end` kept.
        if rest.starts_with('[') {
            let end = closing(rest, '[', ']')?;
            m.multiplicity = Some(rest[1..end].trim().to_string()).filter(|t| !t.is_empty());
            rest = rest[end + 1..].trim_start();
        }
        if let Some(after) = rest.strip_prefix('=') {
            let end = after.rfind('{').filter(|at| after.trim_end().ends_with('}') && !after[..*at].trim().is_empty()).unwrap_or(after.len());
            m.default = Some(after[..end].trim().to_string());
            rest = after[end..].trim_start();
        }
        if rest.starts_with('{') && rest.ends_with('}') && closing(rest, '{', '}') == Some(rest.len() - 1) {
            for prop in rest[1..rest.len() - 1].split(',').map(str::trim).filter(|p| !p.is_empty()) {
                match prop {
                    "static" => m.is_static = true,
                    "abstract" => m.is_abstract = true,
                    other => m.props.push(other.to_string()),
                }
            }
            rest = "";
        }
        rest.is_empty().then_some(m)
    }

    /// Where a type ends in `Map<String, List<T>>[0..*] = x`: at a `[` that opens a multiplicity
    /// (not `[]`), an `=` or a `{` outside the brackets `<>` and `()`.
    fn type_end(s: &str) -> usize {
        let mut depth = 0i32;
        let mut chars = s.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            match c {
                '<' | '(' => depth += 1,
                '>' | ')' => depth -= 1,
                '[' if depth <= 0 && chars.peek().is_none_or(|(_, n)| *n != ']') => return i,
                '=' | '{' if depth <= 0 => return i,
                _ => {}
            }
        }
        s.len()
    }

    /// Whether this is an operation.
    #[cfg(test)]
    pub fn is_operation(&self) -> bool {
        self.params.is_some()
    }

    fn body(&self) -> String {
        let mut out = String::new();
        if self.derived {
            out.push('/');
        }
        out += &self.name;
        if let Some(params) = &self.params {
            out += &format!("({params})");
        }
        match (&self.ty, &self.multiplicity) {
            (Some(ty), None) => out += &format!(": {ty}"),
            (Some(ty), Some(m)) => out += &format!(": {ty}[{m}]"),
            (None, Some(m)) => out += &format!("[{m}]"),
            (None, None) => {}
        }
        if let Some(default) = &self.default {
            out += &format!(" = {default}");
        }
        if !self.props.is_empty() {
            out += &format!(" {{{}}}", self.props.join(", "));
        }
        out
    }

    /// The line as it is shown: the visibility and the notation, without the keywords `static` and
    /// `abstract`, which are drawn as a style.
    pub fn shown(&self) -> String {
        match self.visibility {
            Some(v) => format!("{} {}", v.sign(), self.body()),
            None => self.body(),
        }
    }

    /// The line in its one canonical spelling, keywords included: what `parse` reads back to the same member.
    #[cfg(test)]
    pub fn written(&self) -> String {
        let mut out = String::new();
        if let Some(v) = self.visibility {
            out.push(v.sign());
            out.push(' ');
        }
        if self.is_static {
            out += "static ";
        }
        if self.is_abstract {
            out += "abstract ";
        }
        out + &self.body()
    }
}

/// The name of a stereotype line: `«interface»` or `<<interface>>`.
pub fn stereotype(line: &str) -> Option<&str> {
    let line = line.trim();
    let inner = line.strip_prefix('«').and_then(|l| l.strip_suffix('»')).or_else(|| line.strip_prefix("<<")?.strip_suffix(">>"))?;
    Some(inner.trim()).filter(|name| !name.is_empty())
}

/// An *object* is written `name : Type` (a class name has no single colon).
fn is_object(name: &str) -> bool {
    let mut colons = name.match_indices(':').filter(|(i, _)| !name[..*i].ends_with(':') && !name[i + 1..].starts_with(':'));
    colons.next().is_some_and(|(i, _)| !name[..i].trim().is_empty() && !name[i + 1..].trim().is_empty())
}

/// A title line of a compartment after the second: `Responsibilities:`.
fn is_title(line: &str) -> bool {
    let line = line.trim();
    line.strip_suffix(':').is_some_and(|name| !name.is_empty() && !name.contains(':'))
}

/// Puts a style on every run of a line.
fn styled(mut spans: Vec<Span>, italic: bool, under: bool, bold: bool) -> Vec<Span> {
    for span in &mut spans {
        span.style.italic |= italic;
        span.style.under |= under;
        span.style.bold |= bold;
    }
    spans
}

/// The runs of each line of a box whose text has compartments (`---` lines), from its text, or
/// `None` when it has none and is not a class: the lines of the text, not of the drawing.
///
/// The stereotype is not bold; the name is bold, in italics if the class is abstract (`«abstract»`
/// or `{abstract}`), underlined if it is an object (`name : Type`); in the other compartments a
/// `static` member is underlined, an `abstract` one in italics.
pub fn class_runs(text: &str, rule: &str) -> Option<Vec<Vec<Span>>> {
    let lines: Vec<&str> = text.split('\n').collect();
    if !lines.iter().any(|l| l.trim() == rule) {
        return None;
    }
    let first_rule = lines.iter().position(|l| l.trim() == rule).unwrap_or(lines.len());
    let head = &lines[..first_rule];
    let stereotypes: Vec<&str> = head.iter().filter_map(|l| stereotype(l)).collect();
    let abstract_class = stereotypes.contains(&"abstract") || head.iter().any(|l| l.trim_end().ends_with("{abstract}"));
    let enumeration = stereotypes.contains(&"enumeration");
    let mut compartment = 0;
    let mut at_top = true;
    let runs = lines
        .iter()
        .map(|line| {
            if line.trim() == rule {
                compartment += 1;
                at_top = true;
                return richtext::plain(line, false);
            }
            let top = std::mem::take(&mut at_top);
            if compartment == 0 {
                return match stereotype(line) {
                    Some(name) => richtext::spans(&format!("«{name}»"), false),
                    None => {
                        let name = line.trim_end().trim_end_matches("{abstract}").trim_end();
                        styled(richtext::spans(name, true), abstract_class, is_object(name), true)
                    }
                };
            }
            if enumeration {
                return richtext::spans(line, false);
            }
            if compartment >= 2 && top && is_title(line) {
                return styled(richtext::spans(line.trim().trim_end_matches(':'), false), true, false, true);
            }
            match Member::parse(line) {
                Some(m) => styled(richtext::spans(&m.shown(), false), m.is_abstract, m.is_static, false),
                None => richtext::spans(line, false),
            }
        })
        .collect();
    Some(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(line: &str) -> Member {
        Member::parse(line).unwrap_or_else(|| panic!("{line:?} is a member"))
    }

    #[test]
    fn reads_attributes_and_operations() {
        let m = member("- /total: Map<String, List<T>>[0..*] = {1, 2} {readOnly, ordered}");
        assert_eq!((m.visibility, m.derived, m.name.as_str(), m.params.as_ref()), (Some(Visibility::Private), true, "total", None));
        assert_eq!((m.ty.as_deref(), m.multiplicity.as_deref(), m.default.as_deref()), (Some("Map<String, List<T>>"), Some("0..*"), Some("{1, 2}")));
        assert_eq!(m.props, ["readOnly", "ordered"]);
        let m = member("# static abstract area(width: int = 2, h: List<T>): f32 {query}");
        assert_eq!((m.visibility, m.is_static, m.is_abstract, m.is_operation()), (Some(Visibility::Protected), true, true, true));
        assert_eq!((m.params.as_deref(), m.ty.as_deref(), m.props.as_slice()), (Some("width: int = 2, h: List<T>"), Some("f32"), &["query".to_string()][..]));
        // The properties `static` and `abstract` are styles too; `[]` is an array, not a multiplicity.
        let m = member("~ items: int[] {static}");
        assert_eq!((m.visibility, m.ty.as_deref(), m.multiplicity.as_deref(), m.is_static, m.props.is_empty()), (Some(Visibility::Package), Some("int[]"), None, true, true));
        // Nothing but a name is a member (an attribute with no type); the sign can be missing.
        assert_eq!((member("count").name.as_str(), member("count").visibility), ("count", None));
        assert_eq!(member("+ draw()").shown(), "+ draw()");
        // What is not notation is not a member: it is drawn as typed.
        for text in ["", "   ", "+", "+ (x)", "see the note: (a", "a(b", "a: b[1", "x {oops"] {
            assert!(Member::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn hides_the_keywords_and_keeps_the_rest() {
        let m = member("+ static abstract /x: int {static}");
        assert_eq!(m.shown(), "+ /x: int");
        assert_eq!(m.written(), "+ static abstract /x: int");
        assert_eq!(member("  -   count  :   int  =  3  ").written(), "- count: int = 3");
    }

    /// A small deterministic generator, for lines nobody wrote by hand.
    struct Dice(u64);
    impl Dice {
        fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            of[(self.0 >> 33) as usize % of.len()]
        }
    }

    #[test]
    fn round_trips_a_hundred_lines() {
        let mut dice = Dice(7);
        let names = ["x", "solde", "Ünï", "日本語", "名前😀", "a|b", "a\\b", "snake_case", "veryLong".repeat(40).leak(), "é", "n1"];
        let types = ["int", "List<T>", "Map<String, List<Ünï>>", "日本", "int[]", "(a, b)", "Σ", "T"];
        let mults = ["", "[0..*]", "[1]", "[*]", "[2..5]"];
        let defaults = ["", " = 0", " = \"a|b\"", " = {1, 2}", " = 日本", " = x == y"];
        let props = ["", " {readOnly}", " {ordered, unique}"];
        let mut lines = Vec::new();
        for _ in 0..100 {
            let vis = dice.pick(&["", "+ ", "- ", "# ", "~ "]);
            let keywords = dice.pick(&["", "static ", "abstract ", "static abstract "]);
            let derived = dice.pick(&["", "/"]);
            let name = dice.pick(&names);
            let op = dice.pick(&["", "()", "(a: int)", "(a: int, b: List<T>)", "(日本: Σ = 2)"]);
            let ty = dice.pick(&types);
            let tail = if dice.pick(&["a", "b"]) == "a" { format!(": {ty}{}", dice.pick(&mults)) } else { String::new() };
            lines.push(format!("{vis}{keywords}{derived}{name}{op}{tail}{}{}", dice.pick(&defaults), dice.pick(&props)));
        }
        assert_eq!(lines.len(), 100);
        for line in &lines {
            let m = Member::parse(line).unwrap_or_else(|| panic!("{line:?} is a member"));
            assert_eq!(&m.written(), line, "{line:?}");
            // And what is shown is the same line, without the keywords.
            let bare = line.replacen("static ", "", 1).replacen("abstract ", "", 1);
            assert_eq!(m.shown(), bare.replacen("static ", "", 1).replacen("abstract ", "", 1), "{line:?}");
        }
    }

    #[test]
    fn never_panics_whatever_the_line() {
        // Tens of thousands of strange lines: no panic, and a member always writes back to a member.
        let mut dice = Dice(99);
        let bits = ["+", "-", "#", "~", " ", "static ", "abstract ", "/", "(", ")", "[", "]", "{", "}", ":", "=", ",", "x", "日本", "é", "😀", "<", ">", "|", "\\", "«", "»", "0..*", "a b"];
        for _ in 0..30_000 {
            let line: String = (0..dice.pick(&["1", "3", "6", "12"]).parse().unwrap()).map(|_| dice.pick(&bits)).collect();
            if let Some(m) = Member::parse(&line) {
                assert!(Member::parse(&m.written()).is_some(), "{line:?} -> {:?}", m.written());
            }
        }
    }

    #[test]
    fn reads_stereotypes_names_and_objects() {
        assert_eq!((stereotype("«interface»"), stereotype("<<enumeration>>"), stereotype(" « a b » ")), (Some("interface"), Some("enumeration"), Some("a b")));
        assert_eq!((stereotype("«»"), stereotype("<<>>"), stereotype("Name"), stereotype("«open")), (None, None, None, None));
        assert!(is_object("order : Order") && is_object("a:B") && !is_object("Order") && !is_object("pkg::Order") && !is_object(": B") && !is_object("a :"));
    }

    fn describe(runs: &[Span]) -> String {
        runs.iter()
            .map(|s| {
                let st = s.style;
                format!("{}{}{}{}", s.text, if st.bold { "·b" } else { "" }, if st.italic { "·i" } else { "" }, if st.under { "·u" } else { "" })
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn styles_the_compartments() {
        let text = "«abstract»\nShape\n---\n+ static count: int\n# abstract name()\n+ plain: int\n---\n- helper(x)\nResponsibilities:\n+ other";
        let runs = class_runs(text, "---").unwrap();
        let shown: Vec<String> = runs.iter().map(|r| describe(r)).collect();
        assert_eq!(shown[0], "«abstract»");
        assert_eq!(shown[1], "Shape·b·i");
        assert_eq!(shown[3], "+ count: int·u");
        assert_eq!(shown[4], "# name()·i");
        assert_eq!(shown[5], "+ plain: int");
        assert_eq!(shown[7], "- helper(x)");
        // A title of a later compartment, only when it opens it; the colon is dropped.
        assert_eq!(shown[8], "Responsibilities:");
        let runs = class_runs("A\n---\n+ x\n---\nResponsibilities:\n- to be", "---").unwrap();
        assert_eq!(describe(&runs[4]), "Responsibilities·b·i");
        // An object is underlined; an enumeration's literals are not members.
        let runs = class_runs("order : Order\n---\n+ total = 3", "---").unwrap();
        assert_eq!(describe(&runs[0]), "order : Order·b·u");
        let runs = class_runs("«enumeration»\nColor\n---\nRED\n+ static GREEN", "---").unwrap();
        assert_eq!((describe(&runs[3]), describe(&runs[4])), ("RED".into(), "+ static GREEN".into()));
        // `{abstract}` after the name does the same as the stereotype, and is not drawn.
        let runs = class_runs("Shape {abstract}\n---\n", "---").unwrap();
        assert_eq!(describe(&runs[0]), "Shape·b·i");
        // The marks of notes still work in the text, and a box with no compartment is no class.
        let runs = class_runs("A\n---\n+ **x**: int", "---").unwrap();
        assert_eq!(describe(&runs[2]), "+ |x·b|: int");
        assert!(class_runs("Just a box", "---").is_none() && class_runs("", "---").is_none());
    }
}
