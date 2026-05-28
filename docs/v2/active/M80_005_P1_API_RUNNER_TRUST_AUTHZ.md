# M80_005: Platform-admin gate for runner enrollment — only usezombie's operator may mint a runner

**Prototype:** v2.0.0
**Milestone:** M80
**Workstream:** 005
**Date:** May 27, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — multi-tenant secret confinement: today any tenant's API key can enroll a host into the shared fleet that receives every tenant's inline `secrets_map`. This gates who may add a host to that fleet down to usezombie's platform operator.
**Categories:** API
**Batch:** B1
**Branch:** feat/m80-005-runner-trust-authz
**Depends on:** M80_001 (register handler + `runnerBearer` + `fleet.runners`), M80_002 (assignment/fencing/reclaim cutover)
**Provenance:** agent-generated (Opus 4.7, May 27, 2026), then reshaped after a CEO review (SCOPE REDUCTION), an engineering review, and a Codex outside-voice pass on May 27, 2026.

> **Scope was reduced from the original draft.** The first M80_005 draft was "operator-assigned `trust_class` + `allowed_workspace_ids` + trust-gated placement." The reviews collapsed it: at an all-shared launch the cross-tenant exposure is a *who-may-enroll* problem, not a placement problem. The placement filter, trust_class, and the workspace allowlist are deferred to **M80_007 (scheduler)**, where a "required trust" data source actually lands. See *Decomposition* and *Out of Scope*. The original placement framing survives only in the roadmap line that this spec corrects.

> **Provenance is load-bearing.** LLM-drafted, security-boundary + auth-flow spec — the implementing agent reads `docs/AUTH.md` first and cross-checks the JWT verifier (`jwks.zig`), the middleware registry (`mod.zig`), and the runner daemon (`src/runner/`) before touching code.

**Canonical architecture:** `docs/architecture/runner_fleet.md` (the runner is the credential-free execution plane; enrollment is the trust decision) and `docs/AUTH.md` (principal model).

---

## Implementing agent — read these first

1. `docs/AUTH.md` — the principal model. Today `role` is **per-tenant** (Clerk metadata), and **every `zmb_t_` api_key authenticates as `.role=.admin`** (`tenant_api_key.zig`). There is no platform-operator principal. This spec adds one.
2. `src/zombied/auth/jwks.zig` — the OIDC verifier. `jwks.zig:138` hard-requires `alg=="RS256"` and `verifyRs256` checks the RSA signature before any claim is read. The new `platform_admin` claim inherits exactly this tamper-proofing (same trust as `role`/`tenant_id`).
3. `src/zombied/auth/middleware/mod.zig` — `MiddlewareRegistry`: pre-built policy chains + accessors (`admin()`, `runnerBearer()`, …). The new `platformAdmin()` chain is wired here. `require_role.zig` is the shape the new middleware mirrors.
4. `src/zombied/http/route_table.zig` — `register_runner => registry.admin()` is the line that re-gates to `platformAdmin()`.
5. `src/runner/main.zig` + `src/runner/daemon/config.zig` + `control_plane_client.zig` — the host daemon. It currently registers on startup with a bootstrap credential; Option B flips it to hold a pre-minted `zrn_`.
6. `playbooks/006_worker_bootstrap_dev/001_playbook.md` + `007_worker_bootstrap_prod` + `deploy/baremetal/deploy.sh` — the bootstrap writes a **worker-era datastore `.env`** (`DATABASE_URL_WORKER`, `REDIS_URL_WORKER`, `ENCRYPTION_MASTER_KEY`) that `deploy.sh` syncs to `/etc/default/zombie-runner`. The runner needs `ZOMBIE_API_URL` + `ZOMBIE_RUNNER_TOKEN` + `RUNNER_HOST_ID` and none of those — this is the live CI break.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** gate runner enrollment on a platform-admin principal; flip the host to hold a pre-minted `zrn_`
- **Intent (one sentence):** only usezombie's platform operator may mint a runner token, the host is configured with that scoped `zrn_` and never self-registers, and the deploy stops shipping the deleted worker's datastore env (and root vault key) to credential-free runner hosts.
- **Handshake (restated):** the cross-tenant secret-exposure surface at an all-shared launch is *enrollment*, not placement. Closing it = a platform-admin authorization on the one endpoint that mints `zrn_`. Trust-class/placement filtering is real but belongs with the scheduler (M80_007); building it now is dead code against a "required trust" source that does not exist. **ASSUMPTIONS I'M MAKING:** (1) launch fleet is 100% platform-owned shared hosts (`tenant_id=NULL`), so a shared runner serving all tenants is intended, not a leak; (2) platform-admin is granted by a manual Clerk `publicMetadata` flip, read from a verified JWT, fail-closed when absent; (3) `zmb_t_` api_keys can never be platform-admin (the api_key path never sets the claim). Correct these now or they ship.

