# M89_001: PostHog product-event coverage — dashboard first-class events + server-side capture

**Prototype:** v2.0.0
**Milestone:** M89
**Workstream:** 001
**Date:** Jun 10, 2026
**Status:** PENDING
**Priority:** **P1 — launch-relevant.** PostHog is wired app-wide for autocapture + pageviews, but the dashboard's core product actions emit **zero first-class events** and there is **no server-side capture** — so at launch, activation/conversion funnels would be measured off fragile autocapture Document-Object-Model (DOM) clicks, and billing/server-side conversions would be **invisible**. This closes the gap so usezombie can measure the launch (zombie-created → run → billing) with reliable, typed events.
**Categories:** UI, OBS
**Batch:** B1 — standalone; no backend (Zig) change, no schema. Pure UI/analytics instrumentation.
**Branch:** {feat/m89-posthog-product-events — added at CHORE(open)}
**Depends on:** none. PostHog client init + autocapture + `identify`-on-Clerk already ship; this layers first-class events on top.
**Provenance:** agent-surfaced in the **Jun 09, 2026 observability audit** (zombied/runner/UI). The UI sweep found global autocapture coverage but sparse manual events and no `posthog-node`. Re-confirm the cited `file:line` anchors at PLAN — they were read during the audit, not re-verified at authoring.

> **Provenance is load-bearing.** Every `file:line` below comes from the Jun 09 UI audit. Re-confirm at PLAN: `ui/packages/app/lib/analytics/posthog.ts:102` (client init), `ui/packages/app/instrumentation-client.ts` (global init), `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx:10-13` (identify on Clerk), `ui/packages/website/src/analytics/posthog.ts` (the dead `trackSignupCompleted` / `trackLeadCapture*` exports).

**Canonical architecture:** the dashboard (`ui/packages/app`, Next.js App Router) + marketing site (`ui/packages/website`, Vite). PostHog is the product-analytics plane (distinct from Prometheus metrics, the OpenTelemetry Protocol (OTLP) export, and the Postgres execution-telemetry store — see the observability map). This workstream adds the *product-event* layer; it does not touch the metrics/OTLP/telemetry planes.

---

## Implementing agent — read these first

