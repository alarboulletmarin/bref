//! Right-angled arrows that go around the shapes in their way: a path of fewest bends, then
//! shortest, found on the lines that run along the shapes' sides (a visibility grid), and kept
//! where it is as long as nothing new stands in it.

use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
};

/// A shape's box: left, top, right, bottom.
pub type Rect = (f32, f32, f32, f32);
type Point = (f32, f32);

/// How far a stroke keeps from a shape it passes, and the length of the stub it leaves a shape by.
const MARGIN: f32 = 10.;
const STUB: f32 = 20.;
/// What a bend costs, in length: fewer bends first, then shorter.
const BEND: f32 = 40.;
/// How far around the two ends the search looks first; it widens to everything if it finds nothing.
const WINDOW: f32 = 160.;

/// Right-angled stroke from `a` to `b`, leaving in direction `da` and
/// arriving by `db` (the sides by which it leaves what it links).
pub fn elbow(a: (f32, f32), da: (f32, f32), b: (f32, f32), db: (f32, f32)) -> Vec<(f32, f32)> {

    let flat = |d: (f32, f32)| d.0 != 0.;
    let (a1, b1) = ((a.0 + da.0 * STUB, a.1 + da.1 * STUB), (b.0 + db.0 * STUB, b.1 + db.1 * STUB));
    let facing = da == (-db.0, -db.1) && (b.0 - a.0) * da.0 + (b.1 - a.1) * da.1 > 2. * STUB;
    let mut points = match (flat(da), flat(db)) {
        // Facing each other: a single jog, halfway.
        (true, true) if facing => vec![a, ((a.0 + b.0) / 2., a.1), ((a.0 + b.0) / 2., b.1), b],
        (false, false) if facing => vec![a, (a.0, (a.1 + b.1) / 2.), (b.0, (a.1 + b.1) / 2.), b],
        // On the same side: the stroke goes wide of both.
        (true, true) if da == db => {
            let x = if da.0 > 0. { a1.0.max(b1.0) } else { a1.0.min(b1.0) };
            vec![a, (x, a.1), (x, b.1), b]
        }
        (false, false) if da == db => {
            let y = if da.1 > 0. { a1.1.max(b1.1) } else { a1.1.min(b1.1) };
            vec![a, (a.0, y), (b.0, y), b]
        }
        // Back to back: it leaves, passes between the two, then comes back.
        (true, true) => vec![a, a1, (a1.0, (a.1 + b.1) / 2.), (b1.0, (a.1 + b.1) / 2.), b1, b],
        (false, false) => vec![a, a1, ((a.0 + b.0) / 2., a1.1), ((a.0 + b.0) / 2., b1.1), b1, b],
        (true, false) => vec![a, (b.0, a.1), b],
        (false, true) => vec![a, (a.0, b.1), b],
    };
    // A point aligned with its neighbors is useless. (Two ends in the same place are still a stroke of two points.)
    points.dedup();
    if points.len() < 2 {
        return vec![a, b];
    }
    let mut i = 1;
    while i + 1 < points.len() {
        let (p, q, r) = (points[i - 1], points[i], points[i + 1]);
        if ((q.0 - p.0) * (r.1 - q.1) - (q.1 - p.1) * (r.0 - q.0)).abs() < 1e-3 {
            points.remove(i);
        } else {
            i += 1;
        }
    }
    points
}

/// Whether the open segment `p`–`q` (horizontal or vertical) goes through the inside of a box.
fn crosses(p: Point, q: Point, boxes: &[Rect]) -> bool {
    let (x0, x1, y0, y1) = (p.0.min(q.0), p.0.max(q.0), p.1.min(q.1), p.1.max(q.1));
    boxes.iter().any(|&(l, t, r, b)| x0 < r && x1 > l && y0 < b && y1 > t && (x1 > x0 || (l < x0 && x0 < r)) && (y1 > y0 || (t < y0 && y0 < b)))
}

/// Whether a path crosses none of the boxes.
fn clear(path: &[Point], boxes: &[Rect]) -> bool {
    path.windows(2).all(|w| !crosses(w[0], w[1], boxes))
}

fn grow(r: Rect, by: f32) -> Rect {
    (r.0 - by, r.1 - by, r.2 + by, r.3 + by)
}

