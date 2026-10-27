//! How the window's panes are arranged: a tree of rows (side by side) and columns (stacked)
//! whose leaves are panes, each child with its share of its parent. Pure: sizes are fractions of
//! the space the panes take, the caller turns them into pixels.

/// Side by side (`Row`) or stacked (`Column`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Row,
    Column,
}

/// Where a pane is split, or which neighbour is wanted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

impl Side {
    pub fn axis(self) -> Axis {
        match self {
            Side::Left | Side::Right => Axis::Row,
            Side::Up | Side::Down => Axis::Column,
        }
    }

    /// The new pane comes before the old one (left of it, above it).
    fn before(self) -> bool {
        matches!(self, Side::Left | Side::Up)
    }
}

/// A rectangle in the space of the panes, from (0, 0) to (1, 1).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// A pane (its index), or a row or column of children with their shares (summing to 1). A split
/// has two children at least, and never a child split along its own axis: that one is flattened
/// into it.
#[derive(Clone, PartialEq, Debug)]
pub enum Node {
    Pane(usize),
    Split(Axis, Vec<(f32, Node)>),
}

const EPS: f32 = 1e-4;

impl Default for Node {
    fn default() -> Self {
        Node::Pane(0)
    }
}

impl Node {
    /// The panes, in reading order (left to right, top to bottom within each split).
    pub fn panes(&self) -> Vec<usize> {
        match self {
            Node::Pane(pane) => vec![*pane],
            Node::Split(_, children) => children.iter().flat_map(|(_, child)| child.panes()).collect(),
        }
    }

    /// Splits `pane` on `side`: `new` takes half of its place. Inside a split of the same axis, the
    /// new pane becomes a sibling; otherwise the pane becomes a split of the two.
    pub fn split(&mut self, pane: usize, side: Side, new: usize) -> bool {
        let axis = side.axis();
        if let Node::Split(own, children) = self
            && *own == axis
            && let Some(at) = children.iter().position(|(_, child)| *child == Node::Pane(pane))
        {
            let half = children[at].0 / 2.;
            children[at].0 = half;
            children.insert(if side.before() { at } else { at + 1 }, (half, Node::Pane(new)));
            return true;
        }
        match self {
            Node::Pane(own) if *own == pane => {
                let pair = [(0.5, Node::Pane(new)), (0.5, Node::Pane(pane))];
                let pair = if side.before() { pair.to_vec() } else { pair.into_iter().rev().collect() };
                *self = Node::Split(axis, pair);
                true
            }
            Node::Pane(_) => false,
            Node::Split(_, children) => children.iter_mut().any(|(_, child)| child.split(pane, side, new)),
        }
    }

    /// Takes `pane` out: its siblings share its place in proportion, a split left with one child
    /// becomes that child, and the panes after it are renumbered down by one.
    pub fn remove(&mut self, pane: usize) {
        self.cut(pane);
        self.renumber(pane);
        self.tidy();
    }

    fn cut(&mut self, pane: usize) -> bool {
        let Node::Split(_, children) = self else { return false };
        if let Some(at) = children.iter().position(|(_, child)| *child == Node::Pane(pane)) {
            let (gone, _) = children.remove(at);
            let left: f32 = children.iter().map(|(share, _)| share).sum();
            for (share, _) in children.iter_mut() {
                *share += gone * *share / left.max(EPS);
            }
            return true;
        }
        children.iter_mut().any(|(_, child)| child.cut(pane))
    }

    fn renumber(&mut self, gone: usize) {
        match self {
            Node::Pane(pane) if *pane > gone => *pane -= 1,
            Node::Pane(_) => {}
            Node::Split(_, children) => children.iter_mut().for_each(|(_, child)| child.renumber(gone)),
        }
    }

    /// A split of one child becomes the child; a child split along its parent's axis is
    /// flattened into the parent, its shares scaled.
    fn tidy(&mut self) {
        if let Node::Split(axis, children) = self {
            children.iter_mut().for_each(|(_, child)| child.tidy());
            let axis = *axis;
            let mut flat = Vec::with_capacity(children.len());
            for (share, child) in children.drain(..) {
                match child {
                    Node::Split(inner, grand) if inner == axis => flat.extend(grand.into_iter().map(|(s, g)| (s * share, g))),
                    child => flat.push((share, child)),
                }
            }
            *children = flat;
            if children.len() == 1 {
                *self = children.pop().unwrap().1;
            }
        }
    }