1. `ui/packages/app/lib/analytics/posthog.ts` — the client init (`posthog.init`, line ~102) + `identifyAnalyticsUser` (line ~112). The typed event catalog + client capture helpers land here (or a sibling `events.ts`).
2. `ui/packages/app/instrumentation-client.ts` — Next.js global client-instrumentation; where `initAnalytics()` + `onRouterTransitionStart` fire today. No `PostHogProvider` exists — init is global, not a React provider.
3. `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx` — `identify` on Clerk auth-state change (root layout). **`reset()` on logout lands here** (§4).
4. `ui/packages/app/app/(dashboard)/**/actions.ts` — the 9 server-action files (zombies, runners, api-keys, models, billing, credentials, approvals, defaults, events). Today they emit **nothing**; §3 adds server-side capture at the mutation points.
5. `ui/packages/website/src/analytics/posthog.ts` — the marketing init + the **dead** `trackSignupCompleted` + `trackLeadCapture*` exports; §5 wires them to real call sites.
6. `dispatch/write_ts_adhere_bun.md` — all `*.ts`/`*.tsx` edits (const/import discipline, the UI + DESIGN-TOKEN gates). Analytics is mostly non-visual, but the gates still fire on `.tsx`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m89): PostHog product-event coverage — dashboard first-class events + server-side capture`
- **Intent (one sentence):** Every meaningful product action (zombie created/run, runner registered, API key minted, Bring-Your-Own-Key (BYOK) model added, credential added, approval resolved, checkout started/completed) emits a **single-sourced, typed PostHog event** — client-side for user-driven actions, server-side (`posthog-node`) for actions that complete without a click — so launch funnels are measured on reliable events, not autocapture DOM clicks.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Confirm the event taxonomy** (the exact event names + props per the catalog) and **which actions are client- vs server-emitted** — a click-driven mutation (create zombie form submit) emits client-side; a server-action/webhook-driven one (billing confirmed, background state change) emits server-side via `posthog-node`. **Confirm no Personally-Identifiable-Information (PII) / secret lands in event props** (no tokens, API keys, raw credentials — IDs + names only).

---

## Applicable Rules

- **`dispatch/write_ts_adhere_bun.md`** — `const`/import discipline; raw-HTML → design-system primitive (UI gate); `*-[...]` arbitrary → token utility (DESIGN TOKEN gate). The capture helpers are non-visual, but any touched `.tsx` still trips the gates.
- **UFS (manual, ui/)** — the UFS audit skips `ui/`; **by hand**, extract every event-name literal + prop shape into a single-sourced **as-const** catalog (`events.ts`), referenced everywhere — never re-spell `"zombie_created"` at a call site.
- **PII / secret discipline** (mirrors `docs/LOGGING_STANDARD.md` §6 in spirit) — event props carry IDs + names + enums only; **never** a token, API key, raw credential, or full secret. A reviewer audits every new `capture(...)` prop bag.
- **Effect-TS / module style** — if a touched server action is Effect-style, capture within the effect; if plain async, capture + `flush` before return. Confirm per-file at PLAN (don't assume one style).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI (component substitution) | **maybe** — only if a touched `.tsx` adds raw HTML | Use design-system primitives; most edits are capture-call insertions, not markup. |
| DESIGN TOKEN | **maybe** — touched `.tsx` | No arbitrary `*-[...]`; analytics edits add no styles. |
| LOGGING | **no** (PostHog is not the logfmt logger) | But apply the §6 PII discipline to event props by analogy. |
| SCHEMA / ERROR REGISTRY / ZIG | **no** | No Zig, no schema, no error codes — pure UI/analytics. |
| Coverage | **yes** — new helpers/components | `make test-unit-app` + the `dry-app` lane; aggregate coverage gate (per-file declaration-line artifacts excepted). |

---

## Overview

**Goal (testable):** A defined set of product actions each emit a typed PostHog event with the documented props (no PII), single-sourced from an as-const catalog; user-driven actions capture client-side, server-completed actions capture server-side via `posthog-node` (with a flush), and `reset()` fires on logout. Verified by unit tests on the catalog/helpers + capture-call assertions, and the marketing funnel's `signup_completed` actually fires.

**Problem (Jun 09 audit):** PostHog coverage is **global on the baseline** — both `app` and `website` init app-wide with `autocapture: true` + auto pageviews + pageleave, env-gated — but **thin on first-class events**:
1. **Dashboard product actions emit ZERO explicit events.** Creating a zombie, running it, registering a runner, minting an API key, adding a BYOK model, checkout/billing — all are autocapture-only (DOM clicks), which is fragile (selector-dependent) and useless for clean funnels.
2. **No server-side capture.** There is no `posthog-node` in the web app; the 9 `actions.ts` server-action files and the Server-Sent-Events route handler emit nothing — so any conversion that completes without a click (billing confirmed, background state change) is **invisible**.
3. **No `reset()` on logout.** Identity is set via `identify` on sign-in but never cleared → cross-session stitching risk on shared browsers.
4. **Website funnel is half-wired.** `trackSignupCompleted` + the four `trackLeadCapture*` events are defined/exported but **never called** — the marketing signup funnel has a start (`signup_started`) with no recorded completion.

**Solution summary:** Add a **single-sourced typed event catalog** and capture helpers; emit first-class events at the action sites (client-side for click-driven, server-side `posthog-node` for server-completed); fire `reset()` on logout; and wire the dead website funnel events. No backend change.

**Prioritization.** This is the **one launch-relevant** observability gap: without it, the launch is measured on autocapture DOM clicks with billing conversions invisible. The logging-discipline + error_code gaps from the same audit are internal hygiene (separate, post-launch).

---

## Prior-Art / Reference Implementations

- **PostHog server-side `posthog-node`** — the canonical pattern for capturing events from Next.js server actions / route handlers (init once, `capture({distinctId, event, properties})`, `await flush()` before the serverless function returns so events aren't dropped). The `distinctId` is the Clerk user id, stitched to the client `identify`.
- **Typed event catalog (as-const)** — the in-repo pattern for single-sourcing literals in `ui/` (UFS-by-hand): one `events.ts` exporting `EVENTS` as-const + a `Props<E>` map, so call sites reference `EVENTS.zombie_created`, never the bare string.
- **`identify` / `reset` lifecycle** — PostHog's standard: `identify` on auth (already wired in `AnalyticsBootstrap`), `reset` on logout (the missing half) to prevent anonymous/next-user stitching.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/analytics/events.ts` | CREATE | The single-sourced as-const event catalog (names + per-event prop types). UFS-by-hand. |
| `ui/packages/app/lib/analytics/posthog.ts` | EDIT | Add typed client `capture(event, props)` helper that consults the catalog. |
| `ui/packages/app/lib/analytics/posthog-server.ts` | CREATE | `posthog-node` server client + typed `captureServer(distinctId, event, props)` + `flush`. |
| `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx` | EDIT | `posthog.reset()` on Clerk sign-out (§4). |
| `ui/packages/app/app/(dashboard)/**/actions.ts` (subset that mutates) | EDIT | Server-side `captureServer` at the mutation points (zombie run, billing, runner register, key mint, BYOK add, credential add, approval resolve) (§3). |
| dashboard client components for click-driven actions (create-zombie form, register-runner dialog, mint-key dialog, BYOK wizard, checkout button) | EDIT | Client `capture` at the user action (§2). |
| `ui/packages/website/src/analytics/posthog.ts` | EDIT (small) | Keep the exports; they get wired (not re-defined). |
| website signup/CTA/Pricing components (`Hero`, `CTABlock`, `Pricing`, signup form) | EDIT | Call `trackSignupCompleted` + `trackLeadCapture*` at the real interactions (§5). |
| `docs/architecture/` analytics note OR `ui/packages/app/lib/analytics/README` | CREATE (small) | The event taxonomy (naming convention + catalog index). |

