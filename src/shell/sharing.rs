//! Document-aware local sharing. A copy never adopts a new path or saves the source.

use super::*;
use crate::{logic::sharing, platform::export, views::nav::{Menu, Share}};

impl Shell {
    pub(super) fn share_choices(&self, pane: usize) -> Vec<Share> {
        let Some(pane) = self.panes.get(pane) else { return Vec::new() };
        if pane.tool.is_some() || pane.listing.is_some() || pane.figure.is_some() {
            return Vec::new();
        }
        match pane.file.as_ref().map(|file| &file.kind) {
            None => vec![Share::MarkdownCopy, Share::PlainCopy, Share::MarkdownSave, Share::PlainSave, Share::Pdf],
            Some(FileKind::Drawing(_)) => vec![Share::SvgCopy, Share::SvgSave, Share::Png],
            Some(FileKind::Svg(_)) => vec![Share::SvgCopy, Share::SvgSave],
            Some(FileKind::Sheet(_)) => vec![Share::TableCopy, Share::TableExport],
            Some(FileKind::Image) => vec![Share::ImageSave],
        }
    }

    pub(super) fn show_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.share_choices(self.active).is_empty() { return; }
        // Anchored clamps the common floating menu to the window, including narrow panes.
        let area = self.pane_box.get();
        let whole = layout::Rect { x: 0., y: 0., w: 1., h: 1. };
        let rect = if self.alone().is_some() { whole } else {
            self.layout.rects().into_iter().find(|(group, _)| *group == self.group()).map_or(whole, |(_, rect)| rect)
        };
        let right = area.left() + area.size.width * (rect.x + rect.w);
        let bottom = area.top() + area.size.height * (rect.y + rect.h);
        self.show_menu(Menu {
            at: point((right - self.theme.px(290.)).max(px(8.)), (bottom - self.theme.px(190.)).max(px(8.))),
            share: Some(self.active), chosen: Some(0), ..Default::default()
        }, window, cx);
    }

    pub(super) fn share_document(&mut self, choice: Share, window: &mut Window, cx: &mut Context<Self>) {
        match choice {
            Share::MarkdownCopy => {
                self.copy_all(false, cx);
                self.say(Tone::Done, tr("Note copied as Markdown", "Note copiée en Markdown"), cx);
                return;
            }
            Share::PlainCopy => {
                let text = sharing::plain_text(self.pane().editor.read(cx).text());
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.say(Tone::Done, tr("Note copied as plain text", "Note copiée en texte brut"), cx);
                return;
            }
            Share::Pdf => return self.export_pdf(cx),
            Share::TableCopy => return self.choose_table(Pick::CopyAs, window, cx),
            Share::TableExport => return self.choose_table(Pick::Export, window, cx),
            Share::Png => {
                if let (Some(path), Some(canvas)) = (self.pane().file_path().cloned(), self.pane().drawing().cloned()) {
                    self.export_png(path, &canvas, cx);
                }
                return;
            }
            _ => {}
        }
        let result: std::io::Result<(Vec<u8>, &str)> = match choice {
            Share::MarkdownSave => Ok((self.pane().editor.read(cx).text().as_bytes().to_vec(), "md")),
            Share::PlainSave => Ok((sharing::plain_text(self.pane().editor.read(cx).text()).into_bytes(), "txt")),
            Share::SvgCopy | Share::SvgSave => {
                match self.pane().drawing() {
                    Some(canvas) => Ok((canvas.read(cx).diagram().to_svg(diagram::COLORS[0]).into_bytes(), "svg")),
                    None => self.pane().file_path().ok_or_else(|| std::io::Error::other("SVG"))
                        .and_then(fs::read).map(|bytes| (bytes, "svg")),
                }
            }
            Share::ImageSave => self.pane().file_path().ok_or_else(|| std::io::Error::other("Image"))
                .and_then(|path| fs::read(path).map(|bytes| (bytes, path.extension().and_then(|s| s.to_str()).unwrap_or("png")))),
            _ => return,
        };
        let (bytes, extension) = match result {
            Ok(copy) => copy,
            Err(e) => {
                self.say(Tone::Failed, format!("{} : {}", tr("Copy unavailable", "Copie indisponible"), crate::lang::io_reason(&e)), cx);
                return;
            }
        };
        if matches!(choice, Share::SvgCopy) {
            cx.write_to_clipboard(ClipboardItem::new_string(String::from_utf8_lossy(&bytes).into_owned()));
            self.say(Tone::Done, tr("SVG copied", "SVG copié"), cx);
            return;
        }
        let source = self.pane().displayed_path().cloned();
        let dir = source.as_deref().and_then(Path::parent).map(Path::to_path_buf)
            .or(self.vault.clone()).unwrap_or_else(|| PathBuf::from("."));
        let stem = source.as_deref().map(vault::stem).unwrap_or_else(|| tr("Untitled", "Sans titre").to_string());
        let name = format!("{stem}{}.{extension}", tr(" copy", " copie"));
        // Bytes and suggested directory belong to this document, even if tabs change
        // while the native dialog is open. Cancellation performs no write.
        let selected = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            let selected = selected.await;
            match selected {
                Ok(Ok(Some(path))) => {
                    let saved = cx.background_executor().spawn(async move {
                        let result = export::save(&path, &bytes);
                        (path, result)
                    }).await;
                    this.update(cx, |this, cx| {
                        let (path, result) = saved;
                        match result {
                            Ok(()) => this.say(Tone::Done, format!("{} « {} »", tr("Copy saved", "Copie enregistrée"), path.display()), cx),
                            Err(e) => this.say(Tone::Failed, format!("{} « {} » : {}", tr("Copy not saved", "Copie non enregistrée"), path.display(), crate::lang::io_reason(&e)), cx),
                        }
                    }).ok();
                }
                Ok(Ok(None)) => {}
                _ => { this.update(cx, |this, cx| this.say(Tone::Failed, tr("Could not open the save dialog", "Impossible d’ouvrir le dialogue d’enregistrement"), cx)).ok(); }
            }
        }).detach();
    }
}
