//! How the palette ranks a search: a name first (a substring, then a subsequence), then a note
//! whose text holds every word. Pure, so an agent searches the vault the way the palette does.

/// The words of a search, each ready to be found in a text whatever its case: the notes are
/// searched as written, and no lowercase copy of the vault is kept for that.
#[derive(Default)]
pub struct Words(Vec<Word>);

/// One word, in lowercase.
enum Word {
    /// Its bytes, and for each the bit that tells a capital from its small letter (`0x20`, or 0
    /// where the byte has one form only): `text | fold == lower` says the two are the same letter.
    /// That holds for the letters of ASCII and of Latin-1 (é, à, ç, ñ, ü…), whose two forms are
    /// as long and differ by that bit alone. `rare` is where its two rarest bytes are: the pair
    /// looked for first (twice the same place in a word of one byte).
    Bytes { lower: Box<[u8]>, fold: Box<[u8]>, rare: (usize, usize) },
    /// A word with letters of another alphabet (Greek, Cyrillic, œ, ß…), whose capitals follow no
    /// such rule.
    // ponytail: a pattern costs about a microsecond per note before it reads anything; fold these
    // alphabets by hand too if vaults written in them turn out slow to search.
    Pattern(regex::Regex),
}

impl Words {
    /// The words of `query`, split at its spaces.
    pub fn of(query: &str) -> Words {
        Words(query.split_whitespace().filter_map(|word| Word::of(&word.to_lowercase())).collect())
    }

    /// Every word appears in the text.
    pub fn all_in(&self, text: &str) -> bool {
        self.0.iter().all(|word| word.find(text).is_some())
    }

    /// The note's line where the first word appears, as written, recentred on the word and shortened.
    pub fn snippet(&self, body: &str) -> String {
        let Some(at) = self.0.first().and_then(|word| word.find(body)) else {
            return String::new();
        };
        let from = body[..at].rfind('\n').map_or(0, |i| i + 1);
        let line = body[from..].lines().next().unwrap_or_default();
        let col = body[from..at].chars().count();
        let chars: Vec<char> = line.chars().collect();
        let start = col.saturating_sub(24);
        let part: String = chars.iter().skip(start).take(80).collect();
        format!(
            "{}{}{}",
            if start > 0 { "…" } else { "" },
            part.trim(),
            if start + 80 < chars.len() { "…" } else { "" }
        )
    }
}

impl Word {
    /// `word` is in lowercase.
    fn of(word: &str) -> Option<Word> {
        let mut fold = Vec::with_capacity(word.len());
        for c in word.chars() {
            match c {
                'a'..='z' => fold.push(0x20),
                // à … þ but ÷: the capital is the same two bytes, the second without that bit.
                'à'..='þ' if c != '÷' => fold.extend([0, 0x20]),
                // A capital written otherwise: left to a pattern, which knows every alphabet.
                _ if c.to_uppercase().ne(std::iter::once(c)) => {
                    return regex::RegexBuilder::new(&regex::escape(word)).case_insensitive(true).build().ok().map(Word::Pattern);
                }
                _ => fold.extend(std::iter::repeat_n(0, c.len_utf8())),
            }
        }
        let rarest = |but: Option<usize>| word.bytes().enumerate().filter(|&(at, _)| Some(at) != but).max_by_key(|&(_, byte)| rarity(byte)).map(|(at, _)| at);
        let one = rarest(None)?;
        Some(Word::Bytes { lower: word.as_bytes().into(), fold: fold.into(), rare: (one, rarest(Some(one)).unwrap_or(one)) })
    }

    /// Where the word first appears in the text, whatever its case.
    fn find(&self, text: &str) -> Option<usize> {
        match self {
            Word::Bytes { lower, fold, rare } => find_folded(text.as_bytes(), lower, fold, *rare),
            Word::Pattern(pattern) => pattern.find(text).map(|found| found.start()),
        }
    }
}

/// How rarely a byte shows up in notes: the rarer the bytes looked for first, the fewer places to check.
fn rarity(byte: u8) -> usize {
    // From the most common down; a byte of an accented letter sits with the middle letters, and
    // what is not listed (digits, signs) is rarer than any letter.
    const COMMON: &[u8] = b" \neasitnrulodcmp\xC3vqfbghjxyzwk";
    match byte {
        0x80.. => COMMON.iter().position(|&c| c == 0xC3).unwrap_or(0),
        _ => COMMON.iter().position(|&c| c == byte).unwrap_or(COMMON.len()),
    }
}

