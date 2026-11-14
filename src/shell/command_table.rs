//! The command table: one entry per action, from which the palette's commands, the default
//! shortcuts (`keymap::defaults`) and the help's text and editable shortcuts all derive. The
//! action is a type, so an entry that names no action does not compile.

use super::*;
use commands::Group::{self, *};
use gpui::{Action, Div};
use keys::context as ctx;

/// What an action does to the window: the table registers it on the window's element.
pub(super) type Handler = fn(&mut Shell, &mut Window, &mut Context<Shell>);

/// Registers the handler of one action type on an element (one instance per action type).
type Bind = fn(Div, &Context<Shell>, Handler) -> Div;

fn bind<A: Action>(element: Div, cx: &Context<Shell>, handler: Handler) -> Div {
    element.on_action(cx.listener(move |this, _: &A, window, cx| handler(this, window, cx)))
}

/// A default key, with the context where it applies (`None`: everywhere).
#[derive(Clone, Debug)]
pub(super) struct Key {
    pub keys: String,
    pub context: Option<&'static str>,
}

/// Everything the app says about one action. The label, icon and group are what the palette
/// offers (none: not a palette command); `help` is the line the help gives it; `editable`:
/// the person may change its shortcut.
#[derive(Clone, Debug)]
pub(super) struct CommandSpec {
    pub action: &'static str,
    pub label: Option<&'static str>,
    pub icon: &'static str,
    pub group: Option<Group>,
    pub help: Option<&'static str>,
    pub editable: bool,
    /// The palette lists it even when nothing is typed.
    pub pinned: bool,
    /// Its own names, that put it before « Create » in the palette (`Command::also`).
    pub also: &'static str,
    pub keys: Vec<Key>,
    /// What the window does when the action arrives (none: a view handles it).
    handler: Option<Handler>,
    bind: Bind,
}

impl CommandSpec {
    fn label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }
    fn icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }
    fn group(mut self, group: Group) -> Self {
        self.group = Some(group);
        self
    }
    fn help(mut self, help: &'static str) -> Self {
        self.help = Some(help);
        self
    }
    fn also(mut self, words: &'static str) -> Self {
        self.also = words;
        self
    }
    fn pinned(mut self) -> Self {
        self.pinned = true;
        self
    }
    /// What the window does on this action.
    fn does(mut self, handler: Handler) -> Self {
        self.handler = Some(handler);
        self
    }
    /// Its shortcut can be changed (the help lists it).
    fn edit(mut self) -> Self {
        self.editable = true;
        self
    }
    fn key(mut self, keys: impl AsRef<str>) -> Self {
        self.keys.push(Key { keys: keys::canon(keys.as_ref()).expect("default keys unreadable"), context: None });
        self
    }
    fn key_in(mut self, keys: impl AsRef<str>, context: &'static str) -> Self {
        self.keys.push(Key { keys: keys::canon(keys.as_ref()).expect("default keys unreadable"), context: Some(context) });
        self
    }
    /// A key that exists on macOS only.
    fn mac(self, keys: &str, context: &'static str) -> Self {
        if cfg!(target_os = "macos") { self.key_in(keys, context) } else { self }
    }
}

fn s<A: Action>() -> CommandSpec {
    CommandSpec { action: A::name_for_type(), label: None, icon: "", group: None, help: None, editable: false, pinned: false, also: "", keys: Vec::new(), handler: None, bind: bind::<A> }
}

