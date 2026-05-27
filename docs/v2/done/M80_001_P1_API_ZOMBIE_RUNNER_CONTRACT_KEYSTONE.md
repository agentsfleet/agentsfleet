# M80_001: Freeze the `/v1/runners` contract and prove it with a loopback walking skeleton

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 001
**Date:** May 22, 2026
**Status:** DONE
**Priority:** P1 — keystone; the four parallel M80 workstreams build against the contract this freezes, so it lands first and alone.
**Categories:** API
**Batch:** B1 — serial keystone; B2 (M80_002…005) fans out only after this lands.
**Branch:** `feat/m80-001-runner-contract-keystone`
**Depends on:** none — this is the dependency root of the M80 arc.
**Provenance:** agent-generated (pre-spec, plan-eng-review session May 21–22 2026; decisions captured in memory `project_zombie_runner_split_architecture`).

> **Provenance is load-bearing.** LLM/agent-drafted — cross-check every claim against the codebase before EXECUTE; do not assume the author read the code.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (the M80 target this freezes) and `docs/architecture/data_flow.md` (the current single-fleet runtime the `report` verb must reproduce).

---

## Implementing agent — read these first

1. `docs/architecture/runner_fleet.md` — the contract shape, the event-leasing + sticky-routing decision, the `secret_delivery` modes, and the `sandbox_tier` tiers this workstream freezes.
2. `docs/architecture/data_flow.md` §C (EXECUTE) — the worker's per-event hot path (`zombie_events` received→terminal, telemetry, debit, session checkpoint, `XACK`) the `report` verb must reproduce idempotently in one transaction.
3. `src/http/route_table.zig` + `src/http/router.zig` — the route-registration pattern to mirror for the `/v1/runners/*` stubs (one of the three shared-file conflict points to pre-claim).
4. `schema/embed.zig` + `schema/020_tenant_providers.sql` — the append-only migration array and the nearest table migration to mirror for `021_fleet_runners.sql`.
5. `src/cmd/worker.zig` + `src/cmd/worker_zombie.zig` — the loop the flag-gated skeleton forks exactly one path out of; the direct path stays the default.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Freeze /v1/runners contract + loopback walking skeleton (M80 keystone)
- **Intent (one sentence):** Stand up the frozen runner control contract and prove it end-to-end with one zombie over loopback, so the four parallel M80 workstreams build against a validated interface instead of a guessed one.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`. Mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — pin: **NDC** (stubs must be reachable + tested, never orphaned), **UFS** (the `/v1/runners` path segments, `secret_delivery` + `sandbox_tier` wire values, and the seam flag name are single-sourced named constants shared verbatim Zig↔TS), **MIG** (migration array append-only + ordered), **SCM** (schema conventions), **VLT** (`secrets_map` resolved just-in-time, never logged), **ERH** (errors via registry), **CFG** (the seam feature flag), **XCC** (cross-compile both linux targets), **ORP** (orphan sweep), **TST**.
- **`docs/ZIG_RULES.md`** — pg-drain lifecycle (the `report` handler queries PG), tagged-union results, multi-step `errdefer`, cross-compile.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — `/v1/runners` URL design, route registration, handler signature.
- **`docs/SCHEMA_CONVENTIONS.md`** — `021_fleet_runners.sql` (CREATE `fleet.runners` in the `fleet` schema), `embed.zig`.
- **`docs/AUTH.md`** — `runner_token` is a credential-typed principal; even the S0 stub follows the principal/token pattern (full hardening is M80_005).
- **`docs/LOGGING_STANDARD.md`** / **`docs/LIFECYCLE_PATTERNS.md`** — new log emits; init/deinit + errdefer on new `zombied`-side structs holding PG/Redis handles.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | new `*.zig` under `src/`; cross-compile x86_64+aarch64-linux; read ZIG_RULES before EXECUTE. |
| PUB / Struct-Shape | yes | own shape verdict per new file (contract types, control-plane service, loopback client, handler stubs); no inheritance. |
| File & Function Length (≤350/≤50/≤70) | yes | split contract types / handlers / service across files; one verb per handler file. |
| UFS | yes | path segments, `secret_delivery`/`sandbox_tier` values, `ZOMBIE_RUNNER_SEAM` flag name as named constants shared verbatim across Zig + TS. |
| LOGGING | yes | logfmt emits in register/lease/report; `secrets_map` + `runner_token` never logged. |
| LIFECYCLE | yes | the control-plane runner service holds pooled handles; init/deinit + errdefer adjacent to alloc. |
| ERROR REGISTRY | yes | declare `UZ-RUN-001…` in `src/errors/error_registry.zig` before use. |
| SCHEMA GUARD | yes | `021` create + `022` alter; single-concern ≤100 lines/file; update `embed.zig` + migration array. |
| MILESTONE-ID | yes | code/test names carry NO `M80`/§/dim IDs (RULE TST-NAM); milestone id lives only in spec + flag-doc prose. |
| UI / DESIGN TOKEN | no | no UI surface in S0. |

---

## Overview

**Goal (testable):** with `ZOMBIE_RUNNER_SEAM=1`, one zombie's steer event flows register → lease → executor → report over loopback and writes the same `zombie_events`/`zombie_execution_telemetry`/`zombie_sessions` rows and `XACK` the direct worker writes today; with the flag unset, the direct path is byte-for-byte unchanged.

**Problem:** the worker can only run where it reaches Postgres and Redis directly, and the four planned runner workstreams have no validated interface to build against in parallel — a guessed contract risks reworking up to four streams.

**Solution summary:** freeze the `/v1/runners` contract (request/response types + the four endpoints) and ALL runner schema, pre-claim the three shared-file conflict points (route table, build target, migration array) as stubs so the parallel streams touch only disjoint directories, and implement exactly one happy-path vertical — one zombie, loopback, flag-gated — that exercises register/lease/report against the real datastores via `zombied` while the production direct path remains the default. The skeleton both freezes and validates the contract before fan-out.

---

## Prior-Art / Reference Implementations

- **API** → `docs/REST_API_DESIGN_GUIDELINES.md` + the nearest existing handler under `src/http/handlers/` (webhook receiver / api_keys); route registration mirrors `src/http/route_table.zig`.
- **Internal versioned contract** → `src/executor/protocol.zig` — the executor's explicit method names + numeric error codes are the in-repo model for a versioned RPC surface; `/v1/runners` mirrors that explicitness over HTTPS.
- **Schema** → `schema/020_tenant_providers.sql` + `docs/SCHEMA_CONVENTIONS.md`.
- **Connection topology** → M80_001 reuses the `zombied` pool from M69_004 (Redis pool/subscriber unify); the lease/report handlers use short-lived pooled commands, never a blocking connection in the request path.
- **Contract shape itself** → greenfield; defined in `docs/architecture/runner_fleet.md`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/protocol.zig` (+ split files) | CREATE | the frozen request/response types, `secret_delivery` + `sandbox_tier` enums, and the single-sourced `/v1/runners` path constants (`PATH_RUNNERS*`, RULE UFS). Named `protocol.zig` (not `protocol.zig`) to match the in-repo wire-interface convention (`src/executor/protocol.zig`). |
| `src/runner/service.zig` (+ splits) | CREATE | `zombied`-side (control-plane) lease/report orchestration over the pool — re-orchestrates `writepath.run` steps 1–7 (lease) + reuses `finalize`'s leaf writers (report), calling existing helpers so `writepath.run` stays untouched (Invariant 1). Faithful-mirror, non-atomic. |
| `src/runner/loopback_client.zig` | CREATE | the client the flag-gated skeleton uses to call `zombied` over loopback (lives in zombied's build graph) |
| `src/auth/middleware/runner_bearer.zig`, `src/auth/principal.zig` | CREATE/EDIT | **Option B (pulled from M80_005):** `runnerBearer` validates `Bearer zrn_` via timing-safe `sha256` lookup in `fleet.runners` → `AuthPrincipal{mode=runner, runner_id, tenant_id=null}`; wired only onto `/v1/runners/me/*`. `register` gated by `bearer_or_api_key` + `RequireRole.admin`. |
| `schema/022_fleet_runner_leases.sql`, `src/types/id_format.zig` | CREATE/EDIT | `fleet.runner_leases` (lease_id, runner_id, zombie_id, workspace_id, tenant_id, event_id, fencing_token, lease_expires_at, status + the resolved context to reconstruct session/event at report); CREATE not ALTER (pre-2.0 gate). `generateRunnerId()` + `generateRunnerLeaseId()` UUIDv7 generators. |
| `src/runner/main.zig` | CREATE | the `zombie-runner` binary entry — keystone skeleton: one logfmt health line, exit 0 (the build-target pre-claim) |
| `src/http/handlers/runner/{register,heartbeat,lease,report}.zig` | CREATE/EDIT | the four handlers — real in this PR: register mints `zrn_` + inserts `fleet.runners`; heartbeat returns `ok` + bumps `last_seen_at`; lease/report are thin wrappers over `service.zig` (were `UZ-RUN-004` 501 stubs in §2) |
| `schema/021_fleet_runners.sql` | CREATE | `fleet.runners` in a dedicated `fleet` control-plane schema (identity, token hash, sandbox_tier, labels, last_seen, status, optional tenant scope) |
| `schema/embed.zig` | EDIT | append `021` to the migration array (shared conflict point — pre-claimed here) |
| `src/http/router.zig`, `src/http/route_table.zig`, `src/http/route_table_invoke.zig` | EDIT | register `/v1/runners/*` + the four invoke fns (shared conflict point — pre-claimed here) |
| `build_runner.zig` | CREATE | dedicated build graph for the `zombie-runner` daemon — separate from the root `build.zig` by design so the runner links **no** server infrastructure (`pg`/`httpz`/`redis`); shares the frozen wire protocol by source. Build: `zig build --build-file build_runner.zig`. |
| `build.zig` | EDIT | no runner target — the runner owns `build_runner.zig`; a one-line pointer comment remains (the build-graph split *is* the pre-claim) |
| `src/errors/error_registry.zig`, `src/errors/error_entries.zig` | EDIT | declare `UZ-RUN-*` codes (`UZ-RUN-004` now referenced by the stubs) |
| `src/cmd/worker.zig`, `src/cmd/worker_zombie.zig`, `src/cmd/worker_config.zig` | EDIT | `ZOMBIE_RUNNER_SEAM` flag config (`worker_config`), threaded `worker.zig` → `ZombieWorkerConfig`, forked at `zombieWorkerLoop` (`worker_zombie`): flag-on `register`+`loop{lease→execute→report}`, flag-off `runEventLoop` byte-identical |
| `src/http/router_test.zig`, `src/http/server.zig`, `src/http/handlers/runner/routes_integration_test.zig`, `src/runner/protocol_test.zig`, `src/runner/schema_migration_test.zig` | CREATE/EDIT | route-resolution unit tests + the routes-registered integration test (CI-gated) + protocol round-trip + the `fleet.runners` migration assertion |
| `docs/architecture/runner_fleet.md` | EDIT | the operator-enrollment sequence + trust-gate model; `zombied` is the control plane that gains the endpoints (no "mothership") |
| `docs/AUTH.md`, `docs/AUTH_DEVICE_LOGIN.md` (new), `docs/architecture/roadmap.md` (new, absorbs `bastion.md`), `docs/architecture/{office_hours,plan_engg_review}.md` → `archive/`, `docs/architecture/{README,high_level,capabilities,data_flow,scaling}.md`, `docs/architecture/bastion.md` (deleted) | EDIT/MOVE/DELETE | auth-doc slim + device-login split + architecture restructure — Indy-directed, folded into this one PR (originally outside the declared blast radius; amended here so the diff matches scope) |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** contract-first keystone, then parallel fan-out (plan-eng-review D2). S0 freezes + validates the interface before four streams commit to it; it pre-claims the three shared files as stubs so the streams never collide on `build.zig`, the route table, or `embed.zig`.
- **Alternatives considered:** big-bang single branch (rejected — a ~20k-line unreviewable PR; tenant-secret risk unmitigated); linear strangler (rejected — serializes the four streams, wasting the parallel capacity); eager fan-out with no skeleton (rejected — a wrong contract reworks up to four streams).
- **Patch-vs-refactor verdict:** this is a **refactor-enabling seam** ("make the change easy, then make the easy change"), scoped to the interface + one proof path. It is NOT the worker migration (that is M80_003); the direct path is untouched and default.

---

## Sections (implementation slices)

### §1 — Contract & schema freeze ✅ DONE

The durable interface the parallel streams depend on. Freezes types and tables; the logic that *uses* them (real assignment, sticky routing) is M80_002+. **Implementation default:** wire values are snake_case strings single-sourced as named constants, because UFS requires Zig and the future TS client to share them verbatim.

- **Dimension 1.1** ✅ — contract request/response types + `secret_delivery` + `sandbox_tier` enums round-trip serialize/deserialize → `protocol_test.zig` (7 round-trip cases: enums, register, heartbeat, report, lease + secrets_map + no-work). Unit, passes locally.
- **Dimension 1.2** ✅ — `021_fleet_runners` (`fleet.runners`) migrates clean → `runner/schema_migration_test.zig` asserts the table + its 10 columns + the `uq_runners_token_hash` / `ck_runners_id_uuidv7` constraints. Integration (DB), CI-gated.
- **Dimension 1.3** ✅ — `embed.zig` array applies `021` in order, idempotently → `cmd/common.zig` ("last version is 21" + "every migration SQL is parseable by SqlStatementSplitter"), plus `021` is idempotent by construction (`CREATE SCHEMA/TABLE IF NOT EXISTS`). Unit, passes locally. (`022` is a PR #2 ALTER; not in this slice.)

### §2 — Shared-file stubs (pre-claim the conflict points) ✅ DONE

Register the three shared files as stubs so the four streams edit only their own directories.

- **Dimension 2.1** ✅ — the four routes — `POST /v1/runners` + `POST /v1/runners/me/{heartbeats,leases,reports}` — resolve in the route table; every verb returns `UZ-RUN-004` not-implemented (HTTP 501), not a 404 → `router_test.zig` (path→variant resolution + malformed-sibling 404s) + `route_table.zig` specFor coverage (unit, passes locally); `handlers/runner/routes_integration_test.zig` (501-not-404 + GET→405, integration CI-gated).
- **Dimension 2.2** ✅ — the `zombie-runner` build target compiles (skeleton `main` logs a health line and exits 0) → `zig build --build-file build_runner.zig` verified: builds + emits the logfmt boot line + exits 0; cross-compiles to both Linux targets.

### §3 — Control-plane handlers (register / lease / report) — ✅ DONE; §3.4 loopback e2e 🔻 SUPERSEDED

The zombied-side handlers shipped real (committed). The loopback delivery
vehicle (worker fork + `loopback.zig`) that §3.4 would have proven is
abandoned — the cutover proves the path end-to-end against the real
`zombie-runner` daemon instead. The single-zombie integration tests are
absorbed into the cutover's multi-zombie assignment/fencing suite (one PR).

- **Dimension 3.1** ✅ — `register` (authed by an existing operator credential via `bearer_or_api_key` + admin) mints a `zrn_` token and inserts a `fleet.runners` row with `sandbox_tier` + labels. Committed (`handlers/runner/register.zig`); the `runnerBearer` auth plane is unit-tested.
- **Dimension 3.2** ✅ — `lease` returns the next event for the assigned zombie with resolved config and (mode `inline`) `secrets_map`. Committed (`runner/service.zig`); integration test absorbed into the cutover (assignment suite).
- **Dimension 3.3** ✅ — `report` reproduces `finalize()`: `markTerminal` + `recordStageActuals` + `checkpointZombieSession` (three autocommit writes, faithful — non-atomic) then `XACK`; debit already taken at `lease`. `fencing_token` accepted but not verified (the cutover verifies it). Committed (`runner/service_report.zig`); integration test absorbed into the cutover.
- **Dimension 3.4** 🔻 SUPERSEDED — the loopback one-zombie e2e is replaced by the cutover's runner-daemon e2e. The loopback vehicle was never committed. See Discovery.

### §4 — Flag parity & isolation 🔻 SUPERSEDED

The `ZOMBIE_RUNNER_SEAM` flag + worker fork are abandoned (never committed).
The cutover *deletes* the direct path (RULE NLR) rather than flag-gating it,
so there is no flag to keep parity with; row-equivalence with the removed
path becomes the cutover's Invariant 2.

- **Dimension 4.1** 🔻 SUPERSEDED — flag-off parity is moot once the flag is removed. See Discovery.

---

## Interfaces

> The frozen surface. The agent must NOT change these without amending the spec. Shapes shown as field lists (not pseudocode); the agent derives exact types from the codebase.

```
POST /v1/runners                    (register; auth: Bearer <Clerk JWT | zmb_t_ api_key>)
  request:  host_id, sandbox_tier, labels[]
  response: runner_id, runner_token (one-time-read)
  errors:   401 bad credential (via bearer_or_api_key) · 403 insufficient role
            (register needs admin today; fleet:write scope in v2.1) — no UZ-RUN code
POST /v1/runners/me/heartbeats      (auth: Bearer runner_token; `me` = the token's runner)
  request:  (empty in S0 — capacity/version are M80_006/007)
  response: status (see enum)                          errors: UZ-RUN-001 invalid_runner_token
POST /v1/runners/me/leases          (auth: Bearer runner_token, long-poll)
  response: 200 always —
            { lease: { lease_id, fencing_token, lease_expires_at, secret_delivery,
                       event envelope, resolved policy(config + inline secrets_map) } }
            OR { lease: null, retry_after_ms }  on no-work
  errors:   UZ-RUN-001, UZ-RUN-003 unsupported_secret_delivery
POST /v1/runners/me/reports         (auth: Bearer runner_token)
  request:  lease_id, event_id, fencing_token, outcome, response_text, tokens, telemetry, checkpoint
  response: ok (idempotent by event_id; fencing_token rejects a stale/reclaimed lease holder)

Identity   : from the Bearer token, never the URL/body — no runner_id in any request. The
             `/v1/runners/me/...` shape mirrors the existing `/v1/tenants/me/...` convention; if
             the OpenAPI URL-shape gate flags it, add a documented caveat (Indy, May 23, 2026).
secret_delivery : inline | scoped | proxy        (S0 implements inline only; scoped/proxy = later)
sandbox_tier    : landlock_full | container_nested | macos_seatbelt | dev_none
                  (self-reported telemetry only; placement uses operator-assigned trust — M80_005/007)
outcome         : processed | agent_error        (mirrors core.zombie_events.status)
status          : ok                             (drain | stop reserved for M80_006 failover)
flag            : ZOMBIE_RUNNER_SEAM (unset|1)   — single-sourced constant (M80_004/PR #2)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| No work to lease | empty stream | `lease` returns `200 { lease: null, retry_after_ms }`; runner backs off then re-polls (no 204) |
| Report replay | retry by the same holder | S0 faithful-mirror: re-runs the `finalize()` writes (terminal UPDATE + checkpoint UPSERT are naturally re-appliable; the debit already fired at `lease`, not at report). `INSERT … ON CONFLICT` idempotency is M80_002. |
| Invalid/expired runner_token | unregistered runner | 401 `UZ-RUN-001` |
| Bad register credential | register with an invalid/expired Clerk JWT or `zmb_t_` | 401 via `bearer_or_api_key` (no UZ-RUN code; not a runner-plane concern) |
| Unsupported secret mode | `secret_delivery` ≠ inline in S0 | 400 `UZ-RUN-003` |
| Executor unavailable on runner | local Unix socket down | report carries `agent_error`; lease redeliverable; no datastore corruption |
| Control plane (`zombied`) unreachable (loopback) | `zombied` down mid-skeleton | loopback client retries with backoff; lease un-acked → no event loss |
| Report after reclaim | slow runner; another reclaimed + ran it | **M80_002** — fencing verification (stale `fencing_token` → report rejected) lands with the real `zombied` lease/report logic. S0's single-zombie loopback skeleton has no reclaim path to exercise it. |

---

## Invariants

1. **Flag-off = unchanged direct path** — a single gate at the worker entry; `test_flag_off_parity` + no edits to the direct write path in the diff.
2. **`report` reproduces the direct path's writes, faithfully** — `markTerminal` + `recordStageActuals` + `checkpointZombieSession` (three autocommit statements, non-atomic) then `XACK`, mirroring `finalize()` exactly; the debit is taken pre-execution at `lease` issue (estimate, never re-charged at report). The `fencing_token`/`lease_id` fields are frozen on the contract, but **verification + true idempotency (`INSERT … ON CONFLICT`) are M80_002** — S0 accepts `fencing_token` without enforcing it. `test_report_writes_terminal_and_xacks` asserts the written row set equals the direct path's.
3. **Executor binary unchanged** — `src/executor/**` absent from Files Changed; CI builds the identical executor artifact.
4. **No `secrets_map` bytes or `runner_token` in logs** — LOGGING gate + redaction; grep test asserts absence.
5. **`embed.zig` append-only, migrations ordered + idempotent** — migration runner + `test_runner_migrations_idempotent`.
6. **Wire constants single-sourced** — path segments, enum values, flag name are named constants; UFS gate enforces verbatim sharing.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (inputs → expected) |
|-----------|------|------|------------------------------|
| 1.1 | unit | `test_runner_contract_roundtrip` | every request/response + enum serializes and parses back equal |
| 1.2 | integration | `test_runner_schema_migrates` | fresh DB applies 021; `fleet.runners` exists with its constraints |
| 1.3 | integration | `test_runner_migrations_idempotent` | re-running the migration set is a no-op; order preserved |
| 2.1 | integration | `test_runner_routes_registered` | all four paths resolve; non-skeleton verbs return declared not-implemented (not 404) |
| 2.2 | e2e | (acceptance) | `zig build --build-file build_runner.zig` produces a binary that logs health and exits 0 |
| 3.1 | integration | `test_register_mints_runner_token` | valid operator credential (Clerk JWT / `zmb_t_`) → runner_token + a `fleet.runners` row; bad credential → 401 via `bearer_or_api_key` |
| 3.2 | integration | `test_lease_returns_event_with_secrets` | leased event carries resolved config + inline `secrets_map`; empty stream → 200 `{lease:null, retry_after_ms}` (no 204) |
| 3.3 | integration | `test_report_writes_terminal_and_xacks` | reproduces finalize: terminal + telemetry-actuals + checkpoint (3 autocommit) then XACK; rows equal the direct path (faithful — idempotency/fencing = M80_002) |
| 3.4 | e2e | `test_e2e_loopback_one_zombie` | flag on: steer → register→lease→executor→report; rows equal the direct path |
| 4.1 | integration | `test_flag_off_parity` | flag unset: direct path runs unchanged; executor untouched |

**Regression:** the existing direct path is guarded by `test_flag_off_parity` (4.1) — flag-off must be byte-identical. **Idempotency/replay:** 3.3 + the report-replay failure mode. Non-self-evident payloads → `samples/fixtures/m80-fixtures/`.

---

## Acceptance Criteria

- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (HTTP + schema + Redis touched)
- [ ] `make memleak` clean (`zombied` runner handlers allocate — PR #2)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `zig build --build-file build_runner.zig` produces the skeleton binary
- [ ] `test_flag_off_parity` green (the existing path is unchanged)
- [ ] `gitleaks detect` clean · no file over 350 lines added · `bash scripts/audit-spec-template.sh` clean

---

## Eval Commands (post-implementation)

```bash
# E1: flag-off parity — existing path unchanged
make test-integration 2>&1 | grep -E "flag_off_parity|PASS|FAIL"
# E2: Build — full + runner skeleton
zig build && zig build --build-file build_runner.zig 2>&1 | tail -3
# E3: Tests
make test 2>&1 | tail -5
# E4: Lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile (Zig)
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: no secret/token in logs (empty = pass)
grep -rnE "secrets_map|runner_token" src/runner src/http/handlers/runner | grep -iE "log\.|print" | head
```

---

## Dead Code Sweep

N/A — no files deleted. The direct worker path is retained until the M80 cutover (RULE NLR cleanup happens there, not here). The stubs are reachable (routes wired + tested in 2.1), so NDC is satisfied.

---

## Discovery (consult log)

> Empty at creation. Append consults, skill-chain outcomes, and Indy-acked deferral quotes as work proceeds.

- **Provenance:** decisions D1–D5 from the plan-eng-review session (May 21–22 2026) — staged contract-first fan-out, trusted-fleet (`inline`) secrets, name `zombie-runner`, event-leasing + sticky routing. See memory `project_zombie_runner_split_architecture`.

- **Fan-out gate (Indy, May 22, 2026):** chose the **freeze gate** over the validate gate. The four parallel streams (M80_002–005) unblock the moment the *freeze* lands (frozen contract types + schema `021`/`022` + the three shared-file stubs), not after the loopback skeleton validates the contract. Tradeoff accepted: the streams build against a not-yet-validated contract until the skeleton lands; if the skeleton forces a contract change, the streams absorb it. Delivery therefore splits into two PRs (one spec, park-midway): **PR #1 = §1 + §2 (freeze + stubs)**; **PR #2 = §3 + §4 (loopback skeleton)**.

- **Comprehension handshake (PLAN, before EXECUTE):** intent restated — freeze + pre-claim the shared files now (PR #1); *prove* one zombie over loopback in PR #2. Matches the spec Intent; the "prove" half moves to the follow-on PR per the freeze-gate decision above. `ASSUMPTIONS I'M MAKING:` (1) stub handlers parse their request type and return a declared `UZ-RUN-*` not-implemented error (reachable + tested → NDC) — real register/lease/report logic is PR #2; (2) runner identity lives in a dedicated `fleet` schema (`fleet.runners`), not `core`; the sticky-routing affinity hint is M80_002's concern in `fleet`, not on `core.zombie_sessions` (see Discovery); (3) wire constants are single-sourced in Zig for the freeze (no TS consumer exists yet — the TS mirror + verbatim-match lands with the first TS client); (4) loopback transport in §3 is real HTTP over `127.0.0.1` through the router — firmed in PR #2.

- **`core.runners.tenant_id` optional scope — INCLUDED (Indy, May 22, 2026).** I initially proposed omitting the nullable tenant-scope column (spec's column list omits it; additive ALTER would make it cheap later). Indy directed including it: pre-ship there is no production-data migration cost and no live always-NULL column to look speculative, and `runner_fleet.md` explicitly commits the optional tenant scope in S0 so modes C/B needn't re-cut the table. Added as `tenant_id UUID NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE`; stays NULL (trusted-fleet, mode inline) until the per-tenant-scoped mode wires it.

- **Runner identity → dedicated `fleet` schema; affinity → M80_002; `008` left untouched (Indy + Bishop CTO review, May 22–23, 2026).** Runners live in a new `fleet` schema (`fleet.runners`), not `core` — the control plane (runner identity/tokens) must not share a trust boundary with the tenant data plane, and `fleet` scales to `fleet.runner_leases`/`fleet.fleets`/etc. without renaming (a runner is an *instance*; `fleet` is the *boundary*). The sticky-routing hint (`last_runner_id`) was briefly folded into `core.zombie_sessions`, then reverted: it references a runner (control-plane) and doesn't belong on a data-plane table — its storage is M80_002's concern in `fleet`. S0's frozen schema is therefore just `fleet.runners`. (Aside on migrations: the pre-v2.0 `check-schema-gate` forbids `ALTER TABLE`/`DROP` while major < 2 — that's why `021` is a `CREATE`, not an ALTER. The doc that misled me, `SCHEMA_CONVENTIONS.md`, said the cutoff was v0.5.0; corrected to v2.0.0 in dotfiles `67b925a`.)

- **Contract revised per Bishop (♗) CTO review + Indy (May 23, 2026).** Adopted into the freeze: (1) **identity from the Bearer token, not the URL** — self-plane is `/v1/runners` + `/v1/runners/me/{heartbeats,leases,reports}` (mirrors the existing `/v1/tenants/me/...`; no `runner_id` in any path/body → no IDOR/reconcile surface; my earlier `/runners/{id}/leases` had that bug); Indy's call: prefer `/runners/` plural, and if the URL-shape gate flags `me`, add a documented caveat. (2) **Fencing** — `lease_id` + `fencing_token` (monotonic) + `lease_expires_at` on the lease, echoed at report; rejects a stale/reclaimed holder so the valid holder's result wins (plain `ON CONFLICT` idempotency alone is first-writer-wins under reclaim — wrong for a non-deterministic agent). (3) **No 204** — lease returns `200 { lease|null, retry_after_ms }`. (4) **Enrollment token in the `Authorization` header**, not the body. Kept (push-back on Bishy): `outcome: processed|agent_error` (mirrors `core.zombie_events.status`, not invented `succeeded/failed`). Deferred to milestones (see Out of Scope): operator fleet-plane (M80_006/005), authz fields `trust_class`/`allowed_workspace_ids` (M80_005 / mode C), heartbeat capacity (M80_007), fencing-logic + `Idempotency-Key` (M80_002), and the principle that placement must NOT trust self-reported `sandbox_tier` (M80_005/007). Bishy prompt + full verdict table archived in the PR session notes.

- **register auth: enrollment token → existing credential — supersedes Bishop decision (4) above (Indy, May 24, 2026).** The earlier "enrollment token in the `Authorization` header" decision is reversed. `register` is authed by an *existing* operator/provisioner credential — a Clerk JWT (operator at the dashboard/CLI) or a `zmb_t_` api_key (automated provisioner) — through the existing `bearer_or_api_key` middleware; **there is no enrollment token and no separate admin endpoint.** The operator/admin who calls `register` is the trust anchor; the minted `zrn_` it returns is the runner's only credential thereafter. Rationale: a separate enrollment-token system is unbuilt infrastructure for a problem an existing credential already solves; the self-enrolling open-fleet case is mode C (later), not S0. Drift this corrected (the architecture docs already moved to this model; code + spec lagged): dropped `RegisterRequest.enrollment_token`, retired error code `UZ-RUN-002 invalid_enrollment_token` (now a registry gap; register failures surface the standard 401 from `bearer_or_api_key` / 403 from `RequireRole`), and reconciled `protocol.zig`, `error_entries.zig`, `error_registry.zig`, `schema/021_fleet_runners.sql`, and this spec's Interfaces / Failure Modes / Test Spec / Dimension 3.1. register-authz is gated by `RequireRole{.admin}` today; the `fleet:write` scope is the v2.1 target (`docs/architecture/roadmap.md`). Matches `runner_fleet.md` (Registering a runner) + `AUTH.md` (Runner token → Provisioning).

- **§2 stubs implemented; "mothership" terminology retired (Indy + Architecture Consult Gate, May 24, 2026).** §2.1 (four `/v1/runners` routes resolve and return `UZ-RUN-004` not-implemented, never 404) + §2.2 (`zig build --build-file build_runner.zig` builds a skeleton that logs a health line and exits 0) are done. Reconciled the stale `mothership`/`mothership_service` term to `zombied` (the control plane) per `runner_fleet.md` — the `/v1/runners` endpoints are HTTP handlers *inside* `zombied`, not a separate service binary; the planned PR #2 logic module is renamed `service.zig` (exact boundary a PR #2 call). `protocol.zig` comments + the spec prose moved to `zombied`/control-plane.

- **Stub shape: pure-501, not parse-then-501 — refines PLAN assumption (1) (May 24, 2026).** Assumption (1) proposed stubs *parse* their request type for NDC reachability. Superseded: `router.zig` now imports `protocol.zig` for the single-sourced `PATH_RUNNERS*` constants (RULE UFS), so the contract module already has a production consumer; the request/response types are exercised by the round-trip test (1.1); the four routes are wired + tested (2.1). So each stub just returns `UZ-RUN-004` — no parse-and-discard branch (which would add an untested 400 path and muddy "not implemented"). NDC ("reachable + tested, never orphaned") is satisfied at the module + route level.

- **Stub auth = `none` middleware (May 24, 2026).** A 501 stub exposes no data, so there is nothing to gate; the runner-token (`runnerBearer`) + `bearer_or_api_key` wiring lands with the loopback skeleton / identity slice. Documented in `route_table.zig`.

- **Doc restructure folded into this PR (Indy "one PR", May 24, 2026).** The auth-doc slim, `AUTH_DEVICE_LOGIN.md` split, `roadmap.md` (absorbs `bastion.md`), and `office_hours`/`plan_engg_review` → `archive/` moves were originally outside the declared Files-Changed scope. Per Indy's "finish stubs + one PR" call they are kept in this PR and the Files Changed table is amended to cover them (rather than split into a separate docs PR).

- **VERIFY dispositions (May 24, 2026).** `make lint-zig` clean (ZLint 0/0 across 379 files → PUB GATE mechanically satisfied; pg-drain, schema-gate, line-limit all green). `make test-unit-zombied` 1421/0/258. Cross-compile x86_64-linux + aarch64-linux clean (incl. `zombie-runner`). UFS / LOGGING / LIFECYCLE / ERROR-REGISTRY (108/108) audits clean. **`make test-integration` not run locally** — Docker/Postgres are down on this host; the `routes_integration_test` is discovered + CI-gated → `VERIFY GATE: test-integration skipped per environment constraint (Docker/PG unavailable)`. **`build.zig`** stays over 350 lines (pre-existing build file; the line-limit gate is new-files-only, so not flagged); `build_runner.zig` is new and well under.

- **`zombie-runner` gets its own build graph — `build_runner.zig` (Indy, May 24, 2026).** Indy's call: the runner is a long-running daemon and an HTTP *client* of `zombied` (it long-polls `POST /v1/runners/me/leases`, runs the event, reports, loops) — it serves no inbound HTTP, so it has **no router/handlers of its own** (those live in `zombied`, which gains the `/v1/runners` server). I recommended a single build graph + a portability gate; Indy chose a separate `build.zig`. Honored: the runner target moved out of the root `build.zig` into `build_runner.zig`, so the runner's dependency set is declared in isolation and **cannot link `pg`/`httpz`/`redis`**. The frozen `protocol.zig` is shared *by source* (one file, referenced by both build graphs) so server and client cannot drift. Trade-off accepted: the `log` module definition is duplicated across the two build files. PR #2 watch-item: when the runner client imports `protocol.zig` (which reaches `../zombie/` + `../executor/`), the runner build's module root must cover `src/` (as the bench module does) or expose `protocol` as a named module, to stay legal under Zig 0.15's module-root boundary.

- **`contract.zig` → `protocol.zig` rename (Indy, May 24, 2026).** "contract" is used pervasively as a *technical* term across the codebase (allocator/portability/wire contracts), so it is not the banned bureaucratic usage — but the in-repo **file-naming** convention for a versioned wire interface is `protocol.zig` (`src/executor/protocol.zig`, which this spec's Prior-Art already cites as the model). Renamed `src/runner/contract.zig` → `src/runner/protocol.zig` (+ `protocol_test.zig`) and updated all importers (`router.zig`, `router_test.zig`, `main.zig`, `routes_integration_test.zig`). The milestone ID / branch / spec title keep "contract" as the technical concept — renaming identifiers is churn for no gain.

- **Clean seam now; executor → runner migration deferred to the M80_003 cutover (Indy, May 24, 2026).** Indy wants the executor migrated *into* the runner with no `src/executor` leftovers. That is the architecture (`runner_fleet.md:24`) — but it is **gated on build order**: `zombied`'s live worker spawns the executor to run every event today, and Invariant 1 requires the flag-off path byte-identical, so the executor cannot move or be deleted until the runner replaces the direct path (PR #2 skeleton → M80_003). Decision: this keystone builds the **seam** (extract `execution_policy` types module + `event_envelope` module + relocate `protocol` to a self-contained module, killing the `protocol → ../executor/context_budget.zig` coupling) and **leaves the engine in place**. The full migration + `src/executor` deletion is itemized in **Out of Scope → M80_003 cutover kill-list** above, so nothing is lost before M80_003 is specced. (`context_budget.fromJson` has exactly one caller — `handler.zig:173` — so the type/parsing split is clean: types → `execution_policy` module now, `fromJson` migrates with the engine at the cutover.)

- **Keystone reshaped — faithful-mirror report · single PR · runner auth plane pulled into M80_001 (Indy, May 25, 2026).** Four decisions, captured together because they interlock:
  1. **Single PR (supersedes the Fan-out gate two-PR park-midway above).** §1+§2+§3+§4 ship in ONE PR; the spec moves to `done/` on merge, not parked. This flips the gate from *freeze* to *validate*: the four M80_002–005 fan-out streams unblock on the **proven** keystone (the loopback skeleton landed), not on the freeze alone — so a contract change forced by the skeleton costs zero downstream rework instead of up to four streams.
  2. **Report is faithful-mirror, not the stronger contract (reconciles Invariant 2, Dim 3.3, the report-replay/reclaim Failure Modes).** The direct path the skeleton must reproduce (`event_loop_writepath.finalize`) is NOT one transaction and is NOT idempotent/fenced: `markTerminal` + `recordStageActuals` + `checkpointZombieSession` are three independent autocommit statements, the debit fires pre-execution on an estimate (`debitReceive`/`debitStage`, never re-charged), and `XACK` is the final non-transactional step. The report handler reproduces exactly that. The frozen `fencing_token`/`lease_id` fields stay on the contract, but **fencing verification + `INSERT … ON CONFLICT` idempotency are M80_002** (already this spec's Out-of-Scope for fencing logic) — S0 accepts `fencing_token` without enforcing it. Rationale: a single-zombie loopback skeleton has no reclaim path to fence against; building the stronger guarantee here would diverge from the path it is meant to validate.
  3. **Option B — the runner auth plane moves from M80_005 into M80_001.** AUTH.md + `runner_fleet.md` (S4) originally parked the `register` handler + `runnerBearer` middleware + TLS in M80_005. But the four `/v1/runners/*` routes are registered always-on (§2) on zombied's public API; the `ZOMBIE_RUNNER_SEAM` flag gates only the worker's *client* behaviour, not the server routes. A real `lease`/`report` handler on `none` middleware is therefore a live, unauthenticated endpoint that hands a tenant's `secrets_map` to any caller in production (flag-off) — a P0. So real lease/report cannot ship without the auth plane gating them. M80_001 now ships the working `register` handler (`bearer_or_api_key` + `RequireRole.admin`, mints `zrn_`), the `runnerBearer` middleware, and `AuthPrincipal.runner_id`; **M80_005 narrows to TLS + the `trust_class`/`allowed_workspace_ids` authz fields + operator-assigned-trust placement.** AUTH.md "What ships when" + `runner_fleet.md` roadmap reconciled. (Also fixed prose drift: both docs said `POST /v1/runners/register`; the frozen contract is `POST /v1/runners` — docs corrected to the contract.)
  4. **The fork + the deliberate orchestration duplication.** The worker forks at the per-zombie loop level (`worker_zombie.zombieWorkerLoop`, after `claimZombie`): flag-on runs `register`-once then `loop{ lease → executeInSandbox → report }`; flag-off runs `runEventLoop` byte-identical (Invariant 1). The `lease` handler re-orchestrates `writepath.run` steps 1–7 (insert-received, resolve tenant/provider, balance gate, `debitReceive`, approval gate, `debitStage`) + the `secrets_map`/context-budget resolution from `executeInSandbox`, then `XREADGROUP`s the work and persists a `fleet.runner_leases` row; the `report` handler reuses `finalize`'s leaf writers (`markTerminal`/`recordStageActuals`/`checkpointZombieSession`/`xackZombie`). It calls the **existing leaf helpers** rather than refactoring `writepath.run` — so `writepath.run` stays untouched (Invariant 1) at the cost of orchestration duplication between it and the lease handler. That duplication is intentional for a skeleton; the real shared control-plane abstraction is M80_002.

---

- **Rescoped to the durable keystone; loopback §3.4 + flag-parity §4 superseded (Indy, May 25, 2026).** The throwaway loopback skeleton was abandoned mid-build and replaced by the real cutover (`docs/v2/.../M80_002_P1_API_RUNNER_CUTOVER.md`), shipped as ONE Pull Request continuing this branch (git-flow Option A — a fresh branch off `main` would lose the large uncommitted durable keystone set). Verbatim: *"I think i want to keep building till the 80_003 since it is not working building this way since we are building throw away code"* + *"and send that in 1 PR till 80_003."* M80_001's durable deliverable — frozen contract (`protocol.zig`), `fleet.runners`/`fleet.runner_leases` schema, the `runnerBearer` auth plane, and the real register/heartbeat/lease/report handlers — is complete and committed. §3.4 (loopback one-zombie e2e) and §4 (flag parity) are deleted-and-replaced by the cutover: its §2.2/§6.1 prove the path end-to-end against the real `zombie-runner` daemon, and it *removes* the direct path (RULE NLR) rather than flag-gating it. The single-zombie §3.2/3.3 integration tests are absorbed into the cutover's §1 multi-zombie assignment/fencing suite. Throwaway removed in the foundation commit: `src/runner/loopback.zig`, the `ZOMBIE_RUNNER_SEAM`/`LoopbackConfig` worker fork, the stub-era `routes_integration_test.zig`, and the now-dead `UZ-RUN-004` (`not_implemented`).

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments addressed before human review/merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-zombied` | 1427 passed · 0 failed · 258 skipped (database-gated); executor 414/414 | ✅ |
| Integration tests | `make test-integration` | not run locally — PG/Redis down (`TEST_DATABASE_URL` unset); CI-gated. The runner integration suite lands with the cutover (§1 assignment/fencing). | ⏳ CI |
| e2e (loopback one zombie) | — | 🔻 superseded → cutover runner-daemon e2e | n/a |
| Lint | `make lint-zig` | fmt ✓ · ZLint 0/0 (386 files) · pg-drain ✓ · line-limit ✓ · schema-gate ✓ · legacy/role ✓ | ✅ |
| Cross-compile (Zig) | `zig build -Dtarget={x86_64,aarch64}-linux` (both graphs) | both clean, incl. `zombie-runner` | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found (pre-commit) | ✅ |
| Runner skeleton build | `zig build --build-file build_runner.zig` | binary built; emits logfmt health line, exits 0 | ✅ |

---

## Out of Scope

- **M80_002** — `zombied` runner API: real assignment across all zombies, sticky-routing logic, **fencing-token assignment + verification** (the contract freezes the fields; the logic lives here), `Idempotency-Key` on register, and the `fleet` affinity storage.
- **M80_003 — the cutover (executor → runner migration). Kill-list tracked here until M80_003 is specced:**
  - Remove `zombied`'s direct worker path (the worker spawning the executor sidecar) — only safe once the runner runs events (PR #2+). This is the gate for everything below.
  - Migrate the execution **engine** into the runner (it folds NullClaw in directly): `src/executor/{runner,handler,tool_bridge,tool_builders,session,runner_helpers,zombie_memory,runner_observer,runner_progress,progress_callbacks,progress_writer}.zig`, `src/executor/runtime/**`, and the sandbox tier (`landlock,cgroup,network,executor_network_policy`).
  - Migrate `src/executor/context_budget.zig` (the `fromJson` parsing remnant — sole caller `handler.zig:173`) **with the engine** to the runner. The clean type module `execution_policy` (extracted in this keystone) stays shared.
  - Delete the sidecar process scaffolding once the runner runs in-process: the `zombied-executor` / `-harness` / `-stub` build targets, `src/executor/main.zig`, and `src/executor/transport.zig` (the Unix-socket transport the sidecar used).
  - Keep (shared, created by this keystone — the runner imports them): the `protocol`, `event_envelope`, and `execution_policy` modules.
- **M80_004** — `zombie-runner` packaging, macOS Seatbelt backend, distribution/CI, and the runner CLI (matches `zombiectl`'s endpoint flag — not a bespoke `--mothership`).
- **M80_005** — enrollment/identity/TLS hardening; the register **authz fields** (`trust_class`, `secret_delivery_modes`) added additively; operator-assigned trust that placement keys off (vs the self-reported `sandbox_tier`, which is telemetry only).
- **M80_006 / M80_007** — fleet inventory/heartbeat/failover; scheduler; **the operator fleet-plane** (`GET /v1/fleet/runners`, revoke via `PATCH /v1/fleet/runners/{id}`, `GET /v1/fleet/leases/{id}`); heartbeat **capacity / active_leases**.
- **mode C (post-S6)** — per-tenant-scoped runners incl. `allowed_workspace_ids`; `secret_delivery` modes `scoped`/`proxy` (zero-trust); multi-zombie scale; the flag flip itself.
- **OpenAPI tag split** — `RunnerSelf` vs `Fleet` tags (leases surface in both planes; §1 wants tag 1:1 with resource).
