# M11_007: Reconcile ghost-table investigation — `run_side_effect_outbox`

**Prototype:** v2
**Milestone:** M11
**Workstream:** 007
**Date:** Apr 17, 2026
**Status:** PENDING
**Priority:** P2 — operator tooling cleanup; no user-visible path depends on it today, but the `zombied reconcile` subcommand currently fails at the first DB call against any provisioned schema.
**Batch:** backlog
**Depends on:** none (investigation — decides the fix shape)

---

## Overview

**Goal (testable):** Either (a) re-add `run_side_effect_outbox` to the schema and wire it into the production outbox path so `zombied reconcile` has something to reconcile, or (b) delete `src/cmd/reconcile.zig` + `src/state/outbox_reconciler.zig` + related test scaffolding as dead code left over from the M10_001 pipeline removal. The right answer depends on whether side-effect outboxing is still a product requirement post-M10; the current tree is incoherent.

**Problem:** `src/cmd/reconcile.zig` (top-level `zombied reconcile` CLI) and `src/state/outbox_reconciler.zig` both read/write a table named `run_side_effect_outbox`. That table is **not** in `schema/*.sql`. The `schema/002_vault_schema.sql:40-41` comment block explicitly lists it in the family of tables dropped during M10_001 (`core.specs`, `core.runs`, `core.run_transitions`, `core.artifacts`, `core.workspace_memories`, `core.policy_events`, `billing.usage_ledger` — `run_side_effect_outbox` is the outbox companion to `run_transitions`).

Concrete live references to the ghost table:

- `src/cmd/reconcile.zig` — 8 sites (lines 75, 99, 108, 113, 132, 159, 176, 234)
- `src/state/outbox_reconciler.zig` — 2 sites (lines 67, 72)

The `reconcile.zig` tests mock the missing table via `CREATE TEMP TABLE run_side_effect_outbox` (line 75). That's the signal that the dev who removed the real table either:

1. Intentionally deferred the reconciler removal (thinking it'd be repurposed)
2. Accidentally left it in (the TEMP TABLE mock hid the failure in unit tests)
3. Did not realise the reconciler existed at the time of pipeline removal

**Solution shape (to be decided during this workstream):**

- **Path A — Revive.** If side-effect outboxing is coming back under M10_001-successor work, re-add `schema/0NN_run_side_effect_outbox.sql` (PK UUIDv7 with CHECK, status enum, FK to current-era run identifier). Convert the TEMP-TABLE-based test in `reconcile.zig` into a real integration test with `src/db/test_fixtures_reconcile.zig` per **RULE ITF**. Wire the outbox back into whatever side-effect emission path exists today.
- **Path B — Delete.** If not coming back: remove `src/cmd/reconcile.zig`, `src/state/outbox_reconciler.zig`, the `zombied reconcile` subcommand dispatch, any systemd/Dockerfile references, and the `reconcile_*.zig` submodules under `src/cmd/reconcile/`. Drop the `make reconcile-*` targets if any.

---

## 1.0 Investigate product intent

**Status:** PENDING

### 1.1 Survey

- Check git history (`git log --all --full-history -- schema/*outbox*`) for the original `CREATE TABLE run_side_effect_outbox` and its removal commit. Capture the commit message and diff context.
- Check if side-effect outboxing appears in any spec under `docs/v2/done/`, `docs/v2/pending/`, or prior `docs/v1/**` — it may have been renamed (e.g. to the zombie session ledger introduced in M1).
- Ask the project owner whether the reconciler is load-bearing for any external workflow.

**Dimensions:**

- 1.1 PENDING — target: produce a short note in the workstream PR (under `docs/nostromo/LOG_*.md`) summarising the archaeology: when the table was added, when it was dropped, whether the code was meant to follow, whether any current feature references the reconciler.

---

## 2.0 Execute chosen path

**Status:** PENDING (gated on §1.0)

**Dimensions (Path A — revive):**

- 2A.1 PENDING — target: new `schema/0NN_run_side_effect_outbox.sql`, registered in `schema/embed.zig` + `canonicalMigrations()`. Tier-3 `make down && make up && make test-integration` passes.
- 2A.2 PENDING — target: `src/db/test_fixtures_reconcile.zig` per **RULE ITF**; existing TEMP-TABLE tests rewritten to seed via the fixture and run against the real table.
- 2A.3 PENDING — target: verify the outbox is actually written to somewhere in the current emission path (grep for producers — the reconciler is the consumer; a consumer with no producer is still dead code). If no producer exists, loop back to Path B.

**Dimensions (Path B — delete):**

- 2B.1 PENDING — target: `rm src/cmd/reconcile.zig src/state/outbox_reconciler.zig src/cmd/reconcile/*.zig`. Remove the `reconcile` arm from the CLI dispatch in `src/main.zig` (or wherever `zombied reconcile` is wired). Drop any outbox-reconciler make targets.
- 2B.2 PENDING — target: orphan sweep (RULE ORP). `grep -rn "run_side_effect_outbox\|outbox_reconciler\|zombied reconcile"` across src/, docs/, schema/, scripts/, Dockerfile* — zero non-historical hits.
- 2B.3 PENDING — target: `zig build` + `make test` green at the same baseline as before the deletion (120X/132Y passed, 1 pre-existing REGISTRY failure — identical count minus however many reconciler-specific tests went away).

---

## 3.0 Acceptance Criteria

- [ ] §1.0 archaeology note committed under `docs/nostromo/LOG_*.md`.
- [ ] One of {Path A complete, Path B complete} — not both, not neither.
- [ ] `grep -rn "CREATE TEMP TABLE run_side_effect_outbox"` returns zero hits after this workstream.
- [ ] Build + full test suite green; baseline regression count matches.
- [ ] RULE ITF referenced and followed if Path A.

---

## Applicable Rules

- **RULE ITF** — Integration tests use real schema via `test_fixtures_<name>.zig` (enforced in Path A).
- **RULE ORP** — Orphan sweep across schema/Zig/JS/tests/docs when a symbol is renamed or deleted (critical in Path B).
- **RULE NDC** — No dead code; the code currently references a table that does not exist, which is the canonical form of dead code.
- Schema Table Removal Guard — Path A adds a table; additive, so the guard prints `additive only, no teardown path required` per `AGENTS.md`.

---

## Out of Scope

- Redesigning the whole side-effect emission model. If Path A turns up a need for broader redesign, close this workstream with the archaeology note and spawn a new milestone.
- Anything unrelated to `run_side_effect_outbox`. The other dropped-table families (`run_transitions`, `usage_ledger`, `policy_events`) were likewise referenced by M10 code that was presumably also cleaned up — this workstream touches only the outbox reconciler.
