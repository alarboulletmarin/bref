//! CSV and TSV files open in their grid.

use super::*;

impl Shell {
    /// A table takes the note's place, like a picture; the note stays loaded underneath.
    pub(super) fn open_table(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        if self.pane().sheet().is_some() && self.pane().file_path().map(PathBuf::as_path) == Some(path) {
            cx.notify();
            return false;
        }
        self.mark_place(cx);
        match sheet::load(path, None) {
            Ok(loaded) => {
                let theme = self.theme;
                let sheet = cx.new(|cx| Sheet::new(path.to_path_buf(), loaded, theme, cx));
                cx.subscribe(&sheet, Self::on_sheet_event).detach();
                self.set_file(Some(FileView { path: path.to_path_buf(), kind: FileKind::Sheet(sheet) }), cx);
                self.calm();
            }
            Err(e) => {
                self.set_file(None, cx);
                self.say(Tone::Failed, e, cx);
            }
        }
        cx.notify();
        false
    }

    pub(super) fn on_sheet_event(&mut self, _: Entity<Sheet>, event: &SheetEvent, cx: &mut Context<Self>) {
        match event {
            SheetEvent::Error(message) => self.say(Tone::Failed, message.clone(), cx),
            // A file appeared next to the table: the tree shows it.
            SheetEvent::Exported => {
                if let Some(root) = self.vault.clone() {
                    let (notes, dirs, images) = vault::rescan(&root, &self.notes);
                    self.sync(notes, dirs, images, cx);
                }
            }
        }
        cx.notify();
    }

    /// Choice list for a setting of the shown table (delimiter, encoding, copy, export).
    pub(super) fn choose_table(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.shown_sheet() else {
            return;
        };
        if pick == Pick::Header {
            return sheet.update(cx, |sheet, cx| sheet.toggle_header(cx));
        }
        let auto = tr("Detect automatically", "Détecter automatiquement");
        let format = sheet.read(cx).format();
        let styles = [
            ("TSV", Style::Tsv),
            ("CSV", Style::Csv(format.delimiter)),
            ("Markdown", Style::Markdown),
            ("JSON", Style::Json),
        ];
        let (options, current): (Vec<String>, String) = match pick {
            Pick::Delimiter => (
                std::iter::once(auto.to_string()).chain(table::DELIMITERS.map(sheet::delimiter_option)).collect(),
                sheet::delimiter_option(format.delimiter),
            ),
            Pick::Encoding => (
                std::iter::once(auto.to_string()).chain(table::Encoding::ALL.map(|e| e.label().to_string())).collect(),
                format.encoding.label().to_string(),
            ),
            _ => (styles.iter().map(|(name, _)| name.to_string()).collect(), styles[0].0.to_string()),
        };
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(pick.label(), options, &current, theme, cx));
        cx.subscribe_in(&palette, window, move |this, _, event, window, cx| {
            let PaletteEvent::Submit(name) = event else {
                // No preview for a table: only the confirmed choice counts.
                if !matches!(event, PaletteEvent::Preview(_)) {
                    this.palette = None;
                    window.focus(&sheet.focus_handle(cx));
                    cx.notify();
                }
                return;
            };
            this.palette = None;
            window.focus(&sheet.focus_handle(cx));
            let style = styles.iter().find(|(label, _)| label == name).map(|&(_, style)| style);
            sheet.update(cx, |sheet, cx| match pick {
                Pick::Delimiter | Pick::Encoding if name == auto => sheet.reformat(None, cx),
                Pick::Delimiter => {
                    let delimiter = table::DELIMITERS.into_iter().find(|&d| sheet::delimiter_option(d) == *name);
                    sheet.reformat(delimiter.map(|delimiter| Format { delimiter, ..format }), cx)
                }
                Pick::Encoding => {
                    let encoding = Encoding::from_label(name);
                    sheet.reformat(encoding.map(|encoding| Format { encoding, ..format }), cx)
                }
                Pick::CopyAs => style.into_iter().for_each(|style| sheet.copy_as(style, cx)),
                Pick::Export => style.into_iter().for_each(|style| sheet.export(style, cx)),
                Pick::Header => {}
            });
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }
}