/// The first place where `lower` is written in `text`, a byte counting as the same when it only
/// differs by its bit of `fold`. The place is the start of a character: the first byte of a word
/// matches an ASCII byte or the same first byte of a character, never the middle of one.
fn find_folded(text: &[u8], lower: &[u8], fold: &[u8], (one, two): (usize, usize)) -> Option<usize> {
    // As many places as the word fits at.
    let places = (text.len() + 1).checked_sub(lower.len())?;
    let whole = |at: usize| text[at..at + lower.len()].iter().zip(lower.iter().zip(fold)).all(|(&byte, (&low, &bit))| byte | bit == low);
    // The word's two rare bytes, each read at its own distance from the place tried: a place is
    // worth comparing whole only where both are right.
    let (ones, twos) = (&text[one..one + places], &text[two..two + places]);
    let (low_one, bit_one, low_two, bit_two) = (lower[one], fold[one], lower[two], fold[two]);
    let rare = |x: u8, y: u8| (x | bit_one == low_one) & (y | bit_two == low_two);
    // By blocks: a loop without a way out, which the compiler turns into comparisons of 16 bytes at
    // once. The places of a block are tried one by one only when it holds a pair.
    const BLOCK: usize = 32;
    let (mut blocks_one, mut blocks_two) = (ones.chunks_exact(BLOCK), twos.chunks_exact(BLOCK));
    let mut at = 0;
    for (xs, ys) in blocks_one.by_ref().zip(blocks_two.by_ref()) {
        if xs.iter().zip(ys).fold(false, |any, (&x, &y)| any | rare(x, y))
            && let Some(found) = (0..BLOCK).find(|&i| rare(xs[i], ys[i]) && whole(at + i))
        {
            return Some(at + found);
        }
        at += BLOCK;
    }
    let (xs, ys) = (blocks_one.remainder(), blocks_two.remainder());
    (0..xs.len()).find(|&i| rare(xs[i], ys[i]) && whole(at + i)).map(|found| at + found)
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
        let text = "# Courses\n\n- Lait et PÂTES au Citron\n";
        assert!(Words::of("pâtes citron").all_in(text));
        assert!(Words::of("Pâtes CITRON").all_in(text));
        assert!(!Words::of("pâtes beurre").all_in(text));
        // A sign is itself, and only itself.
        assert!(Words::of("c++ (a|b)").all_in("du C++ et (a|b)"));
        assert!(!Words::of("a|b").all_in("a ou b"));
        assert!(!Words::of("a{b").all_in("A[B") && !Words::of("a@b").all_in("a`b") && !Words::of("÷").all_in("×"));
        // No word: nothing to find, nothing to show.
        assert!(Words::of("  ").all_in(text) && Words::of("  ").snippet(text).is_empty());
        // Longer than the text, at its very start, at its very end.
        assert!(!Words::of("citronnade").all_in("Citron"));
        assert!(Words::of("citron").all_in("CITRON") && Words::of("à").all_in("À") && Words::of("fin").all_in("à la FIN"));
    }

    #[test]
    fn finds_words_of_any_alphabet() {
        // No capitals: found as written. Capitals of another kind: found all the same.
        assert!(Words::of("日本").all_in("これは日本語です") && !Words::of("日本").all_in("これは本日です"));
        assert!(Words::of("ωμέγα").all_in("Το ΩΜΈΓΑ") && Words::of("привет").all_in("ПРИВЕТ, мир"));
        assert!(Words::of("œuvre").all_in("Une ŒUVRE") && Words::of("straße").all_in("Die Straße"));
        assert_eq!(Words::of("мир").snippet("Раз\nПривет, МИР\n"), "Привет, МИР");
    }

    /// The search by eights agrees with the plain one, on texts where lowering keeps every length.
    #[test]
    fn folded_search_matches_a_lowered_copy() {
        let letters: Vec<char> = "aAbBeEéÉàÀçÇzZ ÷×@`[{\n-7tTnN".chars().collect();
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = |below: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % below as u64) as usize
        };
        let (mut hits, mut misses) = (0, 0);
        for _ in 0..20_000 {
            let text: String = (0..next(40)).map(|_| letters[next(letters.len())]).collect();
            let word: String = (0..1 + next(4)).map(|_| letters[next(letters.len())]).filter(|c| !c.is_whitespace()).collect();
            if word.is_empty() {
                continue;
            }
            let found = Words::of(&word).0.first().and_then(|w| w.find(&text));
            assert_eq!(found, text.to_lowercase().find(&word.to_lowercase()), "{word:?} in {text:?}");
            *if found.is_some() { &mut hits } else { &mut misses } += 1;
        }
        assert!(hits > 2_000 && misses > 2_000, "{hits} found, {misses} not");
    }

    #[test]
    fn snippet_keeps_the_case_and_trims() {
        let body = "# Courses\n\n- Lait et Pâtes au Citron\n";
        assert_eq!(Words::of("pâtes").snippet(body), "- Lait et Pâtes au Citron");
        // The first word decides the line, at its first place in the note.
        assert_eq!(Words::of("lait cru").snippet("Rien\nLe LAIT cru\net du lait\n"), "Le LAIT cru");
        assert_eq!(Words::of("beurre").snippet(body), "");
        let long = format!("{}Citron{}", "a".repeat(100), "b".repeat(100));
        let cut = Words::of("citron").snippet(&long);
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
