//! Vault sync through git: we call the `git` binary, as `update` calls
//! `curl`. Reading its outputs (pure functions), then the steps of a sync:
//! commit everything, fetch, merge without ever leaving markers in a note, push.

use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use crate::lang::tr;

/// A repository in a state the sync does not touch: we say so, without guessing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Odd {
    /// A merge or a rebase is in progress.
    Merging,
    Detached,
    NoUpstream,
    /// `index.lock`: another git is working, or stopped halfway.
    Locked,
    /// The vault is only a folder of a larger repository.
    Inside,
    /// A conflict that two side-by-side copies do not settle (a rename against an edit).
    Unsettled,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fail {
    /// The `git` binary is not installed.
    Missing,
    Odd(Odd),
    /// What git, or the system, says about it.
    Told(String),
}

impl Fail {
    pub fn text(&self) -> String {
        let text = match self {
            Fail::Told(text) => return text.clone(),
            Fail::Missing => tr(
                "git is not installed: install it (git-scm.com, or your package manager), then try again.",
                "git n'est pas installé : installe-le (git-scm.com, ou ton gestionnaire de paquets), puis réessaie.",
            ),
            Fail::Odd(Odd::Merging) => tr(
                "a merge or a rebase is in progress in this repository: finish it or abort it with git first.",
                "une fusion ou un rebase est en cours dans ce dépôt : termine-le ou abandonne-le avec git d'abord.",
            ),
            Fail::Odd(Odd::Detached) => tr("the repository is not on a branch (detached HEAD).", "le dépôt n'est sur aucune branche (HEAD détachée)."),
            Fail::Odd(Odd::NoUpstream) => tr(
                "the branch follows no branch of the server: run « git push -u origin <branch> » once.",
                "la branche ne suit aucune branche du serveur : lance « git push -u origin <branche> » une fois.",
            ),
            Fail::Odd(Odd::Locked) => tr(
                "another git is working in this repository (index.lock): try again when it is done.",
                "un autre git travaille dans ce dépôt (index.lock) : réessaie quand il a fini.",
            ),
            Fail::Odd(Odd::Inside) => tr(
                "the vault is a folder of a larger git repository: syncing would commit the whole repository.",
                "le coffre est un dossier d'un dépôt git plus grand : synchroniser en commiterait tout le dépôt.",
            ),
            Fail::Odd(Odd::Unsettled) => tr(
                "a conflict could not be settled by keeping both versions (a rename against an edit): nothing was changed, merge with git.",
                "un conflit ne se règle pas en gardant les deux versions (renommage contre modification) : rien n'a changé, fusionne avec git.",
            ),
        };
        text.to_string()
    }
}

/// What a merge did.
#[derive(Debug, PartialEq)]
pub struct Merged {
    /// Files changed here, to push; files changed on the other side, received.
    pub sent: usize,
    pub received: usize,
    /// The other side's versions written next to the conflicting notes.
    pub conflicts: Vec<PathBuf>,
    /// Something is still left to push.
    pub ahead: bool,
}

#[derive(Debug, PartialEq)]
pub enum Pushed {
    Done,
    /// Another machine pushed in the meantime: fetch again, then push again.
    Rejected,
}

/// `rev-list --left-right --count HEAD...@{u}`: commits ahead, commits behind.
pub fn counts(out: &str) -> Option<(usize, usize)> {
    let mut numbers = out.split_whitespace().map(str::parse::<usize>);
    match (numbers.next(), numbers.next()) {
        (Some(Ok(ahead)), Some(Ok(behind))) => Some((ahead, behind)),
        _ => None,
    }
}

