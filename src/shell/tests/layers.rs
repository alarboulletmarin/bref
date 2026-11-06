//! The layers only depend downwards (see ARCHITECTURE.md). The scan reads the source: comments are
//! dropped, every `use` tree is expanded to full paths (braces, nesting, `self`, `super`, `as`, globs)
//! and each path is checked against what its layer may not see.

use super::*;

/// The identifiers and the punctuation of a `use` statement (`::`, `{`, `}`, `,`, `*`).
fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || c == '_' {
            let mut word = c.to_string();
            while let Some(&n) = chars.peek().filter(|n| n.is_alphanumeric() || **n == '_') {
                word.push(n);
                chars.next();
            }
            out.push(word);
        } else if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            out.push("::".into());
        } else if "{},*".contains(c) {
            out.push(c.to_string());
        }
    }
    out
}

/// Expands one use tree starting at `at` into the full paths it names, after `prefix`.
/// `a::{b, c::{d, self}}` gives `a::b`, `a::c::d` and `a::c`; a glob ends in `*`; an alias is dropped.
fn tree(t: &[String], at: &mut usize, prefix: &[String], out: &mut Vec<Vec<String>>) {
    let mut path = prefix.to_vec();
    while let Some(token) = t.get(*at) {
        match token.as_str() {
            "::" => *at += 1,
            "{" => {
                *at += 1;
                loop {
                    tree(t, at, &path, out);
                    match t.get(*at).map(String::as_str) {
                        Some(",") => {
                            *at += 1;
                            if t.get(*at).map(String::as_str) == Some("}") {
                                *at += 1;
                                return;
                            }
                        }
                        Some("}") => {
                            *at += 1;
                            return;
                        }
                        _ => return,
                    }
                }
            }
            "as" => {
                *at += 2;
                break;
            }
            "," | "}" => break,
            word => {
                path.push(word.to_string());
                *at += 1;
            }
        }
    }
    if path.len() > prefix.len() {
        if path.last().is_some_and(|w| w == "self") && path.len() > 1 {
            path.pop();
        }
        out.push(path);
    }
}

/// Every path imported by a `use` statement of `code` (comments already removed), `self` and `super`
/// resolved against `module`, the file's own module path (`["crate", "logic", "dock"]`).
fn imports(code: &str, module: &[String]) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("use ") {
        let before = rest[..i].chars().next_back();
        let end = rest[i..].find(';').map_or(rest.len(), |e| i + e);
        if before.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
            let t = tokens(&rest[i + 4..end]);
            let mut full = Vec::new();
            tree(&t, &mut 0, &[], &mut full);
            out.extend(full.into_iter().map(|path| resolve(path, module)));
        }
        rest = &rest[end..];
    }
    out
}

/// `imports` of a file, where an inline `mod tests { .. }` is one module deeper than the file.
fn all_imports(code: &str, module: &[String]) -> Vec<Vec<String>> {
    match code.find("mod tests") {
        Some(i) => {
            let inner: Vec<String> = module.iter().cloned().chain(["tests".to_string()]).collect();
            imports(&code[..i], module).into_iter().chain(imports(&code[i..], &inner)).collect()
        }
        None => imports(code, module),
    }
}

/// `self::x` and `super::x` made absolute.
fn resolve(path: Vec<String>, module: &[String]) -> Vec<String> {
    let mut at = 0;
    let mut base = module.to_vec();
    if path.first().is_some_and(|w| w == "self") {
        at = 1;
    }
    while path.get(at).is_some_and(|w| w == "super") {
        base.pop();
        at += 1;
    }
    if at == 0 {
        path
    } else {
        base.into_iter().chain(path.into_iter().skip(at)).collect()
    }
}

/// The module path of a file: `src/logic/markdown/mod.rs` is `crate::logic::markdown`.
fn module_of(path: &Path) -> Vec<String> {
    let mut parts: Vec<String> = path.strip_prefix("src").unwrap_or(path).with_extension("").iter().map(|p| p.to_string_lossy().into_owned()).collect();
    if parts.last().is_some_and(|p| p == "mod") {
        parts.pop();
    }
    if parts.last().is_some_and(|p| p == "main") {
        parts.pop();
    }
    std::iter::once("crate".to_string()).chain(parts).collect()
}

