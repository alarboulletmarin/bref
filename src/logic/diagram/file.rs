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
        let label = |out: &mut String, x: f32, y: f32, spans: &[Span], centered: bool, ink: &str| {
            if spans.is_empty() {
                return;
            }
            let anchor = if centered { " text-anchor=\"middle\"" } else { "" };
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
                let fill = if i == 0 && shape.fill { format!(" fill=\"{ink}\" fill-opacity=\"0.15\"") } else { String::new() };
                let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{fill}{}/>", path_d(path), dash(shape.dashed));
            }
            let (lines, rules) = shape.text_lines();
            for y in rules {
                let _ = writeln!(out, "<path d=\"M{} {}H{}\" stroke=\"{ink}\"/>", num(shape.x), num(y), num(shape.x + shape.w));
            }
            for line in lines {
                label(&mut out, line.x, line.y, &self.styled(line.text, line.bold), line.centered, &ink);
            }
        }
        for link in &self.links {
            let Some(trace) = self.trace(link) else { continue };
            let ink = color(link.color);
            let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{}/>", path_d(&trace.line), dash(link.dashed));
            for (path, full) in &trace.heads {
                let fill = if *full { format!(" fill=\"{ink}\"") } else { String::new() };
                let _ = writeln!(out, "<path d=\"{}\" stroke=\"{ink}\"{fill}/>", path_d(path));
            }
            let lines: Vec<&str> = link.text.split('\n').collect();
            for (i, text) in lines.iter().enumerate() {
                let (x, y, centered) = Diagram::label_at(&trace, i, lines.len());
                label(&mut out, x, y, &self.styled(text, false), centered, &ink);
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
        };
        let text = |text: &str| text.replace('\\', "\\\\").replace('\n', "\\n");
        let mut out = format!("version {}\n{}", self.needed(), if self.rich { "rich 1\n" } else { "" });
        for s in &self.shapes {
            let _ = writeln!(out, "shape {} {} {} {} {} {} {} {} {} {}", s.id, name(s.form), num(s.x), num(s.y), num(s.w), num(s.h), s.color, s.fill as u8, s.dashed as u8, text(&s.text));
        }
        for l in &self.links {
            let route = Route::ALL.iter().find(|(r, _)| *r == l.route).map_or("straight", |(_, n)| n);
            let _ = writeln!(out, "link {} {} {} {} {} {route} {} {} {}", l.id, end(l.from), end(l.to), tip(l.start), tip(l.end), l.color, l.dashed as u8, text(&l.text));
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
            _ => None,
        };
        let mut diagram = Diagram::default();
        // A line this version cannot read is kept as it is: the rest of the diagram opens.
        let mut declared = false;
        for line in body.lines() {
            // The first `version` line is the file's; a second one is just another unknown line.
            let version = !declared && is_version(line);
            declared |= version;
            if version || line.trim().is_empty() {
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
                    });
                    Some(())
                } else if line == "rich 1" {
                    diagram.rich = true;
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
}
