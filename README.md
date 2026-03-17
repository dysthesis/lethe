**TODO: REVISE THIS README**

## Architecture (current direction)

- `note`: the Note data model plus edit semantics.
- `store`: persistence boundary (`Store` trait) with `NoteStore` as the filesystem implementation.
- `repository`: vault/graph layer (aliases, links, search) built on a `Store`.

- Notes similar to microblogging, such that each note is atomic, without a mandatory title (but instead, a possibly empty list of aliases)
- Main view is timeline
- Inspecting each note will also show a GitHub-issues-esque timeline/list of mentions for that note (Obsidian-style note linking)
- Each note has optionally, additional key-value pairs as user-defined attributes
- Notes may have validation rules, similar to types in programming languages, where types enforces some sort of schema/key-value pairs. For example, a note tagged as `task` must have a due date, priority, and some `Option<Task>` for parent (and a derived list of children), while a `transaction` is similar to a double-entry accounting, having a list of accounts with associated mutations that must sum to zero
  - I need to figure out what kind of primitive types the tool should recognise. Other than the usual int, char, string, bool, etc., I was thinking date-time, references to other notes (with optional constraints in the form of a boolean predicate that the target note should have, e.g. the parent of a task must be another task).
    - Should I implement something similar to type refinement, where I can, say, implement a priority type that refines int such that it must be between 1-5?
- Intended use case is event-driven journalling, such that I constantly write everything that happens, every thought I have, etc. as I go about my day, with the ability for rapid capture and later review and refinement
- Instead of the common wisdom of carefully pruning and choosing what goes in your notes to maintain a high signal-to-noise ratio, I want to instead capture _everything_, and maintain the high signal-to-noise ratio via a powerful, precise query DSL.
- Supports zettelkasten as well; a fleeting or permanent note is simply another event/"tweet"
- Everything is a file, and the storage for this tool is a folder that may as well be (but not necessarily!) a Git repository, such that Git syncing is possible. In fact, we want to optimise for minimal diffs and conflicts.
- Although the first iteration would be a CLI tool, as I live in the terminal, I also have a phone with which I would like to use this tool, so I might have to build in things such as Git support, notifications (for tasks, or a built-in "Notifiable" type/trait), etc. in the core crate (I'm using rust, such that as much as possible is in the -core crate, while the -cli crate wraps around -core with Unix CLI support, and we use FFI to call these logic from Kotlin for the Android app)
- instead of having frontmatter in Markdown files, I was thinking of having a UUID for each note, and have the text body be in some notes/UUID/body.md, any key-value pairs in notes/UUID/meta.json (any attachments for a note would go in notes/UUID/ATTACHMENT)
- Query should be powerful enough to, say, calculate the PageRank score of some note or the Personalised PageRank score between two notes, or perform a full-text search over BM25, find the semantic similarity of note bodies via vector embeddings (maybe make the embedding of the body a built-in value?)
- SQLite database for caching, as long as the data in the db is strictly derivable from the plaintext.

## Formatting

- Run `nix fmt` to format Nix, Rust (edition 2024), TOML, and Markdown via treefmt.
- `nix flake check` includes the treefmt check; formatting drift will fail CI.
- Dev shell bundles the treefmt wrapper (`config.treefmt.build.wrapper`) and formatter binaries for offline use.
