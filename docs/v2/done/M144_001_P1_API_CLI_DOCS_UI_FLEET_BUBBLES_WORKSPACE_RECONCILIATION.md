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

# M144_001: Fleet chat becomes quiet; workspace creation reconciles

**Prototype:** v2.0.0
**Milestone:** M144
**Workstream:** 001
**Date:** Jul 25, 2026
**Status:** DONE
**Priority:** P1 — the merged chat no longer matches its approved design, while workspace creation persists replay state the product does not need and reports duplicate names as server errors
**Categories:** API, command-line interface (CLI), documentation (DOCS), user interface (UI)
**Batch:** B1 — standalone; one Pull Request (PR) because both regressions came from the same merged follow-up and the owner requested one specification
**Branch:** fix/fleet-bubbles-workspace-reconciliation — the worktree at `~/Projects/agentsfleet-fleet-bubbles-reconciliation` already exists on this branch; do not create a second one
**Test Baseline:** `unit=2958 integration=393` via `make _lint_zig_test_depth`
**Depends on:** none — PR #556 is merged on `main`; this workstream corrects its final behaviour
**Provenance:** drafted by a Large Language Model (LLM), Generative Pre-trained Transformer 5 (GPT-5), on Jul 25, 2026, from Indy's decisions, commit `68ce6a1e7`, the Jul 24 durable mockup, and current `origin/main` at `d3269188d`
**Canonical architecture:** `docs/architecture/user_flow.md` §8.4

---

## Overview

**Goal (testable):** Fleet replies render as approved quiet open text, the compact transcript header names connection state while saved history remains usable, and every workspace create uses a required tenant-unique name so response loss can be reconciled through an exact-name lookup without replay keys or automatic POST retries.
**Problem:** The earlier chat treatment accumulated sender chrome, repeated action rows, warning color, and bubble containers until operational failures became harder to scan than the conversation. On chat load, saved history already renders while the live stream connects, so connection context must be clear without inserting another banner into the transcript. Separately, workspace creation stores client replay keys, original request fields, and session attempts to reconstruct a lost POST response. That machinery cannot help a fresh CLI invocation, and it is unnecessary for the browser: the workspace list already exposes a committed workspace after refresh. Duplicate named creates currently fall through as a 5xx instead of the conflict the user can understand.
**Solution summary:** Keep operator turns as restrained right bubbles while fleet replies use open, left-aligned text in the reading column; integration activity remains compact and full width. Use the existing transcript header for Connecting, Reconnecting, Live, and Offline, with no second status band. Failure rows use calm registered copy, one compact detail disclosure, and no repeated edit-instructions action line. Remove workspace-create idempotency from the API, schema migration set, browser, and CLI. Keep server-assigned IDs and require caller-supplied names. Map the existing tenant/name uniqueness constraint to Request for Comments (RFC) 7807 `409 Conflict`; every browser create failure is shown and followed by `router.refresh()`, while the command-line interface resolves a registered duplicate or uncertain response through one tenant-scoped exact-name GET. The tenant workspace endpoint uses cursor pagination so every workspace remains reachable without an unbounded response. Retrying the same name cannot create a second row.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix: simplify fleet chat and reconcile workspace creation
- **Intent (one sentence):** Make fleet chat quiet and scannable, and make workspace-create uncertainty recover through the workspace list rather than request replay state.
- **Handshake** — Keep chat visually quiet and make workspace-create uncertainty resolve through truthful conflict responses and the authoritative workspace list, with no replay state. `ASSUMPTIONS I'M MAKING:` `app-dev` is the only deployed database to reconcile; production never applied migration 35; PlanetScale credentials are available through 1Password; Indy's instruction authorizes removing the migration-35 objects from `app-dev`; and a larger refactor is chosen only if tracing proves it materially improves security, reliability, concurrency, or performance.

## Implementing agent — read these first

