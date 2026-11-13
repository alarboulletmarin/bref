//! The settings page: every choice of the app in one tab, in sections, each with its current value,
//! applied at once and saved in the `settings` file as the palette commands do. The palette commands
//! stay: the page is the map, the palette the shortcut.

use super::*;
use crate::ui::widgets::{self, card, choice, switch, text_button};

/// How the settings tab is written in the saved pane layout (as `@graph` for a tool).
pub(super) const TAB: &str = "@settings";

/// Under this width, the sections go above the settings as a row of pills instead of a column beside them.
const NARROW: f32 = 560.;

/// The sections, in the order of the column on the left.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Section {
    Appearance,
    Editor,
    Files,
    Sync,
    Agents,
    Shortcuts,
    About,
}

impl Section {
    pub(super) const ALL: [Section; 7] = [Section::Appearance, Section::Editor, Section::Files, Section::Sync, Section::Agents, Section::Shortcuts, Section::About];

    pub(super) fn label(self) -> &'static str {
        match self {
            Section::Appearance => tr("Appearance", "Apparence"),
            Section::Editor => tr("Editor", "Éditeur"),
            Section::Files => tr("Files & links", "Fichiers et liens"),
            Section::Sync => tr("Sync & backup", "Synchro et sauvegarde"),
            Section::Agents => tr("Agents", "Agents"),
            Section::Shortcuts => tr("Shortcuts", "Raccourcis"),
            Section::About => tr("Updates & about", "Mises à jour et à propos"),
        }
    }

    /// Its name for the debug selectors (`settings-section-files`).
    pub(super) fn key(self) -> &'static str {
        match self {
            Section::Appearance => "appearance",
            Section::Editor => "editor",
            Section::Files => "files",
            Section::Sync => "sync",
            Section::Agents => "agents",
            Section::Shortcuts => "shortcuts",
            Section::About => "about",
        }
    }
}

/// One setting: a name, a line saying what it changes when it is not obvious, and its control.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Row {
    Theme,
    Font,
    Mono,
    Size,
    Motion,
    Page,
    Wide,
    Bar,
    Vault,
    NewNotes,
    Daily,
    Trash,
    Config,
    Git,
    Backup,
    Server,
    Access,
    Client,
    Claude,
    Keys,
    Version,
    Updates,
    Links,
}

impl Row {
    /// Every row, section by section, in the order they are shown: Up and Down walk this list.
    pub(super) const ALL: [Row; 23] = [
        Row::Theme,
        Row::Font,
        Row::Mono,
        Row::Size,
        Row::Motion,
        Row::Page,
        Row::Wide,
        Row::Bar,
        Row::Vault,
        Row::NewNotes,
        Row::Daily,
        Row::Trash,
        Row::Config,
        Row::Git,
        Row::Backup,
        Row::Server,
        Row::Access,
        Row::Client,
        Row::Claude,
        Row::Keys,
        Row::Version,
        Row::Updates,
        Row::Links,
    ];

    pub(super) fn section(self) -> Section {
        match self {
            Row::Theme | Row::Font | Row::Mono | Row::Size | Row::Motion => Section::Appearance,
            Row::Page | Row::Wide | Row::Bar => Section::Editor,
            Row::Vault | Row::NewNotes | Row::Daily | Row::Trash | Row::Config => Section::Files,
            Row::Git | Row::Backup => Section::Sync,
            Row::Server | Row::Access | Row::Client | Row::Claude => Section::Agents,
            Row::Keys => Section::Shortcuts,
            Row::Version | Row::Updates | Row::Links => Section::About,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Row::Theme => tr("Theme", "Thème"),
            Row::Font => tr("App font", "Police de l'app"),
            Row::Mono => tr("Code font", "Police du code"),
            Row::Size => tr("Text size", "Taille du texte"),
            Row::Motion => tr("Animations", "Animations"),
            Row::Page => tr("Page width", "Largeur de page"),
            Row::Wide => tr("Wide tables", "Grands tableaux"),
            Row::Bar => tr("Formatting bar", "Barre de mise en forme"),
            Row::Vault => tr("Vault", "Coffre"),
            Row::NewNotes => tr("New notes", "Nouvelles notes"),
            Row::Daily => tr("Daily notes", "Notes du jour"),
            Row::Trash => tr("Trash", "Corbeille"),
            Row::Config => tr("Configuration folder", "Dossier de configuration"),
            Row::Git => tr("Git", "Git"),
            Row::Backup => tr("Backup", "Sauvegarde"),
            Row::Server => tr("Agents' server", "Serveur des agents"),
            Row::Access => tr("Access", "Accès"),
            Row::Client => tr("Claude Desktop, Cursor, VS Code…", "Claude Desktop, Cursor, VS Code…"),
            Row::Claude => tr("Claude Code", "Claude Code"),
            Row::Keys => tr("Keyboard shortcuts", "Raccourcis clavier"),
            Row::Version => tr("Version", "Version"),
            Row::Updates => tr("Check for updates", "Chercher les mises à jour"),
            Row::Links => tr("Guide and links", "Guide et liens"),
        }
    }