    /// Where each pane is, in the space of the panes.
    pub fn rects(&self) -> Vec<(usize, Rect)> {
        let mut out = Vec::new();
        self.place(Rect { x: 0., y: 0., w: 1., h: 1. }, &mut out);
        out
    }

    fn place(&self, at: Rect, out: &mut Vec<(usize, Rect)>) {
        match self {
            Node::Pane(pane) => out.push((*pane, at)),
            Node::Split(axis, children) => {
                let mut from = 0.;
                for (share, child) in children {
                    let part = match axis {
                        Axis::Row => Rect { x: at.x + from * at.w, w: share * at.w, ..at },
                        Axis::Column => Rect { y: at.y + from * at.h, h: share * at.h, ..at },
                    };
                    child.place(part, out);
                    from += share;
                }
            }
        }
    }

    /// The rectangle of the node at `path` (child indices from the root).
    pub fn rect_of(&self, path: &[usize]) -> Rect {
        let (mut node, mut at) = (self, Rect { x: 0., y: 0., w: 1., h: 1. });
        for &step in path {
            let Node::Split(axis, children) = node else { break };
            let Some((share, child)) = children.get(step) else { break };
            let from: f32 = children[..step].iter().map(|(s, _)| s).sum();
            at = match axis {
                Axis::Row => Rect { x: at.x + from * at.w, w: share * at.w, ..at },
                Axis::Column => Rect { y: at.y + from * at.h, h: share * at.h, ..at },
            };
            node = child;
        }
        at
    }

    /// The pane on `side` of `pane`: among those whose edge touches it there, the one facing the
    /// middle of that edge, else the one that faces it the most.
    pub fn neighbour(&self, pane: usize, side: Side) -> Option<usize> {
        let rects = self.rects();
        let (_, me) = *rects.iter().find(|(p, _)| *p == pane)?;
        let touches = |r: &Rect| match side {
            Side::Left => (r.right() - me.x).abs() < EPS,
            Side::Right => (r.x - me.right()).abs() < EPS,
            Side::Up => (r.bottom() - me.y).abs() < EPS,
            Side::Down => (r.y - me.bottom()).abs() < EPS,
        };
        // How far the candidate faces this pane, along the edge.
        let facing = |r: &Rect| match side.axis() {
            Axis::Row => r.bottom().min(me.bottom()) - r.y.max(me.y),
            Axis::Column => r.right().min(me.right()) - r.x.max(me.x),
        };
        // Just before the middle: when it falls on a border, the pane above (or left) wins.
        let middle = |r: &Rect| match side.axis() {
            Axis::Row => (r.y..r.bottom()).contains(&(me.y + me.h / 2. - EPS)),
            Axis::Column => (r.x..r.right()).contains(&(me.x + me.w / 2. - EPS)),
        };
        let near: Vec<&(usize, Rect)> = rects.iter().filter(|(p, r)| *p != pane && touches(r) && facing(r) > EPS).collect();
        near.iter()
            .find(|(_, r)| middle(r))
            .or_else(|| near.iter().max_by(|a, b| facing(&a.1).total_cmp(&facing(&b.1))))
            .map(|(p, _)| *p)
    }

    /// Moves the border after child `border` of the split at `path` to `pos`, a fraction of that
    /// split's size; each of the two children keeps `min` of it at least.
    pub fn drag(&mut self, path: &[usize], border: usize, pos: f32, min: f32) {
        let Some(Node::Split(_, children)) = self.at_mut(path) else { return };
        if border + 1 >= children.len() {
            return;
        }
        let start: f32 = children[..border].iter().map(|(s, _)| s).sum();
        let end = start + children[border].0 + children[border + 1].0;
        let min = min.min((end - start) / 2.);
        let pos = pos.clamp(start + min, end - min);
        children[border].0 = pos - start;
        children[border + 1].0 = end - pos;
    }

    /// The node at `path`.
    pub fn at(&self, path: &[usize]) -> Option<&Node> {
        let mut node = self;
        for &step in path {
            let Node::Split(_, children) = node else { return None };
            node = &children.get(step)?.1;
        }
        Some(node)
    }

