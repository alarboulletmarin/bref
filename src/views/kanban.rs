//! Tableau kanban : une note montrée en colonnes. Chaque titre `##` est une colonne, chaque
//! tâche `- [ ]` sous lui une carte ; le fichier reste du Markdown (la disposition du greffon
//! Kanban d'Obsidian). Les fonctions pures d'abord, puis le tableau lui-même (`impl Shell`).

use std::ops::Range;

use gpui::{
    App, Bounds, Context, Div, ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, Pixels,
    Point, UTF16Selection, Window, canvas, div, prelude::*, px, svg,
};

use crate::{
    ui::theme::Theme,
    logic::markdown::{self, Kind},
    ui::line::{self, Line},
    views::palette::{Confirm, Dismiss},
    lang::tr,
};

#[derive(Debug, PartialEq)]
pub struct Card<'a> {
    pub text: &'a str,
    pub done: bool,
    /// Sa ligne dans la note, fin de ligne comprise.
    pub line: Range<usize>,
}

#[derive(Debug, PartialEq)]
pub struct Column<'a> {
    pub title: &'a str,
    /// La ligne de son titre, fin de ligne comprise.
    pub line: Range<usize>,
    /// Où poser une carte de plus : après la dernière, ou après le titre.
    pub end: usize,
    pub cards: Vec<Card<'a>>,
}

/// What a pane keeps of the board it shows.
pub struct Board {
    /// La note ouverte est un tableau kanban dont on regarde le texte : on l'a demandé, ou on
    /// vient d'en écrire la clé (taper `kanban: true` ne fait pas quitter le texte).
    pub text: bool,
    /// La note était un tableau à sa dernière frappe.
    pub was: bool,
    /// Carte qu'on déplace dans le tableau, et où elle se poserait (colonne, rang).
    pub drag: Option<((usize, usize), (usize, usize))>,
    /// Largeur d'une carte du tableau, relevée à son dessin.
    pub card: std::rc::Rc<std::cell::Cell<Pixels>>,
    pub scroll: gpui::ScrollHandle,
    /// Ce qu'on écrit dans le tableau, et son champ de saisie.
    pub field: Option<(Slot, Entity<Field>)>,
    /// Le tableau reçoit la saisie à la place de la note qu'il cache.
    pub focus: FocusHandle,
}

impl Board {
    pub fn new(cx: &mut App) -> Self {
        Board { text: false, was: false, drag: None, card: Default::default(), scroll: Default::default(), field: None, focus: cx.focus_handle() }
    }
}

/// La note s'ouvre en tableau : son en-tête YAML porte la clé `kanban` (ou `kanban-plugin`,
/// celle d'Obsidian).
pub fn is_board(text: &str) -> bool {
    markdown::front_keys(text).iter().any(|key| matches!(*key, "kanban" | "kanban-plugin"))
}

/// Les colonnes de la note et leurs cartes. Une tâche en retrait est le détail d'une carte, pas
/// une carte ; ce qui précède le premier `##` n'est dans aucune colonne.
pub fn columns(text: &str) -> Vec<Column<'_>> {
    let mut found: Vec<Column> = Vec::new();
    let mut at = markdown::front_matter(text);
    for line in text[at..].split_inclusive('\n') {
        let end = at + line.len();
        let bare = line.trim_end_matches(['\n', '\r']);
        match (markdown::classify(bare, false), found.last_mut()) {
            ((Kind::Heading(_), marker), _) if bare.starts_with("## ") => found.push(Column { title: bare[marker..].trim(), line: at..end, end, cards: Vec::new() }),
            ((Kind::Task(done), marker), Some(column)) if bare.starts_with('-') => {
                column.cards.push(Card { text: bare[marker..].trim(), done, line: at..end });
                column.end = end;
            }
            _ => {}
        }
        at = end;
    }
    found
}

/// La note une fois la carte `from` (colonne, rang) posée dans la colonne `to.0`, avant la carte
/// de rang `to.1` (ou à la fin si ce rang n'existe pas).
pub fn move_card(text: &str, from: (usize, usize), to: (usize, usize)) -> Option<String> {
    let all = columns(text);
    let card = all.get(from.0)?.cards.get(from.1)?;
    let column = all.get(to.0)?;
    let into = column.cards.get(to.1).map_or(column.end, |next| next.line.start);
    // Posée sur elle-même ou juste après elle : rien ne bouge.
    if (card.line.start..=card.line.end).contains(&into) {
        return None;
    }
    let mut line = text[card.line.clone()].to_string();
    if !line.ends_with('\n') {
        line.push('\n');
    }
    let mut moved = String::with_capacity(text.len() + 1);
    let (first, second) = (card.line.start.min(into), card.line.start.max(into));
    moved.push_str(&text[..first]);
    if into < card.line.start {
        moved.push_str(&line);
        moved.push_str(&text[into..card.line.start]);
        moved.push_str(&text[card.line.end..]);
    } else {
        moved.push_str(&text[card.line.end..second]);
        // La colonne d'arrivée finit le fichier sans fin de ligne : la carte en ouvre une.
        if !moved.ends_with('\n') {
            moved.push('\n');
        }
        moved.push_str(&line);
        moved.push_str(&text[second..]);
    }
    Some(moved)
}

