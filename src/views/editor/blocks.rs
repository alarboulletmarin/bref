//! Blocks: the handle left of each block, its menu, and dragging blocks around.

use super::*;

/// Block handles: the block under the pointer (at its text version), the one being dragged, the
/// one the handle's menu targets, and the zones of the handle and of the « + ».
#[derive(Default)]
pub(super) struct BlockHandles {
    pub(super) hover: Option<(Range<usize>, u64)>,
    pub(super) drag: Option<BlockDrag>,
    pub(super) menu: Option<(Range<usize>, u64)>,
    pub(super) hitboxes: Option<(Hitbox, Option<Hitbox>)>,
}

/// Un geste sur un bloc de la note, depuis sa poignée, le clavier ou la palette : le transformer, le
/// doubler, le déplacer d'un bloc, le supprimer (voir `block`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BlockOp {
    Turn(block::Turn),
    Duplicate,
    Up,
    Down,
    Delete,
}

impl BlockOp {
    pub const ALL: [BlockOp; 13] = [
        BlockOp::Turn(block::Turn::Text),
        BlockOp::Turn(block::Turn::Heading(1)),
        BlockOp::Turn(block::Turn::Heading(2)),
        BlockOp::Turn(block::Turn::Heading(3)),
        BlockOp::Turn(block::Turn::Bullet),
        BlockOp::Turn(block::Turn::Ordered),
        BlockOp::Turn(block::Turn::Task),
        BlockOp::Turn(block::Turn::Quote),
        BlockOp::Turn(block::Turn::Code),
        BlockOp::Duplicate,
        BlockOp::Up,
        BlockOp::Down,
        BlockOp::Delete,
    ];

    pub fn label(self) -> &'static str {
        use block::Turn;
        match self {
            BlockOp::Turn(Turn::Text) => tr("Text", "Texte"),
            BlockOp::Turn(Turn::Heading(1)) => tr("Heading 1", "Titre 1"),
            BlockOp::Turn(Turn::Heading(2)) => tr("Heading 2", "Titre 2"),
            BlockOp::Turn(Turn::Heading(_)) => tr("Heading 3", "Titre 3"),
            BlockOp::Turn(Turn::Bullet) => tr("Bulleted list", "Liste à puces"),
            BlockOp::Turn(Turn::Ordered) => tr("Numbered list", "Liste numérotée"),
            BlockOp::Turn(Turn::Task) => tr("Task list", "Liste de tâches"),
            BlockOp::Turn(Turn::Quote) => tr("Quote", "Citation"),
            BlockOp::Turn(Turn::Code) => tr("Code block", "Bloc de code"),
            BlockOp::Duplicate => tr("Duplicate", "Dupliquer"),
            BlockOp::Up => tr("Move up", "Monter"),
            BlockOp::Down => tr("Move down", "Descendre"),
            BlockOp::Delete => tr("Delete", "Supprimer"),
        }
    }

    /// Ce que dit l'aide : le geste, et sur quoi il porte.
    pub fn effect(self) -> &'static str {
        use block::Turn;
        match self {
            BlockOp::Turn(Turn::Text) => tr("Block: turn into text", "Bloc : en texte"),
            BlockOp::Turn(Turn::Heading(1)) => tr("Block: turn into heading 1", "Bloc : en titre 1"),
            BlockOp::Turn(Turn::Heading(2)) => tr("Block: turn into heading 2", "Bloc : en titre 2"),
            BlockOp::Turn(Turn::Heading(_)) => tr("Block: turn into heading 3", "Bloc : en titre 3"),
            BlockOp::Turn(Turn::Bullet) => tr("Block: turn into a bulleted list", "Bloc : en liste à puces"),
            BlockOp::Turn(Turn::Ordered) => tr("Block: turn into a numbered list", "Bloc : en liste numérotée"),
            BlockOp::Turn(Turn::Task) => tr("Block: turn into a task list", "Bloc : en liste de tâches"),
            BlockOp::Turn(Turn::Quote) => tr("Block: turn into a quote", "Bloc : en citation"),
            BlockOp::Turn(Turn::Code) => tr("Block: turn into a code block", "Bloc : en bloc de code"),
            BlockOp::Duplicate => tr("Block: duplicate", "Bloc : dupliquer"),
            BlockOp::Up => tr("Block: move up (in a table, the row)", "Bloc : monter (dans un tableau, la ligne)"),
            BlockOp::Down => tr("Block: move down (in a table, the row)", "Bloc : descendre (dans un tableau, la ligne)"),
            BlockOp::Delete => tr("Block: delete", "Bloc : supprimer"),
        }
    }

    /// L'action qui fait la même chose au clavier.
    pub fn action(self) -> Box<dyn gpui::Action> {
        use block::Turn;
        match self {
            BlockOp::Turn(Turn::Text) => Box::new(BlockText),
            BlockOp::Turn(Turn::Heading(1)) => Box::new(BlockHeading1),
            BlockOp::Turn(Turn::Heading(2)) => Box::new(BlockHeading2),
            BlockOp::Turn(Turn::Heading(_)) => Box::new(BlockHeading3),
            BlockOp::Turn(Turn::Bullet) => Box::new(BlockBullet),
            BlockOp::Turn(Turn::Ordered) => Box::new(BlockOrdered),
            BlockOp::Turn(Turn::Task) => Box::new(BlockTask),
            BlockOp::Turn(Turn::Quote) => Box::new(BlockQuote),
            BlockOp::Turn(Turn::Code) => Box::new(BlockCode),
            BlockOp::Duplicate => Box::new(BlockDuplicate),
            BlockOp::Up => Box::new(BlockUp),
            BlockOp::Down => Box::new(BlockDown),
            BlockOp::Delete => Box::new(BlockDelete),
        }
    }
}

