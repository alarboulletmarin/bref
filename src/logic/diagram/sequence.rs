//! Sequence diagrams: the layout that lifelines and messages need and a free drawing does not. A
//! message is an ordinary arrow whose end is a lifeline (`End::Life`); its height is not stored but
//! comes from its place in the list of messages, so that moving one is reordering, inserting one
//! pushes the others down, and lifelines keep the places they were given. Activation bars and the
//! cross of a destroyed lifeline follow from the messages.

use super::*;

/// Height of the head of a lifeline, the space from the heads to the first message, between two
/// messages, and below the last one.
pub const HEAD: f32 = 36.;
const FIRST: f32 = 36.;
pub const STEP: f32 = 44.;
const TAIL: f32 = 44.;
/// Half the size of the cross of a destroyed lifeline.
const CROSS: f32 = 14.;
/// Width of an activation bar, and how far each nested one is moved over the one under it.
const BAR: f32 = 10.;
const NEST: f32 = 5.;
/// Size of the loop of a message to itself.
const LOOP: (f32, f32) = (34., 22.);

/// What a message is, from how its arrow looks: a call that waits (filled triangle), one that does
/// not (open arrow), the answer (dashed), the one that creates a lifeline or destroys it, and one
/// that comes from nowhere (a dot at its start) or goes nowhere (a dot at its end).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Message {
    Sync,
    Async,
    Reply,
    Create,
    Destroy,
    Found,
    Lost,
}

impl Message {
    pub fn of(link: &Link) -> Message {
        let stereotype = uml::stereotype(link.text.lines().next().unwrap_or(""));
        match (link.start, link.end, link.dashed) {
            (Head::Dot, ..) => Message::Found,
            (_, Head::Dot, _) => Message::Lost,
            (_, Head::TriangleFull, _) if stereotype == Some("destroy") => Message::Destroy,
            (_, Head::TriangleFull, _) => Message::Sync,
            (_, _, true) if stereotype == Some("create") => Message::Create,
            (_, _, true) => Message::Reply,
            _ => Message::Async,
        }
    }
}

/// An activation bar: a lifeline busy, from `y0` to `y1`, `depth` bars deep.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bar {
    pub life: u32,
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
    pub depth: usize,
}

impl Bar {
    pub fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y0, BAR, self.y1 - self.y0)
    }
}

/// What the messages add to the drawing: the activation bars, and the crosses of destroyed lifelines.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Marks {
    pub bars: Vec<Bar>,
    pub crosses: Vec<(f32, f32)>,
}

impl Diagram {
    /// The messages, in order: the arrows that have a lifeline at an end.
    pub fn messages(&self) -> Vec<u32> {
        self.links.iter().filter(|l| matches!(l.from, End::Life(_)) || matches!(l.to, End::Life(_))).map(|l| l.id).collect()
    }

    fn lifelines(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter().filter(|s| s.form == Form::Lifeline)
    }

    /// The lifelines a message creates: their head is lower than the others, where it arrives.
    fn created(&self) -> Vec<u32> {
        self.links.iter().filter(|l| Message::of(l) == Message::Create).filter_map(|l| if let End::Life(id) = l.to { Some(id) } else { None }).collect()
    }

    /// The line the heads of the lifelines end at, where messages begin.
    fn heads_end(&self) -> f32 {
        let made = self.created();
        self.lifelines().filter(|s| !made.contains(&s.id)).map(|s| s.y).fold(f32::MAX, f32::min).min(f32::MAX / 2.) + HEAD
    }

    /// The height of the message in place `rank`.
    pub fn rank_y(&self, rank: usize) -> f32 {
        self.heads_end() + FIRST + STEP * rank as f32
    }

    /// The height of message `id`.
    pub fn message_y(&self, id: u32) -> Option<f32> {
        self.messages().iter().position(|m| *m == id).map(|rank| self.rank_y(rank))
    }

    /// The place a message dragged to this height takes among the others.
    pub fn rank_at(&self, y: f32) -> usize {
        let last = self.messages().len().saturating_sub(1);
        (((y - self.rank_y(0)) / STEP).round().max(0.) as usize).min(last)
    }

