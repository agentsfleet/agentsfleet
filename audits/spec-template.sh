#!/usr/bin/env bash
# spec-template.sh — enforce docs/TEMPLATE.md across every spec under
# docs/v*/{pending,active,done}/. Two check families:
#
#   1. PROHIBITED patterns (negative space) — TEMPLATE.md "Prohibited" section:
#      time/effort estimates, effort/complexity columns, %-complete, owners, dates.
#      Always BLOCK.
#   2. REQUIRED-PRESENT + NO-PLACEHOLDER (positive space) — the determinism
#      sections the agent-facing template mandates (PR Intent, Applicable Gates,
#      Prior-Art, Metrics, Decomposition, tiered Test Spec, Product Clarity,
#      Discovery, …) must EXIST and be FILLED: no unfilled {slot} sentinels and
#      no surviving "tpl:" guidance comments (template fill grammar). A spec
#      that omits them forces the executing agent to guess intent. This is the
#      half that makes a spec "built for the agent".
#
# Dispatch façade: dispatch/write_spec.md (SPEC TEMPLATE GATE)
# Fires in: make lint (after audit-logging, before audit-error-codes).
#
# Modes:
#   --staged         diff-scope: only specs in `git diff --cached`
#   --all            (default) pending+active specs only — current/in-flight work
#   --include-done   adds done/ specs to the scan (one-time sweep tool)
#
# Family 2 runs ONLY in --staged scope — the spec being authored/edited right
# now, which is exactly the agent's own output. The bulk scans (--all /
# --include-done) run Family 1 only, so they behave identically over the whole
# corpus and never break an existing spec. No legacy carve-out, no heuristics:
# author a spec → full template required; scan the tree → prohibited-only.
#
# Done specs are excluded by default: historical artifacts, not work product.
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
# Family 1 — Prohibited patterns from docs/TEMPLATE.md "Prohibited" section.
# Each pattern: <severity>:<one-line description>:<regex> — the regex comes
# LAST because it may contain colons (e.g. the [:|] label-row class); read(1)
# assigns the remainder of the line to the final field, so the regex survives
# intact. Descriptions must stay colon-free.
# Severity: BLOCK = exit 1; INFO = stdout note only. Patterns use ERE (egrep -E)
# only — no PCRE constructs like (?:...), which grep -E rejects.
#
# DESIGN NOTE — patterns are deliberately structural, not prose-level.
# Bare regex like `\b\d+\s*days?\b` false-positives on legitimate prose
# ("Day 50", "≥7 days uptime", "within 3 days of install"). Instead we
# match where time/effort estimates LIVE structurally:
#   - Section headings: `## Estimated effort`, `## Effort`, `## Sizing`
#   - Label rows:       `**Effort:** medium`, `**Owner:** alice`
#   - Tilde-bulleted:   `- ~2 h survey`, `* ~5 days OAuth setup`
#                       (the `~` prefix is Indy's idiom for effort estimates;
#                       legitimate prose doesn't use it)
# This catches the actual drift class without churning real content.
# ---------------------------------------------------------------------------
PATTERNS=(
  'BLOCK:Estimated effort section heading:^#+ .*Estimated [Ee]ffort'
  'BLOCK:Effort/Complexity/Sizing section heading:^#+ .*\b(Effort|Complexity|Sizing|Cost)\b\s*$'
  'BLOCK:Scale estimate section heading:^#+ .*\bScale\b.*estimate'
  'BLOCK:Tilde-bulleted hour estimate (e.g. "- ~2 h"):^[-*]\s*~\s*[0-9]+\s*h\b'
  'BLOCK:Tilde-bulleted time estimate:^[-*]\s*~\s*[0-9]+\s*(hours?|days?|min(utes?)?)\b'
  'BLOCK:Effort/Estimate label row:^\s*\*\*\s*(Effort|Estimate|Sizing|Complexity|Cost)\s*(:\s*\*\*|\*\*\s*[:|])'
  'BLOCK:Owner label row:^\s*\*\*\s*Owner\s*(:\s*\*\*|\*\*\s*[:|])'
  'BLOCK:Assigned-to label row:^\s*\*\*\s*Assigned to\s*(:\s*\*\*|\*\*\s*[:|])'
  'BLOCK:Date-bound deadline label:^\s*\*\*\s*(Due|Deadline)\s*(:\s*\*\*|\*\*\s*[:|])\s*[0-9]'
  'BLOCK:Percentage-complete metric:^[-*\|]?\s*\*?\*?\s*[0-9]+\s*%\s*complete'
  'BLOCK:Hour-range estimate in effort context:\b(effort|estimate|takes?|spend)\b[^.]{0,40}\b[0-9]+\s*[-–]\s*[0-9]+\s*h\b'
  'BLOCK:Effort/complexity rating:\b(low|medium|high|small|large)\s*[/|-]\s*(effort|complexity)\b'
)