---

## Applicable Rules

- **`docs/AUTH.md`** — credential-typed principal rules (auth-flow surface). The new claim rides the existing verifier; do not weaken `alg`/signature checks.
- **`docs/greptile-learnings/RULES.md`** — UFS (the env-var names, the claim name, the new error code single-sourced as named constants), NLG (pre-2.0: replace the register-on-startup flow in place; no compat shim, no `legacy_` framing).
- **`docs/ZIG_RULES.md`** — middleware + config are `*.zig` (tagged-union outcomes, cross-compile both linux targets, no leaks on error paths).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — `POST /v1/runners` keeps its shape; only its auth policy tightens. Error envelope for the platform-admin reject.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — middleware, config, daemon `*.zig` | cross-compile `x86_64-linux` + `aarch64-linux`; zero-leak on the reject + config-error paths |
| ERROR REGISTRY | yes — new `UZ-AUTH-*` platform-admin reject | declare in `error_registry.zig` + `error_entries.zig` before use; used-but-undeclared is blocking |
| UFS | yes — claim name, env-var names, error code | named constants single-sourced; the runner-token prefix already lives in `runner_bearer.zig` |
| PUB / Struct-Shape | yes — new `platform_admin` field on the principal + new middleware struct | shape verdict per changed struct; mirror `RequireRole` |
| LOGGING | yes — the enrollment-deny emit | logfmt with `error_code`, `user_id`; never the JWT or claim contents |
| LENGTH | watch — `mod.zig`, `route_table.zig` near caps | factor; do not squash |
| SPEC TEMPLATE | this file | conform to `docs/TEMPLATE.md` |

---

## Overview

**Goal (testable):** `POST /v1/runners` issues a `zrn_` only to a caller whose verified JWT carries `metadata.platform_admin == true`; a tenant admin's JWT and any `zmb_t_` api_key are rejected `403`; an absent claim fails closed — asserted by `test_register_requires_platform_admin`, `test_api_key_cannot_enroll_runner`, and `test_platform_admin_absent_claim_fails_closed`. The host daemon boots from a pre-minted `zrn_` with no register call — asserted by `test_runner_boots_from_env_token_without_register`.

**Problem:** Post-M80_002 a runner authenticates with a `zrn_` (`runnerBearer`) and `assign.select` hands it any active zombie's event, secrets inline. The only control over *which hosts join that fleet* is `POST /v1/runners`, gated by `registry.admin()`. But `admin` is per-tenant and **every `zmb_t_` api_key is `.role=.admin`** (`tenant_api_key.zig:110`), so **any tenant's API key can enroll a host into the shared fleet that receives every tenant's secrets.** Separately, the deploy layer never migrated off the deleted worker: `playbooks/006_worker_bootstrap_dev` writes a datastore `.env` (`DATABASE_URL_WORKER`, `REDIS_URL_WORKER`, `ENCRYPTION_MASTER_KEY` — the root vault KEK) that `deploy.sh` syncs to the runner host, which (a) breaks the runner with `MissingEnvVar` (it needs `ZOMBIE_API_URL`/`ZOMBIE_RUNNER_TOKEN`/`RUNNER_HOST_ID`) and (b) puts the root vault key on a host that must hold zero datastore credentials.

