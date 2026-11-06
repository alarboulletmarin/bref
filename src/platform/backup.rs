//! Backup of a vault as a `.tar.gz` archive.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use super::vault::free_path;
use crate::lang::tr;

/// Backup: the whole vault (trash included) in a `<vault> <date>.tar.gz` archive
/// in folder `dir`, numbered if the name is taken. Blocking: run off the UI thread.
// The archive is written here rather than by the system's `tar`: on Windows, both the one
// Windows ships and Git for Windows' crash (access violation) on a name outside the ANSI
// code page, such as `中文.md`. flate2 compresses; the tar format is a few headers.
pub fn backup(root: &Path, dir: &Path, date: &str) -> io::Result<PathBuf> {
    let Some(name) = root.file_name() else {
        return Err(io::Error::other(tr("this vault has no parent folder", "ce coffre n'a pas de dossier parent")));
    };
    // Inside the vault, the archive would contain itself.
    if dir.starts_with(root) {
        return Err(io::Error::other(tr("choose a folder outside the vault", "choisir un dossier hors du coffre")));
    }
    let to = free_path(dir, &format!("{} {date}", name.to_string_lossy()), "tar.gz");
    let written = fs::File::create_new(&to).and_then(|file| {
        let mut gz = flate2::write::GzEncoder::new(io::BufWriter::new(file), flate2::Compression::default());
        tar_entry(&mut gz, root, &name.to_string_lossy())?;
        // The end of a tar: two empty blocks.
        gz.write_all(&[0; 1024])?;
        gz.finish()?.into_inner().map_err(|e| e.into_error())?.sync_all()
    });
    if let Err(e) = written {
        // An incomplete archive must not pass for a backup.
        let _ = fs::remove_file(&to);
        return Err(e);
    }
    Ok(to)
}

/// Writes `path` into the tar under `name`, and a folder's contents after it, by name.
fn tar_entry(out: &mut impl Write, path: &Path, name: &str) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    #[cfg(unix)]
    let mode = std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o7777;
    #[cfg(not(unix))]
    let mode = if meta.is_dir() { 0o755 } else { 0o644 };
    if meta.is_symlink() {
        let target = fs::read_link(path)?;
        return tar_header(out, name, b'2', mode, 0, mtime, &target.to_string_lossy().replace('\\', "/"));
    }
    if meta.is_dir() {
        tar_header(out, &format!("{name}/"), b'5', mode, 0, mtime, "")?;
        let mut entries: Vec<_> = fs::read_dir(path)?.collect::<io::Result<_>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            tar_entry(out, &entry.path(), &format!("{name}/{}", entry.file_name().to_string_lossy()))?;
        }
        return Ok(());
    }
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len();
    tar_header(out, name, b'0', mode, size, mtime, "")?;
    // Exactly `size` bytes, even if the file changes meanwhile: the header has already said how many.
    let copied = io::copy(&mut io::Read::take(&mut file, size), out)?;
    io::copy(&mut io::Read::take(io::repeat(0), size - copied), out)?;
    out.write_all(&vec![0; (512 - size % 512) as usize % 512])
}

/// A ustar header; preceded by a pax one (UTF-8 by definition) when the name, the link
/// or the size does not fit in its field.
fn tar_header(out: &mut impl Write, name: &str, kind: u8, mode: u32, size: u64, mtime: u64, link: &str) -> io::Result<()> {
    let fits = |s: &str, n: usize| s.is_ascii() && s.len() <= n;
    let mut pax = String::new();
    for (key, value, needed) in [("path", name, !fits(name, 100)), ("linkpath", link, !fits(link, 100)), ("size", &size.to_string(), size >= 1 << 33)] {
        if needed {
            // Each record starts with its own length, digits included.
            let body = format!(" {key}={value}\n");
            let mut len = body.len() + 1;
            while (len.to_string().len() + body.len()) != len {
                len = len.to_string().len() + body.len();
            }
            pax.push_str(&format!("{len}{body}"));
        }
    }
    if !pax.is_empty() {
        tar_header(out, "././@PaxHeader", b'x', 0o644, pax.len() as u64, mtime, "")?;
        out.write_all(pax.as_bytes())?;
        out.write_all(&vec![0; (512 - pax.len() % 512) % 512])?;
    }
    let mut block = [0u8; 512];
    let mut put = |at: usize, len: usize, bytes: &[u8]| {
        let n = bytes.len().min(len);
        block[at..at + n].copy_from_slice(&bytes[..n]);
    };
    // Past the field: the pax header above has the whole of it.
    let ascii = |s: &str| s.chars().map(|c| if c.is_ascii() { c } else { '_' }).collect::<String>();
    put(0, 100, ascii(name).as_bytes());
    put(100, 8, format!("{:07o}\0", mode).as_bytes());
    put(108, 8, b"0000000\0");
    put(116, 8, b"0000000\0");
    put(124, 12, format!("{:011o}\0", size.min((1 << 33) - 1)).as_bytes());
    put(136, 12, format!("{:011o}\0", mtime.min((1 << 33) - 1)).as_bytes());
    put(148, 8, b"        ");
    put(156, 1, &[kind]);
    put(157, 100, ascii(link).as_bytes());
    put(257, 8, b"ustar\x0000");
    let sum: u32 = block.iter().map(|&b| b as u32).sum();
    block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    out.write_all(&block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn backs_up_the_vault() {
        let base = env::temp_dir().join(format!("bref-backup-{}", std::process::id()));
        let (root, out) = (base.join("Mon coffre"), base.join("sauvegardes"));
        fs::create_dir_all(root.join(".trash")).unwrap();
        fs::create_dir_all(&out).unwrap();
        fs::write(root.join("A.md"), "# A\n").unwrap();
        fs::write(root.join(".trash/Vieux.md"), "jeté").unwrap();
        fs::write(root.join("abz文cd.md"), "# abz文cd\n").unwrap();
        let first = backup(&root, &out, "2026-10-09").unwrap();
        assert_eq!(first, out.join("Mon coffre 2026-10-09.tar.gz"));
        // Same day: a second archive, the first one stays.
        assert_eq!(backup(&root, &out, "2026-10-09").unwrap(), out.join("Mon coffre 2026-10-09 2.tar.gz"));
        let mut listed = Vec::new();
        io::Read::read_to_end(&mut flate2::read::GzDecoder::new(fs::File::open(&first).unwrap()), &mut listed).unwrap();
        let listed = String::from_utf8_lossy(&listed);
        assert!(listed.contains("Mon coffre/A.md") && listed.contains("Mon coffre/.trash/Vieux.md") && listed.contains("jeté"), "{listed}");
        // A name outside the ANSI code page goes in a pax header: Windows' `tar` crashed on it.
        assert!(listed.contains("path=Mon coffre/abz文cd.md\n"), "{listed}");
        // Inside the vault, the archive would contain itself: refused, nothing is written.
        assert!(backup(&root, &root.join(".trash"), "2026-10-09").is_err());
        assert_eq!(fs::read_dir(root.join(".trash")).unwrap().count(), 1);
        fs::remove_dir_all(&base).unwrap();
    }
}