1. `ui/packages/app/components/domain/FleetMessageRow.tsx` + its colocated test — final approved grammar: operator bubble, open fleet reply, compact integration evidence.
2. `docs/DESIGN_SYSTEM.md` §Fleet transcripts — durable open-reply rule and restrained operational evidence hierarchy.
3. `src/agentsfleetd/http/handlers/workspaces/{lifecycle,provision,sql}.zig` + `ui/packages/app/components/layout/{useWorkspaceCreation,WorkspaceCreationProvider}.tsx` — current replay path and browser lifecycle to simplify.
4. `docs/REST_API_DESIGN_GUIDELINES.md` §§2, 5, 6 and `src/agentsfleetd/http/handlers/problem_response.zig` — document the workspace-create exception and emit the repository-standard conflict envelope.
5. `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — schema rules conflict on pre-release slot removal; the explicit owner decision and rollout gate in Discovery govern migration 035 only.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/components/domain/FleetMessageRow.tsx`; `FleetMessageRow.test.tsx`; `fleetFailureCopy.tsx`; `fleetMessageRenderers.tsx` | EDIT | Keep operator turns bounded, render fleet replies as quiet open text, and make failures compact without a repeated action line. |
| `ui/packages/app/components/domain/FleetThread.tsx`; `FleetConnectionNotice.tsx`; `FleetConnectionNotice.test.tsx`; `ui/packages/app/tests/fleet-thread.test.ts` | EDIT | Keep saved history visible and name connection state in the compact header; preserve Live and Offline behavior without a second banner. |
| `docs/DESIGN_SYSTEM.md` | EDIT | Make the durable transcript rule match the final approved open fleet reply. |
| `docs/architecture/user_flow.md` | EDIT | Keep the canonical dashboard flow aligned with quiet open fleet replies, cursor pagination, and exact-name create recovery. |
| `src/agentsfleetd/http/handlers/workspaces/lifecycle.zig`; `provision.zig`; `sql.zig` | EDIT | Remove key parsing, replay lookup/body comparison, stored request reconstruction, generated-name retries, and idempotency columns; require a name and map duplicates to conflict. |
| `src/agentsfleetd/http/handlers/workspaces/create_integration_test.zig` | EDIT | Replace replay and generated-name tests with required-name validation, conflict, ordinary-create, and no-header coverage. |
| `src/agentsfleetd/http/handlers/common_authz.zig`; `common.zig`; `tenant_workspaces.zig`; `tenant_workspaces_integration_test.zig` | EDIT | Resolve the database subject mapping before the token claim, paginate in stable oldest-to-newest order, support exact-name equality, and prove tenant isolation plus equal-time cursor ties. |
| `public/openapi/paths/tenant-workspaces.yaml`; `ui/packages/app/lib/api/workspaces.ts`; `workspaces.test.ts`; dashboard layouts/switcher/routing tests | EDIT | Document and strictly decode every cursor page so the dashboard owns a complete workspace list and route guards can decide from authoritative data. |
| `scripts/mint-scope-personas.mjs`; `src/agentsfleetd/http/test_scope_tokens.zig` | EDIT/REGENERATE | Give the reconciliation test a private signed persona so its database identity cannot race with other integration suites. |
| `src/agentsfleetd/errors/error_entries.zig`; `error_registry.zig`; `error_registry_test.zig` | EDIT | Register the workspace-name conflict with dashboard-safe copy and prove its status. **Register it with `eu()`, not `e()`** — the browser create dialog renders it, so it needs a user message, and the registry lint rejects a reachable `e()` entry that lacks one. Prior art is `UZ-AGT-006` "Fleet name already exists" (`error_entries.zig:169`), which is the same shape one namespace over; no `UZ-WS-*` or `UZ-WORKSPACE-*` namespace exists yet, so the implementing agent opens one and records the chosen code in Discovery rather than inventing it at the call site. |
| `src/agentsfleetd/errors/gen_error_codes.zig`; `gen_error_codes_test.zig` | EDIT | Publish and pin the new workspace error category in generated error-code documentation. |
| `public/openapi/paths/workspaces.yaml`; `public/openapi.json` | EDIT/REGENERATE | Remove the header/replay promise and document the 409 response; regenerate, never hand-edit, the bundle. |
| `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | Record workspace creation as the owner-approved exception to mandatory POST replay keys: list reconciliation is its recovery model. |
| `schema/035_workspace_create_idempotency.sql` | DELETE | Remove the three replay-only columns and tenant/key index from the pre-production migration set. |
| `schema/038_tenant_workspace_list_index.sql` | CREATE | Add the composite tenant/time index required by cursor pagination after main's migrations 36 and 37, while leaving removed slot 35 absent. |
| `schema/embed.zig` | EDIT | Remove replay migration version 35, retain main's versions 36 and 37, and register the pagination index as version 38. |
| `src/agentsfleetd/cmd/common.zig`; `src/agentsfleetd/db/pool.zig`; `src/agentsfleetd/db/pool_migrations.zig`; `src/agentsfleetd/db/pool_migration_state.zig`; `src/agentsfleetd/fleet/schema_ahead_migration_test.zig`; `src/agentsfleetd/tests.zig` | EDIT/CREATE | Refuse an ahead PostgreSQL ledger under the canonical migration lock before cleanup, preserve the low-level custom-migration reaper, split read-only ledger inspection from mutation, and register the non-mutating proof in test discovery. |
| `ui/packages/app/app/(dashboard)/actions.ts`; `ui/packages/app/lib/api/workspaces.ts` | EDIT | Require a name in the create interface and stop forwarding `Idempotency-Key`. |
| `ui/packages/app/lib/workspace-create-attempt.ts`; `workspace-create-attempt.test.ts` | DELETE | Remove session-persisted replay attempts and client UUIDv7 generation. |
| `ui/packages/app/components/layout/CreateWorkspaceDialog.tsx`; `useWorkspaceCreation.ts`; `WorkspaceCreationProvider.tsx` | EDIT | Require a one-to-128-code-point name, remove recoverable POST state, and refresh/reconcile failures without automatic replay. |
| `ui/packages/app/tests/create-workspace-dialog.test.ts`; `dashboard-actions.test.ts`; `dashboard-error-and-empty.test.tsx`; `dashboard-workspace.test.ts`; `workspace-client.test.ts`; `workspace-create.test.ts` | EDIT | Pin required-name input, no replay, 409/uncertain failure messaging, and authoritative-list refresh. |
| `ui/packages/app/tests/e2e/acceptance/workspace-create.spec.ts`; `operator-journey.spec.ts`; `fleet-thread.spec.ts` | EDIT | Walk required-name and duplicate-name reconciliation while preserving authenticated workspace and fleet journeys. |
| `cli/src/program/cli-tree.ts`; `cli/src/commands/{workspace,workspace-create-reconcile,workspace-guards,login-helpers,workspace-response-decoders,core-ops}.ts`; related CLI tests/state types | EDIT/CREATE | Require the create name, strictly decode canonical server responses, remove replay state, reconcile through one encoded exact-name GET, and replace local workspace state when the API resolves a different authoritative tenant. |
| `cli/src/services/http-client.ts`; `cli/test/services-http-client.unit.test.ts` | EDIT | Give the registered duplicate-name conflict executable list-or-rename guidance instead of the generic retry suggestion. |
| `cli/src/output/format.ts`; `cli/test/output-format.unit.test.ts` | EDIT | Strip terminal control characters from table and key/value cells so server-provided workspace names cannot forge command-line output. |
| `cli/README.md` | EDIT | Keep the command reference aligned with the required workspace name. |
| `cli/test/acceptance/fixtures/workspace-ops.ts` | EDIT | Keep command acceptance documentation aligned with the required `<name>` positional. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — Orphan sweep (ORP) removes every replay reference; No Dead Code (NDC) and No Legacy Retained (NLR) forbid compatibility shims or dormant key paths; Unified Form for Symbols (UFS) keeps shared header/error strings named; Named constants and Schema-qualified SQL (NSQ) keeps workspace SQL qualified; Flush all Layers (FLS) and Integration Test Fixtures (ITF) require a drained, real-PostgreSQL conflict path; Test Naming (TST-NAM) keeps behavioural names; Cross-Compile (XCC) covers both Linux targets.
- `dispatch/write_zig.md` — handler/result shape, PostgreSQL drain, error and function-length rules.
- `dispatch/write_ts_adhere_bun.md` — React lifecycle and token-only UI edits.
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — migration 035 removal and the explicit owner exception in Discovery.
- `docs/REST_API_DESIGN_GUIDELINES.md` §§2, 3, 5, 6 — non-idempotent POST semantics, cursor pagination and equality filtering, registered 409 with `current_state`, OpenAPI bundle discipline.
- `docs/DESIGN_SYSTEM.md` §Fleet transcripts + Motion — quiet conversation surfaces; pulse only while live; stream entry is opacity-only and reduced-motion-safe.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — workspace handler changes | Focused unit/integration tests plus both Linux cross-compiles. |
| Public Surface (PUB) / Struct-Shape | yes — create/list outcomes change | Remove impossible replay variants; expose the authoritative tenant and standard cursor response without compatibility aliases. |
| File & Function Length (≤350/≤50/≤70) | yes | Simplification must shrink the workspace path; do not fold provisioning back into the handler. |
| UFS (repeated/semantic literals) | yes | One registered conflict code/copy; no duplicated header or refresh strings. |
| UI Substitution / DESIGN TOKEN | yes | Exact existing tokens from `68ce6a1e7`; no new color/radius values. |
| ERROR REGISTRY | yes — new 409 code | Add the entry via `eu()` so the dashboard has a user message, plus exported constant, reachability test, OpenAPI response, and zero orphan codes. The code itself is chosen at CHORE(open) in a new workspace namespace and recorded in Discovery; `UZ-AGT-006` is the shape to copy. |
| SCHEMA | yes — migration deletion and pagination index | `VERSION=0.22.0`; owner explicitly chose removal. Leave slot 35 absent, retain main's slots 36 and 37, register the independent index as slot 38, and add no DROP migration. |

## Prior-Art / Reference Implementations

- **Fleet UI:** `docs/DESIGN_SYSTEM.md` and the final approved local review — operator right bubble, fleet open left reply, compact integration rows, calm failures, and connection state in the header.
- **Conflict response:** `src/agentsfleetd/http/handlers/problem_response.zig` and existing duplicate-name handlers — registry-owned 409 plus mandatory `current_state`, never raw PostgreSQL error text.
- **Reconciliation:** the current `WorkspaceCreationProvider` already owns navigation notices, `router.refresh()`, and `knownWorkspaceIds`; extend that source of truth instead of adding another attempt store.

## Sections (implementation slices)

### §1 — Keep fleet replies open and quiet — **DONE**

Fleet replies are open, left-aligned conversation turns in the centered reading column. They use no surrounding card, border, bubble background, or repeated visible sender/time chrome; the sender remains available to assistive technology. Operator turns retain the restrained bounded bubble. Detailed evidence remains structured with the reply; integration/system activity remains a compact operational row. Failure copy uses the registered calm foreground treatment, one error glyph, and one adjacent detail disclosure without repeating a remediation action on every row.

- **Dimension 1.1 — DONE** — Fleet replies stay open and left aligned with no rounded, bordered, or filled container; operator styling and side alignment remain unchanged → Test `keeps a fleet reply open and left aligned`
- **Dimension 1.2 — DONE** — Fleet replies omit repeated visible sender/time chrome while retaining the sender for assistive technology; pulse color remains reserved for live connection indicators → Tests `renders a fleet reply as open text without conversation chrome` and `retains a non-operator sender for assistive technology`
- **Dimension 1.3 — DONE** — Conversation/activity rows keep opacity-only stream entry, no slide, with the existing reduced-motion gate → Test `uses the operational opacity-only entry motion`
- **Dimension 1.4 — DONE** — Long replies stay inside the reading column; activity rows remain compact; failure rows have one calm error line and no repeated edit action → Tests `keeps a long body inside its own row rather than widening the page` and `names the failing check and what to do about it on a startup failure`

### §2 — Connecting is explicit without hiding history — **DONE**

Saved server-rendered history remains visible and scrollable while the live stream connects; replacing it with a loading screen would discard useful context. The compact header indicator names Connecting, Reconnecting, Live, and Offline without inserting a second banner above the conversation. Offline alone retains an actionable Reconnect notice that accurately restarts the stream.

- **Dimension 2.1 — DONE** — Connecting/reconnecting with existing history names the state in the header while durable rows and the composer remain usable, with no duplicate band → Tests `names each connection state rather than only the live one` and `shows motion while connecting, and no band above the conversation`
- **Dimension 2.2 — DONE** — Live is named directly, Offline offers Reconnect, and reduced motion still communicates every state through text → Test `keeps a band only for a connection that asks the operator to decide`

### §3 — Make the workspace API conflict-honest — **DONE**

`POST /v1/workspaces` remains a server-ID create and now requires a non-blank name. It no longer accepts or stores a replay key. The existing tenant-scoped unique name constraint is authoritative: a duplicate name returns a registered RFC 7807 409, not a generic 5xx. There is no generated-name branch or retry loop.

- **Dimension 3.1 — DONE** — A named POST made with a login token resolves the database subject mapping before a stale token tenant claim; tenant API keys remain bound to the tenant recorded for the key. A successful create returns 201 with authoritative `tenant_id`, backend-generated `workspace_id`, and `request_id`; missing, blank, unsafe, or over-128-code-point names return 400 before a database write → Tests `workspace create assigns server identity without replay state`, `workspace create uses the database tenant mapping over a stale claim`, `workspace create keeps API keys bound to their issuing tenant`, and `workspace create validates the workspace name`
- **Dimension 3.2 — DONE** — One hundred simultaneous creates for the same explicit tenant/name yield exactly one 201 and ninety-nine 409 responses carrying `UZ-WORKSPACE-001` and `current_state: "name_exists"`; one row remains, requests overlap in the server, and no SQL detail leaks → Test `concurrent duplicate workspace names create exactly one row`
- **Dimension 3.3 — DONE** — The OpenAPI source/bundle and Representational State Transfer (REST) guide require `name`, contain no workspace replay promise/header, document same-name reconciliation, and expose exact-name filtering plus standard cursor pagination. The list reports `total: null` rather than a page count and carries the authoritative `tenant_id` as the documented security exception to the standard list envelope; unrelated POST idempotency policy remains intact → Test `workspace create OpenAPI documents reconciliation exception`

### §4 — Reconcile browser failures from the list — **DONE**

The browser owns at most one in-flight create but persists no request attempt and never automatically issues a second POST. Success keeps the existing immediate navigation/local list settlement. Any returned or thrown failure is visible in the attached dialog or detached notice and triggers `router.refresh()` in a React transition. Copy tells the user the workspace list is refreshing and to check it before retrying; creation remains locked until the transition settles. A 409 specifically explains that the name already exists and tells the operator to check the refreshed list or choose another name.

- **Dimension 4.1 — DONE** — Dialog, action, controller, and client require a non-blank `name` after trimming only the ASCII whitespace recognized by the server; no client ID, replay key, session storage, or recoverable attempt remains → Tests `workspace create requires a name` and `workspace create sends name only`
- **Dimension 4.2 — DONE** — 409 displays duplicate-name guidance and refreshes once without issuing another POST → Test `duplicate create refreshes authoritative workspace list`
- **Dimension 4.3 — DONE** — A transport/5xx failure displays uncertainty guidance and refreshes once; the dashboard consumes every default-50/max-100 cursor page in stable oldest-to-newest order, including identical creation-time ties, and retry requires a new user action → Tests `uncertain create reconciles before retry` and `tenant workspaces paginate without skipping equal-time rows`
- **Dimension 4.4 — DONE** — Closing the dialog may detach one in-flight request, but failure still produces a notice and refresh; success still settles/navigates once → Test `detached workspace create preserves settlement semantics`

### §5 — Remove replay storage and callers — **DONE**

Delete migration 035 and every API, UI, and CLI dependency on its fields/header. The CLI sends no key and persists a create only after either the POST returns or one authoritative exact-name GET confirms the tenant-unique name. **Implementation default:** no forward DROP migration at `0.22.0`; already-applied development state is an environment rollout concern, not permanent product schema.

- **Dimension 5.1 — DONE** — The replay migration, replay SQL/variants, and browser attempt module are absent with zero references → Test `dead code sweep`
- **Dimension 5.2 — DONE** — CLI syntax requires a safe one-to-128-code-point `<name>` using the server's ASCII-only edge trimming, sends no `Idempotency-Key`, strictly decodes canonical create/list responses, persists backend ID/name/tenant after 201 or one encoded exact-name reconciliation GET, replaces stale local tenant state, labels reconciliation as selection rather than creation, preserves the original error on no match or malformed reconciliation data, and strips terminal or line-separator controls from structured output → Focused create/recovery/state/output tests
- **Dimension 5.3 — DONE** — Slot 35 remains absent, main's versions 36 and 37 remain intact, version 38 contains only the composite pagination index using the standard transactional migration style, and a fresh database bootstraps successfully through the intentional gap → Tests `canonical schema bootstrap: removed replay slot stays absent` and `workspace list index migration uses the standard transactional index creation`
- **Dimension 5.4 — DONE** — A database ahead of the canonical migration set is refused rather than silently accepted → Test `a ledger ahead of the binary is refused, not ignored`

**Slot 35 remains absent.** Main already owns versions 36 and 37, so the pagination index uses version 38. A persistent database carrying the removed version cannot silently treat different SQL as already applied.

**The migration ledger stores versions, not SQL digests.** Reusing version 35 would therefore be unsafe even after app-dev reconciliation. Version 38 makes the replacement unambiguous: canonical-version membership identifies 35 as removed and 38 as pending, while Dimension 5.4 separately proves that a ledger genuinely ahead of the binary is refused before cleanup.

## Interfaces

```text
POST /v1/workspaces
  request:  application/json { name: string }; name is non-blank; server assigns workspace_id
  success:  201 { workspace_id: string, tenant_id: string, name: string, request_id: string }
  missing or blank name: 400 registered invalid-request response
  duplicate name: 409 RFC 7807 + workspace-name error code
                  + current_state: "name_exists"
  recovery: no automatic client replay; list workspaces; a deliberate
            same-name retry cannot create another row

