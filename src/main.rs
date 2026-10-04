// Pas de console derrière la fenêtre sous Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod editor;
mod markdown;
mod palette;
mod vault;

use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use gpui::{
    App, Application, AssetSource, Bounds, BoxShadow, ClipboardItem, Context, CursorStyle, Decorations, Entity,
    FocusHandle, Focusable, Hsla, KeyBinding, MouseButton, MouseMoveEvent, PathPromptOptions,
    Pixels, Point, ResizeEdge, SharedString, Size, TitlebarOptions, Window, WindowAppearance, WindowBounds,
    WindowBackgroundAppearance, WindowDecorations, WindowOptions, actions, div, hsla, point,
    prelude::*, px, rgb, rgba, size, svg,
};

use editor::{Editor, EditorEvent};
use markdown::Link;
use palette::{Entry, Palette, PaletteEvent};
use vault::Note;

/// Touche des raccourcis, telle qu'affichée : Cmd sur macOS, Ctrl ailleurs.
pub const MOD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

/// Texte d'interface : français si la langue du système l'est, anglais sinon.
// ponytail: langue lue dans LC_ALL / LC_MESSAGES / LANG, absentes sous Windows
// (donc anglais) ; interroger l'API de locale Windows si le besoin apparaît.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    static FRENCH: OnceLock<bool> = OnceLock::new();
    let french = *FRENCH.get_or_init(|| {
        // Les tests tournent en français pour rester déterministes.
        cfg!(test)
            || ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
                .is_some_and(|lang| lang.starts_with("fr"))
    });
    if french { fr } else { en }
}

/// Polices (texte, code) retenues au démarrage parmi celles installées.
static FONTS: OnceLock<(&str, &str)> = OnceLock::new();

pub fn sans() -> &'static str {
    FONTS.get().map_or("Cantarell", |f| f.0)
}

pub fn mono() -> &'static str {
    FONTS.get().map_or("DejaVu Sans Mono", |f| f.1)
}

/// Marge transparente autour de la fenêtre quand l'app dessine ses décorations.
const SHADOW: Pixels = px(10.);

/// Icônes embarquées dans le binaire (traits de 1,2 px sur une grille de 16 px).
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let shapes = match path {
            "minimize.svg" => r#"<path d="M4 8H12"/>"#,
            "maximize.svg" => r#"<path d="M4.5 4.5H11.5V11.5H4.5Z"/>"#,
            "restore.svg" => r#"<path d="M3.5 6.5H9.5V12.5H3.5Z"/><path d="M10 8.5H12.5V3.5H7.5V6"/>"#,
            "close.svg" => r#"<path d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5"/>"#,
            "copy.svg" => {
                r#"<rect x="6" y="6" width="7.5" height="7.5" rx="1.5"/><path d="M10 4A1.5 1.5 0 0 0 8.5 2.5H4A1.5 1.5 0 0 0 2.5 4V8.5A1.5 1.5 0 0 0 4 10"/>"#
            }
            "check.svg" => r#"<path d="M4 8.5L7 11L12 5"/>"#,
            _ => return Ok(None),
        };
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="black" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">{shapes}</svg>"#
        );
        Ok(Some(Cow::Owned(svg.into_bytes())))
    }

    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

actions!(app, [OpenPalette, NewNote, OpenVault, CopyAll, ToggleHelp, CloseHelp, Quit]);

#[derive(Clone, Copy)]
pub struct Theme {
    pub bg: Hsla,
    pub text: Hsla,
    pub dim: Hsla,
    pub accent: Hsla,
    pub selection: Hsla,
    pub code_bg: Hsla,
    pub panel: Hsla,
    pub border: Hsla,
}

