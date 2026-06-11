<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M90_003: Dead-surface purge & hygiene — truthful audit rows, dormant tests running, zero-consumer code deleted

**Prototype:** v2.0.0
**Milestone:** M90
**Workstream:** 003
**Date:** Jun 10, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — metering audit rows (the future invoice substrate) over-report under concurrency; ~1,170 lines of dead/test-only surface mislead readers and reviews; 24 src/lib tests never execute
**Categories:** API, OBS
**Batch:** B3 — after M90_001 + M90_002 merge (they consume symbols this purge would otherwise delete; every deletion re-greps against the merged tree)
**Branch:** feat/m90-003-purge
**Test Baseline:** unit=1966 integration=172
**Depends on:** M90_001 (wires `incApiBackpressureRejections`; reshapes gate files), M90_002 (wires `xautoclaimZombie`, adds failure-label consumers)
**Provenance:** LLM-drafted (Claude Fable 5, Jun 10, 2026) — from the Jun 10 full audit of `src/lib`, `src/zombied`, `src/runner`

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` (metering rows feed billing summaries — §1 makes them equal the real drain). The purge slices change no architecture.

---

## Implementing agent — read these first

1. `src/zombied/fleet/renewal.zig` + `renewal_settle.zig` — the dual-row writable-Common-Table-Expression (CTE) money operations §1 corrects: `charged` must come from the wallet CTE's actual delta, not the pre-lock probe read.
2. `src/zombied/fleet/concurrency_renew_test.zig` — the existing concurrency invariants (exactly-once charge, convergence) §1 extends with the exhaustion-overlap case.
3. `src/zombied/state/signup_bootstrap.zig` — documents why `conn.rollback()` (not `conn.exec(ROLLBACK)`) is required in FAIL state; §4 applies it to `account_teardown.zig`.
4. `src/zombied/observability/metrics_runner.zig` — the compliant `// safe because:` ordering-comment template §4 replicates.
5. `build.zig` (`lib_tests` module) + `src/lib/tests.zig` — the dead test lane §3 wires: the aggregator reaches neither the logging barrel nor `common/env.zig`, and the test module declares no named-module imports.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `chore(m90): purge dead surface, fix metering audit rows, wire src/lib test lane`
- **Intent (one sentence):** What the code claims matches what runs — audit rows equal real money movement, every metric rendered is one production increments, every test discovered executes, and zero-consumer surface is gone.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`; mismatch → STOP and reconcile.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — **RULE NDC/NLR/HLP** (the purge's mandate), **RULE DFS** (dead struct fields: dangling borrowed-config pointer), **RULE TXN** (rollback on every transactional failure path), **RULE OBS** (deleting flatlined metrics beats lying dashboards; anything kept must gain a producer), **RULE UFS**, **RULE ORP/CHR** (sweep every deleted symbol across src/, schema/, zombiectl/, docs/), **RULE TST** (new test discovery imports), **RULE TST-NAM**, **RULE ESO** (no silent sentinel substitution — context for kept helpers), **RULE IMS**, **RULE XCC**.
- `dispatch/write_zig.md` — Progressive Cleanup, pub-surface audit on touch, Concurrency (`// safe because:` comments), Memory Safety, Build Verification (`make`, not `zig build`).
- `docs/LIFECYCLE_PATTERNS.md` — §6 defer/errdefer discipline for the teardown-rollback fix.
- `docs/AUTH.md` — sensitive-data table: if any telemetry struct is kept instead of deleted, `session_id` must ship hashed, never raw.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all-Zig diff | façade read; cross-compile both linux targets + linux test graphs |
| PUB / Struct-Shape | yes — pub surface shrinks heavily | per-file pub audit on touch; zlint unused-decls stays `error` |
| File & Function Length | yes — many files touched | deletions only shrink; no new file expected |
| UFS | yes — §4 renames/comments | named consts preserved through deletions |
| LOGGING | yes — §1 paths keep their logs | RULE OBS check on every modified branch |
| ERROR REGISTRY | yes — ~30 dead registry consts deleted | paired `error_entries.zig` rows removed; comptime coverage stays green |
| SCHEMA | no — no SQL files change (CTE text lives in Zig) | N/A |
| UI / DESIGN TOKEN | no | N/A |

