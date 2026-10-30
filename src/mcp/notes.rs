//! Reading and writing notes. The rules are the app's: a taken name is refused, a write is
//! atomic, a rename rewrites links unless another note still has the old name, and nothing
//! is deleted.

use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::{
    lang::{date_name, time_name},
    logic::{
        daily, embed, markdown,
        embed::Anchor,
        markdown::Link,
        rank,
    },
    platform::{clock, vault as files},
};

use super::{
    Outcome, arg_opt, arg_str, arg_usize, disk,
    vault::{self, Vault},
};

pub fn vault_info(vault: &Vault) -> Outcome {
    let mut tags: Vec<(String, usize)> = Vec::new();
    for note in vault.notes.iter().filter(|n| !vault.is_template(n)) {
        for tag in note.tags.iter() {
            if let Some((_, count)) = tags.iter_mut().find(|(name, _)| name == tag) {
                *count += 1;
            } else {
                tags.push((tag.clone(), 1));
            }
        }
    }
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    tags.truncate(12);
    let folders: Vec<String> = {
        let mut dirs: Vec<String> = vault.dirs.iter().map(|dir| vault::rel(&vault.root, dir)).filter(|dir| !dir.is_empty()).collect();
        dirs.sort();
        dirs
    };
    Outcome::ok(json!({
        "folder": vault.root.display().to_string(),
        "notes": vault.notes.iter().filter(|n| !vault.is_template(n)).count(),
        "templates": vault.notes.iter().filter(|n| vault.is_template(n)).count(),
        "diagrams": vault.diagrams.len(),
        "tables": vault.tables.len(),
        "pictures": vault.pictures.len(),
        "attachments": vault.attachments.len(),
        "folders": folders,
        "tags": tags.into_iter().map(|(tag, count)| json!({"tag": tag, "count": count})).collect::<Vec<_>>(),
        "daily": date_name(clock::today()),
        "version": env!("CARGO_PKG_VERSION"),
        "diagram_version": crate::logic::diagram::VERSION,
        "unreadable": vault.unreadable.iter().map(|(path, kind)| json!({
            "path": vault::rel(&vault.root, path),
            "error": disk(std::io::Error::new(*kind, "")),
        })).collect::<Vec<_>>(),
    }))
}

pub fn list_notes(vault: &Vault, args: &Value) -> Outcome {
    let kind = arg_opt(args, "kind").unwrap_or("all");
    let sort = arg_opt(args, "sort").unwrap_or("name");
    if !matches!(kind, "note" | "diagram" | "table" | "picture" | "attachment" | "all") {
        return Outcome::err(format!("unknown kind `{kind}`"));
    }
    if !matches!(sort, "name" | "recent") {
        return Outcome::err(format!("unknown sort `{sort}`"));
    }
    let limit = arg_usize(args, "limit").unwrap_or(100).clamp(1, 200);
    let cursor: usize = arg_opt(args, "cursor").and_then(|c| c.parse().ok()).unwrap_or(0);
    let mut rows = Vec::new();
    if matches!(kind, "note" | "all") {
        for note in &vault.notes {
            if !kept(vault, &note.path, &note.tags, args) {
                continue;
            }
            rows.push((note.name.clone(), vault::unix(note.mtime), json!({
                "path": vault::rel(&vault.root, &note.path),
                "title": note.name,
                "folder": vault::folder(&vault.root, &note.path),
                "tags": note.tags,
                "modified": vault::unix(note.mtime),
                "kind": "note",
            })));
        }
    }
    let files: [(&str, &[std::path::PathBuf]); 4] = [
        ("diagram", &vault.diagrams),
        ("table", &vault.tables),
        ("picture", &vault.pictures),
        ("attachment", &vault.attachments),
    ];
    for (file_kind, paths) in files {
        if kind != "all" && kind != file_kind {
            continue;
        }
        for path in paths {
            if !kept(vault, path, &[], args) {
                continue;
            }
            let modified = path.metadata().and_then(|m| m.modified()).map(vault::unix).unwrap_or(0);
            let title = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            rows.push((title.clone(), modified, json!({
                "path": vault::rel(&vault.root, path),
                "title": title,
                "folder": vault::folder(&vault.root, path),
                "tags": [],
                "modified": modified,
                "kind": file_kind,
            })));
        }
    }
    if sort == "recent" {
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| files::natural(&a.0).cmp(&files::natural(&b.0))));
    } else {
        rows.sort_by(|a, b| files::natural(&a.0).cmp(&files::natural(&b.0)));
    }
    let next = (cursor + limit < rows.len()).then(|| (cursor + limit).to_string());
    let notes: Vec<Value> = rows.into_iter().skip(cursor).take(limit).map(|(_, _, row)| row).collect();
    Outcome::ok(json!({"notes": notes, "next": next}))
}

