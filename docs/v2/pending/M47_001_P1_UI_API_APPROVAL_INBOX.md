# M47_001: Approval Inbox — Dashboard Surface for Pending Gate Actions

**Prototype:** v2.0.0
**Milestone:** M47
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — without this, the dashboard misleads (zombies stalled at approval gates appear "Active"). Not strictly launch-blocking for the platform-ops wedge (which is read-only diagnostics in v1) but adjacent enough that shipping the runtime without it leaves a known UX hole. Approval gates can still fire on platform-ops (e.g., tool-call cost overruns, agent proposing a write action) and any future zombie that performs gated destructive operations.
**Categories:** UI, API
**Batch:** B2 — after M40-M45 substrate, parallel with M46/M48/M49.
**Branch:** feat/m47-approval-inbox (to be created)
**Depends on:** M42_001 (events stream + history — pending gates write to `core.zombie_events` with `status='gate_blocked'`). M4_001 (existing approval gate state machine — DONE in prior milestone, this builds on it).

**Canonical architecture:** `docs/ARCHITECHTURE.md` §10 (capabilities — approval gating row), §13 (path to bastion — per-audience approval).

---

## Implementing agent — read these first

1. `docs/ARCHITECHTURE.md` §10 (capabilities — approval gating row) — what the platform guarantees about gate semantics.
2. M4_001 (done spec) — existing gate state machine. Read its acceptance criteria to understand what gate states exist (`pending`, `approved`, `denied`, `timeout`).
3. M42's spec for `core.zombie_events.status='gate_blocked'` — the mechanism that surfaces a pending gate.
4. `ui/packages/app/` — existing dashboard surface; mirror the events table pattern from M42's `events.tsx` for the `/approvals` page.
5. Slack DM approval flow (existing M4 implementation) — the parallel channel that already works. M47 is the dashboard mirror, NOT a replacement.

---

## Overview

**Goal (testable):** An operator opens the dashboard with a zombie stalled at an approval gate sees:
1. A badge on the zombie's status card showing "N pending approvals"
2. A `/approvals` page listing all pending gate actions across the workspace with full action details (what the zombie wants to do, evidence gathered so far, blast radius assessment)
3. Approve and Deny buttons that resolve the gate immediately from the browser
4. The zombie resuming (or receiving denied + halting) within 2s of the click

Same flow accessible programmatically: `GET /v1/.../approvals` → list, `POST /v1/.../approvals/{gate_id}:approve` and `:deny` → resolve. Multi-channel deduplication: clicking Approve in the dashboard makes Slack's buttons no-ops on next click (with a clear "already approved" message).

**Problem:** Approval gate interactions today flow only through Slack DMs. If a zombie hits a gate while the operator is looking at the dashboard, the zombie shows "Active" — no indication it's stalled. For platform-ops, gates can fire on tool-call cost overruns or when the agent proposes a write action; the operator needs a dashboard view of pending gates so they're not invisible behind a "healthy zombie" status pill. The mechanism generalizes to any future zombie that performs gated destructive work.

