# Web app — `ui/packages/app`

Five statements govern the dashboard (Next.js 16 App Router, React 19.3).
Each one is falsifiable by grep; the migration table at the bottom is the
scoreboard. The backend has had this kind of doctrine for a year
(`runner_fleet.md`, `data_flow.md`); the front end gets its own here.

Stack facts this page assumes:

- Turbopack dev/build.

- The React Compiler in annotation mode, so only annotated components compile (`next.config.ts`).

- Clerk for auth (`auth()` server-side).

- All backend traffic through server reads, actions or authenticated same-origin `/live` route handlers.

- The server uses `NEXT_PUBLIC_API_URL`, which throws when unset (`lib/api/client.ts` `requireApiOrigin()`); no silent backend guessing.

## Chat workspace

The dashboard applies these server and client boundaries to one conversation per fleet.
A person's first message on **New fleet** creates the fleet, and every later message goes to that fleet's chat.
The chat keeps assistant-ui, keys its streams by account, workspace and signed-in person, and loads fleet details on intent.

[API resources](api_resources.md) owns the routes the app calls.

## The five statements

**1 · The server fetches. The client reacts.**

Backend authentication happens in a Server Component, Server Action or
authenticated route handler. The token comes from `auth()` on the server.
Live chat requests use the browser's same-origin cookie through the allowlisted
`/live/v1/workspaces/{workspaceId}/...` route handlers; they carry no browser bearer.

Those handlers authenticate upstream requests, bound their bodies and check write origins.
They carry a fleet's messages, turn streams, uploads and task reads.

`app/cli-auth/[session_id]/page.tsx` is another deliberate browser transport, the command-line
handoff. It derives an Elliptic-Curve Diffie-Hellman key in the browser and
posts the encrypted result. The call cannot move to the server without moving
the private key with it.

It is the documented exception
([`../AUTH.md`](../AUTH.md) §"Why the dashboard rides one token"), and it is
separate from the `/live` handlers.

Every server fetch makes one attempt and sets no deadline of its own: a
caller that wants one passes its own signal. A failed request surfaces to the
caller, who decides whether to send it again; a send carries its operation id,
so a resend is answered with the first turn.

**2 · `"use client"` marks a leaf, never a branch.**

The directive goes on the smallest interactive unit: a button, a form, a
menu. When a client file imports a component that renders data, the
boundary is drawn too high. Everything below a client boundary ships to the
browser whether it needs to or not.

**3 · Every route paints a shell before it paints data.**

`Suspense` uses the shared `RouteLoading` indicator around data-dependent regions.
The header, navigation and frame render immediately.
`force-dynamic` with no `Suspense` makes the whole page wait on the slowest call.

The workspace fleet entry reads one fleet and redirects to its conversation. An empty workspace opens the new-fleet composer. The shared route fallback covers that lookup.

**4 · Mutations feel instant.**

Writes go through a Server Action wrapped in `useActionState`, with
`useOptimistic` painting the result before the server confirms. React 19
added these primitives so `useState` + `useEffect` + `fetch` chains stop
reimplementing them badly.

**5 · `useEffect` is for subscriptions, not for loading.**

Event streams, timers, observers: yes. Fetching on mount: no. That is
statement 1 in disguise, and it guarantees a blank frame plus a waterfall.

## The two shapes

Today, the client-heavy path:

```
 Browser                      Next server                 agentsfleetd
    │                              │                            │
    │──── GET /w/x/fleets ────────▶│                            │
    │                              │─── async page, await ─────▶│
    │                              │◀────── fleets JSON ────────│   ← page blocks
    │◀═══ full HTML, all at once ══│                            │
    │  hydrate ~150 client files ──────────────────┐            │
    │                                              ▼            │
    │  useEffect fires ── second fetch from browser ───────────▶│   ← waterfall
    │◀──────────────────────────────────────────────────────────│
    ▼
  first interaction ═══════════════════════════════▶ late
```

Target: shell first, stream the rest, mutate optimistically:

```
 Browser                      Next server                 agentsfleetd
    │                              │                            │
    │──── GET /w/x/fleets ────────▶│                            │
    │◀═ shell + loader (instant) ═══│                           │
    │                              │─── Suspense boundary ─────▶│
    │◀═══ streamed rows ════════════│◀───── fleets JSON ────────│
    │  hydrate ~25 leaf client files (buttons, forms only)      │
    ▼                                                           │
  interactive early                                             │
    │  user renames a fleet                                     │
    │  useOptimistic paints new name          ← 0 ms            │
    │  useActionState → Server Action ─────────────────────────▶│
    │◀── revalidate; real value replaces optimistic ────────────│
```

