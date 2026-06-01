# M80_008: Retire the dead worker datastore substrate left by the cutover

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 008
**Date:** May 27, 2026
**Status:** IN_PROGRESS
**Branch:** feat/m80-008-worker-substrate-retirement
**Priority:** P1 — collapses two datastore security roles to one and removes a dead startup requirement; operator-facing (deploy contract changes: `*_WORKER` secrets retire).
**Categories:** API, INFRA
**Batch:** B1 — single workstream; depends only on the M80_002 cutover already having moved execution-path writes onto the `api` role.
**Depends on:** M80_002 (the cutover that deleted the worker process and moved its writes onto the `api_runtime` pool) — merged in PR #349.
**Provenance:** agent-generated (Indy direction, May 27, 2026: after the cutover, "*_WORKER vars is not valid var… must be removed" + "env vars must be nuked since runners don't have access to db/redis"; finding verified against `serve.zig`/`service.zig`).

**Canonical architecture:** `docs/architecture/data_flow.md` (the post-cutover runtime — `zombied` is the sole writer via its control-plane pool) and `docs/architecture/runner_fleet.md` (runner holds zero datastore credentials).

---

## Implementing agent — read these first

1. `src/zombied/cmd/serve.zig` (≈144–150) — proves only the `.api` DB pool + `.api` Redis client are initialised in production; there is no `.worker` pool.
2. `src/zombied/fleet/service.zig` (≈126) + `service_report.zig` — the lease/report execution-path writes use `hx.ctx.pool` (the api pool); `metering`, `event_rows`, `approval_gate`, `zombie_session` all write through it.
3. `src/zombied/config/env_vars.zig` — the startup validation that *requires* `DATABASE_URL_WORKER`/`REDIS_URL_WORKER` (the dead requirement) + the `CheckMode`/role-separation logic to remove.
4. `schema/002_vault_schema.sql` — the `worker_runtime` role CREATE + search_path; the GRANTs to remove are spread across the schema files listed in Files Changed.
5. `docs/SCHEMA_CONVENTIONS.md` and `docs/ZIG_RULES.md` — pre-v2.0 CREATE-not-ALTER teardown convention; the role-removal touches the migration files.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Retire the dead `worker_runtime` datastore substrate; run `zombied` on a single `api` role
- **Intent (one sentence):** the M80_002 cutover deleted the worker process but left its Postgres/Redis role + `DATABASE_URL_WORKER`/`REDIS_URL_WORKER` env vars + bootstrap as dead-but-required scaffolding; remove them so `zombied` boots and runs on the single `api_runtime` DB/Redis role it already uses for every write.
- **Handshake (agent fills at PLAN):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`. The load-bearing assumption to re-verify: `api_runtime` already holds every GRANT the lease/report path needs (M80_002 integration passes prove it). If any execution-path write fails as `api_runtime`, STOP — the role consolidation is not safe and the spec must be reconciled.
- **Confirmed anchors (Indy, May 27, 2026):** (1) the `worker` role + `*_WORKER` env vars are invalid post-cutover and must be removed, not renamed; (2) renaming to `runner_*` is wrong — the runner holds zero datastore credentials, so a `runner`-named DB role would actively mislead; (3) the live `*_WORKER` secret / PG-role / Redis-ACL teardown is deploy-coordinated (Indy's window).

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NLR** (touch-it-fix-it: the worker substrate is *deleted*, not left dead), **NDC** (no dead code/requirements: the env validation that requires an unused URL is exactly the dead requirement being removed), **NLG** (pre-2.0: no compat shim that accepts the old `*_WORKER` vars as a fallback — they go, full stop), **ORP** (orphan sweep: no `worker_runtime`/`DATABASE_URL_WORKER`/`REDIS_URL_WORKER` symbol survives), **UFS** (env-var names + role constants single-sourced).
- **`docs/SCHEMA_CONVENTIONS.md`** — role + GRANT removal across the migration files; pre-v2.0 CREATE-not-ALTER teardown convention; `embed.zig` + migration array stay consistent; Schema Removal Guard fires.
- **`docs/ZIG_RULES.md`** — enum removal (`DbRole`/`RedisRole`/`CheckMode`), error-set narrowing (drop the `*Worker*` `EnvVarsErrors`), file-as-struct discipline, cross-compile both Linux targets.
- **`docs/AUTH.md`** — the role model: removing `worker_runtime` collapses the data plane onto `api_runtime`; document that the control plane is the sole writer and the runner holds no datastore role.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes | role + GRANT removal across `schema/*.sql`; single-concern; `embed.zig` + migration array stay consistent; pre-v2.0 teardown convention. |
| ZIG GATE | yes | `*.zig` edits (env_vars, pool, redis_types, doctor, serve); cross-compile x86_64+aarch64-linux. |
| ERROR REGISTRY | maybe | if any `UZ-*` error referenced the worker role config; otherwise n/a. |
| LOGGING | yes | `doctor`/`serve` log emits change when the worker checks are removed; logfmt, no secrets. |
| UFS | yes | the `*_WORKER` env-var name constants + role-name constants are removed at their single source. |
| MILESTONE-ID | yes | no `M80`/§/dim IDs in code or schema bodies. |
| Architecture Consult & Update | yes | `data_flow.md` already names the api pool as the writer; confirm coherent — small or no doc delta. |
| LENGTH | yes | net-removing lines; no new file over cap. |

---

## Overview

**Goal (testable):** `zombied serve` boots and passes `make test-integration` + `make memleak` with **only** `DATABASE_URL_API` + `REDIS_URL_API` set (no `*_WORKER` vars); a repo grep for `worker_runtime` / `DATABASE_URL_WORKER` / `REDIS_URL_WORKER` returns zero hits in `src/`, `schema/`, `docker-compose.yml`, and the workflows.

**Problem:** the M80_002 cutover deleted the `zombied worker` process but left its `worker_runtime` Postgres role, its `worker` Redis ACL role, and its `DATABASE_URL_WORKER`/`REDIS_URL_WORKER` env vars in place. `serve.zig` never creates a `.worker` pool — the lease/report execution-path writes go through the `.api` pool (`service.zig` `hx.ctx.pool`). The `*_WORKER` env vars are therefore **required at startup but never used to connect** — a dead requirement that confuses operators (it reads as if the runner needs a database) and carries an unused security role.

**Solution summary:** remove the `worker_runtime` Postgres role (CREATE + all GRANTs + `ALTER ROLE`), the `worker` Redis ACL role, the `DATABASE_URL_WORKER`/`REDIS_URL_WORKER` env reading + the `config/env_vars.zig` validation that requires them, the `DbRole.worker`/`RedisRole.worker`/`CheckMode.worker` enum values, and the `doctor` worker checks; collapse the data plane onto the single `api_runtime` role `zombied` already uses for every write. The live PG-role / Redis-ACL / Fly+1Password `*_WORKER` secret teardown is sequenced into a deploy window.

---

## Prior-Art / Reference Implementations

- **The api role path** → `serve.zig` `.api` pool + `connectFromEnvWithOptions(.api, …)` is the exact pattern that survives; the worker path is removed to match it. The lease/report writes already mirror the old worker writes through this pool (M80_002 row-equivalence).
- **Role-removal precedent** → the M10 pipeline-v1 removal (`schema/002_vault_schema.sql` comment block "Pipeline v1 removed. Grants to dropped tables removed: …") is the in-repo pattern for retiring schema grants pre-v2.0.

---

## Files Changed (blast radius)

| File / area | Action | Why |
|------|--------|-----|
| `schema/002_vault_schema.sql` | EDIT | drop `worker_runtime` from the role array + the `CREATE ROLE` loop, its `GRANT USAGE`/table grants, the `REVOKE` line, and `ALTER ROLE worker_runtime SET search_path` |
| `schema/006,007,008,009,010,014,017,018,020_*.sql` | EDIT | remove every `GRANT … TO worker_runtime` (api_runtime keeps its grants) |
| `src/zombied/config/env_vars.zig` | EDIT | remove the `db_worker`/`redis_worker` reads + their `EnvVars` fields, the **entire `CheckMode` enum** + `validateLoadedWithMode` + `enforceFromEnvWithMode` (no non-test callers) + `validateRoleSeparatedValues`, and the `Missing/SameDatabaseUrlForApiAndWorker` + `Missing/SameRedisUrlForApiAndWorker` + `RedisWorkerTlsRequired` errors; `validateLoaded`/`enforceFromEnv` become api-only |
| `src/zombied/db/pool.zig` | EDIT | remove `DbRole.worker` + its env-var mapping |
| `src/zombied/queue/redis_types.zig` | EDIT | remove `RedisRole.worker` + its env-var mapping |
| `src/zombied/cmd/doctor.zig` | EDIT | remove the worker DB + Redis readiness/ACL checks |
| `src/zombied/cmd/serve.zig` | EDIT | drop worker env-validation branches; api-only startup |
| `docker-compose.yml` | EDIT | remove `DATABASE_URL_WORKER` + `REDIS_URL_WORKER` |
| `.github/workflows/deploy-dev.yml`, `release.yml` | EDIT | remove `DATABASE_URL_WORKER`/`REDIS_URL_WORKER` from the deploy env (CI/CD edit — needs the standing Indy approval that authorised this milestone) |
| `playbooks/006_worker_bootstrap_dev/001_playbook.md`, `007_worker_bootstrap_prod/001_playbook.md` | EDIT | retire the worker-role/secret bootstrap; fold any still-needed runner-enrollment bootstrap into the runner playbooks (or delete if fully obsolete) |
| `playbooks/002_preflight`, `003_priming_infra` (role docs), `011_database_teardown/03_verify.sh` | EDIT | drop `worker_runtime` from the documented role set + the teardown role-existence assertions |
| `src/zombied/db/pool_test.zig`, `queue/redis_test.zig` | EDIT | drop the `.worker` role-name assertions |
| `docs/architecture/data_flow.md` | EDIT (if needed) | confirm the "control plane is the sole writer via the api pool" prose is coherent; small/no delta |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a single removal workstream — the substrate is genuinely dead, so this is deletion + GRANT cleanup, not a refactor. The Redis consumer-group rename (`zombie_workers`) is carved into its own Dimension because it is a *runtime* drain-migration, not a code edit.
- **Alternatives considered:** (a) keep `worker_runtime` as a distinct service role for the execution-path writes (rejected — nothing connects as it; keeping it is the dead requirement the milestone removes); (b) rename `worker_*` → `runner_*` (rejected by Indy — the runner holds zero datastore credentials, so a `runner`-named DB role misleads); (c) rename → `control_*`/`execution_*` (rejected — the role is simply unused; removal beats renaming a dead role).
- **Patch-vs-refactor verdict:** a **deletion** (NLR/NDC). The risk surface is the security-posture change (two roles → one) — gated by re-proving `api_runtime`'s grants suffice (integration) and by the deploy-coordinated live teardown.

---

## Sections (implementation slices)

### §1 — Remove the worker datastore role from the schema

- **Dimension 1.1** — `worker_runtime` is removed from the role array + `CREATE ROLE` loop and every `GRANT … TO worker_runtime` across the schema files; `api_runtime` grants are unchanged → Test `test_schema_has_no_worker_runtime_role` (migration applies clean; `pg_roles` has no `worker_runtime`; `api_runtime` retains its grants).
- **Dimension 1.2** — the `ALTER ROLE worker_runtime SET search_path` + the `REVOKE … FROM … worker_runtime` lines are removed without affecting the other roles → Verified by migration apply + the teardown verify script.

### §2 — Remove the worker role + env vars from the binary

- **Dimension 2.1** — `DbRole.worker` / `RedisRole.worker` and their `*_WORKER` env-var mappings are removed (the enums keep `.api` + the other live roles); **and the entire `CheckMode` enum is removed, not merely its `.worker` value** — with `worker` gone, `.both` becomes identical to `.api`, so the mode parameter is dead scaffolding (NDC). The code compiles with the narrowed enums → Test `test_dbrole_has_no_worker` (compile-time / enum-exhaustiveness; no `CheckMode` survives).
- **Dimension 2.2** — `config/env_vars.zig` no longer reads or requires `DATABASE_URL_WORKER`/`REDIS_URL_WORKER`; `zombied serve` boots with only `DATABASE_URL_API` + `REDIS_URL_API` → Test `test_serve_boots_api_only` (startup validation passes with the worker vars unset).
- **Dimension 2.3** — the api/worker role-separation machinery is removed **wholesale**: `validateRoleSeparatedValues`, `validateLoadedWithMode`, and `enforceFromEnvWithMode` are deleted (the last two have **zero non-test callers** and collapse to a single behaviour once `worker` is gone), and `validateLoaded` / `enforceFromEnv` become api-only; the `*Worker*` `EnvVarsErrors` variants (`Missing/SameDatabaseUrlForApiAndWorker`, `Missing/SameRedisUrlForApiAndWorker`, `RedisWorkerTlsRequired`) are dropped from the error set → Test `test_env_validation_api_only` (no worker error reachable; no mode parameter survives).
- **Dimension 2.4** — `cmd/doctor.zig` no longer probes a worker DB/Redis role → Test `test_doctor_no_worker_checks` (doctor output has no worker check ids).

### §3 — Retire the worker substrate from infra + playbooks

- **Dimension 3.1** — `docker-compose.yml` + both workflows carry no `*_WORKER` datastore vars; CI deploy passes with api-only env → Verified by workflow lint + a green deploy-dev dry run.
- **Dimension 3.2** — `playbooks/006`/`007` worker-bootstrap are retired (deleted or folded into runner enrollment); `002`/`003` role docs + `011` teardown verify carry no `worker_runtime` → Verified by grep + playbook review.

### §4 — `zombie_workers` consumer-group rename (runtime drain-migration)

- **Dimension 4.1** — the `zombie_workers` Redis consumer group is renamed to a name that reflects its post-cutover owner (`zombied` consumes it), via a **new-group + drain-old** migration (not an in-place rename); install (`ensureZombieConsumerGroup`) + lease (`xreadgroupZombieOnce`) + report (`xackZombie`) all use the new constant → Test `test_lease_uses_new_consumer_group` + a documented drain step. **Migration risk:** in-flight pending entries in the old group are abandoned on cutover; acceptable pre-launch, must be drained or accepted explicitly.

---

## Interfaces

> No HTTP or wire-contract change. The only external-facing change is the **deploy contract**: `zombied` stops reading `DATABASE_URL_WORKER`/`REDIS_URL_WORKER` and the `worker_runtime` PG role / `worker` Redis ACL user are removed.

```
deploy env (before): DATABASE_URL_API, DATABASE_URL_WORKER, REDIS_URL_API, REDIS_URL_WORKER
deploy env (after):  DATABASE_URL_API, REDIS_URL_API
pg roles  (before):  db_migrator, api_runtime, worker_runtime, memory_runtime, ops_readonly_*
pg roles  (after):   db_migrator, api_runtime, memory_runtime, ops_readonly_*
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A lease/report write needs a grant only `worker_runtime` held | `api_runtime` is missing a GRANT the old service role had | the write fails under `api_runtime`; caught by `make test-integration` BEFORE the role is dropped → STOP, add the missing GRANT to `api_runtime`, do not drop the role until green → `test_lease_report_writes_as_api_runtime` |
| zombied still required `*_WORKER` at boot after the edit | a validation branch was missed | startup rejects with a worker env error → grep gate + `test_serve_boots_api_only` catch it |
| Live deploy drops the role before the new binary ships | teardown ordering wrong | the OLD binary (still reading `worker_runtime`) loses its connection → the spec's deploy sequence ships the new binary FIRST, then drops the role/secrets |
| `zombie_workers` rename loses in-flight events | group renamed with pending entries unclaimed | pending entries in the old group are abandoned → drain the old group before cutover, or accept (pre-launch, documented) → `test_lease_uses_new_consumer_group` |

---

## Invariants

1. **`api_runtime` is the sole data-plane role** — after this milestone `zombied` connects to Postgres and Redis only as `api_runtime`; enforced by the removed enum (no `.worker` to request) + a grep gate.
2. **No dead datastore requirement** — `zombied serve` boots with `*_WORKER` unset; enforced by `test_serve_boots_api_only`.
3. **Grant-equivalence** — every write the lease/report path performs succeeds as `api_runtime`; enforced by `make test-integration` run BEFORE the role is dropped (sequencing).
4. **Runner holds zero datastore credentials** — unchanged and re-affirmed: no datastore role is named after the runner; the runner reaches the platform only over `/v1/runners`.
5. **Orphan-clean** — no `worker_runtime` / `DATABASE_URL_WORKER` / `REDIS_URL_WORKER` symbol survives in `src/`, `schema/`, `docker-compose.yml`, workflows; enforced by the grep gate (RULE ORP).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (inputs → expected) |
|-----------|------|------|------------------------------|
| 1.1 | integration | `test_schema_has_no_worker_runtime_role` | migrations apply; `pg_roles` lacks `worker_runtime`; `api_runtime` retains grants |
| 2.1 | unit | `test_dbrole_has_no_worker` | `DbRole`/`RedisRole`/`CheckMode` have no `worker`; exhaustive switch compiles |
| 2.2 | integration | `test_serve_boots_api_only` | `*_WORKER` unset → startup validation passes; serve connects |
| 2.3 | unit | `test_env_validation_api_only` | no `*Worker*` error reachable; api-only validation holds |
| 2.4 | unit | `test_doctor_no_worker_checks` | doctor output has no worker check ids |
| 3.1 | n/a | (CI) | workflow lint + deploy-dev dry run green with api-only env |
| 3.2 | n/a | (grep) | no `worker_runtime` in playbooks/teardown |
| 4.1 | integration | `test_lease_uses_new_consumer_group` | lease/report use the new group constant; old group drained |
| (regression) | integration | `test_lease_report_writes_as_api_runtime` | the full lease→report write set succeeds as `api_runtime` (grant-equivalence) |

---

## Acceptance Criteria

- [ ] `zombied serve` boots + `make test-integration` + `make memleak` pass with **only** `DATABASE_URL_API` + `REDIS_URL_API`
- [ ] Grep gate: zero `worker_runtime` / `DATABASE_URL_WORKER` / `REDIS_URL_WORKER` in `src/`, `schema/`, `docker-compose.yml`, `.github/workflows/`
- [ ] `pg_roles` after migration has no `worker_runtime`; `api_runtime` grants unchanged + sufficient (`test_lease_report_writes_as_api_runtime`)
- [ ] `make lint` clean; cross-compile clean (`zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux`)
- [ ] `zombie_workers` group migration documented (drain step) and tested
- [ ] Deploy sequence written: ship the api-only binary FIRST, then drop the PG role + Redis ACL user + `*_WORKER` Fly/1Password secrets
- [ ] `bash scripts/audit-spec-template.sh` clean

---

## Eval Commands (post-implementation)

```bash
# E1: api-only boot
DATABASE_URL_API=… REDIS_URL_API=… zombied serve   # boots with no *_WORKER set
# E2: build
zig build && zig build --build-file build_runner.zig 2>&1 | tail -3
# E3: tests
make test-integration 2>&1 | grep -E "api_runtime|PASS|FAIL"
# E4: lint
make lint 2>&1 | grep -E "✓|FAIL"
# E5: cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux 2>&1 | tail -3
# E6: gitleaks
gitleaks detect 2>&1 | tail -3
# E7: orphan sweep (empty = pass)
git grep -nE "worker_runtime|DATABASE_URL_WORKER|REDIS_URL_WORKER" -- src schema docker-compose.yml .github | head
```

---

## Dead Code Sweep

The `worker_runtime` Postgres role, the `worker` Redis ACL role, the `DATABASE_URL_WORKER`/`REDIS_URL_WORKER` env vars, the `DbRole.worker`/`RedisRole.worker` enum values, the **entire `CheckMode` enum + its `validateLoadedWithMode`/`enforceFromEnvWithMode`/`validateRoleSeparatedValues` machinery** (no non-test callers; collapses to a single api-only path), the `*Worker*` `EnvVarsErrors`, the `doctor` worker checks, and the `playbooks/006`/`007` worker bootstrap are **removed** (RULE NLR/NDC). The `.workers`/`api_http_workers` httpz thread-pool field is explicitly **out of scope** (it is the HTTP server's worker-thread count, not the deleted zombie worker). Orphan sweep (RULE ORP) is an acceptance criterion.

---

## Discovery (consult log)

- **Origin (Indy, May 27, 2026):** during the M80_002 docs/cleanup, Indy flagged `*_WORKER` vars as invalid and said the env vars must be nuked "since runners don't have access to db/redis." Investigation showed the worker DB/Redis role is dead post-cutover (`serve.zig` inits only the `.api` pool; the fleet write-path uses the api pool). An attempted in-PR rename `worker_runtime`→`runner_runtime` was discarded as the wrong direction (the runner holds zero datastore credentials → a `runner`-named DB role misleads); the correct action is removal, captured here as its own spec because collapsing two datastore security roles to one is a security-posture change that warrants an audited PR.
- **Scope boundary (Indy):** runner enrollment hardening → M80_005; runner identity persistence → M80_004/006; `AUTH_DEVICE_LOGIN.md` refresh → doc task; `.workers` httpz field → not residue, leave.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; grant-equivalence + api-only-boot tests present; iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned; special attention to the role-consolidation security posture. |
| After `gh pr create` | `/review-pr` | Comments addressed before human review/merge. |
| After every push | `kishore-babysit-prs` | greptile polled, walked, triaged, fixed, reported. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| api-only boot | `zombied serve` (no `*_WORKER`) | — | ⏳ |
| Integration | `make test-integration` | — | ⏳ |
| Memleak | `make memleak` | — | ⏳ |
| Lint | `make lint` | — | ⏳ |
| Cross-compile | `zig build -Dtarget={x86_64,aarch64}-linux` | — | ⏳ |
| Orphan sweep | grep `worker_runtime`/`*_WORKER` | — | ⏳ |
| Spec template | `bash scripts/audit-spec-template.sh` | — | ⏳ |

---

## Out of Scope

- **Runner enrollment hardening** — restricting *who* may register a runner (today any operator-minted `zmb_t_` key authenticates as admin and can enroll). → **M80_005** (trust authz).
- **Runner identity persistence** — the runner re-mints a `zrn_` on every restart (held in memory, never persisted), accumulating `fleet.runners` rows. → **M80_004 / M80_006**.
- **`AUTH_DEVICE_LOGIN.md` refresh** — the branch-only doc is stale (still documents the removed CLI polling). → a documentation task, not this milestone.
- **`.workers` / `api_http_workers`** (`serve.zig`) — the httpz HTTP thread-pool field; not worker-substrate residue, do not touch.
