//! Drawing the window: title bar, panes, empty page, badges, resize edges.

use super::*;

/// Le nombre de la pastille des commentaires : « 9+ » au-delà de neuf, pour qu'elle reste ronde.
pub(super) fn badge_label(count: usize) -> String {
    if count > 9 { "9+".into() } else { count.to_string() }
}

/// Marge transparente autour de la fenêtre quand l'app dessine ses décorations.
pub(super) const SHADOW: Pixels = px(10.);

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        // Le pane qui contient le focus est l'actif ; quand le code en a changé sans déplacer la
        // saisie (une note déjà ouverte dans l'autre pane), elle suit.
        if std::mem::take(&mut self.grab) {
            self.focus_pane(self.active, window, cx);
        } else if let Some(pane) = (0..self.panes.len()).find(|&pane| self.panes[pane].zone.contains_focused(window, cx)) {
            self.active = pane;
        }
        #[cfg(test)]
        let dragging = cx.has_active_drag() || self.keep_aim;
        #[cfg(not(test))]
        let dragging = cx.has_active_drag();
        if !dragging {
            self.pane_aim = None;
            // Le glisser est fini (lâché, ou sorti de la fenêtre) : plus d'étiquette ni de trait.
            if self.drop_aim.is_some() {
                self.aim_drop(None, cx);
            }
        }
        if self.pane().editor.focus_handle(cx).is_focused(window) {
            self.keep_preview();
        }
        self.track(cx);
        let title = match (self.pane().file_path().or(self.pane().path.as_ref()), &self.vault) {
            (Some(p), _) => vault::stem(p),
            (None, Some(_)) => tr("New note", "Nouvelle note").to_string(),
            (None, None) => "Bref".to_string(),
        };
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }

        // Sous Linux l'app fournit barre de titre, coins arrondis et ombre ; sous
        // macOS et Windows, gpui annonce `Server` et le système s'en charge.
        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => Some(tiling),
            Decorations::Server => None,
        };
        let client = tiling.is_some();
        let framed = tiling.is_some_and(|t| !t.is_tiled())
            && !window.is_maximized()
            && !window.is_fullscreen();
        let inset = if framed { SHADOW } else { px(0.) };
        window.set_client_inset(inset);
        self.nav.left = inset;
        self.nav.native_bar = !client;
        self.nav.total = window.viewport_size().width - inset * 2.;

        let pill = self.render_window_controls(window);

        let body = div().flex_1().min_h_0().relative();
        let body = if self.vault.is_none() {
            self.render_welcome(body, cx)
        } else {
            // Un pane, ou deux côte à côte : chacun se dessine comme s'il était l'actif. Le panneau
            // des commentaires, à droite de tout, est celui du pane actif.
            let (two, active) = (self.panes.len() == 2, self.active);
            let (mut areas, mut comments) = (Vec::new(), None);
            for pane in 0..self.panes.len() {
                let (area, said) = self.in_pane(pane, |this| this.render_pane(pane, pane == active, client, window, cx));
                areas.push(area);
                if pane == active {
                    comments = said;
                }
            }
            let mut areas = areas.into_iter();
            // La bordure entre les deux se tire ; un double clic la remet au milieu.
            let divider = two.then(|| {
                div()
                    .id("pane-divider")
                    .group("pane-divider")
                    .w(px(5.))
                    .h_full()
                    .flex_none()
                    .cursor(CursorStyle::ResizeLeftRight)
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w(px(1.))
                            .h_full()
                            .bg(if self.pane_drag { t.accent } else { t.border })
                            .group_hover("pane-divider", |s| s.bg(t.accent)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &gpui::MouseDownEvent, _, cx| {
                            match e.click_count >= 2 {
                                true => this.pane_ratio = 0.5,
                                false => this.pane_drag = true,
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            });
            let area = self.pane_box.clone();
            let note = div()
                .flex_1()
                .min_w_0()
                .h_full()
                .relative()
                .flex()
                .child(gpui::canvas(move |bounds, _, _| area.set(bounds), |_, _, _, _| ()).absolute().size_full())
                .children(areas.next())
                .children(divider)
                .children(areas.next());
            // La disposition des panes est retenue dès qu'elle change.
            let state = self.panes_state();
            if state != self.panes_saved {
                vault::save_file("panes", &state);
                self.panes_saved = state;
            }
            let toasts = self.render_toasts(cx);
            let banner = self.render_update_banner(cx);
            // La bannière tient à gauche du compteur de mots ; dans une fenêtre étroite, où elle le
            // couvrirait, elle rejoint la pile des messages, au-dessus de lui.
            let narrow = self.nav.total < px(640.);
            let (low, stacked) = if narrow { (None, banner) } else { (banner, None) };
            let toasts = div().absolute().bottom(px(54.)).left_4().right_4().flex().flex_col().items_end().gap_2().children(stacked).children(toasts);
            body.flex()
                .child(self.render_nav(cx))
                .when(self.nav.panel != Panel::Full, |d| d.child(note).children(comments))
                .children(self.palette.clone())
                .children(low.map(|banner| banner.absolute().bottom_3().left_16()))
                .child(toasts)
        };

        let content = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.bg)
            .text_color(t.text)
            .font_family(sans())
            .when(framed, |d| {
                d.rounded(px(12.)).border_1().border_color(t.border).shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.35),
                    offset: point(px(0.), px(2.)),
                    blur_radius: SHADOW / 2.,
                    spread_radius: px(0.),
                }])
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OpenPalette, window, cx| {
                this.open_palette("", window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewNote, window, cx| this.new_note_here(window, cx)))
            .on_action(cx.listener(|this, _: &NewFromTemplate, window, cx| this.open_templates(TemplateAct::New, window, cx)))
            .on_action(cx.listener(|this, _: &NewDiagram, window, cx| this.new_diagram(window, cx)))
            // Échap dans un schéma où plus rien n'est en cours : retour à la note.
            .on_action(cx.listener(|this, _: &canvas::Cancel, window, cx| {
                window.focus(&this.pane().editor.focus_handle(cx));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &nav::ShowTree, window, cx| {
                this.show_nav(Mode::Tree, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowRecent, window, cx| {
                this.show_nav(Mode::Recent, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::NewFolder, window, cx| this.new_folder(window, cx)))
            .on_action(cx.listener(|this, _: &nav::ShowGraph, window, cx| {
                this.show_nav(Mode::Graph, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowTags, window, cx| {
                this.show_nav(Mode::Tags, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowCalendar, window, cx| {
                this.show_nav(Mode::Calendar, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ShowLinks, window, cx| {
                this.show_nav(Mode::Links, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &nav::ToggleFull, window, cx| this.toggle_full(window, cx)))
            // Séparateur du panneau : il suit le pointeur tant que le bouton est tenu.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                // La bordure entre les deux panes, de même.
                if this.pane_drag {
                    let area = this.pane_box.get();
                    if e.pressed_button == Some(MouseButton::Left) && area.size.width > px(0.) {
                        let ratio = (e.position.x - area.left()) / area.size.width;
                        this.pane_ratio = ratio.clamp(*PANE_RATIO.start(), *PANE_RATIO.end());
                    } else {
                        this.pane_drag = false;
                    }
                    cx.notify();
                }
                if !this.nav.dragging {
                } else if e.pressed_button == Some(MouseButton::Left) {
                    this.drag_nav(e.position.x, cx)
                } else {
                    this.settle_nav(window, cx)
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if std::mem::take(&mut this.pane_drag) {
                        cx.notify();
                    }
                    if this.nav.dragging {
                        this.settle_nav(window, cx)
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &sheet::PickDelimiter, window, cx| this.choose_table(Pick::Delimiter, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickEncoding, window, cx| this.choose_table(Pick::Encoding, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickHeader, window, cx| this.choose_table(Pick::Header, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickCopyAs, window, cx| this.choose_table(Pick::CopyAs, window, cx)))
            .on_action(cx.listener(|this, _: &sheet::PickExport, window, cx| this.choose_table(Pick::Export, window, cx)))
            .on_action(cx.listener(|this, _: &OpenVault, window, cx| this.choose_vault(window, cx)))
            .on_action(cx.listener(|this, _: &CopyAll, _, cx| this.copy_all(true, cx)))
            .on_action(cx.listener(|this, _: &Outline, window, cx| this.open_outline(window, cx)))
            .on_action(cx.listener(|this, _: &Today, _, cx| this.open_today(cx)))
            .on_action(cx.listener(|this, _: &SyncVault, window, cx| this.git_sync(window, cx)))
            .on_action(cx.listener(|this, _: &SplitPane, window, cx| this.split(window, cx)))
            .on_action(cx.listener(|this, _: &FocusLeft, window, cx| this.focus_pane(0, window, cx)))
            .on_action(cx.listener(|this, _: &FocusRight, window, cx| this.focus_pane(1, window, cx)))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| this.close_pane(window, cx)))
            .on_action(cx.listener(|this, _: &GoBack, window, cx| this.travel(false, window, cx)))
            .on_action(cx.listener(|this, _: &GoForward, window, cx| this.travel(true, window, cx)))
            // Les boutons précédent et suivant de la souris.
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Back),
                cx.listener(|this, _, window, cx| this.travel(false, window, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(gpui::NavigationDirection::Forward),
                cx.listener(|this, _, window, cx| this.travel(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &SearchVault, window, cx| this.open_search(window, cx)))
            .on_action(cx.listener(|this, _: &ChooseTheme, window, cx| {
                this.choose_setting(Setting::Theme, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.resize_text(Some(1.), window, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| this.resize_text(Some(-1.), window, cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, window, cx| this.resize_text(None, window, cx)))
            .on_action(cx.listener(|this, _: &ToggleHelp, window, cx| {
                this.set_help(!this.help.open, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleCommands, window, cx| this.open_commands(window, cx)))
            // Échap dans la note, quand elle n'a rien à fermer : le menu du clic droit.
            .on_action(cx.listener(|this, _: &editor::Cancel, _, cx| {
                this.icon_pick = None;
                this.menu = None;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CloseHelp, window, cx| {
                if this.help.open {
                    this.set_help(false, window, cx)
                } else if this.pane().zone.is_focused(window) {
                    // Un pane qui ne montre qu'une image : Échap revient à sa note.
                    window.focus(&this.pane().editor.focus_handle(cx));
                }
            }))
            .key_context("Shell")
            .child(body)
            .when(client, |d| d.child(pill))
            .children(self.render_menu(window, cx))
            .children(self.render_icons(window, cx))
            .children(self.render_drop_chip(window.viewport_size()))
            .when(self.help.open, |d| d.child(self.render_help(window.viewport_size().height < px(420.), cx)));

        // La marge transparente porte l'ombre et sert de poignée de redimensionnement.
        div()
            .size_full()
            // Alt + glisser, n'importe où : déplace la fenêtre (la phase de capture passe
            // avant l'éditeur, qui n'utilise pas Alt avec la souris).
            .when(client, |d| {
                d.capture_any_mouse_down(|e, window, cx| {
                    if e.button == MouseButton::Left && e.modifiers.alt {
                        window.start_window_move();
                        cx.stop_propagation();
                    }
                })
            })
            .when(framed, |d| {
                d.p(SHADOW)
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                        let edge = resize_edge(e.position, window.viewport_size());
                        if edge != this.edge {
                            this.edge = edge;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, _| {
                            if let Some(edge) = this.edge {
                                window.start_window_resize(edge)
                            }
                        }),
                    )
                    .when_some(self.edge, |d, edge| {
                        d.cursor(match edge {
                            ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                            ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
                            ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                CursorStyle::ResizeUpLeftDownRight
                            }
                            ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                CursorStyle::ResizeUpRightDownLeft
                            }
                        })
                    })
            })
            .child(content)
    }
}

/// Bord ou coin de fenêtre sous le pointeur, dans une marge de quelques pixels.
pub(super) fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>) -> Option<ResizeEdge> {
    let m = SHADOW;
    let (left, right) = (pos.x < m, pos.x > size.width - m);
    let (top, bottom) = (pos.y < m, pos.y > size.height - m);
    Some(match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Top,
        (_, true, ..) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        (_, _, _, true) => ResizeEdge::Right,
        _ => return None,
    })
}

impl Shell {
    // Pastille flottante en haut à droite de la note : un fond et un bord fins la
    // distinguent du texte, ni flou ni transparence. Elle sert aussi de poignée de
    // déplacement, comme l'en-tête du panneau, le vide du rail et la barre du tableau ;
    // Alt + glisser déplace depuis n'importe où. ponytail: pas de bande transparente
    // sur le haut de la note, elle gênerait les premières lignes.
    pub(super) fn render_window_controls(&self, window: &Window) -> Div {
        let t = self.theme;
        let controls = window.window_controls();
        div()
            .absolute()
            .top_2()
            .right_2()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(px(10.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.18),
                offset: point(px(0.), px(1.)),
                blur_radius: px(6.),
                spread_radius: px(0.),
            }])
            .occlude()
            .map(drag_window)
            .when(controls.minimize, |d| {
                d.child(icon_button("minimize", "minimize.svg", t).on_click(|_, window, _| window.minimize_window()))
            })
            .when(controls.maximize, |d| {
                let icon = if window.is_maximized() { "restore.svg" } else { "maximize.svg" };
                d.child(icon_button("maximize", icon, t).on_click(|_, window, _| window.zoom_window()))
            })
            .child(icon_button("close", "close.svg", t).on_click(|_, window, _| window.remove_window()))
    }

    /// Sans coffre : l'accueil, qui en propose un tout prêt, en ouvre un ou en clone un.
    pub(super) fn render_welcome(&self, body: Div, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
            // Sans coffre, un dossier lâché sur l'accueil devient le coffre.
            body.drag_over::<gpui::ExternalPaths>(move |style, _, _, _| style.bg(t.selection))
                .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                    if let Some(dir) = paths.paths().iter().find(|p| p.is_dir()) {
                        this.set_vault(dir.clone(), window, cx);
                    }
                }))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_4()
                .text_center()
                .child(
                    div()
                        .text_size(px(26.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child("Bref"),
                )
                .child(
                    div()
                        .text_color(t.soft)
                        .child(tr(
                            "Your notes live in a folder of Markdown files: the vault.",
                            "Tes notes vivent dans un dossier de fichiers Markdown : le coffre.",
                        )),
                )
                // Le plus court chemin d'abord : un coffre tout prêt, d'un clic, sans fenêtre de
                // sélection ni dossier à créer soi-même.
                .children(vault::default_vault().map(|place| {
                    div()
                        .id("welcome-create")
                        .debug_selector(|| "welcome-create".into())
                        .mt_4()
                        .px_4()
                        .py_2()
                        .rounded(px(8.))
                        .bg(t.accent)
                        .text_color(t.bg)
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.9))
                        .child(format!("{} {}", tr("Start in", "Commencer dans"), vault::shown(&place)))
                        .on_click(cx.listener(move |this, _, window, cx| this.create_vault(place.clone(), window, cx)))
                }))
                .child(
                    div()
                        .id("welcome-open")
                        .px_3()
                        .py_1()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_color(t.accent)
                        .hover(|s| s.bg(t.border))
                        .child(tr("Open another folder…", "Ouvrir un autre dossier…"))
                        .on_click(cx.listener(|this, _, window, cx| this.choose_vault(window, cx))),
                )
                .child(
                    div()
                        .id("welcome-clone")
                        .px_3()
                        .py_1()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_color(t.accent)
                        .hover(|s| s.bg(t.border))
                        .child(tr("Clone a vault from a git address…", "Cloner un coffre depuis une adresse git…"))
                        .on_click(cx.listener(|this, _, window, cx| this.ask_clone(window, cx))),
                )
                .child(div().text_size(px(12.)).text_color(t.soft).child(format!(
                    "{} · {}",
                    keys::of(cx, &OpenVault),
                    tr(
                        "an existing Obsidian vault works as it is",
                        "un coffre Obsidian existant fonctionne tel quel",
                    )
                )))
                // L'adresse à cloner se demande ici aussi, sans coffre.
                .children(self.palette.clone())
    }

    pub(super) fn recalled(&self) -> Vec<recall::Recalled> {
        let notes: Vec<Note> = self.notes.iter().filter(|n| !self.is_template(n)).cloned().collect();
        recall::recall(&notes, today(), std::time::SystemTime::now())
    }

    /// La page vide, sous la ligne où l'on écrit : le jour (qui ouvre sa note), les touches
    /// utiles, les notes à relire. Tout se clique, passe à la ligne dans un pane étroit et
    /// disparaît à la première frappe.
    pub(super) fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        // `fill` : la ligne se teinte au survol, comme une ligne de liste.
        let row = |id: (&'static str, usize), fill: bool| {
            div()
                .id(id)
                .debug_selector(move || if id.0 == "empty-today" { id.0.to_string() } else { format!("{}-{}", id.0, id.1) })
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(t.soft)
                .hover(|s| if fill { s.text_color(t.text).bg(t.border) } else { s.text_color(t.text) })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        // Les touches en vigueur ; une action laissée sans raccourci n'est pas annoncée.
        let actions: [(Box<dyn gpui::Action>, &str); 3] = [
            (Box::new(OpenPalette), "notes"),
            (Box::new(NewNote), tr("new note", "nouvelle note")),
            (Box::new(ToggleHelp), tr("shortcuts", "raccourcis")),
        ];
        let keys = actions.into_iter().enumerate().filter_map(|(i, (action, what))| {
            let keys = keys::of(cx, action.as_ref());
            let cap = div().px(px(5.)).rounded(px(4.)).border_1().border_color(t.border).bg(t.panel).text_size(px(11.)).child(keys.clone());
            (!keys.is_empty()).then(|| {
                row(("empty-key", i), false)
                    .gap(px(6.))
                    .child(cap)
                    .child(what)
                    .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
            })
        });
        let day = self.vault.is_some().then(|| {
            row(("empty-today", 0), false)
                .child(logo(t))
                .child(recall::long_date(today()))
                .on_click(cx.listener(|this, _, _, cx| this.open_today(cx)))
        });
        let recalled = if self.vault.is_some() { self.recalled() } else { Vec::new() };
        let again = (!recalled.is_empty()).then(|| {
            let rows = recalled.into_iter().enumerate().map(|(i, note)| {
                row(("empty-recall", i), true)
                    .mx(px(-6.))
                    .px(px(6.))
                    .py(px(2.))
                    .child(div().flex_1().min_w_0().truncate().child(note.name))
                    .child(div().flex_none().text_size(px(12.)).child(recall::ago(note.days)))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_note(&note.path, cx)))
            });
            div()
                .max_w(px(380.))
                .flex()
                .flex_col()
                .child(div().mb_1().text_size(px(11.)).text_color(t.soft).child(tr("READ AGAIN", "À RELIRE")))
                .children(rows)
        });
        // Dans la colonne du texte, sous sa première ligne.
        div().absolute().top(px(76.)).left_0().right_0().flex().justify_center().child(
            div()
                .w_full()
                .max_w(self.theme.page.measure(self.theme.size) + px(48.))
                .px(px(32.))
                .flex()
                .flex_col()
                .gap_4()
                .text_size(px(13.))
                .children(day)
                .child(div().flex().flex_wrap().gap_x_4().gap_y_1().children(keys))
                .children(again),
        )
    }

    pub(super) fn render_pane(&mut self, pane: usize, active: bool, client: bool, window: &mut Window, cx: &mut Context<Self>) -> (gpui::AnyElement, Option<gpui::AnyElement>) {
        let t = self.theme;
        let two = self.panes.len() == 2;
        // Copie de toute la note : simple icône flottante, hors de la barre de titre.
        // Commentaires de la note, à sa droite : le texte commenté, puis ce qu'on en dit. Un
        // clic mène au commentaire, prêt à être retouché ; la coche le résout.
        // ponytail: la note est relue à chaque rendu (une recherche de `{==`) ; garder la
        // liste d'une version du texte à l'autre si de très longues notes en pâtissent.
        let said: Vec<(String, String, usize)> = match self.pane().file.is_none() && self.pane().listing.is_none() && !self.board_shown(cx) {
            true => markdown::all_comments(self.pane().editor.read(cx).text()).into_iter().map(|(noted, said, at)| (noted.into(), said.into(), at)).collect(),
            false => Vec::new(),
        };
        // Leur bouton, à gauche de celui de la copie, ouvre et referme le panneau ; le
        // compteur de mots de la note lui laisse la place.
        let talk = (!said.is_empty()).then(|| {
            ui::widgets::button("comments-toggle", "i-chat.svg", !self.comments_shut, t)
                .debug_selector(|| "comments-toggle".into())
                .tooltip(ui::widgets::tip(tr("Comments of the note", "Commentaires de la note"), String::new(), t))
                .absolute()
                .bottom_4()
                .right(px(if kanban::is_board(self.pane().editor.read(cx).text()) { 84. } else { 50. }))
                .size(px(30.))
                .rounded(px(8.))
                .occlude()
                // Une pastille et leur nombre, que le panneau soit ouvert ou refermé.
                .child(
                    div()
                        .debug_selector(|| "comments-badge".into())
                        .absolute()
                        .top(px(-3.))
                        .right(px(-3.))
                        .min_w(px(15.))
                        .h(px(15.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(t.accent)
                        .text_color(t.bg)
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(badge_label(said.len())),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.comments_shut = !this.comments_shut;
                    cx.notify();
                }))
        });
        // Une note qui est un tableau kanban : son bouton passe du tableau au texte. Le
        // tableau cache la note, qui ne doit plus recevoir la frappe.
        let board = self.board_shown(cx);
        if board && self.pane().editor.focus_handle(cx).is_focused(window) {
            window.focus(&self.pane().board.focus);
        }
        if !cx.has_active_drag() {
            self.pane_mut().board.drag = None;
        }
        // La page liste refermée avec la saisie dans son filtre ou une de ses cellules : plus
        // rien ne la reçoit, elle revient à la note.
        if active && self.pane().listing.is_none() && window.focused(cx).is_none() {
            window.focus(&self.pane().editor.focus_handle(cx));
        }
        if !board {
            self.pane_mut().board.field = None;
            // Le tableau quitté (une autre note s'ouvre), la saisie revient à la note.
            if self.pane().board.focus.is_focused(window) {
                window.focus(&self.pane().editor.focus_handle(cx));
            }
        }
        let boards = kanban::is_board(self.pane().editor.read(cx).text()) && self.pane().file.is_none() && self.pane().listing.is_none();
        let flip = boards.then(|| {
            ui::widgets::button("board-toggle", "board.svg", board, t)
                .tooltip(ui::widgets::tip(if board { tr("Show the text", "Voir le texte") } else { tr("Show the board", "Voir le tableau") }, String::new(), t))
                .absolute()
                .bottom_4()
                // Toujours à la même place, tableau ou texte : à gauche de la copie.
                .right(px(50.))
                .size(px(30.))
                .rounded(px(8.))
                .occlude()
                .on_click(cx.listener(|this, _, window, cx| this.toggle_board(window, cx)))
        });
        self.pane().editor.update(cx, |e, _| {
            e.corner = 1 + talk.is_some() as usize + boards as usize;
            e.bar_here = active;
        });
        // La colonne des commentaires prend ce que le panneau et la note lui laissent, 270 px au
        // plus ; faute de 140 px, elle ne se montre pas (son bouton reste, la fenêtre élargie la rend).
        let column = (active && !said.is_empty() && !self.comments_shut)
            .then(|| {
                self.nav.aside = px(270.);
                let panel = if self.nav.panel == Panel::Split { self.split_width() + nav::DIVIDER } else { px(0.) };
                (self.nav.total - nav::RAIL - panel - nav::NOTE_MIN).min(px(270.))
            })
            .filter(|width| *width >= px(140.));
        if active && column.is_none() {
            self.nav.aside = px(0.);
        }
        let cards = said.iter().filter(|_| !self.comments_shut).enumerate().map(|(i, (noted, said, at))| {
            let at = *at;
            let inside = at + noted.len() + 9;
            div()
                .id(("comment", i))
                .p_2()
                .rounded(px(6.))
                .border_1()
                .border_color(t.border)
                .bg(t.bg)
                .flex()
                .flex_col()
                .gap_1()
                .cursor_pointer()
                .hover(|s| s.border_color(t.accent.opacity(0.6)))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_1()
                        .child(div().flex_1().min_w_0().px_1().rounded(px(3.)).bg(t.accent.opacity(0.22)).line_clamp(2).child(noted.to_string()))
                        .child(ui::widgets::button(("resolve", i), "check.svg", false, t).on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.pane().editor.update(cx, |e, cx| e.resolve_comment(at, cx));
                            window.focus(&this.pane().editor.focus_handle(cx));
                        }))),
                )
                .child(div().text_color(if said.is_empty() { t.soft } else { t.text }).child(match said.is_empty() {
                    true => tr("(nothing written yet)", "(rien d'écrit pour l'instant)").to_string(),
                    false => said.to_string(),
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pane().editor.update(cx, |e, cx| e.jump(inside, false, cx));
                    window.focus(&this.pane().editor.focus_handle(cx));
                }))
        });
        let comments = column.map(|column| {
            let title = format!("{} ({})", tr("Comments", "Commentaires"), said.len());
            let shut = ui::widgets::button("comments-shut", "close.svg", false, t).tooltip(ui::widgets::tip(tr("Close", "Fermer"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| {
                this.comments_shut = true;
                cx.notify();
            }));
            div()
                .id("comments")
                .debug_selector(|| "comments".into())
                .w(column)
                .flex_none()
                .h_full()
                .pt(px(48.))
                .px_3()
                .pb_3()
                .border_l_1()
                .border_color(t.border)
                .bg(t.panel)
                .text_size(px(13.))
                .flex()
                .flex_col()
                .gap_2()
                .overflow_y_scroll()
                .child(div().flex().items_center().child(div().flex_1().text_color(t.soft).child(title)).child(shut))
                .children(cards)
        });
        let copy = icon_button("copy-all", if self.copied { "check.svg" } else { "copy.svg" }, t)
            .tooltip(ui::widgets::tip(tr("Copy the note", "Copier la note"), keys::of(cx, &CopyAll), t))
            .absolute()
            .bottom_4()
            .right_4()
            .size(px(30.))
            .rounded(px(8.))
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| this.copy_all(false, cx)));
        // L'image choisie dans le panneau ; elle s'efface dès que la note reprend la main.
        if self.pane().editor.focus_handle(cx).is_focused(window) {
            self.set_file(None, cx);
            self.pane_mut().listing = None;
            self.pane_mut().figure = None;
        }
        let note = div().flex_1().min_w_0().h_full().relative();
        let listing = self.pane().listing.is_some().then(|| self.render_listing(cx));
        let file = self.pane().file.as_ref().map(|file| (file.path.clone(), file.kind.clone()));
        let note = match file {
            _ if listing.is_some() => note.children(listing),
            _ if let Some(figure) = self.pane().figure.clone() => {
                figure.update(cx, |figure, _| figure.sync(t));
                note.child(figure)
            }
            _ if board => note.child(self.render_board(cx)).children(flip),
            Some((_, FileKind::Sheet(sheet))) => {
                sheet.update(cx, |sheet, _| sheet.sync(t, client));
                note.child(sheet)
            }
            Some((_, FileKind::Drawing(canvas))) => {
                canvas.update(cx, |canvas, _| canvas.sync(t, client));
                note.child(canvas)
            }
            Some((_, FileKind::Svg(viewer))) => {
                viewer.update(cx, |viewer, _| viewer.sync(t));
                note.child(viewer)
            }
            Some((path, FileKind::Image)) => note.p_6().flex().items_center().justify_center().child(
                img(path).max_w_full().max_h_full().object_fit(ObjectFit::ScaleDown),
            ),
            None => {
                let empty = self.pane().editor.read(cx).text().is_empty().then(|| self.render_empty(cx));
                note.child(self.pane().editor.clone()).children(empty).child(copy).children(talk).children(flip)
            }
        };
        // Pendant qu'on glisse un fichier de l'arbre, la zone qui le recevrait est teintée.
        let tint = self.pane_aim.filter(|(over, _)| *over == pane).map(|(_, half)| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .map(|d| if half { d.w(gpui::relative(0.5)) } else { d.left_0() })
                .bg(t.accent.opacity(0.12))
                .border_1()
                .border_color(t.accent.opacity(0.5))
        });
        let ratio = self.pane_ratio;
        let area = div()
            .id(("pane", pane))
            .debug_selector(move || format!("pane-{pane}"))
            .track_focus(&self.pane().zone)
            .relative()
            .h_full()
            .min_w_0()
            // Rien d'un pane ne se dessine chez son voisin, si étroit soit-il.
            .overflow_hidden()
            .flex()
            .map(|d| if two && pane == 0 { d.flex_none().w(gpui::relative(ratio)) } else { d.flex_1() })
            // Un geste de souris dans un pane vaut pour lui, avant que ses vues ne le reçoivent.
            .capture_any_mouse_down(cx.listener(move |this, _, _, cx| {
                if this.active != pane && pane < this.panes.len() {
                    this.active = pane;
                    cx.notify();
                }
            }))
            // Des fichiers du système sur la note : un trait entre deux lignes dit où ils se rangeraient.
            .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                let mine = matches!(&this.drop_aim, Some(DropAim { aim: Aim::Note { pane: p, .. }, .. }) if *p == pane);
                let at = e.event.position;
                match e.bounds.contains(&at).then(|| this.note_aim(pane, at, e.drag(cx).paths().len(), cx)).flatten() {
                    Some(aim) => this.aim_drop(Some(aim), cx),
                    None if mine => this.aim_drop(None, cx),
                    None => {}
                }
            }))
            .on_drop(cx.listener(move |this, paths: &gpui::ExternalPaths, _, cx| {
                if let Some(DropAim { aim: aim @ Aim::Note { pane: p, .. }, .. }) = this.drop_aim.clone()
                    && p == pane
                {
                    this.drop_paths(paths.paths().to_vec(), aim, cx);
                }
            }))
            .on_drag_move(cx.listener(move |this, e: &gpui::DragMoveEvent<nav::Dragged>, _, cx| {
                let at = e.event.position;
                let aim = e.bounds.contains(&at).then(|| (pane, this.panes.len() == 1 && at.x > e.bounds.center().x));
                let was = this.pane_aim.filter(|(over, _)| *over == pane);
                if aim != was {
                    this.pane_aim = aim;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(move |this, dragged: &nav::Dragged, window, cx| {
                let aside = this.pane_aim.take().is_some_and(|(_, half)| half);
                if let Some(path) = dragged.paths.first().cloned() {
                    this.drop_on_pane(pane, aside, &path, window, cx);
                }
            }))
            .child(note)
            // Avec deux panes, un filet marque celui qui a la saisie.
            .when(two && active, |d| d.child(div().absolute().top_0().left_0().right_0().h(px(2.)).bg(t.accent.opacity(0.7))))
            .children(tint);
        (area.into_any_element(), comments.map(IntoElement::into_any_element))
    }
}