impl Theme {
    fn of(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self {
                bg: rgb(0x1b1a19).into(),
                text: rgb(0xe4e0da).into(),
                dim: rgb(0x77716a).into(),
                accent: rgb(0xe0926b).into(),
                selection: rgba(0xe0926b40).into(),
                code_bg: rgb(0x262423).into(),
                panel: rgb(0x242221).into(),
                border: rgb(0x3a3735).into(),
            },
            _ => Self {
                bg: rgb(0xfbfaf8).into(),
                text: rgb(0x26231f).into(),
                dim: rgb(0xa8a29a).into(),
                accent: rgb(0xb4532a).into(),
                selection: rgba(0xb4532a33).into(),
                code_bg: rgb(0xf1eeea).into(),
                panel: rgb(0xffffff).into(),
                border: rgb(0xe2ddd6).into(),
            },
        }
    }
}

struct Shell {
    focus: FocusHandle,
    editor: Entity<Editor>,
    palette: Option<Entity<Palette>>,
    vault: Option<PathBuf>,
    notes: Vec<Note>,
    /// Fichier de la note ouverte ; `None` tant qu'une nouvelle note n'est pas enregistrée.
    path: Option<PathBuf>,
    /// Le nom du fichier suit le titre (première ligne) de la note.
    synced: bool,
    dirty: bool,
    save_gen: usize,
    error: Option<String>,
    title: String,
    theme: Theme,
    /// Bord de fenêtre survolé (redimensionnement sans décorations système).
    edge: Option<ResizeEdge>,
    copied: bool,
    help: bool,
}

impl Shell {
    fn new(
        vault: Option<PathBuf>,
        last: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = Theme::of(window.appearance());
        let editor = cx.new(|cx| Editor::new(theme, cx));
        cx.subscribe_in(&editor, window, Self::on_editor_event).detach();
        cx.observe_window_appearance(window, |this, window, cx| {
            this.theme = Theme::of(window.appearance());
            this.editor.update(cx, |e, cx| e.set_theme(this.theme, cx));
            cx.notify();
        })
        .detach();
        cx.on_app_quit(|this, cx| {
            this.flush(cx);
            async {}
        })
        .detach();

        let mut this = Self {
            focus: cx.focus_handle(),
            editor,
            palette: None,
            vault: None,
            notes: Vec::new(),
            path: None,
            synced: true,
            dirty: false,
            save_gen: 0,
            error: None,
            title: String::new(),
            theme,
            edge: None,
            copied: false,
            help: false,
        };
        match vault {
            Some(root) => {
                this.set_vault(root, window, cx);
                if let Some(last) = last.filter(|p| p.is_file()) {
                    this.open_note(&last, cx);
                }
            }
            None => window.focus(&this.focus),
        }
        this
    }

