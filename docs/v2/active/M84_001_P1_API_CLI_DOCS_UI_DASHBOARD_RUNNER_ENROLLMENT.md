# M84_001: Runner enrollment via dashboard — retire the admin-JWT CLI, add the fleet list + honest liveness

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 001
**Date:** Jun 03, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator credential-surface fix; removes the only CLI/shell use of the operator's full identity JWT, and makes the fleet observable from the dashboard.
**Categories:** API, CLI, DOCS, UI
**Batch:** B1 — §1 (CLI removal), §2 (fleet read + honest liveness), §3 (dashboard mint + list) ship together: removing the CLI before the UI just moves the admin JWT to `curl`, and the list needs an honest status to show.
**Branch:** feat/m84-dashboard-runner-enrollment
**Depends on:** none hard — `POST /v1/runners` (the mint primitive) already exists; `GET /v1/fleet/runners` is net-new here (the read half of the M80_006-deferred operator plane).
**Provenance:** agent-generated (Indy CTO consult, Jun 03 2026 — reverses the "leave it" in memory `project_runner_register_admin_token_intentional`; scope-expanded by Indy Jun 04 2026, see Discovery).

**Canonical architecture:** `docs/architecture/runner_fleet.md` (runner enrollment "Option B"; the operator plane; the M80_007→**M85_001** placement renumber) + `docs/architecture/roadmap.md` (the deferred operator plane / scheduler) + `docs/AUTH.md` (runner-token provisioning). This reconciles the implementation to the GitLab-16 "create runner → auth token" model those docs already describe, and builds the **read** half of the operator plane they reserved.

---

## Implementing agent — read these first

1. `docs/v2/done/M80_004_P1_API_CLI_RUNNER_OPERATOR_CLI.md` — the spec this **supersedes in part** (it added `register --token`); read §1 + Interfaces to know what to unwind.
2. `docs/v2/done/M80_006_P1_API_RUNNER_FLEET_PLANE.md` + `docs/architecture/roadmap.md` (the "Fleet operator plane + proactive reassignment" section) — the operator plane (`GET`/`PATCH /v1/fleet/runners`, cordon/revoke) was **carved out after a design study**. §2 builds **only the read** (`GET /v1/fleet/runners`); `PATCH`/cordon/revoke + heartbeat-lapse reassignment stay **deferred**.
3. `src/zombied/http/router.zig` (the route enum + `match`) + `route_matchers.zig` + `route_table_invoke.zig` + `auth/middleware/mod.zig` (the `platformAdmin` chain) — the seams a new `GET /v1/fleet/runners` route threads through; mirror the `register_runner` wiring.
4. `ui/packages/app/app/(dashboard)/settings/api-keys/` — **the prior-art to mirror**: `CreateApiKeyDialog.tsx` + its `RevealPanel` mint a credential and show it **once** (copy-to-clipboard, dismissal locked during reveal, raw value dropped from React state on close); `ApiKeyList.tsx` is the list. The runner surface is the same UX against runner endpoints.
5. `ui/packages/app/lib/workspace.ts` (`readWorkspaceClaim`) + `lib/actions/with-token.ts` + `lib/api/api_keys.ts` — the session-claim read, the server-action wrapper, and the API-client patterns to mirror for a platform-admin gate + runner endpoints.
6. `src/zombied/http/handlers/runner/register.zig` + `middleware/platform_admin.zig` — `POST /v1/runners`, the platform_admin-gated mint primitive. The **mint contract is unchanged**; §2 changes only what `register` stores for `last_seen_at` (`0` = never connected) so liveness is honest.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Runner enrollment via dashboard — retire the admin-JWT CLI, add the fleet list + honest liveness
- **Intent (one sentence):** Move runner enrollment to the GitHub/GitLab model — a platform admin mints a dedicated `zrn_` from a session-authed dashboard action (shown once), sees the fleet with an **honest** liveness state, and the runner CLI never takes an identity credential — by removing `register --token`, adding a read-only `GET /v1/fleet/runners`, and adding a platform-admin "Add runner" + list surface.
- **Handshake (done at PLAN):** intent restated above; `ASSUMPTIONS`: (1) `GET /v1/fleet/runners` is read-only here — cordon/revoke/reassignment are out; (2) liveness is **derived** at read (registered/online/busy/offline) from `last_seen_at` + live-lease join, the stored `status` auth-gate column is untouched; (3) mint sets `last_seen_at = 0` so a fresh runner reads **registered**, not a fake online; (4) tag/label routing is **authored as M85_001**, not implemented. A mismatch with the Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (remove the orphaned CLI minter + client fn; no dead code in the new handler), NLR (touch-it-fix-it), NLG (clean break, no deprecated-flag alias), ORP (orphan sweep across `src/`, `docs/`, `schema/`), UFS (`ENV_ZOMBIE_TOKEN` const dies with its caller; the offline-threshold + liveness-state strings are named consts shared verbatim Zig↔TS).
- **`docs/ZIG_RULES.md`** — §1 + §2 are `*.zig`; cross-compile both linux targets; ZLint `unused-decls`. New handler: PUB/LIFECYCLE/LENGTH gates; `conn.query()` needs `.drain()` (the list read), `conn.exec()` for no-rows.
- **`docs/AUTH.md`** — re-read before §1 + §2 (the live model is platform_admin-gated `POST /v1/runners`; `GET /v1/fleet/runners` is the same `platformAdmin()` gate — App-authorization Layer 1, never a Postgres GRANT).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — **APPLIES** to §2 (`GET /v1/fleet/runners` is a net-new route): pagination/sort shape, error envelope, no token material in the response.
- **UI (§3)** — design-system primitives + `theme.css` tokens (UI Substitution + DESIGN TOKEN gates); mirror the api-keys components; UFS for ui/ literals is manual (extract liveness-state union as-const).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes (§1 + §2) | cross-compile both targets; read ZIG_RULES. |
| PUB / Struct-Shape | yes | §1 removal (ZLint `unused-decls`); §2 new `GET` handler (own shape verdict, no inheritance). |
| File & Function Length | yes (§2) | new handler ≤350/file, fns ≤50; factor the liveness derivation + the SQL read into helpers. |
| LIFECYCLE | yes (§2) | the list read: `conn.query()` → `.drain()` before `release`; arena for row strings. |
| ERROR REGISTRY | yes | reuse `UZ-AUTH-021` (platform_admin gate); add the frontend `CODE_MAP` entry for it. No new `UZ-` code unless the list surfaces one. |
| UFS | yes (§1 + §2) | `ENV_ZOMBIE_TOKEN` removed with caller; liveness-state strings + offline-threshold are named consts (cross-runtime identical Zig↔TS). |
| UI Substitution / DESIGN TOKEN | yes (§3) | design-system primitives + theme tokens; mirror api-keys components. |
| SCHEMA | **no** | no migration: `status` column already exists; liveness is derived; `last_seen_at=0` is an app-level value (RULE STS — no static CHECK). |
| SPEC TEMPLATE | yes (§5) | M85_001 authored via `kishore-spec-new`. |
| LOGGING | yes (§2) | new handler log emits follow the logfmt envelope; never log a `zrn_`/`token_hash`. |

