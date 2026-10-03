<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M210_004: Memory lives behind one store trait in agentsfleetd, a workspace admin lets chosen fleets read and publish memory across the workspace, and a store flip copies every entry before it switches

**Prototype:** v2.0.0
**Milestone:** M210
**Workstream:** 004
**Date:** Oct 03, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Indy's memory decisions of Oct 03: the twenty-first fleet should start from what twenty others learned, and a workspace must be able to move stores without losing a memory
**Categories:** API, UI
**Batch:** B2 — folded into M210_002 and shipped in its Pull Request, by Indy's call ("Fold into this PR")
**Branch:** `feat/m210-agent-loop-hosted-tools`
**Folded-into:** `M210_002`
**Baseline revision:** `4339afb59fe83a20fb643004e432b9755e1b14a7`
**Test Baseline:** pending — measured before the Pull Request, with M210_002's
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M210_002 (`afr_memory::MemoryBackend` and `Hydrated`, committed `bab2368e0`; the decisions recorded in `da27c6623`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 03, 2026) from Indy's in-session decisions and a source trace of the branch at `da27c6623`
**Canonical architecture:** `docs/architecture/runner_fleet.md` §"Memory backends and scope", §"Memory continuity"; `docs/architecture/memory.md` §1–§5

---

## Overview

