//! Bref: a fast, minimal Markdown note-taking app. ARCHITECTURE.md maps the code.

// No console behind the window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod actions;
mod lang;
mod logic;
mod platform;
mod shell;
mod ui;
mod views;

fn main() {
    shell::run();
}