/// `ls-files -u -z`: each conflicting file, and which of its three versions exist
/// (the common ancestor, ours, theirs).
pub fn unmerged(out: &str) -> Vec<(String, [bool; 3])> {
    let mut files: Vec<(String, [bool; 3])> = Vec::new();
    for entry in out.split('\0').filter(|entry| !entry.is_empty()) {
        let Some((meta, path)) = entry.split_once('\t') else { continue };
        let Some(stage) = meta.rsplit(' ').next().and_then(|stage| stage.parse::<usize>().ok()).filter(|stage| (1..=3).contains(stage)) else {
            continue;
        };
        match files.iter_mut().find(|(known, _)| known == path) {
            Some((_, stages)) => stages[stage - 1] = true,
            None => {
                let mut stages = [false; 3];
                stages[stage - 1] = true;
                files.push((path.to_string(), stages));
            }
        }
    }
    files
}

/// Where to write the other side's version of a conflicting note: next to it, as Dropbox
/// and Syncthing do.
pub fn conflict_path(path: &Path, date: &str, host: &str) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let mark = if host.is_empty() { format!("conflict {date}") } else { format!("conflict {date} {host}") };
    let name = match path.extension() {
        Some(extension) => format!("{stem} ({mark}).{}", extension.to_string_lossy()),
        None => format!("{stem} ({mark})"),
    };
    path.with_file_name(name)
}

/// What git wrote when failing, reduced to one sentence: a refused connection and an unreachable
/// server have their own, the rest keeps git's last line.
pub fn explain(stderr: &str) -> String {
    let low = stderr.to_lowercase();
    let has = |parts: &[&str]| parts.iter().any(|part| low.contains(part));
    if has(&["authentication failed", "permission denied", "could not read username", "terminal prompts disabled", "invalid username or password"]) {
        return tr(
            "the server refused the sign-in: check the key or the password git uses for this repository.",
            "le serveur a refusé la connexion : vérifie la clé ou le mot de passe que git utilise pour ce dépôt.",
        )
        .to_string();
    }
    if has(&["could not resolve host", "unable to access", "timed out", "network is unreachable", "connection refused", "could not read from remote"]) {
        return tr("the server cannot be reached: check the network.", "le serveur est injoignable : vérifie le réseau.").to_string();
    }
    let last = stderr.lines().rev().map(str::trim).find(|line| !line.is_empty()).unwrap_or("git failed");
    last.trim_start_matches("fatal: ").trim_start_matches("error: ").to_string()
}

/// Folder name of a cloned vault: the last piece of the address (a URL, an SSH
/// address, or a path, Windows style included), without `.git`.
pub fn folder_of(url: &str) -> String {
    let last = url.trim().trim_end_matches('/').rsplit(['/', ':', '\\']).next().unwrap_or_default();
    let name = last.strip_suffix(".git").unwrap_or(last).trim();
    if name.is_empty() { "vault".to_string() } else { name.to_string() }
}

/// Name of this machine, to name a conflict's copy; empty if it cannot be found.
pub fn host() -> String {
    let named = crate::platform::command("hostname").stdin(Stdio::null()).output().ok().filter(|out| out.status.success());
    named.map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string()).unwrap_or_default()
}

/// Runs git in `root`, never waiting for input: no password prompt (neither from
/// git nor from ssh), the failure is reported. Line endings of notes are never converted.
fn call(root: &Path, args: &[&str]) -> Result<Output, Fail> {
    let mut command = crate::platform::command("git");
    command.current_dir(root).args(["-c", "core.quotepath=off", "-c", "core.autocrlf=false"]).args(args);
    command.env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C").stdin(Stdio::null());
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    }
    // Tests do not depend on the machine's git settings (signature, templates…).
    #[cfg(test)]
    {
        let empty = std::env::temp_dir().join("bref-test-gitconfig");
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(&empty);
        command.env("GIT_CONFIG_GLOBAL", empty).env("GIT_CONFIG_NOSYSTEM", "1");
    }
    command.output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Fail::Missing,
        _ => Fail::Told(e.to_string()),
    })
}

/// What git answers, or why it failed.
fn run(root: &Path, args: &[&str]) -> Result<String, Fail> {
    let out = call(root, args)?;
    match out.status.success() {
        true => Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string()),
        false => Err(Fail::Told(explain(&String::from_utf8_lossy(&out.stderr)))),
    }
}

