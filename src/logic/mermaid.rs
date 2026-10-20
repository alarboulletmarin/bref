//! A Mermaid `classDiagram`, `sequenceDiagram` or `stateDiagram` turned into a canvas diagram, to be
//! carried on by hand. A bridge, not a renderer: what Bref can draw is kept (classes with their members
//! and relations, lifelines with their messages and combined fragments, states with their transitions),
//! the lines it does not understand are left out and counted. Pure functions.

use std::collections::{HashMap, HashSet};

use super::{
    diagram::{Diagram, End, Form, Head, LANE, LINE, RULE, Route, snap},
    relation::Relation,
};

/// The kind of Mermaid diagram a source is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Class,
    Sequence,
    State,
}

/// A diagram made of a Mermaid source, and how many of its lines were not understood.
pub struct Converted {
    pub diagram: Diagram,
    pub skipped: usize,
}

/// The lines that count: no blank line, no comment, no front matter or directive.
fn body(source: &str) -> Vec<&str> {
    let mut lines = source.lines().map(str::trim);
    let mut out = Vec::new();
    let mut front = false;
    for (i, line) in lines.by_ref().enumerate() {
        if line == "---" && (i == 0 || front) {
            front = !front;
        } else if !front && !line.is_empty() && !line.starts_with("%%") {
            out.push(line);
        }
    }
    out
}

/// What kind of diagram this source is, if Bref can take it.
pub fn kind(source: &str) -> Option<Kind> {
    let first = body(source).into_iter().next()?;
    let word = first.split(|c: char| c.is_whitespace() || c == ';').next()?;
    match word {
        "classDiagram" | "classDiagram-v2" => Some(Kind::Class),
        "sequenceDiagram" => Some(Kind::Sequence),
        "stateDiagram" | "stateDiagram-v2" => Some(Kind::State),
        _ => None,
    }
}

/// The canvas diagram of a Mermaid source; `None` if it is not one of the three kinds.
pub fn to_diagram(source: &str) -> Option<Converted> {
    let lines = body(source);
    let rest = lines.get(1..)?;
    Some(match kind(source)? {
        Kind::Class => class(rest),
        Kind::Sequence => sequence(rest),
        Kind::State => state(rest),
    })
}

/// The text as one line: Mermaid breaks lines with `<br>`.
fn flat(text: &str) -> String {
    text.replace("<br/>", " ").replace("<br />", " ").replace("<br>", " ").trim().to_string()
}

/// A name without its quotes.
fn unquoted(text: &str) -> &str {
    text.trim().trim_matches('"')
}

/// Width and height of a box that holds these lines.
fn fit(lines: &[String], min: (f32, f32)) -> (f32, f32) {
    let rules = lines.iter().filter(|l| l.as_str() == RULE).count() as f32;
    let wide = lines.iter().map(|l| Diagram::guess_width(l)).fold(0., f32::max);
    ((wide + 28.).max(min.0).ceil(), ((lines.len() as f32 - rules) * LINE + rules * 8. + 18.).max(min.1).ceil())
}

/// Places boxes of these sizes by layers: each is under what points to it (`edges` run from the upper to
/// the lower; one that closes a loop is not counted), centred in its layer. Returns the top left corner
/// of each, and the size of the whole.
fn layered(sizes: &[(f32, f32)], edges: &[(usize, usize)]) -> (Vec<(f32, f32)>, (f32, f32)) {
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

// ------------------------------------------------------------------ classes

struct Class {
    name: String,
    note: Option<String>,
    attributes: Vec<String>,
    operations: Vec<String>,
}

impl Class {
    fn new(name: &str) -> Class {
        Class { name: name.to_string(), note: None, attributes: Vec::new(), operations: Vec::new() }
    }

    /// `+String name` is `+ name: String`, `+getName() String` is `+ getName(): String`.
    fn member(&mut self, raw: &str) {
        let raw = flat(raw);
        if raw.is_empty() {
            return;
        }
        let (visibility, rest) = match raw.chars().next() {
            Some(c @ ('+' | '-' | '#' | '~')) => (c.to_string() + " ", raw[1..].trim()),
            _ => (String::new(), raw.as_str()),
        };
        if let Some(close) = rest.find(')').filter(|_| rest.contains('(')) {
            let (head, tail) = rest.split_at(close + 1);
            let tail = tail.trim().trim_start_matches(':').trim();
            let text = if tail.is_empty() { format!("{visibility}{head}") } else { format!("{visibility}{head}: {tail}") };
            self.operations.push(text);
            return;
        }
        let words: Vec<&str> = rest.split_whitespace().collect();
        let text = match words[..] {
            [kind, name] if !rest.contains(':') => format!("{visibility}{name}: {kind}"),
            _ => format!("{visibility}{rest}"),
        };
        self.attributes.push(text);
    }

    fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self.note.iter().map(|n| format!("«{n}»")).collect();
        out.push(self.name.clone());
        if !self.attributes.is_empty() {
            out.push(RULE.into());
            out.extend(self.attributes.iter().cloned());
        }
        if !self.operations.is_empty() {
            out.push(RULE.into());
            out.extend(self.operations.iter().cloned());
        }
        out
    }
}

