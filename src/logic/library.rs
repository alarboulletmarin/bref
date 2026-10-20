//! The library of the diagram canvas: the shapes of each family of UML diagrams, ready to drop with
//! their text and size, and the starting points (a new diagram that already holds a few of them).
//! Pure: the canvas shows them, this says what they are.

use super::{
    diagram::{Diagram, End, Form, Head, Route},
    relation::Relation,
};
use crate::lang::{fold, tr};

/// A shape of the library: what a click or a drop on the canvas makes.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub family: &'static str,
    pub name: &'static str,
    /// The icon of its row, and of what is dragged.
    pub icon: &'static str,
    pub form: Form,
    pub w: f32,
    pub h: f32,
    /// What is written in it to begin with.
    pub text: &'static str,
    pub fill: bool,
    pub dashed: bool,
}

impl Preset {
    /// Whether this shape answers `query`: every word of it is in its name or its family.
    pub fn matches(&self, query: &str) -> bool {
        let words = fold(query);
        let (name, family) = (fold(self.name), fold(self.family));
        words.split_whitespace().all(|w| name.contains(w) || family.contains(w))
    }
}

impl Template {
    /// Whether this starting point answers `query`: every word of it is in its name.
    pub fn matches(&self, query: &str) -> bool {
        let name = fold(self.name);
        fold(query).split_whitespace().all(|w| name.contains(w))
    }
}

/// The shapes, family by family.
pub fn presets() -> Vec<Preset> {
    let mut all = shapes();
    // The border of a region is dashed.
    for preset in &mut all {
        preset.dashed |= preset.form == Form::Divider;
    }
    all
}

