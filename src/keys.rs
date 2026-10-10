//! Raccourcis : la table des liaisons, ce que l'utilisateur en a changé, les conflits, le
//! fichier `keys`, et ce que le panneau d'aide montre et cherche (fonctions pures).

use crate::{MOD, tr};
use gpui::{Action, App, Keystroke};
use std::collections::BTreeMap;

/// Une liaison : des touches (écrites par `canon`), le nom d'une action, le contexte où elle vaut.
#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub keys: String,
    pub action: &'static str,
    pub context: Option<&'static str>,
}

/// Ce que l'utilisateur a changé : action → touches, vides pour « aucun raccourci ».
pub type Changes = BTreeMap<String, String>;

/// Une liaison de la table par défaut ; des touches illisibles y sont une faute du code.
pub fn bind(keys: &str, action: &dyn Action, context: Option<&'static str>) -> Bind {
    Bind { keys: canon(keys).expect("touches par défaut illisibles"), action: action.name(), context }
}

/// Les touches en vigueur d'une action, lues dans les liaisons de l'app : pour les indications
/// affichées hors du panneau d'aide (palette, note vide).
pub fn of(cx: &App, action: &dyn Action) -> String {
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let first = keymap.bindings_for_action(action).find_map(|binding| binding.keystrokes().first().map(|stroke| spell(stroke.inner())));
    first.map(|keys| show(&keys)).unwrap_or_default()
}

/// Pourquoi des touches sont refusées.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    Unreadable,
    /// Une touche seule, qui écrit du texte.
    Types,
    /// Échap : le panneau lui-même en a besoin.
    Panel,
}

/// Une ligne du panneau d'aide ; `action` quand ses touches se changent.
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

/// Sections du panneau d'aide : (titre, lignes).
pub type Sections = Vec<(&'static str, Vec<Row>)>;

const NAMED: [&str; 15] =
    ["enter", "tab", "escape", "space", "backspace", "delete", "insert", "home", "end", "pageup", "pagedown", "left", "right", "up", "down"];

fn parse(keys: &str) -> Option<Keystroke> {
    let stroke = Keystroke::parse(keys).ok()?;
    let key = stroke.key.as_str();
    let function = key.strip_prefix('f').is_some_and(|n| n.parse::<u8>().is_ok_and(|n| (1..=35).contains(&n)));
    (key.chars().count() == 1 || NAMED.contains(&key) || function).then_some(stroke)
}

/// Les touches d'une frappe, écrites d'une seule façon. `secondary` (Cmd sous macOS, Ctrl
/// ailleurs) reste ce qu'on écrit : un fichier porté sur un autre système garde son sens.
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

/// Des touches lues puis réécrites ; `None` quand elles sont illisibles.
pub fn canon(keys: &str) -> Option<String> {
    parse(keys).map(|stroke| spell(&stroke))
}

/// Des touches telles qu'on les lit sur un clavier : « Ctrl+Maj+P ».
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

/// Deux contextes où la même frappe peut arriver. Une liaison sans contexte, ou du « Shell »
/// qui englobe tout, croise toutes les autres ; les vues, elles, ne se rencontrent pas (les
/// flèches d'un tableau et celles d'un schéma), sauf un champ de saisie avec la vue qui le porte.
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

/// Les liaisons en vigueur : celles par défaut, où chaque action changée ne garde que les
/// touches choisies (dans chacun de ses contextes, à la place qu'elle occupait : à contexte
/// égal, la dernière liaison l'emporte). Un choix l'emporte sur une action modifiable (`open`)
/// qui a les mêmes touches par défaut : elle reste sans raccourci.
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

/// Pourquoi ces touches ne peuvent servir de raccourci, s'il y a une raison.
pub fn refusal(keys: &str) -> Option<Why> {
    let Some(stroke) = parse(keys) else { return Some(Why::Unreadable) };
    let m = stroke.modifiers;
    if stroke.key == "escape" && !(m.control || m.alt || m.platform || m.shift) {
        return Some(Why::Panel);
    }
    let types = stroke.key.chars().count() == 1 || stroke.key == "space";
    (types && !(m.control || m.alt || m.platform)).then_some(Why::Types)
}

/// Des touches que le système garde souvent pour lui : à signaler, pas à refuser.
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

/// Les actions qui ont déjà ces touches là où `action` s'applique.
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

/// Le fichier `keys` : une ligne `action=touches` par action changée.
pub fn write(changes: &Changes) -> String {
    changes.iter().map(|(action, keys)| format!("{action}={keys}\n")).collect()
}

/// Lit le fichier `keys`. Une ligne fautive (action inconnue ou non modifiable, touches
/// illisibles ou refusées, touches déjà prises) est écartée et rendue à part : la valeur par
/// défaut reste.
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
            // Prises par un autre choix du fichier, ou par une liaison qui ne se change pas.
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

/// Les touches en vigueur d'une action, telles qu'on les lit : « F1 / Ctrl+/ », ou rien.
pub fn shown(bound: &[Bind], action: &str) -> String {
    let mut keys: Vec<&str> = Vec::new();
    for bind in bound.iter().filter(|bind| bind.action == action) {
        if !keys.contains(&bind.keys.as_str()) {
            keys.push(&bind.keys);
        }
    }
    keys.into_iter().map(show).collect::<Vec<_>>().join(" / ")
}

/// Texte ramené à ce que la recherche compare : minuscules, sans accents.
// ponytail: les seuls accents du français ; passer à une normalisation Unicode si une autre
// langue arrive dans `tr`.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' => out.push('i'),
            'ô' | 'ö' => out.push('o'),
            'ù' | 'û' | 'ü' => out.push('u'),
            'ç' => out.push('c'),
            'œ' => out.push_str("oe"),
            '’' => out.push('\''),
            c => out.push(c),
        }
    }
    out
}