/// A relation between two classes: the link runs from `from` to `to`; `upper` when `to` is drawn above.
struct Rel {
    from: usize,
    to: usize,
    relation: Relation,
    dashed: bool,
    upper: bool,
    start: String,
    end: String,
    text: String,
}

/// How a relation is written: the arrow, and what it is, the first end being at the left.
/// `(arrow, relation, the link runs from the right to the left)`.
const ARROWS: [(&str, Relation, bool); 16] = [
    ("<|--", Relation::Generalization, true),
    ("--|>", Relation::Generalization, false),
    ("<|..", Relation::Realization, true),
    ("..|>", Relation::Realization, false),
    ("*--", Relation::Composition, false),
    ("--*", Relation::Composition, true),
    ("o--", Relation::Aggregation, false),
    ("--o", Relation::Aggregation, true),
    ("<--", Relation::Navigable, true),
    ("-->", Relation::Navigable, false),
    ("<..", Relation::Dependency, true),
    ("..>", Relation::Dependency, false),
    ("--", Relation::Association, false),
    ("..", Relation::Association, false),
    ("()--", Relation::Association, false),
    ("--()", Relation::Association, false),
];

/// The first arrow of the line, outside quotes: where it is, which one.
fn arrow_in(line: &str) -> Option<(usize, &'static (&'static str, Relation, bool))> {
    let mut quoted = false;
    for (at, c) in line.char_indices() {
        quoted ^= c == '"';
        if quoted {
            continue;
        }
        // Longest first: `<|--` before `--`.
        // An arrow stands between spaces (or quotes): `Foo--Bar` is not one.
        let before = line[..at].chars().next_back().is_none_or(|c| c.is_whitespace() || c == '"');
        let best = ARROWS.iter().filter(|(a, ..)| before && line[at..].starts_with(a) && line[at + a.len()..].chars().next().is_none_or(|c| c.is_whitespace() || c == '"')).max_by_key(|(a, ..)| a.len());
        if let Some(found) = best {
            return Some((at, found));
        }
    }
    None
}

fn class(lines: &[&str]) -> Converted {
    let mut classes: Vec<Class> = Vec::new();
    let mut skipped = 0;
    let mut open: Option<usize> = None;
    let mut links: Vec<Rel> = Vec::new();
    let index = |classes: &mut Vec<Class>, name: &str| -> usize {
        let name = flat(unquoted(name));
        classes.iter().position(|c| c.name == name).unwrap_or_else(|| {
            classes.push(Class::new(&name));
            classes.len() - 1
        })
    };
    for line in lines {
        let line = line.trim_end_matches(';').trim();
        if let Some(i) = open {
            if line == "}" {
                open = None;
            } else if let Some(note) = line.strip_prefix("<<").and_then(|n| n.strip_suffix(">>")) {
                classes[i].note = Some(note.trim().to_string());
            } else {
                classes[i].member(line);
            }
        } else if line.starts_with("direction ") {
        } else if let Some(rest) = line.strip_prefix("class ") {
            let name = rest.trim_end_matches('{').trim();
            let i = index(&mut classes, name.split(['[', ' ']).next().unwrap_or(name));
            if rest.trim_end().ends_with('{') {
                open = Some(i);
            }
        } else if let Some(note) = line.strip_prefix("<<").and_then(|r| r.split_once(">>")) {
            let i = index(&mut classes, note.1);
            classes[i].note = Some(note.0.trim().to_string());
        } else if let Some((at, (arrow, relation, back))) = arrow_in(line) {
            let (left, right) = (&line[..at], &line[at + arrow.len()..]);
            let (right, label) = match right.split_once(" : ").or_else(|| right.split_once(':')) {
                Some((r, l)) => (r, flat(l)),
                None => (right, String::new()),
            };
            let ((a, ma), (b, mb)) = (end_of_left(left), end_of_right(right));
            if a.is_empty() || b.is_empty() {
                skipped += 1;
                continue;
            }
            let (a, b) = (index(&mut classes, &a), index(&mut classes, &b));
            // The diamond of a whole is at the start of the link; a parent is the end of its link.
            let (from, to, m_from, m_to) = if *back { (b, a, mb, ma) } else { (a, b, ma, mb) };
            let upper = matches!(relation, Relation::Generalization | Relation::Realization);
            // A dotted association is a dashed line.
            links.push(Rel { from, to, relation: *relation, dashed: arrow == &".." || arrow.contains("()") && arrow.contains(".."), upper, start: m_from, end: m_to, text: label });
        } else if let Some((name, member)) = line.split_once(" : ").or_else(|| line.split_once(':')) {
            let i = index(&mut classes, name);
            classes[i].member(member);
        } else {
            skipped += 1;
        }
    }
    let mut d = Diagram::blank();
    let lines: Vec<Vec<String>> = classes.iter().map(Class::lines).collect();
    let sizes: Vec<(f32, f32)> = lines.iter().map(|l| fit(l, (140., 50.))).collect();
    let edges: Vec<(usize, usize)> = links.iter().filter(|l| l.from != l.to).map(|l| if l.upper { (l.to, l.from) } else { (l.from, l.to) }).collect();
    let (at, _) = layered(&sizes, &edges);
    let ids: Vec<u32> = (0..classes.len())
        .map(|i| {
            let id = d.add_shape(Form::Rect, at[i].0, at[i].1, sizes[i].0, sizes[i].1);
            if let Some(shape) = d.shape_mut(id) {
                shape.text = lines[i].join("\n");
            }
            id
        })
        .collect();
    for rel in links {
        let id = d.add_link(End::Shape(ids[rel.from]), End::Shape(ids[rel.to]), Head::None, Route::Elbow);
        if let Some(link) = d.link_mut(id) {
            rel.relation.apply(link);
            link.dashed |= rel.dashed;
            (link.start_text, link.end_text, link.text) = (rel.start, rel.end, rel.text);
        }
    }
    Converted { diagram: d, skipped }
}