fn shapes() -> Vec<Preset> {
    let class = tr("Class", "Classe");
    let state = tr("State machine", "Machine à états");
    let sequence = tr("Sequence", "Séquence");
    let activity = tr("Activity", "Activité");
    let p = |family: &'static str, name: &'static str, icon: &'static str, form: Form, (w, h): (f32, f32), text: &'static str| Preset { family, name, icon, form, w, h, text, fill: false, dashed: false };
    vec![
        p(class, tr("Class", "Classe"), "d-rect.svg", Form::Rect, (160., 120.), tr("ClassName\n---\n+ attribute: Type\n---\n+ operation()", "NomDeClasse\n---\n+ attribut: Type\n---\n+ operation()")),
        p(class, tr("Abstract class", "Classe abstraite"), "d-rect.svg", Form::Rect, (160., 120.), tr("«abstract»\nClassName\n---\n+ attribute: Type\n---\n+ abstract operation()", "«abstract»\nNomDeClasse\n---\n+ attribut: Type\n---\n+ abstract operation()")),
        p(class, tr("Interface", "Interface"), "d-rect.svg", Form::Rect, (160., 90.), tr("«interface»\nInterfaceName\n---\n+ operation()", "«interface»\nNomDInterface\n---\n+ operation()")),
        p(class, tr("Enumeration", "Énumération"), "d-rect.svg", Form::Rect, (140., 110.), tr("«enumeration»\nName\n---\nFIRST\nSECOND", "«enumeration»\nNom\n---\nPREMIER\nSECOND")),
        p(class, tr("Object (instance)", "Objet (instance)"), "d-rect.svg", Form::Rect, (160., 80.), tr("name : ClassName\n---\nslot = value", "nom : NomDeClasse\n---\nattribut = valeur")),
        p(class, tr("Package", "Paquetage"), "d-package.svg", Form::Package, (220., 150.), tr("package", "paquetage")),
        p(class, tr("Note", "Note"), "d-note.svg", Form::Note, (140., 60.), ""),
        p(class, tr("N-ary association", "Association n-aire"), "d-rel-nary.svg", Form::Branch, (40., 40.), ""),
        p(tr("Use case", "Cas d'utilisation"), tr("Use case", "Cas d'utilisation"), "d-ellipse.svg", Form::Ellipse, (160., 80.), tr("Use case", "Cas d'utilisation")),
        p(tr("Use case", "Cas d'utilisation"), tr("Actor", "Acteur"), "d-actor.svg", Form::Actor, (40., 80.), tr("Actor", "Acteur")),
        p(tr("Use case", "Cas d'utilisation"), tr("System boundary", "Frontière du système"), "d-frame.svg", Form::Frame, (320., 220.), tr("System", "Système")),
        p(state, tr("State", "État"), "d-round.svg", Form::Round, (140., 90.), tr("State\n---\nentry / action\ndo / activity\nexit / action", "État\n---\nentry / action\ndo / activité\nexit / action")),
        p(state, tr("Composite state", "État composite"), "d-st-composite.svg", Form::Composite, (300., 200.), tr("Composite state", "État composite")),
        p(state, tr("Initial state", "État initial"), "d-st-dot.svg", Form::Dot, (20., 20.), ""),
        p(state, tr("Final state", "État final"), "d-st-final.svg", Form::Final, (26., 26.), ""),
        p(state, tr("Choice", "Choix"), "d-diamond.svg", Form::Branch, (30., 30.), ""),
        p(state, tr("Junction", "Jonction"), "d-st-dot.svg", Form::Dot, (12., 12.), ""),
        p(state, tr("Fork or join", "Fourche ou jointure"), "d-st-bar.svg", Form::Bar, (90., 8.), ""),
        p(state, tr("Shallow history", "Historique simple"), "d-ellipse.svg", Form::Ellipse, (30., 30.), "H"),
        p(state, tr("Deep history", "Historique profond"), "d-ellipse.svg", Form::Ellipse, (30., 30.), "H*"),
        p(state, tr("Terminate", "Terminaison"), "d-st-terminate.svg", Form::Terminate, (24., 24.), ""),
        p(state, tr("Entry point", "Point d'entrée"), "d-ellipse.svg", Form::Ellipse, (16., 16.), ""),
        p(state, tr("Exit point", "Point de sortie"), "d-st-exit.svg", Form::Exit, (16., 16.), ""),
        p(state, tr("Region divider (dashed)", "Séparateur de régions (pointillés)"), "d-dash.svg", Form::Divider, (300., 10.), ""),
        p(activity, tr("Action", "Action"), "d-round.svg", Form::Round, (140., 50.), tr("Action", "Action")),
        p(activity, tr("Call behaviour", "Appel de comportement"), "d-act-call.svg", Form::Behavior, (140., 50.), tr("Behaviour", "Comportement")),
        p(activity, tr("Object node", "Nœud d'objet"), "d-rect.svg", Form::Rect, (110., 50.), tr("Object", "Objet")),
        p(activity, tr("Pin (on the border of an action)", "Broche (sur le bord d'une action)"), "d-act-pin.svg", Form::Pin, (14., 14.), ""),
        p(activity, tr("Initial node", "Nœud initial"), "d-st-dot.svg", Form::Dot, (20., 20.), ""),
        p(activity, tr("Activity final", "Fin d'activité"), "d-st-final.svg", Form::Final, (26., 26.), ""),
        p(activity, tr("Flow final", "Fin de flot"), "d-st-exit.svg", Form::Exit, (24., 24.), ""),
        p(activity, tr("Decision or merge", "Décision ou fusion"), "d-diamond.svg", Form::Branch, (30., 30.), ""),
        p(activity, tr("Fork or join (bar)", "Fourche ou jointure (barre)"), "d-st-bar.svg", Form::Bar, (90., 8.), ""),
        p(activity, tr("Swimlane (column)", "Couloir (colonne)"), "d-lane.svg", Form::Lane, (200., 400.), tr("Lane", "Couloir")),
        p(activity, tr("Swimlane (row)", "Couloir (ligne)"), "d-act-row.svg", Form::Row, (520., 130.), tr("Lane", "Couloir")),
        Preset { dashed: true, ..p(activity, tr("Interruptible region", "Région interruptible"), "d-act-region.svg", Form::Frame, (260., 170.), "«interruptible»") },
        p(activity, tr("Activity frame", "Cadre d'activité"), "d-frame.svg", Form::Frame, (400., 300.), tr("act Activity", "act Activité")),
        p(sequence, tr("Lifeline", "Ligne de vie"), "d-seq-life.svg", Form::Lifeline, (110., 276.), tr("name : Type", "nom : Type")),
        p(sequence, tr("Actor", "Acteur"), "d-actor.svg", Form::Actor, (40., 80.), tr("Actor", "Acteur")),
        p(sequence, tr("Fragment alt (choice)", "Fragment alt (choix)"), "d-seq-frag.svg", Form::Fragment, (300., 150.), tr("alt\n[condition]\n[else]", "alt\n[condition]\n[sinon]")),
        p(sequence, tr("Fragment opt (optional)", "Fragment opt (facultatif)"), "d-seq-frag.svg", Form::Fragment, (300., 120.), tr("opt\n[condition]", "opt\n[condition]")),
        p(sequence, tr("Fragment loop (repeat)", "Fragment loop (répétition)"), "d-seq-frag.svg", Form::Fragment, (300., 120.), tr("loop\n[while condition]", "loop\n[tant que condition]")),
        p(sequence, tr("Fragment par (parallel)", "Fragment par (parallèle)"), "d-seq-frag.svg", Form::Fragment, (300., 150.), "par\n\n"),
        p(sequence, tr("Fragment break (leave)", "Fragment break (sortie)"), "d-seq-frag.svg", Form::Fragment, (300., 120.), tr("break\n[condition]", "break\n[condition]")),
        p(sequence, tr("Fragment critical (exclusive)", "Fragment critical (exclusif)"), "d-seq-frag.svg", Form::Fragment, (300., 120.), "critical\n"),
        p(sequence, tr("Fragment neg (forbidden)", "Fragment neg (interdit)"), "d-seq-frag.svg", Form::Fragment, (300., 120.), "neg\n"),
        p(sequence, tr("Fragment ref (reference)", "Fragment ref (référence)"), "d-seq-frag.svg", Form::Fragment, (240., 70.), tr("ref\nOther diagram", "ref\nUn autre diagramme")),
    ]
}

