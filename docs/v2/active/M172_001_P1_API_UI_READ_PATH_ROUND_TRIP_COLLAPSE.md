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

# M172_001: Read-path round-trip collapse — dashboard and lease queries

**Prototype:** v2.0.0
**Milestone:** M172
**Workstream:** 001
**Date:** Aug 20, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — every dashboard page and every runner poll pays avoidable Postgres round trips; the fleet chat view pays twenty-one avoidable HTTP requests
**Categories:** API, UI
**Batch:** B1 — single workstream, no parallel sibling
**Branch:** feat/m172-read-path-collapse
**Test Baseline:** unit=4157 integration=709
**Depends on:** none
**Provenance:** LLM-drafted (Claude Fable 5, Aug 20, 2026) — grounded in a source walk of every named handler, statement, and dashboard fetcher on main
**Canonical architecture:** `docs/architecture/scaling.md` §Which recurring Postgres reads are index-served; `docs/architecture/web_app.md`

---

## Overview

**Goal (testable):** the fleet detail chat view renders from ONE thread request instead of one list request plus one detail request per turn; every workspace-scoped read authorizes in ONE Postgres statement instead of two-or-three; an idle runner lease poll costs ONE Postgres query instead of two — with zero change to any existing wire shape or authorization verdict.
**Problem:** operators feel the dashboard as sluggish. The chat view of a fleet issues 1 + 1 + 20 sequential-then-fanned HTTP requests (list, then one detail read per turn), and every one of those requests independently re-authorizes with two sequential statements plus a session-context write before its data statement. Runner pickup pays the same tax: an idle poll reads the same `fleet.runners` row twice every second, and a fresh claim spends three pool acquires on three single-row statements.
**Solution summary:** merge the authorization funnel into one statement inside `common_authz.zig` (verdict, tenant resolve, and Row-Level Security (RLS) context write together, context written only on allow); add a `GET` read to the existing fleet messages route that returns the newest chat turns with bodies in one keyset page; collapse the api-keys and tenant-workspaces list handlers to one statement each; fold the runner `degraded` flag into the runner auth lookup, the fleet-session claim into one connection and one joined read, and the per-credential vault loop into one bulk read. The dashboard consumes the new thread read and drops its per-turn fan-out.

## PR Intent & comprehension handshake

