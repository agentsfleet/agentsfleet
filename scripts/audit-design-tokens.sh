#!/usr/bin/env bash
# audit-design-tokens.sh — enforce design-system token discipline across
# ui/packages/{app,website}/**/*.tsx.
#
# Gate body: docs/gates/design-token.md (proposed).
# Fires in: make lint, HARNESS VERIFY.
#
# Rejects arbitrary Tailwind classes when an equivalent design-system
# token utility exists in ui/packages/design-system/src/theme.css.
# The token set is encoded below; when the design system adds a token,
# add the matching grep here in the same commit.
#
# Modes:
#   --diff   (default) audit files in `git diff --name-only origin/main`
#   --all    audit the whole worktree (slower; periodic runs)
#
# Exits 0 clean, 1 on any violation. Test files exempt
# (snapshot/E2E assert on rendered DOM).

set -euo pipefail

MODE="${1:---diff}"
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

PATTERNS=(
  'text-\[[0-9]+(px|rem)\]|text-{label,eyebrow,body-sm,body,body-lg,heading,display-md,display-lg,display-xl}'
  'text-\[clamp\(|text-fluid-{display-md,display-lg,hero}'
  'tracking-\[-?[0-9]|tracking-{display-xl,display-lg,display-md,eyebrow,label}'
  'leading-\[[0-9]|leading-{display-xl,display-lg,display-md,heading,eyebrow,body-lg,body,body-sm,label,mono,prose}'
  'max-w-\[[0-9]+(px|rem|ch)\]|max-w-{trim,narrow,measure,form,wide,content,tagline,prose}'
  '(text|bg|border)-(red|blue|green|yellow|purple|indigo|violet|pink|orange|gray|slate|zinc|neutral)-[0-9]|use semantic tokens (text-destructive, text-error, text-success, …)'
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
  --diff)
    BASE="${BASE:-origin/main}"
    if ! git rev-parse --verify "$BASE" >/dev/null 2>&1; then BASE="HEAD"; fi
    FILES=$(git diff --name-only "$BASE"...HEAD 2>/dev/null || true)
    ;;
  --all)
    FILES=$(find ui/packages/app ui/packages/website -type f \( -name '*.tsx' -o -name '*.jsx' \) -not -path '*/node_modules/*' -not -path '*/.next/*' 2>/dev/null)
    ;;
  *)
    echo "usage: $0 [--diff|--all]" >&2; exit 2 ;;
esac

FAIL=0
FILES_WITH_VIOLATIONS=0

while IFS= read -r f; do
  [ -z "$f" ] && continue
  [ ! -f "$f" ] && continue
  in_scope "$f" || continue

  file_failed=0
  for entry in "${PATTERNS[@]}"; do
    regex="${entry%%|*}"
    suggestion="${entry#*|}"
    while IFS= read -r match; do
      [ -z "$match" ] && continue
      # Allow inline override:
      #   // DESIGN TOKEN: SKIPPED per user override (reason: ...)
      ln="${match%%:*}"
      ctx=$(awk -v n="$ln" 'NR>=n-1 && NR<=n' "$f" 2>/dev/null || true)
      if printf '%s' "$ctx" | grep -q 'DESIGN TOKEN: SKIPPED'; then continue; fi
      printf '%s\n  -> %s\n' "$f:$match" "$suggestion"
      file_failed=1
      FAIL=1
    done < <(grep -nE "$regex" "$f" 2>/dev/null || true)
  done
  [ "$file_failed" = "1" ] && FILES_WITH_VIOLATIONS=$((FILES_WITH_VIOLATIONS + 1))
done <<< "$FILES"

if [ "$FAIL" = "0" ]; then
  echo "OK: design-token discipline — no arbitraries that have a token equivalent (mode=$MODE)"
  exit 0
fi

echo
echo "FAIL: design-token discipline ($FILES_WITH_VIOLATIONS file(s) with violations)"
echo "      Replace each match with the suggested token utility, or add the inline override:"
echo "        // DESIGN TOKEN: SKIPPED per user override (reason: <concrete reason>)"
echo "      Token source: ui/packages/design-system/src/theme.css"
echo "      Rule body:    docs/gates/design-token.md (proposed)"
exit 1
