//! Where a shape sits when nobody placed it: a size that follows its text, and a layer under
//! what points to it. The Mermaid import lays a diagram out this way, and so does an agent
//! that names shapes without giving them a place.

use std::collections::HashSet;

use super::{Diagram, LINE, RULE, snap};

/// Width and height of a box that holds these lines, at least `min`.
pub fn fit(lines: &[String], min: (f32, f32)) -> (f32, f32) {
    let rules = lines.iter().filter(|l| l.as_str() == RULE).count() as f32;
    let wide = lines.iter().map(|l| Diagram::guess_width(l)).fold(0., f32::max);
    ((wide + 28.).max(min.0).ceil(), ((lines.len() as f32 - rules) * LINE + rules * 8. + 18.).max(min.1).ceil())
}

/// Places boxes of these sizes by layers: each is under what points to it (`edges` run from the upper to
/// the lower; one that closes a loop is not counted), centred in its layer. Returns the top left corner
/// of each, and the size of the whole.
pub fn layered(sizes: &[(f32, f32)], edges: &[(usize, usize)]) -> (Vec<(f32, f32)>, (f32, f32)) {
    let n = sizes.len();
    let mut next = vec![Vec::new(); n];
    for (a, b) in edges.iter().filter(|(a, b)| a < &n && b < &n) {
        next[*a].push(*b);
    }
    // A depth-first walk finds the edges that go back (loops), and a topological order.
    let (mut seen, mut order, mut back) = (vec![0u8; n], Vec::new(), HashSet::new());
    // ponytail: the walk recurses as deep as the longest chain (a few thousand states are fine); make it iterative if a block ever has tens of thousands.
    fn walk(u: usize, next: &[Vec<usize>], seen: &mut [u8], order: &mut Vec<usize>, back: &mut HashSet<(usize, usize)>) {
        seen[u] = 1;
        for &v in &next[u] {
            match seen[v] {
                0 => walk(v, next, seen, order, back),
                1 => {
                    back.insert((u, v));
                }
                _ => {}
            }
        }
        seen[u] = 2;
        order.push(u);
    }
    for u in 0..n {
        if seen[u] == 0 {
            walk(u, &next, &mut seen, &mut order, &mut back);
        }
    }
    let mut rank = vec![0usize; n];
    for &u in order.iter().rev() {
        for &v in next[u].iter().filter(|v| !back.contains(&(u, **v))) {
            rank[v] = rank[v].max(rank[u] + 1);
        }
    }
    let layers = rank.iter().copied().max().map_or(0, |m| m + 1);
    let rows: Vec<Vec<usize>> = (0..layers).map(|r| (0..n).filter(|i| rank[*i] == r).collect()).collect();
    let widths: Vec<f32> = rows.iter().map(|row| row.iter().map(|i| sizes[*i].0).sum::<f32>() + 50. * row.len().saturating_sub(1) as f32).collect();
    let wide = widths.iter().copied().fold(0., f32::max);
    let (mut at, mut y) = (vec![(0., 0.); n], 0.);
    for (row, width) in rows.iter().zip(&widths) {
        let mut x = (wide - width) / 2.;
        for &i in row {
            at[i] = (snap(x), snap(y));
            x += sizes[i].0 + 50.;
        }
        y += row.iter().map(|i| sizes[*i].1).fold(0., f32::max) + 70.;
    }
    (at, (wide, (y - 70.).max(0.)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_goes_down_and_a_loop_does_not_grow_forever() {
        let sizes = [(100., 40.), (80., 40.), (60., 40.)];
        let (at, _) = layered(&sizes, &[(0, 1), (1, 2), (2, 0)]);
        assert!(at[1].1 > at[0].1 && at[2].1 > at[1].1);
        assert_eq!(layered(&sizes, &[]).0.len(), 3);
        // Three hundred boxes still come back, one place each.
        let many = vec![(40., 20.); 300];
        let edges: Vec<_> = (0..299).map(|i| (i, i + 1)).collect();
        assert_eq!(layered(&many, &edges).0.len(), 300);
    }

    #[test]
    fn a_box_grows_with_its_text() {
        let short = fit(&["A".into()], (40., 20.));
        let long = fit(&["A name that is quite a bit longer".into()], (40., 20.));
        assert!(long.0 > short.0 && short.0 >= 40. && short.1 >= 20.);
    }
}
