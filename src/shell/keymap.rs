//! The default shortcuts, the help sections that list them, and their installation.

use super::*;

/// Content of the help panel, and with it the list of shortcuts that can be changed: one `k`
/// line per action, whose keys are the ones in force (`bound`); one `t` line for what
/// is not a binding (mouse, typed text, palette command, fixed keys).
pub(super) fn help_sections(bound: &[keys::Bind], changes: &keys::Changes) -> keys::Sections {
    use gpui::Action;
    use keys::Row;
    // The line of an action comes from the command table: its text, and the keys in force.
    let table = command_table::specs();
    let row = |action: &'static str| {
        let effect = table.iter().find(|spec| spec.action == action).and_then(|spec| spec.help).expect("an action of the help has no help text");
        Row { keys: keys::shown(bound, action), effect, action: Some(action), changed: changes.contains_key(action) }
    };
    macro_rules! k {
        ($action:ty) => {
            row(<$action as Action>::name_for_type())
        };
    }
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
                k!(OpenPalette),
                k!(ToggleCommands),
                k!(editor::Find),
                k!(editor::FindReplace),
                k!(SearchVault),
                k!(editor::FindNext),
                k!(editor::FindPrev),
                k!(editor::FindCase),
                k!(editor::FindWord),
                k!(editor::FindRegex),
                t("Tab", tr("Search bar: replace field", "Barre de recherche : champ de remplacement")),
                k!(editor::ReplaceAll),
                k!(editor::ReplaceInVault),
                k!(Outline),
                k!(NewNote),
                k!(NewFromTemplate),
                p(tr("template", "modèle"), tr("Insert a template at the cursor, or make a template of the open note", "Insérer un modèle au curseur, ou faire un modèle de la note ouverte")),
                k!(Today),
                p("capture", tr("Quick capture: sets up a shortcut of your desktop (Super+Shift+N on GNOME) that opens a one-line window anywhere; what you type goes to today's note", "Capture rapide : crée un raccourci du bureau (Super+Maj+N sous GNOME) qui ouvre partout une fenêtre d'une ligne ; ce qu'on y tape va dans la note du jour")),
                k!(OpenVault),
                k!(palette::RemoveChoice),
                k!(palette::ChoiceUp),
                k!(palette::ChoiceDown),
                p(tr("name this vault", "nommer ce coffre"), tr("Name this vault: a short name for the switcher", "Nommer ce coffre : un nom court pour la liste des coffres")),
                p(tr("colour", "couleur"), tr("Colour of this vault: a band at the top of the rail, to see at a glance where you write", "Couleur de ce coffre : une bande en haut du rail, pour voir d'un coup d'œil où vous écrivez")),
                p("terminal", tr("Open a terminal in the vault", "Ouvrir un terminal dans le coffre")),
                k!(SyncVault),
                p("clone", tr("Clone a vault from a git address (also on the welcome screen)", "Cloner un coffre depuis une adresse git (aussi sur l'écran d'accueil)")),
                t(tr("A cloud folder", "Un dossier synchronisé"), tr("iCloud, Dropbox, Google Drive, Syncthing: put the vault there, nothing else; never a git repository inside one", "iCloud, Dropbox, Google Drive, Syncthing : y poser le coffre, rien d'autre ; jamais un dépôt git dans un tel dossier")),
                k!(ShareDocument),
                k!(CopyAll),
                k!(ExportPdf),
                t(&format!("{MOD}+{click}"), tr("Open a [[link]], #tag or URL", "Ouvrir un [[lien]], #tag ou URL")),
                k!(ToggleHelp),
                k!(Quit),
            ],
        ),
        (
            "Navigation",
            vec![
                k!(GoBack),
                k!(GoForward),
                k!(SplitPane),
                k!(SplitDown),
                k!(PaneLeft),
                k!(PaneRight),
                k!(PaneUp),
                k!(PaneDown),
                k!(FocusLeft),
                k!(FocusRight),
                k!(TabToLeft),
                k!(TabToRight),
                k!(TabToUp),
                k!(TabToDown),
                k!(EvenPanes),
                k!(MaximizePane),
                k!(MoveTool),
                k!(ToolAsTab),
                t(tr("Shift+click on the rail, title of a view", "Maj+clic sur le rail, titre d'une vue"), tr("Shift+click stacks the view under those shown in its dock; drag its title to another place in a dock, or to the right edge to open the right dock. Escape lets go", "Maj+clic empile la vue sous celles montrées dans son dock ; glisser son titre vers une autre place d'un dock, ou vers le bord droit pour ouvrir le dock de droite. Échap lâche")),
                t(tr("Dragging a tab", "Glisser un onglet"), tr("Onto a tab bar: it moves there; onto the middle of a pane: it joins it; near an edge of a pane: a new pane opens there with it. A file from the tree does the same. Escape lets go without moving anything", "Sur une barre d'onglets : il s'y déplace ; au milieu d'un volet : il le rejoint ; près d'un bord d'un volet : un nouveau volet s'y ouvre avec lui. Un fichier de l'arbre fait de même. Échap lâche sans rien déplacer")),
                t(tr("Border between panes", "Bordure entre volets"), tr("Drag it to share the room; double click: even shares", "La glisser pour partager la place ; double-clic : parts égales")),
                t(tr("Opening a file", "Ouvrir un fichier"), tr("Click in the tree, choose in the palette, follow a link: the file opens in a tab of its own, or its tab comes to the front if it is already open; the arrows in the tree preview notes in a single italic tab", "Clic dans l'arbre, choix dans la palette, lien suivi : le fichier s'ouvre dans son propre onglet, ou son onglet passe devant s'il est déjà ouvert ; les flèches dans l'arbre montrent les notes en aperçu dans un seul onglet en italique")),
                k!(ClosePane),
                k!(CloseOtherTabs),
                k!(CloseTabsInPane),
                k!(CloseAllFiles),
                t(tr("Right click on a tab", "Clic droit sur un onglet"), tr("Close it, close the other tabs, or all the tabs of its pane", "Le fermer, fermer les autres onglets, ou tous ceux de son volet")),
                k!(ReopenTab),
                k!(NextTab),
                k!(PrevTab),
                k!(MoveTabLeft),
                k!(MoveTabRight),
                t(tr("Middle click", "Clic du milieu"), tr("On a tab: close it", "Sur un onglet : le fermer")),
                k!(palette::ConfirmAside),
                t(&format!("{MOD}+{shift}+{click}"), tr("Open a [[link]] in the other pane", "Ouvrir un [[lien]] dans l'autre volet")),
                t(tr("Drag from the tree", "Glisser depuis l'arbre"), tr("Onto a pane: open the file there; onto the right half of a single pane: open a second one", "Sur un volet : y ouvrir le fichier ; sur la moitié droite du seul volet : en ouvrir un second")),
                k!(nav::ShowTree),
                t(tr("Current file", "Fichier affiché"), tr("The tree marks the file shown in the active tab and pane, including diagrams, pictures and tables; selection stays separate. The graph marks it when it has a node. Backlinks stay on the last note", "L'arbre marque le fichier du volet et de l'onglet actifs, y compris les schémas, images et tableaux ; la sélection reste distincte. Le graphe le marque s'il a un nœud. Les rétroliens restent sur la dernière note")),
                k!(nav::ShowRecent),
                k!(nav::ShowGraph),
                k!(nav::ShowTags),
                k!(nav::ShowLinks),
                k!(nav::ShowCalendar),
                t(tr("A picture", "Une image"), tr("Shown in place of the note; a square in the graph", "Affichée à la place de la note ; un carré dans le graphe")),
                t(tr("A .csv or .tsv", "Un .csv ou .tsv"), tr("A grid in place of the note; not in the graph", "Une grille à la place de la note ; absent du graphe")),
                k!(nav::ToggleFull),
                t(tr("Arrows / Tab", "Flèches / Tab"), tr("Select and preview / linked notes (graph)", "Sélectionner en aperçu / notes liées (graphe)")),
                t(tr("Enter / Esc", "Entrée / Échap"), tr("Open the note / back to the note", "Ouvrir la note / revenir à la note")),
                k!(nav::NewFolder),
                t(tr("Point at an icon", "Pointer une icône"), tr("Its name and its shortcut", "Son nom et son raccourci")),
                t("Alt + drag", tr("Move the window from anywhere (Linux)", "Déplacer la fenêtre depuis n'importe où (Linux)")),
                k!(nav::Rename),
                k!(nav::Duplicate),
                k!(nav::Trash),
                t(&format!("{MOD}+{click} / {shift}+{click}"), tr("Select several rows: move, duplicate, trash them together", "Sélectionner plusieurs lignes : les déplacer, dupliquer, jeter ensemble")),
                t(tr("Right click", "Clic droit"), tr("Copy the link or the path, set an icon, show a folder or a tag as a list…", "Copier le lien ou le chemin, donner une icône, afficher un dossier ou un tag en liste…")),
                k!(FocusNext),
                k!(FocusPrev),
                k!(ContextMenu),
                t(tr("In a list", "Dans une liste"), tr("Type to filter (key:text for one column), click a cell to rewrite it", "Taper pour filtrer (clé:texte pour une colonne), cliquer une cellule pour la réécrire")),
                t(tr("Drag a node", "Glisser un nœud"), tr("Move it in the graph, linked notes follow", "Le déplacer dans le graphe, les notes liées suivent")),
            ],
        ),
        (
            tr("Diagrams", "Schémas"),
            vec![
                k!(NewDiagram),
                t(tr("Dock, bar over the selection", "Dock, barre sur la sélection"), tr("The dock at the bottom: select, shapes, arrows and relationships (K), text, library (S), with their names; select something and a bar floats over it: colors, fill, dashes, the relationship of an arrow, its heads, route and texts, duplicate, delete", "Le dock en bas : sélection, formes, flèches et relations (K), texte, bibliothèque (S), avec leurs noms ; sélectionnez quelque chose et une barre flotte au-dessus : couleurs, remplissage, pointillés, relation d'une flèche, ses extrémités, son tracé et ses textes, dupliquer, supprimer")),
                t("1 … 6", tr("The colour of the selection, as the palette gives them; the menus (arrow keys, Enter) and the library are walked with the keys", "La couleur de la sélection, comme la palette ; les menus (flèches, Entrée) et la bibliothèque se parcourent au clavier")),
                t(tr("Handles, right click", "Poignées, clic droit"), tr("Point at a shape: four handles, drag one to another shape to draw an arrow; a right click opens the menu of what is under it (write in it, turn an arrow around, route, duplicate, delete)", "Pointer une forme : quatre poignées, en tirer une vers une autre forme dessine une flèche ; un clic droit ouvre le menu de ce qui est dessous (écrire dedans, retourner une flèche, tracé, dupliquer, supprimer)")),
                t("R U O D C P N T", tr("Rectangle, rounded, ellipse, diamond, cylinder, person, note, text", "Rectangle, arrondi, ellipse, losange, cylindre, personnage, note, texte")),
                t(tr("Selected shape, loop button", "Forme sélectionnée, bouton boucle"), tr("A transition from a state to itself, its label written at once; `trigger [guard] / effect`, the guard is in italics", "Une transition d'un état vers lui-même, son étiquette écrite aussitôt ; `déclencheur [garde] / effet`, la garde est en italique")),
                t(tr("Sequence: arrows between lifelines", "Séquence : flèches entre lignes de vie"), tr("A message's height follows its order: drag it up or down to reorder; a call that waits opens a bar until its answer; the bar of the bar numbers them; a fragment (S: alt, opt, loop, par…) picked with messages selected wraps them, the first line of its text is the operator, each line after it an operand", "La hauteur d'un message suit son ordre : le glisser en haut ou en bas le déplace ; un appel qui attend ouvre une barre jusqu'à sa réponse ; le bouton de la barre les numérote ; un fragment (S : alt, opt, loop, par…) choisi avec des messages sélectionnés les enveloppe, la première ligne de son texte est l'opérateur, chaque ligne suivante un opérande")),
                t(tr("Activity (S): actions, decisions, bars, pins", "Activité (S) : actions, décisions, barres, broches"), tr("A pin dropped on the border of a shape sits on it, follows it and is removed with it; swimlanes (G F W) are columns or rows; the guard of a flow is its text", "Une broche déposée sur le bord d'une forme s'y pose, la suit et part avec elle ; les couloirs (G F W) sont des colonnes ou des lignes ; la garde d'un flux est son texte")),
                t(tr("Components (S): components, ports, nodes", "Composants (S) : composants, ports, nœuds"), tr("A port sits on the border of a component and goes with it; a node, a device and an execution environment hold the artifacts and nodes dropped on them; the ball and the socket (K B, K Q) are its interfaces", "Un port se pose sur le bord d'un composant et part avec lui ; un nœud, un appareil et un environnement d'exécution contiennent les artefacts et nœuds déposés dessus ; la boule et la prise (K B, K Q) sont ses interfaces")),
                t("G F W", tr("Package, frame, swimlane: they hold the shapes dropped on them or drawn around, and move, copy and remove them together", "Paquetage, cadre, couloir : ils contiennent les formes déposées dessus ou autour desquelles on les trace, et les déplacent, copient et retirent avec eux")),
                t("S", tr("Shape library: classes, use cases… with their text; type to search, Enter picks, drag a row onto the canvas; starting points open a new diagram", "Bibliothèque de formes : classes, cas d'utilisation… avec leur texte ; taper pour chercher, Entrée choisit, glisser une ligne sur le canevas ; les points de départ ouvrent un nouveau schéma")),
                t("K", tr("Relationships: a letter picks one (A N X G C H Z D, U I S L E M for a dependency with its stereotype, R association class, B ball, Q socket, J V «deploy» «manifest», F O W P T the messages of a sequence diagram, Y n-ary association); on selected arrows it changes them", "Relations : une lettre en choisit une (A N X G C H Z D, U I S L E M pour une dépendance avec son stéréotype, K classe d'association, B boule, Q prise, J V «deploy» «manifest», F O W P T les messages d'un diagramme de séquence, Y association n-aire) ; sur des flèches sélectionnées, elle les change")),
                t("A / L / V", tr("Arrow / line, held by the shapes they join / select", "Flèche / trait, accrochés aux formes reliées / sélection")),
                t(tr("Enter, double click", "Entrée, double-clic"), tr("Write in the shape or on the arrow; Esc when done", "Écrire dans la forme ou sur la flèche ; Échap pour finir")),
                t(tr("While writing", "En écrivant"), tr("Click to place the cursor, drag or Shift+click to select, double click a word; arrows, words, Home, End, select all, copy, cut and paste act on the text; a click elsewhere ends it", "Cliquer place le curseur, glisser ou Maj+clic sélectionne, double-clic prend un mot ; flèches, mots, Début, Fin, tout sélectionner, copier, couper et coller agissent sur le texte ; un clic ailleurs le termine")),
                k!(canvas::SelectUp),
                k!(canvas::SelectDown),
                k!(canvas::NextText),
                t("---", tr("Alone on a line of a box: a compartment (UML class)", "Seul sur une ligne d'une boîte : un compartiment (classe UML)")),
                t("«interface»  + static x: int  abstract f()", tr("In a class: a stereotype line, then members; static is underlined, abstract in italics; name : Type is an object", "Dans une classe : une ligne de stéréotype, puis des membres ; static est souligné, abstract en italique ; nom : Type est un objet")),
                t(&format!("{shift}+{click} / {MOD}+{click}"), tr("Add a shape to the selection, or take it out", "Ajouter une forme à la sélection, ou l'en retirer")),
                t(&format!("Alt+{click}"), tr("Select what lies under the selection at that spot, then round to the top: shapes stacked, an arrow under a filled shape", "Sélectionner ce qui est sous la sélection à cet endroit, puis revenir au-dessus : formes empilées, flèche sous une forme remplie")),
                t(tr("Delete", "Suppr"), tr("Remove the selection", "Retirer la sélection")),
                k!(canvas::Duplicate),
                k!(canvas::Undo),
                k!(canvas::Redo),
                t(tr("Edge of a shape", "Bord d'une forme"), tr("An arrow end dropped there stays there; in the middle, it follows the other end", "Un bout de flèche lâché là y reste ; au milieu, il suit l'autre bout")),
                t(tr("Wheel / + - 0", "Molette / + - 0"), tr("Move the view / zoom in, out, fit all", "Déplacer la vue / zoomer, dézoomer, tout cadrer")),
                t("![](Schéma.svg)", tr("Show the diagram in a note", "Afficher le schéma dans une note")),
                t(".svg", tr("An SVG from elsewhere (draw.io, Mermaid.js…) opens in a viewer: drag or wheel to move, + - 0 to zoom", "Un SVG d'ailleurs (draw.io, Mermaid.js…) s'ouvre dans une visionneuse : glisser ou molette pour se déplacer, + - 0 pour zoomer")),
                t(tr("Picture, formula, diagram", "Image, formule, schéma"), tr("Point at one in a note: a button at its corner shows it full size, to zoom and move around (Esc goes back)", "Pointer l'un d'eux dans une note : un bouton à son coin l'affiche en grand, pour zoomer et se déplacer (Échap revient)")),
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
                k!(sheet::Copy),
                k!(sheet::Cut),
                k!(sheet::Paste),
                k!(sheet::SelectRow),
                k!(sheet::SelectCol),
                t(tr("Edge of a header", "Bord d'un en-tête"), tr("Drag it to set the column's width, kept for the file; double-click fits the column to its text; Esc puts it back", "Le tirer règle la largeur de la colonne, retenue pour le fichier ; un double-clic l'ajuste à son texte ; Échap la remet")),
                k!(sheet::WidenColumn),
                k!(sheet::NarrowColumn),
                k!(sheet::FitColumn),
                k!(sheet::ResetColumnWidths),
                k!(sheet::InsertBelow),
                k!(sheet::InsertAbove),
                k!(sheet::DeleteRows),
                k!(sheet::Undo),
                k!(sheet::Redo),
                p("table", tr("Delimiter, encoding, header row, copy as…, export (CSV, TSV, Markdown, JSON)", "Délimiteur, encodage, en-tête, copier en…, exporter (CSV, TSV, Markdown, JSON)")),
            ],
        ),
        (
            tr("Appearance", "Apparence"),
            vec![
                k!(OpenSettings),
                p(tr("theme", "thème"), tr("Theme, previewed as you browse", "Thème, en aperçu pendant le choix")),
                p(tr("font", "police"), tr("Font of the app / of the code", "Police de l'app / du code")),
                p(tr("page width", "largeur de page"), tr("Width of the text column: narrow, normal, wide, full", "Largeur de la colonne de texte : étroite, normale, large, pleine")),
                p(tr("wide tables", "grands tableaux"), tr("A table wider than the text: in the column, or centered and wider", "Un tableau plus large que le texte : dans la colonne, ou centré et plus large")),
                p(tr("animations", "animations"), tr("Short fades (about 0.13 s) when the panel, the formatting bar or a folder opens; none on typing, scrolling or hover. Off: everything is instant (motion=off in settings)", "Courts fondus (environ 0,13 s) à l'ouverture du panneau, de la barre de mise en forme ou d'un dossier ; rien à la frappe, au défilement ni au survol. Désactivées : tout est instantané (motion=off dans settings)")),
                k!(ZoomIn),
                k!(ZoomOut),
                k!(ZoomReset),
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
            .chain(editor::TableOp::ALL.into_iter().map(|op| row(op.action().name())))
            .chain([
                k!(editor::TableDeleteSpan),
                t(
                    tr("Formulas", "Formules"),
                    tr(
                        "A cell that starts with = computes: =B*C, =sum(above), =avg(B1:B9), =round(B/C, 2), =if(B>10, 1, 0). Columns are letters, rows are numbers from the first row under the header (C2, B1:B10); B alone is column B of this row; above, left and column are ranges. Functions: sum avg min max count round abs if. The cell shows its result (right-aligned, a different color); the formula shows when the cursor is in it, and the file keeps it. A cell with no result shows #ERR: point at it for the reason. Completion offers functions, ranges and columns after =",
                        "Une cellule qui commence par = calcule : =B*C, =sum(above), =avg(B1:B9), =round(B/C, 2), =if(B>10, 1, 0). Les colonnes sont des lettres, les lignes des numéros depuis la première ligne sous l'en-tête (C2, B1:B10) ; B seul est la colonne B de cette ligne ; above, left et column sont des plages. Fonctions : sum avg min max count round abs if. La cellule montre son résultat (à droite, d'une autre couleur) ; la formule apparaît quand le curseur y est, et le fichier la garde. Une cellule sans résultat montre #ERR : pointez-la pour la raison. La complétion propose fonctions, plages et colonnes après =",
                    ),
                ),
                k!(editor::TableAutoSum),
                k!(editor::TableCopyResults),
            ])
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
                t(tr("Board: arrows", "Tableau : flèches"), tr("Go from card to card, to a column's title, to Add a card or Add a column; Space ticks the card", "Aller de carte en carte, au titre d'une colonne, à Ajouter une carte ou une colonne ; Espace coche la carte")),
                k!(kanban::CardEdit),
                k!(kanban::CardRemove),
                k!(kanban::CarryUp),
                k!(kanban::CarryDown),
                k!(kanban::CarryLeft),
                k!(kanban::CarryRight),
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
                k!(editor::AlignLeft),
                k!(editor::AlignCenter),
                k!(editor::AlignRight),
                t(tr("Pasting a table", "Coller un tableau"), tr("Spreadsheet cells or Markdown fill the grid, or become a table", "Des cellules de tableur ou du Markdown remplissent la grille, ou deviennent un tableau")),
                k!(editor::ListCycle),
                k!(editor::ListBullet),
                k!(editor::ListOrdered),
                k!(editor::ListTask),
                k!(editor::ListText),
                k!(editor::ToggleTask),
                k!(editor::Bold),
                k!(editor::Italic),
                k!(editor::Underline),
                k!(editor::Strike),
                k!(editor::Highlight),
                k!(editor::InlineCode),
                p(tr("formatting bar", "barre de mise en forme"), tr("Under the note, centered: bold, italic, underline, strikethrough, highlight (five colors), code, link, lists, tasks, table (in a table: add or delete a row or a column); a lit button means the selection has the style, click again to remove it; only the active pane shows it; in a narrow note it folds into a button at the bottom, in the middle, that opens it; hide it with the palette", "Sous la note, au centre : gras, italique, souligné, barré, surligné (cinq couleurs), code, lien, listes, tâches, tableau (dans un tableau : ajouter ou supprimer une ligne ou une colonne) ; un bouton allumé : la sélection a le style, un second clic le retire ; seul le pane actif la montre ; dans une note étroite, elle se replie en un bouton en bas, au centre, qui l'ouvre ; masquable par la palette")),
                k!(editor::SelectNext),
                t(&format!("Alt+{click}"), tr("Add a cursor", "Ajouter un curseur")),
                k!(editor::InsertLink),
                t(
                    tr("Embed a section", "Incorporer une section"),
                    tr(
                        "![[Note#Heading]] alone on a line shows that heading and what is under it, up to the next heading of the same level, in place and live: ![[Note]] the whole note (without its front matter), ![[Note#^id]] the block that ends with ^id. Framed, with the source's name at the top right: a click on it, or a double click on the frame, opens the note there (Alt+Left comes back). Typing ![[Note# offers the headings of the note, #^ its blocks. Read-only; an embed that leads back to itself says so, and three levels are shown at most. Renaming the note or the heading rewrites the embeds and links to it",
                        "![[Note#Titre]] seul sur une ligne montre ce titre et ce qu'il y a dessous, jusqu'au titre suivant de même niveau, sur place et en direct : ![[Note]] toute la note (sans son en-tête), ![[Note#^id]] le bloc qui finit par ^id. Dans un cadre, avec le nom de la source en haut à droite : un clic dessus, ou un double clic sur le cadre, ouvre la note à cet endroit (Alt+Gauche revient). Taper ![[Note# propose les titres de la note, #^ ses blocs. Lecture seule ; une incorporation qui revient sur elle-même le dit, et trois niveaux s'affichent au plus. Renommer la note ou le titre réécrit les incorporations et les liens qui y mènent",
                    ),
                ),
                k!(editor::OpenEmbed),
                k!(editor::Comment),
                p(tr("comments", "commentaires"), tr("Comments: show their panel, jump to one (a badge on the speech bubble counts them)", "Commentaires : montrer leur panneau, aller à l'un d'eux (une pastille sur la bulle les compte)")),
                t(tr("Right click", "Clic droit"), tr("Menu of the note: link, comment, cut, copy, paste, bold, italic", "Menu de la note : lien, commentaire, couper, copier, coller, gras, italique")),
                k!(editor::Copy),
                k!(editor::Cut),
                k!(editor::Paste),
                k!(editor::SelectAll),
                k!(editor::Undo),
                k!(editor::Redo),
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
            .chain(editor::BlockOp::ALL.into_iter().map(|op| row(op.action().name())))
            .collect(),
        ),
    ]
}

/// The actions whose shortcut can be changed: those the table marks, which have their line in
/// the help.
pub(super) fn editable() -> std::collections::HashSet<&'static str> {
    command_table::specs().into_iter().filter(|spec| spec.editable).map(|spec| spec.action).collect()
}

/// The default shortcut table (derived from the command table; see `command_table::bindings`).
pub(super) fn defaults() -> Vec<keys::Bind> {
    let mut all = command_table::bindings();
    // After the fields that use them (palette, grid): its keys take precedence over theirs.
    all.extend(line::bindings(if cfg!(target_os = "macos") { "alt" } else { "ctrl" }));
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