/// The class and its multiplicity at the left of an arrow: `A "1"`.
fn end_of_left(side: &str) -> (String, String) {
    let side = side.trim();
    match side.find('"') {
        Some(q) => (side[..q].trim().to_string(), unquoted(&side[q..]).to_string()),
        None => (side.to_string(), String::new()),
    }
}

/// The class and its multiplicity at the right of an arrow: `"*" B`.
fn end_of_right(side: &str) -> (String, String) {
    let side = side.trim();
    match side.strip_prefix('"').and_then(|s| s.split_once('"')) {
        Some((m, name)) => (name.trim().to_string(), m.to_string()),
        None => (side.to_string(), String::new()),
    }
}

// ----------------------------------------------------------------- sequence

/// A message arrow: `(arrow, relation)`, longest first.
const MESSAGES: [(&str, Relation); 8] = [
    ("-->>", Relation::Dependency),
    ("->>", Relation::Sync),
    ("--x", Relation::NonNavigable),
    ("-x", Relation::NonNavigable),
    ("--)", Relation::Dependency),
    ("-)", Relation::Navigable),
    ("-->", Relation::Association),
    ("->", Relation::Association),
];

/// A combined fragment being read: its operator, its guards, where it began and where each operand did.
struct Block {
    operator: Option<&'static str>,
    guards: Vec<String>,
    start: usize,
    splits: Vec<usize>,
}

