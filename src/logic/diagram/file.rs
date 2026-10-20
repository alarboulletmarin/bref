//! The diagram file: its drawing as SVG, and its source in `<metadata>`, read and written.

use super::*;

impl Diagram {
    /// The diagram file: its drawing as SVG, and its source in `<metadata>`.
    /// `neutral` is the neutral stroke color, as `0xrrggbb`.
    pub fn to_svg(&self, neutral: u32) -> String {
        let color = |i: u8| format!("#{:06x}", if i == 0 { neutral } else { COLORS[i as usize % COLORS.len()] });
        let (x0, y0, x1, y1) = self.bounds().unwrap_or((0., 0., 100., 60.));
        let (x0, y0, w, h) = (x0 - 16., y0 - 16., x1 - x0 + 32., y1 - y0 + 32.);
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" fill=\"none\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\" font-family=\"sans-serif\" font-size=\"{FONT}\">\n",
            num(w), num(h), num(x0), num(y0), num(w), num(h),
        );
        let dash = |dashed: bool| if dashed { " stroke-dasharray=\"6 4\"" } else { "" };
        let label = |out: &mut String, x: f32, y: f32, spans: &[Span], anchor: Anchor, ink: &str| {
            if spans.is_empty() {
                return;
            }
            let anchor = match anchor {
                Anchor::Start => "",
                Anchor::Middle => " text-anchor=\"middle\"",
                Anchor::End => " text-anchor=\"end\"",
            };
            // One style for the whole line: on the `<text>`; several: a `<tspan>` for each run.
            let uniform = spans.windows(2).all(|w| w[0].style == w[1].style);
            let (attrs, body) = match uniform {
                true => (style_attrs(spans[0].style), xml(&spans.iter().map(|s| s.text.as_str()).collect::<String>())),
                false => (String::new(), spans.iter().map(|s| format!("<tspan{}>{}</tspan>", style_attrs(s.style), xml(&s.text))).collect()),
            };
            let _ = writeln!(out, "<text x=\"{}\" y=\"{}\"{anchor}{attrs} fill=\"{ink}\" stroke=\"none\">{body}</text>", num(x), num(y + FONT));
        };
        for shape in &self.shapes {
            let ink = color(shape.color);
            for (i, path) in shape.outline().iter().enumerate() {
                let fill = if shape.solid().contains(&i) {
                    format!(" fill=\"{ink}\"")
                } else if i == 0 && shape.fill {
                    format!(" fill=\"{ink}\" fill-opacity=\"0.15\"")
                } else {
                    String::new()
                };
                let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{fill}{}/>", path_d(path), dash(shape.dashed || shape.dashed_paths().contains(&i)));
            }
            let (lines, rules) = self.lines(shape);
            for y in rules {
                let (x0, x1) = shape.rule_span(y);
                // Between the operands of a combined fragment: dashed.
                let _ = writeln!(out, "<path d=\"M{} {}H{}\" stroke=\"{ink}\"{}/>", num(x0), num(y), num(x1), dash(shape.form == Form::Fragment));
            }
            for (line, spans) in lines.iter().zip(self.runs(shape, false)) {
                label(&mut out, line.x, line.y, &spans, line.centered.into(), &ink);
            }
        }
        // What the messages add: the bars of the lifelines that are busy, the cross of one that is destroyed.
        let marks = self.marks();
        for bar in &marks.bars {
            let (x, y, w, h) = bar.rect();
            let ink = color(self.shape(bar.life).map_or(0, |s| s.color));
            let _ = writeln!(out, "<path d=\"M{} {}H{}V{}H{}Z\" stroke=\"{ink}\" fill=\"{ink}\" fill-opacity=\"0.15\"/>", num(x), num(y), num(x + w), num(y + h), num(x));
        }
        for (x, y) in &marks.crosses {
            let _ = writeln!(out, "<path d=\"M{} {}L{} {}M{} {}L{} {}\" stroke=\"{}\"/>", num(x - 10.), num(y - 10.), num(x + 10.), num(y + 10.), num(x + 10.), num(y - 10.), num(x - 10.), num(y + 10.), color(0));
        }
        for link in &self.links {
            let Some(trace) = self.trace(link) else { continue };
            let ink = color(link.color);
            let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{}/>", path_d(&trace.line), dash(link.dashed));
            for (path, full) in &trace.heads {
                let fill = if *full { format!(" fill=\"{ink}\"") } else { String::new() };
                let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{fill}/>", path_d(path));
            }
            let shown = self.shown_text(link);
            let lines: Vec<&str> = if shown.is_empty() { vec![""] } else { shown.split('\n').collect() };
            for (i, text) in lines.iter().enumerate() {
                let (x, y, centered) = Diagram::label_at(&trace, i, lines.len());
                label(&mut out, x, y, &self.label_runs(text), centered.into(), &ink);
            }
            for (tip, slot) in [(trace.tips[0], Slot::Start), (trace.tips[1], Slot::End)] {
                let along = if slot == Slot::Start { link.start_along } else { link.end_along };
                let lines: Vec<&str> = link.text_of(slot).split('\n').filter(|_| !link.text_of(slot).is_empty()).collect();
                for (i, text) in lines.iter().enumerate() {
                    let (x, y, anchor) = Diagram::tip_label_at(&tip, along, i, lines.len());
                    label(&mut out, x, y, &self.styled(text, false), anchor, &ink);
                }
            }
        }
        let _ = write!(out, "<metadata id=\"bref-diagram\">\n{}</metadata>\n</svg>\n", xml(&self.source()));
        out
    }

    /// The source: its version, then one line per element, text last, then the lines kept as found.
    fn source(&self) -> String {
        let name = |form: Form| Form::ALL.iter().find(|(f, _)| *f == form).map_or("rect", |(_, n)| n);
        let tip = |head: Head| Head::ALL.iter().find(|(h, _)| *h == head).map_or("none", |(_, n)| n);
        let end = |end: End| match end {
            End::Shape(id) => format!("s{id}"),
            End::Pin(id, fx, fy) => format!("s{id}@{fx},{fy}"),
            End::Point(x, y) => format!("p{},{}", num(x), num(y)),
            End::Mid(id) => format!("m{id}"),
            End::Life(id) => format!("l{id}"),
        };
        let text = |text: &str| text.replace('\\', "\\\\").replace('\n', "\\n");
        let mut out = format!("version {}\n{}{}", self.needed(), if self.rich { "rich 1\n" } else { "" }, if self.numbered { "numbered 1\n" } else { "" });
        for s in &self.shapes {
            let _ = writeln!(out, "shape {} {} {} {} {} {} {} {} {} {}", s.id, name(s.form), num(s.x), num(s.y), num(s.w), num(s.h), s.color, s.fill as u8, s.dashed as u8, text(&s.text));
        }
        for l in &self.links {
            let route = Route::ALL.iter().find(|(r, _)| *r == l.route).map_or("straight", |(_, n)| n);
            let _ = writeln!(out, "link {} {} {} {} {} {route} {} {} {}", l.id, end(l.from), end(l.to), tip(l.start), tip(l.end), l.color, l.dashed as u8, text(&l.text));
        }
        // What holds what: a line of its own for each held shape.
        for s in self.shapes.iter().filter(|s| s.parent.is_some()) {
            let _ = writeln!(out, "in {} {}{}", s.id, s.parent.unwrap_or(0), if s.operand != 0 { format!(" {}", s.operand) } else { String::new() });
        }
        // The messages in a combined fragment, and the operand of each.
        for l in self.links.iter().filter(|l| l.holder.is_some()) {
            let _ = writeln!(out, "held {} {} {}", l.id, l.holder.unwrap_or(0), l.operand);
        }
        // The texts at the ends of arrows: a line of their own, so that a build that does not know
        // them leaves the arrow as it is. Fields are told apart by tabs.
        let field = |t: &str| t.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t");
        for l in self.links.iter().filter(|l| l.has_end_texts()) {
            let _ = writeln!(out, "ends {} {}\t{}\t{}\t{}", l.id, field(&l.start_text), field(&l.end_text), num(l.start_along), num(l.end_along));
        }
        for line in &self.kept {
            let _ = writeln!(out, "{line}");
        }
        out
    }

    /// The source a Bref diagram file carries, unescaped.
    fn source_of(svg: &str) -> Option<String> {
        let body = svg.split_once("<metadata id=\"bref-diagram\">")?.1.split_once("</metadata>")?.0;
        Some(body.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&"))
    }

    /// Version declared by a source: the first `version` line, 0 without one. An
    /// unreadable number counts as the highest: what is not understood is not overwritten.
    fn version_of(source: &str) -> u32 {
        match source.lines().find(|l| is_version(l)) {
            Some(line) => line.strip_prefix("version ").and_then(|v| v.parse().ok()).unwrap_or(u32::MAX),
            None => 0,
        }
    }

    /// Whether this is a Bref diagram written by a version this one cannot read.
    pub fn newer(svg: &str) -> bool {
        Diagram::source_of(svg).is_some_and(|source| Diagram::version_of(&source) > VERSION)
    }

    /// The diagram this SVG file carries; `None` if it is not one, or if a newer version wrote it.
    pub fn from_svg(svg: &str) -> Option<Diagram> {
        let body = Diagram::source_of(svg)?;
        if Diagram::version_of(&body) > VERSION {
            return None;
        }
        let text = |raw: &str| {
            let (mut out, mut chars) = (String::new(), raw.chars());
            while let Some(c) = chars.next() {
                match (c == '\\').then(|| chars.next()).flatten() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => out.push(other),
                    None => out.push(c),
                }
            }
            out
        };
        let end = |raw: &str| match raw.split_at_checked(1)? {
            ("s", id) => match id.split_once('@') {
                Some((id, at)) => at.split_once(',').and_then(|(fx, fy)| Some(End::Pin(id.parse().ok()?, fx.parse().ok()?, fy.parse().ok()?))),
                None => Some(End::Shape(id.parse().ok()?)),
            },
            ("p", at) => at.split_once(',').and_then(|(x, y)| Some(End::Point(x.parse().ok()?, y.parse().ok()?))),
            ("m", id) => Some(End::Mid(id.parse().ok()?)),
            ("l", id) => Some(End::Life(id.parse().ok()?)),
            _ => None,
        };
        let mut diagram = Diagram::default();
        // A line this version cannot read is kept as it is: the rest of the diagram opens.
        let mut declared = false;
        // Lines about the arrows' ends wait for the arrows.
        let mut ends: Vec<(&str, &str)> = Vec::new();
        let mut held: Vec<(&str, &str)> = Vec::new();
        let mut in_fragments: Vec<(&str, &str)> = Vec::new();
        for line in body.lines() {
            // The first `version` line is the file's; a second one is just another unknown line.
            let version = !declared && is_version(line);
            declared |= version;
            if version || line.trim().is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("ends ") {
                ends.push((line, rest));
                continue;
            }
            if let Some(rest) = line.strip_prefix("in ") {
                held.push((line, rest));
                continue;
            }
            if let Some(rest) = line.strip_prefix("held ") {
                in_fragments.push((line, rest));
                continue;
            }
            let mut read = || -> Option<()> {
                if let Some(rest) = line.strip_prefix("shape ") {
                    let f: Vec<&str> = rest.splitn(10, ' ').collect();
                    let n = |i: usize| f.get(i)?.parse::<f32>().ok();
                    diagram.shapes.push(Shape {
                        id: f.first()?.parse().ok()?,
                        form: Form::ALL.iter().find(|(_, name)| Some(name) == f.get(1)).map(|(form, _)| *form)?,
                        x: n(2)?,
                        y: n(3)?,
                        w: n(4)?,
                        h: n(5)?,
                        color: f.get(6)?.parse().ok()?,
                        fill: *f.get(7)? == "1",
                        dashed: *f.get(8)? == "1",
                        text: text(f.get(9).unwrap_or(&"")),
                        parent: None,
                        operand: 0,
                    });
                    Some(())
                } else if line == "rich 1" {
                    diagram.rich = true;
                    Some(())
                } else if line == "numbered 1" {
                    diagram.numbered = true;
                    Some(())
                } else if let Some(rest) = line.strip_prefix("link ") {
                    let f: Vec<&str> = rest.splitn(9, ' ').collect();
                    let tip = |i: usize| Head::ALL.iter().find(|(_, name)| Some(name) == f.get(i)).map(|(head, _)| *head);
                    diagram.links.push(Link {
                        id: f.first()?.parse().ok()?,
                        from: end(f.get(1)?)?,
                        to: end(f.get(2)?)?,
                        start: tip(3)?,
                        end: tip(4)?,
                        route: Route::ALL.iter().find(|(_, name)| Some(name) == f.get(5)).map(|(route, _)| *route)?,
                        color: f.get(6)?.parse().ok()?,
                        dashed: *f.get(7)? == "1",
                        text: text(f.get(8).unwrap_or(&"")),
                        ..Link::blank()
                    });
                    Some(())
                } else {
                    None
                }
            };
            if read().is_none() {
                diagram.kept.push(line.to_string());
            }
        }
        for (line, rest) in held {
            let read = || -> Option<(u32, u32, u32)> {
                let mut f = rest.split(' ');
                let (id, parent): (u32, u32) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?);
                let operand = f.next().map_or(Some(0), |o| o.parse().ok())?;
                (id != parent && f.next().is_none()).then_some((id, parent, operand))
            };
            match read().filter(|(id, parent, _)| diagram.shape(*id).is_some() && diagram.shape(*parent).is_some()) {
                Some((id, parent, operand)) => {
                    if let Some(shape) = diagram.shape_mut(id) {
                        (shape.parent, shape.operand) = (Some(parent), operand);
                    }
                }
                None => diagram.kept.push(line.to_string()),
            }
        }
        diagram.arrange();
        for (line, rest) in in_fragments {
            let mut read = || -> Option<()> {
                let f: Vec<&str> = rest.split(' ').collect();
                let (id, fragment, operand): (u32, u32, u32) = (f.first()?.parse().ok()?, f.get(1)?.parse().ok()?, f.get(2)?.parse().ok()?);
                (f.len() == 3).then_some(())?;
                diagram.shape(fragment).filter(|s| s.form == Form::Fragment)?;
                let link = diagram.links.iter_mut().find(|l| l.id == id)?;
                (link.holder, link.operand) = (Some(fragment), operand);
                Some(())
            };
            if read().is_none() {
                diagram.kept.push(line.to_string());
            }
        }
        for (line, rest) in ends {
            let mut read = || -> Option<()> {
                let (id, fields) = rest.split_once(' ')?;
                let f: Vec<&str> = fields.split('\t').collect();
                let id: u32 = id.parse().ok()?;
                let link = diagram.links.iter_mut().find(|l| l.id == id)?;
                (link.start_text, link.end_text) = (text(f.first()?), text(f.get(1)?));
                (link.start_along, link.end_along) = (f.get(2)?.parse().ok()?, f.get(3)?.parse().ok()?);
                Some(())
            };
            if read().is_none() {
                diagram.kept.push(line.to_string());
            }
        }
        // A kept line still owns its id (its second field), which no new element may take.
        let owned = diagram.kept.iter().filter_map(|l| l.split(' ').nth(1)?.parse().ok());
        let ids = diagram.shapes.iter().map(|s| s.id).chain(diagram.links.iter().map(|l| l.id)).chain(owned);
        diagram.next = ids.max().unwrap_or(0);
        Some(diagram)
    }
}

