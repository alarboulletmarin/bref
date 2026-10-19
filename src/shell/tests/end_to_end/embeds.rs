//! Sections of other notes embedded in place: `![[Note#Heading]]`, `![[Note#^id]]`, `![[Note]]`.

use super::*;

pub(super) fn run(j: &Journey, cx: &mut gpui::VisualTestContext) {
    let (shell, root) = (&j.shell, &j.root);
    let text = |cx: &mut gpui::VisualTestContext| j.text(cx);
    let embeds = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).embeds_shown());
    let section = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).first_section(cx));
    let note = |name: &str| root.join(format!("{name}.md"));
    let open = |cx: &mut gpui::VisualTestContext, path: &Path| {
        shell.update_in(cx, |s, window, cx| s.open_from_nav(path, window, cx));
        cx.run_until_parked();
    };
    let project = "---\ntags: [x]\n---\n# Projet\n\nIntro.\n\n## Objectifs\n\n- vite\n- simple ^obj\n\n### Détail\n\ndétail.\n\n## Contacts\n\nLéa.\n";
    fs::write(note("Projet emb"), project).unwrap();
    let week = "# Semaine\n\n![[Projet emb#Objectifs]]\n\navant ![[Projet emb#Contacts]] après\n\n![[Projet emb#^obj]]\n\n![[Projet emb#Absent]]\n\n![[Projet emb#^zzz]]\n\n![[Inconnue emb]]\n\n![[Projet emb]]\n\nfin\n";
    fs::write(note("Semaine emb"), week).unwrap();
    j.settle(cx);
    open(cx, &note("Semaine emb"));
    // Each embed alone on its line has its frame: the section, or why there is none. One inside a sentence is only text.
    let shown = embeds(cx);
    let written: Vec<&str> = shown.iter().map(|(w, ..)| w.as_str()).collect();
    assert_eq!(written, ["Projet emb#Objectifs", "Projet emb#^obj", "Projet emb#Absent", "Projet emb#^zzz", "Inconnue emb", "Projet emb"]);
    let messages: Vec<Option<&str>> = shown.iter().map(|(_, m, _)| m.as_deref()).collect();
    assert_eq!(messages, [None, None, Some("titre introuvable"), Some("bloc introuvable"), Some("note introuvable"), None]);
    assert!(shown[0].2 > 100. && shown[1].2 > 20. && shown[2].2 == 0.);
    assert_eq!(section(cx).as_deref(), Some("## Objectifs\n\n- vite\n- simple ^obj\n\n### Détail\n\ndétail."));
    // The note itself is untouched, and typing in it goes on as before.
    assert_eq!(text(cx), week);
    // A circular embed, a note that embeds itself or leads back to the one shown, and a chain too deep.
    fs::write(note("Boucle emb"), "# Boucle\n\n![[Boucle emb]]\n\n![[Autre emb]]\n").unwrap();
    fs::write(note("Autre emb"), "# Autre\n\n![[Boucle emb]]\n").unwrap();
    for (name, depth) in [("A1", "A2"), ("A2", "A3"), ("A3", "A4"), ("A4", "A5")] {
        fs::write(note(&format!("{name} emb")), format!("# {name}\n\n![[{depth} emb]]\n")).unwrap();
    }
    fs::write(note("A5 emb"), "# A5\n\nfin\n").unwrap();
    j.settle(cx);
    open(cx, &note("Boucle emb"));
    let looped = embeds(cx);
    assert_eq!(looped[0], ("Boucle emb".to_string(), Some("incorporation circulaire".to_string()), 0.));
    assert!(looped[1].1.is_none(), "{looped:?}");
    open(cx, &note("A1 emb"));
    // A1 embeds A2, which embeds A3, which embeds A4: three deep; A4 leading to A5 is one too many.
    let chain = shell.read_with(cx, |s, cx| {
        let mut found = Vec::new();
        let editor = s.pane().editor.read(cx);
        fn walk(e: &Editor, cx: &gpui::App, found: &mut Vec<Option<String>>) {
            for (_, message, _) in e.embeds_shown() {
                found.push(message);
            }
            if let Some(child) = e.first_child(cx) {
                walk(child.read(cx), cx, found);
            }
        }
        walk(editor, cx, &mut found);
        found
    });
    assert_eq!(chain, [None, None, None, Some("incorporation trop profonde".to_string())], "{chain:?}");
    // The embed follows its note: a text added to the section, a heading renamed away, the note deleted.
    open(cx, &note("Semaine emb"));
    fs::write(note("Projet emb"), project.replace("- vite", "- vite\n- encore")).unwrap();
    shell.update(cx, |s, cx| s.flush(cx));
    cx.run_until_parked();
    assert!(section(cx).is_some_and(|s| s.contains("- encore")), "{:?}", section(cx));
    fs::write(note("Projet emb"), project.replace("## Objectifs", "## Buts")).unwrap();
    shell.update(cx, |s, cx| s.flush(cx));
    cx.run_until_parked();
    assert_eq!(embeds(cx)[0].1.as_deref(), Some("titre introuvable"));
    fs::write(note("Projet emb"), project).unwrap();
    shell.update(cx, |s, cx| s.flush(cx));
    cx.run_until_parked();
    assert_eq!(embeds(cx)[0].1, None);
    // Opening: the source's name, a double click on the frame, Alt+Enter on the line, Ctrl+click on a link to a heading.
    let points = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).embed_points());
    let none = gpui::Modifiers::none();
    cx.simulate_click(points[0].1, none);
    let at_contacts = |cx: &mut gpui::VisualTestContext, heading: &str| {
        let opened = shell.read_with(cx, |s, cx| (s.pane().path.clone(), s.pane().editor.read(cx).caret(), s.pane().editor.read(cx).text().to_string()));
        (opened.0, opened.2[opened.1..].starts_with(heading))
    };
    assert_eq!(at_contacts(cx, "## Objectifs"), (Some(note("Projet emb")), true));
    let back = if cfg!(target_os = "macos") { "cmd-[" } else { "alt-left" };
    cx.simulate_keystrokes(back);
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(note("Semaine emb")));
    let points = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).embed_points());
    cx.simulate_event(gpui::MouseDownEvent { position: points[1].0, modifiers: none, button: MouseButton::Left, click_count: 2, first_mouse: false });
    assert_eq!(at_contacts(cx, "simple ^obj").0, Some(note("Projet emb")));
    cx.simulate_keystrokes(back);
    // A single click in the frame only puts the cursor on its line.
    cx.simulate_click(points[0].0, none);
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(note("Semaine emb")));
    let line = week.find("![[Projet emb#Objectifs]]").unwrap();
    assert_eq!(shell.read_with(cx, |s, cx| s.pane().editor.read(cx).caret()), line + "![[Projet emb#Objectifs]]".len());
    cx.simulate_keystrokes("alt-enter");
    assert_eq!(at_contacts(cx, "## Objectifs"), (Some(note("Projet emb")), true));
    cx.simulate_keystrokes(back);
    // What follows is typed in a note of its own, not in the week's.
    fs::write(note("Brouillon emb"), "# Brouillon\n").unwrap();
    j.settle(cx);
    open(cx, &note("Brouillon emb"));
    // Enter at the end of an embed's line is a line break, as ever: typing an embed does not open it.
    j.load(cx, "![[Projet emb#Contacts]]", 24);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "![[Projet emb#Contacts]]\n");
    assert_eq!(shell.read_with(cx, |s, _| s.pane().path.clone()), Some(note("Brouillon emb")));
    // A line that is no embed: a picture stays a picture, a code block is left alone, a plain link is a link.
    j.load(cx, "![[photo.png]]\n\n```\n![[Projet emb]]\n```\n\n[[Projet emb]]\n\n- ![[Projet emb]]\n", 0);
    assert_eq!(embeds(cx), []);
    // Completion after `#`: the headings of the note, then after `^` its blocks.
    j.load(cx, "![[Projet emb#", 14);
    let offered = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, cx| s.pane().editor.read(cx).completions());
    assert_eq!(offered(cx), ["#Projet", "#Objectifs", "#Détail", "#Contacts"]);
    cx.simulate_input("d");
    assert_eq!(offered(cx), ["#Détail"]);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(cx), "![[Projet emb#Détail]]");
    j.load(cx, "![[Projet emb#^", 15);
    assert_eq!(offered(cx), ["#^obj"]);
    j.load(cx, "[[Inconnue emb#", 15);
    assert!(offered(cx).is_empty());
    // A heading renamed in its note: the embeds and the links that point at it follow, in the other notes.
    open(cx, &note("Projet emb"));
    let at = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().find("Objectifs").unwrap());
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.place_cursor(at, cx)));
    cx.simulate_keystrokes("shift-right shift-right shift-right shift-right shift-right shift-right shift-right shift-right shift-right");
    cx.simulate_input("Buts");
    shell.update(cx, |s, cx| s.flush(cx));
    cx.run_until_parked();
    let follows = fs::read_to_string(note("Semaine emb")).unwrap();
    assert!(follows.contains("![[Projet emb#Buts]]") && !follows.contains("#Objectifs"), "{follows}");
    // Two headings changed at once, or an emptied one, are no rename.
    let at = shell.read_with(cx, |s, cx| s.pane().editor.read(cx).text().find("Buts").unwrap());
    shell.update(cx, |s, cx| s.pane().editor.update(cx, |e, cx| e.place_cursor(at, cx)));
    cx.simulate_keystrokes("shift-right shift-right shift-right shift-right");
    cx.simulate_keystrokes("backspace");
    shell.update(cx, |s, cx| s.flush(cx));
    cx.run_until_parked();
    assert!(fs::read_to_string(note("Semaine emb")).unwrap().contains("![[Projet emb#Buts]]"), "an emptied heading renames nothing");
}