/// A question git answers through its exit code.
fn holds(root: &Path, args: &[&str]) -> Result<bool, Fail> {
    Ok(call(root, args)?.status.success())
}

/// Is the vault a git repository linked to a server? A folder of a larger repository is refused.
pub fn linked(root: &Path) -> Result<bool, Fail> {
    let out = call(root, &["rev-parse", "--show-toplevel"])?;
    if !out.status.success() {
        return Ok(false);
    }
    let top = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    if std::fs::canonicalize(top).ok() != std::fs::canonicalize(root).ok() {
        return Err(Fail::Odd(Odd::Inside));
    }
    Ok(!run(root, &["remote"])?.is_empty())
}

/// Refuses a repository in an unusual state, before touching it.
fn usual(root: &Path) -> Result<(), Fail> {
    let dir = PathBuf::from(run(root, &["rev-parse", "--absolute-git-dir"])?);
    if dir.join("index.lock").exists() {
        return Err(Fail::Odd(Odd::Locked));
    }
    if ["MERGE_HEAD", "rebase-merge", "rebase-apply"].iter().any(|mark| dir.join(mark).exists()) {
        return Err(Fail::Odd(Odd::Merging));
    }
    if !holds(root, &["symbolic-ref", "-q", "HEAD"])? {
        return Err(Fail::Odd(Odd::Detached));
    }
    if !holds(root, &["rev-parse", "-q", "--verify", "@{u}"])? {
        return Err(Fail::Odd(Odd::NoUpstream));
    }
    Ok(())
}

/// Without `user.name` or `user.email` (a machine where git was never used), git refuses to create a
/// commit, merge or plain: a fallback identity signs it.
fn signature(root: &Path) -> Vec<&'static str> {
    match run(root, &["config", "user.email"]).unwrap_or_default().is_empty() {
        true => vec!["-c", "user.name=Bref", "-c", "user.email=bref@localhost"],
        false => Vec::new(),
    }
}

/// Commits what changed, if anything.
fn commit(root: &Path, extra: &[&str]) -> Result<(), Fail> {
    run(root, &["add", "-A"])?;
    let unborn = !holds(root, &["rev-parse", "-q", "--verify", "HEAD"])?;
    if holds(root, &["diff", "--cached", "--quiet"])? && !unborn && extra.is_empty() {
        return Ok(());
    }
    let mut args = signature(root);
    args.extend(["commit", "-q"]);
    match extra.is_empty() {
        true => args.extend(["--allow-empty", "-m", "Bref: sync"]),
        false => args.extend(extra),
    }
    run(root, &args).map(drop)
}

/// First step, local: everything that changed is committed. Never a `stash`: a commit puts
/// everything into history, and `merge --abort` always goes back to a clean state.
pub fn save(root: &Path) -> Result<(), Fail> {
    usual(root)?;
    commit(root, &[])
}

/// Second step, network: what the server has that is new.
pub fn fetch(root: &Path) -> Result<(), Fail> {
    run(root, &["fetch", "-q"]).map(drop)
}