fn sequence(lines: &[&str]) -> Converted {
    let mut names: Vec<(String, String)> = Vec::new();
    let mut messages: Vec<(usize, usize, Relation, String)> = Vec::new();
    let (mut stack, mut closed): (Vec<Block>, Vec<(Block, usize)>) = (Vec::new(), Vec::new());
    let (mut skipped, mut numbered) = (0, false);
    let lifeline = |names: &mut Vec<(String, String)>, id: &str| -> usize {
        let id = id.trim();
        names.iter().position(|(i, _)| i == id).unwrap_or_else(|| {
            names.push((id.to_string(), id.to_string()));
            names.len() - 1
        })
    };
    for line in lines {
        let line = line.trim_end_matches(';').trim();
        let word = line.split_whitespace().next().unwrap_or("");
        let after = line[word.len()..].trim();
        match word {
            "participant" | "actor" => {
                let (id, label) = after.split_once(" as ").map_or((after, after), |(i, l)| (i, l));
                let i = lifeline(&mut names, id);
                names[i].1 = flat(label);
            }
            "autonumber" => numbered = true,
            "activate" | "deactivate" | "title" | "box" | "link" | "links" => {}
            "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect" => {
                let operator = ["loop", "alt", "opt", "par", "critical", "break"].into_iter().find(|o| *o == word);
                stack.push(Block { operator, guards: vec![flat(after)], start: messages.len(), splits: vec![messages.len()] });
            }
            "else" | "and" | "option" => match stack.last_mut() {
                Some(block) => {
                    block.guards.push(flat(after));
                    block.splits.push(messages.len());
                }
                None => skipped += 1,
            },
            "end" => match stack.pop() {
                Some(block) => closed.push((block, messages.len())),
                None => skipped += 1,
            },
            _ => {
                // `Alice->>+Bob: text`: the arrow is the first of the table found outside the text.
                let (head, text) = line.split_once(':').map_or((line, ""), |(h, t)| (h, t));
                let found = MESSAGES.iter().filter_map(|(a, r)| head.find(a).map(|at| (at, a, r))).min_by_key(|(at, a, _)| (*at, usize::MAX - a.len()));
                match found {
                    Some((at, arrow, relation)) => {
                        let (from, to) = (head[..at].trim(), head[at + arrow.len()..].trim().trim_start_matches(['+', '-']).trim());
                        if from.is_empty() || to.is_empty() {
                            skipped += 1;
                            continue;
                        }
                        let (a, b) = (lifeline(&mut names, from), lifeline(&mut names, to));
                        messages.push((a, b, *relation, flat(text)));
                    }
                    None => skipped += 1,
                }
            }
        }
    }
    // A block never closed ends with the diagram.
    while let Some(block) = stack.pop() {
        closed.push((block, messages.len()));
    }
    let mut d = Diagram::blank();
    d.numbered = numbered;
    let mut x = 60.;
    let lives: Vec<u32> = names
        .iter()
        .map(|(_, label)| {
            let w = (Diagram::guess_width(label) + 28.).max(110.).ceil();
            let id = d.add_shape(Form::Lifeline, snap(x), 30., w, 276.);
            x += w + 110.;
            if let Some(shape) = d.shape_mut(id) {
                shape.text = label.clone();
            }
            id
        })
        .collect();
    let ids: Vec<u32> = messages
        .iter()
        .map(|(a, b, relation, text)| {
            let id = d.add_link(End::Life(lives[*a]), End::Life(lives[*b]), Head::None, Route::Straight);
            if let Some(link) = d.link_mut(id) {
                relation.apply(link);
                link.text = text.clone();
            }
            id
        })
        .collect();
    // The outer fragments first: what is wrapped is then in the operand it belongs to when the inner ones come.
    closed.sort_by_key(|(block, end)| (block.start, usize::MAX - *end));
    for (block, end) in closed {
        let Some(operator) = block.operator.filter(|_| end > block.start) else {
            skipped += usize::from(block.operator.is_some());
            continue;
        };
        let guard = |i: usize, g: &String| match (i, g.is_empty()) {
            (_, false) => format!("[{g}]"),
            (0, true) => String::new(),
            _ if operator == "alt" => "[else]".to_string(),
            _ => String::new(),
        };
        let text = std::iter::once(operator.to_string()).chain(block.guards.iter().enumerate().map(|(i, g)| guard(i, g))).collect::<Vec<_>>().join("\n");
        let Some(fragment) = d.wrap(&ids[block.start..end], &text) else {
            skipped += 1;
            continue;
        };
        for (k, from) in block.splits.iter().enumerate().skip(1) {
            let to = block.splits.get(k + 1).copied().unwrap_or(end).min(end);
            for id in ids.iter().take(to).skip(*from) {
                if let Some(link) = d.link_mut(*id).filter(|l| l.holder == Some(fragment)) {
                    link.operand = k as u32;
                }
            }
        }
    }
    d.fit_sequence();
    Converted { diagram: d, skipped }
}

// ------------------------------------------------------------------- states

#[derive(Clone, Copy, PartialEq)]
enum Node {
    State,
    Start,
    End,
    Choice,
    Bar,
}

struct StateNode {
    id: String,
    label: String,
    notes: Vec<String>,
    kind: Node,
    scope: Option<usize>,
    /// A composite state: it holds the nodes whose scope is it.
    holds: bool,
}

/// Nodes with the corner of each, and the size of them all.
type Placed = (Vec<(usize, (f32, f32))>, (f32, f32));

struct States {
    nodes: Vec<StateNode>,
    edges: Vec<(usize, usize, String)>,
}

impl States {
    fn find(&mut self, id: &str, scope: Option<usize>) -> usize {
        let id = unquoted(id);
        if let Some(i) = self.nodes.iter().position(|n| n.id == id && !matches!(n.kind, Node::Start | Node::End)) {
            return i;
        }
        self.add(id, id, Node::State, scope)
    }

