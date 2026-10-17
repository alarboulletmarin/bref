//! Status messages at the bottom of the window.

use super::*;

/// Ton d'un message d'état : il choisit son icône, et s'il part de lui-même.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Tone {
    Info,
    Done,
    Failed,
    /// Opération en cours : reste affiché jusqu'au message qui en donne l'issue.
    Busy,
}

pub(super) struct Toast {
    pub(super) id: usize,
    pub(super) tone: Tone,
    pub(super) text: String,
}

/// Durée d'affichage d'un message d'état.
pub(super) const TOAST: Duration = Duration::from_secs(4);

impl Shell {
    /// Affiche un message d'état, qui part au clic ou après `TOAST`. Il remplace le message
    /// « en cours », dont il est l'issue.
    // ponytail: une seule opération « en cours » à la fois ; rendre le numéro du message à
    // l'appelant le jour où deux opérations longues peuvent se chevaucher.
    pub(super) fn say(&mut self, tone: Tone, text: impl Into<String>, cx: &mut Context<Self>) {
        self.toasts.retain(|t| t.tone != Tone::Busy);
        self.toasted += 1;
        let id = self.toasted;
        self.toasts.push(Toast { id, tone, text: text.into() });
        if tone != Tone::Busy {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TOAST).await;
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

    /// Une réussite retire les erreurs encore affichées.
    pub(super) fn calm(&mut self) {
        self.toasts.retain(|t| t.tone != Tone::Failed);
    }

    /// Un message de ce ton contient `part`.
    #[cfg(test)]
    pub(super) fn told(&self, tone: Tone, part: &str) -> bool {
        self.toasts.iter().any(|t| t.tone == tone && t.text.contains(part))
    }
}

/// Fond d'un message d'état ou de la bannière de mise à jour.
pub(super) fn status_card(t: Theme) -> Div {
        div()
            .pl_3()
            .pr_2()
            .py_1p5()
            .rounded(px(6.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .text_size(px(13.))
            .flex()
            .items_center()
            .gap_3()
}

impl Shell {
    /// Messages d'état, empilés au-dessus du compteur de mots : un clic les retire.
    pub(super) fn render_toasts(&self, cx: &mut Context<Self>) -> Vec<Div> {
        let t = self.theme;
        self.toasts.iter().map(|toast| {
            let id = toast.id;
            let (icon, color) = match toast.tone {
                Tone::Info => ("info.svg", t.soft),
                Tone::Done => ("check.svg", rgb(0x3fa46a).into()),
                Tone::Failed => ("alert.svg", rgb(0xd9483b).into()),
                Tone::Busy => ("spinner.svg", t.soft),
            };
            let icon = svg().path(icon).size(px(14.)).flex_none().text_color(color);
            let icon = match toast.tone {
                Tone::Busy => icon
                    .with_animation(("toast-spin", id), Animation::new(Duration::from_secs(1)).repeat(), |icon, delta| {
                        icon.with_transformation(Transformation::rotate(percentage(delta)))
                    })
                    .into_any_element(),
                _ => icon.into_any_element(),
            };
            status_card(t)
                .max_w(px(440.))
                .shadow_lg()
                .cursor_pointer()
                .child(icon)
                .child(div().min_w_0().child(toast.text.clone()))
                .child(svg().path("close.svg").size(px(14.)).flex_none().text_color(t.soft))
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
