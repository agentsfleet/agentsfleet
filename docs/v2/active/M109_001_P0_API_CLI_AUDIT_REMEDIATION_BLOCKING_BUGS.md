<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M109_001: Fix 3 confirmed P0 defects — approval decision durability, CLI timestamp crash, phantom CLI command

**Prototype:** v2.0.0
**Milestone:** M109
**Workstream:** 001
**Date:** Jul 02, 2026
**Status:** IN_PROGRESS
**Test Baseline:** unit=2272 integration=243
**Priority:** P0 — one defect silently diverges the approval audit trail from what actually gets enforced (a security/compliance-grade correctness bug); one crashes a CLI command on realistic input; one sends operators to a command that does not exist.
**Categories:** API CLI
**Batch:** B1 — independent of M109_002/003/004; no shared files, safe to land in parallel with them.
**Branch:** feat/m109-001-audit-p0-fixes
**Depends on:** None.
**Provenance:** agent-generated (pre-spec, fleet-wide-refactor-audit `Workflow` run `wf_8ec169f4-8e4`, each finding independently re-verified by 2 adversarial verifier passes against current source before this spec was drafted, Jul 02, 2026).

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

**Canonical architecture:** `docs/architecture/data_flow.md` (balance gate → receive debit → **approval gate** → run debit, lines ~89, ~562) and `docs/architecture/runner_fleet.md:376` — the approval gate is a lease-issue precondition; its DB row is the durable record, Redis is a latency-optimized mirror the async lease-poll path reads.

---

## Implementing agent — read these first