    /// Puts message `id` in place `to` among the messages; `false` if it already is there.
    pub fn reorder(&mut self, id: u32, to: usize) -> bool {
        let order = self.messages();
        let (Some(from), true) = (order.iter().position(|m| *m == id), !order.is_empty()) else { return false };
        let to = to.min(order.len() - 1);
        if from == to {
            return false;
        }
        let Some(at) = self.links.iter().position(|l| l.id == id) else { return false };
        let link = self.links.remove(at);
        let Some(target) = self.links.iter().position(|l| l.id == order[to]) else { return false };
        self.links.insert(if to > from { target + 1 } else { target }, link);
        true
    }

    /// The bars and crosses the messages make: a call that waits opens a bar on the lifeline it
    /// reaches, closed by the answer that comes back from it to the caller; a destroy message closes
    /// what is open and puts a cross; a bar never answered stays open to the last message of its lifeline.
    pub fn marks(&self) -> Marks {
        let mut marks = Marks::default();
        let mut open: Vec<(u32, f32, Option<u32>)> = Vec::new();
        let mut last: Vec<(u32, f32)> = Vec::new();
        let touch = |last: &mut Vec<(u32, f32)>, life: u32, y: f32| match last.iter_mut().find(|(l, _)| *l == life) {
            Some(entry) => entry.1 = entry.1.max(y),
            None => last.push((life, y)),
        };
        let life = |end: End| if let End::Life(id) = end { Some(id) } else { None };
        let depth = |open: &[(u32, f32, Option<u32>)], on: u32| open.iter().filter(|(l, ..)| *l == on).count();
        let x_of = |on: u32, depth: usize| self.shape(on).map_or(0., |s| s.x + s.w / 2. - BAR / 2. + NEST * depth as f32);
        for (rank, id) in self.messages().into_iter().enumerate() {
            let Some(link) = self.link(id) else { continue };
            let y = self.rank_y(rank);
            let (from, to) = (life(link.from), life(link.to));
            for on in [from, to].into_iter().flatten() {
                touch(&mut last, on, y);
            }
            match Message::of(link) {
                // A call that waits opens a bar where it arrives (a call to oneself, a bar over its own).
                Message::Sync | Message::Create => {
                    if let Some(on) = to {
                        if from == Some(on) {
                            // A call to oneself lasts as long as its loop, unless an answer comes.
                            marks.bars.push(Bar { life: on, x: x_of(on, depth(&open, on)), y0: y, y1: y + LOOP.1, depth: depth(&open, on) });
                        } else {
                            open.push((on, y, from));
                        }
                    }
                }
                Message::Reply => {
                    if let Some(on) = from
                        && let Some(at) = open.iter().rposition(|(l, _, c)| *l == on && *c == to)
                    {
                        let (_, y0, _) = open.remove(at);
                        marks.bars.push(Bar { life: on, x: x_of(on, depth(&open, on)), y0, y1: y, depth: depth(&open, on) });
                    }
                }
                Message::Destroy => {
                    if let Some(on) = to {
                        // What was open there ends at the cross.
                        while let Some(at) = open.iter().rposition(|(l, ..)| *l == on) {
                            let (_, y0, _) = open.remove(at);
                            marks.bars.push(Bar { life: on, x: x_of(on, depth(&open, on)), y0, y1: y, depth: depth(&open, on) });
                        }
                        if let Some(shape) = self.shape(on) {
                            marks.crosses.push((shape.x + shape.w / 2., y));
                        }
                    }
                }
                _ => {}
            }
        }
        // What was never answered lasts to the last message of its lifeline, and a little more.
        while let Some((on, y0, _)) = open.pop() {
            let y1 = last.iter().find(|(l, _)| *l == on).map_or(y0, |(_, y)| *y) + 14.;
            marks.bars.push(Bar { life: on, x: x_of(on, depth(&open, on)), y0, y1, depth: depth(&open, on) });
        }
        marks.bars.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.y0.total_cmp(&b.y0)));
        marks
    }

    /// The way of a message: straight across at its height from one lifeline's bar to the other's,
    /// or a loop on the right of its lifeline when it is to itself. A free end (found or lost) keeps its place across.
    pub(super) fn message_path(&self, link: &Link) -> Option<Vec<(f32, f32)>> {
        let life = |end: End| if let End::Life(id) = end { Some(id) } else { None };
        if life(link.from).is_none() && life(link.to).is_none() {
            return None;
        }
        let y = self.message_y(link.id)?;
        let marks = self.marks();
        // The middle of what the end is on, and which side of it the other end is.
        let across = |end: End| match end {
            End::Life(id) => self.shape(id).map(|s| s.x + s.w / 2.),
            End::Point(x, _) => Some(x),
            other => self.shape(other.shape()?).map(|s| s.center().0),
        };
        let edge = |on: u32, toward: f32, created: bool| -> Option<f32> {
            let shape = self.shape(on)?;
            let middle = shape.x + shape.w / 2.;
            // A lifeline that is created is reached at the edge of its head.
            if created {
                return Some(if toward < middle { shape.x } else { shape.x + shape.w });
            }
            let bar = marks.bars.iter().filter(|b| b.life == on && b.y0 - 0.5 <= y && y <= b.y1 + 0.5).max_by_key(|b| b.depth);
            let (left, right) = bar.map_or((middle, middle), |b| (b.x, b.x + BAR));
            Some(if toward < middle { left } else { right })
        };
        if let (Some(a), Some(b)) = (life(link.from), life(link.to))
            && a == b
        {
            let x = edge(a, f32::MAX, false)?;
            return Some(vec![(x, y), (x + LOOP.0, y), (x + LOOP.0, y + LOOP.1), (x, y + LOOP.1)]);
        }
        let creates = Message::of(link) == Message::Create;
        let (xa, xb) = (across(link.from)?, across(link.to)?);
        let start = match link.from {
            End::Life(id) => edge(id, xb, false)?,
            _ => xa,
        };
        let end = match link.to {
            End::Life(id) => edge(id, xa, creates)?,
            _ => xb,
        };
        Some(vec![(start, y), (end, y)])
    }

    /// Lays the lifelines out as the messages ask: the heads of those not created on one line, those
    /// that are created where their message arrives, each line long enough for its last message, or
    /// its destruction, and a margin.
    pub fn fit_sequence(&mut self) {
        let heads = self.heads_end() - HEAD;
        let ids: Vec<u32> = self.lifelines().map(|s| s.id).collect();
        if ids.is_empty() {
            return;
        }
        let made = self.created();
        let order = self.messages();
        let bottom = if order.is_empty() { 0. } else { self.rank_y(order.len() - 1) };
        let mut ends: Vec<(u32, f32)> = Vec::new();
        for (rank, id) in order.iter().enumerate() {
            let y = self.rank_y(rank);
            let Some(link) = self.link(*id) else { continue };
            match (Message::of(link), link.to) {
                (Message::Create, End::Life(on)) => {
                    if let Some(shape) = self.shape_mut(on) {
                        shape.y = y - HEAD / 2.;
                    }
                }
                // A destroyed lifeline ends at its cross.
                (Message::Destroy, End::Life(on)) => ends.push((on, y + CROSS)),
                _ => {}
            }
        }
        for id in ids {
            let end = ends.iter().find(|(l, _)| *l == id).map(|(_, y)| *y);
            let Some(shape) = self.shape_mut(id) else { continue };
            if !made.contains(&id) {
                shape.y = heads;
            }
            // All the others go down to the last message, and a margin.
            shape.h = (end.unwrap_or(bottom + TAIL) - shape.y).max(HEAD + 60.);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` lifelines side by side, and the arrows between them, each as `(from, to, look)`.
    fn talk(lives: usize, say: &[(usize, usize, Message)]) -> (Diagram, Vec<u32>, Vec<u32>) {
        let mut d = Diagram::default();
        let lifes: Vec<u32> = (0..lives).map(|i| d.add_shape(Form::Lifeline, 100. + 180. * i as f32, 20., 100., 200.)).collect();
        let links: Vec<u32> = say
            .iter()
            .map(|(a, b, kind)| {
                let id = d.add_link(End::Life(lifes[*a]), End::Life(lifes[*b]), Head::None, Route::Straight);
                let link = d.link_mut(id).unwrap();
                let (start, end, dashed) = match kind {
                    Message::Sync | Message::Destroy => (Head::None, Head::TriangleFull, false),
                    Message::Async | Message::Found | Message::Lost => (Head::None, Head::Arrow, false),
                    Message::Reply | Message::Create => (Head::None, Head::Arrow, true),
                };
                (link.start, link.end, link.dashed) = (start, end, dashed);
                match kind {
                    Message::Create => link.text = "«create»".into(),
                    Message::Destroy => link.text = "«destroy»".into(),
                    _ => {}
                }
                id
            })
            .collect();
        d.fit_sequence();
        (d, lifes, links)
    }

    #[test]
    fn a_message_is_placed_by_its_order() {
        let (mut d, lives, links) = talk(3, &[(0, 1, Message::Sync), (1, 2, Message::Sync), (2, 1, Message::Reply), (1, 0, Message::Reply)]);
        // Under the heads (20 + 36), then one step for each, whatever was drawn where.
        let ys: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        assert_eq!(ys, [92., 136., 180., 224.]);
        // It goes straight across, from lifeline to lifeline, at that height.
        let path = d.trace(d.link(links[0]).unwrap()).unwrap().points;
        assert_eq!(path.iter().map(|p| p.1).collect::<Vec<_>>(), [92., 92.]);
        // A message put in the middle pushes the others down by one step; the lifelines stay where they are.
        let x: Vec<f32> = lives.iter().map(|id| d.shape(*id).unwrap().x).collect();
        let extra = d.add_link(End::Life(lives[0]), End::Life(lives[2]), Head::None, Route::Straight);
        assert!(d.reorder(extra, 1));
        d.fit_sequence();
        let order = d.messages();
        assert_eq!(order, [links[0], extra, links[1], links[2], links[3]]);
        assert_eq!(d.message_y(extra), Some(136.));
        assert_eq!(d.message_y(links[3]), Some(268.));
        assert_eq!(lives.iter().map(|id| d.shape(*id).unwrap().x).collect::<Vec<_>>(), x);
        // The lifelines grew to hold the last message, with a margin; the heads are on one line.
        assert!(lives.iter().all(|id| d.shape(*id).unwrap().y == 20. && d.shape(*id).unwrap().y + d.shape(*id).unwrap().h >= 268. + 44.));
        // Moving to the same place changes nothing; the places are clamped to the messages there are.
        assert!(!d.reorder(extra, 1) && d.reorder(extra, 99) && d.messages().last() == Some(&extra) && !d.reorder(999, 0));
        assert_eq!((d.rank_at(-500.), d.rank_at(137.), d.rank_at(10_000.)), (0, 1, 4));
    }

    #[test]
    fn calls_open_bars_and_replies_close_them() {
        // A calls B, which calls C and gets the answer, then answers A.
        let (d, lives, _) = talk(3, &[(0, 1, Message::Sync), (1, 2, Message::Sync), (2, 1, Message::Reply), (1, 0, Message::Reply)]);
        let marks = d.marks();
        assert!(marks.crosses.is_empty());
        let on = |life: usize| marks.bars.iter().filter(|b| b.life == lives[life]).map(|b| (b.y0, b.y1, b.depth)).collect::<Vec<_>>();
        assert_eq!((on(0), on(1), on(2)), (vec![], vec![(92., 224., 0)], vec![(136., 180., 0)]));
        // The arrows reach the bars' edges, not the middle of the lifeline.
        let bar = marks.bars.iter().find(|b| b.life == lives[1]).unwrap();
        let call = d.trace(d.link(d.messages()[0]).unwrap()).unwrap().points;
        assert_eq!(call[1].0, bar.x, "arrives at the left edge of B's bar");
        let call = d.trace(d.link(d.messages()[1]).unwrap()).unwrap().points;
        assert_eq!(call[0].0, bar.x + 10., "leaves from the right edge of B's bar");
        // A call to oneself nests: a bar over the bar, five to the right.
        let (d, lives, _) = talk(2, &[(0, 1, Message::Sync), (1, 1, Message::Sync), (1, 0, Message::Reply)]);
        let own = d.marks().bars.iter().filter(|b| b.life == lives[1]).map(|b| (b.depth, b.y0)).collect::<Vec<_>>();
        assert!(own.contains(&(1, 136.)), "{own:?}");
    }

    #[test]
    fn create_and_destroy_move_a_head_and_end_a_line() {
        let (d, lives, links) = talk(3, &[(0, 1, Message::Sync), (1, 2, Message::Create), (2, 1, Message::Reply), (1, 2, Message::Destroy)]);
        // The created lifeline's head is where its message arrives, centered on it; the others stay on the line.
        let third = d.shape(lives[2]).unwrap();
        assert_eq!((third.y, d.shape(lives[0]).unwrap().y), (136. - 18., 20.));
        // The arrow reaches the edge of the head, not the line.
        let path = d.trace(d.link(links[1]).unwrap()).unwrap().points;
        assert_eq!(path.last().unwrap().0, third.x);
        // The destroyed lifeline has a cross at the height of its message, and ends there (plus the margin).
        let marks = d.marks();
        assert_eq!(marks.crosses, [(third.x + third.w / 2., 224.)]);
        assert_eq!(third.y + third.h, 224. + 14.);
    }

    #[test]
    fn a_message_to_itself_is_a_loop_and_a_free_end_keeps_its_place() {
        let (d, lives, links) = talk(2, &[(0, 0, Message::Async)]);
        let path = d.trace(d.link(links[0]).unwrap()).unwrap().points;
        let middle = d.shape(lives[0]).unwrap().x + 50.;
        assert_eq!(path, [(middle, 92.), (middle + 34., 92.), (middle + 34., 114.), (middle, 114.)]);
        // Found: from a dot at the left, to the lifeline; lost: from the lifeline, to a dot at the right.
        let mut d = d;
        let found = d.add_link(End::Point(40., 0.), End::Life(lives[1]), Head::None, Route::Straight);
        d.link_mut(found).unwrap().start = Head::Dot;
        let lost = d.add_link(End::Life(lives[1]), End::Point(600., 0.), Head::Dot, Route::Straight);
        let _ = lost;
        d.fit_sequence();
        let path = d.trace(d.link(found).unwrap()).unwrap().points;
        assert_eq!((path[0].0, path[1].0, path[0].1), (40., d.shape(lives[1]).unwrap().x + 50., 136.));
        assert_eq!(Message::of(d.link(found).unwrap()), Message::Found);
        let path = d.trace(d.link(lost).unwrap()).unwrap().points;
        assert_eq!((path[1].0, path[0].1), (600., 180.));
        assert_eq!(Message::of(d.link(lost).unwrap()), Message::Lost);
    }

    #[test]
    fn twenty_messages_among_six_lifelines_stay_apart_when_one_is_inserted() {
        let mut dice = 11u64;
        let mut roll = |n: usize| {
            dice = dice.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((dice >> 33) as usize) % n
        };
        let say: Vec<(usize, usize, Message)> = (0..20)
            .map(|i| {
                let (a, b) = (roll(6), roll(6));
                (a, b, if i % 2 == 0 { Message::Sync } else { Message::Reply })
            })
            .collect();
        let (mut d, lives, links) = talk(6, &say);
        let before: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        let extra = d.add_link(End::Life(lives[1]), End::Life(lives[4]), Head::None, Route::Straight);
        d.reorder(extra, 9);
        d.fit_sequence();
        let after: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        // The first nine did not move; every later one went down by exactly one step; the new one is between.
        assert_eq!(&after[..9], &before[..9]);
        assert!(after[9..].iter().zip(&before[9..]).all(|(a, b)| (a - b - STEP).abs() < 1e-3));
        assert_eq!(d.message_y(extra), Some(before[9]));
        // Every message is on its own height, 44 apart, and the whole stays right-angled and finite.
        let mut ys: Vec<f32> = d.messages().iter().map(|id| d.message_y(*id).unwrap()).collect();
        let n = ys.len();
        ys.dedup();
        assert_eq!(ys.len(), n);
        assert!(ys.windows(2).all(|w| (w[1] - w[0] - STEP).abs() < 1e-3));
        for id in d.messages() {
            let path = d.trace(d.link(id).unwrap()).unwrap().points;
            assert!(path.iter().all(|p| p.0.is_finite() && p.1.is_finite()) && path.windows(2).all(|w| w[0].0 == w[1].0 || w[0].1 == w[1].1));
        }
        // The lifelines hold the last message.
        let last = d.rank_y(n - 1);
        assert!(lives.iter().all(|id| { let s = d.shape(*id).unwrap(); s.y + s.h >= last }));
    }
}
