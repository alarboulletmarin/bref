//! The canvas as a view: its tool bar, its zoom buttons and its actions.

use super::*;

impl Render for Canvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let (this, focus) = (cx.entity(), self.focus.clone());
        let drawing = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus, ElementInputHandler::new(bounds, this.clone()), cx);
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    this.update(cx, |canvas, cx| canvas.paint(bounds, window, cx))
                })
            },
        )
        .size_full();

        // A tool is chosen with a click; a shape can also be dragged to the canvas.
        let tools = TOOLS.iter().map(|&(tool, icon, key)| {
            let tool_button = button(icon, icon, self.tool == tool, t).tooltip(tip(tool_label(tool), key.to_uppercase(), t)).on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)));
            match tool {
                Tool::Shape(_) => tool_button.on_drag(Placing(tool, icon, t), |placing, _, _, cx| cx.new(|_| placing.clone())),
                _ => tool_button,
            }
        });
        let percent = format!("{} %", (self.zoom * 100.).round());
        let zoom = div()
            .absolute()
            .bottom_3()
            .right_3()
            .p(px(2.))
            .flex()
            .items_center()
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .occlude()
            .child(button("d-zoom-out", "minimize.svg", false, t).tooltip(tip(tr("Zoom out", "Dézoomer"), "-".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(0.8, cx))))
            .child(
                div()
                    .id("d-zoom-fit")
                    .w(px(52.))
                    .text_center()
                    .text_size(px(12.))
                    .text_color(t.soft)
                    .cursor_pointer()
                    .child(percent)
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_fit(cx))),
            )
            .child(button("d-zoom-in", "plus.svg", false, t).tooltip(tip(tr("Zoom in", "Zoomer"), "+".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(1.25, cx))));
        // Selection settings: colour, fill, dashes, arrow heads.
        let any = !self.selected.is_empty();
        let links = self.selected.iter().any(|id| self.diagram.shape(*id).is_none());
        let holder = matches!(self.selected[..], [id] if self.diagram.shape(id).is_some_and(|s| s.form.holds()) && !self.diagram.children(id).is_empty());
        let swatches = (0..diagram::COLORS.len() as u8).map(|i| {
            let color = self.ink(i);
            div()
                .id(("d-color", i as usize))
                .size(px(16.))
                .flex_none()
                .rounded_full()
                .cursor_pointer()
                .bg(color)
                .hover(|s| s.opacity(0.75))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.style(cx, |shape, link| {
                        if let Some(shape) = shape {
                            shape.color = i;
                        }
                        if let Some(link) = link {
                            link.color = i;
                        }
                    })
                }))
        });
        let toggle = |id, icon, cx: &Context<Self>, set: fn(Option<&mut diagram::Shape>, Option<&mut diagram::Link>)| {
            button(id, icon, false, t).on_click(cx.listener(move |this, _, _, cx| this.style(cx, set)))
        };
        // Writes in the text of an end of the selected arrow.
        let text_button = |id: &'static str, icon: &'static str, slot: Slot, tooltip: &'static str, cx: &Context<Self>| {
            button(id, icon, false, t).tooltip(tip(tooltip, String::new(), t)).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(link) = this.selected.iter().copied().find(|id| this.diagram.link(*id).is_some()) {
                    this.stop_editing(cx);
                    this.start_editing_slot(link, slot, cx);
                }
            }))
        };
        // The menu of relationships, a row of its own: those that look like the selected arrow, or the tool, are lit.
        let current = self.selected.iter().find_map(|id| self.diagram.link(*id)).and_then(Relation::of);
        let relation_button = |relation: Relation, cx: &Context<Self>| {
            let lit = match self.tool {
                Tool::Link(tool) => tool == relation,
                _ => current == Some(relation),
            };
            let id = ("d-relation", relation as usize);
            let hint = tip(relation.name(), relation.key().to_uppercase().to_string(), t);
            let pick = cx.listener(move |this, _, _, cx| this.choose_relation(relation, cx));
            match relation.icon() {
                Some(icon) => button(id, icon, lit, t).tooltip(hint).on_click(pick).into_any_element(),
                // A dependency with a stereotype is told by it: its name is its button.
                None => div()
                    .id(id)
                    .h(px(28.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(if lit { t.accent } else { t.soft })
                    .when(lit, |d| d.bg(t.selection))
                    .hover(|s| s.bg(t.border))
                    .child(relation.name())
                    .tooltip(hint)
                    .on_click(pick)
                    .into_any_element(),
            }
        };
        let relations = div().w_full().flex().flex_wrap().items_center().gap_1().children(Relation::ALL.map(|r| relation_button(r, cx))).child(
            button("d-nary", "d-rel-nary.svg", self.tool == Tool::Shape(Form::Branch), t)
                .tooltip(tip(tool_label(Tool::Shape(Form::Branch)), Self::NARY.to_uppercase(), t))
                .on_click(cx.listener(|this, _, _, cx| this.set_tool(Tool::Shape(Form::Branch), cx))),
        );
        // The library: a panel over the canvas, shapes by family and starting points, searched by typing.
        let panel = self.library.as_ref().map(|query| {
            let presets = library::presets();
            let row = |id: gpui::ElementId, icon: &'static str, label: &'static str| {
                div()
                    .id(id)
                    .h(px(26.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .text_color(t.text)
                    .hover(|s| s.bg(t.border))
                    .child(svg().path(icon).size(px(14.)).flex_none().text_color(t.soft))
                    .child(label)
            };
            let title = |text: &'static str| div().px_2().pt_2().pb_1().text_size(px(11.)).text_color(t.soft).child(text.to_uppercase());
            let search = div()
                .h(px(28.))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(6.))
                .bg(t.bg)
                .border_1()
                .border_color(t.border)
                .text_size(px(12.5))
                .child(svg().path("search.svg").size(px(14.)).flex_none().text_color(t.soft))
                .child(if query.is_empty() { div().text_color(t.soft).child(tr("Search shapes…", "Chercher une forme…")) } else { div().child(format!("{query}▏")) });
            let mut body = div().flex().flex_col().gap_px().child(search);
            // Starting points are listed with no search, or when their name answers it.
            let starts: Vec<(usize, library::Template)> = library::templates().into_iter().enumerate().filter(|(_, t)| t.matches(query)).collect();
            if !starts.is_empty() {
                body = body.child(title(tr("Starting points", "Points de départ")));
                for (i, template) in starts {
                    body = body.child(row(("d-start", i).into(), "file-plus.svg", template.name).on_click(cx.listener(move |this, _, _, cx| {
                        this.library = None;
                        cx.emit(CanvasEvent::Template(i));
                        cx.notify();
                    })));
                }
            }
            let mut family = "";
            for (i, preset) in presets.iter().enumerate().filter(|(_, p)| p.matches(query)) {
                if preset.family != family {
                    family = preset.family;
                    body = body.child(title(family));
                }
                body = body.child(
                    row(("d-preset", i).into(), preset.icon, preset.name)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_preset(i, cx)))
                        .on_drag(Placing(Tool::Preset(i), preset.icon, t), |placing, _, _, cx| cx.new(|_| placing.clone())),
                );
            }
            if presets.iter().all(|p| !p.matches(query)) {
                body = body.child(div().px_2().py_1().text_size(px(12.)).text_color(t.soft).child(tr("No shape answers.", "Aucune forme ne répond.")));
            }
            div()
                .id("d-library-panel")
                .absolute()
                .top_3()
                .left_3()
                .w(px(244.))
                .max_h(px(560.))
                .overflow_y_scroll()
                .p_2()
                .rounded(px(8.))
                .bg(t.panel)
                .border_1()
                .border_color(t.border)
                .occlude()
                .child(body)
        });
        // In a narrow window, the bar wraps: no tool is out of reach.
        let bar = div()
            .flex_none()
            .min_h(px(44.))
            .pl_2()
            .pr(px(if self.client { 128. } else { 8. }))
            .py_1()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .children(tools)
            .when(any, |d| {
                d.child(div().w(px(1.)).h(px(16.)).mx_1().bg(t.border))
                    .children(swatches)
                    .child(div().w(px(4.)))
                    .child(toggle("d-fill", "d-fill.svg", cx, |shape, _| {
                        if let Some(shape) = shape {
                            shape.fill = !shape.fill;
                        }
                    }))
                    .child(toggle("d-dash", "d-dash.svg", cx, |shape, link| {
                        if let Some(shape) = shape {
                            shape.dashed = !shape.dashed;
                        }
                        if let Some(link) = link {
                            link.dashed = !link.dashed;
                        }
                    }))
            })
            .when(holder, |d| {
                d.child(button("d-fit", "d-fit.svg", false, t).tooltip(tip(tr("Fit to what it holds", "Ajuster à son contenu"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| this.fit_container(cx))))
            })
            .when(links, |d| {
                d.child(button("d-route", "d-route.svg", false, t).tooltip(tip(tr("Route: elbowed, straight or curved", "Tracé : coudé, droit ou courbe"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| {
                    // The chosen path also applies to arrows to come.
                    let first = this.selected.iter().find_map(|id| this.diagram.link(*id)).map(|l| l.route.next());
                    if let Some(route) = first {
                        this.route = route;
                        this.style(cx, move |_, link| {
                            if let Some(link) = link {
                                link.route = route;
                            }
                        });
                    }
                })))
                .child(text_button("d-text-start", "d-label-start.svg", Slot::Start, tr("Text at the start of the arrow: a role, a multiplicity", "Texte au début de la flèche : un rôle, une multiplicité"), cx))
                .child(text_button("d-text-end", "d-label-end.svg", Slot::End, tr("Text at the end of the arrow: a role, a multiplicity", "Texte à la fin de la flèche : un rôle, une multiplicité"), cx))
                .child(toggle("d-head-start", "d-head-start.svg", cx, |_, link| {
                    if let Some(link) = link {
                        link.start = link.start.next();
                    }
                }))
                .child(toggle("d-head-end", "d-head-end.svg", cx, |_, link| {
                    if let Some(link) = link {
                        link.end = link.end.next();
                    }
                }))
            })
            .child(
                button("d-library", "d-library.svg", self.library.is_some(), t)
                    .tooltip(tip(tr("Shape library: classes, use cases… and starting points", "Bibliothèque de formes : classes, cas d'utilisation… et points de départ"), Self::LIBRARY.to_uppercase(), t))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_library(cx))),
            )
            .child(
                button("d-relations", "d-rel-menu.svg", self.relations, t)
                    .tooltip(tip(tr("Relationships: generalization, composition, dependency…", "Relations : généralisation, composition, dépendance…"), Self::RELATIONS.to_uppercase(), t))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.relations = !this.relations;
                        cx.notify();
                    })),
            )
            // A diagram made before styled text keeps its texts as they are, until asked.
            .when(!self.diagram.rich, |d| {
                let tooltip = tr("Style the text: *italic*, **bold**, <u>underlined</u>, ~~struck~~", "Mettre le texte en forme : *italique*, **gras**, <u>souligné</u>, ~~barré~~");
                d.child(button("d-rich", "c-italic.svg", false, t).tooltip(tip(tooltip, String::new(), t)).on_click(cx.listener(|this, _, _, cx| this.enrich(cx))))
            })
            .child(div().flex_1())
            .child(button("d-export", if self.exported { "check.svg" } else { "export.svg" }, false, t).tooltip(tip(tr("Export as PNG", "Exporter en PNG"), String::new(), t)).on_click(cx.listener(|_, _, _, cx| cx.emit(CanvasEvent::Export))))
            .when(self.relations, |d| d.child(relations));

        div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("Canvas")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom_by(1.25, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom_by(0.8, cx)))
            .on_action(cx.listener(|this, _: &ZoomFit, _, cx| this.zoom_fit(cx)))
            .on_action(cx.listener(|this, _: &Erase, _, cx| this.erase(false, cx)))
            .on_action(cx.listener(|this, _: &EraseNext, _, cx| this.erase(true, cx)))
            .on_action(cx.listener(|this, _: &Left, _, cx| this.nudge(-1., 0., cx)))
            .on_action(cx.listener(|this, _: &Right, _, cx| this.nudge(1., 0., cx)))
            .on_action(cx.listener(|this, _: &Up, _, cx| this.nudge(0., -1., cx)))
            .on_action(cx.listener(|this, _: &Down, _, cx| this.nudge(0., 1., cx)))
            .on_action(cx.listener(|this, _: &NextText, _, cx| this.next_text(cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.restore(false, cx)))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.restore(true, cx)))
            .on_action(cx.listener(|this, _: &Duplicate, _, cx| {
                if !this.selected.is_empty() {
                    this.remember();
                    this.selected = this.diagram.duplicate(&this.selected, 2. * diagram::GRID);
                    this.changed(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.selected = this.diagram.shapes.iter().map(|s| s.id).chain(this.diagram.links.iter().map(|l| l.id)).collect();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &Paste, _, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|item: ClipboardItem| item.text()) {
                    this.typed(&text, cx);
                }
            }))
            // Enter: write in the selected element; while writing, new line.
            .on_action(cx.listener(|this, _: &Confirm, _, cx| match (this.editing, this.selected.as_slice()) {
                // In the library: Enter picks the first shape that answers the search.
                (None, _) if this.library.is_some() => {
                    let query = this.library.take().unwrap_or_default();
                    if let Some(first) = library::presets().iter().position(|p| p.matches(&query)) {
                        this.choose_preset(first, cx);
                    }
                    cx.notify();
                }
                (Some(_), _) => this.typed("\n", cx),
                (None, &[id]) => this.start_editing(id, cx),
                _ => {}
            }))
            // Escape: finish writing, drop the tool, then the selection; otherwise the window decides.
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                if this.editing.is_some() {
                    this.stop_editing(cx);
                } else if this.library.is_some() {
                    this.library = None;
                    cx.notify();
                } else if this.relations {
                    this.relations = false;
                    cx.notify();
                } else if this.tool != Tool::Select {
                    this.set_tool(Tool::Select, cx);
                } else if !this.selected.is_empty() {
                    this.selected.clear();
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .child(bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .cursor(self.hand)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, e: &MouseDownEvent, _, _| this.drag = Drag::Pan { last: e.position }),
                    )
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(|this, _, _, _| this.drag = Drag::None))
                    .on_scroll_wheel(cx.listener(Self::scroll))
                    .on_drop(cx.listener(|this, placing: &Placing, window, cx| {
                        this.place(placing.0, window.mouse_position(), cx)
                    }))
                    .relative()
                    .child(drawing)
                    .child(zoom)
                    .children(panel),
            )
    }
}
