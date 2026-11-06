//! The file behind the grid: loading, the format remembered per file, saving in the background, reloading what changed elsewhere.

use crate::platform::config;
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
    let format = forced.or_else(|| kept(path).format).unwrap_or_else(|| table::detect(&bytes));
    // Checked after decoding: Latin-1 and UTF-16 text is longer than the file.
    let too_big = || format!("{} : {}", path.display(), tr("file too large", "fichier trop gros"));
    let text = table::decode_within(bytes, format.encoding, table::MAX_BYTES).ok_or_else(too_big)?;
    let table = Table::parse(text, format.delimiter);
    Ok(Loaded { table, format, stamp: stamp_of(path) })
}

/// What is kept for a file in the config file `tables`: the read settings the user chose (none: guessed), and
/// the widths of the columns they set.
#[derive(Default, Debug, PartialEq)]
pub(super) struct Kept {
    pub(super) format: Option<Format>,
    pub(super) widths: Vec<Option<f32>>,
}

/// A line of `tables`: `path ⇥ delimiter ⇥ encoding ⇥ header ⇥ widths`. The settings are empty when only the
/// widths are kept; a line of an older version has no widths.
fn read_line(line: &str) -> Option<(&str, Kept)> {
    let mut parts = line.split('\t');
    let key = parts.next().filter(|k| !k.is_empty())?;
    let (delimiter, encoding, header) = (parts.next()?, parts.next()?, parts.next()?);
    let format = match (delimiter.parse().ok(), Encoding::from_label(encoding)) {
        (Some(delimiter), Some(encoding)) => Some(Format { delimiter, encoding, header: header == "1" }),
        _ => None,
    };
    Some((key, Kept { format, widths: parts.next().map(widths::parse).unwrap_or_default() }))
}

/// The line for `key`; none when nothing is kept.
fn write_line(key: &str, kept: &Kept) -> Option<String> {
    let widths = widths::write(&kept.widths);
    let format = match kept.format {
        Some(f) => format!("{}\t{}\t{}", f.delimiter, f.encoding.label(), f.header as u8),
        None if widths.is_empty() => return None,
        None => "\t\t".to_string(),
    };
    Some(match widths.is_empty() {
        true => format!("{key}\t{format}"),
        false => format!("{key}\t{format}\t{widths}"),
    })
}

/// What is kept for this file.
pub(super) fn kept(path: &Path) -> Kept {
    let Some(key) = path.to_str() else {
        return Kept::default();
    };
    config::load(config::File::Tables).lines().filter_map(read_line).find(|(k, _)| *k == key).map(|(_, kept)| kept).unwrap_or_default()
}

/// Changes what is kept for this file; the config file is only written if it changes.
fn keep(path: &Path, change: impl FnOnce(&mut Kept)) {
    let Some(key) = path.to_str().filter(|k| !k.contains(['\t', '\n'])) else {
        return;
    };
    let text = config::load(config::File::Tables);
    let mut kept = text.lines().filter_map(read_line).find(|(k, _)| *k == key).map(|(_, kept)| kept).unwrap_or_default();
    change(&mut kept);
    let mut lines: Vec<String> = text.lines().filter(|l| l.split('\t').next() != Some(key)).map(String::from).collect();
    lines.extend(write_line(key, &kept));
    let new = lines.join("\n");
    if new != text {
        config::keep(config::File::Tables, &new);
    }
}

pub(super) fn remember_format(path: &Path, format: Option<Format>) {
    keep(path, |kept| kept.format = format);
}

pub(super) fn remember_widths(path: &Path, chosen: &[Option<f32>]) {
    keep(path, |kept| kept.widths = chosen.to_vec());
}

/// `from` (a table, or a folder holding some) became `to` inside the app: what is kept for its tables follows.
pub fn move_tables(from: &Path, to: &Path) {
    if let Some(text) = moved_lines(&config::load(config::File::Tables), from, to) {
        config::keep(config::File::Tables, &text);
    }
}