/// Every command, in the palette's order: group by group, then the actions it does not offer.
/// `word`: the key that moves by word (Alt on macOS, Ctrl elsewhere).
pub(super) fn specs() -> Vec<CommandSpec> {
    use editor::{BlockOp, TableOp};
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    let mut all = vec![
        s::<editor::BlockHeading1>()
            .label(tr("Turn into heading 1", "Transformer en titre 1")).icon("c-heading.svg").group(Block),
        s::<editor::BlockHeading2>()
            .label(tr("Turn into heading 2", "Transformer en titre 2")).icon("c-heading.svg").group(Block),
        s::<editor::BlockBullet>()
            .label(tr("Turn into a bulleted list", "Transformer en liste à puces")).icon("list-bullet.svg").group(Block),
        s::<editor::BlockTask>()
            .label(tr("Turn into a task list", "Transformer en liste de tâches")).icon("list-task.svg").group(Block),
        // A table: move the row up or down, delete it; the rest via the menu, the palette or the help.
        // Outside a table, Alt+arrow moves the cursor's block up or down; in a table, its row.
        s::<editor::BlockUp>()
            .label(tr("Move the block up", "Monter le bloc")).icon("c-up.svg").group(Block)
            .key_in("alt-up", ctx::EDITOR),
        s::<editor::BlockDown>()
            .label(tr("Move the block down", "Descendre le bloc")).icon("c-down.svg").group(Block)
            .key_in("alt-down", ctx::EDITOR),
        s::<editor::BlockDuplicate>()
            .label(tr("Duplicate the block", "Dupliquer le bloc")).icon("copy.svg").group(Block),
        s::<editor::BlockDelete>()
            .label(tr("Delete the block", "Supprimer le bloc")).icon("trash.svg").group(Block),
        s::<editor::BlockText>()
            .label(tr("Turn into text", "Transformer en texte")).icon("c-text.svg").group(Block),
        s::<editor::BlockHeading3>()
            .label(tr("Turn into heading 3", "Transformer en titre 3")).icon("c-heading.svg").group(Block),
        s::<editor::BlockOrdered>()
            .label(tr("Turn into a numbered list", "Transformer en liste numérotée")).icon("list-number.svg").group(Block),
        s::<editor::BlockQuote>()
            .label(tr("Turn into a quote", "Transformer en citation")).icon("c-quote.svg").group(Block),
        s::<editor::BlockCode>()
            .label(tr("Turn into a code block", "Transformer en bloc de code")).icon("code.svg").group(Block),
        s::<editor::TableRowAbove>()
            .label(tr("Insert a row above", "Insérer une ligne au-dessus")).icon("c-ins-above.svg").group(Table),
        s::<editor::TableRowBelow>()
            .label(tr("Insert a row below", "Insérer une ligne en dessous")).icon("c-ins-below.svg").group(Table),
        s::<editor::TableColLeft>()
            .label(tr("Insert a column to the left", "Insérer une colonne à gauche")).icon("c-ins-left.svg").group(Table),
        s::<editor::TableColRight>()
            .label(tr("Insert a column to the right", "Insérer une colonne à droite")).icon("c-ins-right.svg").group(Table),
        s::<editor::TableMoveRowUp>()
            .label(tr("Move the row up", "Monter la ligne")).icon("c-up.svg").group(Table)
            .key_in("alt-up", ctx::SHEET),
        s::<editor::TableMoveRowDown>()
            .label(tr("Move the row down", "Descendre la ligne")).icon("c-down.svg").group(Table)
            .key_in("alt-down", ctx::SHEET),
        s::<editor::TableMoveColLeft>()
            .label(tr("Move the column left", "Décaler la colonne à gauche")).icon("c-left.svg").group(Table),
        s::<editor::TableMoveColRight>()
            .label(tr("Move the column right", "Décaler la colonne à droite")).icon("c-right.svg").group(Table),
        s::<editor::TableDuplicateRow>()
            .label(tr("Duplicate the row", "Dupliquer la ligne")).icon("copy.svg").group(Table),
        s::<editor::TableDuplicateCol>()
            .label(tr("Duplicate the column", "Dupliquer la colonne")).icon("copy.svg").group(Table),
        s::<editor::TableSortAsc>()
            .label(tr("Sort A to Z", "Trier de A à Z")).icon("c-sort-az.svg").group(Table),
        s::<editor::TableSortDesc>()
            .label(tr("Sort Z to A", "Trier de Z à A")).icon("c-sort-za.svg").group(Table),
        s::<editor::TableTranspose>()
            .label(tr("Swap rows and columns", "Échanger lignes et colonnes")).icon("c-transpose.svg").group(Table),
        s::<editor::TableAutoSum>()
            .label(tr("Write a total (sum of the cells above)", "Écrire un total (somme des cellules au-dessus)")).icon("table.svg").group(Table)
            .help(tr("Write a total in the cell: the sum of the cells above, or on its left", "Écrire un total dans la cellule : la somme des cellules au-dessus, ou à sa gauche")).edit()
            .key_in("alt-=", ctx::EDITOR),
        s::<editor::TableCopyResults>()
            .label(tr("Copy the cells with their results, for a spreadsheet", "Copier les cellules avec leurs résultats, pour un tableur")).icon("copy.svg").group(Table)
            .help(tr("Copy the cells (the taken ones, or the table) with the results of their formulas, for a spreadsheet", "Copier les cellules (celles prises, ou le tableau) avec le résultat de leurs formules, pour un tableur")).edit()
            .key_in("secondary-alt-c", ctx::EDITOR),
        s::<editor::TableDeleteRow>()
            .label(tr("Delete the row", "Supprimer la ligne")).icon("row-del.svg").group(Table),
        s::<editor::TableDeleteCol>()
            .label(tr("Delete the column", "Supprimer la colonne")).icon("col-del.svg").group(Table),
        s::<editor::TableDelete>()
            .label(tr("Delete the table", "Supprimer le tableau")).icon("trash.svg").group(Table),
        // As in a spreadsheet: the whole row, the whole column of the selection.
        s::<sheet::SelectRow>()
            .label(tr("Select the row", "Sélectionner la ligne")).icon("c-select-row.svg").group(Table)
            .help(tr("Table: select the whole rows of the selection", "Tableau : sélectionner les lignes entières de la sélection")).edit()
            .key_in("shift-space", ctx::SHEET),
        s::<sheet::SelectCol>()
            .label(tr("Select the column", "Sélectionner la colonne")).icon("c-select-col.svg").group(Table)
            .help(tr("Table: select the whole columns of the selection", "Tableau : sélectionner les colonnes entières de la sélection")).edit()
            .key_in("ctrl-space", ctx::SHEET),
        s::<sheet::Copy>()
            .label(tr("Copy the cells", "Copier les cellules")).icon("copy.svg").group(Table)
            .help(tr("Table: copy as tab-separated text", "Tableau : copier en texte à tabulations")).edit()
            .key_in("secondary-c", ctx::SHEET),
        s::<sheet::Cut>()
            .label(tr("Cut the cells", "Couper les cellules")).icon("c-cut.svg").group(Table)
            .help(tr("Table: cut", "Tableau : couper")).edit()
            .key_in("secondary-x", ctx::SHEET),
        s::<sheet::Paste>()
            .label(tr("Paste", "Coller")).icon("c-paste.svg").group(Table)
            .help(tr("Table: paste cells from a spreadsheet, Markdown or CSV", "Tableau : coller des cellules d'un tableur, de Markdown ou de CSV")).edit()
            .key_in("secondary-v", ctx::SHEET),
        s::<sheet::Undo>()
            .label(tr("Undo", "Annuler")).icon("c-undo.svg").group(Table)
            .help(tr("Table: undo", "Tableau : annuler")).edit()
            .key_in("secondary-z", ctx::SHEET),
        s::<sheet::Redo>()
            .label(tr("Redo", "Rétablir")).icon("c-redo.svg").group(Table)
            .help(tr("Table: redo", "Tableau : rétablir")).edit()
            .key_in("secondary-shift-z", ctx::SHEET)
            .key_in("secondary-y", ctx::SHEET),
        s::<sheet::WidenColumn>()
            .label(tr("Widen the column", "Élargir la colonne")).icon("table.svg").group(Table)
            .help(tr("Table: widen the column", "Tableau : élargir la colonne")).edit()
            .key_in("alt-.", ctx::SHEET),
        s::<sheet::NarrowColumn>()
            .label(tr("Narrow the column", "Rétrécir la colonne")).icon("table.svg").group(Table)
            .help(tr("Table: narrow the column", "Tableau : rétrécir la colonne")).edit()
            .key_in("alt-,", ctx::SHEET),
        s::<sheet::FitColumn>()
            .label(tr("Fit the column to its text", "Ajuster la colonne à son texte")).icon("table.svg").group(Table)
            .help(tr("Table: fit the column to its text", "Tableau : ajuster la colonne à son texte")).edit(),
        s::<sheet::ResetColumnWidths>()
            .label(tr("Reset the column widths", "Remettre les largeurs des colonnes")).icon("table.svg").group(Table)
            .help(tr("Table: reset the column widths", "Tableau : remettre les largeurs des colonnes")).edit(),
        s::<sheet::PickDelimiter>()
            .does(|this, window, cx| this.choose_table(Pick::Delimiter, window, cx))
            .label(tr("Change the delimiter…", "Changer le délimiteur…")).icon("table.svg").group(Table),
        s::<sheet::PickEncoding>()
            .does(|this, window, cx| this.choose_table(Pick::Encoding, window, cx))
            .label(tr("Change the encoding…", "Changer l'encodage…")).icon("table.svg").group(Table),
        s::<sheet::PickHeader>()
            .does(|this, window, cx| this.choose_table(Pick::Header, window, cx))
            .label(tr("Header row…", "Ligne d'en-tête…")).icon("table.svg").group(Table),
        s::<sheet::PickCopyAs>()
            .does(|this, window, cx| this.choose_table(Pick::CopyAs, window, cx))
            .label(tr("Copy the table as…", "Copier le tableau en…")).icon("copy.svg").group(Table),
        s::<sheet::PickExport>()
            .does(|this, window, cx| this.choose_table(Pick::Export, window, cx))
            .label(tr("Export the table…", "Exporter le tableau…")).icon("export.svg").group(Table),
        s::<canvas::SelectAll>()
            .label(tr("Select everything", "Tout sélectionner")).icon("d-select.svg").group(Diagram)
            .key_in("secondary-a", ctx::CANVAS),
        s::<canvas::Duplicate>()
            .label(tr("Duplicate the selection", "Dupliquer la sélection")).icon("copy.svg").group(Diagram)
            .help(tr("Diagram: duplicate the selection", "Schéma : dupliquer la sélection")).edit()
            .key_in("secondary-d", ctx::CANVAS),
        s::<canvas::Erase>()
            .label(tr("Delete the selection", "Supprimer la sélection")).icon("trash.svg").group(Diagram)
            .key_in("backspace", ctx::CANVAS),
        s::<canvas::ZoomFit>()
            .label(tr("Fit the whole diagram", "Voir tout le schéma")).icon("target.svg").group(Diagram)
            .key_in("secondary-0", ctx::CANVAS),
        s::<canvas::ZoomIn>()
            .label(tr("Zoom in", "Zoomer")).icon("c-zoom-in.svg").group(Diagram)
            .key_in("secondary-=", ctx::CANVAS)
            .key_in("secondary-+", ctx::CANVAS),
        s::<canvas::ZoomOut>()
            .label(tr("Zoom out", "Dézoomer")).icon("c-zoom-out.svg").group(Diagram)
            .key_in("secondary--", ctx::CANVAS),
        s::<canvas::Undo>()
            .label(tr("Undo", "Annuler")).icon("c-undo.svg").group(Diagram)
            .help(tr("Diagram: undo", "Schéma : annuler")).edit()
            .key_in("secondary-z", ctx::CANVAS),
        s::<canvas::Redo>()
            .label(tr("Redo", "Rétablir")).icon("c-redo.svg").group(Diagram)
            .help(tr("Diagram: redo", "Schéma : rétablir")).edit()
            .key_in("secondary-shift-z", ctx::CANVAS),
        s::<editor::Bold>()
            .label(tr("Bold", "Gras")).icon("c-bold.svg").group(Format)
            .help(tr("Bold", "Gras")).edit()
            .key_in("secondary-b", ctx::EDITOR),
        s::<editor::Italic>()
            .label(tr("Italic", "Italique")).icon("c-italic.svg").group(Format)
            .help(tr("Italic", "Italique")).edit()
            .key_in("secondary-i", ctx::EDITOR),
        s::<editor::Underline>()
            .label(tr("Underline", "Souligné")).icon("c-underline.svg").group(Format)
            .help(tr("Underline", "Souligné")).edit()
            .key_in("secondary-u", ctx::EDITOR),
        s::<editor::Strike>()
            .label(tr("Strikethrough", "Barré")).icon("c-strike.svg").group(Format)
            .help(tr("Strikethrough", "Barré")).edit()
            .key_in("secondary-shift-x", ctx::EDITOR),
        s::<editor::Highlight>()
            .label(tr("Highlight", "Surligner")).icon("c-highlight.svg").group(Format)
            .help(tr("Highlight", "Surligné")).edit()
            .key_in("secondary-shift-h", ctx::EDITOR),
        s::<editor::InlineCode>()
            .label(tr("Inline code", "Code en ligne")).icon("code.svg").group(Format)
            .help(tr("Inline code", "Code en ligne")).edit()
            .key_in("secondary-shift-k", ctx::EDITOR),
        s::<editor::InsertLink>()
            .label(tr("Link", "Lien")).icon("link.svg").group(Format)
            .help(tr("Link: [text](address) from the selection; on a link, change its address", "Lien : [texte](adresse) depuis la sélection ; sur un lien, changer son adresse")).edit()
            .key_in("secondary-k", ctx::EDITOR),
        s::<editor::Comment>()
            .label(tr("Comment", "Commenter")).icon("c-comment.svg").group(Format)
            .help(tr("Comment the selection or the line (in a table: the cell, or the taken row, column or table); on a comment, resolve it", "Commenter la sélection ou la ligne (dans un tableau : la cellule, ou la ligne, la colonne, le tableau pris) ; sur un commentaire, le résoudre")).edit()
            .key_in("secondary-shift-m", ctx::EDITOR),
        s::<editor::ToggleTask>()
            .label(tr("Tick the task", "Cocher la tâche")).icon("check.svg").group(Format)
            .help(tr("Check / uncheck a task or a [ ] box; in a table cell, add one", "Cocher / décocher une tâche ou une case [ ] ; dans une cellule, en ajouter une")).edit()
            .key_in("secondary-enter", ctx::EDITOR),
        // Search in the note: the bar has its own context as long as it receives input.
        s::<editor::Find>()
            .label(tr("Find in the note", "Chercher dans la note")).icon("search.svg").group(Note)
            .help(tr("Find in the note", "Chercher dans la note")).edit()
            .key_in("secondary-f", ctx::EDITOR)
            .key_in("secondary-f", ctx::FIND),
        s::<editor::FindReplace>()
            .label(tr("Replace in the note", "Remplacer dans la note")).icon("search.svg").group(Note)
            .help(tr("Find and replace in the note", "Chercher et remplacer dans la note")).edit()
            .key_in("secondary-r", ctx::EDITOR)
            .key_in("secondary-r", ctx::FIND),
        s::<ShowComments>()
            .does(|this, window, cx| this.open_comments(window, cx))
            .label(tr("Comments of the note", "Commentaires de la note")).icon("c-comment.svg").group(Note),
        s::<Outline>()
            .does(|this, window, cx| this.open_outline(window, cx))
            .label(tr("Outline of the note", "Plan de la note")).icon("c-outline.svg").group(Note)
            .help(tr("Outline: jump to a heading of the note", "Plan : aller à un titre de la note")).edit()
            .key("secondary-shift-o"),
        s::<ShareDocument>()
            .does(|this, window, cx| this.show_share(window, cx))
            .label(tr("Share / Export the current document", "Partager / Exporter le document courant")).icon("export.svg").group(Note)
            .help(tr("Share / Export: copy the whole note as Markdown or plain text, save a copy; SVG/PNG for diagrams, table formats", "Partager / Exporter : copier toute la note en Markdown ou texte brut, enregistrer une copie ; SVG/PNG pour les schémas, formats de tableaux")).edit()
            .key("secondary-alt-e"),
        s::<CopyAll>()
            .does(|this, _, cx| this.copy_all(true, cx))
            .label(tr("Copy the whole note", "Copier toute la note")).icon("copy.svg").group(Note)
            .help(tr("Copy the code block, else the note", "Copier le bloc de code, sinon la note")).edit()
            .key("secondary-shift-c"),
        s::<ExportPdf>()
            .does(|this, _, cx| this.export_pdf(cx))
            .label(tr("Save the note as PDF…", "Enregistrer la note en PDF…")).icon("export.svg").group(Note)
            .help(tr("Save the note as PDF (A4, unsaved edits included); pictures, figures and embeds that cannot be printed are listed", "Enregistrer la note en PDF (A4, modifications non enregistrées comprises) ; les images, figures et incorporations impossibles à imprimer sont signalées")).edit(),
        // The row menu's entries, from the palette, on the file of the active pane.
        s::<nav::OpenAside>()
            .does(|this, window, cx| this.file_do(nav::Do::OpenAside, window, cx))
            .label(tr("Open in the other pane", "Ouvrir dans l'autre volet")).icon("c-split.svg").group(Note),
        s::<nav::CopyLink>()
            .does(|this, window, cx| this.file_do(nav::Do::CopyLink, window, cx))
            .label(tr("Copy the [[link]] of the note", "Copier le [[lien]] de la note")).icon("link.svg").group(Note),
        s::<nav::CopyPath>()
            .does(|this, window, cx| this.file_do(nav::Do::CopyPath, window, cx))
            .label(tr("Copy the path of the note", "Copier le chemin de la note")).icon("copy.svg").group(Note),
        s::<nav::CopyRelative>()
            .does(|this, window, cx| this.file_do(nav::Do::CopyRelative, window, cx))
            .label(tr("Copy the relative path of the note", "Copier le chemin relatif de la note")).icon("copy.svg").group(Note),
        s::<nav::Reveal>()
            .does(|this, window, cx| this.file_do(nav::Do::Reveal, window, cx))
            .label(commands::reveal_label()).icon("export.svg").group(Note),
        s::<nav::MoveTo>()
            .does(|this, window, cx| this.file_do(nav::Do::MoveTo, window, cx))
            .label(tr("Move the note to a folder…", "Déplacer la note vers un dossier…")).icon("folder-plus.svg").group(Note),
        s::<nav::SetIcon>()
            .does(|this, window, cx| this.file_do(nav::Do::Icon, window, cx))
            .label(tr("Icon of the note…", "Icône de la note…")).icon("heart.svg").group(Note),
        s::<nav::ShowFolder>()
            .does(|this, window, cx| this.file_do(nav::Do::List, window, cx))
            .label(tr("Show the note's folder as a list", "Afficher le dossier de la note en liste")).icon("list-bullet.svg").group(Note),
        // A gesture made on the kanban board is undone from the board.
        s::<editor::Undo>()
            .label(tr("Undo", "Annuler")).icon("c-undo.svg").group(Note)
            .help(tr("Undo", "Annuler")).edit()
            .key_in("secondary-z", ctx::EDITOR)
            .key_in("secondary-z", ctx::BOARD),
        s::<editor::Redo>()
            .label(tr("Redo", "Rétablir")).icon("c-redo.svg").group(Note)
            .help(tr("Redo", "Rétablir")).edit()
            .key_in("secondary-shift-z", ctx::EDITOR)
            .key_in("secondary-y", ctx::EDITOR)
            .key_in("secondary-shift-z", ctx::BOARD)
            .key_in("secondary-y", ctx::BOARD),
        s::<NewNote>()
            .does(|this, window, cx| this.new_note_here(window, cx))
            .label(tr("New note", "Nouvelle note")).icon("file-plus.svg").group(Create)
            .help(tr("New note", "Nouvelle note")).edit()
            .key("secondary-n"),
        s::<NewFromTemplate>()
            .does(|this, window, cx| this.open_templates(TemplateAct::New, window, cx))
            .label(tr("New note from a template", "Nouvelle note depuis un modèle")).icon("file-plus.svg").group(Create)
            .help(tr("New note from a template (a note of the Templates folder: {{title}} {{date}} {{time}} {{cursor}})", "Nouvelle note depuis un modèle (une note du dossier Templates ou Modèles : {{title}} {{date}} {{time}} {{cursor}})")).edit(),
        s::<NewDiagram>()
            .does(|this, window, cx| this.new_diagram(window, cx))
            .label(tr("New diagram", "Nouveau schéma")).icon("d-rect.svg").group(Create)
            .help(tr("New diagram: an SVG file in the vault", "Nouveau schéma : un fichier SVG du coffre")).edit()
            .key_in("secondary-shift-d", ctx::SHELL),
        s::<ImportDiagram>()
            .does(|this, window, cx| this.import_diagram(window, cx))
            .label(tr("Import a diagram (Excalidraw, draw.io)…", "Importer un schéma (Excalidraw, draw.io)…")).icon("diagram.svg").group(Create),
        s::<NewBoard>()
            .does(|this, window, cx| {
                this.fresh_tab(window, cx);
                this.new_note(kanban::new_board(), cx);
            })
            .label(tr("New kanban board", "Nouveau tableau kanban")).icon("board.svg").group(Create),
        s::<nav::NewFolder>()
            .does(|this, window, cx| this.new_folder(window, cx))
            .label(tr("New folder", "Nouveau dossier")).icon("folder-plus.svg").group(Create)
            .help(tr("New folder (also in the palette)", "Nouveau dossier (aussi dans la palette)")).edit()
            .key_in("secondary-shift-n", ctx::SHELL),
        s::<editor::OpenEmbed>()
            .label(tr("Open the embedded note (on its line)", "Ouvrir la note incorporée (sur sa ligne)")).icon("link.svg").group(Go)
            .help(tr("On the line of an embed: open its note, at the section", "Sur la ligne d'une incorporation : ouvrir sa note, à la section")).edit()
            .key_in("alt-enter", ctx::EDITOR),
        s::<Today>()
            .does(|this, window, cx| this.open_today(window, cx))
            .label(tr("Today's note", "Note du jour")).icon("c-calendar.svg").group(Go)
            .help(tr("Today's note: open it, or create it", "Note du jour : l'ouvrir, ou la créer")).edit()
            .key("secondary-j"),
        s::<SearchVault>()
            .does(|this, window, cx| this.open_vault_find(window, cx))
            .label(tr("Search the whole vault", "Chercher dans tout le coffre")).icon("find-text.svg").group(Go)
            .help(tr("Search the text of the whole vault, in the panel: matches note by note, replace", "Chercher dans le texte de tout le coffre, dans le panneau : passages note par note, remplacer")).edit()
            .key("secondary-shift-f"),
        s::<nav::ShowTree>()
            .does(|this, window, cx| this.show_nav(Mode::Tree, false, window, cx))
            .label(tr("Vault tree", "Arbre du coffre")).icon("tree.svg").group(Go)
            .help(tr("Vault tree", "Arbre du coffre")).edit()
            .key_in("secondary-e", ctx::SHELL),
        s::<nav::ShowRecent>()
            .does(|this, window, cx| this.show_nav(Mode::Recent, false, window, cx))
            .label(tr("Recent notes", "Notes récentes")).icon("clock.svg").group(Go)
            .help(tr("Recent notes", "Notes récentes")).edit()
            .key_in("secondary-shift-t", ctx::SHELL),
        s::<nav::ShowGraph>()
            .does(|this, window, cx| this.show_nav(Mode::Graph, false, window, cx))
            .label(tr("Graph of the notes", "Graphe des notes")).icon("graph.svg").group(Go)
            .help(tr("Graph of the notes", "Graphe des notes")).edit()
            .key_in("secondary-g", ctx::SHELL),
        s::<nav::ShowTags>()
            .does(|this, window, cx| this.show_nav(Mode::Tags, false, window, cx))
            .label("Tags").icon("tag.svg").group(Go)
            .help("Tags").edit()
            .key_in("secondary-t", ctx::SHELL),
        s::<nav::ShowLinks>()
            .does(|this, window, cx| this.show_nav(Mode::Links, false, window, cx))
            .label(tr("Backlinks", "Rétroliens")).icon("backlink.svg").group(Go)
            .help(tr("Backlinks: the notes that link to this one", "Rétroliens : les notes qui mènent à celle-ci")).edit()
            .key_in("secondary-l", ctx::SHELL),
        s::<nav::ShowCalendar>()
            .does(|this, window, cx| this.show_nav(Mode::Calendar, false, window, cx))
            .label(tr("Calendar", "Calendrier")).icon("c-calendar.svg").group(Go)
            .help(tr("Calendar: arrows, Page Up/Down for the month, Enter opens the day (a dot marks the days with a note)", "Calendrier : flèches, Page haut/bas pour le mois, Entrée ouvre le jour (un point marque les jours qui ont une note)")).edit()
            .key_in("secondary-shift-j", ctx::SHELL),
        // On macOS, Alt+arrow traverses words: brackets, as in a browser.
        s::<GoBack>()
            .does(|this, window, cx| this.travel(false, window, cx))
            .label(tr("Back", "Précédent")).icon("chevron-left.svg").group(Go)
            .help(tr("Back: what was shown before, at the same place (also the mouse button, and the rail)", "Précédent : ce qui était affiché avant, au même endroit (aussi le bouton de la souris, et le rail)")).edit()
            .key(if cfg!(target_os = "macos") { "cmd-[" } else { "alt-left" }),
        s::<GoForward>()
            .does(|this, window, cx| this.travel(true, window, cx))
            .label(tr("Forward", "Suivant")).icon("chevron-right.svg").group(Go)
            .help(tr("Forward: what was shown after going back", "Suivant : ce qui était affiché après un retour")).edit()
            .key(if cfg!(target_os = "macos") { "cmd-]" } else { "alt-right" }),
        s::<SplitPane>()
            .does(|this, window, cx| this.split(Side::Right, window, cx))
            .label(tr("Split the pane to the right", "Découper le volet à droite")).icon("c-split.svg").group(Go)
            .help(tr("Split the pane: a new one on its right, opened on the palette to choose what goes there", "Découper le volet : un nouveau à sa droite, ouvert sur la palette pour choisir ce qui y va")).edit()
            .key("secondary-\\"),
        s::<SplitDown>()
            .does(|this, window, cx| this.split(Side::Down, window, cx))
            .label(tr("Split the pane below", "Découper le volet en dessous")).icon("c-split-down.svg").group(Go)
            .help(tr("Split the pane: a new one below it", "Découper le volet : un nouveau en dessous")).edit()
            .key("secondary-alt-\\"),
        s::<TabToLeft>()
            .does(|this, window, cx| this.tab_to_side(Side::Left, window, cx))
            .label(tr("Move the tab to the pane on the left", "Déplacer l'onglet vers le volet de gauche")).icon("c-left.svg").group(Go)
            .help(tr("Move the tab to the pane on the left (split off if there is none)", "Déplacer l'onglet vers le volet de gauche (découpé s'il n'y en a pas)")).edit(),
        s::<TabToRight>()
            .does(|this, window, cx| this.tab_to_side(Side::Right, window, cx))
            .label(tr("Move the tab to the pane on the right", "Déplacer l'onglet vers le volet de droite")).icon("c-right.svg").group(Go)
            .help(tr("Move the tab to the pane on the right (split off if there is none)", "Déplacer l'onglet vers le volet de droite (découpé s'il n'y en a pas)")).edit(),
        s::<TabToUp>()
            .does(|this, window, cx| this.tab_to_side(Side::Up, window, cx))
            .label(tr("Move the tab to the pane above", "Déplacer l'onglet vers le volet du dessus")).icon("c-up.svg").group(Go)
            .help(tr("Move the tab to the pane above (split off if there is none)", "Déplacer l'onglet vers le volet du dessus (découpé s'il n'y en a pas)")).edit(),
        s::<TabToDown>()
            .does(|this, window, cx| this.tab_to_side(Side::Down, window, cx))
            .label(tr("Move the tab to the pane below", "Déplacer l'onglet vers le volet du dessous")).icon("c-down.svg").group(Go)
            .help(tr("Move the tab to the pane below (split off if there is none)", "Déplacer l'onglet vers le volet du dessous (découpé s'il n'y en a pas)")).edit(),
        s::<CloseOtherTabs>()
            .does(|this, window, cx| this.close_other_tabs(this.active, window, cx))
            .label(tr("Close the other tabs of the pane", "Fermer les autres onglets du volet")).icon("close.svg").group(Go)
            .help(tr("Close the other tabs of the pane, each saved first; one that fails to save stays open", "Fermer les autres onglets du volet, chacun enregistré d'abord ; celui qui échoue reste ouvert")).edit(),
        s::<CloseTabsInPane>()
            .does(|this, window, cx| this.close_tabs_in_pane(this.group(), window, cx))
            .label(tr("Close all the tabs of the pane", "Fermer tous les onglets du volet")).icon("close.svg").group(Go)
            .help(tr("Close all the tabs of the pane; the other panes keep theirs", "Fermer tous les onglets du volet ; les autres volets gardent les leurs")).edit(),
        s::<CloseAllFiles>()
            .does(|this, window, cx| this.close_all_files(window, cx))
            .label(tr("Close all files (every pane)", "Fermer tous les fichiers (tous les volets)")).icon("close.svg").group(Go)
            .help(tr("Close all files, in every pane, without deleting any; the window is left on a new empty note", "Fermer tous les fichiers, dans tous les volets, sans en supprimer aucun ; la fenêtre reste sur une note vide")).edit(),
        s::<EvenPanes>()
            .does(|this, _, cx| {
                this.layout.even();
                cx.notify();
            })
            .label(tr("Even out the panes", "Égaliser les volets")).icon("c-split.svg").group(Go)
            .help(tr("Give every pane the same room", "Donner la même place à chaque volet")).edit(),
        s::<MoveTool>()
            .does(|this, window, cx| this.move_tool(window, cx))
            .label(tr("Move the panel's view to the other dock", "Déplacer la vue du panneau dans l'autre dock")).icon("c-right.svg").group(Go)
            .help(tr("Move the panel's view to the other dock (left or right of the panes); the rail shows or hides it where it lives", "Déplacer la vue du panneau dans l'autre dock (à gauche ou à droite des volets) ; le rail la montre ou la cache là où elle est")).edit(),
        s::<ToolAsTab>()
            .does(|this, window, cx| this.tool_as_tab(window, cx))
            .label(tr("Open the graph or the calendar as a tab", "Ouvrir le graphe ou le calendrier dans un onglet")).icon("c-split.svg").group(Go)
            .help(tr("Open the graph or the calendar as a tab; closing the tab gives it back to its dock", "Ouvrir le graphe ou le calendrier dans un onglet ; fermer l'onglet le rend à son dock")).edit(),
        s::<MaximizePane>()
            .does(|this, _, cx| this.toggle_zoom(cx))
            .label(tr("Pane on the whole window, and back", "Volet sur toute la fenêtre, et retour")).icon("expand.svg").group(Go)
            .help(tr("The pane on the whole window, and back (the others keep what they hold)", "Le volet sur toute la fenêtre, et retour (les autres gardent ce qu'ils ont)")).edit()
            .key("alt-shift-enter"),
        s::<nav::ToggleFull>()
            .does(|this, window, cx| this.toggle_full(window, cx))
            .label(tr("Panel on the whole window", "Panneau sur toute la fenêtre")).icon("expand.svg").group(Go)
            .help(tr("Panel on the whole window", "Panneau en pleine fenêtre")).edit()
            .key_in("secondary-m", ctx::SHELL),
        s::<SyncVault>()
            .does(|this, window, cx| this.git_sync(window, cx))
            .label(tr("Sync the vault (git)", "Synchroniser le coffre (git)")).icon("backup.svg").group(Vault)
            .help(tr("Sync the vault with git: commit, receive, merge, send; a conflict keeps both versions as two notes", "Synchroniser le coffre par git : commiter, recevoir, fusionner, envoyer ; un conflit garde les deux versions en deux notes")).edit()
            .key("secondary-shift-s"),
        s::<OpenVault>()
            .does(|this, window, cx| this.choose_vault(window, cx))
            .label(tr("Change vault…", "Changer de coffre…")).icon("tree.svg").group(Vault).pinned()
            .help(tr("Change vault", "Changer de coffre")).edit()
            .key("secondary-o"),
        s::<RestoreTrash>()
            .does(|this, window, cx| this.open_trash(window, cx))
            .label(tr("Restore from the trash…", "Restaurer depuis la corbeille…")).icon("trash.svg").group(Vault),
        s::<VaultHealth>()
            .does(|this, window, cx| this.open_health(window, cx))
            .label(tr("Vault health: broken links, orphans, leftovers", "Santé du coffre : liens cassés, orphelins, restes")).icon("heart.svg").group(Vault)
            .also("health santé"),
        s::<BackupVault>()
            .does(|this, window, cx| this.choose_backup(window, cx))
            .label(tr("Back up the vault…", "Sauvegarder le coffre…")).icon("backup.svg").group(Vault),
        s::<CloneVault>()
            .does(|this, window, cx| this.ask_clone(window, cx))
            .label(tr("Clone a vault from a git address…", "Cloner un coffre depuis une adresse git…")).icon("folder-plus.svg").group(Vault),
        s::<OpenTerminal>()
            .does(|this, _, cx| this.open_terminal(cx))
            .label(tr("Open a terminal in the vault", "Ouvrir un terminal dans le coffre")).icon("code.svg").group(Vault),
        s::<SetUpCapture>()
            .does(|this, _, cx| this.capture_shortcut(cx))
            .label(tr("Set up the quick capture shortcut", "Créer le raccourci de capture rapide")).icon("plus.svg").group(Vault),
        s::<OpenSettings>()
            .does(|this, window, cx| this.open_settings(None, window, cx))
            .label(tr("Settings", "Réglages")).icon("gear.svg").group(App).pinned()
            .help(tr("Settings: every choice of the app, by section, in a tab", "Réglages : tous les choix de l'app, par section, dans un onglet")).edit()
            .key("secondary-,"),
        s::<ChooseTheme>()
            .does(|this, window, cx| {
                this.choose_setting(Setting::Theme, window, cx)
            })
            .label(tr("Theme…", "Thème…")).icon("theme.svg").group(Look),
        s::<ZoomIn>()
            .does(|this, window, cx| this.resize_text(Some(1.), window, cx))
            .label(tr("Bigger text", "Texte plus grand")).icon("c-zoom-in.svg").group(Look)
            .help(tr("Bigger text; the panel, menus and help follow, up to 150 %", "Texte plus grand ; le panneau, les menus et l'aide suivent, jusqu'à 150 %")).edit()
            .key("secondary-=")
            .key("secondary-+"),
        s::<ZoomOut>()
            .does(|this, window, cx| this.resize_text(Some(-1.), window, cx))
            .label(tr("Smaller text", "Texte plus petit")).icon("c-zoom-out.svg").group(Look)
            .help(tr("Smaller text; the interface follows, down to 85 %", "Texte plus petit ; l'interface suit, jusqu'à 85 %")).edit()
            .key("secondary--"),
        s::<ZoomReset>()
            .does(|this, window, cx| this.resize_text(None, window, cx))
            .label(tr("Default text size", "Taille du texte d'origine")).icon("c-zoom-reset.svg").group(Look)
            .help(tr("Default text size", "Taille du texte d'origine")).edit()
            .key("secondary-0"),
        s::<ToggleHelp>()
            .does(|this, window, cx| {
                this.set_help(!this.help.open, window, cx)
            })
            .label(tr("Keyboard shortcuts, about", "Raccourcis clavier, à propos")).icon("help.svg").group(App).pinned()
            .help(tr("This help", "Cette aide")).edit()
            .key("f1")
            .key("secondary-/"),
        s::<Quit>()
            .does(|this, window, cx| {
                cx.stop_propagation();
                if this.may_close_window(window, cx) {
                    cx.quit();
                }
            })
            .label(tr("Quit Bref", "Quitter Bref")).icon("c-quit.svg").group(App)
            .help(tr("Save and quit; stay open if saving fails", "Enregistrer et quitter ; rester ouvert si l’enregistrement échoue")).edit()
            .key("secondary-q"),
        s::<OpenPalette>()
            .does(|this, window, cx| {
                this.show_palette("", window, cx)
            })
            .help(tr("Find by name or text, create, filter by #tag", "Chercher par nom ou texte, créer, filtrer par #tag")).edit()
            .key("secondary-p"),
        s::<CloseHelp>()
            .does(|this, window, cx| {
                if this.help.open {
                    this.set_help(false, window, cx)
                } else if this.pane().zone.is_focused(window) && this.has_note(cx) {
                    // A pane that only shows an image: Escape goes back to its note.
                    window.focus(&this.pane().editor.focus_handle(cx));
                }
            })
            .key_in("escape", ctx::SHELL),
        // Ctrl+1 and Ctrl+2: the first and second panes, in reading order.
        s::<FocusLeft>()
            .does(|this, window, cx| {
                let first = this.layout.panes()[0];
                this.focus_group(first, window, cx)
            })
            .help(tr("Go to the first pane", "Aller au premier volet")).edit()
            .key("secondary-1"),
        s::<FocusRight>()
            .does(|this, window, cx| {
                if let Some(&second) = this.layout.panes().get(1) {
                    this.zoomed = None;
                    this.focus_group(second, window, cx)
                }
            })
            .help(tr("Go to the second pane", "Aller au deuxième volet")).edit()
            .key("secondary-2"),
        // Ctrl+Alt+arrows belong to the desktop (workspaces) on Linux: Alt+Shift there.
        s::<PaneLeft>()
            .does(|this, window, cx| this.focus_side(Side::Left, window, cx))
            .help(tr("Go to the pane on the left", "Aller au volet de gauche")).edit()
            .key(if cfg!(target_os = "macos") { "cmd-alt-left" } else { "alt-shift-left" }),
        s::<PaneRight>()
            .does(|this, window, cx| this.focus_side(Side::Right, window, cx))
            .help(tr("Go to the pane on the right", "Aller au volet de droite")).edit()
            .key(if cfg!(target_os = "macos") { "cmd-alt-right" } else { "alt-shift-right" }),
        s::<PaneUp>()
            .does(|this, window, cx| this.focus_side(Side::Up, window, cx))
            .help(tr("Go to the pane above", "Aller au volet du dessus")).edit()
            .key(if cfg!(target_os = "macos") { "cmd-alt-up" } else { "alt-shift-up" }),
        s::<PaneDown>()
            .does(|this, window, cx| this.focus_side(Side::Down, window, cx))
            .help(tr("Go to the pane below", "Aller au volet du dessous")).edit()
            .key(if cfg!(target_os = "macos") { "cmd-alt-down" } else { "alt-shift-down" }),
        s::<ClosePane>()
            .does(|this, window, cx| { this.close_pane(window, cx); })
            .help(tr("Save and close the focused tab; keep it open if saving fails", "Enregistrer et fermer l'onglet actif ; le garder ouvert si l'enregistrement échoue")).edit()
            .key("secondary-w"),
        s::<ReopenTab>()
            .does(|this, window, cx| this.reopen_tab(window, cx))
            .help(tr("Reopen the last closed tab", "Rouvrir le dernier onglet fermé")).edit()
            .key("secondary-alt-t"),
        s::<NextTab>()
            .does(|this, window, cx| this.cycle_tab(true, window, cx))
            .help(tr("Next tab", "Onglet suivant")).edit()
            .key("ctrl-tab"),
        s::<PrevTab>()
            .does(|this, window, cx| this.cycle_tab(false, window, cx))
            .help(tr("Previous tab", "Onglet précédent")).edit()
            .key("ctrl-shift-tab"),
        s::<MoveTabLeft>()
            .does(|this, _, cx| this.move_tab(false, cx))
            .help(tr("Move the tab one place to the left", "Déplacer l'onglet d'un cran à gauche")).edit()
            .key("ctrl-shift-pageup"),
        s::<MoveTabRight>()
            .does(|this, _, cx| this.move_tab(true, cx))
            .help(tr("Move the tab one place to the right", "Déplacer l'onglet d'un cran à droite")).edit()
            .key("ctrl-shift-pagedown"),
        // The command palette: F10, like the menu bar of a desktop app.
        s::<ToggleCommands>()
            .does(|this, window, cx| this.open_commands(window, cx))
            .help(tr("Command palette: every command, found by what it does (also the first button of the rail, or > in the palette)", "Palette des commandes : toutes les commandes, trouvées par ce qu'elles font (aussi le premier bouton du rail, ou > dans la palette)")).edit()
            .key("f10")
            .key("secondary-shift-p"),
        // From button to button, where Tab has no other use (the note indents with it). In the window's own
        // context: a binding with none counts as the deepest and would take Tab from the views that use it.
        s::<FocusNext>()
            .does(|_, window, _| window.focus_next())
            .help(tr("From the panel: go from button to button (Shift+Tab: back); Enter or Space presses it", "Depuis le panneau : aller de bouton en bouton (Maj+Tab : revenir) ; Entrée ou Espace l'actionne")).edit()
            .key_in("tab", ctx::SHELL),
        s::<ui::widgets::Press>()
            .key_in("enter", ctx::BUTTON),
        s::<FocusPrev>()
            .does(|_, window, _| window.focus_prev())
            .help(tr("Back to the previous button", "Revenir au bouton précédent")).edit()
            .key_in("shift-tab", ctx::SHELL),
        // The context menu of the selected row or of the cursor, as on every desktop.
        s::<ContextMenu>()
            .does(|this, window, cx| this.menu_from_keys(window, cx))
            .help(tr("The same menu from the keys, for the selected row or the cursor: arrows, Enter, Esc", "Le même menu au clavier, pour la ligne choisie ou le curseur : flèches, Entrée, Échap")).edit()
            .key("shift-f10")
            .key("menu"),
        s::<canvas::EraseNext>()
            .key_in("delete", ctx::CANVAS),
        s::<canvas::Left>()
            .key_in("left", ctx::CANVAS),
        s::<canvas::Right>()
            .key_in("right", ctx::CANVAS),
        s::<canvas::Up>()
            .key_in("up", ctx::CANVAS),
        s::<canvas::Down>()
            .key_in("down", ctx::CANVAS),
        s::<canvas::SelectUp>()
            .help(tr("Diagram, while writing: select up to the line above", "Schéma, en écrivant : sélectionner jusqu'à la ligne du dessus")).edit()
            .key_in("shift-up", ctx::CANVAS),
        s::<canvas::SelectDown>()
            .help(tr("Diagram, while writing: select down to the line below", "Schéma, en écrivant : sélectionner jusqu'à la ligne du dessous")).edit()
            .key_in("shift-down", ctx::CANVAS),
        s::<canvas::Paste>()
            .key_in("secondary-v", ctx::CANVAS),
        s::<canvas::NextText>()
            .help(tr("Arrow: the text at its start, in its middle, at its end (a role, a multiplicity); drag one along the stroke", "Flèche : le texte à son début, au milieu, à sa fin (un rôle, une multiplicité) ; en faire glisser un le long du trait")).edit()
            .key_in("tab", ctx::CANVAS),
        s::<canvas::Confirm>()
            .key_in("enter", ctx::CANVAS),
        // Escape in a diagram where nothing is in progress anymore: back to the note, if its
        // tab has one under it.
        s::<canvas::Cancel>()
            .does(|this, window, cx| {
                if this.has_note(cx) {
                    window.focus(&this.pane().editor.focus_handle(cx));
                    cx.notify();
                }
            })
            .key_in("escape", ctx::CANVAS),
        // Table (CSV, TSV): arrows, selection with Shift, input, clipboard, undo.
        s::<sheet::Left>()
            .key_in("left", ctx::SHEET),
        s::<sheet::Right>()
            .key_in("right", ctx::SHEET),
        s::<sheet::Up>()
            .key_in("up", ctx::SHEET),
        s::<sheet::Down>()
            .key_in("down", ctx::SHEET),
        s::<sheet::ExtendLeft>()
            .key_in("shift-left", ctx::SHEET),
        s::<sheet::ExtendRight>()
            .key_in("shift-right", ctx::SHEET),
        s::<sheet::ExtendUp>()
            .key_in("shift-up", ctx::SHEET),
        s::<sheet::ExtendDown>()
            .key_in("shift-down", ctx::SHEET),
        s::<sheet::PageUp>()
            .key_in("pageup", ctx::SHEET),
        s::<sheet::PageDown>()
            .key_in("pagedown", ctx::SHEET),
        s::<sheet::RowStart>()
            .key_in("home", ctx::SHEET),
        s::<sheet::RowEnd>()
            .key_in("end", ctx::SHEET),
        s::<sheet::Origin>()
            .key_in("secondary-home", ctx::SHEET),
        s::<sheet::Corner>()
            .key_in("secondary-end", ctx::SHEET),
        s::<sheet::FirstRow>()
            .key_in("secondary-up", ctx::SHEET),
        s::<sheet::LastRow>()
            .key_in("secondary-down", ctx::SHEET),
        s::<sheet::Next>()
            .key_in("tab", ctx::SHEET),
        s::<sheet::Previous>()
            .key_in("shift-tab", ctx::SHEET),
        s::<sheet::Enter>()
            .key_in("enter", ctx::SHEET),
        s::<sheet::Edit>()
            .key_in("f2", ctx::SHEET),
        s::<sheet::Cancel>()
            .key_in("escape", ctx::SHEET),
        s::<sheet::Backspace>()
            .key_in("backspace", ctx::SHEET),
        s::<sheet::Delete>()
            .key_in("delete", ctx::SHEET),
        s::<sheet::SelectAll>()
            .key_in("secondary-a", ctx::SHEET),
        s::<sheet::InsertBelow>()
            .help(tr("Table: insert a row below (a column to the right when whole columns are selected)", "Tableau : insérer une ligne dessous (une colonne à droite si des colonnes entières sont sélectionnées)")).edit()
            .key_in("secondary-enter", ctx::SHEET),
        s::<sheet::InsertAbove>()
            .help(tr("Table: insert a row above (a column to the left when whole columns are selected)", "Tableau : insérer une ligne dessus (une colonne à gauche si des colonnes entières sont sélectionnées)")).edit()
            .key_in("secondary-shift-enter", ctx::SHEET),
        s::<sheet::DeleteRows>()
            .help(tr("Table: remove the selected rows, or the columns when whole columns are selected", "Tableau : retirer les lignes sélectionnées, ou les colonnes si des colonnes entières sont sélectionnées")).edit()
            .key_in("secondary-delete", ctx::SHEET),
        s::<editor::TableDeleteSpan>()
            .help(tr("Remove the row, or the columns taken by their grip", "Supprimer la ligne, ou les colonnes prises par leur poignée")).edit()
            .key_in("secondary-shift-backspace", ctx::SHEET)
            .key_in("secondary-shift-backspace", ctx::EDITOR),
        s::<graph::Cycle>()
            .key_in("tab", ctx::NAV),
        s::<nav::Rename>()
            .help(tr("Panel: rename", "Panneau : renommer")).edit()
            .key_in("f2", ctx::NAV),
        s::<nav::Duplicate>()
            .help(tr("Panel: duplicate", "Panneau : dupliquer")).edit()
            .key_in("secondary-d", ctx::NAV),
        s::<nav::Trash>()
            .help(tr("Panel: move to the trash", "Panneau : mettre à la corbeille")).edit()
            .key_in("delete", ctx::NAV),
        s::<nav::Prev>()
            .key_in("up", ctx::NAV),
        s::<nav::Next>()
            .key_in("down", ctx::NAV),
        s::<nav::Fold>()
            .key_in("left", ctx::NAV),
        s::<nav::Unfold>()
            .key_in("right", ctx::NAV),
        s::<nav::Open>()
            .key_in("enter", ctx::NAV),
        s::<nav::PrevMonth>()
            .key_in("pageup", ctx::NAV),
        s::<nav::NextMonth>()
            .key_in("pagedown", ctx::NAV),
        s::<nav::Close>()
            .key_in("escape", ctx::NAV),
        s::<palette::Prev>()
            .key_in("up", ctx::PALETTE),
        s::<palette::Next>()
            .key_in("down", ctx::PALETTE),
        s::<palette::Confirm>()
            .key_in("enter", ctx::PALETTE),
        s::<palette::ConfirmAside>()
            .help(tr("In the palette: open the chosen note in the other pane", "Dans la palette : ouvrir la note choisie dans l'autre volet")).edit()
            .key_in("secondary-enter", ctx::PALETTE),
        s::<palette::Dismiss>()
            .key_in("escape", ctx::PALETTE),
        s::<editor::Backspace>()
            .key_in("backspace", ctx::EDITOR),
        s::<editor::Delete>()
            .key_in("delete", ctx::EDITOR),
        s::<editor::DeleteWordLeft>()
            .key_in(format!("{word}-backspace"), ctx::EDITOR),
        s::<editor::DeleteWordRight>()
            .key_in(format!("{word}-delete"), ctx::EDITOR),
        s::<editor::Left>()
            .key_in("left", ctx::EDITOR),
        s::<editor::Right>()
            .key_in("right", ctx::EDITOR),
        s::<editor::Up>()
            .key_in("up", ctx::EDITOR),
        s::<editor::Down>()
            .key_in("down", ctx::EDITOR),
        s::<editor::WordLeft>()
            .key_in(format!("{word}-left"), ctx::EDITOR),
        s::<editor::WordRight>()
            .key_in(format!("{word}-right"), ctx::EDITOR),
        s::<editor::Home>()
            .key_in("home", ctx::EDITOR)
            .mac("cmd-left", ctx::EDITOR),
        s::<editor::End>()
            .key_in("end", ctx::EDITOR)
            .mac("cmd-right", ctx::EDITOR),
        s::<editor::DocStart>()
            .key_in("secondary-home", ctx::EDITOR)
            .mac("cmd-up", ctx::EDITOR),
        s::<editor::DocEnd>()
            .key_in("secondary-end", ctx::EDITOR)
            .mac("cmd-down", ctx::EDITOR),
        s::<editor::PageUp>()
            .key_in("pageup", ctx::EDITOR),
        s::<editor::PageDown>()
            .key_in("pagedown", ctx::EDITOR),
        s::<editor::SelectLeft>()
            .key_in("shift-left", ctx::EDITOR),
        s::<editor::SelectRight>()
            .key_in("shift-right", ctx::EDITOR),
        s::<editor::SelectUp>()
            .key_in("shift-up", ctx::EDITOR),
        s::<editor::SelectDown>()
            .key_in("shift-down", ctx::EDITOR),
        s::<editor::SelectWordLeft>()
            .key_in(format!("{word}-shift-left"), ctx::EDITOR),
        s::<editor::SelectWordRight>()
            .key_in(format!("{word}-shift-right"), ctx::EDITOR),
        s::<editor::SelectHome>()
            .key_in("shift-home", ctx::EDITOR)
            .mac("cmd-shift-left", ctx::EDITOR),
        s::<editor::SelectEnd>()
            .key_in("shift-end", ctx::EDITOR)
            .mac("cmd-shift-right", ctx::EDITOR),
        s::<editor::SelectDocStart>()
            .key_in("secondary-shift-home", ctx::EDITOR)
            .mac("cmd-shift-up", ctx::EDITOR),
        s::<editor::SelectDocEnd>()
            .key_in("secondary-shift-end", ctx::EDITOR)
            .mac("cmd-shift-down", ctx::EDITOR),
        s::<editor::SelectAll>()
            .help(tr("Select all", "Tout sélectionner")).edit()
            .key_in("secondary-a", ctx::EDITOR),
        s::<editor::FindNext>()
            .help(tr("Search bar: next match (Enter too)", "Barre de recherche : passage suivant (Entrée aussi)")).edit()
            .key_in("f3", ctx::EDITOR)
            .key_in("f3", ctx::FIND),
        s::<editor::FindPrev>()
            .help(tr("Search bar: previous match", "Barre de recherche : passage précédent")).edit()
            .key_in("shift-f3", ctx::EDITOR)
            .key_in("shift-f3", ctx::FIND)
            .key_in("shift-enter", ctx::FIND),
        s::<editor::FindEnter>()
            .key_in("enter", ctx::FIND),
        s::<editor::ReplaceAll>()
            .help(tr("Search bar: replace all (Enter: this match)", "Barre de recherche : tout remplacer (Entrée : ce passage)")).edit()
            .key_in("secondary-enter", ctx::FIND),
        s::<editor::ReplaceInVault>()
            .help(tr("Search bar: replace in the whole vault, after showing the count", "Barre de recherche : remplacer dans tout le coffre, après en avoir montré le compte")).edit()
            .key_in("secondary-shift-enter", ctx::FIND),
        s::<editor::FindSwitch>()
            .key_in("tab", ctx::FIND)
            .key_in("shift-tab", ctx::FIND),
        // Also when the bar is open but input has gone back to the note.
        s::<editor::FindCase>()
            .help(tr("Search bar: match case", "Barre de recherche : respecter la casse")).edit()
            .key_in("alt-c", ctx::FIND)
            .key_in("alt-c", ctx::EDITOR),
        s::<editor::FindWord>()
            .help(tr("Search bar: whole words", "Barre de recherche : mots entiers")).edit()
            .key_in("alt-w", ctx::FIND)
            .key_in("alt-w", ctx::EDITOR),
        s::<editor::FindRegex>()
            .help(tr("Search bar: regular expression ($1 in the replacement)", "Barre de recherche : expression régulière ($1 dans le remplacement)")).edit()
            .key_in("alt-r", ctx::FIND)
            .key_in("alt-r", ctx::EDITOR),
        s::<editor::Newline>()
            .key_in("enter", ctx::EDITOR),
        s::<editor::Indent>()
            .key_in("tab", ctx::EDITOR),
        s::<editor::Outdent>()
            .key_in("shift-tab", ctx::EDITOR),
        // The four direct choices have no key: the menu and the palette offer them, and the help grants them.
        s::<editor::ListCycle>()
            .help(tr("Turn the selected lines into a list, then the next kind: bullets, numbered, tasks, plain text", "Mettre les lignes choisies en liste, puis au genre suivant : puces, numérotée, tâches, texte")).edit()
            .key_in("secondary-shift-b", ctx::EDITOR),
        s::<editor::AlignLeft>()
            .help(tr("Align the column of the cursor to the left", "Aligner la colonne du curseur à gauche")).edit()
            .key_in("secondary-shift-l", ctx::EDITOR),
        s::<editor::AlignCenter>()
            .help(tr("Center the column of the cursor", "Centrer la colonne du curseur")).edit()
            .key_in("secondary-shift-e", ctx::EDITOR),
        s::<editor::AlignRight>()
            .help(tr("Align the column of the cursor to the right", "Aligner la colonne du curseur à droite")).edit()
            .key_in("secondary-shift-r", ctx::EDITOR),
        s::<editor::SelectNext>()
            .help(tr("Several cursors: select the word, then its next occurrence (Esc: back to one)", "Plusieurs curseurs : prendre le mot, puis son occurrence suivante (Échap : un seul)")).edit()
            .key_in("secondary-d", ctx::EDITOR),
        s::<editor::Copy>()
            .label(tr("Copy", "Copier"))
            .help(tr("Copy", "Copier")).edit()
            .key_in("secondary-c", ctx::EDITOR),
        s::<editor::Cut>()
            .label(tr("Cut", "Couper"))
            .help(tr("Cut", "Couper")).edit()
            .key_in("secondary-x", ctx::EDITOR),
        s::<editor::Paste>()
            .label(tr("Paste", "Coller"))
            .help(tr("Paste text or a picture; an address over a selection makes it a link", "Coller du texte ou une image ; une adresse sur une sélection en fait un lien")).edit()
            .key_in("secondary-v", ctx::EDITOR),
        // The board from the keys: arrows go, Alt+arrows carry (before Back and Forward there).
        s::<kanban::CardUp>()
            .key_in("up", ctx::BOARD),
        s::<kanban::CardDown>()
            .key_in("down", ctx::BOARD),
        s::<kanban::CardLeft>()
            .key_in("left", ctx::BOARD),
        s::<kanban::CardRight>()
            .key_in("right", ctx::BOARD),
        s::<kanban::CarryUp>()
            .help(tr("Board: carry the card up", "Tableau : monter la carte")).edit()
            .key_in("alt-up", ctx::BOARD),
        s::<kanban::CarryDown>()
            .help(tr("Board: carry the card down", "Tableau : descendre la carte")).edit()
            .key_in("alt-down", ctx::BOARD),
        s::<kanban::CarryLeft>()
            .help(tr("Board: carry the card to the column on the left", "Tableau : porter la carte dans la colonne de gauche")).edit()
            .key_in("alt-left", ctx::BOARD),
        s::<kanban::CarryRight>()
            .help(tr("Board: carry the card to the column on the right", "Tableau : porter la carte dans la colonne de droite")).edit()
            .key_in("alt-right", ctx::BOARD),
        s::<kanban::CardEdit>()
            .help(tr("Board: write the card or the title (Enter also adds what is chosen)", "Tableau : réécrire la carte ou le titre (Entrée ajoute aussi ce qui est choisi)")).edit()
            .key_in("enter", ctx::BOARD)
            .key_in("f2", ctx::BOARD),
        s::<kanban::CardRemove>()
            .help(tr("Board: remove the card, or the column with its cards (undone with the board's undo)", "Tableau : retirer la carte, ou la colonne avec ses cartes (annulé par l'annulation du tableau)")).edit()
            .key_in("delete", ctx::BOARD),
        // Escape in the note, when the note has nothing to cancel: the failures said, the icon grid
        // and the menu go.
        s::<editor::Cancel>()
            .does(|this, window, cx| {
                this.dismiss_failures();
                this.close_icons(window, cx);
                this.close_menu(window, cx);
            })
            .key_in("escape", ctx::EDITOR),
        s::<editor::ListBullet>()
            .help(tr("Selected lines: bulleted list (again: plain text)", "Lignes choisies : liste à puces (encore : texte)")).edit(),
        s::<editor::ListOrdered>()
            .help(tr("Selected lines: numbered list (again: plain text)", "Lignes choisies : liste numérotée (encore : texte)")).edit(),
        s::<editor::ListTask>()
            .help(tr("Selected lines: task list (again: plain text)", "Lignes choisies : liste de tâches (encore : texte)")).edit(),
        s::<editor::ListText>()
            .help(tr("Selected lines: plain text", "Lignes choisies : texte simple")).edit(),
    ];
    // The gestures of a block and of a table carry their help line in the view that knows them.
    let mut help = |action: Box<dyn Action>, text: &'static str| {
        let spec = all.iter_mut().find(|spec| spec.action == action.name()).expect("a gesture of the view has no entry");
        (spec.help, spec.editable) = (Some(text), true);
    };
    BlockOp::ALL.into_iter().for_each(|op| help(op.action(), op.effect()));
    TableOp::ALL.into_iter().for_each(|op| help(op.action(), op.label()));
    all
}

