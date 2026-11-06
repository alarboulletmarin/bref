//! The side panel's tools and the `layout` file line that remembers where they are: the panel's
//! place, the widths, the tools stacked in the left dock and those of the right dock. Pure: the
//! panel itself (`views/nav.rs`) turns the widths into pixels.

use std::collections::HashSet;

/// Default width of a dock, at scale 1.
pub const LIST_W: f32 = 260.;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Mode {
    #[default]
    Tree,
    Recent,
    Graph,
    Tags,
    /// Backlinks: the notes that lead to the open note.
    Links,
    /// Calendar: a month, showing the days that have their note.
    Calendar,
    /// The vault search: the matches of a search, note by note.
    Search,
}

impl Mode {
    pub const ALL: [Mode; 7] = [Mode::Tree, Mode::Recent, Mode::Graph, Mode::Tags, Mode::Links, Mode::Calendar, Mode::Search];

    /// As written in the `layout` file.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Tree => "tree",
            Mode::Recent => "recent",
            Mode::Graph => "graph",
            Mode::Tags => "tags",
            Mode::Links => "links",
            Mode::Calendar => "calendar",
            Mode::Search => "search",
        }
    }

    pub fn named(name: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.name() == name)
    }

    /// Only the graph and the calendar open as a tab: they need room, the lists do not.
    pub fn tabs(self) -> bool {
        matches!(self, Mode::Graph | Mode::Calendar)
    }
}

/// Place of the panel: collapsed onto its rail, next to the note, or alone.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Panel {
    Rail,
    Split,
    Full,
}

/// What the `layout` file holds. `mode` is the first tool of the left dock, as older files say it.
#[derive(Clone, PartialEq, Debug)]
pub struct Layout {
    pub mode: Mode,
    pub panel: Panel,
    pub list_w: f32,
    /// Zero: half the window.
    pub graph_w: f32,
    pub left_shown: Vec<Mode>,
    pub right: Vec<Mode>,
    pub right_shown: Vec<Mode>,
    pub right_w: f32,
    pub folded: HashSet<Mode>,
}

/// Reads the file: `mode panel list_w graph_w`, then, in any order, `left=`, `right=`, `width=`,
/// `show=` and `fold=` (older files stop after the numbers). Whatever is missing or unknown
/// takes its default.
pub fn parse(text: &str) -> Layout {
    let mut words = text.split_whitespace();
    let mode = words.next().and_then(Mode::named).unwrap_or_default();
    let (positional, keyed): (Vec<&str>, Vec<&str>) = words.partition(|w| !w.contains('='));
    let key = |name: &str| keyed.iter().find_map(|w| w.strip_prefix(name)?.strip_prefix('='));
    let list = |name: &str| {
        let mut tools: Vec<Mode> = Vec::new();
        for tool in key(name).unwrap_or_default().split(',').filter_map(Mode::named) {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        }
        tools
    };
    // A tool on both sides stays on the left, where it always was.
    let mut left_shown = list("left");
    if !left_shown.contains(&mode) {
        left_shown.insert(0, mode);
    }
    let right: Vec<Mode> = list("right").into_iter().filter(|m| !left_shown.contains(m)).collect();
    let right_shown = list("show").into_iter().filter(|m| right.contains(m)).collect();
    let folded = list("fold").into_iter().collect();
    let mut words = positional.into_iter();
    Layout {
        mode,
        left_shown,
        right_w: key("width").and_then(|w| w.parse().ok()).unwrap_or(LIST_W),
        right,
        right_shown,
        folded,
        panel: match words.next() {
            Some("split") => Panel::Split,
            Some("full") => Panel::Full,
            _ => Panel::Rail,
        },
        list_w: words.next().and_then(|w| w.parse().ok()).unwrap_or(LIST_W),
        graph_w: words.next().and_then(|w| w.parse().ok()).unwrap_or(0.),
    }
}

