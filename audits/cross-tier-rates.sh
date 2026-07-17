#!/usr/bin/env bash
# cross-tier-rates.sh — pin numeric parity of the four rate constants
# across Zig + three TypeScript surfaces.
#
# RULE UFS extension. The shipped billing surface depends on three constants
# having identical numeric values everywhere they appear:
#
#   RUN_NANOS_PER_SEC         — per-second run rate (both postures) in nanos
#   FREE_TRIAL_END_MS         — UTC ms after which the free trial expires
#   FREE_TRIAL_STAGE_NANOS    — per-stage charge during the trial window
#
# Files (one definition site per constant per file):
#
#   src/agentsfleetd/state/tenant_billing.zig       — server enforces the charge
#   ui/packages/website/src/lib/rates.ts    — pricing page display
#   ui/packages/app/lib/types.ts            — dashboard display
#   cli/src/constants/billing.ts    — `agentsfleet doctor --json` billing block
#
# A drift between any two = a billing-display lie or a server-vs-CLI disagreement.
# Zig is the source of truth (server enforces); the three TS surfaces echo it.
#
# Fires in: make harness-verify, make harness-verify-all.
# Exits 0 clean, 1 on any drift.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

readonly NAMES=(
  RUN_NANOS_PER_SEC
  FREE_TRIAL_END_MS
  FREE_TRIAL_STAGE_NANOS
)

readonly FILES=(
  src/agentsfleetd/state/tenant_billing.zig
  ui/packages/website/src/lib/rates.ts
  ui/packages/app/lib/types.ts
  cli/src/constants/billing.ts
)

# Extract the numeric value bound to `name` in `file`. Matches definition
# sites only — `^(pub |export )?const NAME` — not usages elsewhere in the
# file. Normalises: strips underscores (Zig `1_000_000`, TS `1_000_000`),
# strips the bigint `n` suffix (TS `1_000_000n`), trims whitespace.
extract_value() {
  local file="$1" name="$2"
  grep -E "^[[:space:]]*(pub |export )?const ${name}\\b" "$file" 2>/dev/null \
    | head -1 \
    | sed -E "s|.*=[[:space:]]*||; s|[;,].*||; s|//.*||; s|[[:space:]_n]||g"
}

FAIL=0
violations=()

printf "audit-cross-tier-rates: pinning %d constants across %d files\n" \
  "${#NAMES[@]}" "${#FILES[@]}"

for name in "${NAMES[@]}"; do
  expected=""
  expected_file=""
  for file in "${FILES[@]}"; do
    if [[ ! -f "$file" ]]; then
      violations+=("MISSING_FILE: $file")
      FAIL=1
      continue
    fi
    val="$(extract_value "$file" "$name")"
    if [[ -z "$val" ]]; then
      violations+=("MISSING_CONST: $name not defined in $file")
      FAIL=1
      continue
    fi
    if [[ -z "$expected" ]]; then
      expected="$val"
      expected_file="$file"
      continue
    fi
    if [[ "$val" != "$expected" ]]; then
      violations+=("DRIFT: $name = $val in $file vs $expected in $expected_file")
      FAIL=1
    fi
  done
  if [[ -n "$expected" ]]; then
    printf "  OK   %-26s = %s\n" "$name" "$expected"
  fi
done

if [[ $FAIL -ne 0 ]]; then
  printf "\nFAIL: cross-tier rate drift detected:\n" >&2
  for v in "${violations[@]}"; do
    printf "  %s\n" "$v" >&2
  done
  printf "\nFix: align all four files on the canonical Zig value\n" >&2
  printf "     (src/agentsfleetd/state/tenant_billing.zig is the server-side enforcer).\n" >&2
  exit 1
fi

printf "OK:   all %d rate constants pin across %d files\n" \
  "${#NAMES[@]}" "${#FILES[@]}"
exit 0