/// Un bloc tiré par sa poignée : où on l'a pris, où il se poserait, et s'il a déjà bougé (sinon,
/// lâché sur place, c'est un clic, qui ouvre son menu).
pub(super) struct BlockDrag {
    pub(super) block: Range<usize>,
    pub(super) from: Point<Pixels>,
    pub(super) to: Option<usize>,
    pub(super) moved: bool,
}

/// Une poignée est une pastille de 10 px sur 24 ; elle flotte à 5 px du tableau.
pub(super) const GRIP: Pixels = px(10.);

pub(super) const GRIP_LONG: Pixels = px(24.);

pub(super) const GRIP_GAP: Pixels = px(5.);

/// Jusqu'où, autour d'un tableau, le pointeur fait paraître la poignée de sa colonne ou de sa ligne.
pub(super) const GRIP_REACH: Pixels = px(24.);

impl Editor {
    /// Ce qui se clique sous le pointeur prend la main : une poignée de tableau (ouverte, fermée
    /// quand on la tire), un trait où insérer, une case à cocher, un lien quand `Ctrl` est tenu.
    pub(super) fn hand_at(&self, at: Point<Pixels>, held: gpui::Modifiers) -> Option<CursorStyle> {
        if self.moving.is_some() {
            return Some(CursorStyle::ClosedHand);
        }
        if !self.viewport.contains(&at) {
            return None;
        }
        if let Some((_, part)) = self.part_at(at) {
            return Some(match part {
                Part::JoinCol(_) | Part::JoinRow(_) => CursorStyle::PointingHand,
                Part::Col(_) | Part::Row(_) | Part::Corner => CursorStyle::OpenHand,
            });
        }
        if self.rows.is_empty() {
            return None;
        }
        if !held.secondary() && self.checkbox_at(at).is_some() {
            return Some(CursorStyle::PointingHand);
        }
        let i = self.index_at(at);
        let lr = self.line_range(i);
        let (line, col) = (&self.content[lr.clone()], i - lr.start);
        if held.secondary() && md::links(line).into_iter().any(|(r, _)| r.contains(&col)) {
            return Some(CursorStyle::PointingHand);
        }
        let (kind, marker) = md::classify(line, self.in_code(lr.start));
        (matches!(kind, Kind::Task(_)) && col + 4 >= marker && col < marker).then_some(CursorStyle::PointingHand)
    }