1. `src/agentsfleetd/fleet_runtime/approval_gate.zig` — `resolve()` (lines 229–248) is where §1's defect lives: the `.resolved` branch best-effort-mirrors the just-committed DB decision into Redis and swallows a mirror-write failure.
2. `src/agentsfleetd/fleet_runtime/approval_gate_async.zig` — `evaluateRef()`/`readDecision()` (lines ~93–118) is the read side that only ever consults Redis; this is where §1's fix (a DB fallback) lands.
3. `src/agentsfleetd/fleet_runtime/approval_gate_db.zig` — `ResolveArgs.atomic()` (lines 150–185) is the already-correct pattern for a check-then-act DB transition (single conditional `UPDATE ... RETURNING`, losers fall through to a `SELECT`). Mirror this shape's discipline (no separate read-then-write step) when reading the DB fallback in §1 — read the current row directly, don't re-derive it from Redis semantics.
4. `cli/src/commands/auth.ts:64` — the `typeof ms === "number" && Number.isFinite(ms)` guard idiom is the closest existing safe-timestamp pattern in the CLI; §2 improves on it (also needs to survive a genuinely invalid `Date`, not just a non-finite number).
5. `cli/src/program/cli-tree-fleet.ts:129-158` — the actual, correctly-registered top-level `credential` command group (`add`/`show`/`list`/`delete`) that §3's fix must point users to instead of the nonexistent `agent credential`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Fix approval-decision Redis mirror gap, CLI timestamp crash, and phantom credential command
- **Intent (one sentence):** An approved/denied grant's audit trail never diverges from what's actually enforced, and two CLI commands stop crashing/misdirecting operators on realistic input.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: …`). A mismatch between this restatement and the Intent above → STOP and reconcile before any edit.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — an operator approves a Slack grant during a live incident; even if the Redis write that mirrors that decision fails, the fleet's lease-poll path still observes "approved" (not a false timeout) because it falls back to the DB row that already committed.
2. **Preserved user behaviour** — the happy-path approve/deny flow (DB commit → Redis mirror succeeds → lease-poll reads Redis) is unchanged; this only closes the gap on the Redis-failure branch, which today silently corrupts state. `fleet_logs`/`workspace` CLI output format is otherwise unchanged.
3. **Optimal-way check** — the unconstrained-optimal fix makes Redis unnecessary for correctness (DB is always consulted). That's a bigger refactor than P0 warrants; the accepted gap is that Redis stays the fast-path read and the DB fallback only fires when the mirror key is absent — acceptable because it's the exact failure this bug already exposed, no new failure mode is introduced.
4. **Rebuild-vs-iterate** — no rebuild. This is a two-file patch (async read gets a DB fallback) inside an existing, otherwise-correct atomic-transition design (`approval_gate_db.zig`); a refactor would trade a small, well-understood fix for unrelated risk.
5. **What we build** — a DB fallback read in `evaluateRef` when the Redis mirror key is absent; a guarded `formatTimestamp` in `fleet_logs.ts`; a corrected command string in `workspace.ts`.
6. **What we do NOT build** — a general Redis-write retry/queueing layer (the DB fallback makes it unnecessary for correctness); a systemic audit of every other unguarded `new Date(x).toISOString()` call site in `cli/src/` (flagged in Discovery as a real pattern, out of scope here — P2 follow-up).
7. **Fit with existing features** — compounds directly with the approval-gate sweeper (`approval_gate_sweeper.zig`), which must not be destabilized: the DB fallback must not cause it to double-process a row the sweeper is concurrently timing out.
8. **Surface order** — API-first for §1 (the durability fix has no CLI/UI surface); CLI-first for §2/§3 (both are `agentsfleet` command fixes with no new UI).
9. **Dashboard restraint** — N/A, no UI surface in this workstream.
10. **Confused-user next step** — §3 already is the "confused user" fix (the CLI now tells them the real command); §1's confused user is an operator seeing a mismatched dashboard status — after the fix, the dashboard/audit trail and the lease-poll path are guaranteed to agree, closing that confusion at the source rather than adding a support doc.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (always applies); RULE NLR (touch-it-fix-it — don't leave the swallowed-Redis-error `catch |err| { log.warn(...) }` in place once a proper fallback exists, tighten the log to reflect it's now a recoverable-by-fallback condition, not a silent one).
- **`dispatch/write_zig.md`** — §1 touches `*.zig`: pg-drain lifecycle on any new DB read in `evaluateRef`'s fallback, tagged-union results for the fallback outcome, no allocator-lifecycle regressions in `approval_gate_async.zig`.
- **`dispatch/write_ts_adhere_bun.md`** — §2/§3 touch `*.ts`: const/import discipline, no new raw-HTML/design-token surface (pure CLI text).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — §1 | cross-compile both linux targets after adding the DB fallback read in `approval_gate_async.zig`; read `dispatch/write_zig.md` for pg-drain + errdefer discipline on the new query. |
| PUB / Struct-Shape | yes — §1 | the fallback's outcome (present in Redis / present via DB fallback / genuinely absent) should be a tagged union, not a bare bool, so callers can log which path answered. |
| File & Function Length (≤350/≤50/≤70) | no | `approval_gate_async.zig`, `fleet_logs.ts`, `workspace.ts` are all well under caps; the new fallback logic is small. |
| UFS (repeated/semantic literals) | yes — §3 | the corrected `agentsfleet credential` string should be a single named constant reused by both the JSON-mode and human-readable message, not two independent literals (today's bug — two independently-typed copies of the wrong string — is itself a UFS violation). |
| UI Substitution / DESIGN TOKEN | no | no UI surface in this workstream. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes — LOGGING, §1 | the existing `resolve_redis_fail` warn log (line 241) stays, but a new log event should mark when the DB fallback is actually used by the read side, per `docs/LOGGING_STANDARD.md`. |

---

## Overview

**Goal (testable):** A committed approval/denial decision is observable by the lease-poll path even when the Redis mirror write fails; `agentsfleet fleet logs` never crashes on a malformed `created_at`; `agentsfleet workspace credentials` prints a command that actually exists.

**Problem:** (1) `approval_gate.zig`'s `resolve()` commits the terminal decision to Postgres, then best-effort-mirrors it into Redis; if that mirror write fails, the HTTP caller still gets `200 approved`, but the fleet's own lease-poll path (`approval_gate_async.evaluateRef`, which only reads Redis) never sees the decision, times out, and the sweeper marks the same action `timed_out` — the audit trail says approved, enforcement says timed-out, permanently, with no reconciliation path. (2) `agentsfleet fleet logs` throws `RangeError: Invalid time value` and crashes the whole command if any event's `created_at` is a non-parseable string. (3) `agentsfleet workspace credentials` tells users to run `agentsfleet agent credential` — a command that does not exist anywhere in the CLI tree, sending them into a dead end for actually managing credentials.

**Solution summary:** Give `evaluateRef` a fallback: when the Redis mirror key for an action is absent, read the DB row directly (the same row `ResolveArgs.atomic()` already commits) before concluding `.pending`/`.expired` — closing the durability gap at the read side rather than adding write-side retry machinery. Guard `fleet_logs.ts`'s `formatTimestamp` so an unparseable input falls back to the existing "—" literal instead of throwing. Replace the two occurrences of the nonexistent `agentsfleet agent credential` string in `workspace.ts` with the real `agentsfleet credential` command.

---

## Prior-Art / Reference Implementations

- **API (Redis fallback)** → `src/agentsfleetd/fleet_runtime/approval_gate_db.zig`'s `ResolveArgs.atomic()` (lines 150–185) — the established "DB row is the single source of truth, read it directly on the loser/fallback path" shape already used for the winner-vs-already-resolved split. **Alignment:** the new DB fallback read in `evaluateRef` should query the same `core.fleet_approval_gates` row shape this function already returns, not invent a new query. **Divergence:** none — this is applying the existing pattern to a second caller.
- **CLI** → `cli/src/commands/auth.ts:64` for the `typeof`/`Number.isFinite` guard idiom on timestamp formatting. **Alignment:** reuse the guard style. **Divergence:** `fleet_logs.ts`'s input can also be a string (per `EventRow.created_at`'s type), so the guard additionally needs to reject a `Date` whose `.getTime()` is `NaN` after construction, which `auth.ts`'s numeric-only guard doesn't need to.
- **CLI (command tree)** → `cli/src/program/cli-tree-fleet.ts:129-158`, the real `credential` command group. **Alignment:** point `workspace.ts`'s redirect message at this exact group. **Divergence:** none.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/fleet_runtime/approval_gate_async.zig` | EDIT | `evaluateRef`/`readDecision` gain a DB fallback read when the Redis mirror key is absent. |
| `src/agentsfleetd/fleet_runtime/approval_gate.zig` | EDIT | tighten the `resolve_redis_fail` log event to note the read-side fallback now covers this case (LOGGING rule); no behavior change to the write path itself. |
| `src/agentsfleetd/http/handlers/approvals/inbox_integration_test.zig` | EDIT | add integration coverage for "Redis mirror write fails on the winning resolve path, lease-poll still observes the correct terminal decision via DB fallback." |
| `cli/src/commands/fleet_logs.ts` | EDIT | guard `formatTimestamp` against a non-parseable `created_at` instead of throwing. |
| `cli/test/fleet-logs-linecov.unit.test.ts` | EDIT | add a case feeding a malformed `created_at`, asserting no throw and the fallback literal is rendered. |
| `cli/src/commands/workspace.ts` | EDIT | replace both `agentsfleet agent credential` occurrences (lines 272, 278) with the real `agentsfleet credential` command, as one shared constant. |
| `cli/test/workspace-effect.unit.test.ts` | EDIT | assert the redirect message names the real `credential` command, not the phantom `agent credential` one. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three independent, narrowly-scoped patches sharing one P0 workstream because each is blocking-severity and small; no shared code between the sections.
- **Alternatives considered:** for §1, a write-side retry/queue on the Redis mirror write was considered and rejected for now — it adds retry-state machinery for a case the read-side DB fallback already closes correctly and more simply; a write-side retry could still lose to a sustained Redis outage, while a DB fallback read cannot.
- **Patch-vs-refactor verdict:** **patch**, all three. None of these change an interface or add new state; each closes a specific gap in existing, otherwise-correct logic.

