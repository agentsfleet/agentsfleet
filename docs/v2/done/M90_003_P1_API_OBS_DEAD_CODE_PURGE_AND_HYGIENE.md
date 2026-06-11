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
**Status:** DONE
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
| `schema/026_account_purge_gate_bypass.sql` + `schema/embed.zig` + `src/zombied/cmd/common.zig` | CREATE/EDIT | §5 gate-bypass migration, registered |
| `src/zombied/state/account_teardown.zig`, `http/handlers/zombies/delete.zig`, `zombie/approval_gate_db.zig` | EDIT | §5 purge bypass + fleet sweep + FAIL-state rollback |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 money-truth isolated (smallest reviewable money diff), §2 inventory purge (mechanical, grep-guarded), §3 test-lane wiring, §4 micro-fixes — four slices that don't interleave review concerns.
- **Alternatives considered:** splitting §1 into its own workstream for a standalone money diff — viable and revisitable at CHORE(open) if the combined diff reads poorly; kept in per Indy's approved Jun 10 scope grouping. Wiring the flatlined zombie-completion metrics to producers instead of deleting — rejected without a named consumer (RULE HLP); resurrect via a future observability spec if dashboards want them.
- **Patch-vs-refactor verdict:** **patch** throughout — deletions, single-statement fixes, and build wiring; nothing re-architected.

---

## Sections (implementation slices)

### §1 — Metering audit rows tell the truth

`charged` derives from the wallet CTE's actual applied delta (returned old−new balance), not the pre-lock probe value; ledger/breakdown rows persist that. Metered-token cursors clamp monotonic (never move backwards on a regressed cumulative report). Extends the concurrency suite with same-tenant exhaustion overlap.

- **Dimension 1.1** — two concurrent renewals at exhaustion: audit rows sum == wallet drain → Test `test_renewal_audit_equals_drain_at_exhaustion` — **DONE**
- **Dimension 1.2** — settle path same property → Test `test_settle_audit_equals_drain_at_exhaustion` — **DONE**
- **Dimension 1.3** — regressed cumulative token report charges zero and cursor holds → Test `test_metered_cursor_monotonic` — **DONE**

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

- **Dimension 2.1** — zombied inventory rows resolved (delete/demote/keep+justify) → Test: Eval E8 family greps return empty + `make test` green — **DONE** (E8 empty; test-unit-zombied green at unit=1889/integration=175)
- **Dimension 2.2** — runner inventory rows resolved; bwrap network args deduplicated behind one helper → Test `test_sandbox_args_network_policy_parity` — **DONE, dedup premise stale** (no `appendBwrapNetworkArgs` exists on the merged tree; `sandbox_args.zig` builds the network args once through `network/Policy.zig` with no duplicate path, so there is nothing to dedup and no second implementation for a parity test to compare — see Discovery)
- **Dimension 2.3** — registry purge keeps comptime coverage + OpenAPI checks green → Test: `make lint` + `make check-openapi` pass — **DONE** (25 never-raised codes + paired rows removed; lint-zig + check-openapi green)

### §3 — src/lib test lane executes everything

The lib test module gains the `common` named-module import in `build.zig`; `src/lib/tests.zig` imports the logging barrel (reaching envelope/pretty/sinks tests) and `common/env.zig`, via the barrel-import shape its own header comment promises. Newly-running tests that fail get fixed in this diff.

- **Dimension 3.1** — `zig build test-lib --summary all` runs the full set (logging + env tests included; count pinned in test output) → Test: summary paste in Verification Evidence — **DONE** (`zombie-lib-tests 30 pass` + `zombie-logging-tests 24 pass` = 54, vs 29 pre-wiring)
- **Dimension 3.2** — a named-filter control (`-Dtest-filter` on a logging test name) matches ≥1 → Test: filter run paste — **DONE** (`-Dtest-filter="writeFields encodes integers"` → matched in zombie-logging-tests, 5/5 pass across the step)

### §4 — Micro-correctness & ordering-comment debt

`StringBuilder.append` precondition asserts `len + slice.len <= cap` before the copy (parity with `appendZ`); `account_teardown.zig` errdefer rolls back via `conn.rollback()`; five `ArrayListUnmanaged` spellings become `ArrayList`; the eager-dial dangling `cfg` pointer and write-only `read_timeout_ms` fields are removed; weak atomic orderings across the surviving observability/queue/state files gain `// safe because:` pairing comments per the `metrics_runner.zig` template; the three uncommented `unreachable` arms gain invariant comments.