**Solution summary:** New API endpoints over the existing M4 gate state. Dashboard surfaces: (a) badge on zombie cards, (b) `/approvals` page with table + inline detail/approve flow, (c) "Pending" tab on zombie detail page. Multi-channel dedup: gate state machine accepts the FIRST resolution (dashboard or Slack); subsequent resolutions return `already_resolved` with the original outcome.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/approvals/list.zig` | NEW | `GET /v1/workspaces/{ws}/approvals?status=pending` |
| `src/http/handlers/approvals/resolve.zig` | NEW | `POST /v1/.../approvals/{gate_id}:approve` and `:deny` |
| `src/state/approval_gate.zig` | EXTEND | Read-side queries for the inbox; multi-channel dedup |
| `ui/packages/app/src/routes/approvals/index.tsx` | NEW | List page |
| `ui/packages/app/src/routes/approvals/[gate_id].tsx` | NEW | Detail page with Approve/Deny |
| `ui/packages/app/src/routes/zombies/[id]/approvals.tsx` | NEW | Per-zombie approvals tab |
| `ui/packages/app/src/components/ZombieCard.tsx` | EDIT | Add pending-approvals badge |
| `ui/packages/app/src/lib/approvals.ts` | NEW | Client helpers + SWR hooks |
| `tests/integration/approval_inbox_test.zig` | NEW | E2E: gate fires → appears in inbox → approve via API → zombie resumes |
| `samples/fixtures/m47-gate-fixture/SKILL.md` | NEW | Synthetic test fixture that fires a gate via the platform-ops zombie shape; used by the integration test only, NOT a public sample |

---

## Sections (implementation slices)

### §1 — Read endpoint: `GET /approvals`

```
GET /v1/workspaces/{ws}/approvals?status=pending&limit=50&cursor=
  → 200 {
    items: [
      {
        gate_id: string,
        zombie_id: uuid,
        zombie_name: string,
        gate_kind: string,           # "destructive_action" | "cost_overrun" | "external_call"
        proposed_action: string,     # human-readable summary
        evidence: object,            # what the zombie has gathered so far
        blast_radius: string,        # zombie's own assessment
        created_at: timestamptz,
        timeout_at: timestamptz,     # auto-deny if not resolved by then
      }
    ],
    next_cursor: string,
  }
```

Filterable by `zombie_id`, `status`, `gate_kind`. Default `status=pending`.

**Implementation default**: cursor encodes `(created_at, gate_id)`. Pending gates ordered oldest-first (oldest is most urgent).

### §2 — Resolve endpoints: approve / deny

```
POST /v1/workspaces/{ws}/approvals/{gate_id}:approve
  body: { reason?: string }
  → 200 { gate_id, resolved_at, resolved_by: "user:<email>", outcome: "approved" }
  → 409 { error: "already_resolved", outcome: "approved"|"denied"|"timeout", resolved_at }

POST /v1/workspaces/{ws}/approvals/{gate_id}:deny
  body: { reason?: string }
  → 200 { gate_id, resolved_at, resolved_by, outcome: "denied" }
  → 409 (same shape)
```

State machine transition is atomic (DB row update with `WHERE status='pending'` precondition). Multi-channel dedup falls out of the precondition.

**Implementation default**: on resolution, the worker thread (which is blocked on the gate) sees the state change within 2s via the gate's wait loop polling `core.approval_gates`. Faster: signal via Redis pubsub `gate:{gate_id}:resolved`.

### §3 — Multi-channel dedup with Slack

When a gate fires (M4 existing behavior), Slack DM is sent with Approve/Deny buttons. M47 adds the dashboard mirror. Either channel resolves the gate. The OTHER channel's button click returns `409 already_resolved` with the original outcome. Slack message is patched to show the resolution status (existing M4 hook).

### §4 — Dashboard list page

`ui/packages/app/src/routes/approvals/index.tsx`:

- Table: zombie name, gate kind, proposed action, evidence summary, blast radius, age, [Approve] [Deny] buttons
- Filter chips: zombie, gate kind
- Empty state: "No pending approvals 🎉"
- Real-time: SWR poll every 5s OR subscribe to a workspace-wide SSE stream (use existing M42 SSE infra extended to `core.approval_gates` if affordable; else poll)

**Implementation default**: poll every 5s in v1. SSE for approvals is a follow-up if needed.

### §5 — Per-zombie approvals tab

`ui/packages/app/src/routes/zombies/[id]/approvals.tsx`: same shape as global inbox but filtered to `zombie_id`. Surfaces in the zombie detail page nav as "Pending (N)".

### §6 — ZombieCard badge

`ui/packages/app/src/components/ZombieCard.tsx`: if `pending_approvals_count > 0`, show red badge "N pending". Click → routes to per-zombie approvals tab.

**Implementation default**: pending count comes from a workspace-wide query at dashboard load + revalidates on visibility change. Not a per-card live count (that'd thrash).

### §7 — Detail page with Approve/Deny

`ui/packages/app/src/routes/approvals/[gate_id].tsx`:

- Full proposed_action prose
- Evidence rendered as expandable JSON (or formatted by gate_kind)
- Blast radius callout
- Approve button (green, primary) + Deny button (red, secondary)
- Optional reason text field
- On click → POST → on success, redirect to /approvals with toast
- On 409 → show "Already resolved by <X> at <when>" and disable buttons

### §8 — Test fixture for gate firing

`samples/fixtures/m47-gate-fixture/SKILL.md`: a synthetic SKILL.md whose only purpose is to exercise the gate path under integration test. Lives under `samples/fixtures/`, not `samples/` — it is NOT a public sample. The shape is a minimal platform-ops-shaped zombie with a `gate_request` tool wired in, so the integration test can install it, fire a gate, and verify the inbox flow end-to-end. The fixture moves to `samples/` only if and when a real public zombie shape needs to ship that has gated destructive ops.

---

## Interfaces

```
HTTP:
  GET    /v1/workspaces/{ws}/approvals?status=&zombie_id=&gate_kind=&cursor=&limit=
  POST   /v1/workspaces/{ws}/approvals/{gate_id}:approve  body: { reason? }
  POST   /v1/workspaces/{ws}/approvals/{gate_id}:deny     body: { reason? }

