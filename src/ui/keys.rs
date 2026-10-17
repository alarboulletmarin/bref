//! Shortcuts: the binding table, what the user changed in it, conflicts, the
//! `keys` file, and what the help panel shows and searches (pure functions).

use crate::lang::tr;
use crate::lang::fold;
use gpui::{Action, App, Keystroke};
use std::collections::BTreeMap;

/// Shortcut key, as displayed: Cmd on macOS, Ctrl elsewhere.
pub const MOD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

/// A binding: keys (written by `canon`), an action name, the context where it applies.
#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub keys: String,
    pub action: &'static str,
    pub context: Option<&'static str>,
}

/// What the user changed: action → keys, empty for "no shortcut".
pub type Changes = BTreeMap<String, String>;

/// A binding from the default table; unreadable keys there are a bug in the code.
pub fn bind(keys: &str, action: &dyn Action, context: Option<&'static str>) -> Bind {
    Bind { keys: canon(keys).expect("default keys unreadable"), action: action.name(), context }
}

/// The keys in force for an action, read from the app's bindings: for the hints
/// shown outside the help panel (palette, empty note).
pub fn of(cx: &App, action: &dyn Action) -> String {
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let first = keymap.bindings_for_action(action).find_map(|binding| binding.keystrokes().first().map(|stroke| spell(stroke.inner())));
    first.map(|keys| show(&keys)).unwrap_or_default()
}

/// Why keys are refused.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    Unreadable,
        /// A single key, which types text.
    Types,
        /// Escape: the panel itself needs it.
    Panel,
}

/// A help panel line; `action` when its keys can be changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub keys: String,
    pub effect: &'static str,
    pub action: Option<&'static str>,
    pub changed: bool,
}

impl Row {
    pub fn text(keys: impl Into<String>, effect: &'static str) -> Self {
        Self { keys: keys.into(), effect, action: None, changed: false }
    }
}

/// Help panel sections: (title, lines).
pub type Sections = Vec<(&'static str, Vec<Row>)>;

const NAMED: [&str; 15] =
    ["enter", "tab", "escape", "space", "backspace", "delete", "insert", "home", "end", "pageup", "pagedown", "left", "right", "up", "down"];

fn parse(keys: &str) -> Option<Keystroke> {
    let stroke = Keystroke::parse(keys).ok()?;
    let key = stroke.key.as_str();
    let function = key.strip_prefix('f').is_some_and(|n| n.parse::<u8>().is_ok_and(|n| (1..=35).contains(&n)));
    (key.chars().count() == 1 || NAMED.contains(&key) || function).then_some(stroke)
}

/// The keys of a keystroke, written one way only. `secondary` (Cmd on macOS, Ctrl
/// elsewhere) stays as written: a file carried to another system keeps its meaning.
pub fn spell(stroke: &Keystroke) -> String {
    let m = stroke.modifiers;
    let mac = cfg!(target_os = "macos");
    let (secondary, other) = if mac { (m.platform, m.control) } else { (m.control, m.platform) };
    let mut parts = Vec::new();
    for (held, name) in [(secondary, "secondary"), (other, if mac { "ctrl" } else { "cmd" }), (m.alt, "alt"), (m.shift, "shift"), (m.function, "fn")] {
        if held {
            parts.push(name);
        }
    }
    parts.push(&stroke.key);
    parts.join("-")
}

/// Keys read then rewritten; `None` when they are unreadable.
pub fn canon(keys: &str) -> Option<String> {
    parse(keys).map(|stroke| spell(&stroke))
}

/// Keys as read on a keyboard: "Ctrl+Shift+P".
pub fn show(keys: &str) -> String {
    let Some(stroke) = parse(keys) else { return keys.to_string() };
    let m = stroke.modifiers;
    let mac = cfg!(target_os = "macos");
    let (secondary, other) = if mac { (m.platform, m.control) } else { (m.control, m.platform) };
    let mut parts: Vec<String> = Vec::new();
    for (held, name) in [(secondary, MOD), (other, if mac { "Ctrl" } else { "Super" }), (m.alt, "Alt"), (m.shift, tr("Shift", "Maj")), (m.function, "Fn")] {
        if held {
            parts.push(name.to_string());
        }
    }
    parts.push(match stroke.key.as_str() {
        "enter" => tr("Enter", "Entrée").into(),
        "escape" => tr("Esc", "Échap").into(),
        "space" => tr("Space", "Espace").into(),
        "backspace" => tr("Backspace", "Retour arrière").into(),
        "delete" => tr("Delete", "Suppr").into(),
        "insert" => tr("Insert", "Inser").into(),
        "home" => tr("Home", "Début").into(),
        "end" => tr("End", "Fin").into(),
        "pageup" => tr("Page Up", "Page haut").into(),
        "pagedown" => tr("Page Down", "Page bas").into(),
        "left" => tr("Left", "Gauche").into(),
        "right" => tr("Right", "Droite").into(),
        "up" => tr("Up", "Haut").into(),
        "down" => tr("Down", "Bas").into(),
        "tab" => "Tab".into(),
        key => key.to_uppercase(),
    });
    parts.join("+")
}

/// Two contexts where the same keystroke can arrive. A binding with no context, or of "Shell"
/// which encloses everything, meets all others; views do not meet each other (a table's
/// arrows and a diagram's), except an input field with the view that carries it.
pub fn meets(a: Option<&str>, b: Option<&str>) -> bool {
    let everywhere = |c: Option<&str>| matches!(c, None | Some("Shell"));
    let field = |a: Option<&str>, b: Option<&str>| a == Some("Line") && matches!(b, Some("Sheet" | "Palette"));
    everywhere(a) || everywhere(b) || a == b || field(a, b) || field(b, a)
}

fn contexts(defaults: &[Bind], action: &str) -> Vec<Option<&'static str>> {
    let mut found = Vec::new();
    for bind in defaults.iter().filter(|bind| bind.action == action) {
        if !found.contains(&bind.context) {
            found.push(bind.context);
        }
    }
    found
}