    /// One line on what the setting changes; none where the name and the value say it all.
    pub(super) fn line(self) -> Option<&'static str> {
        Some(match self {
            Row::Theme => tr("Default follows the system, light or dark.", "Par défaut suit le système, clair ou sombre."),
            Row::Font | Row::Mono => return None,
            Row::Size => tr("The panel, menus and help follow, from 85 % to 150 %.", "Le panneau, les menus et l'aide suivent, de 85 % à 150 %."),
            Row::Motion => tr("Short fades when the panel, the formatting bar or a folder opens. Off: everything is instant.", "Courts fondus à l'ouverture du panneau, de la barre de mise en forme ou d'un dossier. Désactivées : tout est instantané."),
            Row::Page => tr("How wide the column of text is.", "La largeur de la colonne de texte."),
            Row::Wide => tr("A table wider than the text: centered and wider, or kept in the column with a scroll.", "Un tableau plus large que le texte : centré et plus large, ou gardé dans la colonne avec un défilement."),
            Row::Bar => tr("The bar of styles under the note.", "La barre des styles sous la note."),
            Row::Vault => tr("The folder of your notes, plain Markdown files.", "Le dossier de vos notes, de simples fichiers Markdown."),
            Row::NewNotes => tr("In the folder selected in the tree, otherwise at the root of the vault.", "Dans le dossier choisi dans l'arbre, sinon à la racine du coffre."),
            Row::Daily => tr("Named by the date, at the root; born from a template named Daily or Journal if there is one.", "Nommée par la date, à la racine ; née d'un modèle nommé Daily ou Journal s'il y en a un."),
            Row::Trash => tr("What you delete goes to the vault's .trash folder, never destroyed.", "Ce que vous supprimez va dans le dossier .trash du coffre, jamais détruit."),
            Row::Config => tr("Where these settings, the shortcuts and the layout are kept, one file each.", "Où sont gardés ces réglages, les raccourcis et la disposition, un fichier chacun."),
            Row::Git => tr("Commit, receive, merge and send in one step; a conflict keeps both versions.", "Commiter, recevoir, fusionner et envoyer en une fois ; un conflit garde les deux versions."),
            Row::Backup => tr("The whole vault as one .tar.gz archive, in a folder you choose.", "Tout le coffre en une archive .tar.gz, dans le dossier de votre choix."),
            Row::Server => tr("bref mcp lets an agent read and write the vault with Bref's rules: links, trash, diagrams. Nothing leaves this computer.", "bref mcp permet à un agent de lire et d'écrire le coffre avec les règles de Bref : liens, corbeille, schémas. Rien ne quitte cet ordinateur."),
            Row::Access => tr("Read only: the agent cannot write, rename or trash anything.", "Lecture seule : l'agent ne peut rien écrire, renommer ni mettre à la corbeille."),
            Row::Client => tr("Add this to the client's configuration file, next to its other servers.", "Ajoutez ceci au fichier de configuration du client, à côté de ses autres serveurs."),
            Row::Claude => tr("Run this once in a terminal.", "Lancez ceci une fois dans un terminal."),
            Row::Keys => tr("Every shortcut, and giving any of them other keys.", "Tous les raccourcis, et leur donner d'autres touches."),
            Row::Version => return None,
            Row::Updates => tr("Once a day, through GitHub; nothing is installed without asking.", "Une fois par jour, par GitHub ; rien n'est installé sans demander."),
            Row::Links => return None,
        })
    }

    /// Its name for the debug selectors (`settings-row-theme`).
    pub(super) fn key(self) -> &'static str {
        match self {
            Row::Theme => "theme",
            Row::Font => "font",
            Row::Mono => "mono",
            Row::Size => "size",
            Row::Motion => "motion",
            Row::Page => "page",
            Row::Wide => "wide",
            Row::Bar => "bar",
            Row::Vault => "vault",
            Row::NewNotes => "new-notes",
            Row::Daily => "daily",
            Row::Trash => "trash",
            Row::Config => "config",
            Row::Git => "git",
            Row::Backup => "backup",
            Row::Server => "server",
            Row::Access => "access",
            Row::Client => "client",
            Row::Claude => "claude",
            Row::Keys => "keys",
            Row::Version => "version",
            Row::Updates => "updates",
            Row::Links => "links",
        }
    }

    /// Whether `query` finds it: every word, in its name, its line, its section or one of its values.
    fn matches(self, query: &str) -> bool {
        let values: Vec<&str> = match self {
            Row::Theme => THEMES.iter().map(|t| t.0).collect(),
            Row::Page => Page::ALL.iter().map(|p| p.label()).collect(),
            Row::Wide => Wide::ALL.iter().map(|w| w.label()).collect(),
            Row::Server | Row::Client | Row::Claude => vec!["mcp", "json"],
            _ => Vec::new(),
        };
        let text = [self.name(), self.line().unwrap_or_default(), self.section().label()].into_iter().chain(values).collect::<Vec<_>>().join(" ");
        let text = crate::lang::fold(&text);
        crate::lang::fold(query).split_whitespace().all(|word| text.contains(word))
    }
}

/// The state of the settings page; the tab itself is `Shown::Settings`, one at most.
pub(super) struct SettingsPage {
    /// Holds the keys while the page is shown and its search field does not.
    pub(super) focus: FocusHandle,
    pub(super) find: Option<Entity<field::Field>>,
    pub(super) section: Section,
    /// The row picked with the keyboard or a click.
    pub(super) sel: Option<Row>,
    /// The picked row must be scrolled into view on the next frame.
    reveal: bool,
    /// The agents' configuration is shown for a read-only server.
    pub(super) read_only: bool,
    /// The row whose text was just copied: its button says so for a moment.
    copied: Option<Row>,
    /// The address the vault syncs with, read when the page opens (git runs once, not on every frame).
    pub(super) remote: Option<String>,
    /// Time of the last sync that succeeded in this session.
    pub(super) synced: Option<String>,
    scroll: gpui::ScrollHandle,
    /// Width of the page at the last frame: under `NARROW`, the sections go above.
    width: std::rc::Rc<std::cell::Cell<Pixels>>,
}

