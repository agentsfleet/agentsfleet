#!/usr/bin/env bash
# dispatch/write_rust.sh — the Rust dispatch, DETERMINISTIC façade.
#
# Pairs with dispatch/write_rust.md (the LATENT façade — prose the agent reads).
# This .sh runs the mechanically-checkable subset of the Rust discipline and
# emits ONE verdict block. The latent trigger ("about to write a *.rs file")
# dispatches here; everything below is deterministic and re-runnable.
#
#   dispatch/write_rust.sh <src/file.rs> [...]   # explicit targets (EXECUTE)
#   dispatch/write_rust.sh --staged              # staged *.rs (CONFORM, pre-commit)
#
# Layering:  AGENTS.md → write_rust.md (latent) → write_rust.sh (this) → audits/*.sh
# Signals:   🟢 pass · 🔴 fail (blocks) · 🤔 DECIDE (judgment; blocks the TURN, not the script) · 🟣 delegated
# Exit:      0 = mechanical gates pass · 1 = ≥1 failed · 2 = usage error.
#
# Note: Rust had no deterministic façade at all until this file, which is why
# the error standard the operating model calls mandatory was enforced by review
# alone. ERR-RS carries the two clauses a machine can decide from the text — a
# map_err that stringifies its own cause, and an error type with no Result alias
# beside it. The rest of write_rust.md stays judgment on purpose: ownership,
# justified `unsafe`, feature combinations, and whether a source() implementation
# returns its own kind all need a call site read beside the diff, and a gate that
# guesses at those is a gate people learn to ignore.
#
# UFS over *.rs is NOT re-run here. It is wired once in its home façade
# (write_any), and the universal-code carve-out makes that row satisfy the tag
# wherever it appears — running it twice would report one violation as two.

source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

dispatch_init "RUST" '*.rs'
dispatch_resolve_files "$@"
dispatch_header

# ── deterministic gates ────────────────────────────────────────────
dispatch_run_helper "ERR-RS" rust-error.sh --staged

dispatch_verdict
