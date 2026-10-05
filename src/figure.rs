//! Figures : diagrammes Mermaid et formules LaTeX, rendus en SVG. Fonctions
//! pures, sans dépendance à l'UI.

use std::panic::catch_unwind;

use latex_rust::{Color, Dim, MathFont, SvgOptions, latex_to_svg};
use mermaid_rs_renderer::{RenderOptions, Theme, render_with_options};

/// Couleurs du thème, en `#rrggbb`, et police du texte.
pub struct Look {
    pub dark: bool,
    pub bg: String,
    pub fill: String,
    pub text: String,
    pub line: String,
    pub font: String,
}

/// Un SVG et sa taille à l'écran, en pixels.
pub type Figure = (String, f32, f32);

/// Le diagramme Mermaid de `source` ; `None` s'il est invalide.
pub fn mermaid(source: &str, look: &Look) -> Option<Figure> {
    let base = if look.dark { Theme::dark() } else { Theme::modern() };
    let theme = Theme {
        font_family: look.font.clone(),
        background: look.bg.clone(),
        primary_color: look.fill.clone(),
        primary_text_color: look.text.clone(),
        text_color: look.text.clone(),
        primary_border_color: look.line.clone(),
        line_color: look.line.clone(),
        edge_label_background: look.bg.clone(),
        cluster_background: look.fill.clone(),
        cluster_border: look.line.clone(),
        sequence_actor_fill: look.fill.clone(),
        sequence_actor_border: look.line.clone(),
        sequence_actor_line: look.line.clone(),
        ..base
    };
    let options = RenderOptions { theme, ..Default::default() };
    // Le moteur vient d'ailleurs : un diagramme qui le ferait paniquer ne doit pas
    // emporter l'app, donc la note en cours de frappe.
    let svg = catch_unwind(|| render_with_options(source, options).ok()).ok()??;
    sharpen(&svg)
}

/// La formule LaTeX `source`, en couleur `rgb` et corps `size` (pixels) ;
/// `display` pour une formule hors texte. `None` si elle est invalide.
pub fn latex(source: &str, display: bool, rgb: [u8; 3], size: f32) -> Option<Figure> {
    let svg = catch_unwind(|| {
        let options = SvgOptions {
            // Le corps est en points : 3/4 de pixel.
            font_size_pt: Dim::parse(&format!("{:.2}", size * 0.75)),
            color: Color::rgb(rgb[0], rgb[1], rgb[2]),
            display,
        };
        latex_to_svg(source, &MathFont::stix_two_math().ok()?, &options).ok()
    })
    .ok()??;
    sharpen(&svg)
}

/// Le SVG prêt pour gpui 0.2, et sa taille d'origine en pixels. Un SVG donné en
/// mémoire y est rendu à l'échelle 1, donc flou, et avec le rouge et le bleu
/// échangés (seul le chargement d'un fichier les remet en ordre) : on l'agrandit
/// deux fois et on échange les deux couleurs d'avance.
pub fn sharpen(svg: &str) -> Option<Figure> {
    let body = &svg[svg.find("<svg")?..];
    let head = &body[..body.find('>')?];
    let dim = |name: &str| {
        let value = head.split(&format!(" {name}=\"")).nth(1)?.split('"').next()?;
        let (value, unit) = match value.strip_suffix("pt") {
            Some(points) => (points, 4. / 3.),
            None => (value.trim_end_matches("px"), 1.),
        };
        Some(value.parse::<f32>().ok()? * unit)
    };
    let (w, h) = (dim("width")?, dim("height")?);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {w} {h}"><filter id="bgr" x="0" y="0" width="100%" height="100%" color-interpolation-filters="sRGB"><feColorMatrix values="0 0 1 0 0 0 1 0 0 0 1 0 0 0 0 0 0 0 1 0"/></filter><g filter="url(#bgr)">{body}</g></svg>"#,
        w * 2.,
        h * 2.,
    );
    Some((svg, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_figures() {
        let look = Look {
            dark: true,
            bg: "#101010".into(),
            fill: "#202020".into(),
            text: "#eeeeee".into(),
            line: "#808080".into(),
            font: "sans-serif".into(),
        };
        let (svg, w, h) = mermaid("flowchart LR\n    A --> B", &look).unwrap();
        assert!(svg.contains("#101010") && w > h && h > 10.);
        assert!(mermaid("ceci n'est pas un diagramme", &look).is_none() && mermaid("", &look).is_none());

        let (svg, w, h) = latex(r"\frac{1}{2}", true, [255, 0, 0], 16.).unwrap();
        assert!(svg.starts_with("<svg") && svg.contains("<path") && w > 4. && h > 16.);
        assert!(latex(r"\frac{1}{", false, [0, 0, 0], 16.).is_none());
    }
}