**Solution summary:** Introduce a **platform-admin** principal — a `metadata.platform_admin` Clerk claim, set by a manual `publicMetadata` flip on usezombie's operator user, read from the verified JWT (`bearer_oidc`), fail-closed when absent. A new `PlatformAdmin` middleware re-gates `POST /v1/runners` from `admin()` to `platformAdmin()`. The runner daemon stops self-registering: the platform operator pre-mints a `zrn_` via the gated endpoint and configures the host with it (`ZOMBIE_RUNNER_TOKEN=zrn_`); the daemon reads it from the env and goes straight to the lease loop (Option B / the GitLab-16 model), so no host holds an enrollment-grade credential. The bootstrap playbooks are migrated to write the runner env (three vars, zero datastore secrets), and `deploy.sh` validates them and fails loudly.

---

## Prior-Art / Reference Implementations

- **Middleware** → `src/zombied/auth/middleware/require_role.zig` — the exact shape `PlatformAdmin` mirrors (struct + `middleware()` + type-erased execute + 401-on-null-principal). Registry wiring follows `_admin_chain` / `admin()` in `mod.zig`.
- **Claim plumbing** → `bearer_oidc.zig` already lifts `role` + `tenant_id` from Clerk metadata into the principal; `platform_admin` is the same path.
- **Enrollment model** → GitLab 16 "create runner → authentication token" (operator pre-creates; the host holds only its scoped token; no open registration endpoint). The registration-token anti-pattern GitLab deprecated is exactly the god-credential-on-every-host this avoids.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/auth/principal.zig` | EDIT | add `platform_admin: bool = false` |
| `src/zombied/auth/claims.zig` | EDIT | parse `metadata.platform_admin` (absent ⇒ false) |
| `src/zombied/auth/middleware/bearer_oidc.zig` | EDIT | set `principal.platform_admin` from verified claims |
| `src/zombied/auth/middleware/platform_admin.zig` | CREATE | `PlatformAdmin` middleware: `platform_admin==true ? next : 403` |
| `src/zombied/auth/middleware/mod.zig` | EDIT | register the chain + `platformAdmin()` accessor |
| `src/zombied/http/route_table.zig` | EDIT | `register_runner`: `admin()` → `platformAdmin()` |
| `src/zombied/errors/error_registry.zig`, `error_entries.zig` | EDIT | new `UZ-AUTH-*` `platform_admin_required` (403) |
| `src/runner/main.zig` | EDIT | drop `registerWithRetry`; boot from env `zrn_` straight into the lease loop |
| `src/runner/daemon/config.zig` | EDIT | `register_token` → `runner_token`; validate `zrn_` prefix; fail loud |
| `src/runner/daemon/control_plane_client.zig` | EDIT | remove dead `register()` (RULE NLR) |
| `playbooks/006_worker_bootstrap_dev/001_playbook.md`, `007_worker_bootstrap_prod/001_playbook.md` | EDIT | write runner env (3 vars), drop all datastore vars incl. `ENCRYPTION_MASTER_KEY`; retire worker/ant naming |
| `deploy/baremetal/deploy.sh` | EDIT | `sync_env` validates required runner keys, fails loud |
| `.github/workflows/deploy-dev.yml` | EDIT | fix the comment listing 2 of 3 required vars |
| `playbooks/003_priming_infra/001_playbook.md` | EDIT | manual Clerk step: `publicMetadata.platform_admin` + claim on both templates |
| `docs/AUTH.md`, `docs/architecture/runner_fleet.md`, `data_flow.md`, `user_flow.md` | EDIT | reconcile to the new enrollment flow (see §5) |
| `docs/v2/pending/M80_004_*` | EDIT | its `zombie-runner enroll → register` self-enroll flow contradicts Option B; reconcile |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Option B (operator pre-creates, host holds only `zrn_`), one PR, API-only.
- **Alternatives considered:** (a) gate the open `register` but keep host self-registration — rejected: the host's bootstrap credential must then be platform-admin-grade, a god-credential on every box (the GitLab registration-token anti-pattern); (b) a static `PLATFORM_ADMIN_IDS` config allowlist — rejected by Indy in favor of a manual Clerk metadata flip (no redeploy to grant access; rides the existing claim trust); (c) build `trust_class` + `allowed_workspace_ids` + placement now — rejected: dead code against a non-existent "required trust" source; it is M80_007 scheduler scope.
- **Patch-vs-refactor verdict:** **additive auth refactor + an in-place daemon flip.** The verifier, registry, and route table gain one principal/middleware/policy; the daemon's register path is replaced in place (RULE NLR), not twinned.

---

## Sections (implementation slices)

### §1 — Platform-admin principal (Clerk claim, fail-closed)

The data: a `platform_admin` boolean on `AuthPrincipal`, parsed from the verified JWT's `metadata.platform_admin`, set by a manual Clerk `publicMetadata` flip on usezombie's operator user and added to **both** the customized session token and the api template (so it works from a dashboard session or a CLI-minted JWT). Absent ⇒ `false` — which also means custom-OIDC deployments have no platform admin via this path. The `zmb_t_` api_key path never sets it.

- **Dimension 1.1** — `metadata.platform_admin: true` on a verified JWT ⇒ `principal.platform_admin == true`; absent/`false`/non-bool ⇒ `false` → Test `test_claims_parse_platform_admin` → **DONE** (`ea2f33b8` — unit test green)
- **Dimension 1.2** — an `api_key` principal always has `platform_admin == false` (the api_key middleware never sets it) → Test `test_api_key_principal_never_platform_admin` → **DONE** (`ea2f33b8` — unit test green)

### §2 — Re-gate enrollment to platform-admin

The enforcement: a `PlatformAdmin` middleware (mirror of `RequireRole`) short-circuits `403 platform_admin_required` unless `principal.platform_admin == true`, and `401` when no principal ran (composition bug). `route_table.zig` swaps `register_runner` from `admin()` to a `platformAdmin()` chain `[bearer_or_api_key, PlatformAdmin]`.

- **Dimension 2.1** — a JWT with the claim mints a `zrn_` (201); a tenant-admin JWT without it → `403`; a `zmb_t_` api_key → `403` → Tests `test_register_requires_platform_admin`, `test_api_key_cannot_enroll_runner` → **IN_PROGRESS** (`ea2f33b8` — unit assertions on the middleware + route gating committed; the full register-handler integration assertion is pending Task 6)
- **Dimension 2.2** — absent claim fails closed (`403`), and a chain with no auth middleware before it `401`s rather than granting → Tests `test_platform_admin_absent_claim_fails_closed`, `test_platform_admin_null_principal_401` → **DONE** (`ea2f33b8` — both unit tests green)

### §3 — Runner daemon flip (Option B)

The host stops self-registering: `main.zig` drops `registerWithRetry` and uses the env token directly for the heartbeat/lease loop; `config.zig` renames `register_token` → `runner_token` and validates the `zrn_` prefix (fail loud, not a silent auth loop); `control_plane_client.register()` is removed as dead.

- **Dimension 3.1** — the daemon boots from `ZOMBIE_RUNNER_TOKEN=zrn_...` and reaches the lease loop with no register call → Test `test_runner_boots_from_env_token_without_register` → **IN_PROGRESS** (`c1ac7343` — `register()` dead-code removed + `registerWithRetry` gone + `runner_token` rename landed; the daemon-boot integration test that asserts "zero register calls during lease-loop entry" is pending Task 6)
- **Dimension 3.2** — a non-`zrn_` token (e.g. a stale `zmb_t_`) fails config load with a clear prefix error, not a downstream 401 loop → Test `test_runner_rejects_non_zrn_token_loudly` → **DONE** (`c1ac7343` — `assertRunnerTokenPrefix` unit test green, covers `zmb_t_` / empty / `zrn` prefix-only)

### §4 — Deploy + bootstrap migration (the CI fix)

The bootstrap playbooks write the runner env (`ZOMBIE_API_URL`, `ZOMBIE_RUNNER_TOKEN`, `RUNNER_HOST_ID`; optional `RUNNER_SANDBOX_TIER`/`RUNNER_LABELS`) and **drop every datastore var** (`DATABASE_URL_WORKER`, `REDIS_URL_WORKER`, `ENCRYPTION_MASTER_KEY`). `deploy.sh sync_env` validates the required keys are present and dies loudly otherwise. The `deploy-dev.yml` comment is corrected to list all three. Worker/ant naming residue is retired.

- **Dimension 4.1** — the dev + prod bootstrap `.env` contain exactly the runner vars and no datastore vars → Test `test_bootstrap_env_is_runner_only` (a grep-gate over the playbook bodies) → **DONE** (`c2305c19` — playbook §4.2 in both 006 + 007 rewritten; spec's E5 eval `grep -E "ENCRYPTION_MASTER_KEY|DATABASE_URL_WORKER|REDIS_URL_WORKER" playbooks/00[67]_*/001_playbook.md` returns no matches; all 3 runner vars present in each `.env` heredoc)
- **Dimension 4.2** — `deploy.sh` fails with a named error when a synced env is missing a required runner key → Test `test_deploy_sync_env_validates_required_keys` → **DONE** (`c2305c19` — `sync_env()` hardened: dies loudly when `/opt/zombie/.env` is absent, validates all 3 required keys are present in `/etc/default/zombie-runner`, and rejects the documented `zrn_FAKE_…` placeholder shape)

### §5 — Docs reconciliation

`AUTH.md` gains the platform-admin principal + the claim placement; `runner_fleet.md` "Registering a runner" is rewritten to operator-creates + host-holds-`zrn_` and its roadmap line 259 corrected; `data_flow.md` register row + `user_flow.md` §8.2.2 step 6 (which still cites the deleted `zombie:control` watcher) are reconciled; `M80_004` self-enroll flow is reconciled to Option B.

- **Dimension 5.1** — no architecture doc describes register-on-startup or `zombie:control`-watcher claim as current → Test `test_docs_no_stale_register_flow` (grep-gate) → **IN_PROGRESS** (`c2305c19` — `deploy/baremetal/zombie-runner.service` comment and `.github/workflows/deploy-dev.yml` line 254 comment reconciled to Option B; bootstrap playbooks 006 + 007 fully retired worker-era prose. **Pending:** `docs/AUTH.md` platform-admin principal addition, `docs/architecture/runner_fleet.md` "Registering a runner" rewrite + roadmap line 259, `docs/architecture/data_flow.md` register row, `docs/architecture/user_flow.md` §8.2.2 step 6 (`zombie:control` watcher), and `M80_004` self-enroll flow reconciliation.)

---

## Interfaces

```
AuthPrincipal (additive):
  platform_admin  bool = false   -- from verified JWT metadata.platform_admin; api_key path never sets it

