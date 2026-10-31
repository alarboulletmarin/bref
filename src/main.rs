//! Bref: a fast, minimal Markdown note-taking app. ARCHITECTURE.md maps the code.

// No console behind the window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod actions;
mod lang;
mod logic;
mod mcp;
mod platform;
mod shell;
mod ui;
mod views;

fn main() {
    // `bref mcp` is a Model Context Protocol server on stdio: no window, no gpui.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        std::process::exit(mcp::run());
    }
    // The window has no console on Windows: the last panic is kept in the config folder.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        platform::vault::save_file("crash", &format!("bref {}\n{info}\n", env!("CARGO_PKG_VERSION")));
        report(info);
    }));
    shell::run();
}