    fn add(&mut self, id: &str, label: &str, kind: Node, scope: Option<usize>) -> usize {
        self.nodes.push(StateNode { id: id.to_string(), label: flat(label), notes: Vec::new(), kind, scope, holds: false });
        self.nodes.len() - 1
    }

    /// The `[*]` of a scope: its start, or its end.
    fn pseudo(&mut self, kind: Node, scope: Option<usize>) -> usize {
        match self.nodes.iter().position(|n| n.kind == kind && n.scope == scope) {
            Some(i) => i,
            None => self.add("[*]", "", kind, scope),
        }
    }

    fn lines(&self, i: usize) -> Vec<String> {
        let node = &self.nodes[i];
        let mut out = vec![node.label.clone()];
        if !node.notes.is_empty() {
            out.push(RULE.into());
            out.extend(node.notes.iter().cloned());
        }
        out
    }

    /// The size of node `i`, and for a composite state the places of what it holds.
    fn size(&self, i: usize) -> (f32, f32) {
        let node = &self.nodes[i];
        match node.kind {
            Node::Start => (20., 20.),
            Node::End => (26., 26.),
            Node::Choice => (30., 30.),
            Node::Bar => (90., 8.),
            Node::State if node.holds => {
                let (_, (w, h)) = self.arrange(Some(i));
                let head = Diagram::guess_width(&node.label) + 40.;
                ((w + 40.).max(head).max(160.), h + LANE + 36.)
            }
            Node::State => fit(&self.lines(i), (120., 44.)),
        }
    }

    /// The nodes of a scope, placed by layers, and the size of what they take.
    fn arrange(&self, scope: Option<usize>) -> Placed {
        let members: Vec<usize> = (0..self.nodes.len()).filter(|i| self.nodes[*i].scope == scope).collect();
        let sizes: Vec<(f32, f32)> = members.iter().map(|i| self.size(*i)).collect();
        // A transition between nodes of other scopes counts for the scope that holds them both.
        let place = |n: usize| members.iter().position(|m| self.within(n, *m));
        let edges: Vec<(usize, usize)> = self.edges.iter().filter_map(|(a, b, _)| Some((place(*a)?, place(*b)?))).filter(|(a, b)| a != b).collect();
        let (at, total) = layered(&sizes, &edges);
        (members.into_iter().zip(at).collect(), total)
    }

    /// Whether `n` is `holder`, or in it, however deep.
    fn within(&self, n: usize, holder: usize) -> bool {
        let mut at = Some(n);
        for _ in 0..=self.nodes.len() {
            match at {
                Some(now) if now == holder => return true,
                Some(now) => at = self.nodes[now].scope,
                None => return false,
            }
        }
        false
    }

    /// Draws the nodes of a scope with their corner at `(x, y)`; `ids[i]` is the shape of node `i`.
    fn draw(&self, d: &mut Diagram, scope: Option<usize>, (x, y): (f32, f32), ids: &mut HashMap<usize, u32>) {
        let (placed, _) = self.arrange(scope);
        for (i, (dx, dy)) in placed {
            let (w, h) = self.size(i);
            let node = &self.nodes[i];
            let form = match node.kind {
                Node::Start => Form::Dot,
                Node::End => Form::Final,
                Node::Choice => Form::Branch,
                Node::Bar => Form::Bar,
                Node::State if node.holds => Form::Composite,
                Node::State => Form::Round,
            };
            let id = d.add_shape(form, snap(x + dx), snap(y + dy), w, h);
            if matches!(node.kind, Node::State)
                && let Some(shape) = d.shape_mut(id)
            {
                shape.text = self.lines(i).join("\n");
            }
            if let Some(holder) = scope.and_then(|s| ids.get(&s)) {
                d.set_parent(id, Some(*holder));
            }
            ids.insert(i, id);
            if node.holds {
                self.draw(d, Some(i), (x + dx + 20., y + dy + LANE + 16.), ids);
            }
        }
    }
}

