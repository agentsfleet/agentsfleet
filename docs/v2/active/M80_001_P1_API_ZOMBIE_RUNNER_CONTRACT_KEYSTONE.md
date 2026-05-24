# M80_001: Freeze the `/v1/runners` contract and prove it with a loopback walking skeleton

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 001
**Date:** May 22, 2026
**Status:** IN_PROGRESS
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
- **`docs/LOGGING_STANDARD.md`** / **`docs/LIFECYCLE_PATTERNS.md`** — new log emits; init/deinit + errdefer on new mothership-side structs holding PG/Redis handles.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | new `*.zig` under `src/`; cross-compile x86_64+aarch64-linux; read ZIG_RULES before EXECUTE. |
| PUB / Struct-Shape | yes | own shape verdict per new file (contract types, mothership service, loopback client); no inheritance. |
| File & Function Length (≤350/≤50/≤70) | yes | split contract types / handlers / service across files; one verb per handler file. |
| UFS | yes | path segments, `secret_delivery`/`sandbox_tier` values, `ZOMBIE_RUNNER_SEAM` flag name as named constants shared verbatim across Zig + TS. |
| LOGGING | yes | logfmt emits in register/lease/report; `secrets_map` + `runner_token` never logged. |
| LIFECYCLE | yes | mothership service holds pooled handles; init/deinit + errdefer adjacent to alloc. |
| ERROR REGISTRY | yes | declare `UZ-RUN-001…` in `src/errors/error_registry.zig` before use. |
| SCHEMA GUARD | yes | `021` create + `022` alter; single-concern ≤100 lines/file; update `embed.zig` + migration array. |
| MILESTONE-ID | yes | code/test names carry NO `M80`/§/dim IDs (RULE TST-NAM); milestone id lives only in spec + flag-doc prose. |
| UI / DESIGN TOKEN | no | no UI surface in S0. |

---

## Overview

**Goal (testable):** with `ZOMBIE_RUNNER_SEAM=1`, one zombie's steer event flows register → lease → executor → report over loopback and writes the same `zombie_events`/`zombie_execution_telemetry`/`zombie_sessions` rows and `XACK` the direct worker writes today; with the flag unset, the direct path is byte-for-byte unchanged.

**Problem:** the worker can only run where it reaches Postgres and Redis directly, and the four planned runner workstreams have no validated interface to build against in parallel — a guessed contract risks reworking up to four streams.

**Solution summary:** freeze the `/v1/runners` contract (request/response types + the four endpoints) and ALL runner schema, pre-claim the three shared-file conflict points (route table, build target, migration array) as stubs so the parallel streams touch only disjoint directories, and implement exactly one happy-path vertical — one zombie, loopback, flag-gated — that exercises register/lease/report against the real datastores via the mothership while the production direct path remains the default. The skeleton both freezes and validates the contract before fan-out.

---

## Prior-Art / Reference Implementations

