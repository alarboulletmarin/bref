//! Les blocs d'une note, comme ceux de Notion : ce qu'une poignée prend, déplace, duplique, supprime
//! ou transforme. Un bloc est fait de lignes entières : un titre, une ligne de texte, un item de liste
//! avec ce qui est plus en retrait sous lui, une citation ou un panneau (ses lignes `>` qui se
//! suivent), un bloc de code ou de formule, un tableau, une règle, une image. Les lignes vides
//! séparent les blocs sans en être, et l'en-tête YAML n'en est pas un. Un bloc qui bouge emporte un
//! des espaces qui le bordent, et le rend où il arrive : des paragraphes séparés par une ligne vide le
//! restent, des items de liste restent collés. Fonctions pures.

use crate::logic::markdown::{self as md, Kind};
use std::ops::Range;

/// Ce qu'un bloc peut devenir.
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

/// Début et fin (sans le retour à la ligne) de chaque ligne.
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

/// Le rang de la dernière ligne du bloc qui commence à la ligne `i`.
fn extent(text: &str, lines: &[(usize, usize)], i: usize) -> usize {
    let at = |k: usize| &text[lines[k].0..lines[k].1];
    let last = lines.len() - 1;
    let line = at(i);
    // Un bloc ouvert et jamais refermé va jusqu'au bout de la note.
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

/// Les blocs de premier niveau, dans l'ordre : du début de leur première ligne à la fin de leur
/// dernière, sans le retour à la ligne qui la suit.
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

/// Le bloc de la ligne qui commence en `line` : un item de liste est le sien (avec ses sous-items),
/// une ligne d'un bloc de code, d'un tableau ou d'une citation donne le bloc entier ; une ligne vide,
/// rien. `top` : les blocs de premier niveau (`blocks`).
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
    // Un sous-item : lui et ce qui est plus en retrait sous lui, sans sortir du bloc.
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

/// Où un bloc peut se poser : avant chaque bloc de premier niveau, avant chaque item d'une liste,
/// et à la fin (`text.len()`).
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

/// L'espace qui suit le bloc jusqu'au bloc suivant (lignes vides et retours à la ligne), et où
/// commence ce suivant ; `None` pour le dernier bloc.
fn after(text: &str, block: &Range<usize>) -> (Range<usize>, bool) {
    let rest = &text[block.end..];
    let blank = rest.len() - rest.trim_start_matches(|c: char| c == '\n' || c == ' ' || c == '\t').len();
    // L'espace s'arrête au début de la ligne suivante qui a du texte.
    let gap = &rest[..blank];
    let to = block.end + gap.rfind('\n').map_or(0, |n| n + 1);
    (block.end..to, to < text.len() && blank < rest.len())
}

/// L'espace qui précède le bloc depuis la fin du bloc d'avant.
fn before(text: &str, block: &Range<usize>) -> Range<usize> {
    let head = &text[..block.start];
    let kept = head.trim_end_matches(|c: char| c == '\n' || c == ' ' || c == '\t').len();
    // Il commence au retour à la ligne qui clôt la dernière ligne écrite.
    let from = head[kept..].find('\n').map_or(block.start, |n| kept + n);
    from.max(md::front_matter(text).saturating_sub(1)).min(block.start)..block.start
}

/// La première ligne d'un bloc.
fn head(piece: &str) -> &str {
    piece.split('\n').next().unwrap_or_default()
}

/// Le texte sans le bloc ni le plus petit des deux espaces qui le bordent (celui d'après, à
/// égalité) : le dernier item d'une liste part avec le retour à la ligne qui le lie à l'item d'avant,
/// et la ligne vide qui sépare la liste de ce qui suit reste en place. Rend aussi l'espace parti et
/// où était le bloc dans le nouveau texte.
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

/// L'espace à mettre entre deux blocs voisins `a` et `b` : deux items de liste restent collés ; un
/// item à côté d'autre chose prend l'espace `there` du lieu où il arrive ; sinon `kept`, l'espace
/// que le bloc avait, garde la façon d'écrire de la note.
fn spacing(a: &str, b: &str, there: &str, kept: &str) -> String {
    let gap = match (is_item(head(a)), is_item(head(b))) {
        (true, true) => "\n",
        (false, false) => kept,
        _ => there,
    };
    if gap.contains('\n') { gap.to_string() } else { "\n\n".to_string() }
}

/// Le bloc posé avant le bloc qui commence en `to` (dans le texte d'origine), ou à la fin
/// (`text.len()`) : le nouveau texte et où est le bloc. `None` quand rien ne bougerait.
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
        // L'espace de cet endroit : celui qui précède le dernier bloc.
        let last = blocks(&rest[..end]).last().cloned().unwrap_or(0..0);
        let there = rest[before(&rest, &last)].to_string();
        let sep = spacing(&rest[last], piece, &there, &kept);
        rest.insert_str(end, &format!("{sep}{piece}"));
        let at = end + sep.len();
        return Some((rest, at..at + piece.len()));
    }
    // L'espace qui précède, à l'origine, le bloc devant lequel on se pose : celui de cet endroit.
    let there_start = text[..to].trim_end_matches(['\n', ' ', '\t']).len();
    let there = &text[there_start..to];
    let target = text[to..].split('\n').next().unwrap_or_default();
    let sep = spacing(piece, target, there, &kept);
    let at = if to > block.start { to - removed } else { to };
    rest.insert_str(at, &format!("{piece}{sep}"));
    Some((rest, at..at + piece.len()))
}

