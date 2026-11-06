//! Names and kinds of files, read from the path alone: natural sort key, what the app can show.
//! Pure functions; the disk is `platform/vault.rs`.

use std::path::{Path, PathBuf};

/// Piece of a name for natural sort: a number compares by its value, before text.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum Chunk {
    Number(u128),
    Text(String),
}

/// "Natural" sort key, like file managers use: `Note 2` before
/// `Note 10`, ignoring case.
pub fn natural(name: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut rest = name;
    while let Some(first) = rest.chars().next() {
        let digit = first.is_ascii_digit();
        let end = rest.find(|c: char| c.is_ascii_digit() != digit).unwrap_or(rest.len());
        let (part, tail) = rest.split_at(end);
        chunks.push(match part.parse() {
            Ok(n) if digit => Chunk::Number(n),
            _ => Chunk::Text(part.to_lowercase()),
        });
        rest = tail;
    }
    chunks
}

/// The file is an image the app can display.
pub fn is_image(path: &Path) -> bool {
    let known = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "tif", "tiff", "avif"];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| known.iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// Files kept in the vault without being read: they are listed in the tree, found in
/// the palette, targeted by `[[name.pdf]]` and opened in the system's application.
const ATTACHMENTS: [&str; 34] = [
    "pdf", "mp4", "m4v", "mov", "mkv", "webm", "avi", "mp3", "m4a", "wav", "ogg", "opus", "flac", "zip", "7z", "tar", "gz", "rar",
    "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "epub", "txt", "json", "xml", "html", "rtf", "log",
];

pub fn is_attachment(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ATTACHMENTS.iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// The file is an image, a table or an attachment: what the tree shows besides notes.
pub fn is_file_shown(path: &Path) -> bool {
    is_image(path) || is_table(path) || is_attachment(path)
}

/// The file is a table (CSV or TSV) the app opens in a grid.
pub fn is_table(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["csv", "tsv"].iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// The images among the vault's files (the graph and `![[…]]` links ignore tables).
pub fn pictures(files: &[PathBuf]) -> Vec<PathBuf> {
    files.iter().filter(|p| is_image(p)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_names_naturally() {
        let mut names = vec!["Note 10", "note 2", "Note 1", "Notes", "2026-10-09", "2026-9-30", "Note 1b"];
        names.sort_by_cached_key(|n| natural(n));
        assert_eq!(names, ["2026-9-30", "2026-10-09", "Note 1", "Note 1b", "note 2", "Note 10", "Notes"]);
    }

    #[test]
    fn files_are_told_apart_by_extension_ignoring_case() {
        for name in ["a.PDF", "b.Mp4", "c.docx"] {
            assert!(is_attachment(Path::new(name)) && is_file_shown(Path::new(name)), "{name}");
        }
        for name in ["a.md", "b", "c.exe", ".pdf", ""] {
            assert!(!is_attachment(Path::new(name)) && !is_file_shown(Path::new(name)), "{name}");
        }
        assert!(is_image(Path::new("x/Photo.JPG")) && is_file_shown(Path::new("p.svg")) && !is_image(Path::new("a.csv")));
        assert!(is_table(Path::new("a.TSV")) && is_file_shown(Path::new("a.csv")) && !is_table(Path::new("csv")));
        assert_eq!(pictures(&["a.png".into(), "b.md".into(), "c.GIF".into()]), [PathBuf::from("a.png"), PathBuf::from("c.GIF")]);
    }
}
