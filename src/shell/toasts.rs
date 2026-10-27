//! Status messages at the bottom of the window.

use super::*;

/// Tone of a status message: it picks its icon, and whether it goes away by itself.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Tone {
    Info,
    Done,
    Failed,
    /// Operation in progress: stays shown until the message that gives its outcome.
    Busy,
}

pub(super) struct Toast {
    pub(super) id: usize,
    pub(super) tone: Tone,
    pub(super) text: String,
    /// What the « Undo » button of the message does: puts back where it was (`.0`, now in the trash) what went to `.1`.
    pub(super) undo: Option<(PathBuf, PathBuf)>,
}

/// Display duration of a status message.
pub(super) const TOAST: Duration = Duration::from_secs(4);
/// A failure is read, not glanced at: it stays longer, and a click or Esc takes it away.
pub(super) const FAILED: Duration = Duration::from_secs(12);

impl Shell {
    /// Shows a status message, which goes on click or after `TOAST`. It replaces the
    /// "in progress" message whose outcome it is.
    // ponytail: a single "in progress" operation at a time; return the message number to
    // the caller the day two long operations can overlap.
    pub(super) fn say(&mut self, tone: Tone, text: impl Into<String>, cx: &mut Context<Self>) {
        self.say_undoable(tone, text, None, cx);
    }

    /// `say`, with an « Undo » button that puts `undo.0` back at `undo.1`.
    pub(super) fn say_undoable(&mut self, tone: Tone, text: impl Into<String>, undo: Option<(PathBuf, PathBuf)>, cx: &mut Context<Self>) {
        self.toasts.retain(|t| t.tone != Tone::Busy);
        self.toasted += 1;
        let id = self.toasted;
        self.toasts.push(Toast { id, tone, text: text.into(), undo });
        if tone != Tone::Busy {
            // A failure stays longest; a long message is given time to be read.
            let text_len = self.toasts.last().map_or(0, |t| t.text.chars().count());
            let lasts = if tone == Tone::Failed { FAILED } else { TOAST + Duration::from_millis(40 * text_len.saturating_sub(40) as u64) };
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(lasts).await;
                this.update(cx, |this, cx| {
                    this.toasts.retain(|t| t.id != id);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// Esc: the failures still shown go away.
    pub(super) fn dismiss_failures(&mut self) {
        self.toasts.retain(|t| t.tone != Tone::Failed);
    }

    /// A success removes the errors still shown.
    pub(super) fn calm(&mut self) {
        self.toasts.retain(|t| t.tone != Tone::Failed);
    }

    /// A message of this tone contains `part`.
    #[cfg(test)]
    pub(super) fn told(&self, tone: Tone, part: &str) -> bool {
        self.toasts.iter().any(|t| t.tone == tone && t.text.contains(part))
    }
}

/// Background of a status message or of the update banner.
pub(super) fn status_card(t: Theme) -> Div {
        div()
            .pl_3()
            .pr_2()
            .py_1p5()
            .rounded(t.small())
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .text_size(t.px(13.))
            .flex()
            .items_center()
            .gap_3()
}

impl Shell {
    /// Status messages, stacked above the word counter: a click removes them.
    pub(super) fn render_toasts(&self, cx: &mut Context<Self>) -> Vec<Div> {
        let t = self.theme;
        self.toasts.iter().map(|toast| {
            let id = toast.id;
            let (icon, color) = match toast.tone {
                Tone::Info => ("info.svg", t.soft),
                Tone::Done => ("check.svg", t.hue(ui::theme::GREEN, t.panel)),
                Tone::Failed => ("alert.svg", t.hue(ui::theme::RED, t.panel)),
                Tone::Busy => ("spinner.svg", t.soft),
            };
            let icon = svg().path(icon).size(t.px(14.)).flex_none().text_color(color);
            let icon = match toast.tone {
                Tone::Busy => icon
                    .with_animation(("toast-spin", id), Animation::new(Duration::from_secs(1)).repeat(), |icon, delta| {
                        icon.with_transformation(Transformation::rotate(percentage(delta)))
                    })
                    .into_any_element(),
                _ => icon.into_any_element(),
            };
            status_card(t)
                .max_w(t.px(440.))
                .shadow(t.floating())
                .cursor_pointer()
                .child(icon)
                .child(div().min_w_0().child(toast.text.clone()))
                .children(toast.undo.clone().map(|(from, to)| {
                    div()
                        .id(("toast-undo", id))
                        .debug_selector(|| "toast-undo".into())
                        .flex_none()
                        .px_2()
                        .rounded(t.small())
                        .text_color(t.accent)
                        .hover(|s| s.bg(t.hover))
                        .child(tr("Undo", "Annuler"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.toasts.retain(|t| t.id != id);
                                this.put_back(&from, &to, cx);
                                cx.stop_propagation();
                            }),
                        )
                }))
                .child(svg().path("close.svg").size(t.px(14.)).flex_none().text_color(t.soft))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.toasts.retain(|t| t.id != id);
                        cx.notify();
                    }),
                )
        }).collect()
    }
}
