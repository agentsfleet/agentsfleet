<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M144_001: Fleet replies regain bubbles; workspace creation reconciles

**Prototype:** v2.0.0
**Milestone:** M144
**Workstream:** 001
**Date:** Jul 25, 2026
**Status:** PENDING
**Priority:** P1 — the merged chat no longer matches its approved design, while workspace creation persists replay state the product does not need and reports duplicate names as server errors
**Categories:** API, CLI, DOCS, UI
**Batch:** B1 — standalone; one PR because both regressions came from the same merged follow-up and the owner requested one specification
**Branch:** fix/m144-bubbles-workspace-reconciliation
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none — PR #556 is merged on `main`; this workstream corrects its final behaviour
**Provenance:** LLM-drafted (GPT-5, Jul 25, 2026) from Indy's decisions, commit `68ce6a1e7`, the Jul 24 durable mockup, and current `origin/main` at `d3269188d`
**Canonical architecture:** `docs/architecture/user_flow.md` §8.4

---

## Overview

**Goal (testable):** Fleet replies render in the approved quiet left bubble without losing live pulse or reduced-motion-safe entry behaviour, and a failed workspace create informs the user then refreshes the authoritative workspace list without replay keys or automatic POST retries.
**Problem:** The evidence-first follow-up removed the fleet reply container even though the shipped Jul 24 design made fleet and operator turns compact side bubbles and reserved flat full-width rows for integration activity. Separately, workspace creation now stores client replay keys, original request fields, and session attempts to reconstruct a lost POST response. That machinery cannot help a fresh CLI invocation, and it is unnecessary for the browser: the workspace list already exposes a committed workspace after refresh. Duplicate named creates currently fall through as a 5xx instead of the conflict the user can understand.
**Solution summary:** Restore only the fleet container treatment from `68ce6a1e7`; retain current `main`'s conditional live chip, opacity-only entrance, and reduced-motion behaviour. Remove workspace-create idempotency from the API, schema migration set, browser, and CLI. Keep server-assigned IDs and names. Map the existing tenant/name uniqueness constraint to RFC 7807 `409 Conflict`; every browser create failure is shown and followed by `router.refresh()` so a committed-but-unacknowledged workspace appears in the authoritative list before the user decides whether to retry.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix: restore fleet bubbles and reconcile workspace creation
- **Intent (one sentence):** Restore the approved fleet conversation grammar and make workspace-create uncertainty recover through the workspace list rather than request replay state.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `git show 68ce6a1e7:ui/packages/app/components/domain/FleetMessageRow.tsx` and `~/.gstack/projects/agentsfleet/designs/fleet-chat-turn-bubbles-20260724/fleet-chat-redesign.{html,png}` — exact approved bubble grammar; restore the container, not stale motion.
2. `ui/packages/app/components/domain/FleetMessageRow.tsx` + its colocated test on current `main` — preserve the later live-only pulse and opacity-only entry contract while reversing the no-bubble assertion.
3. `src/agentsfleetd/http/handlers/workspaces/{lifecycle,provision,sql}.zig` + `ui/packages/app/components/layout/{useWorkspaceCreation,WorkspaceCreationProvider}.tsx` — current replay path and browser lifecycle to simplify.
4. `docs/REST_API_DESIGN_GUIDELINES.md` §§2, 5, 6 and `src/agentsfleetd/http/handlers/problem_response.zig` — document the workspace-create exception and emit the repository-standard conflict envelope.
5. `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — schema rules conflict on pre-release slot removal; the explicit owner decision and rollout gate in Discovery govern migration 035 only.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/components/domain/FleetMessageRow.tsx`; `FleetMessageRow.test.tsx` | EDIT | Restore the quiet bounded fleet bubble and reverse the evidence-first no-container regression assertions while retaining current pulse/motion tests. |
| `docs/DESIGN_SYSTEM.md` | EDIT | Make the durable transcript rule match the approved Jul 24 fleet bubble rather than the superseded open-reply wording. |
| `src/agentsfleetd/http/handlers/workspaces/lifecycle.zig`; `provision.zig`; `sql.zig` | EDIT | Remove key parsing, replay lookup/body comparison, stored request reconstruction, and idempotency columns; map duplicate explicit names to conflict. |
| `src/agentsfleetd/http/handlers/workspaces/create_integration_test.zig` | EDIT | Replace replay tests with conflict, ordinary-create, and no-header contract coverage. |
| `src/agentsfleetd/errors/error_entries.zig`; `error_registry.zig`; `error_registry_test.zig` | EDIT | Register the workspace-name conflict with dashboard-safe copy and prove its status. |
| `public/openapi/paths/workspaces.yaml`; `public/openapi.json` | EDIT/REGENERATE | Remove the header/replay contract and document the 409 response; regenerate, never hand-edit, the bundle. |
| `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | Record workspace creation as the owner-approved exception to mandatory POST replay keys: list reconciliation is its recovery contract. |
| `schema/035_workspace_create_idempotency.sql` | DELETE | Remove the three replay-only columns and tenant/key index from the pre-production migration set. |
| `schema/embed.zig` | EDIT | Remove migration version 35; versions 1–34 remain contiguous. |
| `ui/packages/app/app/(dashboard)/actions.ts`; `ui/packages/app/lib/api/workspaces.ts` | EDIT | Restore the name-only create interface and stop forwarding `Idempotency-Key`. |
| `ui/packages/app/lib/workspace-create-attempt.ts`; `workspace-create-attempt.test.ts` | DELETE | Remove session-persisted replay attempts and client UUIDv7 generation. |
| `ui/packages/app/components/layout/useWorkspaceCreation.ts`; `WorkspaceCreationProvider.tsx` | EDIT | Remove recoverable POST state; report failures and refresh/reconcile without automatic replay. |
| `ui/packages/app/tests/dashboard-actions.test.ts`; `dashboard-error-and-empty.test.tsx`; `dashboard-workspace.test.ts`; `workspace-client.test.ts`; `workspace-create.test.ts` | EDIT | Pin name-only input, no replay, 409/uncertain failure messaging, and authoritative-list refresh. |
| `ui/packages/app/tests/e2e/acceptance/workspace-create.spec.ts`; `fleet-thread.spec.ts` | EDIT | Walk duplicate-name reconciliation and preserve the authenticated fleet conversation surface. |
| `cli/src/commands/workspace.ts`; `cli/test/workspace-create.test.ts` | EDIT | Stop generating/forwarding a one-shot key; keep backend-assigned ID/name persistence and existing error semantics. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — ORP (remove every replay reference), NDC/NLR (no compatibility shim or dormant key path), UFS (shared header/error strings disappear or remain named), NSQ (workspace SQL stays schema-qualified), FLS/ITF (real PostgreSQL conflict path drains and is integration-tested), TST-NAM (behavioural test names), XCC (both Linux targets).
- `dispatch/write_zig.md` — handler/result shape, PostgreSQL drain, error and function-length rules.
- `dispatch/write_ts_adhere_bun.md` — React lifecycle and token-only UI edits.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — migration 035 removal and the explicit owner exception in Discovery.
- `docs/REST_API_DESIGN_GUIDELINES.md` §§2, 5, 6 — non-idempotent POST semantics, registered 409 with `current_state`, OpenAPI bundle discipline.
- `docs/DESIGN_SYSTEM.md` §Fleet transcripts + Motion — quiet conversation surfaces; pulse only while live; stream entry is opacity-only and reduced-motion-safe.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — workspace handler changes | Focused unit/integration tests plus both Linux cross-compiles. |
| PUB / Struct-Shape | yes — create input/outcome simplify | Remove impossible replay variants; keep the public 201 response shape. |
| File & Function Length (≤350/≤50/≤70) | yes | Simplification must shrink the workspace path; do not fold provisioning back into the handler. |
| UFS (repeated/semantic literals) | yes | One registered conflict code/copy; no duplicated header or refresh strings. |
| UI Substitution / DESIGN TOKEN | yes | Exact existing tokens from `68ce6a1e7`; no new color/radius values. |
| ERROR REGISTRY | yes — new 409 code | Add entry, exported constant, reachability test, OpenAPI response, and zero orphan codes. |
| SCHEMA | yes — migration deletion | `VERSION=0.22.0`; owner explicitly chose removal. Delete slot 35 + embed entry, add no DROP/compensating migration, and satisfy the rollout precondition before deployment. |

## Prior-Art / Reference Implementations

- **Fleet UI:** commit `68ce6a1e7` and the Jul 24 durable HTML/PNG — operator right bubble, fleet quiet left bubble, integration rows full-width; current `main` supplies the newer pulse and motion behaviour that stays.
- **Conflict response:** `src/agentsfleetd/http/handlers/problem_response.zig` and existing duplicate-name handlers — registry-owned 409 plus mandatory `current_state`, never raw PostgreSQL error text.
- **Reconciliation:** the current `WorkspaceCreationProvider` already owns navigation notices, `router.refresh()`, and `knownWorkspaceIds`; extend that source of truth instead of adding another attempt store.

## Sections (implementation slices)

### §1 — Restore the approved fleet reply container

Fleet replies are compact, left-aligned conversation turns in the same bounded `max-w-xl` stack as operator turns. The reply body regains the approved fit-content rounded border, secondary background, padding, and lower-left notch. Detailed evidence remains structured inside that body; integration/system activity remains a flat operational row. **Implementation default:** restore the fleet branch from `68ce6a1e7` only, because current `main`'s surrounding lifecycle and animation changes are newer and correct.

- **Dimension 1.1** — Fleet replies use `w-fit max-w-full rounded-lg rounded-bl-sm border border-border bg-secondary px-md py-sm`; operator styling and side alignment remain unchanged → Test `anchors a fleet turn to the left in its own quieter bubble`
- **Dimension 1.2** — A live fleet chip alone spends pulse color; resting fleet chips remain muted → Test `spends the pulse color only while a fleet reply is live`
- **Dimension 1.3** — Conversation/activity rows keep opacity-only stream entry, no slide, with the existing reduced-motion gate → Test `uses the operational opacity-only entry motion`
- **Dimension 1.4** — Long evidence wraps inside the bounded bubble and activity rows never acquire bubble treatment → Test `keeps evidence bounded without bubbling activity`

### §2 — Make the workspace API conflict-honest

`POST /v1/workspaces` remains a server-ID, optional-name, non-idempotent create. It no longer accepts or stores a replay key. The existing tenant-scoped unique name constraint is authoritative: an explicit duplicate name returns a registered RFC 7807 409, not a generic 5xx. Generated-name collisions continue their bounded server-side regeneration loop.

- **Dimension 2.1** — Ordinary named and unnamed POSTs return 201 with backend-generated `workspace_id`, `name`, and `request_id`, independent of any `Idempotency-Key` header → Test `workspace create ignores no replay contract and assigns server identity`
- **Dimension 2.2** — A second explicit name in one tenant returns 409, the workspace-name error code, and `current_state: "name_exists"`; it creates no second row and leaks no SQL detail → Test `duplicate workspace name returns conflict`
- **Dimension 2.3** — The OpenAPI source/bundle and REST guide contain no workspace replay promise/header while documenting list reconciliation; unrelated POST idempotency policy remains intact → Test `workspace create OpenAPI documents reconciliation exception`

### §3 — Reconcile browser failures from the list

The browser owns at most one in-flight create but persists no request attempt and never automatically issues a second POST. Success keeps the existing immediate navigation/local list settlement. Any returned or thrown failure is visible in the attached dialog or detached notice and triggers `router.refresh()`. Copy tells the user the workspace list was refreshed and to check it before retrying. A 409 specifically explains that the name already exists; after refresh the existing row is selectable.

- **Dimension 3.1** — Create action/client accept only optional `name`; no client ID, replay key, session storage, or recoverable attempt remains → Test `workspace create sends name only`
- **Dimension 3.2** — 409 displays duplicate-name guidance and refreshes once; the refreshed existing workspace is visible/selectable without another POST → Test `duplicate create refreshes authoritative workspace list`
- **Dimension 3.3** — A transport/5xx failure displays uncertainty guidance and refreshes once so a DB-committed workspace can appear; retry requires a new user action → Test `uncertain create reconciles before retry`
- **Dimension 3.4** — Closing the dialog may detach one in-flight request, but failure still produces a notice and refresh; success still settles/navigates once → Test `detached workspace create preserves settlement semantics`

### §4 — Remove replay storage and callers

Delete migration 035 and every API, UI, and CLI dependency on its fields/header. The CLI keeps its current create result and local-state behaviour but sends no key; a failed invocation does not persist a workspace. **Implementation default:** no forward DROP migration at `0.22.0`; already-applied development state is an environment rollout concern, not permanent product schema.

- **Dimension 4.1** — Migration 35, embed entry, replay SQL/variants, and browser attempt module are absent with zero references → Test `dead code sweep`
- **Dimension 4.2** — CLI create sends no `Idempotency-Key`, persists backend ID/name on 201, and leaves local state unchanged on failure → Test `workspace create uses backend identity without replay header`
- **Dimension 4.3** — Removing slot 35 leaves canonical migrations contiguous through 34 and a fresh database bootstraps successfully → Test `canonical migrations end contiguously at 34`

## Interfaces

```text
POST /v1/workspaces
  request:  application/json { name?: string }; server assigns workspace_id
  success:  201 { workspace_id: string, name: string, request_id: string }
  duplicate explicit name: 409 RFC 7807 + workspace-name error code
                           + current_state: "name_exists"
  retry: no automatic client replay; GET /v1/tenants/me/workspaces is authoritative

