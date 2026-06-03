# M84_001: Retire the admin-JWT `zombie-runner register` CLI path

**Prototype:** v2.0.0
**Milestone:** M84
**Workstream:** 001
**Date:** Jun 03, 2026
**Status:** PENDING
**Priority:** P1 — operator credential-surface fix; removes the host CLI's only use of the operator's full identity JWT.
**Categories:** API, CLI, DOCS
**Batch:** B1 — standalone.
**Branch:** {feat/m84-retire-register-token — added when work begins}
**Depends on:** none hard (the mint primitive `POST /v1/runners` already exists). **Pairs with M84_002** (dashboard "Add runner" UI mint) — see Out of Scope; until M84_002 ships the interim mint is a direct API call.
**Provenance:** agent-generated (Indy CTO consult, Jun 03 2026 — reverses the "leave it" call recorded in memory `project_runner_register_admin_token_intentional`).

**Canonical architecture:** `docs/architecture/runner_fleet.md` (runner enrollment, "Option B") + `docs/AUTH.md` (runner token provisioning). This workstream reconciles the *implementation* to the model those docs already describe.

---

## Implementing agent — read these first

1. `docs/v2/done/M80_004_P1_API_CLI_RUNNER_OPERATOR_CLI.md` — the spec this **supersedes in part**: it added `register` to the runner CLI authenticating with the operator's platform-admin Clerk JWT via `ZOMBIE_TOKEN`/`--token`. Read its §1 + Interfaces to know exactly what to unwind.
2. `src/runner/cmd/registry.zig` + `src/runner/cmd/help.zig` — the typed `Command` enum → `commandSpec` table that drives both dispatch and the byte-exact `--help` golden. Dropping `register` from the enum is the single source that removes its help row.
3. `docs/architecture/runner_fleet.md` (runner enrollment / "Option B") — the model: the operator pre-mints the `zrn_` via `POST /v1/runners`; the host holds only `ZOMBIE_RUNNER_TOKEN` and never self-registers.
4. `playbooks/founding/06_runner_bootstrap_dev/` + `07_runner_bootstrap_prod/` — the host bootstrap installs the `zrn_` directly from vault; it never runs `register`. Confirms removing the CLI subcommand strands nothing on the host.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Retire the admin-JWT zombie-runner register CLI; mint via the API
- **Intent (one sentence):** Remove the only place the runner CLI takes the operator's full platform-admin Clerk JWT — the `register --token` path — so runner enrollment authenticates the way GitHub/GitLab do: a platform call (API now, dashboard next) mints a dedicated `zrn_`, and the host CLI never touches an identity credential.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`. A mismatch with the Intent → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (no dead code: remove `register`'s now-orphaned client + config surface), **NLR** (touch-it-fix-it on every file touched), **NLG** (pre-2.0: no compat shim / deprecated-alias for the removed flag — clean break), **ORP** (orphan sweep after the CLI + its client fn go), **UFS** (the `ENV_ZOMBIE_TOKEN` constant + its callers vanish together).
- **`docs/ZIG_RULES.md`** — Progressive Cleanup, cross-compile both linux targets, ZLint `unused-decls` as the dead-surface safety net. Diff is mostly `*.zig`.
- **`docs/AUTH.md`** — re-read before touching the runner-token provisioning narrative; the live model is the platform_admin-gated `POST /v1/runners`.
- REST / SCHEMA / BUN / LOGGING — **N/A** (the `POST /v1/runners` handler + its auth are UNCHANGED; no schema, Bun, or new log surface).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `*.zig` edited/deleted | Build both graphs + cross-compile both linux targets; read ZIG_RULES. |
| PUB / Struct-Shape | yes — removing `pub` surface (`Command.register`, `control_plane_client.register`, `ENV_ZOMBIE_TOKEN`) | Removal-only; ZLint `unused-decls` confirms no new dead `pub`. |
| File & Function Length | no | Net-removing lines. |
| UFS | yes — `ENV_ZOMBIE_TOKEN` single-source const removed with its sole caller | No orphaned constant left behind. |
| MILESTONE-ID / SPEC TEMPLATE | yes | This spec obeys the template; code carries no `M84` literal. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA / UI | no | No log-emit, lifecycle, error-code, schema, or UI change in this workstream. |

---

## Overview

**Goal (testable):** After this PR, `zombie-runner --help` lists no `register` command and no `--token`/`ZOMBIE_TOKEN`; `grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner` returns zero; both build graphs + cross-compile pass; the `POST /v1/runners` endpoint and its `platform_admin` gate are unchanged and still covered by an integration test that does not spawn the removed CLI; and the runner daemon (`status`/`doctor` included, all on `ZOMBIE_RUNNER_TOKEN`) is untouched.

