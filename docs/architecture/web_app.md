# Web app — `ui/packages/app`

Five statements govern the dashboard (Next.js 16 App Router, React 19.3).
Each one is falsifiable by grep; the migration table at the bottom is the
scoreboard. The backend has had this kind of doctrine for a year
(`runner_fleet.md`, `data_flow.md`); the front end gets its own here.

Stack facts this doc assumes: Turbopack dev/build, React Compiler off until
annotated (`next.config.ts`), Clerk for auth (`auth()` server-side), all
backend traffic through server reads/actions or authenticated same-origin
`/live` route handlers. The server uses `NEXT_PUBLIC_API_URL`, which throws when unset
(`lib/api/client.ts` `requireApiOrigin()`); no silent backend guessing.

## Chat workspace

The [chat workspace design](../designs/chat-within-a-workspace.md) applies these server/client boundaries to one authoring-and-runtime conversation.
It retains assistant-ui, keys private draft/shared fleet adapters by account/workspace/subject, and loads configuration details on intent. Save, Save and run now, and explicit Save to library follow the [single resource reference](api_resources.md).
The authoring API and workspace chat adapters are implemented. The [original API proposal](../reviews/workspace-setup/api.md) records design history; [API resources](api_resources.md) describes the current routes.

## The five statements

**1 · The server fetches. The client reacts.**
Backend authentication happens in a Server Component, Server Action or
authenticated route handler. The token comes from `auth()` on the server.
Stateful setup requests use the browser's same-origin cookie through the
whitelisted `/live/v1/workspaces/{workspaceId}/...` proxy; they carry no browser bearer.
The proxy authenticates upstream requests, bounds their bodies and checks write origins
(`lib/api/setup-proxy/`). Browser setup state lives in `lib/api/drafts.ts`.

`app/cli-auth/[session_id]/page.tsx` is another deliberate browser transport. That page is the command-line
handoff: it derives an Elliptic-Curve Diffie-Hellman key in the browser and
posts the encrypted result, so the call cannot move to the server without
moving the private key with it. It is the documented exception
([`../AUTH.md`](../AUTH.md) §"Why the dashboard rides one token"), and it is
separate from the setup proxy.
Every server fetch runs under one policy, `lib/api/retry.ts`: a declared
schedule with a 20 s deadline, a 10 s `Retry-After` cap, full jitter, and a
replay gate read from where the failure happened. A write the server may hold
is never sent twice on a gateway error or a client timeout; a socket the
transport reported dropping is sent again whatever the method, because a reply
nobody saw is usually a request that never ran. The price is named rather than
hidden: a reset arriving after the server accepted a write can double it, and
an idempotency key is what would retire that. The policy's dependency,
`effect`, is server-only:
a client component reads status facts from `lib/api/errors.ts`, and
`.size-limit.mjs` fails the build if the runtime reaches a client chunk.

**2 · `"use client"` marks a leaf, never a branch.**
The directive goes on the smallest interactive unit — a button, a form, a
menu. When a client file imports a component that renders data, the
boundary is drawn too high: everything below a client boundary ships to the
browser whether it needs to or not.

**3 · Every route paints a shell before it paints data.**
`Suspense` with a skeleton wraps the data-dependent region; header, nav,
and frame render immediately. `force-dynamic` with no `Suspense` makes the
whole page wait on the slowest call. Exception on record: the fleet wall
(`w/[workspaceId]/fleets/page.tsx`) deliberately blocks its header on data
so it cannot paint "Fleets" over a first-run checklist — a documented trade,
not a precedent.

**4 · Mutations feel instant.**
Writes go through a Server Action wrapped in `useActionState`, with
`useOptimistic` painting the result before the server confirms. React 19
added these primitives so `useState` + `useEffect` + `fetch` chains stop
reimplementing them badly.

**5 · `useEffect` is for subscriptions, not for loading.**
Event streams, timers, observers — yes. Fetching on mount — no; that is
statement 1 in disguise, and it guarantees a blank frame plus a waterfall.

## The two shapes

Today — client-heavy path:

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

Target — shell first, stream the rest, mutate optimistically:

```
 Browser                      Next server                 agentsfleetd
    │                              │                            │
    │──── GET /w/x/fleets ────────▶│                            │
    │◀═ shell + skeleton (instant) ═│                           │
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
  │  layout.tsx   page.tsx   lib/api/*   actions/* │  ← tokens, fetches, secrets
  │  Suspense boundaries drawn here                │
  └────────────────────┬───────────────────────────┘
                       │  props: plain data only
  ┌────────────────────▼─── CLIENT (leaves) ───────┐
  │  <FleetTile onRename>  <SecretForm>  <Menu>    │  ← useTransition, useOptimistic,
  │  cookie-only setup transport; no bearer/key    │     useActionState live here
  └────────────────────────────────────────────────┘
```

## Scoreboard

Run the greps, then move the numbers. Every row is measurable in one command,
so a stale row is a choice. Re-measure at any milestone that touches the app
and update this table in the same diff.

Measured Oct 14, 2026 against `ui/packages/app`: 210 non-test `.tsx` files
under `app` and `components`.

| Signal | Today | Target | Grep |
|---|---|---|---|
| `"use client"` files | 148 | ~25 | `grep -rl '"use client"' app components \| wc -l` |
| `useEffect` files | 39 | ~5 | `grep -rl useEffect app components hooks \| wc -l` |
| `useActionState` | 0 | every form | `grep -rl useActionState app components \| wc -l` |
| `useOptimistic` | 5 | every mutation surface | `grep -rl useOptimistic app components \| wc -l` |
| `Suspense` files | 5 | every data route | `grep -rl Suspense app \| wc -l` |

