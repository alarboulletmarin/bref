//! Vault graph maths: edges from links, force-directed layout, node sizes, ring of unlinked notes,
//! drag propagation. Pure functions; drawing and navigation are in `views/graph.rs`.

use std::path::{Path, PathBuf};

use crate::logic::note::{Note, NoteKey};

/// Links from which a note is a hub: its name is always shown.
pub const HUB: usize = 6;
/// Room (layout units) each note without links takes on the outer ring.
const ORPHAN_GAP: f32 = 0.5;
/// Frames during which dragged-along nodes finish settling after a drag.
pub const SETTLE: u32 = 60;

/// Name of an image in the graph and in links: its file name.
pub fn image_name(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Edges `(i, j)`, `i < j`, without duplicates: wikilinks targeting a note of the
/// vault, and shown images. Images are numbered after notes.
pub fn edges(notes: &[Note], images: &[PathBuf]) -> Vec<(usize, usize)> {
    let mut by_name = crate::logic::wiki::by_key(notes.iter());
    for (i, image) in images.iter().enumerate() {
        by_name.entry(NoteKey::of(&image_name(image))).or_insert(notes.len() + i);
    }
    let mut edges: Vec<(usize, usize)> = notes
        .iter()
        .enumerate()
        .flat_map(|(i, note)| note.links.iter().map(move |link| (i, link)))
        .filter_map(|(i, link)| {
            let j = *by_name.get(link)?;
            (i != j).then_some((i.min(j), i.max(j)))
        })
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges
}

/// Starting position of node `i`: a spiral, identical from one launch to the next.
pub fn spiral(i: usize) -> (f32, f32) {
    let (r, a) = ((i as f32 + 0.5).sqrt(), i as f32 * 2.399_963);
    (r * a.cos(), r * a.sin())
}

/// Force-directed layout (Fruchterman-Reingold): close nodes
/// repel, edges pull together, a slight gravity brings unlinked notes back
/// to the center. The unit is the ideal length of an edge.
/// `seed` reuses known positions.
// ponytail: O(n²) repulsion per iteration, fine up to ~1500 notes;
// move to Barnes-Hut if vaults grow.
pub fn layout(seed: &[Option<(f32, f32)>], edges: &[(usize, usize)]) -> Vec<(f32, f32)> {
    let n = seed.len();
    let mut pos: Vec<(f32, f32)> =
        seed.iter().enumerate().map(|(i, s)| s.unwrap_or_else(|| spiral(i))).collect();
    let iterations = (40_000_000 / (n * n).max(1)).clamp(30, 300);
    let start = 0.5 + (n as f32).sqrt() * 0.1;
    let mut push = vec![(0f32, 0f32); n];
    for step in 0..iterations {
        push.fill((0., 0.));
        for i in 0..n {
            for j in i + 1..n {
                let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                let d2 = (dx * dx + dy * dy).max(0.01);
                // Limited range: without it, isolated notes drift very far.
                if d2 > 9. {
                    continue;
                }
                let (fx, fy) = (dx / d2, dy / d2);
                push[i] = (push[i].0 + fx, push[i].1 + fy);
                push[j] = (push[j].0 - fx, push[j].1 - fy);
            }
        }
        for &(i, j) in edges {
            let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
            let d = (dx * dx + dy * dy).sqrt();
            let (fx, fy) = (dx * d, dy * d);
            push[i] = (push[i].0 - fx, push[i].1 - fy);
            push[j] = (push[j].0 + fx, push[j].1 + fy);
        }
        // The maximum step decreases: nodes freeze little by little.
        let heat = start * (1. - step as f32 / iterations as f32) + 0.01;
        for (p, f) in pos.iter_mut().zip(&push) {
            let (fx, fy) = (f.0 - p.0 * 0.05, f.1 - p.1 * 0.05);
            let len = (fx * fx + fy * fy).sqrt().max(1e-6);
            let by = len.min(heat) / len;
            *p = (p.0 + fx * by, p.1 + fy * by);
        }
    }
    pos
}

/// Size of a node (px at the reference zoom) by its number of links: a few steps, not a curve.
pub fn node_size(links: usize) -> f32 {
    match links {
        0 => 3.5,
        1..=2 => 4.5,
        3..=5 => 6.,
        6..=11 => 8.,
        _ => 10.,
    }
}

/// `count` places evenly spaced on a ring of at least `inner` units, from the top, clockwise.
// ponytail: a single ring, so a vault of thousands of notes without links makes a huge
// circle; concentric rings if that gets in the way.
pub fn ring(count: usize, inner: f32) -> Vec<(f32, f32)> {
    let radius = inner.max(count as f32 * ORPHAN_GAP / std::f32::consts::TAU);
    (0..count)
        .map(|k| {
            let angle = k as f32 / count as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            (radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

/// Places the notes: linked ones by `layout`, the others on a ring around them.
pub fn arrange(seed: &[Option<(f32, f32)>], edges: &[(usize, usize)]) -> Vec<(f32, f32)> {
    let mut linked = vec![false; seed.len()];
    for &(i, j) in edges {
        (linked[i], linked[j]) = (true, true);
    }
    let mut slot = vec![0; seed.len()];
    let mut inner_seed = Vec::new();
    for (i, s) in seed.iter().enumerate() {
        if linked[i] {
            slot[i] = inner_seed.len();
            inner_seed.push(*s);
        }
    }
    let inner_edges: Vec<(usize, usize)> = edges.iter().map(|&(i, j)| (slot[i], slot[j])).collect();
    let placed = layout(&inner_seed, &inner_edges);
    let reach = placed.iter().map(|p| p.0.hypot(p.1)).fold(0., f32::max);
    let mut outer = ring(seed.len() - placed.len(), reach + 1.5).into_iter();
    (0..seed.len()).map(|i| if linked[i] { placed[slot[i]] } else { outer.next().unwrap_or_default() }).collect()
}

/// One drag step: the held node's move propagates along links.
/// Each linked node follows the mean move of its neighbors and returns
/// a little toward its `rest` place; the farther from the held node, the less it moves.
// ponytail: no repulsion during the drag, a dragged-along node can
// cover another; rerun `layout` on release if that gets in the way.
pub fn follow(pos: &mut [(f32, f32)], rest: &[(f32, f32)], near: &[Vec<usize>], held: usize) {
    let shift: Vec<(f32, f32)> = pos.iter().zip(rest).map(|(p, r)| (p.0 - r.0, p.1 - r.1)).collect();
    for (i, links) in near.iter().enumerate() {
        if i == held || links.is_empty() {
            continue;
        }
        let n = links.len() as f32;
        let mean = links.iter().fold((0., 0.), |a, &j| (a.0 + shift[j].0 / n, a.1 + shift[j].1 / n));
        let ease = |own: f32, mean: f32| own + 0.12 * (mean - own) - 0.012 * own;
        pos[i] = (rest[i].0 + ease(shift[i].0, mean.0), rest[i].1 + ease(shift[i].1, mean.1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn note(name: &str, links: &[&str]) -> Note {
        Note {
            name: name.into(),
            path: PathBuf::from(format!("/v/{name}.md")),
            tags: Default::default(),
            links: links.iter().map(|l| NoteKey::of(l)).collect(),
            aliases: Default::default(),
            mtime: SystemTime::UNIX_EPOCH,
            body: "".into(),
            lower: "".into(),
        }
    }

    #[test]
    fn an_alias_is_an_edge() {
        // A front matter alias counts as the name.
        let mut notes = vec![note("Appli", &[]), note("Journal", &["bref app"])];
        notes[0].aliases = vec!["Bref App".into()].into();
        assert_eq!(edges(&notes, &[]), [(0, 1)]);
    }

    #[test]
    fn a_name_beats_an_alias_of_an_earlier_note() {
        let mut notes = vec![note("Autre", &[]), note("Cible", &[]), note("Source", &["cible"])];
        notes[0].aliases = vec!["Cible".into()].into();
        assert_eq!(edges(&notes, &[]), [(1, 2)]);
    }

    #[test]
    fn resolves_edges() {
        let notes = [note("A", &["b", "a", "absente"]), note("B", &["a", "c"]), note("c", &[])];
        // No duplicate, no loop, no link to a missing note.
        assert_eq!(edges(&notes, &[]), [(0, 1), (1, 2)]);
        // An image shown by a note is linked to it; it comes after the notes.
        let notes = [note("a", &["plan.png"]), note("b", &[])];
        assert_eq!(edges(&notes, &[PathBuf::from("/v/img/Plan.PNG")]), [(0, 2)]);
    }

    #[test]
    fn lays_out_linked_notes_closer() {
        let dist = |p: &[(f32, f32)], i: usize, j: usize| (p[i].0 - p[j].0).hypot(p[i].1 - p[j].1);
        let pos = layout(&[None; 6], &[(0, 1), (1, 2)]);
        assert!(pos.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
        assert!(dist(&pos, 0, 1) < dist(&pos, 0, 4));
        assert!(dist(&pos, 1, 2) < dist(&pos, 2, 5));
        // No node overlaps another, and the result is reproducible.
        assert!((0..6).all(|i| (i + 1..6).all(|j| dist(&pos, i, j) > 0.3)));
        assert_eq!(pos, layout(&[None; 6], &[(0, 1), (1, 2)]));
    }

    #[test]
    fn sizes_grow_by_steps_with_links() {
        let sizes: Vec<f32> = (0..=40).map(node_size).collect();
        assert!(sizes.windows(2).all(|w| w[0] <= w[1]));
        // A few steps: 0 | 1-2 | 3-5 | 6-11 | 12+.
        let mut steps = sizes.clone();
        steps.dedup();
        assert_eq!(steps.len(), 5);
        assert!(node_size(HUB - 1) < node_size(HUB));
    }

    #[test]
    fn notes_without_links_sit_on_an_even_outer_ring() {
        // 0-1-2 linked, 3..=202 alone (also the empty and one-note vaults).
        assert!(arrange(&[], &[]).is_empty());
        assert_eq!(arrange(&[None], &[]).len(), 1);
        let n = 203;
        let pos = arrange(&vec![None; n], &[(0, 1), (1, 2)]);
        let r = |i: usize| pos[i].0.hypot(pos[i].1);
        let reach = (0..3).map(r).fold(0., f32::max);
        let ring: Vec<f32> = (3..n).map(r).collect();
        assert!(ring.iter().all(|&d| d > reach && (d - ring[0]).abs() < 1e-3));
        // Evenly spaced: every neighbour at the same distance, none on top of another.
        let gap = |i: usize, j: usize| (pos[i].0 - pos[j].0).hypot(pos[i].1 - pos[j].1);
        let first = gap(3, 4);
        assert!((3..n - 1).all(|i| (gap(i, i + 1) - first).abs() < 1e-3) && first >= ORPHAN_GAP * 0.99);
        assert_eq!(pos, arrange(&vec![None; n], &[(0, 1), (1, 2)]));
    }

    #[test]
    fn dragged_node_pulls_its_neighbours() {
        // Chain 0-1-2, and 3 unlinked. Node 0 is dragged 5 units to the right.
        let rest = [(0., 0.), (1., 0.), (2., 0.), (0., 2.)];
        let near = [vec![1], vec![0, 2], vec![1], vec![]];
        let mut pos = rest;
        pos[0] = (5., 0.);
        for _ in 0..SETTLE {
            follow(&mut pos, &rest, &near, 0);
        }
        let moved = |i: usize| pos[i].0 - rest[i].0;
        // It stays under the pointer; its neighbors follow, less and less far.
        assert_eq!((pos[0], pos[3]), ((5., 0.), rest[3]));
        assert!(moved(1) > 3. && moved(1) < 5. && moved(2) > 2. && moved(2) < moved(1));
    }
}
