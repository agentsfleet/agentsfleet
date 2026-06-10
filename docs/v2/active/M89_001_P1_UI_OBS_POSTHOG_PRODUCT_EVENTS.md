# M89_001: PostHog product-event coverage — dashboard first-class events (client-side activation funnel)

**Prototype:** v2.0.0
**Milestone:** M89
**Workstream:** 001
**Date:** Jun 10, 2026
**Status:** IN_PROGRESS
**Priority:** **P1 — launch-relevant.** PostHog is wired app-wide for autocapture + pageviews, but the dashboard's core product actions emit **zero first-class (client-side) events** — so at launch the **activation funnel** (zombie created → run → key minted → model added) would be measured off fragile autocapture Document-Object-Model (DOM) clicks. This closes that gap with single-sourced, typed **client-side** events. *(Server-side / conversion truth — billing, signup completion — is **already** captured by the zombied backend via `posthog-zig` (`telemetry.zig`: `ZombieTriggered`/`Completed`, `SignupBootstrapped`, `AuthLoginCompleted`, …); extending it is a separate backend workstream, NOT a `posthog-node` path in the web app.)*
**Categories:** UI, OBS
**Batch:** B1 — standalone; no backend (Zig) change, no schema. Pure UI/analytics instrumentation.
**Branch:** feat/m89-posthog-product-events
**Depends on:** none. PostHog client init + autocapture + `identify`-on-Clerk already ship; this layers first-class events on top.
**Provenance:** agent-surfaced in the **Jun 09, 2026 observability audit** (zombied/runner/UI). The UI sweep found global autocapture coverage but sparse manual events and no `posthog-node`. Re-confirm the cited `file:line` anchors at PLAN — they were read during the audit, not re-verified at authoring.

> **Provenance is load-bearing.** Every `file:line` below comes from the Jun 09 UI audit. Re-confirm at PLAN: `ui/packages/app/lib/analytics/posthog.ts:102` (client init), `ui/packages/app/instrumentation-client.ts` (global init), `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx:10-13` (identify on Clerk), `ui/packages/website/src/analytics/posthog.ts` (the dead `trackSignupCompleted` / `trackLeadCapture*` exports). **Re-confirmed at PLAN (Jun 10, 2026) — all anchors exact:** `posthog.init` at `posthog.ts:102`, `ALLOWED_PROP_KEYS` at `:33`/`:77`, `PostHogLike` (no `reset`) at `:26`, identify early-return at `AnalyticsBootstrap.tsx:11`, dead exports at `website/src/analytics/posthog.ts:172-212`.

**Canonical architecture:** the dashboard (`ui/packages/app`, Next.js App Router) + marketing site (`ui/packages/website`, Vite). PostHog is the product-analytics plane (distinct from Prometheus metrics, the OpenTelemetry Protocol (OTLP) export, and the Postgres execution-telemetry store — see the observability map). This workstream adds the *product-event* layer; it does not touch the metrics/OTLP/telemetry planes.

---

## Implementing agent — read these first

