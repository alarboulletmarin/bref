//! The command table: every command the palette offers, with its label, icon and group. The
//! action is a type, so a command that names no action does not compile.

use super::*;
use commands::Group::{self, *};
use gpui::Action;

/// One catalogue entry: the action's name, its label, its icon, its group.
pub(super) type Entry = (&'static str, &'static str, &'static str, Group);

fn c<A: Action>(label: &'static str, icon: &'static str, group: Group) -> Entry {
    (A::name_for_type(), label, icon, group)
}

/// All commands, group by group. One the view cannot do is set aside by the caller.
pub(super) fn catalog() -> Vec<Entry> {
    vec![
        c::<editor::BlockHeading1>(tr("Turn into heading 1", "Transformer en titre 1"), "c-heading.svg", Block),
        c::<editor::BlockHeading2>(tr("Turn into heading 2", "Transformer en titre 2"), "c-heading.svg", Block),
        c::<editor::BlockBullet>(tr("Turn into a bulleted list", "Transformer en liste à puces"), "list-bullet.svg", Block),
        c::<editor::BlockTask>(tr("Turn into a task list", "Transformer en liste de tâches"), "list-task.svg", Block),
        c::<editor::BlockUp>(tr("Move the block up", "Monter le bloc"), "c-up.svg", Block),
        c::<editor::BlockDown>(tr("Move the block down", "Descendre le bloc"), "c-down.svg", Block),
        c::<editor::BlockDuplicate>(tr("Duplicate the block", "Dupliquer le bloc"), "copy.svg", Block),
        c::<editor::BlockDelete>(tr("Delete the block", "Supprimer le bloc"), "trash.svg", Block),
        c::<editor::BlockText>(tr("Turn into text", "Transformer en texte"), "c-text.svg", Block),
        c::<editor::BlockHeading3>(tr("Turn into heading 3", "Transformer en titre 3"), "c-heading.svg", Block),
        c::<editor::BlockOrdered>(tr("Turn into a numbered list", "Transformer en liste numérotée"), "list-number.svg", Block),
        c::<editor::BlockQuote>(tr("Turn into a quote", "Transformer en citation"), "c-quote.svg", Block),
        c::<editor::BlockCode>(tr("Turn into a code block", "Transformer en bloc de code"), "code.svg", Block),
        c::<editor::TableRowAbove>(tr("Insert a row above", "Insérer une ligne au-dessus"), "c-ins-above.svg", Table),
        c::<editor::TableRowBelow>(tr("Insert a row below", "Insérer une ligne en dessous"), "c-ins-below.svg", Table),
        c::<editor::TableColLeft>(tr("Insert a column to the left", "Insérer une colonne à gauche"), "c-ins-left.svg", Table),
        c::<editor::TableColRight>(tr("Insert a column to the right", "Insérer une colonne à droite"), "c-ins-right.svg", Table),
        c::<editor::TableMoveRowUp>(tr("Move the row up", "Monter la ligne"), "c-up.svg", Table),
        c::<editor::TableMoveRowDown>(tr("Move the row down", "Descendre la ligne"), "c-down.svg", Table),
        c::<editor::TableMoveColLeft>(tr("Move the column left", "Décaler la colonne à gauche"), "c-left.svg", Table),
        c::<editor::TableMoveColRight>(tr("Move the column right", "Décaler la colonne à droite"), "c-right.svg", Table),
        c::<editor::TableDuplicateRow>(tr("Duplicate the row", "Dupliquer la ligne"), "copy.svg", Table),
        c::<editor::TableDuplicateCol>(tr("Duplicate the column", "Dupliquer la colonne"), "copy.svg", Table),
        c::<editor::TableSortAsc>(tr("Sort A to Z", "Trier de A à Z"), "c-sort-az.svg", Table),
        c::<editor::TableSortDesc>(tr("Sort Z to A", "Trier de Z à A"), "c-sort-za.svg", Table),
        c::<editor::TableTranspose>(tr("Swap rows and columns", "Échanger lignes et colonnes"), "c-transpose.svg", Table),
        c::<editor::TableAutoSum>(tr("Write a total (sum of the cells above)", "Écrire un total (somme des cellules au-dessus)"), "table.svg", Table),
        c::<editor::TableCopyResults>(tr("Copy the cells with their results, for a spreadsheet", "Copier les cellules avec leurs résultats, pour un tableur"), "copy.svg", Table),
        c::<editor::TableDeleteRow>(tr("Delete the row", "Supprimer la ligne"), "row-del.svg", Table),
        c::<editor::TableDeleteCol>(tr("Delete the column", "Supprimer la colonne"), "col-del.svg", Table),
        c::<editor::TableDelete>(tr("Delete the table", "Supprimer le tableau"), "trash.svg", Table),
        c::<sheet::SelectRow>(tr("Select the row", "Sélectionner la ligne"), "c-select-row.svg", Table),
        c::<sheet::SelectCol>(tr("Select the column", "Sélectionner la colonne"), "c-select-col.svg", Table),
        c::<sheet::Copy>(tr("Copy the cells", "Copier les cellules"), "copy.svg", Table),
        c::<sheet::Cut>(tr("Cut the cells", "Couper les cellules"), "c-cut.svg", Table),
        c::<sheet::Paste>(tr("Paste", "Coller"), "c-paste.svg", Table),
        c::<sheet::Undo>(tr("Undo", "Annuler"), "c-undo.svg", Table),
        c::<sheet::Redo>(tr("Redo", "Rétablir"), "c-redo.svg", Table),
        c::<sheet::WidenColumn>(tr("Widen the column", "Élargir la colonne"), "table.svg", Table),
        c::<sheet::NarrowColumn>(tr("Narrow the column", "Rétrécir la colonne"), "table.svg", Table),
        c::<sheet::FitColumn>(tr("Fit the column to its text", "Ajuster la colonne à son texte"), "table.svg", Table),
        c::<sheet::ResetColumnWidths>(tr("Reset the column widths", "Remettre les largeurs des colonnes"), "table.svg", Table),
        c::<sheet::PickDelimiter>(tr("Change the delimiter…", "Changer le délimiteur…"), "table.svg", Table),
        c::<sheet::PickEncoding>(tr("Change the encoding…", "Changer l'encodage…"), "table.svg", Table),
        c::<sheet::PickHeader>(tr("Header row…", "Ligne d'en-tête…"), "table.svg", Table),
        c::<sheet::PickCopyAs>(tr("Copy the table as…", "Copier le tableau en…"), "copy.svg", Table),
        c::<sheet::PickExport>(tr("Export the table…", "Exporter le tableau…"), "export.svg", Table),
        c::<canvas::SelectAll>(tr("Select everything", "Tout sélectionner"), "d-select.svg", Diagram),
        c::<canvas::Duplicate>(tr("Duplicate the selection", "Dupliquer la sélection"), "copy.svg", Diagram),
        c::<canvas::Erase>(tr("Delete the selection", "Supprimer la sélection"), "trash.svg", Diagram),
        c::<canvas::ZoomFit>(tr("Fit the whole diagram", "Voir tout le schéma"), "target.svg", Diagram),
        c::<canvas::ZoomIn>(tr("Zoom in", "Zoomer"), "c-zoom-in.svg", Diagram),
        c::<canvas::ZoomOut>(tr("Zoom out", "Dézoomer"), "c-zoom-out.svg", Diagram),
        c::<canvas::Undo>(tr("Undo", "Annuler"), "c-undo.svg", Diagram),
        c::<canvas::Redo>(tr("Redo", "Rétablir"), "c-redo.svg", Diagram),
        c::<editor::Bold>(tr("Bold", "Gras"), "c-bold.svg", Format),
        c::<editor::Italic>(tr("Italic", "Italique"), "c-italic.svg", Format),
        c::<editor::Underline>(tr("Underline", "Souligné"), "c-underline.svg", Format),
        c::<editor::Strike>(tr("Strikethrough", "Barré"), "c-strike.svg", Format),
        c::<editor::Highlight>(tr("Highlight", "Surligner"), "c-highlight.svg", Format),
        c::<editor::InlineCode>(tr("Inline code", "Code en ligne"), "code.svg", Format),
        c::<editor::InsertLink>(tr("Link", "Lien"), "link.svg", Format),
        c::<editor::Comment>(tr("Comment", "Commenter"), "c-comment.svg", Format),
        c::<editor::ToggleTask>(tr("Tick the task", "Cocher la tâche"), "check.svg", Format),
        c::<editor::Find>(tr("Find in the note", "Chercher dans la note"), "search.svg", Note),
        c::<editor::FindReplace>(tr("Replace in the note", "Remplacer dans la note"), "search.svg", Note),
        c::<Outline>(tr("Outline of the note", "Plan de la note"), "c-outline.svg", Note),
        c::<ShareDocument>(tr("Share / Export the current document", "Partager / Exporter le document courant"), "export.svg", Note),
        c::<CopyAll>(tr("Copy the whole note", "Copier toute la note"), "copy.svg", Note),
        c::<ExportPdf>(tr("Save the note as PDF…", "Enregistrer la note en PDF…"), "export.svg", Note),
        c::<nav::OpenAside>(tr("Open in the other pane", "Ouvrir dans l'autre volet"), "c-split.svg", Note),
        c::<nav::CopyLink>(tr("Copy the [[link]] of the note", "Copier le [[lien]] de la note"), "link.svg", Note),
        c::<nav::CopyPath>(tr("Copy the path of the note", "Copier le chemin de la note"), "copy.svg", Note),
        c::<nav::CopyRelative>(tr("Copy the relative path of the note", "Copier le chemin relatif de la note"), "copy.svg", Note),
        c::<nav::Reveal>(commands::reveal_label(), "export.svg", Note),
        c::<nav::MoveTo>(tr("Move the note to a folder…", "Déplacer la note vers un dossier…"), "folder-plus.svg", Note),
        c::<nav::SetIcon>(tr("Icon of the note…", "Icône de la note…"), "heart.svg", Note),
        c::<nav::ShowFolder>(tr("Show the note's folder as a list", "Afficher le dossier de la note en liste"), "list-bullet.svg", Note),
        c::<editor::Undo>(tr("Undo", "Annuler"), "c-undo.svg", Note),
        c::<editor::Redo>(tr("Redo", "Rétablir"), "c-redo.svg", Note),
        c::<NewNote>(tr("New note", "Nouvelle note"), "file-plus.svg", Create),
        c::<NewFromTemplate>(tr("New note from a template", "Nouvelle note depuis un modèle"), "file-plus.svg", Create),
        c::<NewDiagram>(tr("New diagram", "Nouveau schéma"), "d-rect.svg", Create),
        c::<nav::NewFolder>(tr("New folder", "Nouveau dossier"), "folder-plus.svg", Create),
        c::<editor::OpenEmbed>(tr("Open the embedded note (on its line)", "Ouvrir la note incorporée (sur sa ligne)"), "link.svg", Go),
        c::<Today>(tr("Today's note", "Note du jour"), "c-calendar.svg", Go),
        c::<SearchVault>(tr("Search the whole vault", "Chercher dans tout le coffre"), "find-text.svg", Go),
        c::<nav::ShowTree>(tr("Vault tree", "Arbre du coffre"), "tree.svg", Go),
        c::<nav::ShowRecent>(tr("Recent notes", "Notes récentes"), "clock.svg", Go),
        c::<nav::ShowGraph>(tr("Graph of the notes", "Graphe des notes"), "graph.svg", Go),
        c::<nav::ShowTags>("Tags", "tag.svg", Go),
        c::<nav::ShowLinks>(tr("Backlinks", "Rétroliens"), "backlink.svg", Go),
        c::<nav::ShowCalendar>(tr("Calendar", "Calendrier"), "c-calendar.svg", Go),
        c::<GoBack>(tr("Back", "Précédent"), "chevron-left.svg", Go),
        c::<GoForward>(tr("Forward", "Suivant"), "chevron-right.svg", Go),
        c::<SplitPane>(tr("Split the pane to the right", "Découper le volet à droite"), "c-split.svg", Go),
        c::<SplitDown>(tr("Split the pane below", "Découper le volet en dessous"), "c-split-down.svg", Go),
        c::<TabToLeft>(tr("Move the tab to the pane on the left", "Déplacer l'onglet vers le volet de gauche"), "c-left.svg", Go),
        c::<TabToRight>(tr("Move the tab to the pane on the right", "Déplacer l'onglet vers le volet de droite"), "c-right.svg", Go),
        c::<TabToUp>(tr("Move the tab to the pane above", "Déplacer l'onglet vers le volet du dessus"), "c-up.svg", Go),
        c::<TabToDown>(tr("Move the tab to the pane below", "Déplacer l'onglet vers le volet du dessous"), "c-down.svg", Go),
        c::<CloseOtherTabs>(tr("Close the other tabs of the pane", "Fermer les autres onglets du volet"), "close.svg", Go),
        c::<CloseTabsInPane>(tr("Close all the tabs of the pane", "Fermer tous les onglets du volet"), "close.svg", Go),
        c::<CloseAllFiles>(tr("Close all files (every pane)", "Fermer tous les fichiers (tous les volets)"), "close.svg", Go),
        c::<EvenPanes>(tr("Even out the panes", "Égaliser les volets"), "c-split.svg", Go),
        c::<MoveTool>(tr("Move the panel's view to the other dock", "Déplacer la vue du panneau dans l'autre dock"), "c-right.svg", Go),
        c::<ToolAsTab>(tr("Open the graph or the calendar as a tab", "Ouvrir le graphe ou le calendrier dans un onglet"), "c-split.svg", Go),
        c::<MaximizePane>(tr("Pane on the whole window, and back", "Volet sur toute la fenêtre, et retour"), "expand.svg", Go),
        c::<nav::ToggleFull>(tr("Panel on the whole window", "Panneau sur toute la fenêtre"), "expand.svg", Go),
        c::<SyncVault>(tr("Sync the vault", "Synchroniser le coffre"), "backup.svg", Vault),
        c::<OpenVault>(tr("Change vault", "Changer de coffre"), "tree.svg", Vault),
        c::<ChooseTheme>(tr("Theme…", "Thème…"), "theme.svg", Look),
        c::<ZoomIn>(tr("Bigger text", "Texte plus grand"), "c-zoom-in.svg", Look),
        c::<ZoomOut>(tr("Smaller text", "Texte plus petit"), "c-zoom-out.svg", Look),
        c::<ZoomReset>(tr("Default text size", "Taille du texte d'origine"), "c-zoom-reset.svg", Look),
        c::<ToggleHelp>(tr("Keyboard shortcuts", "Raccourcis clavier"), "help.svg", App),
        c::<Quit>(tr("Quit Bref", "Quitter Bref"), "c-quit.svg", App),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_names_each_action_once() {
        let mut seen = std::collections::HashSet::new();
        assert!(catalog().iter().all(|(action, ..)| seen.insert(*action)), "an action twice");
    }
}