Internal:
  approval_gate.list(workspace_id, filters) → []Gate
  approval_gate.resolve(gate_id, outcome, by) → Resolution | AlreadyResolved
  approval_gate.signalResolved(gate_id, outcome)  # Redis pubsub for worker wake

UI routes:
  /approvals             # workspace-wide list
  /approvals/{gate_id}   # detail + resolve
  /zombies/{id}/approvals # per-zombie tab
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Approve while already-approved | Race between Slack click + dashboard click | 409 `already_resolved`, original outcome surfaced |
| Gate auto-times-out before user resolves | `timeout_at` reached (default 24h) | Gate transitions to `outcome=timeout`; zombie's wait loop sees this and proceeds with `denied` semantics (safe default for destructive ops) |
| Worker doesn't wake within 2s of resolution | Pubsub miss + polling lag | Worker's wait loop polls every 1s in addition to subscribing — bound at ~1s worst case |
| Network blip during POST | Browser → server connection drops | Idempotent: client retries with same body; server uses gate state precondition |

---

## Invariants

1. **At most one resolution per gate.** State machine precondition `WHERE status='pending'`.
2. **Resolution channel attribution preserved.** `resolved_by` records dashboard user OR Slack user OR API client distinctly.
3. **Auto-timeout = denied semantics for destructive ops.** Safe default; SKILL.md prose can override per zombie if needed.
4. **No write actions execute before resolution.** Gate must be `approved` for the worker's tool call to fire.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_pending_gate_appears_in_list` | Fire a gate → GET /approvals → item present with correct fields |
| `test_approve_unblocks_zombie` | Fire → approve via API → assert worker wakes within 2s + tool call fires |
| `test_deny_halts_zombie` | Fire → deny → worker proceeds with denied path; tool call NOT fired |
| `test_dual_channel_dedup` | Fire → approve via Slack → approve via dashboard → 409 with original `outcome=approved` |
| `test_timeout_to_denied_for_destructive` | Fire destructive_action gate → wait `timeout_at` → assert `outcome=timeout`, worker treats as denied |
| `test_zombie_card_badge_count` | Fire 2 gates on one zombie → dashboard renders "2 pending" badge |
| `test_per_zombie_tab` | 5 gates across 3 zombies → /zombies/{id}/approvals shows only that zombie's |
| `test_idempotent_approve_retry` | POST :approve twice with same body → second returns same outcome (no double-resolve) |

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 8 tests above
- [ ] `bun test` passes UI unit tests for the components
- [ ] Manual smoke: install the m47-gate-fixture → trigger gate → see badge → click Approve in dashboard → zombie resumes
- [ ] Slack + dashboard parity: a gate fires both channels; resolving in either resolves the other within 2s
- [ ] No regressions on M4 Slack-only flow
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
