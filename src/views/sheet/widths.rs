//! Column widths the user chose, in characters so that the zoom keeps their proportions, and how they
//! follow the columns (pure functions). `None` is an automatic column.

use std::ops::Range;

/// Narrowest a column can be set to, in characters: room for the padding and an ellipsis.
pub(super) const NARROWEST: f32 = 3.;
/// Widest a column can be, in pixels: a stray drag never makes the scroll unusable.
pub(super) const WIDEST_PX: f32 = 4000.;
/// Widest value read from the config, in characters (the pixels are bounded again on display).
const MOST_CHARS: f32 = 1000.;

/// Width of a character (0.6 em, the advance of the monospace) at the text size `text_px`.
fn char_px(text_px: f32) -> f32 {
    text_px * 0.6
}

/// Pixels of a column set to `chars` characters, with `pad` on each side.
pub(super) fn to_px(chars: f32, text_px: f32, pad: f32) -> f32 {
    (chars * char_px(text_px) + 2. * pad).min(WIDEST_PX)
}

/// Characters of a column `px` wide, within the limits, to a tenth.
pub(super) fn to_chars(px: f32, text_px: f32, pad: f32) -> f32 {
    let widest = ((WIDEST_PX - 2. * pad) / char_px(text_px)).max(NARROWEST);
    let chars = ((px - 2. * pad) / char_px(text_px)).clamp(NARROWEST, widest);
    (chars * 10.).round() / 10.
}

/// Width of a text in em, `mono` or in Plex Sans, counting at most `cap` characters. Monospace: 0.6 em a
/// character. Plex Sans: lowercase is narrower, capitals and digits a little wider; CJK and emoji (from
/// U+2E80) a full em.
pub(super) fn ems(text: &str, mono: bool, cap: usize) -> f32 {
    text.chars()
        .take(cap)
        .map(|ch| match ch {
            _ if mono => 0.6,
            _ if ch as u32 >= 0x2E80 => 1.,
            _ if ch.is_lowercase() => 0.55,
            _ => 0.65,
        })
        .sum()
}

/// Characters that fit a column whose longest text is `em` wide, no wider than the view (`view` pixels, 0: not
/// measured yet).
pub(super) fn fit(em: f32, view: f32, text_px: f32, pad: f32) -> f32 {
    let px = em * text_px + 2. * pad;
    to_chars(if view > 0. { px.min(view) } else { px }, text_px, pad)
}

/// The widths field of a `tables` line: `12,,30.5`. A value that does not read is automatic.
pub(super) fn parse(field: &str) -> Vec<Option<f32>> {
    if field.trim().is_empty() {
        return Vec::new();
    }
    field
        .split(',')
        .map(|v| v.trim().parse::<f32>().ok().filter(|w| w.is_finite() && *w > 0.).map(|w| w.clamp(NARROWEST, MOST_CHARS)))
        .collect()
}

/// The widths field: automatic columns left empty, none after the last chosen one.
pub(super) fn write(widths: &[Option<f32>]) -> String {
    let last = widths.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
    widths[..last].iter().map(|w| w.map(|w| format!("{}", (w * 10.).round() / 10.)).unwrap_or_default()).collect::<Vec<_>>().join(",")
}

/// Columns put at `at`, with their widths.
pub(super) fn insert(widths: &mut Vec<Option<f32>>, at: usize, put: Vec<Option<f32>>) {
    if widths.len() < at {
        widths.resize(at, None);
    }
    widths.splice(at..at, put);
}

/// Columns taken out, and their widths.
pub(super) fn remove(widths: &mut Vec<Option<f32>>, range: Range<usize>) -> Vec<Option<f32>> {
    if widths.len() < range.end {
        widths.resize(range.end, None);
    }
    widths.drain(range).collect()
}