    fn set_vault(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(cx);
        self.vault = Some(root.clone());
        self.notes.clear();
        self.new_note(String::new(), cx);
        vault::save_config(&root, None);
        window.focus(&self.editor.focus_handle(cx));
        // L'index (noms, tags) se construit hors du thread UI.
        cx.spawn(async move |this, cx| {
            let scan_root = root.clone();
            let notes = cx
                .background_executor()
                .spawn(async move { vault::scan(&scan_root) })
                .await;
            this.update(cx, |this, cx| {
                if this.vault.as_ref() == Some(&root) {
                    this.notes = notes;
                    this.push_names(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn choose_vault(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Use this vault", "Choisir ce coffre").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(root) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.set_vault(root, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Copie toute la note dans le presse-papiers.
    fn copy_all(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Affiche ou masque le panneau des raccourcis ; le focus suit.
    fn set_help(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.help = open;
        if open || self.vault.is_none() {
            window.focus(&self.focus);
        } else {
            window.focus(&self.editor.focus_handle(cx));
        }
        cx.notify();
    }

    fn push_names(&mut self, cx: &mut Context<Self>) {
        let names = self.notes.iter().map(|n| n.name.clone()).collect();
        self.editor.update(cx, |e, _| e.set_notes(names));
    }

    fn open_note(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.flush(cx);
        match fs::read_to_string(path) {
            Ok(text) => {
                self.synced = vault::stem(path) == vault::title_of(&text);
                self.path = Some(path.to_path_buf());
                self.error = None;
                self.editor.update(cx, |e, cx| e.load(text, 0, cx));
                if let Some(root) = &self.vault {
                    vault::save_config(root, Some(path));
                }
            }
            Err(e) => {
                let what = tr("Cannot open", "Impossible d'ouvrir");
                self.error = Some(format!("{what} {} : {e}", path.display()))
            }
        }
        cx.notify();
    }

    fn new_note(&mut self, text: String, cx: &mut Context<Self>) {
        self.flush(cx);
        self.path = None;
        self.synced = true;
        self.dirty = !text.is_empty();
        let cursor = text.len();
        self.editor.update(cx, |e, cx| e.load(text, cursor, cx));
        self.flush(cx);
        cx.notify();
    }

    /// Écrit la note sur disque si elle a changé.
    // ponytail: écriture synchrone sur le thread UI (quelques Ko, < 1 ms) ;
    // passer en tâche de fond si les notes deviennent très grosses.
    fn flush(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.vault.clone() else {
            return;
        };
        if !self.dirty {
            return;
        }
        let content = self.editor.read(cx).text().to_string();
        if self.path.is_none() && content.trim().is_empty() {
            self.dirty = false;
            return;
        }
        match vault::save(&root, self.path.as_deref(), self.synced, &content) {
            Ok(path) => {
                self.dirty = false;
                self.error = None;
                self.notes
                    .retain(|n| Some(&n.path) != self.path.as_ref() && n.path != path);
                self.notes.insert(
                    0,
                    Note {
                        name: vault::stem(&path),
                        tags: markdown::tags(&content),
                        mtime: SystemTime::now(),
                        path: path.clone(),
                    },
                );
                if self.path.as_ref() != Some(&path) {
                    vault::save_config(&root, Some(&path));
                    self.path = Some(path);
                    self.push_names(cx);
                }
            }
            Err(e) => {
                self.error = Some(format!("{} : {e}", tr("Note not saved", "Note non enregistrée")))
            }
        }
        cx.notify();
    }

    fn on_editor_event(
        &mut self,
        _: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::Changed => {
                self.dirty = true;
                self.save_gen += 1;
                let generation = self.save_gen;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(400)).await;
                    this.update(cx, |this, cx| {
                        if this.save_gen == generation {
                            this.flush(cx);
                        }
                    })
                    .ok();
                })
                .detach();
            }
            EditorEvent::Open(Link::Url(url)) => cx.open_url(url),
            EditorEvent::Open(Link::Tag(tag)) => self.open_palette(&format!("#{tag}"), window, cx),
            EditorEvent::Open(Link::Wiki(name)) => self.open_wiki(name, cx),
        }
    }

    fn open_wiki(&mut self, name: &str, cx: &mut Context<Self>) {
        let wanted = name.to_lowercase();
        match self.notes.iter().find(|n| n.name.to_lowercase() == wanted) {
            Some(note) => self.open_note(&note.path.clone(), cx),
            None => self.new_note(format!("# {name}\n\n"), cx),
        }
    }

    fn open_palette(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        self.flush(cx);
        let entries = self
            .notes
            .iter()
            .map(|n| Entry {
                name: n.name.clone(),
                path: n.path.clone(),
                tags: n.tags.clone(),
            })
            .collect();
        let theme = self.theme;
        let palette = cx.new(|cx| Palette::new(entries, query, theme, cx));
        cx.subscribe_in(&palette, window, |this, _, event, window, cx| {
            this.palette = None;
            window.focus(&this.editor.focus_handle(cx));
            match event {
                PaletteEvent::Open(path) => this.open_note(path, cx),
                PaletteEvent::Create(name) => this.open_wiki(name, cx),
                PaletteEvent::ChangeVault => this.choose_vault(window, cx),
                PaletteEvent::Help => this.set_help(true, window, cx),
                PaletteEvent::Dismiss => {}
            }
            cx.notify();
        })
        .detach();
        window.focus(&palette.focus_handle(cx));
        self.palette = Some(palette);
        cx.notify();
    }
}

/// Contenu du panneau d'aide : (titre de section, [(touches, effet)]).
fn help_sections() -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    let m = |key: &str| format!("{MOD}+{key}");
    let word = if cfg!(target_os = "macos") { "Alt" } else { "Ctrl" };
    vec![
        (
            "Notes",
            vec![
                (m("P"), tr("Find, create, filter by #tag", "Chercher, créer, filtrer par #tag")),
                (m("N"), tr("New note", "Nouvelle note")),
                (m("O"), tr("Change vault", "Changer de coffre")),
                (m("Shift+C"), tr("Copy the whole note", "Copier toute la note")),
                (format!("{MOD}+{}", tr("click", "clic")), tr("Open a [[link]], #tag or URL", "Ouvrir un [[lien]], #tag ou URL")),
                ("F1".into(), tr("This help", "Cette aide")),
            ],
        ),
        (
            tr("Typing", "À la frappe"),
            vec![
                ("- ".into(), tr("Bullet list", "Liste à puces")),
                ("1. ".into(), tr("Numbered list", "Liste numérotée")),
                ("[] ".into(), tr("Task", "Tâche à cocher")),
                ("# ## ###".into(), tr("Headings", "Titres")),
                ("> ".into(), tr("Quote", "Citation")),
                ("```".into(), tr("Code block", "Bloc de code")),
                ("---".into(), tr("Divider", "Séparateur")),
                ("[[".into(), tr("Link to a note", "Lien vers une note")),
            ],
        ),
        (
            tr("Editing", "Édition"),
            vec![
                (tr("Enter", "Entrée").into(), tr("Continue the list, or leave it on an empty item", "Continuer la liste, ou en sortir sur un item vide")),
                ("Tab / Shift+Tab".into(), tr("Indent / outdent", "Indenter / désindenter")),
                (m(tr("Enter", "Entrée")), tr("Check / uncheck a task", "Cocher / décocher une tâche")),
                (format!("{} / {}", m("B"), m("I")), tr("Bold / italic", "Gras / italique")),
                (format!("{} / {}", m("Z"), m("Shift+Z")), tr("Undo / redo", "Annuler / rétablir")),
                (format!("{word}+{}", tr("Left / Right", "Gauche / Droite")), tr("Move by word", "Se déplacer par mot")),
            ],
        ),
    ]
}

impl Shell {
    fn render_help(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let sections = help_sections().into_iter().map(|(title, rows)| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().mb_1().text_color(t.accent).text_size(px(12.)).child(title))
                .children(rows.into_iter().map(|(keys, effect)| {
                    div()
                        .flex()
                        .gap_3()
                        .child(div().w(px(176.)).flex_none().font_family(mono()).text_size(px(12.)).child(keys))
                        .child(div().text_color(t.dim).child(effect))
                }))
        });
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| this.set_help(false, window, cx)),
            )
            .child(
                div()
                    .w(px(560.))
                    .max_w_full()
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .bg(t.panel)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(10.))
                    .shadow_lg()
                    .text_size(px(13.))
                    .children(sections),
            )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        let title = match (&self.path, &self.vault) {
            (Some(p), _) => vault::stem(p),
            (None, Some(_)) => tr("New note", "Nouvelle note").to_string(),
            (None, None) => "encre".to_string(),
        };
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }

        // Sous Linux l'app fournit barre de titre, coins arrondis et ombre ; sous
        // macOS et Windows, gpui annonce `Server` et le système s'en charge.
        let tiling = match window.window_decorations() {
            Decorations::Client { tiling } => Some(tiling),
            Decorations::Server => None,
        };
        let client = tiling.is_some();
        let framed = tiling.is_some_and(|t| !t.is_tiled())
            && !window.is_maximized()
            && !window.is_fullscreen();
        window.set_client_inset(if framed { SHADOW } else { px(0.) });

        // Bouton icône : cercle visible au survol, comme les contrôles de fenêtre de Zed.
        let icon_button = |id: &'static str, icon: &'static str| {
            div()
                .id(id)
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .hover(|s| s.bg(t.border))
                .active(|s| s.bg(t.border))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(svg().path(icon).size(px(16.)).flex_none().text_color(t.text))
        };
        let controls = window.window_controls();
        let header = div()
            .flex_none()
            .h(px(34.))
            .flex()
            .items_center()
            .text_size(px(12.5))
            .text_color(t.dim)
            .on_mouse_down(MouseButton::Left, |e, window, _| {
                if e.click_count == 2 {
                    window.zoom_window()
                } else {
                    window.start_window_move()
                }
            })
            .on_mouse_down(MouseButton::Right, |e, window, _| window.show_window_menu(e.position))
            .child(div().flex_1().px_3().truncate().child(self.title.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .when(controls.minimize, |d| {
                        d.child(
                            icon_button("minimize", "minimize.svg")
                                .on_click(|_, window, _| window.minimize_window()),
                        )
                    })
                    .when(controls.maximize, |d| {
                        let icon = if window.is_maximized() { "restore.svg" } else { "maximize.svg" };
                        d.child(
                            icon_button("maximize", icon).on_click(|_, window, _| window.zoom_window()),
                        )
                    })
                    .child(
                        icon_button("close", "close.svg")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            );

        let body = div().flex_1().min_h_0().relative();
        let body = if self.vault.is_none() {
            body.flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .child(
                    div()
                        .text_size(px(26.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child("encre"),
                )
                .child(
                    div()
                        .text_color(t.dim)
                        .child(tr(
                            "Your notes live in a folder of Markdown files: the vault.",
                            "Tes notes vivent dans un dossier de fichiers Markdown : le coffre.",
                        )),
                )
                .child(
                    div()
                        .mt_4()
                        .px_4()
                        .py_2()
                        .rounded(px(8.))
                        .bg(t.accent)
                        .text_color(t.bg)
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.9))
                        .child(tr("Open or create a vault", "Ouvrir ou créer un coffre"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| this.choose_vault(window, cx)),
                        ),
                )
                .child(div().text_size(px(12.)).text_color(t.dim).child(format!(
                    "{MOD}+O · {}",
                    tr(
                        "you can create a new folder from the file dialog",
                        "un nouveau dossier peut être créé depuis la fenêtre de sélection",
                    )
                )))
        } else {
            // Copie de toute la note : simple icône flottante, hors de la barre de titre.
            let copy = icon_button("copy-all", if self.copied { "check.svg" } else { "copy.svg" })
                .absolute()
                .bottom_4()
                .right_4()
                .size(px(30.))
                .rounded(px(8.))
                .occlude()
                .on_click(cx.listener(|this, _, _, cx| this.copy_all(cx)));
            body.child(self.editor.clone())
                .child(copy)
                .children(self.palette.clone())
                .children(self.error.clone().map(|message| {
                    div()
                        .absolute()
                        .bottom_3()
                        .left_3()
                        .px_3()
                        .py_1p5()
                        .rounded(px(6.))
                        .bg(rgb(0xb42318))
                        .text_color(rgb(0xffffff))
                        .text_size(px(13.))
                        .child(message)
                }))
        };

        let content = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.bg)
            .text_color(t.text)
            .font_family(sans())
            .when(framed, |d| {
                d.rounded(px(12.)).border_1().border_color(t.border).shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.35),
                    offset: point(px(0.), px(2.)),
                    blur_radius: SHADOW / 2.,
                    spread_radius: px(0.),
                }])
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OpenPalette, window, cx| {
                this.open_palette("", window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewNote, _, cx| {
                if this.vault.is_some() {
                    this.new_note(String::new(), cx)
                }
            }))
            .on_action(cx.listener(|this, _: &OpenVault, window, cx| this.choose_vault(window, cx)))
            .on_action(cx.listener(|this, _: &CopyAll, _, cx| this.copy_all(cx)))
            .on_action(cx.listener(|this, _: &ToggleHelp, window, cx| {
                this.set_help(!this.help, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseHelp, window, cx| {
                if this.help {
                    this.set_help(false, window, cx)
                }
            }))
            .key_context("Shell")
            .when(client, |d| d.child(header))
            .child(body)
            .when(self.help, |d| d.child(self.render_help(cx)));

        // La marge transparente porte l'ombre et sert de poignée de redimensionnement.
        div()
            .size_full()
            .when(framed, |d| {
                d.p(SHADOW)
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                        let edge = resize_edge(e.position, window.viewport_size());
                        if edge != this.edge {
                            this.edge = edge;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, _| {
                            if let Some(edge) = this.edge {
                                window.start_window_resize(edge)
                            }
                        }),
                    )
                    .when_some(self.edge, |d, edge| {
                        d.cursor(match edge {
                            ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                            ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
                            ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                CursorStyle::ResizeUpLeftDownRight
                            }
                            ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                CursorStyle::ResizeUpRightDownLeft
                            }
                        })
                    })
            })
            .child(content)
    }
}