The bar between the worlds:

```
  ┌──────────────────── SERVER ────────────────────┐
  │  layouts   pages   server reads   actions      │  ← tokens, fetches, secrets
  │  Suspense boundaries drawn here                │
  └────────────────────┬───────────────────────────┘
                       │  props: plain data only
  ┌────────────────────▼─── CLIENT (leaves) ───────┐
  │  <RecentChats>         <SecretForm>  <Menu>    │  ← useTransition, useOptimistic,
  │  cookie-only /live transport; no bearer/key    │     useActionState live here
  └────────────────────────────────────────────────┘
```

## Scoreboard

Run the greps, then move the numbers. Every row is measurable in one command,
so a stale row is a choice. Re-measure at any milestone that touches the app
and update this table in the same diff.

Measured Nov 07, 2026 against `ui/packages/app`: 280 non-test `.tsx` files
under `app` and `components`.

| Signal | Today | Target | Grep |
|---|---|---|---|
| `"use client"` files | 204 | ~25 | `grep -rl '"use client"' app components \| wc -l` |
| `useEffect` files | 53 | ~5 | `grep -rl useEffect app components hooks \| wc -l` |
| `useActionState` | 0 | every form | `grep -rl useActionState app components \| wc -l` |
| `useOptimistic` | 3 | every mutation surface | `grep -rl useOptimistic app components \| wc -l` |
| `Suspense` files | 6 | every data route | `grep -rl Suspense app \| wc -l` |

The first two keep moving away from target as the app grows. `useOptimistic`
lives in three surfaces today: secrets delete, runner state and the fleet
archive action, each reconciling inside the transition that set it.

The chat's summary strip owns one `useEffect`, and it is statement 5 as written. It is a subscription to the fleet stream (`useFleetRunSummary`), never a load.

It selects the two identity-stable slices the strip reads, so a streaming
reply's chunks re-render nothing. A completion frame carries the row and the
fleet facts, so the strip moves with no request behind it.

`ChatView` itself has no effect left. `useActionState` has not moved, and
`Suspense` covers six files. All of it is the point of keeping the table.

The Models route is the worked example. `ModelCatalogueProvider` fetched the
entire global model catalogue in a mount effect. Every visit to Models paid for
data most visits never opened a dialog to use.

It now loads on intent: dialog open, focus, or an eligible hover.

Model display names use one shared store.
The layout starts one catalogue read per sign-in or refresh and passes its promise to `ModelLabelsProvider`.
Next retains that layout across client navigation.

Screens read names through `useModelLabels()` and `modelLabel`.
Unknown, retired or ambiguous identifiers remain as recorded.
Usage reads one report and no catalogue.

Only name consumers suspend: the Usage fleet table waits while the frame, totals and daily graph can stream.
The server catches a label-read failure before forwarding the promise.
Consumers then receive recorded identifiers rather than an unhandled rejection.

Statement 3 covered the route too, which previously awaited every read before
painting a pixel. It now renders its header immediately and streams an
exported async data region under `Suspense`, matching `ApprovalsData`.

One boundary worth recording, because it is easy to get backwards: those
regions never REJECT. Suspense there buys latency, not error handling.

A rejected promise throws in render and needs an ErrorBoundary. That would collapse "this read failed" and "this library is empty" into one
undifferentiated fallback.

Keeping them distinct is what lets a user whose read failed see a retry
instead of being told they have nothing.

Migration happens route by route inside normal milestone work. Statement
compliance is checked for touched files at review, not by a big-bang
refactor. The PLAN quality-ceiling line (operating model) is where a
larger cut gets proposed when a route's patch fights these statements.

## Where this is headed — a Backend-for-Frontend

The dashboard server already obtains the customized Clerk session token for
backend reads, actions and streams. Application code in the browser holds no
backend bearer. [`../AUTH.md`](../AUTH.md) §"Why the dashboard rides one token"
describes this boundary.

The fleet chat uses a narrow Backend-for-Frontend: its `/live` handlers
authenticate each allowed request and stream. Writes also require a matching
origin. These handlers are separate from the generic `/backend` rewrite, which
cannot bypass their checks.

A common boundary for every dashboard route remains future work, alongside
scoped capability tokens. Backend authorization must continue to cover the
command-line and tenant-key callers as well as the dashboard.

Keeping tokens out of application state does not prevent a compromised page
from calling Clerk's own token API. Content-Security-Policy and
Subresource-Integrity remain separate browser protections.

## Fleet chat subjects

