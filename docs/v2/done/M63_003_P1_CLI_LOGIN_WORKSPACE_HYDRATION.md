<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M63_003: zombiectl login hydrates workspaces; architecture starter-grant doc says $5

**Prototype:** v2.0.0
**Milestone:** M63
**Workstream:** 003
**Status:** DONE
**Priority:** P1 — first-run CLI customers are pushed to create a duplicate workspace because login does not see the signup-bootstrap default. Architecture doc disagrees with running code on starter-grant size.
**Categories:** CLI, DOCS
**Branch:** fix/cli-login-workspace-hydration
**Depends on:** M63_001 (default API URL — DONE), M11_006 (signup bootstrap — DONE).

**Canonical architecture:** `docs/architecture/billing_and_byok.md` (starter grant), `docs/architecture/data_flow.md` (signup → tenant + default workspace).

---

## Implementing agent — read these first

1. `zombiectl/src/commands/core.js` — `commandLogin` after `saveCredentials` is the new hydration insertion point.
2. `zombiectl/src/commands/core-ops.js` — `commandDoctor` reads `workspaces.current_workspace_id`; this is the visible symptom of a missing local workspace selection.
3. `zombiectl/src/lib/state.js` — `loadWorkspaces` / `saveWorkspaces` shape (`{current_workspace_id, items:[{workspace_id, name, ...}]}`).
4. `src/http/handlers/tenant_workspaces.zig` — server side. Returns `{items:[{id, name, repo_url, created_at}], total}`. The CLI must accept `id` (server) and store as `workspace_id` (local convention).
5. `src/state/signup_bootstrap.zig` — proof a default workspace already exists at signup time (lines 140-176 — single transaction creates tenant/user/membership/workspace/billing).
6. `src/state/tenant_billing.zig` — `STARTER_GRANT_CENTS = 500`. Source of truth for the doc fix.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal.
- `docs/BUN_RULES.md` — diff is JS only.
- No Zig / schema / HTTP-handler / auth-flow code touched. Server-side handler stays as-is; this milestone only consumes it.

---

## Anti-Patterns to Avoid

- Do NOT auto-create a workspace if the tenant has zero workspaces. Signup already provisions one; if the server returns empty, the right move is "noop, leave state intact" — `workspace add` is the explicit recovery path the user already knows.
- Do NOT re-issue `GET /v1/tenants/me/workspaces` on every command. Hydration belongs in `commandLogin` (one-shot) — every other command keeps reading the persisted `workspaces.json`.
- Do NOT block the `login` exit code on hydration failure. Login returns 0 if credentials persisted; hydration is best-effort. The user can still run `workspace add` if the server was unreachable for the workspace list.
- Do NOT rewrite `done/` historical specs to reflect the $5 grant — they are immutable historical record. Only live architecture docs flip.
- Do NOT thread `repo_url` into `workspace add`'s local-cache shape just for symmetry. Hydrate path keeps the field; legacy `workspace add` path stays minimal — the file format is a tagged union, not a schema.

---

## Overview

**Goal (testable):** A first-time customer runs `npm i -g @usezombie/zombiectl && zombiectl login`, completes Clerk OAuth, and `zombiectl doctor` immediately reports all three checks green — without `workspace add`. The workspace selection comes from the signup-bootstrap default (`signup_bootstrap.zig:140-176`), surfaced via `GET /v1/tenants/me/workspaces`.

**Problem:** The Clerk `user.created` webhook atomically creates a default workspace at signup. The dashboard discovers it via the same endpoint and selects it (see `ui/packages/app/lib/workspace.ts:21-29`). The CLI does not — `commandLogin` saves credentials and stops, so `loadWorkspaces` returns the empty fallback on next invocation, and `doctor` complains "no workspace selected. Run: zombiectl workspace add". The user creates a duplicate workspace, billing rolls up to the same tenant (no double-grant), but the canonical default is invisible.

