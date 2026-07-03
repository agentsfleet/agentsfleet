<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M109_002: Harden 4 backend paths that collapse a real failure into a silent success

**Prototype:** v2.0.0
**Milestone:** M109
**Workstream:** 002
**Date:** Jul 02, 2026
**Status:** DONE
**Priority:** P1 — each defect is a caller-visible-signal loss (a DB/network failure reported as success), not a crash; confirmed by 2-3 independent adversarial verifier passes per finding, none yet observed in production but all reachable under real failure conditions (pool exhaustion, transient network blips).
**Categories:** API OBS
**Batch:** B1 — independent of M109_001/003/004; no shared files.
**Branch:** feat/m109-002-silent-failure-hardening
**Test Baseline:** unit=2272 integration=243
**Depends on:** None.
**Provenance:** agent-generated (pre-spec, fleet-wide-refactor-audit `Workflow` run `wf_8ec169f4-8e4`, each finding independently re-verified against current source before this spec was drafted, Jul 02, 2026).

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

**Canonical architecture:** `docs/architecture/data_flow.md` — the balance/receive/approval/run gate chain (§1 here is the grant-approval sibling of M109_001 §1's approval gate); `docs/architecture/runner_fleet.md` — the runner daemon's control-plane channel (§4).

---

## Implementing agent — read these first

1. `src/agentsfleetd/fleet_runtime/approval_gate_db.zig:150-185` — `ResolveArgs.atomic()`'s `UPDATE ... RETURNING` + fall-through-to-`SELECT` pattern is the exact shape §1 must adopt for `grant_approval.zig`'s `applyDecision`; `error_registry.zig:189`'s `ERR_APPROVAL_ALREADY_RESOLVED = "UZ-APPROVAL-006"` is the existing error code for "lost the race," worth mirroring with a grant-specific code.
2. `src/agentsfleetd/state/model_caps_store.zig:176-211` — `create`/`updateRates`/`remove` in the same file already return `!?i64` and let the caller distinguish DB error from not-found/no-op; `isReferencedByActiveDefault` (lines 161-172) is the outlier §2 brings in line with its own siblings.
3. `src/runner/daemon/control_plane_client.zig:279-344` — `post()`/`get()`'s `self.http.fetch(...) catch return ClientError.RequestFailed;` (lines 308, 341) is the correct "convert transport failure to a named error" shape §3 (`otlp/post.zig`) should adopt for its own `fetch()` call.
4. `docs/LOGGING_STANDARD.md` — the log-event-naming convention for the new/tightened warn events across all four sections.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Stop 4 backend paths from reporting a DB/network failure as success
- **Intent (one sentence):** A caller of grant-approval, model-cap deletion, telemetry export, or the runner's control-plane client can always tell a real failure apart from a genuine success.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: …`). A mismatch between this restatement and the Intent above → STOP and reconcile before any edit.

---

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — a second, late click on an already-resolved Slack grant approval gets an honest "already resolved" response instead of a silent no-op 200; an admin trying to delete a model-cap that's actually the active default gets blocked even during a transient DB blip, not just on the happy path; an operator watching Grafana during an OTLP export outage sees a warn log instead of silence; the runner survives a mid-stream control-plane response failure without leaking memory.
2. **Preserved user behaviour** — every happy-path response (successful grant decision, successful model-cap delete, successful telemetry export, successful control-plane call) is byte-for-byte unchanged; this workstream only changes what happens on the failure branches that currently mask themselves as success.
3. **Optimal-way check** — the unconstrained-optimal fix is to propagate every error precisely, which is exactly what each of the four sections does; no larger design change is warranted.
4. **Rebuild-vs-iterate** — iterate. Three of the four sections (§1, §2, §4) already have a correct sibling function in the same file to mirror; §3 has a correct sibling file (`control_plane_client.zig`) to mirror. No new abstraction needed.
5. **What we build** — a tri-state (or `!?i64`-shaped) return from `applyDecision` distinguishing applied/already-resolved/db-error; an error-propagating `isReferencedByActiveDefault`; an error-propagating `otlp/post.zig` transport call; an `errdefer`-guarded `Allocating` writer in `control_plane_client.zig`'s `post()`/`get()`.
6. **What we do NOT build** — a generic "never swallow an error" lint rule across the whole codebase (out of scope, would need its own spec and tooling); a retry policy for OTLP export (propagating the error to the existing `flushOnce` warn-log caller is sufficient — it already retries on the next flush interval).
7. **Fit with existing features** — §1 must not destabilize the existing double-resolve-returns-409 test already covered for the sibling `approval_gate` flow (`inbox_integration_test.zig:539`) — the grant-approval fix follows the same shape but has its own test file. §2 must not destabilize the existing `"admin models: deleting the active default's model is blocked 409"` happy-path test.
8. **Surface order** — API-only; none of these four defects have a CLI/UI surface.
9. **Dashboard restraint** — N/A, no UI surface.
10. **Confused-user next step** — for §1/§2, the caller now gets a structured error response naming what happened, which is itself the self-serve signal (no separate doc needed); for §3/§4, the "confused user" is an on-call operator, whose self-serve move is the new/tightened warn log itself.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline; RULE NLR (touch-it-fix-it on every function edited, not just the swallowed-error line).
- **`dispatch/write_zig.md`** — all four sections touch `*.zig`: tagged-union results (§1, §2), multi-step `errdefer` placement (§4 — the `Allocating` writer's lifecycle), pg-drain lifecycle (§1, §2's DB queries), cross-compile both linux targets.
- **`docs/LOGGING_STANDARD.md`** — §3/§4 both change what gets logged on a failure path; name the event per the standard, don't ad-hoc a new format.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — all 4 sections | memory-safety review on §4's `errdefer` addition especially (this is the one section with a leak, not just a swallowed-error signal loss); cross-compile both linux targets. |
| PUB / Struct-Shape | yes — §1, §2 | `applyDecision`'s new return shape and `isReferencedByActiveDefault`'s new `!bool` (or tri-state) are both pub-surface changes within their modules; verdict: tagged union for §1 (applied / already_resolved / db_error, mirroring `ResolveArgs`'s own outcome union), `!bool` for §2 (matches its three siblings' `!?i64` error-propagation shape without over-engineering a union for a boolean question). |
| File & Function Length (≤350/≤50/≤70) | no | all four files stay well under caps after these edits. |
| UFS | no | no new repeated/semantic literals introduced. |
| LOGGING | yes — §3, §4 | §3's fetch-failure now reaches `flushOnce`'s existing `catch |err| log.warn(...)`, which already logs `EVENT_IGNORED_ERROR` correctly once the error actually propagates — no new event name needed, just closing the gap that was preventing it from firing; §4 adds a leak-path regression test, no new log event needed since the fix is memory-safety, not signal. |
| ERROR REGISTRY | yes — §1 | new grant-specific "already resolved" error code, mirroring `ERR_APPROVAL_ALREADY_RESOLVED`. |
| SCHEMA / UI / DESIGN TOKEN | no | no schema or UI surface touched. |

---

## Overview

**Goal (testable):** `applyDecision`'s second (racing/late) call on a grant returns a distinguishable "already resolved" outcome instead of a silent 200; `isReferencedByActiveDefault` fails closed (blocks the delete) on a DB error instead of failing open; an OTLP transport failure reaches the exporter's existing warn log instead of looking identical to success; `control_plane_client.zig`'s `post()`/`get()` release their response buffer on every path, including a fetch failure mid-stream.

**Problem:** All four functions currently discard the one signal a caller needs to distinguish "it worked" from "it didn't," in ways that either mask state (grant/model-cap) or mask outages (OTLP export) or leak memory (control-plane client) — each confirmed reachable under realistic failure conditions (concurrent requests, DB pool pressure, network blips, mid-stream connection drops), not just theoretical.

**Solution summary:** Change each function's discarded-value handling so the caller receives the real outcome: `applyDecision` returns a tri-state instead of `bool`; `isReferencedByActiveDefault` propagates the query error instead of collapsing it to `false`; `otlp/post.zig`'s `fetch()` call propagates its error instead of `catch return;`; `control_plane_client.zig` adds an `errdefer` on its `Allocating` writer so a fetch failure releases the partially-written buffer instead of leaking it.

---

## Prior-Art / Reference Implementations

- **API (§1)** → `src/agentsfleetd/fleet_runtime/approval_gate_db.zig:150-185`, `ResolveArgs.atomic()`. **Alignment:** same "conditional UPDATE, RETURNING distinguishes winner from loser" shape. **Divergence:** grant-approval's schema/table differs, so the query itself is new, only the pattern is mirrored.
- **API (§2)** → `src/agentsfleetd/state/model_caps_store.zig:176-211`, `create`/`updateRates`/`remove` — three siblings in the same file already do this correctly. **Alignment:** exact shape match, same file. **Divergence:** none.
- **API (§3)** → `src/runner/daemon/control_plane_client.zig:279-344`, `post()`/`get()`'s `catch return ClientError.RequestFailed;`. **Alignment:** same "name the transport error" shape. **Divergence:** `otlp/post.zig` returns `!void` not a custom error set; propagate via `catch |err| return err;` rather than introducing a new error type, since the caller (`flushOnce`) only logs `@errorName(err)` and doesn't branch on it.
- **API (§4)** → the connection-acquire `defer pool.release(conn)` idiom used throughout (e.g. `grant_approval.zig:186-190`, `model_caps_admin.zig:137-141`) — pair a resource with `defer`/`errdefer` immediately at construction. **Alignment:** apply the same immediately-after-construction discipline to `Allocating`. **Divergence:** none.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/webhooks/grant_approval.zig` | EDIT | `applyDecision` returns a tri-state outcome instead of discarding `conn.exec()`'s affected-row count; handler responds distinctly on the already-resolved case. |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | add the `UZ-GRANT-003` "Grant already resolved" entry (`.conflict`), mirroring `UZ-APPROVAL-006`. (Spec originally named `error_entries.zig`; the grant/approval entries actually live in the `_runtime` sibling.) |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | add the `ERR_GRANT_ALREADY_RESOLVED` const (satisfies the comptime "every ERR_* has a REGISTRY entry" check). |
| `src/agentsfleetd/http/webhook_http_integration_test.zig` | EDIT | add coverage for the racing/late-decision no-op case now surfacing as a distinct response. |
| `src/agentsfleetd/state/model_caps_store.zig` | EDIT | `isReferencedByActiveDefault` propagates query errors instead of collapsing them to `false`. |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin.zig` | EDIT | `innerDeleteAdminModel` handles the new error path (fail closed, respond 503/internal-error) instead of assuming `false` means "not referenced." |
| `src/agentsfleetd/http/handlers/admin/model_caps_admin_integration_test.zig` | EDIT | add coverage for the guard's query-failure path (currently only the happy-path 409 is tested). |
| `src/agentsfleetd/observability/otlp/post.zig` → `Client.zig` | EDIT + RENAME | §3: `fetch()` propagates its error (via `try`); §6 (folded): the remaining URL/auth formatting swallows propagate too, the b64 guard becomes a `std.debug.assert`, `defer` cleanups added, and the file is rearchitected to `const Client = @This()` (Single-Type-Module) — hence the rename. |
| `src/agentsfleetd/observability/otlp/post_test.zig` → `Client_test.zig` | EDIT + RENAME | §3 fault-injected-transport test, now on `std.testing.allocator` (propagation + zero-leak on the error path). Renamed to pair with `Client.zig`. |
| `src/agentsfleetd/observability/otlp/exporter.zig` | EDIT (§6 importer) | `@import("post.zig")` → `@import("Client.zig")`; `post.Client` → `Client`. |
| `src/agentsfleetd/tests.zig` | EDIT (§6 importer) | test-discovery import `otlp/post.zig` → `otlp/Client.zig`. |
| `src/agentsfleetd/observability/otel_logs.zig` | EDIT (§6 rename fan-out) | module doc-comment reference `otlp/post.zig` → `otlp/Client.zig` (repoint a dangling doc reference the rename left; surfaced by `/review`). |
| `src/runner/daemon/control_plane_client.zig` | EDIT | §4: `errdefer aw.deinit();` releases the partial buffer on a mid-stream fetch failure; §5 (folded): `post()`/`get()` deduped into a shared private `send()`, so the errdefer + fetch live in one place (file 350 → 347). |
| `src/runner/daemon/control_plane_client_test.zig` | EDIT | §4: leak-detecting test (via `std.testing.allocator`) for a chunked-stream-cut mid-stream fetch failure, both verbs. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four independent one-function patches sharing a workstream because they're the same class of defect (a discarded signal) at the same priority, not because they share code.
- **Alternatives considered:** a shared "Result-tri-state" helper type across §1 and §2 was considered and rejected — `ResolveArgs`'s outcome union and `model_caps_store`'s `!?i64` are each already-established idioms in their own files; forcing a shared type would fight existing local conventions for no benefit.
- **Patch-vs-refactor verdict:** **patch**, all four — each closes a specific discarded-signal gap in an otherwise-correct function, mirroring an already-correct sibling.

---

## Sections (implementation slices)

### §1 — `grant_approval.zig`: distinguish applied from already-resolved

`applyDecision`'s `UPDATE ... AND status = $5` can affect 0 rows on a race/late-click without that being distinguishable from a genuine 1-row success. **Implementation default:** capture the `?i64` affected-row count from `conn.exec()` and return a tri-state (applied / already_resolved / db_error), mirroring `ResolveArgs`'s outcome union, because the DB error case (query itself failing) must remain distinct from the 0-rows no-op case.

- **Dimension 1.1** — a second/racing decision on an already-resolved grant returns a distinguishable "already resolved" outcome, not a silent 200 → Test `test_apply_decision_distinguishes_already_resolved_from_applied`.
- **Dimension 1.2** — the happy-path single decision still returns "applied" and responds 200 (regression) → Test `test_apply_decision_happy_path_unchanged`.

### §2 — `model_caps_store.zig`: fail closed on a query error

`isReferencedByActiveDefault` returning `false` on a DB error lets a transient blip bypass the guard that blocks deleting the active default's model-cap. **Implementation default:** change the return type to `!bool`, propagating the query error so the caller can fail closed (block the delete, respond internal-error) rather than proceeding — matching the three sibling functions in the same file.

- **Dimension 2.1** — a query error during the referenced-by-default check blocks the delete (fails closed), does not silently allow it → Test `test_delete_blocked_on_referenced_check_query_error`.
- **Dimension 2.2** — the existing happy-path 409-when-referenced behavior is unchanged (regression) → Test `test_delete_blocked_when_genuinely_referenced` (extends the existing happy-path test, doesn't replace it).

### §3 — `otlp/post.zig`: propagate the transport error

The `fetch()` call's `catch return;` makes a DNS/TLS/connection failure look identical to a successful export, so the exporter's own warn-log-on-error path never fires. **Implementation default:** `catch |err| return err;` on the `fetch()` call specifically (the highest-severity swallow of the four in this file); the earlier buffer-formatting `catch return;` calls stay as documented Out of Scope for this workstream.

- **Dimension 3.1** — a fault-injected transport failure during `post()` propagates to `flushOnce`'s existing `catch |err| log.warn(...)`, producing a log entry → Test `test_post_propagates_transport_error_to_exporter_log`.

### §4 — `control_plane_client.zig`: release the buffer on a mid-stream fetch failure

`post()`/`get()` construct an `Allocating` writer but only release it on the success path (`toOwnedSlice()`); a fetch failure after partial bytes were already written leaks that buffer. **Implementation default:** `errdefer aw.deinit();` immediately after constructing `aw` in both functions, matching the connection-acquire `defer`-immediately-after-construction idiom already used elsewhere in this codebase.

- **Dimension 4.1** — a fault-injected fetch failure after partial writes releases the buffer (zero leak under `std.testing.allocator`) → Test `test_post_get_release_buffer_on_mid_stream_fetch_failure`.

---

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| (existing) `EVENT_IGNORED_ERROR` warn log | ops | §3's OTLP `fetch()` failure now reaches `flushOnce`'s existing handler | `err` (error name only) | no payload/credential material | `test_post_propagates_transport_error_to_exporter_log` |

No new event names — §3 restores an existing log's ability to fire; §1/§2/§4 are correctness/memory-safety fixes with no analytics surface. `not applicable — no product/operator signal changes` beyond the restored §3 log.

---

## Interfaces

```
grant_approval.applyDecision(...) — return type changes from bool to a
tri-state outcome (applied | already_resolved | db_error); the HTTP handler's
response body gains a distinct shape for the already_resolved case (still a
2xx family or 409, per the existing sibling ERR_APPROVAL_ALREADY_RESOLVED
convention — the implementing agent follows that convention's status code).

model_caps_store.isReferencedByActiveDefault(...) — return type changes from
bool to !bool; callers must handle the new error path.

otlp.post(...) — return type unchanged (!void); the fetch-failure branch now
returns a real error instead of bare success.

control_plane_client.post()/get() — return type and success-path behavior
unchanged; only the failure-path memory lifecycle changes.
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|----------------------------------------------------------|
| Racing/late grant decision | two Slack clicks, or a click after expiry | second caller gets a distinguishable already-resolved outcome instead of a silent 200. |
| DB query error during model-cap reference check | pool exhaustion, connection reset, timeout | delete is blocked (fail closed), caller gets an internal-error response, not a silent success. |
| OTLP transport failure | DNS/TLS/connection failure to the collector | `flushOnce`'s existing warn log fires; telemetry for that flush is lost but the loss is now visible. |
| Mid-stream control-plane fetch failure | connection dropped after partial response bytes written | `Allocating` buffer is released via `errdefer`; no leak. |

Timeout / auth failure / exceeded quota / replay: unchanged by this workstream — none of these four functions' existing timeout/auth/quota handling is touched.

---

## Invariants

1. `applyDecision`'s caller can always distinguish "this decision was applied" from "a decision was already recorded" from "the DB call itself failed" — enforced by the tri-state return type (compiler-checked exhaustive switch at every call site).
2. `isReferencedByActiveDefault`'s caller cannot proceed with a delete when the reference check itself failed — enforced by the `!bool` return type forcing an explicit `catch`/error-handling branch at the call site.
3. Every early-return inside `otlp.post()`'s `fetch()` call reaches the caller as a distinguishable error — enforced by Dimension 3.1's fault-injection test.
4. `control_plane_client.zig`'s `post()`/`get()` release their `Allocating` buffer on every return path — enforced by Dimension 4.1's leak-detecting test under `std.testing.allocator` (panics on leak at `deinit()`).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|-----------------------------------------------|
| 1.1 | integration | `test_apply_decision_distinguishes_already_resolved_from_applied` | two decisions on the same grant (approve then approve again) → first returns applied/200, second returns already_resolved, not a duplicate 200. |
| 1.2 | integration | `test_apply_decision_happy_path_unchanged` | single decision on a pending grant → applied/200, unchanged response shape. |
| 2.1 | integration | `test_delete_blocked_on_referenced_check_query_error` | fault-injected query error during the reference check → delete request fails closed (blocked, internal-error response), model-cap row still present. |
| 2.2 | integration | `"admin models: deleting the active default's model is blocked 409"` (pre-existing) | model-cap is the active default, no fault injected → 409, unchanged. Regression covered by the existing happy-path test (not a new `test_delete_blocked_when_genuinely_referenced` — the spec's aspirational name; coverage exists under the real test's name). |
| 3.1 | unit | `test_post_propagates_transport_error_to_exporter_log` | fault-injected `fetch()` failure inside `post()` → error propagates to `flushOnce`'s `catch`, warn log observed. |
| 4.1 | unit | `test_post_get_release_buffer_on_mid_stream_fetch_failure` | fault-injected fetch failure after partial writes into `aw` → `std.testing.allocator` reports zero leaks at `deinit()`. |

Regression: Dimensions 1.2 and 2.2 are the explicit regression cases for §1/§2. Idempotency/replay: N/A — none of these four sections add retry semantics.

---

## Acceptance Criteria

- [x] Racing grant decision distinguishable from applied — Dimension 1.1: `apply_decision` filter, 0 skip / 0 fail against real DB+Redis.
- [x] Model-cap delete fails closed on a query error — Dimension 2.1: `referenced_check_query_error` filter, 0 skip / 0 fail against real DB.
- [x] OTLP transport failure reaches the exporter's warn log — Dimension 3.1: agentsfleetd 33/33 (Client_test on `std.testing.allocator`, propagation + zero-leak).
- [x] Control-plane client leaks zero bytes on a mid-stream fetch failure — Dimension 4.1: runner 356 pass; red/green proven (leak fires without the errdefer, clean with it).
- [x] `make lint-zig` clean (ZLint 0/0, pg-drain, line-limit, schema-gate, isolation). `make test`/agentsfleetd unit green.
- [x] `make test-integration` passes (Full integration suite passed, EXIT 0; §1/§2 confirmed non-skipped above).
- [x] `make memleak` clean (agentsfleetd allocator guard; §4's runner-graph leak covered by the runner `testing.allocator` suite + red/green, since `make memleak` builds only the agentsfleetd graph).
- [x] Cross-compile clean ×4: agentsfleetd + runner, both `x86_64-linux` and `aarch64-linux`.
- [x] `gitleaks detect` clean · no file over 350 lines (`control_plane_client.zig` 350→347; `Client.zig` well under).

---

## Eval Commands (post-implementation)

```bash
# E1: full test suite
zig build test --summary all && echo "PASS" || echo "FAIL"
# E2: Build
zig build 2>&1 | tail -5
# E3: memleak (allocator wiring touched in §4)
make memleak 2>&1 | tail -10
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3 && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

No dead code introduced. The scope expansion renames (not deletes) `otlp/post.zig` → `Client.zig` and `post_test.zig` → `Client_test.zig` (`git mv`, all references repointed — no orphaned imports); the `control_plane_client` dedup removes the duplicated `post()`/`get()` bodies in favour of the shared `send()` (no callers lost — `post`/`get` keep their signatures for `mint`/`getSelf`/etc.). RULE ORP orphan sweep: clean.

---

## Discovery (consult log)

- **§3 scope narrowing:** `otlp/post.zig` has 3 other lower-severity `catch return;` calls (buffer-formatting failures on an overlong `instance_id`/`api_key`, lines 39/43/51) beyond the `fetch()` call this workstream fixes. Only the `fetch()` call is in scope here (it's the realistic, non-configuration-error failure mode); the formatting-buffer swallows are flagged for a P2/P3 follow-up in Out of Scope.
- **Metrics review:** no new event names; §3 restores an existing warn log's ability to fire. No analytics/funnel playbook applies to backend-internal telemetry-export plumbing.
- **File-shape (Single-Type-Module) flag — `otlp/post.zig`:** the file is single-primary-type + behavior-bound-to-state (`pub const Client = struct { inner: std.http.Client; init/deinit/post }`), so `dispatch/write_zig.md`'s binding rule says touching it (even §3's 1-line `catch |err| return err`) should rearchitect it to file-as-struct (`const Client = @This()`) and rename → `otlp/Client.zig` with its 3 importers updated. That reshape is **outside §3's Files-Changed scope** (a 1-line edit), and cross-scope opportunistic bundling is a Hard-Safety block without an explicit ask. Surfaced to Indy mid-implementation; no response (away). Resolved toward **scope-discipline: §3 stays the minimal fix; the file-as-struct reshape is a noted debt for Indy's call** (fold into this PR on approval, else a follow-up). `control_plane_client.zig` (§4) is already file-as-struct; the other touched files are legitimately conventional (handler-collections, function-namespace store, data/registry).
- **§4 leak proof (red/green):** with the `errdefer aw.deinit()` present, `test_post_get_release_buffer_on_mid_stream_fetch_failure` is green (5/5) under `std.testing.allocator`; with `post()`'s errdefer removed, the same test reports `memory address … leaked` (the `Allocating` writer's partial buffer) — confirming the leak is real and the fix load-bearing. Fault injected via a `Transfer-Encoding: chunked` response cut before its terminating 0-chunk (a Content-Length short read is tolerated by the client as a partial-success, so it can't exercise the error path).
- **`control_plane_client.zig` at the FLL cap:** the file was exactly 350 lines; §4's two `errdefer` lines were absorbed net-zero by tightening the adjacent BUFFER GATE comments; the §5 dedup then dropped it to 347.
- **Test Coverage Audit (`/write-unit-test`, Change-set mode):** diff ledger **9/9 resolved — 7 tested, 2 `needs-infra`** (§1 `.db_error` arm — unchanged behaviour, restructured not new; §2 handler `catch`→503 over HTTP — the uid is uuid-validated upstream so a malformed uid can't reach the store via HTTP, and the store-level propagation is tested directly). Both need a real pool fault for e2e injection. **One gap filled during the audit:** `test_post_propagates_oversized_auth_formatting_error` (§6 — a 600-byte `instance_id` overflows the Basic-auth `bufPrint`, now propagating `error.NoSpaceLeft` instead of the pre-fix silent success; red/green vs the swallow). Error-path coverage: 100% of the swallow→propagate changes; negative-path ratio ~5/7 (≥50%). **Production-safety:** Zig-leak ✅ (Client_test on `testing.allocator` + §4 red/green + `make memleak` 0 leaks; `checkAllAllocationFailures` not required — `post()`'s sole allocation propagates via `try` before any `defer` registers, so there is no partial-init leak surface). Concurrency/Performance: **N/A** — cold failure-path fixes, no new shared state or hot path (the §5 dedup preserves complexity; §1's exactly-once is DB-atomic via the conditional `UPDATE`, and the 100-way-race proof already exists on the sibling `approval_gate` flow). Mutation not run (Change-set); the §3/§4/§6 red/green cover the risk surface.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `dispatch/write_zig.md`, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `zig build test --summary all` | {paste snippet} | |
| Integration tests | `make test-integration` | {paste snippet} | |
| Memleak | `make memleak` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |

---

## Out of Scope

- A generic "never swallow an error" lint rule + whole-repo sweep — genuinely milestone-sized (new gate tooling + hundreds of files); stays its own spec. (Explicitly excluded from the scope expansion below.)
- A retry policy for OTLP export — the existing flush-interval retry is sufficient once the error is actually visible.

> **Formerly out of scope, now folded in** (see Scope Expansion below): `otlp/Client.zig`'s remaining buffer-formatting swallows (originally deferred to a P2/P3) are now propagated.

---

## Scope Expansion (folded findings)

After the four core fixes landed and verified, the implementing agent surfaced three refactor findings on the files already in scope; Indy directed folding the contained ones into this PR:

> Indy (2026-07-03): "I think fold the findings you did into this PR" — context: fold the contained refactor findings (not the milestone-sized systemic lint+sweep, which stays a separate spec).

Folded (all on files already touched, all in the silent-failure / struct-model theme):

- **§5 — `control_plane_client.zig` `post()`/`get()` dedup.** The two ~90%-identical verbs collapse into one private `send(method, path, bearer, payload: ?[]const u8, deadline)`; `post`/`get` become thin wrappers. Single `errdefer` (replacing the duplicate §4 added), single `fetch`. File drops 350 → 347. Covered by the existing runner verb tests + §4's leak test.
- **§6 — `otlp/post.zig` fully hardened + reshaped to file-as-struct.** The 3 remaining `catch return;` swallows (URL/auth formatting) now `try`-propagate; the provably-satisfied b64 buffer guard becomes a `std.debug.assert` (invariant, not silent return); the discarded response buffer + URL scratch gain `defer` cleanups (allocator-agnostic, no arena-masking). The file is rearchitected to `const Client = @This()` and renamed `post.zig` → `Client.zig` (+ `post_test.zig` → `Client_test.zig`), resolving the Single-Type-Module debt; its 3 importers (`exporter.zig`, `tests.zig`, the internal test import) are updated. §3's test now runs on `std.testing.allocator`, doubling as a zero-leak proof on the error path.

Explicitly **not** folded: the systemic swallowed-error lint rule + whole-repo sweep (its own spec).
