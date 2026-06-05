#!/usr/bin/env bash
# PreToolUse(Bash) guard — deny a leading `cd` into the project root.
#
# The Bash tool's working directory already persists at the project root across
# calls, so `cd /Users/.../40k-sector-generator && <cmd>` is redundant and can
# trip an extra permission prompt. This hook blocks exactly that, with guidance.
# It does NOT block `cd` into subdirectories, or `cd` elsewhere (e.g. /tmp).
#
# Wired from .claude/settings.json: hooks.PreToolUse[matcher="Bash"].
# Background: memory note `no-cd-prefix-in-bash`.
#
# Contract: reads the PreToolUse JSON payload on stdin; emits a deny decision on
# stdout only when blocking; always exits 0 (the decision rides in the JSON, not
# the exit code).

root="${CLAUDE_PROJECT_DIR:-/Users/vassilis/Documents/40k-sector-generator}"
root="${root%/}"
home_form="~${root#"$HOME"}"   # e.g. ~/Documents/40k-sector-generator

cmd="$(jq -r '.tool_input.command // empty')"
[ -z "$cmd" ] && exit 0

# Only consider the first physical line, left-trimmed.
first="$(printf '%s\n' "$cmd" | sed -n '1p')"
first="${first#"${first%%[![:space:]]*}"}"

case "$first" in
  "cd "*)
    target="${first#cd }"
    target="${target#"${target%%[![:space:]]*}"}"   # left-trim
    target="${target#[\"\']}"                        # strip one leading quote
    target="${target%%[[:space:]]*}"                 # cut at first whitespace
    target="${target%%[;&|]*}"                       # cut at ; & |
    target="${target%[\"\']}"                        # strip one trailing quote
    target="${target%/}"                             # strip one trailing slash
    if [ "$target" = "$root" ] || [ "$target" = "$home_form" ]; then
      reason="Redundant cd into the project root ($root): the Bash tool's working directory already persists there across calls, so a leading cd is unnecessary and can trigger an extra permission prompt. Drop it — use absolute paths, or bare relative paths from the repo root. (memory: no-cd-prefix-in-bash)"
      printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":%s}}\n' \
        "$(printf '%s' "$reason" | jq -Rs .)"
    fi
    ;;
esac
exit 0
