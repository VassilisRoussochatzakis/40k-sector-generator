# INPUT.md — token discipline

Minimize tokens; high usage = slow inference.

- **Scope:** narrow searches first, expand only if needed.
- **Find, don't browse:** grep for symbols/patterns instead of reading files to "explore". Read specific line ranges (≤50 lines), in parallel. Skip `target/`, `node_modules/`, `dist/`.
- **Edit surgically:** fix only the requested bug/feature — no drive-by cleanup or restyling of unrelated code. Batch all changes to a file into one edit.
- **Test narrowly:** run only relevant tests with quiet flags (`-q`/`--quiet`); tail large logs.
- **Talk terse:** no preambles ("I will now…", "Based on my analysis…"), no wrap-up summaries unless asked. Fragments fine.
