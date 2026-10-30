//! Diagrams and figures. A diagram file is written only by `Diagram::to_svg`: its version
//! stays, and a line this version cannot read is kept.

use std::{collections::HashMap, fs};

use serde_json::{Value, json};

use crate::{
    logic::{
        diagram::{self, Diagram, End, Form, Head, Route},
        import, library, mermaid,
        relation::Relation,
    },
    platform::vault as files,
    ui::{figure, svg},
};

use super::{Outcome, Server, arg_opt, arg_str, disk, vault::{self, Vault}};

const FLOW: &str = "this Mermaid diagram cannot become a canvas (only classDiagram, sequenceDiagram and stateDiagram can). Put it in a note as a ```mermaid block, then render that block.";
const NEWER: &str = "this diagram was written by a newer version of Bref and is read-only";

/// Stable names of the library's starting points, in the order `library::templates` returns them.
const TEMPLATES: [&str; 8] = ["class", "use-case", "package", "state", "sequence", "activity", "component", "deployment"];

/// How many library shapes belong to each family, in the order `library::presets` returns them.
const FAMILIES: [(&str, usize); 6] = [("class", 8), ("use-case", 3), ("state", 13), ("activity", 13), ("component", 6), ("sequence", 10)];

pub fn check_mermaid(args: &Value) -> Outcome {
    let source = match arg_str(args, "source") {
        Ok(source) => source,
        Err(message) => return Outcome::err(message),
    };
    let kind = mermaid_kind(source);
    let canvas = matches!(kind, "class" | "sequence" | "state");
    match figure::mermaid(source, &look(false)) {
        Ok(_) => Outcome::ok(json!({"readable": true, "error": null, "kind": kind, "canvas": canvas})),
        Err(error) => Outcome::ok(json!({"readable": false, "error": error, "kind": kind, "canvas": false})),
    }
}

pub fn render(server: &mut Server, args: &Value) -> Outcome {
    let theme = match arg_opt(args, "theme").unwrap_or("light") {
        "light" => false,
        "dark" => true,
        other => return Outcome::err(format!("unknown theme `{other}`")),
    };
    let width = args.get("width").and_then(|v| v.as_f64()).map(|n| n as f32);
    if width.is_some_and(|n| n <= 0.) {
        return Outcome::err("`width` must be greater than zero");
    }
    let (mermaid_src, path, note) = (arg_opt(args, "mermaid"), arg_opt(args, "path"), arg_opt(args, "note"));
    let given = usize::from(mermaid_src.is_some()) + usize::from(path.is_some()) + usize::from(note.is_some());
    if given != 1 {
        return Outcome::err("give one of `path`, `mermaid` or `note`");
    }
    let (svg, kind, shown) = if let Some(source) = mermaid_src {
        match figure::mermaid(source, &look(theme)) {
            Ok(svg) => (svg, "mermaid", None),
            Err(error) => return Outcome::err(error),
        }
    } else {
        let vault = match server.need() {
            Ok(vault) => vault,
            Err(outcome) => return outcome,
        };
        match picture(vault, path, note, args, theme) {
            Ok(found) => found,
            Err(outcome) => return outcome,
        }
    };
    match raster(&svg, width) {
        Ok((png, w, h)) => Outcome::image(json!({"width": w, "height": h, "kind": kind, "path": shown}), &png),
        Err(message) => Outcome::err(message),
    }
}

pub fn create_diagram(vault: &mut Vault, args: &Value) -> Outcome {
    let name = match arg_str(args, "name") {
        Ok(name) => name,
        Err(message) => return Outcome::err(message),
    };
    let Some(from) = args.get("from").filter(|v| v.is_object()) else {
        return Outcome::err("`from` must be an object");
    };
    let (diagram, skipped) = match build(from) {
        Ok(built) => built,
        Err(message) => return Outcome::err(message),
    };
    match write_diagram(vault, name, arg_opt(args, "folder"), &diagram, skipped) {
        Ok(outcome) => outcome,
        Err(outcome) => outcome,
    }
}

