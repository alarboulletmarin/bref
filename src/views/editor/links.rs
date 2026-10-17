//! Links and comments: making a link from an address, following it, commenting a selection.

use super::*;

impl Editor {
    /// Address held by the clipboard, if that is all it holds.
    pub(super) fn copied_url(cx: &mut Context<Self>) -> Option<String> {
        let text = cx.read_from_clipboard()?.text()?;
        md::is_url(text.trim()).then(|| text.trim().to_string())
    }

    /// The selection, if it can become a link's text: on a single line, outside
    /// code, and not already an address.
    pub(super) fn linkable(&self) -> bool {
        let text = &self.content[self.sel.clone()];
        !text.contains('\n') && !md::is_url(text.trim()) && !self.in_code(self.line_range(self.sel.start).start)
    }

    /// Makes a link `[text](address)`. On an existing link, its address is selected,
    /// ready to be replaced. Otherwise the selection becomes the link text, to the clipboard's
    /// address if it holds one; the cursor waits for what is missing, the address between the
    /// parentheses or the text between the brackets.
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
        // On a bare address: it becomes the link.
        if let Some((r, md::Link::Url(_))) = on.filter(|_| self.sel.is_empty()) {
            self.sel = lr.start + r.start..lr.start + r.end;
        }
        let sel = self.sel.clone();
        // A selected address keeps its place: it lacks its text.
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

    /// Replaces all the text, like a keystroke: one undo step, the note needs saving.
    pub fn rewrite(&mut self, text: &str, cx: &mut Context<Self>) {
        self.more.clear();
        // A single step: `splice` would add a second one for a multi-line text.
        self.push_undo(true);
        self.held = true;
        self.splice(0..self.content.len(), text, cx);
        self.held = false;
    }

    /// Resolves the comment starting at byte `at`.
    pub fn resolve_comment(&mut self, at: usize, cx: &mut Context<Self>) {
        self.move_to(self.clamp(at), cx);
        self.comment(cx);
    }

    /// Comments the selection, or the line without one: `{==text==}{>>…<<}`, the cursor in the
    /// comment to write. On a comment, resolves it: the markup goes, the text stays.
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
