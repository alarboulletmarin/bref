//! Git sync: committing, fetching, merging and pushing the vault, connecting and cloning.

use super::*;

impl Shell {
    /// Syncs the vault in one action: commit everything, fetch, merge, push. The
    /// network works in a background task, input stays free; a vault with no linked repository
    /// first asks for the repository address.
    pub(super) fn git_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else { return };
        if self.syncing {
            return;
        }
        self.flush(cx);
        match git::linked(&root) {
            Ok(true) => {}
            Ok(false) => {
                let label = tr("Address of an empty git repository", "Adresse d'un dépôt git vide");
                return self.ask_text(label, window, cx, |this, url, _, cx| this.git_connect(url, cx));
            }
            Err(e) => return self.sync_failed(&e, cx),
        }
        if let Err(e) = git::save(&root) {
            return self.sync_failed(&e, cx);
        }
        self.syncing = true;
        self.say(Tone::Busy, tr("Syncing the vault…", "Synchronisation du coffre…"), cx);
        let (date, host) = (date_name(today()), git::host());
        cx.spawn(async move |this, cx| {
            let (mut received, mut conflicts, mut sent) = (0, 0, 0);
            // Rejected because another machine pushed in the meantime: fetch again,
            // three times at most.
            let busy = tr("the server keeps receiving from another machine: try again.", "le serveur reçoit sans cesse d'une autre machine : réessaie.");
            let mut outcome = Err(git::Fail::Told(busy.to_string()));
            for _ in 0..3 {
                let fetch_root = root.clone();
                if let Err(e) = cx.background_executor().spawn(async move { git::fetch(&fetch_root) }).await {
                    outcome = Err(e);
                    break;
                }
                // The local part runs in one go on this thread: nothing can slip in
                // between saving what was just typed and the merge, so nothing
                // typed is lost and nothing received is overwritten.
                // ponytail: the interface is frozen during the merge (a few tenths of a
                // second); move it to a background task with input held back if a very large
                // vault makes it noticeable.
                let merged = this.update(cx, |this, cx| {
                    this.flush(cx);
                    let merged = git::merge(&root, &date, &host);
                    this.rescan(&root, cx);
                    merged
                });
                let merged = match merged {
                    Ok(Ok(merged)) => merged,
                    Ok(Err(e)) => {
                        outcome = Err(e);
                        break;
                    }
                    Err(_) => return,
                };
                (received, conflicts, sent) = (received + merged.received, conflicts + merged.conflicts.len(), merged.sent);
                if !merged.ahead {
                    outcome = Ok(());
                    break;
                }
                let push_root = root.clone();
                match cx.background_executor().spawn(async move { git::push(&push_root) }).await {
                    Ok(git::Pushed::Done) => {
                        outcome = Ok(());
                        break;
                    }
                    Ok(git::Pushed::Rejected) => {}
                    Err(e) => {
                        outcome = Err(e);
                        break;
                    }
                }
            }
            this.update(cx, |this, cx| {
                this.syncing = false;
                match outcome {
                    Err(e) => this.sync_failed(&e, cx),
                    Ok(()) if sent + received + conflicts == 0 => this.say(Tone::Done, tr("Already up to date", "Déjà à jour"), cx),
                    Ok(()) => {
                        let mut told = format!(
                            "{} {sent} {}, {received} {}",
                            tr("Synced:", "Synchronisé :"),
                            if sent > 1 { tr("sent", "envoyés") } else { tr("sent", "envoyé") },
                            if received > 1 { tr("received", "reçus") } else { tr("received", "reçu") },
                        );
                        if conflicts > 0 {
                            let kept = if conflicts > 1 {
                                tr("conflicts, both versions kept (see « (conflict …) »)", "conflits, les deux versions gardées (voir « (conflict …) »)")
                            } else {
                                tr("conflict, both versions kept (see « (conflict …) »)", "conflit, les deux versions gardées (voir « (conflict …) »)")
                            };
                            told.push_str(&format!(" · {conflicts} {kept}"));
                        }
                        this.say(Tone::Done, told, cx)
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn sync_failed(&mut self, why: &git::Fail, cx: &mut Context<Self>) {
        self.syncing = false;
        self.say(Tone::Failed, format!("{} : {}", tr("Sync failed", "Synchronisation impossible"), why.text()), cx);
    }

    /// Links the vault to an empty remote repository, and pushes what it contains.
    pub(super) fn git_connect(&mut self, url: String, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else { return };
        if self.syncing {
            return;
        }
        self.flush(cx);
        self.syncing = true;
        self.say(Tone::Busy, tr("Connecting the vault…", "Liaison du coffre…"), cx);
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { git::connect(&root, &url) }).await;
            this.update(cx, |this, cx| {
                this.syncing = false;
                match done {
                    Ok(()) => this.say(Tone::Done, tr("Vault connected: it syncs with this repository from now on", "Coffre relié : il se synchronise désormais avec ce dépôt"), cx),
                    Err(e) => this.say(Tone::Failed, format!("{} : {}", tr("Not connected", "Liaison impossible"), e.text()), cx),
                }
            })
            .ok();
        })
        .detach();
    }

    /// "Clone a vault": the repository address, then the folder to put it in.
    pub(super) fn ask_clone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = tr("Address of the git repository to clone", "Adresse du dépôt git à cloner");
        self.ask_text(label, window, cx, |_, url, window, cx| {
            let paths = cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(tr("Clone into this folder", "Cloner dans ce dossier").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(Ok(Some(paths))) = paths.await
                    && let Some(parent) = paths.into_iter().next()
                {
                    this.update_in(cx, |this, window, cx| this.clone_to(url, parent, window, cx)).ok();
                }
            })
            .detach();
        });
    }

    /// Clones the repository into a new folder of `parent`, which becomes the open vault.
    pub(super) fn clone_to(&mut self, url: String, parent: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let into = parent.join(git::folder_of(&url));
        if into.exists() {
            let taken = tr("Not cloned: this folder already exists:", "Clonage impossible : ce dossier existe déjà :");
            return self.say(Tone::Failed, format!("{taken} {}", into.display()), cx);
        }
        self.say(Tone::Busy, tr("Cloning the vault…", "Clonage du coffre…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let target = into.clone();
            let done = cx.background_executor().spawn(async move { git::clone(&url, &target) }).await;
            this.update_in(cx, |this, window, cx| match done {
                Ok(()) => {
                    this.set_vault(into, window, cx);
                    this.say(Tone::Done, tr("Vault cloned", "Coffre cloné"), cx)
                }
                Err(e) => this.say(Tone::Failed, format!("{} : {}", tr("Not cloned", "Clonage impossible"), e.text()), cx),
            })
            .ok();
        })
        .detach();
    }
}
