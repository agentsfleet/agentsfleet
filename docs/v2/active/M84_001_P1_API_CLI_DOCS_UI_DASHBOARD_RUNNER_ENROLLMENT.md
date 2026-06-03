# M84_001: Runner enrollment via dashboard — retire the admin-JWT register CLI

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 001
**Date:** Jun 03, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator credential-surface fix; removes the only CLI/shell use of the operator's full identity JWT.
**Categories:** API, CLI, DOCS, UI
**Batch:** B1 — the CLI removal (§1) and the dashboard mint (§2) ship together (removing the CLI before the UI just moves the admin JWT to `curl`).
**Branch:** feat/m84-dashboard-runner-enrollment
**Depends on:** none hard — `POST /v1/runners` (the mint primitive) already exists.
**Provenance:** agent-generated (Indy CTO consult, Jun 03 2026 — reverses the "leave it" in memory `project_runner_register_admin_token_intentional`).

**Canonical architecture:** `docs/architecture/runner_fleet.md` (runner enrollment, "Option B") + `docs/AUTH.md` (runner-token provisioning). This reconciles the implementation to the GitLab-16 "create runner → auth token" model those docs already describe.

---

## Implementing agent — read these first

1. `docs/v2/done/M80_004_P1_API_CLI_RUNNER_OPERATOR_CLI.md` — the spec this **supersedes in part** (it added `register --token`); read §1 + Interfaces to know what to unwind.
2. `src/runner/cmd/registry.zig` + `cmd/help.zig` — the typed `Command` enum → `commandSpec` table driving dispatch + the byte-exact `--help` golden; dropping `register` cascades to both.
3. `ui/packages/app/app/(dashboard)/settings/api-keys/` — **the prior-art to mirror**: `CreateApiKeyDialog.tsx` + its `RevealPanel` mint a credential and show it **once** (copy-to-clipboard, dismissal locked during reveal, raw value dropped from React state on close). The runner mint is the same UX against a different endpoint.
4. `docs/architecture/runner_fleet.md` + `playbooks/founding/0{6,7}_runner_bootstrap_*` — the host installs the `zrn_` from vault and never self-registers; confirms removing the CLI strands nothing on the host.
5. `src/zombied/http/handlers/runner/register.zig` + `middleware/platform_admin.zig` — `POST /v1/runners`, the platform_admin-gated mint primitive, **unchanged**.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Runner enrollment via dashboard; retire the admin-JWT register CLI
- **Intent (one sentence):** Move runner enrollment to the GitHub/GitLab model — a platform admin mints a dedicated `zrn_` from a session-authed dashboard action (shown once) and the runner CLI never takes an identity credential — by removing `register --token` and adding a platform-admin "Add runner" surface.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; a mismatch with the Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (remove the orphaned CLI minter + client fn), NLR (touch-it-fix-it), NLG (clean break, no deprecated-flag alias), ORP (orphan sweep), UFS (`ENV_ZOMBIE_TOKEN` const dies with its caller).
- **`docs/ZIG_RULES.md`** — §1 is mostly `*.zig`; cross-compile both linux targets; ZLint `unused-decls`.
- **`docs/AUTH.md`** — re-read before §1 + §2 (the live model is platform_admin-gated `POST /v1/runners`).
- **UI (§2)** — design-system primitives + `theme.css` tokens (UI Substitution + DESIGN TOKEN gates); mirror the api-keys components. `docs/REST_API_DESIGN_GUIDELINES.md` is **N/A** (the endpoint is unchanged; §2 adds a server action + UI, not a route).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes (§1) | cross-compile both targets; read ZIG_RULES. |
| PUB / Struct-Shape | yes (§1 removal) | removal-only; ZLint `unused-decls` confirms. |
| UFS | yes (§1) | `ENV_ZOMBIE_TOKEN` removed with its sole caller. |
| UI Substitution / DESIGN TOKEN | yes (§2) | design-system primitives + theme tokens; mirror api-keys components. |
| File & Function Length / LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | net-removing Zig; no schema/log/lifecycle/error-code change. |

---

## Overview

**Goal (testable):** After this PR, `zombie-runner --help` lists no `register` and no `--token`/`ZOMBIE_TOKEN` (`grep -rn "ZOMBIE_TOKEN\|--token" src/runner` → 0); a platform admin mints a `zrn_` from a dashboard "Add runner" action (session-authed, revealed once, copyable) and a non-platform-admin cannot; `POST /v1/runners` + its gate are unchanged; both build graphs + cross-compile pass.

