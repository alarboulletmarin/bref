//! The drawing of a diagram as a list of primitives, in painting order: the one place that decides
//! which strokes, fills, dashes and texts make a diagram. The SVG file and the canvas both draw it.

use super::*;

/// How a path is filled: not at all, in its ink at 15% (the body of a filled shape, a bar), or in full (a closed head).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fill {
    None,
    Tint,
    Solid,
}

/// A path, stroked in an ink (a color index, 0 for the neutral), dashed or not, and filled.
#[derive(Clone, PartialEq, Debug)]
pub struct Stroke {
    pub path: Vec<Cmd>,
    pub ink: u8,
    pub dashed: bool,
    pub fill: Fill,
}

/// Where a line of the text being written is in that text: the canvas puts the cursor and the selection on it.
#[derive(Clone, PartialEq, Debug)]
pub struct Written {
    /// Byte of the field's text where the line begins.
    pub start: usize,
    pub text: String,
    pub bold: bool,
}

/// A line of text, written at `(x, y)` from its `anchor`.
#[derive(Clone, PartialEq, Debug)]
pub struct Label {
    pub x: f32,
    pub y: f32,
    pub spans: Vec<Span>,
    pub anchor: Anchor,
    pub ink: u8,
    /// Set for the lines of the text being written only.
    pub written: Option<Written>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Item {
    Path(Stroke),
    Text(Label),
}

/// What a diagram draws, in order, and the traces of its arrows (`None`: an arrow that is not drawn).
pub struct Scene {
    pub items: Vec<Item>,
    pub traces: Vec<Option<Trace>>,
}

impl Diagram {
    /// The primitives of the drawing. `writing` is the text being written, as `(element, slot)`: it is
    /// shown as typed, and its lines say where they are in it.
    pub fn scene(&self, writing: Option<(u32, Slot)>) -> Scene {
        let (traces, flow) = self.traces();
        let mut items = Vec::new();
        let path = |items: &mut Vec<Item>, path: Vec<Cmd>, ink: u8, dashed: bool, fill: Fill| items.push(Item::Path(Stroke { path, ink, dashed, fill }));
        for shape in &self.shapes {
            for (i, outline) in shape.outline().into_iter().enumerate() {
                let fill = if shape.solid().contains(&i) {
                    Fill::Solid
                } else if i == 0 && shape.fill {
                    Fill::Tint
                } else {
                    Fill::None
                };
                path(&mut items, outline, shape.color, shape.dashed || shape.dashed_paths().contains(&i), fill);
            }
            let (lines, rules) = self.lines(shape);
            for y in rules {
                let (x0, x1) = shape.rule_span(y);
                // Between the operands of a combined fragment: dashed.
                path(&mut items, vec![Cmd::Move(x0, y), Cmd::Line(x1, y)], shape.color, shape.form == Form::Fragment, Fill::None);
            }
            let plain = writing.is_some_and(|(id, _)| id == shape.id);
            for (line, spans) in lines.iter().zip(self.runs(shape, plain)) {
                // A line is found in the text by its place in it (separator strokes are not text lines).
                let start = (line.text.as_ptr() as usize).checked_sub(shape.text.as_ptr() as usize).filter(|at| at + line.text.len() <= shape.text.len());
                let written = start.filter(|_| plain).map(|start| Written { start, text: line.text.to_string(), bold: line.bold });
                items.push(Item::Text(Label { x: line.x, y: line.y, spans, anchor: line.centered.into(), ink: shape.color, written }));
            }
        }
        // What the messages add: the bars of the lifelines that are busy, the cross of one that is destroyed.
        for bar in &flow.marks.bars {
            let (x, y, w, h) = bar.rect();
            let ink = self.shape(bar.life).map_or(0, |s| s.color);
            path(&mut items, vec![Cmd::Move(x, y), Cmd::Line(x + w, y), Cmd::Line(x + w, y + h), Cmd::Line(x, y + h), Cmd::Close], ink, false, Fill::Tint);
        }
        for &(x, y) in &flow.marks.crosses {
            let cross = vec![Cmd::Move(x - 10., y - 10.), Cmd::Line(x + 10., y + 10.), Cmd::Move(x + 10., y - 10.), Cmd::Line(x - 10., y + 10.)];
            path(&mut items, cross, 0, false, Fill::None);
        }
        for (link, drawn) in self.links.iter().zip(&traces) {
            let Some(drawn) = drawn else { continue };
            path(&mut items, drawn.line.clone(), link.color, link.dashed, Fill::None);
            for (head, full) in &drawn.heads {
                path(&mut items, head.clone(), link.color, false, if *full { Fill::Solid } else { Fill::None });
            }
            // Written, the text is as typed; otherwise a message of a numbered diagram has its number.
            let written = |slot: Slot| writing == Some((link.id, slot));
            let shown = if written(Slot::Middle) { link.text.clone() } else { self.shown_text(link) };
            let lines: Vec<&str> = shown.split('\n').collect();
            let mut start = 0;
            for (i, text) in lines.iter().enumerate() {
                let (x, y, centered) = Diagram::label_at(drawn, i, lines.len());
                let spans = if written(Slot::Middle) { richtext::plain(text, false) } else { self.label_runs(text) };
                let written = written(Slot::Middle).then(|| Written { start, text: text.to_string(), bold: false });
                items.push(Item::Text(Label { x, y, spans, anchor: centered.into(), ink: link.color, written }));
                start += text.len() + 1;
            }
            // The texts at the ends: a role, a multiplicity.
            for (tip, slot, along) in [(drawn.tips[0], Slot::Start, link.start_along), (drawn.tips[1], Slot::End, link.end_along)] {
                let lines: Vec<&str> = link.text_of(slot).split('\n').collect();
                let mut start = 0;
                for (i, text) in lines.iter().enumerate() {
                    let (x, y, anchor) = Diagram::tip_label_at(&tip, along, i, lines.len());
                    let spans = if written(slot) { richtext::plain(text, false) } else { self.styled(text, false) };
                    let written = written(slot).then(|| Written { start, text: text.to_string(), bold: false });
                    items.push(Item::Text(Label { x, y, spans, anchor, ink: link.color, written }));
                    start += text.len() + 1;
                }
            }
        }
        Scene { items, traces }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(scene: &Scene) -> Vec<&Label> {
        scene.items.iter().filter_map(|i| if let Item::Text(l) = i { Some(l) } else { None }).collect()
    }

    #[test]
    fn only_the_text_being_written_says_where_its_lines_are() {
        let mut d = Diagram::blank();
        let a = d.add_shape_mut(Form::Rect, 0., 0., 100., 60.).id;
        d.shape_mut(a).unwrap().text = "*one*\ntwo".into();
        let b = d.add_shape(Form::Rect, 300., 0., 100., 60.);
        let l = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
        d.link_mut(l).unwrap().text = "*m*".into();
        // Nothing written: styled, no line says where it is.
        let quiet = d.scene(None);
        assert!(labels(&quiet).iter().all(|l| l.written.is_none()) && labels(&quiet)[0].spans[0].style.italic);
        // The shape written: as typed (marks kept), its lines at their byte in the text.
        let scene = d.scene(Some((a, Slot::Middle)));
        let lines: Vec<_> = labels(&scene).iter().filter_map(|l| l.written.as_ref().map(|w| (w.start, w.text.clone()))).collect();
        assert_eq!(lines, [(0, "*one*".to_string()), (6, "two".to_string())]);
        assert!(!labels(&scene)[0].spans[0].style.italic);
        // The start end of an arrow, empty and written: one empty line to put the cursor on.
        let scene = d.scene(Some((l, Slot::Start)));
        let written: Vec<_> = labels(&scene).into_iter().filter(|l| l.written.is_some()).collect();
        assert!(written.len() == 1 && written[0].written.as_ref().is_some_and(|w| w.start == 0 && w.text.is_empty()));
        // Items run in painting order: shapes, then the arrows.
        assert!(matches!(quiet.items.last(), Some(Item::Text(_))) && quiet.traces.len() == 1);
    }
}
