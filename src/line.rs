//! Ligne de saisie : le texte d'un champ d'une ligne (carte du kanban, cellule de la grille,
//! palette), son curseur et sa sélection. Les mêmes touches partout : flèches, mot à mot, début
//! et fin, sélection, effacement, presse-papiers. Le champ qui s'en sert ajoute « Line » à son
//! contexte de touches et branche les gestes par `keys`.

use std::ops::Range;

use gpui::{
    Action, App, Bounds, ClipboardItem, Context, HighlightStyle, InteractiveElement, StyledText, TextLayout, UTF16Selection,
    actions, canvas, div, fill, prelude::*, px, size,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::Theme;
use crate::keys::{Bind, bind};

actions!(
    line,
    [
        Left, Right, WordLeft, WordRight, Start, End, SelectLeft, SelectRight, SelectWordLeft, SelectWordRight, SelectStart,
        SelectEnd, SelectAll, Backspace, Delete, DeleteWordLeft, DeleteWordRight, Copy, Cut, Paste
    ]
);

#[derive(Clone, Copy)]
pub enum To {
    Left,
    Right,
    WordLeft,
    WordRight,
    Start,
    End,
}

#[derive(Default)]
pub struct Line {
    pub text: String,
    /// Le curseur, et l'autre bout de la sélection (le même octet s'il n'y en a pas).
    at: usize,
    anchor: usize,
}

impl Line {
    /// Le texte, curseur à la fin.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self { at: text.len(), anchor: text.len(), text }
    }

    #[cfg(test)]
    pub fn cursor(&self) -> usize {
        self.at
    }

    pub fn selection(&self) -> Range<usize> {
        self.at.min(self.anchor)..self.at.max(self.anchor)
    }

    fn target(&self, to: To) -> usize {
        let at = self.at;
        match to {
            To::Left => self.text[..at].grapheme_indices(true).next_back().map_or(0, |(i, _)| i),
            To::Right => self.text[at..].graphemes(true).next().map_or(at, |g| at + g.len()),
            To::WordLeft => self.text[..at].split_word_bound_indices().rev().find(|(_, w)| !w.trim().is_empty()).map_or(0, |(i, _)| i),
            To::WordRight => self.text[at..].split_word_bound_indices().find(|(_, w)| !w.trim().is_empty()).map_or(self.text.len(), |(i, w)| at + i + w.len()),
            To::Start => 0,
            To::End => self.text.len(),
        }
    }

    /// Déplace le curseur ; `select` étend la sélection. Sans lui, une flèche sur une sélection
    /// la referme de son côté.
    pub fn go(&mut self, to: To, select: bool) {
        let chosen = self.selection();
        self.at = match to {
            To::Left if !select && !chosen.is_empty() => chosen.start,
            To::Right if !select && !chosen.is_empty() => chosen.end,
            _ => self.target(to),
        };
        if !select {
            self.anchor = self.at;
        }
    }

    /// Pose le curseur à cet octet, ramené sur une limite de caractère.
    pub fn place(&mut self, at: usize) {
        self.at = (0..=at.min(self.text.len())).rev().find(|&i| self.text.is_char_boundary(i)).unwrap_or(0);
        self.anchor = self.at;
    }

    pub fn select_all(&mut self) {
        (self.anchor, self.at) = (0, self.text.len());
    }

    /// Écrit à la place de la sélection ; un champ d'une ligne n'a pas de retour à la ligne.
    pub fn insert(&mut self, text: &str) {
        let text = text.replace("\r\n", " ").replace(['\n', '\r'], " ");
        let chosen = self.selection();
        self.text.replace_range(chosen.clone(), &text);
        self.at = chosen.start + text.len();
        self.anchor = self.at;
    }

    /// Efface la sélection, sinon du curseur jusqu'à `to`.
    pub fn delete(&mut self, to: To) {
        if self.selection().is_empty() {
            self.anchor = self.target(to);
        }
        self.insert("");
    }

    pub fn selected(&self) -> &str {
        &self.text[self.selection()]
    }

    /// La sélection telle que la demande la saisie du système (unités UTF-16).
    pub fn utf16(&self) -> UTF16Selection {
        let chosen = self.selection();
        let units = |to: usize| self.text[..to].encode_utf16().count();
        UTF16Selection { range: units(chosen.start)..units(chosen.end), reversed: self.at < self.anchor }
    }

    /// Le texte, sa sélection et son curseur ; la mise en page sert à poser le curseur au clic.
    pub fn shown(&self, t: Theme) -> (gpui::Div, TextLayout) {
        // Une ligne vide garde sa hauteur, et de quoi placer le curseur.
        let text = if self.text.is_empty() { " ".to_string() } else { self.text.clone() };
        let chosen = Some(self.selection()).filter(|chosen| !chosen.is_empty());
        let styled = StyledText::new(text).with_highlights(chosen.map(|chosen| (chosen, HighlightStyle { background_color: Some(t.selection), ..Default::default() })));
        let (layout, at) = (styled.layout().clone(), self.at);
        let caret = canvas(|_, _, _| (), {
            let layout = layout.clone();
            move |_, _, window, _| {
                if let Some(origin) = layout.position_for_index(at) {
                    window.paint_quad(fill(Bounds::new(origin, size(px(1.5), layout.line_height())), t.accent));
                }
            }
        });
        (div().relative().child(styled).child(caret.absolute().size_0()), layout)
    }
}

