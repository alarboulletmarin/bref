//! Analyse Markdown ligne par ligne : fonctions pures, sans dépendance à l'UI.

use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Para,
    Heading(u8),
    Bullet,
    Ordered,
    Task(bool),
    Quote,
    Fence,
    Code,
    Rule,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Link {
    Wiki(String),
    Url(String),
    Tag(String),
}

// Styles en ligne, un mot de drapeaux par octet de texte.
pub const BOLD: u16 = 1;
pub const ITALIC: u16 = 2;
pub const CODE: u16 = 4;
pub const STRIKE: u16 = 8;
pub const DIM: u16 = 16;
pub const LINK: u16 = 32;
pub const TAG: u16 = 64;
pub const MARK: u16 = 128;
// Coloration des blocs de code.
pub const KEYWORD: u16 = 256;
pub const STRING: u16 = 512;
pub const NUMBER: u16 = 1024;

pub const INDENT: &str = "    ";

fn indent_len(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

pub fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

/// Type de la ligne et longueur (en octets) de son marqueur, indentation comprise.
pub fn classify(line: &str, in_code: bool) -> (Kind, usize) {
    let indent = indent_len(line);
    let rest = &line[indent..];
    if rest.starts_with("```") {
        return (Kind::Fence, line.len());
    }
    if in_code {
        return (Kind::Code, 0);
    }
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        return (Kind::Heading(hashes.min(3) as u8), indent + hashes + 1);
    }
    let t = rest.trim_end();
    if t.len() >= 3 && ['-', '*', '_'].iter().any(|&c| t.chars().all(|x| x == c)) {
        return (Kind::Rule, line.len());
    }
    if rest == ">" || rest.starts_with("> ") {
        return (Kind::Quote, indent + rest.len().min(2));
    }
    let b = rest.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' {
        let after = &rest[2..];
        if after.starts_with("[ ] ") {
            return (Kind::Task(false), indent + 6);
        }
        if after.starts_with("[x] ") || after.starts_with("[X] ") {
            return (Kind::Task(true), indent + 6);
        }
        return (Kind::Bullet, indent + 2);
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && (rest[digits..].starts_with(". ") || rest[digits..].starts_with(") "))
    {
        return (Kind::Ordered, indent + digits + 2);
    }
    (Kind::Para, 0)
}