---

## Overview

**Goal (testable):** Two concurrent same-tenant renewals at balance exhaustion produce audit rows summing exactly to the wallet drain; `zig build test-lib` executes the full src/lib test set including logging; every symbol in the §2 inventory is deleted/demoted (or re-justified with a production consumer found at execution time); the builder precondition, teardown rollback, deprecated aliases, dangling config pointer, and missing ordering comments are fixed.

**Problem:** `renewal.zig`/`renewal_settle.zig` compute `charged` from a probe read taken before the wallet row serializes — overlapping renewals at exhaustion over-report the drain in `fleet.metering_periods` and the telemetry breakdown (the invoice substrate), and the metered-token cursors can move backwards (later re-charge of the same tokens). The audit confirmed ~61 zero-reference symbols, 2 effectively test-only production files, 8 of 12 PostHog telemetry structs never captured, a flatlined-metrics cluster rendering series nothing increments, and 24 src/lib tests in no build graph. Plus micro-defects: `StringBuilder.append`'s precondition assert checks the wrong inequality (heap overflow lands before it fires), `account_teardown.zig`'s errdefer issues `ROLLBACK` via the exec path that refuses to run in FAIL state, five files spell the deprecated `ArrayListUnmanaged` alias, eagerly-dialed Redis connections store a dangling config pointer (write-only fields), and 150+ weak atomic orderings lack their mandated pairing comments.

