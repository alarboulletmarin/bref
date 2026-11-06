//! Zooming a view around a point: the same arithmetic for the picture viewer, the canvas and the graph.

/// The smallest and the largest zoom a surface allows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min: f32,
    pub max: f32,
}

impl Limits {
    pub const fn new(min: f32, max: f32) -> Self {
        Limits { min, max }
    }

    pub fn clamp(self, zoom: f32) -> f32 {
        zoom.clamp(self.min, self.max)
    }
}

/// The zoom a wheel turn of `dy` pixels asks for, from `zoom`: exponential, so that opposite turns cancel.
pub fn wheel(zoom: f32, dy: f32) -> f32 {
    zoom * (dy * 0.004).exp()
}

/// Zoom to `target` (kept within `limits`) so that the point `around` (pixels from the corner of the view)
/// stays where it is on screen. `pan` is where the origin of the content is drawn; returns the new
/// `(zoom, pan)`.
pub fn zoom_at(zoom: f32, pan: (f32, f32), target: f32, limits: Limits, (x, y): (f32, f32)) -> (f32, (f32, f32)) {
    let target = limits.clamp(target);
    let k = target / zoom;
    (target, (x - (x - pan.0) * k, y - (y - pan.1) * k))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits::new(0.2, 4.);

    #[test]
    fn the_point_under_the_pointer_stays_put() {
        let (zoom, pan) = (1.5, (30., -12.));
        for target in [0.2, 0.7, 1.5, 3., 4.] {
            let (z, p) = zoom_at(zoom, pan, target, LIMITS, (200., 100.));
            // The content point under (200, 100) before and after.
            let before = ((200. - pan.0) / zoom, (100. - pan.1) / zoom);
            let after = ((200. - p.0) / z, (100. - p.1) / z);
            assert!((before.0 - after.0).abs() < 1e-3 && (before.1 - after.1).abs() < 1e-3, "{target}: {before:?} {after:?}");
        }
    }

    #[test]
    fn the_limits_hold_and_a_wheel_turn_undoes_its_opposite() {
        assert_eq!(zoom_at(1., (0., 0.), 100., LIMITS, (0., 0.)).0, 4.);
        assert_eq!(zoom_at(1., (0., 0.), 0., LIMITS, (0., 0.)).0, 0.2);
        assert!((wheel(wheel(1., 40.), -40.) - 1.).abs() < 1e-5);
    }
}