fn state(lines: &[&str]) -> Converted {
    let mut s = States { nodes: Vec::new(), edges: Vec::new() };
    let mut scopes: Vec<usize> = Vec::new();
    let mut skipped = 0;
    for line in lines {
        let line = line.trim_end_matches(';').trim();
        let scope = scopes.last().copied();
        if line == "}" {
            if scopes.pop().is_none() {
                skipped += 1;
            }
        } else if line.starts_with("direction ") {
        } else if let Some(rest) = line.strip_prefix("state ") {
            // `state "Long name" as id`, `state id`, `state id {`, `state id <<choice>>`.
            let opens = rest.trim_end().ends_with('{');
            let rest = rest.trim_end_matches('{').trim();
            let (rest, stereotype) = match rest.split_once("<<") {
                Some((r, k)) => (r.trim(), k.trim_end_matches(">>").trim()),
                None => (rest, ""),
            };
            let (id, label) = match rest.strip_prefix('"').and_then(|r| r.split_once('"')) {
                Some((label, tail)) => (tail.trim().trim_start_matches("as").trim(), label),
                None => (rest, rest),
            };
            let i = s.find(id, scope);
            s.nodes[i].label = flat(label);
            match stereotype {
                "choice" => s.nodes[i].kind = Node::Choice,
                "fork" | "join" => s.nodes[i].kind = Node::Bar,
                _ => {}
            }
            if opens {
                s.nodes[i].holds = true;
                scopes.push(i);
            }
        } else if let Some((from, rest)) = line.split_once("-->") {
            let (to, text) = rest.split_once(':').map_or((rest, ""), |(t, x)| (t, x));
            let (from, to) = (from.trim(), to.trim());
            let a = if from == "[*]" { s.pseudo(Node::Start, scope) } else { s.find(from, scope) };
            let b = if to == "[*]" { s.pseudo(Node::End, scope) } else { s.find(to, scope) };
            s.edges.push((a, b, flat(text)));
        } else if let Some((id, text)) = line.split_once(':') {
            let i = s.find(id.trim(), scope);
            s.nodes[i].notes.push(flat(text));
        } else {
            skipped += 1;
        }
    }
    let mut d = Diagram::blank();
    let mut ids = HashMap::new();
    s.draw(&mut d, None, (0., 0.), &mut ids);
    for (a, b, text) in &s.edges {
        let (Some(from), Some(to)) = (ids.get(a), ids.get(b)) else { continue };
        let id = d.add_link(End::Shape(*from), End::Shape(*to), Head::Arrow, Route::Elbow);
        if let Some(link) = d.link_mut(id) {
            link.text = text.clone();
        }
    }
    Converted { diagram: d, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::diagram::COLORS;

    fn made(source: &str) -> Converted {
        let c = to_diagram(source).expect("a Mermaid diagram");
        // Whatever it is, it is a valid diagram: it writes, reads back the same, and every arrow has its ends.
        let svg = c.diagram.to_svg(COLORS[0]);
        assert_eq!(Diagram::from_svg(&svg).as_ref(), Some(&c.diagram));
        assert!(c.diagram.links.iter().all(|l| c.diagram.trace(l).is_some()));
        c
    }

    fn boxes_apart(d: &Diagram) {
        let top: Vec<&crate::logic::diagram::Shape> = d.shapes.iter().filter(|s| s.parent.is_none()).collect();
        for (i, a) in top.iter().enumerate() {
            for b in &top[i + 1..] {
                let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn the_kind_is_the_first_line_that_counts() {
        assert_eq!(kind("classDiagram\n A <|-- B"), Some(Kind::Class));
        assert_eq!(kind("%% a comment\n\n  sequenceDiagram"), Some(Kind::Sequence));
        assert_eq!(kind("---\ntitle: x\n---\nstateDiagram-v2\n[*] --> A"), Some(Kind::State));
        assert_eq!(kind("stateDiagram"), Some(Kind::State));
        // Not one of the three: a flowchart, a word, nothing.
        for source in ["graph TD\n A --> B", "flowchart LR", "pie title x", "", "   \n\n", "classDiagrams", "sequence"] {
            assert!(kind(source).is_none() && to_diagram(source).is_none(), "{source:?}");
        }
    }

    #[test]
    fn a_class_diagram_keeps_its_classes_members_and_relations() {
        let c = made(
            "classDiagram
    class Animal {
        <<abstract>>
        +String name
        +int age
        +eat(food) bool
        -sleep()
    }
    class Duck
    Animal <|-- Duck
    Animal <|-- Fish : is a
    Duck \"1\" --> \"*\" Egg : lays
    Pond o-- Duck
    Farm *-- \"2..*\" Pond
    Duck ..> Water
    Duck -- Fish
    Duck : +swim()
    direction RL
    note for Duck \"a note\"
",
        );
        let d = &c.diagram;
        assert_eq!(c.skipped, 1, "only the note is not understood");
        let text = |name: &str| d.shapes.iter().find(|s| s.text.lines().any(|l| l == name)).unwrap();
        // Members are in compartments, the type after the name, the stereotype first.
        assert_eq!(text("Animal").text, "«abstract»\nAnimal\n---\n+ name: String\n+ age: int\n---\n+ eat(food): bool\n- sleep()");
        assert_eq!(text("Duck").text, "Duck\n---\n+ swim()");
        let rel = |from: &str, to: &str| d.links.iter().find(|l| matches!((l.from, l.to), (End::Shape(a), End::Shape(b)) if a == text(from).id && b == text(to).id)).unwrap();
        // A parent is the end of its arrow; a whole is its start; the multiplicities and the label come along.
        assert_eq!(Relation::of(rel("Duck", "Animal")), Some(Relation::Generalization));
        assert_eq!(rel("Fish", "Animal").text, "is a");
        let lays = rel("Duck", "Egg");
        assert_eq!((Relation::of(lays), lays.start_text.as_str(), lays.end_text.as_str(), lays.text.as_str()), (Some(Relation::Navigable), "1", "*", "lays"));
        assert_eq!(Relation::of(rel("Pond", "Duck")), Some(Relation::Aggregation));
        let farm = rel("Farm", "Pond");
        assert_eq!((Relation::of(farm), farm.end_text.as_str()), (Some(Relation::Composition), "2..*"));
        assert_eq!(Relation::of(rel("Duck", "Water")), Some(Relation::Dependency));
        assert_eq!(Relation::of(rel("Duck", "Fish")), Some(Relation::Association));
        // The parent is above its children, and nothing overlaps.
        assert!(text("Animal").y < text("Duck").y && text("Animal").y < text("Fish").y);
        boxes_apart(d);
    }

    #[test]
    fn a_sequence_diagram_keeps_its_order_and_its_fragments() {
        let c = made(
            "sequenceDiagram
    autonumber
    participant C as Client
    actor S as Server
    C->>+S: login()
    alt known
        S-->>-C: token
    else unknown
        S--xC: refuse
        loop 3 times
            C-)S: retry
        end
    end
    opt
        C->>C: log
    end
    rect rgb(0,0,0)
        S->>DB: save
    end
    Note over C,S: hello
    end
",
        );
        let d = &c.diagram;
        assert!(d.numbered);
        assert_eq!(c.skipped, 2, "the note and the end with no beginning");
        let lives: Vec<&str> = d.shapes.iter().filter(|s| s.form == Form::Lifeline).map(|s| s.text.as_str()).collect();
        assert_eq!(lives, ["Client", "Server", "DB"]);
        let texts: Vec<&str> = d.messages().iter().map(|id| d.link(*id).unwrap().text.as_str()).collect();
        assert_eq!(texts, ["login()", "token", "refuse", "retry", "log", "save"]);
        let relations: Vec<Option<Relation>> = d.messages().iter().map(|id| Relation::of(d.link(*id).unwrap())).collect();
        assert_eq!(relations[..4], [Some(Relation::Sync), Some(Relation::Dependency), Some(Relation::NonNavigable), Some(Relation::Navigable)]);
        // alt with its two operands, a loop inside the second, an opt of one message; the rect is only a colour.
        let fragments: Vec<&str> = d.shapes.iter().filter(|s| s.form == Form::Fragment).map(|s| s.text.as_str()).collect();
        assert_eq!(fragments, ["alt\n[known]\n[unknown]", "loop\n[3 times]", "opt\n"]);
        let held = |text: &str| d.link(d.messages()[texts.iter().position(|t| *t == text).unwrap()]).map(|l| (l.holder.is_some(), l.operand)).unwrap();
        assert_eq!((held("login()"), held("token"), held("refuse"), held("retry"), held("save")), ((false, 0), (true, 0), (true, 1), (true, 0), (false, 0)));
        let alt = d.shapes.iter().find(|s| s.text.starts_with("alt")).unwrap();
        let lp = d.shapes.iter().find(|s| s.text.starts_with("loop")).unwrap();
        assert_eq!((lp.parent, lp.operand), (Some(alt.id), 1));
        // Messages are in order, apart, each lower than the one before.
        let ys: Vec<f32> = d.messages().iter().map(|id| d.message_y(*id).unwrap()).collect();
        assert!(ys.windows(2).all(|w| w[1] > w[0] + 20.), "{ys:?}");
    }

    #[test]
    fn a_state_diagram_keeps_its_states_composites_and_pseudostates() {
        let c = made(
            "stateDiagram-v2
    [*] --> Still
    Still --> [*]
    Still --> Moving : push
    Moving --> Still
    Moving --> Crash
    Crash --> [*]
    state \"Long name\" as Crash
    state Fork <<fork>>
    state Pick <<choice>>
    Moving --> Fork
    Fork --> Pick
    Still : is at rest
    state Active {
        [*] --> Run
        Run --> [*]
        Run --> Run : tick
    }
    Pick --> Active
    Active --> Active
    --
",
        );
        let d = &c.diagram;
        assert_eq!(c.skipped, 1, "the line of regions only");
        let by = |text: &str| d.shapes.iter().find(|s| s.text.lines().next() == Some(text)).unwrap();
        assert_eq!(by("Still").text, "Still\n---\nis at rest");
        assert_eq!(by("Long name").form, Form::Round);
        assert_eq!((by("Active").form, by("Run").parent), (Form::Composite, Some(by("Active").id)));
        // One start and one end for the scope, one for the composite; they are held by it.
        let count = |form| d.shapes.iter().filter(|s| s.form == form).count();
        assert_eq!((count(Form::Dot), count(Form::Final), count(Form::Bar), count(Form::Branch)), (2, 2, 1, 1));
        assert_eq!(d.shapes.iter().filter(|s| s.parent == Some(by("Active").id)).count(), 3);
        let push = d.links.iter().find(|l| l.text == "push").unwrap();
        assert_eq!(push.end, Head::Arrow);
        // The states of the scope do not overlap each other, and the composite holds what it holds.
        boxes_apart(d);
        let active = by("Active");
        assert!(d.shapes.iter().filter(|s| s.parent == Some(active.id)).all(|s| s.x >= active.x && s.y >= active.y + LANE && s.x + s.w <= active.x + active.w && s.y + s.h <= active.y + active.h));
    }

    #[test]
    fn what_cannot_be_read_is_skipped_and_never_breaks_the_rest() {
        // Only the header; the header and garbage; an unclosed block and an unclosed class; a loop of fragments left open.
        for source in ["classDiagram", "sequenceDiagram", "stateDiagram", "classDiagram\n???\n{{{\n}}}", "sequenceDiagram\nloop forever\nA->>B: x\nalt\nB->>A: y", "classDiagram\nclass A {\n+x", "stateDiagram\nstate A {\nstate B {\nA --> B", "stateDiagram\n} \n}", "sequenceDiagram\nelse\nend\nend"] {
            let c = made(source);
            let _ = c.skipped;
        }
        assert!(made("sequenceDiagram\nloop forever\nA->>B: x\nalt\nB->>A: y").diagram.shapes.iter().filter(|s| s.form == Form::Fragment).count() == 2);
        // A block with no message is not drawn, and says so.
        assert_eq!(made("sequenceDiagram\nloop x\nend\nA->>B: m").skipped, 1);
        // Names with accents, CJK, emoji, `|`, `\`; a name that is only an arrow; a message with a colon in its text.
        let c = made("classDiagram\nclass Éléphant\n日本語 <|-- 😀\nA|B --> C\\D : x: y\n--> E");
        assert!(c.diagram.shapes.len() >= 5);
        let c = made("sequenceDiagram\nÀ->>日本: a: b: c\n😀-->>À: |\\");
        assert_eq!(c.diagram.messages().len(), 2);
        assert_eq!(c.diagram.links[0].text, "a: b: c");
        // A relation to itself, a cycle, two relations between the same classes, the same state declared twice.
        let c = made("classDiagram\nA --> A\nA --> B\nB --> A\nA --> B");
        assert_eq!((c.diagram.shapes.len(), c.diagram.links.len()), (2, 4));
        boxes_apart(&c.diagram);
        let c = made("stateDiagram\nstate A\nstate A\nA --> A\nA --> B\nB --> A");
        assert_eq!((c.diagram.shapes.len(), c.diagram.links.len()), (2, 3));
    }

    #[test]
    fn a_big_diagram_is_made_in_no_time() {
        let mut classes = String::from("classDiagram\n");
        for i in 0..500 {
            classes += &format!("class C{i} {{\n+int a{i}\n+run()\n}}\nC{} <|-- C{i}\nC{i} --> C{}\n", i / 2, (i * 7) % 500);
        }
        let mut sequence = String::from("sequenceDiagram\n");
        for i in 0..800 {
            sequence += &format!("P{}->>P{}: m{i}\n", i % 30, (i + 1) % 30);
            if i % 50 == 0 {
                sequence += &format!("opt c{i}\nP1->>P2: in{i}\nend\n");
            }
        }
        let mut states = String::from("stateDiagram-v2\n");
        for i in 0..500 {
            states += &format!("S{i} --> S{} : e{i}\nS{i} --> S{}\n", i + 1, (i * 3) % 500);
        }
        for (name, source) in [("class", classes), ("sequence", sequence), ("state", states)] {
            let started = std::time::Instant::now();
            let c = to_diagram(&source).unwrap();
            let took = started.elapsed();
            assert!(!c.diagram.shapes.is_empty() && took.as_secs_f32() < 5., "{name}: {took:?}");
            eprintln!("{name}: {} shapes, {} arrows in {took:?}", c.diagram.shapes.len(), c.diagram.links.len());
        }
    }
}