POST /v1/runners (register/create) — auth policy ONLY changes:
  middleware: registry.admin()  ->  registry.platformAdmin()  ([bearer_or_api_key, PlatformAdmin])
  body (RegisterRequest) UNCHANGED -- frozen wire contract
  caller: usezombie platform operator's Clerk JWT carrying metadata.platform_admin=true
  -> 201 { runner_id, runner_token: "zrn_..." }  (shown once)
  -> 403 platform_admin_required  (tenant admin, api_key, or absent claim)

Runner host (Option B):
  ZOMBIE_RUNNER_TOKEN = zrn_...   -- pre-minted by the operator, NOT a bootstrap zmb_t_/JWT
  daemon: NO register call; validate zrn_ prefix; heartbeat/lease/report/activity loop

errors (new): UZ-AUTH-0xx platform_admin_required (403)   -- exact number assigned from the registry
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Tenant admin / api_key tries to enroll | per-tenant admin or any `zmb_t_` | `403 platform_admin_required`; no `zrn_` minted → `test_register_requires_platform_admin`, `test_api_key_cannot_enroll_runner` |
| Platform-admin claim absent | operator hasn't flipped Clerk metadata, or custom-OIDC | fail-closed `403`; nobody enrolls until the claim is set → `test_platform_admin_absent_claim_fails_closed` |
| Forged/tampered claim | attacker edits JWT payload | signature check fails at `jwks.zig` (`alg=RS256` + RSA verify) → `401`; claim is as unforgeable as `role`/`tenant_id` |
| Host has a stale `zmb_t_` in `ZOMBIE_RUNNER_TOKEN` | old bootstrap env after the flip | config load fails loud with a `zrn_`-prefix error, not a silent 401 loop → `test_runner_rejects_non_zrn_token_loudly` |
| Synced `.env` missing a runner var | worker-era / incomplete bootstrap | `deploy.sh` dies with a named missing-key error before restart → `test_deploy_sync_env_validates_required_keys` |
| Stolen valid `zrn_` replayed | token theft from a host | **residual, not closed here** — a copied `zrn_` joins the shared fleet and reads inline secrets until expiry/revoke. Revocation/rotation is M80_006; scoped/proxy secrets is the zero-trust future. Documented in the threat model. |

