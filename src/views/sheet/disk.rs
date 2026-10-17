//! The file behind the grid: loading, the format remembered per file, saving in the background, reloading what changed elsewhere.

use super::*;

/// A setting or an edit waits this long before being written to disk.
pub(super) const SAVE_DELAY: Duration = Duration::from_millis(400);

/// The file as just read.
pub struct Loaded {
    pub(super) table: Table,
    pub(super) format: Format,
    pub(super) stamp: Option<(SystemTime, u64)>,
}

pub(super) fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Reads the file. Without `forced`, encoding, delimiter and header are guessed, except
/// those the user has already chosen for this file.
pub fn load(path: &Path, forced: Option<Format>) -> Result<Loaded, String> {
    let fail = |e: std::io::Error| format!("{} {} : {e}", tr("Cannot open", "Impossible d'ouvrir"), path.display());
    let size = fs::metadata(path).map_err(fail)?.len();
    if size > table::MAX_BYTES as u64 {
        return Err(format!("{} : {}", path.display(), tr("file too large", "fichier trop gros")));
    }
    // ponytail: synchronous read and parse (100 MB: about 0.3 s); move to a background
    // task with a "loading" state if files of several hundred MB get opened.
    let bytes = fs::read(path).map_err(fail)?;
    let format = forced.or_else(|| saved_format(path)).unwrap_or_else(|| table::detect(&bytes));
    let table = Table::parse(table::decode(bytes, format.encoding), format.delimiter);
    Ok(Loaded { table, format, stamp: stamp_of(path) })
}

/// Settings the user chose for this file: `path ⇥ delimiter ⇥ encoding ⇥ header`.
pub(super) fn saved_format(path: &Path) -> Option<Format> {
    let key = path.to_str()?;
    vault::load_file("tables").lines().find_map(|line| {
        let mut parts = line.split('\t');
        (parts.next()? == key).then_some(())?;
        Some(Format {
            delimiter: parts.next()?.parse().ok()?,
            encoding: Encoding::from_label(parts.next()?)?,
            header: parts.next()? == "1",
        })
    })
}

pub(super) fn remember_format(path: &Path, format: Option<Format>) {
    let Some(key) = path.to_str().filter(|k| !k.contains(['\t', '\n'])) else {
        return;
    };
    let mut lines: Vec<String> =
        vault::load_file("tables").lines().filter(|l| l.split('\t').next() != Some(key)).map(String::from).collect();
    if let Some(f) = format {
        lines.push(format!("{key}\t{}\t{}\t{}", f.delimiter, f.encoding.label(), f.header as u8));
    }
    vault::save_file("tables", &lines.join("\n"));
}

/// Last generation written: a late write never replaces a newer one.
#[derive(Default)]
pub(super) struct Disk(Mutex<usize>);

impl Disk {
    pub(super) fn write(&self, generation: usize, path: &Path, table: &Table, encoding: Encoding) -> Result<(), WriteError> {
        let mut last = self.0.lock().unwrap();
        if *last >= generation {
            return Ok(());
        }
        // Atomic write: a crash never leaves a truncated table.
        let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().unwrap_or_default().to_string_lossy()));
        let result = (|| {
            let file = fs::File::create(&tmp)?;
            if let Ok(meta) = fs::metadata(path) {
                fs::set_permissions(&tmp, meta.permissions()).ok();
            }
            let mut out = BufWriter::with_capacity(1 << 16, file);
            table.write_to(&mut out, encoding)?;
            let file = out.into_inner().map_err(|e| e.into_error())?;
            // Without `fsync`, a power cut can leave the rename without the data.
            file.sync_all()?;
            fs::rename(&tmp, path)?;
            Ok(())
        })();
        if result.is_err() {
            fs::remove_file(&tmp).ok();
        } else {
            *last = generation;
        }
        result
    }
}

impl Sheet {
    pub fn moved(&mut self, path: PathBuf) {
        self.path = path;
    }

    /// An empty table keeps one empty row to write in.
    pub(super) fn fresh_table(&mut self) {
        if self.table.rows() == 0 {
            self.table.insert_row(0);
        }
        self.measure();
    }

    /// Takes a version reread from disk, keeping the cursor's place.
    pub(super) fn replace(&mut self, loaded: Loaded) {
        self.format = loaded.format;
        self.table = loaded.table;
        self.stamp = loaded.stamp;
        self.undo.clear();
        self.redo.clear();
        self.edit = None;
        self.dirty = false;
        self.fresh_table();
        self.clamp();
    }

    /// Another program rewrote the file and nothing is pending here: reread it.
    pub fn reload_if_changed(&mut self, cx: &mut Context<Self>) {
        if self.dirty || self.stamp == stamp_of(&self.path) {
            return;
        }
        match load(&self.path, Some(self.format)) {
            Ok(loaded) => self.replace(loaded),
            Err(e) => cx.emit(SheetEvent::Error(e)),
        }
        cx.notify();
    }

    /// Changes the read settings of this file (`None`: guess them again):
    /// what is pending is written, then the file is reread that way, and the choice remembered.
    pub fn reformat(&mut self, format: Option<Format>, cx: &mut Context<Self>) {
        self.flush(cx);
        if self.dirty {
            return;
        }
        remember_format(&self.path, format);
        match load(&self.path, format) {
            Ok(loaded) => self.replace(loaded),
            Err(e) => cx.emit(SheetEvent::Error(e)),
        }
        cx.notify();
    }

    pub fn format(&self) -> Format {
        self.format
    }

    /// Writes in the background: a big file does not freeze the window.
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        let (generation, snapshot, path, encoding, disk) =
            (self.generation, self.table.snapshot(), self.path.clone(), self.format.encoding, self.disk.clone());
        cx.spawn(async move |this, cx| {
            let written = cx
                .background_executor()
                .spawn(async move { disk.write(generation, &path, &snapshot, encoding).map(|()| stamp_of(&path)) })
                .await;
            this.update(cx, |this, cx| match written {
                Ok(stamp) => {
                    this.stamp = stamp.or(this.stamp);
                    this.dirty &= this.generation != generation;
                }
                Err(e) => cx.emit(SheetEvent::Error(this.not_saved(e))),
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn not_saved(&self, e: WriteError) -> String {
        format!("{} : {e}", tr("Table not saved", "Tableau non enregistré"))
    }

    /// Writes what is pending right away (file change, close).
    pub fn flush(&mut self, cx: &mut Context<Self>) {
        self.finish(cx);
        if !self.dirty {
            return;
        }
        match self.disk.write(self.generation, &self.path, &self.table, self.format.encoding) {
            Ok(()) => {
                self.dirty = false;
                self.stamp = stamp_of(&self.path).or(self.stamp);
            }
            Err(e) => cx.emit(SheetEvent::Error(self.not_saved(e))),
        }
    }
}