- **PR title (eventual):** perf: collapse dashboard and lease read-path round trips
- **Intent (one sentence):** make the dashboard and runner pickup visibly snappier by removing repeated per-request queries and the chat view's per-turn request fan-out, without changing any verdict or wire shape.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/common_authz.zig` — the authorization funnel being merged; every semantic (user-row authority, claim fallback, fail-closed runner mode, audited cross-tenant bypass, context-only-on-allow) must survive verbatim.
2. `src/agentsfleetd/state/fleet_event_detail_store.zig` — the bodies-included row shape and workspace-predicate-inside-SQL discipline the new thread page mirrors.
3. `src/agentsfleetd/http/handlers/fleets/list.zig` + `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` §3 Filtering/sorting/pagination — the keyset pagination shape (`starting_after`/`limit`/`next_cursor`) the new read must use; `?include=` is forbidden, which is why this is a dedicated read on the messages route.
4. `src/agentsfleetd/http/route_table_invoke.zig` (`invokeWorkspaceSecretItem`) — the method-switch pattern for adding `GET` beside the existing `POST` on one route arm.
5. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` — the chat loader whose list-then-per-turn-detail fan-out §5 deletes.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/http/handlers/common_authz.zig` | EDIT | one-statement authorize + context write |
| `src/agentsfleetd/http/handlers/common_authz_test.zig` | EDIT | parity coverage for every verdict arm |
| `src/agentsfleetd/http/handlers/api_keys/sql.zig` | EDIT | page statement carries the total |
| `src/agentsfleetd/http/handlers/api_keys/list.zig` | EDIT | drop the separate count round trip |
| `src/agentsfleetd/http/handlers/tenant_workspaces.zig` | EDIT | tenant resolve folded into the page statement |
| `src/agentsfleetd/http/handlers/fleets/messages.zig` | EDIT | add the thread `GET` beside the steer `POST` |
| `src/agentsfleetd/state/fleet_event_detail_store.zig` | EDIT | keyset page of bodies-included rows |
| `src/agentsfleetd/http/route_table_invoke.zig` | EDIT | method switch on the messages arm |
| `src/agentsfleetd/http/route_scopes.zig` | EDIT | `GET` messages carries the fleet-read capability |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | thread-page body-ceiling refusal code |
| `src/agentsfleetd/cmd/serve_runner_lookup.zig` | EDIT | auth projection carries `degraded` |
| `src/agentsfleetd/auth/middleware/runner_bearer.zig` | EDIT | lookup result + principal carry `degraded` |
| `src/agentsfleetd/auth/principal.zig` | EDIT | runner principal field for `degraded` |
| `src/agentsfleetd/http/handlers/runner/lease.zig` | EDIT | degraded gate reads the principal, not Postgres |
| `src/agentsfleetd/http/handlers/runner/sql.zig` | EDIT | retire the standalone degraded select |
| `src/agentsfleetd/fleet/fleet_session.zig` | EDIT | one acquire; fleets joined with fleet_sessions; conditional execution clear |
| `src/agentsfleetd/fleet/secrets_resolve.zig` | EDIT | one bulk vault read for all credential names |
| `src/agentsfleetd/secrets/crypto_store.zig` | EDIT | name-filtered bulk load beside `loadAllForWorkspace` |
| `src/agentsfleetd/secrets/sql.zig` | EDIT | names-filtered secrets select |
| `public/openapi/` (paths + root as needed) | EDIT | document the messages `GET` |
| `ui/packages/app/lib/api/events.ts` | EDIT | thread fetcher for the messages `GET` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | chat loads via one thread read; view loaders run beside the detail read |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | entry redirect resolves the first workspace with `limit=1` |
| `ui/packages/app/lib/api/workspaces.ts` | EDIT | single-page first-workspace read beside the full walk |
| `ui/packages/app/components/domain/FleetThread.tsx` | EDIT | terminal-event refresh debounced |
| new/adjacent `*_test.zig` and UI test files for the surfaces above | CREATE/EDIT | per-Dimension coverage |
| `docs/v2/*/M172_001_P1_API_UI_READ_PATH_ROUND_TRIP_COLLAPSE.md` | EDIT | lifecycle status |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, NLR, NDC-adjacent DFS (no dead struct fields when the degraded select retires), UFS (new literals → named constants), KYS (composite keyset for the thread page), NSQ (schema-qualified statements in domain `sql.zig` — SQLMOD), FLS/DRAIN (drain results before reuse), ORP (retired symbols swept), TST-NAM, ERR (new registry code declared + referenced), MSID (no milestone ids in source).
- `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` §1 (plural nouns, ids in path), §3 (list envelope, keyset pagination, no `?include=`), §5 (error registry), §6–§7 (OpenAPI + route registration steps), §9 (additive method on an existing path) — the messages `GET` is new public surface.
- `dispatch/write_zig.md` — memory ownership, errdefer ladders, PgQuery drain discipline on every touched statement.
- `dispatch/write_ts_adhere_bun.md` — dashboard fetcher and page edits.
- `docs/AUTH.md` (product repo) — auth-flow files are in scope (`principal.zig`, `runner_bearer.zig`, `common_authz.zig`); read before EXECUTE.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — Zig edits throughout | follow façade; cross-compile both linux targets before commit |
| PUB / Struct-Shape | yes — new store entry point + lookup-result field | shape verdict recorded per new pub surface at EXECUTE |
| File & Function Length (≤350/≤50/≤70) | yes — `messages.zig`, `common_authz.zig`, `fleet_event_detail_store.zig` grow | split helpers before a cap is approached; the thread read may land in a sibling file if `messages.zig` nears 350 |
| UFS (repeated/semantic literals) | yes — limits, ceiling, error code | named constants, single owner each |
| UI Substitution / DESIGN TOKEN | no — no new UI affordances, data-fetch edits only | n/a |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | ERROR REGISTRY yes (one new code); LOGGING yes (scoped events on new paths); SCHEMA no — zero schema edits | registry row + reference; scoped log events with error_code |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/http/handlers/fleets/list.zig` (keyset page + counters join) and `src/agentsfleetd/state/fleet_event_detail_store.zig` (bodies read, tenancy inside SQL) — the thread read composes these two proven shapes; API conventions per `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` with the nearest handler as tie-break. UI mirrors the existing server-component fetchers in `ui/packages/app/lib/api/`.

## Sections (implementation slices)

### §1 — One-statement workspace authorization

Every workspace-scoped handler funnels through `common_authz.zig`, which today issues up to three sequential statements per request (user-row tenant resolve, workspace membership probe, session-context write). One statement can produce the same verdict and, on allow, the same context write. Call sites do not change; the funnel's internals do. The audited cross-tenant bypass path stays exactly as is.
**Implementation default:** resolve the effective tenant inside the statement as user-row-first with the token claim as fallback, because that is today's authority order; a token claim that is not a well-formed Universally Unique Identifier (UUID) is treated as absent before binding (today it can only surface on the fallback arm, where it already denies).

- **Dimension 1.1** — one statement yields allow for a member, deny for a foreign workspace, deny for an unknown workspace, for OpenID Connect (OIDC), api-key, and Command-Line Interface (CLI) credential modes; runner mode never reaches the statement → Test `test_authorize_single_statement_verdict_parity`
- **Dimension 1.2** — context variant sets `app.current_tenant_id` only on allow; a denied request leaves the pooled connection's context untouched → Test `test_tenant_context_written_only_on_allow`
- **Dimension 1.3** — user-row tenant outranks a differing token claim; claim-only principals still resolve when no user row exists; malformed claim degrades to user-row-only, never a statement error → Test `test_tenant_authority_order_preserved`
- **Dimension 1.4** — `workspace:any` bypass still authorizes, still audits, still targets the victim tenant's context → existing bypass coverage re-run as regression → Test `test_cross_tenant_bypass_regression`

### §2 — Fleet chat thread read (`GET …/fleets/{fleet_id}/messages`)

The chat view needs the newest N turns with bodies; today it fans out one detail request per turn because the events list deliberately omits bodies. The messages route already exists for the steer `POST`; this section adds the `GET` that answers "the conversation, bodies included" as one keyset page. Sparse fieldsets are forbidden by the REST guidelines, so this is a dedicated read, not a list parameter.
**Implementation default:** page rows reuse the detail-row field set (list row + `request_json` + `response_text` + `cost_nanos`); default `limit` 20, max 25 — deliberately below the standard list caps because rows carry bodies; the encoded page is refused past a named byte ceiling via the existing encoded-size guard rather than truncated.

- **Dimension 2.1** — `GET` returns newest-first chat-relevant events with bodies, keyset-paged via `starting_after`/`limit` with `next_cursor` continuation → Test `test_thread_page_bodies_and_keyset`
- **Dimension 2.2** — workspace predicate lives inside the statement; a fleet id from another workspace yields an empty page, indistinguishable from no history → Test `test_thread_cross_workspace_empty`
- **Dimension 2.3** — a page whose encoded body exceeds the ceiling is refused with the new registry code, never silently shortened → Test `test_thread_page_ceiling_refusal`
- **Dimension 2.4** — `GET` requires the fleet-read capability; `POST` keeps fleet-write; other methods answer method-not-allowed → Test `test_messages_method_and_scope_split`
- **Dimension 2.5** — OpenAPI documents the `GET` (parameters, envelope, error arms) and route-coverage checks pass → Test `make check-openapi`

### §3 — Single-statement list reads

Two tenant-scoped lists still pay a second round trip for a scalar the page statement can carry.

- **Dimension 3.1** — api-keys list: the page statement carries the page-stable tenant total (uncorrelated scalar subquery); the separate count read retires; wire shape (`items`, `total`, `next_cursor`) and every sort remain byte-identical → Test `test_api_keys_single_statement_total`
- **Dimension 3.2** — tenant workspaces list: the principal-tenant resolve folds into the page statement; the response still carries `tenant_id` when the tenant owns zero workspaces → Test `test_tenant_workspaces_single_statement`

### §4 — Lease-path query folds

Runner pickup latency is the product's "agent starts working" moment. Three folds, all verdict-preserving.

- **Dimension 4.1** — the runner auth lookup projects `degraded` beside `admin_state`; the lease handler reads the principal instead of re-querying; the standalone degraded select retires; fail-closed default (missing flag → degraded) survives → Test `test_lease_degraded_from_principal`
- **Dimension 4.2** — `claimFleet` performs one pool acquire and one statement joining `core.fleets` with `core.fleet_sessions` (checkpoint may be absent → fresh context) → Test `test_claim_fleet_single_acquire_join`
- **Dimension 4.3** — the crash-recovery execution clear updates only rows where an execution id is set, so the steady-state claim writes nothing → Test `test_execution_clear_conditional`
- **Dimension 4.4** — `resolveSecretsMap` loads all requested credential names in one statement (name-filtered bulk read beside `loadAllForWorkspace`); per-row decrypt isolation (one bad envelope degrades that row only) survives → Test `test_secrets_bulk_resolve_isolation`

### §5 — Dashboard consumption

Server-component fetch-graph edits only; zero visual change.

- **Dimension 5.1** — the chat view loads turns via the thread `GET`; the per-turn detail fan-out is deleted; turn grouping and steering behave as before → Test `test_chat_single_thread_fetch`
- **Dimension 5.2** — fleet-detail view loaders start from route params concurrently with the detail read instead of after it → Test `test_detail_view_loaders_concurrent`
- **Dimension 5.3** — the entry redirect resolves the first workspace with a single `limit=1` page instead of walking the full list → Test `test_entry_redirect_single_page`
- **Dimension 5.4** — terminal-event refreshes are debounced so a burst of completions triggers one re-render, with a trailing refresh guaranteed → Test `test_terminal_refresh_debounced`

## Interfaces

```
GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages
  ?starting_after=<cursor>   optional — keyset continuation from next_cursor
  &limit=<1..25>             optional — default 20