1. `ui/packages/app/lib/analytics/posthog.ts` — the client init (`posthog.init`, line ~102) + `identifyAnalyticsUser` (line ~112). The typed event catalog + client capture helpers land here (or a sibling `events.ts`).
2. `ui/packages/app/instrumentation-client.ts` — Next.js global client-instrumentation; where `initAnalytics()` + `onRouterTransitionStart` fire today. No `PostHogProvider` exists — init is global, not a React provider.
3. `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx` — `identify` on Clerk auth-state change (root layout). **`reset()` on logout lands here** (§4).
4. dashboard **client components** for the click-driven actions (verified at PLAN): `app/(dashboard)/zombies/new/InstallZombieForm.tsx`, `app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx`, `app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx`, `app/(dashboard)/settings/models/components/ProviderSelector.tsx`, `app/(dashboard)/credentials/components/AddCredentialForm.tsx`, `app/(dashboard)/approvals/[gateId]/ResolveButtons.tsx` — where the typed client `captureProductEvent(EVENTS.x, props)` fires after the user action (§2). *(Server-action files are NOT instrumented here — server-side events are zombied's `posthog-zig` domain.)*
5. `ui/packages/website/src/analytics/posthog.ts` — the marketing init + the **dead** `trackSignupCompleted` + `trackLeadCapture*` exports; §5 deletes them (PLAN-ratified inversion — see Discovery).
6. `dispatch/write_ts_adhere_bun.md` — all `*.ts`/`*.tsx` edits (const/import discipline, the UI + DESIGN-TOKEN gates). Analytics is mostly non-visual, but the gates still fire on `.tsx`.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m89): PostHog product-event coverage — dashboard first-class events (client-side)`
- **Intent (one sentence):** Every meaningful **user-driven** dashboard action (zombie created, runner token minted, API key minted, Bring-Your-Own-Key (BYOK) model added, credential added, approval resolved) emits a **single-sourced, typed PostHog event client-side** (`posthog-js`) — so the launch activation funnel is measured on reliable events, not autocapture DOM clicks. *(Server-completed conversions are captured separately by zombied's `posthog-zig`; `zombie_run` + checkout events have no client UI and were dropped at PLAN.)*
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. **Confirm the event taxonomy** (the exact event names + props per the catalog). All events here are **client-side** (`posthog-js`), fired in a browser component at the user action — there is **no** `posthog-node` / server-side path in this workstream. **Confirm no Personally-Identifiable-Information (PII) / secret lands in event props** (no tokens, API keys, raw credentials — IDs + names only).
- **Handshake fulfilled (Jun 10, 2026):** taxonomy confirmed at six events (see Interfaces); PII discipline confirmed — IDs/names/enums only, never `runner_token`, the raw API key, `data_json`, or the approval reason text; Indy greenlit with four code-reality amendments (see Discovery).

---

## Applicable Rules

- **`dispatch/write_ts_adhere_bun.md`** — `const`/import discipline; raw-HTML → design-system primitive (UI gate); `*-[...]` arbitrary → token utility (DESIGN TOKEN gate). The capture helpers are non-visual, but any touched `.tsx` still trips the gates.
- **UFS (manual, ui/)** — the UFS audit skips `ui/`; **by hand**, extract every event-name literal + prop shape into a single-sourced **as-const** catalog (`events.ts`), referenced everywhere — never re-spell `"zombie_created"` at a call site.
- **PII / secret discipline** (mirrors `docs/LOGGING_STANDARD.md` §6 in spirit) — event props carry IDs + names + enums only; **never** a token, API key, raw credential, or full secret. A reviewer audits every new `capture(...)` prop bag.
- **Client capture only** — every `capture` runs in a browser component after the user action; there is no server-action / `posthog-node` capture in this workstream (server events are zombied's `posthog-zig`).

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

**Goal (testable):** A defined set of **user-driven** product actions each emit a typed PostHog event **client-side** (`posthog-js`) with the documented props (no PII), single-sourced from an as-const catalog; `reset()` fires on logout; and the website analytics module carries no dead exports. Verified by unit tests on the catalog/helpers + capture-call assertions.

**Problem (Jun 09 audit):** PostHog coverage is **global on the baseline** — both `app` and `website` init app-wide with `autocapture: true` + auto pageviews + pageleave, env-gated — but **thin on first-class events**:
1. **Dashboard product actions emit ZERO explicit events.** Creating a zombie, running it, registering a runner, minting an API key, adding a BYOK model, checkout/billing — all are autocapture-only (DOM clicks), which is fragile (selector-dependent) and useless for clean funnels.
2. **Server-side conversions live in the backend (already captured).** zombied already emits server-truth events to PostHog via `posthog-zig` (`telemetry.zig`: `ZombieTriggered`/`Completed`, `SignupBootstrapped`, `AuthLoginCompleted`). So the UI gap is **only** the missing client-side first-class events (point 1) — **not** a `posthog-node` path. (Extending the backend's coverage — e.g. a billing-conversion event — is a separate Zig/`posthog-zig` workstream.)
3. **No `reset()` on logout.** Identity is set via `identify` on sign-in but never cleared → cross-session stitching risk on shared browsers.
4. **Website funnel is half-wired.** `trackSignupCompleted` + the four `trackLeadCapture*` events are defined/exported but **never called** — the marketing signup funnel has a start (`signup_started`) with no recorded completion.

**Solution summary:** Add a **single-sourced typed event catalog** + a thin **client** `captureProductEvent` helper; emit first-class events at the user-action sites (`posthog-js`); fire `reset()` on logout; and delete the structurally-unwireable dead website funnel exports (PLAN-ratified — see §5/Discovery). **Client-only — no `posthog-node`.** No backend change.

**Prioritization.** This is the **one launch-relevant** observability gap: without it, the launch is measured on autocapture DOM clicks with billing conversions invisible. The logging-discipline + error_code gaps from the same audit are internal hygiene (separate, post-launch).

---

## Prior-Art / Reference Implementations

- **Server-side already exists (`posthog-zig`)** — zombied captures server-truth events (`telemetry.zig`), so this workstream adds **no** Next.js `posthog-node` path; client events stitch to the same person via the existing `identify` (Clerk user id).
- **Typed event catalog (as-const)** — the in-repo pattern for single-sourcing literals in `ui/` (UFS-by-hand): one `events.ts` exporting `EVENTS` as-const + a `Props<E>` map, so call sites reference `EVENTS.zombie_created`, never the bare string.
- **`identify` / `reset` lifecycle** — PostHog's standard: `identify` on auth (already wired in `AnalyticsBootstrap`), `reset` on logout (the missing half) to prevent anonymous/next-user stitching.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/analytics/events.ts` | CREATE | The single-sourced as-const event catalog (names + per-event prop types). UFS-by-hand. |
| `ui/packages/app/lib/analytics/posthog.ts` | EDIT | Add typed client `captureProductEvent(event, props)` helper that consults the catalog. **It must NOT route catalog-event props through the existing `sanitizeProps` closed allowlist (`ALLOWED_PROP_KEYS`, `posthog.ts:33`/`:77`) — that would silently drop event-specific keys (`zombie_id`, `api_key_id`); the `EventProps` types are the compile-time PII guard instead (§1.3).** Also **extend the `PostHogLike` interface (`posthog.ts:26`) with `reset?: () => void`** and add `resetAnalyticsIdentity()` — it must null the module-cached `identifiedUserId` so a same-user re-login re-identifies (§4). |
| *(no new server file / dependency)* | — | **Client-only — `posthog-node` dropped (Indy, Jun 10).** Server-side events are zombied's `posthog-zig`; `ui/packages/app` keeps only `posthog-js`, no new dependency. |
| `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx` | EDIT | `posthog.reset()` on Clerk sign-out (§4). |
| the six dashboard client components listed in "read these first" item 4 | EDIT | Client `captureProductEvent(EVENTS.x, props)` at the verified success point (§2). |
| `ui/packages/website/src/analytics/posthog.ts` | EDIT | Delete the five dead exports (`trackSignupCompleted`, `trackLeadCapture*`) + their `EVENT_*` constants + union members (§5). `signup_started` + navigation stay. |
| website + app test files (`ui/packages/website/src/**/*.test.tsx`, `ui/packages/app/tests/`) | EDIT/CREATE | Drop the dead-export tests; add catalog/helper/per-action capture tests (Test Specification). |
| `docs/architecture/product_analytics.md` | CREATE (small) | The event taxonomy (naming convention + catalog index + the client-activation vs server-conversion split). |