    pub fn at_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        let mut node = self;
        for &step in path {
            let Node::Split(_, children) = node else { return None };
            node = &mut children.get_mut(step)?.1;
        }
        Some(node)
    }

    /// Every split shares its space evenly.
    pub fn even(&mut self) {
        if let Node::Split(_, children) = self {
            let share = 1. / children.len() as f32;
            for (s, child) in children.iter_mut() {
                *s = share;
                child.even();
            }
        }
    }

    /// As saved: `row(0.5 0, 0.5 col(0.5 1, 0.5 2))`.
    pub fn write(&self) -> String {
        match self {
            Node::Pane(pane) => pane.to_string(),
            Node::Split(axis, children) => {
                let name = if *axis == Axis::Row { "row" } else { "col" };
                let inner: Vec<String> = children.iter().map(|(share, child)| format!("{share} {}", child.write())).collect();
                format!("{name}({})", inner.join(", "))
            }
        }
    }

    /// Reads what `write` wrote, for `count` panes: `None` unless each pane appears exactly once.
    /// Shares are made positive and to sum to 1, and the tree is tidied.
    pub fn read(text: &str, count: usize) -> Option<Node> {
        let (mut node, rest) = parse(text.trim())?;
        if !rest.trim().is_empty() {
            return None;
        }
        let mut panes = node.panes();
        panes.sort_unstable();
        if panes != (0..count).collect::<Vec<_>>() {
            return None;
        }
        node.normalize();
        node.tidy();
        Some(node)
    }

    fn normalize(&mut self) {
        if let Node::Split(_, children) = self {
            for (share, child) in children.iter_mut() {
                *share = if share.is_finite() { share.clamp(0.05, 1.) } else { 0.5 };
                child.normalize();
            }
            let sum: f32 = children.iter().map(|(s, _)| s).sum();
            children.iter_mut().for_each(|(s, _)| *s /= sum);
        }
    }

    /// A row of `count` panes, the first taking `first` of the width (the layout before trees).
    pub fn row(count: usize, first: f32) -> Node {
        match count {
            0 | 1 => Node::Pane(0),
            _ => {
                let rest = (1. - first) / (count - 1) as f32;
                Node::Split(Axis::Row, (0..count).map(|p| (if p == 0 { first } else { rest }, Node::Pane(p))).collect())
            }
        }
    }
}

fn parse(text: &str) -> Option<(Node, &str)> {
    let text = text.trim_start();
    for (name, axis) in [("row(", Axis::Row), ("col(", Axis::Column)] {
        if let Some(mut rest) = text.strip_prefix(name) {
            let mut children = Vec::new();
            loop {
                rest = rest.trim_start();
                let end = rest.find(' ')?;
                let share: f32 = rest[..end].parse().ok()?;
                let (child, after) = parse(&rest[end..])?;
                children.push((share, child));
                let after = after.trim_start();
                if let Some(after) = after.strip_prefix(',') {
                    rest = after;
                } else {
                    let after = after.strip_prefix(')')?;
                    return (children.len() >= 2).then_some((Node::Split(axis, children), after));
                }
            }
        }
    }
    let end = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    Some((Node::Pane(text[..end].parse().ok()?), &text[end..]))
}

/// Where something dropped on a pane goes: into it, or into a new pane split off on a side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Center,
    Edge(Side),
}