/// Le bloc doublé juste après lui : un item colle à son double, un autre bloc garde l'espace qui le
/// borde. Le nouveau texte et où est la copie.
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

/// Le texte sans le bloc ni un des espaces qui le bordent, et où mettre le curseur.
pub fn removed(text: &str, block: Range<usize>) -> (String, usize) {
    match take(text, &block) {
        Some((out, _, at)) => {
            let at = at.min(out.len());
            (out, at)
        }
        // Le seul bloc de la note : il part, ce qui l'entoure reste.
        None => {
            let mut out = text.to_string();
            out.replace_range(block.clone(), "");
            (out, block.start)
        }
    }
}

/// Le contenu d'une ligne, sans son marqueur de titre, de liste ou de citation.
fn bare(line: &str) -> &str {
    let (kind, marker) = md::classify(line, false);
    match kind {
        Kind::Heading(_) | Kind::Quote | Kind::Bullet | Kind::Ordered | Kind::Task(_) => &line[marker.min(line.len())..],
        _ => line.trim_start(),
    }
}

/// Les lignes du bloc `piece` dans la forme `turn`. Un bloc de code perd ses clôtures avant de
/// devenir autre chose ; une liste naît de chaque ligne, numérotée à partir de 1.
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
            // Une liste qui est déjà de ce genre le reste : `restyle` la remettrait en texte.
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

/// Le plus petit remplacement qui fait passer de `old` à `new` : la plage de `old` et son texte.
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
        // L'en-tête YAML n'est pas un bloc ; un bloc de code jamais refermé va jusqu'au bout.
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
        // Les items d'une liste sont autant de places où poser un bloc, en plus des blocs et de la fin.
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
        // Juste avant soi, ou juste après : rien ne bouge.
        assert_eq!(moved(text, 3..4, 3), None);
        assert_eq!(moved(text, 3..4, 6), None);
        assert_eq!(moved(text, c, text.len()), None);
        // Sans retour à la ligne final, et des items de liste qui restent collés.
        let list = "- a\n- b\n- c";
        assert_eq!(moved(list, 0..3, list.len()).unwrap().0, "- b\n- c\n- a");
        assert_eq!(moved(list, 8..11, 0).unwrap().0, "- c\n- a\n- b");
        // Le seul bloc n'a nulle part où aller.
        assert_eq!(moved("seul\n", 0..4, 5), None);
        // Le dernier item d'une liste, monté en tête : la liste reste collée, et la ligne vide qui la
        // sépare du panneau reste en place.
        let list = "Titre\n\n- a\n- b\n- c\n\n> note\n";
        let c = list.find("- c").unwrap();
        assert_eq!(moved(list, c..c + 3, list.find("- a").unwrap()).unwrap().0, "Titre\n\n- c\n- a\n- b\n\n> note\n");
        // Un item sorti de sa liste prend l'espace de l'endroit où il arrive.
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