/// The bindings in force: the default ones, where each changed action keeps only the
/// chosen keys (in each of its contexts, at the place it held: for an equal
/// context, the last binding wins). A choice wins over a changeable action (`open`)
/// that has the same keys by default: it is left without a shortcut.
pub fn effective(defaults: &[Bind], changes: &Changes, open: &dyn Fn(&str) -> bool) -> Vec<Bind> {
    let taken: Vec<(&str, Option<&'static str>)> = changes
        .iter()
        .filter(|(_, keys)| !keys.is_empty())
        .flat_map(|(action, keys)| contexts(defaults, action).into_iter().map(move |context| (keys.as_str(), context)))
        .collect();
    let mut out: Vec<Bind> = Vec::with_capacity(defaults.len());
    for bind in defaults {
        match changes.get(bind.action) {
            Some(keys) => {
                let placed = out.iter().any(|b| b.action == bind.action && b.context == bind.context);
                if !keys.is_empty() && !placed {
                    out.push(Bind { keys: keys.clone(), ..bind.clone() });
                }
            }
            None => {
                let lost = open(bind.action) && taken.iter().any(|(keys, context)| *keys == bind.keys && meets(*context, bind.context));
                if !lost {
                    out.push(bind.clone());
                }
            }
        }
    }
    out
}

/// Why these keys cannot be a shortcut, if there is a reason.
pub fn refusal(keys: &str) -> Option<Why> {
    let Some(stroke) = parse(keys) else { return Some(Why::Unreadable) };
    let m = stroke.modifiers;
    if stroke.key == "escape" && !(m.control || m.alt || m.platform || m.shift) {
        return Some(Why::Panel);
    }
    let types = stroke.key.chars().count() == 1 || stroke.key == "space";
    (types && !(m.control || m.alt || m.platform)).then_some(Why::Types)
}

/// Keys the system often keeps for itself: to flag, not to refuse.
pub fn system(keys: &str) -> bool {
    let Some(stroke) = parse(keys) else { return false };
    let m = stroke.modifiers;
    let key = stroke.key.as_str();
    if cfg!(target_os = "macos") {
        m.platform && matches!(key, "tab" | "space" | "h" | "m")
    } else {
        m.platform || (m.alt && matches!(key, "tab" | "f4" | "space")) || (m.control && m.alt && key == "delete")
    }
}

/// The actions that already have these keys where `action` applies.
pub fn clashes(bound: &[Bind], defaults: &[Bind], action: &str, keys: &str) -> Vec<&'static str> {
    let at = contexts(defaults, action);
    let mut found = Vec::new();
    for bind in bound {
        if bind.keys == keys && bind.action != action && at.iter().any(|context| meets(*context, bind.context)) && !found.contains(&bind.action) {
            found.push(bind.action);
        }
    }
    found
}