> **Action set verified at PLAN (Jun 10, 2026):** the six components above. No "Run now" UI exists (trigger cards are read-only) and no checkout UI exists ("Purchase Credits" is a disabled placeholder) — `zombie_run` + `checkout_*` dropped from the catalog (see Discovery). All capture is **client-side**.

---

## Decomposition & alternatives

- **Chosen shape:** one workstream (B1) — catalog + client `capture` helper + call-site wiring + website funnel. It is **additive instrumentation** (no behaviour change; events are side-effects). Separable into the Sections below.
- **Alternatives considered:** (a) **Rely on autocapture alone** — rejected: DOM-selector-fragile, no clean funnels (the status quo this closes). (b) **A heavyweight analytics abstraction / event bus** — rejected: over-engineered; a typed catalog + a thin client helper is enough. (c) **Add a Next.js `posthog-node` server path** — rejected (Indy, Jun 10): server-side conversion truth already lives in zombied's `posthog-zig` (`telemetry.zig`); a second server path in the web app would duplicate it. This workstream is **client-only**.

---

## Sections (implementation slices)

### §1 — Single-sourced typed event catalog — DONE

One `events.ts` exports `EVENTS` (as-const, snake_case object-first past tense — `zombie_created`) + a per-event prop-type map. Every call site references the catalog; no bare event-name strings; no PII in any prop type (IDs/names/enums only).

