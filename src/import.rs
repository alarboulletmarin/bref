//! Import de schémas faits ailleurs : fichiers Excalidraw (JSON) et draw.io
//! (XML). Fonctions pures. On reprend ce que Bref sait dessiner : formes,
//! textes, flèches et ce à quoi elles sont accrochées ; le reste est laissé.

use std::collections::HashMap;

use serde_json::Value;

use crate::{
    diagram::{COLORS, Diagram, End, Form, Head, RULE, Route},
    tr,
};

/// Rang de la couleur de Bref la plus proche de `#rrggbb` ; le noir, le blanc
/// et les gris donnent le neutre.
fn color(hex: &str) -> u8 {
    let Some(rgb) = hex.strip_prefix('#').filter(|h| h.len() == 6).and_then(|h| u32::from_str_radix(h, 16).ok()) else {
        return 0;
    };
    let parts = |c: u32| [(c >> 16) as i32 & 255, (c >> 8) as i32 & 255, c as i32 & 255];
    let [r, g, b] = parts(rgb);
    if r.max(g).max(b) - r.min(g).min(b) < 40 {
        return 0;
    }
    let far = |c: &u32| parts(*c).iter().zip([r, g, b]).map(|(a, b)| (a - b).pow(2)).sum::<i32>();
    (1..COLORS.len()).min_by_key(|i| far(&COLORS[*i])).unwrap_or(0) as u8
}

/// Un fichier `.excalidraw`.
pub fn excalidraw(json: &str) -> Result<Diagram, String> {
    let unreadable = || tr("not an Excalidraw file", "ce n'est pas un fichier Excalidraw").to_string();
    let file: Value = serde_json::from_str(json).map_err(|_| unreadable())?;
    let elements: Vec<&Value> = file["elements"].as_array().ok_or_else(unreadable)?.iter().filter(|e| e["isDeleted"] != true).collect();
    let n = |e: &Value, key: &str| e[key].as_f64().unwrap_or(0.) as f32;
    let mut diagram = Diagram::default();
    // Identifiant d'Excalidraw → celui de la forme ou de la flèche créée.
    let mut made: HashMap<&str, u32> = HashMap::new();
    for e in &elements {
        let form = match e["type"].as_str() {
            Some("rectangle") if e["roundness"].is_null() => Form::Rect,
            Some("rectangle") => Form::Round,
            Some("ellipse") => Form::Ellipse,
            Some("diamond") => Form::Diamond,
            Some("text") if e["containerId"].is_null() => Form::Text,
            _ => continue,
        };
        let id = diagram.add_shape(form, n(e, "x"), n(e, "y"), n(e, "width"), n(e, "height"));
        let shape = diagram.shape_mut(id).unwrap();
        shape.color = color(e["strokeColor"].as_str().unwrap_or(""));
        shape.fill = e["backgroundColor"].as_str().is_some_and(|c| c != "transparent");
        shape.dashed = e["strokeStyle"].as_str().is_some_and(|s| s != "solid");
        shape.text = e["text"].as_str().unwrap_or("").to_string();
        made.insert(e["id"].as_str().unwrap_or(""), id);
    }
    for e in &elements {
        if !matches!(e["type"].as_str(), Some("arrow" | "line")) {
            continue;
        }
        let point = |i: usize| e["points"][i].as_array().map(|p| (p[0].as_f64().unwrap_or(0.) as f32, p[1].as_f64().unwrap_or(0.) as f32));
        let last = e["points"].as_array().map_or(0, |p| p.len().saturating_sub(1));
        let end = |binding: &str, i: usize| match e[binding]["elementId"].as_str().and_then(|id| made.get(id)) {
            Some(id) => Some(End::Shape(*id)),
            None => point(i).map(|(x, y)| End::Point(n(e, "x") + x, n(e, "y") + y)),
        };
        let head = |key: &str| match e[key].as_str() {
            None => Head::None,
            Some("triangle" | "triangle_outline") => Head::Triangle,
            Some("diamond") => Head::DiamondFull,
            Some("diamond_outline") => Head::Diamond,
            Some(_) => Head::Arrow,
        };
        let (Some(from), Some(to)) = (end("startBinding", 0), end("endBinding", last)) else { continue };
        let bent = e["points"].as_array().is_some_and(|p| p.len() > 2);
        let route = match (e["elbowed"] == true, bent && !e["roundness"].is_null()) {
            (true, _) => Route::Elbow,
            (_, true) => Route::Curve,
            _ => Route::Straight,
        };
        let id = diagram.add_link(from, to, head("endArrowhead"), route);
        let link = diagram.link_mut(id).unwrap();
        link.start = head("startArrowhead");
        link.color = color(e["strokeColor"].as_str().unwrap_or(""));
        link.dashed = e["strokeStyle"].as_str().is_some_and(|s| s != "solid");
        made.insert(e["id"].as_str().unwrap_or(""), id);
    }
    // Un texte tenu par une forme ou une flèche devient son texte.
    for e in elements.iter().filter(|e| e["type"] == "text") {
        let Some(id) = e["containerId"].as_str().and_then(|id| made.get(id)).copied() else { continue };
        let text = e["text"].as_str().unwrap_or("").to_string();
        if let Some(shape) = diagram.shape_mut(id) {
            shape.text = text;
        } else if let Some(link) = diagram.link_mut(id) {
            link.text = text;
        }
    }
    Ok(diagram)
}