/// Bord ou coin de fenêtre sous le pointeur, dans une marge de quelques pixels.
fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>) -> Option<ResizeEdge> {
    let m = SHADOW;
    let (left, right) = (pos.x < m, pos.x > size.width - m);
    let (top, bottom) = (pos.y < m, pos.y > size.height - m);
    Some(match (top, bottom, left, right) {
        (true, _, true, _) => ResizeEdge::TopLeft,
        (true, _, _, true) => ResizeEdge::TopRight,
        (_, true, true, _) => ResizeEdge::BottomLeft,
        (_, true, _, true) => ResizeEdge::BottomRight,
        (true, ..) => ResizeEdge::Top,
        (_, true, ..) => ResizeEdge::Bottom,
        (_, _, true, _) => ResizeEdge::Left,
        (_, _, _, true) => ResizeEdge::Right,
        _ => return None,
    })
}

fn bind_keys(cx: &mut App) {
    use editor::*;
    use palette::{Confirm, DeleteChar, Dismiss, Next, Prev};
    let e = Some("Editor");
    let p = Some("Palette");
    // `secondary` = Cmd sur macOS, Ctrl ailleurs ; les mots se parcourent avec Alt sur macOS.
    let word = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
    cx.bind_keys([
        KeyBinding::new("secondary-p", OpenPalette, None),
        KeyBinding::new("secondary-n", NewNote, None),
        KeyBinding::new("secondary-o", OpenVault, None),
        KeyBinding::new("secondary-shift-c", CopyAll, None),
        KeyBinding::new("f1", ToggleHelp, None),
        KeyBinding::new("secondary-/", ToggleHelp, None),
        KeyBinding::new("escape", CloseHelp, Some("Shell")),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("up", Prev, p),
        KeyBinding::new("down", Next, p),
        KeyBinding::new("enter", Confirm, p),
        KeyBinding::new("escape", Dismiss, p),
        KeyBinding::new("backspace", DeleteChar, p),
        KeyBinding::new("backspace", Backspace, e),
        KeyBinding::new("delete", Delete, e),
        KeyBinding::new(&format!("{word}-backspace"), DeleteWordLeft, e),
        KeyBinding::new(&format!("{word}-delete"), DeleteWordRight, e),
        KeyBinding::new("left", Left, e),
        KeyBinding::new("right", Right, e),
        KeyBinding::new("up", Up, e),
        KeyBinding::new("down", Down, e),
        KeyBinding::new(&format!("{word}-left"), WordLeft, e),
        KeyBinding::new(&format!("{word}-right"), WordRight, e),
        KeyBinding::new("home", Home, e),
        KeyBinding::new("end", End, e),
        KeyBinding::new("secondary-home", DocStart, e),
        KeyBinding::new("secondary-end", DocEnd, e),
        KeyBinding::new("pageup", PageUp, e),
        KeyBinding::new("pagedown", PageDown, e),
        KeyBinding::new("shift-left", SelectLeft, e),
        KeyBinding::new("shift-right", SelectRight, e),
        KeyBinding::new("shift-up", SelectUp, e),
        KeyBinding::new("shift-down", SelectDown, e),
        KeyBinding::new(&format!("{word}-shift-left"), SelectWordLeft, e),
        KeyBinding::new(&format!("{word}-shift-right"), SelectWordRight, e),
        KeyBinding::new("shift-home", SelectHome, e),
        KeyBinding::new("shift-end", SelectEnd, e),
        KeyBinding::new("secondary-shift-home", SelectDocStart, e),
        KeyBinding::new("secondary-shift-end", SelectDocEnd, e),
        KeyBinding::new("secondary-a", SelectAll, e),
        KeyBinding::new("enter", Newline, e),
        KeyBinding::new("tab", Indent, e),
        KeyBinding::new("shift-tab", Outdent, e),
        KeyBinding::new("secondary-enter", ToggleTask, e),
        KeyBinding::new("secondary-b", Bold, e),
        KeyBinding::new("secondary-i", Italic, e),
        KeyBinding::new("secondary-c", Copy, e),
        KeyBinding::new("secondary-x", Cut, e),
        KeyBinding::new("secondary-v", Paste, e),
        KeyBinding::new("secondary-z", Undo, e),
        KeyBinding::new("secondary-shift-z", Redo, e),
        KeyBinding::new("secondary-y", Redo, e),
        KeyBinding::new("escape", Cancel, e),
    ]);
}

