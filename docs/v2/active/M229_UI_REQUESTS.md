# UI requests

**Branch:** `feat/m229-postgres-schema-clean-slate`
**Folded-into:** `M229_001`

This request log belongs to the existing schema clean-slate milestone.

### Private setup-chat search and titles — open

- Endpoint: `GET /v1/workspaces/{workspace_id}/drafts`.
- Field: creator-scoped `q` query matching chat title and message text, with the query included in cursor binding.
- Why: the current endpoint supports only state and linked-fleet filters. The sidebar can search loaded titles, but cannot truthfully offer search across all conversations.
- Screen: Fleet setup chats in the sidebar.
- Endpoint: `POST /v1/workspaces/{workspace_id}/drafts` and its list/read responses.
- Field: initialize `title` from the first user request or the selected fleet/Library description instead of the constant `New fleet`. Preserve explicit title edits.
- Why: old and new conversations need meaningful persisted titles without fetching each transcript or trying a title mutation while authoring is running.
- Screen: Fleet setup chats and the private chat heading.

### Setup-chat revision and publication summaries — open

- Endpoint: `GET /v1/workspaces/{workspace_id}/drafts`.
- Fields: current candidate revision, saved fleet setup revision, and publication revision when present.
- Why: state and linked fleet identity already support In progress / Saving / Saved labels. Revision and publication indicators for other chats require summary fields; only the open draft currently supplies these facts.
- Screen: Fleet setup chats in the sidebar.

### Historical runner leases and valid fleet destinations — open

- Endpoint: `GET /v1/fleet-runners/{runner_id}/leases`.
- Fields: explicit fleet existence/navigation availability and the fleet kind when the lease belongs to a private setup conversation rather than a user fleet.
- Why: the current projection inner-joins `fleet.fleets` and `fleet.events`, so deleting a probe fleet can remove its lease from the list. A non-null name does not establish that the signed-in operator can open the tenant fleet screen. The reported setup/probe lease opens a missing fleet page.
- Requested behavior: retain lease history within the server's retention window when the related fleet/event disappears, and expose an authorized destination or an explicit unavailable state. Do not infer this from a fleet-name prefix.
- Screens: Runner → Leases and Review lease → Open fleet.

### Saved schedule summary — open

- Endpoint: `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}` and `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules`.
- Fields: a human-readable schedule description and nullable `next_run_at`, calculated by the server from the registered schedule and its timezone.
- Why: Setup should answer whether a schedule is set and when it will run. The browser must not calculate a misleading time for paused, unregistered, or stopped schedules.
- Behavior: distinguish no schedule, paused, activation pending, and next run unavailable. Keep execution history in Fleet Chat and Activity.
- Screen: the small Schedule summary within Fleet Setup.
- Dependency cleanup: the UI removed `cron-parser` from its package manifest. Claude owns the root `bun.lock`; refresh it with the next dependency update.

The UI continues with explicit loaded-title search, available setup state, and safe destination checking while these requests are open.


## Overview

This is the UI-to-server request log for the existing milestone, folded into its sole active spec. It records missing API support without changing backend files from the UI stream.

## PR Intent

Record endpoint, field, reason, and consuming screen for each unresolved UI need. There is no separate pull request or deployment for this log.

## Implementing agent

1. `public/openapi.json` defines the published API shapes.
2. `ui/packages/app/lib/api/draft-types.ts` parses private setup responses.
3. `rustd/crates/afd_fleet_ops/src/sql/select_runner_lease_page.sql` defines lease visibility.

## Files Changed

Only this request log is changed by its documentation commit. Backend implementations remain part of the containing milestone.

## Applicable Rules

Use source-backed requests, preserve creator privacy, and do not invent fields before the server publishes them.

## Applicable Gates

Commit hooks remain enabled. The user delegates application lint, unit tests, and coverage to the backend stream.

## Prior-Art

Existing draft title mutation, state filters, and nullable fleet names are described by `public/openapi.json`. They do not provide complete chat search or historical destination availability.

## Sections

- Dimension 1.1 — Each open request records its endpoint, field or behavior, reason, and screen. → Test `request completeness inspection`

## Interfaces

The requests above identify the affected HTTP interfaces. Updated shapes must be committed with regenerated OpenAPI before the UI consumes them.

## Failure Modes

A missing search endpoint must not be represented as a complete search. A missing fleet must not be presented as a working navigation link. Failed reads remain separate from empty history.

## Invariants

Private conversations remain creator-scoped. No heuristic fleet-name prefix decides authorization or fleet kind.

## Metrics and Observability

The log tracks explicit open requests. It introduces no telemetry or runtime counters.

## Test Specification

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | Manual source inspection | request completeness inspection | Every request includes endpoint, field or behavior, reason, and screen, checked against the cited source. |

## Acceptance Rubric

The containing milestone retains these verification commands. This documentation commit does not claim they were run; the UI user has delegated their execution.

| Verify | Expected |
|---|---|
| `make harness-verify` | Commit hook accepts the request log and staged scope. |
| `make lint-all` | Containing milestone lint passes when its verification runs. |
| `make test-unit-all` | Containing milestone unit checks pass when its verification runs. |
| `make test-integration-rustd` | Backend request implementations pass real service checks. |
| `make check-version` | Containing milestone version checks pass. |

## Dead Code Sweep

No executable code or public symbols are introduced by this request log.

## Out of Scope

Backend edits, unrelated documentation, pushes, pull requests, merges, and branch changes are outside the UI stream.

## Product Clarity

Search must say what it searches. Setup status must reflect recorded state. Lease history must explain unavailable destinations rather than manufacture a link.

## Decomposition

Keep this log inside the existing milestone. Each request can be answered independently while the other UI fixes continue.

## Discovery

The commit harness treats every Markdown file in the active directory as a spec. This log declares its fold into the existing milestone and includes the required structure without creating a second stream.