> **PLAN must enumerate the exact action set** (which of the 9 `actions.ts` files mutate + warrant an event) and **client-vs-server** per action — the table above is the audit-derived candidate set, not the verified final list.

---

## Decomposition & alternatives

- **Chosen shape:** one workstream (B1) — catalog + client helper + server helper + call-site wiring + website funnel. It is **additive instrumentation** (no behaviour change to the product itself; events are side-effects). Separable into the 5 Sections below.
- **Alternatives considered:** (a) **Rely on autocapture alone** — rejected: DOM-selector-fragile, no server-side conversions, no clean funnels (the status quo this closes). (b) **A heavyweight analytics abstraction / event bus** — rejected: over-engineered; a typed catalog + thin capture helpers is enough. (c) **Capture everything client-side** — rejected: server-completed conversions (billing) never reach the client, so `posthog-node` is required for those.

---

## Sections (implementation slices)

### §1 — Single-sourced typed event catalog

One `events.ts` exports `EVENTS` (as-const, snake_case `verb_noun`) + a per-event prop-type map. Every call site references the catalog; no bare event-name strings; no PII in any prop type (IDs/names/enums only).

- **Dimension 1.1** — the catalog enumerates the launch event set and is the only source of event-name strings → Test `events catalog is the single source (no bare event-name literals at call sites)`
- **Dimension 1.2** — no prop type admits a secret/token/raw-credential field (compile-time + a lint/test check) → Test `event props carry no PII/secret fields`

### §2 — Dashboard first-class events (client-side, user-driven)

At each click-driven action, the component calls the typed client `capture(EVENTS.x, props)`. Covers the actions a user triggers in the browser (create-zombie submit, register-runner, mint-key, BYOK-add, checkout-start).

- **Dimension 2.1** — each click-driven action emits its catalog event with the documented props on success → Test per action (`capture` called with `EVENTS.zombie_created` + props)
- **Dimension 2.2** — capture is **not** fired on validation failure / aborted action (event = success signal) → Test (no capture on error path)

### §3 — Server-side capture (`posthog-node`)

A server `posthog-node` client captures events that complete without a click — billing confirmed, background mutations — from the server actions / route handlers, with `distinctId = clerkUserId` and a `flush()` before return (serverless drops un-flushed events).

- **Dimension 3.1** — a server-completed action (e.g. checkout_completed / zombie run finished server-side) emits via `posthog-node` with the Clerk `distinctId` → Test (`captureServer` called + awaited flush)
- **Dimension 3.2** — the server client flushes before the serverless function returns (no dropped events) → Test (flush awaited on the success path)

### §4 — Identity hygiene: `reset()` on logout

`AnalyticsBootstrap` (or the logout handler) calls `posthog.reset()` when Clerk transitions to signed-out, so a subsequent anonymous/other session does not stitch to the prior `distinct_id`.

- **Dimension 4.1** — on sign-out, `posthog.reset()` is called exactly once → Test (`reset` called on auth→null transition)

### §5 — Website funnel completion

Wire the **already-defined** `trackSignupCompleted` + `trackLeadCapture*` exports to their real call sites so the marketing funnel records completions, not just `signup_started`.

- **Dimension 5.1** — `signup_completed` fires on a successful marketing signup → Test (event fired on the success interaction)
- **Dimension 5.2** — the `lead_capture_*` events fire at their interactions; no dead exports remain → Test + grep (every export has ≥1 call site)

---

## Interfaces

> **Illustrative — exact catalog + signatures verified at PLAN.** Contract, not implementation.

