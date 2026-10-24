//! Drawing the window: title bar, panes, empty page, badges, resize edges.

use super::*;

/// The number in the comments badge: "9+" beyond nine, so that it stays round.
pub(super) fn badge_label(count: usize) -> String {
    if count > 9 { "9+".into() } else { count.to_string() }
}

/// Transparent margin around the window when the app draws its decorations.
pub(super) const SHADOW: Pixels = px(10.);
/// The window buttons' pill: its width (padding, three buttons, gaps, border) and, from the window's edge, its width and margin
/// with some air: what the panel header leaves free under it.
/// Width of the window buttons' pill at scale 1: three 24 px buttons, their gaps and margins.
const PILL_W: f32 = 106.;
pub(super) const PILL_ROOM: f32 = 124.;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        // The spacings of the interface are in rems: they follow its scale.
        window.set_rem_size(t.px(16.));
        // The pane holding the focus is the active one; when the code changed it without moving
        // input (a note already open in the other pane), it follows.
        if let Some(template) = self.template.take() {
            self.new_diagram_from(template, window, cx);
        }
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
            // The drag is over (dropped, or out of the window): no more label or line.
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

        // On Linux the app provides title bar, rounded corners and shadow; on
        // macOS and Windows, gpui announces `Server` and the system takes care of it.
        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => Some(tiling),
            Decorations::Server => None,
        };
        #[cfg(test)]
        let tiling = tiling.or(self.client.then(Default::default));
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
            // One pane, or two side by side: each draws itself as if it were the active one. The comments
            // panel, right of everything, is the active pane's.
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
            // The border between the two can be dragged; a double click puts it back in the middle.
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
            // The pane layout is saved as soon as it changes.
            let state = self.panes_state();
            if state != self.panes_saved {
                vault::save_file("panes", &state);
                self.panes_saved = state;
            }
            let toasts = self.render_toasts(cx);
            let toasts = div().absolute().bottom(px(54.)).left_4().right_4().flex().flex_col().items_end().gap_2().children(toasts);
            body.flex()
                .child(self.render_nav(cx))
                .when(self.nav.panel != Panel::Full, |d| d.child(note).children(comments))
                .children(self.palette.clone())
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
            // Escape in a diagram where nothing is in progress anymore: back to the note.
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
            // Panel separator: it follows the pointer as long as the button is held.
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                // The border between the two panes, likewise.
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
            // The mouse back and forward buttons.
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
            // Escape in the note, when it has nothing to close: the right-click menu.
            .on_action(cx.listener(|this, _: &editor::Cancel, window, cx| {
                this.dismiss_failures();
                this.close_icons(window, cx);
                this.close_menu(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ContextMenu, window, cx| this.menu_from_keys(window, cx)))
            .on_action(|_: &FocusNext, window, _| window.focus_next())
            .on_action(|_: &FocusPrev, window, _| window.focus_prev())
            // The row menu's entries, from the palette, on the file of the active pane.
            .on_action(cx.listener(|this, _: &nav::OpenAside, window, cx| this.file_do(nav::Do::OpenAside, window, cx)))
            .on_action(cx.listener(|this, _: &nav::SetIcon, window, cx| this.file_do(nav::Do::Icon, window, cx)))
            .on_action(cx.listener(|this, _: &nav::ShowFolder, window, cx| this.file_do(nav::Do::List, window, cx)))
            .on_action(cx.listener(|this, _: &nav::CopyLink, window, cx| this.file_do(nav::Do::CopyLink, window, cx)))
            .on_action(cx.listener(|this, _: &nav::CopyPath, window, cx| this.file_do(nav::Do::CopyPath, window, cx)))
            .on_action(cx.listener(|this, _: &nav::CopyRelative, window, cx| this.file_do(nav::Do::CopyRelative, window, cx)))
            .on_action(cx.listener(|this, _: &nav::Reveal, window, cx| this.file_do(nav::Do::Reveal, window, cx)))
            .on_action(cx.listener(|this, _: &nav::MoveTo, window, cx| this.file_do(nav::Do::MoveTo, window, cx)))
            .on_action(cx.listener(|this, _: &CloseHelp, window, cx| {
                if this.help.open {
                    this.set_help(false, window, cx)
                } else if this.pane().zone.is_focused(window) {
                    // A pane that only shows an image: Escape goes back to its note.
                    window.focus(&this.pane().editor.focus_handle(cx));
                }
            }))
            .key_context("Shell")
            .child(body)
            // Its own row under the panes, never over the note's footer (counter, formatting bar).
            .children(self.render_update_banner(cx))
            .when(client, |d| d.child(pill))
            .children(self.render_menu(window, cx))
            .children(self.render_icons(window, cx))
            .children(self.render_drop_chip(window.viewport_size()))
            .when(self.help.open, |d| d.child(self.render_help(window.viewport_size().height < px(420.), cx)));

        // The transparent margin carries the shadow and serves as resize handle.
        div()
            .size_full()
            // Alt + drag, anywhere: moves the window (the capture phase comes
            // before the editor, which does not use Alt with the mouse).
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

/// Window edge or corner under the pointer, within a margin of a few pixels.
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
    // Floating badge at the top right of the note: a thin background and border set it
    // apart from the text, neither blur nor transparency. It also serves as a move
    // handle, like the panel header, the empty part of the rail and the board bar;
    // Alt + drag moves from anywhere. ponytail: no transparent strip
    // over the top of the note, it would get in the way of the first lines.
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
            .debug_selector(|| "window-pill".into())
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

    /// Without a vault: the welcome page, which offers a ready-made one, opens one or clones one.
    pub(super) fn render_welcome(&self, body: Div, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
            // Without a vault, a folder dropped on the welcome page becomes the vault.
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
                        .text_size(t.px(26.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child("Bref"),
                )
                .child(
                    div()
                        .text_color(t.soft)
                        .child(tr(
                            "Your notes live in a folder of Markdown files: the vault.",
                            "Vos notes vivent dans un dossier de fichiers Markdown : le coffre.",
                        )),
                )
                // The vault of the last time is gone (moved, renamed, a disk that is not plugged in): say so.
                .children(self.lost.as_ref().map(|gone| {
                    div()
                        .debug_selector(|| "welcome-lost".into())
                        .max_w(t.px(520.))
                        .px_3()
                        .py_2()
                        .rounded(t.px(8.))
                        .border_1()
                        .border_color(t.border)
                        .bg(t.panel)
                        .text_size(t.px(13.))
                        .child(format!("{} « {} »", tr("The vault was not found:", "Le coffre est introuvable :"), vault::shown(gone)))
                        .child(div().text_color(t.soft).child(tr(
                            "Moved, renamed, or on a disk that is not plugged in? Open it again, or start another one.",
                            "Déplacé, renommé, ou sur un disque qui n'est pas branché ? Rouvrez-le, ou commencez-en un autre.",
                        )))
                }))
                // Shortest path first: a ready-made vault, in one click, without a picker
                // window or a folder to create yourself.
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
                .child(div().text_size(t.px(12.)).text_color(t.soft).child(format!(
                    "{} · {}",
                    keys::of(cx, &OpenVault),
                    tr(
                        "a folder of Markdown notes you already have works as it is",
                        "un dossier de notes Markdown existant fonctionne tel quel",
                    )
                )))
                // The address to clone is asked here too, without a vault.
                .children(self.palette.clone())
    }

    pub(super) fn recalled(&self) -> Vec<recall::Recalled> {
        let notes: Vec<Note> = self.notes.iter().filter(|n| !self.is_template(n)).cloned().collect();
        recall::recall(&notes, today(), std::time::SystemTime::now())
    }

    /// The empty page, under the line where you write: the day (which opens its note), the useful
    /// keys, the notes to reread. Everything is clickable, wraps in a narrow pane and
    /// disappears at the first keystroke.
    pub(super) fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        // `fill`: the line is tinted on hover, like a list row.
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
        // The keys in effect; an action left without a shortcut is not announced.
        let actions: [(Box<dyn gpui::Action>, &str); 3] = [
            (Box::new(OpenPalette), "notes"),
            (Box::new(NewNote), tr("new note", "nouvelle note")),
            (Box::new(ToggleHelp), tr("shortcuts", "raccourcis")),
        ];
        // A vault that cannot be read takes no note: neither a new one nor the day's.
        let closed = self.vault.as_ref().is_some_and(|root| self.unreadable.iter().any(|(dir, _)| dir == root));
        let keys = actions.into_iter().enumerate().filter(|(i, _)| !(closed && *i == 1)).filter_map(|(i, (action, what))| {
            let keys = keys::of(cx, action.as_ref());
            let cap = keycaps(&keys, t, false);
            (!keys.is_empty()).then(|| {
                row(("empty-key", i), false)
                    .gap(t.px(6.))
                    .child(cap)
                    .child(what)
                    .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
            })
        });
        let day = (self.vault.is_some() && !closed).then(|| {
            row(("empty-today", 0), false)
                .child(logo(t))
                .child(recall::long_date(today()))
                .on_click(cx.listener(|this, _, _, cx| this.open_today(cx)))
        });
        let recalled = if self.vault.is_some() { self.recalled() } else { Vec::new() };
        let again = (!recalled.is_empty()).then(|| {
            let rows = recalled.into_iter().enumerate().map(|(i, note)| {
                row(("empty-recall", i), true)
                    .mx(t.px(-6.))
                    .px(t.px(6.))
                    .py(t.px(2.))
                    .child(div().flex_1().min_w_0().truncate().child(note.name))
                    .child(div().flex_none().text_size(t.px(12.)).child(recall::ago(note.days)))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_note(&note.path, cx)))
            });
            div()
                .max_w(t.px(380.))
                .flex()
                .flex_col()
                .child(div().mb_1().text_size(t.px(11.)).text_color(t.soft).child(tr("READ AGAIN", "À RELIRE")))
                .children(rows)
        });
        // In the text column, under its first line.
        div().absolute().top(px(76.)).left_0().right_0().flex().justify_center().child(
            div()
                .w_full()
                .max_w(self.theme.page.measure(self.theme.size) + px(48.))
                .px(t.px(32.))
                .flex()
                .flex_col()
                .gap_4()
                .text_size(t.px(13.))
                .children(day)
                .child(div().flex().flex_wrap().gap_x_4().gap_y_1().children(keys))
                .children(again),
        )
    }

    pub(super) fn render_pane(&mut self, pane: usize, active: bool, client: bool, window: &mut Window, cx: &mut Context<Self>) -> (gpui::AnyElement, Option<gpui::AnyElement>) {
        let t = self.theme;
        let two = self.panes.len() == 2;
        // Copy of the whole note: a plain floating icon, outside the title bar.
        // Comments of the note, on its right: the commented text, then what is said about it. A
        // click leads to the comment, ready to be edited; the check mark resolves it.
        // ponytail: the note is reread on every render (a search for `{==`); keep the
        // list from one version of the text to the next if very long notes suffer from it.
        let said: Vec<(String, String, usize, usize, String)> = match self.pane().file.is_none() && self.pane().listing.is_none() && !self.board_shown(cx) {
            true => markdown::all_comments(self.pane().editor.read(cx).text()).into_iter().map(|n| (n.noted.into(), n.said.into(), n.at, n.inside, n.scope)).collect(),
            false => Vec::new(),
        };
        // Their button, left of the copy one, opens and closes the panel; the
        // note's word counter leaves room for it.
        let talk = (!said.is_empty()).then(|| {
            ui::widgets::button("comments-toggle", "i-chat.svg", !self.comments_shut, t)
                .debug_selector(|| "comments-toggle".into())
                .tooltip(ui::widgets::tip(tr("Comments of the note", "Commentaires de la note"), String::new(), t))
                .absolute()
                .bottom_4()
                .right(t.px(if kanban::is_board(self.pane().editor.read(cx).text()) { 84. } else { 50. }))
                .size(t.px(30.))
                .rounded(t.px(8.))
                .occlude()
                // A badge and their number, whether the panel is open or closed.
                .child(
                    div()
                        .debug_selector(|| "comments-badge".into())
                        .absolute()
                        .top(t.px(-3.))
                        .right(t.px(-3.))
                        .min_w(t.px(15.))
                        .h(t.px(15.))
                        .px(t.px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(t.accent)
                        .text_color(t.bg)
                        .text_size(t.px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(badge_label(said.len())),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.comments_shut = !this.comments_shut;
                    cx.notify();
                }))
        });
        // A note that is a kanban board: its button switches from the board to the text. The
        // board hides the note, which must no longer receive keystrokes.
        let board = self.board_shown(cx);
        if board && self.pane().editor.focus_handle(cx).is_focused(window) {
            window.focus(&self.pane().board.focus);
        }
        if !cx.has_active_drag() {
            self.pane_mut().board.drag = None;
        }
        // The list page closed with input in its filter or one of its cells: nothing
        // receives it anymore, it goes back to the note.
        if active && self.pane().listing.is_none() && window.focused(cx).is_none() {
            window.focus(&self.pane().editor.focus_handle(cx));
        }
        if !board {
            self.pane_mut().board.field = None;
            // The board left (another note opens), input goes back to the note.
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
                // Always in the same place, board or text: left of the copy button.
                .right(t.px(50.))
                .size(t.px(30.))
                .rounded(t.px(8.))
                .occlude()
                .on_click(cx.listener(|this, _, window, cx| this.toggle_board(window, cx)))
        });
        let pill = client.then(|| window.viewport_size().width - self.nav.left - px(8.) - t.px(PILL_W));
        self.pane().editor.update(cx, |e, _| {
            e.corner = 1 + talk.is_some() as usize + boards as usize;
            e.bar_here = active;
            e.pill = pill;
        });
        // The comments column takes what the panel and the note leave it, 270 px at
        // most; short of 140 px, it does not show (its button stays, a wider window brings it back).
        let column = (active && !said.is_empty() && !self.comments_shut)
            .then(|| {
                self.nav.aside = px(270.);
                let panel = if self.nav.panel == Panel::Split { self.split_width() + nav::DIVIDER } else { px(0.) };
                (self.nav.total - t.px(nav::RAIL) - panel - nav::NOTE_MIN).min(px(270.))
            })
            .filter(|width| *width >= px(140.));
        if active && column.is_none() {
            self.nav.aside = px(0.);
        }
        let cards = said.iter().filter(|_| !self.comments_shut).enumerate().map(|(i, (noted, said, at, inside, scope))| {
            let (at, inside) = (*at, *inside);
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
                // Where it is, in a table: `Cell C2`, above the commented text.
                .when(!scope.is_empty() && !noted.is_empty(), |d| d.child(div().px_1().text_size(t.px(11.)).text_color(t.soft).child(scope.to_string())))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_1()
                        .child(match (noted.is_empty(), scope.is_empty()) {
                            // A row, a column, the table: what it is about is the chip.
                            (true, false) => div().flex_1().min_w_0().px_1().rounded(px(3.)).bg(t.selection).line_clamp(2).child(scope.to_string()),
                            // A comment on its own, in the text: it takes the place.
                            (true, true) => div().flex_1().min_w_0().px_1().text_color(if said.is_empty() { t.soft } else { t.text }).child(if said.is_empty() { tr("(nothing written yet)", "(rien d'écrit pour l'instant)").to_string() } else { said.to_string() }),
                            (false, _) => div().flex_1().min_w_0().px_1().rounded(px(3.)).bg(t.selection).line_clamp(2).child(noted.to_string()),
                        })
                        .child(ui::widgets::button(("resolve", i), "check.svg", false, t).on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.pane().editor.update(cx, |e, cx| e.resolve_comment(at, cx));
                            window.focus(&this.pane().editor.focus_handle(cx));
                        }))),
                )
                .when(!noted.is_empty() || !scope.is_empty(), |d| {
                    d.child(div().text_color(if said.is_empty() { t.soft } else { t.text }).child(match said.is_empty() {
                        true => tr("(nothing written yet)", "(rien d'écrit pour l'instant)").to_string(),
                        false => said.to_string(),
                    }))
                })
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
                .text_size(t.px(13.))
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
            .size(t.px(30.))
            .rounded(t.px(8.))
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| this.copy_all(false, cx)));
        // The image picked in the panel; it goes away as soon as the note takes over again.
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
            Some((path, FileKind::Image)) => {
                // A big picture takes a while: it says so after a moment; a file that is no picture says so too.
                let said = move |text: &'static str| move || div().debug_selector(|| "image-said".into()).text_size(t.px(13.)).text_color(t.soft).child(text).into_any_element();
                note.p_6().flex().items_center().justify_center().child(
                    img(path)
                        .max_w_full()
                        .max_h_full()
                        .object_fit(ObjectFit::ScaleDown)
                        .with_loading(said(tr("Loading…", "Chargement…")))
                        .with_fallback(said(tr("Unreadable image", "Image illisible"))),
                )
            }
            None => {
                let empty = self.pane().editor.read(cx).text().is_empty().then(|| self.render_empty(cx));
                note.child(self.pane().editor.clone()).children(empty).child(copy).children(talk).children(flip)
            }
        };
        // While a file is dragged from the tree, the area that would receive it is tinted.
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
            // Nothing of a pane is drawn in its neighbour, however narrow.
            .overflow_hidden()
            .flex()
            .map(|d| if two && pane == 0 { d.flex_none().w(gpui::relative(ratio)) } else { d.flex_1() })
            // A mouse gesture in a pane counts for it, before its views receive it.
            .capture_any_mouse_down(cx.listener(move |this, _, _, cx| {
                if this.active != pane && pane < this.panes.len() {
                    this.active = pane;
                    cx.notify();
                }
            }))
            // System files over the note: a line between two lines says where they would go.
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
            // The active pane of two: a line across its top, in the full accent (3:1 at least on the page).
            .when(two && active, |d| d.child(div().debug_selector(|| "pane-active".into()).absolute().top_0().left_0().right_0().h(t.px(3.)).bg(t.accent)))
            .children(tint);
        (area.into_any_element(), comments.map(IntoElement::into_any_element))
    }
}