pub fn read_diagram(vault: &Vault, args: &Value) -> Outcome {
    let path = match open_file(vault, args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let text = match fs::read_to_string(&path).map_err(disk) {
        Ok(text) => text,
        Err(message) => return Outcome::err(message),
    };
    if Diagram::newer(&text) {
        return Outcome::ok(json!({
            "path": vault::rel(&vault.root, &path),
            "read_only": true,
            "shapes": [],
            "links": [],
            "bounds": null,
            "note": NEWER,
        }));
    }
    let Some(diagram) = Diagram::from_svg(&text) else {
        return Outcome::err("this SVG is not a Bref diagram");
    };
    Outcome::ok(described(&diagram, &vault::rel(&vault.root, &path), false))
}

pub fn edit_diagram(vault: &mut Vault, args: &Value) -> Outcome {
    let path = match open_file(vault, args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let text = match fs::read_to_string(&path).map_err(disk) {
        Ok(text) => text,
        Err(message) => return Outcome::err(message),
    };
    if Diagram::newer(&text) {
        return Outcome::err(NEWER);
    }
    let Some(diagram) = Diagram::from_svg(&text) else {
        return Outcome::err("this SVG is not a Bref diagram");
    };
    let Some(ops) = args.get("ops").and_then(|v| v.as_array()) else {
        return Outcome::err("`ops` must be a list");
    };
    let mut draft = diagram;
    let mut keys: HashMap<String, u32> = HashMap::new();
    let mut shapes = Vec::new();
    let mut links = Vec::new();
    for op in ops {
        match apply(&mut draft, op, &mut keys, &mut shapes, &mut links) {
            Ok(()) => {}
            Err(message) => return Outcome::err(message),
        }
    }
    draft.fit_sequence();
    if let Err(message) = files::write(&path, &draft.to_svg(diagram::COLORS[0])).map_err(disk) {
        return Outcome::err(message);
    }
    vault.rescan();
    let mut described = described(&draft, &vault::rel(&vault.root, &path), false);
    described["ids"] = ids_of(&keys);
    described["new_shapes"] = json!(shapes);
    described["new_links"] = json!(links);
    Outcome::ok(described)
}

pub fn library(args: &Value) -> Outcome {
    match library_value(arg_opt(args, "family")) {
        Ok(value) => Outcome::ok(value),
        Err(message) => Outcome::err(message),
    }
}

pub fn import_diagram(vault: &mut Vault, args: &Value) -> Outcome {
    let content = match arg_str(args, "content") {
        Ok(content) => content,
        Err(message) => return Outcome::err(message),
    };
    let format = match arg_str(args, "format") {
        Ok(format) => format,
        Err(message) => return Outcome::err(message),
    };
    let name = match arg_str(args, "name") {
        Ok(name) => name,
        Err(message) => return Outcome::err(message),
    };
    let imported = match format {
        "excalidraw" => import::excalidraw(content).map_err(|_| "not an Excalidraw file".to_string()),
        "drawio" => import::drawio(content).map_err(|_| drawio_error(content)),
        other => Err(format!("unknown format `{other}`")),
    };
    let imported = match imported {
        Ok(imported) => imported,
        Err(message) => return Outcome::err(message),
    };
    write_diagram(vault, name, arg_opt(args, "folder"), &imported.diagram, imported.skipped).unwrap_or_else(|e| e)
}

/// What `diagram_library` returns, also served at `bref://library`.
pub fn library_value(family: Option<&str>) -> Result<Value, String> {
    let families = family_of_each();
    if let Some(family) = family
        && !FAMILIES.iter().any(|(name, _)| *name == family)
    {
        return Err(format!("unknown family `{family}`"));
    }
    let shapes: Vec<Value> = library::presets()
        .into_iter()
        .enumerate()
        .filter(|(i, _)| family.is_none_or(|name| families.get(*i).copied() == Some(name)))
        .map(|(i, preset)| {
            json!({
                "family": families.get(i).copied().unwrap_or("other"),
                "form": form_name(preset.form),
                "name": preset.name,
                "text": preset.text,
                "w": preset.w,
                "h": preset.h,
                "fill": preset.fill,
                "dashed": preset.dashed,
            })
        })
        .collect();
    let templates: Vec<Value> = library::templates()
        .into_iter()
        .enumerate()
        .map(|(i, template)| json!({"key": TEMPLATES.get(i).copied().unwrap_or("diagram"), "name": template.name}))
        .filter(|row| family.is_none_or(|name| row["key"] == name))
        .collect();
    Ok(json!({
        "forms": Form::ALL.iter().map(|(form, name)| json!({"name": name, "holds": form.holds()})).collect::<Vec<_>>(),
        "heads": Head::ALL.iter().map(|(_, name)| name).collect::<Vec<_>>(),
        "routes": Route::ALL.iter().map(|(_, name)| name).collect::<Vec<_>>(),
        "relations": Relation::ALL.iter().copied().map(relation_row).collect::<Vec<_>>(),
        "shapes": shapes,
        "templates": templates,
    }))
}

fn family_of_each() -> Vec<&'static str> {
    let mut names = Vec::new();
    for (name, count) in FAMILIES {
        names.extend(std::iter::repeat(name).take(count));
    }
    names
}

fn relation_row(relation: Relation) -> Value {
    let (start, end, dashed) = relation.look();
    json!({
        "name": relation_key(relation),
        "start": head_name(start),
        "end": head_name(end),
        "dashed": dashed,
        "stereotype": relation.stereotype(),
    })
}

fn relation_key(relation: Relation) -> &'static str {
    use Relation::*;
    match relation {
        Association => "association",
        Navigable => "navigable",
        NonNavigable => "non-navigable",
        Aggregation => "aggregation",
        Composition => "composition",
        Generalization => "generalization",
        Realization => "realization",
        Dependency => "dependency",
        Use => "use",
        Import => "import",
        Access => "access",
        Include => "include",
        Extend => "extend",
        Merge => "merge",
        Deploy => "deploy",
        Manifest => "manifest",
        AssociationClass => "association-class",
        Ball => "ball",
        Socket => "socket",
        Sync => "sync",
        Create => "create",
        Destroy => "destroy",
        Found => "found",
        Lost => "lost",
    }
}

