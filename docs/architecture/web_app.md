# Web app — `ui/packages/app`

Five statements govern the dashboard (Next.js 16 App Router, React 19.2).
Each one is falsifiable by grep; the migration table at the bottom is the
scoreboard. The backend has had this kind of doctrine for a year
(`runner_fleet.md`, `data_flow.md`); the front end gets its own here.

Stack facts this doc assumes: Turbopack dev/build, React Compiler off until
annotated (`next.config.ts`), Clerk for auth (`auth()` server-side), all
backend traffic through the `/backend` same-origin rewrite in the browser
and `NEXT_PUBLIC_API_URL` on the server — which throws when unset
(`lib/api/client.ts` `requireApiOrigin()`); no silent backend guessing.

## The five statements

**1 · The server fetches. The client reacts.**
Every network call to the backend happens in a Server Component or a Server
Action. A `"use client"` file that calls `fetch` is a bug. The token comes
from `auth()` on the server; shipping it to the browser to repeat the same
call adds a round trip and widens the credential surface.
Grep: `grep -rl '"use client"' app components | xargs grep -l 'fetch('` → 0.

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
    │  hydrate ~90 client files ───────────────────┐            │
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
  │  no fetch, no token, no secret                 │     useActionState live here
  └────────────────────────────────────────────────┘
```

## Scoreboard

Measured Jul 27, 2026 (`ui/packages/app`, 161 `.tsx` files). Move the
numbers, not the prose. Re-measure at any milestone that touches the app
and update this table in the same diff.

| Signal | Today | Target | Grep |
|---|---|---|---|
| `"use client"` files | 90 | ~25 | `grep -rl '"use client"' app components \| wc -l` |
| `useEffect` files | 23 | ~5 | `grep -rl useEffect app components hooks \| wc -l` |
| `useActionState` | 0 | every form | `grep -rl useActionState app components \| wc -l` |
| `useOptimistic` | 1 | every mutation surface | `grep -rl useOptimistic app components \| wc -l` |
| `Suspense` files | 4 | every data route | `grep -rl Suspense app \| wc -l` |

Migration happens route by route inside normal milestone work — statement
compliance is checked for touched files at review, not by a big-bang
refactor. The PLAN quality-ceiling line (operating model) is where a
larger cut gets proposed when a route's patch fights these statements.