→ 200 {
    "items": [ {
      "fleet_id", "event_id", "workspace_id", "actor", "event_type", "status",
      "request_json", "response_text", "tokens", "wall_ms",
      "failure_label", "failure_detail", "checkpoint_id", "resumes_event_id",
      "created_at", "updated_at", "cost_nanos"
    } ],
    "next_cursor": <cursor|null>
  }
→ 400 invalid cursor / limit out of range (RFC 7807, existing codes)
→ 4xx new registry code when the encoded page exceeds the byte ceiling
POST on the same path is unchanged. All other existing wire shapes in this
workstream are frozen: api-keys, tenant-workspaces, fleets, events, secrets
responses stay byte-compatible.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| invalid thread cursor | crafted or stale `starting_after` | 400 invalid-request, same shape as sibling lists |
| thread limit out of range | `limit=0` or `>25` | 400 invalid-request naming the bounds |
| thread page over ceiling | pathological body sizes | refusal with the new registry code; no truncated page |
| cross-workspace fleet id | probing another tenant's fleet | empty page; existence never disclosed |
| malformed tenant claim | token with a non-UUID tenant claim | treated as absent; user-row arm still authorizes; otherwise deny |
| authz statement error | Postgres failure mid-verdict | fail closed (deny), 5xx from the handler exactly as today |
| degraded flag unreadable | lookup row missing the flag | fail closed — runner treated as degraded, lease refused |
| missing session checkpoint | fresh fleet, no `fleet_sessions` row | joined read yields fresh `{}` context; claim proceeds |
| one bad vault envelope in bulk | corrupt or legacy row among requested names | that credential degrades exactly as the per-row path did; others resolve |

