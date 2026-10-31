# UML diagrams

The [canvas](diagrams.md) reads and draws UML notation. The shapes of each family are in the [library](diagrams.md#library) (`S`), with a starting point for each kind of diagram.

What the notation does not read (a sentence, a comment) is drawn as typed, never refused.

## Classes

In a rectangle, a line made of `---` starts a new compartment:

1. the name of the class, with a stereotype line first if any: `«interface»`, `«enumeration»`, `«abstract»` or `<<interface>>`;
2. the attributes;
3. the operations;
4. any other compartment, which can open with a title (`Responsibilities:`).

A member is written as the specification does:

```text
+ name
- /derived: Type[0..*] = default {readOnly}
# area(width: int): f32
```

- `+ - # ~` are the visibilities.
- `static` is drawn underlined and `abstract` in italics. The keyword itself is not drawn.
- An abstract class has its name in italics: write `«abstract»`, or `{abstract}` after the name.
- The literals of an enumeration stay as typed.

## Objects

An object (an instance) is written `name : Type`, or `: Type` with no name, and is underlined. Its slots are the attributes below it: `total = 12`.

An object diagram is made of such boxes, linked by arrows that carry a name in the middle and a role at each end.

## Packages

A package is a container (`G`). Its name is in the tab. The lines after it, such as a qualified name (`pkg::billing`), are under the tab.

Packages nest to any depth. `«import»`, `«access»` and `«merge»` between them are in the relationships menu.

## Use cases

- A use case is an ellipse with its name (`O`). A `---` line in it starts the compartment of its extension points, under the name, behind a separator that stays inside the ellipse.
- The system boundary is a frame (`F`) that holds the use cases. The actors (`P`) stay outside.
- An association is a line (`L`).
- `«include»` (`K` then `L`), `«extend»` (`K` then `E`, with its condition on the next line), and the generalization of actors or of use cases (`K` then `H`) are in the relationships menu.

## State machines

- A state is a rounded box (`U`). Its name is on top. Its internal activities (`entry / action`, `do / activity`, `exit / action`) are in a compartment under a `---` line.
- A composite state is a rounded container with its name in a header. It holds its substates, and its regions are cut by a dashed divider.
- The library holds the pseudostates: the initial state (a filled dot), the final state, the choice (a small diamond), the junction, the fork and join (a bar), the shallow and deep history (`H`, `H*`), the terminate (a cross), and the entry and exit points.
- A transition is an arrow whose text reads `trigger [guard] / effect`. The guard, between brackets, is in italics.
- A transition that stays in its state is a loop: with a shape selected, a button of the bar adds it and writes its label.

## Sequence diagrams

**Lifelines** come from the library. They stand side by side, their heads on one line.

**Messages.** An arrow drawn from one lifeline to another is a message. A lifeline is joined whole, whatever the height.

- A message's height is not stored: it follows from its **order**. The first is under the heads, the next ones 44 below each other. Inserting one pushes the others down.
- Dragging a message up or down puts it where its height says, in one step of undo. The lifelines keep their places and grow to hold the last message.
- A button of the bar numbers the messages (`3: find()`). The number is shown, never typed.

**Kinds of messages**

| Message | How |
|---|---|
| A call that waits (filled triangle) | `K` then `F`. It opens an activation bar on the lifeline it reaches. |
| The answer (dashed arrow) | `K` then `D`, from the called lifeline back to the caller. It closes the bar. |
| A call to oneself | an arrow from a lifeline to itself: a loop on its right, and a short bar |
| `«create»` | `K` then `O`. It brings the head of the new lifeline down to the message. |
| `«destroy»` | `W`. It ends the lifeline with a cross. |
| Found and lost messages | `P` and `T`. A dot at one end stays where it was put. |

Calls inside calls nest their bars.

**Combined fragments** (`alt`, `opt`, `loop`, `par`, `break`, `critical`, `neg`, `ref`) are in the library.

- With messages selected, picking one wraps them in it: a frame with the operator in its tab and the guard beside it, as wide as the lifelines they touch and as tall as they need. Otherwise it is a shape to draw.
- The first line of its text is the operator. Each line after it is an operand (`alt`, `[ok]`, `[else]`), with a dashed line between operands.
- A message is picked through the fragment as usual.
- A message dragged into a fragment is held by it; dragged out, it leaves. Let it go low in the fragment to reach the next operand.
- Fragments nest. Removing a fragment keeps its messages.

## Activity diagrams

The library has the nodes of an activity:

- an action (a rounded box), and a call behaviour (the same, with a rake in its corner);
- an object node;
- the initial node, the activity final, and the flow final (a cross in a circle);
- the decision and the merge (a small diamond);
- the fork and the join (a bar that stretches to any length, horizontal or vertical);
- the activity frame (`act Name`) and the `«interruptible»` region (a dashed frame that holds shapes).

**Swimlanes** are a column (a name in a header on top) or a row (a name in a header on the left). Both hold the shapes dropped on them, and move, copy and remove them together.

**Pins** are small squares. Dropped within a few pixels of the border of a shape, a pin sits on that border and is held by the shape: it follows it, stays on its border when it is resized, and goes with it when it is removed. Its name is written outside. Dragged far from every border, it is let go.

**Flows**: control and object flows are arrows (`A`, `N`). Their `[guard]` is their text.

A starting point in the library shows an order in two swimlanes, with a pin, a decision and two ends.

## Components and deployment

- A **component** is a box with its small icon in a corner.
- A **port** is the pin of activity diagrams: it sits on the border of the component, follows it and goes with it.
- The **node**, the **device** and the **execution environment** are 3D boxes whose stereotype is their first line. They hold the shapes dropped on them (nodes inside nodes) and move, copy and remove them together.
- The **artifact** is `«artifact»`.
- The provided interface is the ball (`K` then `B`), the required one the socket (`K` then `Q`). Two arrows that end at the same point meet there.
- The communication path is a line (`A`) with its stereotype as text (`«HTTPS»`).
- `«deploy»` (`K` then `J`) puts an artifact on a node. `«manifest»` (`K` then `V`) makes it the form of a component.

Two starting points: a shop and its payment service, and a laptop and a server.

## Relationships

`K`, or the arrows button of the dock, opens a menu of the UML relationships by family. Each has its name and its letter. With an arrow selected, the first button of its bar names its relationship and opens the same menu.

| Family | Relationship (key) |
|---|---|
| Associations | association (`A`), navigable association (`N`, an arrow), non-navigable end (`X`, a cross), aggregation (`G`, hollow diamond), composition (`C`, full diamond), association class (`R`) |
| Inheritance and interfaces | generalization (`H`, hollow triangle), realization (`Z`, dashed, hollow triangle), provided interface (`B`, a ball), required interface (`Q`, a socket) |
| Dependencies | dependency (`D`, dashed arrow), and its stereotypes `«use»` (`U`), `«import»` (`I`), `«access»` (`S`), `«include»` (`L`), `«extend»` (`E`), `«merge»` (`M`), `«deploy»` (`J`), `«manifest»` (`V`) |
| Messages | a call that waits (`F`), `«create»` (`O`), `«destroy»` (`W`), found (`P`), lost (`T`) |

- The menu also offers the n-ary association (`Y`, a small diamond).
- The association class is a dotted line from a class that holds on to the middle of an association, and follows it.
- Picked with nothing selected, a relationship is the tool for the next arrow. With arrows selected, it changes them.
- A relationship is not marked in the file. An arrow is recognised from its two heads, its dashes and its stereotype, so one made by hand with the same look is the same relationship.
