//! The text of the vault's PDFs, for the vault search: read one at a time in the background by poppler's
//! `pdftotext` (`platform/pdftext.rs`), cached outside the vault, and read again only when a PDF changes.
//! Nothing happens until the vault search is first shown.

use std::{collections::{HashMap, VecDeque}, ffi::OsString};

use super::*;
use crate::logic::pdftext;
use crate::platform::pdftext::{self as reader, Failure, Stamp};
use gpui::Task;

/// A PDF whose text was read.
pub(super) struct PdfDoc {
    pub path: PathBuf,
    /// Its file name, extension included: how the results list it.
    pub name: String,
    stamp: Stamp,
    /// Its text, form feeds made line breaks (`pdftext::searchable`).
    pub body: Arc<str>,
    /// Where each page starts in `body`.
    pub pages: Arc<[usize]>,
}

pub(super) struct Pdfs {
    /// The vault these PDFs belong to: another vault starts again.
    root: Option<PathBuf>,
    docs: Vec<PdfDoc>,
    /// PDFs that could not be read, as they were then: left alone until they change.
    failed: HashMap<PathBuf, Stamp>,
    /// PDFs found new or changed, waiting for their turn.
    queue: VecDeque<(PathBuf, Stamp)>,
    /// The PDFs were compared with the disk since the vault last changed.
    surveyed: bool,
    /// The survey or the reading in progress; one at a time.
    busy: Option<Task<()>>,
    /// The program is not installed: said once, then nothing more is tried.
    missing: bool,
    /// Bumped at each change of `docs`: the vault search runs again.
    version: u64,
    /// The program that reads a PDF; a test points it elsewhere.
    pub program: OsString,
}

impl Default for Pdfs {
    fn default() -> Self {
        Pdfs {
            root: None,
            docs: Vec::new(),
            failed: HashMap::new(),
            queue: VecDeque::new(),
            surveyed: false,
            busy: None,
            missing: false,
            version: 0,
            program: reader::PROGRAM.into(),
        }
    }
}

impl Pdfs {
    pub fn docs(&self) -> &[PdfDoc] {
        &self.docs
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// PDFs found but not read yet, the one being read included.
    pub fn waiting(&self) -> usize {
        self.queue.len() + usize::from(self.busy.is_some() && self.surveyed)
    }

    /// The vault changed on disk: compare the PDFs with it again.
    pub fn stale(&mut self) {
        self.surveyed = false;
    }
}

/// Whether the vault search reads `path`.
fn is_pdf(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

impl Shell {
    /// One step of reading the PDFs, if none is running: compare them with the disk, or read the next one.
    /// Each step that ends searches again, which takes the next step.
    pub(super) fn read_pdfs(&mut self, cx: &mut Context<Self>) {
        if self.pdfs.root != self.vault {
            let program = std::mem::take(&mut self.pdfs.program);
            self.pdfs = Pdfs { root: self.vault.clone(), missing: self.pdfs.missing, program, ..Pdfs::default() };
        }
        if self.pdfs.busy.is_some() || self.pdfs.missing || self.scanning || self.vault.is_none() {
            return;
        }
        if !self.pdfs.surveyed {
            let paths: Vec<PathBuf> = self.images.iter().filter(|path| is_pdf(path)).cloned().collect();
            let known: HashMap<PathBuf, Stamp> = (self.pdfs.docs.iter().map(|doc| (doc.path.clone(), doc.stamp))).chain(self.pdfs.failed.iter().map(|(path, stamp)| (path.clone(), *stamp))).collect();
            let work = cx.background_executor().spawn(async move {
                let changed = paths.iter().filter_map(|path| reader::stamp(path).filter(|stamp| known.get(path) != Some(stamp)).map(|stamp| (path.clone(), stamp))).collect::<VecDeque<_>>();
                (paths, changed)
            });
            self.pdfs.busy = Some(cx.spawn(async move |this, cx| {
                let (paths, changed) = work.await;
                this.update(cx, |this, cx| {
                    let pdfs = &mut this.pdfs;
                    let before = pdfs.docs.len();
                    pdfs.docs.retain(|doc| paths.contains(&doc.path));
                    pdfs.failed.retain(|path, _| paths.contains(path));
                    if pdfs.docs.len() != before {
                        pdfs.version += 1;
                    }
                    (pdfs.queue, pdfs.surveyed, pdfs.busy) = (changed, true, None);
                    this.vault_search(cx);
                    cx.notify();
                })
                .ok();
            }));
            return;
        }
        let Some((path, stamp)) = self.pdfs.queue.pop_front() else { return };
        let program = self.pdfs.program.clone();
        let read = path.clone();
        let work = cx.background_executor().spawn(async move { reader::text(&program, &read, stamp, reader::TIMEOUT) });
        self.pdfs.busy = Some(cx.spawn(async move |this, cx| {
            let text = work.await;
            this.update(cx, |this, cx| {
                this.pdfs.busy = None;
                this.took_pdf(path, stamp, text, cx);
                this.vault_search(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    fn took_pdf(&mut self, path: PathBuf, stamp: Stamp, text: Result<String, Failure>, cx: &mut Context<Self>) {
        let pdfs = &mut self.pdfs;
        pdfs.docs.retain(|doc| doc.path != path);
        match text {
            Ok(text) => {
                let pages: Arc<[usize]> = pdftext::page_starts(&text).into();
                let body: Arc<str> = pdftext::searchable(text).into();
                let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                pdfs.docs.push(PdfDoc { path, name, stamp, body, pages });
                pdfs.version += 1;
            }
            Err(Failure::Missing) => {
                pdfs.missing = true;
                pdfs.queue.clear();
                self.say(
                    Tone::Info,
                    tr("PDFs are not searched: install poppler, which provides pdftotext", "Les PDF ne sont pas cherchés : installez poppler, qui fournit pdftotext"),
                    cx,
                );
            }
            // ponytail: a PDF that cannot be read is left out without a word; listed on the health page if
            // people wonder why one is never found.
            Err(Failure::Failed(_)) => {
                pdfs.failed.insert(path, stamp);
            }
        }
    }
}