```ts
// ui/packages/app/lib/analytics/events.ts — single-sourced (UFS-by-hand)
export const EVENTS = {
  zombie_created: "zombie_created",
  zombie_run: "zombie_run",
  runner_registered: "runner_registered",
  api_key_minted: "api_key_minted",
  model_added: "model_added",            // BYOK
  credential_added: "credential_added",
  approval_resolved: "approval_resolved",
  checkout_started: "checkout_started",
  checkout_completed: "checkout_completed",
} as const;
export type EventName = (typeof EVENTS)[keyof typeof EVENTS];
// Per-event props: IDs/names/enums ONLY — NEVER a token/key/secret.
export type EventProps = {
  [EVENTS.zombie_created]: { zombie_id: string; template?: string };
  [EVENTS.api_key_minted]: { api_key_id: string };          // id, NOT the key
  // … one entry per event
};

// client capture (ui/packages/app/lib/analytics/posthog.ts)
export function capture<E extends EventName>(event: E, props: EventProps[E]): void;

// server capture (ui/packages/app/lib/analytics/posthog-server.ts)
export async function captureServer<E extends EventName>(
  distinctId: string, event: E, props: EventProps[E],
): Promise<void>; // awaits flush() before resolving
```

Contract: the product behaves identically; events are additive side-effects. No prop bag carries a secret. Without a PostHog key configured, both client + server capture are no-ops (env-gated, as today).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Server event dropped | serverless returns before `posthog-node` flushes | `captureServer` **awaits** `flush()`; Dim 3.2 asserts it. |
| PII leak in a prop | a dev adds `{ token }` / `{ api_key }` to a prop bag | the `EventProps` types forbid secret fields; reviewer audits every new `capture`; Dim 1.2 test. |
| Event name drift | a call site re-spells `"zombie_created"` | only `EVENTS.x` is allowed; Dim 1.1 test + grep for bare event literals. |
| No PostHog key (dev/preview) | env unset | capture is a no-op (env-gated); product unaffected. |
| Double-count (autocapture + first-class) | autocapture click + explicit event for the same action | acceptable — first-class events are the funnel source of truth; PLAN notes the overlap, optionally excludes the action's element from autocapture (`ph-no-capture`) only if it pollutes a funnel. |

---

## Invariants

1. **Single-sourced events** — every emitted event name comes from the `EVENTS` catalog; no bare event-name literal at any call site. Enforced by Dim 1.1.
2. **No PII/secret in props** — no event prop carries a token, API key, raw credential, or full secret; IDs/names/enums only. Enforced by `EventProps` types + Dim 1.2 + reviewer audit.
3. **Server events flush** — every `captureServer` awaits `flush()` before its caller returns. Enforced by Dim 3.2.
4. **Identity cleared on logout** — `reset()` fires on sign-out. Enforced by Dim 4.1.
5. **No dead funnel exports** — every `track*` export in the website analytics module has ≥1 call site. Enforced by Dim 5.2 + grep.
6. **Env-gated no-op** — without a PostHog key, capture is inert; the product is unchanged. (Existing behaviour, preserved.)

---

## Test Specification (tiered)

> **Lane:** UI unit + dry lanes — `make test-unit-app` (Vitest) for the catalog/helpers + capture assertions, `make dry-app` (Vitest + Playwright page renders, no Clerk) for render-safety. No backend lane.

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `events catalog single source` | call sites reference `EVENTS.*`; grep finds no bare event-name literals |
| 1.2 | unit | `event props carry no PII` | `EventProps` admits no `token`/`api_key`/`secret`/`password` field; a sample capture has only IDs/names |
| 2.1 | unit | `client capture per action` | each click-driven action calls `capture(EVENTS.x, props)` on success |
| 2.2 | unit | `no capture on error path` | a failed/aborted action does NOT capture |
| 3.1 | unit | `server capture via posthog-node` | a server-completed action calls `captureServer(distinctId, EVENTS.x, props)` |
| 3.2 | unit | `server flush awaited` | `captureServer` awaits `flush()` before resolving |
| 4.1 | unit | `reset on logout` | auth→null transition calls `posthog.reset()` exactly once |
| 5.1 | unit | `signup_completed fires` | a successful marketing signup fires `signup_completed` |
| 5.2 | unit + grep | `no dead funnel exports` | every `track*` export has a call site |

- **Regression:** existing app + website suites pass; autocapture/pageview behaviour unchanged.
- **Branch coverage:** feed success AND error inputs to each action so the "capture-on-success-only" branch is exercised (per the branch-coverage discipline).

---

## Acceptance Criteria

