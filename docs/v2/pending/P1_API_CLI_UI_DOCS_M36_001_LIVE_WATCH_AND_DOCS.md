# M36_001: Live Watch Stream + Docs Polish + Launch Post Replacement

**Prototype:** v2.0.0
**Milestone:** M36
**Workstream:** 001
**Date:** Apr 23, 2026
**Status:** PENDING
**Priority:** P1 — the UI acceptance track of M37_001 §3.2 needs live activity streaming. The chat CLI's polling-based response read-back (M34) is an MVP; the real UX is SSE-driven. Also: the external docs repo (`~/Projects/docs/`) still talks about homelab-zombie and kubectl-first direction; it needs to match what ships.
**Batch:** B2 — after M34/M35/M36 land (consumes their data + endpoints). Last workstream in the M31 rework before dogfood acceptance (M33 §3.1+3.2).
**Branch:** feat/m36-live-watch-docs (to be created)
**Depends on:** M33_001 (chat path + control-stream terminology), M34_001 (event history read surface), M35_001 (executor policy — not directly, but live watch shows per-event state driven by M35 rows).

**Canonical architecture:** `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` — referenced by the new public docs pages as the source of truth for how the product works under the hood.

---

## Overview

**Goal (testable):** An operator opens the zombie detail page in the dashboard, switches to "Live Activity", and sees tool calls + agent responses streaming in as they happen, with <200ms latency from event emission to render. Running `zombiectl zombie watch {id}` prints the same stream to stdout in color. The chat widget upgrades transparently from polling to SSE — no code duplication. The external docs site (`~/Projects/docs/`) has a platform-ops page replacing the old homelab-zombie launch post, a homebox-audit page, and the outdated/sloppy pages touched by these flows are polished.

**Problem:** M33_001 ships chat-via-polling and M34_001 ships `/events` endpoint. Neither gives a live streaming view suitable for the "watch the zombie think" operator experience that makes the product feel alive. And the external docs still pitch a kubectl-first homelab narrative that no longer ships.

**Solution summary:** One SSE endpoint in zombied-api tailing `core.zombie_activities` (a table that already exists and already receives `tool_call_requested` / `tool_call_completed` / `zombie_event_received` / etc. writes from M34 + M35 write paths). CLI `zombiectl zombie watch` and UI live activity panel both consume the same SSE shape. Chat widget's polling is replaced with SSE in a single-line client-side swap. Docs work: audit the `~/Projects/docs/` tree, replace homelab launch post with platform-ops post, add homebox-audit page, polish inventoried "sloppy" pages.

---

## Files Changed (blast radius)

### HTTP (Zig)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/zombies/activity_stream.zig` | CREATE | `GET /v1/workspaces/{ws}/zombies/{id}/activity:stream` — SSE endpoint. Authenticates, subscribes to a pg LISTEN channel OR tails `core.zombie_activities` via periodic query (pick one; LISTEN preferred for low latency). Emits SSE frames: `event: tool_call_requested`, `event: tool_call_completed`, `event: zombie_event_received`, `event: zombie_event_completed`, `event: heartbeat` (every 15s). ≤300 lines. |
| `src/db/pg_listen.zig` | CREATE or EXTEND | Dedicated pg connection that LISTENs on `zombie_activity_insert` channel. Trigger on `core.zombie_activities` INSERT fires NOTIFY with `{zombie_id, event_id, activity_id}`. SSE handler filters by zombie_id. ≤150 lines. |
| `schema/NNN_zombie_activity_notify.sql` | CREATE (pre-v2.0 teardown) | Trigger + NOTIFY function. Schema Guard output required. |
| `src/http/router.zig` + `route_table.zig` + `route_manifest.zig` | MODIFY | Register SSE route; mark it as streaming (different timeout / buffering behavior). |

