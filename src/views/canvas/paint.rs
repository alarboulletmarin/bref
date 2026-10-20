//! Drawing the diagram, its selection and its text.

use super::*;

impl Canvas {
    /// The shape being written in grows to hold its text.
    pub(super) fn fit_text(&mut self, window: &mut Window) {
        let Some(shape) = self.editing.and_then(|(id, _)| self.diagram.shape(id)) else { return };
        let (lines, _) = shape.text_lines();
        let widest = lines.iter().map(|l| self.measure(l.text, l.bold, 1., window)).fold(0f32, f32::max);
        let tall = lines.last().map_or(0., |l| l.y + diagram::LINE - shape.y);
        let (form, id) = (shape.form, shape.id);
        let Some(shape) = self.diagram.shape_mut(id) else { return };
        match form {
            Form::Text => (shape.w, shape.h) = (widest.max(20.), tall.max(diagram::LINE)),
            Form::Actor => {}
            _ => {
                let grow = |have: f32, need: f32| if need > have { (need / diagram::GRID).ceil() * diagram::GRID } else { have };
                (shape.w, shape.h) = (grow(shape.w, widest + 20.), grow(shape.h, tall + 6.));
            }
        }
    }

    pub(super) fn line(&self, text: &str, bold: bool, color: Hsla, scale: f32, window: &mut Window) -> gpui::ShapedLine {
        let weight = if bold { FontWeight::BOLD } else { FontWeight::NORMAL };
        let run = TextRun {
            len: text.len(),
            font: gpui::Font { weight, ..font(sans()) },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(SharedString::from(text.to_string()), px(diagram::FONT * scale), &[run], None)
    }

    pub(super) fn measure(&self, text: &str, bold: bool, scale: f32, window: &mut Window) -> f32 {
        f32::from(self.line(text, bold, self.theme.text, scale, window).width)
    }

    pub(super) fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        self.bounds = bounds;
        if std::mem::take(&mut self.fit) {
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            match self.diagram.bounds() {
                Some((x0, y0, x1, y1)) => {
                    self.zoom = (w / (x1 - x0 + 80.)).min(h / (y1 - y0 + 80.)).clamp(0.2, 1.);
                    self.pan = (w / 2. - (x0 + x1) / 2. * self.zoom, h / 2. - (y0 + y1) / 2. * self.zoom);
                }
                None => (self.pan, self.zoom) = ((w / 2., h / 2.), 1.),
            }
        }
        self.fit_text(window);
        let (t, zoom) = (self.theme, self.zoom);
        let at = |p: (f32, f32)| self.spot(p.0, p.1);
        let trace = |builder: &mut PathBuilder, path: &[Cmd]| {
            for cmd in path {
                match *cmd {
                    Cmd::Move(x, y) => builder.move_to(at((x, y))),
                    Cmd::Line(x, y) => builder.line_to(at((x, y))),
                    Cmd::Curve(a, b, c, d, x, y) => builder.cubic_bezier_to(at((x, y)), at((a, b)), at((c, d))),
                    Cmd::Close => builder.close(),
                }
            }
        };
        let pen = |dashed: bool| {
            let pen = PathBuilder::stroke(px(1.5 * zoom.max(0.6)));
            if dashed { pen.dash_array(&[px(6. * zoom), px(4. * zoom)]) } else { pen }
        };
        let stroke = |window: &mut Window, path: &[Cmd], dashed: bool, color: Hsla| {
            let mut builder = pen(dashed);
            trace(&mut builder, path);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        };
        let flood = |window: &mut Window, path: &[Cmd], color: Hsla| {
            let mut builder = PathBuilder::fill();
            trace(&mut builder, path);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        };
        let write = |this: &Self, window: &mut Window, cx: &mut App, x: f32, y: f32, text: &str, centered: bool, bold: bool, color: Hsla| {
            if text.is_empty() {
                return;
            }
            let line = this.line(text, bold, color, zoom, window);
            let origin = at((x, y)) - point(if centered { line.width / 2. } else { px(0.) }, px(0.));
            line.paint(origin, px(diagram::LINE * zoom), window, cx).ok();
        };

        // Cursor of the text being written: (line, byte in the line).
        let caret = self.editing.and_then(|(id, at)| {
            let text = self.diagram.shape(id).map(|s| &s.text).or_else(|| self.diagram.link(id).map(|l| &l.text))?;
            let before = &text[..at];
            Some((id, before.matches('\n').count(), before.len() - before.rfind('\n').map_or(0, |i| i + 1)))
        });
        let mut caret_at = None;

        for shape in &self.diagram.shapes {
            let ink = self.ink(shape.color);
            let outline = shape.outline();
            if shape.fill && let Some(body) = outline.first() {
                flood(window, body, ink.opacity(0.15));
            }
            for path in &outline {
                stroke(window, path, shape.dashed, ink);
            }
            let (lines, rules) = shape.text_lines();
            for y in rules {
                stroke(window, &[Cmd::Move(shape.x, y), Cmd::Line(shape.x + shape.w, y)], false, ink);
            }
            // Separator strokes are not text lines: the cursor's line
            // is found by its place in the text.
            let starts: Vec<usize> = shape.text.split('\n').scan(0, |at, l| Some(std::mem::replace(at, *at + l.len() + 1))).collect();
            for line in &lines {
                write(self, window, cx, line.x, line.y, line.text, line.centered, line.bold, ink);
                if let Some((id, row, col)) = caret
                    && id == shape.id
                    && starts.get(row) == Some(&(line.text.as_ptr() as usize - shape.text.as_ptr() as usize))
                {
                    let width = self.measure(line.text, line.bold, 1., window);
                    let left = if line.centered { line.x - width / 2. } else { line.x };
                    caret_at = Some((left + self.measure(&line.text[..col], line.bold, 1., window), line.y));
                }
            }
        }
        for link in &self.diagram.links {
            let Some(drawn) = self.diagram.trace(link) else { continue };
            let ink = self.ink(link.color);
            stroke(window, &drawn.line, link.dashed, ink);
            for (path, full) in &drawn.heads {
                if *full {
                    flood(window, path, ink);
                }
                stroke(window, path, false, ink);
            }
            let lines: Vec<&str> = link.text.split('\n').collect();
            for (i, text) in lines.iter().enumerate() {
                let (x, y, centered) = Diagram::label_at(&drawn, i, lines.len());
                write(self, window, cx, x, y, text, centered, false, ink);
                if let Some((id, row, col)) = caret
                    && id == link.id
                    && row == i
                {
                    let left = if centered { x - self.measure(text, false, 1., window) / 2. } else { x };
                    caret_at = Some((left + self.measure(&text[..col], false, 1., window), y));
                }
            }
        }

        // Selection: a frame, and handles at the corners of a single shape.
        let frame = |window: &mut Window, x0: f32, y0: f32, x1: f32, y1: f32, color: Hsla| {
            let path = [Cmd::Move(x0, y0), Cmd::Line(x1, y0), Cmd::Line(x1, y1), Cmd::Line(x0, y1), Cmd::Close];
            let mut builder = PathBuilder::stroke(px(1.)).dash_array(&[px(4.), px(3.)]);
            trace(&mut builder, &path);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        };
        let knob = |window: &mut Window, p: (f32, f32)| {
            let c = at(p);
            let bounds = Bounds::new(point(c.x - px(4.), c.y - px(4.)), size(px(8.), px(8.)));
            window.paint_quad(quad(bounds, px(2.), t.bg, px(1.5), t.accent, gpui::BorderStyle::default()));
        };
        let pad = 4. / zoom;
        for &id in &self.selected {
            if let Some(shape) = self.diagram.shape(id) {
                frame(window, shape.x - pad, shape.y - pad, shape.x + shape.w + pad, shape.y + shape.h + pad, t.accent);
                if self.selected.len() == 1 && self.editing.is_none() {
                    Self::corners(shape).into_iter().for_each(|c| knob(window, c));
                }
            } else if let Some((a, b)) = self.diagram.link(id).and_then(|l| self.diagram.ends(l)) {
                knob(window, a);
                knob(window, b);
            }
        }
        // While dragging an end over a shape: its attachment points.
        if let Drag::Tip { id, to } = self.drag
            && let Some(link) = self.diagram.link(id)
            && let Some(shape) = (if to { link.to } else { link.from }).shape().and_then(|id| self.diagram.shape(id))
        {
            frame(window, shape.x - pad, shape.y - pad, shape.x + shape.w + pad, shape.y + shape.h + pad, t.accent);
            for hook in shape.hooks() {
                let c = at(hook);
                let dot = Bounds::new(point(c.x - px(2.5), c.y - px(2.5)), size(px(5.), px(5.)));
                window.paint_quad(quad(dot, px(2.5), t.accent, px(0.), t.accent, gpui::BorderStyle::default()));
            }
        }
        if let Drag::Marquee { from, to } = self.drag {
            let (a, b) = (at((from.0.min(to.0), from.1.min(to.1))), at((from.0.max(to.0), from.1.max(to.1))));
            window.paint_quad(fill(Bounds::from_corners(a, b), t.selection));
        }
        if let Some((x, y)) = caret_at
            && self.focus.is_focused(window)
        {
            let top = at((x, y + diagram::LINE * 0.1));
            window.paint_quad(fill(Bounds::new(top, size(px(1.5), px(diagram::LINE * 0.8 * zoom))), t.accent));
        }
        if self.diagram.shapes.is_empty() && self.diagram.links.is_empty() {
            let hint = tr(
                "Pick a shape above, or press R, O, D, A, T…, then drag. Double-click to write.",
                "Choisis une forme ci-dessus, ou tape R, O, D, A, T…, puis fais glisser. Double-clic pour écrire.",
            );
            // Word by word: the hint wraps in a narrow canvas.
            let room = bounds.size.width - px(32.);
            let mut lines: Vec<String> = Vec::new();
            for word in hint.split(' ') {
                let longer = lines.last().map(|last| format!("{last} {word}"));
                match longer.filter(|longer| self.line(longer, false, t.soft, 0.93, window).width <= room) {
                    Some(longer) => *lines.last_mut().unwrap() = longer,
                    None => lines.push(word.to_string()),
                }
            }
            let top = bounds.center().y - px(10.) * lines.len() as f32;
            for (i, text) in lines.iter().enumerate() {
                let line = self.line(text, false, t.soft, 0.93, window);
                let origin = point(bounds.center().x - line.width / 2., top + px(20.) * i as f32);
                line.paint(origin, px(20.), window, cx).ok();
            }
        }
    }
}