- **Dimension 1.1 — DONE** — the catalog enumerates the launch event set and is the only source of event-name strings → Test `events catalog is the single source (no bare event-name literals at call sites)`
- **Dimension 1.2 — DONE** — no prop type admits a secret/token/raw-credential field (compile-time + a lint/test check) → Test `event props carry no PII/secret fields`
- **Dimension 1.3 (sanitizer interaction) — DONE** — the app's existing `sanitizeProps` drops any key not in the **closed** `ALLOWED_PROP_KEYS` set (`posthog.ts:33`,`:77`); routing the catalog's event-specific keys (`zombie_id`, `api_key_id`, …) through it would **silently emit empty prop bags**. The typed `captureProductEvent` therefore **bypasses `sanitizeProps` for catalog events** — the `EventProps` types are the compile-time guard, **and the emit path allowlists the payload against the `EVENT_PROP_KEYS` runtime mirror** (review hardening: excess-property checks only cover object literals, so a spread or widened argument must not smuggle extra keys). *(PLAN picked bypass; extending `ALLOWED_PROP_KEYS` was the rejected alternative.)* → Test `catalog event props survive the emit path (not dropped by ALLOWED_PROP_KEYS)` + Test `smuggled extra keys never reach the payload`

### §2 — Dashboard first-class events (client-side, user-driven) — DONE

At each click-driven action, the component calls the typed client `captureProductEvent(EVENTS.x, props)`. Covers the actions a user triggers in the browser (create-zombie submit, runner-token mint, key mint, BYOK model add, credential add, approval resolve). The approvals `already_resolved` race branch does **not** capture — that click did not resolve the gate.

- **Dimension 2.1 — DONE** — each click-driven action emits its catalog event with the documented props on success → Test per action (`captureProductEvent` called with `EVENTS.zombie_created` + props)
- **Dimension 2.2 — DONE** — capture is **not** fired on validation failure / aborted action (event = success signal) → Test (no capture on error path; no capture on approvals `already_resolved`)

### §3 — Server-side conversions — OUT OF SCOPE (zombied's `posthog-zig`)

Server-completed conversions (billing confirmed, signup completion, zombie run finished) are **not** captured here. They already flow from the zombied backend via `posthog-zig` (`telemetry.zig`: `ZombieTriggered`/`Completed`, `SignupBootstrapped`, `AuthLoginCompleted`, …), where the state-owning code lives and the event is authoritative (browser events get ad-blocked / lost on tab close). **No `posthog-node` path is added to the web app.** Extending the backend's server-event coverage (e.g. a dedicated billing-conversion event) is a separate Zig / `posthog-zig` workstream.

### §4 — Identity hygiene: `reset()` on logout — DONE

`AnalyticsBootstrap` (or the logout handler) calls `posthog.reset()` when Clerk transitions to signed-out, so a subsequent anonymous/other session does not stitch to the prior `distinct_id`.

- **Dimension 4.1 — DONE** — `posthog.reset()` is called **exactly once on the `isSignedIn: true → false` edge** — NOT on every signed-out render. The current `AnalyticsBootstrap` effect early-returns `if (!isLoaded || !isSignedIn || !userId)` (`AnalyticsBootstrap.tsx:11`); a naive `else { reset() }` would fire on every render while signed out. **Required pattern:** a `useRef(prevSignedIn)` (or a dedicated effect keyed on `isSignedIn`) that calls `reset()` only when the previous value was `true` and the current is `false`. → Test (`reset` fired once on the sign-in→sign-out transition; NOT fired on repeated signed-out renders)

Reset routes through `resetAnalyticsIdentity()` (`posthog.ts`), which **also nulls the module-cached `identifiedUserId`** — `identifyAnalyticsUser` caches it, so without clearing a same-user re-login would never re-identify. → Test (sign-out then sign-in re-identifies)

