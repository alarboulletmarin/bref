//! UML relationships between two shapes, as presets of what an arrow already has: its two heads,
//! its dashes and a stereotype for its text. A link is not marked with one: it *is* recognised
//! from what it has (`Relation::of`), so a diagram stays a drawing, and any file made by hand
//! with the same heads and dashes is the same relationship.

use super::diagram::{Head, Link};
use crate::lang::tr;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Relation {
    /// A plain line.
    Association,
    /// An arrow at the end: the end can be navigated.
    Navigable,
    /// A cross at the end: it cannot.
    NonNavigable,
    /// A hollow diamond at the start: the whole.
    Aggregation,
    /// A full diamond at the start.
    Composition,
    /// A hollow triangle at the end, on the parent.
    Generalization,
    /// The same, dashed.
    Realization,
    /// A dashed open arrow.
    Dependency,
    Use,
    Import,
    Access,
    Include,
    Extend,
    Merge,
    /// A dotted line from a class to the middle of an association.
    AssociationClass,
    /// A ball at the end: a provided interface.
    Ball,
    /// A socket at the end: a required interface.
    Socket,
    /// A call that waits for its answer (a filled triangle), between two lifelines.
    Sync,
    /// The message that creates a lifeline (dashed, `«create»`).
    Create,
    /// The message that destroys one (`«destroy»`).
    Destroy,
    /// A message that comes from nowhere: a dot at its start.
    Found,
    /// A message that goes nowhere: a dot at its end.
    Lost,
}

impl Relation {
    pub const ALL: [Relation; 22] = [
        Relation::Association,
        Relation::Navigable,
        Relation::NonNavigable,
        Relation::Aggregation,
        Relation::Composition,
        Relation::Generalization,
        Relation::Realization,
        Relation::Dependency,
        Relation::Use,
        Relation::Import,
        Relation::Access,
        Relation::Include,
        Relation::Extend,
        Relation::Merge,
        Relation::AssociationClass,
        Relation::Ball,
        Relation::Socket,
        Relation::Sync,
        Relation::Create,
        Relation::Destroy,
        Relation::Found,
        Relation::Lost,
    ];

    /// The head at the start, the head at the end, and whether the stroke is dashed.
    pub fn look(self) -> (Head, Head, bool) {
        use Relation::*;
        match self {
            Association => (Head::None, Head::None, false),
            Navigable => (Head::None, Head::Arrow, false),
            NonNavigable => (Head::None, Head::Cross, false),
            Aggregation => (Head::Diamond, Head::None, false),
            Composition => (Head::DiamondFull, Head::None, false),
            Generalization => (Head::None, Head::Triangle, false),
            Realization => (Head::None, Head::Triangle, true),
            Dependency | Use | Import | Access | Include | Extend | Merge => (Head::None, Head::Arrow, true),
            AssociationClass => (Head::None, Head::None, true),
            Ball => (Head::None, Head::Ball, false),
            Socket => (Head::None, Head::Socket, false),
            Sync | Destroy => (Head::None, Head::TriangleFull, false),
            Create => (Head::None, Head::Arrow, true),
            Found => (Head::Dot, Head::TriangleFull, false),
            Lost => (Head::None, Head::Dot, false),
        }
    }

