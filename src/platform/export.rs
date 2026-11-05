//! Publish a local document copy atomically, without replacing an occupied destination.

use std::{fs, io::{self, Write}, path::Path, sync::atomic::{AtomicU64, Ordering}};

/// Stage beside the destination, then link it into place. Unlike rename, a hard link
/// refuses an existing name (including a symlink), even if it appeared during the write.
pub fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().unwrap_or(Path::new("."));
    let (temp, mut file) = loop {
        let temp = dir.join(format!(".bref-export-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        match fs::File::create_new(&temp) {
            Ok(file) => break (temp, file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::hard_link(&temp, path)
    })();
    let _ = fs::remove_file(temp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_never_replace_an_occupied_name() {
        let dir = std::env::temp_dir().join(format!("bref-export-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("copie été.md");
        save(&path, "é😀\nline".as_bytes()).unwrap();
        assert!(save(&path, b"replacement").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "é😀\nline");
        assert!(save(&dir, b"directory").is_err());
        assert!(save(&dir.join("missing/copy.txt"), b"").is_err());
        save(&dir.join("empty.txt"), b"").unwrap();
        // Two writers racing for the same name cannot both publish or mix their bytes.
        let race = dir.join("race.txt");
        let writers: Vec<_> = (0..8).map(|i| {
            let path = race.clone();
            std::thread::spawn(move || save(&path, format!("writer {i}").as_bytes()).is_ok())
        }).collect();
        assert_eq!(writers.into_iter().filter_map(|w| w.join().unwrap().then_some(())).count(), 1);
        assert!(fs::read_to_string(race).unwrap().starts_with("writer "));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = dir.join("locked");
            fs::create_dir(&locked).unwrap();
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
            let denied = save(&locked.join("copy.txt"), b"denied");
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(denied.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
            let link = dir.join("link.txt");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(save(&link, b"replacement").is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), "é😀\nline");
        }
        assert!(fs::read_dir(&dir).unwrap().all(|p| !p.unwrap().file_name().to_string_lossy().starts_with(".bref-export-")));
        fs::remove_dir_all(dir).unwrap();
    }
}
