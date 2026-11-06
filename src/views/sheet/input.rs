//! Text input from the system (IME) into the cell being edited.

use super::*;

impl crate::ui::line::LineInput for Sheet {
    fn line(&self) -> Option<&Line> {
        self.edit.as_ref()
    }

    fn line_mut(&mut self) -> Option<&mut Line> {
        self.edit.as_mut()
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    /// Typing on a cell replaces its content, as in a spreadsheet.
    fn start(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        if text.chars().all(char::is_control) {
            return false;
        }
        self.begin_edit(Some(""), cx);
        self.edit.is_some()
    }
}

crate::ui::line::line_input!(Sheet);