fn mermaid_kind(source: &str) -> &'static str {
    match mermaid::kind(source) {
        Some(mermaid::Kind::Class) => "class",
        Some(mermaid::Kind::Sequence) => "sequence",
        Some(mermaid::Kind::State) => "state",
        None => {
            let word = source.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with("%%")).unwrap_or("");
            let word = word.split(|c: char| c.is_whitespace() || c == ';').next().unwrap_or("");
            if word.eq_ignore_ascii_case("flowchart") || word.eq_ignore_ascii_case("graph") { "flowchart" } else { "other" }
        }
    }
}

fn look(dark: bool) -> figure::Look {
    if dark {
        figure::Look { dark: true, bg: "#1e1e1e".into(), fill: "#2a2a2a".into(), text: "#e6e6e6".into(), line: "#9a9a9a".into(), font: "sans-serif".into() }
    } else {
        figure::Look { dark: false, bg: "#ffffff".into(), fill: "#f3f3f3".into(), text: "#1c1c1c".into(), line: "#5c5c5c".into(), font: "sans-serif".into() }
    }
}

fn ink(dark: bool) -> u32 {
    if dark { 0xe6e6e6 } else { 0x1c1c1c }
}

/// The SVG to draw, what kind it is, and the vault path when it came from a file.
fn picture(vault: &Vault, path: Option<&str>, note: Option<&str>, args: &Value, dark: bool) -> Result<(String, &'static str, Option<String>), Outcome> {
    if let Some(note) = note {
        let block = super::arg_usize(args, "block").ok_or_else(|| Outcome::err("`block` is missing"))?;
        if block == 0 {
            return Err(Outcome::err("`block` starts at 1"));
        }
        let found = vault.find_note(note).map_err(Outcome::err)?;
        let text = fs::read_to_string(&found.path).map_err(disk).map_err(Outcome::err)?;
        let blocks = mermaid_blocks(&text);
        let Some(source) = blocks.get(block - 1) else {
            return Err(Outcome::err(format!("no Mermaid block {block}")));
        };
        let svg = figure::mermaid(source, &look(dark)).map_err(Outcome::err)?;
        return Ok((svg, "mermaid", Some(vault::rel(&vault.root, &found.path))));
    }
    let path = path.unwrap_or("");
    let file = vault.find_file(path).map_err(Outcome::err)?;
    let text = fs::read_to_string(&file).map_err(disk).map_err(Outcome::err)?;
    let shown = Some(vault::rel(&vault.root, &file));
    if Diagram::newer(&text) {
        return Err(Outcome::err(NEWER));
    }
    if let Some(diagram) = Diagram::from_svg(&text) {
        return Ok((diagram.to_svg(ink(dark)), "diagram", shown));
    }
    if text.contains("<svg") {
        return Ok((svg::readable(&text, dark), "svg", shown));
    }
    Err(Outcome::err("this file is not a diagram or an SVG"))
}

fn mermaid_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut open = false;
    for line in text.lines() {
        if crate::logic::markdown::is_fence(line) {
            if open {
                blocks.push(std::mem::take(&mut current));
                open = false;
            } else {
                let info = line.trim_start().trim_start_matches('`').trim();
                let word = info.split_whitespace().next().unwrap_or("");
                open = word.eq_ignore_ascii_case("mermaid");
            }
            continue;
        }
        if open {
            current.push_str(line);
            current.push('\n');
        }
    }
    blocks
}