The first two moved away from target as the app grew. `useOptimistic` moved
from one surface to five — secrets delete, runner state, the approvals inbox,
the fleet kill switch and workspace library removal — each reconciling inside
the transition that set it,
so the row is now a pattern rather than an exception. The one `useEffect` file
the chat's summary strip gained is statement 5 as written: a subscription to
the fleet stream (`useFleetRunSummary`, selecting the two identity-stable
slices the strip reads so a streaming reply's chunks re-render nothing), never
a load — a completion frame carries the row and the fleet facts, so the strip
moves with no request behind it. `ChatView` itself has no effect left.
`useActionState` and `Suspense` have not moved. All of it is the point of
keeping the table.

The Models route is the worked example. `ModelCatalogueProvider` fetched the
entire global model catalogue in a mount effect, so every visit to Models
paid for data most visits never opened a dialog to use; it now loads on
intent — dialog open, focus, or an eligible hover.

Statement 3 covered the route too, which previously awaited every read before
painting a pixel. It now renders its header immediately and streams an
exported async data region under `Suspense`, matching `ApprovalsData`.

One boundary worth recording, because it is easy to get backwards: those
regions never REJECT. Suspense there buys latency, not error handling — a
rejected promise throws in render and needs an ErrorBoundary, which would
collapse "this read failed" and "this library is empty" into one
undifferentiated fallback. Keeping them distinct is what lets a user whose
read failed see a retry instead of being told they have nothing.

Migration happens route by route inside normal milestone work — statement
compliance is checked for touched files at review, not by a big-bang
refactor. The PLAN quality-ceiling line (operating model) is where a
larger cut gets proposed when a route's patch fights these statements.

## Where this is headed — a Backend-for-Frontend

The dashboard server already obtains the customized Clerk session token for
backend reads, actions and streams. Application code in the browser holds no
backend bearer. [`../AUTH.md`](../AUTH.md) §"Why the dashboard rides one token"
describes this boundary.

Workspace setup uses a narrow Backend-for-Frontend today: its `/live` handler
authenticates each allowed request and stream. Writes also require a matching
origin. This handler is separate from the generic `/backend` rewrite, which
cannot bypass its checks.

A common boundary for every dashboard route remains future work, alongside
scoped capability tokens. Backend authorization must continue to cover the
command-line and tenant-key callers as well as the dashboard.

Keeping tokens out of application state does not prevent a compromised page
from calling Clerk's own token API. Content-Security-Policy and
Subresource-Integrity remain separate browser protections.

## Workspace setup and runtime subjects

Opening New fleet allocates no resource. The first mutation creates a creator-private draft. Draft and fleet adapters key all messages, pending sends, streams and caches by account/workspace/subject. Recovery reads durable messages; streams carry authorized hints. Closing, logout, revocation, delete and subject switches clear private buffers; unsent text stays in memory, never browser local storage. No workspace-wide private broadcast is permitted.

The private setup, workspace activity and fleet activity proxies bound stream handshakes to 30 seconds and rotate established bodies at 240 seconds from request start.
`StreamLifetime` forwards bytes on demand, ends rotation cleanly, and aborts the upstream request.
Browser cancellation, request cancellation, upstream failure and ordinary completion release timers and the upstream reader.
The proxies forward `Last-Event-ID` unchanged. Private setup reconnects retain the authorized subject and recover durable messages through the existing client. Runtime event streams use connection-local counters; the daemon ignores the header and the client recovers gaps through the durable events list.

A shared Definition powers drawer edits, model proposals and review. Save retains one fleet identity with visible setup checks. Save and run now exposes Pending until the committed initial admission is confirmed with its expected event identity.

Save does not publish a Library entry. Models and billing resolve from the workspace owner's account; joined members receive authorized owner-facing actions. Edit fleet starts a private draft against a pinned setup revision.

Runtime controls and shared fleet history retain their existing authority. See [fleet_definitions.md](fleet_definitions.md#validated-definition), [data_flow.md](data_flow.md#fleet-creation-and-setup-recovery), and [API resources](api_resources.md).

Review explains the proposed task, run triggers, access, and spending limits before save. Opening review requests no run and creates no fleet.
The saved view separates setup readiness from first-run admission; an accepted run can still be running.

The inline receipt shows `Saved vN` and explains which runs use that version.
The saved response supplies the Definition when present; otherwise the view retains the reviewed candidate's Definition. Unknown trigger or access data stays unconfirmed.

### One fleet workspace

New fleet keeps the private setup conversation open across multiple messages.
The header exposes **Save fleet** when the reviewed candidate can be saved.
Successful Save opens that fleet's Chat.

The sidebar's **Fleets** list shows authorized saved fleets.
Private drafts appear under **Unfinished setups** on **New fleet**.

Active fleet dots use the brand color; hover or keyboard focus reveals the delete control.

Fleet **Edit setup** opens a private chat panel alongside the mounted runtime conversation. **Stop** and permanent **Kill** live in **Fleet actions**.

The panel exposes **Save changes** above its composer and optional **Edit instructions manually** below its conversation.
Saving a revision closes the panel, refreshes server data and preserves unsent runtime text.

Existing runs keep their pinned revision; new runs use the saved version.
The fleet sections are **Chat**, **Activity**, and **Memory**. **Your fleets** in the sidebar opens the fleet listing.

Both purposes use `ChatConnection`, `ReplyStream`, `ChatThread`, `ChatMessageRow` and `ChatComposerFrame`.
Effects own connections and cleanup; render stays pure and Hooks remain static.
Private history loading does not wait for model or sidebar lists.

An identity-owned cache retains at most five conversations and 1 MiB for 120 seconds, with access loss clearing its contents.
See [shared chat implementation and verification](shared_chat_verification.md#browser-ownership) for the call graph and recovery rules.
