//! New versions: the daily check, the banner, installing.

use super::*;

impl Shell {
    /// Cherche en tâche de fond une version plus récente (au plus une requête par jour).
    /// Les tests ne sortent pas sur le réseau.
    pub(super) fn check_updates(&mut self, cx: &mut Context<Self>) {
        if !self.prefs.updates || cfg!(test) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { update::check() }).await;
            this.update(cx, |this, cx| {
                if this.prefs.updates && found.is_some() {
                    this.update = found;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Contrôle demandé depuis la palette : interroge GitHub tout de suite, même si la recherche
    /// quotidienne est coupée, et dit ce qu'il en est. Les tests ne sortent pas sur le réseau.
    pub(super) fn check_now(&mut self, cx: &mut Context<Self>) {
        self.say(Tone::Busy, tr("Checking for updates…", "Recherche d'une mise à jour…"), cx);
        if cfg!(test) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let latest = cx.background_executor().spawn(async move { update::check_now() }).await;
            this.update(cx, |this, cx| this.checked(latest, cx)).ok();
        })
        .detach();
    }

    /// Suite du contrôle : la bannière si `latest` est plus récente, sinon un mot.
    pub(super) fn checked(&mut self, latest: Option<update::Release>, cx: &mut Context<Self>) {
        match latest {
            None => self.say(Tone::Failed, tr("No answer from GitHub: check the connection", "GitHub ne répond pas : vérifier la connexion"), cx),
            Some(release) if update::is_new(&release) => {
                self.toasts.retain(|t| t.tone != Tone::Busy);
                self.update = Some(release);
                cx.notify();
            }
            Some(_) => self.say(Tone::Done, format!("{} ({})", tr("Bref is up to date", "Bref est à jour"), env!("CARGO_PKG_VERSION")), cx),
        }
    }

    /// Télécharge et installe la version trouvée, puis quitte : le nouveau programme se
    /// relance de lui-même. Un échec laisse l'app telle qu'elle est, avec son message.
    pub(super) fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some((url, sha256)) = self.update.as_ref().and_then(|r| Some((r.asset.clone()?, r.sha256.clone()))) else {
            return;
        };
        if self.updating {
            return;
        }
        self.updating = true;
        self.calm();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { update::install(&url, sha256.as_deref()) }).await;
            this.update(cx, |this, cx| match result {
                // La sortie enregistre la note en cours (`on_app_quit`).
                Ok(()) => cx.quit(),
                Err(e) => {
                    this.updating = false;
                    this.say(Tone::Failed, format!("{} : {e}", tr("Update failed", "Mise à jour impossible")), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Active ou coupe la recherche de nouvelle version, et retient le choix.
    pub(super) fn toggle_updates(&mut self, cx: &mut Context<Self>) {
        self.prefs.updates = !self.prefs.updates;
        vault::save_settings(&self.prefs.to_text());
        self.update = None;
        self.check_updates(cx);
        cx.notify();
    }
}

impl Shell {
    /// La bannière d'une nouvelle version : l'installer, ou la page où la télécharger.
    pub(super) fn render_update_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.theme;
        self.update.clone().map(|release| {
            let banner = status_card(t).max_w_full();
            if self.updating {
                return banner.pr_3().child(tr("Updating, Bref restarts…", "Mise à jour, Bref redémarre…"));
            }
            // « Mettre à jour » quand l'app sait le faire seule, sinon la page de téléchargement.
            let (action, install) = match release.asset {
                Some(_) => (tr("Update", "Mettre à jour"), true),
                None => (tr("Download", "Télécharger"), false),
            };
            banner
                .child(format!("{} {}", tr("New version:", "Nouvelle version :"), release.version))
                .child(
                    div()
                        .cursor_pointer()
                        .text_color(t.accent)
                        .hover(|s| s.underline())
                        .child(action)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                if install {
                                    this.install_update(cx)
                                } else {
                                    cx.open_url(&update::releases_url())
                                }
                            }),
                        ),
                )
                .child(
                    svg()
                        .path("close.svg")
                        .size(px(14.))
                        .flex_none()
                        .text_color(t.soft)
                        .cursor_pointer()
                        .hover(|s| s.text_color(t.text))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.update = None;
                                cx.notify();
                            }),
                        ),
                )
        })
    }
}
