//! The text of a PDF as `pdftotext` gives it: pages separated by a form feed (`\f`). Where each page
//! starts, which page a byte is on, and the name of its cached copy. Pure: reading the PDF is
//! `platform/pdftext.rs`, searching it is `logic/search.rs`.

use sha2::{Digest, Sha256};

/// The byte where each page starts: the first at 0, the next ones after each form feed. A form feed
/// that ends the text opens no page.
pub fn page_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\u{c}').map(|(at, _)| at + 1).filter(|&at| at < text.len()));
    starts
}

/// The page (from 1) that holds byte `at`, given `page_starts`.
pub fn page_at(starts: &[usize], at: usize) -> usize {
    starts.partition_point(|&start| start <= at).max(1)
}

/// The text without its form feeds, which no search should see: each becomes a line break, so the bytes
/// (and `page_starts`) stay where they are.
pub fn searchable(text: String) -> String {
    if text.contains('\u{c}') { text.replace('\u{c}', "\n") } else { text }
}

/// The file name of a PDF's cached text: a digest of where the PDF is, its size and when it was last
/// written. Any change gives another name, so a stale copy is never read.
// ponytail: an old copy stays in the cache once its PDF changes or goes; a sweep of the names no PDF
// gives any more, if the folder ever grows large.
pub fn cache_name(path: &str, size: u64, modified_ns: u128) -> String {
    let digest = Sha256::digest(format!("{path}\n{size}\n{modified_ns}").as_bytes());
    let hex: String = digest.iter().take(16).map(|byte| format!("{byte:02x}")).collect();
    format!("{hex}.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_start_after_each_form_feed() {
        let text = "one\n\u{c}two\n\u{c}three\n\u{c}";
        let starts = page_starts(text);
        assert_eq!(starts, [0, 5, 10]);
        assert_eq!(page_at(&starts, 0), 1);
        assert_eq!(page_at(&starts, 3), 1);
        assert_eq!(page_at(&starts, 5), 2);
        assert_eq!(page_at(&starts, text.find("three").unwrap()), 3);
        // A text with no form feed is one page.
        assert_eq!(page_starts("plain"), [0]);
        assert_eq!(page_at(&[0], 42), 1);
        // An empty page is still a page.
        assert_eq!(page_starts("a\u{c}\u{c}b"), [0, 2, 3]);
    }

    #[test]
    fn form_feeds_become_line_breaks_in_place() {
        let text = "one\u{c}two".to_string();
        let starts = page_starts(&text);
        let clean = searchable(text);
        assert_eq!(clean, "one\ntwo");
        assert_eq!(&clean[starts[1]..], "two");
    }

    #[test]
    fn the_cache_name_changes_with_the_file() {
        let name = cache_name("/v/a.pdf", 10, 1);
        assert!(name.ends_with(".txt") && name.len() == 36);
        assert_eq!(name, cache_name("/v/a.pdf", 10, 1));
        for other in [cache_name("/v/b.pdf", 10, 1), cache_name("/v/a.pdf", 11, 1), cache_name("/v/a.pdf", 10, 2)] {
            assert_ne!(name, other);
        }
    }
}