/// The `tables` text once `from` became `to`; none if no line changes.
fn moved_lines(text: &str, from: &Path, to: &Path) -> Option<String> {
    let moved = |line: &str| -> Option<String> {
        let (key, rest) = line.split_once('\t')?;
        let rest_of = Path::new(key).strip_prefix(from).ok()?;
        let new = if rest_of.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest_of) };
        new.to_str().filter(|k| !k.contains(['\t', '\n'])).map(|k| format!("{k}\t{rest}"))
    };
    let (mut moved_lines, mut others) = (Vec::new(), Vec::new());
    for line in text.lines() {
        match moved(line) {
            Some(new) => moved_lines.push(new),
            None => others.push(line.to_string()),
        }
    }
    if moved_lines.is_empty() {
        return None;
    }
    // A line already there for a new path (a file renamed outside the app long ago): the moved one wins.
    let keys: std::collections::HashSet<String> = moved_lines.iter().filter_map(|l| l.split('\t').next()).map(String::from).collect();
    others.retain(|l| !keys.contains(l.split('\t').next().unwrap_or_default()));
    others.extend(moved_lines);
    Some(others.join("\n"))
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
        let (tmp, file) = crate::platform::vault::staging(path)?;
        let result = (|| {
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
        self.history.clear();
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
        format!("{} « {} » : {e}", tr("Table not saved", "Tableau non enregistré"), self.path.display())
    }

    /// Whether a write or a cell edit still needs saving.
    pub fn is_dirty(&self) -> bool {
        self.dirty || self.edit.is_some()
    }

    /// Writes what is pending right away (file change, close), waiting for any older write.
    /// False leaves the data and undo history alive for a retry.
    pub fn flush(&mut self, cx: &mut Context<Self>) -> bool {
        self.finish(cx);
        if !self.dirty {
            return true;
        }
        match self.disk.write(self.generation, &self.path, &self.table, self.format.encoding) {
            Ok(()) => {
                self.dirty = false;
                self.stamp = stamp_of(&self.path).or(self.stamp);
            }
            Err(e) => cx.emit(SheetEvent::Error(self.not_saved(e))),
        }
        !self.dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_the_tables_lines() {
        let semicolon = Format { delimiter: b';', encoding: Encoding::Utf8, header: true };
        // A line of the previous version: four fields, no widths.
        let (key, old) = read_line("/v/a.csv\t59\tUTF-8\t1").unwrap();
        assert_eq!((key, old.format, old.widths.len()), ("/v/a.csv", Some(semicolon), 0));
        // Widths alone: the settings stay guessed.
        let only = Kept { format: None, widths: vec![None, Some(12.)] };
        let line = write_line("/v/b.csv", &only).unwrap();
        assert_eq!(line, "/v/b.csv\t\t\t\t,12");
        assert_eq!(read_line(&line).unwrap().1, only);
        let both = Kept { format: Some(semicolon), widths: vec![Some(5.5)] };
        assert_eq!(read_line(&write_line("/v/c.csv", &both).unwrap()).unwrap().1, both);
        // Nothing to keep: no line.
        assert_eq!(write_line("/v/d.csv", &Kept::default()), None);
        assert_eq!(write_line("/v/d.csv", &Kept { format: None, widths: vec![None] }), None);
        // Malformed widths are automatic, a broken line is skipped.
        assert_eq!(read_line("/v/e.csv\t44\tUTF-8\t0\tabc,-1,7").unwrap().1.widths, [None, None, Some(7.)]);
        assert!(read_line("/v/f.csv\t44").is_none());
        assert!(read_line("").is_none());
    }

    #[test]
    fn moved_tables_keep_their_line() {
        let text = "/v/a.csv\t44\tUTF-8\t1\t9\n/v/dir/b.csv\t\t\t\t4\n/v/other.csv\t44\tUTF-8\t0\n/v/new.csv\t9\tUTF-8\t0";
        // A file renamed: its line follows, an old line for the new name gives way.
        let renamed = moved_lines(text, Path::new("/v/a.csv"), Path::new("/v/new.csv")).unwrap();
        assert_eq!(renamed, "/v/dir/b.csv\t\t\t\t4\n/v/other.csv\t44\tUTF-8\t0\n/v/new.csv\t44\tUTF-8\t1\t9");
        // A folder moved: the tables inside follow, not a file whose name only starts the same.
        let moved = moved_lines(text, Path::new("/v/dir"), Path::new("/v/x/dir")).unwrap();
        assert!(moved.contains("/v/x/dir/b.csv\t\t\t\t4") && !moved.contains("/v/dir/"));
        assert_eq!(moved_lines(text, Path::new("/v/d"), Path::new("/v/e")), None);
        assert_eq!(moved_lines("", Path::new("/v/a.csv"), Path::new("/v/b.csv")), None);
    }
}