/// SVG attributes of a text style, with a leading space each.
fn style_attrs(style: Style) -> String {
    let mut out = String::new();
    if style.bold {
        out += " font-weight=\"bold\"";
    }
    if style.italic {
        out += " font-style=\"italic\"";
    }
    match (style.under, style.strike) {
        (true, true) => out += " text-decoration=\"underline line-through\"",
        (true, false) => out += " text-decoration=\"underline\"",
        (false, true) => out += " text-decoration=\"line-through\"",
        _ => {}
    }
    if style.code {
        out += " font-family=\"monospace\"";
    }
    out
}

fn is_version(line: &str) -> bool {
    line == "version" || line.starts_with("version ")
}

/// Number written to the tenth, without a useless decimal.
fn num(v: f32) -> String {
    let v = (v * 10.).round() / 10.;
    if v.fract() == 0. { format!("{v:.0}") } else { format!("{v:.1}") }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn path_d(path: &[Cmd]) -> String {
    let mut d = String::new();
    for cmd in path {
        let _ = match *cmd {
            Cmd::Move(x, y) => write!(d, "M{} {}", num(x), num(y)),
            Cmd::Line(x, y) => write!(d, "L{} {}", num(x), num(y)),
            Cmd::Curve(a, b, c, e, x, y) => write!(d, "C{} {} {} {} {} {}", num(a), num(b), num(c), num(e), num(x), num(y)),
            Cmd::Close => write!(d, "Z"),
        };
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A diagram file around `source`, as another version of Bref would have written it.
    fn file(source: &str) -> String {
        format!("<svg>\n<metadata id=\"bref-diagram\">\n{}</metadata>\n</svg>\n", xml(source))
    }

    #[test]
    fn keeps_the_lines_it_cannot_read() {
        let shape = "shape 1 rect 0 0 100 60 0 0 0 A";
        // An unknown form, an unknown kind of line, a short line, a second `version`, and text
        // that XML escapes: none is dropped, and none is changed.
        let kept = [
            "shape 9 hexagon 5 5 10 10 0 0 0 <&> é 日本 😀",
            "note 4 &lt; &amp; ]]> \\\\ \u{3000}",
            "link 3 s1",
            "version 1",
            &format!("shape 7 oval {}", "x".repeat(10_000)),
        ];
        let source = format!("version 1\n{shape}\n{}\nlink 2 s1 s9 none arrow straight 0 0 x\n\n   \n{}\n", kept[0], kept[1..].join("\n"));
        let mut d = Diagram::from_svg(&file(&source)).unwrap();
        assert_eq!((d.shapes.len(), d.links.len()), (1, 1));
        assert_eq!(d.kept, kept);
        // A link to a shape it cannot read stays on disk without being drawn.
        assert!(d.trace(&d.links[0]).is_none());
        // Written back: the same bytes, and the same again after another round.
        let svg = d.to_svg(COLORS[0]);
        let source = Diagram::source_of(&svg).unwrap();
        assert!(kept.iter().all(|line| source.lines().any(|l| l == *line)));
        let back = Diagram::from_svg(&svg).unwrap();
        assert_eq!((back.to_svg(COLORS[0]), &back), (svg, &d));
        // No new element takes the id of one it cannot read.
        assert_eq!(d.add_shape(Form::Rect, 0., 0., 10., 10.), 10);
    }

    #[test]
    fn refuses_what_a_newer_version_wrote() {
        let shape = "shape 1 rect 0 0 100 60 0 0 0 A\n";
        // Higher, unreadable, not a number, beyond `u32`, with a stray space, empty: all newer.
        for version in [&(VERSION + 1).to_string(), "99999999999", "-1", "1.5", "x", " 1", "", "1 "] {
            let svg = file(&format!("version {version}\n{shape}"));
            assert!((Diagram::from_svg(&svg), Diagram::newer(&svg)) == (None, true), "version {version:?}");
        }
        let svg = file(&format!("{shape}version\n"));
        assert!((Diagram::from_svg(&svg), Diagram::newer(&svg)) == (None, true));
        // The version may come after the elements.
        assert!(Diagram::newer(&file(&format!("{shape}version {}\n", VERSION + 1))));
        // Version 0, 1, none, or an empty source: opens. Not a Bref file: not "newer", just foreign.
        for source in ["version 0\n", "version 1\n", "", "\n"] {
            let svg = file(&format!("{source}{shape}"));
            assert!(Diagram::from_svg(&svg).is_some_and(|d| d.shapes.len() == 1) && !Diagram::newer(&svg), "{source:?}");
        }
        assert_eq!(Diagram::from_svg(&file("")), Some(Diagram::default()));
        assert!(!Diagram::newer("<svg></svg>") && !Diagram::newer(""));
    }

    #[test]
    fn an_old_file_opens_as_it_was() {
        // Written by 0.3.8: no `version` line.
        let old = "shape 1 rect 0 0 120 60 0 0 0 Compte\\n---\\n+ solde\nshape 2 ellipse 300 0 100 100 1 1 0 \nlink 3 s1 s2@0.5,1 none triangle elbow 0 1 hérite\n";
        let d = Diagram::from_svg(&file(old)).unwrap();
        assert!(d.kept.is_empty() && (d.shapes.len(), d.links.len()) == (2, 1));
        // Saving adds the version line, and nothing else.
        let svg = d.to_svg(COLORS[0]);
        assert_eq!(Diagram::source_of(&svg).unwrap().trim_start(), format!("version 1\n{old}"));
        assert_eq!(Diagram::from_svg(&svg).unwrap().to_svg(COLORS[0]), svg);
    }

    #[test]
    fn styled_text_is_written_and_read_back() {
        let mut d = Diagram::blank();
        let a = d.add_shape(Form::Rect, 0., 0., 200., 80.);
        d.shape_mut(a).unwrap().text = "*Shape*\n---\n+ <u>count</u>: int\n+ **area**(): f32\n~~old~~ `List<T>`".into();
        let b = d.add_shape(Form::Rect, 300., 0., 100., 40.);
        d.shape_mut(b).unwrap().text = "*whole line*".into();
        let link = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
        d.link_mut(link).unwrap().text = "*calls*".into();
        let svg = d.to_svg(COLORS[0]);
        // A line with several styles has a run for each; a line with one has it on the text itself.
        assert!(svg.contains("<tspan font-style=\"italic\">Shape</tspan>") || svg.contains("font-style=\"italic\" fill"), "{svg}");
        assert!(svg.contains("<tspan text-decoration=\"underline\">count</tspan>") && svg.contains("<tspan font-weight=\"bold\">area</tspan>"));
        assert!(svg.contains("text-decoration=\"line-through\"") && svg.contains("font-family=\"monospace\">List&lt;T&gt;"));
        assert!(svg.contains("font-style=\"italic\" fill=\"#7d8590\" stroke=\"none\">whole line</text>"));
        // The marks are not drawn, but they are in the source.
        assert!(!svg.contains(">*Shape*<") && Diagram::source_of(&svg).unwrap().contains("*Shape*"));
        assert_eq!(Diagram::source_of(&svg).unwrap().trim_start().lines().take(2).collect::<Vec<_>>(), ["version 2", "rich 1"]);
        let back = Diagram::from_svg(&svg).unwrap();
        assert!(back.rich && back == d && back.to_svg(COLORS[0]) == svg);
    }

    #[test]
    fn a_diagram_from_before_styles_keeps_its_stars() {
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 40.);
        d.shape_mut(a).unwrap().text = "a * b * c *d* <u>e</u>".into();
        let svg = d.to_svg(COLORS[0]);
        assert!(svg.contains(">a * b * c *d* &lt;u&gt;e&lt;/u&gt;</text>") && !svg.contains("italic") && !svg.contains("tspan"));
        // Plain, it still opens in the builds that know only version 1.
        assert_eq!(Diagram::source_of(&svg).unwrap().trim_start().lines().next(), Some("version 1"));
        assert!(!Diagram::from_svg(&svg).unwrap().rich);
    }

    #[test]
    fn the_texts_at_the_ends_of_an_arrow_have_a_line_of_their_own() {
        let mut d = Diagram::blank();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 60.);
        let b = d.add_shape(Form::Rect, 300., 0., 100., 60.);
        let link = d.add_link(End::Shape(a), End::Shape(b), Head::DiamondFull, Route::Straight);
        let l = d.link_mut(link).unwrap();
        (l.text, l.start_text, l.end_text, l.start_along, l.end_along) = ("owns".into(), "1".into(), "0..*\n+items\t| \\ é 日本 😀 <&>".into(), 12.5, 0.);
        let svg = d.to_svg(COLORS[0]);
        let source = Diagram::source_of(&svg).unwrap();
        assert_eq!(source.trim_start().lines().take(2).collect::<Vec<_>>(), ["version 3", "rich 1"]);
        // The shape and link lines are the ones of the builds before: only a line is added.
        assert!(source.contains("\nlink 3 s1 s2 none fulldiamond straight 0 0 owns\n") && source.contains("\nends 3 1\t0..*\\n+items\\t| \\\\ é 日本 😀 <&>\t12.5\t0\n"));
        // Drawn: the start text before the diamond's head, the end text after the other end.
        assert!(svg.contains(">1</text>") && svg.contains("text-anchor=\"end\" fill=\"#7d8590\" stroke=\"none\">0..*</text>"), "{svg}");
        let back = Diagram::from_svg(&svg).unwrap();
        assert!(back == d && back.to_svg(COLORS[0]) == svg);
        // No text at the ends, no line, and the file stays readable by the builds before.
        let mut plain = d.clone();
        let l = plain.link_mut(link).unwrap();
        (l.start_text, l.end_text) = (String::new(), String::new());
        l.start_along = 0.;
        let source = Diagram::source_of(&plain.to_svg(COLORS[0])).unwrap();
        assert!(!source.contains("ends ") && source.contains("version 2\n"));
        // A line about an arrow that is not there, or that is cut short, is kept, not applied.
        let svg = file("version 3\nshape 1 rect 0 0 10 10 0 0 0 \nshape 2 rect 50 0 10 10 0 0 0 \nlink 3 s1 s2 none arrow straight 0 0 \nends 9 a\tb\t0\t0\nends 3 only\nends x\n");
        let d = Diagram::from_svg(&svg).unwrap();
        assert_eq!(d.kept, ["ends 9 a\tb\t0\t0", "ends 3 only", "ends x"]);
        assert!(!d.links[0].has_end_texts());
    }

    #[test]
    fn draws_a_class_with_its_notation() {
        let mut d = Diagram::blank();
        let a = d.add_shape(Form::Rect, 0., 0., 240., 160.);
        d.shape_mut(a).unwrap().text = "«abstract»\nShape\n---\n+ static count: int\n- /area: f32 {readOnly}\n---\n+ abstract draw()\n+ see | the \\ note".into();
        let svg = d.to_svg(COLORS[0]);
        // The stereotype is plain and centered; the name is bold and, the class being abstract, italic.
        assert!(svg.contains("text-anchor=\"middle\" fill=\"#7d8590\" stroke=\"none\">«abstract»</text>"), "{svg}");
        assert!(svg.contains("font-weight=\"bold\" font-style=\"italic\" fill=\"#7d8590\" stroke=\"none\">Shape</text>"), "{svg}");
        // A static member is underlined, an abstract one in italics, the keyword is not drawn.
        assert!(svg.contains("font-style=\"italic\"") && svg.contains("text-decoration=\"underline\" fill=\"#7d8590\" stroke=\"none\">+ count: int</text>"), "{svg}");
        let drawing = svg.split("<metadata").next().unwrap();
        assert!(drawing.contains(">+ draw()</text>") && !drawing.contains("static ") && !drawing.contains("abstract draw"));
        // What is not a member is drawn as typed.
        assert!(svg.contains(">+ see | the \\ note</text>") && svg.contains(">- /area: f32 {readOnly}</text>"));
        // The source keeps what was typed, and a diagram of before the styles draws the same text as typed.
        assert!(Diagram::source_of(&svg).unwrap().contains("+ static count: int"));
        d.rich = false;
        let old = d.to_svg(COLORS[0]);
        assert!(old.contains(">+ static count: int</text>") && old.contains(">«abstract»</text>") && !old.contains("italic"), "{old}");
    }

    #[test]
    fn what_a_build_before_cannot_draw_asks_for_version_4() {
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 60.);
        let b = d.add_shape(Form::Rect, 300., 0., 100., 60.);
        let link = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
        let version = |d: &Diagram| Diagram::source_of(&d.to_svg(COLORS[0])).unwrap().trim_start().lines().next().unwrap().to_string();
        assert_eq!(version(&d), "version 1");
        // A new head, an arrow held by a middle, a new form: each asks for it, and reads back the same.
        for change in ["cross", "ball", "socket", "mid", "branch"] {
            let mut d = d.clone();
            match change {
                "cross" => d.link_mut(link).unwrap().end = Head::Cross,
                "ball" => d.link_mut(link).unwrap().start = Head::Ball,
                "socket" => d.link_mut(link).unwrap().end = Head::Socket,
                "mid" => drop(d.add_link(End::Shape(a), End::Mid(link), Head::None, Route::Straight)),
                _ => drop(d.add_shape(Form::Branch, 150., 150., 40., 40.)),
            }
            let svg = d.to_svg(COLORS[0]);
            assert_eq!(version(&d), "version 4", "{change}");
            assert_eq!(Diagram::from_svg(&svg).as_ref(), Some(&d), "{change}");
            assert_eq!(Diagram::from_svg(&svg).unwrap().to_svg(COLORS[0]), svg, "{change}");
        }
        d.add_link(End::Shape(a), End::Mid(link), Head::None, Route::Straight);
        assert!(Diagram::source_of(&d.to_svg(COLORS[0])).unwrap().contains("\nlink 4 s1 m3 none none straight 0 0 \n"));
    }

    #[test]
    fn what_a_shape_holds_has_a_line_of_its_own() {
        let mut d = Diagram::blank();
        let pack = d.add_shape(Form::Package, 0., 0., 300., 200.);
        d.shape_mut(pack).unwrap().text = "billing\npkg::billing".into();
        let frame = d.add_shape(Form::Frame, 20., 40., 200., 140.);
        d.shape_mut(frame).unwrap().text = "System".into();
        let a = d.add_shape(Form::Rect, 40., 90., 80., 40.);
        let lane = d.add_shape(Form::Lane, 400., 0., 160., 300.);
        d.shape_mut(lane).unwrap().text = "Customer".into();
        assert!(d.set_parent(frame, Some(pack)) && d.set_parent(a, Some(frame)));
        let svg = d.to_svg(COLORS[0]);
        let source = Diagram::source_of(&svg).unwrap();
        assert_eq!(source.trim_start().lines().take(2).collect::<Vec<_>>(), ["version 5", "rich 1"]);
        // The shape lines are those of the builds before; each held shape adds a line of its own.
        assert!(source.contains(&format!("\nin {frame} {pack}\n")) && source.contains(&format!("\nin {a} {frame}\n")) && !source.contains(&format!("in {pack} ")));
        // The package's name is in its tab, the second line under it; the lane's in its header.
        assert!(svg.contains(">billing</text>") && svg.contains(">pkg::billing</text>") && svg.contains("text-anchor=\"middle\"") && svg.contains(">Customer</text>"), "{svg}");
        let back = Diagram::from_svg(&svg).unwrap();
        assert!(back == d && back.to_svg(COLORS[0]) == svg);
        // No holding, no line, and no version 5.
        let mut plain = d.clone();
        plain.set_parent(a, None);
        plain.set_parent(frame, None);
        plain.shapes.retain(|s| !s.form.holds());
        assert!(!Diagram::source_of(&plain.to_svg(COLORS[0])).unwrap().contains("\nin "));
        assert!(Diagram::source_of(&plain.to_svg(COLORS[0])).unwrap().contains("version 2\n"));
        // A held shape before its holder is put after; a line about a shape that is not there, or that holds
        // itself, is kept and not applied.
        let svg = file("version 5\nshape 1 rect 0 0 10 10 0 0 0 \nshape 2 package 0 0 100 100 0 0 0 P\nin 1 2\nin 1 9\nin 2 2\nin x\n");
        let d = Diagram::from_svg(&svg).unwrap();
        assert_eq!(d.shapes.iter().map(|s| s.id).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(d.shape(1).unwrap().parent, Some(2));
        assert_eq!(d.kept, ["in 1 9", "in 2 2", "in x"]);
    }

    #[test]
    fn a_state_machine_is_drawn_with_its_dots_and_its_guards() {
        let mut d = Diagram::blank();
        let start = d.add_shape(Form::Dot, 20., 40., 20., 20.);
        let red = d.add_shape(Form::Round, 100., 20., 120., 60.);
        d.shape_mut(red).unwrap().text = "Red\n---\nentry / stop()".into();
        let end = d.add_shape(Form::Final, 300., 40., 26., 26.);
        let fork = d.add_shape(Form::Bar, 100., 120., 90., 8.);
        let link = d.add_link(End::Shape(red), End::Shape(end), Head::Arrow, Route::Straight);
        d.link_mut(link).unwrap().text = "timer [clear] / go()".into();
        d.add_link(End::Shape(red), End::Shape(red), Head::Arrow, Route::Elbow);
        let _ = (start, fork);
        let svg = d.to_svg(COLORS[0]);
        assert!(Diagram::source_of(&svg).unwrap().trim_start().starts_with("version 6\n"), "{svg}");
        // The dot and the bar are filled solid, the final state has its inner dot filled and not its ring.
        let paths: Vec<&str> = svg.lines().filter(|l| l.starts_with("<path")).collect();
        assert_eq!(paths.iter().filter(|p| p.contains("fill=\"#7d8590\"") && !p.contains("fill-opacity")).count(), 3, "{svg}");
        // The guard alone is in italics.
        assert!(svg.contains("<tspan font-style=\"italic\">[clear]</tspan>") && svg.contains(">timer </tspan>") && svg.contains("> / go()</tspan>"), "{svg}");
        // Read back and written again: the same.
        let back = Diagram::from_svg(&svg).unwrap();
        assert!(back == d && back.to_svg(COLORS[0]) == svg);
        // A diagram from before keeps the label as typed.
        d.rich = false;
        assert!(d.to_svg(COLORS[0]).contains(">timer [clear] / go()</text>"));
    }

    #[test]
    fn a_sequence_diagram_is_written_with_its_bars_and_its_numbers() {
        let template = crate::logic::library::templates().into_iter().find(|t| t.name == crate::lang::tr("Sequence diagram", "Diagramme de séquence")).unwrap();
        let mut d = (template.make)();
        let svg = d.to_svg(COLORS[0]);
        let source = Diagram::source_of(&svg).unwrap();
        assert!(source.trim_start().starts_with("version 7\nrich 1\n") && source.contains(" l1 l2 ") && source.contains("shape 1 lifeline "), "{source}");
        // The line of a lifeline is dashed, its head is not; a bar is drawn for each call that waits.
        let paths: Vec<&str> = svg.lines().filter(|l| l.starts_with("<path")).collect();
        assert_eq!(paths.iter().filter(|p| p.contains("stroke-dasharray") && p.contains("fill-opacity")).count(), 0);
        assert!(paths.iter().filter(|p| p.contains("fill-opacity=\"0.15\"")).count() >= 3, "{svg}");
        // Numbered, the messages show their place; unnumbered, they do not, and the file says so with a line.
        assert!(!svg.contains(">1: login") && !source.contains("numbered"));
        d.numbered = true;
        let numbered = d.to_svg(COLORS[0]);
        assert!(numbered.contains(">1: login(user, password)</text>") && numbered.contains(">5: token</text>"), "{numbered}");
        assert!(Diagram::source_of(&numbered).unwrap().trim_start().starts_with("version 7\nrich 1\nnumbered 1\n"));
        // What is typed is not changed: only what is shown.
        assert_eq!(d.links[0].text, "login(user, password)");
        let back = Diagram::from_svg(&numbered).unwrap();
        assert!(back == d && back.to_svg(COLORS[0]) == numbered);
        // A message with no text has only its number.
        d.link_mut(d.links[1].id).unwrap().text.clear();
        assert_eq!(d.shown_text(&d.links[1].clone()), "2");
        // A diagram of no message draws no bar and writes no sequence line.
        let plain = Diagram::blank();
        assert!(!plain.to_svg(COLORS[0]).contains("fill-opacity"));
    }

    #[test]
    fn combined_fragments_are_written_nested_and_a_damaged_file_loses_only_the_line() {
        let template = crate::logic::library::templates().into_iter().find(|t| t.name == crate::lang::tr("Sequence diagram", "Diagramme de séquence")).unwrap();
        let mut d = (template.make)();
        let m = d.messages();
        let outer = d.wrap(&[m[1], m[2], m[3]], "alt\n[ok]\n[else]").unwrap();
        let inner = d.wrap(&[m[1], m[2]], "loop\n[n]").unwrap();
        d.link_mut(m[3]).unwrap().operand = 1;
        let svg = d.to_svg(COLORS[0]);
        let source = Diagram::source_of(&svg).unwrap();
        // A diagram of fragments needs version 8; the lines are keyed by id, never new columns.
        assert!(source.trim_start().starts_with("version 8\n"), "{source}");
        assert!(source.contains(&format!("in {inner} {outer}\n")) && source.contains(&format!("held {} {inner} 0\n", m[1])) && source.contains(&format!("held {} {outer} 1\n", m[3])), "{source}");
        let back = Diagram::from_svg(&svg).unwrap();
        assert!(back == d && back.to_svg(COLORS[0]) == svg);
        // The dashed line between the operands is drawn, the operator is in bold in the tab.
        assert!(svg.contains(">alt</text>") && svg.lines().any(|l| l.starts_with("<path d=\"M") && l.contains("stroke-dasharray")), "{svg}");
        // A file with no fragment is still version 7.
        let plain = (template.make)();
        assert!(Diagram::source_of(&plain.to_svg(COLORS[0])).unwrap().trim_start().starts_with("version 7\n"));
        // Damaged: a message held by a shape that is no fragment, by a missing one, with a fourth field, a word as operand:
        // each line is kept as it is, the arrow is not held, and nothing panics or is lost.
        for bad in [format!("held {} {} 0", m[0], d.shapes.iter().find(|s| s.form == Form::Lifeline).unwrap().id), format!("held {} 999 0", m[0]), format!("held {} {outer} 0 7", m[0]), format!("held {} {outer} x", m[0]), format!("held 999 {outer} 0")] {
            let damaged = svg.replacen("held ", &format!("{bad}\nheld "), 1);
            let read = Diagram::from_svg(&damaged).unwrap_or_else(|| panic!("{bad}"));
            assert!(read.link(m[0]).unwrap().holder.is_none(), "{bad}");
            assert!(read.kept.contains(&bad) && read.to_svg(COLORS[0]).contains(&bad), "{bad}");
        }
        // Fragments held by each other in a loop: the messages are still laid out, once.
        let looped = svg.replace(&format!("in {inner} {outer}"), &format!("in {inner} {outer}\nin {outer} {inner}"));
        let read = Diagram::from_svg(&looped).unwrap();
        assert_eq!(read.layout().ys.len(), read.messages().len());
    }

    #[test]
    fn an_activity_diagram_asks_for_version_9_only_for_its_new_forms() {
        let template = crate::logic::library::templates().into_iter().find(|t| t.name == crate::lang::tr("Activity diagram", "Diagramme d'activité")).unwrap();
        let activity = (template.make)();
        let svg = activity.to_svg(COLORS[0]);
        let source = Diagram::source_of(&svg).unwrap();
        assert!(source.trim_start().starts_with("version 9\n") && source.contains(" pin ") && source.contains("\nin "), "{source}");
        let back = Diagram::from_svg(&svg).unwrap();
        assert!(back == activity && back.to_svg(COLORS[0]) == svg);
        // The pin is held by its action, and read back on the border.
        let pin = back.shapes.iter().find(|s| s.form == Form::Pin).unwrap();
        let holder = back.shape(pin.parent.unwrap()).unwrap();
        assert_eq!(holder.form, Form::Round);
        assert_eq!(pin.center().0, holder.x + holder.w);
        // Only the swimlanes and the nodes it shares with state machines: still version 6, so older builds read it.
        let mut plain = Diagram::blank();
        plain.add_shape(Form::Bar, 0., 0., 90., 8.);
        plain.add_shape(Form::Lane, 0., 20., 100., 100.);
        assert!(Diagram::source_of(&plain.to_svg(COLORS[0])).unwrap().trim_start().starts_with("version 6\n"));
        // A form held by a container keeps the version of its form (a dot in a lane is not a version 5).
        let mut held = Diagram::blank();
        let (lane, dot) = (held.add_shape(Form::Lane, 0., 0., 100., 100.), held.add_shape(Form::Dot, 40., 40., 20., 20.));
        held.set_parent(dot, Some(lane));
        assert!(Diagram::source_of(&held.to_svg(COLORS[0])).unwrap().trim_start().starts_with("version 6\n"));
        // Each of the new forms asks for 9 on its own; a damaged `in` line (a pin held by a shape that is not there) is kept.
        for form in [Form::Behavior, Form::Pin, Form::Row] {
            let mut d = Diagram::blank();
            d.add_shape(form, 0., 0., 40., 40.);
            assert!(Diagram::source_of(&d.to_svg(COLORS[0])).unwrap().trim_start().starts_with("version 9\n"), "{form:?}");
        }
        let damaged = svg.replacen("\nin ", "\nin 77 99\nin ", 1);
        let read = Diagram::from_svg(&damaged).unwrap();
        assert!(read.kept.contains(&"in 77 99".to_string()) && read.to_svg(COLORS[0]).contains("in 77 99"));
    }
}
