//! Text input from the system (IME), which counts in UTF-16.

use super::*;

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.sel),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.find.as_ref().is_some_and(|find| find.active) {
            return self.find_type(text, cx);
        }
        // Multiple cursors: each receives the text, in place of its selection.
        if !self.more.is_empty() && range_utf16.is_none() && self.marked.is_none() {
            return self.each(cx, |this, cx| this.edit(this.sel.clone(), text, cx));
        }
        if range_utf16.is_none() && self.marked.is_none() {
            self.clear_cells(cx);
        }
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        if self.type_grid(text, cx) {
            return;
        }
        self.edit(range, text, cx);
        self.realign(cx);
        if text != " " {
            return;
        }
        // `[] ` at line start becomes a checkbox.
        let c = self.cursor();
        let lr = self.line_range(c);
        let typed = self.content[lr.start..c].trim_start();
        let task = match typed {
            "[] " | "[ ] " => "- [ ] ",
            "[x] " => "- [x] ",
            _ => return,
        };
        if !self.in_code(lr.start) {
            self.edit(c - typed.len()..c, task, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.find.as_ref().is_some_and(|find| find.active) {
            return;
        }
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        let start = range.start;
        self.edit(range, text, cx);
        self.marked = (!text.is_empty()).then_some(start..start + text.len());
        if let Some(r) = new_selected_utf16 {
            let to = |n: usize| -> usize {
                text.char_indices()
                    .scan(0, |u, (i, ch)| {
                        let at = (*u, i);
                        *u += ch.len_utf16();
                        Some(at)
                    })
                    .find(|(u, _)| *u >= n)
                    .map_or(text.len(), |(_, i)| i)
            };
            self.sel = start + to(r.start)..start + to(r.end);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let i = self.offset_from_utf16(range_utf16.start);
        let row = &self.rows[self.row_at(i)?];
        let p = row.pos(i.saturating_sub(row.start));
        Some(Bounds::new(
            point(self.origin.x + p.x, self.origin.y + row.y + row.pad + p.y),
            size(px(2.), row.lh),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_at(point)))
    }
}

impl Editor {
    pub(super) fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (i, ch) in self.content.char_indices() {
            if utf16 >= offset {
                return i;
            }
            utf16 += ch.len_utf16();
        }
        self.content.len()
    }

    pub(super) fn offset_to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].encode_utf16().count()
    }

    pub(super) fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }

    pub(super) fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }
}