Fleet reply body
  left aligned; content-sized within max-w-xl; secondary surface + border;
  rounded large with lower-left small notch; evidence remains inside
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Response lost after commit | DB commit succeeds, browser receives network failure | Show uncertainty message, refresh list once; committed ID/name appears from GET; never replay POST automatically. |
| Duplicate explicit name | Tenant/name unique index rejects insert | 409 registered problem with `current_state=name_exists`; UI explains and refreshes; one row remains. |
| Unnamed create retried by user | No stable name exists to reconcile semantically | Refresh before enabling a user decision; a later explicit click is a new POST and may create another workspace by documented non-idempotent semantics. |
| Auth or database failure before commit | Request cannot create a row | Show the safe registered/fallback message and refresh; list remains unchanged. |
| Dialog closes in flight | Owner detaches while request is pending | Request completes once; success settles globally, failure shows global notice and refreshes. |
| Migration 35 already applied | Development migration ledger is at 35 | Do not deploy a binary expecting 34 until owner-approved dev reset/reconciliation; leave already-written nullable columns inert until that operation. |
| Production deployment status changed | Slot 35 reached production before execution | STOP; do not delete the slot or mutate production. Re-open schema strategy with Indy. |

## Invariants

1. Workspace IDs are generated only by the server — action/client types accept no ID and tests inspect the request body.
2. The browser never automatically repeats a workspace POST — controller test counts one action call across failure + refresh.
3. The workspace list is authoritative after uncertainty — every failure path invokes one router refresh and renders check-before-retry copy.
4. Duplicate names create at most one tenant row — database unique index plus integration row-count assertion.
5. Pulse color marks only a live fleet — component role/live-state test.
6. Integration/activity rows never use conversation bubble classes — component regression test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| existing `WorkspaceCreated` / workspace analytics | product | successful create only | existing coarse IDs/outcome | no names, keys, tokens, or request bodies added | existing success tests plus `duplicate workspace name returns conflict` proves no success emission on 409 |