    pub(super) fn aim_hand(&mut self, at: Point<Pixels>, held: gpui::Modifiers, cx: &mut Context<Self>) {
        let hand = self.hand_at(at, held);
        if hand != self.hand {
            self.hand = hand;
            cx.notify();
        }
    }

    /// Les blocs de premier niveau de la note, refaits une fois par version du texte.
    pub(super) fn top_blocks(&mut self) -> Rc<Vec<Range<usize>>> {
        match &self.blocks {
            Some((version, blocks)) if *version == self.version => blocks.clone(),
            _ => {
                let mut blocks = block::blocks(&self.content);
                // La première ligne est le titre de la note, et le nom de son fichier : elle n'a pas de
                // poignée, et rien ne passe devant elle.
                if !blocks.is_empty() {
                    blocks.remove(0);
                }
                let blocks = Rc::new(blocks);
                self.blocks = Some((self.version, blocks.clone()));
                blocks
            }
        }
    }

    /// Le bloc que montre la poignée : celui qu'on tire, sinon celui sous le pointeur, tant que le texte
    /// n'a pas changé depuis.
    pub(super) fn handled(&self) -> Option<Range<usize>> {
        if let Some(drag) = &self.handles.drag {
            return Some(drag.block.clone());
        }
        self.handles.hover.clone().filter(|(_, version)| *version == self.version).map(|(block, _)| block)
    }

    /// Bord gauche de ce qu'occupe la ligne qui commence en `at` : la page, ou le cadre de son tableau
    /// (plus large qu'elle, et dont les poignées prennent déjà la marge proche).
    /// Un item en retrait a sa poignée à son retrait.
    pub(super) fn block_left(&self, at: usize) -> Pixels {
        if let Some((_, frame)) = self.tables.iter().find(|(range, _)| range.contains(&at)) {
            return frame.left() - px(20.);
        }
        let line = &self.content[self.line_range(at)];
        let indent = line.len() - line.trim_start().len();
        let lead = self.row_at(at).map_or(px(0.), |r| self.rows[r].pos(indent).x);
        self.origin.x + lead
    }

    /// La poignée du bloc montré et son « + » (absent quand la marge est trop étroite pour lui).
    pub(super) fn handle_bounds(&self) -> Option<(Bounds<Pixels>, Option<Bounds<Pixels>>)> {
        let block = self.handled()?;
        let row = &self.rows[self.row_at(block.start)?];
        if row.hidden() {
            return None;
        }
        let left = self.block_left(block.start);
        let mid = self.origin.y + row.y + row.pad + row.lh / 2.;
        let grip = Bounds::new(point(left - px(24.), mid - px(10.)), size(px(18.), px(20.)));
        let plus = Bounds::new(point(left - px(44.), mid - px(10.)), size(px(18.), px(20.)));
        if grip.left() < self.viewport.left() {
            return None;
        }
        Some((grip, (plus.left() >= self.viewport.left()).then_some(plus)))
    }

    /// Le haut de la ligne qui commence en `at` à l'écran ; la fin du texte : le bas de la dernière ligne.
    pub(super) fn line_top(&self, at: usize) -> Option<Pixels> {
        if at >= self.content.len() {
            let row = self.rows.iter().rev().find(|r| !r.hidden())?;
            return Some(self.origin.y + row.y + row.height);
        }
        let row = &self.rows[self.row_at(at)?];
        Some(self.origin.y + row.y)
    }