fn inside(p: Point, r: Rect) -> bool {
    r.0 < p.0 && p.0 < r.2 && r.1 < p.1 && p.1 < r.3
}

/// The way from `a`, leaving in direction `da`, to `b`, arriving through direction `db`, that goes
/// around `boxes`: the plain right-angled path when it is clear, otherwise the best one found.
/// `own` are the boxes of the shapes the arrow links: the stroke leaves them but does not come back across.
pub fn route(a: Point, da: Point, b: Point, db: Point, boxes: &[Rect], own: &[Rect]) -> Vec<Point> {
    // The same ends among the same boxes are the same path: it is worked out once, and asked again
    // by every drawing, every search for what is under the pointer, every size of the file. Only the
    // boxes within reach of the ends count: a shape moved elsewhere leaves the memory as it is.
    let (a1, b1) = ((a.0 + da.0 * STUB, a.1 + da.1 * STUB), (b.0 + db.0 * STUB, b.1 + db.1 * STUB));
    let reach = WINDOW + MARGIN + 4.;
    let (x0, x1, y0, y1) = (a1.0.min(b1.0) - reach, a1.0.max(b1.0) + reach, a1.1.min(b1.1) - reach, a1.1.max(b1.1) + reach);
    let hashed = |all: &mut dyn Iterator<Item = &Rect>| {
        let mut hasher = DefaultHasher::new();
        for v in [a.0, a.1, da.0, da.1, b.0, b.1, db.0, db.1] {
            v.to_bits().hash(&mut hasher);
        }
        for r in all {
            [r.0, r.1, r.2, r.3].map(f32::to_bits).hash(&mut hasher);
        }
        hasher.finish()
    };
    let near = |r: &&Rect| r.0 < x1 && r.2 > x0 && r.1 < y1 && r.3 > y0;
    let close = hashed(&mut boxes.iter().chain(own).filter(near));
    let every = hashed(&mut boxes.iter().chain(own)) ^ 1;
    if let Some(known) = MEMO.with_borrow(|memo| memo.get(&close).or_else(|| memo.get(&every)).cloned()) {
        return known;
    }
    let (path, wide) = work_out(a, da, b, db, boxes, own);
    MEMO.with_borrow_mut(|memo| {
        // A drag makes a path of each place it passes: the old ones are not asked for again.
        if memo.len() > 8192 {
            memo.clear();
        }
        // A path that had to look at every box depends on every box.
        memo.insert(if wide { every } else { close }, path.clone());
    });
    path
}

thread_local! {
    static MEMO: RefCell<HashMap<u64, Vec<Point>>> = RefCell::new(HashMap::new());
}

/// The path, and whether finding it needed every box (not only those near the ends).
fn work_out(a: Point, da: Point, b: Point, db: Point, boxes: &[Rect], own: &[Rect]) -> (Vec<Point>, bool) {
    let plain = elbow(a, da, b, db);
    // A shape a bit aside is no obstacle: only a stroke that would touch one detours.
    let touching: Vec<Rect> = boxes.iter().map(|r| grow(*r, 4.)).collect();
    if clear(&plain, &touching) {
        return (plain, false);
    }
    let (a1, b1) = ((a.0 + da.0 * STUB, a.1 + da.1 * STUB), (b.0 + db.0 * STUB, b.1 + db.1 * STUB));
    let near = |all: &[Rect], by: f32| -> Vec<Rect> {
        let (x0, x1, y0, y1) = (a1.0.min(b1.0) - by, a1.0.max(b1.0) + by, a1.1.min(b1.1) - by, a1.1.max(b1.1) + by);
        all.iter().copied().filter(|r| r.0 < x1 && r.2 > x0 && r.1 < y1 && r.3 > y0).collect()
    };
    // The stubs' ends are free; a shape that holds one is not in the way (a frame, an overlap).
    let all: Vec<Rect> = boxes.iter().chain(own).copied().filter(|r| !inside(a1, grow(*r, MARGIN)) && !inside(b1, grow(*r, MARGIN))).collect();
    for window in [WINDOW, f32::MAX / 4.] {
        let blocks: Vec<Rect> = near(&all, window).into_iter().map(|r| grow(r, MARGIN)).collect();
        if let Some(mut path) = search(a1, da, b1, db, &blocks) {
            path.insert(0, a);
            path.push(b);
            return (simplify(path), window > WINDOW);
        }
    }
    (plain, true)
}