fn kept(vault: &Vault, path: &Path, tags: &[String], args: &Value) -> bool {
    if let Some(folder) = arg_opt(args, "folder") {
        let dir = vault::folder(&vault.root, path);
        if dir != folder && !dir.starts_with(&format!("{folder}/")) {
            return false;
        }
    }
    if let Some(tag) = arg_opt(args, "tag") {
        let tag = tag.trim_start_matches('#');
        if !tags.iter().any(|t| t == tag) {
            return false;
        }
    }
    true
}

pub fn search_notes(vault: &Vault, args: &Value) -> Outcome {
    let query = match arg_str(args, "query") {
        Ok(query) => query,
        Err(message) => return Outcome::err(message),
    };
    let limit = arg_usize(args, "limit").unwrap_or(20).clamp(1, 50);
    let q = query.to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut scored = Vec::new();
    let mut named = vec![false; vault.notes.len()];
    for (i, note) in vault.notes.iter().enumerate() {
        if vault.is_template(note) || !kept(vault, &note.path, &note.tags, args) {
            continue;
        }
        if let Some(score) = rank::fuzzy(&q, &note.name.to_lowercase()) {
            scored.push((score, i));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, i) in &scored {
        named[i] = true;
    }
    let mut hits: Vec<Value> = scored
        .into_iter()
        .take(limit)
        .map(|(_, i)| hit(vault, &vault.notes[i], "name", ""))
        .collect();
    if hits.len() < limit && q.chars().count() >= 2 {
        let word = words.first().copied().unwrap_or(q.as_str());
        for (i, note) in vault.notes.iter().enumerate() {
            if named[i] || vault.is_template(note) || !kept(vault, &note.path, &note.tags, args) {
                continue;
            }
            if rank::contains_all(&note.lower, &words) {
                hits.push(hit(vault, note, "text", &rank::snippet(&note.body, &note.lower, word)));
                if hits.len() == limit {
                    break;
                }
            }
        }
    }
    Outcome::ok(json!({"hits": hits}))
}

fn hit(vault: &Vault, note: &crate::logic::note::Note, where_: &str, snippet: &str) -> Value {
    json!({
        "path": vault::rel(&vault.root, &note.path),
        "title": note.name,
        "folder": vault::folder(&vault.root, &note.path),
        "where": where_,
        "snippet": snippet,
    })
}

pub fn read_note(vault: &Vault, args: &Value) -> Outcome {
    let note = match open(vault, args) {
        Ok(note) => note,
        Err(outcome) => return outcome,
    };
    let (text, revision) = match vault::read_text(&note.path) {
        Ok(read) => read,
        Err(message) => return Outcome::err(message),
    };
    let shown = match arg_opt(args, "section") {
        Some(section) => match section_text(&text, section) {
            Ok(part) => part,
            Err(message) => return Outcome::err(message),
        },
        None => text.clone(),
    };
    let meta = markdown::front_matter(&text);
    Outcome::ok(json!({
        "path": vault::rel(&vault.root, &note.path),
        "title": note.name,
        "front_matter": &text[..meta],
        "tags": note.tags,
        "aliases": note.aliases,
        "text": shown,
        "links": outgoing(vault, &text),
        "embeds": embeds(vault, &text),
        "comments": comments_of(&text),
        "revision": revision,
    }))
}

fn section_text(text: &str, section: &str) -> Result<String, String> {
    let anchor = if let Some(id) = section.strip_prefix('^') {
        if id.is_empty() {
            return Err("the block id is empty".into());
        }
        Anchor::Block(id.to_string())
    } else {
        Anchor::Heading(section.to_string())
    };
    embed::section(text, &anchor).ok_or_else(|| match &anchor {
        Anchor::Block(id) => format!("no block `^{id}`"),
        _ => format!("no heading `{section}`"),
    })
}

fn outgoing(vault: &Vault, text: &str) -> Vec<Value> {
    let mut found = Vec::new();
    let mut seen = Vec::new();
    let mut in_code = false;
    for (row, line) in text.lines().enumerate() {
        if markdown::is_fence(line) {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        for (_, link) in markdown::links(line) {
            let Link::Wiki(name) = link else { continue };
            let target = name.split(['#', '|']).next().unwrap_or(&name).trim().to_string();
            if target.is_empty() || seen.contains(&target.to_lowercase()) {
                continue;
            }
            seen.push(target.to_lowercase());
            let resolved = wiki_path(vault, &target);
            found.push(json!({"target": target, "path": resolved, "line": row + 1}));
        }
    }
    found
}

fn wiki_path(vault: &Vault, name: &str) -> Option<String> {
    vault.find_note(name).ok().map(|note| vault::rel(&vault.root, &note.path))
}

fn embeds(vault: &Vault, text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(embed::parse)
        .map(|target| {
            let resolved = wiki_path(vault, &target.note);
            json!({
                "note": target.note,
                "heading": match &target.anchor {
                    Anchor::Heading(h) => Some(h.clone()),
                    Anchor::Block(id) => Some(format!("^{id}")),
                    Anchor::Whole => None,
                },
                "path": resolved,
            })
        })
        .collect()
}

fn comments_of(text: &str) -> Vec<Value> {
    markdown::all_comments(text)
        .into_iter()
        .map(|note| json!({"passage": note.noted, "comment": note.said, "scope": note.scope}))
        .collect()
}

pub fn note_links(vault: &Vault, args: &Value) -> Outcome {
    let note = match open(vault, args) {
        Ok(note) => note,
        Err(outcome) => return outcome,
    };
    let path = note.path.clone();
    let back: Vec<Value> = wiki::backlinks_of(vault, &path);
    let text = fs::read_to_string(&path).unwrap_or_default();
    let pictures: Vec<Value> = text
        .lines()
        .filter_map(|line| markdown::image(line).map(|(_, name)| name))
        .map(|name| json!({"name": name, "path": vault.find_file(&name).ok().map(|p| vault::rel(&vault.root, &p))}))
        .collect();
    Outcome::ok(json!({
        "outgoing": outgoing(vault, &text),
        "backlinks": back,
        "tags": note.tags,
        "embeds": embeds(vault, &text),
        "pictures": pictures,
    }))
}

pub fn list_tags(vault: &Vault) -> Outcome {
    let mut tags: Vec<(String, usize)> = Vec::new();
    for note in vault.notes.iter().filter(|n| !vault.is_template(n)) {
        for tag in note.tags.iter() {
            if let Some((_, count)) = tags.iter_mut().find(|(name, _)| name == tag) {
                *count += 1;
            } else {
                tags.push((tag.clone(), 1));
            }
        }
    }
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Outcome::ok(json!({"tags": tags.into_iter().map(|(tag, count)| json!({"tag": tag, "count": count})).collect::<Vec<_>>()}))
}

fn open<'a>(vault: &'a Vault, args: &Value) -> Result<&'a crate::logic::note::Note, Outcome> {
    let name = arg_str(args, "note").map_err(Outcome::err)?;
    vault.find_note(name).map_err(Outcome::err)
}

