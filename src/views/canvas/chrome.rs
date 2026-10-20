//! What surrounds the drawing: the dock at the bottom with the menus that open above it, the library
//! drawer and the small cluster of buttons for the whole diagram. Nothing here touches the diagram.

use super::*;
use crate::ui::widgets::keycaps;
use gpui::{Div, Stateful};

/// A row of a menu: an icon, a name, and the key that picks it. `lit`: it is the tool, or what the selection is.
fn row(id: impl Into<gpui::ElementId>, icon: &'static str, label: &'static str, key: String, lit: bool, t: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(12.5))
        .text_color(if lit { t.accent } else { t.text })
        .when(lit, |d| d.bg(t.selection))
        .hover(|s| s.bg(t.border))
        .child(svg().path(icon).size(px(16.)).flex_none().text_color(if lit { t.accent } else { t.soft }))
        .child(div().flex_1().truncate().child(label))
        .child(keycaps(&key, t, lit))
}

/// The title of a group of rows.
fn title(text: &'static str, t: Theme) -> Div {
    div().px_2().pt_2().pb_1().text_size(px(11.)).text_color(t.soft).child(text.to_uppercase())
}

/// A menu that opens over the canvas.
fn floating(t: Theme) -> Div {
    div().p_1().rounded(px(10.)).bg(t.panel).border_1().border_color(t.border).shadow_md().occlude()
}

impl Canvas {
    /// Closes the menus and the library, the one that is asked for excepted.
    fn only(&mut self, shapes: bool, relations: bool, library: bool) {
        self.shapes = shapes;
        self.relations = relations;
        if !library {
            self.library = None;
        } else if self.library.is_none() {
            self.library = Some(String::new());
        }
    }

    pub(super) fn toggle_shapes(&mut self, cx: &mut Context<Self>) {
        self.only(!self.shapes, false, false);
        cx.notify();
    }

    pub(super) fn toggle_relations(&mut self, cx: &mut Context<Self>) {
        self.only(false, !self.relations, false);
        cx.notify();
    }