---

## Overview

**Goal (testable):** After this PR — (1) `zombie-runner --help` lists no `register` and no `--token`/`ZOMBIE_TOKEN` (`grep -rn "ZOMBIE_TOKEN\|--token" src/runner` → 0); (2) a platform admin mints a `zrn_` from a dashboard "Add runner" action (session-authed, revealed once, copyable) and a non-platform-admin cannot; (3) the dashboard lists the fleet via a new platform-admin `GET /v1/fleet/runners`, each runner showing an **honest** `liveness` (a freshly-minted runner reads **registered**, never a fake **online**); (4) `POST /v1/runners` + its gate are otherwise unchanged; (5) both build graphs + cross-compile pass; (6) `M85_001` (tag/label scheduler) is authored as PENDING and the stale `M80_007` placement references are corrected.

**Problem:** `zombie-runner register --token <admin-jwt>` is the one runner surface that takes the operator's full platform-admin identity credential on the CLI. Neither GitHub (registration token / JIT config) nor GitLab-16 (`glrt-` runner auth token) puts the human's identity token on the runner-config CLI — they mint a dedicated token from a platform call (UI/API) **and show the fleet's status**. Our `zrn_` is already that dedicated token; the mint mechanism drifted from the model, and the fleet has **no operator-facing list** — `register` even stores `status='active'` + `last_seen_at=now` at mint, so a never-connected runner looks live.

**Solution summary:** Remove the `register --token` CLI path (§1); add the read half of the operator plane — `GET /v1/fleet/runners` — plus an **honest derived liveness** and a mint that records `last_seen_at = 0` for "never connected" (§2); add a platform-admin dashboard "Add runner" + list surface mirroring api-keys (§3); reconcile the architecture docs + playbooks (§4); and author the tag/label scheduler as `M85_001` (§5). The operator installs the once-revealed `zrn_` into the host's vault/`ZOMBIE_RUNNER_TOKEN`. No identity credential ever reaches a shell. Cordon/revoke (`PATCH`) + heartbeat-lapse reassignment stay **deferred** (the deep operator-plane design study).

