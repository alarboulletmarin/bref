//! The user guide, notes and diagrams as resources, and the prompts an agent can run.

use serde_json::{Value, json};

use super::{Outcome, Server, arg_opt, draw, vault};

const PAGES: &[(&str, &str)] = &[
    ("introduction", include_str!("../../book/src/introduction.md")),
    ("install", include_str!("../../book/src/install.md")),
    ("first-steps", include_str!("../../book/src/first-steps.md")),
    ("writing", include_str!("../../book/src/writing.md")),
    ("tables", include_str!("../../book/src/tables.md")),
    ("links", include_str!("../../book/src/links.md")),
    ("comments", include_str!("../../book/src/comments.md")),
    ("daily-notes", include_str!("../../book/src/daily-notes.md")),
    ("navigation", include_str!("../../book/src/navigation.md")),
    ("tabs-and-panes", include_str!("../../book/src/tabs-and-panes.md")),
    ("search", include_str!("../../book/src/search.md")),
    ("boards", include_str!("../../book/src/boards.md")),
    ("diagrams", include_str!("../../book/src/diagrams.md")),
    ("uml", include_str!("../../book/src/uml.md")),
    ("csv", include_str!("../../book/src/csv.md")),
    ("vault", include_str!("../../book/src/vault.md")),
    ("sync", include_str!("../../book/src/sync.md")),
    ("settings", include_str!("../../book/src/settings.md")),
    ("updates", include_str!("../../book/src/updates.md")),
    ("shortcuts", include_str!("../../book/src/shortcuts.md")),
    ("compatibility", include_str!("../../book/src/compatibility.md")),
    ("troubleshooting", include_str!("../../book/src/troubleshooting.md")),
    ("performance", include_str!("../../book/src/performance.md")),
    ("agents", include_str!("../../book/src/agents.md")),
];

const PROMPTS: &[(&str, &str, &str, &[(&str, &str, bool)])] = &[
    (
        "document_feature",
        "Document a feature",
        "Write a note about a feature: context, decisions, how it works, diagrams, and links to the notes it touches.",
        &[("feature", "The feature to document", true), ("code", "The code or files the feature lives in", false)],
    ),
    (
        "architecture_overview",
        "Architecture overview",
        "A hub note, a component diagram, a package diagram, and one note per part.",
        &[("project", "What the overview is about", true)],
    ),
    (
        "uml_from_code",
        "UML diagram from code",
        "One UML diagram of the given kind, drawn from the code in scope and checked with render.",
        &[("scope", "The code the diagram describes", true), ("kind", "class, sequence, state, component, package, activity, use-case or deployment", true)],
    ),
    (
        "review_note",
        "Review a note",
        "Read a note and leave comments on the passages that need them, with add_comment.",
        &[("note", "The note to review, by name or path", true)],
    ),
    (
        "weekly_review",
        "Weekly review",
        "Read the daily notes of a week and write one note that sums it up, with links to the days and the notes they mention.",
        &[("date", "A day of the week to review, YYYY-MM-DD (default today)", false)],
    ),
    (
        "link_orphans",
        "Link orphan notes",
        "Find the notes with no link in or out, and propose where to link each one.",
        &[("folder", "Only the notes in this folder (default the whole vault)", false)],
    ),
    (
        "tidy_tags",
        "Tidy tags",
        "Find tags that are duplicates or misspellings of each other, and propose which to merge.",
        &[],
    ),
];

pub fn list() -> Vec<Value> {
    let mut resources: Vec<Value> = PAGES.iter().map(|(page, text)| page_resource(page, text)).collect();
    resources.push(json!({
        "uri": "bref://library",
        "name": "library",
        "title": "Diagram library",
        "description": "Forms, heads, routes, UML relationships, library shapes and starting points.",
        "mimeType": "application/json",
    }));
    resources
}

pub fn templates() -> Vec<Value> {
    vec![
        json!({
            "uriTemplate": "bref://note/{path}",
            "name": "note",
            "title": "A note",
            "description": "The text of a note. path is relative to the vault, as list_notes shows it.",
            "mimeType": "text/markdown",
        }),
        json!({
            "uriTemplate": "bref://diagram/{path}",
            "name": "diagram",
            "title": "A diagram",
            "description": "A canvas diagram as JSON and as SVG. path is relative to the vault.",
            "mimeType": "application/json",
        }),
    ]
}

