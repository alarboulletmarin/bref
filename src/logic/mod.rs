//! Pure functions: they read and return values, never touch the disk, a window or gpui.
//! Everything here is unit-tested in its own module.

pub mod block;
pub mod commands;
pub mod diagram;
pub mod embed;
pub mod formula;
pub mod history;
pub mod import;
pub mod library;
pub mod mermaid;
pub mod markdown;
pub mod note;
pub mod prefs;
pub mod recall;
pub mod relation;
pub mod richtext;
pub mod state;
pub mod table;
pub mod uml;
