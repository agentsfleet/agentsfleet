#!/usr/bin/env bash
# dispatch/write_zig.sh — the Zig dispatch, DETERMINISTIC façade.
#
# Pairs with dispatch/write_zig.md (the LATENT façade — prose the agent reads).
# This .sh runs the mechanically-checkable subset of the Zig discipline and
# emits ONE verdict block. The latent trigger ("about to write a *.zig file")
# dispatches here; everything below is deterministic and re-runnable — same
# input, same verdict, no calendar/branch dependence.
#
#   dispatch/write_zig.sh <file.zig> [...]   # explicit targets (EXECUTE)
#   dispatch/write_zig.sh --staged           # staged *.zig (CONFORM, pre-commit)
#
# Layering:  AGENTS.md → write_zig.md (latent) → write_zig.sh (this) → audits/*.sh
# Signals:   🟢 pass · 🔴 fail (blocks) · 🔵 DECIDE (judgment; blocks the TURN, not the script)
# Exit:      0 = mechanical gates pass · 1 = ≥1 failed · 2 = usage error.

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

dispatch_init "ZIG" '*.zig'
dispatch_resolve_files "$@"
dispatch_header

# ── deterministic gates ────────────────────────────────────────────
dispatch_length_gate 350                                       # FLL — write_zig.md §Length
# UFS — universal rule enforced once by write_any (see write_any.sh); not re-run here
dispatch_run_helper "DEINIT" "deinit-pairs.sh" "--staged" # write_zig.md §Multi-Step Init
dispatch_run_helper "SQLMOD" "sql-mod.sh"      "--staged" # write_zig.md §SQLMOD — inline SQL → domain sql.zig

# ── delegations (checker lives in the product repo, not dotfiles) ───
dispatch_delegate "PUB"      "make lint (zlint unused-decls)"
dispatch_delegate "DRAIN"    "make check-pg-drain (lint-zig.py)"
dispatch_delegate "XCOMPILE" "zig build -Dtarget=x86_64-linux && aarch64-linux"

# ── judgment gates (no script can decide; agent states verdict in chat) ──
dispatch_judgment "TGU"  "result type with distinct failure modes? union(enum) with payload, not optional-field struct"
dispatch_judgment "ARCH" "naming a stream/queue/schema? grep docs/architecture/ first"
dispatch_judgment "FSD"  "new/reshaped *.zig file? file-as-struct vs operations-over-value — decide at PLAN, one-sentence why-not"
dispatch_judgment "DIDEM" "type with a cleanup contract? a test proving deinit is idempotent (or single-shot ownership asserted)"

dispatch_verdict
