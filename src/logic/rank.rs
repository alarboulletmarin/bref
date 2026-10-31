//! How the palette ranks a search: a name first (a substring, then a subsequence), then a note
//! whose text holds every word. Pure, so an agent searches the vault the way the palette does.

/// All the words appear in the text (already lowercase).
pub fn contains_all(lower: &str, words: &[&str]) -> bool {
    words.iter().all(|w| lower.contains(w))
}

/// The note's line where `word` (lowercase) appears, with its original case,
/// recentred on the word and shortened.
pub fn snippet(body: &str, lower: &str, word: &str) -> String {
    let Some(at) = lower.find(word) else {
        return String::new();
    };
    snippet_at(body, lower, lower[..at].matches('\n').count(), word)
}

/// The same, for the line at index `row`.
pub fn snippet_at(body: &str, lower: &str, row: usize, word: &str) -> String {
    let (Some(shown), Some(low)) = (body.lines().nth(row), lower.lines().nth(row)) else {
        return String::new();
    };
    let col = low.find(word).map_or(0, |i| low[..i].chars().count());
    let chars: Vec<char> = shown.chars().collect();
    let start = col.saturating_sub(24);
    let part: String = chars.iter().skip(start).take(80).collect();
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        part.trim(),
        if start + 80 < chars.len() { "…" } else { "" }
    )
}

/// The text in lowercase, without its accents: « thème » is found by typing « theme ».
pub fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Match score: substring first, otherwise subsequence. A higher score is a better name.
pub fn fuzzy(query: &str, name: &str) -> Option<i32> {
    if let Some(i) = name.find(query) {
        return Some(1000 - i as i32);
    }
    let mut chars = name.chars();
    for c in query.chars() {
        chars.find(|&x| x == c)?;
    }
    Some(-(name.len() as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_text_with_all_the_words() {
        let lower = "# courses\n\n- lait et pâtes au citron\n";
        assert!(contains_all(lower, &["pâtes", "citron"]));
        assert!(!contains_all(lower, &["pâtes", "beurre"]));
    }

    #[test]
    fn lists_each_line_once() {
        let body = "Lait, lait\n\nrien\nau LAIT cru\n";
        let lower = body.to_lowercase();
        assert_eq!(snippet_at(body, &lower, 3, "lait"), "au LAIT cru");
    }

    #[test]
    fn snippet_keeps_the_case_and_trims() {
        let body = "# Courses\n\n- Lait et Pâtes au Citron\n";
        assert_eq!(
            snippet(body, &body.to_lowercase(), "pâtes"),
            "- Lait et Pâtes au Citron"
        );
        let long = format!("{}Citron{}", "a".repeat(100), "b".repeat(100));
        let cut = snippet(&long, &long.to_lowercase(), "citron");
        assert!(cut.starts_with('…') && cut.ends_with('…') && cut.contains("Citron"));
    }

    #[test]
    fn ranks_matches() {
        assert!(fuzzy("cou", "courses") > fuzzy("cou", "les courses"));
        assert!(fuzzy("crs", "courses").is_some());
        assert!(fuzzy("crs", "courses") < fuzzy("ours", "courses"));
        assert_eq!(fuzzy("xyz", "courses"), None);
        assert!(fuzzy("", "courses").is_some());
    }
}
