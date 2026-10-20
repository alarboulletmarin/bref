//! The bar that floats over the selection: what can be changed in it, where it is, instead of in a
//! bar at the top that grows and shrinks.

use super::*;
use gpui::Div;

/// Side of a button, the gap between two items, and what a separator takes (a hairline and its margins).
const BUTTON: f32 = 28.;
const GAP: f32 = 4.;
const DIVIDER: f32 = 9.;
/// Width of the chip that names the relationship of an arrow.
const CHIP: f32 = 168.;
/// Height of a row of the bar, and the room it keeps from the selection.
const ROW: f32 = 36.;
const AWAY: f32 = 10.;
/// How far right the cluster of buttons at the top left reaches (it grows with the buttons of a sequence diagram).
const CLUSTER: f32 = 200.;

/// What an item of the bar is, with the width it takes, so that the bar can be put where it fits.
type Item = (gpui::AnyElement, f32);

impl Canvas {
    /// Where the selection is, in diagram units: left, top, right, bottom.
    fn selection_box(&self) -> Option<(f32, f32, f32, f32)> {
        let mut boxes = Vec::new();
        for id in &self.selected {
            if let Some(s) = self.diagram.shape(*id) {
                boxes.push((s.x, s.y, s.x + s.w, s.y + s.h));
            } else if let Some((link, trace)) = self.diagram.link(*id).and_then(|l| Some((l, self.diagram.trace(l)?))) {
                boxes.extend(trace.points.iter().map(|p| (p.0, p.1, p.0, p.1)));
                // What is written along it is part of it: the bar must not cover the texts it can change.
                boxes.extend([Slot::Start, Slot::End].into_iter().filter_map(|slot| self.diagram.tip_label_box(link, slot, &trace)));
                let shown = self.diagram.shown_text(link);
                let lines: Vec<&str> = shown.split('\n').filter(|l| !l.is_empty()).collect();
                for (i, text) in lines.iter().enumerate() {
                    let (x, y, centered) = Diagram::label_at(&trace, i, lines.len());
                    let w = Diagram::guess_width(text);
                    let left = if centered { x - w / 2. } else { x };
                    boxes.push((left, y, left + w, y + diagram::LINE));
                }
            }
        }
        boxes.into_iter().reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    }

