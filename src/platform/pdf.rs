//! A PDF made of pages drawn as SVG: each page goes through svg2pdf, which keeps the text as
//! text (fonts embedded, reduced to the letters used), and its web links become clickable.

use std::collections::HashMap;

use pdf_writer::types::{ActionType, AnnotationType};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use svg2pdf::usvg::Tree;

/// A page: its drawing, and its links (x, y, width, height in points from the top left, address).
pub type Sheet = (Tree, Vec<([f32; 4], String)>);

/// The PDF of `pages`, each `w`×`h` points, titled `title`; and the pages (from 1) whose text had
/// to be drawn as shapes, a font of theirs not fitting in a PDF: those cannot be searched.
pub fn document(pages: &[Sheet], (w, h): (f32, f32), title: &str) -> Result<(Vec<u8>, Vec<usize>), String> {
    let mut alloc = Ref::new(1);
    let (catalog, tree, info) = (alloc.bump(), alloc.bump(), alloc.bump());
    let mut pdf = Pdf::new();
    let mut kids = Vec::new();
    let mut chunks = Vec::new();
    let mut outlined = Vec::new();
    for (n, (drawing, links)) in pages.iter().enumerate() {
        let options = svg2pdf::ConversionOptions::default();
        let (chunk, id) = match svg2pdf::to_chunk(drawing, options) {
            Ok(made) => made,
            Err(_) => {
                outlined.push(n + 1);
                svg2pdf::to_chunk(drawing, svg2pdf::ConversionOptions { embed_text: false, ..options }).map_err(|e| e.to_string())?
            }
        };
        let mut map = HashMap::new();
        let chunk = chunk.renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));
        let (page, content) = (alloc.bump(), alloc.bump());
        let annotations: Vec<Ref> = links.iter().map(|_| alloc.bump()).collect();
        let mut p = pdf.page(page);
        p.media_box(Rect::new(0., 0., w, h)).parent(tree).contents(content);
        p.resources().x_objects().pair(Name(b"S"), map[&id]);
        if !annotations.is_empty() {
            p.annotations(annotations.iter().copied());
        }
        p.finish();
        // The drawing is a unit square: stretched over the page.
        let mut stream = Content::new();
        stream.transform([w, 0., 0., h, 0., 0.]).x_object(Name(b"S"));
        pdf.stream(content, &stream.finish());
        for (r, ([x, y, lw, lh], url)) in annotations.iter().zip(links) {
            let mut a = pdf.annotation(*r);
            a.subtype(AnnotationType::Link).rect(Rect::new(*x, h - y - lh, x + lw, h - y)).border(0., 0., 0., None);
            a.action().action_type(ActionType::Uri).uri(Str(url.as_bytes()));
        }
        kids.push(page);
        chunks.push(chunk);
    }
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).count(kids.len() as i32).kids(kids);
    pdf.document_info(info).title(TextStr(title)).creator(TextStr("Bref"));
    chunks.iter().for_each(|c| pdf.extend(c));
    Ok((pdf.finish(), outlined))
}

/// Whether `drawing` can go in a PDF: a picture whose data is damaged cannot.
pub fn draws(drawing: &Tree) -> bool {
    svg2pdf::to_chunk(drawing, svg2pdf::ConversionOptions::default()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_and_links_are_written() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="10" height="10"/></svg>"#;
        let tree = || Tree::from_str(svg, &Default::default()).unwrap();
        let (pdf, outlined) = document(&[(tree(), vec![([1., 2., 3., 4.], "https://ex.fr".into())]), (tree(), vec![])], (100., 50.), "Titre").unwrap();
        assert!(outlined.is_empty());
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-"));
        assert!(text.contains("/Count 2"));
        assert_eq!(text.matches("/Type /Page").count() - text.matches("/Type /Pages").count(), 2);
        assert!(text.contains("(https://ex.fr)"));
    }

    /// A colour emoji font cannot be subset: its page is drawn as shapes, the PDF is still made.
    #[test]
    fn a_font_that_cannot_be_embedded_does_not_stop_the_pdf() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><text x="5" y="30" font-size="12">a 🎉</text></svg>"#;
        let mut options = svg2pdf::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let tree = Tree::from_str(svg, &options).unwrap();
        assert!(document(&[(tree, vec![])], (100., 50.), "x").is_ok());
    }
}
