//! Links and comments: making a link from an address, following it, commenting a selection.

use super::*;

impl Editor {
    /// Adresse que tient le presse-papiers, s'il n'y a que cela.
    pub(super) fn copied_url(cx: &mut Context<Self>) -> Option<String> {
        let text = cx.read_from_clipboard()?.text()?;
        md::is_url(text.trim()).then(|| text.trim().to_string())
    }

    /// La sélection, si elle peut devenir le texte d'un lien : sur une seule ligne, hors du
    /// code, et pas déjà une adresse.
    pub(super) fn linkable(&self) -> bool {
        let text = &self.content[self.sel.clone()];
        !text.contains('\n') && !md::is_url(text.trim()) && !self.in_code(self.line_range(self.sel.start).start)
    }

    /// Fait un lien `[texte](adresse)`. Sur un lien existant, son adresse est sélectionnée,
    /// prête à être remplacée. Sinon la sélection devient le texte du lien, vers l'adresse du
    /// presse-papiers s'il en tient une ; le curseur attend ce qui manque, l'adresse entre les
    /// parenthèses ou le texte entre les crochets.
    pub(super) fn link(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.sel.start);
        let (line, col) = (&self.content[lr.clone()], self.sel.start - lr.start);
        let on = md::links(line).into_iter().find(|(r, _)| (r.start..=r.end).contains(&col));
        if let Some((r, _)) = &on
            && let Some((_, url)) = md::web_link(&line[r.start..])
        {
            let at = lr.start + r.start;
            self.move_to(at + url.start, cx);
            return self.select_to(at + url.end, cx);
        }
        // Sur une adresse nue : c'est elle qui devient le lien.
        if let Some((r, md::Link::Url(_))) = on.filter(|_| self.sel.is_empty()) {
            self.sel = lr.start + r.start..lr.start + r.end;
        }
        let sel = self.sel.clone();
        // Une adresse sélectionnée garde sa place : il lui manque son texte.
        let picked = self.content[sel.clone()].to_string();
        if !md::is_url(picked.trim()) && !self.linkable() {
            return;
        }
        let (text, url) = match md::is_url(picked.trim()) {
            true => (String::new(), picked.trim().to_string()),
            false => (picked, Self::copied_url(cx).unwrap_or_default()),
        };
        self.push_undo(true);
        self.edit(sel.clone(), &format!("[{text}]({url})"), cx);
        let at = match (text.is_empty(), url.is_empty()) {
            (true, _) => 1,
            (false, true) => text.len() + 3,
            (false, false) => text.len() + url.len() + 4,
        };
        self.move_to(sel.start + at, cx);
    }

    /// Remplace tout le texte, comme une frappe : une étape d'annulation, la note est à enregistrer.
    pub fn rewrite(&mut self, text: &str, cx: &mut Context<Self>) {
        self.more.clear();
        // Une seule étape : `splice` en poserait une seconde pour un texte de plusieurs lignes.
        self.push_undo(true);
        self.held = true;
        self.splice(0..self.content.len(), text, cx);
        self.held = false;
    }

    /// Résout le commentaire qui commence à l'octet `at`.
    pub fn resolve_comment(&mut self, at: usize, cx: &mut Context<Self>) {
        self.move_to(self.clamp(at), cx);
        self.comment(cx);
    }

    /// Commente la sélection, ou la ligne sans elle : `{==texte==}{>>…<<}`, le curseur dans le
    /// commentaire à écrire. Sur un commentaire, le résout : le balisage part, le texte reste.
    pub(super) fn comment(&mut self, cx: &mut Context<Self>) {
        let lr = self.line_range(self.sel.start);
        let (line, col) = (&self.content[lr.clone()], self.sel.start - lr.start);
        if let Some((whole, noted, _)) = md::comments(line).into_iter().find(|(whole, ..)| (whole.start..=whole.end).contains(&col)) {
            let kept = line[noted].to_string();
            self.push_undo(true);
            return self.edit(lr.start + whole.start..lr.start + whole.end, &kept, cx);
        }
        let marker = md::classify(line, false).1;
        let sel = if self.sel.is_empty() { lr.start + marker..lr.end } else { self.sel.clone() };
        if sel.is_empty() || sel.end > lr.end || self.in_code(lr.start) {
            return;
        }
        let text = self.content[sel.clone()].to_string();
        self.push_undo(true);
        self.edit(sel.clone(), &format!("{{=={text}==}}{{>><<}}"), cx);
        self.move_to(sel.start + text.len() + 9, cx);
    }
}
