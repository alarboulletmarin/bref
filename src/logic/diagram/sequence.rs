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
/// The room a combined fragment takes: above its first message (its tab and its first guard), at a
/// change of operand (the dashed line and the next guard), and under its last message.
const OPEN: f32 = 46.;
const OPERAND: f32 = 56.;
const CLOSE: f32 = 14.;
/// How far under its last message the frame of a fragment goes.
const BELOW: f32 = 26.;
/// How far a fragment is inside the one that holds it.
const NEST_X: f32 = 10.;

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

/// A combined fragment's frame, from `y0` to `y1`, and the dashed lines between its operands (the
/// operand each begins, and its height).
#[derive(Clone, PartialEq, Debug)]
pub struct Frame {
    pub id: u32,
    pub y0: f32,
    pub y1: f32,
    pub lines: Vec<(u32, f32)>,
}

/// The height of each message, in order, and the frames of the fragments.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Layout {
    pub ys: Vec<f32>,
    pub frames: Vec<Frame>,
}

/// What drawing the messages takes, worked out once for all of them: the height of each and the marks.
#[derive(Default)]
pub struct Flow {
    ys: std::collections::HashMap<u32, f32>,
    pub marks: Marks,
}

impl Diagram {
    /// The heights of the messages and the marks they make, for a drawing that traces all of them.
    pub fn flow(&self) -> Flow {
        let (order, ys) = (self.messages(), self.layout().ys);
        let marks = self.marks_at(&order, &ys);
        Flow { ys: order.into_iter().zip(ys).collect(), marks }
    }

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
    #[cfg(test)]
    pub fn rank_y(&self, rank: usize) -> f32 {
        let layout = self.layout();
        layout.ys.get(rank).copied().unwrap_or_else(|| self.heads_end() + FIRST + STEP * rank as f32)
    }

    /// The height of message `id`.
    pub fn message_y(&self, id: u32) -> Option<f32> {
        let rank = self.messages().iter().position(|m| *m == id)?;
        self.layout().ys.get(rank).copied()
    }

    /// The place a message dragged to this height takes among the others: that of the one nearest.
    pub fn rank_at(&self, y: f32) -> usize {
        let ys = self.layout().ys;
        (0..ys.len()).min_by(|a, b| (ys[*a] - y).abs().total_cmp(&(ys[*b] - y).abs())).unwrap_or(0)
    }