fn raster(svg: &str, width: Option<f32>) -> Result<(Vec<u8>, u32, u32), String> {
    let tree = svg::parse(svg).ok_or("the picture could not be drawn")?;
    let (w, h) = (tree.size().width().max(1.), tree.size().height().max(1.));
    let want = width.map(|px| px / w).unwrap_or(2.).max(0.05);
    let scale = svg::scale_for(w, h, want);
    let png = figure::png(svg, scale).ok_or("the picture could not be drawn")?;
    let (pw, ph) = png_size(&png);
    Ok((png, pw, ph))
}

fn png_size(png: &[u8]) -> (u32, u32) {
    if png.len() < 24 {
        return (0, 0);
    }
    let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap_or([0; 4]));
    (word(16), word(20))
}

fn build(from: &Value) -> Result<(Diagram, usize), String> {
    let mermaid_src = from.get("mermaid").and_then(|v| v.as_str());
    let template = from.get("template").and_then(|v| v.as_str());
    let shapes = from.get("shapes");
    let chosen = usize::from(mermaid_src.is_some()) + usize::from(template.is_some()) + usize::from(shapes.is_some());
    if chosen != 1 {
        return Err("`from` needs one of `mermaid`, `template` or `shapes`".into());
    }
    if let Some(source) = mermaid_src {
        let Some(converted) = mermaid::to_diagram(source) else {
            return Err(FLOW.into());
        };
        return Ok((converted.diagram, converted.skipped));
    }
    if let Some(name) = template {
        let made = library::templates();
        let at = TEMPLATES.iter().position(|key| *key == name).or_else(|| made.iter().position(|template| template.name == name));
        let Some(at) = at else {
            return Err(format!("no template named `{name}`"));
        };
        return Ok(((made[at].make)(), 0));
    }
    let shapes = shapes.and_then(|v| v.as_array()).ok_or("`shapes` must be a list")?;
    let links = from.get("links").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut diagram = Diagram::blank();
    let mut keys = HashMap::new();
    let mut made_shapes = Vec::new();
    let mut made_links = Vec::new();
    let mut unplaced = Vec::new();
    for shape in shapes {
        let (id, placed) = add_shape(&mut diagram, shape)?;
        if let Some(key) = shape.get("key").and_then(|v| v.as_str()) {
            keys.insert(key.to_string(), id);
        }
        if !placed {
            unplaced.push(id);
        }
        made_shapes.push(id);
        if let Some(holder) = shape.get("in") {
            let parent = resolve_id(holder, &keys, &diagram)?;
            if !diagram.set_parent(id, Some(parent)) {
                return Err("a shape cannot contain itself".into());
            }
        }
    }
    for link in &links {
        let id = add_link(&mut diagram, link, &keys)?;
        if let Some(key) = link.get("key").and_then(|v| v.as_str()) {
            keys.insert(key.to_string(), id);
        }
        made_links.push(id);
    }
    lay_out(&mut diagram, &unplaced);
    diagram.fit_sequence();
    let _ = (made_shapes, made_links);
    Ok((diagram, 0))
}

fn lay_out(diagram: &mut Diagram, unplaced: &[u32]) {
    if unplaced.is_empty() {
        return;
    }
    let lives: Vec<u32> = unplaced.iter().copied().filter(|id| diagram.shape(*id).is_some_and(|s| s.form == Form::Lifeline)).collect();
    let roots: Vec<u32> = unplaced
        .iter()
        .copied()
        .filter(|id| diagram.shape(*id).is_some_and(|s| s.parent.is_none() && s.form != Form::Lifeline))
        .collect();
    if !roots.is_empty() {
        let sizes: Vec<(f32, f32)> = roots.iter().map(|id| diagram.shape(*id).map(|s| (s.w, s.h)).unwrap_or((80., 40.))).collect();
        let mut edges = Vec::new();
        for link in &diagram.links {
            let (Some(a), Some(b)) = (link.from.shape(), link.to.shape()) else { continue };
            let (Some(i), Some(j)) = (roots.iter().position(|id| *id == a), roots.iter().position(|id| *id == b)) else { continue };
            if i != j {
                edges.push((i, j));
            }
        }
        let (at, _) = diagram::layered(&sizes, &edges);
        for (i, id) in roots.iter().enumerate() {
            if let Some(shape) = diagram.shape_mut(*id) {
                (shape.x, shape.y) = at[i];
            }
        }
    }
    let mut placed: HashMap<u32, ()> = roots.into_iter().map(|id| (id, ())).collect();
    for _ in 0..=unplaced.len() {
        let mut progressed = false;
        for id in unplaced {
            if placed.contains_key(id) || lives.contains(id) {
                continue;
            }
            let Some(parent) = diagram.shape(*id).and_then(|s| s.parent) else { continue };
            if !placed.contains_key(&parent) && !diagram.shape(parent).is_some_and(|s| !unplaced.contains(&s.id)) {
                continue;
            }
            let (px, py, n) = {
                let parent = diagram.shape(parent).unwrap();
                let n = diagram.children(parent.id).iter().filter(|child| placed.contains_key(child)).count();
                (parent.x, parent.y, n)
            };
            if let Some(shape) = diagram.shape_mut(*id) {
                shape.x = px + 24.;
                shape.y = py + 40. + n as f32 * (shape.h + 16.);
            }
            placed.insert(*id, ());
            progressed = true;
        }
        if !progressed {
            break;
        }
    }
    for (i, id) in lives.iter().enumerate() {
        if let Some(shape) = diagram.shape_mut(*id) {
            shape.x = 40. + i as f32 * 180.;
            shape.y = 40.;
        }
    }
}