**Solution summary:** After credentials persist on a successful login, the CLI calls `GET /v1/tenants/me/workspaces`, normalizes the response, writes `workspaces.json` with the first item as `current_workspace_id`. Failure to fetch is silent — login still returns 0; the user can still run `workspace add` as a manual recovery. As a side carry, the architecture doc (`docs/architecture/billing_and_byok.md`) is corrected from $10 / 1000¢ to $5 / 500¢ to match `STARTER_GRANT_CENTS` — the same value the test fixture asserts (`signup_bootstrap_test.zig:147-158`).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/core.js` | EDIT | Add `hydrateWorkspacesAfterLogin` + `normalizeTenantWorkspace`; call after `saveCredentials`. |
| `zombiectl/test/login.unit.test.js` | EDIT | Three new tests: hydrate selects default, empty items leaves state untouched, pre-existing current id preserved when present in response, hydrate failure does not regress login exit code. |
| `zombiectl/test/onboarding-flow.integration.test.js` | EDIT | Two new tests: end-to-end fresh-state login → workspaces.json populated; end-to-end fresh-state login → doctor green. Existing fresh-state credentials test stands (still asserts the credential shape; new hydration request 404s and is swallowed). |
| `zombiectl/test/cli-analytics.unit.test.js` | EDIT | Pre-existing test routes by `options.method` only; new hydration GET fell through and double-counted polls. Stub the workspaces path explicitly. |
| `docs/architecture/billing_and_byok.md` | EDIT | Two prose lines: `1000 cents (USD $10)` → `500 cents (USD $5)`; "$10 starter grant ... ~300 events / ~1000 events" → "$5 starter grant ... ~150 / ~500". |
| `docs/architecture/README.md` | EDIT | One-line cross-reference description. |
| `docs/architecture/scenarios/README.md` | EDIT | One-line scenario-3 description. |

Out of scope for this branch (called out in PR description, deferred to a follow-up doc-cleanup milestone): the dollar/cent math inside `docs/architecture/scenarios/01_default_install.md` and `docs/architecture/scenarios/03_balance_gate.md`. Both still reference `$10` in scenarios; updating them in-line forces re-deriving the gate-trip arithmetic, which earns a focused milestone.

---

## Sections (implementation slices)

### §1 — Hydration plumbing in `commandLogin`

Insert `await hydrateWorkspacesAfterLogin(ctx, workspaces, deps)` after `await saveCredentials(saved)` in `commands/core.js`. Ordering matters: `ctx.token` must be set so `apiHeaders(ctx)` carries the bearer the workspaces endpoint requires.

The helper:

- Calls `GET /v1/tenants/me/workspaces`, returns `null` on any thrown error.
- Filters response items through `normalizeTenantWorkspace` — accepts either `id` (server shape) or `workspace_id` (defensive); preserves `name`, `repo_url`, `created_at` (with `Date.now()` fallback only if the server omits it).
- Picks `current_workspace_id` = the existing local selection if it appears in the server response, else `items[0].workspace_id`.
- No-op if `items.length === 0` (no save). Empty list means signup is mid-flight or tenant context is broken; the customer's `workspace add` path stays as documented recovery.

### §2 — Test surface

Three new unit cases in `login.unit.test.js`:

1. **happy path** — server returns one workspace, hydration writes `workspaces.json` with the right shape, `Authorization: Bearer <token>` is on the request.
2. **empty items** — server returns `{items:[], total:0}`. `saveWorkspaces` is not called; in-memory state stays empty.
3. **pre-existing current preserved** — server returns two workspaces, local has `current_workspace_id` already pointing at one of them. Hydration keeps the existing selection rather than flipping to `items[0]`.
4. **hydration failure tolerated** — server throws; login still returns 0, credentials persisted, workspaces unchanged.

Two new integration cases in `onboarding-flow.integration.test.js`:

5. **fresh-state login selects signup-created workspace** — full `runCli`, real fetch, mock loopback. Verifies `workspaces.json` shape end-to-end and that the bearer header reaches the mock.
6. **fresh-state login → doctor green** — chains `login` then `doctor --json`; asserts `report.ok === true`, `workspace_selected.detail === DEFAULT_WORKSPACE_ID`, `workspace_binding_valid.ok === true`. This is the customer-visible acceptance behavior.
7. **resilience** — login on a fresh state dir without a `/v1/tenants/me/workspaces` mock route still exits 0; credentials persist, workspaces stay empty.

The pre-existing `cli-analytics.unit.test.js` "post-login distinct id" test counted polls by routing only on `options.method`. Updated stub to discriminate by URL so the workspaces hydrate request returns `{items:[]}` and stays out of the poll counter. Pure test-fixture refinement; no production behavior change.

### §3 — Architecture doc starter-grant correction

Two `docs/architecture/billing_and_byok.md` prose lines (§3 and §4 paragraphs) and two cross-reference table cells (`docs/architecture/README.md`, `docs/architecture/scenarios/README.md`) flip from `$10 / 1000¢` to `$5 / 500¢`. The line in `billing_and_byok.md` §3 also gains a "source of truth: `STARTER_GRANT_CENTS` in `src/state/tenant_billing.zig`" cross-reference so the next agent reading this finds the canonical value without grepping.

---

## Interfaces

No new public interface. The CLI continues to consume `GET /v1/tenants/me/workspaces` (existing `tenant_workspaces.zig` handler, no body shape change). Persisted `workspaces.json` keeps its schema; hydration entries gain `repo_url` (nullable) but readers tolerate the extra field today (see `commandWorkspace`).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `GET /v1/tenants/me/workspaces` 5xx | Server outage | Caught by `try/catch`; `hydrateWorkspacesAfterLogin` returns `null`. Login exits 0. `workspace add` stays as fallback. |
| `GET /v1/tenants/me/workspaces` 404 | Old server before this endpoint shipped | Same path as 5xx — silent skip. |
| Server returns `{items:[]}` | Webhook race / tenant misconfig | No save. `doctor` still says "no workspace selected"; user creates one with `workspace add`. |
| Server returns malformed item (no id) | Bad row | `normalizeTenantWorkspace` returns `null`; bad rows filtered out. If all items invalid, falls into the `items.length === 0` branch. |
| User's local `workspaces.json` already has `current_workspace_id` set | Multi-machine login or stale state | If server response contains that id, preserve it; else default to `items[0]`. Avoids surprising selection-flip on login. |

---

## Invariants

1. **Login exit code is independent of workspace hydration.** A successful credential save → exit 0, full stop. Enforced by unit test "successful login keeps credentials when workspace hydration fails".
2. **Hydration runs at most once per login.** `commandLogin` is the only caller of `hydrateWorkspacesAfterLogin`; verified by `grep -n hydrateWorkspacesAfterLogin zombiectl/src/`.
3. **`workspaces.json` is never written with `items: []`.** Empty response is a noop. Enforced by unit test "empty items[] response does not write".
4. **Server `id` ↔ local `workspace_id`.** `normalizeTenantWorkspace` accepts both keys and emits the local convention.
5. **Bearer header reaches the workspaces endpoint.** Verified by unit + integration tests; `apiHeaders(ctx)` is the same helper every other authed CLI command uses.
6. **Architecture doc starter-grant matches `STARTER_GRANT_CENTS`.** Enforced by reviewer eye + the `Source of truth: STARTER_GRANT_CENTS` cross-reference embedded in the doc.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `login.unit > successful login selects the signup-created default workspace` | Hydrate path called, `Authorization: Bearer <token>` set, `saveWorkspaces` receives normalized shape. |
| `login.unit > hydration with an empty items[] response does not write workspaces.json` | `saveWorkspaces` never called, in-memory `workspaces.current_workspace_id` stays null. |
| `login.unit > hydration preserves a pre-existing current_workspace_id` | When local current_workspace_id matches a server item, it's preserved; else fall back to `items[0]`. |
| `login.unit > successful login keeps credentials when workspace hydration fails` | Login exit 0, credentials persisted, no workspace state change. |
| `onboarding-flow.integration > login from a fresh state dir selects the signup-created workspace` | End-to-end via real fetch + mock API; bearer header lands on `/v1/tenants/me/workspaces`. |
| `onboarding-flow.integration > login on a fresh state dir leaves doctor green end-to-end` | After login, `zombiectl doctor --json` reports all three checks pass; `workspace_binding_valid.ok === true`. Customer-visible acceptance. |
| `onboarding-flow.integration > login on a fresh state dir does not break when the tenant workspace list is unavailable` | 404 from mock workspaces route → login still exits 0; `loadCredentials` finds the token; `loadWorkspaces` is empty. |

---

## Acceptance Criteria

- [x] `zombiectl/src/commands/core.js` exports a `commandLogin` that calls `hydrateWorkspacesAfterLogin` after `saveCredentials` — verify: `grep -n hydrateWorkspacesAfterLogin zombiectl/src/commands/core.js`.
- [x] `bun test` green in `zombiectl/` — verify: `cd zombiectl && bun test` (361 pass / 0 fail).
- [x] `make lint` clean — verify: `make lint`.
- [x] `gitleaks detect` clean — verify: hook on every commit.
- [x] No file in diff over 350 lines — verify: standard 350L gate from `docs/gates/file-length.md`.
- [x] `docs/architecture/billing_and_byok.md` says "$5 / 500 cents" with a source-of-truth cross-reference to `tenant_billing.zig` — verify: `grep -n "STARTER_GRANT_CENTS" docs/architecture/billing_and_byok.md`.
- [x] No regression in pre-existing tests (analytics, doctor, workspace add) — verify: full `bun test` from `zombiectl/`.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Hydration call site
grep -n hydrateWorkspacesAfterLogin zombiectl/src/commands/core.js

# E2: Test suite
cd zombiectl && bun test

# E3: Lint
make lint

# E4: Doc cross-reference
grep -n "STARTER_GRANT_CENTS\|500 cents (USD \$5)" docs/architecture/billing_and_byok.md

# E5: 350-line gate (CLI files only)
git diff --name-only origin/main \
  | grep -E '^zombiectl/' \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350'
```