/// Le texte d'un libellé draw.io, qui peut être du HTML.
fn plain(label: &str) -> String {
    let mut out = String::new();
    let mut rest = label;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else { break };
        let tag = rest[open + 1..open + close].trim_start_matches('/').to_lowercase();
        let name = tag.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("");
        match name {
            "hr" => out.push_str(&format!("\n{RULE}\n")),
            "br" | "div" | "p" if !out.ends_with('\n') && !out.is_empty() => out.push('\n'),
            _ => {}
        }
        rest = &rest[open + close + 1..];
    }
    if !rest.contains('<') {
        out.push_str(rest);
    }
    let out = out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&");
    out.lines().map(str::trim).collect::<Vec<_>>().join("\n").trim().to_string()
}

fn geometry<'a, 'i>(cell: &roxmltree::Node<'a, 'i>) -> Option<roxmltree::Node<'a, 'i>> {
    cell.children().find(|n| n.has_tag_name("mxGeometry"))
}

/// Un fichier `.drawio`, enregistré sans compression.
// ponytail: draw.io sait aussi enregistrer le schéma compressé (deflate puis
// base64) ; on demande alors de le réenregistrer sans compression. Ajouter la
// décompression (`flate2`, `base64`) si ces fichiers sont courants.
pub fn drawio(xml: &str) -> Result<Diagram, String> {
    let file = roxmltree::Document::parse(xml).map_err(|_| tr("not a draw.io file", "ce n'est pas un fichier draw.io").to_string())?;
    let cells: Vec<roxmltree::Node> = file.descendants().filter(|n| n.has_tag_name("mxCell")).collect();
    if cells.is_empty() {
        return Err(tr(
            "this draw.io file is compressed: untick File › Properties › Compressed, then save it again",
            "ce fichier draw.io est compressé : décoche Fichier › Propriétés › Compressé, puis réenregistre-le",
        )
        .to_string());
    }
    let style = |cell: &roxmltree::Node, key: &str| -> Option<String> {
        cell.attribute("style")?.split(';').find_map(|part| match part.split_once('=') {
            Some((k, v)) if k == key => Some(v.to_string()),
            None if part == key => Some(String::new()),
            _ => None,
        })
    };
    // Le libellé et l'identifiant sont parfois portés par un `<object>` autour de la cellule.
    let around = |cell: &roxmltree::Node, name: &str| {
        cell.attribute(name).or_else(|| cell.parent().filter(|p| !p.has_tag_name("root")).and_then(|p| p.attribute(name))).map(str::to_string)
    };
    let id_of = |cell: &roxmltree::Node| around(cell, "id").unwrap_or_default();
    let label = |cell: &roxmltree::Node| plain(&around(cell, "value").or_else(|| around(cell, "label")).unwrap_or_default());
    let n = |node: &roxmltree::Node, key: &str| node.attribute(key).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.);
    let by_id: HashMap<String, &roxmltree::Node> = cells.iter().map(|c| (id_of(c), c)).collect();
    // Les cellules d'un conteneur sont placées par rapport à lui.
    let origin = |cell: &roxmltree::Node| {
        let (mut x, mut y, mut parent) = (0., 0., cell.attribute("parent"));
        while let Some(up) = parent.and_then(|id| by_id.get(id)) {
            if let Some(g) = geometry(up) {
                (x, y) = (x + n(&g, "x"), y + n(&g, "y"));
            }
            parent = up.attribute("parent");
        }
        (x, y)
    };
    let boxed = |cell: &roxmltree::Node| style(cell, "swimlane").is_some();
    let mut diagram = Diagram::default();
    let mut made: HashMap<String, u32> = HashMap::new();
    for cell in cells.iter().filter(|c| c.attribute("vertex") == Some("1")) {
        // Une ligne d'une classe UML : elle s'ajoute au texte de sa boîte.
        let holder = cell.attribute("parent").and_then(|id| by_id.get(id)).filter(|p| boxed(p));
        if let Some(id) = holder.and_then(|p| made.get(&id_of(p))) {
            let line = if style(cell, "line").is_some() { RULE.to_string() } else { label(cell) };
            let shape = diagram.shape_mut(*id).unwrap();
            let rule = if shape.text.contains(RULE) || line == RULE { String::new() } else { format!("{RULE}\n") };
            shape.text = format!("{}\n{rule}{line}", shape.text);
            made.insert(id_of(cell), *id);
            continue;
        }
        let Some(g) = geometry(cell) else { continue };
        let shape_name = style(cell, "shape").unwrap_or_default();
        let form = if style(cell, "ellipse").is_some() {
            Form::Ellipse
        } else if style(cell, "rhombus").is_some() {
            Form::Diamond
        } else if style(cell, "text").is_some() {
            Form::Text
        } else if shape_name.starts_with("cylinder") {
            Form::Cylinder
        } else if shape_name == "umlActor" {
            Form::Actor
        } else if shape_name == "note" {
            Form::Note
        } else if style(cell, "rounded").as_deref() == Some("1") {
            Form::Round
        } else {
            Form::Rect
        };
        let (ox, oy) = origin(cell);
        let id = diagram.add_shape(form, ox + n(&g, "x"), oy + n(&g, "y"), n(&g, "width"), n(&g, "height"));
        let shape = diagram.shape_mut(id).unwrap();
        shape.text = label(cell);
        shape.color = color(&style(cell, "strokeColor").unwrap_or_default());
        shape.fill = style(cell, "fillColor").is_some_and(|c| c.starts_with('#') && !c.eq_ignore_ascii_case("#ffffff"));
        shape.dashed = style(cell, "dashed").as_deref() == Some("1");
        made.insert(id_of(cell), id);
    }
    for cell in cells.iter().filter(|c| c.attribute("edge") == Some("1")) {
        let (ox, oy) = origin(cell);
        let end = |attached: &str, loose: &str| match cell.attribute(attached).and_then(|id| made.get(id)) {
            Some(id) => Some(End::Shape(*id)),
            None => geometry(cell)?.children().find(|p| p.attribute("as") == Some(loose)).map(|p| End::Point(ox + n(&p, "x"), oy + n(&p, "y"))),
        };
        // Sans indication, une flèche de draw.io porte une pointe à son arrivée.
        let head = |key: &str, fill: &str, default: &str| {
            let hollow = style(cell, fill).as_deref() == Some("0");
            match style(cell, key).as_deref().unwrap_or(default) {
                "none" => Head::None,
                "block" | "blockThin" => Head::Triangle,
                d if d.starts_with("diamond") && hollow => Head::Diamond,
                d if d.starts_with("diamond") => Head::DiamondFull,
                _ => Head::Arrow,
            }
        };
        let (Some(from), Some(to)) = (end("source", "sourcePoint"), end("target", "targetPoint")) else { continue };
        let route = match style(cell, "edgeStyle") {
            _ if style(cell, "curved").as_deref() == Some("1") => Route::Curve,
            Some(_) => Route::Elbow,
            None => Route::Straight,
        };
        let id = diagram.add_link(from, to, head("endArrow", "endFill", "classic"), route);
        let link = diagram.link_mut(id).unwrap();
        link.start = head("startArrow", "startFill", "none");
        link.text = label(cell);
        link.color = color(&style(cell, "strokeColor").unwrap_or_default());
        link.dashed = style(cell, "dashed").as_deref() == Some("1");
        made.insert(id_of(cell), id);
    }
    // Le libellé d'une flèche est parfois une cellule à part, tenue par elle.
    for cell in cells.iter().filter(|c| style(c, "edgeLabel").is_some()) {
        if let Some(link) = cell.attribute("parent").and_then(|id| made.get(id)).and_then(|id| diagram.link_mut(*id)) {
            link.text = label(cell);
        }
    }
    Ok(diagram)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_excalidraw() {
        let d = excalidraw(
            r##"{"type":"excalidraw","elements":[
            {"id":"a","type":"rectangle","x":10,"y":20,"width":100,"height":50,"strokeColor":"#e03131","backgroundColor":"#ffc9c9","strokeStyle":"dashed","roundness":{"type":3}},
            {"id":"b","type":"ellipse","x":300,"y":20,"width":80,"height":80,"strokeColor":"#1e1e1e","backgroundColor":"transparent","strokeStyle":"solid","roundness":null},
            {"id":"t","type":"text","x":0,"y":0,"width":10,"height":10,"text":"Client","containerId":"a"},
            {"id":"gone","type":"diamond","x":0,"y":0,"width":10,"height":10,"isDeleted":true},
            {"id":"l","type":"arrow","x":110,"y":45,"points":[[0,0],[190,15]],"startBinding":{"elementId":"a"},"endBinding":null,"endArrowhead":"triangle","startArrowhead":null,"strokeColor":"#1971c2","strokeStyle":"solid"},
            {"id":"lt","type":"text","x":0,"y":0,"width":1,"height":1,"text":"appelle","containerId":"l"}
            ]}"##,
        )
        .unwrap();
        let a = &d.shapes[0];
        assert_eq!((d.shapes.len(), a.form, a.text.as_str(), a.color, a.fill, a.dashed), (2, Form::Round, "Client", 1, true, true));
        assert_eq!((d.shapes[1].form, d.shapes[1].color), (Form::Ellipse, 0));
        let l = &d.links[0];
        assert_eq!((l.from, l.to, l.end, l.color, l.text.as_str()), (End::Shape(a.id), End::Point(300., 60.), Head::Triangle, 4, "appelle"));
        assert!(excalidraw("{}").is_err());
    }

    #[test]
    fn imports_drawio() {
        let d = drawio(
            r##"<mxfile><diagram><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="c" value="Compte" style="swimlane;fontStyle=1;" vertex="1" parent="1"><mxGeometry x="40" y="40" width="160" height="90" as="geometry"/></mxCell>
            <mxCell id="f" value="+ solde: f32" style="text;align=left;" vertex="1" parent="c"><mxGeometry y="26" width="160" height="26" as="geometry"/></mxCell>
            <mxCell id="s" value="" style="line;strokeWidth=1;" vertex="1" parent="c"><mxGeometry y="52" width="160" height="8" as="geometry"/></mxCell>
            <mxCell id="m" value="+ cr&amp;eacute;diter()" style="text;" vertex="1" parent="c"><mxGeometry y="60" width="160" height="26" as="geometry"/></mxCell>
            <object label="&lt;b&gt;Base&lt;/b&gt;&lt;br&gt;clients" id="db"><mxCell style="shape=cylinder3;whiteSpace=wrap;html=1;strokeColor=#82b366;fillColor=#d5e8d4;" vertex="1" parent="1"><mxGeometry x="300" y="40" width="60" height="80" as="geometry"/></mxCell></object>
            <mxCell id="e" value="lit" style="endArrow=block;endFill=0;dashed=1;" edge="1" source="f" target="db" parent="1"><mxGeometry relative="1" as="geometry"/></mxCell>
            <mxCell id="p" style="endArrow=none;" edge="1" target="c" parent="1"><mxGeometry relative="1" as="geometry"><mxPoint x="10" y="200" as="sourcePoint"/></mxGeometry></mxCell>
            </root></mxGraphModel></diagram></mxfile>"##,
        )
        .unwrap();
        assert_eq!(d.shapes.len(), 2);
        assert_eq!((d.shapes[0].form, d.shapes[0].text.as_str()), (Form::Rect, "Compte\n---\n+ solde: f32\n---\n+ cr&eacute;diter()"));
        let db = &d.shapes[1];
        assert_eq!((db.form, db.text.as_str(), db.color, db.fill, db.x), (Form::Cylinder, "Base\nclients", 3, true, 300.));
        // Une flèche tirée d'une ligne de la classe part de la classe.
        let e = &d.links[0];
        assert_eq!((e.from, e.to, e.end, e.dashed, e.text.as_str()), (End::Shape(d.shapes[0].id), End::Shape(db.id), Head::Triangle, true, "lit"));
        assert_eq!((d.links[1].from, d.links[1].end), (End::Point(10., 200.), Head::None));
        assert!(drawio("<mxfile><diagram>dVHBcoMg</diagram></mxfile>").unwrap_err().contains("compress"));
        assert!(drawio("pas du xml").is_err());
    }
}