fn add_shape(diagram: &mut Diagram, spec: &Value) -> Result<(u32, bool), String> {
    let form = form_of(spec.get("form").and_then(|v| v.as_str()).unwrap_or("rect"))?;
    let text = spec.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let w = spec.get("w").and_then(|v| v.as_f64()).map(|n| n as f32);
    let h = spec.get("h").and_then(|v| v.as_f64()).map(|n| n as f32);
    let (w, h) = size_of(form, &text, w, h);
    let placed = spec.get("x").is_some() || spec.get("y").is_some();
    let x = spec.get("x").and_then(|v| v.as_f64()).unwrap_or(0.) as f32;
    let y = spec.get("y").and_then(|v| v.as_f64()).unwrap_or(0.) as f32;
    let id = diagram.add_shape(form, x, y, w, h);
    let shape = diagram.shape_mut(id).unwrap();
    shape.text = text;
    if let Some(color) = spec.get("color").and_then(|v| v.as_u64()) {
        if color > 5 {
            return Err("`color` is a number from 0 to 5".into());
        }
        shape.color = color as u8;
    }
    if let Some(fill) = spec.get("fill").and_then(|v| v.as_bool()) {
        shape.fill = fill;
    }
    if let Some(dashed) = spec.get("dashed").and_then(|v| v.as_bool()) {
        shape.dashed = dashed;
    }
    Ok((id, placed))
}

fn size_of(form: Form, text: &str, w: Option<f32>, h: Option<f32>) -> (f32, f32) {
    let lines: Vec<String> = if text.is_empty() { Vec::new() } else { text.split('\n').map(str::to_string).collect() };
    let min = match form {
        Form::Dot => (20., 20.),
        Form::Final => (26., 26.),
        Form::Pin => (14., 14.),
        Form::Bar => (90., 8.),
        Form::Exit | Form::Terminate => (24., 24.),
        Form::Actor => (40., 80.),
        Form::Branch => (40., 40.),
        Form::Lifeline => (110., 200.),
        Form::Divider => (200., 10.),
        Form::Package => (220., 160.),
        Form::Frame | Form::Composite => (300., 200.),
        Form::Node => (300., 200.),
        Form::Lane => (200., 280.),
        Form::Row => (420., 140.),
        Form::Fragment => (280., 140.),
        _ => (120., 48.),
    };
    let (fw, fh) = diagram::fit(&lines, min);
    (w.unwrap_or(fw), h.unwrap_or(fh))
}

fn add_link(diagram: &mut Diagram, spec: &Value, keys: &HashMap<String, u32>) -> Result<u32, String> {
    let from = end_of(spec.get("from").ok_or("`from` is missing")?, keys, diagram)?;
    let to = end_of(spec.get("to").ok_or("`to` is missing")?, keys, diagram)?;
    let (mut start, mut end, mut dashed) = (Head::None, Head::Arrow, false);
    if let Some(name) = spec.get("relation").and_then(|v| v.as_str()) {
        let relation = relation_of(name)?;
        (start, end, dashed) = relation.look();
        let id = diagram.add_link(from, to, end, route_of(spec.get("route").and_then(|v| v.as_str()))?);
        let link = diagram.link_mut(id).unwrap();
        link.start = start;
        link.dashed = dashed;
        relation.apply(link);
        fill_link(link, spec);
        return Ok(id);
    }
    if let Some(heads) = spec.get("heads") {
        start = head_of(heads.get("start").and_then(|v| v.as_str()).unwrap_or("none"))?;
        end = head_of(heads.get("end").and_then(|v| v.as_str()).unwrap_or("arrow"))?;
        dashed = heads.get("dashed").and_then(|v| v.as_bool()).unwrap_or(false);
    }
    let id = diagram.add_link(from, to, end, route_of(spec.get("route").and_then(|v| v.as_str()))?);
    let link = diagram.link_mut(id).unwrap();
    link.start = start;
    link.dashed = dashed;
    fill_link(link, spec);
    Ok(id)
}