/// The zone under the pointer at (`x`, `y`) in a pane of `w` × `h`: the nearest edge when the
/// pointer is in its band (a quarter of the pane, at most `band`), the centre otherwise.
pub fn zone(x: f32, y: f32, w: f32, h: f32, band: f32) -> Zone {
    if w <= 0. || h <= 0. {
        return Zone::Center;
    }
    let (bx, by) = ((w / 4.).min(band), (h / 4.).min(band));
    let near = [(x / bx, Side::Left), ((w - x) / bx, Side::Right), (y / by, Side::Up), ((h - y) / by, Side::Down)];
    match near.into_iter().filter(|(d, _)| *d < 1.).min_by(|a, b| a.0.total_cmp(&b.0)) {
        Some((_, side)) => Zone::Edge(side),
        None => Zone::Center,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Axis::*;

    fn pane(p: usize) -> Node {
        Node::Pane(p)
    }

    /// The invariants every operation keeps, for `count` panes.
    fn valid(node: &Node, count: usize) -> bool {
        fn ok(node: &Node, parent: Option<Axis>) -> bool {
            match node {
                Node::Pane(_) => true,
                Node::Split(axis, children) => {
                    let sum: f32 = children.iter().map(|(s, _)| s).sum();
                    children.len() >= 2
                        && Some(*axis) != parent
                        && (sum - 1.).abs() < 1e-3
                        && children.iter().all(|(s, child)| *s > 0. && ok(child, Some(*axis)))
                }
            }
        }
        let mut panes = node.panes();
        panes.sort_unstable();
        ok(node, None) && panes == (0..count).collect::<Vec<_>>()
    }

    #[test]
    fn split_beside_or_below() {
        let mut tree = pane(0);
        assert!(tree.split(0, Side::Right, 1));
        assert_eq!(tree, Node::Split(Row, vec![(0.5, pane(0)), (0.5, pane(1))]));
        // Same axis: a sibling, sharing the half of the one split.
        tree.split(1, Side::Left, 2);
        assert_eq!(tree, Node::Split(Row, vec![(0.5, pane(0)), (0.25, pane(2)), (0.25, pane(1))]));
        // Across: the pane becomes a column.
        tree.split(0, Side::Down, 3);
        assert_eq!(tree.write(), "row(0.5 col(0.5 0, 0.5 3), 0.25 2, 0.25 1)");
        assert!(!tree.split(9, Side::Up, 4), "an unknown pane is not split");
        assert!(valid(&tree, 4));
        assert_eq!(tree.panes(), vec![0, 3, 2, 1]);
    }

    #[test]
    fn removing_gives_the_place_back_and_renumbers() {
        let mut tree = Node::read("row(0.5 col(0.5 0, 0.5 3), 0.25 2, 0.25 1)", 4).unwrap();
        tree.remove(2);
        // Its quarter goes to the others in proportion; 3 becomes 2.
        assert_eq!(tree.write(), "row(0.6666667 col(0.5 0, 0.5 2), 0.33333334 1)");
        tree.remove(2);
        assert_eq!(tree.write(), "row(0.6666667 0, 0.33333334 1)");
        tree.remove(0);
        assert_eq!(tree, pane(0));
        tree.remove(0);
        assert_eq!(tree, pane(0), "the last pane stays");
        // A column left with one child inside a row of the same… flattened back into the row.
        let mut tree = Node::read("row(0.5 0, 0.5 col(0.5 1, 0.5 row(0.5 2, 0.5 3)))", 4).unwrap();
        tree.remove(1);
        assert_eq!(tree.write(), "row(0.5 0, 0.25 1, 0.25 2)");
    }

    #[test]
    fn rects_and_neighbours() {
        // 0 | 1
        // --+--
        // 2 | 1
        let tree = Node::read("row(0.5 col(0.5 0, 0.5 2), 0.5 1)", 3).unwrap();
        let rects = tree.rects();
        assert_eq!(rects[1], (2, Rect { x: 0., y: 0.5, w: 0.5, h: 0.5 }));
        assert_eq!(tree.neighbour(0, Side::Right), Some(1));
        assert_eq!(tree.neighbour(0, Side::Down), Some(2));
        assert_eq!(tree.neighbour(1, Side::Left), Some(0), "two face it equally: the upper one wins");
        assert_eq!(tree.neighbour(2, Side::Up), Some(0));
        assert_eq!(tree.neighbour(0, Side::Left), None);
        assert_eq!(tree.neighbour(1, Side::Up), None);
        assert_eq!(tree.rect_of(&[0]), Rect { x: 0., y: 0., w: 0.5, h: 1. });
        assert_eq!(tree.rect_of(&[0, 1]), Rect { x: 0., y: 0.5, w: 0.5, h: 0.5 });
        // 0 above a wide 1: from 1, up goes to the one over the middle of its edge.
        let tree = Node::read("col(0.5 row(0.3 0, 0.7 2), 0.5 1)", 3).unwrap();
        assert_eq!(tree.neighbour(1, Side::Up), Some(2));
    }

    fn shares(tree: &Node) -> Vec<f32> {
        match tree {
            Node::Split(_, children) => children.iter().map(|(s, _)| (s * 1000.).round() / 1000.).collect(),
            Node::Pane(_) => vec![1.],
        }
    }

    #[test]
    fn dragging_a_border_keeps_the_minimum() {
        let mut tree = Node::read("row(0.25 0, 0.25 1, 0.5 2)", 3).unwrap();
        tree.drag(&[], 1, 0.75, 0.1);
        assert_eq!(shares(&tree), vec![0.25, 0.5, 0.25]);
        tree.drag(&[], 0, -3., 0.1);
        assert_eq!(shares(&tree), vec![0.1, 0.65, 0.25]);
        tree.drag(&[], 1, 2., 0.1);
        assert_eq!(shares(&tree), vec![0.1, 0.8, 0.1]);
        // Too small for both minimums: the border stays in the middle of the two.
        tree.drag(&[], 0, 0.9, 0.5);
        assert_eq!(shares(&tree), vec![0.45, 0.45, 0.1]);
        // Borders and paths that do not exist change nothing.
        tree.drag(&[], 5, 0.5, 0.1);
        tree.drag(&[7], 0, 0.5, 0.1);
        assert!(valid(&tree, 3));
        tree.even();
        assert_eq!(shares(&tree), vec![0.333, 0.333, 0.333]);
    }

    #[test]
    fn reading_refuses_what_does_not_fit() {
        assert_eq!(Node::read("0", 1), Some(pane(0)));
        assert_eq!(Node::read("row(0.4 0, 0.6 1)", 2).unwrap().write(), "row(0.4 0, 0.6 1)");
        for bad in ["", "row(", "row(0.5 0)", "row(0.5 0, 0.5 0)", "row(0.5 0, 0.5 2)", "col(0.5 0; 0.5 1)", "row(a 0, 0.5 1)", "row(0.5 0, 0.5 1) x", "1"] {
            assert_eq!(Node::read(bad, 2), None, "{bad}");
        }
        // Shares out of range or not summing to 1 are brought back.
        let tree = Node::read("row(7 0, NaN 1)", 2).unwrap();
        assert!(valid(&tree, 2), "{}", tree.write());
        // Nested along the same axis: flattened on reading.
        assert_eq!(Node::read("row(0.5 0, 0.5 row(0.5 1, 0.5 2))", 3).unwrap().write(), "row(0.5 0, 0.25 1, 0.25 2)");
        assert_eq!(Node::row(2, 0.4).write(), "row(0.4 0, 0.6 1)");
        assert_eq!(Node::row(1, 0.4), pane(0));
    }

    /// Any sequence of splits, removals, drags and evens keeps a valid tree.
    #[test]
    fn random_edits_keep_a_valid_tree() {
        let mut seed = 0x9E3779B97F4A7C15u64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let sides = [Side::Left, Side::Right, Side::Up, Side::Down];
        for _ in 0..200 {
            let (mut tree, mut count) = (pane(0), 1);
            for _ in 0..60 {
                match next(5) {
                    0 | 1 => {
                        assert!(tree.split(next(count), sides[next(4)], count));
                        count += 1;
                    }
                    2 if count > 1 => {
                        tree.remove(next(count));
                        count -= 1;
                    }
                    3 => {
                        let path: Vec<usize> = (0..next(3)).map(|_| next(3)).collect();
                        tree.drag(&path, next(3), next(100) as f32 / 100., 0.05);
                    }
                    _ => tree.even(),
                }
                assert!(valid(&tree, count), "{}", tree.write());
                // What is saved reads back to the same panes in the same places.
                let back = Node::read(&tree.write(), count).expect("saved tree reads back");
                assert!(valid(&back, count) && back.panes() == tree.panes(), "{}", tree.write());
                let area: f32 = tree.rects().iter().map(|(_, r)| r.w * r.h).sum();
                assert!((area - 1.).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn zones_of_a_pane() {
        let z = |x, y| zone(x, y, 800., 400., 80.);
        assert_eq!(z(400., 200.), Zone::Center);
        assert_eq!(z(10., 200.), Zone::Edge(Side::Left));
        assert_eq!(z(79., 200.), Zone::Edge(Side::Left));
        assert_eq!(z(81., 200.), Zone::Center);
        assert_eq!(z(795., 200.), Zone::Edge(Side::Right));
        assert_eq!(z(400., 5.), Zone::Edge(Side::Up));
        assert_eq!(z(400., 399.), Zone::Edge(Side::Down));
        // In a corner, the nearer edge for its band.
        assert_eq!(z(5., 60.), Zone::Edge(Side::Left));
        assert_eq!(z(60., 5.), Zone::Edge(Side::Up));
        // A small pane: the band is a quarter of it, never the whole.
        let small = |x, y| zone(x, y, 200., 100., 80.);
        assert_eq!(small(100., 50.), Zone::Center);
        assert_eq!(small(49., 50.), Zone::Edge(Side::Left));
        assert_eq!(small(51., 50.), Zone::Center);
        assert_eq!(small(100., 24.), Zone::Edge(Side::Up));
        // Outside or degenerate: an edge, never a panic.
        assert_eq!(zone(-5., 50., 200., 100., 80.), Zone::Edge(Side::Left));
        assert_eq!(zone(0., 0., 0., 0., 80.), Zone::Center);
    }
}
