//! Find and replace in the note (Ctrl+F, Ctrl+R), and the vault-wide replace it can start.

use super::*;
use crate::views::field::{Field, FieldEvent};

/// What the matches were found for: the text's version, the search, case, whole words, regular expression, and
/// the lines it keeps to.
type Asked = (u64, String, bool, bool, bool, Option<Range<usize>>);

/// Find in the note: its two fields, its settings and the matches found. Kept when the bar closes:
/// it opens again on the same search.
pub(super) struct Finder {
    pub(super) query: Entity<Field>,
    pub(super) with: Entity<Field>,
    /// The "Replace" line shows.
    pub(super) replace: bool,
    pub(super) case: bool,
    pub(super) word: bool,
    /// The search is a regular expression.
    pub(super) regex: bool,
    /// The lines selected when the bar opened, when there were several; `inside`: the search keeps to them.
    pub(super) span: Option<Range<usize>>,
    pub(super) inside: bool,
    pub(super) hits: Vec<Range<usize>>,
    /// Current match in `hits`.
    pub(super) at: usize,
    /// Text and settings for which `hits` was computed.
    key: Option<Asked>,
    _fields: [gpui::Subscription; 2],
}

impl Finder {
    pub(super) fn query(&self, cx: &App) -> String {
        self.query.read(cx).line.text.clone()
    }

    /// The field that takes the keys: the replacement when it shows and the search is settled (`ready`: the bar
    /// was open, or the selection made it), otherwise the search.
    fn aim(&self, ready: bool, cx: &App) -> FocusHandle {
        let to = if self.replace && ready && !self.query.read(cx).line.text.is_empty() { &self.with } else { &self.query };
        to.focus_handle(cx)
    }

    /// Where the search keeps to: the selected lines, when the toggle says so.
    fn scope(&self, len: usize) -> Option<Range<usize>> {
        self.span.clone().filter(|_| self.inside).map(|span| span.start.min(len)..span.end.min(len))
    }
}

impl Editor {
    /// Search, number of the current match (from 1) and number of matches.
    #[cfg(test)]
    pub fn finding(&self, cx: &App) -> Option<(String, usize, usize)> {
        let find = self.find.as_ref()?;
        Some((find.query(cx), if find.hits.is_empty() { 0 } else { find.at + 1 }, find.hits.len()))
    }

    /// The bar's state for a test: the replace line shows, the replace field has the keys.
    #[cfg(test)]
    pub fn find_mode(&self, window: &Window, cx: &App) -> Option<(bool, bool)> {
        let find = self.find.as_ref()?;
        Some((find.replace, find.with.focus_handle(cx).is_focused(window)))
    }

    /// The bar's two fields for a test: the search and the replacement.
    #[cfg(test)]
    pub fn find_fields(&self) -> Option<(Entity<Field>, Entity<Field>)> {
        self.find.as_ref().map(|find| (find.query.clone(), find.with.clone()))
    }

    /// The bar's search, open or kept: what the vault search starts from.
    pub fn search(&self, cx: &App) -> Option<md::Swap> {
        let find = self.find.as_ref().or(self.found.as_ref())?;
        let with = find.with.read(cx).line.text.clone();
        Some(md::Swap { query: find.query(cx), with, case: find.case, word: find.word, regex: find.regex })
    }

    /// A result of the vault search: the bar opens on its search, the current match the one at byte `at`, and
    /// the note has the keys.
    pub fn show_match(&mut self, swap: &md::Swap, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut find = match self.find.take().or_else(|| self.found.take()) {
            Some(find) => find,
            None => self.new_finder(window, cx),
        };
        find.query.update(cx, |field, cx| field.set(&swap.query, cx));
        find.with.update(cx, |field, cx| field.set(&swap.with, cx));
        (find.case, find.word, find.regex, find.span, find.inside, find.key) = (swap.case, swap.word, swap.regex, None, false, None);
        self.find = Some(find);
        self.sel = at.min(self.content.len())..at.min(self.content.len());
        self.find_changed(cx);
        window.focus(&self.focus);
    }

    fn new_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Finder {
        let theme = self.theme;
        let (query, with) = (cx.new(|cx| Field::new("", theme, cx).scrolling()), cx.new(|cx| Field::new("", theme, cx).scrolling()));
        // Enter in the search: next match; in the replacement: this match replaced. Escape closes.
        let on = |replacing: bool| {
            move |this: &mut Self, _: &Entity<Field>, event: &FieldEvent, window: &mut Window, cx: &mut Context<Self>| match event {
                FieldEvent::Changed if replacing => cx.notify(),
                FieldEvent::Changed => this.find_changed(cx),
                FieldEvent::Done if replacing => this.replace_one(cx),
                FieldEvent::Done => this.find_step(true, window, cx),
                FieldEvent::Cancel => this.close_find(window, cx),
            }
        };
        let fields = [cx.subscribe_in(&query, window, on(false)), cx.subscribe_in(&with, window, on(true))];
        Finder { query, with, replace: false, case: false, word: false, regex: false, span: None, inside: false, hits: Vec::new(), at: 0, key: None, _fields: fields }
    }