**Problem:** `zombie-runner register --token <admin-clerk-jwt>` is the one runner-CLI surface that consumes the operator's **full platform-admin identity credential** on the command line. Neither GitHub (registration token / JIT config) nor GitLab-16 (`glrt-` runner auth token) puts the human's identity token on the enrollment CLI — they mint a dedicated token from a platform call. Our `zrn_` is already that dedicated token; only the *mint mechanism* drifted from the model `runner_fleet.md` claims ("Option B, create-runner → auth-token").

**Solution summary:** Remove the `register` subcommand and its `--token`/`ZOMBIE_TOKEN` credential surface from `zombie-runner`. The `zrn_` is minted by a platform-admin call to the **existing, unchanged** `POST /v1/runners` — directly via the API in the interim, and via the dashboard "Add runner" flow once M84_002 lands. The host never runs `register` (the bootstrap playbook installs the `zrn_` from vault), so nothing on the host regresses.

---

## Prior-Art / Reference Implementations

- **Industry model** → GitHub self-hosted runners (registration token / `generate-jitconfig`) and GitLab 16+ (`glrt-` runner authentication token created in UI/API). Common law: the human authenticates to *mint* a dedicated token; the identity credential never reaches the runner-config CLI. We adopt the GitLab-16 shape (direct long-lived `zrn_`, no exchange) — fits our operator-pinned trusted fleet (`runner_fleet.md` non-goals: no scheduler/autoscale → no need for GitHub's ephemeral JIT).
- **CLI** → `docs/CLI_DX_PILLARS.md` — this is a removal; the surviving `status`/`doctor`/`--help` already follow the pillars (table-driven register, byte-exact golden, handler purity). No new command surface introduced.
- **API** → `src/zombied/http/handlers/runner/register.zig` (`POST /v1/runners`, `performRegister`) + `middleware/platform_admin.zig` — the mint primitive, **unchanged**.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/runner/cmd/register.zig` | DELETE | The CLI minting handler — its only purpose was the admin-JWT `POST /v1/runners` call. |
| `src/runner/cmd/registry.zig` | EDIT | Drop `register` from the `Command` enum + `commandSpec` table (this also removes its `--help` row). |
| `src/runner/cmd/help.zig` | EDIT | Remove `--token` from Flags and `ZOMBIE_TOKEN` from Environment. |
| `src/runner/cmd/testdata/help.txt` | EDIT | Regenerate the byte-exact golden (no `register` row, no `--token`/`ZOMBIE_TOKEN`). |
| `src/runner/cmd/args.zig` | EDIT | Remove `--token` parsing / the `flagOrEnv("--token", …)` plumbing if CLI-wide. |
| `src/runner/daemon/config.zig` | EDIT | Remove the `ENV_ZOMBIE_TOKEN = "ZOMBIE_TOKEN"` constant (sole consumer was `register`). |
| `src/runner/daemon/control_plane_client.zig` | EDIT | Remove the `register` client fn (the runner-side `POST /v1/runners` caller); keep lease/heartbeat/renew/report. |
| `src/zombied/http/runner_register_integration_test.zig` | EDIT | Rework: assert `POST /v1/runners` mints + the minted `zrn_` authenticates, and the tenant-key → 403 gate — **without spawning the removed CLI** (drive the endpoint directly). |
| `docs/architecture/runner_fleet.md` | EDIT | Reconcile the enrollment narrative: mint is a platform-admin `POST /v1/runners` (API now, dashboard M84_002), not a runner-CLI `register --token`. |
| `docs/AUTH.md` | EDIT | Update the runner-token provisioning note (drop the `register`-via-`ZOMBIE_TOKEN` description). |
| `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` + `07_runner_bootstrap_prod/001_playbook.md` | EDIT | State the operator mints the `zrn_` via the API/dashboard; the playbook already only *installs* it. |

> `src/runner/cmd/status.zig` matched a `register` grep — verify at PLAN it's a comment/reference, not a live dependency on the removed surface. Done-spec `M80_004` (`docs/v2/done/`) stays frozen — superseded, not edited.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** two Sections — the CLI/credential removal (§1) and the doc/playbook reconciliation (§2). One workstream; the API mint primitive is untouched so there is no new abstraction.
- **Alternatives considered:** (a) keep `register` but take a *scoped* enrollment token instead of the admin JWT — rejected: the host never runs `register`, so the CLI minter has no real consumer; the dashboard (M84_002) is the right operator surface. (b) Block this on M84_002 (UI) shipping first — rejected as a hard dep: the API mint already works as the interim; see the sequencing note in Discovery.
- **Patch-vs-refactor verdict:** **patch** — removal + test rework against an unchanged endpoint. The structural follow-up (the dashboard mint UX) is **M84_002**, named, not mud-patched here.

---

## Sections (implementation slices)

### §1 — Remove the `register` subcommand + `--token`/`ZOMBIE_TOKEN`

Delete the CLI minting path so `zombie-runner` no longer accepts an identity credential. The `Command` enum drop cascades to dispatch + the help golden; the config constant and the control-plane client `register` fn go with their sole caller. **Invariant to protect:** the daemon and `status`/`doctor` (all on `ZOMBIE_RUNNER_TOKEN`) are untouched, and `POST /v1/runners` + its `platform_admin` gate are unchanged.

- **Dimension 1.1** — `register` gone from the `Command` enum/dispatch; an attempt to run it exits non-zero with the unknown-command help → Test `runner cli rejects removed register command`.
- **Dimension 1.2** — `--help` golden has no `register` row, no `--token`, no `ZOMBIE_TOKEN`; `grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner` → 0 → Test `runner help golden has no enrollment-token surface`.
- **Dimension 1.3** — `control_plane_client.register` removed; ZLint `unused-decls` clean; both build graphs + cross-compile green → Test `runner builds without the register client`.

### §2 — Reconcile enrollment docs + playbooks to the API/dashboard mint

The narrative in `runner_fleet.md` / `AUTH.md` / the bootstrap playbooks must describe minting via `POST /v1/runners` (API now, dashboard M84_002), not the removed CLI. **Invariant to protect:** the host-bootstrap steps (install `zrn_` from vault → `ZOMBIE_RUNNER_TOKEN`) are unchanged; only the *mint* description changes.

- **Dimension 2.1** — no live doc/playbook references `zombie-runner register` or `--token`/`ZOMBIE_TOKEN` for enrollment → Test `enrollment-doc sweep` (`grep` returns zero in live docs/playbooks).
- **Dimension 2.2** — `POST /v1/runners` mint + `platform_admin` 403 gate proven by an integration test that does **not** spawn the CLI → Test `runner register endpoint mints and gates without the CLI`.

---

## Interfaces

`POST /v1/runners` (request/response, `platform_admin` auth, `zrn_<64-hex>` mint) is **unchanged** — this workstream removes a *client* of it, not the contract. No new public HTTP, CLI, or cross-module interface is added. Removed internal surface: `Command.register`, `cmd/register.zig`, `control_plane_client.register`, `config.ENV_ZOMBIE_TOKEN`, the `--token` flag.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Operator runs the old `register` | Muscle memory / stale script | Unknown-command help to stderr, non-zero exit (existing dispatch behaviour for any unknown command). |
| Removed client fn still referenced | Missed a caller | Build fails on the runner graph → restore the reference and re-investigate; never `--no-verify`. |
| Endpoint coverage lost with the CLI test | Over-broad test deletion | The reworked integration test must still mint + gate `POST /v1/runners`; review the diff — only the CLI-spawn arm may go. |
| Stale enrollment doc left | Reconciliation missed a file | §2.1 grep sweep returns non-zero → fix before VERIFY. |

---

## Invariants

1. **`POST /v1/runners` + its `platform_admin` gate are unchanged** — enforced by the reworked integration test (mint succeeds; tenant key → 403).
2. **The host bootstrap path is unchanged** — the playbooks still install the `zrn_` into `ZOMBIE_RUNNER_TOKEN`; no host step removed (doc-diff review).
3. **No enrollment-token surface remains in the runner** — enforced by ZLint `unused-decls` + the §1.2 grep (`ZOMBIE_TOKEN`/`--token`/`ENV_ZOMBIE_TOKEN` → 0).
4. **No compat shim / deprecated alias (RULE NLG)** — clean removal; enforced by the lint legacy-symbol guard + the grep.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete → expected) |
|-----------|------|------|-------------------------------|
| 1.1 | unit | `runner cli rejects removed register command` | dispatch of `register` → unknown-command help on stderr, non-zero exit. |
| 1.2 | unit | `runner help golden has no enrollment-token surface` | `--help` byte-exact golden has no `register`/`--token`/`ZOMBIE_TOKEN`; src grep → 0. |
| 1.3 | regression | `runner builds without the register client` | `zig build --build-file build_runner.zig` + both cross-targets green; ZLint clean. |
| 2.1 | regression | `enrollment-doc sweep` | `grep -rn "zombie-runner register\|--token\|ZOMBIE_TOKEN" docs/ playbooks/` (live) → 0. |
| 2.2 | integration | `runner register endpoint mints and gates without the CLI` | `POST /v1/runners` (platform-admin) → `zrn_` that authenticates a runner call; tenant `zmb_t_` → 403; no CLI spawned. |

**Regression:** the runner daemon suite + `test-auth` (platform_admin) must stay green. **Idempotency/replay:** N/A.

---

## Acceptance Criteria

- [ ] No enrollment-token surface — verify: `grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner` → 0
- [ ] `--help` golden updated, no `register` — verify: `zig build --build-file build_runner.zig test` (golden test green)
- [ ] Both graphs build; cross-compile clean — verify: `zig build && zig build --build-file build_runner.zig && zig build --build-file build_runner.zig -Dtarget=x86_64-linux && -Dtarget=aarch64-linux`
- [ ] `POST /v1/runners` mint + gate intact (no CLI) — verify: `make test-integration` (reworked register test) + `zig build test-auth`
- [ ] Live docs/playbooks reconciled — verify: `grep -rn "zombie-runner register" docs/ playbooks/` → 0 (live only)
- [ ] Lint clean (ZLint unused-decls) · `gitleaks detect` clean · no file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: no enrollment-token surface in the runner
grep -rn "ZOMBIE_TOKEN\|--token\|ENV_ZOMBIE_TOKEN" src/runner | head && echo FAIL || echo PASS
# E2: both graphs + cross-compile
zig build && zig build --build-file build_runner.zig && zig build --build-file build_runner.zig -Dtarget=x86_64-linux && zig build --build-file build_runner.zig -Dtarget=aarch64-linux && echo PASS || echo FAIL
# E3: runner tests (incl. help golden) + auth gate
zig build --build-file build_runner.zig test && zig build test-auth && echo PASS || echo FAIL
# E4: endpoint mint/gate (no CLI) + lint
make test-integration 2>&1 | tail -5 ; make lint-zig 2>&1 | grep -E "ZLint passed|FAIL"
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
| `Command.register` / `cmd/register.zig` | `grep -rn "register" src/runner/cmd/registry.zig` | 0 (no `register` enum/spec) |
| `control_plane_client.register` | `grep -rn "\.register(" src/runner` | 0 |

---

## Discovery (consult log)

> **Empty at creation.** Append as work surfaces consults/decisions.

- **CTO consult (Jun 03 2026)** — Indy questioned the `register --token` model at CTO level; compared to GitHub (registration token / JIT) and GitLab-16 (`glrt-` UI/API mint), both of which mint a dedicated token and never put the human's identity credential on the runner-config CLI. Decision: **retire the CLI register path**; mint via the API/dashboard. Reverses the earlier "leave it" call (memory `project_runner_register_admin_token_intentional`), which held only for M83's scope.
- **Sequencing note (decide at PLAN):** removing the CLI register before **M84_002** (dashboard UI mint) leaves the interim operator mint as a direct `POST /v1/runners` call — which still carries the admin JWT in a shell (curl/script). M84_002 is what fully eliminates the shell-JWT exposure (browser-session-authed mint). **Recommendation:** ship M84_002 alongside or immediately after M84_001; if M84_001 lands first, document the API-mint interim explicitly. Indy to confirm sequencing.
- **Skill chain outcomes** — populate during VERIFY/CHORE(close).
- **Deferrals** — none.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage vs this Test Spec — esp. that the reworked endpoint test still proves mint + 403 gate without the CLI. | Clean; iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs spec, ZIG_RULES, AUTH.md, Failure Modes, Invariants — esp. "did the endpoint lose coverage with the CLI?" | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| No token surface | `grep -rn "ZOMBIE_TOKEN\|--token" src/runner` | {paste} | |
| Runner tests + golden | `zig build --build-file build_runner.zig test` | {paste} | |
| Endpoint mint/gate | `make test-integration` (register test) | {paste} | |
| Auth portability | `zig build test-auth` | {paste} | |
| Cross-compile | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | {paste} | |
| Doc sweep | `grep -rn "zombie-runner register" docs/ playbooks/` | {paste} | |

---

## Out of Scope

- **M84_002 — dashboard "Add runner" UI mint** (the proper operator UX; needs a net-new **platform-admin** dashboard surface — none exists today). It calls the same `POST /v1/runners` with the logged-in session and shows the `zrn_` once. Its own spec; the full elimination of shell-JWT exposure lands there.
- **The `POST /v1/runners` contract + `platform_admin` gate** — unchanged; not re-litigated here.
- **GitHub-style ephemeral/JIT runner tokens** — not adopted; `runner_fleet.md` non-goals (no scheduler/autoscale) make the GitLab-16 direct-mint shape the right fit. Revisit only if open-fleet (mode C) is pursued.