/// Le pilote Vulkan NVIDIA (paquet `nvidia-utils`) met environ 2 s à renoncer
/// quand aucune carte NVIDIA n'est branchée : on demande au chargeur de l'ignorer.
#[cfg(target_os = "linux")]
fn skip_absent_nvidia_driver() {
    let has_nvidia = fs::read_dir("/sys/bus/pci/devices").is_ok_and(|devices| {
        devices.flatten().any(|d| {
            fs::read_to_string(d.path().join("vendor")).is_ok_and(|v| v.trim() == "0x10de")
        })
    });
    if !has_nvidia && std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_none() {
        // Sûr : appelé au tout début de `main`, avant la création du moindre thread.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_DISABLE", "*nvidia*") };
    }
    // Conventions macOS : Cmd+flèches pour les extrémités de ligne et de document.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-left", Home, e),
        KeyBinding::new("cmd-right", End, e),
        KeyBinding::new("cmd-up", DocStart, e),
        KeyBinding::new("cmd-down", DocEnd, e),
        KeyBinding::new("cmd-shift-left", SelectHome, e),
        KeyBinding::new("cmd-shift-right", SelectEnd, e),
        KeyBinding::new("cmd-shift-up", SelectDocStart, e),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, e),
    ]);
}

fn main() {
    #[cfg(target_os = "linux")]
    skip_absent_nvidia_driver();
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        bind_keys(cx);
        let installed = cx.text_system().all_font_names();
        let pick = |wanted: &[&'static str]| {
            let found = wanted.iter().find(|w| installed.iter().any(|i| i == *w));
            *found.unwrap_or(&wanted[wanted.len() - 1])
        };
        FONTS.get_or_init(|| {
            (
                // Adwaita Sans et Cantarell sont des polices variables : pas de gras avec gpui 0.2.
                pick(&[
                    "Inter",
                    "Noto Sans",
                    "Segoe UI",
                    "Helvetica Neue",
                    "DejaVu Sans",
                    "Cantarell",
                ]),
                pick(&[
                    "JetBrains Mono",
                    "JetBrainsMono Nerd Font",
                    "Adwaita Mono",
                    "Noto Sans Mono",
                    "Menlo",
                    "Consolas",
                    "DejaVu Sans Mono",
                ]),
            )
        });
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx| cx.quit()).detach();

        let (vault, last) = vault::load_config();
        let vault = vault.filter(|v| v.is_dir());
        let bounds = Bounds::centered(None, size(px(860.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("encre".into()),
                    ..Default::default()
                }),
                // Doit correspondre au nom du fichier .desktop pour que le bureau associe l'icône.
                app_id: Some("dev.andrea.Encre".into()),
                // Comme Zed : sous Linux l'app dessine sa barre de titre. Demander
                // `Server` ne marche pas sous GNOME/Wayland, qui n'en fournit pas
                // alors que gpui 0.2 se croit quand même décoré. macOS et Windows
                // ignorent la demande et gardent leur barre native.
                window_decorations: Some(WindowDecorations::Client),
                window_background: WindowBackgroundAppearance::Transparent,
                window_min_size: Some(size(px(360.), px(240.))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Shell::new(vault, last, window, cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    /// `secondary` = Cmd sur macOS, Ctrl ailleurs, comme dans `bind_keys`.
    /// Parcours complet au clavier : saisie, listes, enregistrement, palette, wikilien.
    #[gpui::test]
    fn end_to_end(cx: &mut TestAppContext) {
        let root = std::env::temp_dir().join(format!("encre-e2e-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Courses.md"), "# Courses\n\n- lait #maison\n").unwrap();
        // La config de test ne doit pas toucher celle de l'utilisateur.
        for key in ["XDG_CONFIG_HOME", "HOME", "APPDATA"] {
            unsafe { std::env::set_var(key, root.join(".config")) };
        }

        cx.update(bind_keys);
        let (shell, cx) = cx.add_window_view({
            let root = root.clone();
            |window, cx| Shell::new(Some(root), None, window, cx)
        });
        cx.run_until_parked();
        let text = |cx: &mut gpui::VisualTestContext| {
            shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string())
        };

        cx.simulate_input("# Test");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("1. un");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("deux");
        cx.simulate_keystrokes("enter enter");
        cx.simulate_input("[] tâche");
        cx.simulate_keystrokes("secondary-enter enter");
        cx.simulate_input("suite");
        cx.simulate_keystrokes("tab");
        assert_eq!(
            text(cx),
            "# Test\n1. un\n2. deux\n- [x] tâche\n    - [ ] suite"
        );
        cx.simulate_keystrokes("secondary-z");
        assert_eq!(text(cx), "# Test\n1. un\n2. deux\n- [x] tâche\n- [ ] suite");

        // Enregistrement automatique, nommé d'après le titre.
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(root.join("Test.md")).unwrap(), text(cx));

        // Palette : recherche floue puis ouverture.
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("crs");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "# Courses\n\n- lait #maison\n");

        // Wikilien complété puis nouvelle note créée depuis la palette.
        cx.simulate_keystrokes("secondary-end");
        cx.simulate_input("[[te");
        cx.simulate_keystrokes("enter");
        assert!(text(cx).ends_with("[[Test]]"));
        cx.simulate_keystrokes("secondary-p");
        cx.simulate_input("Idées");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(cx), "# Idées\n\n");
        assert!(root.join("Idées.md").is_file());
        assert!(fs::read_to_string(root.join("Courses.md")).unwrap().ends_with("[[Test]]"));


        // Copie rapide de toute la note.
        cx.simulate_keystrokes("secondary-shift-c");
        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
        assert_eq!(clipboard.as_deref(), Some("# Idées\n\n"));


        // Panneau d'aide : F1 l'ouvre, Échap le ferme et rend la main à l'éditeur.
        cx.simulate_keystrokes("f1");
        assert!(shell.read_with(cx, |s, _| s.help));
        cx.simulate_keystrokes("escape");
        assert!(!shell.read_with(cx, |s, _| s.help));
        cx.simulate_input("ok");
        assert_eq!(text(cx), "# Idées\n\nok");

        fs::remove_dir_all(&root).unwrap();
    }
}