#[cfg(test)]
fn clash(bound: &[Bind], defaults: &[Bind], action: &str, keys: &str) -> Option<&'static str> {
    clashes(bound, defaults, action, keys).first().copied()
}

/// The `keys` file: one `action=keys` line per changed action.
pub fn write(changes: &Changes) -> String {
    changes.iter().map(|(action, keys)| format!("{action}={keys}\n")).collect()
}

/// Reads the `keys` file. A faulty line (unknown or unchangeable action, keys
/// unreadable or refused, keys already taken) is set aside and returned separately: the default
/// value stays.
pub fn read(text: &str, defaults: &[Bind], open: &dyn Fn(&str) -> bool) -> (Changes, Vec<String>) {
    let (mut changes, mut faults) = (Changes::new(), Vec::new());
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let wanted = line.split_once('=').and_then(|(action, keys)| {
            let (action, keys) = (action.trim(), keys.trim());
            let known = open(action) && defaults.iter().any(|bind| bind.action == action);
            if !known {
                return None;
            }
            if keys.is_empty() {
                return Some((action, String::new()));
            }
            let keys = canon(keys).filter(|keys| refusal(keys).is_none())?;
                        // Taken by another choice in the file, or by a binding that cannot change.
            let at = contexts(defaults, action);
            let crosses = |other: &str| contexts(defaults, other).iter().any(|a| at.iter().any(|b| meets(*a, *b)));
            let chosen = changes.iter().any(|(other, theirs)| other != action && *theirs == keys && crosses(other));
            let fixed = clashes(defaults, defaults, action, &keys).into_iter().any(|other| !open(other));
            (!chosen && !fixed).then_some((action, keys))
        });
        match wanted {
            Some((action, keys)) => {
                changes.insert(action.to_string(), keys);
            }
            None => faults.push(line.to_string()),
        }
    }
    (changes, faults)
}

/// The keys in force for an action, as read: "F1 / Ctrl+/", or nothing.
pub fn shown(bound: &[Bind], action: &str) -> String {
    let mut keys: Vec<&str> = Vec::new();
    for bind in bound.iter().filter(|bind| bind.action == action) {
        if !keys.contains(&bind.keys.as_str()) {
            keys.push(&bind.keys);
        }
    }
    keys.into_iter().map(show).collect::<Vec<_>>().join(" / ")
}