/// Les lignes dont les touches ou l'effet contiennent chaque mot cherché ; une section vide
/// disparaît.
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
        // Les touches, sans égard à la casse.
        assert_eq!(filter(all.clone(), "ctrl+p"), vec![("Notes", vec![Row::text("Ctrl+P", "Chercher, créer")])]);
        // Le texte, sans les accents ; la section sans ligne disparaît.
        assert_eq!(filter(all.clone(), "selection"), vec![("Navigation", vec![Row::text("Ctrl+clic", "Sélection à étendre")])]);
        // Chaque mot doit s'y trouver, touches et texte confondus.
        assert_eq!(filter(all.clone(), "ctrl graphe"), vec![("Navigation", vec![Row::text("Ctrl+G", "Graphe des notes")])]);
        assert!(filter(all, "zzz").is_empty());
    }

    fn b(keys: &str, action: &'static str, context: Option<&'static str>) -> Bind {
        Bind { keys: canon(keys).unwrap(), action, context }
    }

    /// Un petit jeu de liaisons : `Type` et `Close` ne sont pas modifiables.
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
        assert_eq!(canon(other).as_deref(), Some(other), "la touche qui n'est pas « secondary » garde son nom");
        // Illisibles : rien, deux touches, un nom de touche inconnu.
        assert_eq!(canon(""), None);
        assert_eq!(canon("a-b"), None);
        assert_eq!(canon("ctrl-foo"), None);
        assert_eq!(canon("ctrl"), None);
    }

    #[test]
    fn show_reads_like_a_keyboard() {
        assert_eq!(show("secondary-shift-p"), format!("{}+Maj+P", crate::MOD));
        assert_eq!(show("alt-enter"), "Alt+Entrée");
        assert_eq!(show("f1"), "F1");
        assert_eq!(show("secondary--"), format!("{}+-", crate::MOD));
    }

    #[test]
    fn contexts_that_meet() {
        assert!(meets(None, Some("Editor")) && meets(Some("Sheet"), Some("Shell")));
        assert!(meets(Some("Nav"), Some("Nav")));
        // Les flèches d'un tableau et d'un schéma ne se croisent jamais.
        assert!(!meets(Some("Sheet"), Some("Canvas")) && !meets(Some("Editor"), Some("Find")));
        // Une cellule en cours de saisie reçoit les touches d'une ligne de texte.
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
        // Les deux contextes suivent, à la place qu'occupait la liaison (l'ordre décide qui gagne).
        assert_eq!(of(&now, "editor::Find"), [("secondary-shift-g", Some("Editor")), ("secondary-shift-g", Some("Find"))]);
        assert_eq!(now.iter().position(|b| b.action == "editor::Find"), base.iter().position(|b| b.action == "editor::Find").map(|i| i - 2));
        // Deux touches par défaut : une seule après le choix.
        assert_eq!(of(&now, "app::Help"), [("f9", None)]);
        // Sans raccourci.
        assert!(of(&now, "nav::Graph").is_empty());
        assert_eq!(of(&now, "app::Palette"), [("secondary-p", None)]);
    }

    #[test]
    fn a_choice_wins_over_a_default() {
        // Une version suivante donne à une action les touches que l'utilisateur a déjà prises :
        // son choix reste, l'action arrive sans raccourci.
        let mut changes = Changes::new();
        changes.insert("nav::Graph".into(), "secondary-p".into());
        let now = effective(&base(), &changes, &open);
        assert_eq!(of(&now, "nav::Graph"), [("secondary-p", Some("Shell"))]);
        assert!(of(&now, "app::Palette").is_empty());
        // Dans des contextes qui ne se croisent pas, les deux restent.
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
        // Libre, ou déjà à soi.
        assert_eq!(clash(&base, &base, "app::Palette", "secondary-k"), None);
        assert_eq!(clash(&base, &base, "app::Palette", "secondary-p"), None);
        // Une action de partout croise celle de l'éditeur, et l'inverse.
        assert_eq!(clash(&base, &base, "app::Palette", "secondary-f"), Some("editor::Find"));
        assert_eq!(clash(&base, &base, "editor::Find", "secondary-g"), Some("nav::Graph"));
        // Deux vues qui ne se rencontrent pas.
        assert_eq!(clash(&base, &base, "sheet::Undo", "secondary-d"), None);
        // Dans son propre contexte.
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
        // Un échange est permis : chacune prend les touches de l'autre.
        let (swap, faults) = read("app::Palette=secondary-g\nnav::Graph=secondary-p\n", &base, &open);
        assert_eq!((swap.len(), faults.len()), (2, 0));
        // Chaque ligne fautive est écartée et nommée, les autres restent.
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
        // Deux touches par défaut, une fois chacune même liée dans deux contextes.
        assert_eq!(shown(&base(), "app::Help"), format!("F1 / {}+/", crate::MOD));
        assert_eq!(shown(&base(), "editor::Find"), format!("{}+F", crate::MOD));
    }
}