pub fn read(server: &mut Server, uri: &str) -> Result<Vec<Value>, String> {
    let uri = decode(uri);
    if let Some(page) = uri.strip_prefix("bref://guide/") {
        let Some((_, text)) = PAGES.iter().find(|(name, _)| *name == page) else {
            return Err(format!("no guide page `{page}`"));
        };
        return Ok(vec![text_content(&uri, "text/markdown", text)]);
    }
    if uri == "bref://library" {
        let value = draw::library_value(None)?;
        return Ok(vec![text_content(&uri, "application/json", &value.to_string())]);
    }
    if let Some(path) = uri.strip_prefix("bref://note/") {
        let vault = server.need().map_err(|outcome| sentence(&outcome))?;
        let note = vault.find_note(path)?;
        let (text, _) = vault::read_text(&note.path)?;
        return Ok(vec![text_content(&format!("bref://note/{path}"), "text/markdown", &text)]);
    }
    if let Some(path) = uri.strip_prefix("bref://diagram/") {
        let vault = server.need().map_err(|outcome| sentence(&outcome))?;
        let file = vault.find_file(path)?;
        let svg = std::fs::read_to_string(&file).map_err(super::disk)?;
        let shown = format!("bref://diagram/{}", vault::rel(&vault.root, &file));
        let body = draw::read_diagram(vault, &json!({"path": vault::rel(&vault.root, &file)}));
        if body.is_error {
            return Err(sentence(&body));
        }
        return Ok(vec![text_content(&shown, "application/json", &body.structured.to_string()), text_content(&shown, "image/svg+xml", &svg)]);
    }
    Err(format!("no resource `{uri}`"))
}

pub fn prompts() -> Vec<Value> {
    PROMPTS.iter().map(|(name, title, description, arguments)| prompt_info(name, title, description, arguments)).collect()
}

pub fn prompt(name: &str, args: &Value) -> Result<Value, String> {
    let Some((_, title, _, arguments)) = PROMPTS.iter().find(|(known, ..)| *known == name) else {
        return Err(format!("unknown prompt `{name}`"));
    };
    let mut values = Vec::new();
    for (key, _, required) in *arguments {
        match arg_opt(args, key) {
            Some(value) => values.push((*key, value.to_string())),
            None if *required => return Err(format!("`{key}` is missing")),
            None => {}
        }
    }
    let text = match name {
        "document_feature" => document_feature(&values),
        "architecture_overview" => architecture(&values),
        "uml_from_code" => uml(&values),
        "review_note" => review(&values),
        "weekly_review" => weekly(&values),
        "link_orphans" => orphans(&values),
        "tidy_tags" => TIDY_TAGS.to_string(),
        _ => return Err(format!("unknown prompt `{name}`")),
    };
    Ok(json!({
        "description": title,
        "messages": [{"role": "user", "content": {"type": "text", "text": text}}],
    }))
}

fn page_resource(page: &str, text: &str) -> Value {
    let title = text.lines().find_map(|line| line.strip_prefix("# ")).unwrap_or(page);
    json!({
        "uri": format!("bref://guide/{page}"),
        "name": page,
        "title": title,
        "description": format!("User guide: {title}."),
        "mimeType": "text/markdown",
    })
}

fn prompt_info(name: &str, title: &str, description: &str, arguments: &[(&str, &str, bool)]) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "arguments": arguments.iter().map(|(name, description, required)| json!({
            "name": name,
            "description": description,
            "required": required,
        })).collect::<Vec<_>>(),
    })
}

fn text_content(uri: &str, mime: &str, text: &str) -> Value {
    json!({"uri": uri, "mimeType": mime, "text": text})
}

fn sentence(outcome: &Outcome) -> String {
    outcome.structured.get("error").and_then(|v| v.as_str()).unwrap_or("the resource could not be read").to_string()
}

fn value<'a>(values: &'a [(&str, String)], key: &str) -> &'a str {
    values.iter().find(|(name, _)| *name == key).map(|(_, value)| value.as_str()).unwrap_or("")
}

