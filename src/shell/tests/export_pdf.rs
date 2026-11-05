//! The note saved as PDF: from the palette, with its unsaved edits, under a free name, its missing
//! pictures said; cancelling writes nothing, a folder that cannot be written is said.

use super::*;

#[gpui::test]
fn a_note_is_saved_as_pdf(cx: &mut TestAppContext) {
    let (root, shell, cx, _config) = start(cx, "export-pdf");
    let note = root.join("Rapport.md");
    let written = "# Rapport\n\nTexte ![](absent.png)\n![](p.png)\n![](cut.png)\n- [x] fait\n";
    fs::write(&note, written).unwrap();
    // A 2×1 PNG, and one cut short.
    let png = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAADUlEQVR4nGPYEqwFRAAJcgJjO/BPNwAAAABJRU5ErkJggg==").unwrap();
    fs::write(root.join("p.png"), &png).unwrap();
    fs::write(root.join("cut.png"), &png[..40]).unwrap();
    shell.update(cx, |s, cx| s.rescan(&root, cx));
    shell.update_in(cx, |s, window, cx| s.open_from_nav(&note, window, cx));
    shell.update_in(cx, |s, window, cx| window.focus(&s.pane().editor.focus_handle(cx)));
    // An edit not saved yet: the title changes in the editor only.
    cx.simulate_keystrokes("secondary-home end");
    cx.simulate_input(" final");
    assert_eq!(fs::read_to_string(&note).unwrap(), written);
    let said = |cx: &mut gpui::VisualTestContext| shell.read_with(cx, |s, _| s.toasts.iter().map(|t| (t.tone, t.text.clone())).collect::<Vec<_>>());
    let export = |cx: &mut gpui::VisualTestContext, to: Option<&str>| {
        // From the command palette, as a user finds it.
        cx.simulate_keystrokes("f10");
        cx.simulate_input("pdf");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(cx.did_prompt_for_new_path());
        let (to, folder) = (to.map(|to| root.join(to)), root.clone());
        cx.simulate_new_path_selection(move |dir| {
            assert_eq!(dir, folder);
            to
        });
        cx.run_until_parked();
    };

    export(cx, Some("Rapport.pdf"));
    let pdf = fs::read(root.join("Rapport.pdf")).unwrap_or_else(|e| panic!("{e} {:?}", said(cx)));
    assert!(pdf.starts_with(b"%PDF-"));
    // Its title is the edited one: the text in memory was printed, before the note was saved.
    assert!(String::from_utf8_lossy(&pdf).contains("/Title (Rapport final)"));
    let told = said(cx);
    assert!(told.iter().any(|(tone, t)| *tone == Tone::Info && t.contains("Rapport.pdf") && t.contains("absent.png") && t.contains("cut.png") && !t.contains(" p.png")), "{told:?}");

    // The name is taken: a second file, the first one left alone.
    export(cx, Some("Rapport.pdf"));
    assert_eq!(fs::read(root.join("Rapport.pdf")).unwrap(), pdf);
    assert!(root.join("Rapport 2.pdf").is_file());
    assert!(said(cx).iter().any(|(_, t)| t.contains("Rapport 2.pdf")));

    // Without an extension, it gets one.
    export(cx, Some("Sans"));
    assert!(root.join("Sans.pdf").is_file() && !root.join("Sans").exists());

    // Cancelled: nothing written, nothing said.
    let before = (fs::read_dir(&root).unwrap().count(), said(cx));
    export(cx, None);
    assert_eq!((fs::read_dir(&root).unwrap().count(), said(cx)), before);

    // A folder that does not exist: the failure is said.
    export(cx, Some("absent/x.pdf"));
    assert!(said(cx).iter().any(|(tone, t)| *tone == Tone::Failed && t.contains("PDF")), "{:?}", said(cx));
}
