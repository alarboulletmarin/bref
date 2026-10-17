//! The command palette as the shell opens it: notes, commands, outline, comments, places, questions.

use super::*;

impl Shell {
    /// Asks for a line of free text (a URL) in the palette, then passes it to `then`.
    pub(super) fn ask_text(&mut self, label: &'static str, window: &mut Window, cx: &mut Context<Self>, then: fn(&mut Self, String, &mut Window, &mut Context<Self>)) {
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::prompt(label, "", theme, cx));
        cx.subscribe_in(&palette, window, move |this, _, event, window, cx| {
            this.palette = None;
            match this.vault {
                Some(_) => window.focus(&this.pane().editor.focus_handle(cx)),
                None => window.focus(&this.focus),
            }
            if let PaletteEvent::Submit(text) = event
                && !text.trim().is_empty()
            {
                then(this, text.trim().to_string(), window, cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Outline of the note: the list of its headings, the one of the current
    /// section preselected. The heading browsed shows as a preview, Enter leaves the cursor there,
    /// Escape puts it back where it was.
    pub(super) fn open_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let heads = markdown::headings(self.pane().editor.read(cx).text())
            .into_iter()
            .map(|(level, title, at)| (format!("{}{title}", "  ".repeat(level as usize - 1)), at))
            .collect();
        let none = tr("This note has no headings", "Cette note n'a pas de titres");
        self.open_places(tr("Outline", "Plan"), none, heads, window, cx)
    }

    /// The note's comments, in the same list as its outline: the commented text, then
    /// what is said about it.
    pub(super) fn open_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.comments_shut = false;
        let said = markdown::all_comments(self.pane().editor.read(cx).text())
            .into_iter()
            .map(|(noted, said, at)| (format!("{noted} — {said}"), at))
            .collect();
        let none = tr("This note has no comments", "Cette note n'a pas de commentaires");
        self.open_places(tr("Comments", "Commentaires"), none, said, window, cx)
    }

    /// List of places in the note (label, byte), the one we are at preselected; with
    /// none, `none` says so.
    pub(super) fn open_places(&mut self, title: &str, none: &str, heads: Vec<(String, usize)>, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() || self.pane().file.is_some() {
            return;
        }
        let before = self.pane().editor.read(cx).position();
        if heads.is_empty() {
            self.palette = None;
            self.say(Tone::Info, none, cx);
            window.focus(&self.pane().editor.focus_handle(cx));
            return cx.notify();
        }
        let current = heads.iter().rposition(|(_, at)| *at <= before).unwrap_or(0);
        let (theme, options) = (self.theme, heads.iter().map(|(label, _)| label.clone()).collect());
        let palette = cx.new(|cx| Palette::choose(title, options, "", theme, cx).select(current));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            let to = match event {
                PaletteEvent::Preview(_) | PaletteEvent::Submit(_) => palette.read(cx).chosen().and_then(|i| heads.get(i)),
                _ => None,
            };
            let (at, top) = to.map_or((before, false), |(_, at)| (*at, true));
            this.pane().editor.update(cx, |e, cx| e.jump(at, top, cx));
            if !matches!(event, PaletteEvent::Preview(_)) {
                this.palette = None;
                window.focus(&this.pane().editor.focus_handle(cx));
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    pub(super) fn open_palette(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.show_palette(query, false, window, cx)
    }

    /// Search in the text of the whole vault: the palette, where each choice is a line.
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_palette("", true, window, cx)
    }

    pub(super) fn show_palette(&mut self, query: &str, lines: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.flush(cx);
        let entries = self
            .by_recency()
            .into_iter()
            .map(|n| Entry {
                name: n.name.clone(),
                path: n.path.clone(),
                tags: n.tags.clone(),
                body: n.body.clone(),
                lower: n.lower.clone(),
            })
            // The vault's other files (tables, images, diagrams) are also found by their name.
            .chain(self.images.iter().map(|path| Entry {
                name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                path: path.clone(),
                ..Entry::default()
            }))
            .collect();
        let theme = self.theme;
        let updates = self.prefs.updates;
        let installable = self.update.as_ref().filter(|r| r.asset.is_some()).map(|r| r.version.clone());
        let table = self.shown_sheet().is_some();
        let lists = self.pane().file.is_none() && self.pane().listing.is_none() && !self.board_shown(cx);
        let cells = lists && self.pane().editor.read(cx).in_table();
        let swap = !self.swapped.is_empty();
        let (commands, here) = self.palette_commands(window, cx);
        let context = self.here_group(cx).1;
        let palette = cx.new(|cx| match lines {
            true => Palette::search(entries, theme, cx),
            false => Palette::new(entries, query, theme, cx)
                .with_updates(updates, installable)
                .with_table(table)
                .with_lists(lists)
                .with_cells(cells)
                .with_undo_swap(swap)
                .with_commands(commands, here, context),
        });
        // A command chosen in the palette goes to what had input before it.
        let back = window.focused(cx);
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            this.palette = None;
            window.focus(&this.pane().editor.focus_handle(cx));
            let aside = palette.read(cx).aside;
            match event {
                PaletteEvent::Open(path) if aside => this.open_aside(path, window, cx),
                PaletteEvent::Open(path) => {
                    this.open_note(path, cx);
                    // A table, an image or a diagram takes input: given back to the note, it would erase them.
                    this.focus_pane(this.active, window, cx);
                }
                PaletteEvent::OpenAt(path, row, query) => {
                    this.open_note(path, cx);
                    this.pane().editor.update(cx, |e, cx| e.select_in_row(*row, query, cx));
                }
                PaletteEvent::Create(name) => this.open_wiki(name, cx),
                PaletteEvent::ChangeVault => this.choose_vault(window, cx),
                PaletteEvent::Help => this.set_help(true, window, cx),
                PaletteEvent::NewDiagram => this.new_diagram(window, cx),
                PaletteEvent::NewFolder => this.new_folder(window, cx),
                PaletteEvent::Table(pick) => this.choose_table(*pick, window, cx),
                PaletteEvent::ImportDiagram => this.import_diagram(window, cx),
                PaletteEvent::Setting(setting) => this.choose_setting(*setting, window, cx),
                PaletteEvent::ToggleUpdates => this.toggle_updates(cx),
                PaletteEvent::CheckUpdate => this.check_now(cx),
                PaletteEvent::Outline => this.open_outline(window, cx),
                PaletteEvent::Comments => this.open_comments(window, cx),
                PaletteEvent::Template(TemplateAct::Save) => this.save_as_template(window, cx),
                PaletteEvent::Template(act) => this.open_templates(*act, window, cx),
                PaletteEvent::Grid(op) => {
                    // The gesture goes to what is shown: a CSV's sheet, or the note.
                    match this.shown_sheet() {
                        Some(sheet) => window.focus(&sheet.focus_handle(cx)),
                        None => window.focus(&this.pane().editor.focus_handle(cx)),
                    }
                    window.dispatch_action(op.action(), cx);
                }
                PaletteEvent::Run(name) => {
                    match &back {
                        Some(back) => window.focus(back),
                        None => this.focus_pane(this.active, window, cx),
                    }
                    if let Ok(action) = cx.build_action(name, None) {
                        window.dispatch_action(action, cx);
                    }
                }
                PaletteEvent::Restyle(style) => {
                    window.focus(&this.pane().editor.focus_handle(cx));
                    this.pane().editor.update(cx, |e, cx| e.restyle(*style, cx));
                }
                PaletteEvent::UndoSwap => this.undo_swap(cx),
                PaletteEvent::NewBoard => this.new_note(kanban::new_board(), cx),
                PaletteEvent::Today => this.open_today(cx),
                PaletteEvent::Travel(forward) => this.travel(*forward, window, cx),
                PaletteEvent::Trash => this.open_trash(window, cx),
                PaletteEvent::Backup => this.choose_backup(window, cx),
                PaletteEvent::Sync => this.git_sync(window, cx),
                PaletteEvent::CloneVault => this.ask_clone(window, cx),
                PaletteEvent::Terminal => this.open_terminal(cx),
                PaletteEvent::CaptureKey => this.capture_shortcut(cx),
                PaletteEvent::InstallUpdate => this.install_update(cx),
                // Closed without a choice: we get back what the pane showed, table or diagram included.
                PaletteEvent::Dismiss => this.focus_pane(this.active, window, cx),
                PaletteEvent::Submit(_) | PaletteEvent::Preview(_) => {}
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// The catalogue commands the view can do, with their keys, the view's own first
    /// (how many: the second number). Those it cannot do are not offered.
    pub(super) fn palette_commands(&self, window: &mut Window, cx: &mut Context<Self>) -> (Vec<commands::Command>, usize) {
        use commands::Group;
        let available: std::collections::HashSet<&str> = window.available_actions(cx).iter().map(|action| action.name()).collect();
        let here = self.here_group(cx).0;
        let list = commands::catalog()
            .into_iter()
            .filter(|(action, ..)| available.contains(action))
            // Gestures of a block, a table or a diagram only count there: elsewhere, they would do nothing.
            .filter(|&(_, _, _, group)| !matches!(group, Group::Block | Group::Table | Group::Diagram) || Some(group) == here)
            .filter_map(|(action, label, icon, group)| {
                let keys = keys::of(cx, cx.build_action(action, None).ok()?.as_ref());
                Some(commands::Command { action, label, icon, group, keys })
            })
            .collect();
        commands::arrange(list, here)
    }

    /// The command palette (F10, or the rail's first button): the palette, only the commands.
    pub(super) fn open_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return;
        }
        self.show_palette(">", false, window, cx)
    }
}