---

## Invariants

1. A `zrn_` is minted only for a verified JWT carrying `metadata.platform_admin==true` — enforced by `platformAdmin()` on the route + `test_register_requires_platform_admin`.
2. A `zmb_t_` api_key can never enroll a runner — enforced by the api_key path never setting `platform_admin` + `test_api_key_cannot_enroll_runner`.
3. Absent claim ⇒ no enrollment (fail-closed) — enforced in the middleware + `test_platform_admin_absent_claim_fails_closed`.
4. No runner host holds a datastore credential or the vault KEK — enforced by the bootstrap-env grep-gate `test_bootstrap_env_is_runner_only` (closes the `ENCRYPTION_MASTER_KEY`-on-runner violation).
5. The host never self-registers; identity is a pre-minted `zrn_` from the env — enforced by `test_runner_boots_from_env_token_without_register` + the removed `register()`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_claims_parse_platform_admin` | claims `{metadata:{platform_admin:true}}` → principal true; absent/false/non-bool → false |
| 1.2 | unit | `test_api_key_principal_never_platform_admin` | api_key middleware output has `platform_admin==false` always |
| 2.1 | unit+integration | `test_register_requires_platform_admin` / `test_api_key_cannot_enroll_runner` | claim JWT → 201 + `zrn_`; tenant-admin JWT → 403; `zmb_t_` → 403 |
| 2.2 | unit | `test_platform_admin_absent_claim_fails_closed` / `test_platform_admin_null_principal_401` | absent claim → 403; null principal → 401 (no grant) |
| 3.1 | integration | `test_runner_boots_from_env_token_without_register` | daemon with env `zrn_` reaches lease loop; zero register calls |
| 3.2 | unit | `test_runner_rejects_non_zrn_token_loudly` | `ZOMBIE_RUNNER_TOKEN=zmb_t_...` → config error naming the prefix |
| 4.1 | unit | `test_bootstrap_env_is_runner_only` | playbook `.env` body has the 3 runner vars, none of the datastore vars |
| 4.2 | unit | `test_deploy_sync_env_validates_required_keys` | `deploy.sh` exits non-zero with a named error on a missing key |
| 5.1 | unit | `test_docs_no_stale_register_flow` | grep finds no "register on startup" / `zombie:control` watcher as current |

Regression: M80_001's `runner_bearer` tests + M80_002's lease/fence tests stay green (this changes who may *create* a runner, not how `zrn_` authenticates per-call). Replay: N/A.

---

## Acceptance Criteria

- [ ] Only a platform-admin JWT mints a `zrn_` — verify: `test_register_requires_platform_admin`
- [ ] No api_key can enroll — verify: `test_api_key_cannot_enroll_runner`
- [ ] Absent claim fails closed — verify: `test_platform_admin_absent_claim_fails_closed`
- [ ] Host boots from env `zrn_`, no register — verify: `test_runner_boots_from_env_token_without_register`
- [ ] Runner bootstrap env carries no datastore var / KEK — verify: `test_bootstrap_env_is_runner_only`
- [ ] `make lint` clean · `make test` passes · `make test-integration` passes
- [ ] cross-compile both linux targets · `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: enrollment authz — make test-integration 2>&1 | grep -E "platform_admin|cannot_enroll|PASS|FAIL"
# E2: Build  — zig build && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux
# E3: Tests  — make test && make test-integration
# E4: Lint   — make lint 2>&1 | grep -E "ok|FAIL"
# E5: Bootstrap env audit — grep -E "ENCRYPTION_MASTER_KEY|DATABASE_URL_WORKER|REDIS_URL_WORKER" playbooks/00[67]_*/001_playbook.md  (expect: no matches)
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