/// A new diagram that already holds a few shapes, to start from.
pub struct Template {
    pub name: &'static str,
    pub make: fn() -> Diagram,
}

pub fn templates() -> Vec<Template> {
    vec![
        Template { name: tr("Class diagram", "Diagramme de classes"), make: class_diagram },
        Template { name: tr("Use case diagram", "Diagramme de cas d'utilisation"), make: use_case_diagram },
        Template { name: tr("Package diagram", "Diagramme de paquetages"), make: package_diagram },
        Template { name: tr("State machine diagram", "Diagramme d'états"), make: state_machine },
        Template { name: tr("Sequence diagram", "Diagramme de séquence"), make: sequence_diagram },
        Template { name: tr("Activity diagram", "Diagramme d'activité"), make: activity_diagram },
    ]
}

/// Adds the shape of the preset named `name` of this family, with its text, and returns its id.
fn put(d: &mut Diagram, family: &str, name: &str, (x, y): (f32, f32)) -> u32 {
    let preset = presets().into_iter().find(|p| p.family == family && p.name == name).unwrap_or_else(|| panic!("{family}/{name} is in the library"));
    let id = d.add_shape(preset.form, x, y, preset.w, preset.h);
    if let Some(shape) = d.shape_mut(id) {
        (shape.text, shape.fill, shape.dashed) = (preset.text.to_string(), preset.fill, preset.dashed);
    }
    id
}

fn say(d: &mut Diagram, id: u32, text: &str) {
    if let Some(shape) = d.shape_mut(id) {
        shape.text = text.to_string();
    }
}

fn class_diagram() -> Diagram {
    let mut d = Diagram::blank();
    let class = tr("Class", "Classe");
    let (order, customer) = (put(&mut d, class, class, (60., 80.)), put(&mut d, class, class, (360., 80.)));
    say(&mut d, order, tr("Order\n---\n+ total: f32\n---\n+ pay()", "Commande\n---\n+ total: f32\n---\n+ payer()"));
    say(&mut d, customer, tr("Customer\n---\n+ name: String", "Client\n---\n+ nom: String"));
    let link = d.add_link(End::Shape(order), End::Shape(customer), Head::None, Route::Straight);
    if let Some(link) = d.link_mut(link) {
        (link.start_text, link.end_text, link.text) = ("0..*".into(), "1".into(), tr("placed by", "passée par").into());
    }
    d
}

fn use_case_diagram() -> Diagram {
    let mut d = Diagram::blank();
    let family = tr("Use case", "Cas d'utilisation");
    let boundary = put(&mut d, family, tr("System boundary", "Frontière du système"), (160., 40.));
    let case = put(&mut d, family, tr("Use case", "Cas d'utilisation"), (250., 130.));
    let actor = put(&mut d, family, tr("Actor", "Acteur"), (60., 140.));
    d.set_parent(case, Some(boundary));
    d.add_link(End::Shape(actor), End::Shape(case), Head::None, Route::Straight);
    d
}

