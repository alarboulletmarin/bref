//! Markdown tables: cells, pasting, rows and columns, sorting, aligned formatting.

use super::*;

/// Place of each `|` separating two cells: not those that are escaped or
/// caught in a `[[link|alias]]`.
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

/// Range of each cell's content in the line, without the surrounding spaces.
pub fn cells(line: &str) -> Vec<Range<usize>> {
    let mut bars = bars(line);
        // Without a final `|`, what follows the last one is still a cell.
    if bars.last().is_some_and(|&last| !line[last + 1..].trim().is_empty()) {
        bars.push(line.len());
    }
    bars.windows(2)
        .map(|pair| {
            let cell = &line[pair[0] + 1..pair[1]];
                        // Empty cell: the cursor goes one space from the `|`.
            let lead = if cell.trim().is_empty() { cell.len().min(1) } else { indent_len(cell) };
            let start = pair[0] + 1 + lead;
            start..start + cell.trim().len()
        })
        .collect()
}

pub fn is_dashes(row: &[String]) -> bool {
    !row.is_empty() && row.iter().all(|c| c.contains('-') && c.chars().all(|x| matches!(x, '-' | ':')))
}

/// Lines of a table (header, dashes, then the rest), all of the same
/// length; and whether the dash line, missing, had to be added.
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

/// Text pasted into a cell: on a single line, `|` escaped.
pub fn cell_text(text: &str) -> String {
    let text = text.trim_matches(['\n', '\r']);
    let joined = text.split(['\n', '\r']).map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    joined.replace('\t', " ").replace("\\|", "|").replace('|', "\\|")
}