**Goal (testable):** `test_shared_memory_reaches_only_granted_fleets` — a fleet a workspace admin let read shared memory hydrates the workspace-visible entries other fleets published, each naming its writer; a fleet without that grant hydrates only its own.
**Problem:** Memory is keyed by fleet alone, so a new fleet starts empty even when twenty others learned what it needs (Indy's example: ticket resolutions from ten fleets and pull-request history from ten more). Postgres is wired straight into `agentsfleetd`'s memory module, so moving a workspace to turbopuffer or mem0 would mean rewriting that module instead of adding a store. Nothing lets an admin decide which fleets share.
**Solution summary:** A new crate, `afd_memory`, holds `MemoryStore`, the one trait every memory read and write in `agentsfleetd` goes through, with today's Postgres code as its only implementation. `flip` moves a workspace from one store to another and is proved against an in-memory store; no endpoint calls it and no vendor exists until one is chosen. Every entry keeps its writer fleet in its identity and gains a visibility, fleet or workspace. A forward migration adds `workspace_id` and `workspace_visible` to `memory.memory_entries`, and two grants to `core.fleets` that a workspace admin sets through a new tenant route and the fleet page's memory panel. A granted reader's hydrate carries the workspace's shared entries with their writer, and a recall the window cannot fill asks `agentsfleetd`, a capped number of times per run. The runner still reaches memory only through the runner API.

## PR Intent & comprehension handshake

- **PR title (eventual):** the M210 Pull Request's (owned by `M210_002`); this workstream adds memory shared by workspace grant behind one store
- **Intent (one sentence):** A workspace admin can let fleets learn from each other, every shared fact names the fleet that wrote it, and a workspace can later move to another memory store without losing a memory.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` — §"Memory backends and scope" (both decisions), §"Memory continuity" (hydrate, push, fencing) and the multi-lease isolation invariant the writer-in-identity rule keeps true.
2. `docs/architecture/memory.md` — §1 scope, §2 the `memory_runtime` role that holds no grant on `core.*`, §5 categories.
3. `rustd/crates/afd_memory/src/postgres/mod.rs` — the Postgres store `afd_fleet`'s memory module became; `window.rs` beside it is the hydration window it keeps.
4. `rustd/crates/afd_api_tenant/src/handler/fleet/memory.rs` — the tenant memory routes the access route sits beside.
5. `rustd/crates/afr_memory/src/hydrated.rs` — the runner's side, which gains shared entries and the recall miss.
6. `dispatch/write_sql.md` — §SCHEMA GUARD: at `VERSION` 0.51.1 a schema change is a forward migration over live data.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/afd_memory/` | CREATE | `MemoryStore`, the Postgres store moved from `afd_fleet`, `flip`, the crate's error type, an in-memory store behind `test-util` |
| `rustd/crates/afd_fleet/` (`Cargo.toml`, `src/lib.rs`, `src/memory/`, `src/lease/memory.rs`) | EDIT / DELETE | The memory module leaves for `afd_memory`; the lease paths call the trait |
| `schema/926_memory_entries_workspace_scope.sql`, `schema/927_fleet_memory_access.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | Forward migrations with their `GRANT`s, registered in order |
| `rustd/crates/afd_wire/src/memory.rs`, `src/paths.rs`, `src/fleet.rs` | EDIT | Visibility on a delta; shared entries and the publish grant on the hydrate reply; the recall verb; the grants on a fleet |
| `rustd/crates/afd_api_runner/src/handler/runner/memory.rs`, `rustd/crates/afd_api_tenant/src/handler/fleet/` (`memory.rs`, `memory_access.rs`), `rustd/crates/afd_api/tests/` | EDIT / CREATE | The runner recall verb; the tenant access route; their suites |
| `rustd/crates/afr_memory/`, `rustd/crates/afr_tools/src/memory.rs`, `rustd/crates/afr_supervisor/src/memory.rs`, `rustd/crates/afr_supervisor/src/lease_loop.rs` | EDIT | The `visibility` argument, shared entries the run cannot overwrite, the capped recall miss |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/MemoryPanel.tsx`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/actions.ts`, `ui/packages/app/lib/types.ts`, `ui/packages/app/tests/` | EDIT / CREATE | Two access toggles and a "shared by" mark |
| `public/openapi.json` | EDIT | Regenerated from the build |
| `docs/architecture/runner_fleet.md`, `docs/architecture/memory.md`, `docs/architecture/capabilities.md` | EDIT | From "decided" to "built" |
| Ripple, added at EXECUTE: `rustd/crates/agentsfleetd/tests/integration_runner_e2e.rs` (the hydrate reply's new fields), `rustd/crates/afd_fleet/src/` (`error/`, `lease/{pull,fence,test_dead}.rs`), `afd_fleet/tests/`, `afd_http/` (`Cargo.toml`, `src/services/{memory,leasing}.rs`, `src/handler/refusable.rs`, `src/route/{fleet,runner}.rs`), `afd_api_{tenant,runner}/` (`Cargo.toml`, `src/lib.rs`, `src/openapi.rs`), `afd_api/` (`Cargo.toml`, `src/lib.rs`), `agentsfleetd/` (`Cargo.toml`, `src/plane.rs`, `src/plane/services.rs`, two `tests/`), `afd_bench/` (`Cargo.toml`, one stage), `afd_fleet_lifecycle/tests/integration_purge_ledger_identity.rs`, `afr_supervisor/src/` (`client.rs`, `memory/`, `lease_loop/`, `test_support*`), `afr_agent/src/engine.rs` and four tests, `afr_providers/` (`Cargo.toml`, one test), `afr_tools/src/runtime.rs`, `afd_fleet_lifecycle/src/{read,sql}.rs` and `afd_api_tenant/src/handler/fleet/detail.rs` (the grants on the fleet detail), `ui/packages/app/` (`fleets/[id]/page.tsx`, `MemoryPanel.test.tsx`, `lib/api/memory.ts`, `lib/auth/scopes.ts`) | EDIT | Import, wiring and fixture lines the move, the error lift, the two routes, slot 926's new column, `AgentRun`'s seed and the panel's initial grants force; no other change |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (grant and visibility spellings, the recall cap, the shared window budget), STS (visibility is a boolean column, never a defaulted string), SGR (each migration grants `memory_runtime` what it needs), NSQ, ITF (the migration and store suites run on the real schema), ORP (the module move leaves no reference behind), OWN, NDC, TCF, TST-NAM.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — `afd_memory` declares one `ErrorKind` through `afd_core::error_shell!`.
- `dispatch/write_sql.md` — forward migrations only; `820_memory_entries.sql` is frozen history.
- `docs/REST_API_DESIGN_GUIDELINES.md` — the access route's shape, scope and errors; `docs/LOGGING_STANDARD.md` — `flip` brackets its copy with a started and a completed or failed pair.
- `dispatch/write_ts_adhere_bun.md` — the panel uses design-system primitives and token utilities.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — two new slots | `926` and `927` are forward `ALTER`s; output `migration:schema/926_…`, `migration:schema/927_…` |
| RUST ERR / LOGGING / UFS | yes | `error_shell!`; `flip` events with `error_code` on failure; constants for every bound |
| UI GATE / DESIGN TOKEN | yes — `MemoryPanel.tsx` | Switch and badge primitives, token utilities only |
| File & Function Length (≤350/≤50/≤70) | yes | The Postgres store splits by concern (window, upsert, search, page) as it moves |
| Architecture consult | yes | The two pages above say what is built in the same commit as the code |

## Prior-Art / Reference Implementations

- **Reference:** `afd_fleet`'s memory module, now `rustd/crates/afd_memory/src/postgres/` — moved behind the trait, not rewritten; its window and eviction rules stay byte-for-byte.
- **Reference:** `rustd/crates/afr_providers/src/registry.rs` and `connect.rs` — one trait, a registry choosing the implementation, a refusal for a name with no implementation.
- **Reference:** mem0 (memories scoped by user, agent and run, filtered at read), Letta (per-agent memory plus blocks shared on purpose), ChatGPT's project-only memory — sharing is a grant, never the default.

## Sections (implementation slices)

### §1 — One store trait in agentsfleetd, with Postgres behind it

Every memory read and write in `agentsfleetd`, from the lease paths and both route families, goes through `afd_memory::MemoryStore`; the Postgres store is today's module moved, so a fleet with no grant sees no change. **Implementation default:** `async_trait` with `dyn MemoryStore` held once by `agentsfleetd`, because the store is chosen per workspace at run time.

- **Dimension 1.1** — The Postgres store, reached only through the trait, keeps today's window, upsert, sweep and eviction → Test `test_postgres_store_keeps_fleet_memory_behaviour` — DONE (`rustd/crates/afd_memory/tests/integration_store.rs`)

### §2 — A flip copies every entry, then switches

`flip(workspace, from, to)` copies every entry of the workspace's fleets, writer and visibility kept, into `to`, then makes `to` the workspace's store. While it copies, a push lands in both stores, and a copied row never replaces a newer one, so no write is lost and none lands only in the old store. A failure before the switch leaves `from` the store, untouched. No endpoint calls `flip`; it is proved against the in-memory store.

- **Dimension 2.1** — A flip copies every entry with its writer and visibility, then switches → Test `test_flip_copies_every_entry_then_switches` — DONE (`rustd/crates/afd_memory/src/flip_tests.rs`)
- **Dimension 2.2** — A copy that fails midway leaves the old store in place and unchanged → Test `test_failed_flip_keeps_the_old_store` — DONE (`rustd/crates/afd_memory/src/flip_tests.rs`)
- **Dimension 2.3** — A push during the copy reaches both stores and survives the copy → Test `test_push_during_flip_reaches_both_stores` — DONE (`rustd/crates/afd_memory/src/flip_tests.rs`)

### §3 — Every entry keeps its writer; visibility says who reads it

`926` adds `workspace_id` (backfilled from `core.fleets`, then `NOT NULL`, cascading with the workspace) and `workspace_visible BOOLEAN NOT NULL DEFAULT false`; the unique `(key, fleet_id)` already is the `(workspace_id, fleet_id, key)` identity, since a fleet has one workspace. `memory_store` takes `visibility`, `fleet` by default. A fleet without the publish grant is refused at the tool before the push, and `agentsfleetd` skips such a delta and counts it, so the grant holds even against a runner that skips the tool's check.

- **Dimension 3.1** — The migration gives every existing row its workspace and leaves it fleet-visible; a rerun changes nothing → Test `test_memory_migration_backfills_workspace` — DONE (`rustd/crates/afd_memory/tests/integration_migration.rs`)
- **Dimension 3.2** — `visibility: workspace` from a fleet without publish is refused before the push → Test `test_workspace_store_needs_publish` — DONE (`rustd/crates/afr_tools/src/memory/shared_tests.rs`)
- **Dimension 3.3** — `agentsfleetd` skips a workspace-visible delta from a fleet without publish → Test `test_push_skips_an_unpublished_share` — DONE (`rustd/crates/afd_memory/tests/integration_store.rs`)

### §4 — A workspace admin grants read and publish; readers see the writer

`927` adds `memory_reads_workspace` and `memory_publishes_workspace` (`BOOLEAN NOT NULL DEFAULT false`) to `core.fleets`. `PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}/memory-access` sets them under `fleet:write`. A granted reader's hydrate adds the workspace-visible entries of its workspace's other fleets, newest first within `HYDRATE_SHARED_BYTES` and after its own window, each naming the writer fleet and when it wrote; recall shows the writer. A run cannot forget or overwrite another fleet's entry.

- **Dimension 4.1** — A granted reader hydrates other fleets' shared entries with their writer; an ungranted one hydrates none → Test `test_shared_memory_reaches_only_granted_fleets` — DONE (`rustd/crates/afd_memory/tests/integration_shared.rs`)
- **Dimension 4.2** — The access route sets both grants under `fleet:write` and refuses a token without it → Test `test_memory_access_route_needs_fleet_write` — DONE (`rustd/crates/afd_api/tests/integration_fleet_memory_access.rs`)
- **Dimension 4.3** — Recall names the writer of a shared entry → Test `test_recall_names_the_writer_of_a_shared_entry` — DONE (`rustd/crates/afr_tools/src/memory/shared_tests.rs`)
- **Dimension 4.4** — Forgetting another fleet's entry answers that nothing of this fleet's is under that key → Test `test_forget_leaves_another_fleets_entry` — DONE (`rustd/crates/afr_tools/src/memory/shared_tests.rs`)

### §5 — Recall beyond the window

`POST /v1/runners/me/memory/{fleet_id}/recall` searches the fleet's own entries and, for a granted reader, the workspace's shared ones, key matches first, fenced like the push. The runner asks only when the window answers fewer than the limit, at most `RECALL_MISS_CAP` times per run, and merges without duplicates; past the cap, or when the call fails, recall answers from the window.

- **Dimension 5.1** — A recall the window cannot fill asks once and merges the answer without duplicates → Test `test_recall_miss_asks_agentsfleetd_once` — DONE (`rustd/crates/afr_memory/src/shared_tests.rs`)
- **Dimension 5.2** — A miss past the cap answers from the window with no call → Test `test_recall_miss_cap_answers_from_the_window` — DONE (`rustd/crates/afr_memory/src/shared_tests.rs`)

### §6 — The fleet page's memory panel

The panel gains two toggles, "Read shared memory" and "Publish to shared memory", shown to a member who holds `fleet:write`, and marks each workspace-visible entry with "shared". No new page.

- **Dimension 6.1** — The toggles call the access route and reflect its answer → Test `test_memory_panel_toggles_access` — DONE (`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/MemoryPanel.test.tsx`)
- **Dimension 6.2** — An admin grants publish and read, and the reader's panel lists the published entry → Test `test_e2e_admin_shares_memory_across_fleets`

## Interfaces

```
afd_memory::MemoryStore (async): window(fleet, reads) · upsert(fleet, publishes, deltas) → { stored, skipped }
                                 search(fleet, reads, query, limit) · page · forget      (Postgres: today's module)
afd_memory::flip(workspace, from, to) → Ok after copy and switch | Err with `from` still the store
MemoryDelta { key, content, category, visibility: "fleet" | "workspace" }        absent visibility ⇒ fleet
MemoryHydrateResponse { memory, shared: [{ key, content, category, writer_fleet_id, writer_fleet_name, updated_at }], publish }
POST  /v1/runners/me/memory/{fleet_id}/recall { lease_id, fencing_token, query, limit } → { memory, shared }
PATCH /v1/workspaces/{workspace_id}/fleets/{fleet_id}/memory-access { read, publish } → 200 { read, publish }   fleet:write
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Flip copy fails | Target store refuses or drops | `memory_flip_failed` with `error_code`; the old store stays the store, unchanged (Dimension 2.2) |
| Push during a flip | A run settles mid-copy | The push reaches both stores and the copy cannot overwrite it (Dimension 2.3) |
| Unpublished share | Runner skips the tool's check, or the grant was revoked mid-run | The delta is skipped and counted in `skipped`; the rest of the push stores (Dimension 3.3) |
| Reader not granted | Admin has not granted read | Hydrate and recall carry no shared entry (Dimension 4.1) |
| Recall verb unavailable | `agentsfleetd` blip | Recall answers from the window; the run continues (Dimension 5.2) |
| Migration rerun | A deploy retried mid-migration | `926` and `927` are idempotent; a rerun changes nothing (Dimension 3.1) |
| Writer fleet deleted | A tenant deletes a publishing fleet | Its entries, shared ones too, go with it, as they do today |

## Invariants

1. The runner holds no credential for any memory store — enforced by `agentsfleet_runner/tests/dependency_graph.rs`, which fails if a runner crate links a datastore crate.
2. Every entry has one writer fleet, and no fleet writes another's entry — enforced by the unique `(key, fleet_id)` and a push scoped to the lease's own fleet, derived by `agentsfleetd`.
3. A fleet without the read grant never receives another fleet's entry — enforced in the store's window and search queries; Dimension 4.1 proves it.
4. A workspace-visible entry exists only for a fleet holding the publish grant at push time — enforced at the push; Dimension 3.3.
5. A flip neither loses a write nor leaves one only in the old store — enforced by the dual write and the newer-row rule; Dimensions 2.1–2.3.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `memory_flip_started` / `memory_flip_completed` / `memory_flip_failed` (daemon log) | ops | A flip begins, switches or fails | workspace id, store names, entries copied, `error_code` | No entry content | `test_failed_flip_keeps_the_old_store` |
| `memory_access_changed` (daemon log, info) | ops | An admin changes a grant | workspace id, fleet id, read, publish | No entry content | `test_memory_access_route_needs_fleet_write` |
| Refused shares counted in the push reply's `skipped` | ops | A delta is skipped for lack of publish | count | Count only | `test_push_skips_an_unpublished_share` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_postgres_store_keeps_fleet_memory_behaviour` | today's window, upsert, sweep and eviction cases through the trait → the same rows as before the move |
| 2.1 | unit | `test_flip_copies_every_entry_then_switches` | 3 fleets, 9 entries, 2 shared → target holds 9 with writer and visibility; target is the store |
| 2.2 | unit | `test_failed_flip_keeps_the_old_store` | target fails on entry 5 → source is the store, its 9 entries unchanged |
| 2.3 | unit | `test_push_during_flip_reaches_both_stores` | push of `k`@t2 while the copy holds `k`@t1 → both stores end at t2 |
| 3.1 | integration | `test_memory_migration_backfills_workspace` | populated table, migration run twice → every row has its fleet's workspace, `workspace_visible` false |
| 3.2 | unit | `test_workspace_store_needs_publish` | hydrate reply `publish: false`, store `visibility: workspace` → `[workspace_memory_not_granted]`, nothing pending |
| 3.3 | integration | `test_push_skips_an_unpublished_share` | fleet without publish pushes one shared and one fleet delta → stored 1, skipped 1 |
| 4.1 | integration | `test_shared_memory_reaches_only_granted_fleets` | fleets A (publish), B (read), C (none) → B hydrates A's shared entry with A's name; C hydrates none |
| 4.2 | integration | `test_memory_access_route_needs_fleet_write` | admin token → 200 and both grants set; `fleet:read` token → 403 |
| 4.3 | unit | `test_recall_names_the_writer_of_a_shared_entry` | shared `deploy_target` from `ticket-3` → recall line names `ticket-3` |
| 4.4 | unit | `test_forget_leaves_another_fleets_entry` | forget a key only another fleet's shared entry holds → "nothing remembered under", entry kept |
| 5.1 | unit | `test_recall_miss_asks_agentsfleetd_once` | window has 1 of limit 5, verb returns 3 (1 duplicate) → 1 call, 3 lines |
| 5.2 | unit | `test_recall_miss_cap_answers_from_the_window` | `RECALL_MISS_CAP` misses, then one more → no further call; window answer |
| 6.1 | unit | `test_memory_panel_toggles_access` | toggle publish → access action called with `publish: true`; panel shows the reply |
| 6.2 | e2e | `test_e2e_admin_shares_memory_across_fleets` | admin grants A publish and B read; A's shared entry appears on B's panel marked "shared" |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Shared memory reaches only granted fleets, through one store (§1, §3, §4) | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | All memory SQL lives in `afd_memory` (§1) | `grep -rln "memory\.memory_entries" rustd/crates --include='*.rs' \| grep -v '^rustd/crates/afd_memory/'` | no output | P0 | |
| R3 | The published document carries the new shapes (§3–§5) | `cd rustd && cargo test -p afd_api --features test-util,openapi --test http_substrate test_openapi_build_is_the_source` | exit 0 | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from this table or a folded spec's | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Every required check passes before the Pull Request is ready; a P1 ❌ needs an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_fleet/src/memory/` | `test ! -d rustd/crates/afd_fleet/src/memory` |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `afd_fleet::memory` | `grep -rn "afd_fleet::memory" rustd/crates \| head` | 0 matches |

## Out of Scope

- A vendor store (turbopuffer, mem0, others), a column naming a workspace's store, and an endpoint that flips one — the store comes with its first vendor, which also settles whether `direction.md`'s no-search rule is reversed.
- Overwriting or forgetting another fleet's entry; a fleet corrects a shared fact by publishing its own.
- Runner telemetry — `M214_001`.

---
## Product Clarity (authoring record)

1. **Successful user moment** — An admin turns on "Publish to shared memory" for the ticket and pull-request fleets and "Read shared memory" for a new fleet; its first run recalls "deploy 812 broke iad — from incident-fleet-3".
2. **Preserved user behaviour** — A fleet with no grant behaves exactly as today; a per-channel resident stays private by staying ungranted; the memory tools keep their names and arguments, `visibility` added.
3. **Optimal-way check** — The writer stays in every entry's identity, so shared memory has no conflicts and the one-writer-per-fleet fencing stays true; `agentsfleetd` enforces the grants, whatever store holds the rows.
4. **Rebuild-vs-iterate** — Iterate: the Postgres module moves behind a trait unchanged.
5. **What we build** — The trait, `flip`, visibility, two grants, the access route, the recall verb, two panel toggles.
6. **What we do NOT build** — Vendors, a flip endpoint, search beyond substring matching, cross-fleet overwrite.
7. **Fit with existing features** — Tenant memory routes and the push and hydrate verbs keep their paths; the hydrate reply and a delta gain fields.
8. **Surface order** — API, then runner, then panel, then the published memory page.
9. **Dashboard restraint** — Two toggles and a "shared" mark on the existing panel.
10. **Confused-user next step** — A refused share reads "this fleet may not publish to shared memory; a workspace admin grants it on the fleet's memory panel".

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream folded into the M210 Pull Request, by Indy's call.
- **Alternatives considered:** a milestone of its own (proposed; Indy chose to fold); one shared row per `(workspace_id, key)` with last write winning (Indy chose the writer in the identity); grants declared in `TRIGGER.md` (Indy chose a workspace setting); runners calling vendors with minted keys (option C, replaced by "Always via agentsfleetd").
- **Patch-vs-refactor verdict:** a **refactor** of the daemon's memory module plus a **feature** on top; no runner verb is removed.

## Discovery (consult log)

- **Consults** — Indy, Oct 03, 2026: "Well you can just flip the memory on a workspace level, its on agentsfleetd-rs level, either we use the default postgres or flip to turbopuffer or anyother (when ever we flip, the old memory must be migrate to the newer memory store we select)"; "The runner will call the api to hydrate the memory as well."; chose "Always via agentsfleetd"; "we must have flexibility to have the memory have a key of the fleet_id, workspace_id as well. The workspace_id are restricted based on scope (access control)"; chose "Workspace setting" for access and "Writer in identity (Recommended)" after asking "i thought the workspace_id and the fleet_id will be unique? so why will the conflict happen?"; chose "Fold into this PR".
- **Agent defaults** — `HYDRATE_SHARED_BYTES` and `RECALL_MISS_CAP` are named constants set at EXECUTE from the hydrate window budget; the dual write during a flip is the mechanism the no-lost-write invariant rests on.
- Agent default: `HYDRATE_SHARED_BYTES` is a quarter of the fleet's window (64 KiB) and `RECALL_MISS_CAP` is 3, because shared memory must not crowd a fleet's own and each miss is a round trip the model waits on.
- Agent default: `flip` is `Memories::flip(workspace, to)` with the current store as `from`, and the publish grant is applied once in `Memories` before any store's `upsert`, because one route table owns "the workspace's store" and no store should restate a grant.
- Agent default: the route table is an `arc-swap` map, and a write that saw a route a flip replaced writes the store it missed, because that keeps the dual write lock-free.
- Agent default: a forget during a flip answers `UZ-MEM-003`, because the copy could otherwise put a forgotten entry back.
- Agent default: the access route is presence-based — an absent grant keeps its value — because a toggle sends only what it flips.
- Agent default: `MemoryEntry` gains `visibility` and `writer_fleet_id`, and a granted reader's page carries the workspace's shared entries, because §6 marks shared entries on the reader's panel.
- Agent default: `AgentRun.memory` becomes `afr_memory::Seed` (window, shared, publish, recall seam), because the supervisor is where the hydrate reply and the daemon client live.
- Agent default: the Files Changed ripple row was added at EXECUTE, because the module move and the two routes cannot compile without those lines.
- Agent default: the fleet detail carries `memory_access`, because the panel needs the grants it opens on and §Files Changed puts "the grants on a fleet" in `afd_wire/src/fleet.rs`.
- Agent default: each toggle is a `ghost` `Button` with `aria-pressed` and an On/Off `Badge`, because the design system has no Switch and the button-variant rule permits `ghost`.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
