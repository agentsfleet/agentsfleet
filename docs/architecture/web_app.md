---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Web app: how the dashboard is built

> Read this when you change `ui/packages/app`: a page, a server read, the chat, or the fleet page. Parent: [`README.md`](./README.md) · Routes it calls: [API resources](./api_resources.md).

## What it is

The dashboard is a Next.js App Router application in `ui/packages/app`.
Its server holds every backend token; the browser holds none.
Each fleet is one conversation, and a person's first message on **New fleet** creates the fleet.

| Fact | Value | Where |
|---|---|---|
| Framework | Next.js 16.4.6, React 19.3.1, Turbopack | `package.json` |
| Compiler | the React Compiler in annotation mode; only annotated components compile | `next.config.ts` |
| Sign-in | Clerk; `auth()` on the server | `lib/auth/**` |
| Backend address | `NEXT_PUBLIC_API_URL`; unset throws | `requireApiOrigin()` in `lib/api/client.ts` |
| Live chat transport | same-origin `/live/v1/workspaces/{workspaceId}/…` route handlers | `app/live/` |
| Generic proxy | `/backend/:path*` rewrite to the API | `next.config.ts` |
| Stream lifetime | handshake 30 s; body rotated at 240 s | `StreamLifetime` in `lib/api/stream-lifetime.ts` |
| Chat library | `@assistant-ui/react` 0.15.36 and `@assistant-ui/store` 0.3.23, pinned exactly and patched | root `package.json` |

```text
┌────────────────────── SERVER ──────────────────────┐
│  layouts · pages · server reads · Server Actions   │  ← tokens, fetches, secrets
│  /live route handlers · Suspense boundaries        │
└──────────────────────────┬─────────────────────────┘
                           │  props: plain data only
┌──────────────────────────▼───── CLIENT (leaves) ───┐
│  RecentFleetList · SecretsList · menus · composer  │  ← useTransition, useOptimistic,
│  cookie-only /live transport; no bearer, no key    │     subscriptions
└────────────────────────────────────────────────────┘
```

## Why it exists

A token in browser code can be read by any script on the page.
Keeping every backend call on the server, or behind a same-origin handler, keeps the token there too.
Small client leaves send less JavaScript and paint the page frame before its data arrives.

## How it behaves

### Five rules

| Rule | What it means |
|---|---|
| The server fetches; the client reacts | Backend calls happen in a Server Component, a Server Action or an authenticated route handler, with the token from `auth()` |
| `"use client"` marks a leaf | The directive goes on the smallest interactive unit: a button, a form, a menu |
| A shell paints before data | `Suspense` with the shared `RouteLoading` indicator wraps data regions; header and navigation render at once |
| Mutations feel instant | A Server Action, with `useOptimistic` where the result is known before the server confirms |
| `useEffect` is for subscriptions | Event streams, timers and observers; never a fetch on mount |

Every server fetch makes one attempt and sets no deadline of its own; a caller that wants one passes its own signal.
A failed send surfaces to the caller, and a resend carries the same operation id, so the API answers with the first turn.
A data region under `Suspense` never rejects, so a failed read shows a retry rather than an empty list.

### How the browser reaches the API

The `/live` handlers carry a fleet's messages, turns, turn streams, uploads and task reads.
They authenticate each upstream request with the session's token, bound request bodies, and check the origin on writes.
They are separate from the `/backend` rewrite, which cannot bypass their checks.

`app/cli-auth/[session_id]/page.tsx` is the one other browser transport: the command-line sign-in handoff.
It derives an Elliptic-Curve Diffie-Hellman key in the browser, so the call cannot move to the server ([`../AUTH.md`](../AUTH.md)).

Models and billing resolve from the workspace owner's account; members get the owner-facing actions they are authorized for.
[API resources](./api_resources.md) owns each route's scope.

### New fleet

**New fleet** (`/w/{workspaceId}/fleets/new`) renders the composer, and opening it creates nothing.
Its suggestion buttons fill the composer with editable text from `public/fleet-starters.json`.
On sign-in at `/`, the server selects the default workspace, reads its first fleet and redirects directly to the conversation or new composer. The destination loads the workspace switcher, balance and invites. The billing page loads the global model catalog for display labels.
`/w/{workspaceId}/fleets` redirects to the first fleet, or to **New fleet** when the workspace has none.

Sending calls `POST /v1/workspaces/{workspace_id}/fleets`, and the browser replaces the URL with the fleet's page.
A refused create returns the text to the composer with **Resend** and names the code, such as `UZ-SETUP-008` when no model is usable.
[Data flow](./data_flow.md) owns the create transaction and its retry key.

### The fleet page

The sidebar's **Your Fleets** button shows or hides the workspace's fleet list (`components/layout/SidebarFleets.tsx`, `RecentFleetList.tsx`).
**Fleet actions** holds **Archive**, permanent **Delete** and **Saved memories**.
The fleet detail supplies each initial list the viewer has permission to read. Schedules require `schedule:read`, triggers require `fleet:read` and grants require `grant:read`. Pinned memory has its own read. Turn completion or an approval can refresh the lists.
Beside the chat, three short lists show rows that exist (`components/domain/fleet/FleetListSections.tsx`).