    /// The icon of a shape tool, in the bar of tools.
    fn shape_icon(form: Form) -> &'static str {
        TOOLS.iter().find(|(tool, ..)| *tool == Tool::Shape(form)).map_or("d-rect.svg", |(_, icon, _)| icon)
    }

    /// The menu of shapes: the ones the bar used to show side by side, with their names.
    fn shapes_menu(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let rows = TOOLS.iter().filter(|(tool, ..)| matches!(tool, Tool::Shape(form) if *form != Form::Text)).enumerate().map(|(i, &(tool, icon, key))| {
            row(("d-shape", i), icon, tool_name(tool), key.to_uppercase(), self.tool == tool, t)
                .debug_selector(move || format!("d-tool-{key}"))
                .tooltip(tip(tool_label(tool), key.to_uppercase(), t))
                .on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)))
                .on_drag(Placing(tool, icon, t, Point::default()), |placing, grab, _, cx| cx.new(|_| placing.grabbed(grab)))
        });
        floating(t).w(px(236.)).flex().flex_col().children(rows)
    }

    /// The menu of relationships, by family: a name and a letter for each; the one the selected arrow looks like is lit.
    pub(super) fn relations_menu(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let current = self.selected.iter().find_map(|id| self.diagram.link(*id)).and_then(Relation::of);
        let item = |relation: Relation, cx: &mut Context<Self>| {
            let lit = match self.tool {
                Tool::Link(tool) => tool == relation,
                _ => current == Some(relation),
            };
            let icon = relation.icon().unwrap_or("d-rel-dependency.svg");
            row(("d-relation", relation as usize), icon, relation.label(), relation.key().to_uppercase().to_string(), lit, t)
                .debug_selector(move || format!("d-relation-{}", relation as usize))
                .tooltip(tip(relation.name(), String::new(), t))
                .on_click(cx.listener(move |this, _, _, cx| this.choose_relation(relation, cx)))
        };
        let group = |name: &'static str, cx: &mut Context<Self>| {
            let rows: Vec<_> = Relation::ALL.into_iter().filter(|r| r.group() == name).map(|r| item(r, cx)).collect();
            div().flex().flex_col().child(title(name, t)).children(rows)
        };
        let nary = row("d-nary", "d-rel-nary.svg", tool_name(Tool::Shape(Form::Branch)), Self::NARY.to_uppercase(), self.tool == Tool::Shape(Form::Branch), t)
            .tooltip(tip(tool_label(Tool::Shape(Form::Branch)), String::new(), t))
            .on_click(cx.listener(|this, _, _, cx| this.set_tool(Tool::Shape(Form::Branch), cx)));
        let column = |groups: &[&'static str], cx: &mut Context<Self>| {
            let groups: Vec<_> = groups.iter().map(|g| group(g, cx)).collect();
            div().w(px(228.)).flex().flex_col().children(groups)
        };
        let (a, b) = (Relation::Association.group(), Relation::Generalization.group());
        let (c, d) = (Relation::Dependency.group(), Relation::Sync.group());
        floating(t).flex().flex_wrap().max_w(px(480.)).child(column(&[a], cx).child(nary)).child(column(&[b], cx)).child(column(&[c], cx)).child(column(&[d], cx))
    }

    /// The bar at the bottom: select, shapes, arrows, text, and the library.
    fn dock(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let menu = |id: &'static str, icon: &'static str, lit: bool, open: bool| {
            div()
                .id(id)
                .h(px(28.))
                .pl(px(6.))
                .pr(px(3.))
                .flex()
                .items_center()
                .gap(px(2.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(t.border))
                .when(lit || open, |d| d.bg(t.selection))
                .child(svg().path(icon).size(px(16.)).flex_none().text_color(if lit { t.accent } else { t.soft }))
                .child(svg().path("chevron-down.svg").size(px(12.)).flex_none().text_color(t.soft))
        };
        let shape_pick = match self.tool {
            Tool::Shape(form) if TOOLS.iter().any(|(tool, ..)| *tool == Tool::Shape(form)) => form,
            _ => self.shape_pick,
        };
        let link_pick = match self.tool {
            Tool::Link(relation) => relation,
            _ => self.link_pick,
        };
        let tool_button = |tool: Tool, id: &'static str, cx: &mut Context<Self>| {
            let (icon, key) = TOOLS.iter().find(|(t, ..)| *t == tool).map_or(("", ""), |(_, icon, key)| (*icon, *key));
            button(id, icon, self.tool == tool, t)
                .debug_selector(move || format!("d-tool-{key}"))
                .tooltip(tip(tool_label(tool), key.to_uppercase(), t))
                .on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)))
        };
        let divider = || div().w(px(1.)).h(px(16.)).mx_1().bg(t.border);
        div()
            .p(px(3.))
            .flex()
            .items_center()
            .gap_px()
            .rounded(px(10.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow_md()
            .occlude()
            .child(tool_button(Tool::Select, "d-select", cx))
            .child(divider())
            .child(
                menu("d-shapes", Self::shape_icon(shape_pick), matches!(self.tool, Tool::Shape(f) if f != Form::Text && f != Form::Branch), self.shapes)
                    .debug_selector(|| "d-shapes".into())
                    .tooltip(tip(tr("Shapes: rectangle, ellipse, package…", "Formes : rectangle, ellipse, paquetage…"), String::new(), t))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_shapes(cx))),
            )
            .child(
                menu("d-links", link_pick.icon().unwrap_or("d-rel-dependency.svg"), matches!(self.tool, Tool::Link(_)), self.relations)
                    .debug_selector(|| "d-relations".into())
                    .tooltip(tip(tr("Arrows and relationships: generalization, composition, dependency…", "Flèches et relations : généralisation, composition, dépendance…"), Self::RELATIONS.to_uppercase(), t))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_relations(cx))),
            )
            .child(tool_button(Tool::Shape(Form::Text), "d-text", cx))
            .child(divider())
            .child(
                button("d-library", "d-library.svg", self.library.is_some(), t)
                    .debug_selector(|| "d-library".into())
                    .tooltip(tip(tr("Shape library: classes, use cases… and starting points", "Bibliothèque de formes : classes, cas d'utilisation… et points de départ"), Self::LIBRARY.to_uppercase(), t))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_library(cx))),
            )
    }

    /// The dock, and the menu that is open above it.
    pub(super) fn bottom(&self, cx: &mut Context<Self>) -> Div {
        let menu = if self.shapes {
            Some(self.shapes_menu(cx))
        } else if self.relations {
            Some(self.relations_menu(cx))
        } else {
            None
        };
        div().absolute().bottom_0().left_0().right_0().pb_4().px_3().flex().flex_col().items_center().gap_2().children(menu).child(self.dock(cx))
    }

    /// Undo, redo, export: what is for the whole diagram, at the top left.
    pub(super) fn cluster(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let (undo, redo) = (crate::ui::keys::of(cx, &Undo), crate::ui::keys::of(cx, &Redo));
        div()
            .absolute()
            .top_3()
            .left_3()
            .p(px(3.))
            .flex()
            .items_center()
            .gap_px()
            .rounded(px(10.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow_sm()
            .occlude()
            .child(button("d-undo", "c-undo.svg", false, t).tooltip(tip(tr("Undo", "Annuler"), undo, t)).on_click(cx.listener(|this, _, _, cx| this.restore(false, cx))))
            .child(button("d-redo", "c-redo.svg", false, t).tooltip(tip(tr("Redo", "Rétablir"), redo, t)).on_click(cx.listener(|this, _, _, cx| this.restore(true, cx))))
            .when(!self.diagram.messages().is_empty(), |d| {
                d.child(button("d-numbered", "d-seq-number.svg", self.diagram.numbered, t).tooltip(tip(tr("Number the messages", "Numéroter les messages"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| this.toggle_numbers(cx))))
            })
            // A diagram made before styled text keeps its texts as they are, until asked.
            .when(!self.diagram.rich, |d| {
                let tooltip = tr("Style the text: *italic*, **bold**, <u>underlined</u>, ~~struck~~", "Mettre le texte en forme : *italique*, **gras**, <u>souligné</u>, ~~barré~~");
                d.child(button("d-rich", "c-italic.svg", false, t).tooltip(tip(tooltip, String::new(), t)).on_click(cx.listener(|this, _, _, cx| this.enrich(cx))))
            })
            .child(div().w(px(1.)).h(px(16.)).mx_1().bg(t.border))
            .child(button("d-export", if self.exported { "check.svg" } else { "export.svg" }, false, t).tooltip(tip(tr("Export as PNG", "Exporter en PNG"), String::new(), t)).on_click(cx.listener(|_, _, _, cx| cx.emit(CanvasEvent::Export))))
    }

    /// The zoom, at the bottom right.
    pub(super) fn zoom_box(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let percent = format!("{} %", (self.zoom * 100.).round());
        div()
            .absolute()
            .bottom_4()
            .right_3()
            .p(px(3.))
            .flex()
            .items_center()
            .rounded(px(10.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .shadow_sm()
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
                    .tooltip(tip(tr("Fit the diagram", "Ajuster au diagramme"), "0".into(), t))
                    .child(percent)
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_fit(cx))),
            )
            .child(button("d-zoom-in", "plus.svg", false, t).tooltip(tip(tr("Zoom in", "Zoomer"), "+".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(1.25, cx))))
    }

    /// The library: a drawer on the left, shapes by family and starting points, searched by typing.
    pub(super) fn drawer(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let t = self.theme;
        let query = self.library.as_ref()?;
        let presets = library::presets();
        let search = div()
            .h(px(30.))
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
            body = body.child(title(tr("Starting points", "Points de départ"), t));
            for (i, template) in starts {
                body = body.child(row(("d-start", i), "file-plus.svg", template.name, String::new(), false, t).on_click(cx.listener(move |this, _, _, cx| {
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
                body = body.child(title(family, t));
            }
            body = body.child(
                row(("d-preset", i), preset.icon, preset.name, String::new(), self.tool == Tool::Preset(i), t)
                    .debug_selector(move || format!("d-preset-{i}"))
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_preset(i, cx)))
                    .on_drag(Placing(Tool::Preset(i), preset.icon, t, Point::default()), |placing, grab, _, cx| cx.new(|_| placing.grabbed(grab))),
            );
        }
        if presets.iter().all(|p| !p.matches(query)) {
            body = body.child(div().px_2().py_1().text_size(px(12.)).text_color(t.soft).child(tr("No shape answers.", "Aucune forme ne répond.")));
        }
        Some(floating(t).id("d-library-panel").absolute().top(px(60.)).left_3().bottom(px(76.)).w(px(244.)).overflow_y_scroll().p_2().child(body))
    }
}
