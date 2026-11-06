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
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `bref mcp` is a Model Context Protocol server on stdio: no window, no gpui.
    if args.first().map(String::as_str) == Some("mcp") {
        // Before any `tr`: the language of the library it serves is fixed here, once.
        if !lang::speak_english() {
            eprintln!("bref mcp: the language was already fixed, texts may come in French");
        }
        std::process::exit(mcp::run(&args[1..]));
    }
    // The window has no console on Windows: the last panic is kept in the config folder.
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        platform::config::keep(platform::config::File::Crash, &format!("bref {}\n{info}\n", env!("CARGO_PKG_VERSION")));
        report(info);
    }));
    shell::run();
}
