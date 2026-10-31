//! The running app's door, for `bref mcp`: a TCP port on 127.0.0.1 and a secret, both written in
//! `<config>/bref/agent` when the window opens. One request a connection, one JSON line each way:
//! `{"secret", "show": path}` asks the window to show a file of its vault; the answer is
//! `{"ok": true}` or `{"error": sentence}`. The secret keeps out the other programs of the machine,
//! which can all reach 127.0.0.1. Only the standard library: TCP is the one local channel it has
//! on Linux, macOS and Windows alike.

use std::{
    collections::hash_map::RandomState,
    fs,
    hash::{BuildHasher, Hasher},
    io::{self, BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use serde_json::{Value, json};

use super::vault::{Signal, config_dir};

/// The answer to an agent: a sentence when the window could not show the file.
pub type Answer = Result<(), String>;

/// A file the agent asks the window to show, and where to send the window's answer.
pub struct Asked {
    pub path: PathBuf,
    pub answer: mpsc::Sender<Answer>,
}

/// What `bref mcp` says when no window answers: the file is written all the same.
pub const NOT_OPEN: &str = "Bref is not open: start it to see the file";

fn door() -> PathBuf {
    config_dir().join("bref/agent")
}

/// Opens the door: each request lands in `asked`, and `signal` is raised for it.
pub fn open(asked: Arc<Mutex<Vec<Asked>>>, signal: Arc<Signal>) -> io::Result<()> {
    open_at(&door(), asked, signal)
}

fn open_at(file: &Path, asked: Arc<Mutex<Vec<Asked>>>, signal: Arc<Signal>) -> io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let secret = secret();
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    writeln!(options.open(file)?, "{} {secret}", listener.local_addr()?.port())?;
    thread::Builder::new().name("agent door".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = serve(stream, &secret, &asked, &signal);
        }
    })?;
    Ok(())
}

/// 128 bits from the hasher keys the standard library draws from the system.
fn secret() -> String {
    (0..2).map(|_| format!("{:016x}", RandomState::new().build_hasher().finish())).collect()
}

fn serve(mut stream: TcpStream, secret: &str, asked: &Mutex<Vec<Asked>>, signal: &Signal) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut line = String::new();
    BufReader::new((&stream).take(64 * 1024)).read_line(&mut line)?;
    let answer = match request(&line, secret) {
        Err(e) => Err(e),
        Ok(path) => {
            let (answer, wait) = mpsc::channel();
            asked.lock().unwrap().push(Asked { path, answer });
            signal.raise();
            wait.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| Err("Bref did not answer".into()))
        }
    };
    let reply = match answer {
        Ok(()) => json!({"ok": true}),
        Err(e) => json!({"error": e}),
    };
    writeln!(stream, "{reply}")
}

/// The file asked for, if the request carries the secret.
fn request(line: &str, secret: &str) -> Result<PathBuf, String> {
    let value: Value = serde_json::from_str(line).map_err(|_| "not a request".to_string())?;
    if value["secret"].as_str() != Some(secret) {
        return Err("wrong secret".into());
    }
    value["show"].as_str().map(PathBuf::from).ok_or_else(|| "nothing to show".into())
}

/// Asks the running window to show `path` (absolute).
pub fn show(path: &Path) -> Answer {
    show_at(&door(), path)
}

fn show_at(file: &Path, path: &Path) -> Answer {
    let text = fs::read_to_string(file).map_err(|_| NOT_OPEN.to_string())?;
    let (port, secret) = text.trim().split_once(' ').ok_or(NOT_OPEN)?;
    let port: u16 = port.parse().map_err(|_| NOT_OPEN.to_string())?;
    // Left by a window since closed: nobody listens there any more.
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).map_err(|_| NOT_OPEN.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(6))).map_err(|e| e.to_string())?;
    writeln!(stream, "{}", json!({"secret": secret, "show": path})).map_err(|_| NOT_OPEN.to_string())?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(|_| "Bref did not answer".to_string())?;
    let reply: Value = serde_json::from_str(&line).map_err(|_| NOT_OPEN.to_string())?;
    match reply["error"].as_str() {
        Some(error) => Err(error.to_string()),
        None if reply["ok"] == true => Ok(()),
        None => Err(NOT_OPEN.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A window that answers each request with `answer`, through a door in `dir`.
    fn window(dir: &Path, answer: fn(&Path) -> Answer) -> PathBuf {
        let file = dir.join("bref/agent");
        let (asked, signal) = (Arc::new(Mutex::new(Vec::<Asked>::new())), Arc::new(Signal::default()));
        open_at(&file, asked.clone(), signal.clone()).unwrap();
        thread::spawn(move || {
            loop {
                if signal.take() {
                    for ask in asked.lock().unwrap().drain(..) {
                        let _ = ask.answer.send(answer(&ask.path));
                    }
                }
                thread::sleep(Duration::from_millis(5));
            }
        });
        file
    }

    #[test]
    fn the_window_answers_through_the_door() {
        let dir = std::env::temp_dir().join(format!("bref-link-{}", std::process::id()));
        let file = window(&dir.join("a"), |path| if path.ends_with("Note é.md") { Ok(()) } else { Err(format!("no file {}", path.display())) });
        assert_eq!(show_at(&file, Path::new("/vault/Note é.md")), Ok(()));
        assert_eq!(show_at(&file, Path::new("/vault/Other.md")), Err("no file /vault/Other.md".into()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "the secret is the user's alone");
        }

        // A wrong secret, a broken line, a line without end: refused, and the door still answers.
        let port = fs::read_to_string(&file).unwrap().split(' ').next().unwrap().parse::<u16>().unwrap();
        let ask = |line: &[u8]| {
            let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
            stream.write_all(line).unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        assert!(ask(b"{\"secret\":\"guess\",\"show\":\"/etc/passwd\"}\n").contains("wrong secret"));
        assert!(ask(b"{not json\n").contains("not a request"));
        assert!(ask(b"").contains("not a request"));
        assert_eq!(show_at(&file, Path::new("/vault/Note é.md")), Ok(()));

        // No door, a stale door (its window closed), a door file in a bad state.
        assert_eq!(show_at(&dir.join("none"), Path::new("/x.md")), Err(NOT_OPEN.into()));
        let stale = dir.join("stale");
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
        fs::write(&stale, format!("{port} secret")).unwrap();
        assert_eq!(show_at(&stale, Path::new("/x.md")), Err(NOT_OPEN.into()));
        for bad in ["", "garbage", "99999999 s"] {
            fs::write(&stale, bad).unwrap();
            assert_eq!(show_at(&stale, Path::new("/x.md")), Err(NOT_OPEN.into()), "{bad:?}");
        }
        assert_ne!(secret(), secret());
        fs::remove_dir_all(&dir).ok();
    }
}