    /// Duplicates the selection, a little lower and to the right.
    pub(super) fn duplicate(&mut self, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            self.remember();
            self.selected = self.diagram.duplicate(&self.selected, 2. * diagram::GRID);
            self.changed(cx);
        }
    }

    /// The six colours of the palette, side by side.
    fn swatches(&self, cx: &mut Context<Self>) -> Item {
        let row = div().flex().items_center().gap(px(GAP)).children((0..diagram::COLORS.len() as u8).map(|i| {
            div()
                .id(("d-color", i as usize))
                .size(px(16.))
                .flex_none()
                .rounded_full()
                .cursor_pointer()
                .bg(self.ink(i))
                .border_1()
                .border_color(self.theme.border)
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
        }));
        let n = diagram::COLORS.len() as f32;
        (row.into_any_element(), n * 16. + (n - 1.) * GAP)
    }

    /// Whether the bar is over the selection: there is one, nothing is being written, dragged or drawn.
    pub(super) fn bar_wanted(&self) -> bool {
        let busy = !matches!(self.drag, Drag::None | Drag::Move { begun: false, .. } | Drag::Reorder { begun: false, .. } | Drag::Along { begun: false, .. });
        !self.selected.is_empty() && self.editing.is_none() && !busy && self.tool == Tool::Select
    }

    /// The bar over the selection, if there is something to change in it.
    pub(super) fn selection_bar(&self, cx: &mut Context<Self>) -> Option<gpui::Stateful<Div>> {
        if !self.bar_wanted() {
            return None;
        }
        let (x0, y0, x1, y1) = self.selection_box()?;
        let t = self.theme;
        let links = self.selected.iter().any(|id| self.diagram.link(*id).is_some());
        let shapes = self.selected.iter().any(|id| self.diagram.shape(*id).is_some());
        let alone = matches!(self.selected[..], [id] if self.diagram.shape(id).is_some_and(|s| !s.form.holds() || s.form == diagram::Form::Composite));
        let holder = matches!(self.selected[..], [id] if self.diagram.shape(id).is_some_and(|s| s.form.holds()) && !self.diagram.children(id).is_empty());
        let filled = self.selected.iter().filter_map(|id| self.diagram.shape(*id)).any(|s| s.fill);
        let dashed = self.selected.iter().any(|id| self.diagram.shape(*id).map(|s| s.dashed).or_else(|| self.diagram.link(*id).map(|l| l.dashed)) == Some(true));
        let mut items: Vec<Item> = Vec::new();
        let divider = |items: &mut Vec<Item>| {
            if !items.is_empty() {
                items.push((div().w(px(1.)).h(px(16.)).mx_1().bg(t.border).into_any_element(), DIVIDER));
            }
        };
        // What an arrow is, named, and the menu to change it.
        if links {
            let current = self.selected.iter().find_map(|id| self.diagram.link(*id)).and_then(Relation::of);
            let name = current.map_or(tr("Arrow", "Flèche"), Relation::label);
            let icon = current.and_then(Relation::icon).unwrap_or("d-rel-dependency.svg");
            let chip = div()
                .id("d-relation-chip")
                .debug_selector(|| "d-relation-chip".into())
                .w(px(CHIP))
                .h(px(BUTTON))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(12.5))
                .text_color(t.text)
                .bg(if self.relations { t.selection } else { t.bg })
                .border_1()
                .border_color(t.border)
                .hover(|s| s.bg(t.border))
                .tooltip(tip(tr("Relationship of this arrow", "Relation de cette flèche"), Self::RELATIONS.to_uppercase(), t))
                .child(svg().path(icon).size(px(16.)).flex_none().text_color(t.soft))
                .child(div().flex_1().truncate().child(name))
                .child(svg().path("chevron-down.svg").size(px(12.)).flex_none().text_color(t.soft))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_relations(cx)));
            items.push((chip.into_any_element(), CHIP));
            divider(&mut items);
            let toggle = |id: &'static str, icon: &'static str, tooltip: &'static str, cx: &mut Context<Self>, set: fn(Option<&mut diagram::Shape>, Option<&mut diagram::Link>)| -> Item {
                (button(id, icon, false, t).tooltip(tip(tooltip, String::new(), t)).on_click(cx.listener(move |this, _, _, cx| this.style(cx, set))).into_any_element(), BUTTON)
            };
            items.push(toggle("d-head-start", "d-head-start.svg", tr("Head at the start", "Extrémité du début"), cx, |_, link| {
                if let Some(link) = link {
                    link.start = link.start.next();
                }
            }));
            items.push(toggle("d-head-end", "d-head-end.svg", tr("Head at the end", "Extrémité de la fin"), cx, |_, link| {
                if let Some(link) = link {
                    link.end = link.end.next();
                }
            }));
            let route = button("d-route", "d-route.svg", false, t).tooltip(tip(tr("Route: elbowed, straight or curved", "Tracé : coudé, droit ou courbe"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| {
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
            }));
            items.push((route.into_any_element(), BUTTON));
            divider(&mut items);
            let text = |id: &'static str, icon: &'static str, slot: Slot, tooltip: &'static str, cx: &mut Context<Self>| -> Item {
                let button = button(id, icon, false, t).tooltip(tip(tooltip, String::new(), t)).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(link) = this.selected.iter().copied().find(|id| this.diagram.link(*id).is_some()) {
                        this.stop_editing(cx);
                        this.start_editing_slot(link, slot, cx);
                    }
                }));
                (button.into_any_element(), BUTTON)
            };
            items.push(text("d-text-start", "d-label-start.svg", Slot::Start, tr("Text at the start: a role, a multiplicity", "Texte au début : un rôle, une multiplicité"), cx));
            items.push(text("d-text-end", "d-label-end.svg", Slot::End, tr("Text at the end: a role, a multiplicity", "Texte à la fin : un rôle, une multiplicité"), cx));
            divider(&mut items);
        }
        items.push(self.swatches(cx));
        if shapes {
            items.push((
                button("d-fill", "d-fill.svg", filled, t)
                    .tooltip(tip(tr("Fill", "Remplir"), String::new(), t))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.style(cx, |shape, _| {
                            if let Some(shape) = shape {
                                shape.fill = !shape.fill;
                            }
                        })
                    }))
                    .into_any_element(),
                BUTTON,
            ));
        }
        items.push((
            button("d-dash", "d-dash.svg", dashed, t)
                .tooltip(tip(tr("Dashed stroke", "Trait en pointillés"), String::new(), t))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.style(cx, |shape, link| {
                        if let Some(shape) = shape {
                            shape.dashed = !shape.dashed;
                        }
                        if let Some(link) = link {
                            link.dashed = !link.dashed;
                        }
                    })
                }))
                .into_any_element(),
            BUTTON,
        ));
        if alone || holder {
            divider(&mut items);
        }
        if alone {
            items.push((button("d-self", "d-st-loop.svg", false, t).tooltip(tip(tr("Transition to itself: a loop", "Transition vers elle-même : une boucle"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| this.self_link(cx))).into_any_element(), BUTTON));
        }
        if holder {
            items.push((button("d-fit", "d-fit.svg", false, t).tooltip(tip(tr("Fit to what it holds", "Ajuster à son contenu"), String::new(), t)).on_click(cx.listener(|this, _, _, cx| this.fit_container(cx))).into_any_element(), BUTTON));
        }
        divider(&mut items);
        let (copy, erase) = (crate::ui::keys::of(cx, &Duplicate), crate::ui::keys::of(cx, &Erase));
        items.push((button("d-duplicate", "copy.svg", false, t).tooltip(tip(tr("Duplicate", "Dupliquer"), copy, t)).on_click(cx.listener(|this, _, _, cx| this.duplicate(cx))).into_any_element(), BUTTON));
        items.push((button("d-delete", "trash.svg", false, t).debug_selector(|| "d-delete".into()).tooltip(tip(tr("Delete", "Supprimer"), erase, t)).on_click(cx.listener(|this, _, _, cx| this.erase(false, cx))).into_any_element(), BUTTON));

        // Where it goes: centred over the selection, above it (below if there is no room), inside the canvas.
        // Before the first frame the canvas has no size yet.
        let known = |side: Pixels, otherwise: f32| if f32::from(side) < 1. { otherwise } else { f32::from(side) };
        let (canvas_w, canvas_h) = (known(self.bounds.size.width, 800.), known(self.bounds.size.height, 600.));
        let natural = items.iter().map(|(_, w)| *w).sum::<f32>() + GAP * (items.len() as f32 - 1.) + 12.;
        let width = natural.min(canvas_w - 16.);
        let rows = (natural / width).ceil().max(1.);
        let height = ROW * rows - 4.;
        let middle = self.pan.0 + (x0 + x1) / 2. * self.zoom;
        let left = (middle - width / 2.).clamp(8., (canvas_w - width - 8.).max(8.));
        // Above the selection or below it, whichever covers fewer shapes; at the top if neither fits.
        let (above, below) = (self.pan.1 + y0 * self.zoom - height - AWAY, self.pan.1 + y1 * self.zoom + AWAY);
        let room = canvas_h - DOCK - 8.;
        let covered = |top: f32| {
            let (l, t, r, b) = ((left - self.pan.0) / self.zoom, (top - self.pan.1) / self.zoom, (left + width - self.pan.0) / self.zoom, (top + height - self.pan.1) / self.zoom);
            self.diagram.shapes.iter().filter(|s| !self.selected.contains(&s.id) && !s.form.holds()).filter(|s| s.x < r && s.x + s.w > l && s.y < b && s.y + s.h > t).count()
        };
        // The cluster of buttons at the top left is not covered.
        let least = if left < CLUSTER { 56. } else { 8. };
        let top = match (above >= least, below + height <= room) {
            (true, true) if covered(below) < covered(above) => below,
            (true, _) => above,
            (false, true) => below,
            (false, false) => least,
        };
        Some(
            div()
                .id("d-selection-bar")
                .debug_selector(|| "d-selection-bar".into())
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(width))
                .p(px(4.))
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(GAP))
                .rounded(px(10.))
                .bg(t.panel)
                .border_1()
                .border_color(t.border)
                .shadow_md()
                .occlude()
                .children(items.into_iter().map(|(el, _)| el)),
        )
    }
}
