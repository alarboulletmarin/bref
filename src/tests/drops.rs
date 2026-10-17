//! Files dropped from the system on a note, a folder or the vault.

use super::*;

/// Le glisser-déposer de bout en bout : ce que `drop_paths` fait de ce que la fenêtre reçoit, et ses limites
/// (le câblage des événements du système ne se simule pas : `ExternalPaths` ne se construit pas hors de GPUI).
#[gpui::test]
fn drops_journey(cx: &mut TestAppContext) {
    const PNG: [u8; 69] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f,
        0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f, 0, 0x05, 0xfe, 0x02, 0xfe, 0xa7,
        0x35, 0x81, 0x84, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let base = std::env::temp_dir().join(format!("bref-drop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (root, ext) = (base.join("coffre"), base.join("dehors"));
    fs::create_dir_all(root.join("Dossier")).unwrap();
    fs::create_dir_all(ext.join("Photos/été")).unwrap();
    let carnet = "# Carnet\n\npremier\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```\ncode\n```\n\nfin";
    fs::write(root.join("Carnet.md"), carnet).unwrap();
    fs::write(root.join("Dossier/Autre.md"), "# Autre\n\nici\n").unwrap();
    for (name, bytes) in [("photo.png", PNG.as_slice()), ("rapport.pdf", b"%PDF".as_slice()), ("clip final.mp4", b"mp4".as_slice()), ("inconnu.exe", b"MZ".as_slice()), (".secret", b"x".as_slice()), ("Idées.md", "# Idées\n".as_bytes()), ("Photos/a.png", PNG.as_slice()), ("Photos/été/b.png", PNG.as_slice())] {
        fs::write(ext.join(name), bytes).unwrap();
    }
    let _config = isolated_config(&base);
    cx.update(bind_keys);
    cx.update(init_fonts);
    let (shell, cx) = cx.add_window_view({
        let root = root.clone();
        |window, cx| Shell::new(Some(root), Vec::new(), window, cx)
    });
    cx.run_until_parked();
    let text = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.editor.read(cx).text().to_string());
    let told = |cx: &mut gpui::VisualTestContext, tone, part: &str| shell.read_with(cx, |s, _| s.told(tone, part));
    let drop = |cx: &mut gpui::VisualTestContext, names: &[&str], aim: Aim| {
        let paths = names.iter().map(|n| ext.join(n)).collect();
        shell.update(cx, |s, cx| s.drop_paths(paths, aim, cx));
        cx.run_until_parked();
    };
    let known = |cx: &mut gpui::VisualTestContext, path: PathBuf| shell.read_with(cx, |s, _| s.images.contains(&path));
    // Sur un dossier de l'arbre : les fichiers y sont copiés, l'arbre les montre, un avis le dit.
    drop(cx, &["rapport.pdf", "clip final.mp4"], Aim::Folder(root.join("Dossier")));
    assert!(root.join("Dossier/rapport.pdf").is_file() && root.join("Dossier/clip final.mp4").is_file());
    assert!(known(cx, root.join("Dossier/rapport.pdf")) && known(cx, root.join("Dossier/clip final.mp4")), "les pièces jointes sont dans l'arbre");
    assert!(told(cx, Tone::Done, "Copié 2 fichiers dans Dossier"), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
    assert!(ext.join("rapport.pdf").is_file(), "l'original ne bouge pas");
    // Les mêmes encore : des numéros, rien n'est écrasé. Un fichier caché, un type inconnu, un chemin absent.
    fs::write(root.join("Dossier/rapport.pdf"), "le mien").unwrap();
    drop(cx, &["rapport.pdf", ".secret", "inconnu.exe", "n'existe-pas.png"], Aim::Folder(root.join("Dossier")));
    assert_eq!(fs::read_to_string(root.join("Dossier/rapport.pdf")).unwrap(), "le mien");
    assert!(root.join("Dossier/rapport 2.pdf").is_file() && root.join("Dossier/inconnu.exe").is_file() && !root.join("Dossier/.secret").exists());
    assert!(told(cx, Tone::Info, "2 laissés de côté"), "{:?}", shell.read_with(cx, |s, _| s.toasts.iter().map(|t| t.text.clone()).collect::<Vec<_>>()));
    assert!(!known(cx, root.join("Dossier/inconnu.exe")), "un type inconnu est copié mais pas listé");
    // Rien de copiable : un avis d'échec, et rien ne change. Une liste vide : rien du tout.
    let before = fs::read_dir(root.join("Dossier")).unwrap().count();
    drop(cx, &["n'existe-pas.png", ".secret"], Aim::Folder(root.join("Dossier")));
    assert!(told(cx, Tone::Failed, "Rien n'a été copié") && fs::read_dir(root.join("Dossier")).unwrap().count() == before);
    drop(cx, &[], Aim::Root);
    // Un dossier à la racine du coffre, avec ses sous-dossiers.
    drop(cx, &["Photos"], Aim::Root);
    assert!(root.join("Photos/a.png").is_file() && root.join("Photos/été/b.png").is_file());
    assert!(told(cx, Tone::Done, "Copié 2 fichiers dans le coffre"));
    // Où se pose un dépôt sur la note : au début de la ligne visée, ou de la suivante dans sa moitié basse ;
    // jamais dans un tableau ni dans un bloc de code ; après la dernière ligne sous le texte.
    shell.update(cx, |s, cx| s.open_note(&root.join("Carnet.md"), cx));
    cx.run_until_parked();
    let (editor, line) = (shell.read_with(cx, |s, _| s.editor.clone()), |text: &str, what: &str| text.find(what).unwrap());
    let at = |cx: &mut gpui::VisualTestContext, what: &str, dy: f32| {
        let start = line(carnet, what);
        let mut spot = editor.read_with(cx, |e, _| e.point_of(start).unwrap());
        spot.y += px(dy);
        editor.read_with(cx, |e, _| e.drop_point(spot))
    };
    assert_eq!(at(cx, "premier", -4.), line(carnet, "premier"));
    assert_eq!(at(cx, "premier", 9.), line(carnet, "premier") + "premier\n".len(), "moitié basse : la ligne suivante, ici la vide");
    assert_eq!(at(cx, "| 1 | 2 |", -2.), line(carnet, "```") - 1, "dans un tableau : après lui (la ligne vide qui le suit)");
    assert_eq!(at(cx, "| a | b |", -8.), line(carnet, "| a | b |"), "avant la première ligne du tableau, c'est le bord du bloc");
    assert_eq!(at(cx, "code", 2.), line(carnet, "fin") - 1, "dans un bloc de code : après sa clôture");
    assert_eq!(at(cx, "fin", 40.), carnet.len());
    // Le dépôt sur la note : une ligne par élément, à leur place, dans le dossier de la note ; un seul pas
    // d'annulation ; le trait et l'étiquette disparaissent.
    let (version, premier) = (editor.read_with(cx, |e, _| e.revision()), line(carnet, "premier"));
    drop(cx, &["photo.png", "rapport.pdf", "Idées.md", "Photos", "inconnu.exe", ".secret"], Aim::Note { pane: 0, at: premier, version });
    let lines = "![[photo.png]]\n[[rapport.pdf]]\n[[Idées]]\n[[Photos 2]]\n";
    assert_eq!(text(cx), format!("# Carnet\n\n{lines}premier{}", &carnet["# Carnet\n\npremier".len()..]));
    assert!(root.join("photo.png").is_file() && root.join("Photos 2/a.png").is_file() && root.join("inconnu.exe").is_file());
    assert!(shell.read_with(cx, |s, _| s.notes.iter().any(|n| n.name == "Idées")), "la note déposée est une note du coffre");
    shell.update_in(cx, |s, window, cx| window.focus(&s.editor.focus_handle(cx)));
    cx.simulate_keystrokes("secondary-z");
    assert_eq!(text(cx), carnet);
    // Au bout d'une note sans retour à la ligne final, puis dans une note rangée dans un sous-dossier.
    let (version, end) = (editor.read_with(cx, |e, _| e.revision()), carnet.len());
    drop(cx, &["clip final.mp4"], Aim::Note { pane: 0, at: end, version });
    assert_eq!(text(cx), format!("{carnet}\n[[clip final.mp4]]"));
    cx.simulate_keystrokes("secondary-z");
    shell.update(cx, |s, cx| s.open_note(&root.join("Dossier/Autre.md"), cx));
    let (version, top) = (shell.read_with(cx, |s, cx| s.editor.read(cx).revision()), 0);
    drop(cx, &["photo.png"], Aim::Note { pane: 0, at: top, version });
    assert!(root.join("Dossier/photo.png").is_file(), "dans le dossier de la note, comme une image collée");
    assert!(text(cx).starts_with("![[photo.png]]\n# Autre"));
    cx.simulate_keystrokes("secondary-z");
    // Le texte a changé pendant la copie : les lignes vont où est le curseur, pas où était le trait.
    let stale = Aim::Note { pane: 0, at: 0, version: 0 };
    drop(cx, &["rapport.pdf"], stale);
    assert!(text(cx).contains("[[rapport 3.pdf]]") || text(cx).contains("[[rapport.pdf]]") || text(cx).contains("[[rapport 2.pdf]]"), "{}", text(cx));
    cx.simulate_keystrokes("secondary-z");
    // Les liens : une pièce jointe s'ouvre sans créer de note, un dossier s'ouvre en page liste, un nom
    // inconnu crée la note comme toujours.
    let notes = shell.read_with(cx, |s, _| s.notes.len());
    shell.update_in(cx, |s, window, cx| s.follow(&markdown::Link::Wiki("rapport.pdf".into()), window, cx));
    shell.update_in(cx, |s, window, cx| s.follow(&markdown::Link::Wiki("RAPPORT.PDF".into()), window, cx));
    assert_eq!(shell.read_with(cx, |s, _| s.notes.len()), notes, "ouvrir une pièce jointe ne crée pas de note");
    let opened = shell.read_with(cx, |s, _| s.opened.clone());
    assert!(opened.len() == 2 && opened.iter().all(|p| p.file_name().unwrap() == "rapport.pdf"), "le système ouvre la pièce jointe, sans tenir compte de la casse : {opened:?}");
    shell.update_in(cx, |s, window, cx| s.follow(&markdown::Link::Wiki("Photos".into()), window, cx));
    assert_eq!(shell.read_with(cx, |s, _| s.listing.as_ref().map(|l| l.of.clone())), Some(root.join("Photos")));
    shell.update_in(cx, |s, window, cx| s.follow(&markdown::Link::Wiki("nuage".into()), window, cx));
    assert!(shell.read_with(cx, |s, cx| s.listing.is_none() && s.editor.read(cx).text().starts_with("# nuage")));
    // L'étiquette : près de chaque bord de la fenêtre, elle reste dedans, et elle disparaît avec le glisser.
    let view = cx.update(|window, _| window.viewport_size());
    shell.update(cx, |s, _| s.keep_aim = true);
    for pointer in [point(px(4.), px(4.)), point(view.width - px(3.), px(4.)), point(px(4.), view.height - px(3.)), point(view.width - px(3.), view.height - px(3.))] {
        shell.update(cx, |s, cx| s.aim_drop(Some(DropAim { aim: Aim::Folder(root.join("Dossier")), items: 12, pointer }), cx));
        cx.run_until_parked();
        let chip = cx.debug_bounds("drop-chip").unwrap();
        assert!(chip.left() >= px(0.) && chip.top() >= px(0.) && chip.right() <= view.width && chip.bottom() <= view.height, "{pointer:?} : {chip:?}");
    }
    // Le glisser fini, l'étiquette part d'elle-même au prochain dessin.
    shell.update(cx, |s, cx| {
        s.keep_aim = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(shell.read_with(cx, |s, _| s.drop_aim.is_none()));
    let _ = fs::remove_dir_all(&base);
}
