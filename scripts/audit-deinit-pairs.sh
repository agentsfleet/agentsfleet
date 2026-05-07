#!/usr/bin/env bash
# audit-deinit-pairs.sh — verify init/deinit pairing for Zig structs.
#
# Gate body: docs/gates/lifecycle.md
# Standalone tool — not wired into make lint or pre-commit (per Captain's
# direction, gates fire pre-emptively via printed proof-blocks during
# agent work; lint integration is intentionally absent).
#
# Lifecycle methods recognized (per LIFECYCLE_PATTERNS.md §10A.LC2):
#   init / deinit / close / release / destroy / shutdown / dispose / free
#
# Refined pairing rule:
#   A `pub fn init` requires a matching cleanup method (deinit / close /
#   release / destroy / shutdown / dispose / free) IN THE SAME FILE only
#   when the init either:
#     (a) allocates inside its body
#         (alloc/create/dupe/dupeZ/initCapacity/allocPrint/realloc, plus
#          std.fs.File.open|create, std.Thread.spawn, etc.), OR
#     (b) returns a heap pointer (`*T` or `!*T` or `*Self` or `!*Self`).
#
#   Pure value-type init (e.g. `pub fn init() Self` returning a struct
#   literal with no heap state) is permitted without deinit per
#   LIFECYCLE_PATTERNS.md §4 decision matrix. Reviewer responsibility for
#   the harder case where a struct stores ArrayList/HashMap fields whose
#   first append/insert happens at use-site rather than init time.
#
# Heuristics implemented:
#   1. Init/deinit pair existence (with allocation refinement) — BLOCKING.
#   2. defer X.free(buf) + errdefer X.free(buf) on same target adjacent
#      lines — BLOCKING (per §10A.LC4).
#      Skips trivial optional bindings (`if (X) |v| free(v)`) where the
#      captured name is a single 1-2 char identifier — those are loop-
#      style bindings, not the same allocation.
#
# Heuristics deferred to reviewer:
#   - Errdefer placement (last errdefer must precede last protected alloc).
#   - Idempotency of deinit (requires test inspection).
#   - Arena leakage into long-lived structs.
#   - ArrayList/HashMap field with no init-time alloc but use-time growth.
#
# Modes:
#   --staged   diff-scope: only files in `git diff --cached`
#   --all      (default) src/ scan including *_test.zig
#
# Exits 0 clean, 1 on blocking findings.

set -euo pipefail

MODE="${1:-${SCOPE:-all}}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

FAIL=0
fail() { printf "FAIL: %s\n" "$*" >&2; FAIL=1; }
ok()   { printf "OK:   %s\n" "$*"; }
note() { printf "NOTE: %s\n" "$*"; }

# ---------------------------------------------------------------------------
# 1. Gather Zig files in scope. Tests INCLUDED — same lifecycle rules apply.
# ---------------------------------------------------------------------------
case "$MODE" in
  --staged|staged)
    mapfile -t FILES < <(git diff --cached --name-only --diff-filter=ACMRT \
      | grep -E '^src/.*\.zig$' || true)
    ;;
  --all|all)
    mapfile -t FILES < <(find src -type f -name '*.zig' 2>/dev/null || true)
    ;;
  *)
    printf "usage: %s [--staged|--all]\n" "$0" >&2
    exit 64
    ;;
esac