/// Third step, local: what was typed since `save` is committed, then what was received
/// is merged. When git cannot merge a file, both versions are kept:
/// the note stays the local one, the other side's is written next to it.
pub fn merge(root: &Path, date: &str, host: &str) -> Result<Merged, Fail> {
    save(root)?;
    let count = |range: &str| run(root, &["diff", "--name-only", range]).map(|out| out.lines().count());
    let (sent, received) = (count("@{u}...HEAD")?, count("HEAD...@{u}")?);
    let (ahead, behind) = counts(&run(root, &["rev-list", "--left-right", "--count", "HEAD...@{u}"])?).unwrap_or_default();
    let mut conflicts = Vec::new();
    if behind > 0 && ahead == 0 {
        run(root, &["merge", "-q", "--ff-only", "@{u}"])?;
    } else if behind > 0 {
        let mut args = signature(root);
        args.extend(["merge", "-q", "--no-edit", "@{u}"]);
        let out = call(root, &args)?;
        let files = if out.status.success() { Vec::new() } else { unmerged(&run(root, &["ls-files", "-u", "-z"])?) };
        // A failure without a conflicting file: git started nothing, it says why.
        if !out.status.success() && files.is_empty() {
            return Err(Fail::Told(explain(&String::from_utf8_lossy(&out.stderr))));
        }
        if !files.iter().all(|(_, stages)| matches!(stages, [_, true, true] | [true, true, false] | [true, false, true])) {
            run(root, &["merge", "--abort"])?;
            return Err(Fail::Odd(Odd::Unsettled));
        }
        for (path, stages) in &files {
            match stages {
                // Changed on both sides: theirs next to it, ours in its place.
                [_, true, true] => {
                    let theirs = call(root, &["show", &format!(":3:{path}")])?.stdout;
                    let mut other = conflict_path(&root.join(path), date, host);
                    let named = other.clone();
                    let mut n = 1;
                    while other.exists() {
                        n += 1;
                        let stem = named.file_stem().unwrap_or_default().to_string_lossy().to_string();
                        other = named.with_file_name(match named.extension() {
                            Some(extension) => format!("{stem} {n}.{}", extension.to_string_lossy()),
                            None => format!("{stem} {n}"),
                        });
                    }
                    std::fs::write(&other, theirs).map_err(|e| Fail::Told(e.to_string()))?;
                    run(root, &["checkout", "-q", "--ours", "--", path])?;
                    conflicts.push(other);
                }
                // Edited here, deleted on the other side: it stays.
                [true, true, false] => {}
                // Deleted here, edited on the other side: it comes back.
                _ => drop(run(root, &["checkout", "-q", "--theirs", "--", path])?),
            }
        }
        if !files.is_empty() {
            commit(root, &["--no-edit"])?;
        }
    }
    let ahead = counts(&run(root, &["rev-list", "--left-right", "--count", "HEAD...@{u}"])?).is_some_and(|(ahead, _)| ahead > 0);
    Ok(Merged { sent, received, conflicts, ahead })
}

/// Last step, network: push. Refused when another machine pushed in the meantime.
pub fn push(root: &Path) -> Result<Pushed, Fail> {
    let out = call(root, &["push", "-q"])?;
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    match out.status.success() {
        true => Ok(Pushed::Done),
        false if ["rejected", "non-fast-forward", "fetch first"].iter().any(|mark| stderr.contains(mark)) => Ok(Pushed::Rejected),
        false => Err(Fail::Told(explain(&stderr))),
    }
}

/// Links the vault to an empty remote repository: `git init` if needed, the trash ignored, a
/// first commit, a first push. A server that already has a history is not overwritten.
pub fn connect(root: &Path, url: &str) -> Result<(), Fail> {
    if !call(root, &["rev-parse", "--show-toplevel"])?.status.success() && !holds(root, &["init", "-q", "-b", "main"])? {
        run(root, &["init", "-q"])?;
    }
    if linked(root).is_err() {
        return Err(Fail::Odd(Odd::Inside));
    }
    // The trash stays on this machine.
    let ignore = root.join(".gitignore");
    let known = std::fs::read_to_string(&ignore).unwrap_or_default();
    if !known.lines().any(|line| line.trim() == ".trash/") {
        let gap = if known.is_empty() || known.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&ignore, format!("{known}{gap}.trash/\n")).map_err(|e| Fail::Told(e.to_string()))?;
    }
    let verb = if run(root, &["remote"])?.lines().any(|name| name == "origin") { "set-url" } else { "add" };
    run(root, &["remote", verb, "origin", url])?;
    commit(root, &[])?;
    let out = call(root, &["push", "-q", "-u", "origin", "HEAD"])?;
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    match out.status.success() {
        true => Ok(()),
        false if ["rejected", "non-fast-forward", "fetch first"].iter().any(|mark| stderr.contains(mark)) => Err(Fail::Told(
            tr(
                "this repository already holds something: use « Clone a vault » to get it, or connect an empty one.",
                "ce dépôt contient déjà quelque chose : utilise « Cloner un coffre » pour le récupérer, ou relie un dépôt vide.",
            )
            .to_string(),
        )),
        false => Err(Fail::Told(explain(&stderr))),
    }
}