impl SettingsPage {
    pub(super) fn new(cx: &mut Context<Shell>) -> Self {
        SettingsPage {
            focus: cx.focus_handle(),
            find: None,
            section: Section::Appearance,
            sel: None,
            reveal: false,
            read_only: false,
            copied: None,
            remote: None,
            synced: None,
            scroll: gpui::ScrollHandle::new(),
            width: Default::default(),
        }
    }
}

/// The theme choices, in the order of the cards: the default (empty, follows the system), then each theme.
fn theme_keys() -> Vec<&'static str> {
    std::iter::once("").chain(THEMES.iter().map(|t| t.0)).collect()
}

/// The path of this program, as an agent's client must start it.
fn program() -> String {
    std::env::current_exe().map_or_else(|_| "bref".to_string(), |path| path.display().to_string())
}

/// The arguments of the server: `mcp`, and `--read-only` when asked.
fn server_args(read_only: bool) -> Vec<&'static str> {
    if read_only { vec!["mcp", "--read-only"] } else { vec!["mcp"] }
}

/// The client configuration of the server (Claude Desktop, Cursor, VS Code…): `program` started with its arguments.
pub(super) fn client_json(program: &str, read_only: bool) -> String {
    let args = server_args(read_only).iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ");
    let command = serde_json::to_string(program).unwrap_or_default();
    format!("{{\n  \"mcpServers\": {{\n    \"bref\": {{\n      \"command\": {command},\n      \"args\": [{args}]\n    }}\n  }}\n}}")
}

/// The command line that adds the server to Claude Code, for every project.
pub(super) fn claude_command(program: &str, read_only: bool) -> String {
    let program = if program.contains(' ') { format!("\"{program}\"") } else { program.to_string() };
    format!("claude mcp add --scope user bref -- {program} {}", server_args(read_only).join(" "))
}

impl Shell {
    /// Opens the settings as a tab, or brings their tab to the front; on `section` when given.
    // ponytail: without a vault (welcome page) there is no pane to hold the tab, so nothing opens;
    // the welcome page could show the page in its place if the need comes.
    pub(super) fn open_settings(&mut self, section: Option<Section>, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        // The file edited by hand since: the page shows what it says now.
        let disk = Prefs::parse(&config::load(config::File::Settings));
        if disk != self.prefs {
            self.prefs = disk;
            self.restyle(window, cx);
        }
        self.prepare_settings(window, cx);
        if let Some(section) = section {
            self.settings.section = section;
            self.settings.sel = None;
        }
        let tab = match self.panes.iter().position(Pane::settings) {
            Some(tab) => tab,
            None => {
                self.fresh_tab(window, cx);
                self.pane_mut().set_settings();
                self.active
            }
        };
        self.focus_pane(tab, window, cx);
    }

