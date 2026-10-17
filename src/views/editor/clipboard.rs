//! Copy and paste: text, pictures, table cells.

use super::*;

impl Editor {
    pub(super) fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        if let Some(text) = self.pick_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            if cut {
                self.clear_pick(cx);
            }
            return;
        }
        if self.sel.is_empty() {
            return;
        }
        let text = self.cells_text().unwrap_or_else(|| self.content[self.sel.clone()].to_string());
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if cut && !self.clear_cells(cx) {
            self.edit(self.sel.clone(), "", cx);
            self.realign(cx);
        }
    }

    pub(super) fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let image = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        });
        let text = match (image, self.dirs.first()) {
            // A pasted picture is saved next to the note, which points to it.
            (Some(image), Some(dir)) => {
                let extension = match image.format {
                    ImageFormat::Png => "png",
                    ImageFormat::Jpeg => "jpg",
                    ImageFormat::Webp => "webp",
                    ImageFormat::Gif => "gif",
                    ImageFormat::Svg => "svg",
                    ImageFormat::Bmp => "bmp",
                    ImageFormat::Tiff => "tiff",
                };
                match vault::save_image(dir, extension, &image.bytes) {
                    Ok(name) => format!("![]({name})"),
                    Err(_) => return,
                }
            }
            _ => match item.text() {
                Some(text) => text.replace("\r\n", "\n"),
                None => return,
            },
        };
        if self.paste_in_table(&text, cx) {
            return;
        }
        // A Markdown table, or spreadsheet or web page cells, land as a table,
        // on their own lines and reformatted.
        let line = self.line_range(self.sel.start);
        let (sel, table) = (self.sel.clone(), md::paste_table(&text).filter(|_| !self.in_code(line.start)));
        if let Some(table) = table {
            let alone = self.content[line.start..sel.start].trim().is_empty();
            let apart = line.start == 0 || self.content[..line.start].ends_with("\n\n");
            let lead = match (alone, apart) {
                (true, true) => "",
                (true, false) => "\n",
                (false, _) => "\n\n",
            };
            let rest = self.line_range(sel.end);
            let next = self.content[rest.end..].strip_prefix('\n').and_then(|n| n.lines().next());
            let tail = if !self.content[sel.end..rest.end].trim().is_empty() {
                "\n\n"
            } else if next.is_some_and(|n| !n.trim().is_empty()) {
                "\n"
            } else {
                ""
            };
            let from = if alone { line.start } else { sel.start };
            self.push_undo(true);
            return self.edit(from..sel.end, &format!("{lead}{table}{tail}"), cx);
        }
        self.push_undo(true);
        // An address pasted over text makes it a link.
        if !sel.is_empty() && md::is_url(text.trim()) && self.linkable() {
            let linked = format!("[{}]({})", &self.content[sel.clone()], text.trim());
            return self.edit(sel, &linked, cx);
        }
        self.edit(sel, &text, cx);
        // A line pasted under a table becomes part of it: its columns line up.
        self.realign(cx);
    }
}
