//! Quick capture: a one-line window (`bref --capture`) and its system shortcut.

use super::*;

/// The capture window (`bref --capture`, to bind to a desktop shortcut): one input
/// line, nothing else. Enter appends it to the daily note and closes; Escape closes. The vault
/// is not read, the window is up as fast as the app.
// ponytail: if Bref is open on the daily note, its vault watcher reloads it;
// a keystroke pending there is saved 400 ms later (well before anything is typed here).
// Talk to the open instance if that delay ever grew longer.
pub(super) struct Capture {
    pub(super) root: PathBuf,
    pub(super) theme: Theme,
    pub(super) field: Entity<Palette>,
}

impl Capture {
    pub(super) fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prefs = Prefs::parse(&vault::load_settings());
        apply_fonts(&prefs);
        let theme = Theme::of(&prefs, window.appearance());
        let field = Self::ask(tr("Add to today's note", "Ajouter à la note du jour").into(), "", theme, window, cx);
        Self { root, theme, field }
    }

    /// The input field: the palette's, announcing `label` and starting from `text`.
    pub(super) fn ask(label: String, text: &str, theme: Theme, window: &mut Window, cx: &mut Context<Self>) -> Entity<Palette> {
        let field = cx.new(|cx| Palette::prompt(label, text, theme, cx).at_top());
        cx.subscribe_in(&field, window, |this, _, event, window, cx| match event {
            PaletteEvent::Submit(text) if !text.is_empty() => match vault::capture(&this.root, &date_name(today()), text) {
                Ok(_) => window.remove_window(),
                // Nothing is lost: the line stays, with the reason.
                Err(e) => {
                    let label = format!("{} : {}", tr("Not saved", "Non enregistré"), crate::lang::io_reason(&e));
                    this.field = Self::ask(label, text, this.theme, window, cx);
                    cx.notify();
                }
            },
            PaletteEvent::Submit(_) | PaletteEvent::Dismiss => window.remove_window(),
            _ => {}
        })
        .detach();
        window.focus(&field.focus_handle(cx));
        field
    }
}

impl Render for Capture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().font_family(sans()).text_color(self.theme.text).child(self.field.clone())
    }
}

impl Shell {
    /// Binds capture to a desktop shortcut, and says which. Where Bref cannot set it
    /// itself, the command to bind is copied: all that is left is to paste it.
    pub(super) fn capture_shortcut(&mut self, cx: &mut Context<Self>) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let made = cx.background_executor().spawn(async move { vault::capture_shortcut(&exe) }).await;
            this.update(cx, |this, cx| {
                let told = match made {
                    vault::Shortcut::Bound(keys) => format!("{} : {keys}", tr("Quick capture, from anywhere", "Capture rapide, depuis n'importe où")),
                    vault::Shortcut::Unbound(keys) => format!(
                        "{keys} {}",
                        tr("is taken: choose the keys of “Bref: capture” in Settings › Keyboard", "est pris : choisir les touches de « Bref : capture » dans Réglages › Clavier")
                    ),
                    vault::Shortcut::Manual(command) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(command));
                        tr("Capture command copied: paste it into a new shortcut of your system settings", "Commande de capture copiée : la coller dans un nouveau raccourci des réglages du système").to_string()
                    }
                };
                this.say(Tone::Done, told, cx);
            })
            .ok();
        })
        .detach();
    }
}
