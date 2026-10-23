//! The help panel: every shortcut by section, searchable, and editable in place.

use super::*;

/// The help panel: open or not, its search field, the row picked with the keyboard, and the
/// shortcut being typed.
#[derive(Default)]
pub(super) struct Help {
    pub(super) open: bool,
    /// Search field of the help panel, while it is open.
    pub(super) find: Option<Entity<kanban::Field>>,
    /// The help row picked with the keyboard, and the shortcut being typed.
    pub(super) sel: Option<&'static str>,
    pub(super) edit: Option<KeyEdit>,
    /// Receives each keystroke before shortcuts, for the one being awaited.
    pub(super) key_tap: Option<gpui::Subscription>,
}

/// What prevents keys from becoming a shortcut as they are.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum KeyIssue {
    /// Already taken by these actions: to replace, or to leave.
    Taken(Vec<&'static str>),
    Refused(&'static str),
}

/// A shortcut awaited in the help panel: the next keystroke becomes it.
pub(super) struct KeyEdit {
    pub(super) action: &'static str,
    pub(super) keys: String,
    pub(super) issue: Option<KeyIssue>,
}

pub(super) const KOFI: &str = "https://ko-fi.com/T6T01WC5ZC";

impl Shell {
    /// Shows or hides the shortcuts panel; focus follows.
    pub(super) fn set_help(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.help.open = open;
        self.help.find = None;
        self.help.sel = None;
        self.help.edit = None;
        if open {
            let theme = self.theme;
            let find = cx.new(|cx| kanban::Field::new("", theme, cx));
            cx.observe(&find, |_, _, cx| cx.notify()).detach();
            // Escape clears the field, then closes the panel.
            cx.subscribe_in(&find, window, |this, find, event: &kanban::FieldEvent, window, cx| {
                if let kanban::FieldEvent::Done = event {
                    // Enter on the picked row: its keys are awaited.
                    if let Some(action) = this.help.sel.filter(|sel| this.help_actions(cx).contains(sel)) {
                        this.key_edit(action, cx);
                    }
                } else if let kanban::FieldEvent::Cancel = event {
                    if find.read(cx).line.text.is_empty() {
                        this.set_help(false, window, cx);
                    } else {
                        find.update(cx, |find, cx| {
                            find.line = line::Line::default();
                            cx.notify();
                        });
                    }
                }
            })
            .detach();
            window.focus(&find.focus_handle(cx));
            self.help.find = Some(find);
        } else if self.vault.is_none() {
            window.focus(&self.focus);
        } else {
            // To what the pane shows: given to the note, input would clear a table or a diagram.
            self.focus_pane(self.active, window, cx);
        }
        cx.notify();
    }

    /// The actions of the rows the help shows, in order.
    pub(super) fn help_actions(&self, cx: &App) -> Vec<&'static str> {
        self.help_rows(cx).into_iter().flat_map(|(_, rows)| rows).filter_map(|row| row.action).collect()
    }

    /// Up and down in the help: the picked row, among those whose keys can be changed.
    // ponytail: the list does not scroll to the picked row; searching brings it back
    // on screen. One `ScrollHandle` per section if the need is confirmed.
    pub(super) fn help_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let actions = self.help_actions(cx);
        let at = self.help.sel.and_then(|sel| actions.iter().position(|action| *action == sel));
        let next = match (at, down) {
            (None, true) => 0,
            (None, false) => actions.len().saturating_sub(1),
            (Some(at), true) => (at + 1).min(actions.len().saturating_sub(1)),
            (Some(at), false) => at.saturating_sub(1),
        };
        self.help.sel = actions.get(next).copied();
        cx.notify();
    }

    /// Awaits the keys of the shortcut for `action`.
    pub(super) fn key_edit(&mut self, action: &'static str, cx: &mut Context<Self>) {
        self.help.sel = Some(action);
        self.help.edit = Some(KeyEdit { action, keys: String::new(), issue: None });
        cx.notify();
    }

    /// A keystroke while a shortcut is awaited: it becomes the shortcut, except Escape (give up),
    /// Backspace (no shortcut) and Enter on keys already taken (replace).
    /// `false` when nothing is awaited: the keystroke goes on its way.
    pub(super) fn key_pressed(&mut self, stroke: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(edit) = &self.help.edit else { return false };
        let (action, taken) = (edit.action, matches!(edit.issue, Some(KeyIssue::Taken(_))));
        let keys = keys::spell(stroke);
        match keys.as_str() {
            "escape" => self.help.edit = None,
            "enter" if taken => self.key_replace(cx),
            "backspace" | "delete" => self.key_set(action, String::new(), &[], cx),
            _ => {
                let open = editable();
                let others = keys::clashes(&self.bound, &defaults(), action, &keys);
                let issue = match keys::refusal(&keys) {
                    Some(keys::Why::Types) => Some(KeyIssue::Refused(tr(
                        "A key alone types text: add Ctrl, Alt or Cmd.",
                        "Une touche seule écrit du texte : ajouter Ctrl, Alt ou Cmd.",
                    ))),
                    Some(_) => Some(KeyIssue::Refused(tr("These keys cannot be a shortcut.", "Ces touches ne peuvent pas servir de raccourci."))),
                    None if others.iter().any(|other| !open.contains(other)) => Some(KeyIssue::Refused(tr(
                        "Kept for typing and moving in the text and the views.",
                        "Réservées à la saisie et au déplacement dans le texte et les vues.",
                    ))),
                    None if !others.is_empty() => Some(KeyIssue::Taken(others)),
                    None => None,
                };
                match issue {
                    None => self.key_set(action, keys, &[], cx),
                    issue => self.help.edit = Some(KeyEdit { action, keys, issue }),
                }
            }
        }
        cx.notify();
        true
    }

    /// "Replace": the proposed keys go to the action, those that had them lose them.
    pub(super) fn key_replace(&mut self, cx: &mut Context<Self>) {
        if let Some(KeyEdit { action, keys, issue: Some(KeyIssue::Taken(others)) }) = self.help.edit.take() {
            self.key_set(action, keys, &others, cx);
        }
    }

    /// Gives `keys` to `action` (empty: no shortcut) and takes them from `losers`.
    pub(super) fn key_set(&mut self, action: &'static str, keys: String, losers: &[&'static str], cx: &mut Context<Self>) {
        let mut usual: Vec<String> = defaults().into_iter().filter(|bind| bind.action == action).map(|bind| bind.keys).collect();
        usual.dedup();
        // Back to its original keys: nothing left to remember for this action.
        if usual == [keys.clone()] {
            self.changes.remove(action);
        } else {
            self.changes.insert(action.to_string(), keys.clone());
        }
        for other in losers {
            self.changes.insert(other.to_string(), String::new());
        }
        self.help.edit = None;
        if keys::system(&keys) {
            self.say(Tone::Info, tr("The system may keep these keys for itself.", "Le système garde peut-être ces touches pour lui."), cx);
        }
        self.rebind(cx);
        vault::save_file("keys", &keys::write(&self.changes));
    }

    /// Restores the original keys of one action, or of all.
    pub(super) fn key_reset(&mut self, action: Option<&'static str>, cx: &mut Context<Self>) {
        match action {
            Some(action) => drop(self.changes.remove(action)),
            None => self.changes.clear(),
        }
        self.help.edit = None;
        self.rebind(cx);
        vault::save_file("keys", &keys::write(&self.changes));
    }

    /// Applies `changes`: the bindings in effect change right away, without restarting.
    pub(super) fn rebind(&mut self, cx: &mut Context<Self>) {
        let open = editable();
        self.bound = keys::effective(&defaults(), &self.changes, &|action| open.contains(action));
        install(cx, &self.bound);
        cx.notify();
    }

    /// The help rows the search keeps.
    pub(super) fn help_rows(&self, cx: &App) -> keys::Sections {
        let query = self.help.find.as_ref().map(|find| find.read(cx).line.text.clone()).unwrap_or_default();
        let mut sections = keys::filter(help_sections(&self.bound, &self.changes), &query);
        // Opened on a table or a diagram, the panel starts with its keys.
        if let Some(at) = self.here(cx).and_then(|here| sections.iter().position(|(title, _)| *title == here)) {
            let first = sections.remove(at);
            sections.insert(0, first);
        }
        sections
    }

    /// The help section about what is shown: a CSV table, a diagram, a table
    /// of the note under the cursor; none for an ordinary note, the help starts with notes.
    pub(super) fn here(&self, cx: &App) -> Option<&'static str> {
        if self.shown_sheet().is_some() {
            Some(tr("CSV tables", "Tableaux CSV"))
        } else if self.pane().drawing().is_some() {
            Some(tr("Diagrams", "Schémas"))
        } else if self.pane().file.is_none() && self.pane().listing.is_none() && self.pane().editor.read(cx).in_table() {
            Some(tr("Tables in a note", "Tableaux d'une note"))
        } else {
            None
        }
    }

    /// The group of commands for what is being looked at, and its name for the palette field: a CSV
    /// table, a diagram, a table of the note under the cursor, otherwise the cursor's block.
    pub(super) fn here_group(&self, cx: &App) -> (Option<commands::Group>, &'static str) {
        use commands::Group;
        if self.shown_sheet().is_some() {
            (Some(Group::Table), tr("CSV table", "Tableau CSV"))
        } else if self.pane().drawing().is_some() {
            (Some(Group::Diagram), tr("Diagram", "Schéma"))
        } else if self.pane().file.is_some() || self.pane().listing.is_some() || self.board_shown(cx) {
            (None, "")
        } else if self.pane().editor.read(cx).in_table() {
            (Some(Group::Table), tr("Table", "Tableau"))
        } else {
            (Some(Group::Block), "Note")
        }
    }

    /// `short`: the window is low, the "about" footer scrolls with the list instead of taking its place.
    pub(super) fn render_help(&self, short: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let rows = self.help_rows(cx);
        let none = rows.is_empty();
        let asked = self.help.find.as_ref().is_some_and(|find| !find.read(cx).line.text.is_empty());
        let small = |id: String, text: &'static str| {
            div()
                .id(gpui::SharedString::from(id.clone()))
                .debug_selector(move || id.clone())
                .px_1()
                .h(t.px(24.))
                .flex()
                .items_center()
                .rounded(t.px(4.))
                .cursor_pointer()
                .text_size(t.px(11.))
                .text_color(t.soft)
                .hover(|s| s.bg(t.border).text_color(t.text))
                .child(text)
        };
        let hint = div().absolute().left_0().top_0().text_color(t.soft).child(tr("Search the shortcuts…", "Chercher un raccourci…"));
        let find = div()
            .flex_none()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(t.border)
            .flex()
            .items_center()
            .gap_2()
            .child(svg().path("search.svg").size(t.px(14.)).flex_none().text_color(t.soft))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .children(self.help.find.clone())
                    .when(!asked, |d| d.child(hint)),
            )
            .when(!self.changes.is_empty(), |d| {
                d.child(
                    small("keys-reset".into(), tr("Reset all", "Tout rétablir")).on_click(cx.listener(|this, _, _, cx| this.key_reset(None, cx))),
                )
            });
        let label = |action: &str| {
            help_sections(&[], &keys::Changes::new()).into_iter().flat_map(|(_, rows)| rows).find(|row| row.action == Some(action)).map_or("", |row| row.effect)
        };
        let sections = rows.into_iter().map(|(title, rows)| {
            div()
                .w(t.px(370.))
                .max_w_full()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().mb_1().text_color(t.accent).text_size(t.px(12.)).child(title))
                .children(rows.into_iter().map(|row| {
                    let edit = self.help.edit.as_ref().filter(|edit| Some(edit.action) == row.action);
                    let chosen = row.action.is_some() && row.action == self.help.sel;
                    // The keys: a button when they can be changed, text otherwise.
                    let keys = match row.action {
                        // Badges for real keys; a line of words ("Drag from the tree") or of
                        // alternatives ("F3 / Enter") stays text.
                        None if row.keys.contains(' ') => div().w(t.px(156.)).flex_none().font_family(mono()).text_size(t.px(12.)).child(row.keys.clone()),
                        None => div().w(t.px(156.)).flex_none().child(keycaps(&row.keys, t, false).flex_wrap()),
                        Some(action) => {
                            let color = if edit.is_some() || row.changed { t.accent } else { t.text };
                            let shown = if edit.is_some() {
                                div().font_family(mono()).text_size(t.px(12.)).child(tr("Press the keys…", "Tapez les touches…"))
                            } else if row.keys.is_empty() {
                                div().font_family(mono()).text_size(t.px(12.)).child("—")
                            } else {
                                keycaps(&row.keys, t, false).flex_wrap()
                            };
                            let key = div()
                                .id(gpui::SharedString::from(format!("key-{action}")))
                                .debug_selector(move || format!("key-{action}"))
                                .px_1()
                                .rounded(t.px(4.))
                                .bg(t.border.opacity(0.6))
                                .cursor_pointer()
                                .text_color(color)
                                .hover(|s| s.bg(t.border))
                                .child(shown)
                                .on_click(cx.listener(move |this, _, _, cx| this.key_edit(action, cx)));
                            let reset = small(format!("key-reset-{action}"), tr("default", "défaut"))
                                .on_click(cx.listener(move |this, _, _, cx| this.key_reset(Some(action), cx)));
                            div()
                                .debug_selector(move || format!("key-column-{action}"))
                                .w(t.px(156.))
                                .flex_none()
                                .flex()
                                .flex_wrap()
                                .items_start()
                                .gap_1()
                                .child(key)
                                .when(row.changed && edit.is_none(), |d| d.child(reset))
                        }
                    };
                    // Under the row being typed: what to do, or what is wrong.
                    let note = edit.map(|edit| match &edit.issue {
                        None => div().text_size(t.px(12.)).text_color(t.soft).child(tr(
                            "Backspace: no shortcut · Esc: cancel",
                            "Retour arrière : aucun raccourci · Échap : annuler",
                        )),
                        Some(KeyIssue::Refused(why)) => div().text_size(t.px(12.)).child(format!("{} : {why}", keys::show(&edit.keys))),
                        Some(KeyIssue::Taken(others)) => div()
                            .text_size(t.px(12.))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .child(div().w_full().child(format!(
                                "{} : {} {}",
                                keys::show(&edit.keys),
                                tr("already used by:", "déjà utilisé par :"),
                                others.iter().map(|other| label(other)).collect::<Vec<_>>().join(", "),
                            )))
                            .child(
                                small("key-replace".into(), tr("Replace (Enter)", "Remplacer (Entrée)"))
                                    .text_color(t.text)
                                    .on_click(cx.listener(|this, _, _, cx| this.key_replace(cx))),
                            )
                            .child(small("key-keep".into(), tr("Cancel (Esc)", "Annuler (Échap)")).on_click(cx.listener(|this, _, _, cx| {
                                this.help.edit = None;
                                cx.notify();
                            }))),
                    });
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .rounded(t.px(4.))
                        .when(chosen, |d| d.bg(t.selection.opacity(0.5)))
                        .child(div().flex().gap_3().child(keys).child(div().flex_1().min_w_0().text_color(if chosen { t.text } else { t.soft }).child(row.effect)))
                        .children(note)
                }))
        });
        // About, at the foot of the panel: what the app is, and where to find it.
        let link = |id: &'static str, icon: &'static str, label: &'static str, url: &'static str| {
            div()
                .id(id)
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_1p5()
                .rounded(t.px(6.))
                .cursor_pointer()
                .text_color(t.accent)
                .hover(|s| s.bg(t.border))
                .child(svg().path(icon).size(t.px(14.)).flex_none().text_color(t.accent))
                .child(label)
                .on_mouse_down(MouseButton::Left, move |_, _, cx| cx.open_url(url))
        };
        let about = div()
            .flex_none()
            .map(|d| if short { d.w_full().pt_3() } else { d.px_5().py_3() })
            .border_t_1()
            .border_color(t.border)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(logo(t))
            .child(format!("Bref {}", env!("CARGO_PKG_VERSION")))
            .child(div().flex_1().min_w(t.px(200.)).text_color(t.soft).child(tr(
                "Fast, minimal Markdown notes, in plain files you own.",
                "Des notes Markdown rapides et minimales, dans de simples fichiers qui t'appartiennent.",
            )))
            .child(link("about-github", "code.svg", "GitHub", env!("CARGO_PKG_REPOSITORY")))
            .child(link("about-kofi", "heart.svg", tr("Support on Ko-fi", "Soutenir sur Ko-fi"), KOFI));
        let (inside, below) = if short { (Some(about), None) } else { (None, Some(about)) };
        div()
            .absolute()
            .inset_0()
            .occlude()
            .p_4()
            .flex()
            // Pinned at the top: the search field does not move when the list gets shorter.
            .items_start()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.set_help(false, window, cx)),
            )
            .child(
                div()
                    .id("help")
                    // Two columns if the window is wide enough, one otherwise.
                    .w(t.px(812.))
                    .max_w_full()
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .rounded(t.px(10.))
                    .shadow_lg()
                    .text_size(t.px(13.))
                    // A click in the panel does not close it.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_action(cx.listener(|this, _: &palette::Prev, _, cx| this.help_step(false, cx)))
                    .on_action(cx.listener(|this, _: &palette::Next, _, cx| this.help_step(true, cx)))
                    .child(find)
                    .child(
                        div()
                            .id("help-keys")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_5()
                            .flex()
                            .flex_wrap()
                            .gap_x_6()
                            .gap_y_4()
                            .children(sections)
                            .when(none, |d| {
                                d.child(div().text_color(t.soft).child(tr("No shortcut matches.", "Aucun raccourci ne correspond.")))
                            })
                            .children(inside),
                    )
                    .children(below)
                    .with_animation(
                        "help-in",
                        Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
                        |help, delta| help.opacity(delta),
                    ),
            )
    }
}