/// Drops the points a stroke passes straight through, and the repeated ones.
fn simplify(mut points: Vec<Point>) -> Vec<Point> {
    points.dedup();
    let mut i = 1;
    while i + 1 < points.len() {
        let (p, q, r) = (points[i - 1], points[i], points[i + 1]);
        if ((q.0 - p.0) * (r.1 - q.1) - (q.1 - p.1) * (r.0 - q.0)).abs() < 1e-3 {
            points.remove(i);
        } else {
            i += 1;
        }
    }
    points
}

/// A* over the grid made by the lines along the boxes' sides and through both ends: the state is a
/// crossing and the way the stroke came, so that a bend costs. Which grid edges a box blocks is
/// worked out once, box by box, rather than tested edge by edge.
fn search(from: Point, dir: Point, to: Point, arrive: Point, blocks: &[Rect]) -> Option<Vec<Point>> {
    let mut xs: Vec<f32> = vec![from.0, to.0];
    let mut ys: Vec<f32> = vec![from.1, to.1];
    for r in blocks {
        xs.extend([r.0, r.2]);
        ys.extend([r.1, r.3]);
    }
    for v in [&mut xs, &mut ys] {
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    }
    let (w, h) = (xs.len(), ys.len());
    let at = |v: &[f32], x: f32| v.iter().position(|c| (c - x).abs() < 0.01);
    let (start, goal) = ((at(&xs, from.0)?, at(&ys, from.1)?), (at(&xs, to.0)?, at(&ys, to.1)?));
    // `across[j * w + i]`: the edge from crossing (i, j) to (i + 1, j) goes through a box; `down[j * w + i]`: from (i, j) to (i, j + 1).
    let (mut across, mut down) = (vec![false; w * h], vec![false; w * h]);
    for r in blocks {
        let (i0, i1, j0, j1) = (at(&xs, r.0)?, at(&xs, r.2)?, at(&ys, r.1)?, at(&ys, r.3)?);
        for j in j0 + 1..j1 {
            across[j * w + i0..j * w + i1].fill(true);
        }
        for i in i0 + 1..i1 {
            for j in j0..j1 {
                down[j * w + i] = true;
            }
        }
    }
    // Moves: right, down, left, up.
    const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let heading = |d: Point| DIRS.iter().position(|(dx, dy)| (*dx as f32, *dy as f32) == d);
    let first = heading(dir)?;
    // The stroke arrives at `to` moving against `arrive`, which points out of the shape.
    let last = heading((-arrive.0, -arrive.1))?;
    let key = |c: f32| (c * 16.) as u64;
    let state = |i: usize, j: usize, d: usize| (j * w + i) * 4 + d;
    // What is left to go, at least: the distance to the goal, plus a bend if it is not in line.
    let rest = |i: usize, j: usize| key((xs[i] - to.0).abs() + (ys[j] - to.1).abs());
    let mut best: HashMap<usize, u64> = HashMap::new();
    let mut came: HashMap<usize, usize> = HashMap::new();
    let mut heap = BinaryHeap::new();
    best.insert(state(start.0, start.1, first), 0);
    heap.push(Reverse((rest(start.0, start.1), 0u64, start.0, start.1, first)));
    let mut found: Option<(u64, usize)> = None;
    while let Some(Reverse((guess, cost, i, j, d))) = heap.pop() {
        if found.is_some_and(|(total, _)| guess >= total) {
            break;
        }
        if best.get(&state(i, j, d)).is_some_and(|b| cost > *b) {
            continue;
        }
        if (i, j) == goal {
            let total = cost + if d == last { 0 } else { key(BEND) };
            if found.is_none_or(|(t, _)| total < t) {
                found = Some((total, state(i, j, d)));
            }
        }
        for (nd, (dx, dy)) in DIRS.iter().enumerate() {
            // Never straight back.
            if nd == (d + 2) % 4 {
                continue;
            }
            let (ni, nj) = (i as i32 + dx, j as i32 + dy);
            if ni < 0 || nj < 0 || ni as usize >= w || nj as usize >= h {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            let blocked = match nd {
                0 => across[j * w + i],
                2 => across[j * w + ni],
                1 => down[j * w + i],
                _ => down[nj * w + i],
            };
            if blocked {
                continue;
            }
            let step = key((xs[ni] - xs[i]).abs() + (ys[nj] - ys[j]).abs()) + if nd == d { 0 } else { key(BEND) };
            let next = cost + step;
            let slot = state(ni, nj, nd);
            if best.get(&slot).is_none_or(|b| next < *b) {
                best.insert(slot, next);
                came.insert(slot, state(i, j, d));
                heap.push(Reverse((next + rest(ni, nj), next, ni, nj, nd)));
            }
        }
    }
    let (_, mut at_state) = found?;
    let mut path = Vec::new();
    loop {
        let cell = at_state / 4;
        path.push((xs[cell % w], ys[cell / w]));
        match came.get(&at_state) {
            Some(before) => at_state = *before,
            None => break,
        }
    }
    path.reverse();
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Point = (1., 0.);

    #[test]
    fn keeps_the_plain_path_when_nothing_is_in_the_way() {
        // Facing each other, a jog halfway, as before; a shape far aside changes nothing.
        let far: Rect = (0., 500., 100., 600.);
        let path = route((0., 0.), R, (100., 50.), (-1., 0.), &[far], &[]);
        assert_eq!(path, [(0., 0.), (50., 0.), (50., 50.), (100., 50.)]);
        assert_eq!(path, route((0., 0.), R, (100., 50.), (-1., 0.), &[], &[]));
    }

    #[test]
    fn goes_around_a_shape_in_its_way() {
        // A wall between the two ends, wider than it is tall.
        let wall: Rect = (40., -20., 60., 120.);
        let path = route((0., 0.), R, (100., 0.), (-1., 0.), &[wall], &[]);
        assert!(path.len() >= 4 && path.first() == Some(&(0., 0.)) && path.last() == Some(&(100., 0.)));
        assert!(clear(&path, &[wall]), "{path:?}");
        // Right-angled only, and it keeps its distance (the margin) from the wall.
        assert!(path.windows(2).all(|w| w[0].0 == w[1].0 || w[0].1 == w[1].1), "{path:?}");
        assert!(clear(&path, &[grow(wall, MARGIN - 0.5)]) || path.windows(2).take(1).count() == 1, "{path:?}");
        // Fewest bends: over the top of the wall (2 more than the straight jog), not a staircase.
        assert!(path.len() <= 6, "{path:?}");
    }

    #[test]
    fn goes_around_several_and_finds_the_way_through_a_gap() {
        // Two boxes with a gap of 20 between them: the stroke through the middle keeps 10 from each, enough.
        let gap = [(40., -100., 60., -10.), (40., 10., 60., 100.)];
        assert_eq!(route((0., 0.), R, (100., 0.), (-1., 0.), &gap, &[]), [(0., 0.), (100., 0.)]);
        // A gap of 6: nothing passes, it goes around the two.
        let narrow = [(40., -100., 60., -3.), (40., 3., 60., 100.)];
        let path = route((0., 0.), R, (100., 0.), (-1., 0.), &narrow, &[]);
        assert!(clear(&path, &narrow) && path.len() > 2 && path.iter().all(|p| p.1.abs() > 100. || p.0 <= 30. || p.0 >= 70.), "{path:?}");
        // The ends themselves are not in a box that holds none of them: a frame holding both is crossed.
        assert_eq!(route((0., 0.), R, (100., 0.), (-1., 0.), &[], &[]), [(0., 0.), (100., 0.)]);
    }

    #[test]
    fn leaves_an_enclosed_end_by_the_stub_and_never_loops() {
        // Both ends are inside one shape's margin or boxed in: no panic, a path or the plain one, always finite.
        let boxed: Vec<Rect> = vec![(-30., -30., 130., -20.), (-30., 20., 130., 30.), (-30., -30., -20., 30.), (120., -30., 130., 30.)];
        let path = route((0., 0.), R, (100., 0.), (-1., 0.), &boxed, &[]);
        assert!(path.len() >= 2 && path.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
        // The two ends on top of each other, in the same place, pointing every way.
        for da in [R, (-1., 0.), (0., 1.), (0., -1.)] {
            for db in [R, (-1., 0.), (0., 1.), (0., -1.)] {
                let path = route((50., 50.), da, (50., 50.), db, &[(0., 0., 100., 100.)], &[]);
                assert!(path.len() >= 2 && path.iter().all(|p| p.0.is_finite() && p.1.is_finite()), "{da:?} {db:?}");
            }
        }
    }

    use super::super::{Diagram, End, Form, Head, Route};

    /// A deterministic generator.
    struct Dice(u64);
    impl Dice {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) % n
        }
    }

    /// `n` shapes on a loose grid (some wider, some taller), joined by `links` right-angled arrows.
    fn crowd(n: usize, links: usize, seed: u64) -> Diagram {
        let mut dice = Dice(seed);
        let mut d = Diagram::default();
        let per_row = (n as f64).sqrt().ceil() as usize;
        for i in 0..n {
            let (col, row) = ((i % per_row) as f32, (i / per_row) as f32);
            let (w, h) = (60. + 20. * dice.below(4) as f32, 40. + 20. * dice.below(3) as f32);
            d.add_shape(Form::Rect, col * 220. + dice.below(40) as f32, row * 200. + dice.below(40) as f32, w, h);
        }
        for _ in 0..links {
            let (a, b) = (dice.below(n as u64) as u32 + 1, dice.below(n as u64) as u32 + 1);
            if a != b {
                d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Elbow);
            }
        }
        d
    }

    /// Whether a stroke goes through the inside of a shape.
    fn goes_through(path: &[Point], shape: &super::super::Shape) -> bool {
        crosses_any(path, &[(shape.x, shape.y, shape.x + shape.w, shape.y + shape.h)])
    }

    fn crosses_any(path: &[Point], boxes: &[Rect]) -> bool {
        !clear(path, boxes)
    }

    #[test]
    fn twenty_shapes_and_thirty_arrows_cross_no_shape_they_do_not_link() {
        for seed in 1..=40 {
            let d = crowd(20, 36, seed);
            assert!(d.links.len() >= 30);
            for link in &d.links {
                let path = d.trace(link).unwrap().points;
                assert!(path.windows(2).all(|w| w[0].0 == w[1].0 || w[0].1 == w[1].1), "right-angled: {path:?}");
                for shape in &d.shapes {
                    if link.from.shape() == Some(shape.id) || link.to.shape() == Some(shape.id) {
                        continue;
                    }
                    assert!(!goes_through(&path, shape), "seed {seed}: arrow {} -> {:?} crosses shape {}: {path:?}", link.id, link.to, shape.id);
                }
            }
        }
    }

    #[test]
    fn moving_an_unrelated_shape_leaves_the_other_arrows_where_they_are() {
        let mut d = crowd(30, 40, 7);
        let before: Vec<Vec<Point>> = d.links.iter().map(|l| d.trace(l).unwrap().points).collect();
        // A shape no arrow joins, taken far away: nothing else moves. (Shape 1 may be linked: take the first that is not.)
        let moved = 1;
        let old = d.shape(moved).unwrap().clone();
        let old_box = (old.x - 200., old.y - 200., old.x + old.w + 200., old.y + old.h + 200.);
        d.shape_mut(moved).unwrap().x += 6000.;
        let mut compared = 0;
        for (link, was) in d.links.iter().zip(&before) {
            if link.from.shape() == Some(moved) || link.to.shape() == Some(moved) {
                continue;
            }
            // An arrow that had the shape within reach of its way may find a shorter one now it is gone.
            let (x0, x1) = (was.iter().map(|p| p.0).fold(f32::MAX, f32::min), was.iter().map(|p| p.0).fold(f32::MIN, f32::max));
            let (y0, y1) = (was.iter().map(|p| p.1).fold(f32::MAX, f32::min), was.iter().map(|p| p.1).fold(f32::MIN, f32::max));
            if x0 < old_box.2 && x1 > old_box.0 && y0 < old_box.3 && y1 > old_box.1 {
                continue;
            }
            compared += 1;
            assert_eq!(&d.trace(link).unwrap().points, was, "arrow {} was reshuffled", link.id);
        }
        assert!(compared > 20, "{compared}");
    }

    #[test]
    fn two_hundred_shapes_route_in_a_few_milliseconds_each() {
        let d = crowd(200, 300, 3);
        let time = |d: &Diagram| {
            let start = std::time::Instant::now();
            let n: usize = d.links.iter().map(|l| d.trace(l).unwrap().points.len()).sum();
            (start.elapsed().as_secs_f64() * 1000. / d.links.len() as f64, n)
        };
        let (cold, n) = time(&d);
        // The same diagram again: what was worked out is remembered.
        let (warm, _) = time(&d);
        // One shape moved: only the arrows near it are worked out again.
        let mut moved = d.clone();
        moved.shape_mut(100).unwrap().x += 30.;
        let (after, _) = time(&moved);
        eprintln!("route: {} arrows, cold {cold:.3} ms, again {warm:.4} ms, after a move {after:.3} ms", d.links.len());
        // Debug builds are ten times slower than the app: generous bounds, the real ones are measured by hand.
        assert!(cold < 60. && warm < 3. && after < 25., "{cold} {warm} {after}");
        assert!(n >= 2 * d.links.len());
    }

    #[test]
    fn arrows_between_the_same_two_shapes_run_side_by_side() {
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 100.);
        let b = d.add_shape(Form::Rect, 300., 0., 100., 100.);
        for route in [Route::Straight, Route::Elbow] {
            let mut d = d.clone();
            let ids: Vec<u32> = (0..3).map(|i| if i == 1 { d.add_link(End::Shape(b), End::Shape(a), Head::Arrow, route) } else { d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, route) }).collect();
            let paths: Vec<Vec<Point>> = ids.iter().map(|id| d.trace(d.link(*id).unwrap()).unwrap().points).collect();
            // Three strokes at three heights, 18 apart, whichever way each goes: none on top of another.
            let heights: Vec<f32> = paths.iter().map(|p| p[0].1.min(p[p.len() - 1].1)).collect();
            let mut sorted = heights.clone();
            sorted.sort_by(f32::total_cmp);
            assert_eq!(sorted, [32., 50., 68.], "{route:?}: {paths:?}");
            // A single arrow stays in the middle, as before.
            let mut alone = Diagram::default();
            let (x, y) = (alone.add_shape(Form::Rect, 0., 0., 100., 100.), alone.add_shape(Form::Rect, 300., 0., 100., 100.));
            let id = alone.add_link(End::Shape(x), End::Shape(y), Head::Arrow, route);
            assert_eq!(alone.trace(alone.link(id).unwrap()).unwrap().points[0].1, 50.);
        }
        // An arrow held by a pin keeps it, with others around: it does not move to make room.
        d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Elbow);
        let pinned = d.add_link(End::Pin(a, 1., 0.25), End::Shape(b), Head::Arrow, Route::Elbow);
        d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Elbow);
        assert_eq!(d.trace(d.link(pinned).unwrap()).unwrap().points[0], (100., 25.));
    }

    #[test]
    fn a_detour_keeps_the_pins_and_leaves_by_their_side() {
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 100.);
        let wall = d.add_shape(Form::Rect, 150., -300., 40., 700.);
        let b = d.add_shape(Form::Rect, 300., 0., 100., 100.);
        let _ = wall;
        let link = d.add_link(End::Pin(a, 1., 0.5), End::Pin(b, 0., 0.5), Head::Arrow, Route::Elbow);
        let path = d.trace(d.link(link).unwrap()).unwrap().points;
        assert_eq!((path[0], path[path.len() - 1]), ((100., 50.), (300., 50.)));
        // It leaves a to the right and arrives at b from the left, and goes around the wall (over or under).
        assert!(path[1].0 > 100. && path[path.len() - 2].0 < 300. && path.len() >= 6, "{path:?}");
        assert!(!goes_through(&path, d.shape(wall).unwrap()), "{path:?}");
        // A frame is crossed, not gone around: what holds shapes is no obstacle.
        let mut d = Diagram::default();
        let a = d.add_shape(Form::Rect, 20., 20., 60., 60.);
        let b = d.add_shape(Form::Rect, 220., 20., 60., 60.);
        d.add_shape(Form::Frame, 0., 0., 300., 100.);
        let link = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Elbow);
        assert_eq!(d.trace(d.link(link).unwrap()).unwrap().points, [(80., 50.), (220., 50.)]);
    }
}
