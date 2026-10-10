//! Raccourcis : ce que le panneau d'aide montre et cherche (fonctions pures).

/// Sections du panneau d'aide : (titre, [(touches, effet)]).
pub type Sections = Vec<(&'static str, Vec<(String, &'static str)>)>;

/// Texte ramené à ce que la recherche compare : minuscules, sans accents.
// ponytail: les seuls accents du français ; passer à une normalisation Unicode si une autre
// langue arrive dans `tr`.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' => out.push('i'),
            'ô' | 'ö' => out.push('o'),
            'ù' | 'û' | 'ü' => out.push('u'),
            'ç' => out.push('c'),
            'œ' => out.push_str("oe"),
            '’' => out.push('\''),
            c => out.push(c),
        }
    }
    out
}

/// Les lignes dont les touches ou l'effet contiennent chaque mot cherché ; une section vide
/// disparaît.
pub fn filter(sections: Sections, query: &str) -> Sections {
    let query = fold(query);
    let words: Vec<&str> = query.split_whitespace().collect();
    sections
        .into_iter()
        .filter_map(|(title, mut rows)| {
            rows.retain(|(keys, effect)| {
                let row = fold(&format!("{keys} {effect}"));
                words.iter().all(|word| row.contains(word))
            });
            (!rows.is_empty()).then_some((title, rows))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sections {
        vec![
            ("Notes", vec![("Ctrl+P".into(), "Chercher, créer"), ("Ctrl+Shift+F".into(), "Chercher dans le texte")]),
            ("Navigation", vec![("Ctrl+G".into(), "Graphe des notes"), ("Ctrl+clic".into(), "Sélection à étendre")]),
        ]
    }

    #[test]
    fn fold_ignores_case_and_accents() {
        assert_eq!(fold("Créer À la FRAPPE"), "creer a la frappe");
        assert_eq!(fold("cœur, Çà, où"), "coeur, ca, ou");
        assert_eq!(fold("l’aide"), "l'aide");
    }

    #[test]
    fn filter_matches_keys_and_text() {
        let all = sample();
        assert_eq!(filter(all.clone(), "  "), all);
        // Les touches, sans égard à la casse.
        assert_eq!(filter(all.clone(), "ctrl+p"), vec![("Notes", vec![("Ctrl+P".to_string(), "Chercher, créer")])]);
        // Le texte, sans les accents ; la section sans ligne disparaît.
        assert_eq!(filter(all.clone(), "selection"), vec![("Navigation", vec![("Ctrl+clic".to_string(), "Sélection à étendre")])]);
        // Chaque mot doit s'y trouver, touches et texte confondus.
        assert_eq!(filter(all.clone(), "ctrl graphe"), vec![("Navigation", vec![("Ctrl+G".to_string(), "Graphe des notes")])]);
        assert!(filter(all, "zzz").is_empty());
    }
}