---

## Sections (implementation slices)

### §1 — Approval decision survives a Redis mirror-write failure

The DB commit in `ResolveArgs.atomic()` is already durable and race-safe; the gap is that the only reader of the terminal decision on the enforcement path (`evaluateRef`) trusts an ephemeral Redis mirror that can silently fail to be written. **Implementation default:** when `readDecision` finds no Redis key for `action_id`, `evaluateRef` queries the DB row directly (mirroring `ResolveArgs.atomic()`'s row shape) before falling through to `.pending`/`.expired`, because the DB is already the durable source of truth and this closes the gap with no new failure mode — deviate only if a live measurement shows this adds unacceptable per-poll DB load, in which case cache the fallback result briefly rather than abandoning it.

- **Dimension 1.1** — a committed `.resolved` DB row is observable by `evaluateRef` even when the Redis mirror key was never written → Test `test_evaluate_ref_falls_back_to_db_on_missing_redis_key`.
- **Dimension 1.2** — the sweeper does not double-process a row that the DB fallback already resolved (no race between the fallback read and the sweeper's own timeout transition) → Test `test_sweeper_does_not_reprocess_db_fallback_resolved_row`.

### §2 — `fleet_logs.ts` never throws on a malformed timestamp

`formatTimestamp` currently only guards falsy input; any truthy-but-unparseable `created_at` throws `RangeError` and kills the whole `logs` command. **Implementation default:** guard via `Number.isNaN(date.getTime())` after construction (covers both `string` and `number` inputs per `EventRow.created_at`'s type), falling back to the existing `"—"` literal — because that literal already means "no usable timestamp" everywhere else in this function.

- **Dimension 2.1** — a malformed `created_at` (unparseable string, `NaN`, out-of-range number) renders `"—"` instead of throwing → Test `test_format_timestamp_falls_back_on_invalid_input`.

### §3 — `workspace.ts` points users at a command that exists

Both messages in `workspaceCredentialsEffect` print `agentsfleet agent credential`, which has no corresponding command anywhere in the CLI tree. **Implementation default:** replace with `agentsfleet credential` (the real top-level group), sourced from one named constant shared by the JSON and human-readable messages so they cannot re-diverge.

- **Dimension 3.1** — both the JSON-mode and human-readable redirect messages name the real `credential` command group → Test `test_workspace_credentials_redirect_names_real_command`.

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `approval_decision_db_fallback_used` | ops | `evaluateRef` resolves an action via the new DB fallback (Redis mirror key was absent) | `action_id`, `outcome` | no token/secret material | `test_evaluate_ref_falls_back_to_db_on_missing_redis_key` |

Existing `resolve_redis_fail` warn log (unchanged emission point) continues to prove the write-side failure occurred; the new event above proves the read-side recovered from it. No CLI-facing analytics event changes — `fleet_logs`/`workspace` fixes are correctness-only, no new signal.

---

## Interfaces

```
No public HTTP interface changes. approval_gate_async.evaluateRef(...) internal
return shape gains a variant/field indicating the decision source
(redis | db_fallback) for the new metric above; callers outside this module
are unaffected (they already only care about the resolved outcome).

CLI: `agentsfleet fleet logs` output format unchanged (same "—" fallback
literal, now reached on more inputs). `agentsfleet workspace credentials`
output text changes only the command string it prints.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Redis mirror write fails on resolve | network blip / Redis unavailable at write time | DB commit still succeeds; `evaluateRef` DB-fallback closes the gap so enforcement matches the audit trail; existing `resolve_redis_fail` warn log still fires. |
| DB fallback read races the sweeper | sweeper's timeout transition and the fallback read happen concurrently | the DB row's own `WHERE status = 'pending'` guard (already atomic in `approval_gate_db.zig`) means only one of them can win the terminal transition; the fallback read only *observes*, never writes, so no new race is introduced. |
| Malformed `created_at` from the API | upstream/legacy row has a non-numeric or unparseable timestamp | `fleet_logs` renders `"—"` for that row instead of crashing the command. |
| `workspace credentials` invoked | user runs the redirect command | prints the real `agentsfleet credential` group instead of a dead-end string. |

Timeout / auth failure / network blip / exceeded quota / dependency unavailable: unchanged by this workstream — covered by existing approval-gate and CLI-transport handling, not touched here.

---

## Invariants

1. A terminal approval/denial decision committed to `core.fleet_approval_gates` is observable by `evaluateRef` regardless of whether the Redis mirror write succeeded — enforced by Dimension 1.1's test plus the DB fallback being unconditional (not feature-flagged).
2. `formatTimestamp` never throws for any `raw: number | string | null | undefined` input — enforced by Dimension 2.1's test covering the full input-type surface.
3. Every user-facing string naming the credential-management command matches an actually-registered CLI command — enforced by Dimension 3.1's test asserting against the real `cli-tree-fleet.ts` registration, not a hardcoded string duplicate.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|-----------------------------------------------|
| 1.1 | integration | `test_evaluate_ref_falls_back_to_db_on_missing_redis_key` | DB row committed `status='approved'`, Redis key for `action_id` never written → `evaluateRef` returns the approved outcome, not `.pending`/`.expired`. |
| 1.2 | integration | `test_sweeper_does_not_reprocess_db_fallback_resolved_row` | Same setup as 1.1, sweeper sweep runs concurrently → row stays `approved`, sweeper does not overwrite it to `timed_out`. |
| 2.1 | unit | `test_format_timestamp_falls_back_on_invalid_input` | `formatTimestamp("garbage")`, `formatTimestamp(NaN)`, `formatTimestamp(Number.MAX_VALUE)` each return `"—"`, no throw; `formatTimestamp(1735689600000)` still returns a valid ISO string (regression). |
| 3.1 | unit | `test_workspace_credentials_redirect_names_real_command` | both JSON-mode and human-readable output from `workspaceCredentialsEffect` contain `agentsfleet credential`, not `agentsfleet agent credential`. |

Regression: Dimension 2.1 includes a valid-timestamp case to prove the existing happy path is unchanged. Idempotency/replay: N/A — no retry semantics added in this workstream (the DB fallback is a read, not a write).

---

## Acceptance Criteria

- [x] Approved/denied decision observable via DB fallback even when Redis mirror write fails — verify: `zig build test --summary all` (Dimensions 1.1/1.2, plus denied + pending-not-over-fired coverage)
- [x] `fleet_logs` does not throw on a malformed timestamp — verify: `bun test cli/test/fleet-logs-linecov.unit.test.ts`
- [x] `workspace credentials` names a real command — verify: `bun test cli/test/workspace-effect.unit.test.ts`
- [x] `make lint-zig` clean · CLI unit tests pass (32/32) · Zig unit graph green
- [~] `make test-integration` — §1 tests pass on a clean DB (see Verification Evidence); full local suite non-deterministic due to Docker Postgres instability, deferred to CI
- [x] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [x] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: Zig unit + integration tests
zig build test --summary all && echo "PASS" || echo "FAIL"
# E2: Build
zig build 2>&1 | tail -5
# E3: CLI tests
cd cli && bun test test/fleet-logs-linecov.unit.test.ts test/workspace-effect.unit.test.ts
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: Orphan sweep for the removed phantom-command string
grep -rn "agent credential" cli/src/ | head
```

---

## Dead Code Sweep

N/A — no files deleted; this workstream only edits existing functions in place.

---

## Discovery (consult log)

- **Root-cause correction vs the audit's original vote text:** the fleet-wide-refactor-audit's per-vote reason text for §1 described this as resembling a double-resolve/race defect. Direct re-verification (this spec's authoring pass) found the DB-side transition is already race-safe (`ResolveArgs.atomic()`'s single conditional `UPDATE ... RETURNING`); the actual P0 defect is the swallowed Redis-mirror-write error causing DB/Redis divergence. The spec is written against the re-verified root cause, not the original vote summary.
- **Systemic pattern flagged, not fixed here:** every other `new Date(x).toISOString()` call site in `cli/src/` (`fleet_steer_events.ts:71`, `fleet_events.ts:82`, `fleet_key.ts:149,194`, `grant.ts:105,108`, `memory.ts:107`) has the same unguarded shape as §2's bug. Out of scope for this P0 workstream (see Out of Scope) — flagged for a P2/P3 follow-up spec.
- **Metrics review:** one new event added (`approval_decision_db_fallback_used`); no existing analytics/funnel playbook covers backend approval-gate internals, so no playbook update required.
- **`/write-unit-test` coverage audit (diff ledger):** beyond the spec's Dimensions 1.1/1.2 (approved-via-fallback), the audit flagged two uncovered §1 branches on a security-grade change and closed them: (a) `statusToDecision` denied/timed_out→`.denied` — added `evaluateRef DB fallback surfaces a denied decision`; (b) `readTerminalDecision` non-terminal→null (fallback must not fabricate a decision) — added `evaluateRef stays pending when the DB row is unresolved`. Waived: `readTerminalDecision` no-row→null (a gate ref and its DB row are created atomically in `requestNewGate`, so ref-without-row is unreachable in production). §2/§3 fully covered (all `formatTimestamp` branches + boundaries; both credential-redirect modes). Ledger: 8 changed units — 7 tested, 1 `won't-test` (unreachable).
- **Integration-suite environment finding:** the full `make test-integration` is non-deterministic on this local macOS-Docker Postgres — under the parallel test-server load it returns `error.PG`/`ConnectionRefused` from shared harness/seed code (`openHandlerTestConn`, `seedTestData`), failing a *different* set of untouched tests each run (tenant_billing, IDOR, auth-401, pool-grants). A bounded clean-DB slice (`-Dtest-filter=inbox_integration`, LIVE_DB + TLS Redis) passes all four §1 fallback tests (62/64; the 2 fails were pre-existing 401 tests on the same flaky seed). Authoritative integration gate deferred to CI (stable Postgres). Not a code defect in this diff.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, architecture, `dispatch/write_zig.md`, Failure Modes, Invariants, Metrics. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Zig unit graph | `zig build test` | `test success` — 1421 pass, 456 skip (pre-existing flaky `worker_started` boot race unrelated) | ✅ |
| CLI unit tests | `bun test cli/test/fleet-logs-linecov.unit.test.ts cli/test/workspace-effect.unit.test.ts` | 32 pass / 0 fail | ✅ |
| §1 integration (clean DB) | `zig build test -Dtest-filter=inbox_integration` (LIVE_DB, TLS Redis) | 62/64 pass incl. all four §1 fallback tests; the 2 fails were pre-existing `401` tests on flaky Postgres seed | ✅ (§1 tests) |
| Integration (full suite) | `make test-integration` | non-deterministic locally — Docker Postgres returns `error.PG`/`ConnectionRefused` in shared harness/seed code under parallel load; failure set varies per run across untouched tests too. Deferred to CI (stable Postgres). | ⚠️ env |
| Lint (Zig) | `make lint-zig` | ZLint 0/0 · pg-drain ✅ · fmt ✅ · line-limit ✅ · test-depth unit=2276 integration=247 (baseline 2272/243, +4/+4) | ✅ |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | both green | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found | ✅ |

---

## Out of Scope

- A general retry/queue layer for Redis mirror writes — the DB fallback read makes this unnecessary for correctness; follow-up only if a live measurement shows fallback-read DB load is unacceptable.
- Auditing/fixing every other unguarded `new Date(x).toISOString()` call site in `cli/src/` beyond `fleet_logs.ts` — flagged in Discovery, tracked as a P2/P3 follow-up spec.
- Any UI-surface change — this workstream has no UI category.
