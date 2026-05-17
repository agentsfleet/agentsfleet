# M71_001: CLI Login Resilience and UX Polish

**Prototype:** v2.0.0
**Milestone:** M71
**Workstream:** 001
**Date:** May 17, 2026
**Status:** PENDING
**Priority:** P2 — production login already works; this hardens the poll loop, error taxonomy, and UX prose for operators on flaky networks. Not blocking shipping the M68 trigger DX surface. Sections §6–§11 carry the deferred dashboard / website trigger DX work from M68's post-close amendment (May 17, 2026); those are independent of §1–§5 (the CLI-login work) and can ship in either order.
**Categories:** CLI, UI, WEBSITE
**Batch:** B1
**Branch:** feat/m71-001-cli-login-resilience (to be created at CHORE(open))
**Depends on:** M68_001 (DONE) — §13 D27 decomposition landed the named-stage skeleton this spec's §1–§5 extends; §6–§11 inherit the unfinished M68 §D / §E / §G surface (see "Deferred from M68" below).
**Provenance:** agent-generated. §1–§5 deferred from `docs/v2/done/M68_001*.md` §13 during CHORE(close) on May 17, 2026; §6–§11 deferred from the same spec during the `/write-unit-test` Path-C audit on May 17, 2026 (see [M68 Post-Close Amendments](../done/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md#post-close-amendments)).

**Canonical architecture:** `docs/ARCHITECTURE.md` §CLI — zombiectl login flow (Clerk JWT path; OAuth-poll handshake).

---

## Implementing agent — read these first

1. `zombiectl/src/commands/core.ts` — the canonical home for every dimension in this spec. M68_001 §13 D27 split `commandLogin` into named stages (`resolvePollParams`, `createLoginSession`, `announceLoginSession`, `maybeOpenBrowser`, `pollUntilComplete`, `persistAndHydrate`, `emitLoginResult`); each dimension below grafts onto one of those stages. The orchestrator's external signature `commandLogin(ctx, parsed, workspaces, deps)` is locked — do not change it.
2. `zombiectl/src/lib/error-map-presets.ts` — `AUTH_PRESET` is the existing error remap table. D28 tightens this, doesn't replace it. Mirror the per-key entry shape already in place.
3. `zombiectl/test/login.unit.test.ts` (post-D42 migration) — the pre-existing exit-code + stdout-shape contracts. The plural-flagged contracts (lines 145–172 and 174–204 in the legacy JS form) explicitly pin `exit 0` when hydration fails; D23 must preserve that.
4. `docs/v2/done/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md` §13 — the parent spec's "Deferred to follow-up" list names each dimension and what changed. Treat that prose as the contract this spec inherits.
5. `zombiectl/src/lib/http.ts` + `zombiectl/src/program/http-client.ts` — `RetryConfig` shape, attempt-event callback contract, and the existing exp-backoff helper for non-login HTTP. D29's poll-loop backoff should mirror this pattern, not reinvent it.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (RULE NDC, RULE NSQ, RULE UFS, RULE TST-NAM).
- **`zombiectl/CLAUDE.md`** (if present) — package-local conventions; otherwise the global TS-strict migration intent in `~/.claude/CLAUDE.md` applies (no `as any`, `!`, or `@ts-expect-error` to silence strictness).
- **TS strict settings already enforced via `zombiectl/tsconfig.json`** — `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `useUnknownInCatchVariables`, `strict: true`. Every dimension below must compile under these unchanged.
- `docs/ZIG_RULES.md` and `docs/SCHEMA_CONVENTIONS.md` — N/A; this spec is TS-only.
- `docs/REST_API_DESIGN_GUIDELINES.md` — N/A; this spec is a CLI-side concern; the server-side login session endpoints stay frozen for this milestone.

---

## Overview

**Goal (testable):** `zombiectl login` survives transient network conditions (single 503/blip per poll cycle), gives operators a live countdown of session expiry, surfaces workspace-hydration failures on stderr, and maps every recoverable poll error to a distinct `AUTH_PRESET` code — without breaking the existing exit-code contract pinned in `test/login.unit.test.ts`.

**Problem:** M68_001 §13 D27 decomposed `commandLogin` into named stages but left five behaviors stubbed or absent. Operators on flaky networks see opaque "session expired" errors, silent workspace-hydration failures (post-login state never gets populated, next command 401s with no breadcrumb), and a poll loop that gives up on the first 503. The browser-handoff window has no visible countdown — operators alt-tab to the browser, miss a notification, then come back to a CLI that's already timed out.

**Solution summary:** Extend the existing `pollUntilComplete` and `persistAndHydrate` stages with five focused dimensions, each carrying its own test surface. No new top-level command; no new server-side endpoint; no schema change. The orchestrator's signature stays locked. Net new prod code is ~150 LOC across `src/commands/core.ts` + a thin extension of `src/lib/error-map-presets.ts`. Spec is sized to ship as a single PR.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `zombiectl/src/commands/core.ts` | EDIT | All five dimensions land here. The named stages from §13 D27 are the grafting points. |
| `zombiectl/src/lib/error-map-presets.ts` | EDIT | D28 tightens `AUTH_PRESET` with the new code → message mappings. No new exported preset, just additional entries on the existing one. |
| `zombiectl/test/login.unit.test.ts` | EDIT | Pre-existing exit-code/stdout-shape contracts unchanged; this spec adds rows. Specifically: countdown-tick assertions for D22, stderr-warning assertions for D23, per-code mapping for D28, backoff-delay calls for D29, single-blip survival for D30. |
| `zombiectl/test/login.acceptance.spec.ts` *(if it exists post-D42 acceptance migration)* | EDIT | One acceptance case: the full poll loop survives a single injected 503 and produces a successful login. Mirrors the §13 D31 acceptance pattern. |

> **Anti-pattern guard:** every other file in `zombiectl/src/` stays untouched. If a dimension demands cross-file work, it doesn't belong here — surface to spec author before reaching for new surface.

The CLI section (§1–§5) is bounded to the four rows above. The deferred-from-M68 surface (§6–§11) adds the following files; their full design lives in those sections:

| File | Action | § | Why |
|------|--------|---|-----|
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | EDIT | §6 | Tabs UI → per-trigger card list. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.test.ts` | EDIT or NEW | §6 | Multi-card variant rows. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.ts` | NEW | §7 | Per-provider data table (six or seven entries). |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.test.ts` | NEW | §7 | Per-provider snapshot tests. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/GuidedTriggerCard.tsx` | NEW | §8 | State-B (known provider) card. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/CronCard.tsx` | NEW | §9 | Read-only cron card. |
| `ui/packages/website/src/components/OnboardingFlow.tsx` | NEW | §10 | 4-step pictorial. |
| `ui/packages/website/src/components/OnboardingFlow.test.tsx` | NEW | §10 | Snapshot. |
| `ui/packages/website/src/pages/Home.tsx` | EDIT | §10 | Mount OnboardingFlow (disposition a or b). |
| `ui/packages/website/src/components/FeatureFlow.tsx` + `FeatureFlow.test.tsx` | DELETE (disposition a only) | §10 | Replaced by OnboardingFlow. |
| `ui/packages/website/src/components/Hero.tsx` | EDIT | §11 | Primary CTA redesign — clipboard + toast + smooth-scroll to `#onboarding-flow`. |
| `ui/packages/website/src/components/Hero.test.tsx` | EDIT | §11 | New CTA assertions. |

---

## Sections (implementation slices)

### §1 — D22: Session-expiry countdown (UX)

The browser-handoff window today shows a silent spinner during `pollUntilComplete`. Replace with a per-tick line: `Session expires in MM:SS — open the link in your browser…` updating on every poll. Deadline is computed client-side once at session-creation (`Date.now() + timeoutSec * 1000`); the response shape today carries no `expires_at_ms` from the server (the eventual cli-auth handshake hardening spec will add server-supplied deadlines; until then client-derived is the source). When `< 10s` remain, switch the prose to `Session expires in 0:0X — finish login soon` so operators get a visible nudge before timeout. Hidden in `--no-input` mode — that path stays mute.

**Implementation default:** mutate the in-place spinner line via the existing `stream.write("\r" + text)` pattern that `announceLoginSession` already uses. Don't reach for a new TTY library.

### §2 — D23: Fail-loud workspace hydration

`persistAndHydrate` today silences `hydrateWorkspacesAfterLogin` failures with `catch { return null; }`. That's correct for the exit code (login succeeded; hydration is best-effort), but the operator sees nothing on stderr — the next `zombiectl workspace list` 401s and looks like a broken login. Emit a single-line stderr warning: `warn: post-login workspace hydration failed (<err.code or "network">) — run "zombiectl workspace list" to retry`. Exit code remains 0; the unit-test pin in `test/login.unit.test.ts` (the "hydration fails but login still succeeds" rows) explicitly asserts exit 0 — that pin is binding.

**Implementation default:** narrow the catch to `unknown` per `useUnknownInCatchVariables`; type-guard via `instanceof ApiError` to extract `.code`; fall through to `"network"` literal otherwise.

### §3 — D28: Per-error-code AUTH_PRESET tightening

Today `AUTH_PRESET` maps a generic "auth failed" bucket. Tighten it to surface six distinct reasons during the poll loop:

| Internal trigger | Public code | User prose |
|---|---|---|
| Session row not in server's auth_sessions store | `InvalidSession` | "Login session not recognized — start over with `zombiectl login`." |
| Server returned 410 / past `expires_at` | `ExpiredSession` | "Login session expired. Start over with `zombiectl login`." |
| Fetch errored with `ECONNREFUSED`/`ENOTFOUND`/`ETIMEDOUT` | `NetworkError` | "Can't reach the server. Check connection and retry." |
| Server returned 429 | `RateLimited` | "Server rate-limited the poll loop. Backing off — this is transient." |
| Client-side `timeoutSec` exhausted | `Timeout` | "Login took too long. Start over with `zombiectl login`." |
| SIGINT during poll | `Interrupted` | "Login cancelled." |

These remap conditions zombiectl already encounters — no new error pathways, no new server contract. The taxonomy is what `--json` callers and the acceptance suite assert on.

**Implementation default:** keep the existing `AUTH_PRESET` export name and shape; this is six new keys, not a new preset. Conditions that don't match any of these continue to surface as the generic auth fallback (preserves backwards compat for unknown shapes).

### §4 — D29: Exponential-backoff polling with jitter

The poll loop today runs at a fixed `pollMs` cadence (default 2s, settable via `--poll-ms`). Switch to exp-backoff: start at the configured `pollMs` (default 1s), grow ×1.5 per attempt up to 5s cap, add ±20% jitter per tick. Honor server `Retry-After` (seconds or HTTP-date) if present in the 429 response — that beats the local backoff.

This caps polling RPS during retry storms (cli backlog of N operators all polling at 2s is the worst case; jittered exp-backoff smears it). It also doesn't change the happy-path latency meaningfully — the first attempt fires immediately, second at 1s+jitter, by which point most logins are already complete.

**Implementation default:** mirror the existing `backoffDelay()` helper in `src/lib/http.ts` rather than rolling a second one. RULE UFS — one named exp-backoff helper per package.

### §5 — D30: Transient-retry inside the poll loop

A single 503 or network blip mid-poll today kills the entire login. Treat one (1) transient failure per poll loop as recoverable: log it via the existing `attempt`-event callback (already routed to PostHog via `trackHttpRequest`/`trackHttpRetry` from `src/lib/analytics.ts`) and continue. A second transient in the same loop counts as `NetworkError` and propagates (the D28 path takes over).

The 1-blip budget is intentionally conservative — bigger budgets mask real outages. The acceptance test makes this contract visible: inject one 503, the login completes; inject two, the login surfaces `NetworkError`.

**Implementation default:** carry `transientCount: number = 0` in the `pollUntilComplete` local state. The same `RetryConfig`-style contract used by HTTP client retries is the wrong shape here — this is a much smaller, login-specific budget. Inline it.

---

### §6 — Deferred from M68 §D / §E1 / §F4: Trigger panel multi-card switch

**Provenance:** M68_001 §D narrative (line 73) + §E1 (line 128) + §F4 (line 141). Spec intent preserved verbatim below; implementing agent has the design ready.

**What M68 said the trigger panel should do:** "`TriggerPanel.tsx` renders one card per declared trigger in `zombie.triggers[]`. Card variants: `GuidedTriggerCard` (known webhook provider; pre-renders terminal registration command), `CopyUrlCard` (unknown source; today's behaviour as fallback), `CronCard` (schedule + next fire), `ApiCard` (catch-all `POST /v1/zombies/{id}/events` ingress)." `type: api` was carved out (§E5 / Out of Scope); `ApiCard.tsx` is **not** in scope for this spec either — it lands with the workspace-API-tokens spec. The four in-scope variants for M71_001 are `GuidedTriggerCard`, `CopyUrlCard` (already conceptually in the shipped Tabs UI as the default Webhook tab), `CronCard`, and the per-trigger loop in `TriggerPanel.tsx` itself.

**What shipped in M68:** a 78-line 2-tab UI (Webhook tab with one URL + Copy button; Schedule tab with "Cron scheduling is CLI-only for V1" placeholder). Tested at `ui/packages/app/tests/zombies.test.ts:690` (`describe("TriggerPanel interactions")` with three rows — copy semantics + cron-placeholder visibility). Those existing tests stay green after this section lands (the Tabs UI either remains as the "no triggers declared" fallback or its assertions move to assert the new per-card layout — implementing agent decides at design time).

**What this section delivers:**

| Sub-dim | File | Action | Why |
|---|---|---|---|
| 6.1 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | EDIT | Switch from `<Tabs defaultValue="webhook">…</Tabs>` to `<>{zombie.triggers.map(t => <Card variant={t.type, t.source} t={t} />)}</>`. Footer prose: "Edit `TRIGGER.md` and reinstall to change triggers — the source markdown is the source of truth." Prop signature changes from `{ zombieId: string }` to `{ zombieId: string; triggers: ZombieTrigger[] }` (parent page already has `zombie.triggers` from the M68 list-projection change). |
| 6.2 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.test.ts` | EDIT (move from `tests/zombies.test.ts:690` *or* extend in place) | Multi-card-variant rows: (a) 3-trigger zombie → 3 cards in order; (b) `source: "weirdco"` → falls back to `CopyUrlCard`; (c) last-delivery line populates from `listZombieEvents(actor_prefix, limit:1)`. Preserve the existing Tabs-UI test assertions if those code paths remain. |

**Acceptance:** an authenticated user installs a zombie with `triggers: [{type: webhook, source: github, events: ["push"]}, {type: cron, schedule: "*/15 * * * *"}]` → `/zombies/{id}` renders TriggerPanel with exactly two cards in order: a `GuidedTriggerCard` for the github webhook (uses §8) and a `CronCard` for the cron (uses §9).

### §7 — Deferred from M68 §E2 / §F3: `provider-guidance.ts` data table + tests

**Provenance:** M68_001 §E2 (line 129) + §F3 (line 140) + §"`provider-guidance.ts` schema" (line 371). Verbatim design carried forward; M71 implementer ships the table.

**What M68 said:** "Static `PROVIDER_GUIDANCE: Record<Source, GuidanceCard>` map. Entries for `github`, `linear`, `jira`, `grafana`, `slack`, `agentmail`. Each defines: title, events-label formatter, terminal-command template, web-User-Interface deep-link template, user-input variable list (e.g. `OWNER/REPO`, `TEAM_ID`, `WORKSPACE`)." Note: M68 also planned a `clerk` entry as a deep-link-only variant (line 371) — that brings the count to seven providers if the M71 implementer chooses to include it; minimum six per the §E2 row.

**Schema** (TypeScript — copy verbatim from M68 §371 onward when implementing):

```typescript
type Source = "github" | "linear" | "jira" | "grafana" | "slack" | "agentmail" | "clerk";

type GuidanceCard = {
  title: string;
  eventsLabel: (events: string[]) => string;        // e.g. ["push","pull_request"] → "On push, pull_request"
  command: (vars: Record<string, string>, webhookUrl: string) => string;
  webUiDeepLink: (vars: Record<string, string>) => string;
  variables: Array<{ name: string; example: string; required: boolean }>;
};

export const PROVIDER_GUIDANCE: Record<Source, GuidanceCard>;
```

**Per-provider verbatim content** lives in M68 §"`provider-guidance.ts` schema" (line 371 onward). Implementer copies that block into the new file.

**Files:**

| Sub-dim | File | Action | Why |
|---|---|---|---|
| 7.1 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.ts` | NEW | The data table. RULE FLL — if it crosses 350 lines, split per provider into `provider-guidance/{github,linear,jira,grafana,slack,agentmail,clerk}.ts` with a `mod.ts` aggregator (M68's note at §line 315 stays binding). |
| 7.2 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.test.ts` | NEW | Per-provider snapshot test: given `triggers[0] = {source, events}` + webhook URL, the rendered command + deep-link strings match a fixture. Pins prose. Six (or seven) fixture files alongside. |

**Acceptance:** `PROVIDER_GUIDANCE.github` rendering a `gh api repos/OWNER/REPO/hooks` command with the M68 webhook URL substituted matches the fixture byte-for-byte.

### §8 — Deferred from M68 §E3: `GuidedTriggerCard.tsx`

**Provenance:** M68_001 §E3 (line 130).

**What M68 said:** "Renders State B (known provider). Composes events label, webhook URL with Copy button, rendered command block with Copy button, web-UI deep link, last-delivery line. Pure presentational."

**File:** `ui/packages/app/app/(dashboard)/zombies/[id]/components/GuidedTriggerCard.tsx` — NEW.

**Props (suggested):**

```typescript
type Props = {
  trigger: ZombieTrigger;          // type === "webhook"
  webhookUrl: string;
  guidance: GuidanceCard;          // PROVIDER_GUIDANCE[trigger.source]
  lastDeliveryAt?: number | null;  // from listZombieEvents(actor_prefix, limit:1)
};
```

**Composition** (top-to-bottom in the card):

1. Header: `guidance.title` + `guidance.eventsLabel(trigger.events ?? [])`.
2. Webhook URL row: copyable code block (use the M68 shipped TriggerPanel's copy-button pattern — `useState<boolean>` + `navigator.clipboard.writeText` + 1.5s reset).
3. Rendered command block: variable inputs above (one per `guidance.variables`), the rendered `guidance.command(vars, webhookUrl)` below, Copy button. The command re-renders client-side as the user types into the variable inputs.
4. Web-UI deep link: `<a href={guidance.webUiDeepLink(vars)} target="_blank" rel="noreferrer">` with the provider name.
5. Last-delivery line: `Last delivery: <relative-time>` if `lastDeliveryAt`; otherwise `Last delivery: never`.

**Pure presentational** — no data fetching inside; the parent (`TriggerPanel.tsx`) passes `webhookUrl` + `guidance` + `lastDeliveryAt` down.

### §9 — Deferred from M68 §E4: `CronCard.tsx`

**Provenance:** M68_001 §E4 (line 131).

**What M68 said:** "Renders cron triggers. Shows the schedule, next-fire computed client-side (timezone-aware), and links the user to Recent Activity filtered `actor LIKE 'cron:%'`. ~50 lines."

**File:** `ui/packages/app/app/(dashboard)/zombies/[id]/components/CronCard.tsx` — NEW.

**Props:** `{ trigger: ZombieTrigger /* type === "cron" */; zombieId: string }`.

**Composition:**

1. Header: `Cron — ${trigger.schedule}` (the raw cron expression).
2. Next-fire line: computed client-side from the cron expression + `Date.now()` + IANA tz from `Intl.DateTimeFormat().resolvedOptions().timeZone`. Implementer picks a lightweight cron-parsing dep (`cron-parser` is ~9 kB minified; check bundle budget against M68's 220 kB asserted ceiling).
3. "Cron is read-only in the Dashboard" prose: "Declared in TRIGGER.md, runtime-managed by NullClaw's `cron_add` tool. Edit `TRIGGER.md` and reinstall to change the schedule." Mirrors the Out-of-Scope note from M68 line 993.
4. Recent-activity filter link: `<Link href={\`/zombies/${zombieId}?actor_prefix=cron:\`}>View cron deliveries →</Link>`.

### §10 — Deferred from M68 §G5 / §G6 / §G7: website `OnboardingFlow`

**Provenance:** M68_001 §G5 (line 153) + §G6 (line 154) + §G7 (line 155) + §"`OnboardingFlow.tsx` design" (line 398).

**Coexistence with `FeatureFlow.tsx`** (M71-specific decision the implementer makes at PLAN time):

`FeatureFlow.tsx` shipped at M68 in the slot OnboardingFlow was supposed to fill (`Home.tsx:40`). FeatureFlow is a 3-row alternating evidence layout (install / event-trace / mission-control); OnboardingFlow as spec'd is a 4-card horizontally-laid pictorial step-by-step (install / run skill / wire webhook / steer). Two valid M71 dispositions:

- **(a) Replace** `FeatureFlow` with `OnboardingFlow` on `Home.tsx` (the original M68 intent); delete `FeatureFlow.tsx` and its tests; reroute any other call sites of `FeatureFlow` to OnboardingFlow.
- **(b) Coexist** — keep `FeatureFlow` as the evidence section, mount `OnboardingFlow` either above it (between Hero and FeatureFlow) or below Pricing per M68's original placement. Two distinct sections with different user goals (evidence-of-product vs step-by-step-getting-started).

Disposition (a) is closer to the M68 design intent; disposition (b) preserves the post-M68 shipped state and adds the missing pictorial. Implementer picks at PLAN with `plan-ceo-review` or `plan-design-review` input; default is (a).

**File spec (carried forward from M68 §line 398):** "`OnboardingFlow.tsx` renders four horizontally-laid cards on desktop, stacked on mobile. Each card carries: an icon, a 1-line label, a code snippet (real shell command — `npm install -g @usezombie/zombiectl`, `npx skills add usezombie/usezombie`, `gh api repos/OWNER/REPO/hooks -F …`, `zombiectl steer zom_… 'howdy'`), and a sub-caption (≤2 lines explaining when the user runs it). ~180 LOC, no images — typography + design-system tokens only."

The four cards (verbatim from M68 §G5):

1. `npm install -g @usezombie/zombiectl` + `npx skills add usezombie/usezombie`.
2. Run `/usezombie-install-platform-ops` in Claude (or paste `TRIGGER.md` + `SKILL.md` in the Dashboard).
3. Wire the webhook (`gh api` one-liner pre-rendered; or copy the command from the Dashboard).
4. Steer the zombie ("howdy" from terminal `zombiectl steer` or from the Dashboard chat composer).

**Files:**

| Sub-dim | File | Action |
|---|---|---|
| 10.1 | `ui/packages/website/src/components/OnboardingFlow.tsx` | NEW (~180 LOC). |
| 10.2 | `ui/packages/website/src/components/OnboardingFlow.test.tsx` | NEW — snapshot test for the four cards; deterministic rendering. Asserts (a) four cards rendered in numbered order, (b) each card contains the expected code snippet text. |
| 10.3 | `ui/packages/website/src/pages/Home.tsx` | EDIT — mount `<OnboardingFlow />` per the chosen disposition (a) or (b). |
| 10.4 | `ui/packages/website/src/components/FeatureFlow.tsx` + `FeatureFlow.test.tsx` (if disposition (a)) | DELETE — only if FeatureFlow is fully replaced; carry FeatureFlow's existing tests into the OnboardingFlow test surface where they overlap (the install-command card is in both). |

**Anchor:** the section's outer container gets `id="onboarding-flow"` so M68 §G11's smooth-scroll target works after §11 lands.

### §11 — Deferred from M68 §G11: `Hero.tsx` primary-CTA redesign

**Provenance:** M68_001 §G11 (line 159).

**What M68 said:** Replace the `<a href={DOCS_QUICKSTART_URL}>` "→ install in Claude Code" button with a `<button>` whose onClick (a) writes `npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie` to `navigator.clipboard`, (b) shows a 2-second "Copied — paste into your terminal" toast (existing design-system `<Toast>` or an `aria-live` region fallback), (c) smooth-scrolls to the `#onboarding-flow` anchor on the same page. Keep `DOCS_QUICKSTART_URL` as a small tertiary "read the full quickstart →" link inside OnboardingFlow itself (§10). Update `Hero.test.tsx:52-56` accordingly.

**Depends on §10** — the `#onboarding-flow` anchor must exist before the scroll target makes sense. Land §10 first or in the same PR.

**File:** `ui/packages/website/src/components/Hero.tsx` — EDIT lines around 64–70 (per M68's pinned range; verify line numbers at PLAN time since intervening edits may have shifted them).

**Tests:** `Hero.test.tsx:52-56` — assert (a) clicking the CTA writes the install command to a mocked clipboard, (b) the toast appears for ~2s then disappears, (c) `scrollIntoView` is called on the `#onboarding-flow` element.

---

## Interfaces

```
External signature — LOCKED, do not change:
  commandLogin(ctx: CommandCtx, parsed: ParsedArgs, workspaces: Workspaces, deps: CommandDeps): Promise<number>

Internal stage signatures — see §13 D27 in M68_001; this spec inherits them verbatim. New code grafts onto:
  - pollUntilComplete(session, deps, signal) — gains the transient budget + backoff
  - persistAndHydrate(token, workspaces, deps) — gains the stderr warning emit
  - emitLoginResult(result, ctx) — unchanged (already handles the success/failure branch)

JSON mode (`--json`) error envelope — UNCHANGED in shape, NEW codes:
  { error: { code: "InvalidSession" | "ExpiredSession" | "NetworkError" |
                   "RateLimited" | "Timeout" | "Interrupted",
             message: "..." } }
```

No new HTTP endpoints. No new flags. No new env vars.

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Server returns 503 once during poll | Transient server / network blip | Log via analytics `attempt` callback, increment local transient counter, continue. |
| Server returns 503 twice in same poll | Sustained outage | Propagate as `NetworkError`; exit 1; D28 prose. |
| Server returns 429 with `Retry-After: N` | Rate limit | Sleep N seconds (capped at 30s); next attempt counts as fresh; D28 prose `RateLimited`. |
| Server returns 410 / 404 mid-poll | Session expired or deleted | Surface as `ExpiredSession` / `InvalidSession` respectively; exit 1. |
| `fetch` throws `TypeError: fetch failed` | DNS / TLS / refused | `NetworkError`; exit 1 unless first-blip budget covers it. |
| SIGINT mid-poll | Operator hit Ctrl-C | `Interrupted`; exit 130 (POSIX standard); existing handler. |
| Wall-clock `timeoutSec` exhausted | User stalled in browser past `--timeout` | `Timeout`; exit 1; clear the per-tick countdown. |
| `hydrateWorkspacesAfterLogin` rejects | Server unreachable post-login, or workspace endpoint 401 | Stderr warning per D23; exit code stays 0; credentials.json was already saved. |
| `Date.now()` clock skew during D22 countdown | Operator's clock jumps | Countdown shows the delta vs. session start; if it goes negative, prose flips to `Session expires in 0:00`. Real timeout is server-side; client display is informational. |
| `--no-input` mode | Scripted invocation | D22 countdown suppressed (no spinner); D23 stderr warning still emits (scripts want to see hydration failures); D29 backoff still applies (rate-limit safety isn't UX). |

---

## Invariants

1. `commandLogin` exit codes match `test/login.unit.test.ts` exactly — enforced by the pin tests already in the file. 0 = success (incl. hydration-failed), 1 = login failed, 130 = SIGINT.
2. `AUTH_PRESET` retains every existing key — D28 only adds keys. Enforced by the spec's own test that lists every pre-existing entry must still be in the exported preset.
3. `pollUntilComplete` makes ≤ ⌈timeoutSec / pollMs⌉ × 2 HTTP calls (the ×2 buffer absorbs jittered ticks and the transient budget). Enforced by an acceptance test that counts mock fetch invocations.
4. No prod TS file in this spec's blast-radius exceeds 350 lines after the edits. Enforced by RULE FLL pre-commit hook.
5. No `as any` / `!` / `@ts-expect-error` introduced. Enforced by `bun run lint` + `bun run typecheck`.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_d22_countdown_ticks_per_poll` | Per-poll write to stdout matches `/Session expires in \d+:\d{2}/`; transitions to single-digit second prose at `< 10s`. |
| `test_d22_countdown_suppressed_in_no_input` | `--no-input` produces zero countdown writes; only the final success line. |
| `test_d23_hydration_failure_emits_stderr_warning_exit_0` | When `hydrateWorkspacesAfterLogin` rejects, stderr matches `/warn: post-login workspace hydration failed/`; exit code is 0; credentials.json exists with 0600 mode. |
| `test_d23_hydration_success_emits_no_warning` | Happy path — no `warn:` line on stderr. |
| `test_d28_invalid_session_maps_to_InvalidSession` | 404 from server mid-poll → JSON envelope `error.code === "InvalidSession"`. |
| `test_d28_expired_session_maps_to_ExpiredSession` | 410 from server → `"ExpiredSession"`. |
| `test_d28_network_error_maps_to_NetworkError` | `fetch` throws `TypeError` → `"NetworkError"` (after blip budget). |
| `test_d28_rate_limited_maps_to_RateLimited` | 429 → `"RateLimited"`. |
| `test_d28_timeout_maps_to_Timeout` | `timeoutSec` exhausted → `"Timeout"`. |
| `test_d28_interrupted_maps_to_Interrupted` | SIGINT propagated → `"Interrupted"`, exit 130. |
| `test_d28_unknown_error_falls_through_to_generic` | Unmapped error retains the existing fallback prose; backwards compat. |
| `test_d29_first_poll_immediate` | First mock-fetch call fires at t≈0 (no backoff). |
| `test_d29_subsequent_polls_use_exp_backoff_with_jitter` | Inter-poll delays grow geometrically up to 5s cap; each delay is within ±20% of the nominal. |
| `test_d29_retry_after_honored` | 429 with `Retry-After: 3` → next poll fires at t+3s (overrides local backoff). |
| `test_d30_single_503_survives_login_completes` | Inject one 503 mid-poll; login completes; analytics `attempt` callback fires with `attempt: 2`. |
| `test_d30_double_503_surfaces_NetworkError` | Inject two 503s back-to-back; login fails with `NetworkError`. |
| `test_invariant_existing_pin_tests_still_pass` | All `test/login.unit.test.ts` rows that predate this spec still pass byte-for-byte. |
| `test_invariant_auth_preset_keys_superset` | Exported `AUTH_PRESET` contains every pre-existing key plus the six new codes from D28. |

Per-dimension acceptance:
- `acceptance_d30_full_poll_survives_one_injected_503` — stub backend injects one 503 on the second poll tick; assert exit 0, stdout contains `login complete`, credentials.json exists.

---

## Acceptance Criteria

- [ ] `bun run typecheck` clean — verify: `(cd zombiectl && bun run typecheck)`
- [ ] `bun run lint` 0/0 — verify: `(cd zombiectl && bun run lint)`
- [ ] `bun test` baseline + new rows all pass — verify: `(cd zombiectl && bun test)`
- [ ] `make harness-verify` 7/7 green — verify: `make harness-verify`
- [ ] `AUTH_PRESET` contains all six new codes — verify: `grep -E "InvalidSession|ExpiredSession|NetworkError|RateLimited|Timeout|Interrupted" zombiectl/src/lib/error-map-presets.ts | wc -l` ≥ 6
- [ ] `src/commands/core.ts` stays ≤ 350 lines — verify: `wc -l zombiectl/src/commands/core.ts`
- [ ] No `as any` / `!` / `@ts-expect-error` added — verify: `git diff origin/main..HEAD -- 'zombiectl/**/*.ts' | grep -E "as any|@ts-expect-error|: !" | wc -l` == 0
- [ ] PR #326 (parent M68) merged into main — verify: `gh pr view 326 --json state -q .state` == `MERGED`

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Typecheck + lint clean
(cd zombiectl && bun run typecheck && bun run lint) && echo "PASS" || echo "FAIL"

# E2: Test baseline preserved + new rows green
(cd zombiectl && bun test) | tail -5

# E3: AUTH_PRESET completeness
grep -cE "InvalidSession|ExpiredSession|NetworkError|RateLimited|Timeout|Interrupted" \
  zombiectl/src/lib/error-map-presets.ts

# E4: File-length cap on the primary touched file
wc -l zombiectl/src/commands/core.ts

# E5: No silenced strictness in the diff
git diff origin/main..HEAD -- 'zombiectl/**/*.ts' \
  | grep -E "^\\+.*\\b(as any|@ts-expect-error)\\b|^\\+.*: !\\s*[A-Z]" \
  | grep -v "^+++ "

# E6: Harness gates
make harness-verify
```

---

## Dead Code Sweep

N/A — no files deleted. D27's stage decomposition is the architecture; this spec extends those stages, doesn't replace any.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does |
|------|-------|--------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against the Test Specification above. The 18 listed test rows are the floor. |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec + the locked external signature + the pinned exit-code contracts. |
| After `gh pr create` | `/review-pr` | Post-merge-diff review on the open PR; comment-resolve before requesting human review. |

If `/review` flags D29's exp-backoff helper as duplicating the HTTP-side `backoffDelay`: that's the RULE UFS-relevant judgment call — pick one named helper and route both call sites through it. Captain decision per `feedback_gate_flag_triage`.

---

## Verification Evidence

> Filled in during VERIFY; this section is empty at PENDING.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit + acceptance tests | `(cd zombiectl && bun test)` | _pending_ | |
| Lint | `(cd zombiectl && bun run lint)` | _pending_ | |
| Harness | `make harness-verify` | _pending_ | |
| File-length | `wc -l zombiectl/src/commands/core.ts` | _pending_ | |
| Strictness compliance | `git diff ... \| grep ...` | _pending_ | |

---

## Out of Scope

- **D20 — Idempotency check** (already-logged-in detection). Deferred to the **cli-auth handshake hardening** sibling spec; overlaps with the new handshake UX.
- **D21 — Token name flag** (device label). Same sibling spec; needs schema work this milestone doesn't touch.
- **D24 — Token validation before save** (`/me` ping). Same sibling spec; the introspection endpoint is its responsibility.
- **D25 — Argv-leak warning for `--token`**. Adds a new auth pathway that the sibling spec owns.
- **D26 — TTY-priority env resolution** (`ZMB_TOKEN`/`ZOMBIE_TOKEN`). Same sibling spec.
- **D32 — `zombiectl logout --all`**. Needs server-side revocation design from the sibling spec (Clerk JWTs are stateless).
- **Server-side handshake redesign** — the `auth_sessions` endpoint shape, token introspection, expiry semantics, revocation. Out-of-scope on both axes (CLI-only milestone; M68_001 closed without touching them).
- **PostHog event-schema changes** — D30 emits the existing `cli_http_retry` event shape; no new event types or properties.
