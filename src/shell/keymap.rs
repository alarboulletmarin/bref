//! The default shortcuts, the help sections that list them, and their installation.

use super::*;

/// Content of the help panel, and with it the list of shortcuts that can be changed: one `k`
/// line per action, whose keys are the ones in force (`bound`); one `t` line for what
/// is not a binding (mouse, typed text, palette command, fixed keys).
pub(super) fn help_sections(bound: &[keys::Bind], changes: &keys::Changes) -> keys::Sections {
    use gpui::Action;
    use keys::Row;
    let k = |action: &dyn Action, effect: &'static str| Row {
        keys: keys::shown(bound, action.name()),
        effect,
        action: Some(action.name()),
        changed: changes.contains_key(action.name()),
    };
    let t = |keys: &str, effect: &'static str| Row::text(keys, effect);
    // A palette command: its keys, then the word to type in it.
    let p = |word: &str, effect: &'static str| {
        let palette = keys::shown(bound, OpenPalette.name());
        Row::text(format!("{} › {word}", if palette.is_empty() { "Palette" } else { &palette }), effect)
    };
    let click = tr("click", "clic");
    let shift = tr("Shift", "Maj");
    let word = if cfg!(target_os = "macos") { "Alt" } else { "Ctrl" };
    vec![
        (
            "Notes",
            vec![
                k(&OpenPalette, tr("Find by name or text, create, filter by #tag", "Chercher par nom ou texte, créer, filtrer par #tag")),
                k(&ToggleCommands, tr("Command palette: every command, found by what it does (also the first button of the rail, or > in the palette)", "Palette des commandes : toutes les commandes, trouvées par ce qu'elles font (aussi le premier bouton du rail, ou > dans la palette)")),
                k(&editor::Find, tr("Find in the note", "Chercher dans la note")),
                k(&editor::FindReplace, tr("Find and replace in the note", "Chercher et remplacer dans la note")),
                k(&SearchVault, tr("Search the text of the whole vault, line by line", "Chercher dans le texte de tout le coffre, ligne par ligne")),
                k(&editor::FindNext, tr("Search bar: next match (Enter too)", "Barre de recherche : passage suivant (Entrée aussi)")),
                k(&editor::FindPrev, tr("Search bar: previous match", "Barre de recherche : passage précédent")),
                k(&editor::FindCase, tr("Search bar: match case", "Barre de recherche : respecter la casse")),
                k(&editor::FindWord, tr("Search bar: whole words", "Barre de recherche : mots entiers")),
                k(&editor::FindRegex, tr("Search bar: regular expression ($1 in the replacement)", "Barre de recherche : expression régulière ($1 dans le remplacement)")),
                t("Tab", tr("Search bar: replace field", "Barre de recherche : champ de remplacement")),
                k(&editor::ReplaceAll, tr("Search bar: replace all (Enter: this match)", "Barre de recherche : tout remplacer (Entrée : ce passage)")),
                k(&editor::ReplaceInVault, tr("Search bar: replace in the whole vault, after showing the count", "Barre de recherche : remplacer dans tout le coffre, après en avoir montré le compte")),
                k(&Outline, tr("Outline: jump to a heading of the note", "Plan : aller à un titre de la note")),
                k(&NewNote, tr("New note", "Nouvelle note")),
                k(&NewFromTemplate, tr("New note from a template (a note of the Templates folder: {{title}} {{date}} {{time}} {{cursor}})", "Nouvelle note depuis un modèle (une note du dossier Templates ou Modèles : {{title}} {{date}} {{time}} {{cursor}})")),
                p(tr("template", "modèle"), tr("Insert a template at the cursor, or make a template of the open note", "Insérer un modèle au curseur, ou faire un modèle de la note ouverte")),
                k(&Today, tr("Today's note: open it, or create it", "Note du jour : l'ouvrir, ou la créer")),
                p("capture", tr("Quick capture: sets up a shortcut of your desktop (Super+Shift+N on GNOME) that opens a one-line window anywhere; what you type goes to today's note", "Capture rapide : crée un raccourci du bureau (Super+Maj+N sous GNOME) qui ouvre partout une fenêtre d'une ligne ; ce qu'on y tape va dans la note du jour")),
                k(&OpenVault, tr("Change vault", "Changer de coffre")),
                p("terminal", tr("Open a terminal in the vault", "Ouvrir un terminal dans le coffre")),
                k(&SyncVault, tr("Sync the vault with git: commit, receive, merge, send; a conflict keeps both versions as two notes", "Synchroniser le coffre par git : commiter, recevoir, fusionner, envoyer ; un conflit garde les deux versions en deux notes")),
                p("clone", tr("Clone a vault from a git address (also on the welcome screen)", "Cloner un coffre depuis une adresse git (aussi sur l'écran d'accueil)")),
                t(tr("A cloud folder", "Un dossier synchronisé"), tr("iCloud, Dropbox, Google Drive, Syncthing: put the vault there, nothing else; never a git repository inside one", "iCloud, Dropbox, Google Drive, Syncthing : y poser le coffre, rien d'autre ; jamais un dépôt git dans un tel dossier")),
                k(&CopyAll, tr("Copy the code block, else the note", "Copier le bloc de code, sinon la note")),
                t(&format!("{MOD}+{click}"), tr("Open a [[link]], #tag or URL", "Ouvrir un [[lien]], #tag ou URL")),
                k(&ToggleHelp, tr("This help", "Cette aide")),
                k(&Quit, tr("Quit", "Quitter")),
            ],
        ),
        (
            "Navigation",
            vec![
                k(&GoBack, tr("Back: what was shown before, at the same place (also the mouse button, and the rail)", "Précédent : ce qui était affiché avant, au même endroit (aussi le bouton de la souris, et le rail)")),
                k(&GoForward, tr("Forward: what was shown after going back", "Suivant : ce qui était affiché après un retour")),
                k(&SplitPane, tr("Second pane, side by side: opens on the palette, to choose what goes there", "Second volet, côte à côte : s'ouvre sur la palette, pour choisir ce qui y va")),
                k(&FocusLeft, tr("Go to the left pane", "Aller au volet de gauche")),
                k(&FocusRight, tr("Go to the right pane", "Aller au volet de droite")),
                k(&ClosePane, tr("Close the pane that has the focus", "Fermer le volet qui a la saisie")),
                k(&palette::ConfirmAside, tr("In the palette: open the chosen note in the other pane", "Dans la palette : ouvrir la note choisie dans l'autre volet")),
                t(&format!("{MOD}+{shift}+{click}"), tr("Open a [[link]] in the other pane", "Ouvrir un [[lien]] dans l'autre volet")),
                t(tr("Drag from the tree", "Glisser depuis l'arbre"), tr("Onto a pane: open the file there; onto the right half of a single pane: open a second one", "Sur un volet : y ouvrir le fichier ; sur la moitié droite du seul volet : en ouvrir un second")),
                k(&nav::ShowTree, tr("Vault tree", "Arbre du coffre")),
                k(&nav::ShowRecent, tr("Recent notes", "Notes récentes")),
                k(&nav::ShowGraph, tr("Graph of the notes", "Graphe des notes")),
                k(&nav::ShowTags, "Tags"),
                k(&nav::ShowLinks, tr("Backlinks: the notes that link to this one", "Rétroliens : les notes qui mènent à celle-ci")),
                k(&nav::ShowCalendar, tr("Calendar: arrows, Page Up/Down for the month, Enter opens the day", "Calendrier : flèches, Page haut/bas pour le mois, Entrée ouvre le jour")),
                t(tr("A picture", "Une image"), tr("Shown in place of the note; a square in the graph", "Affichée à la place de la note ; un carré dans le graphe")),
                t(tr("A .csv or .tsv", "Un .csv ou .tsv"), tr("A grid in place of the note; not in the graph", "Une grille à la place de la note ; absent du graphe")),
                k(&nav::ToggleFull, tr("Panel on the whole window", "Panneau en pleine fenêtre")),
                t(tr("Arrows / Tab", "Flèches / Tab"), tr("Select and preview / linked notes (graph)", "Sélectionner en aperçu / notes liées (graphe)")),
                t(tr("Enter / Esc", "Entrée / Échap"), tr("Open the note / back to the note", "Ouvrir la note / revenir à la note")),
                k(&nav::NewFolder, tr("New folder (also in the palette)", "Nouveau dossier (aussi dans la palette)")),
                t(tr("Point at an icon", "Pointer une icône"), tr("Its name and its shortcut", "Son nom et son raccourci")),
                t("Alt + drag", tr("Move the window from anywhere (Linux)", "Déplacer la fenêtre depuis n'importe où (Linux)")),
                k(&nav::Rename, tr("Panel: rename", "Panneau : renommer")),
                k(&nav::Duplicate, tr("Panel: duplicate", "Panneau : dupliquer")),
                k(&nav::Trash, tr("Panel: move to the trash", "Panneau : mettre à la corbeille")),
                t(&format!("{MOD}+{click} / {shift}+{click}"), tr("Select several rows: move, duplicate, trash them together", "Sélectionner plusieurs lignes : les déplacer, dupliquer, jeter ensemble")),
                t(tr("Right click", "Clic droit"), tr("Copy the link or the path, set an icon, show a folder or a tag as a list…", "Copier le lien ou le chemin, donner une icône, afficher un dossier ou un tag en liste…")),
                t(tr("In a list", "Dans une liste"), tr("Type to filter (key:text for one column), click a cell to rewrite it", "Taper pour filtrer (clé:texte pour une colonne), cliquer une cellule pour la réécrire")),
                t(tr("Drag a node", "Glisser un nœud"), tr("Move it in the graph, linked notes follow", "Le déplacer dans le graphe, les notes liées suivent")),
            ],
        ),
        (
            tr("Diagrams", "Schémas"),
            vec![
                k(&NewDiagram, tr("New diagram: an SVG file in the vault", "Nouveau schéma : un fichier SVG du coffre")),
                t("R U O D C P N T", tr("Rectangle, rounded, ellipse, diamond, cylinder, person, note, text", "Rectangle, arrondi, ellipse, losange, cylindre, personnage, note, texte")),
                t("A / L / V", tr("Arrow / line, held by the shapes they join / select", "Flèche / trait, accrochés aux formes reliées / sélection")),
                t(tr("Enter, double click", "Entrée, double-clic"), tr("Write in the shape or on the arrow; Esc when done", "Écrire dans la forme ou sur la flèche ; Échap pour finir")),
                t("---", tr("Alone on a line of a box: a compartment (UML class)", "Seul sur une ligne d'une boîte : un compartiment (classe UML)")),
                t(&format!("{shift}+{click} / {MOD}+{click}"), tr("Add a shape to the selection, or take it out", "Ajouter une forme à la sélection, ou l'en retirer")),
                t(tr("Delete", "Suppr"), tr("Remove the selection", "Retirer la sélection")),
                k(&canvas::Duplicate, tr("Diagram: duplicate the selection", "Schéma : dupliquer la sélection")),
                k(&canvas::Undo, tr("Diagram: undo", "Schéma : annuler")),
                k(&canvas::Redo, tr("Diagram: redo", "Schéma : rétablir")),
                t(tr("Edge of a shape", "Bord d'une forme"), tr("An arrow end dropped there stays there; in the middle, it follows the other end", "Un bout de flèche lâché là y reste ; au milieu, il suit l'autre bout")),
                t(tr("Wheel / + - 0", "Molette / + - 0"), tr("Move the view / zoom in, out, fit all", "Déplacer la vue / zoomer, dézoomer, tout cadrer")),
                t("![](Schéma.svg)", tr("Show the diagram in a note", "Afficher le schéma dans une note")),
                t(".svg", tr("An SVG from elsewhere (draw.io, Mermaid.js…) opens in a viewer: drag or wheel to move, + - 0 to zoom", "Un SVG d'ailleurs (draw.io, Mermaid.js…) s'ouvre dans une visionneuse : glisser ou molette pour se déplacer, + - 0 pour zoomer")),
                p("import", tr("Bring in an Excalidraw or draw.io file", "Reprendre un fichier Excalidraw ou draw.io")),
            ],
        ),
        (
            tr("CSV tables", "Tableaux CSV"),
            vec![
                t(".csv .tsv", tr("Opens in a grid; encoding and delimiter are detected, written straight into the file", "S'ouvre dans une grille ; encodage et délimiteur sont devinés, écrit directement dans le fichier")),
                t(tr("Arrows, Tab", "Flèches, Tab"), tr("Move; with Shift (or drag), select a range; click a row number or a header", "Se déplacer ; avec Maj (ou en glissant), sélectionner une plage ; clic sur un numéro ou un en-tête")),
                t(&format!("{MOD}+{click}"), tr("Add another selection: copy, cut, empty or remove their rows together", "Ajouter une autre sélection : les copier, couper, vider ou retirer leurs lignes ensemble")),
                t(tr("Type, Enter, F2", "Taper, Entrée, F2"), tr("Replace the cell / validate and go down / open the cell; Esc cancels", "Remplacer la cellule / valider et descendre / ouvrir la cellule ; Échap annule")),
                k(&sheet::Copy, tr("Table: copy as tab-separated text", "Tableau : copier en texte à tabulations")),
                k(&sheet::Cut, tr("Table: cut", "Tableau : couper")),
                k(&sheet::Paste, tr("Table: paste cells from a spreadsheet, Markdown or CSV", "Tableau : coller des cellules d'un tableur, de Markdown ou de CSV")),
                k(&sheet::SelectRow, tr("Table: select the whole rows of the selection", "Tableau : sélectionner les lignes entières de la sélection")),
                k(&sheet::SelectCol, tr("Table: select the whole columns of the selection", "Tableau : sélectionner les colonnes entières de la sélection")),
                k(&sheet::InsertBelow, tr("Table: insert a row below (a column to the right when whole columns are selected)", "Tableau : insérer une ligne dessous (une colonne à droite si des colonnes entières sont sélectionnées)")),
                k(&sheet::InsertAbove, tr("Table: insert a row above (a column to the left when whole columns are selected)", "Tableau : insérer une ligne dessus (une colonne à gauche si des colonnes entières sont sélectionnées)")),
                k(&sheet::DeleteRows, tr("Table: remove the selected rows, or the columns when whole columns are selected", "Tableau : retirer les lignes sélectionnées, ou les colonnes si des colonnes entières sont sélectionnées")),
                k(&sheet::Undo, tr("Table: undo", "Tableau : annuler")),
                k(&sheet::Redo, tr("Table: redo", "Tableau : rétablir")),
                p("table", tr("Delimiter, encoding, header row, copy as…, export (CSV, TSV, Markdown, JSON)", "Délimiteur, encodage, en-tête, copier en…, exporter (CSV, TSV, Markdown, JSON)")),
            ],
        ),
        (
            tr("Appearance", "Apparence"),
            vec![
                p(tr("theme", "thème"), tr("Theme, previewed as you browse", "Thème, en aperçu pendant le choix")),
                p(tr("font", "police"), tr("Font of the app / of the code", "Police de l'app / du code")),
                p(tr("page width", "largeur de page"), tr("Width of the text column: narrow, normal, wide, full", "Largeur de la colonne de texte : étroite, normale, large, pleine")),
                p(tr("wide tables", "grands tableaux"), tr("A table wider than the text: in the column, or centered and wider", "Un tableau plus large que le texte : dans la colonne, ou centré et plus large")),
                k(&ZoomIn, tr("Bigger text", "Texte plus grand")),
                k(&ZoomOut, tr("Smaller text", "Texte plus petit")),
                k(&ZoomReset, tr("Default text size", "Taille du texte d'origine")),
            ],
        ),
        (
            // The table gestures of the menu: the frame says where they act, each can have its key.
            tr("Tables in a note", "Tableaux d'une note"),
            std::iter::once(t(
                tr("Grips", "Poignées"),
                tr(
                    "Over a table: a small grip above the column and left of the row under the pointer, a round one at the corner for the whole table. Click one to take the column, the row or the table (copy, cut, erase, or any gesture below), drag it to move it, Esc lets go; the + between two grips inserts there",
                    "Sur un tableau : une petite poignée au-dessus de la colonne et à gauche de la ligne sous le pointeur, une ronde au coin pour tout le tableau. Un clic prend la colonne, la ligne ou le tableau (copier, couper, effacer, ou un geste ci-dessous), tirer la déplace, Échap la lâche ; le + entre deux poignées insère à cet endroit",
                ),
            ))
            .chain(editor::TableOp::ALL.into_iter().map(|op| k(op.action().as_ref(), op.label())))
            .chain([k(
                &editor::TableDeleteSpan,
                tr("Remove the row, or the columns taken by their grip", "Supprimer la ligne, ou les colonnes prises par leur poignée"),
            )])
            .collect(),
        ),
        (
            tr("Typing", "À la frappe"),
            vec![
                t("/", tr("Components: headings, lists, panel, table, code, formula…", "Composants : titres, listes, panneau, tableau, code, formule…")),
                t("/table", tr("Table: pick its size on the grid (mouse, arrows), or type it: 12x5", "Tableau : choisir sa taille sur la grille (souris, flèches), ou la taper : 12x5")),
                t("> [!NOTE]", tr("Colored panel: NOTE, TIP, IMPORTANT, WARNING, CAUTION", "Panneau coloré : NOTE, TIP, IMPORTANT, WARNING, CAUTION")),
                t("- ", tr("Bullet list", "Liste à puces")),
                t("1. ", tr("Numbered list", "Liste numérotée")),
                t("[] ", tr("Task", "Tâche à cocher")),
                t("# ## ###", tr("Headings; the first # names the file", "Titres ; le premier # nomme le fichier")),
                t("> ", tr("Quote", "Citation")),
                t("```rust", tr("Code block: colors, copy icon", "Bloc de code : couleurs, icône de copie")),
                t("---", tr("Divider", "Séparateur")),
                t("[[", tr("Link to a note", "Lien vers une note")),
                p("kanban", tr("New kanban board: cards you type, drag and tick; double click to rewrite", "Nouveau tableau kanban : des cartes à écrire, glisser et cocher ; double-clic pour réécrire")),
                t("@", tr("Link to the note of a day: @today, @monday, @2026-10-09", "Lien vers la note d'un jour : @demain, @lundi, @2026-10-09")),
                t("![](image.png)", tr("Picture, under its line", "Image, sous sa ligne")),
                t("![[", tr("Suggests the pictures and diagrams of the vault", "Propose les images et les schémas du coffre")),
                t(tr("Drop files", "Déposer des fichiers"), tr("Drag files or folders from your file manager onto a note (a line shows where) or onto the tree (the folder under the pointer): they are copied, never overwritten, and a note gets a link per item", "Glisser des fichiers ou des dossiers du gestionnaire de fichiers sur une note (un trait montre où) ou sur l'arbre (le dossier sous le pointeur) : ils sont copiés, jamais écrasés, et la note reçoit un lien par élément")),
                t(tr("Double / triple click", "Double / triple clic"), tr("Select a word / a line; drag to extend by words / lines", "Sélectionner un mot / une ligne ; glisser étend par mots / par lignes")),
                t("```mermaid", tr("Diagram, under its block; its magnifier shows it full size", "Diagramme, sous son bloc ; sa loupe le montre en grand")),
                t("$x^2$  $$…$$", tr("LaTeX formula, under its line", "Formule LaTeX, sous sa ligne")),
                t("-> != <= =>", tr("Shown as → ≠ ≤ ⇒", "Affichés → ≠ ≤ ⇒")),
            ],
        ),
        (
            tr("Editing", "Édition"),
            vec![
                t(tr("Enter", "Entrée"), tr("Continue the list, or leave it on an empty item", "Continuer la liste, ou en sortir sur un item vide")),
                t(&format!("Tab / {shift}+Tab"), tr("Indent / outdent", "Indenter / désindenter")),
                t(tr("Tab / Enter in a table", "Tab / Entrée dans un tableau"), tr("Next cell / new row; columns stay aligned; + buttons add a row or a column", "Cellule suivante / nouvelle ligne ; les colonnes restent alignées ; les boutons + ajoutent une ligne ou une colonne")),
                k(&editor::AlignLeft, tr("Align the column of the cursor to the left", "Aligner la colonne du curseur à gauche")),
                k(&editor::AlignCenter, tr("Center the column of the cursor", "Centrer la colonne du curseur")),
                k(&editor::AlignRight, tr("Align the column of the cursor to the right", "Aligner la colonne du curseur à droite")),
                t(tr("Pasting a table", "Coller un tableau"), tr("Spreadsheet cells or Markdown fill the grid, or become a table", "Des cellules de tableur ou du Markdown remplissent la grille, ou deviennent un tableau")),
                k(&editor::ListCycle, tr("Turn the selected lines into a list, then the next kind: bullets, numbered, tasks, plain text", "Mettre les lignes choisies en liste, puis au genre suivant : puces, numérotée, tâches, texte")),
                k(&editor::ListBullet, tr("Selected lines: bulleted list (again: plain text)", "Lignes choisies : liste à puces (encore : texte)")),
                k(&editor::ListOrdered, tr("Selected lines: numbered list (again: plain text)", "Lignes choisies : liste numérotée (encore : texte)")),
                k(&editor::ListTask, tr("Selected lines: task list (again: plain text)", "Lignes choisies : liste de tâches (encore : texte)")),
                k(&editor::ListText, tr("Selected lines: plain text", "Lignes choisies : texte simple")),
                k(&editor::ToggleTask, tr("Check / uncheck a task or a [ ] box; in a table cell, add one", "Cocher / décocher une tâche ou une case [ ] ; dans une cellule, en ajouter une")),
                k(&editor::Bold, tr("Bold", "Gras")),
                k(&editor::Italic, tr("Italic", "Italique")),
                k(&editor::Underline, tr("Underline", "Souligné")),
                k(&editor::Strike, tr("Strikethrough", "Barré")),
                k(&editor::Highlight, tr("Highlight", "Surligné")),
                k(&editor::InlineCode, tr("Inline code", "Code en ligne")),
                p(tr("formatting bar", "barre de mise en forme"), tr("Under the note, centered: bold, italic, underline, strikethrough, highlight (five colors), code, link, lists, tasks, table (in a table: add or delete a row or a column); a lit button means the selection has the style, click again to remove it; only the active pane shows it; in a narrow note it folds into a button at the bottom left that opens it; hide it with the palette", "Sous la note, au centre : gras, italique, souligné, barré, surligné (cinq couleurs), code, lien, listes, tâches, tableau (dans un tableau : ajouter ou supprimer une ligne ou une colonne) ; un bouton allumé : la sélection a le style, un second clic le retire ; seul le pane actif la montre ; dans une note étroite, elle se replie en un bouton en bas à gauche qui l'ouvre ; masquable par la palette")),
                k(&editor::SelectNext, tr("Several cursors: select the word, then its next occurrence (Esc: back to one)", "Plusieurs curseurs : prendre le mot, puis son occurrence suivante (Échap : un seul)")),
                t(&format!("Alt+{click}"), tr("Add a cursor", "Ajouter un curseur")),
                k(&editor::InsertLink, tr("Link: [text](address) from the selection; on a link, change its address", "Lien : [texte](adresse) depuis la sélection ; sur un lien, changer son adresse")),
                k(&editor::Comment, tr("Comment the selection or the line; on a comment, resolve it", "Commenter la sélection ou la ligne ; sur un commentaire, le résoudre")),
                p(tr("comments", "commentaires"), tr("Comments: show their panel, jump to one (a badge on the speech bubble counts them)", "Commentaires : montrer leur panneau, aller à l'un d'eux (une pastille sur la bulle les compte)")),
                t(tr("Right click", "Clic droit"), tr("Menu of the note: link, comment, cut, copy, paste, bold, italic", "Menu de la note : lien, commentaire, couper, copier, coller, gras, italique")),
                k(&editor::Copy, tr("Copy", "Copier")),
                k(&editor::Cut, tr("Cut", "Couper")),
                k(&editor::Paste, tr("Paste text or a picture; an address over a selection makes it a link", "Coller du texte ou une image ; une adresse sur une sélection en fait un lien")),
                k(&editor::SelectAll, tr("Select all", "Tout sélectionner")),
                k(&editor::Undo, tr("Undo", "Annuler")),
                k(&editor::Redo, tr("Redo", "Rétablir")),
                t(&format!("{word}+{}", tr("Left / Right", "Gauche / Droite")), tr("Move by word", "Se déplacer par mot")),
            ],
        ),
        (
            tr("Blocks", "Blocs"),
            std::iter::once(t(
                tr("Grip", "Poignée"),
                tr(
                    "Left of the line under the pointer: drag it to move the block (a line shows where it lands), click it to select the block and open its menu; the + left of it adds a block under it and opens the / list",
                    "À gauche de la ligne sous le pointeur : la tirer déplace le bloc (un trait montre où il arrive), un clic sélectionne le bloc et ouvre son menu ; le + à sa gauche ajoute un bloc dessous et ouvre la liste /",
                ),
            ))
            .chain(editor::BlockOp::ALL.into_iter().map(|op| k(op.action().as_ref(), op.effect())))
            .collect(),
        ),
    ]
}