if [[ ${#FILES[@]} -eq 0 ]]; then
  ok "no zig source files in scope ($MODE)"
  exit 0
fi

# ---------------------------------------------------------------------------
# Helper regexes.
# ---------------------------------------------------------------------------
CLEANUP_RE='pub fn (deinit|close|release|destroy|shutdown|dispose|free)\('
# Heap-pointer return on init signature: `) *T` or `) !*T` or returning *Self.
HEAP_RETURN_RE='\)[[:space:]]*!?\*([A-Z@]|Self\b)'
# Allocation patterns inside init body.
ALLOC_RE='\.(alloc|create|dupe|dupeZ|initCapacity|allocPrint|allocSentinel|reAlloc|realloc)\(|std\.fs\.(File|cwd\(\))\.(open|create)|Thread\.spawn|\.allocator\(\)|ArrayList(Unmanaged)?\(.*\)\.init|HashMap(Unmanaged)?\(.*\)\.init|StringHashMap(Unmanaged)?\(.*\)\.init'

# ---------------------------------------------------------------------------
# 2. Per-init evaluation. Each `pub fn init` is checked individually:
#    look at the signature for heap-return; look at next ~50 lines of body
#    for allocation. Either positive triggers the deinit requirement.
# ---------------------------------------------------------------------------
files_scanned=0
inits_total=0
inits_value_type=0
inits_requires_cleanup=0
inits_unpaired=0

for f in "${FILES[@]}"; do
  [[ -f "$f" ]] || continue
  files_scanned=$((files_scanned + 1))

  has_cleanup=0
  if grep -qE "$CLEANUP_RE" "$f"; then
    has_cleanup=1
  fi

  while IFS=: read -r ln sig; do
    [[ -z "$ln" ]] && continue
    inits_total=$((inits_total + 1))

    heap=0
    if grep -qE "$HEAP_RETURN_RE" <<<"$sig"; then
      heap=1
    fi

    # Body window: from sig line to next pub-fn / fn / EOF, capped at +50.
    body=$(awk -v start="$ln" '
      NR <= start { next }
      /^[[:space:]]*(pub )?fn [a-zA-Z_]/ { exit }
      NR > start + 50 { exit }
      { print }
    ' "$f")

    allocates=0
    if grep -qE "$ALLOC_RE" <<<"$body"; then
      allocates=1
    fi

    if [[ $heap -eq 0 && $allocates -eq 0 ]]; then
      inits_value_type=$((inits_value_type + 1))
      continue
    fi

    inits_requires_cleanup=$((inits_requires_cleanup + 1))
    if [[ $has_cleanup -eq 0 ]]; then
      inits_unpaired=$((inits_unpaired + 1))
      reason=""
      [[ $heap -eq 1 ]] && reason="heap-return"
      [[ $allocates -eq 1 ]] && reason="${reason:+$reason+}allocates"
      fail "$f:$ln — \`pub fn init\` requires cleanup ($reason) but no cleanup method in file"
      printf "        > %s\n" "$(echo "$sig" | sed 's/^[[:space:]]*//')" >&2
    fi
  done < <(grep -nE 'pub fn init\(' "$f")
done

# ---------------------------------------------------------------------------
# 3. Detect defer/errdefer conflicts on the same allocation target.
#    Skip trivial optional-binding names (1-2 char like `v`, `it`) which
#    are local to each `if (x) |v|` scope and not actual conflicts.
# ---------------------------------------------------------------------------
for f in "${FILES[@]}"; do
  [[ -f "$f" ]] || continue
  awk -v F="$f" '
    function extract_target(line,    s, p, q) {
      p = index(line, ".free(");
      if (p == 0) p = index(line, ".destroy(");
      if (p == 0) return "";
      s = substr(line, p);
      p = index(s, "(");
      q = index(s, ")");
      if (p == 0 || q == 0 || q <= p + 1) return "";
      return substr(s, p + 1, q - p - 1);
    }
    function is_optional_binding(line) {
      # Pattern: `if (X) |Y| ... free(Y)` — Y is a local capture, not a real shared target.
      return (line ~ /\|[a-z_]+\|/);
    }
    /^[[:space:]]*defer[[:space:]].*\.(free|destroy)\(/ {
      tgt = extract_target($0);
      if (tgt != "" && length(tgt) > 2 && !is_optional_binding($0)) defers[NR] = tgt;
    }
    /^[[:space:]]*errdefer[[:space:]].*\.(free|destroy)\(/ {
      tgt = extract_target($0);
      if (tgt != "" && length(tgt) > 2 && !is_optional_binding($0)) errdefers[NR] = tgt;
    }
    END {
      for (d in defers) {
        for (e in errdefers) {
          gap = d - e; if (gap < 0) gap = -gap;
          if (defers[d] == errdefers[e] && gap <= 4) {
            printf "%s:%d:%d: defer/errdefer conflict on %s\n", F, d, e, defers[d];
          }
        }
      }
    }
  ' "$f" | while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    fail "defer/errdefer conflict (same target, ≤4 line window): $line"
  done
done

# ---------------------------------------------------------------------------
# 4. Verdict.
# ---------------------------------------------------------------------------
ok "scanned $files_scanned files; $inits_total init methods ($inits_value_type value-type, $inits_requires_cleanup require cleanup, $inits_unpaired unpaired)"
if [[ $FAIL -ne 0 ]]; then
  printf "\n🔴 LIFECYCLE GATE: violations found. See docs/gates/lifecycle.md.\n" >&2
  exit 1
fi
ok "LIFECYCLE GATE: clean"
exit 0