/// The lines whose keys or effect contain each searched word; an empty section
/// disappears.
pub fn filter(sections: Sections, query: &str) -> Sections {
    let query = fold(query);
    let words: Vec<&str> = query.split_whitespace().collect();
    sections
        .into_iter()
        .filter_map(|(title, mut rows)| {
            rows.retain(|row| {
                let row = fold(&format!("{} {}", row.keys, row.effect));
                words.iter().all(|word| row.contains(word))
            });
            (!rows.is_empty()).then_some((title, rows))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sections {
        vec![
            ("Notes", vec![Row::text("Ctrl+P", "Chercher, créer"), Row::text("Ctrl+Shift+F", "Chercher dans le texte")]),
            ("Navigation", vec![Row::text("Ctrl+G", "Graphe des notes"), Row::text("Ctrl+clic", "Sélection à étendre")]),
        ]
    }

    #[test]
    fn fold_ignores_case_and_accents() {
        assert_eq!(fold("Créer À la FRAPPE"), "creer a la frappe");
        assert_eq!(fold("cœur, Çà, où"), "coeur, ca, ou");
        assert_eq!(fold("l’aide"), "l'aide");
    }

    #[test]
    fn filter_matches_keys_and_text() {
        let all = sample();
        assert_eq!(filter(all.clone(), "  "), all);
                // The keys, ignoring case.
        assert_eq!(filter(all.clone(), "ctrl+p"), vec![("Notes", vec![Row::text("Ctrl+P", "Chercher, créer")])]);
                // The text, without accents; a section with no line disappears.
        assert_eq!(filter(all.clone(), "selection"), vec![("Navigation", vec![Row::text("Ctrl+clic", "Sélection à étendre")])]);
                // Each word must be there, keys and text together.
        assert_eq!(filter(all.clone(), "ctrl graphe"), vec![("Navigation", vec![Row::text("Ctrl+G", "Graphe des notes")])]);
        assert!(filter(all, "zzz").is_empty());
    }

    fn b(keys: &str, action: &'static str, context: Option<&'static str>) -> Bind {
        Bind { keys: canon(keys).unwrap(), action, context }
    }

        /// A small set of bindings: `Type` and `Close` cannot be changed.
    fn base() -> Vec<Bind> {
        vec![
            b("secondary-p", "app::Palette", None),
            b("f1", "app::Help", None),
            b("secondary-/", "app::Help", None),
            b("secondary-g", "nav::Graph", Some("Shell")),
            b("enter", "editor::Type", Some("Editor")),
            b("escape", "editor::Close", Some("Editor")),
            b("secondary-d", "editor::Next", Some("Editor")),
            b("secondary-f", "editor::Find", Some("Editor")),
            b("secondary-f", "editor::Find", Some("Find")),
            b("secondary-d", "nav::Twin", Some("Nav")),
            b("secondary-z", "sheet::Undo", Some("Sheet")),
        ]
    }

    fn open(action: &str) -> bool {
        !matches!(action, "editor::Type" | "editor::Close")
    }

    fn of<'a>(binds: &'a [Bind], action: &str) -> Vec<(&'a str, Option<&'static str>)> {
        binds.iter().filter(|b| b.action == action).map(|b| (b.keys.as_str(), b.context)).collect()
    }

    #[test]
    fn canon_spells_one_way() {
        assert_eq!(canon("shift-secondary-P").as_deref(), Some("secondary-shift-p"));
        assert_eq!(canon("secondary--").as_deref(), Some("secondary--"));
        assert_eq!(canon("alt-F3").as_deref(), Some("alt-f3"));
        let other = if cfg!(target_os = "macos") { "ctrl-k" } else { "cmd-k" };
        assert_eq!(canon(other).as_deref(), Some(other), "a key other than `secondary` keeps its name");
                // Unreadable: nothing, two keys, an unknown key name.
        assert_eq!(canon(""), None);
        assert_eq!(canon("a-b"), None);
        assert_eq!(canon("ctrl-foo"), None);
        assert_eq!(canon("ctrl"), None);
    }

    #[test]
    fn show_reads_like_a_keyboard() {
        assert_eq!(show("secondary-shift-p"), format!("{}+Maj+P", MOD));
        assert_eq!(show("alt-enter"), "Alt+Entrée");
        assert_eq!(show("f1"), "F1");
        assert_eq!(show("secondary--"), format!("{}+-", MOD));
    }

    #[test]
    fn contexts_that_meet() {
        assert!(meets(None, Some("Editor")) && meets(Some("Sheet"), Some("Shell")));
        assert!(meets(Some("Nav"), Some("Nav")));
                // A table's arrows and a diagram's never meet.
        assert!(!meets(Some("Sheet"), Some("Canvas")) && !meets(Some("Editor"), Some("Find")));
                // A cell being edited receives the keys of a text line.
        assert!(meets(Some("Line"), Some("Sheet")) && meets(Some("Palette"), Some("Line")));
    }

    #[test]
    fn a_change_replaces_the_default_in_each_context() {
        let base = base();
        assert_eq!(effective(&base, &Changes::new(), &open), base);
        let mut changes = Changes::new();
        changes.insert("editor::Find".into(), "secondary-shift-g".into());
        changes.insert("app::Help".into(), "f9".into());
        changes.insert("nav::Graph".into(), String::new());
        let now = effective(&base, &changes, &open);
                // Both contexts follow, at the place the binding held (the order decides who wins).
        assert_eq!(of(&now, "editor::Find"), [("secondary-shift-g", Some("Editor")), ("secondary-shift-g", Some("Find"))]);
        assert_eq!(now.iter().position(|b| b.action == "editor::Find"), base.iter().position(|b| b.action == "editor::Find").map(|i| i - 2));
                // Two default keys: only one after the choice.
        assert_eq!(of(&now, "app::Help"), [("f9", None)]);
                // No shortcut.
        assert!(of(&now, "nav::Graph").is_empty());
        assert_eq!(of(&now, "app::Palette"), [("secondary-p", None)]);
    }

    #[test]
    fn a_choice_wins_over_a_default() {
                // A later version gives an action keys the user has already taken:
                // their choice stays, the action arrives without a shortcut.
        let mut changes = Changes::new();
        changes.insert("nav::Graph".into(), "secondary-p".into());
        let now = effective(&base(), &changes, &open);
        assert_eq!(of(&now, "nav::Graph"), [("secondary-p", Some("Shell"))]);
        assert!(of(&now, "app::Palette").is_empty());
                // In contexts that do not meet, both stay.
        let mut changes = Changes::new();
        changes.insert("sheet::Undo".into(), "secondary-d".into());
        let now = effective(&base(), &changes, &open);
        assert_eq!(of(&now, "editor::Next").len() + of(&now, "nav::Twin").len(), 2);
    }

    #[test]
    fn refused_keys() {
        assert_eq!(refusal("a"), Some(Why::Types));
        assert_eq!(refusal("shift-a"), Some(Why::Types));
        assert_eq!(refusal("space"), Some(Why::Types));
        assert_eq!(refusal("escape"), Some(Why::Panel));
        assert_eq!(refusal("ctrl-foo"), Some(Why::Unreadable));
        assert_eq!(refusal("secondary-a"), None);
        assert_eq!(refusal("f5"), None);
        assert_eq!(refusal("alt-left"), None);
    }

    #[test]
    fn keys_the_system_keeps() {
        assert!(!system("secondary-shift-y") && !system("f5") && !system("alt-left"));
        if cfg!(target_os = "macos") {
            assert!(system("cmd-tab") && system("cmd-space"));
        } else {
            assert!(system("alt-tab") && system("alt-f4") && system("cmd-l"));
        }
    }

    #[test]
    fn clash_names_who_has_the_keys() {
        let base = base();
                // Free, or already one's own.
        assert_eq!(clash(&base, &base, "app::Palette", "secondary-k"), None);
        assert_eq!(clash(&base, &base, "app::Palette", "secondary-p"), None);
                // An everywhere action meets the editor's, and the reverse.
        assert_eq!(clash(&base, &base, "app::Palette", "secondary-f"), Some("editor::Find"));
        assert_eq!(clash(&base, &base, "editor::Find", "secondary-g"), Some("nav::Graph"));
                // Two views that do not meet.
        assert_eq!(clash(&base, &base, "sheet::Undo", "secondary-d"), None);
                // In its own context.
        assert_eq!(clash(&base, &base, "editor::Find", "secondary-d"), Some("editor::Next"));
        assert_eq!(clash(&base, &base, "editor::Find", "enter"), Some("editor::Type"));
    }

    #[test]
    fn the_file_keeps_what_differs_and_survives_faults() {
        let base = base();
        let mut changes = Changes::new();
        changes.insert("app::Help".into(), "f9".into());
        changes.insert("nav::Graph".into(), String::new());
        let text = write(&changes);
        assert_eq!(text, "app::Help=f9\nnav::Graph=\n");
        assert_eq!(read(&text, &base, &open), (changes.clone(), Vec::new()));
                // A swap is allowed: each takes the other's keys.
        let (swap, faults) = read("app::Palette=secondary-g\nnav::Graph=secondary-p\n", &base, &open);
        assert_eq!((swap.len(), faults.len()), (2, 0));
                // Each faulty line is set aside and named, the others stay.
        let text = "# à la main\n\napp::Help=f9\nno::Such=f8\neditor::Type=f7\napp::Palette=ctrl-foo\nnav::Graph=a\nsans égal\neditor::Find=enter\nsheet::Undo=f9\n";
        let (kept, faults) = read(text, &base, &open);
        assert_eq!(kept, Changes::from([("app::Help".to_string(), "f9".to_string())]));
        assert_eq!(faults, ["no::Such=f8", "editor::Type=f7", "app::Palette=ctrl-foo", "nav::Graph=a", "sans égal", "editor::Find=enter", "sheet::Undo=f9"]);
    }

    #[test]
    fn rows_show_the_keys_in_effect() {
        let mut changes = Changes::new();
        changes.insert("app::Help".into(), "f9".into());
        changes.insert("nav::Graph".into(), String::new());
        let now = effective(&base(), &changes, &open);
        assert_eq!(shown(&now, "app::Help"), "F9");
        assert_eq!(shown(&now, "nav::Graph"), "");
                // Two default keys, once each even when bound in two contexts.
        assert_eq!(shown(&base(), "app::Help"), format!("F1 / {}+/", MOD));
        assert_eq!(shown(&base(), "editor::Find"), format!("{}+F", MOD));
    }
}