    /// The combined fragments a message is in, outermost first, each with the operand it is in.
    fn chain(&self, link: &Link) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let (mut at, mut operand) = (link.holder, link.operand);
        // A damaged file with a loop of fragments ends here.
        while let Some(id) = at.filter(|_| out.len() < 16) {
            let Some(shape) = self.shape(id).filter(|s| s.form == Form::Fragment) else { break };
            out.push((id, operand.min(Diagram::operands(shape).saturating_sub(1))));
            (at, operand) = (shape.parent, shape.operand);
        }
        out.reverse();
        out
    }

    /// How many operands a fragment has: a line of its text for each, after the operator. A `ref` has none.
    pub fn operands(shape: &Shape) -> u32 {
        let lines = shape.text.split('\n').count() as u32;
        if shape.text.lines().next().is_some_and(|op| op.trim() == "ref") { 1 } else { lines.saturating_sub(1).max(1) }
    }

    /// Where the messages and the fragments are: each message by its order, with the room fragments
    /// ask for (above their first message, at a change of operand, under their last), and each fragment's
    /// frame and its dashed lines.
    pub fn layout(&self) -> Layout {
        let order = self.messages();
        let mut layout = Layout::default();
        let mut cursor = self.heads_end() + FIRST;
        let mut before: Vec<(u32, u32)> = Vec::new();
        // The fragments open, outermost first: their index in `layout.frames`.
        let mut open: Vec<usize> = Vec::new();
        let mut last = cursor;
        for id in order {
            let Some(link) = self.link(id) else { continue };
            let now = self.chain(link);
            let common = before.iter().zip(&now).take_while(|(a, b)| a.0 == b.0).count();
            // The first level that stays open with another operand: a dashed line, and the next guard.
            let change = (0..common).find(|j| before[*j].1 != now[*j].1);
            let kept = change.map_or(common, |j| j + 1);
            let mut extra = 0.;
            for index in open.drain(kept..).rev() {
                layout.frames[index].y1 = last + BELOW;
                extra += CLOSE;
            }
            if change.is_some() {
                extra += OPERAND;
            }
            let opens = now.len() - kept;
            extra += OPEN * opens as f32;
            let y = cursor + extra;
            if let (Some(j), Some(index)) = (change, change.and_then(|j| open.get(j).copied())) {
                layout.frames[index].lines.push((now[j].1, y - OPERAND + 8.));
            }
            for (k, (frag, _)) in now.iter().enumerate().skip(kept) {
                let top = y - OPEN * (opens - (k - kept)) as f32 + 6.;
                layout.frames.push(Frame { id: *frag, y0: top, y1: top, lines: Vec::new() });
                open.push(layout.frames.len() - 1);
            }
            layout.ys.push(y);
            (cursor, last, before) = (y + STEP, y, now);
        }
        for index in open.into_iter().rev() {
            layout.frames[index].y1 = last + BELOW;
        }
        layout
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
    #[cfg(test)]
    pub fn marks(&self) -> Marks {
        self.marks_at(&self.messages(), &self.layout().ys)
    }

    fn marks_at(&self, order: &[u32], ys: &[f32]) -> Marks {
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
        // The alts the message is in, each with its branch and what was open when it was entered.
        let mut alts: Vec<(u32, u32, Vec<(u32, f32, Option<u32>)>)> = Vec::new();
        let is_alt = |id: u32| self.shape(id).is_some_and(|s| s.text.lines().next().is_some_and(|op| op.trim() == "alt"));
        for (rank, id) in order.iter().enumerate() {
            let Some(link) = self.link(*id) else { continue };
            let y = ys[rank];
            // Only one branch of an alt happens: each starts from what was open before the alt, and what the
            // branch above closed is open again, its bar drawn on to where this one closes it.
            let chain: Vec<(u32, u32)> = self.chain(link).into_iter().filter(|(frag, _)| is_alt(*frag)).collect();
            alts.retain(|(frag, ..)| chain.iter().any(|(f, _)| f == frag));
            for (frag, operand) in chain {
                match alts.iter_mut().find(|(f, ..)| *f == frag) {
                    None => alts.push((frag, operand, open.clone())),
                    Some(alt) if alt.1 < operand => {
                        alt.1 = operand;
                        for back in alt.2.iter().filter(|e| !open.contains(e)) {
                            if let Some(at) = marks.bars.iter().rposition(|b| b.life == back.0 && b.y0 == back.1) {
                                marks.bars.remove(at);
                            }
                        }
                        let since: Vec<_> = open.iter().filter(|e| !alt.2.contains(e)).copied().collect();
                        open = alt.2.iter().copied().chain(since).collect();
                    }
                    Some(_) => {}
                }
            }
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
    pub(super) fn message_path(&self, link: &Link, flow: &OnceCell<Flow>) -> Option<Vec<(f32, f32)>> {
        let life = |end: End| if let End::Life(id) = end { Some(id) } else { None };
        if life(link.from).is_none() && life(link.to).is_none() {
            return None;
        }
        let flow = flow.get_or_init(|| self.flow());
        let y = *flow.ys.get(&link.id)?;
        let marks = &flow.marks;
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
        let layout = self.layout();
        // A fragment is as tall as its messages make it.
        for frame in &layout.frames {
            if let Some(shape) = self.shape_mut(frame.id) {
                (shape.y, shape.h) = (frame.y0, frame.y1 - frame.y0);
            }
        }
        let heads = self.heads_end() - HEAD;
        let ids: Vec<u32> = self.lifelines().map(|s| s.id).collect();
        if ids.is_empty() {
            return;
        }
        let made = self.created();
        let order = self.messages();
        let bottom = layout.ys.last().copied().unwrap_or(0.);
        let bottom = layout.frames.iter().map(|f| f.y1).fold(bottom, f32::max);
        let mut ends: Vec<(u32, f32)> = Vec::new();
        for (rank, id) in order.iter().enumerate() {
            let y = layout.ys[rank];
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
            // All the others go down to the last message (or the foot of the last fragment), and a margin.
            shape.h = (end.unwrap_or(bottom + TAIL) - shape.y).max(HEAD + 60.);
        }
    }

    /// Wraps messages `ids` (the places from the first to the last, all in the same fragment and operand)
    /// in a new combined fragment with this text, `alt\n[condition]\n[else]`; returns it. It spans the lifelines
    /// the messages touch, and goes inside the fragment they were in. Never a `ref`.
    pub fn wrap(&mut self, ids: &[u32], text: &str) -> Option<u32> {
        // A `ref` stands for a whole diagram: it holds no message.
        if text.lines().next().is_some_and(|op| op.trim() == "ref") {
            return None;
        }
        let order = self.messages();
        let ranks: Vec<usize> = ids.iter().filter_map(|id| order.iter().position(|m| m == id)).collect();
        let (first, last) = (*ranks.iter().min()?, *ranks.iter().max()?);
        let members = &order[first..=last];
        let (holder, operand) = {
            let link = self.link(members[0])?;
            (link.holder, link.operand)
        };
        // A range that crosses a fragment's border cannot be wrapped: the new one would cut it.
        if members.iter().any(|m| self.link(*m).is_none_or(|l| (l.holder, l.operand) != (holder, operand))) {
            return None;
        }
        let across: Vec<&Shape> = members
            .iter()
            .filter_map(|m| self.link(*m))
            .flat_map(|l| [l.from, l.to])
            .filter_map(|end| if let End::Life(id) = end { self.shape(id) } else { None })
            .collect();
        let (x0, x1) = (across.iter().map(|s| s.x).fold(f32::MAX, f32::min), across.iter().map(|s| s.x + s.w).fold(f32::MIN, f32::max));
        let (x0, x1) = if across.is_empty() { (0., 200.) } else { (x0 - 24., x1 + 24. + LOOP.0) };
        // Inside a fragment, it is drawn inside its borders.
        let (mut x0, mut x1) = (x0, x1);
        if let Some(parent) = holder.and_then(|h| self.shape(h)) {
            (x0, x1) = (x0.max(parent.x + NEST_X), x1.min(parent.x + parent.w - NEST_X).max(x0 + 20.));
        }
        let id = self.add_shape(Form::Fragment, x0, 0., x1 - x0, 100.);
        if let Some(shape) = self.shape_mut(id) {
            (shape.text, shape.parent, shape.operand) = (text.to_string(), holder, operand);
        }
        for m in members {
            if let Some(link) = self.link_mut(*m) {
                (link.holder, link.operand) = (Some(id), 0);
            }
        }
        // A fragment comes after what it holds the shapes of in the drawing order: it is under the lifelines' marks.
        self.fit_sequence();
        Some(id)
    }

    /// After message `id` was moved and let go at height `y`: it is in the fragment, and the operand,
    /// whose frame it was let go in, among those of its new neighbours and of what holds them; between two
    /// messages of one fragment it is always in it, so that a fragment stays in one piece.
    pub fn settle(&mut self, id: u32, y: f32) {
        let order = self.messages();
        let Some(at) = order.iter().position(|m| *m == id) else { return };
        let of = |d: &Diagram, i: Option<usize>| i.and_then(|i| order.get(i)).and_then(|m| d.link(*m)).map(|l| (l.holder, l.operand));
        let (before, after) = (of(self, at.checked_sub(1)), of(self, Some(at + 1)));
        if let (Some(a), Some(b)) = (before, after)
            && a == b
        {
            return self.hold(id, a);
        }
        // What it could be in: what its neighbours are in, and what holds those, each with its operand; and the
        // next operand of what the one above is in, which is how a message gets into the "else".
        let mut candidates: Vec<(Option<u32>, u32)> = Vec::new();
        for (k, neighbour) in [at.checked_sub(1), Some(at + 1)].into_iter().filter_map(|i| i.and_then(|i| order.get(i))).enumerate() {
            let Some(link) = self.link(*neighbour) else { continue };
            for (frag, operand) in self.chain(link) {
                let next = self.shape(frag).filter(|s| k == 0 && operand + 1 < Diagram::operands(s));
                for candidate in [Some((Some(frag), operand)), next.map(|_| (Some(frag), operand + 1))].into_iter().flatten() {
                    if !candidates.contains(&candidate) {
                        candidates.push(candidate);
                    }
                }
            }
        }
        // The deepest whose frame, with the message in it, covers the height it was let go at; of those, the one
        // that draws the message nearest to the pointer.
        let covers = |d: &Diagram, frag: u32| d.layout().frames.iter().any(|f| f.id == frag && f.y0 - STEP / 2. <= y && y <= f.y1 + STEP / 2.);
        let mut best: Option<((Option<u32>, u32), usize, f32)> = None;
        for candidate in candidates {
            let mut trial = self.clone();
            trial.hold(id, candidate);
            let depth = trial.link(id).map_or(0, |l| trial.chain(l).len());
            let away = trial.message_y(id).map_or(f32::MAX, |m| (m - y).abs());
            if candidate.0.is_some_and(|frag| covers(&trial, frag)) && best.is_none_or(|(_, d, a)| depth > d || (depth == d && away < a)) {
                best = Some((candidate, depth, away));
            }
        }
        self.hold(id, best.map_or((None, 0), |(candidate, ..)| candidate));
    }

    fn hold(&mut self, id: u32, to: (Option<u32>, u32)) {
        if let Some(link) = self.link_mut(id) {
            (link.holder, link.operand) = to;
        }
    }

    /// The text lines of a combined fragment and the dashed lines between its operands: the operator in
    /// the tab, the first guard beside it, the others under their dashed line; a `ref` has its name in the middle.
    pub fn fragment_lines<'a>(&self, shape: &'a Shape) -> (Vec<TextLine<'a>>, Vec<f32>) {
        let lines: Vec<&str> = shape.text.split('\n').collect();
        let tab = shape.tab_width();
        let mut out = vec![TextLine { x: shape.x + 6., y: shape.y + 1., text: lines[0], centered: false, bold: true, index: 0 }];
        if lines[0].trim() == "ref" {
            let name = lines.get(1).copied().unwrap_or("");
            out.push(TextLine { x: shape.x + shape.w / 2., y: shape.y + 24. + (shape.h - 24. - LINE).max(0.) / 2., text: name, centered: true, bold: false, index: 1 });
            return (out, Vec::new());
        }
        let frame = self.layout().frames.into_iter().find(|f| f.id == shape.id);
        let seps = frame.map(|f| f.lines).unwrap_or_default();
        let mut rules = Vec::new();
        for (k, text) in lines.iter().enumerate().skip(1) {
            let operand = (k - 1) as u32;
            if operand == 0 {
                out.push(TextLine { x: shape.x + tab + 8., y: shape.y + 1., text, centered: false, bold: false, index: k });
            } else if let Some((_, y)) = seps.iter().find(|(o, _)| *o == operand) {
                rules.push(*y);
                out.push(TextLine { x: shape.x + 8., y: *y + 3., text, centered: false, bold: false, index: k });
            }
        }
        (out, rules)
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
    fn tracing_all_the_arrows_at_once_draws_what_one_by_one_does() {
        let kinds = [Message::Sync, Message::Sync, Message::Reply, Message::Async, Message::Create, Message::Destroy, Message::Found, Message::Lost, Message::Reply];
        let (mut d, lives, _) = talk(4, &kinds.iter().enumerate().map(|(i, k)| (i % 3, (i + 1) % 4, *k)).collect::<Vec<_>>());
        // A plain arrow between two shapes is among them, and so is a broken one.
        d.add_link(End::Shape(lives[0]), End::Point(500., 500.), Head::Arrow, Route::Elbow);
        d.add_link(End::Shape(999), End::Shape(lives[1]), Head::Arrow, Route::Straight);
        let (all, flow) = d.traces();
        assert_eq!(all.len(), d.links.len());
        for (link, one) in d.links.iter().zip(&all) {
            assert_eq!(one.as_ref().map(|t| &t.points), d.trace(link).as_ref().map(|t| &t.points));
        }
        assert!(all.last().unwrap().is_none());
        assert_eq!(flow.marks, d.marks());
        assert_eq!(d.bounds_of(&all), d.bounds());
        // Without a message, nothing is worked out and the marks are empty.
        let (_, none) = Diagram::blank().traces();
        assert_eq!(none.marks, Marks::default());
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
    fn each_branch_of_an_alt_starts_from_what_was_open_before_it() {
        // A calls B; B answers in [ok] and in [else]: only one of them happens, so B is busy until the last.
        let bars = |d: &Diagram, life: u32| d.marks().bars.iter().filter(|b| b.life == life).map(|b| (b.y0, b.y1, b.depth)).collect::<Vec<_>>();
        let y = |d: &Diagram, id: u32| d.message_y(id).unwrap();
        let branches = |d: &mut Diagram, ids: &[u32], text: &str, split: usize| {
            d.wrap(ids, text).unwrap();
            for id in &ids[split..] {
                d.link_mut(*id).unwrap().operand = 1;
            }
            d.fit_sequence();
        };
        let (mut d, lives, links) = talk(2, &[(0, 1, Message::Sync), (1, 0, Message::Reply), (1, 0, Message::Reply), (0, 1, Message::Async)]);
        branches(&mut d, &links[1..3], "alt\n[ok]\n[else]", 1);
        assert_eq!(bars(&d, lives[1]), [(y(&d, links[0]), y(&d, links[2]), 0)]);
        // The answer of the [else] leaves from the bar's edge, and after the alt nothing is open any more.
        let bar = d.marks().bars[0].clone();
        assert_eq!(d.trace(d.link(links[2]).unwrap()).unwrap().points[0].0, bar.x);
        assert!(bar.y1 < y(&d, links[3]));

        // In the [else], B calls C before answering: C's bar is there, B's goes on under it.
        let (mut d, lives, links) = talk(3, &[(0, 1, Message::Sync), (1, 0, Message::Reply), (1, 2, Message::Sync), (2, 1, Message::Reply), (1, 0, Message::Reply)]);
        branches(&mut d, &links[1..5], "alt\n[cache]\n[else]", 1);
        assert_eq!(bars(&d, lives[1]), [(y(&d, links[0]), y(&d, links[4]), 0)]);
        assert_eq!(bars(&d, lives[2]), [(y(&d, links[2]), y(&d, links[3]), 0)]);

        // An alt in the [else] of another: three branches, still one bar to the last answer.
        let (mut d, lives, links) = talk(2, &[(0, 1, Message::Sync), (1, 0, Message::Reply), (1, 0, Message::Reply), (1, 0, Message::Reply)]);
        branches(&mut d, &links[1..4], "alt\n[a]\n[else]", 1);
        branches(&mut d, &links[2..4], "alt\n[b]\n[else]", 1);
        assert_eq!(bars(&d, lives[1]), [(y(&d, links[0]), y(&d, links[3]), 0)]);

        // A par runs its branches together: the first answer closes the bar, as without a fragment.
        let (mut d, lives, links) = talk(2, &[(0, 1, Message::Sync), (1, 0, Message::Reply), (1, 0, Message::Reply)]);
        branches(&mut d, &links[1..3], "par\n[a]\n[b]", 1);
        assert_eq!(bars(&d, lives[1]), [(y(&d, links[0]), y(&d, links[1]), 0)]);
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

    #[test]
    fn a_fragment_makes_room_and_is_as_tall_as_its_messages() {
        let (mut d, _, links) = talk(3, &[(0, 1, Message::Sync), (1, 2, Message::Sync), (2, 1, Message::Reply), (1, 0, Message::Reply), (0, 1, Message::Sync), (1, 0, Message::Reply)]);
        let plain: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        let frag = d.wrap(&[links[1], links[2], links[3]], "alt\n[ok]\n[else]").unwrap();
        let ys: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        // Before it, nothing moves. Inside, the first message has room above it for the tab and the guard;
        // after it, the others are pushed down by that room and by the foot.
        assert_eq!(ys[0], plain[0]);
        assert_eq!(ys[1], plain[1] + 46.);
        assert_eq!((ys[2], ys[3]), (plain[2] + 46., plain[3] + 46.));
        assert_eq!(ys[4], plain[4] + 46. + 14.);
        // The frame goes from just above the tab to under its last message, and the shape follows.
        let layout = d.layout();
        let frame = layout.frames.iter().find(|f| f.id == frag).unwrap();
        assert_eq!((frame.y0, frame.y1), (ys[1] - 46. + 6., ys[3] + 26.));
        let shape = d.shape(frag).unwrap();
        assert_eq!((shape.y, shape.h), (frame.y0, frame.y1 - frame.y0));
        // A second operand: a dashed line and room for the next guard, then the foot.
        d.link_mut(links[3]).unwrap().operand = 1;
        let ys2: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        assert_eq!(ys2[3], ys[3] + 56.);
        assert_eq!(ys2[4], ys[4] + 56.);
        let frame = d.layout().frames.into_iter().find(|f| f.id == frag).unwrap();
        assert_eq!(frame.lines, [(1, ys2[3] - 56. + 8.)]);
        // The lifelines go under the foot of the last fragment, and nothing is above its message.
        d.fit_sequence();
        let (bottom, ymax) = (d.shapes.iter().filter(|s| s.form == Form::Lifeline).map(|s| s.y + s.h).fold(f32::MAX, f32::min), ys2[5]);
        assert!(bottom >= ymax + 44.);
        // Wrapping makes a fragment of what is between the first and the last, never across the border of another.
        assert!(d.wrap(&[links[0], links[2]], "opt\n[x]").is_none());
        // Inside the first one, a second, nested: its room is added to the first message's.
        let inner = d.wrap(&[links[1], links[2]], "loop\n[n]").unwrap();
        assert_eq!((d.shape(inner).unwrap().parent, d.shape(inner).unwrap().operand), (Some(frag), 0));
        // Drawn inside the borders of the first, so that its sides are seen apart.
        let (o, i) = (d.shape(frag).unwrap(), d.shape(inner).unwrap());
        assert!(i.x >= o.x + 10. && i.x + i.w <= o.x + o.w - 10., "{o:?} {i:?}");
        let ys3: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        assert_eq!(ys3[1], ys[1] + 46.);
        let layout = d.layout();
        let (outer, nested) = (layout.frames.iter().find(|f| f.id == frag).unwrap(), layout.frames.iter().find(|f| f.id == inner).unwrap());
        assert!(outer.y0 < nested.y0 && nested.y0 < ys3[1] && nested.y1 < outer.y1, "{outer:?} {nested:?}");
        // The operand of a fragment inside another is the one it is in.
        assert!(d.link(links[3]).unwrap().holder == Some(frag) && d.link(links[3]).unwrap().operand == 1);
    }

    #[test]
    fn a_message_let_go_joins_or_leaves_the_fragment_it_is_let_go_in() {
        let (mut d, _, links) = talk(3, &[(0, 1, Message::Sync), (1, 2, Message::Sync), (2, 1, Message::Reply), (1, 0, Message::Reply), (0, 2, Message::Async)]);
        let frag = d.wrap(&[links[1], links[2]], "opt\n[x]").unwrap();
        let held = |d: &Diagram, id: u32| d.link(id).unwrap().holder;
        // Dragged between the two that are in it, it is in it, wherever the pointer is.
        assert!(d.reorder(links[4], 2));
        d.settle(links[4], 5000.);
        assert_eq!(held(&d, links[4]), Some(frag));
        // Let go inside the frame at the edge, still in; let go under it, out.
        let frame = d.layout().frames.into_iter().find(|f| f.id == frag).unwrap();
        assert!(d.reorder(links[4], 3));
        d.settle(links[4], frame.y1 - 4.);
        assert_eq!(held(&d, links[4]), Some(frag));
        d.settle(links[4], frame.y1 + 200.);
        assert_eq!(held(&d, links[4]), None);
        // A message that was outside, let go inside the frame after the last one, joins it; far under, it does not.
        assert!(d.reorder(links[0], 2));
        let frame = d.layout().frames.into_iter().find(|f| f.id == frag).unwrap();
        d.settle(links[0], frame.y1 - 10.);
        assert_eq!(held(&d, links[0]), Some(frag));
        d.settle(links[0], frame.y1 + 300.);
        assert_eq!(held(&d, links[0]), None);
        // A fragment removed lets its messages go; they are not removed.
        d.remove(&[frag]);
        assert!(d.link(links[1]).is_some() && d.messages().len() == 5 && d.links.iter().all(|l| l.holder.is_none()));
    }

    #[test]
    fn a_message_let_go_low_in_a_fragment_with_an_else_goes_to_the_else() {
        let (mut d, _, links) = talk(3, &[(0, 1, Message::Sync), (1, 2, Message::Sync), (2, 1, Message::Reply), (1, 0, Message::Reply), (0, 2, Message::Async)]);
        let frag = d.wrap(&[links[1], links[2]], "alt\n[ok]\n[else]").unwrap();
        let held = |d: &Diagram| (d.link(links[4]).unwrap().holder, d.link(links[4]).unwrap().operand);
        // Just after the last message of the first operand: let go near where it is drawn, it is in the "else".
        assert!(d.reorder(links[4], 3));
        d.settle(links[4], 5000.);
        assert_eq!(held(&d), (None, 0));
        let near = |d: &Diagram, operand: u32| {
            let mut trial = d.clone();
            trial.hold(links[4], (Some(frag), operand));
            trial.message_y(links[4]).unwrap()
        };
        let (first, second) = (near(&d, 0), near(&d, 1));
        assert!(second > first + 50.);
        d.settle(links[4], first);
        assert_eq!(held(&d), (Some(frag), 0));
        d.settle(links[4], second);
        assert_eq!(held(&d), (Some(frag), 1));
        // Between the operands it is the nearer; and another fragment with one operand never has a second.
        d.settle(links[4], (first + second) / 2. - 5.);
        assert_eq!(held(&d), (Some(frag), 0));
        d.settle(links[4], (first + second) / 2. + 5.);
        assert_eq!(held(&d), (Some(frag), 1));
        let (mut d, _, links) = talk(2, &[(0, 1, Message::Sync), (1, 0, Message::Reply), (0, 1, Message::Sync)]);
        let opt = d.wrap(&[links[0], links[1]], "opt\n[x]").unwrap();
        d.settle(links[2], 5000.);
        assert_eq!(d.link(links[2]).unwrap().holder, None);
        d.settle(links[2], d.message_y(links[1]).unwrap() + 40.);
        assert_eq!((d.link(links[2]).unwrap().holder, d.link(links[2]).unwrap().operand), (Some(opt), 0));
    }

    #[test]
    fn a_fragment_of_one_message_of_none_or_with_a_strange_text_still_lays_out() {
        let (mut d, _, links) = talk(2, &[(0, 1, Message::Sync), (1, 0, Message::Reply)]);
        // Nothing to wrap: no ids, ids of nothing, a message twice.
        assert!(d.wrap(&[], "alt").is_none() && d.wrap(&[9999], "alt").is_none());
        // A `ref` holds no message, so it never wraps.
        assert!(d.wrap(&[links[0]], "ref\nOther").is_none() && d.shapes.iter().all(|s| s.form != Form::Fragment));
        let one = d.wrap(&[links[0], links[0]], "opt\n[x]").unwrap();
        let frame = d.layout().frames.into_iter().find(|f| f.id == one).unwrap();
        assert!(frame.y1 - frame.y0 >= 46. + 26. - 6., "{frame:?}");
        // A fragment that holds no message has no frame, keeps the shape it was drawn with, and is not in the way.
        let free = d.add_shape(Form::Fragment, 0., 500., 200., 100.);
        d.fit_sequence();
        assert!(d.layout().frames.iter().all(|f| f.id != free));
        assert_eq!((d.shape(free).unwrap().y, d.shape(free).unwrap().h), (500., 100.));
        // Whatever is typed in it: empty, an operator alone, a very long word, accents, CJK, emoji, `|`, `\`, a bare `ref`.
        for text in ["", "alt", "alt\n\n\n\n", &"x".repeat(500), "boucle\n[é composé]", "選択\n[日本語]\n[😀]", "alt\n[a | b]\n[c \\ d]", "ref", "ref\n"] {
            d.shape_mut(one).unwrap().text = text.to_string();
            d.fit_sequence();
            let (lines, rules) = d.fragment_lines(d.shape(one).unwrap());
            assert!(!lines.is_empty() && rules.len() <= Diagram::operands(d.shape(one).unwrap()) as usize, "{text:?}");
            assert_eq!(d.layout().ys.len(), 2, "{text:?}");
        }
    }

    #[test]
    fn the_text_of_a_fragment_is_laid_out_with_its_operands() {
        let (mut d, _, links) = talk(2, &[(0, 1, Message::Sync), (1, 0, Message::Reply), (0, 1, Message::Sync)]);
        let frag = d.wrap(&[links[0], links[1], links[2]], "alt\n[ok]\n[else]").unwrap();
        d.link_mut(links[2]).unwrap().operand = 1;
        d.fit_sequence();
        let shape = d.shape(frag).unwrap().clone();
        let (lines, rules) = d.lines(&shape);
        // The operator in the tab; the first guard beside the tab; the second under the dashed line between the operands.
        assert_eq!(lines.iter().map(|l| l.text).collect::<Vec<_>>(), ["alt", "[ok]", "[else]"]);
        assert_eq!((lines[0].x, lines[0].y, lines[0].bold), (shape.x + 6., shape.y + 1., true));
        assert!(lines[1].x > shape.x + 20. && lines[1].y == shape.y + 1.);
        let line = d.layout().frames[0].lines[0].1;
        assert_eq!((rules.clone(), lines[2].y), (vec![line], line + 3.));
        // A guard of an operand no message is in is not drawn; a `ref` has its name in the middle and no operand.
        let mut lonely = shape.clone();
        lonely.text = "alt\n[a]\n[b]\n[c]".into();
        assert_eq!(d.lines(&lonely).0.len(), 3);
        lonely.text = "ref\nLogin".into();
        let (lines, rules) = d.lines(&lonely);
        assert!(rules.is_empty() && lines[1].centered && lines[1].x == lonely.x + lonely.w / 2., "{lines:?}");
        assert_eq!(Diagram::operands(&lonely), 1);
        lonely.text = "par\n\n".into();
        assert_eq!(Diagram::operands(&lonely), 2);
    }

    #[test]
    fn twenty_messages_with_fragments_stay_in_order_and_apart() {
        let say: Vec<(usize, usize, Message)> = (0..20).map(|i| (i % 6, (i * 5 + 1) % 6, if i % 2 == 0 { Message::Sync } else { Message::Reply })).collect();
        let (mut d, _, links) = talk(6, &say);
        d.wrap(&links[2..8], "loop\n[more]").unwrap();
        d.wrap(&links[3..5], "opt\n[x]").unwrap();
        d.wrap(&links[12..16], "alt\n[a]\n[b]").unwrap();
        for id in &links[14..16] {
            d.link_mut(*id).unwrap().operand = 1;
        }
        let ys: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        // Strictly down, never less than a step apart.
        assert!(ys.windows(2).all(|w| w[1] - w[0] >= STEP - 1e-3), "{ys:?}");
        // Each frame holds its messages (above the first, under the last) and is held by its parent's frame.
        let layout = d.layout();
        assert_eq!(layout.frames.len(), 3);
        for frame in &layout.frames {
            let inside: Vec<f32> = d.messages().iter().filter(|m| d.chain(d.link(**m).unwrap()).iter().any(|(f, _)| *f == frame.id)).map(|m| d.message_y(*m).unwrap()).collect();
            assert!(frame.y0 < inside[0] - 10. && frame.y1 > inside[inside.len() - 1] + 10., "{frame:?} {inside:?}");
        }
        let (outer, inner) = (&layout.frames[0], &layout.frames[1]);
        assert!(outer.y0 < inner.y0 && inner.y1 < outer.y1);
        // The two sides that follow do not touch: the first frame ends before the third begins.
        assert!(layout.frames[0].y1 < layout.frames[2].y0);
        // Inserting a message in the middle of the loop pushes everything after it by one step, and nothing before.
        let extra = d.add_link(End::Life(d.shapes[0].id), End::Life(d.shapes[1].id), Head::None, Route::Straight);
        d.reorder(extra, 6);
        d.settle(extra, d.rank_y(6));
        assert_eq!(d.link(extra).unwrap().holder, d.link(links[5]).unwrap().holder, "inside the loop");
        let after: Vec<f32> = links.iter().map(|id| d.message_y(*id).unwrap()).collect();
        assert_eq!(&after[..6], &ys[..6]);
        assert!(after[6..].iter().zip(&ys[6..]).all(|(a, b)| (a - b - STEP).abs() < 1e-3), "{after:?} {ys:?}");
    }
}