**Marker sweep (shipped — EXECUTE decision):** the localStorage identified-marker sweep — set on identify, cleared on reset; a signed-out mount with the marker present resets once — shipped unconditionally rather than as a hard-navigation contingency: session expiry and cross-tab sign-out also make the edge unobservable in-page, so the sweep is required regardless of Clerk's navigation mode. Guarded so anonymous visitors never churn anonymous ids. The shipped pattern is the spec's allowed alternative — a single effect keyed on `isSignedIn` with the staleness predicate (`hasStaleAnalyticsIdentity`) in the analytics module — same exactly-once edge semantics as the `useRef` sketch, plus hard-nav/expiry coverage.

### §5 — Website funnel: delete the dead exports (PLAN-ratified inversion) — DONE

The Jun 09 audit assumed the dead exports needed call sites. PLAN verification (Jun 10) showed they are **structurally unwireable**: the marketing funnel is redirect-based — the "get early access" Call-To-Action (CTA) links navigate to the app origin where Clerk completes signup, so the website can never observe completion (and the deliberate localStorage-only persistence does not cross subdomains). No lead-capture surface exists for the four `trackLeadCapture*` events. Conversion truth is zombied's `SignupBootstrapped` (`posthog-zig`). Per RULE NDC the five dead exports, their `EVENT_*` constants, union members, and dead-only tests are **deleted**. `signup_started` + `navigation_clicked` call sites are live and stay.

- **Dimension 5.1 — DONE** — `trackSignupCompleted` + `trackLeadCaptureClicked/Opened/Submitted/Failed`, their `EVENT_*` constants, and union members are removed; zero references remain → Test (website suite green after deletion) + grep (zero hits)
- **Dimension 5.2 — DONE** — every **remaining** `track*` export has ≥1 production call site → Test + grep

---

## Interfaces

> **Illustrative — exact catalog + signatures verified at PLAN.** Contract, not implementation.

```ts
// ui/packages/app/lib/analytics/events.ts — single-sourced (UFS-by-hand)
export const EVENTS = {
  zombie_created: "zombie_created",            // InstallZombieForm success
  runner_token_minted: "runner_token_minted",  // AddRunnerDialog success (token reveal; liveness derived later)
  api_key_minted: "api_key_minted",            // CreateApiKeyDialog success
  model_added: "model_added",                  // ProviderSelector (BYOK) success
  credential_added: "credential_added",        // AddCredentialForm success
  approval_resolved: "approval_resolved",      // ResolveButtons, kind === "resolved" only
} as const;
export type EventName = (typeof EVENTS)[keyof typeof EVENTS];
// Per-event props: IDs/names/enums ONLY — NEVER a token/key/secret.
export type EventProps = {
  [EVENTS.zombie_created]: { zombie_id: string };
  [EVENTS.runner_token_minted]: { runner_id: string; sandbox_tier: string }; // NEVER runner_token
  [EVENTS.api_key_minted]: { api_key_id: string };                           // id, NOT the key
  [EVENTS.model_added]: { provider: string; mode: string; model?: string };
  [EVENTS.credential_added]: { credential_name: string };                    // NEVER data_json
  [EVENTS.approval_resolved]: { gate_id: string; decision: string; has_reason: boolean }; // NEVER the reason text
};
// Runtime mirror locked to EventProps via `satisfies` — gives the PII test and
// the emit-path test runtime teeth (EventProps alone is erased at runtime).
export const EVENT_PROP_KEYS: Record<EventName, readonly string[]>;

// client capture (ui/packages/app/lib/analytics/posthog.ts)
// NOTE: bypasses the existing sanitizeProps/ALLOWED_PROP_KEYS allowlist for catalog events
// (those keys are not allowlisted → would be silently dropped); EventProps is the PII guard. §1.3
export function captureProductEvent<E extends EventName>(event: E, props: EventProps[E]): void;
// §4 — calls posthogClient.reset() AND nulls the cached identifiedUserId.
export function resetAnalyticsIdentity(): void;
// (No server capture here — server-side events are zombied's posthog-zig; §3.)
```