**Problem:** `zombie-runner register --token <admin-jwt>` is the one runner surface that takes the operator's full platform-admin identity credential on the CLI. Neither GitHub (registration token / JIT config) nor GitLab-16 (`glrt-` runner auth token) puts the human's identity token on the runner-config CLI — they mint a dedicated token from a platform call (UI/API). Our `zrn_` is already that dedicated token; only the mint mechanism drifted from the model `runner_fleet.md` claims.

**Solution summary:** Remove the `register --token` CLI path (§1) and add a platform-admin dashboard "Add runner" flow (§2) that calls the existing `POST /v1/runners` with the logged-in session and reveals the `zrn_` once (mirroring api-keys). The operator installs it into the host's `ZOMBIE_RUNNER_TOKEN`/vault. No identity credential ever reaches a shell.

---

## Prior-Art / Reference Implementations

- **UI (§2)** → `ui/packages/app/app/(dashboard)/settings/api-keys/{page,actions}.ts(x)` + `components/{CreateApiKeyDialog,ApiKeyList,RevokeConfirm}.tsx` — the create-list + **reveal-once** pattern, mirrored for runners (mint → `RevealPanel` shows `zrn_` once → copy → list). Divergence: runners are **platform-scoped, not tenant-scoped**, so the surface is platform-admin-gated (the first such surface in the app — see §2).
- **CLI (§1)** → `docs/CLI_DX_PILLARS.md`; the surviving `status`/`doctor`/help already conform; §1 is a removal.
- **API** → `src/zombied/http/handlers/runner/register.zig` + `platform_admin.zig` — the mint primitive, unchanged.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/cmd/register.zig` | DELETE | The CLI minter — its only job was the admin-JWT `POST /v1/runners` call. |
| `src/runner/cmd/registry.zig` | EDIT | Drop `register` from the `Command` enum + spec table (removes its help row). |
| `src/runner/cmd/help.zig` | EDIT | Remove `--token` (Flags) + `ZOMBIE_TOKEN` (Environment). |
| `src/runner/cmd/testdata/help.txt` | EDIT | Regenerate the byte-exact golden. |
| `src/runner/cmd/args.zig` | EDIT | Remove `--token` parsing. |
| `src/runner/daemon/config.zig` | EDIT | Remove `ENV_ZOMBIE_TOKEN`. |
| `src/runner/daemon/control_plane_client.zig` | EDIT | Remove the `register` client fn; keep lease/heartbeat/renew/report. |
| `src/zombied/http/runner_register_integration_test.zig` | EDIT | Rework: assert mint + 403 gate against `POST /v1/runners` directly (no CLI spawn). |
| `ui/packages/app/app/(dashboard)/<platform-admin>/runners/{page.tsx,actions.ts,components/*}` | CREATE | Platform-admin "Add runner" surface: list + mint dialog with reveal-once (mirror api-keys); a server action calls `POST /v1/runners` session-authed. Route placement + names: implementer's call. |
| `docs/architecture/runner_fleet.md` + `docs/AUTH.md` | EDIT | Reconcile: mint is a platform_admin `POST /v1/runners` (dashboard/API), not `register --token`. |
| `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` + `07_runner_bootstrap_prod/001_playbook.md` | EDIT | Operator mints via dashboard/API; the playbook already only *installs* the `zrn_`. |

> `src/runner/cmd/status.zig` matched a `register` grep — verify at PLAN it's a reference, not a live dep. Done-spec `M80_004` (`docs/v2/done/`) stays frozen — superseded, not edited. The platform-admin route placement + the app's server-side `platform_admin` claim check are the §2 design calls (mirror the app's existing session-auth in server actions).

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, two coupled Sections (§1 CLI removal, §2 dashboard mint) + §3 doc reconciliation — merged because shipping §1 without §2 just moves the admin JWT from the runner CLI to `curl` (Indy's call: they go together, one spec).
- **Alternatives considered:** (a) two specs in one batch — rejected per Indy (one spec). (b) keep `register` with a *scoped* token — rejected: the host never runs it; the dashboard is the right operator surface.
- **Patch-vs-refactor verdict:** **patch + small feature** — Zig removal against an unchanged endpoint, plus a UI surface mirroring an existing pattern. No new API contract; no larger refactor hiding here.

---

## Sections (implementation slices)

### §1 — Remove the `register` subcommand + `--token`/`ZOMBIE_TOKEN`

Delete the CLI minting path so the runner never accepts an identity credential. The `Command` enum drop cascades to dispatch + the help golden; the config constant + client fn go with their sole caller. **Invariant:** the daemon + `status`/`doctor` (all on `ZOMBIE_RUNNER_TOKEN`) and `POST /v1/runners` + its gate are untouched.

- **Dimension 1.1** — `register` gone; running it exits non-zero with unknown-command help → Test `runner cli rejects removed register`.
- **Dimension 1.2** — `--help` golden + src have no `register`/`--token`/`ZOMBIE_TOKEN` → Test `runner help has no enrollment-token surface`.
- **Dimension 1.3** — `control_plane_client.register` removed; ZLint clean; both graphs + cross-compile green → Test `runner builds without register client`.

### §2 — Dashboard "Add runner" platform-admin mint (reveal-once)

A platform-admin-gated dashboard surface lists runners and mints a `zrn_` via "Add runner": a server action calls the session-authed `POST /v1/runners`, and the result is revealed **once** (copy-to-clipboard, dismissal locked, raw value dropped on close) — mirroring `CreateApiKeyDialog`. **Implementation default:** mirror the api-keys components verbatim, swapping the endpoint + adding the platform-admin gate. **Invariant:** a non-platform-admin cannot mint (server enforces 403; the UI does not render the action for non-admins).

- **Dimension 2.1** — a platform admin mints a `zrn_` revealed exactly once (re-open does not re-reveal; raw value leaves the DOM on close) → Test `dashboard mints zrn_ and reveals once` (e2e).
- **Dimension 2.2** — a non-platform-admin cannot reach/mint (server 403; surface hidden) → Test `runner mint is platform-admin-gated`.

### §3 — Reconcile enrollment docs + playbooks

`runner_fleet.md`/`AUTH.md`/the bootstrap playbooks describe minting via the dashboard/API, not the removed CLI. **Invariant:** host-bootstrap steps unchanged; only the mint description changes.

- **Dimension 3.1** — no live doc/playbook references `zombie-runner register` or `--token`/`ZOMBIE_TOKEN` → Test `enrollment-doc sweep`.

---

## Interfaces

`POST /v1/runners` (request/response, `platform_admin` auth, `zrn_<64-hex>` mint, returned once) is **unchanged**. §2 adds a server action wrapping it with the logged-in session; §1 removes a Zig client of it. Removed internal surface: `Command.register`, `cmd/register.zig`, `control_plane_client.register`, `config.ENV_ZOMBIE_TOKEN`, the `--token` flag.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Operator runs old `register` | stale muscle-memory / script | unknown-command help to stderr, non-zero exit (existing dispatch). |
| Non-platform-admin opens the mint UI | wrong role | server 403 / redirect; the action is hidden in nav; no `zrn_` minted. |
| User dismisses reveal without copying | closed the dialog | the `zrn_` is unrecoverable (shown once, like api-keys) → re-mint; copy-state + dismissal-lock reduce the footgun. |
| Mint API error (5xx / session expired) | control-plane down / auth lapsed | dialog surfaces a structured error; no partial state; retry. |
| Removed client fn still referenced | missed caller | runner build fails → restore + re-investigate; never `--no-verify`. |

---

## Invariants

1. **`POST /v1/runners` + its `platform_admin` gate unchanged** — enforced by the reworked integration test (mint succeeds; tenant key → 403) + §2.2.
2. **No enrollment-token surface in the runner** — ZLint `unused-decls` + the §1.2 grep (`ZOMBIE_TOKEN`/`--token`/`ENV_ZOMBIE_TOKEN` → 0).
3. **Reveal-once** — the raw `zrn_` is dropped from client state on close and never re-fetched in plaintext (mirrors api-keys; enforced by the §2.1 component/e2e test).
4. **Platform-admin-only mint** — server-side `platform_admin` check (the API 403s; the UI must not render the action for non-admins) — enforced by §2.2.
5. **No compat shim / deprecated alias (NLG)** — clean removal; lint legacy-symbol guard + grep.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete → expected) |
|-----------|------|------|-------------------------------|
| 1.1 | unit | `runner cli rejects removed register` | dispatch `register` → unknown-command help on stderr, non-zero exit. |
| 1.2 | unit | `runner help has no enrollment-token surface` | `--help` golden + src grep for `register`/`--token`/`ZOMBIE_TOKEN` → 0. |
| 1.3 | regression | `runner builds without register client` | both graphs + cross-targets green; ZLint clean. |
| 2.1 | e2e | `dashboard mints zrn_ and reveals once` | platform admin → Add runner → `zrn_` shown once; re-open does not re-reveal. |
| 2.2 | integration | `runner mint is platform-admin-gated` | non-admin session → 403 / hidden; `POST /v1/runners` tenant key → 403. |
| 3.1 | regression | `enrollment-doc sweep` | `grep -rn "zombie-runner register\|--token\|ZOMBIE_TOKEN" docs/ playbooks/` (live) → 0. |

**Regression:** the runner daemon suite + `test-auth` (platform_admin) stay green. **Idempotency:** N/A (each mint creates a distinct runner).

---

## Acceptance Criteria

- [ ] No enrollment-token surface — verify: `grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner` → 0
- [ ] `--help` golden updated, no `register` — verify: `zig build --build-file build_runner.zig test`
- [ ] Both graphs build + cross-compile clean — verify: `zig build && zig build --build-file build_runner.zig -Dtarget=x86_64-linux && zig build --build-file build_runner.zig -Dtarget=aarch64-linux`
- [ ] Mint + gate intact (no CLI) — verify: `make test-integration` + `zig build test-auth`
- [ ] Dashboard mint reveal-once + platform-admin gate — verify: the app e2e lane for the runners surface (`make acceptance-e2e` or equivalent)
- [ ] UI lint + design-token clean — verify: `make lint-apps-ds-ctl`
- [ ] Docs/playbooks reconciled — verify: `grep -rn "zombie-runner register" docs/ playbooks/` (live) → 0
- [ ] `gitleaks detect` clean · no file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: no enrollment-token surface in the runner
grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner | head && echo FAIL || echo PASS
# E2: both graphs + cross-compile
zig build && zig build --build-file build_runner.zig -Dtarget=x86_64-linux && zig build --build-file build_runner.zig -Dtarget=aarch64-linux && echo PASS || echo FAIL
# E3: runner tests (golden) + auth gate + endpoint mint
zig build --build-file build_runner.zig test && zig build test-auth && make test-integration 2>&1 | tail -3
# E4: UI lint
make lint-apps-ds-ctl 2>&1 | grep -E "PASS|FAIL"
# E5: enrollment-doc sweep (live only; empty = pass)
grep -rn "zombie-runner register\|--token\|ZOMBIE_TOKEN" docs/ playbooks/ | grep -v 'docs/v2/done/'
# E6: gitleaks
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
| `control_plane_client.register` | `grep -rn "\.register(" src/runner` | 0 |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults/decisions.

- **CTO consult (Jun 03 2026)** — Indy questioned `register --token`; vs GitHub (registration token / JIT) + GitLab-16 (`glrt-` UI/API mint), both mint a *dedicated* token and never put the human's identity credential on the runner-config CLI. Decision: retire the CLI path + mint from the dashboard. Reverses memory `project_runner_register_admin_token_intentional` (which held only for M83's scope).
- **Merge decision (Jun 03 2026)** — Indy: §1 (CLI removal) and §2 (UI mint) ship together (removing the CLI before the UI just moves the admin JWT to `curl`) → one spec, one PR (Batch B1), not two.
- **§2 greenfield note (decide at PLAN)** — no platform-admin dashboard surface exists today; §2 introduces the first. Mirror the app's existing session-auth in server actions for the `platform_admin` gate; mirror `CreateApiKeyDialog` for the reveal-once. Route placement is the implementer's call.
- **Skill chain outcomes / Deferrals** — populate during VERIFY/CHORE(close); none deferred.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| Before CHORE(close) | `/write-unit-test` | Coverage vs Test Spec — esp. reveal-once + gating + endpoint-without-CLI. | Clean; count in Discovery. |
| Before CHORE(close) | `/review` | Adversarial diff review vs spec, ZIG_RULES, AUTH.md, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| No token surface | `grep -rn "ZOMBIE_TOKEN\|--token" src/runner` | {paste} | |
| Runner tests + golden | `zig build --build-file build_runner.zig test` | {paste} | |
| Endpoint mint/gate | `make test-integration` | {paste} | |
| Dashboard e2e | app e2e lane (runners) | {paste} | |
| UI lint | `make lint-apps-ds-ctl` | {paste} | |
| Doc sweep | `grep -rn "zombie-runner register" docs/ playbooks/` | {paste} | |

---

## Out of Scope

- **GitHub-style ephemeral / JIT runner tokens** — not adopted; `runner_fleet.md` non-goals (no scheduler/autoscale) make the GitLab-16 direct-mint shape the right fit. Revisit only for open-fleet (mode C).
- **Runner revoke / rotate UI** — mint + list is in scope; full lifecycle management beyond what api-keys' `RevokeConfirm` trivially mirrors is future work.
- **The `POST /v1/runners` contract + `platform_admin` gate** — unchanged; not re-litigated here.
