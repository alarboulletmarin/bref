//! `bref mcp` over real pipes: one session, the way a client drives the binary.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
};

use serde_json::{Value, json};

struct Client {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next: u64,
}

impl Client {
    fn start(root: &Path, read_only: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bref"));
        // A French system: what the server tells the model stays in English.
        command.arg("mcp").arg("--vault").arg(root).env("LC_ALL", "fr_FR.UTF-8").env("LANG", "fr_FR.UTF-8");
        // A config of its own: `show` must never reach a Bref window open on this machine.
        let config = config(root);
        command.env("XDG_CONFIG_HOME", &config).env("APPDATA", &config).env("HOME", &config);
        if read_only {
            command.arg("--read-only");
        }
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Client { child, stdin, stdout, next: 1 }
    }

    fn ask(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert!(line.starts_with('{'), "{line}");
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["id"], json!(id));
        value
    }

    fn notify(&mut self, method: &str) {
        let message = json!({"jsonrpc": "2.0", "method": method});
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn finish(mut self) -> i32 {
        drop(self.stdin);
        self.child.wait().unwrap().code().unwrap_or(1)
    }
}

fn config(root: &Path) -> std::path::PathBuf {
    root.with_extension("config")
}

/// Where the app writes its door (`platform::link`) under the config folder `config`.
fn door(config: &Path) -> std::path::PathBuf {
    let dir = if cfg!(target_os = "macos") { config.join("Library/Application Support") } else { config.to_path_buf() };
    dir.join("bref/agent")
}

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("bref-mcp-bin-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn a_session_over_pipes() {
    let root = scratch("session");
    let mut client = Client::start(&root, false);
    let init = client.ask("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "bref");
    client.notify("notifications/initialized");
    let ping = client.ask("ping", json!({}));
    assert_eq!(ping["result"], json!({}));
    let tools = client.ask("tools/list", json!({}));
    assert!(tools["result"]["tools"].as_array().unwrap().iter().any(|tool| tool["name"] == "create_note"));
    let created = client.ask("tools/call", json!({"name": "create_note", "arguments": {"title": "Session", "text": "Hello."}}));
    assert_eq!(created["result"]["isError"], false);
    let read = client.ask("tools/call", json!({"name": "read_note", "arguments": {"note": "Session"}}));
    let body: Value = serde_json::from_str(read["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let revision = body["revision"].as_str().unwrap().to_string();
    let edited = client.ask("tools/call", json!({"name": "edit_note", "arguments": {"note": "Session", "revision": revision, "edits": [{"old": "Hello.", "new": "Hello there."}]}}));
    assert_eq!(edited["result"]["isError"], false);
    let stale = client.ask("tools/call", json!({"name": "edit_note", "arguments": {"note": "Session", "revision": revision, "edits": [{"old": "Hello.", "new": "no"}]}}));
    assert_eq!(stale["result"]["isError"], true);
    assert!(stale["result"]["content"][0]["text"].as_str().unwrap().contains("changed since"));
    let diagram = client.ask("tools/call", json!({"name": "create_diagram", "arguments": {"name": "Boxes", "from": {"shapes": [{"key": "a", "form": "rect", "text": "One", "x": 0, "y": 0}], "links": []}}}));
    assert_eq!(diagram["result"]["isError"], false);
    let rendered = client.ask("tools/call", json!({"name": "render", "arguments": {"path": "Boxes.svg", "theme": "light"}}));
    assert_eq!(rendered["result"]["isError"], false);
    assert_eq!(rendered["result"]["content"][1]["mimeType"], "image/png");
    let library = client.ask("tools/call", json!({"name": "diagram_library", "arguments": {"family": "class"}}));
    assert_eq!(library["result"]["structuredContent"]["shapes"][0]["name"], "Class");
    let guide = client.ask("resources/read", json!({"uri": "bref://guide/links"}));
    assert!(guide["result"]["contents"][0]["text"].as_str().unwrap().contains("[["));
    // `show` with no Bref open: a sentence, the note is there all the same.
    let alone = client.ask("tools/call", json!({"name": "show", "arguments": {"note": "Session"}}));
    assert_eq!(alone["result"]["isError"], true);
    assert!(alone["result"]["content"][0]["text"].as_str().unwrap().contains("Bref is not open"));
    // A window listening at its door: it is asked for the note's absolute path, with the secret.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let door = door(&config(&root));
    fs::create_dir_all(door.parent().unwrap()).unwrap();
    fs::write(&door, format!("{} s3cret\n", listener.local_addr().unwrap().port())).unwrap();
    let window = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        writeln!(stream, "{}", json!({"ok": true})).unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    });
    let shown = client.ask("tools/call", json!({"name": "show", "arguments": {"note": "Session"}}));
    assert_eq!(shown["result"]["isError"], false, "{shown}");
    assert_eq!(shown["result"]["structuredContent"]["shown"], "Session.md");
    let asked = window.join().unwrap();
    assert_eq!(asked["secret"], "s3cret");
    let path = Path::new(asked["show"].as_str().unwrap());
    assert!(path.is_absolute() && path.ends_with("Session.md"), "{path:?}");
    // A note that is not there is refused before the window is asked.
    let missing = client.ask("tools/call", json!({"name": "show", "arguments": {"note": "Nowhere"}}));
    assert!(missing["result"]["content"][0]["text"].as_str().unwrap().contains("no "), "{missing}");
    let unknown = client.ask("nope", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
    let broken = client.ask_raw("not json");
    assert_eq!(broken["error"]["code"], -32700);
    assert_eq!(client.finish(), 0);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(config(&root));
}

impl Client {
    fn ask_raw(&mut self, line: &str) -> Value {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
        let mut answer = String::new();
        self.stdout.read_line(&mut answer).unwrap();
        serde_json::from_str(&answer).unwrap()
    }
}

#[test]
fn read_only_offers_no_writing_tool() {
    let root = scratch("readonly");
    fs::write(root.join("Note.md"), "# Note\n").unwrap();
    let mut client = Client::start(&root, true);
    let tools = client.ask("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"read_note") && !names.contains(&"create_note"));
    let refused = client.ask("tools/call", json!({"name": "create_note", "arguments": {"title": "Nope"}}));
    assert_eq!(refused["result"]["isError"], true);
    assert!(refused["result"]["content"][0]["text"].as_str().unwrap().contains("read-only"));
    assert_eq!(client.finish(), 0);
    let _ = fs::remove_dir_all(&root);
}