    /// Le bloc sous le pointeur prend une poignée ; le pointeur sur la poignée la garde.
    pub(super) fn aim_block(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.handles.drag.is_some() || self.selecting || self.moving.is_some() {
            return;
        }
        if self.handle_bounds().is_some_and(|(grip, plus)| grip.dilate(px(4.)).contains(&at) || plus.is_some_and(|p| p.dilate(px(4.)).contains(&at))) {
            return;
        }
        let hover = self.block_under(at).map(|block| (block, self.version));
        if hover != self.handles.hover {
            self.handles.hover = hover;
            cx.notify();
        }
    }

    /// Le bloc de la ligne sous le pointeur, s'il est sur la page ou dans sa marge de gauche.
    pub(super) fn block_under(&mut self, at: Point<Pixels>) -> Option<Range<usize>> {
        if self.rows.is_empty() || !self.viewport.contains(&at) || at.x < self.origin.x - px(140.) || at.x > self.origin.x + self.width + px(24.) {
            return None;
        }
        let i = self.index_at(at);
        let line = self.line_range(i).start;
        let row = &self.rows[self.row_at(line)?];
        let top = self.origin.y + row.y;
        // Sous la dernière ligne, ou dans l'espace entre deux : aucune.
        let last = &self.rows[self.row_at(self.line_range(i).end)?];
        if at.y < top || at.y > self.origin.y + last.y + last.height {
            return None;
        }
        let top_blocks = self.top_blocks();
        block::at(&self.content, &top_blocks, line)
    }

    /// Le bloc visé par un geste : celui de la poignée tant que le curseur y est, sinon celui du curseur.
    pub(super) fn block_target(&mut self) -> Option<Range<usize>> {
        let cursor = self.cursor();
        if let Some((block, version)) = self.handles.menu.clone()
            && version == self.version
            && (block.start..=block.end).contains(&cursor)
        {
            return Some(block);
        }
        let top = self.top_blocks();
        block::at(&self.content, &top, self.line_range(cursor).start)
    }

    /// Fait un geste sur le bloc visé ; le bloc qui en sort reste sélectionné, et visé.
    pub fn block_op(&mut self, op: BlockOp, cx: &mut Context<Self>) {
        if !self.more.is_empty() {
            return;
        }
        // Dans un tableau, monter et descendre déplacent sa ligne, comme avant.
        if matches!(op, BlockOp::Up | BlockOp::Down) && self.table_at(self.cursor()).is_some() {
            return self.table_op(if op == BlockOp::Up { TableOp::MoveRowUp } else { TableOp::MoveRowDown }, cx);
        }
        let Some(block) = self.block_target() else { return };
        let old = self.content.clone();
        let top = self.top_blocks();
        let (new, sel) = match op {
            BlockOp::Turn(turn) => {
                let turned = block::turned(&old[block.clone()], turn);
                let mut new = old.clone();
                new.replace_range(block.clone(), &turned);
                (new, block.start..block.start + turned.len())
            }
            BlockOp::Duplicate => block::duplicated(&old, block),
            BlockOp::Delete => {
                let (new, at) = block::removed(&old, block);
                (new, at..at)
            }
            BlockOp::Up | BlockOp::Down => {
                let targets = block::targets(&old, &top);
                let to = match op {
                    // Avant le bloc qui précède.
                    BlockOp::Up => targets.iter().rev().find(|&&t| t < block.start).copied(),
                    // Après le bloc qui suit : avant celui d'après, ou à la fin.
                    _ => targets.iter().find(|&&t| t > block.end).and_then(|&next| {
                        let next = block::at(&old, &top, next).map_or(next, |b| b.end);
                        targets.iter().find(|&&t| t > next).copied()
                    }),
                };
                let Some(moved) = to.and_then(|to| block::moved(&old, block, to)) else { return };
                moved
            }
        };
        if new == old {
            return;
        }
        self.replace_block(&old, &new, cx);
        self.sel = sel.clone();
        self.reversed = false;
        self.handles.menu = Some((sel, self.version));
        self.reveal = true;
        cx.notify();
    }

