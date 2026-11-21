//! The vault search reads the vault's PDFs: their matches come after the notes with their page, open the PDF,
//! and are never replaced. Their text is cached outside the vault; without poppler, it is said once.
//! A link to a page of a PDF opens the PDF, not a heading.

use super::*;
use crate::platform::pdftext;
use gpui::Entity;

type Cx = gpui::VisualTestContext;

fn rows(shell: &Entity<Shell>, cx: &mut Cx) -> Vec<String> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    shell.read_with(cx, |s, _| s.vault_rows())
}

/// The text `pdftotext` would give for `pdf`, put where the app caches it: the journey needs no poppler.
fn cache(pdf: &Path, text: &str) {
    let path = pdftext::cached_path(pdf, pdftext::stamp(pdf).unwrap());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn toasts_with(shell: &Entity<Shell>, cx: &mut Cx, part: &str) -> usize {
    shell.read_with(cx, |s, _| s.toasts.iter().filter(|t| t.text.contains(part)).count())
}

#[gpui::test]
fn the_vault_search_reads_pdfs(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "pdf-find");
    fs::write(root.join("Notes.md"), "# Notes\n\nun zèbre\n").unwrap();
    fs::write(root.join("Article.pdf"), "%PDF-1.4 stand-in").unwrap();
    cache(&root.join("Article.pdf"), "Introduction\n\u{c}La page deux parle\nd'un zèbre.\n\u{c}");
    // Read from the cache only: no program is ever run.
    shell.update(cx, |s, cx| {
        s.pdfs.program = "bref-no-such-program".into();
        s.rescan(&root, cx);
        s.open_note(&root.join("Notes.md"), cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-shift-f");
    cx.simulate_input("zèbre");
    // The note first, then the PDFs under their title, each match with its page.
    assert_eq!(rows(&shell, cx), ["note Notes", "hit 3", "pdfs", "note Article.pdf", "hit p. 2"]);
    assert!(shell.read_with(cx, |s, _| s.toasts.is_empty()), "nothing to say: the cache answered");
    // Enter on the PDF's match: the PDF opens in the system's application, the note stays.
    cx.simulate_keystrokes("down down down down enter");
    let (opened, path) = shell.read_with(cx, |s, _| (s.opened.clone(), s.pane().path.clone()));
    assert_eq!((opened, path), (vec![root.join("Article.pdf")], Some(root.join("Notes.md"))));
    // Replace: the note only, and the count says what is left in the PDFs.
    cx.simulate_keystrokes("secondary-shift-f secondary-r");
    cx.simulate_input("okapi");
    cx.simulate_keystrokes("secondary-enter");
    let title = shell.read_with(cx, |s, cx| s.palette.as_ref().map(|p| p.read(cx).question().unwrap_or_default().to_string()));
    assert!(title.is_some_and(|t| t.contains("1 passages dans des PDF ne sont pas remplacés")));
    cx.simulate_keystrokes("enter");
    assert_eq!(fs::read_to_string(root.join("Notes.md")).unwrap(), "# Notes\n\nun okapi\n");
    assert_eq!(fs::read_to_string(root.join("Article.pdf")).unwrap(), "%PDF-1.4 stand-in");
    assert_eq!(rows(&shell, cx), ["pdfs", "note Article.pdf", "hit p. 2"]);
    // Only PDFs match: nothing replaced, and said why.
    cx.simulate_keystrokes("secondary-shift-f secondary-enter");
    assert!(shell.read_with(cx, |s, _| s.palette.is_none() && s.told(Tone::Info, "ne sont pas remplacés")));
    // A PDF that goes away leaves the results.
    fs::remove_file(root.join("Article.pdf")).unwrap();
    watch(cx);
    assert!(rows(&shell, cx).is_empty());
    // Nothing of the search was written in the vault but the replaced note.
    let mut names: Vec<_> = fs::read_dir(&root).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with('.')).collect();
    names.sort();
    assert_eq!(names, ["Notes.md"]);
}

/// No poppler: said once, however many PDFs; the notes are still searched.
#[gpui::test]
fn without_poppler_it_is_said_once(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "pdf-find-missing");
    fs::write(root.join("Notes.md"), "# Notes\n\nmot\n").unwrap();
    fs::write(root.join("Un.pdf"), "%PDF").unwrap();
    fs::write(root.join("Deux.pdf"), "%PDF").unwrap();
    shell.update(cx, |s, cx| {
        s.pdfs.program = "bref-no-such-program".into();
        s.rescan(&root, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-shift-f");
    cx.simulate_input("mot");
    assert_eq!(rows(&shell, cx), ["note Notes", "hit 3"]);
    assert_eq!(toasts_with(&shell, cx, "poppler"), 1);
    // Another PDF, another search: nothing more is said.
    fs::write(root.join("Trois.pdf"), "%PDF").unwrap();
    watch(cx);
    cx.simulate_keystrokes("secondary-shift-f");
    cx.simulate_input("mo");
    assert_eq!(rows(&shell, cx), ["note Notes", "hit 3"]);
    // The first message has had its time; no second one came.
    assert_eq!(toasts_with(&shell, cx, "poppler"), 0);
}

/// With the program: each PDF is read once, in the background, then from the cache; a changed PDF is read again.
#[cfg(unix)]
#[gpui::test]
fn pdfs_are_read_once_and_again_when_changed(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (root, shell, cx, _config) = start(cx, "pdf-find-read");
    // A stand-in for pdftotext, outside the vault: prints the PDF's own bytes as its text, and counts its runs.
    let bin = TempDir::new("pdf-find-bin");
    let (program, runs) = (bin.join("pdftotext"), bin.join("runs"));
    fs::write(&program, format!("#!/bin/sh\necho run >> '{}'\ncat \"$4\"\n", runs.display())).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("Livre.pdf"), "un zèbre\n").unwrap();
    shell.update(cx, |s, cx| {
        s.pdfs.program = program.clone().into();
        s.rescan(&root, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("secondary-shift-f");
    cx.simulate_input("zèbre");
    assert_eq!(rows(&shell, cx), ["pdfs", "note Livre.pdf", "hit p. 1"]);
    let count = || fs::read_to_string(&runs).map_or(0, |r| r.lines().count());
    assert_eq!(count(), 1);
    // Searched again, the vault rescanned: not read again.
    cx.simulate_input("s");
    watch(cx);
    assert_eq!(count(), 1);
    // Rewritten: read again, its new text found.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(root.join("Livre.pdf"), "deux zèbres, plus long\n").unwrap();
    watch(cx);
    assert_eq!(rows(&shell, cx), ["pdfs", "note Livre.pdf", "hit p. 1"]);
    assert_eq!(count(), 2);
}

/// `[[Article.pdf#page=2]]` opens the PDF: `page=2` is not read as a heading of the note.
#[gpui::test]
fn a_link_to_a_page_opens_the_pdf(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "pdf-page-link");
    fs::write(root.join("Article.pdf"), "%PDF").unwrap();
    fs::write(root.join("Lecture.md"), "# Lecture\n\nvoir [[Article.pdf#page=2]]\n\n## page=2\n").unwrap();
    shell.update(cx, |s, cx| {
        s.rescan(&root, cx);
        s.open_note(&root.join("Lecture.md"), cx);
    });
    cx.run_until_parked();
    let target = crate::logic::embed::link_target("Article.pdf#page=2").unwrap();
    let before = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).caret());
    shell.update_in(cx, |s, window, cx| s.open_at(&target, false, window, cx));
    cx.run_until_parked();
    let (opened, path, cursor, notes) = shell.read_with(cx, |s, cx| (s.opened.clone(), s.pane().path.clone(), s.pane().editor.read(cx).caret(), s.notes.len()));
    assert_eq!(opened, [root.join("Article.pdf")]);
    assert_eq!((path, cursor, notes), (Some(root.join("Lecture.md")), before, 1), "no jump to the heading, no note made");
}
