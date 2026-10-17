//! Typing, lists, autosave, the palette, recent notes, update checks, wikilinks.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    cx.simulate_input("# Test");
    // Bas de page : le compteur suit la frappe, puis ne compte que la sélection.
    let counts = |cx: &mut gpui::VisualTestContext| shell.update(cx, |s, cx| s.editor.update(cx, |e, _| e.counts()));
    assert_eq!(counts(cx), "1 mot · 6 caractères");
    cx.simulate_keystrokes("shift-left shift-left");
    assert_eq!(counts(cx), "Sélection : 1 mot · 2 caractères");
    cx.simulate_keystrokes("right");
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

    // Récents : la dernière note ouverte d'abord, dans l'app comme dans la config.
    let recent = shell.read_with(cx, |s, _| s.recent.clone());
    assert_eq!(recent, [root.join("Courses.md"), root.join("Test.md")]);
    assert_eq!(vault::load_config(), (Some(root.clone()), recent));
    let names = shell.read_with(cx, |s, _| {
        s.by_recency().iter().map(|n| n.name.clone()).collect::<Vec<_>>()
    });
    assert_eq!(names, ["Courses", "Test", "Plan"]);

    // Palette : le texte des notes se cherche aussi (« deux » n'est dans aucun nom),
    // et tous les mots de la requête doivent s'y trouver.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("deux");
    cx.simulate_keystrokes("enter");
    assert!(text(cx).starts_with("# Test\n1. un\n2. deux"));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("lait maison");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "# Courses\n\n- lait #maison\n");

    // Mises à jour : la palette coupe la recherche (et la bannière), puis la rétablit.
    shell.update(cx, |s, _| s.update = Some(update::Release { version: "9.9.9".into(), ..Default::default() }));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("mises à jour");
    cx.simulate_keystrokes("down enter");
    assert!(shell.read_with(cx, |s, _| !s.prefs.updates && s.update.is_none()));
    assert!(vault::load_settings().ends_with("updates=off\n"));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("mises à jour");
    cx.simulate_keystrokes("down enter");
    assert!(shell.read_with(cx, |s, _| s.prefs.updates));
    assert!(!vault::load_settings().contains("updates"));

    // Installer la nouvelle version : la palette le propose quand l'app sait le faire seule.
    // Les tests n'installent rien : l'échec laisse l'app en place, avec son message.
    let release = update::Release { version: "9.9.9".into(), asset: Some("file:///absent".into()), ..Default::default() };
    shell.update(cx, |s, _| s.update = Some(release));
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("mettre à jour");
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| !s.updating && s.told(Tone::Failed, "Mise à jour impossible")));
    shell.update(cx, |s, _| (s.update, s.toasts) = (None, Vec::new()));

    // Contrôle à la main : la palette le lance, puis la réponse dit que Bref est à jour,
    // que GitHub ne répond pas, ou montre la bannière.
    cx.simulate_keystrokes("secondary-p");
    cx.simulate_input("maintenant");
    cx.simulate_keystrokes("down enter");
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Busy, "Recherche")));
    let version = |v: &str| Some(update::Release { version: v.into(), ..Default::default() });
    shell.update(cx, |s, cx| s.checked(version(env!("CARGO_PKG_VERSION")), cx));
    assert!(shell.read_with(cx, |s, _| s.update.is_none() && s.told(Tone::Done, "à jour") && !s.told(Tone::Busy, "")));
    shell.update(cx, |s, cx| s.checked(None, cx));
    assert!(shell.read_with(cx, |s, _| s.told(Tone::Failed, "GitHub")));
    shell.update(cx, |s, cx| s.checked(version("9.9.9"), cx));
    assert!(shell.read_with(cx, |s, _| !s.told(Tone::Busy, "") && s.update.as_ref().is_some_and(|r| r.version == "9.9.9")));
    shell.update(cx, |s, _| (s.update, s.toasts) = (None, Vec::new()));

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
}
