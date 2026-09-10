# Dashboard load — where the seconds go

Five questions an operator asked while walking the development dashboard, each
answered with a measurement and the command that produced it, or recorded as
unresolved with the measurement that would settle it. Nothing here proposes a
fix: this page is the evidence a fix would have to be argued from.

## Facts

| Fact | Value |
|---|---|
| Chat load, click → fleet sections visible | p50 1073 ms · p95 1126 ms (5 samples) |
| Secrets, click → list visible | p50 941 ms · p95 1670 ms (5 samples) |
| Runners, click → wall visible | p50 882 ms · p95 1202 ms (5 samples) |
| Runners retry attempts, every navigation | 1 — the ladder never climbs |
| Upstream reads per Secrets visit | 2, awaited together |
| Workspace-switcher reads per navigation | 1, on every surface |
| Worst-case render wait the retry policy allows | 20 000 ms (the deadline, not deadline + one timeout) |
| How these numbers were taken | `make acceptance-e2e`, project `dashboard-latency` |

> [!WARNING]
> **Read the millisecond figures as shape, not as measurement.** Two runs of the
> identical build produced Secrets p50 2425 ms and 765 ms; the upstream reads
> halved with them. Run-to-run variance on this lane exceeds the differences
> between surfaces. What transfers is the *ratio* (reads are ~40–47 % of a
> navigation) and the *counts* (attempts, reads per template), which are
> deterministic. Absolute latencies do not transfer, and these were taken from a
> local production build against the development Application Programming
> Interface (API) — not from a deployed dashboard.

---

## Chat load, drawn end to end

The question was how a chat load spends its time and what runs concurrently.

```
  click /w/{ws}/fleets/{id}
        │
        ├─ requireCredential()                     lib/auth/credential.ts
        │      resolves the bearer from the session
        │
        ├─ startViewData(view, …)  ────────────┐   .../fleets/[id]/components/view-data.ts
        │      issues listFleetMessages NOW    │   (the thread read, in flight)
        │                                      │
        ├─ await Promise.all([                 │   .../fleets/[id]/page.tsx
        │      loadFleet(...),                 │   fleet detail
        │      getTenantBillingCached(...),    │   billing, failure-tolerant
        │   ])                                 │
        │                                      │
        ├─ await loadFleetView(..., viewData) ─┘   awaits the thread started above
        │
        ├─ server HTML  →  dynamic thread chunk
        │
        └─ ONE EventSource per WORKSPACE           lib/streaming/workspace-stream.ts
               demultiplexed per fleet             → first live frame
```

**The overlap is the point.** `startViewData` runs *before* the `Promise.all`
that awaits the fleet detail, so a slow detail read never serialises the
transcript behind it. That is pinned, not drawn from memory: hold a fleet read
open and the thread promise still settles first
(`the thread read is in flight before the fleet read is awaited`).

**One connection, not one per tile.** The wall opens a single `EventSource` per
workspace and routes each `fleet_id`-tagged frame to the tile that subscribed
for it. The measurement asserts at most one live connection across a whole
sample of navigations.

| Stage | Samples | First | p50 | p95 |
|---|---|---|---|---|
| Chat load | 5 | 1073 ms | 1073 ms | 1126 ms |

## Every request the wall makes

**Not yet measured.** The wall's counter data path is being rewritten to carry
its figures on the frame, which removes a fetch this inventory would otherwise
record. Taking the inventory before that lands would publish a table that is
stale on arrival, so it is deliberately deferred rather than guessed.

What *is* measured is the layout's own cost, below.

## One status, three answers

Three surfaces render a fleet's status and none of them agree. This table is
what each one actually renders, read off real renders rather than off the
source, and it is executable — `every fleet status has a matrix row` iterates
the status union, so a sixth status cannot ship without a row here.

| Status | Wall tile dot | Chat strip | Detail header |
|---|---|---|---|
| `active` | `bg-pulse` (only when genuinely live) | `text-pulse` | *no status element* — `KillSwitch` |
| `installing` | `bg-info` | inherited | `Badge variant="cyan"` |
| `paused` | `bg-muted-foreground` | inherited | *no status element* — `KillSwitch` |
| `stopped` | `bg-muted-foreground` | inherited | *no status element* — `KillSwitch` |
| `killed` | `bg-muted-foreground` | inherited | *no status element* — `FleetConfig` |

**The sharpest disagreement is not a colour.** The detail header gives a status
element to `installing` alone; for the other four it renders a lifecycle
control in that slot. So "is ACTIVE the same colour everywhere?" has no answer
on that surface — it shows no status colour at all.