/// Clones a vault into the folder `into`, which must not exist.
pub fn clone(url: &str, into: &Path) -> Result<(), Fail> {
    let parent = into.parent().unwrap_or(Path::new("."));
    run(parent, &["clone", "-q", "--", url, &into.to_string_lossy()]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reads_what_git_prints() {
        assert_eq!(counts("2\t1\n"), Some((2, 1)));
        assert_eq!(counts("0\t0"), Some((0, 0)));
        assert_eq!(counts("fatal: no upstream"), None);
        // `ls-files -u -z`: one entry per version (1: the ancestor, 2: ours, 3: theirs).
        let listed = "100644 f0f2 1\tNé.md\x00100644 3b18 2\tNé.md\x00100644 c995 3\tNé.md\x00100644 587b 1\tdir/gone.md\x00100644 cf76 3\tdir/gone.md\0";
        assert_eq!(unmerged(listed), [("Né.md".to_string(), [true, true, true]), ("dir/gone.md".to_string(), [true, false, true])]);
        assert!(unmerged("").is_empty());
    }

    #[test]
    fn names_the_other_version() {
        assert_eq!(conflict_path(Path::new("dir/Note.md"), "2026-10-09", "laptop"), Path::new("dir/Note (conflict 2026-10-09 laptop).md"));
        assert_eq!(conflict_path(Path::new("Note.md"), "2026-10-09", ""), Path::new("Note (conflict 2026-10-09).md"));
        assert_eq!(conflict_path(Path::new("sans extension"), "2026-10-09", "pc"), Path::new("sans extension (conflict 2026-10-09 pc)"));
    }

    #[test]
    fn explains_a_failure() {
        let auth = explain("remote: Invalid username or password.\nfatal: Authentication failed for 'https://example.org/notes.git/'\n");
        assert_eq!(auth, explain("git@example.org: Permission denied (publickey).\nfatal: Could not read from remote repository.\n"));
        let network = explain("fatal: unable to access 'https://example.org/notes.git/': Could not resolve host: example.org\n");
        assert_eq!(network, explain("ssh: connect to host example.org port 22: Connection timed out\nfatal: Could not read from remote repository.\n"));
        assert_ne!(auth, network);
        // The rest: git's last line, without its prefix.
        assert_eq!(explain("hint: a\nfatal: refusing to merge unrelated histories\n\n"), "refusing to merge unrelated histories");
    }

    #[test]
    fn names_the_folder_of_a_clone() {
        assert_eq!(folder_of("git@github.com:me/notes.git"), "notes");
        assert_eq!(folder_of("https://example.org/me/Mes notes/"), "Mes notes");
        assert_eq!(folder_of("/srv/git/vault.git"), "vault");
        assert_eq!(folder_of(r"C:\Users\moi\dépôts\notes.git"), "notes");
        assert_eq!(folder_of(""), "vault");
    }

    /// A server (bare repository) and two machines, in a throwaway folder.
    struct Lab {
        base: PathBuf,
        a: PathBuf,
        b: PathBuf,
    }

    impl Lab {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir().join(format!("bref-git-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&base);
            let (remote, a, b) = (base.join("remote.git"), base.join("a"), base.join("b"));
            fs::create_dir_all(&remote).unwrap();
            fs::create_dir_all(&a).unwrap();
            run(&remote, &["init", "--bare", "-b", "main"]).unwrap();
            // The first machine already has notes, and a trash: it connects.
            fs::write(a.join("Note.md"), "# Note\n\nun\ndeux\ntrois\n").unwrap();
            fs::create_dir_all(a.join(".trash")).unwrap();
            fs::write(a.join(".trash/Vieille.md"), "jetée").unwrap();
            assert_eq!(linked(&a), Ok(false));
            connect(&a, remote.to_str().unwrap()).unwrap();
            assert_eq!(linked(&a), Ok(true));
            // The second one clones the vault.
            clone(remote.to_str().unwrap(), &b).unwrap();
            Self { base, a, b }
        }

        /// A full sync, like the command: commit, fetch, merge, push.
        fn sync(&self, root: &Path) -> Merged {
            for _ in 0..3 {
                save(root).unwrap();
                fetch(root).unwrap();
                let merged = merge(root, "2026-10-09", "pc").unwrap();
                if !merged.ahead || push(root).unwrap() == Pushed::Done {
                    return merged;
                }
            }
            panic!("always refused");
        }

        fn files(&self, root: &Path) -> Vec<(String, String)> {
            let mut files: Vec<(String, String)> = fs::read_dir(root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().is_file())
                .map(|entry| (entry.file_name().to_string_lossy().to_string(), fs::read_to_string(entry.path()).unwrap()))
                .collect();
            files.sort();
            files
        }
    }

    impl Drop for Lab {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn two_machines_end_up_identical() {
        let lab = Lab::new("merge");
        // The trash does not travel: it is ignored from the connection on.
        assert_eq!(fs::read_to_string(lab.a.join(".gitignore")).unwrap(), ".trash/\n");
        assert!(lab.b.join("Note.md").is_file() && !lab.b.join(".trash").exists());
        // Nothing to do: nothing pushed, nothing received.
        let idle = lab.sync(&lab.a);
        assert_eq!((idle.sent, idle.received, idle.conflicts.len()), (0, 0, 0));
        // Different notes, and two places in the same note: no question.
        fs::write(lab.a.join("Note.md"), "# Note\n\nUN\ndeux\ntrois\n").unwrap();
        fs::write(lab.a.join("Chez A.md"), "a").unwrap();
        fs::write(lab.b.join("Note.md"), "# Note\n\nun\ndeux\nTROIS\n").unwrap();
        fs::write(lab.b.join("Né chez B.md"), "b").unwrap();
        let sent = lab.sync(&lab.a);
        assert_eq!((sent.sent, sent.received), (2, 0));
        let both = lab.sync(&lab.b);
        assert_eq!((both.sent, both.received, both.conflicts.len()), (2, 2, 0));
        let last = lab.sync(&lab.a);
        assert_eq!((last.sent, last.received), (0, 2));
        assert_eq!(lab.files(&lab.a), lab.files(&lab.b));
        assert_eq!(fs::read_to_string(lab.a.join("Note.md")).unwrap(), "# Note\n\nUN\ndeux\nTROIS\n");
        assert!(lab.a.join("Né chez B.md").is_file());
    }

    #[test]
    fn a_conflict_keeps_both_versions() {
        let lab = Lab::new("conflict");
        // The same line, changed on both sides; a note deleted here, edited there.
        fs::write(lab.b.join("Autre.md"), "x\n").unwrap();
        lab.sync(&lab.b);
        lab.sync(&lab.a);
        fs::write(lab.a.join("Note.md"), "# Note\n\nun\nA\ntrois\n").unwrap();
        fs::remove_file(lab.a.join("Autre.md")).unwrap();
        fs::write(lab.b.join("Note.md"), "# Note\n\nun\nB\ntrois\n").unwrap();
        fs::write(lab.b.join("Autre.md"), "x\nécrit chez B\n").unwrap();
        lab.sync(&lab.a);
        let merged = lab.sync(&lab.b);
        // The note keeps the local version, the other side's is written next to it; nothing is lost
        // and no marker gets into a note.
        let other = lab.b.join("Note (conflict 2026-10-09 pc).md");
        assert_eq!(merged.conflicts, std::slice::from_ref(&other));
        assert_eq!(fs::read_to_string(lab.b.join("Note.md")).unwrap(), "# Note\n\nun\nB\ntrois\n");
        assert_eq!(fs::read_to_string(&other).unwrap(), "# Note\n\nun\nA\ntrois\n");
        // Edited against deleted: the edited version stays.
        assert_eq!(fs::read_to_string(lab.b.join("Autre.md")).unwrap(), "x\nécrit chez B\n");
        // The merge is committed and pushed: the other machine receives both versions.
        lab.sync(&lab.a);
        assert_eq!(lab.files(&lab.a), lab.files(&lab.b));
        assert!(lab.files(&lab.a).iter().all(|(_, text)| !text.contains("<<<<<<<")));
        // A second conflict on the same day: the existing copy is not overwritten.
        fs::write(lab.a.join("Note.md"), "A2\n").unwrap();
        fs::write(lab.b.join("Note.md"), "B2\n").unwrap();
        lab.sync(&lab.a);
        let again = lab.sync(&lab.b);
        assert_eq!(again.conflicts, [lab.b.join("Note (conflict 2026-10-09 pc) 2.md")]);
        assert_eq!(fs::read_to_string(&other).unwrap(), "# Note\n\nun\nA\ntrois\n");
    }

    #[test]
    fn what_is_typed_while_fetching_is_merged_too() {
        let lab = Lab::new("typing");
        fs::write(lab.b.join("Reçue.md"), "de B").unwrap();
        lab.sync(&lab.b);
        save(&lab.a).unwrap();
        fetch(&lab.a).unwrap();
        // Written while the network was answering: committed before the merge, never overwritten.
        fs::write(lab.a.join("Note.md"), "# Note\n\ntapé pendant la réception\n").unwrap();
        let merged = merge(&lab.a, "2026-10-09", "pc").unwrap();
        assert_eq!((merged.received, merged.ahead), (1, true));
        assert_eq!(fs::read_to_string(lab.a.join("Note.md")).unwrap(), "# Note\n\ntapé pendant la réception\n");
        assert_eq!(push(&lab.a), Ok(Pushed::Done));
    }

    #[test]
    fn refuses_what_it_does_not_understand() {
        let lab = Lab::new("odd");
        // A detached head: we do not touch it, and nothing moved.
        let head = run(&lab.a, &["rev-parse", "HEAD"]).unwrap();
        run(&lab.a, &["checkout", "--detach", "-q"]).unwrap();
        fs::write(lab.a.join("Note.md"), "en cours").unwrap();
        assert_eq!(save(&lab.a), Err(Fail::Odd(Odd::Detached)));
        assert_eq!(run(&lab.a, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(fs::read_to_string(lab.a.join("Note.md")).unwrap(), "en cours");
        // An index locked by another git.
        let lock = lab.b.join(".git/index.lock");
        fs::write(&lock, "").unwrap();
        assert_eq!(save(&lab.b), Err(Fail::Odd(Odd::Locked)));
        fs::remove_file(lock).unwrap();
        // A vault that is only a folder of a larger repository: the whole repository would go.
        let inner = lab.b.join("sous-dossier");
        fs::create_dir_all(&inner).unwrap();
        assert_eq!(linked(&inner), Err(Fail::Odd(Odd::Inside)));
        // A folder without git: nothing to sync, connect first.
        let plain = lab.base.join("plain");
        fs::create_dir_all(&plain).unwrap();
        assert_eq!(linked(&plain), Ok(false));
        // A server that already has a history: we do not overwrite it, we say so.
        fs::write(plain.join("x.md"), "x").unwrap();
        assert!(connect(&plain, lab.base.join("remote.git").to_str().unwrap()).is_err());
        assert_eq!(fs::read_to_string(plain.join("x.md")).unwrap(), "x");
    }
}