/// How a UML diagram is drawn on Bref's canvas, so that it stays a diagram the user can edit and
/// that follows UML: shared by the prompts that draw.
const UML: &str = "\
Draw on the canvas with create_diagram from shapes, links and fragments, not from Mermaid: the user moves and edits \
these shapes in Bref. Call diagram_library first for the forms and relations. \
Sequence diagram: one scenario per diagram. Lifelines (form lifeline) left to right in the order they first take part, \
the one that starts first; name each by its role or component, at most about seven. Messages in time order: sync for a call \
that waits, with its reply (dashed) carrying what comes back; async for an event or a call that does not wait; create and \
destroy for lifelines that begin or end; a message to the same lifeline for a call to itself. Write a message as \
operation(arguments), a reply as the value returned. Conditions and repetition are fragments: alt with one [guard] per \
branch and [else], opt, loop [condition], par; outer fragments first. Leave out logging, getters and other noise. \
Class diagram: a class is a rect whose text is the name, ---, the attributes (+ name: Type), ---, the operations \
(+ name(arg: Type): Type); generalization and realization for inheritance, composition and aggregation for ownership \
with multiplicities in start_text and end_text (1, 0..1, *), dependency for a use. \
Then render the diagram, look at it, and fix what overlaps or reads badly with edit_diagram (move, wrap, reorder, update) \
until it is clean. Embed it with the [[Name.svg]] line create_diagram returns.";

fn document_feature(values: &[(&str, String)]) -> String {
    let feature = value(values, "feature");
    let code = value(values, "code");
    format!(
        "Document the feature \"{feature}\" in this Bref vault. \
Read bref://guide/writing, bref://guide/links, bref://guide/diagrams and bref://guide/uml first. \
Search the vault for notes that already mention it, and follow their links. \
Create one note: the context, the decisions, and how it works. Link the notes it touches by name with [[Note]]. \
Add a technical part: the parts involved and how they talk. Draw a class diagram of the main types and a sequence diagram \
of the main scenario, and explain each in a few sentences under it. {UML} \
Do not delete a file. When you are done, show the note in Bref with show. \
The code to start from:\n{code}"
    )
}

fn architecture(values: &[(&str, String)]) -> String {
    let project = value(values, "project");
    format!(
        "Write an architecture overview of \"{project}\" in this Bref vault. \
Read bref://guide/diagrams and bref://guide/uml. \
Create a hub note that links to one note per part. \
Draw a component diagram and a package diagram with create_diagram, embed them, and render them. \
Use diagram_library for the forms and relationships. Do not delete a file."
    )
}

fn uml(values: &[(&str, String)]) -> String {
    let scope = value(values, "scope");
    let kind = value(values, "kind");
    format!(
        "Draw one {kind} diagram of this code, as a Bref canvas diagram. \
Read bref://guide/uml. {UML} \
Put it in a short note that says what it shows. Do not delete a file.\n\n{scope}"
    )
}

fn review(values: &[(&str, String)]) -> String {
    let note = value(values, "note");
    format!(
        "Review the note \"{note}\". Read it, then add_comment on each passage that is unclear, wrong, or missing, \
one comment per passage, and only where the passage occurs once. \
Read bref://guide/comments for the markup. Do not change the note except through add_comment, and do not delete a file."
    )
}

fn weekly(values: &[(&str, String)]) -> String {
    let day = match value(values, "date") {
        "" => "today".to_string(),
        date => date.to_string(),
    };
    format!(
        "Review the week, Monday to Sunday, that holds {day} in this Bref vault. Read bref://guide/daily-notes and bref://guide/links. \
Find the daily notes of those seven days with search_notes or list_notes (sort recent) and read each with read_note. \
Do not call daily_note for a day that has no note: it would create an empty one. \
Create one note that sums the week up: what was done, what was decided, what is still open. \
Link each daily note and each note they mention by name with [[Note]]. Do not delete a file. \
When you are done, show the note in Bref with show."
    )
}

fn orphans(values: &[(&str, String)]) -> String {
    let scope = match value(values, "folder") {
        "" => "the vault".to_string(),
        folder => format!("the folder \"{folder}\""),
    };
    format!(
        "Find the orphan notes in {scope}: list the notes with list_notes (kind note, follow next to the end), \
and call note_links on each; an orphan has no outgoing link and no backlink. \
For each orphan, read it and search_notes for its subject to find the notes it belongs with. \
Then propose, for each orphan, the note that should link to it and the sentence where the [[link]] goes. \
Do not change any note: give the list, and wait for the user to choose which links to add."
    )
}

const TIDY_TAGS: &str = "\
List every tag with list_tags. Group the tags that mean the same: a different case or accent, singular and plural, \
- or _ or / between words, a typo, a translation. For each group, give each tag with its count, the tag to keep \
(usually the most used), and the notes that carry the others (list_notes with tag). \
Do not change any note: give the list, and wait for the user to agree. Then, for each note, read_note and edit_note \
with its revision to replace the tag, and do not delete a file.";

/// `%20` and the like. A `%` that is not an escape is kept.
fn decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && let Ok(byte) = u8::from_str_radix(&uri[i + 1..i + 3], 16) {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| uri.to_string())
}
