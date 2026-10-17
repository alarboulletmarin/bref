//! Completion: wikilinks, images, `/` commands and `@` dates offered under the cursor.

use super::*;

/// Choices shown at once in the completion list.
pub(super) const SHOWN: usize = 8;

pub(super) type Command = (&'static str, &'static str, &'static str, &'static str, &'static str);

pub(super) const COMMANDS: &[Command] = &[
    ("h1", "Heading 1", "Titre 1", "# ", ""),
    ("h2", "Heading 2", "Titre 2", "## ", ""),
    ("h3", "Heading 3", "Titre 3", "### ", ""),
    ("list", "Bullet list", "Liste à puces", "- ", ""),
    ("num", "Numbered list", "Liste numérotée", "1. ", ""),
    ("todo", "Task", "Tâche à cocher", "- [ ] ", ""),
    ("table", "Table", "Tableau", "", ""),
    ("date", "Today's date", "Date du jour", "", ""),
    ("meta", "Front matter: tags, aliases", "En-tête : tags, alias", "", ""),
    ("note", "Panel: note", "Panneau : note", "> [!NOTE]\n> ", ""),
    ("tip", "Panel: tip", "Panneau : astuce", "> [!TIP]\n> ", ""),
    ("important", "Panel: important", "Panneau : important", "> [!IMPORTANT]\n> ", ""),
    ("warning", "Panel: warning", "Panneau : attention", "> [!WARNING]\n> ", ""),
    ("caution", "Panel: danger", "Panneau : danger", "> [!CAUTION]\n> ", ""),
    ("quote", "Quote", "Citation", "> ", ""),
    ("code", "Code block", "Bloc de code", "```", "\n\n```"),
    ("mermaid", "Mermaid diagram", "Diagramme Mermaid", "```mermaid\n", "\n```"),
    ("math", "Formula", "Formule", "$$\n", "\n$$"),
    ("rule", "Divider", "Séparateur", "---\n", ""),
    ("link", "Link to a note", "Lien vers une note", "[[", ""),
    ("image", "Picture or diagram", "Image ou schéma", "![[", ""),
];

/// What completion offers: a name to put after `[[`, or a command.
#[derive(Clone, Copy)]
pub(super) enum Choice<'a> {
    Name(&'a str),
    Command(&'static Command),
        /// Day offered after `@`: its English and French name, its date.
    Date(&'static str, &'static str, md::Date),
}

impl Editor {
        /// Corner of a floating panel of size `panel`: below the cursor, or
        /// above if it does not fit.
    pub(super) fn popup_at(&self, panel: Size<Pixels>) -> Option<Point<Pixels>> {
        let c = self.cursor();
        let row = &self.rows[self.row_at(c)?];
        let p = row.pos(c - row.start);
        let top = self.origin.y + row.y + row.pad + p.y;
        let (below, above) = (top + row.lh + px(4.), top - panel.height - px(4.));
        let fits = below + panel.height <= self.viewport.bottom() - FOOTER || above < self.viewport.top();
        let x = (self.origin.x + p.x).min(self.viewport.right() - panel.width - px(8.));
        Some(point(x.max(self.viewport.left()), if fits { below } else { above }))
    }

    /// Start of the query (after `[[` or `/`) and what matches it.
    pub(super) fn completion(&self) -> Option<(usize, Vec<Choice<'_>>)> {
        if !self.sel.is_empty() || !self.more.is_empty() {
            return None;
        }
        let c = self.cursor();
        let line = self.line_range(c).start;
        let before = &self.content[line..c];
        let (start, items) = self.names(before, c).or_else(|| self.commands(before, c, line)).or_else(|| self.days(before, c, line))?;
        (!items.is_empty() && self.ac_dismissed != Some(start)).then_some((start, items))
    }

    /// After `[[`: the vault's notes; after `![[`: its images and diagrams.
    pub(super) fn names(&self, before: &str, c: usize) -> Option<(usize, Vec<Choice<'_>>)> {
        let open = before.rfind("[[")?;
        let query = &before[open + 2..];
        if query.contains("]]") || query.contains('|') {
            return None;
        }
        let start = c - query.len();
        let query = query.to_lowercase();
                // `![[` displays a file: the vault's images and diagrams are
                // offered, by file name; `[[` offers notes.
        let embed = before[..open].ends_with('!');
        let mut items: Vec<&str> = match embed {
            true => self.images.values().filter_map(|p| p.file_name()?.to_str()).collect(),
            false => self.notes.iter().map(String::as_str).collect(),
        };
        items.retain(|name| name.to_lowercase().contains(&query));
        if embed {
            items.sort_unstable_by_key(|name| name.to_lowercase());
        }
        items.truncate(6);
        Some((start, items.into_iter().map(Choice::Name).collect()))
    }

        /// After a `/` at the start of a line or word, outside code: the commands whose
        /// name starts with the query, then those whose label contains it.
    pub(super) fn commands(&self, before: &str, c: usize, line: usize) -> Option<(usize, Vec<Choice<'_>>)> {
        let slash = before.rfind('/')?;
        let query = before[slash + 1..].to_lowercase();
        let starts_word = before[..slash].chars().next_back().is_none_or(char::is_whitespace);
        if !starts_word || !query.chars().all(char::is_alphanumeric) || self.in_code(line) {
            return None;
        }
        let mut items: Vec<&'static Command> = COMMANDS
            .iter()
            .filter(|(name, en, fr, ..)| name.starts_with(&query) || tr(en, fr).to_lowercase().contains(&query))
            .collect();
        items.sort_by_key(|(name, ..)| !name.starts_with(&query));
        Some((c - query.len(), items.into_iter().map(Choice::Command).collect()))
    }

        /// After an `@` at the start of a line or word, outside code: the days the rest names
        /// (`@demain`, `@lundi`, `@2026-10-09`). An address `name@domain` opens nothing.
    pub(super) fn days(&self, before: &str, c: usize, line: usize) -> Option<(usize, Vec<Choice<'_>>)> {
        let at = before.rfind('@')?;
        let query = &before[at + 1..];
        let starts_word = before[..at].chars().next_back().is_none_or(char::is_whitespace);
        if !starts_word || !query.chars().all(|ch| ch.is_alphanumeric() || matches!(ch, '-' | '\'' | '’')) || self.in_code(line) {
            return None;
        }
        let items = md::dates(query, crate::platform::clock::today()).into_iter().map(|(en, fr, date)| Choice::Date(en, fr, date));
        Some((c - query.len(), items.collect()))
    }

    pub(super) fn accept_completion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, items)) = self.completion() else {
            return false;
        };
        let c = self.cursor();
        match items[self.ac_index.min(items.len() - 1)] {
            Choice::Name(name) => {
                let name = name.to_string();
                let closed = self.content[c..].starts_with("]]");
                self.edit(start..c, &format!("{name}]]"), cx);
                if closed {
                    let end = self.cursor();
                    self.splice(end..end + 2, "", cx);
                }
            }
                        // The day becomes a link to its daily note, created when first opened.
            Choice::Date(_, _, date) => {
                self.push_undo(true);
                self.edit(start - 1..c, &format!("[[{}]]", crate::lang::date_name(date)), cx);
            }
            Choice::Command(&(name, _, _, before, after)) => {
                let slash = start - 1;
                self.push_undo(true);
                if name == "table" {
                    self.edit(slash..c, "", cx);
                    self.grid = Some((3, 3));
                    self.grid_typed.clear();
                    return true;
                }
                                // The front matter goes at the top of the note, wherever it is asked for; if it is already there,
                                // the cursor goes to it. Either way it waits between the tag brackets.
                if name == "meta" {
                    self.edit(slash..c, "", cx);
                    if md::front_matter(&self.content) == 0 {
                        self.edit(0..0, "---\ntags: []\naliases: []\n---\n", cx);
                    }
                    let block = &self.content[..md::front_matter(&self.content)];
                    let at = block.find("tags: [").map_or("---\n".len(), |i| i + "tags: [".len());
                    self.move_to(at, cx);
                    return true;
                }
                                // Today's date is written where it is asked for.
                if name == "date" {
                    self.edit(slash..c, &crate::lang::date_name(crate::platform::clock::today()), cx);
                    return true;
                }
                                // A link goes in the sentence; everything else starts its own line.
                let line = self.line_range(c);
                let inline = before.ends_with("[[") || self.content[line.start..slash].trim().is_empty();
                let lead = if inline { "" } else { "\n" };
                let tail = if after.is_empty() || c == line.end { "" } else { "\n" };
                self.edit(slash..c, &format!("{lead}{before}{after}{tail}"), cx);
                self.move_to(slash + lead.len() + before.len(), cx);
            }
        }
        true
    }
}