/// Puts the window's handlers on its element: every entry that has one.
pub(super) fn register(element: Div, cx: &Context<Shell>) -> Div {
    // The table does not change: its handlers are gathered once, not at every frame.
    static HANDLERS: std::sync::OnceLock<Vec<(Bind, Handler)>> = std::sync::OnceLock::new();
    let all = HANDLERS.get_or_init(|| specs().into_iter().filter_map(|spec| Some((spec.bind, spec.handler?))).collect());
    all.iter().fold(element, |element, (bind, handler)| bind(element, cx, *handler))
}

/// The label of an action, from the table (it panics if the entry has none: a bug in the table).
pub(super) fn label<A: Action>() -> &'static str {
    specs().into_iter().find(|spec| spec.action == A::name_for_type()).and_then(|spec| spec.label).expect("an action the menus use has no label")
}

/// The default shortcuts. For the same keys, the last binding wins at equal depth, and one with
/// no context counts as the deepest: the window's own come first, then the views', so that a
/// view's key is never shadowed by one that holds everywhere. Within a rank, the table's order.
pub(super) fn bindings() -> Vec<keys::Bind> {
    let mut all: Vec<keys::Bind> = specs()
        .into_iter()
        .flat_map(|spec| spec.keys.into_iter().map(move |key| keys::Bind { keys: key.keys, action: spec.action, context: key.context }))
        .collect();
    let rank = |bind: &keys::Bind| match bind.context {
        None => 0,
        Some(ctx::SHELL) => 1,
        Some(_) => 2,
    };
    all.sort_by_key(rank);
    all
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_names_each_action_once() {
        let mut seen = std::collections::HashSet::new();
        assert!(specs().iter().all(|spec| seen.insert(spec.action)), "an action twice");
    }

    #[test]
    fn a_palette_command_has_all_it_needs_and_an_editable_one_its_help() {
        for spec in specs() {
            assert!(spec.group.is_none() || spec.label.is_some(), "{}: a palette command needs a label", spec.action);
            assert!(spec.group.is_none() || spec.icon.ends_with(".svg"), "{}: a palette command has an icon", spec.action);
            assert!(!spec.editable || spec.help.is_some(), "{}: editable without a help line", spec.action);
            assert!(spec.label.is_none_or(|label| !label.is_empty()) && spec.help.is_none_or(|help| !help.is_empty()), "{}", spec.action);
        }
    }

    /// The context menu names its commands from the table (a missing label would panic when it opens).
    #[test]
    fn the_menu_s_commands_have_their_label() {
        let labels = [
            label::<editor::Copy>(), label::<editor::Cut>(), label::<editor::Paste>(), label::<editor::Comment>(), label::<editor::Bold>(),
            label::<editor::Italic>(), label::<editor::Underline>(), label::<editor::Strike>(), label::<editor::Highlight>(),
            label::<editor::InlineCode>(), label::<NewFromTemplate>(), label::<nav::NewFolder>(),
        ];
        assert!(labels.iter().all(|label| !label.is_empty()));
        assert_eq!(label::<editor::Highlight>(), tr("Highlight", "Surligner"), "one word for the toolbar's and the menu's command");
    }

    #[test]
    fn every_default_binding_comes_from_the_table_but_the_input_line_s() {
        let table = specs();
        for bind in keymap::defaults().into_iter().filter(|bind| bind.context != Some(ctx::LINE)) {
            let spec = table.iter().find(|spec| spec.action == bind.action).unwrap_or_else(|| panic!("{} is bound but not in the table", bind.action));
            assert!(spec.keys.iter().any(|key| key.keys == bind.keys && key.context == bind.context), "{} {}", bind.action, bind.keys);
        }
    }

    /// The defaults as they were before the table (generated from the code of that day): the same
    /// bindings, and, for the same keys in contexts that meet, the same order, since the last one wins.
    #[test]
    #[cfg(not(target_os = "macos"))]
    fn the_defaults_are_those_of_the_frozen_snapshot() {
        let frozen: Vec<(String, String, Option<String>)> = include_str!("defaults.snap")
            .lines()
            .map(|line| {
                let mut part = line.split('|');
                let (keys, action, context) = (part.next().unwrap(), part.next().unwrap(), part.next().unwrap());
                (keys.to_string(), action.to_string(), (context != "-").then(|| context.to_string()))
            })
            .collect();
        let now: Vec<(String, String, Option<String>)> =
            keymap::defaults().into_iter().map(|bind| (bind.keys, bind.action.to_string(), bind.context.map(str::to_string))).collect();
        let (mut a, mut b) = (frozen.clone(), now.clone());
        a.sort();
        b.sort();
        assert_eq!(a, b, "not the same bindings");
        let position = |list: &[(String, String, Option<String>)], bind: &(String, String, Option<String>)| list.iter().position(|other| other == bind).unwrap();
        for (i, first) in frozen.iter().enumerate() {
            for second in &frozen[i + 1..] {
                if first.0 == second.0 && keys::meets(first.2.as_deref(), second.2.as_deref()) {
                    assert!(position(&now, first) < position(&now, second), "{first:?} must stay before {second:?}");
                }
            }
        }
    }
}