`control_plane_client.register()` and the daemon's `registerWithRetry` are removed in place (RULE NLR) — Option B has no self-register path. `config.register_token` is renamed (not twinned). No `_v2` shim (pre-2.0, RULE NLG).

---

## Discovery (consult log)

- **Provenance + reshape (May 27, 2026):** authored as trust_class/placement, then reduced by the review chain to the platform-admin enrollment gate. CEO review ran SCOPE REDUCTION; eng review locked the auth mechanism; Codex outside-voice surfaced the token-shape gap (claim must be on both Clerk templates), the recovery/rotation runbook need, the `ZOMBIE_RUNNER_TOKEN` prefix-validation footgun, and the M80_004 dead-path drift — all folded in.
- **Indy-acked decisions (deferral + scope), May 27, 2026:**
  > Indy: "what would you recommend as CTO and why? i thinking simplicity" — context: chose the simplest correct slice; trust_class ordering deferred.
  > Indy: "Yes i prefer Option B" — context: operator pre-creates, host holds only `zrn_`.
  > Indy: "One spec, one PR" — context: server gate + host flip ship together.
  > Indy: "I am thinking now i dont need CLI, just api is fine." — context: API-only; no `zombiectl` runner command; UI → M80_006.
  > Indy: "Well i wont be looking at static ids, where some sort of manual update in clerk so the user gets elevated access" — context: platform-admin via manual Clerk metadata, not a config allowlist.
  > Indy: "I want to got for 1" — context: `platform_admin` claim on both session + api templates, fail-closed.
  > Indy: "yes start" — context: accepted the reduced scope (trust_class + `allowed_workspace_ids` + trust-gated placement deferred to M80_007) and authorized implementation.