## Invariants

1. Authorization verdicts are bit-identical to main for every principal mode and workspace relation — enforced by the §1 parity tests running the same fixtures against both semantics' expected outcomes.
2. A denied request never writes `app.current_tenant_id` — enforced by `test_tenant_context_written_only_on_allow` reading the connection state after a deny.
3. Tenancy predicates live inside every statement this spec touches (never post-filtered in the handler) — enforced by structural statement tests mirroring `fleet_event_detail_store.zig`'s existing pattern.
4. The thread page is doubly bounded (row cap ≤ 25 and encoded-byte ceiling) — enforced at runtime by the limit parser and the encoded-size guard, each with a negative test.
5. No existing response field changes name, type, or presence — enforced by the integration suites for api-keys, tenant-workspaces, events, and lease paths passing unmodified except where they assert round-trip counts.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | existing `http.route` span templates cover the messages route arm already; request-count reduction is observable in existing per-route metrics without new events |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_authorize_single_statement_verdict_parity` | member allow / foreign deny / unknown deny across OIDC, api-key, CLI modes; runner mode denied before SQL |
| 1.2 | integration | `test_tenant_context_written_only_on_allow` | after a deny, `current_setting('app.current_tenant_id', true)` on the same conn is unchanged; after allow it equals the workspace tenant |
| 1.3 | integration | `test_tenant_authority_order_preserved` | user row tenant wins over differing claim; claim-only principal allows its own workspace; malformed claim → no error, user-row arm decides |
| 1.4 | integration | `test_cross_tenant_bypass_regression` | `workspace:any` holder authorizes a foreign workspace and an audit record is emitted |
| 2.1 | integration | `test_thread_page_bodies_and_keyset` | seeded events return newest-first with `request_json`/`response_text`; second page via `next_cursor` continues without overlap or skip |
| 2.2 | integration | `test_thread_cross_workspace_empty` | valid fleet id under another workspace → 200 with empty items |
| 2.3 | integration | `test_thread_page_ceiling_refusal` | oversized seeded bodies → refusal with the new registry code, not a short page |
| 2.4 | integration | `test_messages_method_and_scope_split` | GET with read-only credential 200; POST with it 403; PUT → 405 |
| 2.5 | e2e (gate) | `make check-openapi` | route coverage + lint pass with the documented GET |
| 3.1 | integration | `test_api_keys_single_statement_total` | page two of three keys still reports `total=3`; every sort order byte-matches the pre-change envelope |
| 3.2 | integration | `test_tenant_workspaces_single_statement` | zero-workspace tenant → empty items with correct `tenant_id`; paged walk unchanged |
| 4.1 | integration | `test_lease_degraded_from_principal` | degraded runner refused with today's wire shape; active runner leases; no standalone degraded query remains (grep) |
| 4.2 | integration | `test_claim_fleet_single_acquire_join` | claim of a fleet with and without a checkpoint row yields today's `FleetSession` fields |
| 4.3 | integration | `test_execution_clear_conditional` | claim with NULL execution id performs a zero-row update; stale execution id still cleared |
| 4.4 | integration | `test_secrets_bulk_resolve_isolation` | three names, one corrupt envelope → two resolve, one degrades; one statement observed |
| 5.1 | unit/e2e | `test_chat_single_thread_fetch` | chat loader issues exactly one thread request; rendered turns match the previous composition for the same fixture |
| 5.2 | unit | `test_detail_view_loaders_concurrent` | view loader starts without awaiting the detail read (fetch order observed via instrumented fetchers) |
| 5.3 | unit | `test_entry_redirect_single_page` | redirect fetcher sends `limit=1` and no continuation request |
| 5.4 | unit | `test_terminal_refresh_debounced` | five terminal frames inside the window → one refresh, then a trailing refresh fires |
| regression | integration | existing events/list/detail/lease suites | pass unmodified — frozen wire shapes |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Chat thread served by one request (§2, §5) | `grep -rn "getFleetEvent(" ui/packages/app/app/\(dashboard\)/w/\[workspaceId\]/fleets/\[id\]/page.tsx \| wc -l` | `0` | P0 | |
| R2 | Authorization funnel is one statement (§1) | `bun test ui/packages/app 2>/dev/null; zig build test-integration 2>/dev/null; grep -c "conn.query" src/agentsfleetd/http/handlers/common_authz.zig` | happy path issues one statement — grep count matches the merged design (documented in the file header) | P0 | |
| R3 | Idle lease poll single query (§4) | `grep -rn "SELECT_RUNNER_DEGRADED" src/ \| wc -l` | `0` | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | OpenAPI gate (public surface touched) | `make check-openapi` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted; symbols retire in place | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `SELECT_RUNNER_DEGRADED` | `grep -rn "SELECT_RUNNER_DEGRADED" src/ \| head` | 0 matches |
| `SELECT_TENANT_KEY_COUNT` | `grep -rn "SELECT_TENANT_KEY_COUNT" src/ \| head` | 0 matches |
| `loadSessionCheckpoint` (if folded into the joined read) | `grep -rn "loadSessionCheckpoint" src/ \| head` | 0 matches or a live caller |

## Out of Scope

- `~/Projects/docs` pages + changelog `<Update>` for the messages `GET` — cross-repo write requires explicit per-session approval; listed for Indy at CHORE(close). The PR gate stays red on the docs criterion until that branch lands or an override is recorded.
- Deploy sizing: `DATABASE_POOL_SIZE` (code default 4 per replica) and `fly.toml` env — deploy-config change, Indy's call; surfaced in the review report.
- Authorization result caching (subject→tenant, workspace→tenant): unnecessary once §1 lands one round trip; a cache is a security-posture decision, not a query fix.
- Renewal's duplicate catalogue-revision read and the platform-key decrypt-per-lease cache — money-path and key-handling design calls; follow-up spec if wanted.
- Secrets-list metadata denormalization; api-keys client-side sort; workspace-stream reconnect parity (jitter/visibility) — reported, small or risk-bearing, not latency-critical.
- The `PollCost` round-trip undercount (observability accuracy) — follow-up.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator opens a busy fleet's chat and the thread paints in one beat instead of trickling in after a burst of twenty background requests; a steered fleet picks up work without the runner burning a second query every idle second.
2. **Preserved user behaviour** — every page, list, filter, cursor, sort, SSE stream, and steer works exactly as today; JSON responses on existing calls are byte-compatible.
3. **Optimal-way check** — the unconstrained-optimal shape adds process-level authorization caching and payload-carrying SSE seeds; both are deliberately out of scope because the one-statement funnel and the thread read capture most of the win with zero staleness or security surface.
4. **Rebuild-vs-iterate** — iterate: every handler keeps its file and role; only statements merge and one read is added. No determinism is traded.
5. **What we build** — merged authz statement; messages `GET`; two single-statement lists; three lease-path folds; four dashboard fetch-graph edits.
6. **What we do NOT build** — `?include=` parameters (guideline-forbidden), new tables or schema edits, auth caches, pool-size changes, new UI controls.
7. **Fit with existing features** — compounds with the trigger-maintained `fleet_activity_counters` work (same philosophy: reads stop re-deriving); must not destabilize the steer path sharing the messages route.
8. **Surface order** — API first (the thread read), UI second (consume it); CLI untouched — it already reads events/detail on demand.
9. **Dashboard restraint** — no new controls or claims; the change is invisible except in feel.
10. **Confused-user next step** — a caller hitting the thread-page ceiling gets an RFC 7807 refusal naming the bound and can page with a smaller `limit`; nothing else changes for users.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five sections isolating risk domains — authorization funnel, new read, list merges, lease folds, UI consumption — each independently verifiable and revertible.
- **Alternatives considered:** (a) `?include=payloads` on the events list — forbidden by the REST guidelines' sparse-fieldset rule and rightly so (unbounded fat lists); (b) an in-process authorization cache — bigger win per request but introduces staleness/security review surface; the one-statement merge takes the same latency step without it; (c) splitting lease folds into a second workstream — rejected: one PR per milestone, and the folds are small, test-covered, and thematically identical.
- **Patch-vs-refactor verdict:** this is a **refactor** because the authorization funnel and the chat data path change shape (statement topology and request topology), not just constants — while every observable behaviour is pinned frozen by the parity and regression rows above.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