---

## Dead Code Sweep

N/A — pure additive change. `normalizeTenantWorkspace` and `hydrateWorkspacesAfterLogin` are both new and called from exactly one production site (`commandLogin`).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against this Test Specification. | Clean run. |
| After tests pass | `/review` | Adversarial diff review against this spec, BUN_RULES.md, RULES.md. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments on the PR diff. | All comments addressed before merge. |
| After every push | `kishore-babysit-prs` | Polls greptile, triages findings. | Stops on two consecutive empty polls. |

---

## Discovery (consult log)

- **Server endpoint already existed** — `src/http/handlers/tenant_workspaces.zig` was wired by an earlier milestone for the dashboard switcher. The CLI fix is a pure consumer; no server-side change needed. Confirms the asymmetry was a CLI gap, not a missing API.
- **Dashboard precedent** — `ui/packages/app/lib/workspace.ts:21-29` already does the same hydrate-on-load + first-workspace-fallback pattern. The CLI now follows that pattern instead of inventing a new one.
- **Starter-grant doc drift originated in M48_001** — the BYOK milestone documented "$10 starter grant" in scenario specs and the architecture doc; M11_005 (tenant billing refactor) shipped with `STARTER_GRANT_CENTS = 500` (matching the eventual product decision). The done/ specs are historical record and stay at their original `1000¢`; only live architecture docs flip.
- **Scenario math left untouched** — `scenarios/03_balance_gate.md` walks through gate trips with `1000¢ ≥ 3¢ → pass` and follow-on arithmetic. Halving the grant rewrites the entire walkthrough. Carved out as a follow-up doc milestone rather than dragged into this branch.

---

## Out of Scope

- Cancelling extra workspaces if a customer creates a duplicate via the old broken path — separate cleanup, not blocking.
- Server-side `GET /v1/tenants/me/workspaces` shape changes — handler stays as-is.
- Scenario doc rewrites for the $5 grant math — separate milestone.
- A `zombiectl workspace pull` command (manual re-hydrate) — possible v2.1 follow-up; today the recovery path is "log out, log in".