    /// What the page needs before it is shown, opened or restored: its search field, and the address the vault syncs with.
    pub(super) fn prepare_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.read_remote();
        if self.settings.find.is_none() {
            let find = cx.new(|cx| field::Field::new("", self.theme, cx).scrolling());
            cx.observe(&find, |_, _, cx| cx.notify()).detach();
            cx.subscribe_in(&find, window, |this, find, event: &field::FieldEvent, window, cx| match event {
                // Enter: to the first setting found, the keys back on the page.
                field::FieldEvent::Done => {
                    this.settings.sel = this.settings_rows(cx).first().copied();
                    this.settings.reveal = true;
                    window.focus(&this.settings.focus);
                    cx.notify();
                }
                // Escape clears the search, then gives the keys back to the page.
                field::FieldEvent::Cancel => match find.read(cx).text().is_empty() {
                    true => window.focus(&this.settings.focus),
                    false => find.update(cx, |find, cx| find.clear(cx)),
                },
                field::FieldEvent::Changed => this.settings.sel = None,
            })
            .detach();
            self.settings.find = Some(find);
        }
    }

    /// Reads the address the vault syncs with, once, off the drawing.
    pub(super) fn read_remote(&mut self) {
        self.settings.remote = self.vault.as_ref().filter(|root| root.join(".git").exists()).and_then(|root| git::remote(root));
    }

    fn settings_query(&self, cx: &App) -> String {
        self.settings.find.as_ref().map(|find| find.read(cx).text().trim().to_string()).unwrap_or_default()
    }

    /// The rows shown: those of the section, or, while searching, those found in every section.
    pub(super) fn settings_rows(&self, cx: &App) -> Vec<Row> {
        let query = self.settings_query(cx);
        Row::ALL.into_iter().filter(|row| if query.is_empty() { row.section() == self.settings.section } else { row.matches(&query) }).collect()
    }

    /// Up and Down: the next row, into the next section at the end of one; while searching, among those found.
    pub(super) fn settings_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let searching = !self.settings_query(cx).is_empty();
        let rows = if searching { self.settings_rows(cx) } else { Row::ALL.to_vec() };
        let at = self.settings.sel.and_then(|sel| rows.iter().position(|row| *row == sel));
        let first = rows.iter().position(|row| searching || row.section() == self.settings.section).unwrap_or(0);
        let next = match (at, down) {
            (None, _) => first,
            (Some(at), true) => (at + 1).min(rows.len().saturating_sub(1)),
            (Some(at), false) => at.saturating_sub(1),
        };
        if let Some(&row) = rows.get(next) {
            self.settings.sel = Some(row);
            if !searching {
                self.settings.section = row.section();
            }
            self.settings.reveal = true;
        }
        cx.notify();
    }

    /// Tab and Shift+Tab: from the search field, into the settings found; on the page, the next section
    /// (or the one before).
    pub(super) fn settings_tab(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.find.as_ref().is_some_and(|find| find.focus_handle(cx).is_focused(window)) {
            window.focus(&self.settings.focus);
            if self.settings.sel.is_none() {
                self.settings.sel = self.settings_rows(cx).first().copied();
                self.settings.reveal = true;
            }
            return cx.notify();
        }
        self.settings_section(by, cx);
    }

    /// The next section (or the one before), its first row picked.
    pub(super) fn settings_section(&mut self, by: isize, cx: &mut Context<Self>) {
        let at = Section::ALL.iter().position(|s| *s == self.settings.section).unwrap_or(0) as isize;
        let section = Section::ALL[(at + by).rem_euclid(Section::ALL.len() as isize) as usize];
        self.show_section(section, cx);
        self.settings.sel = Row::ALL.into_iter().find(|row| row.section() == section);
    }

    /// Shows `section`; a search under way is cleared, since it shows every section.
    pub(super) fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if let Some(find) = &self.settings.find {
            find.update(cx, |find, cx| find.clear(cx));
        }
        self.settings.section = section;
        self.settings.sel = None;
        self.settings.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    /// Changes the settings with `change`: applied at once, saved, the window restyled.
    fn set_prefs(&mut self, change: impl FnOnce(&mut Prefs), window: &mut Window, cx: &mut Context<Self>) {
        let mut prefs = self.prefs.clone();
        change(&mut prefs);
        let prefs = Prefs::parse(&prefs.to_text());
        if prefs != self.prefs {
            self.prefs = prefs;
            self.keep(config::File::Settings, &self.prefs.to_text(), cx);
            self.restyle(window, cx);
        }
    }

    /// Left and Right on a row of choices: the one before or after (`by` = -1 or 1), without wrapping;
    /// on a switch, off or on. `false` when the row has no such choices.
    pub(super) fn settings_shift(&mut self, row: Row, by: isize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let step = |keys: &[&'static str], now: &str| {
            let at = keys.iter().position(|k| *k == now).unwrap_or(0) as isize;
            keys[(at + by).clamp(0, keys.len() as isize - 1) as usize].to_string()
        };
        match row {
            Row::Theme => {
                let theme = step(&theme_keys(), &self.prefs.theme);
                self.set_prefs(|p| p.theme = theme, window, cx);
            }
            Row::Page => {
                let keys = Page::ALL.map(|p| p.key());
                let page = step(&keys, Page::of(&self.prefs.page).key());
                self.set_prefs(|p| p.page = page, window, cx);
            }
            Row::Wide => {
                let keys = Wide::ALL.map(|w| w.key());
                let wide = step(&keys, Wide::of(&self.prefs.wide).key());
                self.set_prefs(|p| p.wide = wide, window, cx);
            }
            Row::Size => self.resize_text(Some(by as f32), window, cx),
            Row::Bar => self.set_prefs(|p| p.bar = if by > 0 { "on" } else { "off" }.into(), window, cx),
            Row::Motion => self.set_prefs(|p| p.motion = if by > 0 { "on" } else { "off" }.into(), window, cx),
            Row::Updates => {
                if self.prefs.updates != (by > 0) {
                    self.toggle_updates(cx);
                }
            }
            Row::Access => self.settings.read_only = by < 0,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Enter or Space on a row, or a click on its control: the next choice, the other side of a
    /// switch, or what its button does.
    pub(super) fn settings_press(&mut self, row: Row, window: &mut Window, cx: &mut Context<Self>) {
        let next = |keys: &[&'static str], now: &str| {
            let at = keys.iter().position(|k| *k == now).map_or(0, |at| (at + 1) % keys.len());
            keys[at].to_string()
        };
        match row {
            Row::Theme => {
                let theme = next(&theme_keys(), &self.prefs.theme);
                self.set_prefs(|p| p.theme = theme, window, cx);
            }
            Row::Page => {
                let page = next(&Page::ALL.map(|p| p.key()), Page::of(&self.prefs.page).key());
                self.set_prefs(|p| p.page = page, window, cx);
            }
            Row::Wide => {
                let wide = next(&Wide::ALL.map(|w| w.key()), Wide::of(&self.prefs.wide).key());
                self.set_prefs(|p| p.wide = wide, window, cx);
            }
            Row::Bar => self.set_prefs(|p| p.bar = if p.bar == "off" { "on" } else { "off" }.into(), window, cx),
            Row::Motion => self.set_prefs(|p| p.motion = if p.motion == "off" { "on" } else { "off" }.into(), window, cx),
            Row::Updates => self.toggle_updates(cx),
            Row::Access => self.settings.read_only = !self.settings.read_only,
            Row::Size => self.resize_text(None, window, cx),
            Row::Font => self.choose_setting(Setting::Font, window, cx),
            Row::Mono => self.choose_setting(Setting::Mono, window, cx),
            Row::Vault => self.choose_vault(window, cx),
            Row::Trash => self.open_trash(window, cx),
            Row::Git => self.git_sync(window, cx),
            Row::Backup => self.choose_backup(window, cx),
            Row::Client | Row::Claude => self.settings_copy(row, cx),
            Row::Keys => self.set_help(true, window, cx),
            Row::Version => self.check_now(cx),
            Row::Links => cx.open_url(GUIDE),
            Row::NewNotes | Row::Daily | Row::Config | Row::Server => {}
        }
        cx.notify();
    }

    /// Copies the agents' configuration of `row`; its button says « Copied » for a moment.
    fn settings_copy(&mut self, row: Row, cx: &mut Context<Self>) {
        let text = match row {
            Row::Claude => claude_command(&program(), self.settings.read_only),
            _ => client_json(&program(), self.settings.read_only),
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.settings.copied = Some(row);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(widgets::FLASH).await;
            this.update(cx, |this, cx| {
                if this.settings.copied == Some(row) {
                    this.settings.copied = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// A key on the page itself (not in its search field). `true`: it was used.
    pub(super) fn settings_key(&mut self, stroke: &gpui::Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let m = stroke.modifiers;
        if m.control || m.platform || m.alt || m.function {
            return false;
        }
        match stroke.key.as_str() {
            "up" => self.settings_step(false, cx),
            "down" => self.settings_step(true, cx),
            "left" | "right" => {
                let Some(row) = self.settings.sel else { return false };
                return self.settings_shift(row, if stroke.key == "left" { -1 } else { 1 }, window, cx);
            }
            "enter" | "space" => {
                let Some(row) = self.settings.sel else { return false };
                self.settings_press(row, window, cx);
            }
            // A letter typed on the page starts a search.
            _ => {
                let Some(typed) = stroke.key_char.as_ref().filter(|c| !c.trim().is_empty()) else { return false };
                let Some(find) = self.settings.find.clone() else { return false };
                find.update(cx, |find, cx| find.insert(typed, cx));
                window.focus(&find.focus_handle(cx));
            }
        }
        true
    }

    /// Escape on the page: the search is cleared first, then the tab closes.
    pub(super) fn settings_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.settings.find.as_ref().filter(|find| !find.read(cx).text().is_empty()) {
            Some(find) => find.update(cx, |find, cx| find.clear(cx)),
            None => {
                if let Some(tab) = self.panes.iter().position(Pane::settings) {
                    self.close_tab(tab, window, cx);
                }
            }
        }
    }

    /// The page: the search, the sections on the left, the settings of the one chosen on the right.
    pub(super) fn render_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = self.theme;
        let narrow = self.settings.width.get() > px(0.) && self.settings.width.get() < t.px(NARROW);
        let query = self.settings_query(cx);
        let rows = self.settings_rows(cx);
        let searching = !query.is_empty();
        let typing = self.settings.find.as_ref().is_some_and(|find| find.focus_handle(cx).is_focused(window));
        let find = self.settings.find.clone().map(|find| {
            let frame = widgets::field_frame("settings-find", find, typing, !searching, tr("Search the settings…", "Chercher un réglage…"), t);
            frame.max_w(t.px(320.)).min_w(t.px(140.))
        });
        let head = div()
            .flex_none()
            .px_6()
            .pt_5()
            .pb_3()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_4()
            .gap_y_2()
            .child(widgets::serif_title(div().debug_selector(|| "settings-title".into()).flex_1().min_w(t.px(120.)), t, 22.).child(tr("Settings", "Réglages")))
            .children(find);
        // The sections: a column on the left, or a row of pills above when the page is narrow.
        let sections = Section::ALL.into_iter().map(|section| {
            let on = !searching && section == self.settings.section;
            let key = section.key();
            widgets::pill(t, on)
                .id(gpui::SharedString::from(format!("settings-section-{key}")))
                .debug_selector(move || format!("settings-section-{key}"))
                .h(t.px(28.))
                .flex_none()
                .cursor_pointer()
                .text_size(t.font_body())
                .when(on, |d| d.text_color(t.accent))
                .when(!on, |d| d.hover(|s| s.bg(t.hover).text_color(t.text)))
                .child(section.label())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.show_section(section, cx);
                    window.focus(&this.settings.focus);
                }))
        });
        let nav = div()
            .debug_selector(|| "settings-sections".into())
            .flex_none()
            .flex()
            .map(|d| if narrow { d.flex_wrap().gap_1().px_6().pb_3() } else { d.flex_col().gap(t.px(2.)).w(t.px(200.)).pl_4().pr_2() })
            .children(sections);
        // The settings shown, under the title of their section; while searching, grouped by section.
        let mut list: Vec<gpui::AnyElement> = Vec::new();
        let mut last = None;
        let mut reveal = None;
        for row in rows.iter().copied() {
            if last != Some(row.section()) {
                last = Some(row.section());
                list.push(widgets::serif_title(div().pt_2().pb_1().px_3(), t, 16.).child(row.section().label()).into_any_element());
            }
            if self.settings.sel == Some(row) {
                reveal = Some(list.len());
            }
            list.push(self.render_setting(row, cx).into_any_element());
        }
        if rows.is_empty() {
            list.push(div().px_3().py_2().text_color(t.soft).child(tr("No setting matches.", "Aucun réglage ne correspond.")).into_any_element());
        }
        if std::mem::take(&mut self.settings.reveal)
            && let Some(at) = reveal
        {
            self.settings.scroll.scroll_to_item(at);
        }
        let content = div()
            .id("settings-content")
            .debug_selector(|| "settings-content".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.settings.scroll)
            .pr_6()
            .pb_6()
            .when(narrow, |d| d.pl_6())
            .flex()
            .flex_col()
            .gap_1()
            .children(list);
        let body = div().flex_1().min_h_0().flex().map(|d| if narrow { d.flex_col() } else { d.flex_row() }).child(nav).child(content);
        let width = self.settings.width.clone();
        let me = cx.entity();
        div()
            .id("settings")
            .debug_selector(|| "settings".into())
            .track_focus(&self.settings.focus)
            .size_full()
            .flex()
            .flex_col()
            .text_size(t.font_body())
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        // A new width that crosses `NARROW` asks for one more frame.
                        if width.replace(bounds.size.width) != bounds.size.width {
                            cx.defer(move |cx| me.update(cx, |_, cx| cx.notify()));
                        }
                    },
                    |_, _, _, _| (),
                )
                .absolute()
                .size_full(),
            )
            .on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, window, cx| {
                if this.settings.focus.is_focused(window) && this.settings_key(&e.keystroke, window, cx) {
                    cx.stop_propagation();
                }
            }))
            // Up and Down in the search field: back to the rows.
            .on_action(cx.listener(|this, _: &palette::Prev, window, cx| {
                window.focus(&this.settings.focus);
                this.settings_step(false, cx);
            }))
            .on_action(cx.listener(|this, _: &palette::Next, window, cx| {
                window.focus(&this.settings.focus);
                this.settings_step(true, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusNext, window, cx| this.settings_tab(1, window, cx)))
            .on_action(cx.listener(|this, _: &FocusPrev, window, cx| this.settings_tab(-1, window, cx)))
            .on_action(cx.listener(|this, _: &CloseHelp, window, cx| this.settings_escape(window, cx)))
            .child(head)
            .child(body)
            .into_any_element()
    }

    /// One setting: its name and line on the left, its control on the right (under them when there is no room).
    fn render_setting(&self, row: Row, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let key = row.key();
        let on = self.settings.sel == Some(row);
        let text = div()
            .flex_1()
            .min_w(t.px(200.))
            .flex()
            .flex_col()
            .gap(t.px(2.))
            .child(div().text_color(t.text).child(row.name()))
            .children(row.line().map(|line| div().text_size(t.font_small()).text_color(t.soft).child(line)));
        let (control, below) = self.render_control(row, cx);
        div()
            .id(gpui::SharedString::from(format!("settings-row-{key}")))
            .debug_selector(move || format!("settings-row-{key}"))
            .flex_none()
            .px_3()
            .py(t.px(10.))
            .rounded(t.large())
            .when(on, |d| d.bg(t.selection_idle()))
            .flex()
            .flex_col()
            .gap_2()
            .child(div().w_full().flex().flex_wrap().items_center().gap_x_4().gap_y_2().child(text).children(control))
            .children(below.map(|below| div().w_full().min_w_0().child(below)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.settings.sel = Some(row);
                if !this.settings.find.as_ref().is_some_and(|find| find.focus_handle(cx).is_focused(window)) {
                    window.focus(&this.settings.focus);
                }
                cx.notify();
            }))
    }

    /// The control of a row, beside its name, and what goes under it (the theme cards, a configuration).
    fn render_control(&self, row: Row, cx: &mut Context<Self>) -> (Option<gpui::AnyElement>, Option<gpui::AnyElement>) {
        let t = self.theme;
        let key = row.key();
        let value = |text: String| div().font_family(t.mono).text_size(t.font_small()).text_color(t.soft).child(text);
        let press = move |id: &'static str, label: &'static str| {
            text_button(gpui::SharedString::from(format!("settings-{key}-{id}")), label, t)
                .debug_selector(move || format!("settings-{key}-{id}"))
        };
        let pick = |list: Vec<(&'static str, &'static str)>, now: &str, set: fn(&mut Prefs, String), cx: &mut Context<Self>| {
            let mut pills = Vec::new();
            for (choice_key, label) in list {
                pills.push(
                    choice(gpui::SharedString::from(format!("settings-{key}-{choice_key}")), label, choice_key == now, t)
                        .debug_selector(move || format!("settings-{key}-{choice_key}"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.settings.sel = Some(row);
                            this.set_prefs(|p| set(p, choice_key.to_string()), window, cx);
                        })),
                );
            }
            div().flex().flex_wrap().gap_1().children(pills)
        };
        let flip = |on: bool, cx: &mut Context<Self>| {
            switch(gpui::SharedString::from(format!("settings-{key}-switch")), on, t)
                .debug_selector(move || format!("settings-{key}-switch"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.settings.sel = Some(row);
                    this.settings_press(row, window, cx);
                }))
        };
        let font_name = |chosen: &str, shown: &'static str| if chosen.is_empty() { format!("{shown} ({})", tr("default", "par défaut")) } else { chosen.to_string() };
        let control = match row {
            Row::Theme => None,
            Row::Font => Some(div().flex().items_center().gap_3().child(value(font_name(&self.prefs.font, t.sans))).child(press("change", tr("Change…", "Changer…")).on_click(cx.listener(|this, _, window, cx| this.settings_press(Row::Font, window, cx)))).into_any_element()),
            Row::Mono => Some(div().flex().items_center().gap_3().child(value(font_name(&self.prefs.mono, t.mono))).child(press("change", tr("Change…", "Changer…")).on_click(cx.listener(|this, _, window, cx| this.settings_press(Row::Mono, window, cx)))).into_any_element()),
            Row::Size => {
                let size = self.prefs.size;
                let step = |id: &'static str, label: &'static str, by: Option<f32>| {
                    press(id, label).on_click(cx.listener(move |this, _, window, cx| {
                        this.settings.sel = Some(Row::Size);
                        this.resize_text(by, window, cx);
                    }))
                };
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(step("smaller", "−", Some(-1.)))
                        .child(value(format!("{size} px · {} %", (t.ui * 100.).round())).min_w(t.px(96.)).text_center())
                        .child(step("bigger", "+", Some(1.)))
                        .when(size != Prefs::SIZE, |d| d.child(step("reset", tr("Default", "Par défaut"), None)))
                        .into_any_element(),
                )
            }
            Row::Motion => Some(flip(self.prefs.motion != "off", cx).into_any_element()),
            Row::Bar => Some(flip(self.prefs.bar != "off", cx).into_any_element()),
            Row::Updates => Some(flip(self.prefs.updates, cx).into_any_element()),
            Row::Page => Some(pick(Page::ALL.map(|p| (p.key(), p.label())).into(), Page::of(&self.prefs.page).key(), |p, v| p.page = v, cx).into_any_element()),
            Row::Wide => Some(pick(Wide::ALL.map(|w| (w.key(), w.label())).into(), Wide::of(&self.prefs.wide).key(), |p, v| p.wide = v, cx).into_any_element()),
            Row::Access => {
                let read_only = self.settings.read_only;
                let both = [("full", tr("Read and write", "Lecture et écriture"), false), ("read-only", tr("Read only", "Lecture seule"), true)];
                Some(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(both.into_iter().map(|(id, label, ro)| {
                            choice(gpui::SharedString::from(format!("settings-access-{id}")), label, read_only == ro, t)
                                .debug_selector(move || format!("settings-access-{id}"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.sel = Some(Row::Access);
                                    this.settings.read_only = ro;
                                    cx.notify();
                                }))
                        }))
                        .into_any_element(),
                )
            }
            Row::Vault => Some(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(press("open", tr("Open another…", "En ouvrir un autre…")).on_click(cx.listener(|this, _, window, cx| this.choose_vault(window, cx))))
                    .child(press("clone", tr("Clone…", "Cloner…")).on_click(cx.listener(|this, _, window, cx| this.ask_clone(window, cx))))
                    .into_any_element(),
            ),
            Row::Trash => Some(press("restore", tr("Restore…", "Restaurer…")).on_click(cx.listener(|this, _, window, cx| this.open_trash(window, cx))).into_any_element()),
            Row::Git => {
                let label = match (self.syncing, &self.settings.remote) {
                    (true, _) => tr("Syncing…", "Synchronisation…"),
                    (false, Some(_)) => tr("Sync now", "Synchroniser"),
                    (false, None) => tr("Connect…", "Relier…"),
                };
                Some(press("sync", label).on_click(cx.listener(|this, _, window, cx| this.git_sync(window, cx))).into_any_element())
            }
            Row::Backup => Some(press("backup", tr("Back up…", "Sauvegarder…")).on_click(cx.listener(|this, _, window, cx| this.choose_backup(window, cx))).into_any_element()),
            Row::Keys => {
                let changed = self.changes.len();
                let said = match changed {
                    0 => tr("All by default", "Tous par défaut").to_string(),
                    1 => tr("1 changed", "1 modifié").to_string(),
                    n => format!("{n} {}", tr("changed", "modifiés")),
                };
                let f1 = keys::of(cx, &ToggleHelp);
                let label: &'static str = tr("Open the list", "Ouvrir la liste");
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(value(said))
                        .child(press("open", label).when(!f1.is_empty(), |d| d.gap_2().child(keycaps(&f1, t, false))).on_click(cx.listener(|this, _, window, cx| this.set_help(true, window, cx))))
                        .into_any_element(),
                )
            }
            Row::Version => Some(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(value(format!("Bref {}", env!("CARGO_PKG_VERSION"))))
                    .child(press("check", tr("Check now", "Chercher maintenant")).on_click(cx.listener(|this, _, _, cx| this.check_now(cx))))
                    .into_any_element(),
            ),
            Row::Links => {
                let link = |id: &'static str, label: &'static str, url: &'static str| press(id, label).text_color(t.accent).on_click(move |_, _, cx| cx.open_url(url));
                Some(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .child(link("guide", tr("User guide", "Guide"), GUIDE))
                        .child(link("github", "GitHub", env!("CARGO_PKG_REPOSITORY")))
                        .child(link("kofi", tr("Support on Ko-fi", "Soutenir sur Ko-fi"), KOFI))
                        .into_any_element(),
                )
            }
            Row::Client | Row::Claude => {
                let copied = self.settings.copied == Some(row);
                Some(
                    press("copy", if copied { tr("Copied", "Copié") } else { tr("Copy", "Copier") })
                        .when(copied, |d| d.text_color(t.accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.settings.sel = Some(row);
                            this.settings_copy(row, cx);
                        }))
                        .into_any_element(),
                )
            }
            Row::NewNotes | Row::Daily | Row::Server => None,
            Row::Config => None,
        };
        let block = |text: String| {
            div()
                .debug_selector(move || format!("settings-{key}-text"))
                .px_3()
                .py_2()
                .rounded(t.small())
                .bg(t.code_bg)
                .font_family(t.mono)
                .text_size(t.font_small())
                .text_color(t.text)
                .child(text)
                .into_any_element()
        };
        let below = match row {
            Row::Theme => Some(self.render_theme_cards(cx).into_any_element()),
            Row::Vault => self.vault.as_ref().map(|root| value(root.display().to_string()).debug_selector(|| "settings-vault-path".into()).into_any_element()),
            Row::Config => Some(value(config::path(config::File::Settings).parent().map_or_else(String::new, |p| p.display().to_string())).into_any_element()),
            Row::Daily => Some(value(format!("{}.md", date_name(today()))).into_any_element()),
            Row::Git => Some(
                value(match (&self.settings.remote, self.sync_broken, &self.settings.synced) {
                    (None, ..) if self.vault.as_ref().is_some_and(|root| root.join(".git").exists()) => tr("A git repository, linked to no server yet", "Un dépôt git, relié à aucun serveur pour l'instant").to_string(),
                    (None, ..) => tr("Not a git repository: Connect links it to an empty one", "Pas un dépôt git : Relier le lie à un dépôt vide").to_string(),
                    (Some(url), true, _) => format!("{url} · {}", tr("the last sync failed", "la dernière synchronisation a échoué")),
                    (Some(url), false, Some(at)) => format!("{url} · {} {at}", tr("synced at", "synchronisé à")),
                    (Some(url), false, None) => url.clone(),
                })
                .debug_selector(|| "settings-git-state".into())
                .into_any_element(),
            ),
            Row::Client => Some(block(client_json(&program(), self.settings.read_only))),
            Row::Claude => Some(block(claude_command(&program(), self.settings.read_only))),
            _ => None,
        };
        (control, below)
    }

    /// The themes as cards that show their colours: the page, its text and its accent; the default shows
    /// Bref's two halves, light and dark.
    fn render_theme_cards(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let colors = |name: &str| THEMES.iter().find(|th| th.0 == name).map(|th| th.1.map(|c| gpui::Hsla::from(gpui::rgb(c))));
        let swatch = |c: [gpui::Hsla; 7]| {
            let [bg, text, dim, accent, code, ..] = c;
            div()
                .flex_1()
                .h_full()
                .bg(bg)
                .p(t.px(8.))
                .flex()
                .flex_col()
                .gap(t.px(4.))
                .child(div().w(t.px(36.)).h(t.px(4.)).rounded_full().bg(text))
                .child(div().w(t.px(52.)).h(t.px(3.)).rounded_full().bg(dim))
                .child(div().flex().gap(t.px(3.)).child(div().w(t.px(18.)).h(t.px(3.)).rounded_full().bg(accent)).child(div().w(t.px(22.)).h(t.px(3.)).rounded_full().bg(code)))
        };
        let current = self.prefs.theme.clone();
        let cards = theme_keys().into_iter().enumerate().map(|(i, name)| {
            let on = name == current || (name.is_empty() && colors(&current).is_none());
            let preview = match colors(name) {
                Some(c) => div().size_full().flex().child(swatch(c)),
                None => div().size_full().flex().children([1, 0].map(|i| swatch(THEMES[i].1.map(|c| gpui::Hsla::from(gpui::rgb(c)))))),
            };
            let label = if name.is_empty() { tr("Default", "Par défaut") } else { name };
            card(("settings-theme-card", i), on, t)
                .debug_selector(move || format!("settings-theme-{i}"))
                .w(t.px(118.))
                .child(div().h(t.px(46.)).border_b_1().border_color(t.border).child(preview))
                .child(div().px_2().py_1().truncate().text_size(t.font_small()).text_color(if on { t.accent } else { t.text }).child(label))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.settings.sel = Some(Row::Theme);
                    this.set_prefs(|p| p.theme = name.to_string(), window, cx);
                }))
        });
        div().debug_selector(|| "settings-themes".into()).w_full().flex().flex_wrap().gap_2().children(cards)
    }
}