/// Les touches d'une ligne de saisie, pour le contexte « Line ». À lier après celles des champs
/// qui s'en servent : à profondeur égale, la dernière liaison l'emporte.
pub fn bindings(word: &str) -> Vec<Bind> {
    let c = Some("Line");
    vec![
        bind("left", &Left, c),
        bind("right", &Right, c),
        bind(&format!("{word}-left"), &WordLeft, c),
        bind(&format!("{word}-right"), &WordRight, c),
        bind("home", &Start, c),
        bind("end", &End, c),
        bind("shift-left", &SelectLeft, c),
        bind("shift-right", &SelectRight, c),
        bind(&format!("{word}-shift-left"), &SelectWordLeft, c),
        bind(&format!("{word}-shift-right"), &SelectWordRight, c),
        bind("shift-home", &SelectStart, c),
        bind("shift-end", &SelectEnd, c),
        bind("secondary-a", &SelectAll, c),
        bind("backspace", &Backspace, c),
        bind("delete", &Delete, c),
        bind(&format!("{word}-backspace"), &DeleteWordLeft, c),
        bind(&format!("{word}-delete"), &DeleteWordRight, c),
        bind("secondary-c", &Copy, c),
        bind("secondary-x", &Cut, c),
        bind("secondary-v", &Paste, c),
    ]
}

