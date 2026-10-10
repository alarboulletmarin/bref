//! Tableau kanban : une note montrée en colonnes. Chaque titre `##` est une colonne, chaque
//! tâche `- [ ]` sous lui une carte ; le fichier reste du Markdown (la disposition du greffon
//! Kanban d'Obsidian). Les fonctions pures d'abord, puis le tableau lui-même (`impl Shell`).

use std::ops::Range;

use gpui::{
    App, Bounds, ClickEvent, Context, CursorStyle, Div, DragMoveEvent, ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle, Focusable, Pixels,
    Point, UTF16Selection, Window, canvas, div, prelude::*, px, svg,
};

use crate::{
    Shell, Theme,
    editor::{Redo, Undo},
    markdown::{self, Kind},
    nav,
    line::{self, Line},
    palette::{Confirm, Dismiss},
    tr,
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
const COLUMN: f32 = 250.;
const NARROW: f32 = 140.;

/// Le cadre d'une carte, sa case et son texte : les mêmes pour la carte posée, celle qu'on tient
/// et la place qu'elle prendrait.
fn card_box(t: Theme) -> Div {
    div().p_2().rounded(px(6.)).bg(t.bg).border_1().border_color(t.border).flex().items_start().gap_2()
}

fn check_box(done: bool, t: Theme) -> Div {
    div()
        .size(px(16.))
        .flex_none()
        .mt(px(1.))
        .rounded(px(4.))
        .border_1()
        .border_color(if done { t.accent } else { t.dim })
        .flex()
        .items_center()
        .justify_center()
        .when(done, |d| d.child(svg().path("check.svg").size(px(12.)).text_color(t.accent)))
}

fn card_text(text: &str, done: bool, t: Theme) -> Div {
    div().flex_1().min_w_0().when(done, |d| d.text_color(t.dim).line_through()).child(text.to_string())
}

/// Carte qu'on tient : la carte elle-même, soulevée, qui suit le pointeur par où on l'a prise.
#[derive(Clone)]
struct Dragged {
    from: (usize, usize),
    text: String,
    done: bool,
    /// La largeur de la carte dans sa colonne.
    width: Pixels,
    theme: Theme,
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

/// Pendant qu'on tient une carte, le pointeur sur cette zone dit où elle se poserait : avant la
/// carte de rang `i` de la colonne `c`, ou après elle s'il est dans sa moitié basse (`halves`).
fn aim(c: usize, i: usize, halves: bool, cx: &mut Context<Shell>) -> impl Fn(&DragMoveEvent<Dragged>, &mut Window, &mut App) + 'static {
    cx.listener(move |this, e: &DragMoveEvent<Dragged>, window, cx| {
        let at = e.event.position;
        if !e.bounds.contains(&at) {
            return;
        }
        let below = halves && at.y > e.bounds.origin.y + e.bounds.size.height / 2.;
        let drag = Some((e.drag(cx).from, (c, i + below as usize)));
        if this.board_drag != drag {
            this.board_drag = drag;
            cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
            cx.notify();
        }
    })
}

impl Shell {
    /// La note ouverte se montre en tableau : elle en est un, et on ne lui a pas demandé son texte.
    pub fn board_shown(&self, cx: &gpui::App) -> bool {
        self.picture.is_none() && self.listing.is_none() && !self.board_text && is_board(self.editor.read(cx).text())
    }

    /// Passe du tableau au texte de la note, et retour ; le choix tient tant qu'elle reste ouverte.
    pub fn toggle_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.board_field = None;
        self.board_text = !self.board_text;
        if self.board_text {
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

    /// Ouvre un champ de saisie dans le tableau : une carte ou un titre à réécrire, une carte ou
    /// une colonne de plus. Celui qui était ouvert est d'abord validé.
    pub fn board_write(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        self.board_commit(false, window, cx);
        let text = {
            let all = columns(self.editor.read(cx).text());
            match slot {
                Slot::Card(c, i) => all.get(c).and_then(|column| column.cards.get(i)).map(|card| card.text.to_string()),
                Slot::Column(c) => all.get(c).map(|column| column.title.to_string()),
                Slot::New(_) | Slot::NewColumn => Some(String::new()),
            }
        };
        let Some(text) = text else {
            return;
        };
        let theme = self.theme;
        let field = cx.new(|cx| Field::new(&text, theme, cx));
        cx.subscribe_in(&field, window, |this, _, event: &FieldEvent, window, cx| match event {
            FieldEvent::Done => this.board_commit(true, window, cx),
            FieldEvent::Cancel => {
                this.board_field = None;
                window.focus(&this.board_focus);
                cx.notify();
            }
        })
        .detach();
        window.focus(&field.focus_handle(cx));
        // La colonne qu'on ajoute est au bout du tableau : il défile jusqu'à elle.
        if slot == Slot::NewColumn {
            self.board_scroll.scroll_to_item(columns(self.editor.read(cx).text()).len());
        }
        self.board_field = Some((slot, field));
        cx.notify();
    }

    /// Valide ce qui est écrit dans le champ ouvert. Après une carte ajoutée par Entrée
    /// (`again`), le champ se rouvre pour la suivante : on enchaîne sans reprendre la souris.
    pub fn board_commit(&mut self, again: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((slot, field)) = self.board_field.take() else {
            return;
        };
        let typed = field.read(cx).line.text.clone();
        let written = !typed.trim().is_empty();
        self.board_edit(
            |text| match slot {
                Slot::New(c) if written => add_card(text, c, &typed),
                Slot::Card(c, i) => retitle(text, (c, i), &typed),
                Slot::Column(c) => rename_column(text, c, &typed),
                Slot::NewColumn => add_column(text, &typed),
                Slot::New(_) => None,
            },
            cx,
        );
        match slot {
            Slot::New(c) if again && written => self.board_write(Slot::New(c), window, cx),
            _ => window.focus(&self.board_focus),
        }
        cx.notify();
    }

    /// Le tableau, à la place de la note. Une carte qu'on prend quitte sa place et suit le
    /// pointeur ; une place en pointillés s'ouvre là où elle se poserait, entre deux cartes ou
    /// au bas d'une colonne. Un clic sur sa case la coche, un double-clic la réécrit sur place
    /// (vidée, elle est retirée). « + » ajoute une carte, ou une colonne.
    // ponytail: à la souris ; une sélection de carte au clavier (flèches, Ctrl+flèches pour
    // la déplacer) si le tableau sert au quotidien. Le tableau ne défile pas tout seul quand on
    // tient une carte près d'un bord : à ajouter si les colonnes dépassent souvent la fenêtre.
    pub fn render_board(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.theme;
        let text = self.editor.read(cx).text().to_string();
        let writing = self.board_field.as_ref().map(|(slot, field)| (*slot, field.clone()));
        let field_at = |slot: Slot| writing.as_ref().filter(|(open, _)| *open == slot).map(|(_, field)| field.clone());
        let all = columns(&text);
        let drag = self.board_drag.filter(|_| cx.has_active_drag());
        // La place que prendrait la carte tenue : sa taille exacte, vide.
        let held = drag.and_then(|(from, _)| all.get(from.0)?.cards.get(from.1)).map(|card| (card.text.to_string(), card.done));
        let place = || {
            let (text, done) = held.as_ref()?;
            let empty = card_box(t).bg(t.accent.opacity(0.08)).border_dashed().border_color(t.accent.opacity(0.6));
            Some(empty.child(check_box(*done, t).invisible()).child(card_text(text, *done, t).invisible()).into_any_element())
        };
        /// Un double-clic ouvre le champ de saisie à cet endroit.
        fn twice(slot: Slot, cx: &mut Context<Shell>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
            cx.listener(move |this, e: &ClickEvent, window, cx| {
                if e.click_count() >= 2 {
                    this.board_write(slot, window, cx)
                }
            })
        }
        let mut shown = Vec::new();
        for (c, column) in all.iter().enumerate() {
            let count = column.cards.len();
            let mut cards = Vec::new();
            for (i, card) in column.cards.iter().enumerate() {
                let from = (c, i);
                if drag.is_some_and(|(_, to)| to == from) {
                    cards.extend(place());
                }
                // La carte tenue a quitté sa colonne : elle est sous le pointeur.
                if drag.is_some_and(|(held, _)| held == from) {
                    continue;
                }
                if let Some(field) = field_at(Slot::Card(c, i)) {
                    cards.push(card_box(t).border_color(t.accent).child(field).into_any_element());
                    continue;
                }
                let check = check_box(card.done, t).id(("tick", c * 10_000 + i)).cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.board_edit(|text| tick(text, from), cx)
                }));
                let card = card_box(t)
                    .id(("card", c * 10_000 + i))
                    .debug_selector(|| format!("card-{c}-{i}"))
                    .cursor_grab()
                    .hover(|s| s.border_color(t.dim))
                    .child(check)
                    .child(card_text(card.text, card.done, t))
                    .on_click(twice(Slot::Card(c, i), cx))
                    .on_drag(Dragged { from, text: card.text.to_string(), done: card.done, width: self.board_card.get(), theme: t }, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                    .on_drag_move(aim(c, i, true, cx))
                    .into_any_element();
                cards.push(card);
            }
            if drag.is_some_and(|(_, to)| to.0 == c && to.1 >= count) {
                cards.extend(place());
            }
            let title = match field_at(Slot::Column(c)) {
                Some(field) => div().flex_1().min_w_0().flex().child(field).into_any_element(),
                None => div()
                    .id(("column-title", c))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(column.title.to_string())
                    .on_click(twice(Slot::Column(c), cx))
                    .into_any_element(),
            };
            // Au bas de la colonne : la carte qu'on écrit, sinon de quoi en ajouter une.
            let add = match field_at(Slot::New(c)) {
                Some(field) => card_box(t).border_color(t.accent).child(field).into_any_element(),
                None => div()
                    .id(("card-add", c))
                    .px_2()
                    .py_1()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(t.dim)
                    .hover(|s| s.bg(t.bg).text_color(t.text))
                    .child(format!("+ {}", tr("Add a card", "Ajouter une carte")))
                    .on_click(cx.listener(move |this, _, window, cx| this.board_write(Slot::New(c), window, cx)))
                    .into_any_element(),
            };
            let head = div().flex().items_center().gap_2().child(title).child(div().text_color(t.dim).child(count.to_string()));
            // La largeur d'une carte, relevée au dessin : celle qu'on tient garde la sienne.
            let width = self.board_card.clone();
            let measure = canvas(move |bounds, _, _| width.set(bounds.size.width), |_, _, _, _| ()).absolute().top_0().left_2().right_2().h_0();
            let column = div()
                .relative()
                .when(c == 0, |d| d.child(measure))
                .p_2()
                .rounded(px(8.))
                .bg(t.panel)
                .flex()
                .flex_col()
                .gap_2()
                // Sur le titre, la carte tenue vise le haut de la colonne ; sous les cartes, le bas.
                .child(head.on_drag_move(aim(c, 0, false, cx)))
                .children(cards)
                .child(div().child(add).on_drag_move(aim(c, count, false, cx)));
            // Le couloir de la colonne descend jusqu'en bas : une carte lâchée sous une colonne
            // courte s'y pose quand même.
            let rest = div().debug_selector(|| format!("column-rest-{c}")).flex_1().min_h(px(48.)).on_drag_move(aim(c, count, false, cx));
            shown.push(div().flex_1().min_w(px(NARROW)).max_w(px(COLUMN)).flex().flex_col().child(column).child(rest));
        }
        let more = match field_at(Slot::NewColumn) {
            Some(field) => div().flex_1().min_w(px(NARROW)).max_w(px(COLUMN)).h(px(36.)).p_2().rounded(px(8.)).bg(t.panel).border_1().border_color(t.accent).flex().child(field).into_any_element(),
            None => nav::button("column-add", "plus.svg", false, t)
                .flex_none()
                .on_click(cx.listener(|this, _, window, cx| this.board_write(Slot::NewColumn, window, cx)))
                .into_any_element(),
        };
        div()
            .id("board")
            .key_context("Board")
            .track_focus(&self.board_focus)
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.editor.update(cx, |e, cx| e.restore(false, cx))))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.editor.update(cx, |e, cx| e.restore(true, cx))))
            // La carte tenue sortie du tableau : sa place revient d'où elle vient, et la lâcher
            // là ne déplace rien.
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<Dragged>, _, cx| {
                let from = e.drag(cx).from;
                if !e.bounds.contains(&e.event.position) && this.board_drag != Some((from, from)) {
                    this.board_drag = Some((from, from));
                    cx.notify();
                }
            }))
            .size_full()
            .pt(px(52.))
            .px_6()
            .pb_4()
            .text_size(px(13.))
            .flex()
            .gap_3()
            .overflow_scroll()
            .track_scroll(&self.board_scroll)
            // Un clic à côté valide ce qu'on écrivait.
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, window, cx| this.board_commit(false, window, cx)))
            // La carte lâchée, où que ce soit sur le tableau, va à la place ouverte pour elle.
            .on_drop(cx.listener(|this, _: &Dragged, _, cx| {
                if let Some((from, to)) = this.board_drag.take() {
                    this.board_edit(|text| move_card(text, from, to), cx);
                }
                cx.notify();
            }))
            .children(shown)
            .child(more)
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