/// The user guide, online.
pub(super) const GUIDE: &str = "https://alarboulletmarin.github.io/bref/guide/settings.html";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_in_order_of_its_section() {
        let order: Vec<usize> = Row::ALL.iter().map(|row| Section::ALL.iter().position(|s| *s == row.section()).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "rows grouped by section: {order:?}");
        for section in Section::ALL {
            assert!(Row::ALL.iter().any(|row| row.section() == section), "{section:?} has no row");
        }
    }

    #[test]
    fn the_agents_configuration_names_the_program() {
        let json = client_json("/usr/bin/bref", false);
        let read: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(read["mcpServers"]["bref"]["command"], "/usr/bin/bref");
        assert_eq!(read["mcpServers"]["bref"]["args"], serde_json::json!(["mcp"]));
        // A Windows path keeps its backslashes, escaped.
        let windows: serde_json::Value = serde_json::from_str(&client_json(r"C:\Bref\bref.exe", true)).unwrap();
        assert_eq!(windows["mcpServers"]["bref"]["command"], r"C:\Bref\bref.exe");
        assert_eq!(windows["mcpServers"]["bref"]["args"], serde_json::json!(["mcp", "--read-only"]));
        assert_eq!(claude_command("/usr/bin/bref", true), "claude mcp add --scope user bref -- /usr/bin/bref mcp --read-only");
        assert_eq!(claude_command("/Applications/My Apps/bref", false), "claude mcp add --scope user bref -- \"/Applications/My Apps/bref\" mcp");
    }

    #[test]
    fn search_finds_by_name_line_and_value() {
        let found = |q: &str| crate::lang::in_english(|| Row::ALL.into_iter().filter(|row| row.matches(q)).collect::<Vec<_>>());
        assert_eq!(found("dracula"), vec![Row::Theme]);
        assert!(found("font").contains(&Row::Font) && found("font").contains(&Row::Mono));
        assert!(found("MCP").contains(&Row::Server));
        assert!(found("tables wide").contains(&Row::Wide));
        assert!(found("nothing like this").is_empty());
    }
}
