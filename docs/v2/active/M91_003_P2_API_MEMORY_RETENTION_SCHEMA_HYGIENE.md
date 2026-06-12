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
4. `src/zombied/http/handlers/runner/memory.zig` — the capture handler: the sweep's single call site, after `enforceCap`, same `memory_runtime` role window.
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
| `src/zombied/memory/zombie_memory.zig` | EDIT | `storeEntry` numeric ts + shrunk column list; new `daily`-sweep fn + retention/category constants |
| `src/zombied/http/handlers/memory/helpers.zig` | EDIT | millis timestamp helper; `MemoryEntry.updated_at` numeric; stale comment fixed |
| `src/zombied/http/handlers/memory/handler.zig` | EDIT | row reads follow the numeric type |
| `src/zombied/http/handlers/runner/memory.zig` | EDIT | sweep call after `enforceCap` |
| `src/zombied/memory/zombie_memory_integration_test.zig` | EDIT | sweep + type-change coverage |
| `public/openapi.json` | EDIT | `updated_at` field type corrected to number |
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

- **Dimension 1.1** — store→hydrate→list round-trip carries numeric millis; ordering newest-first holds → `test_numeric_ts_roundtrip_ordering`
- **Dimension 1.2** — tenant JSON `updated_at` is a number; OpenAPI agrees → `test_tenant_memory_updated_at_numeric` + `make check-openapi-errors`

### §2 — `session_id` removed

Dead since the runner push became the only writer (always inserted NULL). Column gone from schema and `storeEntry`; zero readers proven by grep.

- **Dimension 2.1** — schema and insert carry no `session_id`; repo grep finds zero references in `src/` → `test` build green + Dead Code Sweep grep

### §3 — Stale comment corrected

`helpers.zig` claims the category length cap protects a schema `DEFAULT 'core'`; no DEFAULT exists (correctly — schema defaults are rule-banned). Comment rewritten to state the real constraint (bounded label, app-enforced).

- **Dimension 3.1** — comment describes the actual mechanism; no DEFAULT reference remains → review assertion at `/review` + Eval grep

### §4 — `daily` retention sweep

Adapter fn deletes rows for the pushing zombie where category equals the `daily` constant and `updated_at` is older than now minus the retention constant (named constant, 72h per M14_001 §4 intent). Called once per capture push, after `enforceCap`, same role window; failure warns and never fails the capture. Only `daily` — every other category is expiry-exempt by construction (parameter bound from the constant; no pattern match).

- **Dimension 4.1** — aged `daily` rows deleted on next capture; young `daily` rows kept → `test_daily_sweep_deletes_only_aged`
- **Dimension 4.2** — aged `core`/`conversation`/custom rows survive the sweep → `test_sweep_never_touches_other_categories`
- **Dimension 4.3** — sweep is idempotent; second capture deletes nothing further → `test_daily_sweep_idempotent`
- **Dimension 4.4** — injected sweep failure: capture still returns success; warn logged → `test_sweep_failure_never_fails_capture`

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

Regression: M91_002's tier tests (`test_evict_windowed_before_core`, `test_core_survives_thousand_dailies`) rerun green on numeric timestamps. Negative paths: 4.2 and 4.4 are the mandatory negatives for the new behaviour.

---

## Acceptance Criteria

- [ ] Aged `daily` rows expire on capture; all other categories never — verify: `make test-integration` (4.1, 4.2)
- [ ] Timestamps numeric end to end; tenant JSON + OpenAPI agree — verify: `make test-integration` (1.2) + `make check-openapi-errors`
- [ ] `session_id` fully gone — verify: `grep -rn "session_id" src/ schema/013_memory_entries.sql | wc -l` → 0
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
# E4: Dead column (expect 0)
grep -rn "session_id" src/ schema/013_memory_entries.sql | wc -l
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
| `session_id` (column + insert binding) | `grep -rn "session_id" src/ schema/ \| grep -v done/` | 0 matches |
| `nowTs` (if renamed to the millis helper) | `grep -rn "nowTs" src/ \| head` | 0 matches |

---

## Discovery (consult log)

- **Consults** — (empty at creation; SCHEMA GUARD declaration outcome lands here)
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