    /// The stereotype written on the arrow: `«use»`.
    pub fn stereotype(self) -> Option<&'static str> {
        match self {
            Relation::Use => Some("use"),
            Relation::Import => Some("import"),
            Relation::Access => Some("access"),
            Relation::Include => Some("include"),
            Relation::Extend => Some("extend"),
            Relation::Merge => Some("merge"),
            Relation::Create => Some("create"),
            Relation::Destroy => Some("destroy"),
            _ => None,
        }
    }

    /// Gives an arrow the look of this relationship; its text becomes the stereotype, unless the
    /// arrow says something else (a stereotype of another relationship is replaced).
    pub fn apply(self, link: &mut Link) {
        (link.start, link.end, link.dashed) = self.look();
        let old = Relation::stereotyped(&link.text);
        if let Some(name) = self.stereotype()
            && (link.text.trim().is_empty() || old.is_some())
        {
            link.text = format!("«{name}»");
        } else if self.stereotype().is_none() && old.is_some() {
            link.text.clear();
        }
    }

    /// The relationship whose stereotype is written in `text` (`«use»` or `<<use>>`).
    fn stereotyped(text: &str) -> Option<Relation> {
        let name = super::uml::stereotype(text)?;
        Relation::ALL.into_iter().find(|r| r.stereotype() == Some(name))
    }

    /// What an arrow is: the relationship that looks like it, if any. Several look alike (a dashed
    /// open arrow is a dependency, whatever its text; its stereotype tells which).
    pub fn of(link: &Link) -> Option<Relation> {
        let look = (link.start, link.end, link.dashed);
        let same: Vec<Relation> = Relation::ALL.into_iter().filter(|r| r.look() == look).collect();
        match Relation::stereotyped(&link.text) {
            Some(named) if same.contains(&named) => Some(named),
            _ => same.into_iter().find(|r| r.stereotype().is_none()),
        }
    }

    /// The letter that picks it in the menu of relationships.
    pub fn key(self) -> char {
        use Relation::*;
        match self {
            Association => 'a',
            Navigable => 'n',
            NonNavigable => 'x',
            Aggregation => 'g',
            Composition => 'c',
            Generalization => 'h',
            Realization => 'z',
            Dependency => 'd',
            Use => 'u',
            Import => 'i',
            Access => 's',
            Include => 'l',
            Extend => 'e',
            Merge => 'm',
            AssociationClass => 'k',
            Ball => 'b',
            Socket => 'q',
            Sync => 'f',
            Create => 'o',
            Destroy => 'w',
            Found => 'p',
            Lost => 't',
        }
    }

    pub fn name(self) -> &'static str {
        use Relation::*;
        match self {
            Association => tr("Association", "Association"),
            Navigable => tr("Navigable association (arrow)", "Association navigable (flèche)"),
            NonNavigable => tr("Non-navigable end (cross)", "Extrémité non navigable (croix)"),
            Aggregation => tr("Aggregation (hollow diamond)", "Agrégation (losange vide)"),
            Composition => tr("Composition (full diamond)", "Composition (losange plein)"),
            Generalization => tr("Generalization (hollow triangle)", "Généralisation (triangle vide)"),
            Realization => tr("Realization (dashed, hollow triangle)", "Réalisation (pointillés, triangle vide)"),
            Dependency => tr("Dependency (dashed arrow)", "Dépendance (flèche en pointillés)"),
            Use => "«use»",
            Import => "«import»",
            Access => "«access»",
            Include => "«include»",
            Extend => "«extend»",
            Merge => "«merge»",
            AssociationClass => tr("Association class (dotted line to the middle of an association)", "Classe d'association (pointillés vers le milieu d'une association)"),
            Ball => tr("Provided interface (ball)", "Interface fournie (boule)"),
            Socket => tr("Required interface (socket)", "Interface requise (prise)"),
            Sync => tr("Synchronous call (filled triangle)", "Appel synchrone (triangle plein)"),
            Create => "«create»",
            Destroy => "«destroy»",
            Found => tr("Found message (from nowhere)", "Message trouvé (venu de nulle part)"),
            Lost => tr("Lost message (to nowhere)", "Message perdu (vers nulle part)"),
        }
    }

    /// The icon of its button, or `None` for those drawn as their stereotype.
    pub fn icon(self) -> Option<&'static str> {
        use Relation::*;
        Some(match self {
            Association => "d-rel-association.svg",
            Navigable => "d-rel-navigable.svg",
            NonNavigable => "d-rel-nonnavigable.svg",
            Aggregation => "d-rel-aggregation.svg",
            Composition => "d-rel-composition.svg",
            Generalization => "d-rel-generalization.svg",
            Realization => "d-rel-realization.svg",
            Dependency => "d-rel-dependency.svg",
            AssociationClass => "d-rel-assocclass.svg",
            Ball => "d-rel-ball.svg",
            Socket => "d-rel-socket.svg",
            Sync => "d-rel-sync.svg",
            Found => "d-rel-found.svg",
            Lost => "d-rel-lost.svg",
            Use | Import | Access | Include | Extend | Merge | Create | Destroy => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::diagram::{COLORS, Diagram, End, Form, Route};

    fn drawn(relation: Relation) -> (Diagram, String) {
        let mut d = Diagram::blank();
        let a = d.add_shape(Form::Rect, 0., 0., 100., 60.);
        let b = d.add_shape(Form::Rect, 300., 0., 100., 60.);
        let id = d.add_link(End::Shape(a), End::Shape(b), Head::None, Route::Straight);
        relation.apply(d.link_mut(id).unwrap());
        let svg = d.to_svg(COLORS[0]);
        (d, svg)
    }

    #[test]
    fn every_relationship_is_drawn_and_recognised() {
        // (relationship, dashed, paths of heads, one of them filled)
        let table = [
            (Relation::Association, false, 0, false),
            (Relation::Navigable, false, 1, false),
            (Relation::NonNavigable, false, 1, false),
            (Relation::Aggregation, false, 1, false),
            (Relation::Composition, false, 1, true),
            (Relation::Generalization, false, 1, false),
            (Relation::Realization, true, 1, false),
            (Relation::Dependency, true, 1, false),
            (Relation::Use, true, 1, false),
            (Relation::Import, true, 1, false),
            (Relation::Access, true, 1, false),
            (Relation::Include, true, 1, false),
            (Relation::Extend, true, 1, false),
            (Relation::Merge, true, 1, false),
            (Relation::AssociationClass, true, 0, false),
            (Relation::Ball, false, 1, false),
            (Relation::Socket, false, 1, false),
            (Relation::Sync, false, 1, true),
            (Relation::Create, true, 1, false),
            (Relation::Destroy, false, 1, true),
            (Relation::Found, false, 2, true),
            (Relation::Lost, false, 1, true),
        ];
        assert_eq!(table.len(), Relation::ALL.len());
        for (relation, dashed, heads, full) in table {
            let (d, svg) = drawn(relation);
            // Two shapes of one path each, then the stroke, then its heads.
            let paths: Vec<&str> = svg.lines().filter(|l| l.starts_with("<path")).collect();
            assert_eq!(paths.len(), 2 + 1 + heads, "{relation:?}: {svg}");
            assert_eq!(paths[2].contains("stroke-dasharray"), dashed, "{relation:?}");
            assert_eq!(paths.iter().skip(3).any(|p| p.contains("fill=\"#7d8590\"")), full, "{relation:?}");
            // It is recognised again from the file: no mark, only what the arrow has.
            let back = Diagram::from_svg(&svg).unwrap();
            assert_eq!(Relation::of(&back.links[0]), Some(relation), "{relation:?}");
            assert_eq!(back, d);
            // Its stereotype is written on the arrow.
            match relation.stereotype() {
                Some(name) => assert!(svg.contains(&format!(">«{name}»</text>")), "{relation:?}"),
                None => assert!(!svg.contains("<text") || !svg.contains("«"), "{relation:?}"),
            }
        }
    }

    #[test]
    fn the_shapes_of_the_heads() {
        // A ball is a closed circle of 25 points; a socket an open half circle; a cross two strokes.
        let (_, ball) = drawn(Relation::Ball);
        let line = ball.lines().filter(|l| l.starts_with("<path")).nth(3).unwrap();
        assert!(line.ends_with("Z\" stroke=\"#7d8590\"/>") && line.matches('L').count() == 24, "{line}");
        let (_, socket) = drawn(Relation::Socket);
        let line = socket.lines().filter(|l| l.starts_with("<path")).nth(3).unwrap();
        assert!(!line.contains('Z') && line.matches('L').count() == 12, "{line}");
        let (_, cross) = drawn(Relation::NonNavigable);
        let line = cross.lines().filter(|l| l.starts_with("<path")).nth(3).unwrap();
        assert!(line.matches('M').count() == 2 && line.matches('L').count() == 2, "{line}");
        // The stroke stops at the ball's edge and at the socket's apex.
        assert!(ball.contains("M100 30L288 30\"") && socket.contains("M100 30L292 30\""), "{ball}\n{socket}");
    }

    #[test]
    fn applying_changes_the_stereotype_with_the_relationship() {
        let mut d = Diagram::default();
        let id = d.add_link(End::Point(0., 0.), End::Point(100., 0.), Head::None, Route::Straight);
        let link = d.link_mut(id).unwrap();
        Relation::Use.apply(link);
        assert_eq!(link.text, "«use»");
        // Another dependency replaces it; a relationship with none removes it.
        Relation::Import.apply(link);
        assert_eq!(link.text, "«import»");
        Relation::Generalization.apply(link);
        assert_eq!((link.text.as_str(), link.dashed), ("", false));
        // What the arrow says itself stays.
        link.text = "calls".into();
        Relation::Dependency.apply(link);
        assert_eq!(link.text, "calls");
        Relation::Extend.apply(link);
        assert_eq!(link.text, "calls");
        assert_eq!(Relation::of(link), Some(Relation::Dependency));
        // A stereotype typed as <<use>> is the same as «use».
        link.text = "<<use>>".into();
        assert_eq!(Relation::of(link), Some(Relation::Use));
        // An arrow of no relationship: both heads, or a head on the start.
        (link.start, link.end, link.dashed) = (Head::Arrow, Head::Arrow, false);
        assert_eq!(Relation::of(link), None);
    }

    #[test]
    fn each_has_its_own_key_and_a_button() {
        let mut keys: Vec<char> = Relation::ALL.iter().map(|r| r.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), Relation::ALL.len());
        // Those with no icon are drawn as their stereotype.
        assert!(Relation::ALL.iter().all(|r| r.icon().is_some() || r.stereotype().is_some()));
    }
}