**Solution summary:** One isolated money-correctness slice (audit rows derive from the wallet CTE's returned delta; cursors clamp monotonic), a systematic delete/demote pass over the confirmed inventory with a re-grep guard per symbol, build wiring so the src/lib lane executes everything it claims to, and a batch of small mechanical fixes with their tests.

---

## Prior-Art / Reference Implementations

- **Money CTE shape** → the existing `renewal.zig` writable CTE itself; the fix moves `charged` derivation inside it (`RETURNING` the old/new balance delta) — no new statement shape.
- **Rollback in FAIL state** → `state/signup_bootstrap.zig` + `db/pool_migrations.zig` (`conn.rollback()` precedent with rationale comments).
- **Ordering comments** → `observability/metrics_runner.zig` (the audit's compliant template).
- **Test-fixture demotion** → `db/test_fixtures.zig` family (the sanctioned home for test-only helpers like `vault.storeJson`).

---

## Files Changed (blast radius)

> §2's inventory table is the authoritative per-symbol list; this table carries the file-level roles. Deletions marked Δ re-grep at execution time (Batch B3 tree) before removal.

| File | Action | Why |
|------|--------|-----|
| `src/zombied/fleet/renewal.zig`, `renewal_settle.zig` | EDIT | charged = wallet delta; monotonic cursors |
| `src/zombied/fleet/concurrency_renew_test.zig` | EDIT | exhaustion-overlap invariant test |
| `src/zombied/http/handlers/common.zig` | EDIT Δ | delete dead legacy auth trio + alias |
| `src/zombied/auth/middleware/bearer.zig` | EDIT Δ | delete `matchRotatedKey` + its tests |
| `src/zombied/types.zig` | DELETE Δ | test-only file (no production path) |
| `src/zombied/reliability/error_classify.zig` | DELETE Δ | test-only classifier cluster |
| `src/zombied/observability/telemetry_events.zig` + `telemetry.zig` | EDIT Δ | delete 8 uncaptured event structs + aliases |
| `src/zombied/observability/metrics_external.zig`, `metrics_zombie.zig`, `metrics_workspace.zig`, `metrics_counters.zig`, `metrics_histograms.zig`, `metrics.zig` | EDIT Δ | delete flatlined series/fns/aliases not consumed post-M90_001/002 |
| `src/zombied/queue/redis_client.zig`, `redis_zombie.zig`, `redis.zig`, `constants.zig` | EDIT Δ | delete `Client.exists`, dead aliases, dead consts surviving M90_002 |
| `src/zombied/queue/redis_connection.zig`, `redis_pool.zig` | EDIT | drop dangling `cfg` pointer + write-only `read_timeout_ms` (RULE DFS) |
| `src/zombied/auth/audit_events.zig`, `auth/middleware/mod.zig`, `auth/middleware/svix_signature.zig` | EDIT Δ | delete dead emitter + aliases |
| `src/zombied/zombie/approval_gate_resolver.zig`, `approval_gate_db.zig`, `config.zig`, `config_markdown.zig`, `webhook_verify.zig`, `webhook/normalizer/github.zig` | EDIT Δ | delete dead consts/aliases/test-wrappers |
| `src/zombied/state/vault.zig`, `fleet/secrets_resolve.zig`, `src/lib/logging/sinks.zig` | EDIT | demote test-only helpers to fixture homes / test-marked names; fix lying doc-comment |
| `src/zombied/errors/error_registry.zig` + `error_entries.zig` | EDIT Δ | delete ~30 never-raised codes + `ErrorMapping`/`validateErrorTable`; keep comptime coverage green |
| `src/runner/engine/types.zig`, `json_helpers.zig`, `context_budget.zig`, `client_errors.zig`, `cgroup.zig`, `landlock.zig`, `network.zig`, `sandbox_args.zig`, `wire.zig`, `pipe_proto.zig` | EDIT Δ | delete dead symbols; wire `appendBwrapNetworkArgs` into `sandbox_args` (dedup); fix stale header doc; demote pipe test helpers |
| `src/lib/contract/event_envelope.zig` | EDIT Δ | delete `buildContinuationActor` + prefix (no continuation pipeline) |
| `src/zombied/util/strings/string_builder.zig` | EDIT | precondition assert covers `len + slice.len <= cap` |
| `src/zombied/state/account_teardown.zig` | EDIT | errdefer uses `conn.rollback()` |
| `src/zombied/auth/claims.zig` + 4 handler files | EDIT | `ArrayListUnmanaged` → `ArrayList` |
| `src/zombied/observability/*` + `src/zombied/events/bus.zig` survivors | EDIT | `// safe because:` ordering-comment backfill |
| `build.zig` + `src/lib/tests.zig` | EDIT | lib test module imports `common`; aggregator reaches logging + env |
| `src/zombied/http/workspace_guards.zig` | EDIT | move test-fixture import inside the `test {}` block |
| sibling `*_test.zig` per edit | CREATE/EDIT | per Test Specification |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 money-truth isolated (smallest reviewable money diff), §2 inventory purge (mechanical, grep-guarded), §3 test-lane wiring, §4 micro-fixes — four slices that don't interleave review concerns.
- **Alternatives considered:** splitting §1 into its own workstream for a standalone money diff — viable and revisitable at CHORE(open) if the combined diff reads poorly; kept in per Indy's approved Jun 10 scope grouping. Wiring the flatlined zombie-completion metrics to producers instead of deleting — rejected without a named consumer (RULE HLP); resurrect via a future observability spec if dashboards want them.
- **Patch-vs-refactor verdict:** **patch** throughout — deletions, single-statement fixes, and build wiring; nothing re-architected.

---

## Sections (implementation slices)

### §1 — Metering audit rows tell the truth

`charged` derives from the wallet CTE's actual applied delta (returned old−new balance), not the pre-lock probe value; ledger/breakdown rows persist that. Metered-token cursors clamp monotonic (never move backwards on a regressed cumulative report). Extends the concurrency suite with same-tenant exhaustion overlap.

- **Dimension 1.1** — two concurrent renewals at exhaustion: audit rows sum == wallet drain → Test `test_renewal_audit_equals_drain_at_exhaustion`
- **Dimension 1.2** — settle path same property → Test `test_settle_audit_equals_drain_at_exhaustion`
- **Dimension 1.3** — regressed cumulative token report charges zero and cursor holds → Test `test_metered_cursor_monotonic`

### §2 — Dead & test-only surface purge (inventory)

Per symbol: word-boundary re-grep on the post-M90_002 tree; still zero production consumers → delete (or demote to the listed fixture home); a consumer appeared → keep + record in Discovery. zlint `unused-decls` stays `error`. Telemetry note: if any of the 8 structs is kept-and-wired instead, `session_id` ships hashed per `docs/AUTH.md`.

| Where | Symbols (action) |
|---|---|
| `handlers/common.zig` | `authenticate`, `parseBearerToken`, `AuthError`, `AuthMode` alias (delete) |
| `auth/` | `matchRotatedKey` (delete), `emitSessionExpired` (delete), `mod.zig`/`svix_signature.zig` dead aliases (delete) |
| `types.zig`, `reliability/error_classify.zig` | whole files + `metrics_external` inc fns + aliases (delete cluster) |
| `telemetry_events.zig` (+ `telemetry.zig`) | 8 uncaptured structs + aliases (delete) |
| metrics files | zombie completed/failed/tokens/exec-seconds, workspace `addTokens`, run-limit relics, agent-duration histogram, `redisPoolSnapshot` alias — delete those still producer-less after M90_001/002 |
| `queue/` | `Client.exists`, `redis.zig` aliases ×4, `zombie_reclaim_interval_ms` if unused post-M90_002 (delete); dangling `cfg`/`read_timeout_ms` fields (remove) |
| `zombie/` | `SLACK_INTERACTION`, `ListResult`/`GateBehavior`/`AnomalyPattern`/`ZombieTriggerType` aliases, `parseZombieFromTriggerMarkdown` + alias, `verifySignature` + 2 timestamp aliases (delete) |
| `errors/` | ~30 never-raised registry consts + paired `ENTRIES` rows, `ErrorMapping` + `validateErrorTable`, test-only consts (delete; coverage gate green) |
| `runner/engine` | `CorrelationContext`, `parseExecutionId`, `LeaseState`, `generateExecutionId`, `getObjectParam`/`getArrayParam`/`escapeAlloc`/`getIntOrZero`/`getBool`, `fromJson` + `DEFAULT_*` + `NetworkPolicy` aliases, `client_errors` 11 consts, `isAvailable` ×2 (delete); `appendBwrapNetworkArgs` (wire into `sandbox_args` — dedup, RULE UFS); `wire.zig` stale doc (fix) |
| demotions | `vault.storeJson` → fixtures; `secrets_resolve.freeResolved` doc fixed or demoted; `sinks.clearSinks` → test-marked; `pipe_proto.osPipe`/`osClose` → test-marked; `workspace_guards` fixture import → `test {}` block |
| `src/lib` | `buildContinuationActor` + `continuation_actor_prefix` (delete) |

- **Dimension 2.1** — zombied inventory rows resolved (delete/demote/keep+justify) → Test: Eval E8 family greps return empty + `make test` green
- **Dimension 2.2** — runner inventory rows resolved; bwrap network args deduplicated behind one helper → Test `test_sandbox_args_network_policy_parity`
- **Dimension 2.3** — registry purge keeps comptime coverage + OpenAPI checks green → Test: `make lint` + `make check-openapi` pass

### §3 — src/lib test lane executes everything

The lib test module gains the `common` named-module import in `build.zig`; `src/lib/tests.zig` imports the logging barrel (reaching envelope/pretty/sinks tests) and `common/env.zig`, via the barrel-import shape its own header comment promises. Newly-running tests that fail get fixed in this diff.

- **Dimension 3.1** — `zig build test-lib --summary all` runs the full set (logging + env tests included; count pinned in test output) → Test: summary paste in Verification Evidence
- **Dimension 3.2** — a named-filter control (`-Dtest-filter` on a logging test name) matches ≥1 → Test: filter run paste

### §4 — Micro-correctness & ordering-comment debt

`StringBuilder.append` precondition asserts `len + slice.len <= cap` before the copy (parity with `appendZ`); `account_teardown.zig` errdefer rolls back via `conn.rollback()`; five `ArrayListUnmanaged` spellings become `ArrayList`; the eager-dial dangling `cfg` pointer and write-only `read_timeout_ms` fields are removed; weak atomic orderings across the surviving observability/queue/state files gain `// safe because:` pairing comments per the `metrics_runner.zig` template; the three uncommented `unreachable` arms gain invariant comments.

- **Dimension 4.1** — builder precondition catches over-append before memcpy (Debug) → Test `test_string_builder_append_precondition`
- **Dimension 4.2** — teardown failure mid-transaction rolls back (no poisoned conn, no orphan rows) → Test `test_account_teardown_rolls_back_on_failure`
- **Dimension 4.3** — alias renames + dead-field removal compile clean on all targets → Test: E2/E5
- **Dimension 4.4** — ordering-comment audit grep finds zero uncommented weak orderings in the touched set → Test: Eval E9

---

## Interfaces

```
No HTTP shape, wire, or schema changes.
Semantic fix (downstream consumers of audit rows, e.g. future Stripe summaries):
  fleet.metering_periods.charged_nanos and the telemetry breakdown now equal the
  actual wallet delta in every interleaving (previously could exceed it at exhaustion).
Deleted pub surface: per §2 inventory — nothing outside it may be removed without spec amendment.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Symbol gained a consumer since audit | M90_001/002 or parallel work wired it | keep; record in Discovery; inventory row marked kept-with-consumer |
| Newly-wired lib test fails | dormant test rotted | fix in this diff; failure documented in Discovery |
| Registry purge breaks comptime coverage | paired row missed | coverage assertion fails at build — fix pairing before commit |
| Teardown statement fails mid-transaction | Postgres error | rollback issued on the FAIL-state-safe path; warn logged; conn reusable |
| Over-append on builder | counting bug upstream | Debug assert fires pre-copy (no heap corruption); Release relies on correct two-pass counting as today |
| Concurrent renewals at exhaustion | racing money ops | audit rows sum to drain (new invariant test) |

---

## Invariants

1. Audit rows == wallet drain under concurrency — concurrency test asserts SUM(charged) equals balance delta exactly.
2. Metered cursors never decrease — clamp in the UPDATE + regression test.
3. Every §2 deletion is grep-proven zero-consumer at execution time — Eval E8 family; a non-empty grep blocks that row.
4. Builder precondition covers the copy bound — corrected assert + Debug test.
5. All discovered-but-unrun tests now execute — `test-lib` summary count pinned in Verification Evidence.
6. No uncommented weak ordering in touched files — Eval E9 grep is empty.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_renewal_audit_equals_drain_at_exhaustion` | balance 100, two slices 60 concurrent → drain 100, audit sum 100 |
| 1.2 | integration | `test_settle_audit_equals_drain_at_exhaustion` | settle overlapping renew at exhaustion → sums equal |
| 1.3 | unit | `test_metered_cursor_monotonic` | cumulative report below cursor → zero charge, cursor unchanged |
| 2.1 | invariant | Eval E8 greps + `make test` | inventory symbols gone; suite green |
| 2.2 | unit | `test_sandbox_args_network_policy_parity` | network-policy args identical via shared helper |
| 2.3 | invariant | `make lint` + `make check-openapi` | registry/OpenAPI gates green after purge |
| 3.1 | invariant | `zig build test-lib --summary all` | logging/env test names present in summary |
| 3.2 | invariant | `-Dtest-filter` control | named logging test matches ≥1 |
| 4.1 | unit | `test_string_builder_append_precondition` | over-cap append trips assert before copy (Debug) |
| 4.2 | integration | `test_account_teardown_rolls_back_on_failure` | injected failure → rollback, no partial deletes, conn healthy |
| 4.3 | invariant | E2 + E5 | both targets compile |
| 4.4 | invariant | Eval E9 | zero uncommented weak orderings in touched files |

Regression: full `make test` + `make test-integration` pin that deletions removed nothing live; renewal suite's existing exactly-once/convergence tests stay green. Idempotency/replay: N/A beyond existing renewal idempotency pins (unchanged).

---

## Acceptance Criteria

- [ ] `make lint` clean (zlint `unused-decls` still `error`) · `make test` passes
- [ ] `make test-integration` passes (money + teardown paths)
- [ ] `make memleak` clean (allocator-adjacent edits)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` + linux test graphs
- [ ] `gitleaks detect` clean · no production file over 350 lines
- [ ] `zig build test-lib --summary all` shows the logging/env tests — verify: summary paste
- [ ] Inventory greps empty — verify: Eval E8

## Eval Commands (post-implementation)

```bash
# E1: money invariant suite
make test-integration 2>&1 | tail -5
# E2: Build — zig build && zig build --build-file build_runner.zig
# E3: Tests — make test && zig build test-lib --summary all | tail -20
# E4: Lint — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: inventory orphan sweep (empty = pass) — marquee symbols; run the full §2 list likewise
grep -rnE "\bauthenticate\b" src/zombied/http/handlers/common.zig | head
grep -rnE "matchRotatedKey|emitSessionExpired|error_classify|buildContinuationActor|validateErrorTable" src/ | head
grep -rnE "CorrelationContext|parseExecutionId|generateExecutionId|fromJson" src/runner/ | head
# E9: uncommented weak orderings in touched files (empty = pass)
for f in $(git diff --name-only origin/main -- '*.zig'); do awk '/\.(monotonic|acquire|release|acq_rel|unordered)\b/ {if (prev !~ /safe because/ && $0 !~ /safe because/) print FILENAME":"FNR" "$0} {prev=$0}' "$f"; done | head
```

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/zombied/types.zig` | `test ! -f src/zombied/types.zig` |
| `src/zombied/reliability/error_classify.zig` | `test ! -f src/zombied/reliability/error_classify.zig` |

**2. Orphaned references — zero remaining imports/uses** (full per-symbol table = §2 inventory; marquee greps in E8).

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `error_classify` | `grep -rn "error_classify" src/ \| head` | 0 matches |
| `telemetry_events` deleted structs | `grep -rnE "EntitlementRejected\|WorkerStarted\|StartupFailed\|ApiErrorWithContext" src/ \| head` | 0 matches |

---

## Discovery (consult log)

- **Consults** — (empty at creation; append Architecture/Legacy-Design/gate-flag consults + Indy decisions here. Kept-with-consumer inventory rows land here.)
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs` results.
- **Deferrals** — Indy-acked verbatim quotes only.

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | — | |
| Integration tests | `make test-integration` | — | |
| Lib test lane | `zig build test-lib --summary all` | — | |
| Memleak | `make memleak` | — | |
| Lint | `make lint` | — | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | — | |
| Gitleaks | `gitleaks detect` | — | |
| Dead code sweep | E8 greps | — | |

## Out of Scope

- Wiring the runner's dormant context-budget layers (`tool_window`, `stage_chunk_threshold`, `context_cap_tokens` in `engine/runner.zig`) — wire-or-delete is a product call for Indy; parked, not silently decided.
- `handlers/common_authz.zig` cluster (page-allocator test helper in a production file, `set_config` round-trip, `testLiveValue` inversion) — audit P3 follow-up.
- Duplicate-name 409 mapping in `zombies/create.zig`, transient-error masking in `api_keys/tenant.zig`, runner-memory fencing predicate, tenant billing/provider route role floor (RULE BIL) — audit P2s, separate follow-ups with their own review profiles.
- Oversized-log truncation in `main.zig`, cgroup telemetry capture, RESP/logger hot-path perf — audit P3/P2 perf follow-ups.