/// The line `parse` reads back (keys only when they say something).
pub fn write(layout: &Layout) -> String {
    let panel = match layout.panel {
        Panel::Rail => "rail",
        Panel::Split => "split",
        Panel::Full => "full",
    };
    let names = |tools: &mut dyn Iterator<Item = &Mode>| tools.map(|m| m.name()).collect::<Vec<_>>().join(",");
    let mut line = format!("{} {panel} {} {}", layout.mode.name(), layout.list_w, layout.graph_w);
    if layout.left_shown.len() > 1 {
        line += &format!(" left={}", names(&mut layout.left_shown.iter()));
    }
    if !layout.right.is_empty() {
        line += &format!(" right={} width={}", names(&mut layout.right.iter()), layout.right_w);
        if !layout.right_shown.is_empty() {
            line += &format!(" show={}", names(&mut layout.right_shown.iter()));
        }
    }
    if !layout.folded.is_empty() {
        let mut folded: Vec<&Mode> = layout.folded.iter().collect();
        folded.sort_by_key(|m| m.name());
        line += &format!(" fold={}", names(&mut folded.into_iter()));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_line_without_keys_loads() {
        let l = parse("recent split 300 450\n");
        assert_eq!((l.mode, l.panel, l.list_w, l.graph_w), (Mode::Recent, Panel::Split, 300., 450.));
        assert_eq!(l.left_shown, vec![Mode::Recent]);
        assert!(l.right.is_empty() && l.right_shown.is_empty() && l.folded.is_empty());
        assert_eq!(l.right_w, LIST_W);
    }

    #[test]
    fn missing_or_unknown_takes_defaults() {
        for text in ["", "   ", "nonsense", "tree nope x y"] {
            let l = parse(text);
            assert_eq!((l.mode, l.panel, l.list_w, l.graph_w), (Mode::Tree, Panel::Rail, LIST_W, 0.), "{text:?}");
            assert_eq!(l.left_shown, vec![Mode::Tree]);
        }
        assert_eq!(parse("graph full").graph_w, 0.);
    }

    #[test]
    fn current_line_round_trips() {
        let line = "tree split 260 0 left=tree,tags right=graph,links width=320 show=links fold=tags";
        let l = parse(line);
        assert_eq!(l.left_shown, vec![Mode::Tree, Mode::Tags]);
        assert_eq!(l.right, vec![Mode::Graph, Mode::Links]);
        assert_eq!(l.right_shown, vec![Mode::Links]);
        assert_eq!((l.right_w, l.folded.len()), (320., 1));
        assert_eq!(write(&l), line);
        assert_eq!(parse(&write(&l)), l);
    }

    #[test]
    fn a_tool_on_both_sides_stays_left_and_the_mode_leads() {
        let l = parse("graph split 260 0 left=tags right=tags,recent show=tags,recent");
        assert_eq!(l.left_shown, vec![Mode::Graph, Mode::Tags]);
        assert_eq!((l.right.clone(), l.right_shown.clone()), (vec![Mode::Recent], vec![Mode::Recent]));
    }

    /// What the file says, as (left shown, right, right shown): old files, a tool on both sides, unknown names,
    /// a tool shown on a side it does not live on, empty.
    #[test]
    fn the_layout_file_old_new_and_odd() {
        use Mode::*;
        let read = |text: &str| {
            let l = parse(text);
            (l.left_shown, l.right, l.right_shown)
        };
        // Before docks: everything on the left, as it was.
        assert_eq!(read("tags split 300 0\n"), (vec![Tags], vec![], vec![]));
        // A tool on both sides keeps the left; an unknown one is dropped; one shown on the right that
        // does not live there is not shown.
        assert_eq!(read("graph split 300 0 right=links,bogus,graph,links show=tags width=250\n"), (vec![Graph], vec![Links], vec![]));
        assert_eq!(read("tree rail 260 0 width=x right=calendar show=calendar\n"), (vec![Tree], vec![Calendar], vec![Calendar]));
        assert_eq!(read("tree split 260 0 left=links,tree,links right=links,tags show=tags,recent fold=links,zzz\n"), (vec![Links, Tree], vec![Tags], vec![Tags]));
        // Empty or absent: the defaults.
        assert_eq!(read(""), (vec![Tree], vec![], vec![]));
    }

    #[test]
    fn plain_line_has_no_keys() {
        let l = parse("tree rail 260 0");
        assert_eq!(write(&l), "tree rail 260 0");
    }
}
