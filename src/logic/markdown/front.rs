//! Front matter (YAML header): where it ends, its keys and values, list pages, aliases.

/// Length of the front matter block, its closing line and that line's line ending
/// included; zero if there is none. It starts on line 1 with `---` and closes with `---`
/// or `...`; without a closing line, it is not a block.
pub fn front_matter(text: &str) -> usize {
    let mut at = 0;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        at += line.len();
        match (i, line.trim_end()) {
            (0, "---") => {}
            (0, _) => return 0,
            (_, "---" | "...") => return at,
            _ => {}
        }
    }
    0
}

/// Where to put the cursor when a note opens: at the start of its body, after the front matter,
/// the title line and the empty lines that follow it. The first key typed thus goes into
/// the text, without breaking the front matter nor renaming the note through its title.
pub fn body_start(text: &str) -> usize {
    let start = front_matter(text);
    let rest = &text[start..];
    let Some(title) = rest.find('\n') else {
                // A single line, with no ending: the cursor follows it, as it was left.
        return text.len();
    };
    let mut at = start + title + 1;
    for line in text[at..].split_inclusive('\n') {
        if !line.trim().is_empty() {
            break;
        }
        at += line.len();
    }
    at
}

/// Values of key `key` in the front matter, quotes removed: inline list (`[a, b]`),
/// comma-separated values, or a dash list on the following lines.
// ponytail: read by hand, for the two keys the app uses; no nested YAML nor indented
// key. A YAML library if other keys or richer values need reading.
pub fn front_values(text: &str, key: &str) -> Vec<String> {
    let clean = |value: &str| value.trim().trim_matches(['"', '\'']).trim().to_string();
    let mut lines = text[..front_matter(text)].lines().skip(1);
    let Some(rest) = lines.find_map(|line| Some(line.strip_prefix(key)?.strip_prefix(':')?.trim())) else {
        return Vec::new();
    };
    let values: Vec<String> = match rest.is_empty() {
        true => lines.map_while(|line| line.trim_start().strip_prefix("- ")).map(clean).collect(),
        false => rest.strip_prefix('[').and_then(|list| list.strip_suffix(']')).unwrap_or(rest).split(',').map(clean).collect(),
    };
    values.into_iter().filter(|value| !value.is_empty()).collect()
}

/// Front matter keys, in the order they are written.
pub fn front_keys(text: &str) -> Vec<&str> {
    let lines = text[..front_matter(text)].lines().skip(1);
    let keys = lines.filter_map(|line| line.split_once(':')).map(|(key, _)| key);
    keys.filter(|key| !key.is_empty() && !key.starts_with([' ', '\t', '-', '#'])).collect()
}

/// Table of notes: one column per key of their front matter, in order of appearance,
/// and for each note its values (several values: separated by commas).
pub fn list_table(notes: &[&str]) -> (Vec<String>, Vec<Vec<String>>) {
    let mut keys: Vec<String> = Vec::new();
    for key in notes.iter().flat_map(|text| front_keys(text)) {
        if !keys.iter().any(|known| known == key) {
            keys.push(key.to_string());
        }
    }
    let rows = notes.iter().map(|text| keys.iter().map(|key| front_values(text, key).join(", ")).collect()).collect();
    (keys, rows)
}

/// The note once key `key` of its front matter is set to `value`; nothing else changes
/// in the file. A missing key is added at the end of the front matter, created if needed; an empty
/// value removes the key, and the front matter if nothing is left in it. A list (`[a, b]` or dashed) stays
/// a list, written inline.
pub fn set_front(text: &str, key: &str, value: &str) -> String {
    let value = value.trim();
    let block = front_matter(text);
    if block == 0 {
        return if value.is_empty() { text.to_string() } else { format!("---\n{key}: {value}\n---\n{text}") };
    }
    let lines: Vec<&str> = text[..block].split_inclusive('\n').collect();
    let rest_of = |line: &str| Some(line.strip_prefix(key)?.strip_prefix(':')?.trim().to_string());
        // The key spans its line, plus those of its dash list.
    let (first, rest) = match lines.iter().enumerate().skip(1).find_map(|(i, line)| Some((i, rest_of(line)?))) {
        Some(found) => found,
        None if value.is_empty() => return text.to_string(),
        None => {
            let close = block - lines[lines.len() - 1].len();
            return format!("{}{key}: {value}\n{}", &text[..close], &text[close..]);
        }
    };
    let dashes = if rest.is_empty() { lines[first + 1..].iter().take_while(|line| line.trim_start().starts_with("- ")).count() } else { 0 };
    let end = if lines[first].ends_with("\r\n") { "\r\n" } else { "\n" };
    let written = match (value.is_empty(), dashes > 0 || rest.starts_with('[')) {
        (true, _) => String::new(),
        (_, true) => format!("{key}: [{}]{end}", value.split(',').map(str::trim).filter(|v| !v.is_empty()).collect::<Vec<_>>().join(", ")),
        (_, false) => format!("{key}: {value}{end}"),
    };
    let head: String = lines[..first].concat() + &written + &lines[first + 1 + dashes..].concat();
        // Only the opening and closing lines are left: no more front matter.
    let emptied = head.lines().count() == 2;
    format!("{}{}", if emptied { "" } else { &head }, &text[block..])
}

/// A table row passes the filter: the text is searched in all its cells, or,
/// written `key:text`, in that key's column; ignoring case.
pub fn row_kept(keys: &[String], cells: &[String], filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    let column = filter.split_once(':').and_then(|(key, word)| Some((keys.iter().rposition(|k| k.to_lowercase() == key.trim())?, word.trim().to_string())));
    match column {
        Some((at, word)) => cells.get(at).is_some_and(|cell| cell.to_lowercase().contains(&word)),
        None => cells.iter().any(|cell| cell.to_lowercase().contains(&filter)),
    }
}

/// Order of two table cells: numbers by their value, the rest ignoring case;
/// an empty cell comes after the others.
pub fn cell_order(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.trim().replace(',', ".").parse::<f64>(), b.trim().replace(',', ".").parse::<f64>()) {
        (Ok(x), Ok(y)) => x.total_cmp(&y),
        _ => a.is_empty().cmp(&b.is_empty()).then_with(|| a.to_lowercase().cmp(&b.to_lowercase())),
    }
}

/// Other names of the note, given by the `aliases` key of its front matter: a `[[link]]` can
/// target it by one of them.
pub fn aliases(text: &str) -> Vec<String> {
    front_values(text, "aliases")
}
