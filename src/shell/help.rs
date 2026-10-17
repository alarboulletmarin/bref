//! The help panel: every shortcut by section, searchable, and editable in place.

use super::*;

/// The help panel: open or not, its search field, the row picked with the keyboard, and the
/// shortcut being typed.
#[derive(Default)]
pub(super) struct Help {
    pub(super) open: bool,
    /// Champ de recherche du panneau d'aide, tant qu'il est ouvert.
    pub(super) find: Option<Entity<kanban::Field>>,
    /// La ligne de l'aide choisie au clavier, et le raccourci en cours de saisie.
    pub(super) sel: Option<&'static str>,
    pub(super) edit: Option<KeyEdit>,
    /// Reçoit chaque frappe avant les raccourcis, pour celle qu'on attend.
    pub(super) key_tap: Option<gpui::Subscription>,
}

/// Ce qui empêche des touches de devenir un raccourci telles quelles.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum KeyIssue {
    /// Déjà prises par ces actions : à remplacer, ou à laisser.
    Taken(Vec<&'static str>),
    Refused(&'static str),
}

/// Un raccourci attendu dans le panneau d'aide : la prochaine frappe le devient.
pub(super) struct KeyEdit {
    pub(super) action: &'static str,
    pub(super) keys: String,
    pub(super) issue: Option<KeyIssue>,
}

pub(super) const KOFI: &str = "https://ko-fi.com/T6T01WC5ZC";

impl Shell {
    /// Affiche ou masque le panneau des raccourcis ; le focus suit.
    pub(super) fn set_help(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.help.open = open;
        self.help.find = None;
        self.help.sel = None;
        self.help.edit = None;
        if open {
            let theme = self.theme;
            let find = cx.new(|cx| kanban::Field::new("", theme, cx));
            cx.observe(&find, |_, _, cx| cx.notify()).detach();
            // Échap vide le champ, puis ferme le panneau.
            cx.subscribe_in(&find, window, |this, find, event: &kanban::FieldEvent, window, cx| {
                if let kanban::FieldEvent::Done = event {
                    // Entrée sur la ligne choisie : ses touches sont attendues.
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
            // À ce que le pane montre : rendue à la note, la saisie effacerait un tableau ou un schéma.
            self.focus_pane(self.active, window, cx);
        }
        cx.notify();
    }

    /// Les actions des lignes que l'aide montre, dans l'ordre.
    pub(super) fn help_actions(&self, cx: &App) -> Vec<&'static str> {
        self.help_rows(cx).into_iter().flat_map(|(_, rows)| rows).filter_map(|row| row.action).collect()
    }

    /// Haut et bas dans l'aide : la ligne choisie, parmi celles dont les touches se changent.
    // ponytail: la liste ne défile pas jusqu'à la ligne choisie ; la recherche la ramène à
    // l'écran. Un `ScrollHandle` par section si le besoin se confirme.
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

    /// Attend les touches du raccourci de `action`.
    pub(super) fn key_edit(&mut self, action: &'static str, cx: &mut Context<Self>) {
        self.help.sel = Some(action);
        self.help.edit = Some(KeyEdit { action, keys: String::new(), issue: None });
        cx.notify();
    }

    /// Une frappe pendant qu'un raccourci est attendu : elle le devient, sauf Échap (renoncer),
    /// Retour arrière (aucun raccourci) et Entrée sur des touches déjà prises (remplacer).
    /// `false` quand rien n'est attendu : la frappe suit son cours.
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

    /// « Remplacer » : les touches proposées vont à l'action, celles qui les avaient les perdent.
    pub(super) fn key_replace(&mut self, cx: &mut Context<Self>) {
        if let Some(KeyEdit { action, keys, issue: Some(KeyIssue::Taken(others)) }) = self.help.edit.take() {
            self.key_set(action, keys, &others, cx);
        }
    }

    /// Donne `keys` à `action` (vides : aucun raccourci) et les retire à `losers`.
    pub(super) fn key_set(&mut self, action: &'static str, keys: String, losers: &[&'static str], cx: &mut Context<Self>) {
        let mut usual: Vec<String> = defaults().into_iter().filter(|bind| bind.action == action).map(|bind| bind.keys).collect();
        usual.dedup();
        // Revenu à ses touches d'origine : plus rien à retenir pour cette action.
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

    /// Remet les touches d'origine d'une action, ou de toutes.
    pub(super) fn key_reset(&mut self, action: Option<&'static str>, cx: &mut Context<Self>) {
        match action {
            Some(action) => drop(self.changes.remove(action)),
            None => self.changes.clear(),
        }
        self.help.edit = None;
        self.rebind(cx);
        vault::save_file("keys", &keys::write(&self.changes));
    }

    /// Applique `changes` : les liaisons en vigueur changent tout de suite, sans redémarrer.
    pub(super) fn rebind(&mut self, cx: &mut Context<Self>) {
        let open = editable();
        self.bound = keys::effective(&defaults(), &self.changes, &|action| open.contains(action));
        install(cx, &self.bound);
        cx.notify();
    }

    /// Les lignes de l'aide que la recherche garde.
    pub(super) fn help_rows(&self, cx: &App) -> keys::Sections {
        let query = self.help.find.as_ref().map(|find| find.read(cx).line.text.clone()).unwrap_or_default();
        let mut sections = keys::filter(help_sections(&self.bound, &self.changes), &query);
        // Ouvert sur un tableau ou un schéma, le panneau commence par ses touches.
        if let Some(at) = self.here(cx).and_then(|here| sections.iter().position(|(title, _)| *title == here)) {
            let first = sections.remove(at);
            sections.insert(0, first);
        }
        sections
    }

    /// La section de l'aide qui parle de ce qui est affiché : un tableau CSV, un schéma, un tableau
    /// de la note sous le curseur ; aucune pour une note ordinaire, l'aide commence par les notes.
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

    /// Le groupe des commandes de ce qu'on regarde, et son nom pour le champ de la palette : un tableau
    /// CSV, un schéma, un tableau de la note sous le curseur, sinon le bloc du curseur.
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

    /// `short` : la fenêtre est basse, le pied « à propos » défile avec la liste au lieu de lui prendre sa place.
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
                .rounded(px(4.))
                .cursor_pointer()
                .text_size(px(11.))
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
            .child(svg().path("search.svg").size(px(14.)).flex_none().text_color(t.soft))
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
                .w(px(370.))
                .max_w_full()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().mb_1().text_color(t.accent).text_size(px(12.)).child(title))
                .children(rows.into_iter().map(|row| {
                    let edit = self.help.edit.as_ref().filter(|edit| Some(edit.action) == row.action);
                    let chosen = row.action.is_some() && row.action == self.help.sel;
                    // Les touches : un bouton quand elles se changent, du texte sinon.
                    let keys = match row.action {
                        None => div().w(px(156.)).flex_none().font_family(mono()).text_size(px(12.)).child(row.keys.clone()),
                        Some(action) => {
                            let shown = if edit.is_some() {
                                tr("Press the keys…", "Tape les touches…").to_string()
                            } else if row.keys.is_empty() {
                                "—".to_string()
                            } else {
                                row.keys.clone()
                            };
                            let key = div()
                                .id(gpui::SharedString::from(format!("key-{action}")))
                                .debug_selector(move || format!("key-{action}"))
                                .px_1()
                                .rounded(px(4.))
                                .bg(t.border.opacity(0.6))
                                .cursor_pointer()
                                .font_family(mono())
                                .text_size(px(12.))
                                .text_color(if edit.is_some() || row.changed { t.accent } else { t.text })
                                .hover(|s| s.bg(t.border))
                                .child(shown)
                                .on_click(cx.listener(move |this, _, _, cx| this.key_edit(action, cx)));
                            let reset = small(format!("key-reset-{action}"), tr("default", "défaut"))
                                .on_click(cx.listener(move |this, _, _, cx| this.key_reset(Some(action), cx)));
                            div()
                                .w(px(156.))
                                .flex_none()
                                .flex()
                                .flex_wrap()
                                .items_start()
                                .gap_1()
                                .child(key)
                                .when(row.changed && edit.is_none(), |d| d.child(reset))
                        }
                    };
                    // Sous la ligne en cours de saisie : quoi faire, ou ce qui coince.
                    let note = edit.map(|edit| match &edit.issue {
                        None => div().text_size(px(12.)).text_color(t.soft).child(tr(
                            "Backspace: no shortcut · Esc: cancel",
                            "Retour arrière : aucun raccourci · Échap : annuler",
                        )),
                        Some(KeyIssue::Refused(why)) => div().text_size(px(12.)).child(format!("{} : {why}", keys::show(&edit.keys))),
                        Some(KeyIssue::Taken(others)) => div()
                            .text_size(px(12.))
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
                        .rounded(px(4.))
                        .when(chosen, |d| d.bg(t.selection.opacity(0.5)))
                        .child(div().flex().gap_3().child(keys).child(div().flex_1().min_w_0().text_color(t.soft).child(row.effect)))
                        .children(note)
                }))
        });
        // À propos, en pied de panneau : ce qu'est l'app, et où la trouver.
        let link = |id: &'static str, icon: &'static str, label: &'static str, url: &'static str| {
            div()
                .id(id)
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_1p5()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(t.accent)
                .hover(|s| s.bg(t.border))
                .child(svg().path(icon).size(px(14.)).flex_none().text_color(t.accent))
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
            .child(div().flex_1().min_w(px(200.)).text_color(t.soft).child(tr(
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
            // Calé en haut : le champ de recherche ne bouge pas quand la liste raccourcit.
            .items_start()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.set_help(false, window, cx)),
            )
            .child(
                div()
                    .id("help")
                    // Deux colonnes si la fenêtre est assez large, une seule sinon.
                    .w(px(812.))
                    .max_w_full()
                    .max_h_full()
                    .flex()
                    .flex_col()
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(10.))
                    .shadow_lg()
                    .text_size(px(13.))
                    // Un clic dans le panneau ne le ferme pas.
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