pub fn create_note(vault: &mut Vault, args: &Value) -> Outcome {
    let title = match arg_str(args, "title") {
        Ok(title) => title,
        Err(message) => return Outcome::err(message),
    };
    let Some(name) = files::clean_name(title) else {
        return Outcome::err("invalid name");
    };
    let dir = match place(vault, arg_opt(args, "folder")) {
        Ok(dir) => dir,
        Err(outcome) => return outcome,
    };
    if let Err(message) = fs::create_dir_all(&dir).map_err(disk) {
        return Outcome::err(message);
    }
    if let Some(taken) = vault::name_taken(&dir, &name, "md") {
        return Outcome::err(format!("a note already has the name `{name}` ({})", vault::rel(&vault.root, &taken)));
    }
    let body = if let Some(template) = arg_opt(args, "template") {
        let Some(path) = template_path(vault, template) else {
            return Outcome::err(format!("no template named `{template}`"));
        };
        let text = match fs::read_to_string(&path).map_err(disk) {
            Ok(text) => text,
            Err(message) => return Outcome::err(message),
        };
        markdown::fill(&text, &name, &date_name(clock::today()), &time_name(clock::clock())).0
    } else {
        let mut body = format!("# {name}\n\n");
        if let Some(text) = arg_opt(args, "text") {
            body.push_str(text.trim_start_matches('\n'));
            if !body.ends_with('\n') {
                body.push('\n');
            }
        }
        body
    };
    let path = dir.join(format!("{name}.md"));
    if let Err(message) = files::write(&path, &body).map_err(disk) {
        return Outcome::err(message);
    }
    vault.rescan();
    Outcome::ok(json!({"path": vault::rel(&vault.root, &path)}))
}