# Carve-outs — lines that look prohibited but are legitimate. Skip any match.
SAFE_LINES_RE='^\*\*Date:\*\*|^\s*```|<!--|^\s*//|"[0-9]+\s*(h|hour|day|min)"'

# ---------------------------------------------------------------------------
# Family 2 — Required determinism sections (agent-facing docs/TEMPLATE.md).
# Each entry: <heading ERE>:<human description>
# ---------------------------------------------------------------------------
REQUIRED_SECTIONS=(
  '^#+ .*PR Intent:PR Intent & comprehension handshake'
  '^#+ .*Applicable Rules:Applicable Rules'
  '^#+ .*Applicable Gates:Applicable Gates'
  '^#+ .*Overview:Overview'
  '^#+ .*(Prior-Art|Reference Implementation):Prior-Art / Reference Implementations'
  '^#+ .*Files Changed:Files Changed (blast radius)'
  '^#+ .*(Decomposition|Alternatives):Decomposition & alternatives (patch vs refactor)'
  '^#+ .*Sections:Sections (implementation slices)'
  '^#+ .*Metrics.*Observability:Metrics & Observability'
  '^#+ .*Interfaces:Interfaces'
  '^#+ .*Failure Modes:Failure Modes'
  '^#+ .*Invariants:Invariants'
  '^#+ .*Test Specification:Test Specification'
  '^#+ .*Acceptance (Criteria|Rubric):Acceptance Rubric (legacy heading Acceptance Criteria accepted)'
  '^#+ .*Product Clarity:Product Clarity (authoring record)'
  '^#+ .*Discovery:Discovery (consult log)'
)

# Template residue — strings that exist ONLY in the unfilled template. Their
# survival in a pending/active spec means the section was never filled. The
# "<!-- tpl:" marker is the template's fill grammar: every guidance comment is
# deleted at authoring, so any survivor is residue by construction.
PLACEHOLDER_SENTINELS=(
  '<!-- tpl:'
  'Title — testable, not vague'
  'one-line reason}'
  'Slice title}'
  'path/to/file.ext'
  'test_<short_name>'
  'why this is the right pattern to mirror'
  '{outcome the user can observe}'
  '{exit 0 / substring / 0 matches}'
)

scan_spec() {
  local spec="$1"
  local hits=0
  local pattern desc severity line content
  while IFS=: read -r severity desc pattern; do
    while IFS= read -r match; do
      [[ -z "$match" ]] && continue
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
  if [[ $hits -eq 0 ]]; then ok "$spec — no prohibited patterns"; fi
  return 0
}

# Family 2 — required determinism sections present + no template residue.
# BLOCK only; called solely in --staged scope (a spec being authored now).
scan_required() {
  local spec="$1"
  local miss=0 entry sec desc sentinel
  for entry in "${REQUIRED_SECTIONS[@]}"; do
    sec="${entry%%:*}"; desc="${entry#*:}"
    grep -qE "$sec" "$spec" && continue
    fail "$spec — missing required section: $desc"
    miss=$((miss + 1))
  done
  for sentinel in "${PLACEHOLDER_SENTINELS[@]}"; do
    grep -qF "$sentinel" "$spec" || continue
    fail "$spec — unfilled template placeholder: \"$sentinel\""
    miss=$((miss + 1))
  done
  if [[ $miss -eq 0 ]]; then ok "$spec — required sections present, no placeholders"; fi
  return 0
}

for spec in "${SPECS[@]}"; do
  [[ -f "$spec" ]] || continue
  scan_spec "$spec"
  case "$MODE" in --staged|staged) scan_required "$spec" ;; esac
done

if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 SPEC TEMPLATE GATE: violations found. See dispatch/write_spec.md.\n" >&2
  exit 1
fi

ok "SPEC TEMPLATE GATE: clean (${#SPECS[@]} specs)"
exit 0