Contract: the product behaves identically; events are additive side-effects. No prop bag carries a secret. Without a PostHog key configured, client `capture` is a no-op (env-gated, as today).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| PII leak in a prop | a dev adds `{ token }` / `{ api_key }` to a prop bag | the `EventProps` types forbid secret fields; reviewer audits every new `capture`; Dim 1.2 test. |
| Event props silently dropped | the typed `capture` routes catalog props through `sanitizeProps`' closed `ALLOWED_PROP_KEYS` | **every event emits an empty prop bag, silently** — the catalog keys aren't allowlisted. Closed by §1.3: catalog `capture` bypasses `sanitizeProps` (types are the PII guard), or `ALLOWED_PROP_KEYS` is extended first. Caught by the §1.3 test. |
| Event name drift | a call site re-spells `"zombie_created"` | only `EVENTS.x` is allowed; Dim 1.1 test + grep for bare event literals. |
| No PostHog key (dev/preview) | env unset | capture is a no-op (env-gated); product unaffected. |
| Double-count (autocapture + first-class) | autocapture click + explicit event for the same action | acceptable — first-class events are the funnel source of truth; PLAN notes the overlap, optionally excludes the action's element from autocapture (`ph-no-capture`) only if it pollutes a funnel. |

---

## Invariants

1. **Single-sourced events** — every emitted event name comes from the `EVENTS` catalog; no bare event-name literal at any call site. Enforced by Dim 1.1.
2. **No PII/secret in props** — no event prop carries a token, API key, raw credential, or full secret; IDs/names/enums only. Enforced by `EventProps` types + Dim 1.2 + reviewer audit.
3. **Identity cleared on logout** — `reset()` fires **once, on the `isSignedIn: true → false` edge** (not every signed-out render). Enforced by Dim 4.1.
4. **No dead funnel exports** — every `track*` export in the website analytics module has ≥1 call site. Enforced by Dim 5.2 + grep.
5. **Env-gated no-op** — without a PostHog key, capture is inert; the product is unchanged. (Existing behaviour, preserved.)

---

## Test Specification (tiered)

> **Lane:** UI unit + dry lanes — `make test-unit-app` (Vitest) for the catalog/helpers + capture assertions, `make dry-app` (Vitest + Playwright page renders, no Clerk) for render-safety. No backend lane.

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `events catalog single source` | call sites reference `EVENTS.*`; grep finds no bare event-name literals |
| 1.2 | unit | `event props carry no PII` | `EventProps` admits no `token`/`api_key`/`secret`/`password` field; a sample capture has only IDs/names |
| 1.3 | unit | `catalog props survive the emit path` | a catalog capture with `{ zombie_id }` (and a multi-key bag) emits those keys (NOT dropped by `ALLOWED_PROP_KEYS`) |
| 2.1 | unit | `client capture per action` | each click-driven action calls `captureProductEvent(EVENTS.x, props)` on success |
| 2.2 | unit | `no capture on error path` | a failed/aborted action does NOT capture; approvals `already_resolved` does NOT capture |
| 4.1 | unit | `reset on logout edge` | `reset()` fires once on a signed-out render with lingering identity (via the `hasStaleAnalyticsIdentity` predicate — module cache + localStorage marker); NOT fired on repeated signed-out renders; a subsequent sign-in re-identifies; a sweep racing the posthog-js chunk load defers without burning the marker |
| 5.1 | unit | `dead exports deleted` | website suite green after deletion; grep finds zero references to the deleted exports |
| 5.2 | unit + grep | `no dead funnel exports` | every remaining `track*` export has a call site |

- **Regression:** existing app + website suites pass; autocapture/pageview behaviour unchanged.
- **Branch coverage:** feed success AND error inputs to each action so the "capture-on-success-only" branch is exercised (per the branch-coverage discipline).

---

## Acceptance Criteria

- [ ] `events.ts` catalog (as-const) is the single source of event names; no bare event-name literals at call sites — verify: Dim 1.1 test + grep
- [ ] No event prop carries a token/API key/raw credential/secret — verify: Dim 1.2 + `EventProps` types
- [ ] Each launch dashboard action emits its first-class **client** event on success, not on failure — verify: §2 tests
- [ ] `reset()` fires on logout — verify: Dim 4.1
- [ ] Website dead funnel exports deleted (`trackSignupCompleted` + `trackLeadCapture*`); every remaining `track*` export live — verify: Dim 5.1 + 5.2
- [ ] `make test-unit-app` + `make dry-app` green · aggregate coverage gate met
- [ ] Event taxonomy documented (naming convention + catalog index)
- [ ] No PostHog key → capture is a no-op (env-gated, unchanged)