    /// Ctrl+F opens the bar on the search, Ctrl+R with the replace line and the keys in it; pressed with the bar
    /// open, each switches to its mode, the search, its settings and the current match unchanged. A selection on
    /// one line becomes the search; over several, the search keeps to it.
    pub(super) fn open_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let picked = (!self.sel.is_empty()).then(|| (self.sel.clone(), self.content[self.sel.clone()].to_string()));
        let (was_open, picked_any) = (self.find.is_some(), picked.is_some());
        let mut find = match self.find.take().or_else(|| self.found.take()) {
            Some(find) => find,
            None => self.new_finder(window, cx),
        };
        match picked {
            Some((_, text)) if !text.contains('\n') => find.query.update(cx, |field, cx| field.set(&text, cx)),
            Some((range, _)) => (find.span, find.inside) = (Some(range), true),
            // Opened again on the same search: all selected, typing replaces it.
            None if !was_open => find.query.update(cx, |field, cx| field.set(&field.line.text.clone(), cx)),
            None => {}
        }
        if !was_open {
            find.with.update(cx, |field, cx| field.set(&field.line.text.clone(), cx));
        }
        find.replace = replace;
        window.focus(&find.aim(was_open || picked_any, cx));
        self.find = Some(find);
        self.find_changed(cx);
    }

    /// Escape: the bar closes, keeping its search for next time; the note has the keys, on the current match.
    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.found = self.find.take();
        window.focus(&self.focus);
        cx.notify();
    }

    /// The chevron: shows or hides the replace line, the keys going to the field that shows.
    pub(super) fn flip_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.find {
            find.replace = !find.replace;
            window.focus(&find.aim(true, cx));
        }
        cx.notify();
    }

    /// Tab in the bar: from the search to the replacement and back.
    pub(super) fn find_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = self.find.as_ref().filter(|find| find.replace) {
            let to = if find.with.focus_handle(cx).is_focused(window) { &find.query } else { &find.with };
            window.focus(&to.focus_handle(cx));
        }
        cx.notify();
    }

    /// Rebuilds the list of matches if the text, the search or its settings changed (and
    /// says so); the current match is then the first one from the cursor.
    pub(super) fn refresh_find(&mut self, cx: &App) -> bool {
        let (version, from, len) = (self.version, self.sel.start, self.content.len());
        let Some(find) = &mut self.find else {
            return false;
        };
        let (query, scope) = (find.query(cx), find.scope(len));
        let key = (version, query.clone(), find.case, find.word, find.regex, scope.clone());
        if find.key.as_ref() == Some(&key) {
            return false;
        }
        let inside = |hit: &Range<usize>| scope.as_ref().is_none_or(|scope| scope.start <= hit.start && hit.end <= scope.end);
        find.hits = md::replacements(&self.content, &query, "", find.case, find.word, find.regex).into_iter().map(|(hit, _)| hit).filter(inside).collect();
        find.at = find.hits.iter().position(|hit| hit.start >= from).unwrap_or(0);
        find.key = Some(key);
        true
    }

    /// Selects the current match and scrolls it on screen.
    pub(super) fn show_hit(&mut self, cx: &mut Context<Self>) {
        if let Some(hit) = self.find.as_ref().and_then(|find| find.hits.get(find.at).cloned()) {
            self.sel = hit;
            self.reversed = false;
            self.more.clear();
            self.goal_x = None;
            self.grid = None;
            self.reveal = true;
        }
        cx.notify();
    }

    /// Next or previous match, wrapping around from the other end once at the edge.
    pub(super) fn find_step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        // F3 without a bar: it opens, as with Ctrl+F.
        if self.find.is_none() {
            return self.open_find(false, window, cx);
        }
        self.refresh_find(cx);
        if let Some(find) = &mut self.find
            && let n @ 1.. = find.hits.len()
        {
            find.at = if forward { (find.at + 1) % n } else { (find.at + n - 1) % n };
        }
        self.show_hit(cx);
    }

    /// The matches found and what replaces each; `None` without a "Replace" line.
    pub(super) fn replacements(&self, cx: &App) -> Option<Vec<(Range<usize>, String)>> {
        let find = self.find.as_ref().filter(|find| find.replace)?;
        let with = find.with.read(cx).line.text.clone();
        let scope = find.scope(self.content.len());
        let inside = |(hit, _): &(Range<usize>, String)| scope.as_ref().is_none_or(|scope| scope.start <= hit.start && hit.end <= scope.end);
        Some(md::replacements(&self.content, &find.query(cx), &with, find.case, find.word, find.regex).into_iter().filter(inside).collect())
    }

    /// The selected lines the search keeps to grow or shrink with what replaced a match in them.
    fn shift_span(&mut self, hit: &Range<usize>, put: usize) {
        if let Some(span) = self.find.as_mut().and_then(|find| find.span.as_mut()).filter(|span| span.start <= hit.start && hit.end <= span.end) {
            span.end = span.end + put - hit.len();
        }
    }

    /// Enter in the replacement, or its button: the current match replaced, then the next one shown
    /// (from the first one after the last).
    pub(super) fn replace_one(&mut self, cx: &mut Context<Self>) {
        self.refresh_find(cx);
        let hit = self.find.as_ref().and_then(|find| find.hits.get(find.at).cloned());
        let Some((hit, with)) = hit.and_then(|hit| self.replacements(cx)?.into_iter().find(|(range, _)| *range == hit)) else {
            return cx.notify();
        };
        self.edit(hit.clone(), &with, cx);
        self.realign(cx);
        self.shift_span(&hit, with.len());
        self.refresh_find(cx);
        self.show_hit(cx);
    }

    /// Asks the window to replace in the whole vault: the search, the replacement and
    /// the bar's settings.
    pub(super) fn replace_in_vault(&mut self, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_ref().filter(|find| find.replace) else { return };
        let (query, with) = (find.query(cx), find.with.read(cx).line.text.clone());
        if !query.is_empty() {
            cx.emit(EditorEvent::Swap(md::Swap { query, with, case: find.case, word: find.word, regex: find.regex }));
        }
    }

    /// Replaces all matches at once (in the selected lines, when the search keeps to them): a single undo step.
    pub(super) fn replace_all(&mut self, cx: &mut Context<Self>) {
        self.refresh_find(cx);
        let Some(hits) = self.replacements(cx) else {
            return;
        };
        let Some(first) = hits.first().map(|(hit, _)| hit.start) else {
            return;
        };
        let mut text = String::with_capacity(self.content.len());
        let mut done = 0;
        for (hit, with) in &hits {
            text.push_str(&self.content[done..hit.start]);
            text.push_str(with);
            done = hit.end;
        }
        text.push_str(&self.content[done..]);
        let grown = text.len() as isize - self.content.len() as isize;
        self.last_edit = None;
        self.splice(0..self.content.len(), &text, cx);
        if let Some(span) = self.find.as_mut().and_then(|find| find.span.as_mut()) {
            span.end = span.end.saturating_add_signed(grown);
        }
        self.sel = first..first;
        self.reversed = false;
        self.refresh_find(cx);
        cx.notify();
    }

    /// After a change of the search or its settings: the first match shows.
    pub(super) fn find_changed(&mut self, cx: &mut Context<Self>) {
        if self.refresh_find(cx) { self.show_hit(cx) } else { cx.notify() }
    }

    /// One of the search's settings switched: case, whole words, regular expression, in the selection.
    pub(super) fn find_flip(&mut self, which: usize, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.find {
            match which {
                0 => find.case ^= true,
                1 => find.word ^= true,
                2 => find.regex ^= true,
                _ => find.inside ^= true,
            }
        }
        self.find_changed(cx);
    }

    /// The find bar, at the top right of the note (below the window buttons). Its own key context: what the
    /// note's keys do (bold, undo…) never reaches it.
    pub(super) fn find_bar(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::Stateful<gpui::Div>> {
        let (t, find) = (self.theme, self.find.as_ref()?);
        let keys = |action: &dyn gpui::Action| crate::ui::keys::of(cx, action);
        let field = |id: &'static str, field: &Entity<Field>, hint: &'static str| {
            let (on, empty) = (field.focus_handle(cx).is_focused(window), field.read(cx).line.text.is_empty());
            crate::ui::widgets::field_frame(id, field.clone(), on, empty, hint, t)
        };
        let toggle = |id, label, on, name, keys| crate::ui::widgets::toggle(id, label, on, name, keys, t);
        let icon = |id: &'static str, icon: &'static str, on: bool, tip: &'static str, keys: String| {
            crate::ui::widgets::button(id, icon, on, t).debug_selector(move || id.to_string()).tooltip(crate::ui::widgets::tip(tip, keys, t))
        };
        let query = find.query(cx);
        let count = match (query.is_empty(), find.hits.len()) {
            (true, _) => String::new(),
            (false, _) if md::broken(&query, find.word, find.regex) => tr("Invalid expression", "Expression invalide").into(),
            (false, 0) => tr("No results", "Aucun résultat").into(),
            (false, n) => format!("{} / {n}", find.at + 1),
        };
        let flip = |which: usize| cx.listener(move |this, _: &gpui::ClickEvent, _, cx| this.find_flip(which, cx));
        // The chevron: the replace line, shown or not.
        let chevron = crate::ui::widgets::button("find-chevron", if find.replace { "chevron-down.svg" } else { "chevron-right.svg" }, false, t)
            .debug_selector(|| "find-chevron".into())
            .tooltip(crate::ui::widgets::tip(tr("Replace", "Remplacer"), keys(&FindReplace), t))
            .on_click(cx.listener(|this, _, window, cx| this.flip_replace(window, cx)));
        let first = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(chevron)
            .child(field("find-query", &find.query, tr("Find", "Chercher")))
            .child(div().debug_selector(|| "find-count".into()).flex_none().text_size(t.px(12.)).text_color(t.soft).child(count))
            .child(toggle("find-case", "Aa", find.case, tr("Match case", "Respecter la casse"), keys(&FindCase)).on_click(flip(0)))
            .child(toggle("find-word", "\u{201c}ab\u{201d}", find.word, tr("Whole words", "Mots entiers"), keys(&FindWord)).on_click(flip(1)))
            .child(toggle("find-regex", ".*", find.regex, tr("Regular expression", "Expression régulière"), keys(&FindRegex)).on_click(flip(2)))
            .children(find.span.as_ref().map(|_| {
                icon("find-inside", "in-selection.svg", find.inside, tr("In the selected lines only", "Seulement dans les lignes choisies"), String::new()).on_click(flip(3))
            }));
        let second = find.replace.then(|| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                // Lined up under the search field, past the chevron.
                .child(div().flex_none().w(t.px(28.)))
                .child(field("find-with", &find.with, tr("Replace with", "Remplacer par")))
                .child(icon("replace-one", "replace-one.svg", false, tr("Replace this match", "Remplacer ce passage"), keys(&FindEnter)).on_click(cx.listener(|this, _, _, cx| this.replace_one(cx))))
                .child(icon("replace-all", "replace-all.svg", false, tr("Replace all", "Tout remplacer"), keys(&ReplaceAll)).on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))))
                .child(toggle("replace-vault", tr("Vault…", "Coffre…"), false, tr("Replace in the whole vault, after showing the count", "Remplacer dans tout le coffre, après en avoir montré le compte"), keys(&ReplaceInVault)).on_click(cx.listener(|this, _, _, cx| this.replace_in_vault(cx))))
        });
        let bar = div()
            .id("find-bar")
            .debug_selector(|| "find-bar".into())
            .key_context("Find")
            .w(t.px(400.))
            .max_w_full()
            .p_1p5()
            .rounded(t.large())
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .text_size(t.px(13.))
            .text_color(t.text)
            .cursor(CursorStyle::Arrow)
            .flex()
            .flex_col()
            .gap_1p5()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|this, _: &Find, window, cx| this.open_find(false, window, cx)))
            .on_action(cx.listener(|this, _: &FindReplace, window, cx| this.open_find(true, window, cx)))
            .on_action(cx.listener(|this, _: &FindNext, window, cx| this.find_step(true, window, cx)))
            .on_action(cx.listener(|this, _: &FindPrev, window, cx| this.find_step(false, window, cx)))
            // Up and down in a field: the previous and next matches.
            .on_action(cx.listener(|this, _: &crate::views::palette::Prev, window, cx| this.find_step(false, window, cx)))
            .on_action(cx.listener(|this, _: &crate::views::palette::Next, window, cx| this.find_step(true, window, cx)))
            .on_action(cx.listener(|this, _: &FindSwitch, window, cx| this.find_switch(window, cx)))
            .on_action(cx.listener(|this, _: &FindEnter, window, cx| match this.find.as_ref().is_some_and(|find| find.with.focus_handle(cx).is_focused(window)) {
                true => this.replace_one(cx),
                false => this.find_step(true, window, cx),
            }))
            .on_action(cx.listener(|this, _: &FindCase, _, cx| this.find_flip(0, cx)))
            .on_action(cx.listener(|this, _: &FindWord, _, cx| this.find_flip(1, cx)))
            .on_action(cx.listener(|this, _: &FindRegex, _, cx| this.find_flip(2, cx)))
            .on_action(cx.listener(|this, _: &ReplaceAll, _, cx| this.replace_all(cx)))
            .on_action(cx.listener(|this, _: &ReplaceInVault, _, cx| this.replace_in_vault(cx)))
            .child(first)
            .children(second);
        // Pinned to the right, within the note's margins: a narrow note shrinks it instead of letting it overflow.
        Some(div().id("find-place").absolute().top(t.px(52.)).left_4().right_4().flex().justify_end().child(bar))
    }
}
