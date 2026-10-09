//! Analyse Markdown ligne par ligne : fonctions pures, sans dépendance à l'UI.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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
    Table,
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

/// La ligne est une ligne de tableau : `classify(line, false)` la dit `Kind::Table`, sans le reste du travail.
pub fn is_table_line(line: &str) -> bool {
    line[indent_len(line)..].starts_with('|')
}

/// Nombre de lignes de clôture ``` et de lignes réduites à `$$` dans le texte, sans l'analyser ligne par ligne :
/// on cherche les motifs, qui sont rares, puis on regarde la ligne où chacun tombe.
pub fn count_fences_and_dollars(text: &str) -> (usize, usize) {
    let line_start = |at: usize| text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |at: usize| text[at..].find('\n').map_or(text.len(), |i| at + i);
    let (mut fences, mut last) = (0, usize::MAX);
    for (at, _) in text.match_indices("```") {
        let start = line_start(at);
        if start != last && text[start..at].chars().all(char::is_whitespace) {
            fences += 1;
            last = start;
        }
    }
    let (mut dollars, mut last) = (0, usize::MAX);
    for (at, _) in text.match_indices("$$") {
        let start = line_start(at);
        if start != last && text[start..line_end(at)].trim() == "$$" {
            dollars += 1;
            last = start;
        }
    }
    (fences, dollars)
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
    if rest.starts_with('|') {
        return (Kind::Table, 0);
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

/// Les `[ ]` et `[x]` de la ligne à partir de l'octet `from`, seuls entre des espaces, des `|` ou
/// les bords de la ligne : ni `[[ ]]`, ni `[x](lien)`, ni `a[ ]`.
fn boxes(line: &str, from: usize) -> impl Iterator<Item = Range<usize>> + '_ {
    let b = line.as_bytes();
    (from..b.len().saturating_sub(2)).filter_map(move |i| {
        let token = b[i] == b'[' && matches!(b[i + 1], b' ' | b'x' | b'X') && b[i + 2] == b']';
        let open = i == 0 || matches!(b[i - 1], b' ' | b'\t' | b'|');
        let shut = i + 3 == b.len() || matches!(b[i + 3], b' ' | b'\t' | b'|');
        (token && open && shut).then_some(i..i + 3)
    })
}

/// Les cases à cocher de la ligne, hors du code en ligne.
pub fn checkboxes(line: &str, from: usize) -> Vec<Range<usize>> {
    let mut flags = vec![0u16; line.len()];
    inline(line, from, &mut flags);
    boxes(line, from).filter(|r| flags[r.start] & CODE == 0).collect()
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
    // Une case à cocher `[ ]` ou `[x]` : colorée, en gras une fois cochée.
    for r in boxes(line, from) {
        if flags[r.start] & CODE == 0 {
            let checked = if b[r.start + 1] == b' ' { 0 } else { BOLD };
            flags[r.clone()].iter_mut().for_each(|f| *f |= MARK | checked);
        }
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

// ----- Panneaux et tableaux -----

/// Type du panneau qu'ouvre une citation `> [!NOTE]` (syntaxe de GitHub et d'Obsidian).
pub fn callout(line: &str) -> Option<Range<usize>> {
    let open = line.find('>')? + 1;
    let open = open + indent_len(&line[open..]);
    let close = line[open..].strip_prefix("[!")?.find(']')?;
    Some(open..open + close + 3)
}

/// Place de chaque `|` qui sépare deux cellules : pas ceux qui sont échappés ou
/// pris dans un `[[lien|alias]]`.
pub fn bars(line: &str) -> Vec<usize> {
    let b = line.as_bytes();
    let mut bars = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'[' if line[i..].starts_with("[[") => i += line[i..].find("]]").unwrap_or(0),
            b'|' => bars.push(i),
            _ => {}
        }
        i += 1;
    }
    bars
}

/// Étendue du contenu de chaque cellule de la ligne, sans les espaces autour.
pub fn cells(line: &str) -> Vec<Range<usize>> {
    let mut bars = bars(line);
    // Sans `|` final, ce qui suit le dernier est encore une cellule.
    if bars.last().is_some_and(|&last| !line[last + 1..].trim().is_empty()) {
        bars.push(line.len());
    }
    bars.windows(2)
        .map(|pair| {
            let cell = &line[pair[0] + 1..pair[1]];
            // Cellule vide : le curseur s'y place à un espace du `|`.
            let lead = if cell.trim().is_empty() { cell.len().min(1) } else { indent_len(cell) };
            let start = pair[0] + 1 + lead;
            start..start + cell.trim().len()
        })
        .collect()
}

pub fn is_dashes(row: &[String]) -> bool {
    !row.is_empty() && row.iter().all(|c| c.contains('-') && c.chars().all(|x| matches!(x, '-' | ':')))
}

/// Lignes d'un tableau (en-tête, tirets, puis le reste), toutes de même
/// longueur ; et si la ligne de tirets, absente, a dû être ajoutée.
pub fn table(block: &str) -> (Vec<Vec<String>>, bool) {
    let mut rows: Vec<Vec<String>> =
        block.lines().map(|l| cells(l).into_iter().map(|r| l[r].to_string()).collect()).collect();
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let added = rows.len() < 2 || !is_dashes(&rows[1]);
    if added {
        rows.insert(rows.len().min(1), vec!["---".to_string(); cols]);
    }
    rows.iter_mut().for_each(|row| row.resize(cols, String::new()));
    rows[1].iter_mut().filter(|c| c.is_empty()).for_each(|c| *c = "---".to_string());
    (rows, added)
}

/// Texte collé dans une cellule : sur une seule ligne, `|` protégé.
pub fn cell_text(text: &str) -> String {
    let text = text.trim_matches(['\n', '\r']);
    let joined = text.split(['\n', '\r']).map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    joined.replace('\t', " ").replace("\\|", "|").replace('|', "\\|")
}

/// Cellules d'un texte copié d'un tableur (tabulations) ou d'un tableau Markdown,
/// ligne par ligne ; `None` pour un texte ordinaire, qui tient dans une seule cellule.
pub fn paste_grid(text: &str) -> Option<Vec<Vec<String>>> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let markdown = !lines.is_empty() && lines.iter().all(|l| l.trim_start().starts_with('|') && bars(l).len() >= 2);
    let rows: Vec<Vec<String>> = if markdown {
        let mut rows: Vec<Vec<String>> =
            lines.iter().map(|l| cells(l).into_iter().map(|r| l[r].to_string()).collect()).collect();
        // Seule la deuxième ligne peut être celle des tirets : `| - | - |` plus bas est une donnée.
        if rows.get(1).is_some_and(|row| is_dashes(row)) {
            rows.remove(1);
        }
        rows
    } else if lines.iter().any(|l| l.contains('\t')) {
        lines.iter().map(|l| l.split('\t').map(cell_text).collect()).collect()
    } else {
        return None;
    };
    (!rows.is_empty()).then_some(rows)
}