/// Wikiliens, URL et tags de la ligne, avec leur étendue en octets.
pub fn links(line: &str) -> Vec<(Range<usize>, Link)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let rest = &line[i..];
        if rest.starts_with("[[")
            && let Some(e) = rest.find("]]")
        {
            let target = rest[2..e].split(['|', '#']).next().unwrap_or("").trim();
            if !target.is_empty() {
                out.push((i..i + e + 2, Link::Wiki(target.to_string())));
            }
            i += e + 2;
            continue;
        }
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ')')
                .unwrap_or(rest.len());
            out.push((i..i + end, Link::Url(rest[..end].to_string())));
            i += end;
            continue;
        }
        if b[i] == b'#' && (i == 0 || b[i - 1].is_ascii_whitespace()) {
            let n: usize = rest[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .map(char::len_utf8)
                .sum();
            let tag = &rest[1..1 + n];
            if tag.chars().any(|c| !c.is_ascii_digit()) {
                out.push((i..i + 1 + n, Link::Tag(tag.to_lowercase())));
                i += 1 + n;
                continue;
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// Remplit `flags` avec les styles en ligne de `line` à partir de l'octet `from`.
pub fn inline(line: &str, from: usize, flags: &mut [u16]) {
    let b = line.as_bytes();
    let alnum = |i: Option<usize>| i.and_then(|i| b.get(i)).is_some_and(u8::is_ascii_alphanumeric);
    let mut active = 0u16;
    let mut i = from;
    while i < b.len() {
        let rest = &line[i..];
        if b[i] == b'`'
            && let Some(end) = rest[1..].find('`')
        {
            flags[i] |= DIM;
            flags[i + 1 + end] |= DIM;
            flags[i + 1..i + 1 + end].iter_mut().for_each(|f| *f |= CODE);
            i += end + 2;
            continue;
        }
        let (n, flag) = if rest.starts_with("**") {
            (2, BOLD)
        } else if rest.starts_with("~~") {
            (2, STRIKE)
        } else if b[i] == b'*' || b[i] == b'_' {
            (1, ITALIC)
        } else {
            (0, 0)
        };
        if flag != 0 {
            let underscore = b[i] == b'_';
            let ok = if active & flag == 0 {
                rest[n..].chars().next().is_some_and(|c| !c.is_whitespace())
                    && rest[n..].contains(&rest[..n])
                    && !(underscore && alnum(i.checked_sub(1)))
            } else {
                !(underscore && alnum(Some(i + 1)))
            };
            if ok {
                flags[i..i + n].iter_mut().for_each(|f| *f |= DIM | active);
                active ^= flag;
                i += n;
                continue;
            }
        }
        let n = rest.chars().next().map_or(1, char::len_utf8);
        flags[i..i + n].iter_mut().for_each(|f| *f |= active);
        i += n;
    }
    for (r, link) in links(line) {
        if r.start < from {
            continue;
        }
        match link {
            Link::Tag(_) => flags[r].iter_mut().for_each(|f| *f |= TAG),
            Link::Url(_) => flags[r].iter_mut().for_each(|f| *f |= LINK),
            Link::Wiki(_) => {
                flags[r.start..r.start + 2].iter_mut().for_each(|f| *f |= DIM);
                flags[r.end - 2..r.end].iter_mut().for_each(|f| *f |= DIM);
                flags[r.start + 2..r.end - 2].iter_mut().for_each(|f| *f |= LINK);
            }
        }
    }
}

const KEYWORDS: &[&str] = &[
    "False", "None", "Self", "True", "and", "as", "async", "await", "break", "case", "catch",
    "class", "const", "continue", "def", "default", "defer", "do", "done", "elif", "else", "end",
    "enum", "except", "export", "extends", "false", "fi", "final", "finally", "fn", "for", "from",
    "func", "function", "go", "if", "impl", "import", "in", "interface", "is", "lambda", "let",
    "loop", "match", "mod", "mut", "namespace", "new", "nil", "not", "null", "or", "package",
    "private", "protected", "pub", "public", "return", "self", "static", "struct", "super",
    "switch", "then", "this", "throw", "trait", "true", "try", "type", "use", "using", "var",
    "void", "where", "while", "with", "yield",
];

/// Colore une ligne de code : commentaires, chaînes, nombres et mots-clés.
// ponytail: un seul lexeur ligne à ligne pour tous les langages, sans état d'une
// ligne à l'autre (commentaires `/* */` et chaînes sur plusieurs lignes ignorés).
// Passer à une grammaire par langage (tree-sitter) si la coloration devient fausse.
pub fn code(line: &str, lang: &str, flags: &mut [u16]) {
    let comment = match lang {
        "py" | "python" | "sh" | "bash" | "zsh" | "fish" | "shell" | "yaml" | "yml" | "toml"
        | "rb" | "ruby" | "r" | "perl" | "make" | "makefile" | "dockerfile" | "conf" | "ini"
        | "nix" | "elixir" => "#",
        "sql" | "lua" | "haskell" | "hs" | "elm" | "ada" => "--",
        _ => "//",
    };
    let rust = matches!(lang, "rs" | "rust");
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        if rest.starts_with(comment) {
            flags[i..].fill(DIM | ITALIC);
            return;
        }
        let c = rest.chars().next().unwrap();
        let (n, flag) = if matches!(c, '"' | '\'' | '`') {
            let mut end = None;
            let mut escaped = false;
            for (j, x) in rest.char_indices().skip(1) {
                if !escaped && x == c {
                    end = Some(j + 1);
                    break;
                }
                escaped = !escaped && x == '\\';
            }
            match end {
                // En Rust, `'a` est une durée de vie : seul `'x'` ou `'\n'` est un caractère.
                Some(n) if rust && c == '\'' && rest[..n].chars().count() > 3 && !rest[1..].starts_with('\\') => {
                    (1, 0)
                }
                Some(n) => (n, STRING),
                None if c == '"' => (rest.len(), STRING),
                None => (1, 0),
            }
        } else if c.is_alphabetic() || c == '_' {
            let n = rest.find(|x: char| !x.is_alphanumeric() && x != '_').unwrap_or(rest.len());
            (n, if KEYWORDS.contains(&&rest[..n]) { KEYWORD } else { 0 })
        } else if c.is_ascii_digit() {
            let n = rest
                .find(|x: char| !x.is_ascii_alphanumeric() && x != '.' && x != '_')
                .unwrap_or(rest.len());
            (n, NUMBER)
        } else {
            (c.len_utf8(), 0)
        };
        flags[i..i + n].fill(flag);
        i += n;
    }
}

/// Contenu (clôtures exclues) du bloc de code qui contient l'octet `at`. Un
/// ``` sans clôture n'ouvre pas de bloc.
pub fn code_block(text: &str, at: usize) -> Option<Range<usize>> {
    let mut open = None;
    let mut offset = 0;
    for line in text.split('\n') {
        if is_fence(line) {
            match open.take() {
                Some(start) if (start..offset).contains(&at) => return Some(start..offset - 1),
                Some(_) => {}
                None => open = Some(offset + line.len() + 1),
            }
        }
        offset += line.len() + 1;
    }
    None
}

const SYMBOLS: &[(&str, &str)] = &[
    ("<->", "↔"),
    ("<=>", "⇔"),
    ("->", "→"),
    ("<-", "←"),
    ("=>", "⇒"),
    ("!=", "≠"),
    ("<=", "≤"),
    (">=", "≥"),
];

/// Signes à afficher à la place de leur écriture ASCII (`->` devient `→`), hors
/// code et liens : (octet de début, longueur remplacée, signe). Le texte ne change pas.
pub fn symbols(line: &str, from: usize, flags: &[u16]) -> Vec<(usize, usize, &'static str)> {
    let b = line.as_bytes();
    // Un signe collé à un autre opérateur (`-->`, `!==`, `<--`) reste tel quel.
    let glued = |i: Option<usize>| i.and_then(|i| b.get(i)).is_some_and(|c| b"-=<>!".contains(c));
    let mut out = Vec::new();
    let mut i = from;
    while i < b.len() {
        let found = SYMBOLS.iter().find(|(ascii, _)| {
            b[i..].starts_with(ascii.as_bytes())
                && flags[i..i + ascii.len()].iter().all(|f| f & (CODE | LINK) == 0)
                && !glued(i.checked_sub(1))
                && !glued(Some(i + ascii.len()))
        });
        match found {
            Some((ascii, sign)) => {
                out.push((i, ascii.len(), *sign));
                i += ascii.len();
            }
            None => i += 1,
        }
    }
    out
}

/// Première image de la ligne, `![alt](chemin)` ou `![[chemin]]` : son étendue
/// et son chemin.
pub fn image(line: &str) -> Option<(Range<usize>, String)> {
    let start = line.find("![")?;
    let rest = &line[start + 2..];
    let (len, path) = if let Some(inner) = rest.strip_prefix('[') {
        let end = inner.find("]]")?;
        (end + 5, inner[..end].split('|').next()?)
    } else {
        let open = rest.find("](")?;
        let end = open + rest[open..].find(')')?;
        (end + 3, rest[open + 2..end].split(" \"").next()?)
    };
    let path = path.trim().replace("%20", " ");
    (!path.is_empty()).then(|| (start..start + len, path))
}

/// Première formule de la ligne, `$$…$$` ou `$…$` : son étendue, son source, et
/// si elle est hors texte (`$$`).
pub fn math(line: &str) -> Option<(Range<usize>, &str, bool)> {
    let mut from = 0;
    while let Some(i) = line[from..].find('$') {
        let start = from + i;
        let n = if line[start..].starts_with("$$") { 2 } else { 1 };
        let body = &line[start + n..];
        if let Some(len) = body.find(&"$$"[..n])
            && len > 0
            // Comme pandoc : un `$` collé à la formule, et pas suivi d'un chiffre,
            // pour laisser « 5 $ et 10 $ » ou « $5 et $10 » tranquilles.
            && (n == 2
                || !(body.starts_with(' ')
                    || body[..len].ends_with(' ')
                    || body[len + 1..].starts_with(|c: char| c.is_ascii_digit())))
        {
            return Some((start..start + len + 2 * n, body[..len].trim(), n == 2));
        }
        from = start + n;
    }
    None
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Tags et wikiliens (en minuscules) d'une note entière, hors blocs de code, dédupliqués.
/// Les images affichées y figurent par leur nom de fichier.
pub fn index(text: &str) -> (Vec<String>, Vec<String>) {
    let (mut tags, mut wikis) = (Vec::new(), Vec::new());
    let mut in_code = false;
    for line in text.lines() {
        if is_fence(line) {
            in_code = !in_code;
        } else if !in_code {
            for (_, link) in links(line) {
                let (list, item) = match link {
                    Link::Tag(t) => (&mut tags, t),
                    Link::Wiki(w) => (&mut wikis, w.to_lowercase()),
                    Link::Url(_) => continue,
                };
                if !list.contains(&item) {
                    list.push(item);
                }
            }
            // Une image affichée compte comme un lien vers son fichier.
            if let Some((_, path)) = image(line) {
                let name = file_name(&path).to_lowercase();
                if !wikis.contains(&name) {
                    wikis.push(name);
                }
            }
        }
    }
    (tags, wikis)
}

/// Le texte où les wikiliens vers `old` visent `new` (alias et ancre conservés,
/// blocs de code laissés tels quels) ; `None` si aucun lien ne change.
pub fn relink(text: &str, old: &str, new: &str) -> Option<String> {
    let old = old.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    for line in text.split_inclusive('\n') {
        let mut done = 0;
        if is_fence(line) {
            in_code = !in_code;
        } else if !in_code {
            for (range, link) in links(line) {
                if matches!(link, Link::Wiki(target) if target.to_lowercase() == old) {
                    // `[[cible|alias]]` ou `[[cible#ancre]]` : seule la cible change.
                    let inner = &line[range.start + 2..range.end - 2];
                    let rest = inner.find(['|', '#']).map_or("", |i| &inner[i..]);
                    out.push_str(&line[done..range.start + 2]);
                    out.push_str(new);
                    out.push_str(rest);
                    done = range.end - 2;
                }
            }
        }
        out.push_str(&line[done..]);
    }
    let out = reembed(&out, &old, new);
    (out != text).then_some(out)
}

/// Le texte où les images `![…](dossier/old)` désignent le fichier `new`.
fn reembed(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    for line in text.split_inclusive('\n') {
        if is_fence(line) {
            in_code = !in_code;
        } else if !in_code
            && let Some((range, path)) = image(line)
            && !line[range.clone()].starts_with("![[")
            && file_name(&path).to_lowercase() == old
        {
            // Le nom tel qu'il est écrit : avec ses espaces, ou en `%20`.
            let name = file_name(&path);
            let spot = [name.to_string(), name.replace(' ', "%20")]
                .into_iter()
                .find_map(|written| Some((line[range.clone()].rfind(&written)?, written.len())));
            if let Some((at, len)) = spot {
                out.push_str(&line[..range.start + at]);
                out.push_str(&new.replace(' ', "%20"));
                out.push_str(&line[range.start + at + len..]);
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

#[derive(PartialEq, Debug)]
pub enum Enter {
    /// Texte à insérer au curseur.
    Insert(String),
    /// Item vide : on vide la ligne pour sortir de la liste.
    Clear,
}

/// Comportement de la touche Entrée sur `line`, curseur à l'octet `col`.
pub fn on_enter(line: &str, col: usize, in_code: bool) -> Enter {
    let (kind, marker) = classify(line, in_code);
    let indent = &line[..indent_len(line)];
    let list = matches!(kind, Kind::Bullet | Kind::Ordered | Kind::Task(_) | Kind::Quote);
    if !list || col < marker {
        let keep = if col >= indent.len() { indent } else { "" };
        return Enter::Insert(format!("\n{keep}"));
    }
    if line[marker..].trim().is_empty() {
        return Enter::Clear;
    }
    let m = &line[indent.len()..marker];
    let next = match kind {
        Kind::Task(_) => format!("{}[ ] ", &m[..2]),
        Kind::Ordered => {
            let digits = m.len() - 2;
            let n: u64 = m[..digits].parse().unwrap_or(0);
            format!("{}{}", n + 1, &m[digits..])
        }
        _ => m.to_string(),
    };
    Enter::Insert(format!("\n{indent}{next}"))
}

/// Renumérote la liste ordonnée autour du curseur. Renvoie le nouveau texte et
/// le curseur ajusté, ou `None` si rien ne change.
pub fn renumber(text: &str, cursor: usize) -> Option<(String, usize)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut starts = Vec::with_capacity(lines.len());
    let mut off = 0;
    for l in &lines {
        starts.push(off);
        off += l.len() + 1;
    }
    let cur = starts.partition_point(|&s| s <= cursor).saturating_sub(1);
    let ordered = |i: usize| classify(lines[i], false).0 == Kind::Ordered;
    // Après la suppression d'un item, la liste à corriger commence sous le curseur.
    let anchor = [cur, cur + 1]
        .into_iter()
        .find(|&i| i < lines.len() && ordered(i))?;
    let level = indent_len(lines[anchor]);
    let member = |i: usize| {
        let l = lines[i];
        (ordered(i) && indent_len(l) == level) || (!l.trim().is_empty() && indent_len(l) > level)
    };
    let mut first = anchor;
    while first > 0 && member(first - 1) {
        first -= 1;
    }
    let mut last = anchor;
    while last + 1 < lines.len() && member(last + 1) {
        last += 1;
    }

    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mut new_cursor = cursor as isize;
    let mut number = None;
    let mut changed = false;
    for i in first..=last {
        let l = lines[i];
        if !(ordered(i) && indent_len(l) == level) {
            continue;
        }
        let digits = l[level..].bytes().take_while(u8::is_ascii_digit).count();
        let n = match number {
            None => l[level..level + digits].parse::<u64>().unwrap_or(1),
            Some(prev) => prev + 1,
        };
        number = Some(n);
        let fixed = format!("{}{}{}", &l[..level], n, &l[level + digits..]);
        if fixed != l {
            if cursor > starts[i] + level {
                new_cursor += fixed.len() as isize - l.len() as isize;
            }
            out[i] = fixed;
            changed = true;
        }
    }
    changed.then(|| (out.join("\n"), new_cursor.max(0) as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_lines() {
        assert_eq!(classify("# Titre", false), (Kind::Heading(1), 2));
        assert_eq!(classify("### T", false), (Kind::Heading(3), 4));
        assert_eq!(classify("#tag", false), (Kind::Para, 0));
        assert_eq!(classify("- a", false), (Kind::Bullet, 2));
        assert_eq!(classify("    * a", false), (Kind::Bullet, 6));
        assert_eq!(classify("- [ ] a", false), (Kind::Task(false), 6));
        assert_eq!(classify("- [x] a", false), (Kind::Task(true), 6));
        assert_eq!(classify("12. a", false), (Kind::Ordered, 4));
        assert_eq!(classify("> q", false), (Kind::Quote, 2));
        assert_eq!(classify("---", false), (Kind::Rule, 3));
        assert_eq!(classify("```rust", false), (Kind::Fence, 7));
        assert_eq!(classify("- a", true), (Kind::Code, 0));
        assert_eq!(classify("texte", false), (Kind::Para, 0));
    }

    #[test]
    fn finds_links() {
        let l = links("voir [[Ma note|alias]] et #projet/x sur https://a.b/c).");
        assert_eq!(l[0].1, Link::Wiki("Ma note".into()));
        assert_eq!(l[1].1, Link::Tag("projet/x".into()));
        assert_eq!(l[2].1, Link::Url("https://a.b/c".into()));
        assert!(links("a#b et #123").is_empty());
        let (tags, wikis) = index("#A\n```\n#b [[z]]\n```\n#a #c [[X]] [[x|alias]] [[Y]]");
        assert_eq!(tags, vec!["a", "c"]);
        assert_eq!(wikis, vec!["x", "y"]);
    }

    #[test]
    fn relinks_renamed_notes() {
        let text = "[[Test]] et [[test|alias]], [[ Test#titre]]\n```\n[[Test]]\n```\n[[Testé]] [[Test]]";
        let want = "[[Essai]] et [[Essai|alias]], [[Essai#titre]]\n```\n[[Test]]\n```\n[[Testé]] [[Essai]]";
        assert_eq!(relink(text, "Test", "Essai").as_deref(), Some(want));
        assert_eq!(relink(text, "Autre", "Essai"), None);
        // Une image renommée : les deux écritures suivent, le reste de la ligne aussi.
        let text = "![a](img/Le%20chat.png \"t\") ![[le chat.png|300]] le chat.png";
        let want = "![a](img/Un%20chien.png \"t\") ![[Un chien.png|300]] le chat.png";
        assert_eq!(relink(text, "Le chat.png", "Un chien.png").as_deref(), Some(want));
        assert_eq!(index("![x](img/Le%20chat.png)\n![[B.svg]]").1, ["le chat.png", "b.svg"]);
    }

    #[test]
    fn styles_inline() {
        let line = "a **b** `c*` snake_case_x *é*";
        let mut f = vec![0u16; line.len()];
        inline(line, 0, &mut f);
        assert_eq!(f[2], DIM);
        assert_eq!(f[4], BOLD);
        assert_eq!(f[5], DIM | BOLD);
        assert_eq!(f[9], CODE);
        assert_eq!(f[10], CODE);
        assert!(f[13..25].iter().all(|&x| x == 0));
        assert_eq!(f[27], ITALIC);
        assert_eq!(f[28], ITALIC);
    }

    #[test]
    fn colors_code() {
        let line = r#"let x = f("a // b", 42); // fin"#;
        let mut f = vec![0u16; line.len()];
        code(line, "rust", &mut f);
        assert_eq!((f[0], f[4], f[8]), (KEYWORD, 0, 0));
        assert!(f[10..18].iter().all(|&x| x == STRING));
        assert_eq!((f[20], f[25]), (NUMBER, DIM | ITALIC));
        // Une durée de vie n'est pas une chaîne ; `#` commente en Python, pas en Rust.
        let line = "fn f<'a>(x: &'a str) # '\\n'";
        let mut f = vec![0u16; line.len()];
        code(line, "rust", &mut f);
        assert!(f[3..21].iter().all(|&x| x == 0) && f[23] == STRING);
        code(line, "python", &mut f);
        assert_eq!(f[21], DIM | ITALIC);
    }

    #[test]
    fn finds_code_blocks() {
        let text = "a\n```rust\nun\ndeux\n```\nb\n```\nouvert";
        assert_eq!(code_block(text, 10).map(|r| &text[r]), Some("un\ndeux"));
        assert_eq!(code_block(text, 16).map(|r| &text[r]), Some("un\ndeux"));
        // Hors bloc, sur une clôture, ou après un ``` jamais refermé.
        assert_eq!((code_block(text, 0), code_block(text, 3), code_block(text, 30)), (None, None, None));
    }

    #[test]
    fn finds_symbols_and_images() {
        let line = "a -> b != c `x -> y` <-> --> !== [[a->b]]";
        let mut f = vec![0u16; line.len()];
        inline(line, 0, &mut f);
        assert_eq!(symbols(line, 0, &f), vec![(2, 2, "→"), (7, 2, "≠"), (21, 3, "↔")]);
        assert_eq!(image("voir ![un chat](img/le%20chat.png \"titre\") ici"), Some((5..42, "img/le chat.png".into())));
        assert_eq!(image("![[photo.jpg|300]]"), Some((0..18, "photo.jpg".into())));
        assert_eq!((image("![]()"), image("[lien](a.png)")), (None, None));
        assert_eq!(math("soit $x^2$ et $y$"), Some((5..10, "x^2", false)));
        assert_eq!(math("$$ \\frac{1}{2} $$"), Some((0..17, "\\frac{1}{2}", true)));
        assert_eq!((math("$5 et $10"), math("5 $ et 10 $"), math("prix : 3$")), (None, None, None));
    }

    #[test]
    fn continues_lists() {
        assert_eq!(on_enter("- a", 3, false), Enter::Insert("\n- ".into()));
        assert_eq!(on_enter("  9. a", 6, false), Enter::Insert("\n  10. ".into()));
        assert_eq!(on_enter("- [x] a", 7, false), Enter::Insert("\n- [ ] ".into()));
        assert_eq!(on_enter("> a", 3, false), Enter::Insert("\n> ".into()));
        assert_eq!(on_enter("- ", 2, false), Enter::Clear);
        assert_eq!(on_enter("  x", 3, false), Enter::Insert("\n  ".into()));
        assert_eq!(on_enter("- a", 3, true), Enter::Insert("\n".into()));
    }

    #[test]
    fn renumbers_around_cursor() {
        let (t, c) = renumber("1. a\n1. b\n    - x\n5. c\n\n7. z", 9).unwrap();
        assert_eq!(t, "1. a\n2. b\n    - x\n3. c\n\n7. z");
        assert_eq!(c, 9);
        let (t, c) = renumber("9. a\n9. b", 9).unwrap();
        assert_eq!((t.as_str(), c), ("9. a\n10. b", 10));
        assert_eq!(renumber("1. a\n2. b", 0), None);
        assert_eq!(renumber("texte", 0), None);
    }
}