### CLI (JavaScript)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie_watch.js` | CREATE | Opens SSE to `/activity:stream`, prints events with color-coded prefixes (`[tool] kubectl get pods ...`, `[done] 2.3s 450 tokens`, etc.). Ctrl-C disconnects. ≤200 lines. |
| `zombiectl/src/commands/zombie_chat.js` | MODIFY | Replace the 1s `/events` polling with SSE subscription to `/activity:stream`; renders `[claw]` prefix on `zombie_event_completed` frames. Keep the one-time history fetch on session open (still GET `/events?limit=20`). |

### UI (TypeScript/React)

| File | Action | Why |
|---|---|---|
| `ui/packages/app/lib/api/zombies-activity.ts` | CREATE | `openActivityStream(wsId, zId): EventSource` + `parseActivityEvent`. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/LiveActivityPanel.tsx` | CREATE | Shows running feed of tool calls + events. Auto-scrolls. Color-coded by event type. Pause/resume button. Shows "(offline — reconnecting)" on SSE drop. ≤300 lines. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/ChatWidget.tsx` (M33_001) | MODIFY | Swap polling for SSE; one-line change. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/page.tsx` | MODIFY | Add "Live Activity" tab. |

### Docs — external repo `~/Projects/docs/`

| Path | Action | Why |
|---|---|---|
| `~/Projects/docs/launches/platform-ops.mdx` | CREATE | New launch post narrative: "AI on-call for fly.io + upstash." Replaces the old homelab post's role on the homepage / launches index. Tone: direct, concrete, shows real SKILL.md excerpts, concrete Slack output. |
| `~/Projects/docs/samples/platform-ops.mdx` | CREATE | Operator-focused page: install, chat, understand the output. Links to repo's `samples/platform-ops/README.md` for canonical source. |
| `~/Projects/docs/samples/homebox-audit.mdx` | CREATE | Same shape for the drift-detector sample (M38_001 lands its code; this page is the docs face). |
| `~/Projects/docs/architecture/zombie-event-flow.mdx` | CREATE | Public-facing version of `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` — same content, docs-site rendering, one public-friendly diagram. |
| `~/Projects/docs/launches/homelab-zombie.mdx` | DELETE (or redirect) | Superseded. Redirect to `/launches/platform-ops` if the docs platform supports redirects; else delete. |
| `~/Projects/docs/` polish pass | EDIT | Inventory TBD — an audit will surface sloppy pages touched by platform-ops flows (quickstart, pricing, intro). Concrete list gets captured in Discovery during EXECUTE. User directive: "must be polished." |
| `~/Projects/docs/changelog.mdx` | EDIT | Add `<Update>` block for v0.29.0 (or wherever this milestone version lands) summarizing the M31 rework. Follows the AGENTS.md `<Update>` schema. |

### Tests

| File | Action | Why |
|---|---|---|
| `src/http/handlers/zombies/activity_stream_test.zig` | CREATE | Integration: two concurrent subscribers to different zombies get only their own events; reconnect works; auth-fail closes. ≤300 lines. |
| `src/db/pg_listen_test.zig` | CREATE | Unit: LISTEN + notify + filter. ≤150 lines. |
| `zombiectl/test/zombie-watch.unit.test.js` | CREATE | SSE client under fake timers + fake EventSource. |
| `ui/packages/app/tests/live-activity-panel.test.tsx` | CREATE | MSW EventSource mock + render assertions. |

---

## Applicable Rules

**ZIG-DRAIN**, **TST-NAM**, **FLL** on .zig/.js/.tsx files per usual. **Schema Guard** on the NOTIFY trigger migration.

---

## Sections

### §1 — SSE endpoint + pg NOTIFY plumbing

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | schema trigger | INSERT into core.zombie_activities | NOTIFY `zombie_activity_insert '{zombie_id, activity_id}'` fires | integration |
| 1.2 | PENDING | pg_listen client | dedicated conn subscribed | receives notifications with <50ms latency | unit + integration |
| 1.3 | PENDING | SSE endpoint | GET with auth | 200 + text/event-stream; heartbeat every 15s | integration |
| 1.4 | PENDING | subscriber filtering | multiple concurrent subscribers on different zombies | each sees only their own | integration |
| 1.5 | PENDING | reconnect | drop connection; re-open | works; no duplicate replay | integration |
| 1.6 | PENDING | auth-fail | no session | 401; connection closed | integration |

### §2 — CLI watch

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `zombiectl zombie watch {id}` | opens | connects; prints header + awaits events | unit |
| 2.2 | PENDING | tool_call_requested event | SSE frame received | `[tool] <name> <args-summary>` printed with cyan prefix | unit |
| 2.3 | PENDING | zombie_event_completed | SSE frame | `[done] Xms Y tokens: <truncated response>` printed | unit |
| 2.4 | PENDING | Ctrl-C | signal | clean disconnect | unit |

### §3 — CLI chat SSE upgrade

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | chat client swaps polling for SSE | — | after user types + POST /steer, response arrives via SSE within <200ms of completion | integration |
| 3.2 | PENDING | history still fetched on open (not via SSE) | — | one GET /events?limit=20 then SSE takes over | unit |

### §4 — UI live activity

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | LiveActivityPanel.tsx | render on zombie detail | opens SSE; shows "Connected" | unit |
| 4.2 | PENDING | event rendering | receive mock events | list renders; auto-scrolls | unit |
| 4.3 | PENDING | disconnect UI | kill mock source | shows reconnecting indicator | unit |
| 4.4 | PENDING | chat widget SSE swap | send + receive | response renders from SSE, not polling | unit |

### §5 — External docs polish

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `platform-ops.mdx` launch post | read top-to-bottom | narrative present; no homelab-zombie language; includes real command examples + screenshot of Slack output | manual |
| 5.2 | PENDING | `samples/platform-ops.mdx` | renders | install steps match repo README; deep-links to canonical README | manual |
| 5.3 | PENDING | `samples/homebox-audit.mdx` | renders | covers drift-detector framing (M38_001) | manual (post M38) |
| 5.4 | PENDING | `architecture/zombie-event-flow.mdx` | renders | faithful to `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` | manual |
| 5.5 | PENDING | homelab-zombie launch post | — | deleted or redirected to platform-ops | manual |
| 5.6 | PENDING | sloppy pages inventory | audit pass | list captured in Discovery; each rewritten or deleted | manual |
| 5.7 | PENDING | changelog `<Update>` block | v0.29 (or next release) | follows AGENTS.md schema; user-visible language; tagged `["What's new","API","CLI","UI"]` | manual |

### §6 — End-to-end acceptance (§3.2 of M37_001)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 6.1 | PENDING | UI dogfood under kishore@usezombie signup | full 11-step walkthrough from arch doc §7 | all steps green via dashboard; SSE stream visible; no regressions | manual (dogfood) |

---

## Interfaces

**Produced:**

- `GET /v1/workspaces/{ws}/zombies/{id}/activity:stream` — text/event-stream.
- SSE frame taxonomy:
  - `event: heartbeat\ndata: {"ts":...}` (every 15s)
  - `event: zombie_event_received\ndata: {event_id, actor, source, ts}`
  - `event: tool_call_requested\ndata: {event_id, tool, args_summary, ts}`
  - `event: tool_call_completed\ndata: {event_id, tool, duration_ms, result, ts}`
  - `event: zombie_event_completed\ndata: {event_id, status, tokens, wall_ms, response_preview}`
  - `event: zombie_status_changed\ndata: {status, ts}`
  - `event: error\ndata: {code, message}` (terminal — client should reconnect)
- pg NOTIFY channel `zombie_activity_insert`.

**Consumed:**

- `core.zombie_activities` (existing table, existing writers).
- `core.zombie_events` status changes (via M34_001 write path; pg NOTIFY on UPDATE too).
- Auth primitives (existing).

---

## Failure Modes

| Failure | Trigger | Behavior | Observed |
|---|---|---|---|
| pg LISTEN conn drops | network | dedicated conn reconnects; brief event gap | client sees "reconnecting" indicator; no permanent loss (rows are in DB) |
| SSE behind load balancer buffering | Fly.io / nginx config | document required config (flush-headers, no buffering); provide smoke test | docs page on SSE deployment |
| Subscriber never reads | broken client | server-side idle timeout 5 min → 204 close | clean resource release |
| Large event stream floods | zombie emits 1000 tool_calls | server applies rate-limit (50/s per subscriber); excess frames coalesced into `event: burst\ndata: {count, window}` | client handles gracefully |
| Trigger misses a write | pg trigger bug | rare; fallback: chat CLI's initial GET /events still shows truth | no permanent divergence |

---

## Invariants

| # | Invariant | Enforcement |
|---|---|---|
| 1 | SSE frames are workspace-scoped + auth-checked | §1.6 negative test |
| 2 | One NOTIFY per activity INSERT; trigger idempotent | §1.1 unit |
| 3 | Client reconnect is safe (no duplicate state; idempotent consumers) | §1.5 integration |
| 4 | Heartbeat every 15s prevents intermediary timeouts | §1.3 integration |
| 5 | External docs do NOT reference homelab-zombie as a product name (only as superseded history) | §5 grep at VERIFY |

---

## Test Specification

Per sections. Integration tests require a live pg + running zombied-api; SSE tests use httpz's event-stream primitive. UI tests mock EventSource. External docs tests are manual smoke (the docs repo has its own CI — we just verify the new pages render clean).

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | CHORE(open): worktree. | `git worktree list` |
| 2 | Schema: trigger + NOTIFY fn. Schema Guard output. | §1.1 integration |
| 3 | `pg_listen.zig` client + filter. | §1.2 |
| 4 | SSE handler. | §1.3–1.6 |
| 5 | CLI watch + chat SSE upgrade. | §2, §3 |
| 6 | UI live activity panel + chat widget swap. | §4 |
| 7 | External docs: platform-ops + homebox-audit + arch page + homelab deletion + sloppy-page polish. | §5 |
| 8 | §6 UI dogfood under signup. | acceptance |
| 9 | Full gates + memleak (SSE long-lived conn). | green |
| 10 | CHORE(close). | PR green |

---

## Acceptance Criteria

- [ ] SSE endpoint streams events with <200ms latency to first event post-insert.
- [ ] CLI watch + chat both use SSE; chat history still fetched via GET /events on open.
- [ ] UI LiveActivityPanel renders + reconnects + pause-resume works.
- [ ] External docs: platform-ops launch post live, homebox-audit page live, arch doc mirror live, homelab post gone.
- [ ] Sloppy-page polish inventory addressed (list in Discovery).
- [ ] Changelog `<Update>` block lands.
- [ ] M37_001 §3.2 UI dogfood green.
- [ ] All gates green.

---

## Eval Commands

```bash
# SSE smoke
curl -N "$API/v1/workspaces/$WS/zombies/$ZID/activity:stream" -H "Authorization: Bearer $TOKEN" | head -20

# pg NOTIFY trigger
psql $DATABASE_URL -c "LISTEN zombie_activity_insert;" &
# insert into core.zombie_activities and assert notification received

# External docs grep
grep -rn 'homelab-zombie\|kubectl-first' ~/Projects/docs/ \
  | grep -v 'superseded\|historical\|CHANGELOG' \
  && echo "FAIL: stale homelab mention" || echo "ok"
```

---

## Discovery (fills during EXECUTE)

- Inventory of "sloppy pages" in ~/Projects/docs/ (§5.6).
- SSE buffering config for Fly.io deployment (if needed).
- Whether chat widget's polling fallback is kept for legacy browsers without EventSource support.

---

## Out of Scope

- WebSockets / bidirectional streaming. SSE is one-way and sufficient.
- Replay from arbitrary timestamp (client reconnect resumes from "now"). If replay becomes important, it lives on top of M35 GET /events, not SSE.
- Admin dashboard with aggregate graphs. Post-alpha.
- Mobile push notifications for zombie completions. Future.