/// Texte collé en dehors d'un tableau qui en est un, remis en forme : un tableau Markdown de
/// plusieurs lignes, ou des cellules séparées par des tabulations (tableur, page web), en
/// nombre égal sur chaque ligne. Du code indenté par des tabulations n'en est pas un.
pub fn paste_table(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return None;
    }
    let block = if lines.iter().all(|l| l.trim_start().starts_with('|')) {
        lines.iter().map(|l| l.trim()).collect::<Vec<_>>().join("\n")
    } else {
        let grid = paste_grid(text)?;
        let cols = grid[0].len();
        if cols < 2 || grid.iter().any(|row| row.len() != cols) || lines.iter().all(|l| l.starts_with('\t')) {
            return None;
        }
        grid.iter().map(|row| format!("| {} |", row.join(" | "))).collect::<Vec<_>>().join("\n")
    };
    Some(format_table(&table(&block).0, ""))
}

/// Tableau vide de `cols` colonnes et `rows` lignes, en-tête compris.
pub fn new_table(cols: usize, rows: usize) -> Vec<Vec<String>> {
    table(&vec![format!("{}|", "| ".repeat(cols)); rows].join("\n")).0
}

/// Largeur d'une ligne écrite en colonnes de texte : les idéogrammes et les émojis en prennent deux.
fn columns(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Au-delà de cette largeur, les colonnes ne sont plus alignées : une longue cellule
/// remplirait d'espaces toutes les lignes du fichier. Le tableau s'affiche pareil.
const ALIGNED: usize = 100;

/// Le tableau écrit en Markdown, colonnes alignées tant qu'il tient dans `ALIGNED`.
pub fn format_table(rows: &[Vec<String>], indent: &str) -> String {
    // La largeur de chaque cellule, comptée une fois.
    let shown: Vec<Vec<usize>> = rows.iter().map(|row| row.iter().map(|cell| columns(cell)).collect()).collect();
    let cols = rows.first().map_or(0, Vec::len);
    let mut widths: Vec<usize> = (0..cols)
        .map(|c| shown.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, row)| row[c]).max().unwrap_or(0).max(3))
        .collect();
    if widths.iter().map(|w| w + 3).sum::<usize>() + 1 > ALIGNED {
        widths.fill(0);
    }
    let mut text = String::with_capacity(rows.len() * (indent.len() + widths.iter().sum::<usize>() + 3 * cols + 2));
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            text.push('\n');
        }
        text.push_str(indent);
        text.push('|');
        for (c, cell) in row.iter().enumerate() {
            text.push(' ');
            if i == 1 {
                // Les deux-points d'alignement restent aux bouts des tirets.
                let (left, right) = (cell.starts_with(':'), cell.len() > 1 && cell.ends_with(':'));
                text.push_str(if left { ":" } else { "" });
                text.extend(std::iter::repeat_n('-', widths[c].max(3) - left as usize - right as usize));
                text.push_str(if right { ":" } else { "" });
            } else {
                text.push_str(cell);
                text.extend(std::iter::repeat_n(' ', widths[c].saturating_sub(shown[i][c])));
            }
            text.push_str(" |");
        }
    }
    text
}

