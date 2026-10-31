# Agents (MCP)

`bref mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server in the same program as Bref. An agent (Claude Code, Claude Desktop, Cursor, VS Code) can read the vault, write notes and draw diagrams, and Bref shows the files. The agent does not get a generic folder: it gets Bref's links, its trash, and its diagrams.

The server speaks JSON-RPC over the standard input and output of the process. It does not open a window and it does not use the network.

## What you tell the client

The command is the Bref program, and its first argument is `mcp`.

| System | Command |
|---|---|
| Arch Linux | `/usr/bin/bref` |
| macOS | `/Applications/Bref.app/Contents/MacOS/bref` |
| Windows | `C:\Program Files\Bref\bref.exe` |

Claude Code:

```bash
claude mcp add --scope user bref -- bref mcp
```

Claude Desktop, Cursor, VS Code and the other clients that read a JSON file:

```json
{ "mcpServers": { "bref": { "command": "bref", "args": ["mcp"] } } }
```

On macOS and Windows, `command` is the path from the table. The server uses the vault Bref already has open. Another folder is `--vault` followed by its path, added after `mcp`. `--read-only` offers only the tools that do not write.

## What the agent can do

Reading: the shape of the vault, a list, a search ranked like the palette, one note (a heading or a block), its links and its tags.

Writing: create a note, replace a passage that occurs once, append, rename (links follow, unless another note still has the old name), move to the trash and bring back, the daily note, a comment on one passage.

Diagrams: the agent draws on the same canvas as you, with the same UML shapes: lifelines, calls, replies, `alt`, `opt` and `loop` fragments, classes with their compartments, inheritance, composition. You can then move and edit what it drew. It can also start from a template, turn a Mermaid class, sequence or state diagram into a canvas diagram, import Excalidraw or an uncompressed draw.io file, and render a PNG of what it drew to check it. A diagram file is written by Bref. A flowchart stays a Mermaid block in a note.

Showing: when Bref is open on the same vault, the agent can open a note, a diagram or a table in a tab of its own (`show`), and the window comes to the front. When Bref is closed, or has another vault open, it says so; the files are written all the same.

The user guide is a resource (`bref://guide/writing`, `links`, `comments`, `diagrams`, `uml`, and the other pages). Four prompts start a piece of work: document a feature, an architecture overview, a UML diagram from code, a review that leaves comments.

## Asking for a documented feature

Give the agent the work and say where it goes, for example:

> Implement issue 93. Then document it in my Bref notes with the `bref` server: a note on what changed and why, a technical part, a class diagram of the main types and a UML sequence diagram of the main scenario, drawn on the canvas. Render each diagram and fix it until it is clean.

Clients that show prompts offer the same as **document_feature**: it reminds the agent of the rules of a clean UML diagram (one scenario per diagram, lifelines in the order they take part, a reply for each call that waits, `alt` with a guard per branch). Ask for one diagram per question the reader has. A diagram that reads badly can be named in a second message: "the sequence diagram has too many lifelines, split the error path into its own".

## What it will not do

- Leave the vault. An absolute path, `..`, or a link that steps outside is refused. Hidden folders are not listed.
- Delete a file. The trash is the only way out, and a name that is already taken is refused.
- Overwrite a note that changed after it was read. `read_note` returns a revision; `edit_note` with an old revision is refused.
- Open the network, run a program, or touch git.

A note's text is data. It is not an instruction to the agent.

## While you have the note open

What another program writes shows up in Bref within about half a second, with the cursor and the scroll where they were. If you are typing in that note at that moment, Bref merges: the agent's change joins your text, and a single undo takes it back. When you both changed the same lines, your version stays in the note and the agent's is written next to it, as `Note (conflict 2026-10-31).md`, the way a sync conflict is, and a message says so. Nothing is lost either way.
