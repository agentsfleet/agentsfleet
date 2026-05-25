# M80_002: Cut over from the direct worker path to the `zombie-runner` daemon

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 002
**Date:** May 25, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the execution-architecture cutover; it makes the runner the real processor and removes the datastore-welded worker. Customer/operator-facing (work can run on hosts holding no datastore credentials).
**Categories:** API
**Batch:** B2 — single collapsed cutover; absorbs roadmap S1–S4 (see Discovery) and ships in one PR atop the M80_001 keystone.
**Branch:** `feat/m80-001-runner-contract-keystone` (continues the keystone branch — one PR; see Discovery "Git flow")
**Depends on:** M80_001 (frozen `/v1/runners` contract, `fleet.runners`/`fleet.runner_leases` schema, `runnerBearer` auth plane, lease/report handlers) — already committed on the branch.
**Provenance:** agent-generated (Indy redirect, May 25, 2026: "build the real thing, not throwaway, one PR through the cutover"; anchors confirmed via handshake — separate daemon, multi-runner + scheduler-grade assignment/fencing/reclaim, trusted-fleet inline secrets).

> **Provenance is load-bearing.** Agent-drafted from `docs/architecture/runner_fleet.md` + the M80_001 kill-list — cross-check every claim against the codebase before EXECUTE; the engine-fold-in + sandbox subsystems (`src/executor/runtime/**`, Landlock/cgroups) are not yet read at spec-authoring depth and must be walked during PLAN.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (the target this realizes) and `docs/architecture/data_flow.md` (the current single-process path this deletes). Both are reconciled by this workstream (§7).

