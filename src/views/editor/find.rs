//! Find and replace in the note (Ctrl+F), and the vault-wide replace it can start.

use super::*;

/// Recherche dans la note : la barre, ses réglages et les passages trouvés.
#[derive(Default)]
pub(super) struct Finder {
    pub(super) query: String,
    /// Texte de remplacement ; `None` tant que la ligne « Remplacer » est fermée.
    pub(super) with: Option<String>,
    /// La recherche vient de la sélection : elle est comme sélectionnée, la première frappe
    /// la remplace.
    pub(super) fresh: bool,
    /// La saisie va au champ de remplacement.
    pub(super) on_with: bool,
    /// La barre reçoit la saisie ; un clic dans la note la lui reprend sans la fermer.
    pub(super) active: bool,
    pub(super) case: bool,
    pub(super) word: bool,
    /// La recherche est une expression régulière.
    pub(super) regex: bool,
    pub(super) hits: Vec<Range<usize>>,
    /// Passage courant dans `hits`.
    pub(super) at: usize,
    /// Texte et réglages pour lesquels `hits` a été calculé.
    pub(super) key: Option<(u64, String, bool, bool, bool)>,
}

impl Editor {
    /// Recherche, numéro du passage courant (à partir de 1) et nombre de passages.
    #[cfg(test)]
    pub fn finding(&self) -> Option<(&str, usize, usize)> {
        let find = self.find.as_ref()?;
        Some((&find.query, if find.hits.is_empty() { 0 } else { find.at + 1 }, find.hits.len()))
    }

    /// Ouvre la barre de recherche, ou lui rend la saisie ; `replace` montre aussi la ligne
    /// « Remplacer ». Une sélection tenue sur une ligne devient la recherche.
    pub(super) fn open_find(&mut self, replace: bool, cx: &mut Context<Self>) {
        let picked = Some(&self.content[self.sel.clone()]).filter(|s| !s.is_empty() && !s.contains('\n'));
        let picked = picked.map(String::from);
        let find = self.find.get_or_insert_with(Finder::default);
        find.active = true;
        find.on_with = false;
        if let Some(picked) = picked {
            (find.query, find.fresh) = (picked, true);
        }
        if replace {
            find.with.get_or_insert_with(String::new);
        }
        self.refresh_find();
        cx.notify();
    }

    /// Refait la liste des passages si le texte, la recherche ou ses réglages ont changé (et
    /// le dit) ; le passage courant est alors le premier à partir du curseur.
    pub(super) fn refresh_find(&mut self) -> bool {
        let (version, from) = (self.version, self.sel.start);
        let Some(find) = &mut self.find else {
            return false;
        };
        let key = (version, find.query.clone(), find.case, find.word, find.regex);
        if find.key.as_ref() == Some(&key) {
            return false;
        }
        find.hits = md::replacements(&self.content, &find.query, "", find.case, find.word, find.regex).into_iter().map(|(hit, _)| hit).collect();
        find.at = find.hits.iter().position(|hit| hit.start >= from).unwrap_or(0);
        find.key = Some(key);
        true
    }

    /// Sélectionne le passage courant et le fait défiler à l'écran.
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

