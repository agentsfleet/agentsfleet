# M4_001: Inbound GitHub Webhook Handler

**Version:** v2
**Milestone:** M4
**Workstream:** 001
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P0 — foundation layer; all other M4 workstreams depend on delivery state
**Batch:** B1 — first to ship; unblocks M4_002, M4_003, M4_004
**Depends on:** M9_001 (scoring engine), M7_001 (deploy acceptance)

---

## Problem

The run state machine terminates at `DONE` (PR opened). The control plane has no visibility into whether the PR was merged, closed, or revised. This breaks the GTM promise ("Agent Delivery Control Plane") and blocks the agent-to-agent use case, delivery scoring, and auto-merge for trusted agents.

---

## Design Decisions (from CEO Review, Mar 23, 2026)

- **Parallel `delivery_state` column** on `runs` table — existing `run_state` and `isTerminal()` untouched (Issue 4, Option B)
- **6-state reduced delivery machine** — `NULL → PR_OPEN → CHANGES_REQUESTED | APPROVED | CI_FAILED → MERGED | CLOSED` (Issue 5, Option B)
- **Permissive transitions** — terminal states (`MERGED`, `CLOSED`) override from any non-terminal; non-terminal states overwrite each other freely (Issue 12, Option B)
- **Soft-terminal `CLOSED`** — allows `CLOSED → PR_OPEN` on GitHub reopen event; scoring and billing treat first `CLOSED` as final (Issue 13, Option C)
- **Per-app shared secret** — `GITHUB_WEBHOOK_SECRET` env var, HMAC-SHA256 verification (Issue 2, Option A)
- **Branch prefix filter** — discard webhooks for branches not prefixed `zombie/` before DB lookup (Issue 8, Option A)
- **Idempotent transitions** — no replay dedup table; delivery state transitions and outbox uniqueness provide functional idempotency (Issue 10, Option B)

---

## 1.0 Webhook Endpoint and Signature Verification

**Status:** PENDING

Register `POST /v1/github/webhook` in the API handler router. Verify every inbound payload using HMAC-SHA256 with `GITHUB_WEBHOOK_SECRET`.

**Dimensions:**
- 1.1 Register route `POST /v1/github/webhook` in `src/http/handlers/`. Handler reads raw body bytes and `X-Hub-Signature-256` header. Computes `HMAC-SHA256(secret, body)` using `std.crypto.auth.hmac.sha2.HmacSha256`. Rejects with 401 if signature does not match. Returns 200 for all accepted payloads (GitHub convention).
- 1.2 Parse `X-GitHub-Event` header to determine event type. Supported events: `pull_request`, `pull_request_review`, `check_suite`. Unknown event types are logged at DEBUG and acknowledged with 200 (no processing).
- 1.3 Parse JSON payload. Extract `pull_request.head.ref` (branch name), `pull_request.html_url` (PR URL), `action` field, and event-specific fields. Reject malformed payloads with 400.
- 1.4 Add `GITHUB_WEBHOOK_SECRET` to env contract in `docs/CONFIGURATION.md` under Auth partition. Required: Conditional (required when webhook ingestion is enabled). Override source: Process env only, no CLI.

---

## 2.0 Branch Filter and Run Lookup

**Status:** PENDING

Filter out non-UseZombie PRs before any database access. Resolve the originating run from the branch name.

**Dimensions:**
- 2.1 Check `pull_request.head.ref` starts with `zombie/`. If not, log at DEBUG (`webhook.filtered_non_zombie branch={ref}`) and return 200. No DB query.
- 2.2 Extract `run_id` from branch name pattern `zombie/run-{run_id}`. If pattern does not match, log at DEBUG (`webhook.orphaned_run branch={ref}`) and return 200.
- 2.3 Query `runs` table by `run_id`. If not found, log at DEBUG and return 200. If found, proceed to delivery state transition.
- 2.4 Verify that the webhook's `installation.id` matches the workspace's stored `github_app_installation_id`. Cross-workspace event injection is rejected with 200 (logged, not processed). This prevents crafted branch names from triggering state changes on other workspaces' runs.

---

## 3.0 Delivery State Column and Transitions

**Status:** PENDING

Add `delivery_state` column to `runs` table. Implement permissive transition logic separate from `state/machine.zig`.

**Dimensions:**
- 3.1 Schema migration: `ALTER TABLE runs ADD COLUMN delivery_state TEXT DEFAULT NULL`. Nullable — existing runs remain NULL. No table rewrite, no lock.
- 3.2 Create `src/state/delivery.zig` with delivery state enum: `PR_OPEN`, `CHANGES_REQUESTED`, `APPROVED`, `CI_FAILED`, `MERGED`, `CLOSED`. Terminal check: `MERGED` is always terminal. `CLOSED` is soft-terminal (allows transition to `PR_OPEN` on reopen event only).
- 3.3 Transition function `deliveryTransition(conn, run_id, to_state) → bool`. Permissive rules:
  - Any non-terminal → `MERGED`: allowed (terminal override)
  - Any non-terminal → `CLOSED`: allowed (terminal override)
  - `CLOSED → PR_OPEN`: allowed (reopen)
  - Any non-terminal → any non-terminal: allowed (last signal wins)
  - `MERGED → anything`: rejected (true terminal)
  - All transitions logged at INFO with `from` and `to` states.
- 3.4 Set `delivery_state = 'PR_OPEN'` in `handleDoneOutcome()` (`worker_stage_outcomes.zig`) when the PR is successfully created. This is the initial delivery state for every run that produces a PR.

---

## 4.0 Doctor Validation

**Status:** PENDING

Extend `zombied doctor` to validate webhook configuration.

**Dimensions:**
- 4.1 Add `GITHUB_WEBHOOK_SECRET` presence and non-empty check to doctor validation chain. Report as `webhook_secret: OK` or `webhook_secret: MISSING`.
- 4.2 Doctor output includes webhook secret check in both human and JSON modes (`--format=json`).
- 4.3 When `GITHUB_WEBHOOK_SECRET` is not set, doctor reports warning (not failure) — webhook ingestion is optional at startup but required for delivery state tracking.

---

## Webhook-to-State Mapping

| GitHub Event | Action | delivery_state |
|---|---|---|
| `pull_request` | `closed` + `merged=true` | `MERGED` |
| `pull_request` | `closed` + `merged=false` | `CLOSED` |
| `pull_request` | `reopened` | `PR_OPEN` |
| `pull_request_review` | `submitted` + `state=approved` | `APPROVED` |
| `pull_request_review` | `submitted` + `state=changes_requested` | `CHANGES_REQUESTED` |
| `check_suite` | `completed` + `conclusion=failure` | `CI_FAILED` |
| `check_suite` | `completed` + `conclusion=success` | `APPROVED` (if already approved) |

---

## Rollout Sequence

1. Run schema migration (delivery_state column)
2. Deploy new zombied with webhook handler
3. Set `GITHUB_WEBHOOK_SECRET` in Fly secrets
4. Configure webhook URL in GitHub App settings → `https://api.usezombie.com/v1/github/webhook`
5. Subscribe to events: `pull_request`, `pull_request_review`, `check_suite`
6. Verify via logs: `webhook.received` events appearing

## Rollback

Remove webhook URL from GitHub App settings. No events flow. `delivery_state` column stays NULL for new runs — harmless. Previous zombied binary ignores the column.