GET /v1/tenants/me/workspaces?name=<exact>&starting_after=<cursor>&limit=<1..100>
  success: 200 { items, tenant_id, total: null, next_cursor }
  order:   created_at ASC, workspace_id ASC; default limit 50

Fleet reply body
  left aligned; open text inside the centered reading column; no surrounding
  border or bubble fill; evidence remains attached to the same turn
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Saved history looks live | Initial rows render before the stream reaches Live | Compact header identifies connecting/reconnecting; rows and composer remain usable without a transcript band. |
| Response lost after commit | DB commit succeeds, browser or CLI receives network failure | Browser shows uncertainty and refreshes every cursor page; CLI performs one exact-name GET and selects a match. A deliberate same-name retry returns 409 and cannot create another row. |
| Duplicate name | Tenant/name unique index rejects insert | 409 registered problem with `current_state=name_exists`; UI explains and refreshes; one row remains. |
| Missing or blank name | Caller submits no usable retry identity | Return 400 before acquiring a database connection; UI and CLI prevent this request through required inputs. |
| Stale token tenant claim | The identity provider claim lags `core.users.tenant_id` | Login-token create and list use the database subject mapping; tenant API keys stay bound to their issuing tenant. The response carries the authoritative tenant so CLI state cannot mix tenants. |
| Auth or database failure before commit | Request cannot create a row | Show the safe registered/fallback message and refresh; list remains unchanged. |
| Dialog closes in flight | Owner detaches while request is pending | Request completes once; success settles globally, failure shows global notice and refreshes. |
| Replay migration 35 already applied | Development migration ledger identifies the removed replay migration | The owner-authorized app-dev transaction removes the replay index, three replay columns, and old ledger row; the independent index migration then applies as version 38. |
| Production deployment status changed | Slot 35 reached production before execution | STOP; do not delete the slot or mutate production. Re-open schema strategy with Indy. |

