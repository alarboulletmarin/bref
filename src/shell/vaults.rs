//! Several vaults, switched in one shortcut. Bref remembers the vaults it opened (`logic::vaults`); `Ctrl+O`
//! lists them in the palette, and switching brings back what each one had on screen: its tabs and panes, its
//! panel, its recent notes, its back and forward. A vault can wear a short name and a colour (a band at the
//! top of the rail). Taking one off the list never touches its folder; one whose folder is gone stays listed,
//! faded, to be found again.

use crate::logic::vaults::{self, COLORS, Known, Session};
use crate::ui::theme::highlight_tint;

use super::*;

/// The names of the colours a vault can wear, in the order of `COLORS` (the highlights' hues).
fn color_names() -> [&'static str; COLORS] {
    [tr("Yellow", "Jaune"), tr("Green", "Vert"), tr("Blue", "Bleu"), tr("Pink", "Rose"), tr("Purple", "Violet")]
}

/// Seconds since 1970, as the list keeps the last use.
fn now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Shell {
    /// The open vault, as the list knows it.
    pub(super) fn known_vault(&self) -> Option<&Known> {
        let root = self.vault.as_ref()?;
        self.vaults.iter().find(|k| k.path == *root)
    }

    fn save_vaults(&self) {
        config::keep(config::File::Vaults, &vaults::write(&self.vaults));
    }

    /// The vault at `root` was just opened: the list remembers it, and when.
    pub(super) fn remember_vault(&mut self, root: &Path) {
        vaults::remember(&mut self.vaults, root, now());
        self.save_vaults();
    }

    /// The index of the open vault is built: the list keeps how many notes it has, for the switcher.
    pub(super) fn count_notes(&mut self) {
        let count = self.notes.iter().filter(|n| !self.is_template(n)).count();
        let Some(root) = self.vault.clone() else { return };
        if let Some(known) = self.vaults.iter_mut().find(|k| k.path == root)
            && known.notes != Some(count)
        {
            known.notes = Some(count);
            self.save_vaults();
        }
    }

    /// Opens another vault, keeping what this one had on screen and bringing back what that one had. Every way
    /// of changing vault goes through here, but the first one at startup.
    pub(super) fn switch_to(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.as_ref() == Some(&root) {
            return;
        }
        // A sync works in the folder it started in; it ends before another vault takes the window.
        if self.syncing {
            return self.say(Tone::Info, tr("The vault is syncing: switch once it is done", "Le coffre se synchronise : changez une fois terminé"), cx);
        }
        // Unsaved work stops the switch, as it stops closing the window.
        if !self.may_close_window(window, cx) {
            return;
        }
        self.keep_session(cx);
        self.set_vault(root.clone(), window, cx);
        if self.vault.as_ref() == Some(&root) {
            self.bring_back(&root, window, cx);
        }
    }

    /// What the open vault has on screen goes to its session file; its back and forward stay in memory.
    fn keep_session(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else { return };
        self.count_notes();
        let Some(id) = self.known_vault().map(|k| k.id) else { return };
        let session = Session { panel: self.nav.line(), recent: self.recent.clone(), panes: self.panes_state() };
        config::keep(config::File::Session(id), &vaults::write_session(&session));
        self.histories.insert(root, std::mem::take(&mut self.history));
        cx.notify();
    }

    /// The vault just opened gets back its session: panel, recent notes, tabs and panes, history. A vault never
    /// left has none: it opens on a new note, as before.
    fn bring_back(&mut self, root: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.history = self.histories.remove(root).unwrap_or_default();
        let Some(id) = self.known_vault().map(|k| k.id) else { return };
        let text = config::load(config::File::Session(id));
        if text.is_empty() {
            return;
        }
        let session = vaults::read_session(&text);
        if !session.panel.is_empty() {
            self.nav.restore(&session.panel);
            self.settle_nav(window, cx);
        }
        let (tree, saved) = read_panes(&session.panes);
        self.restore_panes(tree, saved, session.panes.starts_with("v3 "), root, window, cx);
        // Reopening the tabs touched their notes: the recent notes are set after, in their own order.
        self.recent = session.recent.into_iter().filter(|p| p.starts_with(root) && p.is_file()).collect();
        config::save_config(root, &self.recent);
        self.panes_saved = self.panes_state();
        config::keep(config::File::Panes, &self.panes_saved);
        cx.notify();
    }

    /// `Ctrl+O`: the vaults of the list in the palette, the open one first selected and marked, then « Open
    /// another folder… ». Without a vault, the folder dialog straight away.
    pub(super) fn open_vaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return self.choose_vault(window, cx);
        }
        self.show_vaults(None, window, cx);
    }

    /// The switcher, row `at` selected, or the open vault's.
    fn show_vaults(&mut self, at: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let current = at.unwrap_or_else(|| self.vaults.iter().position(|k| Some(&k.path) == self.vault.as_ref()).unwrap_or(0));
        let entries = self.vault_entries();
        let theme = self.theme;
        let label = tr("Switch to the vault…", "Passer au coffre…");
        let palette = cx.new(|cx| Palette::choose_entries(label, entries, current, true, theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            let chosen = palette.read(cx).chosen();
            match event {
                PaletteEvent::Preview(_) => return,
                PaletteEvent::Remove(at) => return this.forget_vault(*at, window, cx),
                PaletteEvent::Shift(at, up) => return this.shift_vault(*at, *up, window, cx),
                _ => {}
            }
            this.palette = None;
            this.focus_pane(this.active, window, cx);
            if let (PaletteEvent::Submit(_), Some(at)) = (event, chosen) {
                match this.vaults.get(at).map(|k| k.path.clone()) {
                    Some(path) if path.is_dir() => this.switch_to(path, window, cx),
                    Some(_) => this.find_vault(at, window, cx),
                    None => this.choose_vault(window, cx),
                }
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// The switcher's rows: each vault, what it says (open, its notes, or that its folder is gone) and when
    /// it was last used; then the row that opens the folder dialog.
    fn vault_entries(&self) -> Vec<Entry> {
        let now = now();
        let mut entries: Vec<Entry> = self
            .vaults
            .iter()
            .map(|k| {
                let here = Some(&k.path) == self.vault.as_ref();
                let gone = !here && !k.path.is_dir();
                let notes = k.notes.map(|n| format!("{} {}", crate::lang::number(n), if n == 1 { "note" } else { "notes" }));
                let said = match (here, gone) {
                    (_, true) => tr("folder not found: Enter to find it", "dossier introuvable : Entrée pour le retrouver").to_string(),
                    (true, _) => [Some(tr("open", "ouvert").to_string()), notes].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                    _ => notes.unwrap_or_default(),
                };
                let days = (k.used > 0).then(|| now.saturating_sub(k.used) / 86_400);
                let name = if here { format!("● {}", k.label()) } else { k.label() };
                Entry { name, path: k.path.clone(), days, said, faded: gone, ..Entry::default() }
            })
            .collect();
        entries.push(Entry { name: tr("Open another folder…", "Ouvrir un autre dossier…").to_string(), said: tr("a vault not in the list", "un coffre absent de la liste").to_string(), ..Entry::default() });
        entries
    }

    /// Shift+Delete in the switcher: the vault leaves the list, its folder stays. The open one stays listed.
    fn forget_vault(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(known) = self.vaults.get(at).cloned() else { return };
        if Some(&known.path) == self.vault.as_ref() {
            return self.say(Tone::Info, tr("The open vault stays in the list", "Le coffre ouvert reste dans la liste"), cx);
        }
        self.vaults.remove(at);
        self.save_vaults();
        config::forget(config::File::Session(known.id));
        self.histories.remove(&known.path);
        self.say(Tone::Done, format!("{} « {} » ({})", tr("Taken off the list:", "Retiré de la liste :"), known.label(), tr("its folder is untouched", "son dossier est intact")), cx);
        self.reopen_vaults(at.min(self.vaults.len().saturating_sub(1)), window, cx);
    }

    /// Alt+Up / Alt+Down in the switcher: the vault moves one place, the selection with it.
    fn shift_vault(&mut self, at: usize, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        if vaults::shift(&mut self.vaults, at, up) {
            self.save_vaults();
            self.reopen_vaults(if up { at - 1 } else { at + 1 }, window, cx);
        }
    }

    /// The switcher drawn again after an edit of the list, `at` selected.
    fn reopen_vaults(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.show_vaults(Some(at), window, cx);
    }

    /// A vault whose folder is gone (a drive not plugged in, a folder renamed): the folder dialog, and the folder
    /// chosen takes its place in the list, with its name, colour and session.
    fn find_vault(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("This is the vault", "C'est ce coffre").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(root) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.found_vault(at, root, window, cx)).ok();
            }
        })
        .detach();
    }

    /// The vault at `at` lives at `root` now. A folder already listed under another entry is simply opened.
    pub(super) fn found_vault(&mut self, at: usize, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.vaults.iter().all(|k| k.path != root)
            && let Some(known) = self.vaults.get_mut(at)
        {
            let old = std::mem::replace(&mut known.path, root.clone());
            // Its recent notes and tabs were under the old folder: they follow.
            let text = config::load(config::File::Session(known.id));
            if !text.is_empty() {
                let session = vaults::read_session(&text.replace(&old.display().to_string(), &root.display().to_string()));
                config::keep(config::File::Session(known.id), &vaults::write_session(&session));
            }
            self.save_vaults();
        }
        self.switch_to(root, window, cx);
    }

    /// « Name this vault »: a short name for the switcher and the band of the rail; empty, the folder's name.
    pub(super) fn name_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(known) = self.known_vault() else { return };
        let current = known.name.clone().unwrap_or_default();
        let theme = self.theme;
        let label = tr("Name of this vault (empty: the folder's)", "Nom de ce coffre (vide : celui du dossier)");
        let prompt = cx.new(|cx| Palette::prompt(label, &current, theme, cx));
        cx.subscribe_in(&prompt, window, |this, _, event, window, cx| {
            this.palette = None;
            this.focus_pane(this.active, window, cx);
            if let PaletteEvent::Submit(name) = event
                && let Some(root) = this.vault.clone()
                && let Some(known) = this.vaults.iter_mut().find(|k| k.path == root)
            {
                known.name = Some(name.replace(['\t', '\n', '\r'], " ").trim().to_string()).filter(|n| !n.is_empty());
                this.save_vaults();
            }
            cx.notify();
        })
        .detach();
        window.focus(&prompt.focus_handle(cx));
        self.palette = Some(prompt);
        cx.notify();
    }

    /// « Colour of this vault »: none, or one of five, previewed on the band of the rail as the list is browsed.
    pub(super) fn color_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(known) = self.known_vault() else { return };
        let before = known.color;
        let none = tr("No colour", "Aucune couleur");
        let options: Vec<String> = std::iter::once(none.to_string()).chain(color_names().iter().map(|n| n.to_string())).collect();
        let current = before.map_or(none.to_string(), |c| color_names()[c].to_string());
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::choose(tr("Colour of this vault", "Couleur de ce coffre"), options, &current, theme, cx));
        cx.subscribe_in(&palette, window, move |this, palette, event, window, cx| {
            let picked = palette.read(cx).chosen().map(|i| i.checked_sub(1));
            let color = match event {
                PaletteEvent::Preview(_) | PaletteEvent::Submit(_) => picked.unwrap_or(before),
                _ => before,
            };
            if let Some(root) = this.vault.clone()
                && let Some(known) = this.vaults.iter_mut().find(|k| k.path == root)
            {
                known.color = color;
            }
            if !matches!(event, PaletteEvent::Preview(_)) {
                this.save_vaults();
                this.palette = None;
                this.focus_pane(this.active, window, cx);
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// The band at the top of the rail: the colour of the open vault, its name in a tooltip, the switcher on a
    /// click. Nothing for a vault without a colour.
    pub(super) fn vault_band(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let t = self.theme;
        let known = self.known_vault()?;
        let color = known.color?;
        let tip = crate::ui::widgets::tip(known.label(), keys::of(cx, &OpenVault), t);
        Some(
            div()
                .id("vault-band")
                .debug_selector(|| "vault-band".into())
                .flex_none()
                .w(t.px(22.))
                .h(t.px(4.))
                .mb_1()
                .rounded(t.small())
                .bg(highlight_tint(color))
                .cursor_pointer()
                .tooltip(tip)
                .on_click(cx.listener(|this, _, window, cx| this.open_vaults(window, cx))),
        )
    }

    /// The vaults of the list whose folder is there, but the lost one: the welcome page offers them.
    pub(super) fn other_vaults(&self) -> Vec<Known> {
        self.vaults.iter().filter(|k| Some(&k.path) != self.lost.as_ref() && k.path.is_dir()).cloned().collect()
    }
}