---

## Eval Commands (post-implementation)

```bash
# E1: no bare event-name literals outside the catalog
grep -rnE '"(zombie_created|runner_token_minted|api_key_minted|model_added|credential_added|approval_resolved)"' ui/packages/app --include='*.ts*' | grep -v 'lib/analytics/events.ts'
# E2: posthog-node is NOT added to the web app (server-side is zombied's posthog-zig)
git grep -n 'posthog-node' ui/packages/app/package.json && echo "FAIL: posthog-node should not be in ui/app" || echo "PASS (client-only)"
# E3: reset on logout wired
grep -rn 'posthog.reset' ui/packages/app
# E4: deleted funnel exports stay deleted (expect zero hits)
grep -rnE 'trackSignupCompleted|trackLeadCapture' ui/packages/website/src && echo "FAIL: deleted exports referenced" || echo "PASS (deleted)"
# E5: unit + dry lanes
make test-unit-app 2>&1 | tail -5 && make dry-app 2>&1 | tail -5
```

---

## Dead Code Sweep

**1. Orphaned files — none expected** (additive in the app; the website deletions are in-file export removals).

**2. Orphaned references.** §5 deletes the dead `trackSignupCompleted` / `trackLeadCapture*` exports + their constants and dead-only tests; E4 must show **zero** references. Every remaining `track*` export keeps ≥1 caller (Dim 5.2).

---

## Discovery (consult log)

- **Origin (Jun 09, 2026):** the observability audit (zombied/runner/UI). UI sweep verdict: "Coverage is global on the baseline, thin on explicit events." Both packages init PostHog app-wide (`autocapture:true` + pageviews + pageleave, env-gated); no route group is outside the client. Gaps: dashboard product actions emit zero first-class events; no `posthog-node` server-side (9 `actions.ts` + the SSE route handler silent); no `reset()` on logout; website `signup_completed`/`lead_capture_*` defined-but-never-called.
  - Cited anchors (re-confirm at PLAN): client init `lib/analytics/posthog.ts:102`; global init `instrumentation-client.ts`; identify `components/analytics/AnalyticsBootstrap.tsx:10-13`; dead exports `ui/packages/website/src/analytics/posthog.ts`.
- **PLAN verification + greenlight (Indy, Jun 10, 2026):** all provenance anchors re-confirmed exact. Four code-reality amendments ratified with the greenlight: (1) **`zombie_run` dropped** — no "Run now" UI exists (trigger cards are read-only; the funnel's run leg is zombied's `ZombieTriggered`); (2) **`checkout_started`/`checkout_completed` dropped** — no checkout UI exists ("Purchase Credits" is a disabled contact-support placeholder; billing conversion stays a `posthog-zig` follow-up); (3) **`runner_registered` → `runner_token_minted`** — the dialog mints a one-time `zrn_` token; the runner goes live only when the host calls in (honest-liveness naming, consistent with M84_001); (4) **§5 inverted to deletion** — the website funnel is redirect-based (completion happens on the app origin under Clerk; localStorage-only persistence does not cross subdomains), so the dead exports are structurally unwireable; conversion truth = `SignupBootstrapped`. **Indy greenlight (verbatim):** _"yes greenlgith"_ — full lifecycle authorized (commit/push/PR).
- **Launch relevance + client-only scope (Indy, Jun 10, 2026):** flagged as the **one launch-relevant** gap from the audit. Scoped to **client-side activation-funnel events only** — `posthog-node` / server-side capture is **dropped**: server-truth conversions (billing, signup completion) already flow from zombied via `posthog-zig` (`telemetry.zig`: `ZombieTriggered`/`Completed`, `SignupBootstrapped`, `AuthLoginCompleted`), so a Next.js server path would duplicate the backend. **Indy decision (verbatim):** _"client-only for activation funnel only. drop the posthog-node now"_ — server-side coverage extends via `posthog-zig` later, a separate backend workstream.
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