/// Cells of a text copied from a spreadsheet (tabs) or a Markdown table,
/// line by line; `None` for ordinary text, which fits in a single cell.
pub fn paste_grid(text: &str) -> Option<Vec<Vec<String>>> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let markdown = !lines.is_empty() && lines.iter().all(|l| l.trim_start().starts_with('|') && bars(l).len() >= 2);
    let rows: Vec<Vec<String>> = if markdown {
        let mut rows: Vec<Vec<String>> =
            lines.iter().map(|l| cells(l).into_iter().map(|r| l[r].to_string()).collect()).collect();
                // Only the second line can be the dash line: `| - | - |` further down is data.
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

/// Cells (their text as displayed, `|` not escaped) as a Markdown table: the
/// first row is the header, a line of dashes follows it. A cell keeps a single
/// line and its `|` escaped, so pasting the result rebuilds the same cells.
pub fn grid_markdown(cells: &[Vec<String>]) -> String {
    let block: Vec<String> = cells.iter().map(|row| format!("| {} |", row.iter().map(|c| cell_text(c)).collect::<Vec<_>>().join(" | "))).collect();
    if block.is_empty() {
        return String::new();
    }
    format_table(&table(&block.join("\n")).0, "")
}

/// Text pasted outside a table that is one, reformatted: a Markdown table of
/// several lines, or cells separated by tabs (spreadsheet, web page), in
/// equal number on each line. Code indented with tabs is not one.
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

/// Empty table of `cols` columns and `rows` rows, header included.
pub fn new_table(cols: usize, rows: usize) -> Vec<Vec<String>> {
    table(&vec![format!("{}|", "| ".repeat(cols)); rows].join("\n")).0
}

/// The table's visible rows (without the dashes), and the dashes.
pub(super) fn split_grid(rows: &[Vec<String>]) -> (Vec<Vec<String>>, Vec<String>) {
    let visible = rows.iter().enumerate().filter(|(i, _)| *i != 1).map(|(_, row)| row.clone()).collect();
    (visible, rows.get(1).cloned().unwrap_or_default())
}

pub(super) fn join_grid(mut visible: Vec<Vec<String>>, dashes: Vec<String>) -> Option<Vec<Vec<String>>> {
    if visible.is_empty() || dashes.is_empty() {
        return None;
    }
    visible.insert(1.min(visible.len()), dashes);
    Some(visible)
}

/// Number of visible rows of a grid (the header counts, the dashes do not).
pub fn visible_rows(rows: &[Vec<String>]) -> usize {
    rows.len().saturating_sub(1)
}

/// The names of a table's rows and of its columns, see `scopes`.
pub type Scopes = (Vec<String>, Vec<String>);

/// What a comment under a table can start with, to say what it is about: for each visible row
/// `Row "Léa"` (`Row "Léa" (2)` when several rows begin alike, `Row 3` for an empty first
/// cell), for each column `Column "Âge"` (`Column 2` for an empty header). A name comes from
/// the content, not from a position: a sort leaves the comment with its row.
pub fn scopes(rows: &[Vec<String>]) -> Scopes {
    let first = |v: usize| rows[if v == 0 { 0 } else { v + 1 }].first().map_or("", |c| c.as_str());
    let named = |label: &str, text: &str, nth: usize| if text.is_empty() { format!("{label} {nth}") } else { format!("{label} \"{text}\"") };
    let tall = if rows.len() > 1 { visible_rows(rows) } else { rows.len() };
    let names = (0..tall)
        .map(|v| {
            let same: Vec<usize> = (0..tall).filter(|&w| first(w) == first(v)).collect();
            let name = named("Row", first(v), v + 1);
            match same.iter().position(|&w| w == v) {
                Some(k) if same.len() > 1 && !first(v).is_empty() => format!("{name} ({})", k + 1),
                _ => name,
            }
        })
        .collect();
    let cols = rows.first().map_or(Vec::new(), |head| head.iter().enumerate().map(|(c, text)| named("Column", text, c + 1)).collect());
    (names, cols)
}

/// The comment lines under a table (`{>>Row "Léa": …<<}`) after its rows and columns changed:
/// what each becomes, `None` when what it is about is gone. `rows` and `cols` say where each old
/// row and column went (`None`: deleted); `old` and `new` are their `scopes`. A line that is about
/// something else, or a table that stays, is left as it is.
pub fn retarget(
    lines: &[&str],
    old: &Scopes,
    new: &Scopes,
    (rows, cols): (&[Option<usize>], &[Option<usize>]),
    table_gone: bool,
) -> Vec<Option<String>> {
    lines
        .iter()
        .map(|line| {
            let indent = &line[..line.len() - line.trim_start().len()];
            let Some(body) = line.trim().strip_prefix("{>>") else { return Some(line.to_string()) };
            if body.starts_with("Table: ") {
                return (!table_gone).then(|| line.to_string());
            }
            let hit = [(&old.0, rows, &new.0), (&old.1, cols, &new.1)].into_iter().find_map(|(names, moved, fresh)| {
                names.iter().enumerate().find_map(|(i, name)| {
                    body.strip_prefix(&format!("{name}: ")).map(|rest| (moved.get(i).copied().flatten().and_then(|j| fresh.get(j)), rest))
                })
            });
            match hit {
                None => Some(line.to_string()),
                Some((Some(name), rest)) if !table_gone => Some(format!("{indent}{{>>{name}: {rest}")),
                Some(_) => None,
            }
        })
        .collect()
}

/// An empty row at visible rank `at` (never before the header, never past the end).
pub fn insert_row(rows: &[Vec<String>], at: usize) -> Vec<Vec<String>> {
    let (mut visible, dashes) = split_grid(rows);
    visible.insert(at.clamp(1, visible.len()), vec![String::new(); dashes.len()]);
    join_grid(visible, dashes).unwrap_or_default()
}

/// An empty column at place `at`.
pub fn insert_col(rows: &[Vec<String>], at: usize) -> Vec<Vec<String>> {
    let mut rows = rows.to_vec();
    for (i, row) in rows.iter_mut().enumerate() {
        row.insert(at.min(row.len()), if i == 1 { "---" } else { "" }.to_string());
    }
    rows
}

/// Without the visible rows `range`. Removing the header gives its role to the next row.
pub fn delete_rows(rows: &[Vec<String>], range: std::ops::Range<usize>) -> Option<Vec<Vec<String>>> {
    let (visible, dashes) = split_grid(rows);
    let kept = visible.into_iter().enumerate().filter(|(i, _)| !range.contains(i)).map(|(_, row)| row).collect();
    join_grid(kept, dashes)
}

/// Without the columns `range`.
pub fn delete_cols(rows: &[Vec<String>], range: std::ops::Range<usize>) -> Option<Vec<Vec<String>>> {
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| row.iter().enumerate().filter(|(c, _)| !range.contains(c)).map(|(_, cell)| cell.clone()).collect())
        .collect();
    rows.first().is_some_and(|row| !row.is_empty()).then_some(rows)
}

