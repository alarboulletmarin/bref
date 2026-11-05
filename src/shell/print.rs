//! The note saved as a PDF: laid out for print (`ui::print`), drawn by svg2pdf (`platform::pdf`)
//! off the UI thread, written under a free name.

use std::collections::HashMap;
use std::io;

use super::*;
use crate::platform::pdf;
use crate::ui::print::{self, Found};

impl Shell {
    /// Asks where to save the note of the active tab as PDF, with its unsaved edits, and saves it there.
    pub(super) fn export_pdf(&mut self, cx: &mut Context<Self>) {
        if self.pane().file.is_some() {
            self.say(Tone::Info, tr("Only a note can be saved as PDF", "Seule une note s'enregistre en PDF"), cx);
            return;
        }
        let editor = self.pane().editor.read(cx);
        let text = editor.text().to_string();
        let lookups = editor.lookups();
        let path = self.pane().path.clone();
        let title = vault::title_of(&text);
        let name = path.as_deref().map(vault::stem).filter(|s| !s.is_empty()).unwrap_or_else(|| if title.is_empty() { "Note".into() } else { title.clone() });
        let dir = path.as_deref().and_then(Path::parent).map(Path::to_path_buf).or(self.vault.clone()).unwrap_or_default();
        let chosen = cx.prompt_for_new_path(&dir, Some(&format!("{name}.pdf")));
        cx.spawn(async move |this, cx| {
            // Cancelled: nothing is written, nothing is said.
            let Ok(Ok(Some(chosen))) = chosen.await else { return };
            this.update(cx, |this, cx| this.say(Tone::Busy, tr("Saving the PDF…", "Enregistrement du PDF…"), cx)).ok();
            let saved = cx.background_executor().spawn(async move { save(&text, &title, lookups, &chosen) }).await;
            this.update(cx, |this, cx| {
                match saved {
                    Ok((path, missing)) => {
                        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let mut said = format!("{} « {file} »", tr("PDF saved", "PDF enregistré"));
                        if !missing.is_empty() {
                            said.push_str(&format!(" ; {} : {}", tr("not printed", "non imprimé"), missing.join(", ")));
                        }
                        this.say(if missing.is_empty() { Tone::Done } else { Tone::Info }, said, cx);
                    }
                    Err(e) => this.say(Tone::Failed, format!("{} : {}", tr("PDF not saved", "PDF non enregistré"), crate::lang::io_reason(&e)), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// Lays out `text`, makes its PDF and writes it at `chosen`, given the `.pdf` extension; a name already
/// taken gets a number, nothing is overwritten. The path written, and what could not be printed.
fn save(text: &str, title: &str, (dirs, images, notes): (Vec<PathBuf>, HashMap<String, PathBuf>, HashMap<String, PathBuf>), chosen: &Path) -> io::Result<(PathBuf, Vec<String>)> {
    let find = |written: &str| {
        if let Some(file) = editor::find_picture(&dirs, &images, written) {
            let svg = file.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
            if svg {
                return fs::read_to_string(&file).map_or(Found::Missing, Found::Svg);
            }
            // A damaged picture would stop the whole PDF: it is left out, and said.
            let drawn = |bytes: &[u8]| print::picture(bytes).and_then(|s| ui::svg::parse(&s)).is_some_and(|t| pdf::draws(&t));
            return fs::read(&file).ok().filter(|b| drawn(b)).map_or(Found::Missing, Found::Picture);
        }
        embed::link_target(written)
            .filter(|t| !vault::is_file_shown(Path::new(&t.note)))
            .and_then(|t| Some((fs::read_to_string(notes.get(&t.note.to_lowercase())?).ok()?, t)))
            .and_then(|(text, t)| embed::section(&text, &t.anchor))
            .map_or(Found::Missing, Found::Section)
    };
    let printed = print::print(text, &find);
    let sheets = printed.pages.into_iter().map(|p| Some((ui::svg::parse(&p.svg)?, p.links))).collect::<Option<Vec<pdf::Sheet>>>();
    let (bytes, outlined) = pdf::document(&sheets.ok_or_else(|| io::Error::other("SVG"))?, print::PAGE, title).map_err(io::Error::other)?;
    let mut missing = printed.missing;
    missing.extend(outlined.into_iter().map(|n| format!("{} {n}", tr("searchable text of page", "texte cherchable de la page"))));
    let path = if chosen.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) { chosen.to_path_buf() } else { chosen.with_extension("pdf") };
    let path = if path.exists() { vault::free_path(path.parent().unwrap_or(Path::new("")), &vault::stem(&path), "pdf") } else { path };
    vault::write(&path, bytes)?;
    Ok((path, missing))
}