/// A traffic light: an initial state, three states in a ring, a transition with its guard and its
/// effect, and one that stays in its state.
fn state_machine() -> Diagram {
    let mut d = Diagram::blank();
    let family = tr("State machine", "Machine à états");
    let start = put(&mut d, family, tr("Initial state", "État initial"), (40., 90.));
    let state = tr("State", "État");
    let at = [(140., 60.), (420., 60.), (280., 210.)];
    let names = [tr("Red", "Rouge"), tr("Green", "Vert"), tr("Yellow", "Orange")];
    let [red, green, yellow] = [0, 1, 2].map(|i| {
        let id = put(&mut d, family, state, at[i]);
        say(&mut d, id, names[i]);
        if let Some(shape) = d.shape_mut(id) {
            shape.h = 50.;
        }
        id
    });
    let go = |d: &mut Diagram, from: End, to: End, text: &str| {
        let id = d.add_link(from, to, Head::Arrow, Route::Elbow);
        if let Some(link) = d.link_mut(id) {
            link.text = text.to_string();
        }
    };
    go(&mut d, End::Shape(start), End::Shape(red), "");
    go(&mut d, End::Shape(red), End::Shape(green), "timer [clear] / go()");
    // The way back is on the sides that leave room: out of the bottom, into the right of the last state, and out of its left.
    go(&mut d, End::Pin(green, 0.5, 1.), End::Pin(yellow, 1., 0.5), "timer");
    go(&mut d, End::Pin(yellow, 0., 0.5), End::Pin(red, 0.5, 1.), "timer");
    go(&mut d, End::Shape(green), End::Shape(green), "sensor / extend()");
    d
}

/// A login: three lifelines, a call that waits for its answer at each step, one the server makes to itself.
fn sequence_diagram() -> Diagram {
    let mut d = Diagram::blank();
    let family = tr("Sequence", "Séquence");
    let name = tr("Lifeline", "Ligne de vie");
    let names = [tr("client : Client", "client : Client"), tr("server : Server", "serveur : Serveur"), tr("db : Database", "base : Base")];
    let lives: Vec<u32> = (0..3).map(|i| put(&mut d, family, name, (60. + 220. * i as f32, 30.))).collect();
    for (id, text) in lives.iter().zip(names) {
        say(&mut d, *id, text);
    }
    let send = |d: &mut Diagram, from: usize, to: usize, relation: Relation, text: &str| {
        let id = d.add_link(End::Life(lives[from]), End::Life(lives[to]), Head::None, Route::Straight);
        if let Some(link) = d.link_mut(id) {
            relation.apply(link);
            link.text = text.to_string();
        }
    };
    send(&mut d, 0, 1, Relation::Sync, "login(user, password)");
    send(&mut d, 1, 2, Relation::Sync, "find(user)");
    send(&mut d, 2, 1, Relation::Dependency, "user");
    send(&mut d, 1, 1, Relation::Sync, tr("check(password)", "vérifier(mot de passe)"));
    send(&mut d, 1, 0, Relation::Dependency, "token");
    d.fit_sequence();
    d
}

/// An order in two swimlanes: the customer places it, the shop checks the stock and either ships it or tells the
/// customer; the order leaves the first action through a pin, and a decision cuts the flow in two with guards.
fn activity_diagram() -> Diagram {
    let mut d = Diagram::blank();
    let family = tr("Activity", "Activité");
    let lane = tr("Swimlane (column)", "Couloir (colonne)");
    let (customer, shop) = (put(&mut d, family, lane, (40., 20.)), put(&mut d, family, lane, (280., 20.)));
    say(&mut d, customer, tr("Customer", "Client"));
    say(&mut d, shop, tr("Shop", "Boutique"));
    let action = tr("Action", "Action");
    let held = |d: &mut Diagram, name: &str, at: (f32, f32), text: &str, lane: u32| {
        let id = put(d, family, name, at);
        if !text.is_empty() {
            say(d, id, text);
        }
        // An action is wide enough for its text.
        if let Some(shape) = d.shape_mut(id).filter(|s| s.form == Form::Round) {
            shape.w = 160.;
        }
        d.set_parent(id, Some(lane));
        id
    };
    let start = held(&mut d, tr("Initial node", "Nœud initial"), (140., 70.), "", customer);
    let place = held(&mut d, action, (70., 120.), tr("Place the order", "Passer la commande"), customer);
    let check = held(&mut d, action, (300., 120.), tr("Check the stock", "Vérifier le stock"), shop);
    let choice = held(&mut d, tr("Decision or merge", "Décision ou fusion"), (365., 210.), "", shop);
    let ship = held(&mut d, action, (300., 290.), tr("Ship the order", "Expédier la commande"), shop);
    let end = held(&mut d, tr("Activity final", "Fin d'activité"), (367., 380.), "", shop);
    let tell = held(&mut d, action, (70., 290.), tr("Tell the customer", "Prévenir le client"), customer);
    let stop = held(&mut d, tr("Flow final", "Fin de flot"), (138., 380.), "", customer);
    let pin = put(&mut d, family, tr("Pin (on the border of an action)", "Broche (sur le bord d'une action)"), (223., 138.));
    say(&mut d, pin, tr("order", "commande"));
    d.set_parent(pin, Some(place));
    d.seat(pin);
    let go = |d: &mut Diagram, from: u32, to: u32, text: &str| {
        let id = d.add_link(End::Shape(from), End::Shape(to), Head::Arrow, Route::Elbow);
        if let Some(link) = d.link_mut(id) {
            link.text = text.to_string();
        }
    };
    go(&mut d, start, place, "");
    go(&mut d, pin, check, "");
    go(&mut d, check, choice, "");
    go(&mut d, choice, ship, tr("[in stock]", "[en stock]"));
    go(&mut d, ship, end, "");
    go(&mut d, choice, tell, tr("[out of stock]", "[rupture]"));
    go(&mut d, tell, stop, "");
    d
}