fn fill_link(link: &mut diagram::Link, spec: &Value) {
    if let Some(text) = spec.get("text").and_then(|v| v.as_str()) {
        link.text = text.to_string();
    }
    if let Some(text) = spec.get("start_text").and_then(|v| v.as_str()) {
        link.start_text = text.to_string();
    }
    if let Some(text) = spec.get("end_text").and_then(|v| v.as_str()) {
        link.end_text = text.to_string();
    }
    if let Some(dashed) = spec.get("dashed").and_then(|v| v.as_bool()) {
        link.dashed = dashed;
    }
}

fn end_of(value: &Value, keys: &HashMap<String, u32>, diagram: &Diagram) -> Result<End, String> {
    if let Some(text) = value.as_str() {
        return shape_end(resolve_key(text, keys)?, diagram);
    }
    if let Some(id) = value.as_u64() {
        return shape_end(id as u32, diagram);
    }
    let Some(obj) = value.as_object() else {
        return Err("`from` and `to` name a shape".into());
    };
    if let Some(mid) = obj.get("mid") {
        return Ok(End::Mid(resolve_id(mid, keys, diagram)?));
    }
    if let Some(pin) = obj.get("pin") {
        let id = resolve_id(pin, keys, diagram)?;
        let fx = obj.get("x").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        let fy = obj.get("y").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        return Ok(End::Pin(id, fx, fy));
    }
    if let Some(life) = obj.get("life") {
        return Ok(End::Life(resolve_id(life, keys, diagram)?));
    }
    if let (Some(x), Some(y)) = (obj.get("x").and_then(|v| v.as_f64()), obj.get("y").and_then(|v| v.as_f64())) {
        return Ok(End::Point(x as f32, y as f32));
    }
    if let Some(shape) = obj.get("shape") {
        return shape_end(resolve_id(shape, keys, diagram)?, diagram);
    }
    Err("`from` and `to` name a shape".into())
}

fn shape_end(id: u32, diagram: &Diagram) -> Result<End, String> {
    let shape = diagram.shape(id).ok_or_else(|| format!("no shape `{id}`"))?;
    Ok(if shape.form == Form::Lifeline { End::Life(id) } else { End::Shape(id) })
}

fn resolve_id(value: &Value, keys: &HashMap<String, u32>, diagram: &Diagram) -> Result<u32, String> {
    if let Some(text) = value.as_str() {
        return resolve_key(text, keys);
    }
    if let Some(id) = value.as_u64() {
        let id = id as u32;
        if diagram.shape(id).is_some() || diagram.link(id).is_some() {
            return Ok(id);
        }
        return Err(format!("no element `{id}`"));
    }
    Err("an element is named by its id or its key".into())
}

fn resolve_key(key: &str, keys: &HashMap<String, u32>) -> Result<u32, String> {
    keys.get(key).copied().ok_or_else(|| format!("no shape `{key}`"))
}

fn form_of(name: &str) -> Result<Form, String> {
    Form::ALL.iter().find(|(_, known)| *known == name).map(|(form, _)| *form).ok_or_else(|| format!("unknown form `{name}`"))
}

fn form_name(form: Form) -> &'static str {
    Form::ALL.iter().find(|(known, _)| *known == form).map(|(_, name)| *name).unwrap_or("rect")
}

fn head_of(name: &str) -> Result<Head, String> {
    Head::ALL.iter().find(|(_, known)| *known == name).map(|(head, _)| *head).ok_or_else(|| format!("unknown head `{name}`"))
}

fn head_name(head: Head) -> &'static str {
    Head::ALL.iter().find(|(known, _)| *known == head).map(|(_, name)| *name).unwrap_or("none")
}

fn route_of(name: Option<&str>) -> Result<Route, String> {
    let name = name.unwrap_or("straight");
    Route::ALL.iter().find(|(_, known)| *known == name).map(|(route, _)| *route).ok_or_else(|| format!("unknown route `{name}`"))
}

fn route_name(route: Route) -> &'static str {
    Route::ALL.iter().find(|(known, _)| *known == route).map(|(_, name)| *name).unwrap_or("straight")
}

