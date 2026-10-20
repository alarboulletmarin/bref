//! What surrounds the drawing: the dock at the bottom with the menus that open above it, the library
//! drawer and the small cluster of buttons for the whole diagram. Nothing here touches the diagram.

use super::*;
use crate::ui::widgets::keycaps;
use gpui::{Div, Stateful};

/// A row of a menu: an icon, a name, and the key that picks it. `lit`: it is the tool, or what the selection is;
/// `focus`: it is the row the arrow keys are on.
fn row(id: impl Into<gpui::ElementId>, icon: &'static str, label: &'static str, key: String, lit: bool, focus: bool, t: Theme) -> Stateful<Div> {
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
        .text_color(t.text)
        .when(lit, |d| d.bg(t.code_bg))
        .when(focus, |d| d.bg(t.selection))
        .hover(|s| s.bg(t.border))
        .child(svg().path(icon).size(px(16.)).flex_none().text_color(if lit || focus { t.accent } else { t.soft }))
        .child(div().flex_1().truncate().child(label))
        .child(keycaps(&key, t, focus))
}

/// The title of a group of rows.
fn title(text: &'static str, t: Theme) -> Div {
    div().px_2().pt_2().pb_1().text_size(px(11.)).text_color(t.soft).child(text.to_uppercase())
}

/// A menu that opens over the canvas.
fn floating(t: Theme) -> Div {
    div().p_1().rounded(px(10.)).bg(t.panel).border_1().border_color(t.border).shadow_md().occlude()
}

/// What a row of the menu of a right click does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Ctx {
    Text,
    All,
    Fit,
    Edit,
    Reverse,
    Elbow,
    Straight,
    Curve,
    Loop,
    FitHolder,
    Duplicate,
    Delete,
}

impl Ctx {
    fn id(self) -> &'static str {
        match self {
            Ctx::Text => "d-cm-text",
            Ctx::All => "d-cm-all",
            Ctx::Fit => "d-cm-fit",
            Ctx::Edit => "d-cm-edit",
            Ctx::Reverse => "d-cm-reverse",
            Ctx::Elbow => "d-cm-elbow",
            Ctx::Straight => "d-cm-straight",
            Ctx::Curve => "d-cm-curve",
            Ctx::Loop => "d-cm-loop",
            Ctx::FitHolder => "d-cm-fit-holder",
            Ctx::Duplicate => "d-cm-duplicate",
            Ctx::Delete => "d-cm-delete",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Ctx::Text | Ctx::Edit => "d-text.svg",
            Ctx::All => "d-select.svg",
            Ctx::Fit | Ctx::FitHolder => "d-fit.svg",
            Ctx::Reverse => "d-reverse.svg",
            Ctx::Elbow => "d-route.svg",
            Ctx::Straight => "d-line.svg",
            Ctx::Curve => "d-route-curve.svg",
            Ctx::Loop => "d-st-loop.svg",
            Ctx::Duplicate => "copy.svg",
            Ctx::Delete => "trash.svg",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Ctx::Text => tr("Add a text here", "Ajouter un texte ici"),
            Ctx::All => tr("Select all", "Tout sélectionner"),
            Ctx::Fit => tr("Fit the diagram", "Ajuster au diagramme"),
            Ctx::Edit => tr("Write in it", "Écrire dedans"),
            Ctx::Reverse => tr("Turn the arrow around", "Retourner la flèche"),
            Ctx::Elbow => tr("Route: elbowed", "Tracé : coudé"),
            Ctx::Straight => tr("Route: straight", "Tracé : droit"),
            Ctx::Curve => tr("Route: curved", "Tracé : courbe"),
            Ctx::Loop => tr("Transition to itself", "Transition vers elle-même"),
            Ctx::FitHolder => tr("Fit to what it holds", "Ajuster à son contenu"),
            Ctx::Duplicate => tr("Duplicate", "Dupliquer"),
            Ctx::Delete => tr("Delete", "Supprimer"),
        }
    }
}

/// What a row of the library is: a starting point (a new diagram) or a shape.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Lib {
    Start(usize),
    Preset(usize),
}

