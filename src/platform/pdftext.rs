//! The text of a vault's PDFs, read by poppler's `pdftotext` and kept in the app's cache folder, one
//! file per PDF: nothing is written in the vault. Pages and cache names are `logic/pdftext.rs`.

use std::{
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant, UNIX_EPOCH},
};

use super::{config, vault};
use crate::logic::pdftext;

/// The program that reads a PDF's text (poppler).
pub const PROGRAM: &str = "pdftotext";
/// Longest wait for one PDF: past it, the program is stopped and the PDF left out until it changes.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// Why a PDF has no text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The program is not installed, or cannot be started: no PDF can be read.
    Missing,
    /// This PDF could not be read (damaged, locked, too slow): the reason.
    Failed(String),
}

/// What a PDF was when it was read: its size and when it was last written (nanoseconds). Another stamp
/// means another file, read again.
pub type Stamp = (u64, u128);

pub fn stamp(pdf: &Path) -> Option<Stamp> {
    let meta = fs::metadata(pdf).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    Some((meta.len(), modified))
}

/// Where the text of `pdf`, as it is at `stamp`, is cached.
pub fn cached_path(pdf: &Path, (size, modified): Stamp) -> PathBuf {
    config::cache_dir().join("pdf-text").join(pdftext::cache_name(&pdf.to_string_lossy(), size, modified))
}

/// The text of `pdf` (pages separated by `\f`): from the cache when it has this very file, otherwise
/// read by `program`, stopped after `timeout`, then cached.
pub fn text(program: &OsStr, pdf: &Path, stamp: Stamp, timeout: Duration) -> Result<String, Failure> {
    let cache = cached_path(pdf, stamp);
    if let Ok(text) = fs::read_to_string(&cache) {
        return Ok(text);
    }
    let text = run(program, pdf, timeout)?;
    // A cache that cannot be written only costs a second reading.
    let _ = cache.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| vault::write(&cache, &text));
    Ok(text)
}

/// `pdftotext -enc UTF-8 -q <pdf> -`: the text on its output, in reading order.
fn run(program: &OsStr, pdf: &Path, timeout: Duration) -> Result<String, Failure> {
    let mut child = super::command(program)
        .args(["-enc", "UTF-8", "-q"])
        .arg(pdf)
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        // Not found, or found on a `PATH` folder that cannot be entered: either way no PDF can be read.
        .map_err(|_| Failure::Missing)?;
    // Read on a thread of its own: a long text fills the pipe, and the program waits until it is read.
    let mut out = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        out.read_to_end(&mut bytes).map(|_| bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Failed(format!("stopped after {} s", timeout.as_secs())));
            }
            Err(e) => return Err(Failure::Failed(e.to_string())),
        }
    };
    let bytes = reader.join().map_err(|_| Failure::Failed("output lost".into()))?.map_err(|e| Failure::Failed(e.to_string()))?;
    if !status.success() {
        return Err(Failure::Failed(format!("{program:?} {status}")));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_program_says_so() {
        let pdf = std::env::temp_dir().join("bref-no-such.pdf");
        let found = run(OsStr::new("bref-no-such-program"), &pdf, TIMEOUT);
        assert_eq!(found, Err(Failure::Missing));
    }

    /// A stand-in for `pdftotext`: prints two pages, or hangs.
    #[cfg(unix)]
    fn fake(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("pdftotext");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn reads_once_then_from_the_cache() {
        let root = crate::platform::scratch::TempDir::new("pdftext-read");
        let _home = config::isolate(root.as_path());
        let pdf = root.as_path().join("a.pdf");
        fs::write(&pdf, "%PDF").unwrap();
        let stamp = stamp(&pdf).unwrap();
        let program = fake(root.as_path(), r"printf 'page one\n\fpage two\n\f'");
        assert_eq!(text(program.as_os_str(), &pdf, stamp, TIMEOUT).unwrap(), "page one\n\u{c}page two\n\u{c}");
        assert!(cached_path(&pdf, stamp).is_file());
        // Cached: the program is no longer needed.
        assert_eq!(text(OsStr::new("bref-no-such-program"), &pdf, stamp, TIMEOUT).unwrap(), "page one\n\u{c}page two\n\u{c}");
        // Nothing was written next to the PDF.
        let names: Vec<_> = fs::read_dir(root.as_path()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names.len(), 3, "{names:?}"); // a.pdf, the fake program, and the isolated .cache
    }

    #[cfg(unix)]
    #[test]
    fn a_slow_or_failing_read_is_a_failure() {
        let root = crate::platform::scratch::TempDir::new("pdftext-slow");
        let pdf = root.as_path().join("a.pdf");
        let slow = fake(root.as_path(), "sleep 5");
        let started = Instant::now();
        assert!(matches!(run(slow.as_os_str(), &pdf, Duration::from_millis(200)), Err(Failure::Failed(_))));
        assert!(started.elapsed() < Duration::from_secs(3));
        let broken = fake(root.as_path(), "exit 1");
        assert!(matches!(run(broken.as_os_str(), &pdf, TIMEOUT), Err(Failure::Failed(_))));
    }
}