- **Dimension 4.1** — builder precondition catches over-append before memcpy (Debug) → Test `test_string_builder_append_precondition` — **DONE** (assert now `len + slice.len <= cap`, parity with appendZ; the test pins the exact-capacity boundary — the over-append half is a Debug panic by design and Zig has no in-process panic harness, noted in the test comment)
- **Dimension 4.2** — teardown failure mid-transaction rolls back (no poisoned conn, no orphan rows) → Test `test_account_teardown_rolls_back_on_failure` — **DONE** (red-green proven: the old `exec("ROLLBACK")` errdefer leaves the conn stuck in the aborted transaction and the test fails; `conn.rollback()` recovers it)
- **Dimension 4.3** — alias renames + dead-field removal compile clean on all targets → Test: E2/E5 — **DONE** (5× `ArrayListUnmanaged` → `ArrayList`; dead fields removed in §2's queue batch)
- **Dimension 4.4** — ordering-comment audit grep finds zero uncommented weak orderings in the touched set → Test: Eval E9 — **DONE** (E9 empty over the full diff; of the audit's three uncommented `unreachable` arms only `zombie_events_filter.zig` survived the purge — it now carries its invariant comment)

### §5 — Account erasure completes for gated accounts (added on Indy's directive, Jun 11)

`core.zombie_approval_gates` is append-only by trigger, but both hard-purge paths (account teardown, zombie hard-delete) DELETE from it — so erasure failed for any account that ever raised a gate. Migration 026 adds a transaction-scoped bypass (`SET LOCAL zombie.allow_gate_purge`) the two purge transactions opt into; the trigger still raises for every other DELETE. The teardown's gate DELETE broadens to workspace-OR-zombie so no gate row can strand the erasure on either foreign key, and the purge now also sweeps the fleet rows that carry tenant/zombie identifiers without foreign keys (`metering_periods` via the tenant's lease events, `runner_leases`, `runner_affinity`) — shared `fleet.runners` host rows survive. The zombie-delete handler gains the same bypass plus the FAIL-state-safe `conn.rollback()` errdefer.

- **Dimension 5.1** — purge succeeds for an account with approval gates; gate + fleet rows gone, shared runner row survives → Test `purge succeeds for an account with approval gates` — **DONE**
- **Dimension 5.2** — gated zombie hard-delete uses the same bypass (compile + existing delete suite; the handler change is the same two statements) → Test: E2 + zombied integration suite — **DONE**
- **Dimension 5.3** — the §4.2 rollback test re-based onto a mechanism-agnostic injection (a test-created BEFORE DELETE trigger on `core.users`) so it no longer depends on any purge-order gap → Test `a mid-purge failure rolls back` — **DONE**

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

- **Consults** — (append Architecture/Legacy-Design/gate-flag consults + Indy decisions here. Kept-with-consumer inventory rows land here.)
  - **§2 kept-with-consumer rows (re-grep on the merged tree):** `zombie_reclaim_interval_ms` (consumed by `fleet/reclaim_sweeper.zig` since the strand-recovery work — keep); `redis_pool.zig`'s `cfg` + `read_timeout_ms` fields (read at dial/apply time — keep; only `redis_connection.zig`'s mirror copies were write-only and removed); `network/network.zig` façade (NOT dead: it is the egress stack's `std.net`-pattern façade AND the test-discovery root for all nine network leaves — deleting it would have orphaned the entire egress test suite); `VerifyConfig.prefix/hmac_version/includes_timestamp` (consumed by `serve_webhook_lookup.schemeFromConfig` → the webhook-sig middleware; only the duplicate `verifySignature` fn was production-dead); `vault.storeJson` had test consumers only → demoted to `db/test_fixtures.storeVaultJson` per the fixture-home rule; `secrets_resolve.freeResolved` kept (it is the documented deinit pair, exercised by tests; the fleet service path hands the allocation to a request arena instead — doc now says so); `sinks.clearSinks` test-only → renamed `clearSinksForTest` (house `*ForTest` pattern).
  - **§2 extra producer-less rows found at execution time (same inventory mandate, RULE OBS/NDC on touch):** `addBackoffWaitMs`, `incGateRepairExhausted`, `addAgentTokens` + their series/render lines; `GateRule`/`AnomalyRule` aliases in `zombie/config.zig`; `ParseRequestTimeoutError`/`SubscriberMessage`/`SubscriberInitOptions`/`ClientInitOptions` barrel aliases (the spec's "aliases ×4"); `EV_SESSION_EXPIRED` (orphaned by `emitSessionExpired`'s deletion); `strArray` (orphaned by `fromJson`'s deletion); the runner `isAvailable` pair (cgroup/landlock, test-only). The metering `1,170-line` estimate landed at **net −1,472** once the flatlined metrics' snapshot/render plumbing and their tests were swept with their producers.
  - **§2 cross-runtime sweep:** `UZ-ZMB-001`'s user-facing copy removed from `ui/packages/app/lib/errors.ts` (the code can never reach the UI); the OpenAPI revoke-grant description no longer promises `UZ-GRANT-003` (nothing raises it — the sentence now states the behavior without naming a dead code).
  - **🚨 Production bug #2, found by the cross-model adversarial review (pre-existing on main, out of scope, needs its own follow-up):** the runner's renew call posts an EMPTY body (`control_plane_client.zig` `renew` → `self.post(..., "", ...)`) and the report path sends a single total token count — but `service_renew.zig`/`service_report` price tokens from the split `input_tokens`/`cached_input_tokens`/`output_tokens` body fields, which default to zero. Platform token spend therefore bills run-fee-only in production today; the metering suites construct `MeterInputs` directly and never exercise the wire. Pre-dates this branch (runner client last touched in the M88 work); this branch's §1 fix is orthogonal — it makes audit rows equal whatever actually drains. Indy to spec the wire fix + a wire-level metering test.
  - **Adversarial-review notes (kept-as-designed):** (a) a tenant with NO billing row records `charged = slice` audit rows while the wallet write no-ops — deliberate LEFT-JOIN semantics preserved from the original CTE (the audit row still documents usage; there is no wallet to drain); (b) the `GREATEST` cursor clamp means a legitimately-reset runner counter (process restart reporting lower cumulatives) charges zero tokens until the report passes the old high-water mark — this IS the spec's Dimension on regressed reports (never rewind, never re-charge); the trade-off and the absence of an anomaly log on regression are noted for a future observability pass; (c) purged metric series are ABSENT from /metrics, not zero — RULE OBS sanctioned (they were never incremented; any alert on them was already blind), changelog will say so.
  - **🚨 Production bug found while writing the §4.2 rollback test (out of this spec's scope, needs its own follow-up):** `core.zombie_approval_gates` carries an append-only trigger (`schema/009`, raises on every DELETE), but `account_teardown.purgeByOidcSubject` includes `DELETE FROM core.zombie_approval_gates ...` in its purge order — so the Clerk `user.deleted` hard-purge FAILS for any account that ever raised an approval gate, 500s the webhook, and Clerk retries forever. The new rollback test uses exactly this trigger as its deterministic failure injection (and so also pins today's broken behavior); when the teardown is fixed (trigger bypass, status-update instead of delete, or purge-order change) the test's injection needs re-basing. Indy to spec the fix.
  - **Scope addition, Indy-directed:** > Indy (2026-06-11): "I think the two pre existing production bug must be fixed" — context: bug #1 (teardown vs append-only gates) fixed in-branch as §5; bug #2 (runner token under-billing) handled per the recon decision recorded below.
  - **§5 grants observation:** `api_runtime` holds no DELETE grant on ANY table the purge already deletes from (core or fleet) — production purges therefore run as the owner role today; the role lockdown is pre-existing posture, unchanged by §5. When the lockdown lands, every purge DELETE (old and new) needs grants in the same migration.
  - **Adversarial-review dispositions (Claude subagent):** deadlock risk nil (every `FOR UPDATE`/`tenant_billing` writer enumerated; only the renew/settle CTE holds fleet+billing locks, always l,a → tb). Finding "9 orphaned wire.zig consts" — real, fixed in §5's commit (RULE NDC). Finding "tb row-lock now taken on every active-lease renewal, including ones guard later discards" — accepted consciously: the lock precedes pricing BY DESIGN (locking after guard re-creates the stale-read bug), is held for one fast statement, and renewals are per-15-30s per zombie; flagged as a perf observation for Indy, not silently bundled. Finding "GREATEST clamp gives the pre-reclaim token volume away free on every mid-run reclaim (self-heals past the watermark; fresh leases reset the cursor)" — this is the spec's own chosen trade (never re-charge the same tokens); the old code double-charged the climb-back. Product call for Indy: acceptable under-charge vs customer over-charge; an anomaly log on regressed reports is the suggested follow-up.
  - **§2 frontmatter test migration:** `parseZombieFromTriggerMarkdown` (test-only wrapper) deleted; its eight fixture tests now exercise the production entry `parseTriggerMarkdownWithJson`, so coverage moved to the real path instead of vanishing.
  - **§3 mechanism deviation:** the spec's one-binary shape (lib test module gains the `common` named import AND `tests.zig` file-imports the logging barrel) does not compile — Zig 0.16 hard-errors when one file belongs to two modules (`common/clock.zig` reached both relatively from the test root and via the named `common` instance logging needs). Implemented as two compilations under the same `test-lib` step: `zombie-lib-tests` (contract + common, pure file imports) and `zombie-logging-tests` (rooted at the logging barrel in the exact production module shape, `common` as named import). Named-module references collect zero tests (runner collects root-module tests only — measured, not assumed), which also surfaced a second latent gap: `logging/mod.zig`'s discovery block never referenced `sinks`, so sinks.zig + sinks_test.zig (9 of the 24) were dormant even within the barrel; fixed with `_ = sinks;`. Counts pinned: 29 → 54 (30 + 24; the 24 matches the audit's dormant count exactly).
  - **§1 mechanism deviation (non-blocking, Indy informed Jun 10):** the spec's literal "charged derives from the wallet CTE's returned old−new delta" is implemented as *lock-then-probe* instead: a dedicated `bal` CTE takes `FOR UPDATE` on the `tenant_billing` row before pricing, so `charged = LEAST(slice, bal0)` provably equals the wallet delta in every interleaving. Two reasons the lock cannot ride the probe: (1) Postgres rejects `FOR UPDATE` on the nullable side of an outer join — the first-cut `FOR UPDATE OF l, a, tb` errored on EVERY metered renew/settle (`error.PG` at `conn.query`; the Jun 10 handoff misread this as a fixture issue); (2) the billing row must stay optional (LEFT JOIN semantics — a tenant without a billing row still renews, charged = slice, wallet write no-ops). Lock order is unchanged from the pre-fix code (l, a → tb; the tb lock just moves earlier, from wallet-write time to pricing time), so no new deadlock ordering. Red-green pinned: without the fix, 1.1/1.2 record 4,030,300 nanos of audit vs a 3,022,725 drain and 1.3 rewinds the cursor 1000→500; with it, 38/38 pass.
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
| Unit tests | `make test-unit-all` | all lanes green (zombied 1891 + runner 272/7 linux-skips + lib 30+24 + auth 230 + zombiectl 403 + coverage + bundle) | ✅ |
| Integration tests | `make test-integration` | full suite passed (clean re-run; one pre-existing event-order flake on first run, unrelated symbols) | ✅ |
| Lib test lane | `zig build test-lib --summary all` | zombie-lib-tests 30 pass + zombie-logging-tests 24 pass (was 29 pre-wiring) | ✅ |
| Memleak | `make memleak` | 1230 passed, 350 skipped, 0 failed — 0 leaks | ✅ |
| Lint | `make lint-zig` + `make lint-app` + `make check-openapi` | all green (zlint unused-decls still `error`) | ✅ |
| Cross-compile | x86_64-linux + aarch64-linux prod; x86_64-linux test graphs (zombied + runner) | compile-clean ("unable to execute" = pass signal) | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found (also per-commit via pre-commit) | ✅ |
| Dead code sweep | E8 greps | empty (one doc-comment mention fixed in wire.zig) | ✅ |
| Test delta | `make _lint_zig_test_depth` | unit 1966 → 1892 (−74), integration 172 → 177 (+5). Negative unit delta is the point of a purge: the −75 are the deleted dead surface's own tests (external-metrics routing ×2, run-limit ×5, agent-histogram ×2, M10/M17-era external block ×4, verifySignature ×9, matchRotatedKey ×2, isTimestampFresh dup ×1, registry pin ×2, ErrorMapping ×4, zombie-metrics histogram family ×8, json-helper dead fns ×6, types.zig ×3, continuation ×3, + discovery-block consolidation), while every SURVIVING surface kept or gained coverage (+54-test lib lane now executing, +3 money-invariant tests, +1 rollback injection test, +1 builder boundary, +2 backpressure pins, +1 purged-series regression pin). Lacking-areas verdict: none on the changed surface — the §1 money path is the most-tested unit in the diff (8 concurrency/invariant tests) | ✅ |

## Out of Scope

- Wiring the runner's dormant context-budget layers (`tool_window`, `stage_chunk_threshold`, `context_cap_tokens` in `engine/runner.zig`) — wire-or-delete is a product call for Indy; parked, not silently decided.
- `handlers/common_authz.zig` cluster (page-allocator test helper in a production file, `set_config` round-trip, `testLiveValue` inversion) — audit P3 follow-up.
- Duplicate-name 409 mapping in `zombies/create.zig`, transient-error masking in `api_keys/tenant.zig`, runner-memory fencing predicate, tenant billing/provider route role floor (RULE BIL) — audit P2s, separate follow-ups with their own review profiles.
- Oversized-log truncation in `main.zig`, cgroup telemetry capture, RESP/logger hot-path perf — audit P3/P2 perf follow-ups.
