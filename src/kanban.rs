//! Tableau kanban : une note montrée en colonnes. Chaque titre `##` est une colonne, chaque
//! tâche `- [ ]` sous lui une carte ; le fichier reste du Markdown (la disposition du greffon
//! Kanban d'Obsidian). Les fonctions pures d'abord, puis le tableau lui-même (`impl Shell`).

use std::ops::Range;

use gpui::{Context, Focusable, Window, div, prelude::*, px, svg};

use crate::{
    Shell, Theme,
    markdown::{self, Kind},
    nav, tr,
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
    /// Où poser une carte de plus : après la dernière, ou après le titre.
    pub end: usize,
    pub cards: Vec<Card<'a>>,
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
            ((Kind::Heading(_), marker), _) if bare.starts_with("## ") => found.push(Column { title: bare[marker..].trim(), end, cards: Vec::new() }),
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

/// La note avec une carte vide de plus au bas de la colonne, et l'octet où l'écrire.
pub fn add_card(text: &str, column: usize) -> Option<(String, usize)> {
    let end = columns(text).get(column)?.end;
    let lead = if text[..end].ends_with('\n') { "" } else { "\n" };
    let cursor = end + lead.len() + "- [ ] ".len();
    Some((format!("{}{lead}- [ ] \n{}", &text[..end], &text[end..]), cursor))
}

/// Carte en cours de glisser-déposer ; dessinée sous le pointeur.
#[derive(Clone)]
struct Dragged {
    from: (usize, usize),
    text: String,
    theme: Theme,
}

impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div().px_2().py_1().rounded(px(6.)).bg(t.panel).border_1().border_color(t.border).text_size(px(13.)).text_color(t.text).child(self.text.clone())
    }
}

impl Shell {
    /// La note ouverte se montre en tableau : elle en est un, et on ne lui a pas demandé son texte.
    pub fn board_shown(&self, cx: &gpui::App) -> bool {
        let as_text = self.path.as_ref().is_some_and(|path| self.board_off.contains(path));
        self.picture.is_none() && self.listing.is_none() && !as_text && is_board(self.editor.read(cx).text())
    }

    /// Passe du tableau au texte de la note, et retour ; le choix tient tant que l'app tourne.
    pub fn toggle_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.path.clone()
            && !self.board_off.remove(&path)
        {
            self.board_off.insert(path);
            window.focus(&self.editor.focus_handle(cx));
        }
        cx.notify();
    }

    /// Applique au texte de la note un geste fait sur le tableau : il s'annule comme une frappe.
    pub fn board_edit(&mut self, change: impl Fn(&str) -> Option<String>, cx: &mut Context<Self>) {
        if let Some(text) = change(self.editor.read(cx).text()) {
            self.editor.update(cx, |e, cx| e.rewrite(&text, cx));
        }
    }

    /// Une carte de plus dans la colonne : elle s'écrit dans le texte, où le curseur l'attend.
    fn board_add(&mut self, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((text, cursor)) = add_card(self.editor.read(cx).text(), column) else {
            return;
        };
        self.editor.update(cx, |e, cx| {
            e.rewrite(&text, cx);
            e.jump(cursor, false, cx)
        });
        self.toggle_board(window, cx);
    }

    /// Le tableau, à la place de la note : une carte se glisse d'une colonne à l'autre ou sur une
    /// autre carte pour prendre sa place, un clic sur sa case la coche.
    // ponytail: à la souris seulement, et une carte se rédige dans le texte (« + » y mène) ;
    // une sélection au clavier et une saisie dans la carte si le tableau sert au quotidien.
    pub fn render_board(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let editor = self.editor.read(cx);
        let columns = columns(editor.text()).into_iter().enumerate().map(|(c, column)| {
            let cards = column.cards.iter().enumerate().map(|(i, card)| {
                let from = (c, i);
                let check = div()
                    .id(("tick", c * 10_000 + i))
                    .size(px(16.))
                    .flex_none()
                    .mt(px(1.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if card.done { t.accent } else { t.dim })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(card.done, |d| d.child(svg().path("check.svg").size(px(12.)).text_color(t.accent)))
                    .on_click(cx.listener(move |this, _, _, cx| this.board_edit(|text| tick(text, from), cx)));
                div()
                    .id(("card", c * 10_000 + i))
                    .p_2()
                    .rounded(px(6.))
                    .bg(t.bg)
                    .border_1()
                    .border_color(t.border)
                    .flex()
                    .items_start()
                    .gap_2()
                    .cursor_grab()
                    .child(check)
                    .child(div().flex_1().min_w_0().when(card.done, |d| d.text_color(t.dim).line_through()).child(card.text.to_string()))
                    .on_drag(Dragged { from, text: card.text.to_string(), theme: t }, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                    .drag_over::<Dragged>(move |style, _, _, _| style.border_color(t.accent))
                    .on_drop(cx.listener(move |this, dragged: &Dragged, _, cx| {
                        cx.stop_propagation();
                        let moved = dragged.from;
                        this.board_edit(|text| move_card(text, moved, from), cx)
                    }))
            });
            let count = column.cards.len();
            let add = nav::button(("card-add", c), "plus.svg", false, t).on_click(cx.listener(move |this, _, window, cx| this.board_add(c, window, cx)));
            div()
                .id(("column", c))
                .w(px(250.))
                .flex_none()
                .p_2()
                .rounded(px(8.))
                .bg(t.panel)
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().truncate().font_weight(gpui::FontWeight::BOLD).child(column.title.to_string()))
                        .child(div().text_color(t.dim).child(count.to_string()))
                        .child(add),
                )
                .children(cards)
                .drag_over::<Dragged>(move |style, _, _, _| style.bg(t.selection))
                .on_drop(cx.listener(move |this, dragged: &Dragged, _, cx| {
                    let moved = dragged.from;
                    this.board_edit(|text| move_card(text, moved, (c, usize::MAX)), cx)
                }))
        });
        let columns: Vec<_> = columns.collect();
        let empty = columns.is_empty().then(|| div().text_color(t.dim).child(tr("A board needs columns: one `## heading` each, tasks under it.", "Un tableau a besoin de colonnes : un titre `##` chacune, des tâches dessous.")));
        div()
            .id("board")
            .track_focus(&self.board_focus)
            .size_full()
            .pt(px(52.))
            .px_6()
            .pb_4()
            .text_size(px(13.))
            .flex()
            .items_start()
            .gap_3()
            .overflow_scroll()
            .children(columns)
            .children(empty)
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
        let (added, cursor) = add_card(BOARD, 1).unwrap();
        assert!(added.contains("## En cours\n- [ ] \n\n## Fait") && added[..cursor].ends_with("## En cours\n- [ ] "));
        let (added, cursor) = add_card(BOARD, 2).unwrap();
        assert!(added.ends_with("- [x] Planifier\n- [ ] \n") && cursor == added.len() - 1);
    }
}