## Invariants

1. Workspace IDs are generated only by the server — action/client types accept no ID and tests inspect the request body.
2. Every workspace create has a non-blank caller-supplied name — API validation, UI form rules, CLI syntax, and type signatures agree.
3. The browser never automatically repeats a workspace POST — controller test counts one action call across failure + refresh.
4. Workspace reads are page-bounded after uncertainty — every browser failure invokes one refresh, every page is at most 100 rows, exact-name CLI recovery performs one GET, and the composite cursor never skips equal-time rows.
5. Duplicate names create at most one tenant row under 100-way contention — the database unique index is the sole race arbiter; the integration test asserts one 201, ninety-nine 409 responses, one row, and overlapping server work.
6. Pulse color appears only on live connection indicators — component state tests.
7. Fleet and integration rows never acquire operator bubble classes — component regression test.
8. Connecting never hides durable history or masquerades as Live — header-state tests assert named status while rows remain visible.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| existing `WorkspaceCreated` / workspace analytics | product | successful create only | existing coarse IDs/outcome | no names, keys, tokens, or request bodies added | existing success tests plus `duplicate workspace name returns conflict` proves no success emission on 409 |

No new event is added: list refresh is recovery, not a new funnel stage. Existing successful-create telemetry remains exactly-once because a 409 or failed response does not emit it; no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `keeps a fleet reply open and left aligned` | Fleet role → no rounded, bordered, or filled container; operator role unchanged. |
| 1.2 | unit | `renders a fleet reply as open text without conversation chrome`; `retains a non-operator sender for assistive technology` | Fleet reply omits visible sender/time chrome, keeps an accessible sender label, and spends no pulse color. |
| 1.3 | unit | `uses the operational opacity-only entry motion` | Conversation/activity rows fade at stream duration, never slide; motion-safe gate retained. |
| 1.4 | unit | `keeps a long body inside its own row rather than widening the page`; `names the failing check and what to do about it on a startup failure` | Long reply stays in the reading column; failure row has one calm error treatment and no repeated remediation action. |
| 2.1 | unit | `names each connection state rather than only the live one`; `shows motion while connecting, and no band above the conversation` | Connecting/reconnecting + initial rows → named header state, no duplicate band, history and composer remain usable. |
| 2.2 | unit | `keeps a band only for a connection that asks the operator to decide` | Live has no band; Offline has Reconnect; every state has text independent of motion. |
| 3.1 | integration | `workspace create assigns server identity without replay state`; `workspace create uses the database tenant mapping over a stale claim`; `workspace create keeps API keys bound to their issuing tenant`; `workspace create validates the workspace name` | Named body → authoritative tenant, backend ID, and 201; a stale login claim writes only to the mapped tenant while an API key remains with its issuing tenant; missing, blank, unsafe, or overlong name → 400 and no row. |
| 3.2 | integration | `concurrent duplicate workspace names create exactly one row` | One hundred simultaneous same-tenant/name requests → one 201, ninety-nine 409 responses with `UZ-WORKSPACE-001` and `name_exists`, row count 1, server peak in-flight at least 2, and no DB text. |
| 3.3 | unit | `workspace create OpenAPI documents reconciliation exception` | Bundled operation requires `name`, has no key parameter/replay prose, declares 409 recovery, and documents the nullable total plus authoritative-tenant list-envelope exception. |
| 4.1 | unit | `workspace create requires a name`; `workspace create sends name only` | Empty UI form cannot submit; ASCII edge whitespace is removed while other Unicode whitespace is preserved; action/client body is `{name}` with no header, universally unique identifier (UUID) generation, or session access. |
| 4.2 | e2e | `duplicate create refreshes authoritative workspace list` | Create name, submit same name → conflict message; refreshed menu contains one selectable row. |
| 4.3 | unit + integration | `uncertain create reconciles before retry`; `tenant workspaces paginate without skipping equal-time rows` | Rejected/5xx action → visible check-list message + one refresh + one POST total; pages cap at 100, traverse oldest-to-newest through `next_cursor`, and neither skip nor duplicate equal-time rows. |
| 4.4 | unit | `detached workspace create preserves settlement semantics` | Close pending dialog; failure notice + refresh or success settlement occurs once. |
| 5.1 | repo-grep | `dead code sweep` | Repository grep finds zero replay fields/header/client-attempt symbols in workspace-create surfaces. Graded by rubric R3, not by a test runner — it lives in no test file by design. |
| 5.2 | unit | focused CLI create/recovery/state/output tests | Invalid names fail before dispatch; request has no key; success or exact-name recovery persists authoritative tenant identity; a tenant mismatch replaces stale local rows; no match preserves the original failure; output contains no terminal or line-separator controls. |
| 5.3 | integration + unit | `canonical schema bootstrap: removed replay slot stays absent`; `workspace list index migration uses the standard transactional index creation` | Fresh schema applies every registered migration through version 38; slot 35 remains absent and canonical versions remain strictly increasing; version 38 uses the repository's normal `CREATE INDEX IF NOT EXISTS` migration form. |
| 5.4 | integration | `a ledger ahead of the binary is refused, not ignored` | A database whose ledger records a version greater than the canonical maximum is detected at startup and reported; it never boots as if reconciled. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Fleet reply and connection context match the approved chat outcome (§1–§2) | `cd ui/packages/app && bunx vitest run components/domain/FleetMessageRow.test.tsx tests/fleet-thread.test.ts` | exit 0 | P0 | ✅ `Test Files 2 passed (2); Tests 91 passed (91)` |
| R2 | Workspace conflict and reconciliation path pass (§3–§4) | `cd ui/packages/app && bunx vitest run tests/workspace-create.test.ts tests/workspace-client.test.ts tests/dashboard-error-and-empty.test.tsx tests/dashboard-workspace.test.ts` | exit 0 | P0 | ✅ `Test Files 4 passed (4); Tests 45 passed (45)` |
| R3 | Replay machinery is gone (§5) | `git grep -n -E 'create_idempotency_key|create_request_name|create_request_id|Idempotency-Key|idempotencyKey|recoverableAttempt' -- ':!docs/v2/**' ':!docs/REST_API_DESIGN_GUIDELINES.md'` | 0 matches | P0 | ✅ `0 matches` |
| R4 | OpenAPI source and bundle agree | `make check-openapi` | exit 0 | P0 | ✅ `OpenAPI validation passed` |
| S1 | Unit and integration suites pass | `make test-unit-all && make test-integration` | exit 0 | P0 | ✅ `✓ [agentsfleetd] All integration tests passed` |
| S2 | Lint and schema/error guards pass | `make lint-all` | exit 0 | P0 | ✅ `ALL CHECKS PASSED` |
| S3 | Authenticated UI journeys pass | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts tests/e2e/acceptance/workspace-create.spec.ts tests/e2e/acceptance/fleet-thread.spec.ts` | exit 0 | P0 | ✅ Indy completed the dashboard eyeball and explicitly removed the localhost dashboard requirement. |
| S4 | Zig cross-compiles | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ `exit 0` for both Linux targets |
| S5 | No secrets or oversize production file | `gitleaks detect && git diff --name-only origin/main \| grep -v '\.md$' \| grep -v '^public/' \| grep -vE '\.test\.\|_test\.' \| xargs wc -l 2>/dev/null \| awk '$2!="total" && $1>350 {print; c++} END {exit c?1:0}'` | exit 0 | P0 | ✅ `no leaks found; 0 oversize production files` |

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
| replay migration identity | `git grep -n '035_workspace_create_idempotency' -- schema src` | 0 matches |

## Out of Scope

- Client-generated workspace IDs or changing POST to PUT — the server remains the sole identity authority.
- Generated workspace names — signup may keep its own default-workspace naming, but the public create API, UI, and CLI require an explicit name.
- A generic idempotency service or removal of idempotency from other side-effecting POST endpoints — only workspace creation receives the documented owner-approved exception.
- Redesigning the composer, operator bubble, evidence hierarchy, live pulse, or stream motion beyond the approved simplification.
- A compensating DROP migration or any production mutation. Indy authorized direct app-dev reconciliation for slot 35 in this workstream; production application of slot 35 forces a new decision.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reads the Fleet answer as quiet open text without card noise, sees connection state in the header, and after a flaky workspace create sees truthful guidance plus the committed workspace in the refreshed switcher.
2. **Preserved user behaviour** — saved history remains visible and the composer usable while connecting; operator bubbles, detailed evidence, compact integration rows, live-only pulse, opacity-only/reduced-motion entry, one in-flight create, success navigation, and CLI local-state persistence all keep working.
3. **Optimal-way check** — a required name plus the existing tenant/name unique index is the smallest honest recovery model. GET exposes committed identity, and a same-name retry cannot duplicate the row.
4. **Rebuild-vs-iterate** — refactor the create boundary to make invalid unnamed state unrepresentable, simplify the transcript hierarchy, and delete the recent replay layer.
5. **What we build** — open fleet replies, compact header connection state, calm failure rows, required workspace names, registered duplicate-name conflict, failure notice + refresh, and complete replay teardown.
6. **What we do NOT build** — no client IDs, response cache, attempt store, automatic retry, new workspace table, or new chat grammar.
7. **Fit with existing features** — compounds with the current stream pulse and URL-authoritative workspace navigation; must not destabilize detached creation settlement or integration activity rendering.
8. **Surface order** — API behavior and UI recovery ship together; CLI follows the same simplified request in the same PR so no client keeps a phantom header.
9. **Dashboard restraint** — no new control or persistent status; one actionable failure message and the existing refreshed switcher are sufficient.
10. **Confused-user next step** — read the refreshed workspace list; select the existing workspace or choose another name.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five slices: simplify the visual rules, clarify connection startup without hiding history, make the server failure truthful, reconcile the browser from GET, then remove all replay storage/callers. They ship as one workstream because this is the single corrected chat/create outcome and leaving any replay caller or schema reference behind creates a false interface promise.
- **Alternatives considered:** keep migration 35 and replay exact responses — rejected by Indy as overkill when the list exposes committed state; use client-generated IDs/PUT — rejected because it changes identity ownership; retain generated names plus refresh locking — rejected because the original POST can outlive the client timeout and commit after the refresh; auto-retry POST — rejected because it can duplicate; add migration 36 DROP — rejected for this pre-production `0.22.0` teardown and because production has not been shown to contain slot 35.
- **Patch-vs-refactor verdict:** use a focused interface refactor. Make `name` required across API, UI, and CLI; delete the generated-name branch and replay subsystem; retain the existing create/list boundary and database uniqueness arbiter.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: Indy directed one spec on `docs/m143-library-refactor-specs`, remove replay/idempotency in favor of conflict + refresh, and never add client-generated IDs. The chat design then went through live visual review: the final approved Variant A keeps the operator bubble, uses open left-aligned fleet text, keeps connection state in the compact header, removes the repeated edit-instructions line, and gives failures one calm glyph plus one adjacent detail disclosure. That final decision supersedes the earlier Jul 24 fleet-bubble mockup and the intermediate connecting band. Migration 35 ran only in shared development via deploy run `30147685896`. The source policy conflict is explicit: `docs/SCHEMA_CONVENTIONS.md` freezes applied slots, while `dispatch/write_sql.md` requires pre-2.0 removal; Indy's direct decision governs this slot.
- **Chief Technology Officer review** — A required-name refactor wins over timeout choreography or a workspace-subsystem rewrite. Remove replay parsing, lookup, response reconstruction, client key generation, persisted attempts, generated-name retries, and optional-name types. Retain one server `INSERT`, the existing tenant/name unique index as the concurrency boundary, and exact constraint-name classification. This removes replay queries and the response-loss duplicate race without an application lock or a time-of-check/time-of-use race. Security stays tenant-derived and server-ID-only; reliability is proved by required-name validation, registered conflict responses, browser list reconciliation, and 100-way real-database contention.
- **Red-team correction** — One refresh cannot prove that a timed-out POST has stopped; the original insert may commit after the refresh and after the UI unlocks. Requiring a name gives every create a natural tenant-scoped retry identity, so a late commit and a deliberate same-name retry still produce one row.
- **Adversarial corrections** — The standalone migrate command now inspects and refuses an ahead ledger before orphan-row cleanup can erase that evidence. The former fixed 200-row snapshot is replaced by composite-key cursor pagination plus exact-name equality; dashboard and login consumers walk all pages, while CLI create recovery performs one filtered GET. Create resolves the database subject mapping before a stale token claim and returns that tenant identifier so local state cannot mix principals.
- **Migration-38 rollout** — Development deploys run the existing Fly migration command. Version 38 follows the standard transactional `CREATE INDEX IF NOT EXISTS` form, so the command creates the pagination index and records the version in one run.
- **Error namespace** — `UZ-WORKSPACE-001` is the registered duplicate-name conflict. It opens the workspace namespace at 001; the entry uses `eu()` because the dashboard renders its user message.
- **App-dev reconciliation** — After main introduced migrations 36 and 37, a vault-backed read found development rows 35–37, replay columns, the old idempotency index, and the pagination index recorded under the former version 36, while M143's vault metadata columns were absent. Under the repository migration advisory lock, one development-only transaction added the four version-36 vault columns, removed the three replay columns, and deleted the migration-35 ledger row. The two obsolete indexes were removed separately at Indy's request. Verification returned rows 36/37 present, row 35 absent, replay columns absent, four vault columns present, and both obsolete indexes absent. A normal M144 deploy applies version 38 and recreates only the pagination index under its correct version.
- **Production decision** — > Indy (Jul 26, 2026): "no production change needed, since we havent deployed in prod yet" — context: migrations 35–38 have not been deployed to production, so no production repair or mutation is authorized or required.
- **Acceptance evidence** — The response-loss trace proves the branch API committed the named workspace and the subsequent dashboard refresh issued authoritative list reads. The switcher correctly kept the routed workspace selected. Indy completed the dashboard eyeball and removed the localhost dashboard requirement.
- **Documentation decision** — > Indy (Jul 26, 2026: 02:28 PM): "you can skip the changelog.mdx in docs repo" — context: M144 CHORE(close) makes no cross-repository changelog edit.
- **Verification environment** — After the per-worktree infrastructure fix merged from main, `make test-integration` used namespaced containers and ports 26711–26713. The clean schema applied versions 1–34 and 36–38, then the full repository gate passed. Test depth is `unit=3056 integration=405`, up from the CHORE(open) baseline `unit=2958 integration=393`. > Indy (Jul 26, 2026): "let the PR run `make memleak`" — context: memory-leak verification is a Continuous Integration (CI) gate rather than a local pre-push gate.
- **Metrics review** — no event added or funnel changed; successful-create telemetry remains unchanged and failures/reconciliation emit no duplicate success event.
- **Skill-chain outcomes** — `kishore-spec-new`: repository, mockup, historical commit, current main, API/UI/CLI/schema paths, and rollout consequence reviewed; implementation `/write-unit-test`, `/review`, and `kishore-babysit-prs` outcomes populate at CHORE(close).
- **Deferrals** — none.
