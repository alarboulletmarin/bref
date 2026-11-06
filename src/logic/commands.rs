//! The catalogue of commands the palette offers: for each, a short label in the person's
//! words, an icon and a group. The group of what is being viewed (a block, a table, a diagram)
//! comes first; keys come from the bindings in effect. Pure functions.

use crate::lang::tr;

/// The groups, in the order the palette shows them, after the view's.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Group {
    Block,
    Table,
    Diagram,
    Format,
    Note,
    Create,
    Go,
    Vault,
    Look,
    App,
}

impl Group {
    /// Its title; `here`: it is the group of what is being viewed.
    pub fn title(self, here: bool) -> &'static str {
        match (self, here) {
            (Group::Block, true) => tr("This block", "Ce bloc"),
            (Group::Block, false) => tr("Block", "Bloc"),
            (Group::Table, true) => tr("This table", "Ce tableau"),
            (Group::Table, false) => tr("Table", "Tableau"),
            (Group::Diagram, true) => tr("This diagram", "Ce schéma"),
            (Group::Diagram, false) => tr("Diagram", "Schéma"),
            (Group::Format, _) => tr("Formatting", "Mise en forme"),
            (Group::Note, _) => "Note",
            (Group::Create, _) => tr("Create", "Créer"),
            (Group::Go, _) => tr("Go to", "Aller à"),
            (Group::Vault, _) => tr("Vault", "Coffre"),
            (Group::Look, _) => tr("Appearance", "Apparence"),
            (Group::App, _) => "Bref",
        }
    }
}

/// An offered command: its action (by name), its label, its icon, its group, its keys
/// in effect (empty: none), and whether the palette lists it even when nothing is typed.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub action: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub group: Group,
    pub keys: String,
    pub pinned: bool,
}

/// Commands sorted by group, the view's (`here`) first; within a group, the
/// catalogue's order. Also returns how many belong to the view's group.
pub fn arrange(mut list: Vec<Command>, here: Option<Group>) -> (Vec<Command>, usize) {
    list.sort_by_key(|c| (Some(c.group) != here, c.group));
    let count = list.iter().take_while(|c| Some(c.group) == here).count();
    (list, count)
}

/// Keys as read on a keyboard ("Ctrl+Shift+C") into separate keys, to show each
/// in its badge; "Ctrl++" keeps its "+".
pub fn keycaps(keys: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = keys;
    while !rest.is_empty() {
        match rest.find('+') {
            // A leading "+" is the key itself.
            Some(0) => {
                out.push("+".into());
                rest = rest[1..].strip_prefix('+').unwrap_or(&rest[1..]);
            }
            Some(at) => {
                out.push(rest[..at].to_string());
                rest = &rest[at + 1..];
            }
            None => {
                out.push(rest.to_string());
                rest = "";
            }
        }
    }
    // Arrows and erasing read better as signs than as words.
    for key in &mut out {
        let sign = match key.as_str() {
            "Up" | "Haut" => "↑",
            "Down" | "Bas" => "↓",
            "Left" | "Gauche" => "←",
            "Right" | "Droite" => "→",
            "Backspace" | "Retour arrière" => "⌫",
            _ => continue,
        };
        *key = sign.to_string();
    }
    out
}

/// The menu entry that shows a file in the file manager, under its name on each system.
pub fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        tr("Reveal in Finder", "Afficher dans le Finder")
    } else if cfg!(windows) {
        tr("Show in Explorer", "Afficher dans l'Explorateur")
    } else {
        tr("Show in the file manager", "Afficher dans le gestionnaire de fichiers")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(action: &'static str, group: Group) -> Command {
        Command { action, label: action, icon: "", group, keys: String::new(), pinned: false }
    }

    #[test]
    fn puts_the_group_of_the_view_first() {
        let list = vec![command("a", Group::Note), command("b", Group::Table), command("c", Group::Go), command("d", Group::Table)];
        let (sorted, here) = arrange(list.clone(), Some(Group::Table));
        assert_eq!(sorted.iter().map(|c| c.action).collect::<Vec<_>>(), ["b", "d", "a", "c"]);
        assert_eq!(here, 2);
        let (plain, here) = arrange(list, None);
        assert_eq!(plain.iter().map(|c| c.action).collect::<Vec<_>>(), ["b", "d", "a", "c"]);
        assert_eq!(here, 0);
    }

    #[test]
    fn splits_keys_into_caps() {
        assert_eq!(keycaps("Ctrl+Maj+C"), ["Ctrl", "Maj", "C"]);
        assert_eq!(keycaps("Ctrl++"), ["Ctrl", "+"]);
        assert_eq!(keycaps("F10"), ["F10"]);
        assert_eq!(keycaps("Alt+Haut"), ["Alt", "↑"]);
        assert!(keycaps("").is_empty());
    }
}
