//! Fitting a file name and its detail in a row of the panel: how they share the room, where a text
//! is cut, where an extension starts. Pure functions; measuring and drawing are in `shell/sidebar.rs`.

/// How a row shares `room` between its name and its detail (the folder of a recent note), `gap` between
/// them: the name keeps its whole width and the detail gives way first, cut with an ellipsis, then hidden
/// under `DETAIL_MIN`; only then does the name get cut. Returns the two widths, the detail's `0` if hidden.
pub fn share(room: f32, name: f32, detail: f32, gap: f32, min: f32) -> (f32, f32) {
    let room = room.max(0.);
    if detail <= 0. {
        return (name.min(room), 0.);
    }
    let left = room - name - gap;
    match left {
        l if l >= detail => (name, detail),
        l if l >= min => (name, l),
        _ => (name.min(room), 0.),
    }
}

/// `text` cut to fit `room`, ending with an ellipsis when it had to be (`width` measures a text); empty if
/// not even the ellipsis fits. gpui's own `truncate` leaves a hard cut when a flex row shrinks a text.
pub fn ellipsize(text: &str, room: f32, width: impl Fn(&str) -> f32) -> String {
    if width(text) <= room {
        return text.to_string();
    }
    let mut ends: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    ends.push(text.len());
    // The longest prefix that fits with the ellipsis (width grows with the prefix: bisection).
    let (mut low, mut high) = (0, ends.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        match width(&format!("{}…", &text[..ends[mid]])) <= room {
            true => low = mid,
            false => high = mid - 1,
        }
    }
    let cut = text[..ends[low]].trim_end();
    match width("…") <= room {
        true => format!("{cut}…"),
        false => String::new(),
    }
}

/// A file's name cut into the name and its extension (`contacts` + `.csv`, the dot with the extension); only
/// the last dot counts, and a leading or trailing dot is not an extension.
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => name.split_at(dot),
        _ => (name, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_keeps_its_room_before_the_folder() {
        // Both fit.
        assert_eq!(share(200., 100., 60., 4., 40.), (100., 60.));
        // The folder gives way first, cut but readable.
        assert_eq!(share(200., 100., 120., 4., 40.), (100., 96.));
        assert_eq!(share(200., 156., 120., 4., 40.), (156., 40.));
        // Under the minimum the folder is hidden and the name has all the room.
        assert_eq!(share(200., 157., 120., 4., 40.), (157., 0.));
        assert_eq!(share(200., 190., 120., 4., 40.), (190., 0.));
        // Only a name wider than the row is cut, and no folder is shown.
        assert_eq!(share(200., 300., 50., 4., 40.), (200., 0.));
        // No folder (a note at the root), no room at all.
        assert_eq!(share(200., 80., 0., 4., 40.), (80., 0.));
        assert_eq!(share(-5., 80., 10., 4., 40.), (0., 0.));
    }

    #[test]
    fn a_cut_text_ends_with_an_ellipsis_that_fits() {
        // One unit per character.
        let width = |t: &str| t.chars().count() as f32;
        assert_eq!(ellipsize("abc", 3., width), "abc");
        assert_eq!(ellipsize("abcd", 3., width), "ab…");
        assert_eq!(ellipsize("abcdef", 1., width), "…");
        assert_eq!(ellipsize("abcdef", 0.5, width), "");
        assert_eq!(ellipsize("", 0., width), "");
        // One letter that does not fit (a stem next to a long extension).
        assert_eq!(ellipsize("a", 0.5, width), "");
        assert_eq!(ellipsize("a", -3., width), "");
        assert_eq!(ellipsize("ab", 1., width), "…");
        // Never inside an accent, an emoji or a wide letter; a trailing space goes.
        assert_eq!(ellipsize("é🎉 漢字 x", 4., width), "é🎉…");
        assert_eq!(ellipsize("aa  bbbb", 5., width), "aa…");
        for room in 0..12 {
            let cut = ellipsize("Profonde note à é🎉漢", room as f32, width);
            assert!(width(&cut) <= room as f32, "{room}: {cut}");
        }
    }

    #[test]
    fn an_extension_is_split_off_a_file_name_only() {
        assert_eq!(split_ext("a.csv"), ("a", ".csv"));
        assert_eq!(split_ext("photo.PNG"), ("photo", ".PNG"));
        assert_eq!(split_ext("archive.tar.gz"), ("archive.tar", ".gz"));
        assert_eq!(split_ext(".gitignore"), (".gitignore", ""));
        assert_eq!(split_ext("sans extension"), ("sans extension", ""));
        assert_eq!(split_ext("fin."), ("fin.", ""));
        assert_eq!(split_ext(""), ("", ""));
        assert_eq!(split_ext("é 漢字 🎉.pdf"), ("é 漢字 🎉", ".pdf"));
    }
}
