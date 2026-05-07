#!/usr/bin/env bash
# audit-spec-template.sh — enforce docs/TEMPLATE.md "Prohibited" section
# across every spec under docs/v*/{pending,active,done}/.
#
# Gate body: docs/gates/spec-template.md
# Fires in: make lint (after audit-logging, before audit-error-codes).
#
# Modes:
#   --staged         diff-scope: only files in `git diff --cached`
#   --all            (default) pending+active specs only — current/in-flight work
#   --include-done   adds done/ specs to the scan (one-time sweep tool)
#
# Done specs are excluded by default: they're historical artifacts, not work
# product. Rewriting them risks losing context. The gate prevents new drift;
# existing done specs are tolerated.
#
# Exits 0 clean, 1 on any blocking finding.

set -euo pipefail

MODE="${1:-${SCOPE:-all}}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

FAIL=0
fail() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()   { printf "OK:   %s\n" "$*"; }
note() { printf "NOTE: %s\n" "$*"; }

# Discover spec files in scope.
case "$MODE" in
  --staged|staged)
    mapfile -t SPECS < <(git diff --cached --name-only --diff-filter=ACMRT | grep -E '^docs/v[0-9]+/(pending|active|done)/.*\.md$' || true)
    ;;
  --all|all)
    mapfile -t SPECS < <(find docs/v[0-9]* -type f -name '*.md' 2>/dev/null | grep -E '/(pending|active)/' || true)
    ;;
  --include-done|include-done)
    mapfile -t SPECS < <(find docs/v[0-9]* -type f -name '*.md' 2>/dev/null | grep -E '/(pending|active|done)/' || true)
    ;;
  *)
    printf "usage: %s [--staged|--all|--include-done]\n" "$0" >&2
    exit 64
    ;;
esac

if [[ ${#SPECS[@]} -eq 0 ]]; then
  ok "no spec files in scope ($MODE)"
  exit 0
fi

# ---------------------------------------------------------------------------
# Prohibited patterns from docs/TEMPLATE.md "Prohibited" section.
# Each pattern: <regex>:<one-line description>:<severity>
# Severity: BLOCK = exit 1; INFO = stdout note only.
# Patterns use ERE (egrep -E).
#
# DESIGN NOTE — patterns are deliberately structural, not prose-level.
# Bare regex like `\b\d+\s*days?\b` false-positives on legitimate prose
# ("Day 50", "≥7 days uptime", "within 3 days of install"). Instead we
# match where time/effort estimates LIVE structurally:
#   - Section headings: `## Estimated effort`, `## Effort`, `## Sizing`
#   - Label rows:       `**Effort:** medium`, `**Owner:** alice`
#   - Tilde-bulleted:   `- ~2 h survey`, `* ~5 days OAuth setup`
#                       (the `~` prefix is the Captain's idiom for
#                       effort estimates; legitimate prose doesn't use it)
# This catches the actual drift class without churning real content.
# Lines with explicit "effort"/"estimate" context still get flagged.
# ---------------------------------------------------------------------------
PATTERNS=(
  '^#+ .*Estimated [Ee]ffort:Estimated effort section heading:BLOCK'
  '^#+ .*\b(Effort|Complexity|Sizing|Cost)\b\s*$:Effort/Complexity/Sizing section heading:BLOCK'
  '^#+ .*\bScale\b.*estimate:Scale estimate section heading:BLOCK'
  '^[-*]\s*~\s*[0-9]+\s*h\b:Tilde-bulleted hour estimate (e.g. "- ~2 h"):BLOCK'
  '^[-*]\s*~\s*[0-9]+\s*(hours?|days?|min(utes?)?)\b:Tilde-bulleted time estimate:BLOCK'
  '^\s*\*\*\s*(Effort|Estimate|Sizing|Complexity|Cost)\s*\*\*\s*[:|]:Effort/Estimate label row:BLOCK'
  '^\s*\*\*\s*Owner\s*\*\*\s*[:|]:Owner label row:BLOCK'
  '^\s*\*\*\s*Assigned to\s*\*\*\s*[:|]:Assigned-to label row:BLOCK'
  '^\s*\*\*\s*(Due|Deadline)\s*\*\*\s*[:|]\s*[0-9]:Date-bound deadline label:BLOCK'
  '^[-*\|]?\s*\*?\*?\s*[0-9]+\s*%\s*complete:Percentage-complete metric:BLOCK'
  '\b(?:effort|estimate|takes?|spend)\b[^.]{0,40}\b[0-9]+\s*[-–]\s*[0-9]+\s*h\b:Hour-range estimate in effort context:BLOCK'
  '\b(?:low|medium|high|small|large)\s*[/|-]\s*(?:effort|complexity)\b:Effort/complexity rating:BLOCK'
)

# ---------------------------------------------------------------------------
# Carve-outs — patterns that look prohibited but are legitimate. Skip
# any line matching one of these.
# ---------------------------------------------------------------------------
SAFE_LINES_RE='^\*\*Date:\*\*|^\s*```|<!--|^\s*//|"[0-9]+\s*(h|hour|day|min)"'

scan_spec() {
  local spec="$1"
  local hits=0
  local pattern desc severity line content
  while IFS=: read -r pattern desc severity; do
    while IFS= read -r match; do
      [[ -z "$match" ]] && continue
      # Skip carve-out lines.
      if grep -qE "$SAFE_LINES_RE" <<<"$match"; then
        continue
      fi
      line="${match%%:*}"
      content="${match#*:}"
      content="${content%$'\r'}"
      if [[ "$severity" = "BLOCK" ]]; then
        fail "$spec:$line — $desc"
        printf "        > %s\n" "$content" >&2
      else
        note "$spec:$line — $desc"
      fi
      hits=$((hits + 1))
    done < <(grep -nE "$pattern" "$spec" 2>/dev/null || true)
  done < <(printf '%s\n' "${PATTERNS[@]}")
  [[ $hits -eq 0 ]] && ok "$spec — no prohibited patterns"
}

for spec in "${SPECS[@]}"; do
  [[ -f "$spec" ]] || continue
  scan_spec "$spec"
done

if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 SPEC TEMPLATE GATE: violations found. See docs/gates/spec-template.md.\n" >&2
  exit 1
fi

ok "SPEC TEMPLATE GATE: clean (${#SPECS[@]} specs)"
exit 0