| List | Reads | A person's action |
|---|---|---|
| **Wakes on** | `GET F/schedules` and `GET F/triggers` | remove a schedule or trigger |
| **Can access** | approved grants from `GET F/integration-grants` | **Remove** revokes the grant with `DELETE F/integration-grants/{id}` |
| **Remembers** | pinned entries from `GET F/memories` | remove sends `DELETE F/memories/{key}` |

The fleet adds its own schedules and triggers, pins its own memory and asks for its own access; the lists only show and remove.

When the fleet calls `request_access`, an access card appears in the chat with **Approve** and **Deny**.
A 403 leaves the card open with its message.

The header's model picker sends one `PATCH F` with `model_ref`; the **Allow all** and **Packages** switches each send one `PATCH F` and start off.

### The chat stream in the browser

```text
FleetThread
  ├─ FleetStreams (one per account, workspace and signed-in person)
  │    ├─ ChatConnection: EventSource, deadlines, reconnects, cleanup
  │    └─ ReplyStream: stream order, gaps, final text ── ReplyStreamDecoder
  └─ ChatThread · ChatMessageRow · ChatComposerFrame (shared with other chats)
```

`FleetStreams` keys every stream by account, workspace and signed-in person (`lib/streaming/fleet-stream-registry.ts`).
Turn frames carry one zero-based sequence; a gap stops appending preview text, and the adapter repairs from the durable turns list.
The proxies forward `Last-Event-ID` unchanged, and the daemon ignores it for turn streams.

React builds the stream objects without starting I/O during render; effects start them and dispose them on identity change or unmount.
`useSyncExternalStore` reads stable, immutable snapshots, so earlier rows keep their object identity.
Streaming updates share one animation-frame update, and unmount cancels queued work.

A message sent while the fleet's run is live folds into that run.
Its row reads `Added to the current run` until the run answers it (`lib/streaming/fleet-stream-row.ts`).
Logout and identity change close the streams and clear every cached reply (`lib/streaming/fleet-stream-registry.ts`).

### Sending and reconnecting

Each send enters a ledger from the moment Send is pressed until the daemon's `202` is reconciled, one entry per operation ID (`lib/streaming/pending-sends.ts`).
Resend posts the same operation ID, so the daemon answers the first turn instead of running a second.
The ledger is keyed by signed-in person, workspace and fleet, mirrored to `localStorage` for reloads and other tabs, and expires after a day.

A dismissed send leaves a tombstone, so a tab that missed the dismissal cannot write it back.
The next person on a shared browser never sees the last person's ledger.

| Timer | Value | Source |
|---|---|---|
| Silence before a stream counts as lost, open or connecting | 45 s, three missed 15 s keep-alives | `stream-recovery-window.ts` |
| Grace before a stable connection's loss is shown | 25 s; a connection is stable after 30 s of frames | `stream-recovery-window.ts` |
| Fast reconnects | 5 attempts, 1 s doubling to a 15 s cap, with jitter | `fleet-stream-reconnect.ts` |
| Retry after the fast attempts | every 30 s, shown as not live | `fleet-stream-reconnect.ts` |
| Keep an unwatched stream open | 30 s after the last viewer leaves | `fleet-stream-registry.ts` |

Returning to the tab or coming back online retries at once.

### Tasks in the chat

`FleetThread` renders `DurableTaskPanel` (`components/domain/tasks/TaskPanel.tsx`) above the composer.
It loads the fleet's unfinished tasks; expanding a row loads that task's completion statements.
The fleet stream's `durable_task_changed` hints, its first hello and gap recovery each invalidate the task reads.

A person edits a task's statements in that detail, sending one `PATCH F/durable_tasks/{id}` with `expected_generation`.
A 404, 409, 412 or 428 answer reports the task as changed, so a second editor's change is never overwritten silently (`lib/api/tasks.ts`).
[Durable tasks](./durable_tasks.md) owns what the statements mean.

## Limits

- **No shared backend-for-frontend.** Only the chat's `/live` handlers form a checked boundary; other dashboard reads go through Server Actions or the `/backend` rewrite.
- **Rules are not yet met everywhere.** Most components still carry `"use client"`, and only `SecretsList` uses `useOptimistic`.
- **A compromised page can still ask Clerk for a token.** Keeping tokens out of app state does not stop that. Content security policy and subresource integrity are separate protections.
- **Browser timing stays local.** The `agentsfleet.chat.submit_to_first_visible` measure is not exported.
- **Patched chat library.** An `@assistant-ui` upgrade must keep the Bun patches and the browser checks for folding, resizing and scrolling back through history.

## Related pages

- [API resources](./api_resources.md): every route the app calls
- [Data flow](./data_flow.md): fleet creation and the live tail
- [Durable tasks](./durable_tasks.md): the task panel's data
- [Product analytics](./product_analytics.md): dashboard events
- [`../AUTH.md`](../AUTH.md): why the dashboard rides one token
