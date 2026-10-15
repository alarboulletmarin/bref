//! Le catalogue des commandes que la palette propose : pour chacune, un libellé court dans les mots de
//! la personne, une icône et un groupe. Le groupe de ce qu'on regarde (un bloc, un tableau, un schéma)
//! passe en tête ; les touches viennent des liaisons en vigueur. Fonctions pures.

use crate::tr;

/// Les groupes, dans l'ordre où la palette les montre, après celui de la vue.
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
    /// Son titre ; `here` : c'est le groupe de ce qu'on regarde.
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

/// Une commande proposée : son action (par son nom), son libellé, son icône, son groupe, ses touches
/// en vigueur (vides : aucune).
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub action: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub group: Group,
    pub keys: String,
}

/// Toutes les commandes, groupe par groupe. Une action que l'app ne connaît pas, ou que la vue ne sait
/// pas faire, est écartée par qui s'en sert.
pub fn catalog() -> Vec<(&'static str, &'static str, &'static str, Group)> {
    use Group::*;
    vec![
        ("editor::BlockHeading1", tr("Turn into heading 1", "Transformer en titre 1"), "c-heading.svg", Block),
        ("editor::BlockHeading2", tr("Turn into heading 2", "Transformer en titre 2"), "c-heading.svg", Block),
        ("editor::BlockBullet", tr("Turn into a bulleted list", "Transformer en liste à puces"), "list-bullet.svg", Block),
        ("editor::BlockTask", tr("Turn into a task list", "Transformer en liste de tâches"), "list-task.svg", Block),
        ("editor::BlockUp", tr("Move the block up", "Monter le bloc"), "c-up.svg", Block),
        ("editor::BlockDown", tr("Move the block down", "Descendre le bloc"), "c-down.svg", Block),
        ("editor::BlockDuplicate", tr("Duplicate the block", "Dupliquer le bloc"), "copy.svg", Block),
        ("editor::BlockDelete", tr("Delete the block", "Supprimer le bloc"), "trash.svg", Block),
        ("editor::BlockText", tr("Turn into text", "Transformer en texte"), "c-text.svg", Block),
        ("editor::BlockHeading3", tr("Turn into heading 3", "Transformer en titre 3"), "c-heading.svg", Block),
        ("editor::BlockOrdered", tr("Turn into a numbered list", "Transformer en liste numérotée"), "list-number.svg", Block),
        ("editor::BlockQuote", tr("Turn into a quote", "Transformer en citation"), "c-quote.svg", Block),
        ("editor::BlockCode", tr("Turn into a code block", "Transformer en bloc de code"), "code.svg", Block),
        ("editor::TableRowAbove", tr("Insert a row above", "Insérer une ligne au-dessus"), "c-ins-above.svg", Table),
        ("editor::TableRowBelow", tr("Insert a row below", "Insérer une ligne en dessous"), "c-ins-below.svg", Table),
        ("editor::TableColLeft", tr("Insert a column to the left", "Insérer une colonne à gauche"), "c-ins-left.svg", Table),
        ("editor::TableColRight", tr("Insert a column to the right", "Insérer une colonne à droite"), "c-ins-right.svg", Table),
        ("editor::TableMoveRowUp", tr("Move the row up", "Monter la ligne"), "c-up.svg", Table),
        ("editor::TableMoveRowDown", tr("Move the row down", "Descendre la ligne"), "c-down.svg", Table),
        ("editor::TableMoveColLeft", tr("Move the column left", "Décaler la colonne à gauche"), "c-left.svg", Table),
        ("editor::TableMoveColRight", tr("Move the column right", "Décaler la colonne à droite"), "c-right.svg", Table),
        ("editor::TableDuplicateRow", tr("Duplicate the row", "Dupliquer la ligne"), "copy.svg", Table),
        ("editor::TableDuplicateCol", tr("Duplicate the column", "Dupliquer la colonne"), "copy.svg", Table),
        ("editor::TableSortAsc", tr("Sort A to Z", "Trier de A à Z"), "c-sort-az.svg", Table),
        ("editor::TableSortDesc", tr("Sort Z to A", "Trier de Z à A"), "c-sort-za.svg", Table),
        ("editor::TableTranspose", tr("Swap rows and columns", "Échanger lignes et colonnes"), "c-transpose.svg", Table),
        ("editor::TableDeleteRow", tr("Delete the row", "Supprimer la ligne"), "row-del.svg", Table),
        ("editor::TableDeleteCol", tr("Delete the column", "Supprimer la colonne"), "col-del.svg", Table),
        ("editor::TableDelete", tr("Delete the table", "Supprimer le tableau"), "trash.svg", Table),
        ("sheet::SelectRow", tr("Select the row", "Sélectionner la ligne"), "c-select-row.svg", Table),
        ("sheet::SelectCol", tr("Select the column", "Sélectionner la colonne"), "c-select-col.svg", Table),
        ("sheet::Copy", tr("Copy the cells", "Copier les cellules"), "copy.svg", Table),
        ("sheet::Cut", tr("Cut the cells", "Couper les cellules"), "c-cut.svg", Table),
        ("sheet::Paste", tr("Paste", "Coller"), "c-paste.svg", Table),
        ("sheet::Undo", tr("Undo", "Annuler"), "c-undo.svg", Table),
        ("sheet::Redo", tr("Redo", "Rétablir"), "c-redo.svg", Table),
        ("sheet::PickDelimiter", tr("Change the delimiter…", "Changer le délimiteur…"), "table.svg", Table),
        ("sheet::PickEncoding", tr("Change the encoding…", "Changer l'encodage…"), "table.svg", Table),
        ("sheet::PickHeader", tr("Header row…", "Ligne d'en-tête…"), "table.svg", Table),
        ("sheet::PickCopyAs", tr("Copy the table as…", "Copier le tableau en…"), "copy.svg", Table),
        ("sheet::PickExport", tr("Export the table…", "Exporter le tableau…"), "export.svg", Table),
        ("canvas::SelectAll", tr("Select everything", "Tout sélectionner"), "d-select.svg", Diagram),
        ("canvas::Duplicate", tr("Duplicate the selection", "Dupliquer la sélection"), "copy.svg", Diagram),
        ("canvas::Erase", tr("Delete the selection", "Supprimer la sélection"), "trash.svg", Diagram),
        ("canvas::ZoomFit", tr("Fit the whole diagram", "Voir tout le schéma"), "target.svg", Diagram),
        ("canvas::ZoomIn", tr("Zoom in", "Zoomer"), "c-zoom-in.svg", Diagram),
        ("canvas::ZoomOut", tr("Zoom out", "Dézoomer"), "c-zoom-out.svg", Diagram),
        ("canvas::Undo", tr("Undo", "Annuler"), "c-undo.svg", Diagram),
        ("canvas::Redo", tr("Redo", "Rétablir"), "c-redo.svg", Diagram),
        ("editor::Bold", tr("Bold", "Gras"), "c-bold.svg", Format),
        ("editor::Italic", tr("Italic", "Italique"), "c-italic.svg", Format),
        ("editor::Underline", tr("Underline", "Souligné"), "c-underline.svg", Format),
        ("editor::Strike", tr("Strikethrough", "Barré"), "c-strike.svg", Format),
        ("editor::Highlight", tr("Highlight", "Surligner"), "c-highlight.svg", Format),
        ("editor::InlineCode", tr("Inline code", "Code en ligne"), "code.svg", Format),
        ("editor::InsertLink", tr("Link", "Lien"), "link.svg", Format),
        ("editor::Comment", tr("Comment", "Commenter"), "c-comment.svg", Format),
        ("editor::ToggleTask", tr("Tick the task", "Cocher la tâche"), "check.svg", Format),
        ("editor::Find", tr("Find in the note", "Chercher dans la note"), "search.svg", Note),
        ("editor::FindReplace", tr("Replace in the note", "Remplacer dans la note"), "search.svg", Note),
        ("app::Outline", tr("Outline of the note", "Plan de la note"), "c-outline.svg", Note),
        ("app::CopyAll", tr("Copy the whole note", "Copier toute la note"), "copy.svg", Note),
        ("editor::Undo", tr("Undo", "Annuler"), "c-undo.svg", Note),
        ("editor::Redo", tr("Redo", "Rétablir"), "c-redo.svg", Note),
        ("app::NewNote", tr("New note", "Nouvelle note"), "file-plus.svg", Create),
        ("app::NewFromTemplate", tr("New note from a template", "Nouvelle note depuis un modèle"), "file-plus.svg", Create),
        ("app::NewDiagram", tr("New diagram", "Nouveau schéma"), "d-rect.svg", Create),
        ("nav::NewFolder", tr("New folder", "Nouveau dossier"), "folder-plus.svg", Create),
        ("app::Today", tr("Today's note", "Note du jour"), "c-calendar.svg", Go),
        ("app::SearchVault", tr("Search the whole vault", "Chercher dans tout le coffre"), "search.svg", Go),
        ("nav::ShowTree", tr("Vault tree", "Arbre du coffre"), "tree.svg", Go),
        ("nav::ShowRecent", tr("Recent notes", "Notes récentes"), "clock.svg", Go),
        ("nav::ShowGraph", tr("Graph of the notes", "Graphe des notes"), "graph.svg", Go),
        ("nav::ShowTags", "Tags", "tag.svg", Go),
        ("nav::ShowLinks", tr("Backlinks", "Rétroliens"), "backlink.svg", Go),
        ("nav::ShowCalendar", tr("Calendar", "Calendrier"), "c-calendar.svg", Go),
        ("app::GoBack", tr("Back", "Précédent"), "chevron-left.svg", Go),
        ("app::GoForward", tr("Forward", "Suivant"), "chevron-right.svg", Go),
        ("app::SplitPane", tr("Second pane", "Second panneau"), "c-split.svg", Go),
        ("nav::ToggleFull", tr("Panel on the whole window", "Panneau sur toute la fenêtre"), "expand.svg", Go),
        ("app::SyncVault", tr("Sync the vault", "Synchroniser le coffre"), "backup.svg", Vault),
        ("app::OpenVault", tr("Change vault", "Changer de coffre"), "tree.svg", Vault),
        ("app::ChooseTheme", tr("Theme…", "Thème…"), "theme.svg", Look),
        ("app::ZoomIn", tr("Bigger text", "Texte plus grand"), "c-zoom-in.svg", Look),
        ("app::ZoomOut", tr("Smaller text", "Texte plus petit"), "c-zoom-out.svg", Look),
        ("app::ZoomReset", tr("Default text size", "Taille du texte d'origine"), "c-zoom-reset.svg", Look),
        ("app::ToggleHelp", tr("Keyboard shortcuts", "Raccourcis clavier"), "help.svg", App),
        ("app::Quit", tr("Quit Bref", "Quitter Bref"), "c-quit.svg", App),
    ]
}

/// Les commandes rangées par groupe, celui de la vue (`here`) en tête ; dans un groupe, l'ordre du
/// catalogue. Rend aussi combien sont du groupe de la vue.
pub fn arrange(mut list: Vec<Command>, here: Option<Group>) -> (Vec<Command>, usize) {
    list.sort_by_key(|c| (Some(c.group) != here, c.group));
    let count = list.iter().take_while(|c| Some(c.group) == here).count();
    (list, count)
}

/// Les touches lues sur un clavier (« Ctrl+Maj+C ») en touches séparées, pour les montrer chacune
/// dans sa pastille ; « Ctrl++ » garde son « + ».
pub fn keycaps(keys: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = keys;
    while !rest.is_empty() {
        match rest.find('+') {
            // Un « + » en tête est la touche elle-même.
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
    // Les flèches et l'effacement se lisent mieux en signes qu'en mots.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn command(action: &'static str, group: Group) -> Command {
        Command { action, label: action, icon: "", group, keys: String::new() }
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

    #[test]
    fn the_catalog_names_each_action_once() {
        let all = catalog();
        let mut seen = std::collections::HashSet::new();
        assert!(all.iter().all(|(action, ..)| seen.insert(*action)), "une action en double");
    }
}