/// Branche les gestes d'une ligne de saisie sur l'élément `el` d'une vue : `line` donne la ligne
/// en cours (aucune : le geste ne fait rien), `changed` est appelé quand son texte a changé.
pub fn keys<E: InteractiveElement, V: 'static>(el: E, cx: &Context<V>, line: fn(&mut V) -> Option<&mut Line>, changed: fn(&mut V, &mut Context<V>)) -> E {
    /// Un geste : ce qu'il fait à la ligne ; vrai s'il en change le texte.
    type Act = fn(&mut Line, &mut App) -> bool;
    fn on<A: Action, E: InteractiveElement, V: 'static>(el: E, cx: &Context<V>, line: fn(&mut V) -> Option<&mut Line>, changed: fn(&mut V, &mut Context<V>), act: Act) -> E {
        el.on_action(cx.listener(move |this, _: &A, _, cx| {
            let Some(line) = line(this) else {
                return;
            };
            if act(line, cx) {
                changed(this, cx);
            }
            cx.notify();
        }))
    }
    let moves: [(fn(E, &Context<V>, fn(&mut V) -> Option<&mut Line>, fn(&mut V, &mut Context<V>), Act) -> E, Act); 20] = [
        (on::<Left, E, V>, |l, _| (l.go(To::Left, false), false).1),
        (on::<Right, E, V>, |l, _| (l.go(To::Right, false), false).1),
        (on::<WordLeft, E, V>, |l, _| (l.go(To::WordLeft, false), false).1),
        (on::<WordRight, E, V>, |l, _| (l.go(To::WordRight, false), false).1),
        (on::<Start, E, V>, |l, _| (l.go(To::Start, false), false).1),
        (on::<End, E, V>, |l, _| (l.go(To::End, false), false).1),
        (on::<SelectLeft, E, V>, |l, _| (l.go(To::Left, true), false).1),
        (on::<SelectRight, E, V>, |l, _| (l.go(To::Right, true), false).1),
        (on::<SelectWordLeft, E, V>, |l, _| (l.go(To::WordLeft, true), false).1),
        (on::<SelectWordRight, E, V>, |l, _| (l.go(To::WordRight, true), false).1),
        (on::<SelectStart, E, V>, |l, _| (l.go(To::Start, true), false).1),
        (on::<SelectEnd, E, V>, |l, _| (l.go(To::End, true), false).1),
        (on::<SelectAll, E, V>, |l, _| (l.select_all(), false).1),
        (on::<Backspace, E, V>, |l, _| (l.delete(To::Left), true).1),
        (on::<Delete, E, V>, |l, _| (l.delete(To::Right), true).1),
        (on::<DeleteWordLeft, E, V>, |l, _| (l.delete(To::WordLeft), true).1),
        (on::<DeleteWordRight, E, V>, |l, _| (l.delete(To::WordRight), true).1),
        (on::<Copy, E, V>, |l, cx| (cx.write_to_clipboard(ClipboardItem::new_string(l.selected().to_string())), false).1),
        (on::<Cut, E, V>, |l, cx| (cx.write_to_clipboard(ClipboardItem::new_string(l.selected().to_string())), l.insert(""), true).2),
        (on::<Paste, E, V>, |l, cx| cx.read_from_clipboard().and_then(|item| item.text()).map(|text| l.insert(&text)).is_some()),
    ];
    moves.into_iter().fold(el, |el, (on, act)| on(el, cx, line, changed, act))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_a_line() {
        let mut line = Line::new("Écrire le brouillon");
        // Ctrl+Retour arrière : le mot d'avant, puis l'espace et le mot suivant.
        line.delete(To::WordLeft);
        assert_eq!((line.text.as_str(), line.cursor()), ("Écrire le ", 11));
        line.delete(To::WordLeft);
        assert_eq!(line.text, "Écrire ");
        // Les flèches passent les caractères entiers, accents compris ; Maj sélectionne.
        line.go(To::Start, false);
        line.go(To::Right, true);
        assert_eq!((line.selected(), line.utf16().range), ("É", 0..1));
        line.insert("E");
        assert_eq!((line.text.as_str(), line.cursor()), ("Ecrire ", 1));
        line.go(To::WordRight, true);
        assert_eq!(line.selected(), "crire");
        // Une flèche sans Maj referme la sélection de son côté.
        line.go(To::Left, false);
        assert_eq!((line.cursor(), line.selection()), (1, 1..1));
        line.delete(To::Left);
        line.delete(To::Left);
        assert_eq!(line.text, "crire ");
        line.select_all();
        line.insert("a\nb");
        assert_eq!(line.text, "a b");
        line.place(99);
        line.delete(To::Right);
        assert_eq!((line.text.as_str(), line.cursor()), ("a b", 3));
        let mut accent = Line::new("é");
        accent.place(1);
        assert_eq!(accent.cursor(), 0);
    }
}