impl Canvas {
    /// Closes the menus and the library, the one that is asked for excepted.
    fn only(&mut self, shapes: bool, relations: bool, library: bool) {
        self.context = None;
        self.cursor = 0;
        self.shapes = shapes;
        self.relations = relations;
        if !library {
            self.library = None;
        } else if self.library.is_none() {
            self.library = Some(String::new());
        }
        self.reset_cursor();
    }

    /// Puts the cursor on the first row: in the library, the first shape (Enter draws a shape, not a new diagram).
    pub(super) fn reset_cursor(&mut self) {
        self.cursor = self.library_entries().iter().position(|e| matches!(e, Lib::Preset(_))).unwrap_or(0);
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

    /// The shapes of the menu of shapes, in order.
    fn shape_entries() -> Vec<Tool> {
        TOOLS.iter().filter(|(tool, ..)| matches!(tool, Tool::Shape(form) if *form != Form::Text)).map(|(tool, ..)| *tool).collect()
    }

    /// The rows of the menu of relationships, in the order they are listed: each family, the n-ary association after the associations.
    fn relation_entries() -> Vec<Tool> {
        let mut out = Vec::new();
        for family in [Relation::Association, Relation::Generalization, Relation::Dependency, Relation::Sync].map(Relation::group) {
            out.extend(Relation::ALL.into_iter().filter(|r| r.group() == family).map(Tool::Link));
            if family == Relation::Association.group() {
                out.push(Tool::Shape(Form::Branch));
            }
        }
        out
    }

    /// The rows of the menu of a right click, for what is selected.
    pub(super) fn context_entries(&self) -> Vec<Ctx> {
        let links = self.selected.iter().any(|id| self.diagram.link(*id).is_some());
        let single = self.selected.len() == 1;
        let alone = matches!(self.selected[..], [id] if self.diagram.shape(id).is_some_and(|s| matches!(s.form, Form::Round | Form::Composite)));
        let holder = matches!(self.selected[..], [id] if self.diagram.shape(id).is_some_and(|s| s.form.holds()) && !self.diagram.children(id).is_empty());
        if self.selected.is_empty() {
            return vec![Ctx::Text, Ctx::All, Ctx::Fit];
        }
        let mut rows = Vec::new();
        if single {
            rows.push(Ctx::Edit);
        }
        if links {
            rows.extend([Ctx::Reverse, Ctx::Elbow, Ctx::Straight, Ctx::Curve]);
        }
        if alone {
            rows.push(Ctx::Loop);
        }
        if holder {
            rows.push(Ctx::FitHolder);
        }
        rows.extend([Ctx::Duplicate, Ctx::Delete]);
        rows
    }

    /// The rows of the library for what is typed: the starting points that answer, then the shapes.
    pub(super) fn library_entries(&self) -> Vec<Lib> {
        let Some(query) = &self.library else { return Vec::new() };
        let starts = library::templates().into_iter().enumerate().filter(|(_, t)| t.matches(query)).map(|(i, _)| Lib::Start(i));
        let shapes = library::presets().into_iter().enumerate().filter(|(_, p)| p.matches(query)).map(|(i, _)| Lib::Preset(i));
        starts.chain(shapes).collect()
    }

    /// Whether a menu, the library or the menu of a right click is open: the keys are theirs.
    pub(super) fn menu_open(&self) -> bool {
        self.shapes || self.relations || self.library.is_some() || self.context.is_some()
    }

    /// How many rows the open menu has.
    fn menu_len(&self) -> usize {
        if self.context.is_some() {
            self.context_entries().len()
        } else if self.library.is_some() {
            self.library_entries().len()
        } else if self.shapes {
            Self::shape_entries().len()
        } else if self.relations {
            Self::relation_entries().len()
        } else {
            0
        }
    }

    /// Up and down in the open menu: the row the keys are on goes round.
    pub(super) fn walk(&mut self, by: isize, cx: &mut Context<Self>) {
        let n = self.menu_len() as isize;
        if n > 0 {
            self.cursor = (self.cursor as isize + by).rem_euclid(n) as usize;
            cx.notify();
        }
    }

    /// Enter in the open menu: the row the keys are on.
    pub(super) fn pick(&mut self, cx: &mut Context<Self>) {
        let at = self.cursor;
        if let Some(at_pointer) = self.context {
            if let Some(entry) = self.context_entries().get(at).copied() {
                self.context = None;
                self.run(entry, at_pointer, cx);
            }
        } else if self.library.is_some() {
            match self.library_entries().get(at).copied() {
                Some(Lib::Start(i)) => {
                    self.library = None;
                    cx.emit(CanvasEvent::Template(i));
                    cx.notify();
                }
                Some(Lib::Preset(i)) => {
                    self.library = None;
                    self.choose_preset(i, cx);
                }
                None => {}
            }
        } else if self.shapes {
            if let Some(tool) = Self::shape_entries().get(at).copied() {
                self.set_tool(tool, cx);
            }
        } else if self.relations {
            match Self::relation_entries().get(at).copied() {
                Some(Tool::Link(relation)) => self.choose_relation(relation, cx),
                Some(tool) => self.set_tool(tool, cx),
                None => {}
            }
        }
    }

    /// What a row of the menu of a right click does, for a click at `at`.
    pub(super) fn run(&mut self, entry: Ctx, at: Point<Pixels>, cx: &mut Context<Self>) {
        match entry {
            Ctx::Text => self.text_at(at, cx),
            Ctx::All => self.select_all(cx),
            Ctx::Fit => self.zoom_fit(cx),
            Ctx::Edit => {
                if let [id] = self.selected[..] {
                    self.start_editing(id, cx);
                }
            }
            Ctx::Reverse => self.reverse(cx),
            Ctx::Elbow => self.set_route(Route::Elbow, cx),
            Ctx::Straight => self.set_route(Route::Straight, cx),
            Ctx::Curve => self.set_route(Route::Curve, cx),
            Ctx::Loop => self.self_link(cx),
            Ctx::FitHolder => self.fit_container(cx),
            Ctx::Duplicate => self.duplicate(cx),
            Ctx::Delete => self.erase(false, cx),
        }
        cx.notify();
    }

    /// The menu of shapes: the ones the bar used to show side by side, with their names.
    fn shapes_menu(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let rows = Self::shape_entries().into_iter().enumerate().map(|(i, tool)| {
            let (icon, key) = TOOLS.iter().find(|(o, ..)| *o == tool).map_or(("", ""), |(_, icon, key)| (*icon, *key));
            row(("d-shape", i), icon, tool_name(tool), key.to_uppercase(), self.tool == tool, self.cursor == i, t)
                .debug_selector(move || format!("d-tool-{key}"))
                .tooltip(tip(tool_label(tool), key.to_uppercase(), t))
                .on_click(cx.listener(move |this, _, _, cx| this.set_tool(tool, cx)))
                .on_drag(Placing(tool, icon, t, Point::default()), |placing, grab, _, cx| cx.new(|_| placing.grabbed(grab)))
        });
        floating(t).w(px(236.)).flex().flex_col().children(rows)
    }

    /// The menu of relationships, by family: a name and a letter for each; the one the selected arrow looks like is lit.
    pub(super) fn relations_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let current = self.selected.iter().find_map(|id| self.diagram.link(*id)).and_then(Relation::of);
        let entries = Self::relation_entries();
        let at = |tool: Tool| entries.iter().position(|e| *e == tool);
        let item = |relation: Relation, cx: &mut Context<Self>| {
            let lit = match self.tool {
                Tool::Link(tool) => tool == relation,
                _ => current == Some(relation),
            };
            let icon = relation.icon().unwrap_or("d-rel-dependency.svg");
            row(("d-relation", relation as usize), icon, relation.label(), relation.key().to_uppercase().to_string(), lit, at(Tool::Link(relation)) == Some(self.cursor), t)
                .debug_selector(move || format!("d-relation-{}", relation as usize))
                .tooltip(tip(relation.name(), String::new(), t))
                .on_click(cx.listener(move |this, _, _, cx| this.choose_relation(relation, cx)))
        };
        let group = |name: &'static str, cx: &mut Context<Self>| {
            let rows: Vec<_> = Relation::ALL.into_iter().filter(|r| r.group() == name).map(|r| item(r, cx)).collect();
            div().flex().flex_col().child(title(name, t)).children(rows)
        };
        let nary_tool = Tool::Shape(Form::Branch);
        let nary = row("d-nary", "d-rel-nary.svg", tool_name(nary_tool), Self::NARY.to_uppercase(), self.tool == nary_tool, at(nary_tool) == Some(self.cursor), t)
            .tooltip(tip(tool_label(nary_tool), String::new(), t))
            .on_click(cx.listener(|this, _, _, cx| this.set_tool(Tool::Shape(Form::Branch), cx)));
        let column = |groups: &[&'static str], cx: &mut Context<Self>| {
            let groups: Vec<_> = groups.iter().map(|g| group(g, cx)).collect();
            div().w(px(228.)).flex().flex_col().children(groups)
        };
        let (a, b) = (Relation::Association.group(), Relation::Generalization.group());
        let (c, d) = (Relation::Dependency.group(), Relation::Sync.group());
        // As high as the canvas leaves room for above the dock: it scrolls beyond that.
        let room = f32::from(self.bounds.size.height);
        let max_h = if room < 1. { 560. } else { (room - DOCK - 16.).max(120.) };
        floating(t).id("d-relations-menu").max_h(px(max_h)).overflow_y_scroll().flex().flex_wrap().max_w(px(480.)).child(column(&[a], cx).child(nary)).child(column(&[b], cx)).child(column(&[c], cx)).child(column(&[d], cx))
    }

    /// The menu of a right click: what can be done with what is under it, or with the canvas when nothing is.
    pub(super) fn context_menu(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let at = self.context?;
        let t = self.theme;
        let entries = self.context_entries();
        let rows: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let (entry, id) = (*entry, entry.id());
                let key = match entry {
                    Ctx::Edit => crate::ui::keys::of(cx, &Confirm),
                    Ctx::All => crate::ui::keys::of(cx, &SelectAll),
                    Ctx::Fit => "0".into(),
                    Ctx::Duplicate => crate::ui::keys::of(cx, &Duplicate),
                    Ctx::Delete => crate::ui::keys::of(cx, &Erase),
                    _ => String::new(),
                };
                row(id, entry.icon(), entry.label(), key, false, self.cursor == i, t).debug_selector(move || id.to_string()).on_click(cx.listener(move |this, _, _, cx| {
                    this.context = None;
                    this.run(entry, at, cx);
                }))
            })
            .collect();
        // Where it opens: at the pointer, pushed back inside the canvas.
        let (width, height) = (216., 28. * rows.len() as f32 + 8.);
        let known = |side: Pixels, otherwise: f32| if f32::from(side) < 1. { otherwise } else { f32::from(side) };
        let (canvas_w, canvas_h) = (known(self.bounds.size.width, 800.), known(self.bounds.size.height, 600.));
        let (x, y) = (f32::from(at.x - self.bounds.origin.x), f32::from(at.y - self.bounds.origin.y));
        let (left, top) = (x.min(canvas_w - width - 8.).max(8.), y.min(canvas_h - height - 8.).max(8.));
        Some(floating(t).id("d-context-menu").debug_selector(|| "d-context-menu".into()).absolute().left(px(left)).top(px(top)).w(px(width)).flex().flex_col().children(rows))
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
            Some(self.shapes_menu(cx).into_any_element())
        } else if self.relations {
            Some(self.relations_menu(cx).into_any_element())
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
            .child(button("d-undo", "c-undo.svg", false, t).when(self.undo.is_empty(), |b| b.opacity(0.4)).tooltip(tip(tr("Undo", "Annuler"), undo, t)).on_click(cx.listener(|this, _, _, cx| this.restore(false, cx))))
            .child(button("d-redo", "c-redo.svg", false, t).when(self.redo.is_empty(), |b| b.opacity(0.4)).tooltip(tip(tr("Redo", "Rétablir"), redo, t)).on_click(cx.listener(|this, _, _, cx| this.restore(true, cx))))
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
        // In a narrow canvas the zoom does not fit beside the dock: it goes above it.
        let narrow = f32::from(self.bounds.size.width) > 1. && f32::from(self.bounds.size.width) < NARROW;
        div()
            .absolute()
            .bottom(px(if narrow { 64. } else { 16. }))
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
            .child(button("d-zoom-out", "minimize.svg", false, t).debug_selector(|| "d-zoom-out".into()).tooltip(tip(tr("Zoom out", "Dézoomer"), "-".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(0.8, cx))))
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
            .child(button("d-zoom-in", "plus.svg", false, t).debug_selector(|| "d-zoom-in".into()).tooltip(tip(tr("Zoom in", "Zoomer"), "+".into(), t)).on_click(cx.listener(|this, _, _, cx| this.zoom_by(1.25, cx))))
    }

    /// What the library lists, top to bottom: the search, then titles and rows. The arrow keys find the row
    /// they are on in it, to scroll it into view.
    fn library_layout(&self) -> Vec<LibChild> {
        let mut out = vec![LibChild::Search];
        let entries = self.library_entries();
        let presets = library::presets();
        if entries.iter().any(|e| matches!(e, Lib::Start(_))) {
            out.push(LibChild::Title(tr("Starting points", "Points de départ")));
        }
        let mut family = "";
        for entry in entries {
            if let Lib::Preset(i) = entry
                && presets[i].family != family
            {
                family = presets[i].family;
                out.push(LibChild::Title(family));
            }
            out.push(LibChild::Row(entry));
        }
        if out.iter().all(|c| !matches!(c, LibChild::Row(_))) {
            out.push(LibChild::Empty);
        }
        out
    }

    /// Brings the row the keys are on into view in the library.
    pub(super) fn show_cursor(&self) {
        let at = self.library_entries().get(self.cursor).copied();
        if let Some(at) = at
            && let Some(ix) = self.library_layout().iter().position(|c| *c == LibChild::Row(at))
        {
            self.library_scroll.scroll_to_item(ix);
        }
    }

    /// The library: a drawer on the left, shapes by family and starting points, searched by typing.
    pub(super) fn drawer(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let t = self.theme;
        let query = self.library.as_ref()?;
        let presets = library::presets();
        let templates = library::templates();
        let focus = self.library_entries().get(self.cursor).copied();
        let children: Vec<gpui::AnyElement> = self
            .library_layout()
            .into_iter()
            .map(|child| match child {
                LibChild::Search => div()
                    .h(px(30.))
                    .px_2()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .rounded(px(6.))
                    .bg(t.bg)
                    .border_1()
                    .border_color(t.border)
                    .text_size(px(12.5))
                    .child(svg().path("search.svg").size(px(14.)).flex_none().text_color(t.soft))
                    .child(if query.is_empty() { div().text_color(t.soft).child(tr("Search shapes…", "Chercher une forme…")) } else { div().child(format!("{query}▏")) })
                    .into_any_element(),
                LibChild::Title(name) => title(name, t).into_any_element(),
                LibChild::Empty => div().px_2().py_1().text_size(px(12.)).text_color(t.soft).child(tr("No shape answers.", "Aucune forme ne répond.")).into_any_element(),
                LibChild::Row(Lib::Start(i)) => row(("d-start", i), "file-plus.svg", templates[i].name, String::new(), false, focus == Some(Lib::Start(i)), t)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.library = None;
                        cx.emit(CanvasEvent::Template(i));
                        cx.notify();
                    }))
                    .into_any_element(),
                LibChild::Row(Lib::Preset(i)) => row(("d-preset", i), presets[i].icon, presets[i].name, String::new(), self.tool == Tool::Preset(i), focus == Some(Lib::Preset(i)), t)
                    .debug_selector(move || format!("d-preset-{i}"))
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_preset(i, cx)))
                    .on_drag(Placing(Tool::Preset(i), presets[i].icon, t, Point::default()), |placing, grab, _, cx| cx.new(|_| placing.grabbed(grab)))
                    .into_any_element(),
            })
            .collect();
        Some(floating(t).id("d-library-panel").absolute().top(px(60.)).left_3().bottom(px(76.)).w(px(244.)).overflow_y_scroll().track_scroll(&self.library_scroll).p_2().flex().flex_col().gap_px().children(children))
    }
}

/// A child of the scrolling list of the library.
#[derive(Clone, Copy, PartialEq, Debug)]
enum LibChild {
    Search,
    Title(&'static str),
    Row(Lib),
    Empty,
}