fn place(vault: &Vault, folder: Option<&str>) -> Result<std::path::PathBuf, Outcome> {
    match folder {
        None => Ok(vault.root.clone()),
        Some(folder) => vault.inside(folder, false).map_err(Outcome::err),
    }
}

fn template_path(vault: &Vault, name: &str) -> Option<std::path::PathBuf> {
    let want = name.to_lowercase();
    vault.notes.iter().find(|n| vault.is_template(n) && n.name.to_lowercase() == want).map(|n| n.path.clone())
}

pub fn edit_note(vault: &mut Vault, args: &Value) -> Outcome {
    let path = match vault_note(vault, args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let (text, revision) = match vault::read_text(&path).map_err(Outcome::err) {
        Ok(read) => read,
        Err(outcome) => return outcome,
    };
    if let Some(expected) = arg_opt(args, "revision")
        && expected != revision
    {
        return Outcome::err("the note changed since it was read; read it again and retry");
    }
    let Some(edits) = args.get("edits").and_then(|v| v.as_array()) else {
        return Outcome::err("`edits` must be a list");
    };
    let mut next = text.clone();
    for edit in edits {
        let old = edit.get("old").and_then(|v| v.as_str()).unwrap_or("");
        let new = edit.get("new").and_then(|v| v.as_str()).unwrap_or("");
        if old.is_empty() {
            return Outcome::err("an empty passage cannot be replaced");
        }
        match next.matches(old).count() {
            0 => return Outcome::err(format!("`{old}` was not found")),
            1 => next = next.replacen(old, new, 1),
            n => return Outcome::err(format!("`{old}` was found {n} times; it must occur once")),
        }
    }
    if let Err(message) = files::write(&path, &next).map_err(disk) {
        return Outcome::err(message);
    }
    let (path, renamed, reason, rewritten) = follow_title(vault, &path, &text, &next);
    let revision = vault::revision(next.as_bytes());
    Outcome::ok(json!({
        "path": vault::rel(&vault.root, &path),
        "revision": revision,
        "renamed": renamed,
        "name_kept": reason,
        "rewritten": rewritten,
    }))
}

/// A changed `# title` renames the file and the links, unless the new name is taken:
/// the text is kept and the file keeps its name, as the app does.
fn follow_title(vault: &mut Vault, path: &Path, before: &str, after: &str) -> (std::path::PathBuf, bool, Option<String>, usize) {
    let was = files::h1_of(before);
    let now = files::h1_of(after);
    let Some(title) = now.filter(|title| was.as_ref() != Some(title)) else {
        vault.rescan();
        return (path.to_path_buf(), false, None, 0);
    };
    let old = files::stem(path);
    if title == old {
        vault.rescan();
        return (path.to_path_buf(), false, None, 0);
    }
    let dest = path.with_file_name(format!("{title}.md"));
    if dest.exists() {
        vault.rescan();
        return (path.to_path_buf(), false, Some("this name is already taken".into()), 0);
    }
    if let Err(error) = files::rename(path, &dest) {
        vault.rescan();
        return (path.to_path_buf(), false, Some(disk(error)), 0);
    }
    let rewritten = vault::relink(vault, &old, &title);
    (dest, true, None, rewritten)
}

pub fn append_to_note(vault: &mut Vault, args: &Value) -> Outcome {
    let path = match vault_note(vault, args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let text = match arg_str(args, "text") {
        Ok(text) => text,
        Err(message) => return Outcome::err(message),
    };
    let (body, revision) = match vault::read_text(&path).map_err(Outcome::err) {
        Ok(read) => read,
        Err(outcome) => return outcome,
    };
    if let Some(expected) = arg_opt(args, "revision")
        && expected != revision
    {
        return Outcome::err("the note changed since it was read; read it again and retry");
    }
    let next = match arg_opt(args, "under") {
        Some(heading) => match append_under(&body, heading, text) {
            Ok(next) => next,
            Err(message) => return Outcome::err(message),
        },
        None => {
            let mut next = body.trim_end().to_string();
            next.push_str("\n\n");
            next.push_str(text.trim());
            next.push('\n');
            next
        }
    };
    if let Err(message) = files::write(&path, &next).map_err(disk) {
        return Outcome::err(message);
    }
    vault.rescan();
    Outcome::ok(json!({"path": vault::rel(&vault.root, &path), "revision": vault::revision(next.as_bytes())}))
}

fn append_under(text: &str, heading: &str, extra: &str) -> Result<String, String> {
    let heads = markdown::headings(text);
    let at = heads.iter().position(|(_, label, _)| *label == heading).or_else(|| {
        let want = heading.to_lowercase();
        let found: Vec<usize> = heads.iter().enumerate().filter(|(_, (_, label, _))| label.to_lowercase() == want).map(|(i, _)| i).collect();
        (found.len() == 1).then(|| found[0])
    });
    let Some(at) = at else {
        return Err(format!("no heading `{heading}`"));
    };
    let level = heads[at].0;
    let end = heads[at + 1..].iter().find(|(next, _, _)| *next <= level).map(|(_, _, start)| *start).unwrap_or(text.len());
    let mut out = text[..end].trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(extra.trim());
    out.push('\n');
    if end < text.len() {
        if !text[end..].starts_with('\n') {
            out.push('\n');
        }
        out.push_str(&text[end..]);
    }
    Ok(out)
}

pub fn rename_note(vault: &mut Vault, args: &Value) -> Outcome {
    let path = match vault.find_file(&match arg_str(args, "note") {
        Ok(note) => note.to_string(),
        Err(message) => return Outcome::err(message),
    }) {
        Ok(path) => path,
        Err(message) => return Outcome::err(message),
    };
    // find_file borrows vault. Clone before mut.
    let path = path;
    let old_file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let old_stem = files::stem(&path);
    let image = files::is_image(&path) || files::is_attachment(&path) || vault.diagrams.iter().any(|p| p == &path);
    let name = arg_opt(args, "name").map(|n| n.to_string());
    let folder = arg_opt(args, "folder").map(|n| n.to_string());
    if name.is_none() && folder.is_none() {
        return Outcome::err("`name` or `folder` is missing");
    }
    let dir = match place(vault, folder.as_deref()) {
        Ok(dir) => dir,
        Err(outcome) => return outcome,
    };
    if let Err(message) = fs::create_dir_all(&dir).map_err(disk) {
        return Outcome::err(message);
    }
    let file_name = if let Some(name) = &name {
        if image {
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("svg");
            let Some(clean) = files::clean_name(name) else {
                return Outcome::err("invalid name");
            };
            format!("{clean}.{extension}")
        } else if path.extension().is_some_and(|e| e == "md") {
            let Some(clean) = files::clean_name(name) else {
                return Outcome::err("invalid name");
            };
            format!("{clean}.md")
        } else {
            return Outcome::err("this file cannot be renamed");
        }
    } else {
        old_file.clone()
    };
    let dest = dir.join(&file_name);
    if dest != path && dest.exists() {
        return Outcome::err(format!("a file already has the name `{}` ({})", files::stem(&dest), vault::rel(&vault.root, &dest)));
    }
    if dest != path
        && let Err(message) = files::rename(&path, &dest).map_err(disk)
    {
        return Outcome::err(message);
    }
    let new_stem = files::stem(&dest);
    let (old_link, new_link) = if image || !dest.extension().is_some_and(|e| e == "md") {
        (old_file, dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(new_stem.clone()))
    } else {
        (old_stem.clone(), new_stem.clone())
    };
    // A note whose title was its name keeps the title in step.
    if dest.extension().is_some_and(|e| e == "md") && old_stem != new_stem {
        if let Ok(text) = fs::read_to_string(&dest)
            && files::h1_of(&text).as_deref() == Some(old_stem.as_str())
        {
            let meta = markdown::front_matter(&text);
            let (head, body) = text.split_at(meta);
            if let Some(first) = body.lines().find(|l| !l.trim().is_empty()) {
                let hashes = first.trim_start().chars().take_while(|c| *c == '#').count();
                let title = format!("{} {new_stem}", "#".repeat(hashes.max(1)));
                let _ = files::write(&dest, &format!("{head}{}", body.replacen(first, &title, 1)));
            }
        }
    }
    let rewritten = if old_link != new_link { vault::relink(vault, &old_link, &new_link) } else { vault.rescan(); 0 };
    let left = old_link != new_link && rewritten == 0 && vault.notes.iter().any(|n| n.name.eq_ignore_ascii_case(&old_link));
    Outcome::ok(json!({
        "path": vault::rel(&vault.root, &dest),
        "rewritten": rewritten,
        "links_left": left,
    }))
}

pub fn trash_note(vault: &mut Vault, args: &Value) -> Outcome {
    let path = match vault.find_file(&match arg_str(args, "note") {
        Ok(note) => note.to_string(),
        Err(message) => return Outcome::err(message),
    }) {
        Ok(path) => path,
        Err(message) => return Outcome::err(message),
    };
    match files::trash(&vault.root, &path).map_err(disk) {
        Ok(trashed) => {
            let path = vault::rel(&vault.root, &trashed);
            vault.rescan();
            Outcome::ok(json!({"path": path}))
        }
        Err(message) => Outcome::err(message),
    }
}

pub fn restore_note(vault: &mut Vault, args: &Value) -> Outcome {
    let raw = match arg_str(args, "path") {
        Ok(path) => path,
        Err(message) => return Outcome::err(message),
    };
    let path = match vault.inside(raw, true) {
        Ok(path) => path,
        Err(message) => return Outcome::err(message),
    };
    let trash = vault.root.join(".trash");
    if !path.starts_with(&trash) {
        return Outcome::err("restore only brings a file back from .trash");
    }
    match files::restore(&vault.root, &path).map_err(disk) {
        Ok(restored) => {
            let path = vault::rel(&vault.root, &restored);
            vault.rescan();
            Outcome::ok(json!({"path": path}))
        }
        Err(message) => Outcome::err(message),
    }
}

pub fn daily_note(vault: &mut Vault, args: &Value) -> Outcome {
    let date = match arg_opt(args, "date") {
        Some(date) => match parse_date(date) {
            Ok(date) => date,
            Err(message) => return Outcome::err(message),
        },
        None => clock::today(),
    };
    let name = date_name(date);
    if vault.find_note(&name).is_err() {
        let template = daily_template(vault);
        let body = match template {
            Some(path) => {
                let text = match fs::read_to_string(&path).map_err(disk) {
                    Ok(text) => text,
                    Err(message) => return Outcome::err(message),
                };
                markdown::fill(&text, &name, &name, &time_name(clock::clock())).0
            }
            None => format!("# {name}\n\n"),
        };
        let path = vault.root.join(format!("{name}.md"));
        if path.exists() {
            return Outcome::err(format!("a note already has the name `{name}` ({})", vault::rel(&vault.root, &path)));
        }
        if let Err(message) = files::write(&path, &body).map_err(disk) {
            return Outcome::err(message);
        }
        vault.rescan();
    }
    if let Some(text) = arg_opt(args, "append") {
        return append_to_note(vault, &json!({"note": name, "text": text}));
    }
    read_note(vault, &json!({"note": name}))
}

fn parse_date(text: &str) -> Result<(i32, u32, u32), String> {
    let mut parts = text.split('-');
    let year = parts.next().and_then(|p| p.parse().ok());
    let month = parts.next().and_then(|p| p.parse().ok());
    let day = parts.next().and_then(|p| p.parse().ok());
    match (year, month, day, parts.next()) {
        (Some(year), Some(month), Some(day), None) if (1..=12).contains(&month) && (1..=31).contains(&day) => Ok((year, month, day)),
        _ => Err(format!("`{text}` is not a date (YYYY-MM-DD)")),
    }
}

fn daily_template(vault: &Vault) -> Option<std::path::PathBuf> {
    let named: Vec<(std::path::PathBuf, String)> = vault
        .notes
        .iter()
        .filter(|n| vault.is_template(n))
        .map(|n| (n.path.clone(), n.name.clone()))
        .collect();
    daily::pick(named.iter().map(|(path, stem)| (path.clone(), stem.as_str())))
}

pub fn add_comment(vault: &mut Vault, args: &Value) -> Outcome {
    let path = match vault_note(vault, args) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let passage = match arg_str(args, "passage") {
        Ok(passage) => passage.to_string(),
        Err(message) => return Outcome::err(message),
    };
    let comment = match arg_str(args, "comment") {
        Ok(comment) => comment.to_string(),
        Err(message) => return Outcome::err(message),
    };
    let (text, _) = match vault::read_text(&path).map_err(Outcome::err) {
        Ok(read) => read,
        Err(outcome) => return outcome,
    };
    let next = match comment_on(&text, &passage, &comment) {
        Ok(next) => next,
        Err(message) => return Outcome::err(message),
    };
    if let Err(message) = files::write(&path, &next).map_err(disk) {
        return Outcome::err(message);
    }
    vault.rescan();
    Outcome::ok(json!({"path": vault::rel(&vault.root, &path), "revision": vault::revision(next.as_bytes())}))
}

/// `{==passage==}{>>comment<<}`. In a table cell a `|` of the comment is written `\|`.
fn comment_on(text: &str, passage: &str, comment: &str) -> Result<String, String> {
    if passage.is_empty() {
        return Err("the passage is empty".into());
    }
    let mut at = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(passage) {
        at.push(from + found);
        from += found + passage.len().max(1);
        if at.len() > 1 {
            break;
        }
    }
    let [start] = at.as_slice() else {
        return Err(if at.is_empty() {
            format!("`{passage}` was not found")
        } else {
            format!("`{passage}` occurs more than once")
        });
    };
    let line_start = text[..*start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[*start..].find('\n').map(|i| *start + i).unwrap_or(text.len());
    let in_cell = *start + passage.len() <= line_end && markdown::is_table_line(&text[line_start..line_end]);
    let said = if in_cell { comment.replace('|', "\\|") } else { comment.to_string() };
    let mut out = String::with_capacity(text.len() + said.len() + 12);
    out.push_str(&text[..*start]);
    out.push_str("{==");
    out.push_str(passage);
    out.push_str("==}{>>");
    out.push_str(&said);
    out.push_str("<<}");
    out.push_str(&text[*start + passage.len()..]);
    Ok(out)
}

fn vault_note(vault: &Vault, args: &Value) -> Result<std::path::PathBuf, Outcome> {
    let name = arg_str(args, "note").map_err(Outcome::err)?;
    vault.find_note(name).map(|n| n.path.clone()).map_err(Outcome::err)
}

/// Backlinks with the line each one is written on. Lives next to the tool that shows them.
pub mod wiki {
    use super::*;

    pub fn backlinks_of(vault: &Vault, path: &Path) -> Vec<Value> {
        crate::logic::wiki::backlinks(&vault.notes, path)
            .into_iter()
            .map(|note| {
                let aimed = vault.notes.iter().find(|n| n.path == path);
                let line = aimed.and_then(|aimed| crate::logic::wiki::link_line(&note.body, aimed)).unwrap_or("").trim().to_string();
                json!({"path": vault::rel(&vault.root, &note.path), "title": note.name, "line": line})
            })
            .collect()
    }
}
