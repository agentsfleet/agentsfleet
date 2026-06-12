# M91_003: `daily` retention sweep + memory schema hygiene (numeric timestamps, dead column, stale comment)

**Prototype:** v2.0.0
**Milestone:** M91
**Workstream:** 003
**Date:** Jun 11, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — hygiene + cap-pressure relief: `daily` entries were specced to expire (M14_001 §4) but the sweep never shipped, so ephemera live until they crowd out durable memories at the cap; the schema still carries NullClaw-era `TEXT` timestamps and a never-written `session_id` whose retention rationale died when zombied took table ownership
**Categories:** API
**Batch:** B3 — after M91_002 (shared `zombie_memory.zig` surface; tier-eviction tests must hold on numeric timestamps). **Inherited from the B1 merge order:** M91_004 landed first (PR #396, hold waived), so the CLI side of the wire flip moves INTO this workstream — delete the string-seconds branch in `zombiectl`'s `renderUpdatedAt` (the helper comment marks the exact spot) and flip the memory test fixtures to numeric millis, same diff as the server change.
**Branch:** feat/m91-003-memory-retention
**Test Baseline:** unit=1947 integration=182
**Depends on:** M91_002 (file overlap + tier ordering rides `updated_at`)
**Provenance:** agent-generated (memory-architecture analysis session, Jun 11, 2026) — grounded in `schema/013_memory_entries.sql` (header states zombied owns the layout; the NullClaw-mirroring reason is gone), `storeEntry` (inserts `session_id` as NULL always), `helpers.zig` (comment cites a schema DEFAULT that does not exist), M14_001 §4 (the unshipped retention intent); re-confirm at PLAN.

**Canonical architecture:** `docs/architecture/direction.md:20` (retention is a deletion policy, not search infrastructure — in-bounds) + `docs/architecture/capabilities.md` §4 (gains one line documenting `daily` expiry).

---

## Implementing agent — read these first

1. `schema/013_memory_entries.sql` — the table being edited in place (pre-v2.0 teardown convention is stated in its header: migrations edit in place, dev/test databases rebuild from scratch; no ALTER chain).
2. `src/zombied/memory/zombie_memory.zig` — `storeEntry` (timestamp + `session_id` write), `enforceCap` (the sweep mirrors its warn-and-continue posture and sits beside it), `listAll` ordering.
3. `src/zombied/http/handlers/memory/helpers.zig` — `nowTs` (becomes a millis helper), `MemoryEntry.updated_at` (type change flows to the tenant JSON), the stale category comment.
4. `src/zombied/http/handlers/runner/memory.zig` — the capture handler: the sweep's single call site, **before** `enforceCap` (amended at `/review` — see Discovery), same `memory_runtime` role window.
5. `dispatch/write_sql.md` — Schema Removal Guard + STS/NSQ/SGR/ITF rules; read before touching `schema/`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m91): daily-memory retention sweep + schema hygiene`
- **Intent (one sentence):** `daily` memories expire after their retention window instead of living until cap eviction, and the memory schema sheds its dead NullClaw-era shapes (`TEXT` epoch strings, never-written `session_id`, a comment describing a DEFAULT that does not exist) while pre-v2.0 in-place edits are still cheap.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm against the branch: (a) every reader of `created_at`/`updated_at`/`session_id` (grep, not memory — the tenant JSON serialisation is the known wire-visible one), (b) the migration-array/embed registration shape for an in-place schema edit. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a zombie that worked a noisy week of incidents is not at its memory cap a month later: the scratch notes expired by themselves, and the `core` facts never had to compete with them.
2. **Preserved user behaviour** — `core`, `conversation`, and custom categories are untouched by expiry; agent verbs and the loop are unchanged; nothing user-authored is required.
3. **Optimal-way check** — capture-time per-zombie sweep is the smallest shape that delivers expiry: no scheduler, no scanner, bounded work on an existing transaction-scoped path. The unconstrained-optimal (a background reaper covering dormant zombies) is rejected — a dormant zombie's rows cost only storage, already capped.
4. **Rebuild-vs-iterate** — iterate; the only "rebuild" is the column-type teardown the pre-v2.0 convention exists for, and it gets cheaper never — after v2.0.0 this becomes a real migration.
5. **What we build** — numeric millisecond timestamps, `session_id` removal, comment fix, retention constant + sweep function + its call site, the OpenAPI type correction.
6. **What we do NOT build** — a job scheduler, per-category retention knobs, soft-delete/`forgetReason` machinery, any expiry for non-`daily` categories.
7. **Fit with existing features** — relieves the cap M91_002's eviction guards; numeric timestamps make every age-based comparison (this sweep, future policies) arithmetic instead of string-shaped; M91_001's counters make the sweep observable via the existing log + capture flow.
8. **Surface order** — no CLI/UI; one OpenAPI field-type correction (`updated_at` becomes a number) that M91_004's CLI consumes.
9. **Dashboard restraint** — nothing to show; expiry is ambient. Any future "expiring soon" UI waits for someone to ask.
10. **Confused-user next step** — "where did my note go?" → the user docs (M91_002's hygiene page) say scratch notes are `daily` and expire; durable facts belong in `core`. M91_004's CLI shows what survived, with timestamps.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RULE NDC** (`session_id` is dead at write time — removed, with grep proof), **RULE NLR** (touch-it-fix-it: the stale `helpers.zig` comment), **RULE STS** (no static strings in schema — the category filter binds a named constant as a query parameter), **RULE NSQ** (schema-qualified SQL), **RULE SGR** (schema-removal rationale declared), **RULE ITF** (interface shapes single-sourced), **RULE UFS** (retention duration + category string as named constants), **RULE FLS** (drain on touched query paths), **RULE TXN** (sweep DELETE failure cannot poison the capture's work).
- **`dispatch/write_sql.md`** — Schema Removal Guard: this spec removes one column and changes two column types; the pre-v2.0 teardown posture (edit in place, rebuild) is the declared mechanism.
- **`dispatch/write_zig.md`** — Zig discipline; cross-compile both linux targets.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — column removal + type changes in `schema/013` | declared here; in-place edit per the file's own pre-v2.0 header; `schema/embed.zig`/migration array untouched unless registration shape demands (Handshake b) |
| ZIG GATE | yes | `dispatch/write_zig.md`; cross-compile both linux targets |
| UFS | yes — retention duration, `daily` category constant | named constants in the adapter; bound as query parameters |
| LOGGING | yes — sweep emits count-only lines | counts and zombie scope only; never content |
| PUB | yes — new pub sweep fn on the adapter | mirrors `enforceCap`'s shape and posture |
| LENGTH | watch — `zombie_memory.zig` again | split if approaching 350 lines (M91_002 already watches this) |
| ERROR REGISTRY / UI | no | no new error codes (sweep failures warn-and-continue); no UI |

---

## Overview

**Goal (testable):** after a capture push, every `daily` entry for that zombie older than the retention window is gone from `memory.memory_entries`, while `core`/`conversation`/custom rows of any age and younger `daily` rows remain — and `created_at`/`updated_at` are numeric epoch milliseconds end to end, with the tenant JSON and OpenAPI reflecting the numeric type.

**Problem:** three dead shapes and one missing behaviour. (1) M14_001 §4 specced `daily` auto-expiry (72h intent); it never shipped, so ephemera persist until cap eviction — pressure M91_002 should not have to absorb. (2) Timestamps are decimal-epoch `TEXT` retained to mirror NullClaw's table — a rationale `schema/013`'s own header declares dead; string ordering works today by accident of digit count, and age arithmetic for retention wants numbers. (3) `session_id` has been written as NULL by the single write path since M84_005 — a dead column. (4) `helpers.zig` documents a schema `DEFAULT 'core'` that does not exist (and would be rule-banned if it did).

**Solution summary:** edit `schema/013` in place (pre-v2.0 teardown): `created_at`/`updated_at` become `BIGINT` epoch milliseconds, `session_id` is removed; `storeEntry` and the timestamp helper write millis; the tenant `MemoryEntry.updated_at` becomes numeric in JSON with the OpenAPI corrected (pre-v2, no compat shim). A sweep function beside `enforceCap` deletes `daily` rows older than a named retention constant for the pushing zombie, called once per capture after cap enforcement, warn-and-continue on failure. The stale comment is corrected in passing (RULE NLR).

---

## Prior-Art / Reference Implementations

- **Sweep posture** → `enforceCap` in `zombie_memory.zig`: per-zombie scoped DELETE, called post-push in the same role window, failure warns and never fails the capture. The sweep is its sibling in shape, call site, and tests.
- **Schema** → the nearest in-place pre-v2.0 schema edits in `schema/` history + `dispatch/write_sql.md`.
- **Timestamps** → the fleet plane already runs on `clock.nowMillis()` i64 (lease expiry comparisons); memory joins the same convention.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/013_memory_entries.sql` | EDIT | timestamps → `BIGINT` millis; `session_id` removed; header comment updated |
| `src/zombied/memory/zombie_memory.zig` | EDIT | `storeEntry` numeric ts + shrunk column list; new `daily`-sweep fn + retention constant |
| `src/zombied/http/handlers/memory/helpers.zig` | EDIT | `MemoryEntry.updated_at` numeric; `nowTs` deleted (single caller now reuses the push's one clock read); stale comment fixed |
| `src/zombied/http/handlers/runner/memory.zig` | EDIT | one clock read per push; sweep call after `enforceCap`; delta loop extracted to `storeDeltas` (method-length gate) |
| `src/zombied/memory/zombie_memory_integration_test.zig` | EDIT | sweep + type-change coverage |
| `src/zombied/http/handlers/memory/memories_integration_test.zig` | EDIT | *(added at PLAN)* seed INSERT numeric, no `session_id`; tenant JSON-number test |
| `src/zombied/http/handlers/runner/memory_loop_integration_test.zig` | EDIT | *(added at PLAN)* seed INSERTs numeric, no `session_id`; HTTP capture-sweep test |
| `src/zombied/http/handlers/memory/shapes_test.zig` | EDIT | *(added at PLAN)* `nowTs` tests deleted with the helper; retired-verb/string-shape pins replaced with live numeric envelope pins; milestone tokens stripped on touch |
| `src/zombied/state/account_teardown_test.zig` | EDIT | *(added at VERIFY)* two missed raw memory INSERTs flipped — caught by `make test-integration`, the handshake grep had scoped out `state/` |
| `docs/architecture/capabilities.md` | EDIT | one line: `daily` expires after the retention window |
| `zombiectl/src/commands/memory.ts` | EDIT | inherited from B1 order: `renderUpdatedAt` drops the string-seconds branch (comment marks the spot; M91_004 merged before this workstream) |
| `zombiectl/test/memory-render.unit.test.ts` + memory fixtures (`memory.unit.test.ts`, `memory.integration.test.ts`, `acceptance/memory-read.spec.ts`) | EDIT | `updated_at` fixtures flip to numeric epoch millis |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** hygiene and retention travel together because retention's age arithmetic is what makes the timestamp change load-bearing rather than cosmetic — and the pre-v2.0 in-place window is the cheapest it will ever be.
- **Alternatives considered:** (1) background reaper job — rejected: new scheduling infrastructure for dormant-zombie rows that cost only capped storage. (2) keep `TEXT` timestamps and compare strings in the sweep — rejected: encodes the digit-count accident into a second query. (3) repurpose `session_id` as run provenance instead of dropping — rejected for now: no consumer exists; provenance returns as its own spec when something reads it (RULE NDC cuts the other way too).
- **Patch-vs-refactor verdict:** **patch** — same seams, corrected shapes; the schema edit is the teardown convention working as designed.

---

## Sections (implementation slices)

### §1 — Numeric timestamps end to end

`BIGINT` epoch milliseconds for `created_at`/`updated_at`, written from the project clock; ordering clauses keep their semantics (numeric now, no digit-count caveat); tenant JSON emits the number; OpenAPI corrected. **Implementation default:** milliseconds, because the fleet plane already compares `clock.nowMillis()` i64 — one time unit platform-wide.

- **Dimension 1.1** — store→hydrate→list round-trip carries numeric millis; ordering newest-first holds → `test_numeric_ts_roundtrip_ordering` — **DONE**
- **Dimension 1.2** — tenant JSON `updated_at` is a number; OpenAPI agrees → `test_tenant_memory_updated_at_numeric` + `make check-openapi-errors` — **DONE**

### §2 — `session_id` removed

Dead since the runner push became the only writer (always inserted NULL). Column gone from schema and `storeEntry`; zero readers proven by grep.

- **Dimension 2.1** — schema and insert carry no `session_id`; repo grep finds zero references in `src/` → `test` build green + Dead Code Sweep grep — **DONE**

### §3 — Stale comment corrected

`helpers.zig` claims the category length cap protects a schema `DEFAULT 'core'`; no DEFAULT exists (correctly — schema defaults are rule-banned). Comment rewritten to state the real constraint (bounded label, app-enforced).

- **Dimension 3.1** — comment describes the actual mechanism; no DEFAULT reference remains → review assertion at `/review` + Eval grep — **DONE**

### §4 — `daily` retention sweep

Adapter fn deletes rows for the pushing zombie where category equals the `daily` constant and `updated_at` is older than now minus the retention constant (named constant, 72h per M14_001 §4 intent). Called once per capture push, **before `enforceCap`** *(amended at `/review`, cross-model confirmed: an already-expired `daily` row must not occupy a cap slot during victim selection — sweep-after-cap let eviction delete a durable row in the doomed row's place, breaching Invariant 1 via the cap side door)*, same role window; failure warns and never fails the capture. Only `daily` — every other category is expiry-exempt by construction (parameter bound from the constant; no pattern match).

- **Dimension 4.1** — aged `daily` rows deleted on next capture; young `daily` rows kept → `test_daily_sweep_deletes_only_aged` — **DONE**
- **Dimension 4.2** — aged `core`/`conversation`/custom rows survive the sweep → `test_sweep_never_touches_other_categories` — **DONE**
- **Dimension 4.3** — sweep is idempotent; second capture deletes nothing further → `test_daily_sweep_idempotent` — **DONE**
- **Dimension 4.4** — injected sweep failure: capture still returns success; warn logged → `test_sweep_failure_never_fails_capture` — **DONE**
- **Dimension 4.5** *(added at `/review`)* — sweep precedes cap eviction: at the cap with an aged `daily` present, a push evicts zero durable rows → `test_sweep_frees_cap_slots_before_eviction` — **DONE**

---

## Interfaces

- `memory.memory_entries` columns after this workstream: `uid`, `id`, `key`, `content`, `category`, `zombie_id`, `created_at` (numeric millis), `updated_at` (numeric millis) — shape in prose; the agent writes the SQL conforming to existing migrations.
- Tenant `GET …/memories` response: `updated_at` becomes a JSON number (pre-v2, no compat shim; OpenAPI corrected in the same diff). Keys, pagination, and error codes unchanged.
- New adapter fn: per-zombie `daily` sweep taking (connection, zombie id, cutoff millis); mirrors `enforceCap`'s error posture. Retention duration and `daily` label are named constants (UFS).

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| sweep DELETE fails | database blip | warn + continue; capture response unaffected (Dimension 4.4) |
| zombie never runs again | dormant agent | its aged `daily` rows persist — accepted: bounded by the entry cap, costs storage only |
| clock skew between writes | normal operation | cutoff computed server-side from one clock read per push; worst case a row lives one push longer |
| stale client expects string `updated_at` | external API consumer | pre-v2.0 posture: no shim; OpenAPI is the published truth and changes in the same diff |
| migration meets existing data | none pre-v2.0 | dev/test databases rebuild from scratch per the teardown convention; no live-data path exists |

---

## Invariants (Hard Guardrails)

1. **Only `daily` expires** — the sweep binds the category constant as a parameter; no wildcard, no pattern. Enforced by `test_sweep_never_touches_other_categories`.
2. **Sweep can never fail a capture** — error union consumed at the call site with warn-and-continue (same construction as `enforceCap`); enforced by `test_sweep_failure_never_fails_capture`.
3. **Single write path holds** — the sweep lives in the adapter and is called only by the capture handler; tenant plane stays read-only. Enforced by grep (no other caller) in Eval.
4. **No static strings in schema** — the edit introduces no DEFAULT/CHECK literals. Enforced by SCHEMA GUARD review.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_numeric_ts_roundtrip_ordering` | two stores in sequence → `listAll` newest-first; values are plausible epoch millis |
| 1.2 | integration | `test_tenant_memory_updated_at_numeric` | tenant GET → `updated_at` parses as a JSON number |
| 2.1 | — | build + Dead Code Sweep | zero `session_id` references in `src/` |
| 4.1 | integration | `test_daily_sweep_deletes_only_aged` | seed `daily` aged past retention + young → capture → aged gone, young present |
| 4.2 | integration | `test_sweep_never_touches_other_categories` | aged `core`/`conversation`/custom seeded → capture → all present |
| 4.3 | integration | `test_daily_sweep_idempotent` | immediate second capture → zero additional deletions |
| 4.4 | integration | `test_sweep_failure_never_fails_capture` | injected sweep error → HTTP success, stored count correct, warn logged |
| 4.5 | integration | `test_sweep_frees_cap_slots_before_eviction` | at-cap zombie, aged `daily` newest + push → zero cap evictions, coldest durable row survives, aged row gone (red-green proven: fails on sweep-after-cap order) |

Regression: M91_002's tier tests (`test_evict_windowed_before_core`, `test_core_survives_thousand_dailies`) rerun green on numeric timestamps. Negative paths: 4.2 and 4.4 are the mandatory negatives for the new behaviour.

---

## Acceptance Criteria

- [ ] Aged `daily` rows expire on capture; all other categories never — verify: `make test-integration` (4.1, 4.2)
- [ ] Timestamps numeric end to end; tenant JSON + OpenAPI agree — verify: `make test-integration` (1.2) + `make check-openapi-errors`
- [ ] `session_id` fully gone from the memory surface — verify: `grep -rn "session_id" src/zombied/memory/ src/zombied/http/handlers/memory/ src/zombied/http/handlers/runner/ schema/ | wc -l` → 0 *(amended at PLAN: the auth plane legitimately uses auth-session `session_id` — a different domain; the original repo-wide grep could never reach 0)*
- [ ] Sweep failure never fails capture — verify: `make test-integration` (4.4)
- [ ] M91_002 tier tests still green — verify: `make test-integration`
- [ ] `make lint` · `make test` · `make check-pg-drain` pass · migrations apply: `make run-migrations`
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` · `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: Build + unit + integration
zig build && make test 2>&1 | tail -3 && make test-integration 2>&1 | tail -5
# E2: Migrations from scratch (teardown convention)
make run-migrations 2>&1 | tail -3
# E3: OpenAPI consistency
make check-openapi-errors 2>&1 | tail -3
# E4: Dead column (expect 0; scoped to the memory surface + every raw INSERT site — auth-plane session_id is a different domain)
grep -rn "session_id" src/zombied/memory/ src/zombied/http/handlers/memory/ src/zombied/http/handlers/runner/ src/zombied/state/ schema/ | wc -l
# E5: Stale comment gone (expect empty)
grep -n "DEFAULT 'core'" src/zombied/http/handlers/memory/helpers.zig
# E6: Sweep has exactly one caller (the capture handler)
grep -rn "sweep" src/zombied --include="*.zig" | grep -v test | head
# E7: Lint, drain, cross-compile, gitleaks
make lint 2>&1 | grep -E "✓|FAIL"; make check-pg-drain 2>&1 | tail -2; zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo XC-PASS; gitleaks detect 2>&1 | tail -2
```

---

## Dead Code Sweep

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `session_id` (column + insert binding) | `grep -rn "session_id" src/zombied/memory/ src/zombied/http/handlers/memory/ src/zombied/http/handlers/runner/ schema/` | 0 matches |
| `nowTs` (deleted outright — the capture push's single clock read replaced it) | `grep -rn "\bnowTs\b" src/ \| head` | 0 matches |
| `MS_PER_SECOND` in `zombiectl/src/commands/memory.ts` (orphan after the string branch went) | `grep -n "MS_PER_SECOND" zombiectl/src/commands/memory.ts` | 0 matches |

---

## Discovery (consult log)

- **Consults** —
  - **SCHEMA GUARD (Jun 12, 2026):** VERSION=0.41.0 < 2.0.0 → teardown path; `schema/013` edited in place (`session_id` column removed, timestamps → `BIGINT` millis); `schema/embed.zig` and the `canonicalMigrations()` version-13 slot unchanged (same file, same registration). No `ALTER`/`DROP` introduced; `check-schema-gate` green.
  - **OpenAPI row satisfied by prior merge:** `MemoryEntry.updated_at` was already `integer/int64` on `main` — M91_004 (PR #396) flipped the published shape ahead of the server, which is why the CLI carried the tolerance branch. Zero diff here; `make check-openapi-errors` run as the consistency gate.
  - **E4 grep scoped at PLAN:** repo-wide `session_id` grep can never reach 0 (auth-plane sessions are a different domain); amended to the memory surface. Spec-vs-reality fix, not a scope change.
  - **`nowTs` deleted, not renamed:** its only production caller (the capture handler) now derives entry timestamps from the push's single `clock.nowMillis()` read — also serving the lease check and sweep cutoff (Failure Modes "one clock read per push" realized literally).
  - **Dimension 4.4 tier:** implemented at the adapter tier mirroring the pre-existing `enforceCap` failure test (deterministic `error.InvalidUUID`, count-unchanged proof); the handler's catch-warn-continue is construction-identical to the cap path. An HTTP-level fault-injection seam would be test-only machinery (rejected per "What we do NOT build").
  - **`storeDeltas` extraction:** `innerRunnerMemoryCapture` pre-existed at ~94 lines (over the fn cap); the sweep call would deepen it, so the per-delta store loop moved to a private helper in the same file — gate-compliant minimum, no behavior change.
  - **`shapes_test.zig` cleanup on touch (RULE NLR + MILESTONE-ID):** deleting `nowTs` forced touching the file; milestone tokens stripped from every test name (gate-mandated on save), retired-verb shape pins (store 201 / forget — verbs that answer 404/405 today) deleted, recall/list pins rewritten against the live `{items,total}` envelope with numeric `updated_at`. Net unit-test count drops; replacement coverage is integration-tier.
  - **Doc drift noted (not fixed — out of scope):** `docs/VERIFY_TIERS.md` tier-1 says `make test`, but the target is `test-unit-all` (lanes split). Flagged for a docs follow-up.
  - **`/review` adversarial outcome (Jun 12, 2026) — spec §4 ordering amended.** Both independent passes (fresh-context Claude subagent + Codex high-reasoning) converged, confidence 8-10: sweep-after-`enforceCap` let already-expired `daily` rows occupy cap slots during victim selection, so a capture at the cap evicted a **durable** row in the doomed row's place — Invariant 1 breached via the cap side door. Fixed by swapping the order (sweep first); spec §4 amended to match its own Invariant 1; regression `test_sweep_frees_cap_slots_before_eviction` red-green proven (fails on the old order: eviction counter moved; passes on the new). Secondary fixes from the same review: stale "TEXT seconds wire" comments in three zombiectl test fixtures corrected; tenant list/search `ORDER BY` gains the `, id DESC` tie-break (one-clock-read-per-push makes same-push `updated_at` ties universal; mirrors the adapter). **Dispositions:** sweep Prometheus counter (finding F4) deferred — spec scopes observability to the existing log flow; surfaced to Indy as a follow-up option. HTTP-tier sweep-failure injection (F3) remains adapter-tier by the documented Dimension 4.4 decision (the cap arm shares the posture). `created_at` write-only (F6) is the schema standard. Codex's "in-place migration edit" and "one-clock-read lease staleness" flags are convention false-positives (pre-v2 teardown is the documented mechanism enforced by `check-schema-gate`; clock staleness is bounded by request duration, semantics documented in Failure Modes).
- **Skill chain outcomes** — (`/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`)
- **Deferrals** — none; any deferral needs an Indy-acked verbatim quote here.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count + coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |
| After every push | `kishore-babysit-prs` | final report in Discovery |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit + integration | `make test && make test-integration` | | |
| Migrations | `make run-migrations` | | |
| OpenAPI | `make check-openapi-errors` | | |
| Dead column grep | Eval E4 | | |
| Lint + drain + cross-compile | Eval E7 | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- Run-provenance on memory rows (a reborn `session_id` with an actual reader) — its own spec when a consumer exists.
- Retention knobs (per-category, per-zombie) — the constant serves until evidence demands configuration.
- Background reaper for dormant zombies — rows are cap-bounded; storage-only cost accepted.
- Soft-delete / forget-reason machinery — explicit `memory_forget` remains the deletion verb.