/// La note une fois la carte cochée, ou décochée.
pub fn tick(text: &str, at: (usize, usize)) -> Option<String> {
    let all = columns(text);
    let card = all.get(at.0)?.cards.get(at.1)?;
    let open = card.line.start + text[card.line.clone()].find('[')?;
    let mark = if card.done { " " } else { "x" };
    Some(format!("{}{mark}{}", &text[..open + 1], &text[open + 2..]))
}

/// La note avec la carte `card` de plus au bas de la colonne.
pub fn add_card(text: &str, column: usize, card: &str) -> Option<String> {
    let end = columns(text).get(column)?.end;
    let lead = if text[..end].ends_with('\n') { "" } else { "\n" };
    Some(format!("{}{lead}- [ ] {}\n{}", &text[..end], card.trim(), &text[end..]))
}

/// La note une fois la carte réécrite ; réécrite à vide, elle est retirée.
pub fn retitle(text: &str, at: (usize, usize), card: &str) -> Option<String> {
    let all = columns(text);
    let old = all.get(at.0)?.cards.get(at.1)?;
    if card.trim().is_empty() {
        return Some(format!("{}{}", &text[..old.line.start], &text[old.line.end..]));
    }
    // Le texte de la carte est une tranche de la note : sa place s'en déduit.
    let start = old.text.as_ptr() as usize - text.as_ptr() as usize;
    Some(format!("{}{}{}", &text[..start], card.trim(), &text[start + old.text.len()..]))
}

/// La note une fois la colonne renommée ; un titre vide ne change rien.
pub fn rename_column(text: &str, column: usize, title: &str) -> Option<String> {
    let all = columns(text);
    let old = all.get(column)?;
    let start = old.title.as_ptr() as usize - text.as_ptr() as usize;
    (!title.trim().is_empty()).then(|| format!("{}{}{}", &text[..start], title.trim(), &text[start + old.title.len()..]))
}

/// La note avec une colonne de plus, à la fin.
pub fn add_column(text: &str, title: &str) -> Option<String> {
    let lead = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
    (!title.trim().is_empty()).then(|| format!("{text}{lead}## {}\n", title.trim()))
}

/// Le texte d'un tableau neuf : ses trois colonnes, et la clé qui le fait s'ouvrir en tableau.
pub fn new_board() -> String {
    let columns = [tr("To do", "À faire"), tr("Doing", "En cours"), tr("Done", "Fait")].map(|title| format!("## {title}\n\n")).concat();
    format!("---\nkanban: true\n---\n# {}\n\n{}", tr("Board", "Tableau"), columns.trim_end_matches('\n').to_string() + "\n")
}

/// Ce qu'on est en train d'écrire dans le tableau.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
    /// Une carte de plus au bas de cette colonne.
    New(usize),
    Card(usize, usize),
    Column(usize),
    /// Une colonne de plus.
    NewColumn,
}

pub enum FieldEvent {
    /// Entrée : le texte est validé.
    Done,
    Cancel,
}

/// Champ de saisie posé dans le tableau, à la place de ce qu'il réécrit.
pub struct Field {
    focus: FocusHandle,
    pub line: Line,
    theme: Theme,
}

impl EventEmitter<FieldEvent> for Field {}

impl Field {
    pub fn new(text: &str, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self { focus: cx.focus_handle(), line: Line::new(text), theme }
    }
}

impl Focusable for Field {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for Field {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(self.line.utf16())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.line.insert(text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(range, text, window, cx)
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let (focus, entity) = (self.focus.clone(), cx.entity());
        let input = canvas(|_, _, _| (), move |bounds, _, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx)).size_0();
        let (shown, layout) = self.line.shown(t);
        let field = div()
            // Les touches de la palette (Entrée valide, Échap renonce) et celles d'une ligne.
            .key_context("Palette Line")
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &Confirm, _, cx| cx.emit(FieldEvent::Done)))
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.emit(FieldEvent::Cancel)))
            // Un clic dans le texte y pose le curseur.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let (Ok(at) | Err(at)) = layout.index_for_position(e.position);
                    this.line.place(at);
                    cx.notify();
                }),
            );
        line::keys(field, cx, |this| Some(&mut this.line), |_, _| ())
            .flex_1()
            .min_w_0()
            .cursor_text()
            .child(input)
            .child(shown)
    }
}

/// Largeur d'une colonne quand la fenêtre a la place ; elles se serrent jusqu'à `NARROW` pour
/// tenir côte à côte, puis le tableau défile.
pub const COLUMN: f32 = 250.;
pub const NARROW: f32 = 140.;

/// Le cadre d'une carte, sa case et son texte : les mêmes pour la carte posée, celle qu'on tient
/// et la place qu'elle prendrait.
pub fn card_box(t: Theme) -> Div {
    div().p_2().rounded(px(6.)).bg(t.bg).border_1().border_color(t.border).flex().items_start().gap_2()
}