fn relation_of(name: &str) -> Result<Relation, String> {
    Relation::ALL.into_iter().find(|relation| relation_key(*relation) == name).ok_or_else(|| format!("unknown relation `{name}`"))
}

fn apply(diagram: &mut Diagram, op: &Value, keys: &mut HashMap<String, u32>, shapes: &mut Vec<u32>, links: &mut Vec<u32>) -> Result<(), String> {
    let name = op.get("op").and_then(|v| v.as_str()).ok_or("`op` is missing")?;
    match name {
        "add_shape" => {
            let (id, _) = add_shape(diagram, op)?;
            if let Some(key) = op.get("key").and_then(|v| v.as_str()) {
                keys.insert(key.to_string(), id);
            }
            if let Some(holder) = op.get("in") {
                let parent = resolve_id(holder, keys, diagram)?;
                if !diagram.set_parent(id, Some(parent)) {
                    return Err("a shape cannot contain itself".into());
                }
            }
            shapes.push(id);
        }
        "add_link" => {
            let id = add_link(diagram, op, keys)?;
            if let Some(key) = op.get("key").and_then(|v| v.as_str()) {
                keys.insert(key.to_string(), id);
            }
            links.push(id);
        }
        "update" => update(diagram, op, keys)?,
        "remove" => {
            let mut ids = Vec::new();
            if let Some(id) = op.get("id") {
                ids.push(resolve_id(id, keys, diagram)?);
            }
            if let Some(list) = op.get("ids").and_then(|v| v.as_array()) {
                for id in list {
                    ids.push(resolve_id(id, keys, diagram)?);
                }
            }
            if ids.is_empty() {
                return Err("`id` is missing".into());
            }
            diagram.remove(&ids);
        }
        "move" => {
            let id = resolve_id(op.get("id").ok_or("`id` is missing")?, keys, diagram)?;
            let shape = diagram.shape_mut(id).ok_or("move a shape, not a link")?;
            shape.x = op.get("x").and_then(|v| v.as_f64()).ok_or("`x` is missing")? as f32;
            shape.y = op.get("y").and_then(|v| v.as_f64()).ok_or("`y` is missing")? as f32;
        }
        "relayout" => {
            let ids: Vec<u32> = diagram.shapes.iter().map(|s| s.id).collect();
            lay_out(diagram, &ids);
        }
        other => return Err(format!("unknown op `{other}`")),
    }
    Ok(())
}

fn update(diagram: &mut Diagram, op: &Value, keys: &HashMap<String, u32>) -> Result<(), String> {
    let id = resolve_id(op.get("id").ok_or("`id` is missing")?, keys, diagram)?;
    if diagram.shape(id).is_some() {
        {
            if let Some(name) = op.get("form").and_then(|v| v.as_str()) {
                let form = form_of(name)?;
                diagram.shape_mut(id).unwrap().form = form;
            }
            let shape = diagram.shape_mut(id).unwrap();
            if let Some(text) = op.get("text").and_then(|v| v.as_str()) {
                shape.text = text.to_string();
            }
            if let Some(x) = op.get("x").and_then(|v| v.as_f64()) {
                shape.x = x as f32;
            }
            if let Some(y) = op.get("y").and_then(|v| v.as_f64()) {
                shape.y = y as f32;
            }
            if let Some(w) = op.get("w").and_then(|v| v.as_f64()) {
                shape.w = w as f32;
            }
            if let Some(h) = op.get("h").and_then(|v| v.as_f64()) {
                shape.h = h as f32;
            }
            if let Some(color) = op.get("color").and_then(|v| v.as_u64()) {
                if color > 5 {
                    return Err("`color` is a number from 0 to 5".into());
                }
                shape.color = color as u8;
            }
            if let Some(fill) = op.get("fill").and_then(|v| v.as_bool()) {
                shape.fill = fill;
            }
            if let Some(dashed) = op.get("dashed").and_then(|v| v.as_bool()) {
                shape.dashed = dashed;
            }
        }
        if op.get("in").is_some() || op.get("parent").is_some() {
            let parent = match op.get("in").or_else(|| op.get("parent")) {
                Some(Value::Null) => None,
                Some(value) => Some(resolve_id(value, keys, diagram)?),
                None => None,
            };
            if !diagram.set_parent(id, parent) && parent.is_some() {
                return Err("a shape cannot contain itself".into());
            }
        }
        return Ok(());
    }
    if diagram.link(id).is_some() {
        if let Some(name) = op.get("relation").and_then(|v| v.as_str()) {
            let relation = relation_of(name)?;
            relation.apply(diagram.link_mut(id).unwrap());
        }
        if let Some(heads) = op.get("heads") {
            let start = head_of(heads.get("start").and_then(|v| v.as_str()).unwrap_or("none"))?;
            let end = head_of(heads.get("end").and_then(|v| v.as_str()).unwrap_or("arrow"))?;
            let link = diagram.link_mut(id).unwrap();
            (link.start, link.end) = (start, end);
        }
        if let Some(name) = op.get("route").and_then(|v| v.as_str()) {
            diagram.link_mut(id).unwrap().route = route_of(Some(name))?;
        }
        if op.get("from").is_some() {
            let from = end_of(op.get("from").unwrap(), keys, diagram)?;
            diagram.link_mut(id).unwrap().from = from;
        }
        if op.get("to").is_some() {
            let to = end_of(op.get("to").unwrap(), keys, diagram)?;
            diagram.link_mut(id).unwrap().to = to;
        }
        fill_link(diagram.link_mut(id).unwrap(), op);
        return Ok(());
    }
    Err(format!("no element `{id}`"))
}