/// Passages de `text` qui valent `query`, sans chevauchement. `case` : la casse compte ;
/// `word` : le passage ne touche ni lettre ni chiffre. Le texte est celui du fichier : les
/// marques du Markdown se cherchent comme le reste.
pub fn find(text: &str, query: &str, case: bool, word: bool) -> Vec<Range<usize>> {
    let fold = |c: char| -> Vec<char> { if case { vec![c] } else { c.to_lowercase().collect() } };
    let query: Vec<char> = query.chars().flat_map(fold).collect();
    // Longueur du passage qui commence `rest` et vaut la recherche, s'il y en a un.
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

/// Nombre de mots et de caractères du texte, tel qu'il est écrit : les marques du Markdown
/// comptent comme des caractères, les fins de ligne non.
pub fn counts(text: &str) -> (usize, usize) {
    (text.unicode_words().count(), text.graphemes(true).filter(|g| !matches!(*g, "\n" | "\r\n")).count())
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
    fn finds_passages() {
        let text = "Été, été. L'ÉTÉ d'**été**\nétés";
        assert_eq!(find(text, "été", false, false).len(), 5);
        assert_eq!(find(text, "été", true, false).len(), 3);
        assert_eq!(find(text, "été", false, true).len(), 4);
        assert_eq!(find(text, "**été**", true, true), [text.find("**").unwrap()..text.find('\n').unwrap()]);
        assert_eq!(find("aaaa", "aa", true, false), [0..2, 2..4]);
        assert!(find(text, "", false, false).is_empty() && find("", "a", false, false).is_empty());
        // Une lettre qui s'allonge en minuscules (« İ » : deux caractères) ne décale rien.
        assert_eq!(find("İstanbul", "stan", false, false), ["İ".len().."İstan".len()]);
    }

    #[test]
    fn counts_words_and_characters() {
        assert_eq!(counts(""), (0, 0));
        assert_eq!(counts("# Été\n\nl'idée, **déjà** 2 fois\n"), (5, 28));
        assert_eq!(counts("👨‍👩‍👧 ok"), (1, 4));
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
    #[test]
    fn reads_and_writes_tables() {
        assert_eq!(classify("| a | b |", false), (Kind::Table, 0));
        assert_eq!(callout("> [!NOTE] titre"), Some(2..9));
        assert_eq!(callout("> note"), None);
        // Un `|` échappé ou dans un lien ne sépare pas ; le `|` final est facultatif.
        let line = r"| a \| b | [[n|alias]] |  | fin";
        let found: Vec<&str> = cells(line).into_iter().map(|r| &line[r]).collect();
        assert_eq!(found, [r"a \| b", "[[n|alias]]", "", "fin"]);
        assert_eq!(cells("|   |")[0], 2..2);

        // Colonnes alignées, tirets ajoutés sous l'en-tête, lignes complétées.
        let (rows, added) = table("| Nom | Âge |\n| Élodie |");
        assert!(added);
        assert_eq!(format_table(&rows, ""), "| Nom    | Âge |\n| ------ | --- |\n| Élodie |     |");
        // L'alignement demandé est gardé ; un tableau déjà en forme ne bouge pas.
        let text = "  | a   | b   |\n  | :-- | --: |\n  | 1   | 2   |";
        let (rows, added) = table(text);
        assert!(!added);
        assert_eq!(format_table(&rows, "  "), text);
        assert_eq!(format_table(&new_table(2, 2), ""), "|     |     |\n| --- | --- |\n|     |     |");
    }
    /// Plusieurs centaines de tableaux tirés au hasard (graine fixe) : cellules
    /// variées (accents, idéogrammes, émojis, `\|`, liens à alias, code, vides ou
    /// très longues), lignes irrégulières. Le tableau écrit se relit à l'identique,
    /// garde le même nombre de cellules partout, et ne bouge plus une fois en forme.
    #[test]
    fn tables_survive_random_content() {
        const PIECES: &[&str] = &[
            "a", "é", "Élodie", "日本語", "😀", "x y", r"\|", "[[note|alias]]", "`code`", "**gras**", "12,5", "-", ":",
            "http://a.b/c?d=e", "", "", "très long texte de cellule qui ne tient sur aucune page raisonnable ",
        ];
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        for case in 0..600 {
            let cols = 1 + next(6);
            let rows = 1 + next(7);
            let cell = |next: &mut dyn FnMut(usize) -> usize| {
                let n = next(4);
                (0..n).map(|_| PIECES[next(PIECES.len())]).collect::<Vec<_>>().join(if next(2) == 0 { "" } else { " " })
            };
            // Lignes irrégulières, sans `|` final une fois sur quatre.
            let lines: Vec<String> = (0..rows)
                .map(|_| {
                    let n = 1 + next(cols);
                    let body = (0..n).map(|_| format!(" {} ", cell(&mut next).trim())).collect::<Vec<_>>().join("|");
                    format!("|{body}{}", if next(4) == 0 { "" } else { "|" })
                })
                .collect();
            let (grid, _) = table(&lines.join("\n"));
            let width = grid[0].len();
            assert!(grid.iter().all(|row| row.len() == width), "cas {case}: {grid:?}");
            let text = format_table(&grid, "");
            // Même nombre de cellules sur chaque ligne, quel que soit le contenu.
            for line in text.lines() {
                assert_eq!(cells(line).len(), width, "cas {case}: {line}");
            }
            // Relu, le tableau écrit redonne les mêmes cellules, et le même texte.
            let (again, added) = table(&text);
            assert!(!added, "cas {case}");
            // La ligne de tirets se relit à sa largeur d'écriture ; le reste à l'identique.
            let data = |g: &[Vec<String>]| g.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, r)| r.clone()).collect::<Vec<_>>();
            assert_eq!(data(&again), data(&grid), "cas {case}");
            assert_eq!(format_table(&again, ""), text, "cas {case}");
            // Collé depuis un autre tableau, il reste une grille de même forme.
            let pasted = paste_grid(&text).unwrap();
            assert_eq!(pasted.len(), grid.len() - 1, "cas {case}");
            assert!(pasted.iter().all(|row| row.len() == width), "cas {case}");
        }
    }

    #[test]
    fn pasted_text_stays_in_its_cell() {
        assert_eq!(cell_text("a|b"), r"a\|b");
        assert_eq!(cell_text(r"a\|b"), r"a\|b");
        assert_eq!(cell_text("une\r\n\r\n  deux  \n"), "une deux");
        assert_eq!(cell_text("\tx\t"), "x");
        assert_eq!(cell_text("a\tb"), "a b");
        assert_eq!(paste_grid("texte\nsur deux lignes"), None);
        assert_eq!(paste_grid("a|b"), None);
        assert_eq!(paste_grid("a\tb\n\nc\t\n"), Some(vec![vec!["a".into(), "b".into()], vec!["c".into(), String::new()]]));
        assert_eq!(paste_grid("1\t2\r\n3\t4"), Some(vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]]));
        // Les tirets d'un tableau Markdown ne sont pas des données.
        assert_eq!(paste_grid("| a | b |\n| :-- | --: |\n| c | d |").unwrap().len(), 2);
        assert_eq!(paste_grid("| a | b |").unwrap(), vec![vec!["a".to_string(), "b".to_string()]]);
        assert_eq!(paste_grid("a\t\"x|y\""), Some(vec![vec!["a".into(), "\"x\\|y\"".into()]]));
    }

    #[test]
    fn pastes_tables_from_anywhere() {
        let md = "| a | b |\n|:--|--:|\n| longue | 2 |";
        assert_eq!(paste_table(md).unwrap(), "| a      | b   |\n| :----- | --: |\n| longue | 2   |");
        // Des cellules de page web ou de tableur : la première ligne est l'en-tête.
        assert_eq!(paste_table("Nom\tÂge\nLéa\t31").unwrap(), "| Nom | Âge |\n| --- | --- |\n| Léa | 31  |");
        // Pas de colonnes, colonnes inégales, une seule ligne ou du code indenté : du texte.
        assert_eq!(paste_table("a\nb"), None);
        assert_eq!(paste_table("a\tb\nc"), None);
        assert_eq!(paste_table("a\tb"), None);
        assert_eq!(paste_table("| a | b |"), None);
        assert_eq!(paste_table("\ta\n\tb"), None);
    }

    #[test]
    fn aligns_by_display_width_and_stays_compact_when_wide() {
        // Un idéogramme et un émoji prennent deux colonnes.
        let (rows, _) = table("| a | b |\n| 日本 | 😀 |");
        assert_eq!(format_table(&rows, ""), "| a    | b   |\n| ---- | --- |\n| 日本 | 😀  |");
        // Trop large pour rester aligné : une seule espace autour de chaque cellule.
        let long = "x".repeat(120);
        let (rows, _) = table(&format!("| a | b |\n| {long} | c |"));
        assert_eq!(format_table(&rows, ""), format!("| a | b |\n| --- | --- |\n| {long} | c |"));
        // Réécrit, il ne bouge plus.
        let once = format_table(&rows, "");
        assert_eq!(format_table(&table(&once).0, ""), once);
        let (rows, _) = table(&format!("| a | b |\n|:-:|--:|\n| {long} | c |"));
        assert!(format_table(&rows, "").contains("| :-: | --: |"));
    }

    #[test]
    fn finds_checkboxes_anywhere() {
        fn found(line: &str) -> Vec<&str> {
            checkboxes(line, 0).into_iter().map(|r| &line[r]).collect()
        }
        assert_eq!(found("[ ] a [x] b [X]"), ["[ ]", "[x]", "[X]"]);
        assert_eq!(found("| [ ] | fait [x] | a |"), ["[ ]", "[x]"]);
        assert_eq!(found("|[ ]|"), ["[ ]"]);
        // Ni lien, ni mot collé, ni code, ni échappé, ni autre contenu.
        assert!(found("[[ ]] [x](lien) a[ ] [ ]b `[ ]` \\[ ] [y] [  ]").is_empty());
        // La case d'une tâche de liste est celle du marqueur, qu'on saute.
        assert_eq!(checkboxes("- [ ] a [ ] b", 6).len(), 1);
        // Une case cochée est en gras.
        let mut flags = vec![0u16; 7];
        inline("[x] [ ]", 0, &mut flags);
        assert!(flags[..3].iter().all(|f| f & (MARK | BOLD) == MARK | BOLD));
        assert!(flags[4..].iter().all(|f| f & MARK != 0 && f & BOLD == 0));
    }

    /// Les raccourcis qui évitent de classer chaque ligne disent la même chose que `classify` et `is_fence`.
    #[test]
    fn shortcuts_agree_with_the_line_by_line_scan() {
        let lines = [
            "", "|", "| a |", "  | a |", "\t| a |", "- | x", "# | x", "|---|", "```", "  ```rust", "x ```", "``````", "$$", " $$ ",
            "a $$ b", "$$$$", "> | q", "~~~", "---", "|-", "1. | x",
        ];
        for text in [lines.join("\n"), lines.join("\n\n"), lines.iter().rev().cloned().collect::<Vec<_>>().join("\n"), "```\n```".to_string()] {
            let (fences, dollars) = (
                text.split('\n').filter(|l| is_fence(l)).count(),
                text.split('\n').filter(|l| l.trim() == "$$").count(),
            );
            assert_eq!(count_fences_and_dollars(&text), (fences, dollars), "{text:?}");
        }
        for l in lines {
            assert_eq!(is_table_line(l), classify(l, false).0 == Kind::Table, "{l:?}");
        }
    }
}
