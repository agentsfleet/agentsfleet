#!/usr/bin/env bash
# dispatch/write_ts_adhere_bun.sh — the TypeScript / Bun dispatch, DETERMINISTIC façade.
#
# Pairs with dispatch/write_ts_adhere_bun.md (the LATENT façade — prose the agent reads).
# This .sh runs the mechanically-checkable subset of the TS/Bun discipline and
# emits ONE verdict block. The latent trigger ("about to write a *.ts/*.tsx/*.js/*.jsx
# file") dispatches here; everything below is deterministic and re-runnable — same
# input, same verdict, no calendar/branch dependence.
#
#   dispatch/write_ts_adhere_bun.sh <file.ts> [...]   # explicit targets (EXECUTE)
#   dispatch/write_ts_adhere_bun.sh --staged          # staged TS/JS (CONFORM, pre-commit)
#
# Layering:  AGENTS.md → write_ts_adhere_bun.md (latent) → write_ts_adhere_bun.sh (this) → audits/*.sh
# Signals:   🟢 pass · 🔴 fail (blocks) · 🔵 DECIDE (judgment; blocks the TURN, not the script) · ⚪ delegated
# Exit:      0 = mechanical gates pass · 1 = ≥1 failed · 2 = usage error.

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

dispatch_init "TS" '*.ts' '*.tsx' '*.js' '*.jsx'
dispatch_resolve_files "$@"
dispatch_header

# ── deterministic gates ────────────────────────────────────────────
dispatch_length_gate 350                                       # FLL — write_ts_adhere_bun.md §13 (file ≤ 350)
# UFS — universal rule enforced once by write_any (see write_any.sh); not re-run here

# ── delegations (checker lives in the product repo, not dotfiles) ───
dispatch_delegate "TSC" "make lint (oxlint + tsc — const/import/ordering/naming/anti-patterns)"
dispatch_delegate "UIS" "make lint (msid-ui.sh — design-system primitive over raw HTML)"
dispatch_delegate "DTK" "make lint (design-tokens.sh — token utility over arbitrary value)"

# ── judgment gates (no script can decide; agent states verdict in chat) ──
dispatch_judgment "FSD" "new *.ts/*.tsx under src|app|lib? class vs factory vs functions-module vs type-only — decide at PLAN, one-sentence why-not"
dispatch_judgment "TGU" "result with distinct failure modes? discriminated union ({ok:true…}|{ok:false…}), not optional-field struct"
dispatch_judgment "TSJ" "TS/Bun convention choices (Bun-native over Node-compat, file ordering, error style: throw XOR Result per module)? justify any Node-compat fallback against a concrete dependency constraint"

dispatch_verdict