/// What `code` (a file of `module`) reaches that is in `banned`: a banned word anywhere in the code
/// (`gpui`, `crate::ui`, `impl Shell`), or a `use` that resolves into a banned path, however it is spelled.
fn violations(code: &str, module: &[String], banned: &[&str]) -> Vec<String> {
    // Comments may name anything; only code counts.
    let code: String = code.lines().map(|l| l.split("//").next().unwrap()).collect::<Vec<_>>().join("\n");
    let mut found = Vec::new();
    for word in banned {
        if code.contains(word) {
            found.push(format!("`{word}`"));
        }
        let want: Vec<&str> = word.split("::").collect();
        if word.contains(' ') {
            continue;
        }
        for path in all_imports(&code, module) {
            let glob = path.last().is_some_and(|w| w == "*");
            let named = if glob { &path[..path.len() - 1] } else { &path[..] };
            let inside = named.len() >= want.len() && named.iter().zip(&want).all(|(a, b)| a == b);
            // `use crate::*` brings every layer into scope.
            let above = glob && named.len() < want.len() && named.iter().zip(&want).all(|(a, b)| a == b);
            if inside || above {
                found.push(format!("`use {}` reaches `{word}`", path.join("::")));
            }
        }
    }
    found
}

#[test]
fn layers_only_depend_downwards() {
    // Spelled in two pieces: this file lives in src/shell, and the scan reads the source.
    let server = concat!("crate::", "mcp");
    let rules: &[(&str, &[&str])] = &[
        ("src/logic", &["gpui", "crate::platform", "crate::ui", "crate::views", "crate::shell", server]),
        ("src/platform", &["gpui", "crate::ui", "crate::views", "crate::shell", server]),
        ("src/ui", &["crate::views", "crate::shell", "impl Shell", server]),
        ("src/views", &["crate::shell", "impl Shell", server]),
        ("src/shell", &[server]),
        // The server uses logic, platform and the pure parts of ui. It never sees a window.
        ("src/mcp", &["gpui", "crate::views", "crate::shell", "impl Shell"]),
    ];
    for (dir, banned) in rules {
        for path in files(Path::new(dir)) {
            // This file's own fixtures are deliberate violations.
            if path.ends_with("tests/layers.rs") {
                continue;
            }
            let code = fs::read_to_string(&path).unwrap();
            let found = violations(&code, &module_of(&path), banned);
            assert!(found.is_empty(), "{}: {}", path.display(), found.join(", "));
        }
    }
}

fn module(path: &str) -> Vec<String> {
    module_of(Path::new(path))
}

/// The scan itself is checked on snippets that break the rule in every way a `use` can be spelled.
#[test]
fn the_layers_scan_sees_every_spelling_of_a_use() {
    let logic = module("src/logic/dock.rs");
    let banned = ["gpui", "crate::ui", "crate::views"];
    for bad in [
        "use gpui::App;",
        "use gpui::{App, Window};",
        "pub use crate::ui::theme::Theme;",
        "use crate::ui;",
        "use crate::{ui, logic};",
        "use crate::{logic::table, ui::theme};",
        "use crate::{logic::{table, block}, ui::{self, theme}};",
        "use crate::{ui as interface};",
        "use crate::ui::*;",
        "use crate::*;",
        "use crate::{views::{editor::grid, nav}};",
        "use crate::{\n    logic,\n    views::nav,\n};",
        "use gpui as g;",
        "pub(crate) use ::gpui::Entity;",
        "use super::super::ui::theme;",
        "use super::super::{ui, views};",
        "fn f() { let t = crate::ui::theme::Theme::dark(); }",
    ] {
        assert!(!violations(bad, &logic, &banned).is_empty(), "missed: {bad}");
    }
    // The deep modules of a layer reach each other through `self` and `super` too.
    let deep = module("src/views/editor/layout.rs");
    assert!(!violations("use super::super::super::shell::Shell;", &deep, &["crate::shell"]).is_empty());
    assert!(violations("use super::grid::Grid;", &deep, &["crate::shell"]).is_empty());
}

#[test]
fn the_layers_scan_lets_allowed_code_through() {
    let logic = module("src/logic/dock.rs");
    let banned = ["gpui", "crate::ui", "crate::views"];
    for fine in [
        "use std::fs;",
        "use crate::logic::table;",
        "use crate::{logic::{table, block}, lang::tr};",
        "use super::markdown::*;",
        "use self::inner::thing;",
        "use crate::user_interface::x;",
        "// use crate::ui::theme::Theme;",
        "let uses = 1; // use gpui::App;",
        "use serde::{Deserialize, Serialize as S};",
        "#[cfg(test)]\nmod tests {\n    use super::*;\n}",
    ] {
        assert!(violations(fine, &logic, &banned).is_empty(), "false alarm: {fine}: {:?}", violations(fine, &logic, &banned));
    }
    // A shell file may use anything but the server.
    assert!(violations("use crate::{ui, views, platform};", &module("src/shell/mod.rs"), &[concat!("crate::", "mcp")]).is_empty());
}

#[test]
fn module_paths_follow_the_files() {
    assert_eq!(module("src/logic/markdown/mod.rs"), ["crate", "logic", "markdown"]);
    assert_eq!(module("src/logic/dock.rs"), ["crate", "logic", "dock"]);
    assert_eq!(module("src/main.rs"), ["crate"]);
}
