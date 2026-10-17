//! Figures: Mermaid diagrams and LaTeX formulas, rendered as SVG. Pure
//! functions, with no dependency on the UI.

use std::panic::catch_unwind;

use latex_rust::{Color, Dim, MathFont, SvgOptions, latex_to_svg};
use mermaid_rs_renderer::{RenderOptions, Theme, render_with_options};

/// Theme colours, as `#rrggbb`, and text font.
#[derive(Clone, Hash, PartialEq)]
pub struct Look {
    pub dark: bool,
    pub bg: String,
    pub fill: String,
    pub text: String,
    pub line: String,
    pub font: String,
}

impl Look {
    pub fn of(t: &crate::ui::theme::Theme) -> Self {
        let hex = |c: gpui::Hsla| {
            let c = gpui::Rgba::from(c);
            let [r, g, b] = [c.r, c.g, c.b].map(|v| (v * 255.) as u8);
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        Look {
            dark: t.bg.l < 0.5,
            bg: hex(t.bg),
            fill: hex(t.code_bg),
            text: hex(t.text),
            line: hex(t.dim),
            font: crate::ui::assets::sans().to_string(),
        }
    }
}

/// An SVG and its size on screen, in pixels.
pub type Figure = (String, f32, f32);

/// The SVG of the Mermaid diagram in `source`; `None` if it is invalid.
pub fn mermaid(source: &str, look: &Look) -> Option<String> {
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
    // The engine comes from elsewhere: a diagram that made it panic must not
    // take down the app, and so the note being typed.
    catch_unwind(|| render_with_options(source, options).ok()).ok()?
}

/// The LaTeX formula `source`, in colour `rgb` and size `size` (pixels);
/// `display` for a display formula. `None` if it is invalid.
pub fn latex(source: &str, display: bool, rgb: [u8; 3], size: f32) -> Option<Figure> {
    let svg = catch_unwind(|| {
        let options = SvgOptions {
            // The size is in points: 3/4 of a pixel.
            font_size_pt: Dim::parse(&format!("{:.2}", size * 0.75)),
            color: Color::rgb(rgb[0], rgb[1], rgb[2]),
            display,
        };
        latex_to_svg(source, &MathFont::stix_two_math().ok()?, &options).ok()
    })
    .ok()??;
    sharpen(&svg)
}

/// The PNG image of an SVG, `scale` times larger than life, on a white background.
pub fn png(svg: &str, scale: f32) -> Option<Vec<u8>> {
    use resvg::tiny_skia;
    let tree = crate::ui::svg::parse(svg)?;
    let size = tree.size();
    let mut pixmap = tiny_skia::Pixmap::new((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32)?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// The SVG ready for gpui 0.2, and its original size in pixels. An SVG given in
/// memory is rendered there at scale 1, hence blurry, and with red and blue
/// swapped (only loading a file puts them back in order): we enlarge it
/// twice and swap the two colours in advance.
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
        let svg = mermaid("flowchart LR\n    A --> B", &look).unwrap();
        let size = crate::ui::svg::parse(&svg).unwrap().size();
        assert!(svg.contains("#101010") && size.width() > size.height() && size.height() > 10.);
        assert!(mermaid("ceci n'est pas un diagramme", &look).is_none() && mermaid("", &look).is_none());

        let (svg, w, h) = latex(r"\frac{1}{2}", true, [255, 0, 0], 16.).unwrap();
        assert!(svg.starts_with("<svg") && svg.contains("<path") && w > 4. && h > 16.);
        assert!(latex(r"\frac{1}{", false, [0, 0, 0], 16.).is_none());
    }
}