    /// Passe de `old` (le texte actuel) à `new` en ne remplaçant que ce qui change, en une seule
    /// étape d'annulation : `splice` en poserait une seconde pour un texte de plusieurs lignes.
    pub(super) fn replace_block(&mut self, old: &str, new: &str, cx: &mut Context<Self>) {
        let (span, text) = block::diff(old, new);
        self.push_undo(true);
        self.held = true;
        self.edit(span, &text, cx);
        self.held = false;
    }

    /// Le « + » d'un bloc : une ligne neuve sous lui, séparée comme les blocs qui l'entourent, où
    /// `/` ouvre déjà la liste des composants à poser.
    pub(super) fn add_block_after(&mut self, block: Range<usize>, cx: &mut Context<Self>) {
        let rest = &self.content[block.end..];
        let gap = rest.len() - rest.trim_start_matches('\n').len();
        let sep = "\n".repeat(gap.clamp(1, 2));
        self.push_undo(true);
        self.held = true;
        self.edit(block.end..block.end, &format!("{sep}/"), cx);
        self.held = false;
        self.ac_index = 0;
        self.ac_dismissed = None;
        self.handles.hover = None;
        self.reveal = true;
        cx.notify();
    }

    /// Le bloc tiré suit le pointeur : il se poserait à la place la plus proche.
    pub(super) fn drag_block(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = &self.handles.drag else { return };
        let moved = drag.moved || (at - drag.from).magnitude() > 4.;
        let top = self.top_blocks();
        let targets = block::targets(&self.content, &top);
        let to = targets
            .into_iter()
            .filter_map(|t| Some((t, self.line_top(t)?)))
            .min_by(|a, b| (a.1 - at.y).abs().partial_cmp(&(b.1 - at.y).abs()).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(t, _)| t);
        if let Some(drag) = &mut self.handles.drag {
            (drag.moved, drag.to) = (moved, to);
        }
        cx.notify();
    }

    /// Lâché : posé à sa nouvelle place ; sans avoir bougé, c'est un clic, qui prend le bloc et ouvre son menu.
    pub(super) fn drop_block(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.handles.drag.take() else { return };
        if !drag.moved {
            let anchor = self.handle_bounds_of(&drag.block).map_or(drag.from, |grip| point(grip.left(), grip.bottom() + px(4.)));
            self.move_to(drag.block.start, cx);
            self.select_to(drag.block.end, cx);
            self.handles.menu = Some((drag.block, self.version));
            cx.emit(EditorEvent::BlockMenu(anchor));
            return cx.notify();
        }
        let old = self.content.clone();
        if let Some((new, sel)) = drag.to.and_then(|to| block::moved(&old, drag.block, to)) {
            self.replace_block(&old, &new, cx);
            self.sel = sel.clone();
            self.reversed = false;
            self.handles.menu = Some((sel, self.version));
        }
        cx.notify();
    }

    pub(super) fn handle_bounds_of(&self, block: &Range<usize>) -> Option<Bounds<Pixels>> {
        let row = &self.rows[self.row_at(block.start)?];
        let mid = self.origin.y + row.y + row.pad + row.lh / 2.;
        Some(Bounds::new(point(self.block_left(block.start) - px(24.), mid - px(10.)), size(px(18.), px(20.))))
    }

    /// Où est la poignée du bloc sous le pointeur, pour les tests.
    #[cfg(test)]
    pub fn block_handle(&self) -> Option<(Bounds<Pixels>, Option<Bounds<Pixels>>)> {
        self.handle_bounds()
    }

    /// Le bloc sous la poignée, pour les tests.
    #[cfg(test)]
    pub fn hovered_block(&self) -> Option<&str> {
        self.handled().map(|block| &self.content[block])
    }
}
