//! The page of the user guide that lists every shortcut, `book/src/shortcuts.md`, is written
//! from the help panel's sections: the same lines, in English, with the default keys of Linux
//! and Windows (macOS shows other keys, so the page is not checked there).

use super::*;

const PAGE: &str = "book/src/shortcuts.md";

const HEAD: &str = "# Keyboard shortcuts

<!-- Written by the test `shortcuts_page_is_current` from `help_sections` (src/shell/keymap.rs).
     Do not edit by hand: run `make docs`. -->

Every line of the help panel (`F1`), with the default keys of Linux and Windows. On macOS, read `Cmd` for `Ctrl`, and `Alt` for `Ctrl` when moving by word. Moving between panes is `Cmd + Alt + arrows`, and back and forward are `Cmd + [` and `Cmd + ]`.

`Ctrl+P › word` means: open the palette and type that word. A line without keys has no default shortcut. Any line that names an action can be given keys in the help panel: see [Changing a shortcut](settings.md#changing-a-shortcut).
";

/// Text of a table cell: the Markdown signs are escaped, so the line reads as the panel shows it.
fn cell(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '#') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Keys as code: "F1 / Ctrl+/" gives `F1` / `Ctrl+/`.
fn keys(keys: &str) -> String {
    if keys.is_empty() {
        return String::new();
    }
    keys.split(" / ")
        .map(|k| {
            // A line that names a gesture in words ("A cloud folder") is not a key.
            if k.contains(' ') && !k.contains('+') && !k.contains('›') {
                return cell(k);
            }
            let k = k.replace('|', "\\|");
            if k.contains('`') { format!("`` {k} ``") } else { format!("`{k}`") }
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

fn page() -> String {
    crate::lang::in_english(|| {
        let mut out = String::from(HEAD);
        for (title, rows) in help_sections(&defaults(), &keys::Changes::new()) {
            out.push_str(&format!("\n## {title}\n\n| Keys | Action |\n|---|---|\n"));
            for row in rows {
                out.push_str(&format!("| {} | {} |\n", keys(&row.keys), cell(row.effect)));
            }
        }
        out
    })
}

/// `make docs` (`BREF_WRITE_DOCS=1`) writes the page; otherwise a page that no longer matches
/// the help panel fails, with the command that brings it up to date.
#[cfg(not(target_os = "macos"))]
#[test]
fn shortcuts_page_is_current() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(PAGE);
    let page = page();
    if std::env::var_os("BREF_WRITE_DOCS").is_some() {
        std::fs::write(&path, &page).unwrap();
        return;
    }
    let written = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(written == page, "{PAGE} no longer matches the help panel: run `make docs`");
}

#[test]
fn shortcuts_page_is_in_english_with_every_section() {
    let page = page();
    for title in ["## Notes", "## Navigation", "## Diagrams", "## CSV tables", "## Appearance", "## Typing", "## Editing", "## Blocks"] {
        assert!(page.contains(&format!("\n{title}\n")), "{title} missing");
    }
    assert!(page.contains("| Find by name or text, create, filter by \\#tag |"));
    // The rest of the thread is back in French.
    assert_eq!(crate::lang::tr("Notes", "Remarques"), "Remarques");
}