---

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` — the split, the control protocol, sticky-routing + reclaim (`lease_expires_at` + `fencing_token`), the fork-sandboxed-child model, sandbox tiers, the Redis topology shift.
2. `docs/architecture/data_flow.md` §C — the direct worker hot path (`zombie_events` received→terminal, telemetry, two-debit billing, session checkpoint, `XACK`) the runner now drives via the protocol; the source for the reconciliation in §7.
3. `src/runner/service.zig` + `service_report.zig` — the M80_001 lease/report handlers this extends (assignment beyond the single-zombie pick; fencing verification at report).
4. `src/cmd/worker.zig`, `worker_watcher.zig`, `worker_zombie.zig`, `src/zombie/event_loop*.zig` — the direct path stripped in §3.
5. `src/executor/{client,transport,main,runner,handler,session,tool_bridge,runner_helpers}.zig`, `src/executor/runtime/**`, sandbox (`landlock`, `cgroup`, `network`, `executor_network_policy`) — the engine migrated into the runner in §2; `build_runner.zig` is its build graph.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Cut over execution to the zombie-runner daemon; delete the direct worker path (M80 cutover)
- **Intent (one sentence):** Make the host-resident `zombie-runner` daemon the real event processor — it leases work from `zombied` over HTTPS, runs NullClaw in a forked sandboxed child, and reports back — and delete `zombied`'s datastore-welded direct worker path + executor sidecar, so execution runs on hosts that hold no datastore credentials.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`. Mismatch with the Intent → STOP and reconcile. The four confirmed anchors below are NOT re-litigable without an Indy ack.
- **Confirmed anchors (Indy, May 25, 2026):** (1) separate `zombie-runner` daemon over HTTP, not in-process; (2) multi-runner — real assignment + fencing verification + expiry-reclaim across multiple runner hosts; (3) trusted-fleet **inline** secrets (scoped/proxy deferred); (4) one PR through the flag-flip, absorbing roadmap S1–S4.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NLR** (touch-it-fix-it: the direct path is *deleted*, not left as dead code), **NDC** (no dead code at write time; the runner loop must be reachable + tested before the direct path is removed — sequencing matters), **NLG** (pre-2.0: no `legacy_*`/`V2` framing for the retained-then-removed path; removed routes 404 not 410), **UFS** (the assignment/fencing constants, sandbox-tier values, retry/backoff knobs single-sourced; cross-runtime constants shared verbatim), **VLT** (`secrets_map` resolved just-in-time, inline over TLS, never logged, never runner-cached), **ERH** (errors via registry; new `UZ-RUN-*` for fencing-reject/lease-not-found), **MIG/SCM** (any `fleet` affinity/sequence schema is append-only + single-concern), **ORP** (orphan sweep after the direct-path deletion), **TST/TST-NAM** (no milestone IDs in test/code bodies).
- **`docs/ZIG_RULES.md`** — pg-drain lifecycle (assignment queries), tagged-union results, multi-step `errdefer` (the runner parent↔child supervision + sandbox fork), cross-compile both Linux targets (the runner is Linux-first for Landlock), file-as-struct for new single-type files, PUB surface discipline.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the `activity` verb (`POST /v1/runners/me/leases/{lease_id}/activity`), §7 route registration, §8 `Hx` interface, error envelopes for fencing rejects.
- **`docs/SCHEMA_CONVENTIONS.md`** — any new `fleet.*` table (affinity / fencing sequence) in the `fleet` schema; `embed.zig` + migration array; pre-v2.0 CREATE-not-ALTER.
- **`docs/AUTH.md`** — `runnerBearer` is the machine principal; TLS transport hardening (§5); revocation (`status='revoked'` → 401) on the per-call gate.
- **`docs/LOGGING_STANDARD.md`** / **`docs/LIFECYCLE_PATTERNS.md`** — new log emits across the runner loop + sandbox; init/deinit + errdefer on the runner's child-supervisor + sandbox handles; no secret/token bytes in logs.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | new `*.zig` (runner loop, child supervisor, sandbox glue); cross-compile x86_64+aarch64-linux; read ZIG_RULES before EXECUTE. |
| PUB / Struct-Shape | yes | own shape verdict per new file; file-as-struct for single-type modules; no inheritance. |
| File & Function Length (≤350/≤50/≤70) | yes | the engine migration is mostly `git mv` (no net new lines); new runner orchestration split across files; one concern per file. |
| UFS | yes | assignment/fencing/backoff constants, sandbox-tier values single-sourced; cross-runtime shared verbatim. |
| LOGGING | yes | runner-loop + sandbox emits logfmt; `secrets_map` + `runner_token` never logged. |
| LIFECYCLE | yes | runner child-supervisor + sandbox handles get init/deinit + errdefer; the fork/pipe lifecycle is the highest-risk surface. |
| ERROR REGISTRY | yes | declare new `UZ-RUN-*` (fencing reject, lease not found, sandbox failure) before use. |
| SCHEMA GUARD | yes | `fleet` affinity/fencing-sequence migration (if needed); single-concern ≤100 lines; `embed.zig` + array. |
| MILESTONE-ID | yes | code/test names carry no `M80`/§/dim IDs (RULE TST-NAM). |
| Architecture Consult & Update | yes | this workstream **edits** `runner_fleet.md` + `data_flow.md` + `capabilities.md` + `scaling.md` (§7); the doc and spec must land coherent in the same diff. |
| UI / DESIGN TOKEN | no | no UI surface. |

---

## Overview

**Goal (testable):** with the runner enabled by default, a steer event for any active zombie flows `register → lease → (forked sandboxed child runs NullClaw) → report` over HTTPS, writing the same `zombie_events`/`zombie_execution_telemetry`/`zombie_sessions` rows + `XACK` the direct worker writes today; a runner that dies mid-lease has its lease reclaimed after `lease_expires_at` and re-run by another runner, and the dead runner's late report is rejected by `fencing_token`; `zombied` holds no direct worker path and the `zombied-executor` sidecar binary is gone.

**Problem:** the worker is welded to Postgres + Redis (own pool + blocking Redis connection per zombie, ~15 hot-path writes, `XREADGROUP` self-discovery), so execution can only run where it reaches the datastores, and the Redis connection budget caps the zombie count by pool ceiling, not compute. Work cannot run on hosts the platform doesn't fully own.

**Solution summary:** realize the `runner_fleet.md` split as one cutover. `zombied` gains real multi-zombie **assignment** + **fencing verification** + **expiry-reclaim** on the lease/report path (S1). The `zombie-runner` daemon gains the parent control loop + the migrated NullClaw engine running in a **forked sandboxed child** per event (S3). `zombied`'s direct worker path + the executor sidecar transport are **deleted** (S2), the M80_001 loopback throwaway is removed, and the runner becomes the default processor (cutover). Trusted-fleet inline secrets only; multi-runner correctness rests on `lease_expires_at` + `fencing_token`, not on Redis consumer-idle.

---

## Prior-Art / Reference Implementations

- **Control-plane handlers** → `src/runner/service.zig` + `service_report.zig` (M80_001) — the lease/report orchestration this extends.
- **Direct path semantics to reproduce** → `src/zombie/event_loop_writepath.zig` (the 13-step write path) + `metering.zig` (two-debit billing) — the runner's lease/report must produce identical rows; `data_flow.md` §C is the prose contract.
- **Executor engine + sandbox** → `src/executor/**` (the RPC `createExecution → startStage → getUsage → destroyExecution` + TOCTOU guards) becomes parent↔child supervision inside the runner; the sandbox tiers (`landlock`, `cgroup`, `network`) migrate as-is.
- **Runner build graph** → `build_runner.zig` (M80_001) — the runner links no `pg`/`httpz`/`redis`; the engine + `protocol`/`event_envelope`/`execution_policy` modules are shared by source.
- **HTTP client** → `src/runner/loopback_client.zig` (M80_001) generalizes from loopback to the real control-plane client (remote base URL, TLS).

---

## Files Changed (blast radius)

| File / area | Action | Why |
|------|--------|-----|
| `src/runner/service.zig` | EDIT | assignment across all active zombies (replace the single-zombie `[0]` pick) with sticky-routing preference; monotonic per-zombie `fencing_token` issuance; `lease_expires_at` enforcement |
| `src/runner/service_report.zig` | EDIT | **verify** `fencing_token` (reject a stale/reclaimed holder); reclaim-aware finalize |
| `src/runner/reclaim.zig` (+ splits) | CREATE | expiry sweep: leases past `lease_expires_at` become re-leasable; the durable replacement for `XAUTOCLAIM` |
| `schema/023_fleet_runner_affinity.sql` (+ fencing sequence) | CREATE | sticky-routing affinity (`last_runner_id` per zombie) + per-zombie monotonic fencing source, in the `fleet` schema; `embed.zig` + array |
| `src/runner/main.zig` | EDIT | the real parent loop: register → heartbeat → lease → fork sandboxed child → report → activity; replaces the skeleton |
| `src/runner/child_supervisor.zig`, `src/runner/sandbox/*.zig` | CREATE | fork-per-event + pipe supervision; Landlock + cgroups + netns (Linux), Seatbelt/dev (macOS) — the `zombied-executor` TOCTOU guards relocated |
| `src/executor/{runner,handler,tool_bridge,tool_builders,session,runner_helpers,zombie_memory,runner_observer,runner_progress,progress_callbacks,progress_writer}.zig`, `src/executor/runtime/**`, `src/executor/context_budget.zig`, sandbox tiers | MOVE | the NullClaw engine relocated into the runner build graph (`git mv` where possible to keep the diff a move, not a rewrite) |
| `src/executor/main.zig`, `src/executor/transport.zig` | DELETE | the sidecar process + Unix-socket transport — gone once the engine runs in-process in the runner |
| `src/cmd/worker.zig`, `worker_watcher.zig`, `worker_zombie.zig`, `src/zombie/event_loop*.zig` | DELETE/THIN | strip the direct per-zombie worker path; retire the `zombie:control` stream consumer (per `runner_fleet.md` Redis-topology table) |
| `src/runner/loopback.zig`, `loopback_client.zig` (loopback role), `src/cmd/worker_config.zig` (`LoopbackConfig` + `ZOMBIE_RUNNER_SEAM`) | DELETE/REPURPOSE | the M80_001 loopback throwaway removed; `loopback_client` generalizes into the runner's real control-plane client |
| `src/http/handlers/runner/activity.zig` + route | CREATE | the `activity` verb (write-only progress → `zombied` `PUBLISH` to `zombie:{id}:activity`) |
| TLS transport (`src/runner` client + `zombied` serve) | EDIT | HTTPS for the control plane (secrets travel inline over TLS) |
| `src/errors/error_registry.zig`, `error_entries.zig` | EDIT | new `UZ-RUN-*` (fencing reject, lease not found, sandbox failure) |
| `docs/architecture/{runner_fleet,data_flow,capabilities,scaling,README}.md` | EDIT | §7 reconciliation — roadmap collapse + the post-cutover runtime |
| `docs/v2/active/M80_001_*.md` → `docs/v2/done/` | MOVE/AMEND | rescope to the durable keystone; mark loopback §3.4/§4 superseded |
| build targets (`build.zig` sidecar targets; `build_runner.zig`) | EDIT | drop `zombied-executor`/`-harness`/`-stub`; the runner build gains the engine + sandbox |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one collapsed cutover absorbing roadmap S1–S4, shipped as ordered Section-commits in a single PR, with the engine relocation done as `git mv` to keep the diff reviewable. Chosen over the staged keystone-then-fanout because there are no parallel teams to serve, so the keystone's early-validation payoff did not justify the throwaway loopback scaffolding (Indy, May 25, 2026).
- **Alternatives considered:** keystone + 4 sequential PRs (rejected by Indy — too much throwaway, slow); in-process runner first then re-split (rejected — re-introduces the throwaway it's avoiding); zero-trust secrets now (rejected — out of scope; trusted-fleet inline is the cutover target, scoped/proxy later).
- **Patch-vs-refactor verdict:** a **migration + deletion**, not a patch. The engine moves (not rewrites); the direct path is deleted (NLR), not shimmed. The risk surface is the sandbox fork/pipe lifecycle + assignment/fencing correctness — both get dedicated Sections and negative tests.

---

## Sections (implementation slices)

### §1 — Assignment, fencing & reclaim (zombied control plane)

The lease/report path becomes correct for many zombies and many runners.

- **Dimension 1.1** — `lease` assigns the next event across **all** active zombies (sticky-routing preference for the zombie's `last_runner_id`, falling back to any eligible runner), not the single-zombie pick → Test `test_lease_assigns_across_active_zombies`.
- **Dimension 1.2** — `fencing_token` is monotonic per-zombie (issued from a durable source), recorded on the lease, and **verified** at `report`: a token older than the zombie's current lease is rejected → Test `test_report_rejects_stale_fencing_token`.
- **Dimension 1.3** — a lease past `lease_expires_at` is re-leasable; the reclaim sweep re-issues it to another runner with a fresh higher token → Test `test_expired_lease_reclaimed_and_refenced`.
- **Dimension 1.4** — sticky routing is a *hint*: if the preferred runner is unavailable, any eligible runner is assigned; correctness never blocks on one runner → Test `test_sticky_routing_is_hint_not_ownership`.

### §2 — The `zombie-runner` daemon: parent loop + sandboxed child + NullClaw fold-in

The runner becomes a real processor.

- **Dimension 2.1** — the parent loop runs `register → heartbeat → lease → execute → report → activity`, holding zero datastore handles → Test `test_runner_parent_loop_no_datastore_handles`.
- **Dimension 2.2** — each lease forks a sandboxed child (Landlock + cgroups + netns on Linux); the child runs NullClaw from the leased `ExecutionPolicy` and returns content + tokens + timing over the pipe → Test `test_runner_forks_sandboxed_child_runs_nullclaw`.
- **Dimension 2.3** — the executor RPC TOCTOU guards (lease re-check, orphan reaping, idempotent destroy) are preserved as parent↔child supervision → Test `test_child_supervision_reaps_orphan_and_destroys_idempotently`.
- **Dimension 2.4** — `activity` frames forwarded to `zombied` reach the SSE tail unchanged → Test `test_activity_frames_reach_sse_tail`.

### §3 — Thin zombied: strip the direct worker path + sidecar + loopback throwaway

- **Dimension 3.1** — the direct per-zombie worker path + the `zombie:control` consumer are removed; `zombied worker` no longer spawns per-zombie threads or the executor sidecar → Test `test_direct_worker_path_removed` (the worker entrypoint is gone / inert).
- **Dimension 3.2** — `src/executor/main.zig` + `transport.zig` + the sidecar build targets are deleted; no Unix-socket transport remains → Verified by build (sidecar targets absent) + orphan sweep.
- **Dimension 3.3** — the M80_001 loopback throwaway (`loopback.zig`, `LoopbackConfig`, `ZOMBIE_RUNNER_SEAM`) is removed; `loopback_client` is generalized into the real control-plane client → Test `test_no_loopback_seam_symbols` (grep gate / compile).

### §4 — Steer / kill / pause via heartbeat-carried revocation

- **Dimension 4.1** — `kill` marks the in-flight lease revoked; the runner sees it in the next `heartbeat` reply, kills the child, reports `cancelled`; a late report is fenced out → Test `test_kill_revokes_lease_via_heartbeat`.
- **Dimension 4.2** — `pause` stops lease issuance for the zombie; an in-flight lease runs to completion → Test `test_pause_stops_new_leases`.

### §5 — TLS transport for the control plane

- **Dimension 5.1** — the runner↔zombied control protocol runs over HTTPS; `secrets_map` travels inline over TLS, never in plaintext logs → Test `test_control_plane_requires_tls` + the no-secret-in-logs grep.

### §6 — Cutover: runner is the default

- **Dimension 6.1** — execution defaults to the runner path; the direct path is not selectable (the flag is deleted, not flipped) → Test `test_runner_is_default_processor`.

### §7 — Documentation & spec reconciliation

- **Dimension 7.1** — `runner_fleet.md` roadmap collapses S1–S4 into this cutover; `data_flow.md` / `capabilities.md` / `scaling.md` describe the post-cutover runtime (direct path gone) → Verified by the Architecture Consult & Update Gate (doc + spec coherent in the diff).
- **Dimension 7.2** — M80_001 rescoped to its durable keystone, loopback §3.4/§4 marked superseded, moved to `done/` → Verified by the spec-template audit + the M80_001 diff.

---

## Interfaces

> The wire contract is frozen by M80_001 (`src/runner/protocol.zig`) and is NOT changed here — this workstream implements the *logic* behind it. New surface is additive (the `activity` verb + new error codes).

```
POST /v1/runners/me/leases     — assignment now spans all active zombies; fencing_token monotonic
                                  per-zombie; lease_expires_at enforced; sticky-routing hint applied
POST /v1/runners/me/reports    — fencing_token VERIFIED (stale/reclaimed holder → reject); else finalize
POST /v1/runners/me/leases/{lease_id}/activity   (NEW; Bearer zrn_)
  request:  progress frames (tool_call_started | agent_response_chunk | tool_call_completed | …)
  response: 202 best-effort (no ack); zombied PUBLISHes to zombie:{id}:activity
POST /v1/runners/me/heartbeats — reply carries status (ok|drain|stop) AND revoked lease IDs

errors (new): UZ-RUN-005 stale_fencing_token (report rejected) · UZ-RUN-006 lease_not_found
              · UZ-RUN-007 sandbox_setup_failed (runner-side; reported as agent_error)
reclaim    : lease_expires_at (epoch ms) + monotonic fencing_token replace XAUTOCLAIM consumer-idle
secret_delivery : inline only (trusted fleet, over TLS); scoped/proxy remain reserved
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runner dies mid-lease | host crash / network partition | lease expires at `lease_expires_at`; reclaim sweep re-leases to another runner with a higher fencing_token; no event loss → `test_expired_lease_reclaimed_and_refenced` |
| Stale report after reclaim | slow runner reports after reclaim | `report` rejects on `fencing_token` mismatch (`UZ-RUN-005`); the valid holder's result wins → `test_report_rejects_stale_fencing_token` |
| Sandbox setup fails | Landlock/cgroup/netns error on host | child not started; runner reports `agent_error` (`UZ-RUN-007`); lease redeliverable; no datastore corruption → `test_sandbox_setup_failure_reports_agent_error` |
| Kill mid-execution | operator kills the zombie | lease marked revoked; runner sees it next heartbeat, kills child, reports `cancelled`; late report fenced → `test_kill_revokes_lease_via_heartbeat` |
| Two runners race the same zombie | sticky miss + concurrent lease | only one lease is active per zombie (fencing monotonic); the loser gets no lease or is fenced at report → `test_sticky_routing_is_hint_not_ownership` |
| Config changed between stages | operator PATCH mid-chain | next lease resolves config fresh from Postgres; in-flight stage unaffected → `test_config_resolved_fresh_per_lease` |
| Control plane unreachable | zombied down / TLS failure | runner retries with backoff; un-acked lease redelivers; no event loss → `test_runner_retries_on_control_plane_unreachable` |

---

## Invariants

1. **Runner holds zero datastore credentials** — `zombie-runner`'s build graph (`build_runner.zig`) links no `pg`/`httpz`/`redis`; enforced by the build + a grep gate. (Dimension 2.1)
2. **Row-equivalence with the deleted direct path** — lease+report produce the same `zombie_events`/telemetry/`zombie_sessions` rows + `XACK` the direct path produced; asserted by a fixture replayed through both in the migration window, then by the runner path alone. (§1/§2 tests)
3. **Reclaim is lease-layer, not Redis-consumer** — a dead runner is reclaimed via `lease_expires_at` + `fencing_token`, never `XAUTOCLAIM` (which can't see an off-platform processor); enforced by the reclaim sweep + `test_expired_lease_reclaimed_and_refenced`. (Dimension 1.3)
4. **Fencing rejects stale writers** — `report` verifies `fencing_token`; a reclaimed holder's report cannot mutate state. Code-enforced at the handler, negative-tested. (Dimension 1.2)
5. **Secrets inline over TLS, never logged, never runner-cached** — `secrets_map` rides the lease over HTTPS, is used at the tool bridge, and is never written to logs or runner-local storage; LOGGING gate + grep + no-cache assertion. (Dimension 5.1)
6. **No dead code at cutover** — the direct path is deleted (NLR), not shimmed; the runner path is reachable + tested before the direct path is removed (sequencing); orphan sweep clean. (§3)
7. **Sandbox is mandatory per event** — every lease runs in a forked, isolated child; a sandbox setup failure fails closed (no un-sandboxed execution). (Dimension 2.2 / sandbox failure mode)

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (inputs → expected) |
|-----------|------|------|------------------------------|
| 1.1 | integration | `test_lease_assigns_across_active_zombies` | N active zombies with pending events → successive leases cover them; sticky preference honored when available |
| 1.2 | integration | `test_report_rejects_stale_fencing_token` | report with a token < current → rejected `UZ-RUN-005`; state unchanged |
| 1.3 | integration | `test_expired_lease_reclaimed_and_refenced` | lease past expiry → re-leasable; new lease carries a higher token; old holder fenced |
| 1.4 | integration | `test_sticky_routing_is_hint_not_ownership` | preferred runner unavailable → any eligible runner leased; no stall |
| 2.1 | unit | `test_runner_parent_loop_no_datastore_handles` | runner build links no pg/redis; loop drives the protocol only |
| 2.2 | e2e | `test_runner_forks_sandboxed_child_runs_nullclaw` | one steer → forked sandboxed child runs NullClaw from leased policy → report; rows equal the (pre-deletion) direct path |
| 2.3 | integration | `test_child_supervision_reaps_orphan_and_destroys_idempotently` | orphaned child reaped; double-destroy is a no-op |
| 2.4 | integration | `test_activity_frames_reach_sse_tail` | activity frames → `PUBLISH` → SSE consumer sees ordered frames |
| 3.1 | integration | `test_direct_worker_path_removed` | `zombied worker` spawns no per-zombie thread / no sidecar |
| 3.3 | unit | `test_no_loopback_seam_symbols` | no `loopback.zig`/`ZOMBIE_RUNNER_SEAM`/`LoopbackConfig` symbols remain |
| 4.1 | integration | `test_kill_revokes_lease_via_heartbeat` | kill → heartbeat carries revocation → child killed → `cancelled`; late report fenced |
| 4.2 | integration | `test_pause_stops_new_leases` | pause → no new lease issued; in-flight runs to completion |
| 5.1 | integration | `test_control_plane_requires_tls` | non-TLS control call refused; secrets absent from logs |
| 6.1 | e2e | `test_runner_is_default_processor` | default config → events processed by the runner path end-to-end |
| 7.x | n/a | (doc gate) | Architecture Consult & Update Gate: docs reconciled in the same diff |

**Regression:** Invariant 2 (row-equivalence) is the guard against behavioral drift from the deleted direct path; the e2e (2.2/6.1) is the keystone-grade proof, now against the *real* runner. Non-self-evident payloads → `samples/fixtures/m80-fixtures/`.

---

## Acceptance Criteria

- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (assignment + fencing + reclaim + activity, against PG + Redis)
- [ ] `make memleak` clean (runner child-supervisor + sandbox lifecycle is the highest-risk allocator surface)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` (zombied + runner)
- [ ] `zig build --build-file build_runner.zig` produces the runner with the engine folded in and **no** pg/httpz/redis linkage
- [ ] `test_runner_is_default_processor` + `test_expired_lease_reclaimed_and_refenced` + `test_report_rejects_stale_fencing_token` green
- [ ] Direct worker path + sidecar transport deleted; orphan sweep clean; `gitleaks detect` clean; no file over 350 lines added
- [ ] `docs/architecture/{runner_fleet,data_flow,capabilities,scaling}.md` reconciled; M80_001 in `done/`
- [ ] `bash scripts/audit-spec-template.sh` clean

---

## Eval Commands (post-implementation)

```bash
# E1: runner is default + processes events end-to-end
make test-integration 2>&1 | grep -E "runner_is_default|forks_sandboxed_child|PASS|FAIL"
# E2: reclaim + fencing
make test-integration 2>&1 | grep -E "reclaimed_and_refenced|rejects_stale_fencing|PASS|FAIL"
# E3: builds — zombied + runner (no datastore linkage in runner)
zig build && zig build --build-file build_runner.zig 2>&1 | tail -3
# E4: direct path + sidecar gone
git grep -nE "ZOMBIE_RUNNER_SEAM|LoopbackConfig|src/executor/transport" -- src | head
# E5: cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: no secret/token in logs
grep -rnE "secrets_map|runner_token" src/runner | grep -iE "log\.|print" | head
# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

The direct worker path (`worker_zombie` direct loop, `event_loop*` worker entry), the `zombie-executor` sidecar (`src/executor/main.zig`, `transport.zig`), the `zombie:control` consumer, and the M80_001 loopback throwaway are **deleted** in §3 (RULE NLR — removed, not shimmed). Removal is sequenced AFTER the runner path is reachable + tested (NDC). Orphan sweep (RULE ORP) runs after deletion; the engine modules are *moved* (still consumed by the runner), not orphaned.

---

## Discovery (consult log)

> Append consults, skill-chain outcomes, and Indy-acked deferral/scope quotes as work proceeds.

- **Scope pivot (Indy, May 25, 2026):** abandon the throwaway loopback skeleton; build the real cutover and ship one PR through the flag-flip. Verbatim: *"I think i want to keep building till the 80_003 since it is not working building this way since we are building throw away code"* + *"and send that in 1 PR till 80_003."* Confirmed anchors via handshake: (1) separate daemon, (2) multi-runner + scheduler-grade assignment/fencing/reclaim, (3) trusted-fleet inline secrets.
- **M80_001 loopback superseded (Indy, May 25, 2026):** *AskUserQuestion → "Handler tests only, skip e2e"* then the cutover pivot — the loopback e2e (M80_001 §3.4) + flag-parity (§4) are deleted, not completed. M80_001 rescopes to its durable keystone and moves to `done/`.
- **Numbering reconciliation:** "till M80_003" colloquially means the full cutover. The `runner_fleet.md` roadmap splits it across S1 (M80_002 assignment/fencing), S2 (M80_003 thin-worker), S3 (M80_004 runner+NullClaw), S4 (M80_005 TLS) — but S2 (strip the direct path) cannot precede S3 (the runner that replaces it exists). This workstream **absorbs S1–S4** into one PR; M80_006 (fleet plane) / M80_007 (placement scheduler) / mode-C remain future. `runner_fleet.md` roadmap reconciled in §7.
- **Out-of-scope confirmed:** scoped/proxy secret delivery (zero-trust untrusted hosts), the operator fleet-plane (`GET /v1/fleet/*`), label/capacity placement scheduler, autoscale, warm execution, self-enrolling mode-C — all post-cutover.
- **Git flow — RESOLVED to Option A (Indy, May 25, 2026):** ONE Pull Request continuing `feat/m80-001-runner-contract-keystone`; the spec rides in the PR (a justified deviation from the spec-on-main convention). No fresh `feat/m80-002-*` branch and no rename — branching off `main` would lose the large uncommitted durable keystone set this branch carries. M80_001's durable keystone is committed on this branch and moved to `done/`; this CHORE(open) activates the cutover spec here.
- **CHORE(open) (May 25, 2026):** M80_001's loopback throwaway removed and durable keystone committed (runner auth plane + real lease/report handlers + `fleet.runner_leases` schema); M80_001 rescoped to `done/`. This spec moves `pending/` → `active/`, `Status: IN_PROGRESS`. Implementation begins at §1 (assignment/fencing/reclaim); the §2 engine migration walks `src/executor/runtime/**` + sandbox tiers at PLAN before any move.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments addressed before human review/merge. |
| After every push | `kishore-babysit-prs` | greptile polled, walked, triaged, fixed, reported. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-zombied` | — | ⏳ |
| Integration | `make test-integration` | — | ⏳ |
| e2e (runner default) | `make test-integration` (e2e) | — | ⏳ |
| Lint | `make lint` | — | ⏳ |
| Cross-compile | `zig build -Dtarget={x86_64,aarch64}-linux` (both binaries) | — | ⏳ |
| Memleak | `make memleak` | — | ⏳ |
| Runner build (no datastore linkage) | `zig build --build-file build_runner.zig` | — | ⏳ |
| Gitleaks | `gitleaks detect` | — | ⏳ |

---

## Out of Scope

- **M80_006 — fleet plane:** node inventory, operator `GET /v1/fleet/runners`, revoke via `PATCH`, proactive lease reassignment on heartbeat-detected death (this workstream does expiry-reclaim, not heartbeat-driven reassignment).
- **M80_007 — placement scheduler:** label/capacity/sandbox-tier placement, autoscale by queue depth. This workstream does basic assignment (sticky + any-eligible), not a scheduler.
- **Zero-trust secrets:** `scoped`/`proxy` `secret_delivery`; per-tenant `allowed_workspace_ids` / `trust_class` authz. Trusted-fleet inline only here.
- **Warm execution:** the sandbox-shell-reuse optimization; cold fork-per-event only.
- **mode C:** self-enrolling open-fleet runners.