fn described(diagram: &Diagram, path: &str, read_only: bool) -> Value {
    let shapes: Vec<Value> = diagram
        .shapes
        .iter()
        .map(|shape| {
            json!({
                "id": shape.id,
                "form": form_name(shape.form),
                "text": shape.text,
                "x": shape.x,
                "y": shape.y,
                "w": shape.w,
                "h": shape.h,
                "color": shape.color,
                "fill": shape.fill,
                "dashed": shape.dashed,
                "parent": shape.parent,
            })
        })
        .collect();
    let links: Vec<Value> = diagram
        .links
        .iter()
        .map(|link| {
            json!({
                "id": link.id,
                "from": end_json(link.from),
                "to": end_json(link.to),
                "start": head_name(link.start),
                "end": head_name(link.end),
                "route": route_name(link.route),
                "text": link.text,
                "start_text": link.start_text,
                "end_text": link.end_text,
                "dashed": link.dashed,
                "relation": Relation::of(link).map(relation_key),
            })
        })
        .collect();
    let bounds = diagram.bounds().map(|(x, y, x1, y1)| json!({"x": x, "y": y, "w": x1 - x, "h": y1 - y}));
    json!({
        "path": path,
        "read_only": read_only,
        "shapes": shapes,
        "links": links,
        "bounds": bounds,
    })
}

fn end_json(end: End) -> Value {
    match end {
        End::Shape(id) => json!({"shape": id}),
        End::Pin(id, x, y) => json!({"pin": id, "x": x, "y": y}),
        End::Point(x, y) => json!({"x": x, "y": y}),
        End::Mid(id) => json!({"mid": id}),
        End::Life(id) => json!({"life": id}),
    }
}

fn ids_of(keys: &HashMap<String, u32>) -> Value {
    keys.iter().map(|(key, id)| (key.clone(), json!(id))).collect()
}

fn open_file(vault: &Vault, args: &Value) -> Result<std::path::PathBuf, Outcome> {
    let name = arg_str(args, "path").or_else(|_| arg_str(args, "note")).map_err(Outcome::err)?;
    vault.find_file(name).map_err(Outcome::err)
}

fn write_diagram(vault: &mut Vault, name: &str, folder: Option<&str>, diagram: &Diagram, skipped: usize) -> Result<Outcome, Outcome> {
    let Some(clean) = files::clean_name(name) else {
        return Err(Outcome::err("invalid name"));
    };
    let dir = match folder {
        None => vault.root.clone(),
        Some(folder) => vault.inside(folder, false).map_err(Outcome::err)?,
    };
    fs::create_dir_all(&dir).map_err(disk).map_err(Outcome::err)?;
    if let Some(taken) = vault::name_taken(&dir, &clean, "svg") {
        return Err(Outcome::err(format!("a file already has the name `{clean}` ({})", vault::rel(&vault.root, &taken))));
    }
    let path = dir.join(format!("{clean}.svg"));
    files::write(&path, &diagram.to_svg(diagram::COLORS[0])).map_err(disk).map_err(Outcome::err)?;
    vault.rescan();
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Outcome::ok(json!({
        "path": vault::rel(&vault.root, &path),
        "embed": format!("[[{file}]]"),
        "skipped": skipped,
    })))
}

fn drawio_error(content: &str) -> String {
    if roxmltree::Document::parse(content).is_ok() && !content.contains("mxCell") {
        "this draw.io file is compressed: untick File > Properties > Compressed, then save it again".into()
    } else {
        "not a draw.io file".into()
    }
}