/// The columns of `range` start at `to` (counted once they are taken out), as `Table::move_cols`.
pub(super) fn shift(widths: &mut Vec<Option<f32>>, range: Range<usize>, to: usize) {
    let moved = remove(widths, range);
    insert(widths, to, moved);
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAD: f32 = 8.;

    #[test]
    fn reads_and_writes_the_widths_field() {
        assert_eq!(parse(""), vec![]);
        assert_eq!(parse("12,,30.5"), vec![Some(12.), None, Some(30.5)]);
        // Malformed, negative, zero, infinite: automatic; too small or huge: within the limits.
        assert_eq!(parse("abc,-4,0,inf,1,99999"), vec![None, None, None, None, Some(NARROWEST), Some(MOST_CHARS)]);
        assert_eq!(write(&[Some(12.), None, Some(30.5), None, None]), "12,,30.5");
        assert_eq!(write(&[None, None]), "");
        assert_eq!(write(&[]), "");
        assert_eq!(parse(&write(&[None, Some(7.25)])), vec![None, Some(7.3)]);
    }

    #[test]
    fn widths_follow_their_columns() {
        let mut w = vec![Some(5.), None, Some(9.)];
        insert(&mut w, 1, vec![None, None]);
        assert_eq!(w, [Some(5.), None, None, None, Some(9.)]);
        assert_eq!(remove(&mut w, 0..1), [Some(5.)]);
        assert_eq!(w, [None, None, None, Some(9.)]);
        // Moved left by one, then back: as before.
        shift(&mut w, 3..4, 2);
        assert_eq!(w, [None, None, Some(9.), None]);
        shift(&mut w, 2..3, 3);
        assert_eq!(w, [None, None, None, Some(9.)]);
        // Shorter than the table (automatic columns at the end are not kept): padded first.
        let mut short = vec![Some(4.)];
        insert(&mut short, 3, vec![Some(6.)]);
        assert_eq!(short, [Some(4.), None, None, Some(6.)]);
        let mut short = vec![Some(4.)];
        assert_eq!(remove(&mut short, 2..3), [None]);
        assert_eq!(short, [Some(4.), None]);
    }

    #[test]
    fn widths_stay_within_limits() {
        assert_eq!(to_chars(0., 14., PAD), NARROWEST);
        assert_eq!(to_chars(-500., 14., PAD), NARROWEST);
        assert!(to_px(to_chars(1e9, 14., PAD), 14., PAD) <= WIDEST_PX);
        assert_eq!(to_px(MOST_CHARS, 32., PAD), WIDEST_PX);
        // A drag back to where it started gives the same width.
        let w = to_px(12., 14., PAD);
        assert_eq!(to_chars(w, 14., PAD), 12.);
    }

    #[test]
    fn the_zoom_keeps_the_proportions() {
        let (small, big) = (to_px(10., 11., 0.), to_px(10., 29., 0.));
        assert!((big / small - 29. / 11.).abs() < 1e-4);
        assert!(to_px(20., 14., PAD) > to_px(10., 14., PAD));
    }

    #[test]
    fn fits_the_text_within_the_view() {
        // Empty column: the narrowest; one character: still the narrowest.
        assert_eq!(fit(0., 600., 14., PAD), NARROWEST);
        assert_eq!(fit(ems("x", true, usize::MAX), 600., 14., PAD), NARROWEST);
        // 50 monospace characters: 50, beyond the 40 of the automatic width.
        assert_eq!(fit(ems(&"7".repeat(50), true, usize::MAX), 2000., 14., PAD), 50.);
        // 10 000 characters without a space: the width of the view.
        let long = ems(&"a".repeat(10_000), false, usize::MAX);
        assert_eq!(to_px(fit(long, 700., 14., PAD), 14., PAD).round(), 700.);
        // Not measured yet: the text, within the widest.
        assert!(to_px(fit(long, 0., 14., PAD), 14., PAD) <= WIDEST_PX);
        assert_eq!(ems("漢字", false, 9), 2.);
        assert_eq!(ems("éé", false, 1), 0.55);
    }
}