The header's fourth branch renders a plain `Badge` for anything else. That is
not dead code: `FleetDetail.status` is typed `string`, so the branch is the
forward-compatibility fallback for a status this client does not know, and it
is what keeps an unrecognised state from being handed a kill control.

Choosing one mapping is a design decision and belongs to whichever change
takes it on, with this table as the "before".

## The Secrets wait

The page awaits two upstream reads together and renders behind its own loading
file. Both reads are wrapped in a per-render `cache()`, which dedupes inside one
render and has never spanned navigations — a second visit pays the pair again,
and the measurement asserts exactly that.

| Stage | Samples | First | p50 | p95 |
|---|---|---|---|---|
| Secrets navigation | 5 | 1059 ms | 941 ms | 1670 ms |
| secrets list read (upstream) | 5 | 350 ms | 354 ms | 480 ms |
| tenant provider read (upstream) | 5 | 404 ms | 403 ms | 518 ms |

The provider read is the slower of the pair. **But the pair is not where the
wait lives.** The two are awaited in one `Promise.all`, so their combined cost
is the slower one — ~403 ms against a ~941 ms navigation. Roughly half the wait
is outside the upstream reads, in render and transfer. Attributing the spin to
either read would be wrong.

## The Runners wait

Reported as a page that never returns. It returns, every time.

| Stage | Samples | First | p50 | p95 |
|---|---|---|---|---|
| Runners navigation | 5 | 1202 ms | 882 ms | 1202 ms |
| runners list read (upstream) | 5 | 374 ms | 382 ms | 394 ms |

**Observed retry attempts: 1, 1, 1, 1, 1.**

### Verdict on the candidate causes

| Candidate | Verdict | Why |
|---|---|---|
| Pool acquisition | **Excluded** | Nothing retried, so nothing timed out, so `PoolTimedOut` never fired and `classify_acquire` never saw a capacity case |
| The auth hop | **Excluded** | `hasScope` resolves from session claims; it makes no upstream call |
| A cold instance | **Not implicated** | The first sample sits at the p50, with no cold-start spike |
| The upstream query | **Partly implicated, unresolved** | The read is ~43 % of the navigation; query time cannot be separated from network from a browser |
| Render and transfer | **Unresolved — the majority** | ~57 % is unaccounted; settling it needs server-side timing of the render |

The datastore layer refuses to conflate a full pool with an absent datastore,
and this measurement never reaches that classification at all. Raising the pool
size would buy nothing here.

**The bound, computed rather than typed.** The retry policy allows a worst case
of 20 000 ms — the ladder deadline itself, *not* the deadline plus one attempt
timeout. Two clamps make the deadline a true total: the policy refuses a sleep
that would end past it, and a late attempt is given only the remaining budget as
its own ceiling. The pin derives this from the exported defaults, so changing
any constant moves the assertion.

## The layout's own cost

Every dashboard navigation pays one workspace-switcher read, and exactly one —
the per-request `cache()` in `lib/workspace.ts` does what its comment claims.

| Surface | workspace-list reads | audited reads total |
|---|---|---|
| fleets wall | 1 | 1 |
| secrets | 1 | 3 |
| runners | 1 | 1 |

One earlier window — the first navigation after sign-in — counted 2. Whether a
full document load renders the root layout and the workspace guard in passes
`cache()` cannot span is **unresolved**; the measurement that would settle it is
this same count taken on a cold load versus a client-side transition.

## How these numbers were taken

The lane navigates each surface a fixed number of times, reads the server-side
fetch audit around each navigation, and attaches a stage table to the run.

```
make acceptance-e2e                      # the whole suite, as Continuous Integration runs it
CI=1 bunx playwright test \              # this lane alone; CI=1 turns on the
  --config=playwright.acceptance.config.ts \   # reporters that persist attachments
  --project=dashboard-latency --no-deps
```

Three properties keep the lane honest:

- **Every assertion is a count; every latency is only attached.** Run-to-run
  variance can never fail a build, and a build can never bless a latency.
- **A disabled audit fails loudly** rather than reporting zeros, and a non-zero
  pre-count fails rather than being subtracted.
- **No figure comes from one sample.** A percentile over fewer samples than
  declared fails instead of publishing.

The lane resets an application-global counter, so it runs in its own project
ordered after the other audit-resetting lane. It is therefore the first thing
skipped when anything upstream in the acceptance chain fails — a silent skip is
its likely failure mode, not a red build.