/// The actions whose shortcut can be changed: those that have their line in the help.
pub(super) fn editable() -> std::collections::HashSet<&'static str> {
    help_sections(&[], &keys::Changes::new()).into_iter().flat_map(|(_, rows)| rows).filter_map(|row| row.action).collect()
}

/// The default shortcut table. Its order matters: for the same context, the last binding
/// wins.
pub(super) fn defaults() -> Vec<keys::Bind> {
    use editor::*;
    use keys::bind as b;
    use palette::{Confirm, Dismiss, Next, Prev};
    let e = Some("Editor");
    let p = Some("Palette");
    let f = Some("Find");
    let n = Some("Nav");
    let c = Some("Canvas");
    let sh = Some("Sheet");
    // `secondary` = Cmd on macOS, Ctrl elsewhere; words are traversed with Alt on macOS.
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    let mut all = vec![
        b("secondary-p", &OpenPalette, None),
        b("secondary-n", &NewNote, None),
        b("secondary-o", &OpenVault, None),
        b("secondary-shift-c", &CopyAll, None),
        b("f1", &ToggleHelp, None),
        b("secondary-/", &ToggleHelp, None),
        b("escape", &CloseHelp, Some("Shell")),
        b("secondary-shift-o", &Outline, None),
        b("secondary-j", &Today, None),
        b("secondary-shift-s", &SyncVault, None),
        b("secondary-\\", &SplitPane, None),
        b("secondary-1", &FocusLeft, None),
        b("secondary-2", &FocusRight, None),
        b("secondary-w", &ClosePane, None),
        // On macOS, Alt+arrow traverses words: brackets, as in a browser.
        b(if cfg!(target_os = "macos") { "cmd-[" } else { "alt-left" }, &GoBack, None),
        b(if cfg!(target_os = "macos") { "cmd-]" } else { "alt-right" }, &GoForward, None),
        b("secondary-shift-f", &SearchVault, None),
        b("secondary-=", &ZoomIn, None),
        b("secondary-+", &ZoomIn, None),
        b("secondary--", &ZoomOut, None),
        b("secondary-0", &ZoomReset, None),
        b("secondary-q", &Quit, None),
        // The command palette: F10, like the menu bar of a desktop app.
        b("f10", &ToggleCommands, None),
        b("secondary-shift-d", &NewDiagram, Some("Shell")),
        b("secondary-=", &canvas::ZoomIn, c),
        b("secondary-+", &canvas::ZoomIn, c),
        b("secondary--", &canvas::ZoomOut, c),
        b("secondary-0", &canvas::ZoomFit, c),
        b("backspace", &canvas::Erase, c),
        b("delete", &canvas::EraseNext, c),
        b("left", &canvas::Left, c),
        b("right", &canvas::Right, c),
        b("up", &canvas::Up, c),
        b("down", &canvas::Down, c),
        b("secondary-z", &canvas::Undo, c),
        b("secondary-shift-z", &canvas::Redo, c),
        b("secondary-d", &canvas::Duplicate, c),
        b("secondary-a", &canvas::SelectAll, c),
        b("secondary-v", &canvas::Paste, c),
        b("enter", &canvas::Confirm, c),
        b("escape", &canvas::Cancel, c),
        // Table (CSV, TSV): arrows, selection with Shift, input, clipboard, undo.
        b("left", &sheet::Left, sh),
        b("right", &sheet::Right, sh),
        b("up", &sheet::Up, sh),
        b("down", &sheet::Down, sh),
        b("shift-left", &sheet::ExtendLeft, sh),
        b("shift-right", &sheet::ExtendRight, sh),
        b("shift-up", &sheet::ExtendUp, sh),
        b("shift-down", &sheet::ExtendDown, sh),
        b("pageup", &sheet::PageUp, sh),
        b("pagedown", &sheet::PageDown, sh),
        b("home", &sheet::RowStart, sh),
        b("end", &sheet::RowEnd, sh),
        b("secondary-home", &sheet::Origin, sh),
        b("secondary-end", &sheet::Corner, sh),
        b("secondary-up", &sheet::FirstRow, sh),
        b("secondary-down", &sheet::LastRow, sh),
        b("tab", &sheet::Next, sh),
        b("shift-tab", &sheet::Previous, sh),
        b("enter", &sheet::Enter, sh),
        b("f2", &sheet::Edit, sh),
        b("escape", &sheet::Cancel, sh),
        b("backspace", &sheet::Backspace, sh),
        b("delete", &sheet::Delete, sh),
        b("secondary-c", &sheet::Copy, sh),
        b("secondary-x", &sheet::Cut, sh),
        b("secondary-v", &sheet::Paste, sh),
        b("secondary-a", &sheet::SelectAll, sh),
        b("secondary-z", &sheet::Undo, sh),
        b("secondary-shift-z", &sheet::Redo, sh),
        b("secondary-y", &sheet::Redo, sh),
        b("secondary-enter", &sheet::InsertBelow, sh),
        b("secondary-shift-enter", &sheet::InsertAbove, sh),
        b("secondary-delete", &sheet::DeleteRows, sh),
        // As in a spreadsheet: the whole row, the whole column of the selection.
        b("shift-space", &sheet::SelectRow, sh),
        b("ctrl-space", &sheet::SelectCol, sh),
        b("secondary-shift-backspace", &TableDeleteSpan, sh),
        b("secondary-e", &nav::ShowTree, Some("Shell")),
        b("secondary-r", &nav::ShowRecent, Some("Shell")),
        b("secondary-g", &nav::ShowGraph, Some("Shell")),
        b("secondary-t", &nav::ShowTags, Some("Shell")),
        b("secondary-l", &nav::ShowLinks, Some("Shell")),
        b("secondary-shift-j", &nav::ShowCalendar, Some("Shell")),
        b("secondary-m", &nav::ToggleFull, Some("Shell")),
        b("tab", &graph::Cycle, n),
        b("secondary-shift-n", &nav::NewFolder, Some("Shell")),
        b("f2", &nav::Rename, n),
        b("secondary-d", &nav::Duplicate, n),
        b("delete", &nav::Trash, n),
        b("up", &nav::Prev, n),
        b("down", &nav::Next, n),
        b("left", &nav::Fold, n),
        b("right", &nav::Unfold, n),
        b("enter", &nav::Open, n),
        b("pageup", &nav::PrevMonth, n),
        b("pagedown", &nav::NextMonth, n),
        b("escape", &nav::Close, n),
        b("up", &Prev, p),
        b("down", &Next, p),
        b("enter", &Confirm, p),
        b("secondary-enter", &palette::ConfirmAside, p),
        b("escape", &Dismiss, p),
        b("backspace", &Backspace, e),
        b("delete", &Delete, e),
        b(&format!("{word}-backspace"), &DeleteWordLeft, e),
        b(&format!("{word}-delete"), &DeleteWordRight, e),
        b("left", &Left, e),
        b("right", &Right, e),
        b("up", &Up, e),
        b("down", &Down, e),
        b(&format!("{word}-left"), &WordLeft, e),
        b(&format!("{word}-right"), &WordRight, e),
        b("home", &Home, e),
        b("end", &End, e),
        b("secondary-home", &DocStart, e),
        b("secondary-end", &DocEnd, e),
        b("pageup", &PageUp, e),
        b("pagedown", &PageDown, e),
        b("shift-left", &SelectLeft, e),
        b("shift-right", &SelectRight, e),
        b("shift-up", &SelectUp, e),
        b("shift-down", &SelectDown, e),
        b(&format!("{word}-shift-left"), &SelectWordLeft, e),
        b(&format!("{word}-shift-right"), &SelectWordRight, e),
        b("shift-home", &SelectHome, e),
        b("shift-end", &SelectEnd, e),
        b("secondary-shift-home", &SelectDocStart, e),
        b("secondary-shift-end", &SelectDocEnd, e),
        b("secondary-a", &SelectAll, e),
        // Search in the note: the bar has its own context as long as it receives input.
        b("secondary-f", &Find, e),
        b("secondary-f", &Find, f),
        b("secondary-h", &FindReplace, e),
        b("secondary-h", &FindReplace, f),
        b("f3", &FindNext, e),
        b("shift-f3", &FindPrev, e),
        b("f3", &FindNext, f),
        b("shift-f3", &FindPrev, f),
        b("enter", &FindEnter, f),
        b("shift-enter", &FindPrev, f),
        b("secondary-enter", &ReplaceAll, f),
        b("secondary-shift-enter", &ReplaceInVault, f),
        b("escape", &FindClose, f),
        b("backspace", &FindErase, f),
        b("tab", &FindSwitch, f),
        // Also when the bar is open but input has gone back to the note.
        b("alt-c", &FindCase, f),
        b("alt-w", &FindWord, f),
        b("alt-r", &FindRegex, f),
        b("alt-r", &FindRegex, e),
        b("alt-c", &FindCase, e),
        b("alt-w", &FindWord, e),
        b("secondary-v", &FindPaste, f),
        b("enter", &Newline, e),
        b("tab", &Indent, e),
        b("shift-tab", &Outdent, e),
        b("secondary-enter", &ToggleTask, e),
        // The four direct choices have no key: the menu and the palette offer them, and the help grants them.
        b("secondary-shift-b", &ListCycle, e),
        // A table: move the row up or down, delete it; the rest via the menu, the palette or the help.
        // Outside a table, Alt+arrow moves the cursor's block up or down; in a table, its row.
        b("alt-up", &BlockUp, e),
        b("alt-up", &TableMoveRowUp, sh),
        b("alt-down", &TableMoveRowDown, sh),
        b("alt-down", &BlockDown, e),
        b("secondary-shift-backspace", &TableDeleteSpan, e),
        b("secondary-shift-l", &AlignLeft, e),
        b("secondary-shift-e", &AlignCenter, e),
        b("secondary-shift-r", &AlignRight, e),
        b("secondary-b", &Bold, e),
        b("secondary-i", &Italic, e),
        b("secondary-u", &Underline, e),
        b("secondary-shift-x", &Strike, e),
        b("secondary-shift-h", &Highlight, e),
        b("secondary-shift-k", &InlineCode, e),
        b("secondary-k", &InsertLink, e),
        b("secondary-d", &SelectNext, e),
        b("secondary-shift-m", &Comment, e),
        b("secondary-c", &Copy, e),
        b("secondary-x", &Cut, e),
        b("secondary-v", &Paste, e),
        b("secondary-z", &Undo, e),
        b("secondary-shift-z", &Redo, e),
        b("secondary-y", &Redo, e),
        // A gesture made on the kanban board is undone from the board.
        b("secondary-z", &Undo, Some("Board")),
        b("secondary-shift-z", &Redo, Some("Board")),
        b("secondary-y", &Redo, Some("Board")),
        b("escape", &Cancel, e),
    ];
    // After the fields that use them (palette, grid): its keys take precedence over theirs.
    all.extend(line::bindings(word));
    // macOS conventions: Cmd+arrows for line and document ends.
    #[cfg(target_os = "macos")]
    all.extend([
        b("cmd-left", &Home, e),
        b("cmd-right", &End, e),
        b("cmd-up", &DocStart, e),
        b("cmd-down", &DocEnd, e),
        b("cmd-shift-left", &SelectHome, e),
        b("cmd-shift-right", &SelectEnd, e),
        b("cmd-shift-up", &SelectDocStart, e),
        b("cmd-shift-down", &SelectDocEnd, e),
    ]);
    all
}

/// Replaces all of the app's bindings with these.
pub(super) fn install(cx: &mut App, binds: &[keys::Bind]) {
    let binds: Vec<KeyBinding> = binds
        .iter()
        .filter_map(|bind| {
            let action = cx.build_action(bind.action, None).ok()?;
            let context = bind.context.map(|context| gpui::KeyBindingContextPredicate::parse(context).unwrap().into());
            KeyBinding::load(&bind.keys, action, context, false, None, &gpui::DummyKeyboardMapper).ok()
        })
        .collect();
    cx.clear_key_bindings();
    cx.bind_keys(binds);
}

pub(super) fn bind_keys(cx: &mut App) {
    install(cx, &defaults());
}
