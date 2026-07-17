#!/usr/bin/env bash
# design-tokens.sh — enforce design-system token discipline across
# ui/packages/{app,website}/**/*.tsx.
#
# Dispatch façade: dispatch/write_ts_adhere_bun.md (Design Tokens / DESIGN TOKEN GATE)
# Fires in: CONFORM (via `make harness-verify` in `agentsfleet`).
#
# Rejects arbitrary Tailwind classes when an equivalent design-system
# token utility exists in ui/packages/design-system/src/theme.css.
# The token set is encoded below; when the design system adds a token,
# add the matching grep here in the same commit.
#
# TECHNICAL DEBT (acknowledged on migration to dotfiles, 2026-05-11):
# The PATTERNS array hard-codes agentsfleet-specific token names
# (text-fluid-hero, max-w-prose, tracking-display-md, etc.). If a second
# project adopts this dotfiles set, it must either:
#   (a) share agentsfleet's exact token vocabulary, or
#   (b) fork the PATTERNS array, or
#   (c) wait for the parameterised version that reads token names from
#       $REPO/ui/packages/design-system/src/theme.css at runtime.
# Path (c) is the right long-term shape; not done yet.
#
# Scope (M70):
#   Walks the full working tree via `git ls-files` — sees staged content
#   because the index is what `ls-files` reports. Pre-commit-safe: a fix
#   staged but not yet committed satisfies the check on the same hook run.
#   The previous `--diff` (BASE...HEAD) default was retired with M70
#   because it was blind to the index at pre-commit time.
#
# Modes:
#   (no flag) / --all  full-codebase scan via `git ls-files` (default)
#   --staged           narrow to files in `git diff --cached --name-only`
#                      — opt-in fast loop for iterative dev
#
# Exits 0 clean, 1 on any violation. Test files exempt
# (snapshot/E2E assert on rendered DOM).

set -euo pipefail

MODE="${1:---all}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# ── Patterns ─────────────────────────────────────────────────────────────
#
# Each pattern flags an arbitrary CSS class that has a token equivalent.
# Format: <regex>|<suggestion>. The suggestion column is hint-only — the
# closest token depends on context; the audit just blocks the arbitrary.
#
# Tailwind base utilities (m-0, w-full, flex, gap-N, etc.) are not flagged
# — only arbitrary-value classes (the `text-[...]`, `leading-[...]`,
# `max-w-[...]`, `tracking-[...]` shape).

# Format: <regex>:::<suggestion>. The `:::` delimiter is unambiguous —
# regex content frequently contains `|` (alternation), so we can't use it.
PATTERNS=(
  'text-\[[0-9]+(px|rem)\]:::text-{label,eyebrow,body-sm,body,body-lg,heading,display-md,display-lg,display-xl}'
  'text-\[clamp\(:::text-fluid-{display-md,display-lg,hero}'
  'tracking-\[-?[0-9]:::tracking-{display-xl,display-lg,display-md,eyebrow,label}'
  'leading-\[[0-9]:::leading-{display-xl,display-lg,display-md,heading,eyebrow,body-lg,body,body-sm,label,mono,prose}'
  'max-w-\[[0-9]+(px|rem|ch)\]:::max-w-{trim,narrow,measure,form,wide,content,tagline,prose}'
  '(text|bg|border)-(red|blue|green|yellow|purple|indigo|violet|pink|orange|gray|slate|zinc|neutral)-[0-9]:::use semantic tokens (text-destructive, text-error, text-success, …)'
)

# ── File scope ───────────────────────────────────────────────────────────
in_scope() {
  local f="$1"
  case "$f" in
    *.test.tsx|*.test.jsx) return 1 ;;
    *tests/e2e/*) return 1 ;;
    ui/packages/app/*.tsx|ui/packages/app/*.jsx) return 0 ;;
    ui/packages/website/*.tsx|ui/packages/website/*.jsx) return 0 ;;
    ui/packages/app/**/*.tsx|ui/packages/app/**/*.jsx) return 0 ;;
    ui/packages/website/**/*.tsx|ui/packages/website/**/*.jsx) return 0 ;;
    *) return 1 ;;
  esac
}

case "$MODE" in
  --staged|staged)
    FILES=$(git diff --cached --name-only --diff-filter=ACMRT 2>/dev/null || true)
    ;;
  --all|all)
    # `git ls-files` reports the index, which includes staged content —
    # so pre-commit-style invocations see staged-but-not-committed files
    # without needing a `--staged` flag.
    FILES=$(git ls-files -- 'ui/packages/app/*.tsx' 'ui/packages/app/*.jsx' 'ui/packages/website/*.tsx' 'ui/packages/website/*.jsx' 2>/dev/null || true)
    ;;
  *)
    echo "usage: $0 [--all|--staged]" >&2
    echo "note: --diff was retired in M70 — see dispatch/write_ts_adhere_bun.md (Design Tokens → Scope)." >&2
    exit 2 ;;
esac

FAIL=0
SEEN_FILES_LIST=""

# Pre-filter FILES list once (drop blanks, non-existent, out-of-scope).
SCOPED_FILES=()
while IFS= read -r f; do
  [ -z "$f" ] && continue
  [ ! -f "$f" ] && continue
  in_scope "$f" || continue
  SCOPED_FILES+=("$f")
done <<< "$FILES"

if [ "${#SCOPED_FILES[@]}" -gt 0 ]; then
  for entry in "${PATTERNS[@]}"; do
    regex="${entry%%:::*}"
    suggestion="${entry#*:::}"
    # Single grep across all scoped files per pattern.
    # `grep -nHE` always prefixes with filename + line; `-H` is required
    # when a single file is supplied so the parser sees a uniform shape.
    while IFS= read -r match; do
      [ -z "$match" ] && continue
      f="${match%%:*}"
      rest="${match#*:}"
      ln="${rest%%:*}"
      # Honor inline override `// DESIGN TOKEN: SKIPPED per user override (reason: ...)`
      # by checking the matched line and the line above.
      ctx=$(awk -v n="$ln" 'NR>=n-1 && NR<=n' "$f" 2>/dev/null || true)
      if printf '%s' "$ctx" | grep -q 'DESIGN TOKEN: SKIPPED'; then continue; fi
      printf '%s\n  -> %s\n' "$match" "$suggestion"
      SEEN_FILES_LIST="${SEEN_FILES_LIST}${f}"$'\n'
      FAIL=1
    done < <(grep -nHE "$regex" "${SCOPED_FILES[@]}" 2>/dev/null || true)
  done
fi

FILES_WITH_VIOLATIONS=$(printf '%s' "$SEEN_FILES_LIST" | sort -u | grep -c . || true)

if [ "$FAIL" = "0" ]; then
  echo "OK: design-token discipline — no arbitraries that have a token equivalent (mode=$MODE)"
  exit 0
fi

echo
echo "FAIL: design-token discipline ($FILES_WITH_VIOLATIONS file(s) with violations)"
echo "      Replace each match with the suggested token utility, or add the inline override:"
echo "        // DESIGN TOKEN: SKIPPED per user override (reason: <concrete reason>)"
echo "      Token source: ui/packages/design-system/src/theme.css"
echo "      Rule body:    dispatch/write_ts_adhere_bun.md (Design Tokens)"
exit 1