No new event is added: list refresh is recovery, not a new funnel stage. Existing successful-create telemetry remains exactly-once because a 409 or failed response does not emit it; no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `anchors a fleet turn to the left in its own quieter bubble` | Fleet role → exact approved container token set; operator role unchanged. |
| 1.2 | unit | `spends the pulse color only while a fleet reply is live` | Resting fleet is muted; live fleet chip alone receives pulse classes. |
| 1.3 | unit | `uses the operational opacity-only entry motion` | Conversation/activity rows fade at stream duration, never slide; motion-safe gate retained. |
| 1.4 | unit | `keeps evidence bounded without bubbling activity` | Long reply wraps in bounded fit-content bubble; system row has no bubble surface. |
| 2.1 | integration | `workspace create assigns server identity without replay contract` | Named/empty bodies → distinct backend IDs and 201; no header is required. |
| 2.2 | integration | `duplicate workspace name returns conflict` | Same tenant/name twice → 201 then 409 + code/state, row count 1, no DB text. |
| 2.3 | unit | `workspace create OpenAPI documents reconciliation exception` | Bundled operation has no key parameter/replay prose and declares 409 recovery. |
| 3.1 | unit | `workspace create sends name only` | Action/client body is `{name?}`; no header, UUID generation, or session access. |
| 3.2 | e2e | `duplicate create refreshes authoritative workspace list` | Create name, submit same name → conflict message; refreshed menu contains one selectable row. |
| 3.3 | unit | `uncertain create reconciles before retry` | Rejected/5xx action → visible check-list message + one refresh + one POST total. |
| 3.4 | unit | `detached workspace create preserves settlement semantics` | Close pending dialog; failure notice + refresh or success settlement occurs once. |
| 4.1 | unit | `dead code sweep` | Repository grep finds zero replay fields/header/client-attempt symbols in workspace-create surfaces. |
| 4.2 | unit | `workspace create uses backend identity without replay header` | CLI request has no key; 201 persists returned ID/name; failure persists nothing. |
| 4.3 | integration | `canonical migrations end contiguously at 34` | Fresh schema applies every registered migration with no version gap. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Fleet reply is the approved bubble while pulse/motion remain (§1) | `cd ui/packages/app && bunx vitest run components/domain/FleetMessageRow.test.tsx` | exit 0 | P0 | |
| R2 | Workspace conflict and reconciliation path pass (§2–§3) | `cd ui/packages/app && bunx vitest run tests/workspace-create.test.ts tests/workspace-client.test.ts tests/dashboard-error-and-empty.test.tsx tests/dashboard-workspace.test.ts` | exit 0 | P0 | |
| R3 | Replay machinery is gone (§4) | `git grep -n -E 'create_idempotency_key|create_request_name|create_request_id|Idempotency-Key|idempotencyKey|recoverableAttempt' -- ':!docs/v2/**' ':!docs/REST_API_DESIGN_GUIDELINES.md'` | 0 matches | P0 | |
| R4 | OpenAPI source and bundle agree | `make check-openapi` | exit 0 | P0 | |
| S1 | Unit and integration suites pass | `make test-unit-all && make test-integration` | exit 0 | P0 | |
| S2 | Lint and schema/error guards pass | `make lint-all` | exit 0 | P0 | |
| S3 | Authenticated UI journeys pass | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts tests/e2e/acceptance/workspace-create.spec.ts tests/e2e/acceptance/fleet-thread.spec.ts` | exit 0 | P0 | |
| S4 | Zig cross-compiles | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S5 | No secrets or oversize production file | `gitleaks detect && git diff --name-only origin/main \| grep -v '\.md$' \| grep -vE '\.test\.|_test\.' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | exit 0 and no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅, production confirmed not to have applied migration 35, and development migration state reconciled under explicit owner approval → eligible for CHORE(close); otherwise return to EXECUTE or STOP on the schema gate.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `schema/035_workspace_create_idempotency.sql` | `test ! -f schema/035_workspace_create_idempotency.sql` |
| `ui/packages/app/lib/workspace-create-attempt.ts` | `test ! -f ui/packages/app/lib/workspace-create-attempt.ts` |
| `ui/packages/app/lib/workspace-create-attempt.test.ts` | `test ! -f ui/packages/app/lib/workspace-create-attempt.test.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| replay schema/header/client attempt vocabulary | `git grep -n -E 'create_idempotency_key|create_request_name|create_request_id|Idempotency-Key|idempotencyKey|recoverableAttempt' -- ':!docs/v2/**' ':!docs/REST_API_DESIGN_GUIDELINES.md'` | 0 matches |
| migration slot 35 | `git grep -n '035_workspace_create_idempotency\|version = 35' -- schema src` | 0 matches |

## Out of Scope

- Client-generated workspace IDs or changing POST to PUT — the server remains the sole identity authority.
- Automatic matching/navigation to an unnamed workspace after an uncertain response — the refreshed list is authoritative; the product does not guess which generated row belongs to the failed browser request.
- A generic idempotency service or removal of idempotency from other side-effecting POST endpoints — only workspace creation receives the documented owner-approved exception.
- Redesigning the composer, operator bubble, integration rows, evidence hierarchy, live pulse, or stream motion beyond preventing regression.
- A compensating DROP migration or unapproved shared-database mutation. Development reset/reconciliation is a rollout prerequisite; production application of slot 35 forces a new decision.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator sees the Fleet answer inside the quiet left bubble approved on Jul 24; after a flaky workspace create, the UI says what happened and the refreshed switcher shows the created ID/name if the database committed it.
2. **Preserved user behaviour** — operator bubbles, detailed evidence, compact integration rows, live-only pulse, opacity-only/reduced-motion entry, one in-flight create, success navigation, and CLI local-state persistence all keep working.
3. **Optimal-way check** — direct list reconciliation is the smallest honest recovery model because GET already returns durable workspace identity; exact response replay adds storage and cross-client state without improving the next fresh CLI invocation.
4. **Rebuild-vs-iterate** — iterate: restore one known-good style branch and delete a recent replay layer; no chat or workspace architecture rebuild is justified.
5. **What we build** — restored fleet container, registered duplicate-name conflict, failure notice + refresh, name-only create clients, and complete replay teardown.
6. **What we do NOT build** — no client IDs, response cache, attempt store, automatic retry, new workspace table, or new chat grammar.
7. **Fit with existing features** — compounds with the current stream pulse and URL-authoritative workspace navigation; must not destabilize detached creation settlement or integration activity rendering.
8. **Surface order** — API contract and UI recovery ship together; CLI follows the same simplified request in the same PR so no client keeps a phantom header.
9. **Dashboard restraint** — no new control or persistent status; one actionable failure message and the existing refreshed switcher are sufficient.
10. **Confused-user next step** — read the refreshed workspace list before retrying; on duplicate name, select the existing workspace or choose another name.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four slices: restore the visual contract, make the server failure truthful, reconcile the browser from GET, then remove all replay storage/callers. They ship as one workstream because leaving any replay caller or schema reference behind creates a false contract.
- **Alternatives considered:** keep migration 35 and replay exact responses — rejected by Indy as overkill when the list exposes committed state; use client-generated IDs/PUT — rejected because it changes identity ownership; auto-retry POST — rejected because unnamed creates are non-idempotent and can duplicate; add migration 36 DROP — rejected for this pre-production `0.22.0` teardown and because production has not been shown to contain slot 35.
- **Patch-vs-refactor verdict:** this is a **patch**: one rendering regression is reverted to its approved implementation and one recently added replay subsystem is removed back to the simpler established create/list boundary.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: Indy directed one spec on `docs/m143-library-refactor-specs`, restore the fleet bubble while preserving pulse/motion, remove replay/idempotency in favor of conflict + refresh, and never add client-generated IDs. Durable design evidence is commit `68ce6a1e7` plus `~/.gstack/projects/agentsfleet/designs/fleet-chat-turn-bubbles-20260724/`; temp OpenCode artifacts are explicitly ignored. Current `main` regression commit `c3e5b6188` tests a flat full-width fleet reply and must be selectively reversed. Migration 35 ran in shared development via deploy run `30147685896`; no production deployment was established. The source policy conflict is explicit: `docs/SCHEMA_CONVENTIONS.md` freezes applied slots, while `dispatch/write_sql.md` requires pre-2.0 removal; Indy's direct decision governs this slot, but deployment remains blocked until migration state is owner-approved/reconciled. If production facts change, STOP.
- **Metrics review** — no event added or funnel changed; successful-create telemetry remains unchanged and failures/reconciliation emit no duplicate success event.
- **Skill-chain outcomes** — `kishore-spec-new`: repository, mockup, historical commit, current main, API/UI/CLI/schema paths, and rollout consequence reviewed; implementation `/write-unit-test`, `/review`, and `kishore-babysit-prs` outcomes populate at CHORE(close).
- **Deferrals** — none.