Opening **New fleet** allocates no resource. Send calls `POST /v1/workspaces/{workspace_id}/fleets`, which writes the fleet and its first turn, and the browser replaces the URL with the fleet's page.
Messages, pending sends, streams and caches bind account, workspace and fleet.

A refused create returns the message to the composer with **Resend**, and the sidebar lists no fleet.
The refusal names its code, such as `UZ-SETUP-008` when the workspace has no usable model.
[Fleet creation and recovery](data_flow.md#fleet-creation-and-recovery) owns the transaction and its retry key.

Logout, revocation and deletion clear the browser's buffers. Unsent text stays in memory and is never broadcast across the workspace.

The workspace activity and fleet activity proxies bound stream handshakes to 30 seconds and rotate established bodies at 240 seconds from request start.
`StreamLifetime` forwards bytes on demand, ends rotation cleanly, and aborts the upstream request.
Browser cancellation, request cancellation, upstream failure and ordinary completion release timers and the upstream reader.

The proxies forward `Last-Event-ID` unchanged. Turn streams use connection-local counters; the daemon ignores the header and the client recovers gaps through the durable turns list.

Models and billing resolve from the workspace owner's account; joined members receive authorized owner-facing actions.
Runtime controls and shared fleet history keep their existing authority. [API resources](api_resources.md) owns each route's scope.

### One fleet page

Below, F is `/v1/workspaces/{workspace_id}/fleets/{fleet_id}`.

**New fleet** (`/w/{workspaceId}/fleets/new`) renders the fleet composer.
Its suggestion buttons fill the composer with editable text from `ui/packages/app/public/fleet-starters.json`.
Any member sees a new fleet at once, in the sidebar's **Fleets** list.

Active fleet dots use the brand color; hover or keyboard focus reveals the delete control.
**Archive** and permanent **Delete** live in **Fleet actions**; **Your fleets** opens the listing.

Beside the chat, the fleet page shows three short lists of rows that exist:

| List | Reads | A person's action |
|---|---|---|
| **Wakes on** | `GET F/schedules` and `GET F/triggers` | remove a schedule or a trigger; copy a webhook trigger's URL |
| **Can access** | approved grants from `GET F/integration-grants` | **Remove** sends `DELETE F/integration-grants/{id}`, which revokes the grant |
| **Remembers** | the fleet's pinned entries from `GET F/memories` | remove sends one `DELETE F/memories/{key}`, and the row leaves |

The fleet adds its own schedules and triggers, pins its own memory and asks for its own access; the lists only show and remove.
[Fleet triggers](connectors.md#fleet-triggers), [pinned memory](memory.md#6-what-reaches-the-model) and [grants](connectors.md#where-a-grant-comes-from) own those records.

When the fleet calls `request_access`, an access card appears in the chat with **Approve** and **Deny**.
**Approve** sends one approve request, and the waiting run resumes holding the access. A 403 leaves the card open with the refusal's message.

The header's model picker sends one `PATCH F` carrying `model_ref`. The **Allow all** and **Packages** switches each send one `PATCH F`, carrying `http_egress` or `package_domains`.

Both switches start off, and a 403 leaves the switch off. The [egress model](runner_fleet.md#egress-model-outbound-is-the-only-network-surface) owns what each one opens.

Fleet messages, access cards and integration activity share one chronological conversation. **Saved memories** opens from **Fleet actions**.

A message sent while the fleet's run is live folds into that run. The row shows `Added to the current run` until the run answers it, and the view waits on the message's own event.

The chat uses `ChatConnection`, `ReplyStream`, `ChatThread`, `ChatMessageRow` and `ChatComposerFrame`.
Effects own connections and cleanup; render stays pure and Hooks remain static.

Fleet chat also renders `DurableTaskPanel` inline above its composer through `FleetThreadViewport`.
Its scoped reads load compact unfinished work; expanding a row loads the task's completion statements through `DurableTaskDetail`.
The [durable task owner](durable_tasks.md#status-and-ownership) distinguishes these reads from task attempts and recovery.

A person edits a task's statements in that detail, which sends one `PATCH F/durable_tasks/{id}` carrying `expected_generation`.
A 412 reloads the task and keeps the person's text, so a second editor's change is never overwritten silently.

The existing fleet stream supplies `durable_task_changed` read hints through `durableTasksSeq`; its initial hello and gap recovery also invalidate task reads.
Account or workspace changes abort old requests, and access loss clears private rows.
Transient failures retain labelled confirmed data and expose retry without claiming fresh progress.

See [shared chat implementation and verification](shared_chat_verification.md#browser-ownership) for the call graph and recovery rules.
