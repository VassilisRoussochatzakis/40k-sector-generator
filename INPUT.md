# AGENT CONTEXT OPTIMIZATION (INPUT.md)

## MANDATE: MINIMIZE TOKENS
High token usage = slow inference. Follow these rules.
- **Scope Control:** Start with narrow searches. Expand scope ONLY if needed. Small first → less noise.

## RESEARCH PHASE
- **Grep over Read:** Use `grep_search` to find symbols/patterns. Do NOT read files to "explore".
- **Surgical Reads:** Read only specific lines (`start_line`, `end_line`). Max 50 lines.
- **Parallelize:** Run searches/reads in parallel turns.
- **Ignore Noise:** Respect `.gitignore`. Skip `target/`, `node_modules/`, `dist/`.

## EXECUTION PHASE
- **No Refactor:** Fix ONLY requested bug/feat. No "cleanup" or "styling" of unrelated code.
- **Concise Output:** Keep responses short and technical.
- **Direct Output:** No preambles ("I will now...", "Based on my analysis...").
- **Batch Edits:** Use `replace` once per file. Plan all changes first.

## VALIDATION PHASE
- **Specific Tests:** Run only relevant tests. No full suite runs unless required.
- **Brief Logs:** Use quiet flags (`--quiet`, `-q`). Tail logs if large.

## COMMUNICATION
- **Technical Only:** Focus on logic/rationale. 
- **Fragments OK:** Drop articles/filler.
- **Stop on Done:** No wrap-up summaries unless requested.
