//! Les commandes de l'app, rangées comme le panneau d'aide : le menu de toutes les commandes et la
//! palette les montrent avec leurs touches, celles de la vue en cours d'abord. Fonctions pures.

use crate::keys::Sections;

/// Une commande : ce qu'elle fait, ses touches en vigueur (vides : aucune), le nom de son action.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub effect: &'static str,
    pub keys: String,
    pub action: &'static str,
}

/// Les sections du panneau d'aide réduites à leurs commandes (les lignes qui portent une action),
/// la section `here` en tête : ce qu'on fait là où l'on est passe avant le reste. Une action vue
/// deux fois (une touche pour le texte, une autre pour la grille) ne compte qu'une fois.
pub fn sections(all: Sections, here: Option<&str>) -> Vec<(&'static str, Vec<Command>)> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(&'static str, Vec<Command>)> = all
        .into_iter()
        .map(|(title, rows)| {
            let commands = rows
                .into_iter()
                .filter_map(|row| Some(Command { effect: row.effect, keys: row.keys, action: row.action? }))
                .filter(|command| seen.insert(command.action))
                .collect();
            (title, commands)
        })
        .filter(|(_, commands): &(_, Vec<Command>)| !commands.is_empty())
        .collect();
    if let Some(at) = here.and_then(|here| out.iter().position(|(title, _)| *title == here)) {
        let first = out.remove(at);
        out.insert(0, first);
    }
    out
}

/// Le rang suivant (ou précédent) parmi `n`, en bouclant ; aucun rang choisi part du premier ou du dernier.
pub fn step(at: Option<usize>, n: usize, down: bool) -> Option<usize> {
    if n == 0 {
        return None;
    }
    Some(match (at, down) {
        (None, true) => 0,
        (None, false) => n - 1,
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Row;

    fn act(keys: &str, effect: &'static str, action: &'static str) -> Row {
        Row { keys: keys.into(), effect, action: Some(action), changed: false }
    }

    #[test]
    fn keeps_commands_and_puts_the_view_first() {
        let all = vec![
            ("Notes", vec![act("Ctrl+N", "Nouvelle note", "app::NewNote"), Row::text("Ctrl+clic", "Ouvrir un lien")]),
            ("Clic", vec![Row::text("clic", "Sélectionner")]),
            ("Tableaux", vec![act("Ctrl+Suppr", "Retirer", "sheet::DeleteRows"), act("", "Trier", "editor::TableSortAsc")]),
            ("Texte", vec![act("Ctrl+Z", "Annuler", "sheet::DeleteRows")]),
        ];
        let plain = sections(all.clone(), None);
        // Les lignes sans action et la section qui n'en a pas disparaissent ; une action vue deux fois aussi.
        assert_eq!(plain.iter().map(|(t, c)| (*t, c.len())).collect::<Vec<_>>(), [("Notes", 1), ("Tableaux", 2)]);
        assert_eq!(plain[1].1[1], Command { effect: "Trier", keys: String::new(), action: "editor::TableSortAsc" });
        let here = sections(all.clone(), Some("Tableaux"));
        assert_eq!(here.iter().map(|(t, _)| *t).collect::<Vec<_>>(), ["Tableaux", "Notes"]);
        // Une vue sans section à elle garde l'ordre de l'aide.
        assert_eq!(sections(all, Some("Ailleurs")), plain);
    }

    #[test]
    fn steps_around() {
        assert_eq!(step(None, 3, true), Some(0));
        assert_eq!(step(None, 3, false), Some(2));
        assert_eq!(step(Some(2), 3, true), Some(0));
        assert_eq!(step(Some(0), 3, false), Some(2));
        assert_eq!(step(Some(0), 0, true), None);
    }
}