- **API** → `docs/REST_API_DESIGN_GUIDELINES.md` + the nearest existing handler under `src/http/handlers/` (webhook receiver / api_keys); route registration mirrors `src/http/route_table.zig`.
- **Internal versioned contract** → `src/executor/protocol.zig` — the executor's explicit method names + numeric error codes are the in-repo model for a versioned RPC surface; `/v1/runners` mirrors that explicitness over HTTPS.
- **Schema** → `schema/020_tenant_providers.sql` + `docs/SCHEMA_CONVENTIONS.md`.
- **Connection topology** → M80_001 reuses the mothership pool from M69_004 (Redis pool/subscriber unify); the lease/report handlers use short-lived pooled commands, never a blocking connection in the request path.
- **Contract shape itself** → greenfield; defined in `docs/architecture/runner_fleet.md`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/contract.zig` (+ split files) | CREATE | the frozen request/response types, `secret_delivery` + `sandbox_tier` enums, shared wire-value constants |
| `src/runner/mothership_service.zig` (+ splits) | CREATE | mothership-side lease/report/register logic over the pool; the one-txn report |
| `src/runner/loopback_client.zig` | CREATE | the client the flag-gated skeleton uses to call the mothership over loopback |
| `src/http/handlers/runner/{register,heartbeat,lease,report}.zig` | CREATE | the four handlers; non-skeleton verbs return declared not-implemented |
| `schema/021_fleet_runners.sql` | CREATE | `fleet.runners` in a dedicated `fleet` control-plane schema (identity, token hash, sandbox_tier, labels, last_seen, status, optional tenant scope) |
| `schema/embed.zig` | EDIT | append `021` to the migration array (shared conflict point — pre-claimed here) |
| `src/http/route_table.zig`, `src/http/router.zig` | EDIT | register `/v1/runners/*` (shared conflict point — pre-claimed here) |
| `build.zig` | EDIT | add the `zombie-runner` executable target skeleton (shared conflict point — pre-claimed here) |
| `src/errors/error_registry.zig` | EDIT | declare `UZ-RUN-*` codes |
| `src/cmd/worker.zig`, `src/cmd/worker_config.zig` | EDIT | the single flag gate forking the skeleton path; flag config |
| `src/runner/*_test.zig`, `src/http/handlers/runner/*_test.zig` | CREATE | per-Dimension unit + integration + e2e tests |
| `docs/architecture/runner_fleet.md` | EDIT | add the operator-enrollment sequence + trust-gate model (Gates 1–4) — bundled into this PR per Indy |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** contract-first keystone, then parallel fan-out (plan-eng-review D2). S0 freezes + validates the interface before four streams commit to it; it pre-claims the three shared files as stubs so the streams never collide on `build.zig`, the route table, or `embed.zig`.
- **Alternatives considered:** big-bang single branch (rejected — a ~20k-line unreviewable PR; tenant-secret risk unmitigated); linear strangler (rejected — serializes the four streams, wasting the parallel capacity); eager fan-out with no skeleton (rejected — a wrong contract reworks up to four streams).
- **Patch-vs-refactor verdict:** this is a **refactor-enabling seam** ("make the change easy, then make the easy change"), scoped to the interface + one proof path. It is NOT the worker migration (that is M80_003); the direct path is untouched and default.

---

## Sections (implementation slices)

### §1 — Contract & schema freeze

The durable interface the parallel streams depend on. Freezes types and tables; the logic that *uses* them (real assignment, sticky routing) is M80_002+. **Implementation default:** wire values are snake_case strings single-sourced as named constants, because UFS requires Zig and the future TS client to share them verbatim.

- **Dimension 1.1** — contract request/response types + `secret_delivery` + `sandbox_tier` enums round-trip serialize/deserialize → Test `test_runner_contract_roundtrip`
- **Dimension 1.2** — `021_fleet_runners` (`fleet.runners`) migrates clean on a fresh database → Test `test_runner_schema_migrates`
- **Dimension 1.3** — `embed.zig` array + migration runner apply `021`/`022` in order and are idempotent on re-run → Test `test_runner_migrations_idempotent`

### §2 — Shared-file stubs (pre-claim the conflict points)

Register the three shared files as stubs so the four streams edit only their own directories.

- **Dimension 2.1** — the four routes — `POST /v1/runners` + `POST /v1/runners/me/{heartbeats,leases,reports}` — resolve in the route table; non-skeleton verbs return a declared not-implemented error, not a 404 → Test `test_runner_routes_registered`
- **Dimension 2.2** — the `zombie-runner` build target compiles (skeleton `main` that logs a health line and exits cleanly) → Acceptance: `zig build zombie-runner`

### §3 — Loopback walking skeleton (flag-gated)

One zombie, register→lease→report over loopback, behind `ZOMBIE_RUNNER_SEAM=1`.

- **Dimension 3.1** — `register` (authed by an existing operator/provisioner credential — Clerk JWT or `zmb_t_` api_key, via `bearer_or_api_key`) mints a `runner_token` and inserts a `fleet.runners` row with `sandbox_tier` + labels → Test `test_register_mints_runner_token`
- **Dimension 3.2** — `lease` returns the next event for the one assigned zombie with resolved config and (mode `inline`) `secrets_map` → Test `test_lease_returns_event_with_secrets`
- **Dimension 3.3** — `report` writes received→terminal + telemetry + debit + session checkpoint in one transaction then `XACK`s, idempotent on replay → Test `test_report_batched_idempotent`
- **Dimension 3.4** — end-to-end: flag on, one zombie's steer flows register→lease→executor→report over loopback; the user-visible rows equal the direct path's → Test `test_e2e_loopback_one_zombie`

### §4 — Flag parity & isolation

- **Dimension 4.1** — with `ZOMBIE_RUNNER_SEAM` unset, the worker takes the unchanged direct path and `src/executor/**` is untouched → Test `test_flag_off_parity`

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
| Report replay | retry by the same holder | `INSERT … ON CONFLICT` → idempotent no-op; returns recorded result |
| Invalid/expired runner_token | unregistered runner | 401 `UZ-RUN-001` |
| Bad register credential | register with an invalid/expired Clerk JWT or `zmb_t_` | 401 via `bearer_or_api_key` (no UZ-RUN code; not a runner-plane concern) |
| Unsupported secret mode | `secret_delivery` ≠ inline in S0 | 400 `UZ-RUN-003` |
| Executor unavailable on runner | local Unix socket down | report carries `agent_error`; lease redeliverable; no datastore corruption |
| Mothership unreachable (loopback) | mothership down mid-skeleton | loopback client retries with backoff; lease un-acked → no event loss |
| Report after reclaim | slow runner; another reclaimed + ran it | **fencing**: the slow runner's `fencing_token` is stale → its report is rejected, so the valid holder's result wins (not merely "first writer wins" under idempotency) |

---

## Invariants

1. **Flag-off = unchanged direct path** — a single gate at the worker entry; `test_flag_off_parity` + no edits to the direct write path in the diff.
2. **`report` idempotent per `event_id`, fenced against reclaim** — `INSERT … ON CONFLICT` for replay by the same holder, plus a monotonic `fencing_token` that rejects a stale (reclaimed) lease holder so the valid holder's result wins; `test_report_batched_idempotent` double-reports + a stale-fence report is rejected.
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
| 2.2 | e2e | (acceptance) | `zig build zombie-runner` produces a binary that logs health and exits 0 |
| 3.1 | integration | `test_register_mints_runner_token` | valid operator credential (Clerk JWT / `zmb_t_`) → runner_token + a `fleet.runners` row; bad credential → 401 via `bearer_or_api_key` |
| 3.2 | integration | `test_lease_returns_event_with_secrets` | leased event carries resolved config + inline `secrets_map`; empty stream → 204 |
| 3.3 | integration | `test_report_batched_idempotent` | one txn writes all rows + XACK; second identical report no-ops, same result |
| 3.4 | e2e | `test_e2e_loopback_one_zombie` | flag on: steer → register→lease→executor→report; rows equal the direct path |
| 4.1 | integration | `test_flag_off_parity` | flag unset: direct path runs unchanged; executor untouched |

**Regression:** the existing direct path is guarded by `test_flag_off_parity` (4.1) — flag-off must be byte-identical. **Idempotency/replay:** 3.3 + the report-replay failure mode. Non-self-evident payloads → `samples/fixtures/m80-fixtures/`.

---

## Acceptance Criteria

- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (HTTP + schema + Redis touched)
- [ ] `make memleak` clean (mothership handlers allocate)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `zig build zombie-runner` produces the skeleton binary
- [ ] `test_flag_off_parity` green (the existing path is unchanged)
- [ ] `gitleaks detect` clean · no file over 350 lines added · `bash scripts/audit-spec-template.sh` clean

---

## Eval Commands (post-implementation)

```bash
# E1: flag-off parity — existing path unchanged
make test-integration 2>&1 | grep -E "flag_off_parity|PASS|FAIL"
# E2: Build — full + runner skeleton
zig build && zig build zombie-runner 2>&1 | tail -3
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

- **register auth: enrollment token → existing credential — supersedes Bishop decision (4) above (Indy, May 24, 2026).** The earlier "enrollment token in the `Authorization` header" decision is reversed. `register` is authed by an *existing* operator/provisioner credential — a Clerk JWT (operator at the dashboard/CLI) or a `zmb_t_` api_key (automated provisioner) — through the existing `bearer_or_api_key` middleware; **there is no enrollment token and no separate admin endpoint.** The operator/admin who calls `register` is the trust anchor; the minted `zrn_` it returns is the runner's only credential thereafter. Rationale: a separate enrollment-token system is unbuilt infrastructure for a problem an existing credential already solves; the self-enrolling open-fleet case is mode C (later), not S0. Drift this corrected (the architecture docs already moved to this model; code + spec lagged): dropped `RegisterRequest.enrollment_token`, retired error code `UZ-RUN-002 invalid_enrollment_token` (now a registry gap; register failures surface the standard 401 from `bearer_or_api_key` / 403 from `RequireRole`), and reconciled `contract.zig`, `error_entries.zig`, `error_registry.zig`, `schema/021_fleet_runners.sql`, and this spec's Interfaces / Failure Modes / Test Spec / Dimension 3.1. register-authz is gated by `RequireRole{.admin}` today; the `fleet:write` scope is the v2.1 target (`docs/architecture/roadmap.md`). Matches `runner_fleet.md` (Registering a runner) + `AUTH.md` (Runner token → Provisioning).

---

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
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| e2e (loopback one zombie) | `make test-integration` (e2e target) | | |
| Lint | `make lint` | | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | | |
| Gitleaks | `gitleaks detect` | | |
| Runner skeleton build | `zig build zombie-runner` | | |

---

## Out of Scope

- **M80_002** — mothership API: real assignment across all zombies, sticky-routing logic, **fencing-token assignment + verification** (the contract freezes the fields; the logic lives here), `Idempotency-Key` on register, and the `fleet` affinity storage.
- **M80_003** — worker thinning + removal of the direct PG/Redis path (the cutover).
- **M80_004** — `zombie-runner` packaging, macOS Seatbelt backend, distribution/CI, and the runner CLI (matches `zombiectl`'s endpoint flag — not a bespoke `--mothership`).
- **M80_005** — enrollment/identity/TLS hardening; the register **authz fields** (`trust_class`, `secret_delivery_modes`) added additively; operator-assigned trust that placement keys off (vs the self-reported `sandbox_tier`, which is telemetry only).
- **M80_006 / M80_007** — fleet inventory/heartbeat/failover; scheduler; **the operator fleet-plane** (`GET /v1/fleet/runners`, revoke via `PATCH /v1/fleet/runners/{id}`, `GET /v1/fleet/leases/{id}`); heartbeat **capacity / active_leases**.
- **mode C (post-S6)** — per-tenant-scoped runners incl. `allowed_workspace_ids`; `secret_delivery` modes `scoped`/`proxy` (zero-trust); multi-zombie scale; the flag flip itself.
- **OpenAPI tag split** — `RunnerSelf` vs `Fleet` tags (leases surface in both planes; §1 wants tag 1:1 with resource).