- [ ] `events.ts` catalog (as-const) is the single source of event names; no bare event-name literals at call sites — verify: Dim 1.1 test + grep
- [ ] No event prop carries a token/API key/raw credential/secret — verify: Dim 1.2 + `EventProps` types
- [ ] Each launch dashboard action emits its first-class event (client or server) on success, not on failure — verify: §2/§3 tests
- [ ] Server-completed actions capture via `posthog-node` with the Clerk `distinctId` and an awaited `flush()` — verify: Dim 3.1 + 3.2
- [ ] `reset()` fires on logout — verify: Dim 4.1
- [ ] Website `signup_completed` + `lead_capture_*` wired (no dead exports) — verify: Dim 5.1 + 5.2
- [ ] `make test-unit-app` + `make dry-app` green · aggregate coverage gate met
- [ ] Event taxonomy documented (naming convention + catalog index)
- [ ] No PostHog key → capture is a no-op (env-gated, unchanged)

---

## Eval Commands (post-implementation)

```bash
# E1: no bare event-name literals outside the catalog
grep -rnE '"(zombie_created|zombie_run|runner_registered|api_key_minted|model_added|checkout_(started|completed))"' ui/packages/app --include='*.ts*' | grep -v 'lib/analytics/events.ts'
# E2: posthog-node present + flush awaited
grep -rnE 'posthog-node|\.flush\(' ui/packages/app/lib/analytics/posthog-server.ts
# E3: reset on logout wired
grep -rn 'posthog.reset' ui/packages/app
# E4: no dead website funnel exports
grep -rnE 'trackSignupCompleted|trackLeadCapture' ui/packages/website/src
# E5: unit + dry lanes
make test-unit-app 2>&1 | tail -5 && make dry-app 2>&1 | tail -5
```

---

## Dead Code Sweep

**1. Orphaned files — none expected** (additive; the website `track*` exports become live, not removed).

**2. Orphaned references.** §5 turns the currently-dead `trackSignupCompleted` / `trackLeadCapture*` exports into live call sites; E4 must show ≥1 caller each.

---

## Discovery (consult log)

- **Origin (Jun 09, 2026):** the observability audit (zombied/runner/UI). UI sweep verdict: "Coverage is global on the baseline, thin on explicit events." Both packages init PostHog app-wide (`autocapture:true` + pageviews + pageleave, env-gated); no route group is outside the client. Gaps: dashboard product actions emit zero first-class events; no `posthog-node` server-side (9 `actions.ts` + the SSE route handler silent); no `reset()` on logout; website `signup_completed`/`lead_capture_*` defined-but-never-called.
  - Cited anchors (re-confirm at PLAN): client init `lib/analytics/posthog.ts:102`; global init `instrumentation-client.ts`; identify `components/analytics/AnalyticsBootstrap.tsx:10-13`; dead exports `ui/packages/website/src/analytics/posthog.ts`.
- **Launch relevance (Indy, Jun 10, 2026):** flagged as the **one launch-relevant** gap from the audit (the logging-discipline + error_code gaps are internal hygiene, separate). Without first-class events + server-side capture, launch funnels are measured on fragile autocapture DOM clicks and billing conversions are invisible. **Indy go: author this spec** (the other audit gaps stay as post-launch hygiene).
- **Deferrals** — session recording is **out of scope** (the cookie-less GDPR posture may be deliberate; a separate decision). Any other "deferred to follow-up" needs an Indy-acked verbatim quote here.
- **Skill chain outcomes** — {`/write-unit-test`, `/review`, `/review-pr` results.}

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage of the catalog/helpers + per-action capture assertions vs this Test Specification. | Clean. Iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs Invariants (esp. PII-in-props + single-sourcing), `dispatch/write_ts_adhere_bun.md`. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before human review/merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| App unit | `make test-unit-app` | {paste snippet} | |
| Dry lane | `make dry-app` | {paste snippet} | |
| Coverage | `make test-coverage-all` | {paste snippet} | |
| No bare literals | E1 grep | {paste snippet} | |

---

## Out of Scope

- **The backend analytics planes** — Prometheus metrics, the OTLP logs/traces export, the PostHog *zombied* server events (`telemetry.zig`), and the Postgres execution-telemetry store are unchanged; this is the *UI product-event* layer only.
- **Session recording** — deliberately out (cookie-less GDPR posture); a separate decision.
- **The logging-discipline + `error_code` audit gaps** — internal hygiene, separate (post-launch) specs.
- **New analytics infrastructure** (a warehouse, reverse-ETL, a CDP) — out; PostHog is the sink.