pub fn check_box(done: bool, t: Theme) -> Div {
    div()
        .size(px(16.))
        .flex_none()
        .mt(px(1.))
        .rounded(px(4.))
        .border_1()
        .border_color(if done { t.accent } else { t.soft })
        .flex()
        .items_center()
        .justify_center()
        .when(done, |d| d.child(svg().path("check.svg").size(px(12.)).text_color(t.accent)))
}

pub fn card_text(text: &str, done: bool, t: Theme) -> Div {
    div().flex_1().min_w_0().when(done, |d| d.text_color(t.dim).line_through()).child(text.to_string())
}

/// Carte qu'on tient : la carte elle-même, soulevée, qui suit le pointeur par où on l'a prise.
#[derive(Clone)]
pub struct Dragged {
    /// Le pane du tableau d'où vient la carte : un autre tableau l'ignore.
    pub pane: usize,
    pub from: (usize, usize),
    pub text: String,
    pub done: bool,
    /// La largeur de la carte dans sa colonne.
    pub width: Pixels,
    pub theme: Theme,
}

impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        card_box(t)
            .w(self.width)
            .border_color(t.accent)
            .shadow_lg()
            .text_size(px(13.))
            .text_color(t.text)
            .child(check_box(self.done, t))
            .child(card_text(&self.text, self.done, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "---\nkanban: true\n---\n# Projet\n\n## À faire\n\n- [ ] Écrire\n    - [ ] un détail\n- [ ] Relire\n\n## En cours\n\n## Fait\n- [x] Planifier";

    #[test]
    fn reads_and_moves_cards() {
        assert!(is_board(BOARD) && is_board("---\nkanban-plugin: basic\n---\n") && !is_board("## À faire\n- [ ] x"));
        let all = columns(BOARD);
        let shape: Vec<(&str, Vec<(&str, bool)>)> = all.iter().map(|c| (c.title, c.cards.iter().map(|k| (k.text, k.done)).collect())).collect();
        assert_eq!(shape, [("À faire", vec![("Écrire", false), ("Relire", false)]), ("En cours", vec![]), ("Fait", vec![("Planifier", true)])]);
        let titles = |text: &str| columns(text).iter().map(|c| c.cards.iter().map(|k| k.text.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
        // Dans une colonne vide, puis à la fin de la dernière (le fichier n'a pas de fin de ligne).
        let moved = move_card(BOARD, (0, 0), (1, usize::MAX)).unwrap();
        assert_eq!(titles(&moved), [vec!["Relire"], vec!["Écrire"], vec!["Planifier"]]);
        assert!(moved.contains("## En cours\n- [ ] Écrire\n\n## Fait") && moved.contains("    - [ ] un détail\n- [ ] Relire"));
        let moved = move_card(&moved, (1, 0), (2, usize::MAX)).unwrap();
        assert!(moved.ends_with("- [x] Planifier\n- [ ] Écrire\n"));
        // Devant une autre carte, vers le haut comme vers le bas ; sur elle-même, rien.
        assert_eq!(titles(&move_card(BOARD, (2, 0), (0, 1)).unwrap()), [vec!["Écrire", "Planifier", "Relire"], vec![], vec![]]);
        assert_eq!(titles(&move_card(BOARD, (0, 0), (0, 2)).unwrap())[0], ["Relire", "Écrire"]);
        assert_eq!((move_card(BOARD, (0, 1), (0, 1)), move_card(BOARD, (0, 1), (0, 2)), move_card(BOARD, (5, 0), (0, 0))), (None, None, None));
        // Cocher, décocher, ajouter.
        assert!(tick(BOARD, (0, 1)).unwrap().contains("- [x] Relire") && tick(BOARD, (2, 0)).unwrap().ends_with("- [ ] Planifier"));
        assert!(add_card(BOARD, 1, " Coder ").unwrap().contains("## En cours\n- [ ] Coder\n\n## Fait"));
        assert!(add_card(BOARD, 2, "Fêter").unwrap().ends_with("- [x] Planifier\n- [ ] Fêter\n"));
        // Réécrire une carte (vidée : elle part), renommer une colonne, en ajouter une.
        assert!(retitle(BOARD, (0, 1), "Relire deux fois").unwrap().contains("- [ ] Relire deux fois\n\n## En cours"));
        assert!(retitle(BOARD, (2, 0), "Prévoir").unwrap().ends_with("- [x] Prévoir"));
        assert_eq!(titles(&retitle(BOARD, (0, 0), " ").unwrap())[0], ["Relire"]);
        assert!(rename_column(BOARD, 1, "En route").unwrap().contains("\n## En route\n\n## Fait") && rename_column(BOARD, 1, "").is_none());
        assert!(add_column(BOARD, "Plus tard").unwrap().ends_with("- [x] Planifier\n\n## Plus tard\n") && add_column(BOARD, " ").is_none());
        let fresh = new_board();
        assert!(is_board(&fresh) && columns(&fresh).iter().map(|c| c.title).collect::<Vec<_>>() == ["À faire", "En cours", "Fait"] && fresh.ends_with("## Fait\n"));
    }
}