---

## Prior-Art / Reference Implementations

- **UI (§3)** → `ui/packages/app/app/(dashboard)/settings/api-keys/{page,actions}.ts(x)` + `components/{CreateApiKeyDialog,ApiKeyList,RevokeConfirm}.tsx` — create-list + **reveal-once**, mirrored for runners (mint → `RevealPanel` shows `zrn_` once → copy → list with liveness badges). Divergence: runners are **platform-scoped, not tenant-scoped**, so the surface is platform-admin-gated (the first such surface in the app).
- **API (§2)** → `src/zombied/http/handlers/runner/{register,self}.zig` + `router.zig`/`route_matchers.zig`/`route_table_invoke.zig` + `auth/middleware/{mod,platform_admin}.zig` — mirror `register_runner`'s route wiring + `platformAdmin()` chain for a new `GET /v1/fleet/runners`; mirror `self.zig`'s row→protocol mapping for the list rows.
- **CLI (§1)** → `docs/CLI_DX_PILLARS.md`; the surviving `status`/`doctor`/help already conform; §1 is a removal.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/cmd/register.zig` | DELETE | The CLI minter — its only job was the admin-JWT `POST /v1/runners` call. |
| `src/runner/cmd/registry.zig` | EDIT | Drop `register` from the `Command` enum + spec table (removes its help row). |
| `src/runner/cmd/help.zig` | EDIT | Remove `--token` + `--host-id` (Flags, both register-only) + `ZOMBIE_TOKEN` (Environment); reword `RUNNER_HOST_ID` annotation `(register)`→`(daemon)` (the daemon reads it — `config.zig:47`). |
| `src/runner/cmd/testdata/help.txt` | EDIT | Regenerate the byte-exact golden. |
| `src/runner/cmd/status.zig` | EDIT | Reword the `zombie-runner register` suggestion string (`:38`) → "have a platform admin mint one from the dashboard" (ORP — no live reference to a removed command). |
| `src/runner/daemon/config.zig` | EDIT | Remove `ENV_ZOMBIE_TOKEN` + its comment. Keep `ENV_RUNNER_HOST_ID` (daemon-live). |
| `src/runner/daemon/control_plane_client.zig` | EDIT | Remove the `register` client fn + `RegisterResult`; keep lease/heartbeat/renew/report/activity/getSelf. |
| `src/zombied/http/handlers/runner/register.zig` | EDIT | Mint stores `last_seen_at = 0` (sentinel "never connected") — `created_at`/`updated_at` stay `now`. Mint contract (request/response) unchanged. |
| `src/zombied/http/handlers/fleet/runners_list.zig` | CREATE | `GET /v1/fleet/runners` handler: platform-admin-gated, paginated read; per-row **derived liveness**; never returns `token_hash`. |
| `src/zombied/http/router.zig` + `route_matchers.zig` + `route_table_invoke.zig` | EDIT | Register the `GET /v1/fleet/runners` route (new enum variant + matcher + invoke), `platformAdmin()` chain. |
| `src/zombied/auth/middleware/mod.zig` | EDIT (if needed) | Attach the `platformAdmin` chain to the new route (mirror `register_runner`). |
| `src/lib/contract/protocol.zig` | EDIT | Add `RunnerLiveness` enum + `RunnersListResponse`/`RunnerListItem` (no `token_hash`); `RUNNER_LAST_SEEN_NEVER = 0`; offline threshold const. |
| `src/zombied/http/runner_register_integration_test.zig` | EDIT | Rework: assert mint (+`last_seen_at=0`) + 403 gate against `POST /v1/runners` directly (no CLI spawn); add `GET /v1/fleet/runners` authz + liveness assertions. |
| `ui/packages/app/lib/auth/platform.ts` | CREATE | `readPlatformAdminClaim()` (mirror `lib/workspace.ts`). |
| `ui/packages/app/lib/api/runners.ts` | CREATE | Types (`RunnerListItem`, `CreatedRunner`, `RunnerLiveness`) + `createRunner` (POST /v1/runners) + `listRunners` (GET /v1/fleet/runners). |
| `ui/packages/app/lib/errors.ts` | EDIT | Add `CODE_MAP` entry for `UZ-AUTH-021` (platform-admin required). |
| `ui/packages/app/app/(dashboard)/admin/runners/{page.tsx,actions.ts,components/*}` | CREATE | Platform-admin "Add runner" + list: page guard (`readPlatformAdminClaim` → redirect), `AddRunnerDialog` (mint + reveal-once, mirror `CreateApiKeyDialog`), `RunnerList` (liveness badges, mirror `ApiKeyList`), server actions (mint gated + 403; list). |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | Add a platform-admin-only "Runners" nav item (hidden for non-admins). |
| `docs/architecture/runner_fleet.md` | EDIT | Enrollment = dashboard mint (not `register --token`); add the honest liveness model + the read-only operator-plane list; **fix the stale `M80_007` placement refs → `M85_001`**. |
| `docs/architecture/roadmap.md` | EDIT | Note the operator-plane **read** lands here; placement/scheduler = `M85_001` (M80_007 is taken by observability). |
| `docs/AUTH.md` | EDIT | Enrollment via dashboard/API; `GET /v1/fleet/runners` is `platformAdmin()`-gated (Layer-1 authz, not a DB GRANT). |
| `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` + `07_runner_bootstrap_prod/001_playbook.md` | EDIT (light) | Operator mints via dashboard; the playbook already only *installs* the `zrn_`. |
| `docs/v2/pending/M85_001_*_SCHEDULER_*.md` | CREATE | The tag/label placement spec (authored via `kishore-spec-new`, PENDING). |

> The platform-admin route placement (`(dashboard)/admin/runners`) + the app's server-side `platform_admin` claim check are the §3 design calls (mirror the app's existing session-auth + `readWorkspaceClaim`). Done-spec `M80_004` stays frozen — superseded, not edited.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, five Sections (§1 CLI removal, §2 fleet read + honest liveness, §3 dashboard mint + list, §4 doc reconciliation, §5 author M85_001) — merged into one PR per Indy (Jun 04 2026): the list needs a backend read and an honest status, and the docs must not go stale.
- **Alternatives considered:** (a) mint-only UI, list deferred — **rejected by Indy** (wants Option B / the list now). (b) invent `GET /v1/runners` for the list — **rejected**: the architecture already designed `GET /v1/fleet/runners` (operator plane); use it. (c) tag-routing in this PR — **rejected**: it reverses the documented "no scheduler" non-goal and reaches the zombie/job model → its own spec (M85_001), authored not built. (d) build cordon/revoke + reassignment — **out**: the deep operator-plane design study, deferred.
- **Patch-vs-refactor verdict:** **patch + small feature** — Zig removal + one net-new read endpoint (mirroring existing route wiring) + a UI surface mirroring an existing pattern + doc reconciliation. No change to the lease/scheduler core.

---

## Sections (implementation slices)

### §1 — Remove the `register` subcommand + `--token`/`ZOMBIE_TOKEN`

Delete the CLI minting path so the runner never accepts an identity credential. The `Command` enum drop cascades to dispatch + the help golden; the config constant + client fn go with their sole caller; the dead `--host-id` flag goes (the daemon reads `RUNNER_HOST_ID` env directly). **Invariant:** the daemon + `status`/`doctor` (all on `ZOMBIE_RUNNER_TOKEN`) and `POST /v1/runners` + its gate are untouched.

- **Dimension 1.1** — ✅ DONE — `register` gone; running it exits non-zero with unknown-command help → Test `cli rejects the removed register subcommand with unknown-command exit` (registry.zig).
- **Dimension 1.2** — ✅ DONE — `--help` golden + src have no `register`/`--token`/`ZOMBIE_TOKEN`/`--host-id` flag; the live `zombie-runner register` suggestion string in `status.zig` is reworded → Test `help carries no enrollment-token surface` (help.zig) + the byte-exact golden.
- **Dimension 1.3** — ✅ DONE — `control_plane_client.register` + `RegisterResult` removed; ZLint clean (0/0 across 382 files); both graphs build + cross-compile (x86_64-linux, aarch64-linux) green.

### §2 — `GET /v1/fleet/runners` (read-only operator plane) + honest derived liveness

Build the **read** half of the operator plane: a platform-admin-gated `GET /v1/fleet/runners` returning the fleet, each row carrying a **derived** `liveness` computed server-side from `last_seen_at` + a live-lease join — never the stored `status` (the auth-gate column) and never `token_hash`. Mint (`register.zig`) records `last_seen_at = 0` so a never-connected runner is honestly **registered**. **Invariant:** the mint request/response contract, `POST /v1/runners`'s `platform_admin` gate, and the runner-auth lookup (`status='active'`) are unchanged; `PATCH`/cordon/revoke + reassignment are out.

Liveness derivation (single-sourced consts, Zig↔TS verbatim):
- `last_seen_at == RUNNER_LAST_SEEN_NEVER (0)` → **registered**
- `now - last_seen_at > RUNNER_OFFLINE_AFTER_MS` → **offline**
- else holds ≥1 live lease (`fleet.runner_leases.lease_expires_at > now`) → **busy**
- else → **online**

- **Dimension 2.1** — ✅ DONE — mint stores `last_seen_at = 0`; a never-heartbeated runner derives `registered` → Tests `register: the mint records last_seen_at = 0` (integration) + `deriveLiveness: never-seen sentinel is registered` (unit). Passed in `make test-integration`.
- **Dimension 2.2** — ✅ DONE — `GET /v1/fleet/runners` returns rows with derived liveness (all four states unit-tested across varied `last_seen_at` + lease inputs) and **no `token_hash`/`zrn_`** → Tests `deriveLiveness: …` (unit ×3) + `fleet list: a platform_admin JWT lists the fleet with derived liveness (200)` (integration). Passed.
- **Dimension 2.3** — ✅ DONE — `GET /v1/fleet/runners` is `platformAdmin()`-gated: platform-admin → 200; tenant admin JWT / `zmb_t_` → 403 `UZ-AUTH-021` → Tests `fleet list: a tenant-admin JWT is rejected 403` + `fleet list: a zmb_t_ api_key is rejected 403` (integration). Passed.

### §3 — Dashboard "Add runner" + fleet list (platform-admin, reveal-once)

A platform-admin-gated dashboard surface (`(dashboard)/admin/runners`) lists the fleet (liveness badges) and mints a `zrn_` via "Add runner": a server action calls session-authed `POST /v1/runners`, the result is revealed **once** (copy-to-clipboard, dismissal locked, raw value dropped on close) — mirroring `CreateApiKeyDialog`; the list mirrors `ApiKeyList`. **Implementation default:** mirror the api-keys components, swapping endpoints + adding the platform-admin gate (page-guard via `readPlatformAdminClaim` redirect, server-action early 403, nav item hidden for non-admins). **Invariant:** a non-platform-admin cannot mint or list (server 403; the UI does not render the surface).

- **Dimension 3.1** — a platform admin mints a `zrn_` revealed exactly once (re-open does not re-reveal; raw value leaves the DOM on close) → Test `dashboard mints zrn_ and reveals once` (e2e).
- **Dimension 3.2** — the list renders fleet rows with liveness badges; a freshly minted runner shows **registered** → Test `dashboard lists fleet with liveness` (e2e/component).
- **Dimension 3.3** — a non-platform-admin cannot reach/mint/list (server 403; surface + nav hidden) → Test `runner surface is platform-admin-gated`.

### §4 — Reconcile enrollment + operator-plane docs + playbooks

`runner_fleet.md`/`roadmap.md`/`AUTH.md`/the bootstrap playbooks describe: minting via the dashboard/API (not the removed CLI), the honest derived liveness, the **read-only** operator-plane list that now exists, and the corrected placement number (`M80_007`→`M85_001`, since M80_007 is the shipped observability spec). **Invariant:** host-bootstrap steps unchanged; cordon/revoke/reassignment still described as deferred.

- **Dimension 4.1** — no live doc/playbook references `zombie-runner register`; placement points at `M85_001`, not the taken `M80_007` → Test `enrollment-doc sweep`.

### §5 — Author the tag/label scheduler spec (M85_001)

Author `M85_001` (SCHEDULER) via `kishore-spec-new`: runner advertises labels at enrollment (already stored); `zombied` matches `zombie.required_tags ⊆ runner.labels` server-side (runner does not re-send tags per `/leases` poll); reconcile the `runner_fleet.md` "no scheduler" non-goal as *until M85_001*. **PENDING only** — authored + committed in this PR, not implemented. **Invariant:** no scheduler code lands in M84_001.

- **Dimension 5.1** — `M85_001` exists in `docs/v2/pending/`, template-conformant, Status PENDING → Test `scheduler spec authored`.

---

## Interfaces

`POST /v1/runners` request/response (`platform_admin` auth, `zrn_<64-hex>` mint, returned once) is **unchanged** except the stored `last_seen_at = 0` at mint (a never-connected sentinel; no wire-shape change). §3 adds a server action wrapping it with the logged-in session.

**NEW — `GET /v1/fleet/runners`** (read-only operator plane): `platformAdmin()`-gated. Query: `page`, `page_size`, `sort` (mirror api-keys). Response: `{ items: RunnerListItem[], total, page, page_size }` where `RunnerListItem = { id, host_id, sandbox_tier, liveness, labels[], last_seen_at, created_at }` — **no `token_hash`, no stored `status`**. `liveness ∈ {registered, online, busy, offline}` (derived). Tenant admin JWT / `zmb_t_` → `403 UZ-AUTH-021`.

Removed internal surface: `Command.register`, `cmd/register.zig`, `control_plane_client.register` + `RegisterResult`, `config.ENV_ZOMBIE_TOKEN`, the `--token` + `--host-id` flags.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Operator runs old `register` | stale muscle-memory / script | unknown-command help to stderr, non-zero exit (existing dispatch). |
| Non-platform-admin opens the runners UI | wrong role | server 403 `UZ-AUTH-021` / redirect; the action + list hidden in nav; no `zrn_` minted, no fleet leaked. |
| Freshly minted runner shown as live | mint stored `last_seen_at=now` (the old bug) | fixed: mint stores `0` → derives **registered** until the first heartbeat. |
| Auth-failed runner expected on the list | wrong `zrn_` matches no row | by design unrepresentable on a row (identity *is* the token); surfaced in logs/observability, not a row liveness. A minted-but-never-seen row shows **registered**. |
| `GET /v1/fleet/runners` DB error | control-plane DB down | structured 5xx envelope; the list surfaces a retryable error; `conn` drained before release (no leak). |
| User dismisses reveal without copying | closed the dialog | the `zrn_` is unrecoverable (shown once, like api-keys) → re-mint; copy-state + dismissal-lock reduce the footgun. |
| Removed client fn still referenced | missed caller | runner build fails → restore + re-investigate; never `--no-verify`. |

---

## Invariants

1. **`POST /v1/runners` + its `platform_admin` gate unchanged** (mint request/response, the `status='active'` auth-lookup) — enforced by the reworked integration test (mint succeeds; tenant key → 403).
2. **No enrollment-token surface in the runner** — ZLint `unused-decls` + the §1.2 grep (`ZOMBIE_TOKEN`/`--token`/`ENV_ZOMBIE_TOKEN` → 0).
3. **Honest liveness** — mint stores `last_seen_at = 0`; a never-heartbeated runner derives **registered**; liveness never reflects the stored auth `status` and never exposes `token_hash` — enforced by §2.1/§2.2.
4. **`GET /v1/fleet/runners` is platform-admin-only** (Layer-1 authz claim, never a Postgres GRANT) — enforced by §2.3 + §3.3.
5. **Platform-admin-only dashboard surface** — server `platform_admin` check 403s; the UI does not render the action/list/nav for non-admins — enforced by §3.3.
6. **No compat shim / deprecated alias (NLG)** — clean removal; lint legacy-symbol guard + grep.
7. **No scheduler code (M85_001 authored only)** — placement/tag-matching is PENDING, not implemented; `runner_fleet.md` non-goal reconciled as *until M85_001*.
8. **Operator-plane mutation stays deferred** — no `PATCH /v1/fleet/runners`, cordon, revoke, or reassignment lands.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete → expected) |
|-----------|------|------|-------------------------------|
| 1.1 | unit | `runner cli rejects removed register` | dispatch `register` → unknown-command help on stderr, non-zero exit. |
| 1.2 | unit | `runner help has no enrollment-token surface` | `--help` golden + src grep for `register`/`--token`/`ZOMBIE_TOKEN`/`--host-id` → 0; `status.zig` suggestion reworded. |
| 1.3 | regression | `runner builds without register client` | both graphs + cross-targets green; ZLint clean. |
| 2.1 | unit/integration | `freshly minted runner is registered not online` | mint → row `last_seen_at == 0`; liveness derivation → `registered`. |
| 2.2 | unit | `fleet list derives liveness and hides token hash` | varied `last_seen_at`+lease fixtures → registered/online/busy/offline; response has no `token_hash`/stored `status`. |
| 2.3 | integration | `fleet list is platform-admin-gated` | platform-admin JWT → 200; tenant admin JWT + `zmb_t_` → 403 `UZ-AUTH-021`. |
| 3.1 | e2e | `dashboard mints zrn_ and reveals once` | platform admin → Add runner → `zrn_` shown once; re-open does not re-reveal. |
| 3.2 | e2e/component | `dashboard lists fleet with liveness` | list renders rows; freshly minted → **registered** badge. |
| 3.3 | integration | `runner surface is platform-admin-gated` | non-admin session → 403 / hidden; tenant key → 403. |
| 4.1 | regression | `enrollment-doc sweep` | `grep -rn "zombie-runner register" docs/ playbooks/` (live) → 0; no live `M80_007` placement ref. |
| 5.1 | regression | `scheduler spec authored` | `docs/v2/pending/M85_001_*` exists, template-conformant, `Status: PENDING`. |

**Regression:** the runner daemon suite + `test-auth` (platform_admin) + the existing observability/liveness suite stay green. **Idempotency:** N/A (each mint creates a distinct runner). **Branch coverage:** liveness derivation fed all four state inputs.

---

## Acceptance Criteria

- [ ] No enrollment-token surface — verify: `grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner` → 0
- [ ] `--help` golden updated, no `register` — verify: `zig build --build-file build_runner.zig test`
- [ ] Both graphs build + cross-compile clean — verify: `zig build && zig build --build-file build_runner.zig -Dtarget=x86_64-linux && zig build --build-file build_runner.zig -Dtarget=aarch64-linux`
- [ ] Mint + gate intact, mint stores `last_seen_at=0`, `GET /v1/fleet/runners` derives liveness + is platform-admin-gated — verify: `make test-integration` + `zig build test-auth`
- [ ] Dashboard mint reveal-once + list + platform-admin gate — verify: the app e2e lane for the runners surface (`make acceptance-e2e` or equivalent)
- [ ] UI lint + design-token clean — verify: `make lint-apps-ds-ctl`
- [ ] Docs/playbooks reconciled; placement = M85_001 (no live M80_007 placement ref) — verify: `grep -rn "zombie-runner register" docs/ playbooks/` (live) → 0
- [ ] `M85_001` authored, PENDING, template-conformant
- [ ] `gitleaks detect` clean · no file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: no enrollment-token surface in the runner
grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner | head && echo FAIL || echo PASS
# E2: both graphs + cross-compile
zig build && zig build --build-file build_runner.zig -Dtarget=x86_64-linux && zig build --build-file build_runner.zig -Dtarget=aarch64-linux && echo PASS || echo FAIL
# E3: runner tests (golden) + auth gate + endpoint mint/list/liveness
zig build --build-file build_runner.zig test && zig build test-auth && make test-integration 2>&1 | tail -3
# E4: UI lint
make lint-apps-ds-ctl 2>&1 | grep -E "PASS|FAIL"
# E5: enrollment-doc sweep (scoped to the runner CLI — `zombiectl login --token` is a different, live feature)
grep -rn "zombie-runner register" docs/ playbooks/ | grep -v 'docs/v2/done/'
# E6: placement renumber — no live M80_007 placement reference
grep -rn "M80_007" docs/architecture/ | grep -i "placement\|scheduler\|label"
# E7: scheduler spec authored
ls docs/v2/pending/M85_001_* && grep -m1 "Status:" docs/v2/pending/M85_001_*
# E8: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/runner/cmd/register.zig` | `test ! -f src/runner/cmd/register.zig` |

**2. Orphaned references — zero remaining.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `--token` / `ZOMBIE_TOKEN` / `ENV_ZOMBIE_TOKEN` | `grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner` | 0 |
| `control_plane_client.register` / `RegisterResult` | `grep -rn "\.register(\|RegisterResult" src/runner` | 0 |
| `--host-id` flag (dead after register removal) | `grep -rn "\-\-host-id" src/runner` | 0 |
| live `zombie-runner register` reference | `grep -rn "zombie-runner register" src/ docs/ playbooks/ \| grep -v v2/done` | 0 |

---

## Discovery (consult log)

- **CTO consult (Jun 03 2026)** — Indy questioned `register --token`; vs GitHub (registration token / JIT) + GitLab-16 (`glrt-` UI/API mint), both mint a *dedicated* token and never put the human's identity credential on the runner-config CLI. Decision: retire the CLI path + mint from the dashboard. Reverses memory `project_runner_register_admin_token_intentional`.
- **Scope expansion (Jun 04 2026)** — Indy, after a pictorial walkthrough of the current `register` flow and the enrollment/lifecycle model:
  > "Yes proceed with retire the admin JWT cli + dashboard mint + list (option b) + honest status lifecycle here."
  > "Agree to have a spec written for tag based routing and the spec is pushed as part of this PR itself (no separate worktree)."
  > "Update the runner_fleet.md and any other docs/architecture/**.md files as part of this discussion above (dont keep it stale). Ensure the md update are sent as part of this PR."
  This authorizes folding the backend list + honest liveness into this UI PR (overrides the split-security-features default), authoring `M85_001` in this PR, and reconciling the architecture docs here.
- **Architecture reconciliation (Jun 04 2026, grounded in the code)** — (1) the list is the already-designed `GET /v1/fleet/runners` operator-plane **read** (`roadmap.md`), not a new `GET /v1/runners`; `PATCH`/cordon/revoke + reassignment stay deferred. (2) honest liveness is **derived** (registered/online/busy/offline) from `last_seen_at` + a live-lease join; the stored `status` auth-gate column and the `cordoned`/`revoked` states (left unbuilt per `roadmap.md:41`) are untouched; mint sets `last_seen_at = 0`. (3) "auth failed" can't be a per-runner row state (identity *is* the token; a bad `zrn_` matches no row) — surfaced in logs, not the list. (4) tag/label placement was reserved as `M80_007`, but **`M80_007` is the shipped observability spec** — a real ID collision; the scheduler spec is **`M85_001`** and the stale `M80_007` placement refs are corrected here.
- **LENGTH GATE triage (§2)** — adding the `GET /v1/fleet/runners` route tipped two dispatch-registry files past the 350-line cap (`router.zig` 352, `route_table_invoke.zig` 355). Indy chose the **split** over an override:
  > Indy (2026-06-04): "Yeah. A. split"
  Resolution: extracted the `Route` union → `routes.zig` (router.zig re-exports it) and the runner+fleet invokes → `route_table_invoke_runner.zig` (re-exported, mirroring the existing `route_table_invoke_{api_keys,events,approvals}.zig` precedent). All four files now ≤350; `router_test` confirms matching behavior unchanged.
- **Skill chain outcomes / Deferrals** — populate during VERIFY/CHORE(close). Deferred-by-design (Indy-acked above, not agent-unilateral): `PATCH /v1/fleet/runners` cordon/revoke, heartbeat-lapse reassignment, stored `cordoned`/`revoked` + `UZ-RUN-009`, and the M85_001 *implementation*.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| Before CHORE(close) | `/write-unit-test` | Coverage vs Test Spec — esp. reveal-once + gating + liveness derivation (all 4 states) + endpoint-without-CLI. | Clean; count in Discovery. |
| Before CHORE(close) | `/review` | Adversarial diff review vs spec, ZIG_RULES, AUTH.md, REST guide, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |
| After every push | `kishore-babysit-prs` | Greptile poll/triage/reply loop. | Final report in Session Notes. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| No token surface | `grep -rn "ZOMBIE_TOKEN\|--token" src/runner` | {paste} | |
| Runner tests + golden | `zig build --build-file build_runner.zig test` | {paste} | |
| Endpoint mint/gate/list/liveness | `make test-integration` + `zig build test-auth` | {paste} | |
| Dashboard e2e | app e2e lane (runners) | {paste} | |
| UI lint | `make lint-apps-ds-ctl` | {paste} | |
| Doc sweep + placement renumber | `grep -rn "zombie-runner register" docs/ playbooks/` ; `grep -rn "M80_007" docs/architecture/` | {paste} | |

---

## Out of Scope

- **Tag/label-based job routing (the scheduler)** — authored as `M85_001` (PENDING) in this PR, **not implemented**. It reverses the `runner_fleet.md` "no scheduler" non-goal and reaches the zombie/job model; implementation is its own milestone.
- **`PATCH /v1/fleet/runners` — cordon / revoke / disable** — the **mutation** half of the operator plane; deferred with the design study (`roadmap.md`). Only the read lands here.
- **Heartbeat-lapse reassignment** — expiring a dead runner's affinity so its work re-leases; the deep eligibility-ruleset problem, deferred.
- **Stored `cordoned`/`revoked` statuses + `UZ-RUN-009`** — left unbuilt (design not foreclosed); liveness here is *derived*, not a stored lifecycle.
- **GitHub-style ephemeral / JIT runner tokens** — not adopted; the GitLab-16 direct-mint shape is the fit. Revisit only for open-fleet (mode C).
- **The `POST /v1/runners` contract + `platform_admin` gate** — unchanged (except the `last_seen_at=0` mint value); not re-litigated.