- **Task 3.5 — restore `test-auth` portability (May 28, 2026, Indy-directed scope add beyond original spec):** Indy directed two cuts at the long-red `test-auth` portability gate. (a) **Session-store relocation (Option A):** moved 7 files (`session_store_redis*.zig` + `session_verify_consume.lua`) from `src/zombied/auth/` to a new `src/zombied/session/` peer; `session_state.zig` stayed pure/std-only in `auth/`. The store is auth-adjacent infrastructure (pulls httpz/pg/redis surface into the gate's link set) but not auth logic. (b) **`auth_codes` mirror leaf (R3 guarded mirror):** the only two remaining escapes — `middleware/errors.zig` + `middleware/tenant_api_key.zig` `@import("../../errors/error_registry.zig")` — couldn't be re-pointed at `error_registry` without crossing the gate's module boundary. Initial framing was "auth_codes leaf, re-export"; verification of `audit-error-codes.sh` showed it discovers DECLARED codes by literal-grep of `error_registry.zig` only, so re-exporting would move 14 literals out and trip 14 blocking orphans. R3 ships a **mirror leaf** (`src/zombied/errors/auth_codes.zig`, 13 string consts, wired as a named module in `build.zig` → exe/tests/test_auth) with `error_registry`'s literals untouched + a `comptime` block walking 13 pairs asserting byte-equality. Mirrors the existing `runner/engine/client_errors.zig` pattern; strictly better because drift is a compile error rather than a discipline risk. `test-auth` is now green and wired into `.githooks/pre-commit` as a parallel lane that fires on any staged `*.zig` / `schema/*.sql`. Indy-acked:
  > Indy (May 28, 2026): "R3 guarded mirror" — selected when the re-export framing was shown to break the ERROR REGISTRY GATE audit.
  > Indy (May 28, 2026): "auth_codes leaf" — selected over the 68-file `error_registry`→named-module cascade.
- **Task 4 — bootstrap rewrite scope (May 28, 2026, Indy-directed scope add beyond the original CI-comment fix):** Indy directed a combination of F2 (fold in the bootstrap script + 1Password entry shape) and F1 (flip the gate vars), with the operational flip done by Indy out of diff. What landed in code: full §3.0 + §4.0 + §5.0 + "What CI does" + Sequence Summary rewrite for both `006_worker_bootstrap_dev` and `007_worker_bootstrap_prod` (drop worker+executor binaries + the worker-era datastore `.env` including the root vault Key Encryption Key (KEK); replace with the three runner env vars). New `04_provision_runner_env.sh` idempotent one-shot. Header normalization on both 006 + 007 — dropped the spec-template frontmatter (`Milestone:`/`Workstream:`/`M{N}_{NNN}:` title prefix) that predated the spec-vs-playbook split, matching the modern playbook shape (010/011/013/014). `runner-token` check_ref added to `01_ssh_access.sh` (per-host inventory) + `02_credentials.sh` (platform-wide preflight, both dev + prod ant). `deploy.sh sync_env()` hardened: dies loudly on absent source, validates the three keys are present, and rejects the documented `zrn_FAKE_…` placeholder. Indy-acked:
  > Indy (May 28, 2026): "I would like to do a combination of 2 and 1. Meaning keep everything ready with a fake runner_token in 1Password. And i will move dev_worker_ready and prod_worker_Ready to false manually" — context: the F1+F2 scope split.
  > Indy (May 28, 2026): "I have add the runner-token in the vault. the runner-token is fake, its gated as worker-dev-ready as false in github" — context: confirmed the placeholder + GH variable flip; verified via `gh variable list` showing both `DEV_WORKER_READY=false` and `PROD_WORKER_READY=false`.
- **Operational state (out of diff):** `DEV_WORKER_READY=false`, `PROD_WORKER_READY=false`. 1Password entries `op://ZMB_CD_{DEV,PROD}/zombie-{dev,prod}-worker-ant/runner-token` hold a placeholder `zrn_…`. The gates stay false until a real admin-minted `zrn_` (via this PR's `POST /v1/runners` enrollment gate on a live control plane) replaces the placeholders.
- **Deferred to follow-up (captured for next spec):** (a) directory rename `playbooks/006_worker_bootstrap_dev/` → `runner_bootstrap_dev/` (touches refs across `.github/workflows/deploy-dev.yml`, the playbook scripts, and the docs); (b) broader playbook header normalization for `001_bootstrap`, `002_preflight`, `003_priming_infra`, `004_deploy_dev`, `005_deploy_prod`, `008_credential_rotation_dev`, `009_grafana_observability`, `012_usezombie_admin_bootstrap` (all still carry the spec-template frontmatter); (c) formal test wrappers for the §4 grep-gate (E5) and `deploy.sh sync_env` validation (a bats-style or shell test); (d) Task 6 — `2.1` register-handler integration test + `3.1` daemon-boot integration test.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits coverage vs this Test Specification (esp. the fail-closed + api_key-reject arms) | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial review vs AUTH.md, the cross-tenant + KEK-on-host threats, REST guide | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration (authz) | `make test-integration` | {paste at VERIFY} | |
| Lint | `make lint` | {paste at VERIFY} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` / `aarch64-linux` | {paste at VERIFY} | |
| Bootstrap env audit | E5 above | {paste at VERIFY} | |

---

## Out of Scope

- **`trust_class` + `allowed_workspace_ids` + trust-gated placement** — the eligibility filter in `assign.select`. Deferred to **M80_007 (scheduler)**, where a "required trust" data source lands. At an all-shared launch there is nothing to filter. (Indy-acked above.)
- **Tags/labels matching** (capability routing) — M80_007; self-reported, not a trust mechanism.
- **Management UI** (create/list/revoke runners in the dashboard) — M80_006, behind the same platform-admin gate.
- **BYO tenant-scoped runners** — the `fleet.runners.tenant_id` scope + per-tenant placement; later, design room preserved by the existing nullable column.
- **`zrn_` revocation / rotation / heartbeat reassignment** — M80_006 (and the residual stolen-`zrn_` replay window noted in Failure Modes).
- **Zero-trust scoped/proxied secret delivery** — beyond the trusted-fleet model.
