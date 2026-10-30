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

Diagrams: check a Mermaid source, turn a class, sequence or state diagram into a canvas diagram, start from a template or from shapes and links, read and edit that file, import Excalidraw or an uncompressed draw.io file, and render a PNG of what was drawn. A diagram file is written by Bref. A flowchart stays a Mermaid block in a note.

The user guide is a resource (`bref://guide/writing`, `links`, `comments`, `diagrams`, `uml`, and the other pages). Four prompts start a piece of work: document a feature, an architecture overview, a UML diagram from code, a review that leaves comments.

## What it will not do

- Leave the vault. An absolute path, `..`, or a link that steps outside is refused. Hidden folders are not listed.
- Delete a file. The trash is the only way out, and a name that is already taken is refused.
- Overwrite a note that changed after it was read. `read_note` returns a revision; `edit_note` with an old revision is refused.
- Open the network, run a program, or touch git.

A note's text is data. It is not an instruction to the agent.

## While you have the note open

What another program writes shows up in Bref within about half a second. A note you are typing in is the exception: Bref keeps your version and writes it back, so an edit the agent makes at that moment is lost without a word. Do not type in a note an agent is writing. Pass the revision from the last read, and a change of yours is refused instead of overwritten.