/// Visible row `from` moves to rank `to` (the others shift). The header can move: the row
/// that takes its place becomes the header.
pub fn move_row(rows: &[Vec<String>], from: usize, to: usize) -> Vec<Vec<String>> {
    let (mut visible, dashes) = split_grid(rows);
    if from < visible.len() {
        let row = visible.remove(from);
        visible.insert(to.min(visible.len()), row);
    }
    join_grid(visible, dashes).unwrap_or_default()
}

/// Column `from` moves to place `to`, with its alignment.
pub fn move_col(rows: &[Vec<String>], from: usize, to: usize) -> Vec<Vec<String>> {
    let mut rows = rows.to_vec();
    for row in &mut rows {
        if from < row.len() {
            let cell = row.remove(from);
            row.insert(to.min(row.len()), cell);
        }
    }
    rows
}

/// Visible row `r` copied just below (the header: at the top of the body).
pub fn duplicate_row(rows: &[Vec<String>], r: usize) -> Vec<Vec<String>> {
    let (mut visible, dashes) = split_grid(rows);
    if let Some(row) = visible.get(r).cloned() {
        visible.insert(r + 1, row);
    }
    join_grid(visible, dashes).unwrap_or_default()
}

/// Column `c` copied just to the right.
pub fn duplicate_col(rows: &[Vec<String>], c: usize) -> Vec<Vec<String>> {
    let mut rows = rows.to_vec();
    for row in &mut rows {
        if let Some(cell) = row.get(c).cloned() {
            row.insert(c + 1, cell);
        }
    }
    rows
}

/// The body sorted on column `c` (natural sort, empty cells at the end); the header stays in place.
pub fn sort_rows(rows: &[Vec<String>], c: usize, ascending: bool) -> Vec<Vec<String>> {
    let (mut visible, dashes) = split_grid(rows);
    let blank = |row: &Vec<String>| row.get(c).is_none_or(|cell| cell.trim().is_empty());
    visible[1..].sort_by(|a, b| {
        let order = cell_order(a.get(c).map_or("", |s| s), b.get(c).map_or("", |s| s));
                // Empty cells stay at the end in both directions.
        match (blank(a), blank(b)) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => if ascending { order } else { order.reverse() },
        }
    });
    join_grid(visible, dashes).unwrap_or_default()
}

/// Rows become columns: the first row becomes the first column, and the header.
pub fn transpose(rows: &[Vec<String>]) -> Vec<Vec<String>> {
    let (visible, dashes) = split_grid(rows);
    let flipped: Vec<Vec<String>> =
        (0..dashes.len()).map(|c| visible.iter().map(|row| row.get(c).cloned().unwrap_or_default()).collect()).collect();
    let cols = flipped.first().map_or(0, Vec::len);
    join_grid(flipped, vec!["---".to_string(); cols]).unwrap_or_default()
}

/// Width of a line written in text columns: ideographs and emojis take two.
pub(super) fn columns(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Beyond this width, columns are no longer aligned: a long cell
/// would pad every line of the file with spaces. The table displays the same.
pub(super) const ALIGNED: usize = 100;

/// The table written as Markdown, columns aligned as long as it fits in `ALIGNED`.
pub fn format_table(rows: &[Vec<String>], indent: &str) -> String {
        // The width of each cell, counted once.
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
                                // The alignment colons stay at the ends of the dashes.
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