    /// Passage suivant ou précédent, en repartant de l'autre bout une fois au bord.
    pub(super) fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        // F3 sans barre : elle s'ouvre, comme avec Ctrl+F.
        if self.find.is_none() {
            return self.open_find(false, cx);
        }
        self.refresh_find();
        if let Some(find) = &mut self.find
            && let n @ 1.. = find.hits.len()
        {
            find.at = if forward { (find.at + 1) % n } else { (find.at + n - 1) % n };
        }
        self.show_hit(cx);
    }

    /// Entrée dans la barre : passage suivant, ou remplacement du passage courant quand la
    /// saisie est dans le champ « Remplacer ».
    pub(super) fn find_enter(&mut self, cx: &mut Context<Self>) {
        self.refresh_find();
        let current = self.find.as_ref().filter(|find| find.on_with).and_then(|find| Some((find.hits.get(find.at)?.clone(), self.replacements()?)));
        // Le texte qui remplace ce passage : les groupes d'une expression régulière en font partie.
        let Some((hit, with)) = current.and_then(|(hit, all)| Some((hit.clone(), all.into_iter().find(|(range, _)| *range == hit)?.1))) else {
            return self.find_step(true, cx);
        };
        self.edit(hit, &with, cx);
        self.realign(cx);
        self.refresh_find();
        self.show_hit(cx);
    }

    /// Les passages trouvés et ce qui remplace chacun ; `None` sans champ « Remplacer ».
    pub(super) fn replacements(&self) -> Option<Vec<(Range<usize>, String)>> {
        let find = self.find.as_ref()?;
        Some(md::replacements(&self.content, &find.query, find.with.as_ref()?, find.case, find.word, find.regex))
    }

    /// Demande à la fenêtre de remplacer dans tout le coffre : la recherche, le remplacement et
    /// les réglages de la barre.
    pub(super) fn replace_in_vault(&mut self, cx: &mut Context<Self>) {
        if let Some(find) = self.find.as_ref().filter(|find| !find.query.is_empty())
            && let Some(with) = find.with.clone()
        {
            cx.emit(EditorEvent::Swap(md::Swap { query: find.query.clone(), with, case: find.case, word: find.word, regex: find.regex }));
        }
    }

    /// Remplace tous les passages d'un coup : une seule étape d'annulation.
    pub(super) fn replace_all(&mut self, cx: &mut Context<Self>) {
        self.refresh_find();
        let Some(hits) = self.replacements() else {
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
        self.last_edit = None;
        self.splice(0..self.content.len(), &text, cx);
        self.sel = first..first;
        self.reversed = false;
        self.refresh_find();
    }

    /// Champ de la barre qui reçoit la saisie.
    pub(super) fn find_field(&mut self) -> Option<&mut String> {
        let find = self.find.as_mut()?;
        if find.on_with {
            return find.with.as_mut();
        }
        if std::mem::take(&mut find.fresh) {
            find.query.clear();
        }
        Some(&mut find.query)
    }

    // ponytail: comme la palette, la saisie de la barre se réduit à « ajouter à la fin » (pas
    // de curseur mobile ni de composition IME) ; à remplacer par un vrai champ de texte si
    // les recherches s'allongent.
    pub(super) fn find_type(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(field) = self.find_field() {
            field.extend(text.chars().filter(|c| !c.is_control()));
        }
        self.find_changed(cx);
    }

    pub(super) fn find_erase(&mut self, cx: &mut Context<Self>) {
        if let Some(field) = self.find_field() {
            let last = field.graphemes(true).next_back().map_or(0, str::len);
            field.truncate(field.len() - last);
        }
        self.find_changed(cx);
    }

    /// Après un changement de la recherche ou de ses réglages : le premier passage se montre.
    pub(super) fn find_changed(&mut self, cx: &mut Context<Self>) {
        if self.refresh_find() { self.show_hit(cx) } else { cx.notify() }
    }

    /// La barre de recherche, en haut à droite de la note (sous les boutons de la fenêtre).
    pub(super) fn find_bar(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let (t, find) = (self.theme, self.find.as_ref()?);
        let field = |text: &str, hint: &'static str, on: bool, fresh: bool| {
            let caret = on.then(|| div().flex_none().w(px(1.)).h(px(15.)).bg(t.text));
            let shown = match text.is_empty() {
                true => div().text_color(t.soft).child(hint),
                false => div().when(fresh, |d| d.bg(t.selection)).child(text.to_string()),
            };
            let field = div().flex_1().min_w(px(90.)).h(px(26.)).px_2().rounded(px(6.)).border_1().bg(t.bg).cursor(CursorStyle::IBeam);
            let field = field.border_color(if on { t.accent } else { t.border }).flex().items_center().overflow_hidden();
            match text.is_empty() {
                true => field.children(caret).child(shown),
                false => field.child(shown).children(caret),
            }
        };
        let toggle = |label: &'static str, on: bool| {
            let toggle = div().flex_none().h(px(26.)).px_1p5().rounded(px(6.)).flex().items_center().cursor_pointer();
            toggle.text_color(if on { t.accent } else { t.soft }).when(on, |d| d.bg(t.selection)).child(label)
        };
        let count = match (find.query.is_empty(), find.hits.len()) {
            (true, _) => String::new(),
            (false, 0) => tr("No results", "Aucun résultat").into(),
            (false, n) => format!("{} / {n}", find.at + 1),
        };
        let pick = |on_with: bool| {
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if let Some(find) = &mut this.find {
                    (find.active, find.on_with) = (true, on_with);
                }
                cx.notify();
            })
        };
        let flip = |which: u8| {
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                if let Some(find) = &mut this.find {
                    *[&mut find.case, &mut find.word, &mut find.regex][which as usize] ^= true;
                }
                this.find_changed(cx);
            })
        };
        let bar = div()
            .debug_selector(|| "find-bar".into())
            .w(px(340.))
            .max_w_full()
            .p_1p5()
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .text_size(px(13.))
            .text_color(t.text)
            .cursor(CursorStyle::Arrow)
            .flex()
            .flex_col()
            .gap_1p5()
            .occlude();
        // Dans une note étroite, le compte et les réglages passent sous le champ.
        let first = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1p5()
            .child(field(&find.query, tr("Find", "Chercher"), find.active && !find.on_with, find.fresh).on_mouse_down(MouseButton::Left, pick(false)))
            .child(div().flex_none().text_size(px(12.)).text_color(t.soft).child(count))
            .child(toggle("Aa", find.case).on_mouse_down(MouseButton::Left, flip(0)))
            .child(toggle("\u{201c}ab\u{201d}", find.word).on_mouse_down(MouseButton::Left, flip(1)))
            .child(toggle(".*", find.regex).on_mouse_down(MouseButton::Left, flip(2)));
        let second = find.with.as_ref().map(|with| {
            let everywhere = cx.listener(|this, _: &MouseDownEvent, _, cx| this.replace_in_vault(cx));
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    field(with, tr("Replace with", "Remplacer par"), find.active && find.on_with, false)
                        .on_mouse_down(MouseButton::Left, pick(true)),
                )
                .child(toggle(tr("Vault…", "Coffre…"), false).on_mouse_down(MouseButton::Left, everywhere))
        });
        // Calée à droite, dans les marges de la note : une note étroite la rétrécit au lieu de la laisser sortir.
        Some(div().absolute().top(px(52.)).left_4().right_4().flex().justify_end().child(bar.child(first).children(second)))
    }
}