fn package_diagram() -> Diagram {
    let mut d = Diagram::blank();
    let class = tr("Class", "Classe");
    let package = tr("Package", "Paquetage");
    let (a, b) = (put(&mut d, class, package, (40., 40.)), put(&mut d, class, package, (380., 40.)));
    say(&mut d, a, tr("billing", "facturation"));
    say(&mut d, b, tr("shipping", "livraison"));
    let link = d.add_link(End::Shape(a), End::Shape(b), Head::Arrow, Route::Straight);
    if let Some(link) = d.link_mut(link) {
        (link.dashed, link.text) = (true, "«import»".into());
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::{diagram::COLORS, relation::Relation};

    #[test]
    fn every_shape_is_there_once_and_can_be_found() {
        let all = presets();
        // One name per family; sizes that hold something; a text that fits the shape it is in.
        let mut seen: Vec<(&str, &str)> = all.iter().map(|p| (p.family, p.name)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), all.len());
        assert!(all.iter().all(|p| p.w >= 8. && p.h >= 8. && !p.name.is_empty() && !p.family.is_empty() && p.icon.ends_with(".svg")));
        // A search is every word, in the name or the family, with accents and case ignored.
        let found = |q: &str| all.iter().filter(|p| p.matches(q)).map(|p| p.name).collect::<Vec<_>>();
        assert_eq!(found("").len(), all.len());
        assert_eq!(found(tr("interface", "interface")), [tr("Interface", "Interface")]);
        assert_eq!(found(tr("ENUM", "ÉNUM")), [tr("Enumeration", "Énumération")]);
        let use_case = tr("use case", "cas d'utilisation");
        assert!(found(use_case).contains(&tr("Actor", "Acteur")) && found(use_case).contains(&tr("Use case", "Cas d'utilisation")));
        assert_eq!(found(tr("class abstract", "abstraite classe")), [tr("Abstract class", "Classe abstraite")]);
        assert!(found("zzz").is_empty() && found("  ").len() == all.len());
        // A text of a diagram being typed: `|`, `\`, accents, very long words never match by accident, nor panic.
        for q in ["|", "\\", "é", "日本語", &"x".repeat(1000), "😀"] {
            let _ = found(q);
        }
    }

    #[test]
    fn the_starting_points_are_diagrams_that_read_back() {
        for template in templates() {
            let d = (template.make)();
            assert!(d.rich && !d.shapes.is_empty(), "{}", template.name);
            // Every arrow has its shapes, every held shape its holder, and it is the same after a round of the file.
            assert!(d.links.iter().all(|l| d.trace(l).is_some()), "{}", template.name);
            assert!(d.shapes.iter().all(|s| s.parent.is_none_or(|p| d.shape(p).is_some())), "{}", template.name);
            let svg = d.to_svg(COLORS[0]);
            assert_eq!(Diagram::from_svg(&svg).as_ref(), Some(&d), "{}", template.name);
            assert_eq!(Diagram::from_svg(&svg).unwrap().to_svg(COLORS[0]), svg);
            assert!(d.bounds().is_some());
        }
        let package = (templates().into_iter().find(|t| t.name == tr("Package diagram", "Diagramme de paquetages")).unwrap().make)();
        assert_eq!(Relation::of(&package.links[0]), Some(Relation::Import));
        let use_case = (templates().into_iter().find(|t| t.name == tr("Use case diagram", "Diagramme de cas d'utilisation")).unwrap().make)();
        let by = |form: Form| use_case.shapes.iter().find(|s| s.form == form).unwrap();
        assert_eq!((by(Form::Ellipse).parent, by(Form::Actor).parent), (Some(by(Form::Frame).id), None));
    }
}
