<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M71_001: Trigger Panel multi-card + Provider Guidance + Website OnboardingFlow + Hero CTA (M68 deferred UI/website work)

**Prototype:** v2.0.0
**Milestone:** M71
**Workstream:** 001
**Date:** May 18, 2026
**Status:** DONE
**Priority:** P2 — completes the M68 trigger DX surface (per-trigger cards, provider guidance table, OnboardingFlow, Hero CTA) that was deferred during M68's CHORE(close), plus §7 hero promo pill (Captain ask, in-PR amendment May 18, 2026). Not blocking any other workstream.
**Categories:** UI, WEBSITE
**Batch:** B1
**Branch:** feat/m71-001-p2-trigger-panel-onboarding-flow
**Depends on:** M68_001 (DONE) — this spec inherits the unfinished M68 §D / §E / §G surface listed in M68's "Deferred to follow-up" section.
**Provenance:** agent-generated. Original M71_001 P2 spec (May 17, 2026) bundled CLI login resilience (§1-§5: countdown, hydration warning, error-code split, exp-backoff polling, single-blip tolerance) AND dashboard / website UX work (§6-§11). On May 18, 2026 the CLI portion was **absorbed into M74_002** (CLI Browser Authorization Flow consolidation) — including the M68-deferred dimensions D20/D21/D24/D25/D26/D32 originally listed in this spec's Out of Scope. This spec was renamed from `M71_001_P2_CLI_LOGIN_RESILIENCE_AND_UX_POLISH.md` to its current name and scoped down to the dashboard / website residue. The original §6-§11 content is preserved verbatim below; only the surrounding framing changed.

**Canonical architecture:** N/A — this is dashboard + website UX polish, no architecture-doc surface.

---

## Implementing agent — read these first

1. `docs/v2/done/M68_001_P1_API_CLI_UI_DOCS_WEBSITE_TRIGGER_REGISTRATION_AND_FREE_TRIAL.md` §13 — the parent spec's "Deferred to follow-up" list names each section's design intent and references the carrying-forward files. Treat that prose as the contract this spec inherits.
2. `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` — current M68-shipped 2-tab UI (Webhook tab + Schedule tab). Reshape into per-trigger card list per §1.
3. `ui/packages/app/tests/zombies.test.ts` (the `describe("TriggerPanel interactions")` block, ~line 690) — existing tests stay green after this spec lands; new tests live in the dedicated `TriggerPanel.test.ts`.
4. `ui/packages/website/src/pages/Home.tsx` + `ui/packages/website/src/components/Hero.tsx` + `ui/packages/website/src/components/FeatureFlow.tsx` — current website Home state. §4 (OnboardingFlow) and §5 (Hero CTA) modify these.
5. M68_001 §"`OnboardingFlow.tsx` design" (line 398) and §"`provider-guidance.ts` schema" (line 371) — verbatim design blocks the implementer copies into the new files.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (RULE NDC, RULE NSQ, RULE UFS, RULE TST-NAM, RULE FLL).
- **`zombiectl/CLAUDE.md`** — N/A; this spec touches `ui/packages/app/` and `ui/packages/website/` only, not the CLI.
- **TS strict settings** — every new `.tsx` / `.ts` file must compile under the existing `ui/packages/app/tsconfig.json` and `ui/packages/website/tsconfig.json` settings. No `as any`, `!`, or `@ts-expect-error` added.
- `docs/ZIG_RULES.md`, `docs/SCHEMA_CONVENTIONS.md`, `docs/REST_API_DESIGN_GUIDELINES.md` — N/A; this spec is UI/website-only; no server-side surface touched.
- `docs/AUTH.md` — N/A; CLI login flow lives in M74_002. This spec does NOT modify auth.

---

## Overview

**Goal (testable):** the dashboard renders one card per declared trigger (webhook variants per known provider via `GuidedTriggerCard`; cron via `CronCard`; unknown sources fall back to the existing Copy-URL pattern). The website Home page gains a 4-card pictorial `OnboardingFlow` and the Hero CTA becomes a clipboard-write + toast + smooth-scroll affordance pointing at the OnboardingFlow anchor. All four pieces close the M68 "Deferred to follow-up" list.

**Problem:** M68_001 shipped a minimal 2-tab TriggerPanel (Webhook + Schedule placeholder) and a 3-row evidence-layout `FeatureFlow` on Home as a stand-in for the spec'd 4-card pictorial. The provider-guidance data table (`PROVIDER_GUIDANCE`), the `GuidedTriggerCard`, the `CronCard`, the website `OnboardingFlow`, and the Hero CTA redesign were all listed in M68's "Deferred to follow-up" but did not land before close.

**Solution summary:** Five focused sections, each carrying its own design block copied verbatim from M68:

- **§1** — TriggerPanel switches from tabs to per-trigger card list.
- **§2** — `provider-guidance.ts` data table with six (or seven) provider entries.
- **§3** — `GuidedTriggerCard.tsx` (State B — known provider).
- **§4** — `CronCard.tsx` (read-only cron display).
- **§5** — Website `OnboardingFlow.tsx` (4-card pictorial) + Home mount.
- **§6** — `Hero.tsx` primary-CTA redesign (clipboard + toast + smooth-scroll).

No server-side surface touched. No new HTTP endpoints. No schema changes. No new dependencies beyond a lightweight cron-parsing library if §4 chooses to use one.

---

## Files Changed (blast radius)

| File | Action | § | Why |
|------|--------|---|-----|
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | EDIT | §1 | Tabs UI → per-trigger card list. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.test.ts` | EDIT or NEW | §1 | Multi-card variant rows. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.ts` | NEW | §2 | Per-provider data table (six or seven entries). |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.test.ts` | NEW | §2 | Per-provider snapshot tests. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/GuidedTriggerCard.tsx` | NEW | §3 | State-B (known provider) card. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/CronCard.tsx` | NEW | §4 | Read-only cron card. |
| `ui/packages/website/src/components/OnboardingFlow.tsx` | NEW | §5 | 4-step pictorial. |
| `ui/packages/website/src/components/OnboardingFlow.test.tsx` | NEW | §5 | Snapshot. |
| `ui/packages/website/src/pages/Home.tsx` | EDIT | §5 | Mount OnboardingFlow (disposition a or b). |
| `ui/packages/website/src/components/FeatureFlow.tsx` + `FeatureFlow.test.tsx` | DELETE (disposition a only) | §5 | Replaced by OnboardingFlow. |
| `ui/packages/website/src/components/Hero.tsx` | EDIT | §6, §7 | Primary CTA redesign (§6) — clipboard + toast + smooth-scroll to `#onboarding-flow`. Promo pill (§7) between LIVE eyebrow and headline. |
| `ui/packages/website/src/components/Hero.test.tsx` | EDIT | §6, §7 | New CTA assertions (§6) + promo-pill assertions (§7). |
| `ui/packages/website/src/lib/rates.ts` | EDIT | §7 | Add `RATES_DISPLAY.FREE_TRIAL_PILL` (short pill string) sharing the date with `FREE_TRIAL_BANNER` via a private `FREE_TRIAL_END_DISPLAY` substring. |
| `ui/packages/website/src/lib/rates.test.ts` | EDIT | §7 | Pin pill / banner share a single date substring; pin pill text format. |

> **Anti-pattern guard:** no file in `zombiectl/`, `src/` (Zig), `docs/v2/done/`, or `docs/AUTH.md` is touched by this spec. CLI auth-flow work lives in M74_002.

---

## Sections (implementation slices)

### §1 — Trigger panel multi-card switch (M68 §D / §E1 / §F4)

**Provenance:** M68_001 §D narrative (line 73) + §E1 (line 128) + §F4 (line 141). Spec intent preserved verbatim below; implementing agent has the design ready.

**What M68 said the trigger panel should do:** "`TriggerPanel.tsx` renders one card per declared trigger in `zombie.triggers[]`. Card variants: `GuidedTriggerCard` (known webhook provider; pre-renders terminal registration command), `CopyUrlCard` (unknown source; today's behaviour as fallback), `CronCard` (schedule + next fire), `ApiCard` (catch-all `POST /v1/zombies/{id}/events` ingress)." `type: api` was carved out (§E5 / Out of Scope); `ApiCard.tsx` is **not** in scope for this spec either — it lands with the workspace-API-tokens spec. The four in-scope variants for M71_001 P2 are `GuidedTriggerCard`, `CopyUrlCard` (already conceptually in the shipped Tabs UI as the default Webhook tab), `CronCard`, and the per-trigger loop in `TriggerPanel.tsx` itself.

**What shipped in M68:** a 78-line 2-tab UI (Webhook tab with one URL + Copy button; Schedule tab with "Cron scheduling is CLI-only for V1" placeholder). Tested at `ui/packages/app/tests/zombies.test.ts:690` (`describe("TriggerPanel interactions")` with three rows — copy semantics + cron-placeholder visibility). Those existing tests stay green after this section lands (the Tabs UI either remains as the "no triggers declared" fallback or its assertions move to assert the new per-card layout — implementing agent decides at design time).

**What this section delivers:**

| Sub-dim | File | Action | Why |
|---|---|---|---|
| 1.1 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | EDIT | Switch from `<Tabs defaultValue="webhook">…</Tabs>` to `<>{zombie.triggers.map(t => <Card variant={t.type, t.source} t={t} />)}</>`. Footer prose: "Edit `TRIGGER.md` and reinstall to change triggers — the source markdown is the source of truth." Prop signature changes from `{ zombieId: string }` to `{ zombieId: string; triggers: ZombieTrigger[] }` (parent page already has `zombie.triggers` from the M68 list-projection change). |
| 1.2 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/TriggerPanel.test.ts` | EDIT (move from `tests/zombies.test.ts:690` *or* extend in place) | Multi-card-variant rows: (a) 3-trigger zombie → 3 cards in order; (b) `source: "weirdco"` → falls back to `CopyUrlCard`; (c) last-delivery line populates from `listZombieEvents(actor_prefix, limit:1)`. Preserve the existing Tabs-UI test assertions if those code paths remain. |

**Acceptance:** an authenticated user installs a zombie with `triggers: [{type: webhook, source: github, events: ["push"]}, {type: cron, schedule: "*/15 * * * *"}]` → `/zombies/{id}` renders TriggerPanel with exactly two cards in order: a `GuidedTriggerCard` for the github webhook (uses §3) and a `CronCard` for the cron (uses §4).

### §2 — `provider-guidance.ts` data table + tests (M68 §E2 / §F3)

**Provenance:** M68_001 §E2 (line 129) + §F3 (line 140) + §"`provider-guidance.ts` schema" (line 371). Verbatim design carried forward; M71 P2 implementer ships the table.

**What M68 said:** "Static `PROVIDER_GUIDANCE: Record<Source, GuidanceCard>` map. Entries for `github`, `linear`, `jira`, `grafana`, `slack`, `agentmail`. Each defines: title, events-label formatter, terminal-command template, web-User-Interface deep-link template, user-input variable list (e.g. `OWNER/REPO`, `TEAM_ID`, `WORKSPACE`)." Note: M68 also planned a `clerk` entry as a deep-link-only variant (line 371) — that brings the count to seven providers if the M71 implementer chooses to include it; minimum six per the §E2 row.

**Schema** (TypeScript — copy verbatim from M68 §371 onward when implementing):

```typescript
type Source = "github" | "linear" | "jira" | "grafana" | "slack" | "agentmail" | "clerk";

type GuidanceCard = {
  title: string;
  eventsLabel: (events: string[]) => string;        // e.g. ["push","pull_request"] → "On push, pull_request"
  command: (vars: Record<string, string>, webhookUrl: string, events: readonly string[]) => string;
  webUiDeepLink: (vars: Record<string, string>) => string;
  variables: Array<{ name: string; example: string; required: boolean }>;
};

// NOTE: the `command` callback takes a 3rd `events` arg so per-provider templates
// can vary the rendered command by trigger.events (e.g. GitHub's `-F events[]=push`
// list) without re-deriving them inside the closure. The original M68 §line 122 sketch
// was 2-arg; landing under this spec widens the signature with rationale per
// CLAUDE.md "signature change → update spec first".

export const PROVIDER_GUIDANCE: Record<Source, GuidanceCard>;
```

**Per-provider verbatim content** lives in M68 §"`provider-guidance.ts` schema" (line 371 onward). Implementer copies that block into the new file.

**Files:**

| Sub-dim | File | Action | Why |
|---|---|---|---|
| 2.1 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.ts` | NEW | The data table. RULE FLL — if it crosses 350 lines, split per provider into `provider-guidance/{github,linear,jira,grafana,slack,agentmail,clerk}.ts` with a `mod.ts` aggregator (M68's note at §line 315 stays binding). |
| 2.2 | `ui/packages/app/app/(dashboard)/zombies/[id]/components/provider-guidance.test.ts` | NEW | Per-provider snapshot test: given `triggers[0] = {source, events}` + webhook URL, the rendered command + deep-link strings match a fixture. Pins prose. Six (or seven) fixture files alongside. |

**Acceptance:** `PROVIDER_GUIDANCE.github` rendering a `gh api repos/OWNER/REPO/hooks` command with the M68 webhook URL substituted matches the fixture byte-for-byte.

### §3 — `GuidedTriggerCard.tsx` (M68 §E3)

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

### §4 — `CronCard.tsx` (M68 §E4)

**Provenance:** M68_001 §E4 (line 131).

**What M68 said:** "Renders cron triggers. Shows the schedule, next-fire computed client-side (timezone-aware), and links the user to Recent Activity filtered `actor LIKE 'cron:%'`. ~50 lines."

**File:** `ui/packages/app/app/(dashboard)/zombies/[id]/components/CronCard.tsx` — NEW.

**Props:** `{ trigger: ZombieTrigger /* type === "cron" */; zombieId: string }`.

**Composition:**

1. Header: `Cron — ${trigger.schedule}` (the raw cron expression).
2. Next-fire line: computed client-side from the cron expression + `Date.now()` + IANA tz from `Intl.DateTimeFormat().resolvedOptions().timeZone`. Implementer picks a lightweight cron-parsing dep (`cron-parser` is ~9 kB minified; check bundle budget against the website's `.size-limit.json` 140 kB asserted ceiling).
3. "Cron is read-only in the Dashboard" prose: "Declared in TRIGGER.md, runtime-managed by NullClaw's `cron_add` tool. Edit `TRIGGER.md` and reinstall to change the schedule." Mirrors the Out-of-Scope note from M68 line 993.
4. Recent-activity filter link: `<Link href={\`/zombies/${zombieId}?actor_prefix=cron:\`}>View cron deliveries →</Link>`.

### §5 — Website `OnboardingFlow` (M68 §G5 / §G6 / §G7)

**Provenance:** M68_001 §G5 (line 153) + §G6 (line 154) + §G7 (line 155) + §"`OnboardingFlow.tsx` design" (line 398).

**Coexistence with `FeatureFlow.tsx`** (implementer decision at PLAN time):

`FeatureFlow.tsx` shipped at M68 in the slot OnboardingFlow was supposed to fill (`Home.tsx:40`). FeatureFlow is a 3-row alternating evidence layout (install / event-trace / mission-control); OnboardingFlow as spec'd is a 4-card horizontally-laid pictorial step-by-step (install / run skill / wire webhook / steer). Two valid dispositions:

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
| 5.1 | `ui/packages/website/src/components/OnboardingFlow.tsx` | NEW (~180 LOC). |
| 5.2 | `ui/packages/website/src/components/OnboardingFlow.test.tsx` | NEW — snapshot test for the four cards; deterministic rendering. Asserts (a) four cards rendered in numbered order, (b) each card contains the expected code snippet text. |
| 5.3 | `ui/packages/website/src/pages/Home.tsx` | EDIT — mount `<OnboardingFlow />` per the chosen disposition (a) or (b). |
| 5.4 | `ui/packages/website/src/components/FeatureFlow.tsx` + `FeatureFlow.test.tsx` (if disposition (a)) | DELETE — only if FeatureFlow is fully replaced; carry FeatureFlow's existing tests into the OnboardingFlow test surface where they overlap (the install-command card is in both). |

**Anchor:** the section's outer container gets `id="onboarding-flow"` so §6's smooth-scroll target works after this lands.

### §6 — `Hero.tsx` primary-CTA redesign (M68 §G11)

**Provenance:** M68_001 §G11 (line 159).

**What M68 said:** Replace the `<a href={DOCS_QUICKSTART_URL}>` "→ install in Claude Code" button with a `<button>` whose onClick (a) writes `npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie` to `navigator.clipboard`, (b) shows a 2-second "Copied — paste into your terminal" toast (existing design-system `<Toast>` or an `aria-live` region fallback), (c) smooth-scrolls to the `#onboarding-flow` anchor on the same page. Keep `DOCS_QUICKSTART_URL` as a small tertiary "read the full quickstart →" link inside OnboardingFlow itself (§5). Update `Hero.test.tsx:52-56` accordingly.

**Depends on §5** — the `#onboarding-flow` anchor must exist before the scroll target makes sense. Land §5 first or in the same PR.

**File:** `ui/packages/website/src/components/Hero.tsx` — EDIT lines around 64–70 (per M68's pinned range; verify line numbers at PLAN time since intervening edits may have shifted them).

**Tests:** `Hero.test.tsx:52-56` — assert (a) clicking the CTA writes the install command to a mocked clipboard, (b) the toast appears for ~2s then disappears, (c) `scrollIntoView` is called on the `#onboarding-flow` element.

---

### §7 — Hero promo pill (Pioneer-pattern, in-PR amendment May 18, 2026)

**Provenance:** in-PR Captain ask on PR #330. The free-trial pricing posture ("Free until July 31, 2026 — every event receipt and stage execution is on us") already lives on the pricing component (`Pricing.tsx` consuming `RATES_DISPLAY.FREE_TRIAL_BANNER`) but is invisible above the fold on the landing page. The promo is concrete, time-bound, and asymmetrically converting vs the generic "try for free" framing — the landing should make it explicit.

**What lands:** between the LIVE eyebrow `<p data-testid="hero-eyebrow">` and the `<h1 data-testid="hero-headline">` in `Hero.tsx`, render a React Router `<Link to="/pricing">` styled as a small mono pill carrying a `Promo` lozenge + the short trial-end string + an aria-hidden `→`. Shape mirrors Pioneer's "Free inference on Opus 4.7 until Aug 1 →" pattern. Pill text is **derived from the rates pin**, not hardcoded in `Hero.tsx`.

**Rates-pin coupling:** `ui/packages/website/src/lib/rates.ts` is the source of truth for the trial-end display string. A new `RATES_DISPLAY.FREE_TRIAL_PILL` ("Free until July 31, 2026") is added; both `FREE_TRIAL_BANNER` (pricing) and `FREE_TRIAL_PILL` (hero) consume a single internal `FREE_TRIAL_END_DISPLAY` substring so the date can never drift between hero and pricing. The numeric `FREE_TRIAL_END_MS` constant remains the cross-tier-pinned source (audit-cross-tier-rates.sh enforces it across Zig + 3 TS surfaces); the display string is a TS-only display-layer mirror.

**File:** `ui/packages/website/src/components/Hero.tsx` — EDIT, insert between the existing eyebrow `<p>` and `<h1>` blocks (~lines 78–91 post-§6).

**Design tokens (no arbitrary values, DESIGN TOKEN GATE compliant):**
- Container: `inline-flex items-center gap-2 rounded-full bg-card border border-border px-3 py-1 text-sm font-mono text-text-muted hover:text-text transition-colors w-fit`
- `Promo` lozenge: `rounded-full bg-pulse text-pulse-fg px-2 py-0.5 text-xs uppercase tracking-eyebrow font-medium`
- Trailing `→` is `aria-hidden="true"`; the link's accessible name is its text content.

**Tests:** `Hero.test.tsx` — assert (a) pill renders with `data-testid="hero-promo-pill"`, (b) it is an `<a>` with `href="/pricing"`, (c) it contains the literal "Free until July 31, 2026" string sourced from `RATES_DISPLAY.FREE_TRIAL_PILL`, (d) the pill renders before the `<h1>` in document order (DOM-position check, not snapshot). `rates.test.ts` — assert (e) `RATES_DISPLAY.FREE_TRIAL_PILL` equals the exact pin string, (f) the pill string and the banner string share the same `"July 31, 2026"` date substring (single-source-of-truth invariant).

**Acceptance:** the existing 12 Hero.test.tsx rows continue to pass byte-for-byte (no §6 regression); 4 new rows green; rates.test.ts gains 2 rows.

---

## Interfaces

No HTTP / OpenAPI / wire surface added or changed. No new dashboard or website routes. The contracts this spec locks:

```typescript
// TriggerPanel prop signature changes (§1.1):
type TriggerPanelProps = {
  zombieId: string;
  triggers: ZombieTrigger[];   // NEW — parent passes from zombie.triggers
};

// PROVIDER_GUIDANCE export (§2): GuidanceCard.command is 3-arg
// (vars, webhookUrl, events) — see §2 schema block.
export const PROVIDER_GUIDANCE: Record<Source, GuidanceCard>;

// OnboardingFlow component (§5):
export function OnboardingFlow(): JSX.Element;  // pure presentational
```

No new flags. No new env vars. No new dependencies beyond a lightweight cron-parsing library at §4 implementer's discretion.

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Zombie has zero triggers declared | Edge case from M68 spec | TriggerPanel renders a single "No triggers declared" Card with the legacy bare webhook URL as a fallback ingress (via `CopyUrlFallback` source="legacy"). The M68 Tabs scaffolding is removed by RULE NLR (touch-it-fix-it) — its only remaining caller was this empty-state branch, and dragging it forward solely for that case is dead-code framing. The "user can still find a webhook URL" intent of the M68 row is preserved. |
| `trigger.source` not in `PROVIDER_GUIDANCE` keyset | Unknown / new provider | TriggerPanel renders `CopyUrlCard` (existing M68 fallback shape). |
| Cron expression unparseable | Bad TRIGGER.md input | CronCard shows the raw expression in the header + a "schedule unparseable — check `TRIGGER.md`" warning line in place of next-fire. |
| `navigator.clipboard.writeText` rejects (insecure context / permission denied) | Browser restricts clipboard access | Fallback to a visible "Copy this command:" prose block with the command selectable; the toast still fires but with prose "Selected — copy manually." |
| `scrollIntoView` no-op (anchor missing because §5 didn't land yet) | §6 lands before §5 | §6 PR must include §5; the dependency is documented. CI test for §6 asserts `#onboarding-flow` exists in the rendered Home page. |
| Bundle-size regression beyond the website's 140 kB landing-js ceiling (`ui/packages/website/.size-limit.json`) | `cron-parser` or other §4 dep | Implementer measures pre-/post-bundle size; if over budget, swap for a smaller cron parser or roll a minimal expression-only formatter. |
| Hero promo pill date drifts from `RATES_DISPLAY.FREE_TRIAL_BANNER` date | Someone edits the pill string without touching the banner (or vice versa) | Both consume a single private `FREE_TRIAL_END_DISPLAY` substring in `rates.ts`. `rates.test.ts` pins the shared substring; drift fails the test. |
| Hero promo pill date drifts from `FREE_TRIAL_END_MS` numeric pin | Someone bumps `FREE_TRIAL_END_MS` (Zig + 3 TS surfaces, audit-cross-tier-rates.sh enforced) but forgets the display string | Out-of-scope automation for now; the rates.ts module-level comment names the coupling, the audit script flags numeric drift, and the human PR review is the catch for the display string until a future spec adds a `FREE_TRIAL_END_MS → display` derivation. |

---

## Invariants

1. **No `zombiectl/` file is touched by this spec.** Enforced by RULE NLR (touch-it-fix-it) — anything that asks for CLI changes belongs in M74_002 or a different spec.
2. **No file added or modified by this spec exceeds 350 lines.** Enforced by RULE FLL pre-commit hook. The provider-guidance table splits per-provider if it grows.
3. **The M68 shipped `TriggerPanel` test rows continue to pass** (or their assertions move to the new TriggerPanel.test.ts with equivalent coverage). No regression of M68 acceptance.
4. **No `as any` / `!` / `@ts-expect-error` introduced.** Enforced by `bun run lint` + `bun run typecheck`.
5. **Hero promo pill date string is never hardcoded in `Hero.tsx`.** §7. Pill consumes `RATES_DISPLAY.FREE_TRIAL_PILL` from `rates.ts`. Enforced by code review + rates.test.ts pinning the substring share with `FREE_TRIAL_BANNER`.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_trigger_panel_renders_one_card_per_trigger` | Zombie with 3 triggers → TriggerPanel renders 3 cards in declared order. |
| `test_trigger_panel_unknown_source_falls_back_to_copy_url` | `source: "weirdco"` → `CopyUrlCard` rendered with the webhook URL + Copy button. |
| `test_trigger_panel_last_delivery_populates_from_events_list` | `listZombieEvents(actor_prefix:"github:", limit:1)` returns a delivery → card's last-delivery line shows relative-time. |
| `test_provider_guidance_github_snapshot` | `PROVIDER_GUIDANCE.github.command({OWNER:"x", REPO:"y"}, "https://api...")` matches fixture byte-for-byte. |
| `test_provider_guidance_linear_snapshot` | Same for linear. |
| `test_provider_guidance_jira_snapshot` | Same for jira. |
| `test_provider_guidance_grafana_snapshot` | Same for grafana. |
| `test_provider_guidance_slack_snapshot` | Same for slack. |
| `test_provider_guidance_agentmail_snapshot` | Same for agentmail. |
| `test_provider_guidance_clerk_snapshot` | If clerk entry included — same. |
| `test_guided_trigger_card_re_renders_command_on_variable_input` | User types into the `OWNER/REPO` input → rendered command block updates client-side without re-fetch. |
| `test_cron_card_next_fire_timezone_aware` | `*/15 * * * *` with `America/New_York` tz → next-fire is the correct local time. |
| `test_cron_card_unparseable_expression_shows_warning` | Bad cron expression → warning prose in place of next-fire. |
| `test_onboarding_flow_renders_four_cards_in_order` | OnboardingFlow renders cards 1-2-3-4 in numbered order; each contains the expected snippet text. |
| `test_hero_cta_writes_install_command_to_clipboard` | Click CTA → mocked `navigator.clipboard.writeText` called with the expected string. |
| `test_hero_cta_shows_toast_then_dismisses` | Toast appears for ~2s then disappears. |
| `test_hero_cta_scrolls_to_onboarding_flow` | `scrollIntoView` called on the `#onboarding-flow` element. |
| `test_existing_trigger_panel_tabs_assertions_preserved` | If the Tabs-UI code path remains as the "no triggers" fallback, M68's existing test rows continue to pass byte-for-byte. |
| `test_hero_promo_pill_renders_link_to_pricing` | Pill renders with `data-testid="hero-promo-pill"`, is an `<a>` whose `href="/pricing"`, contains the literal "Free until July 31, 2026" sourced from `RATES_DISPLAY.FREE_TRIAL_PILL`. |
| `test_hero_promo_pill_precedes_headline_in_document_order` | DOM position check: pill node sits before `<h1 data-testid="hero-headline">` and after `<p data-testid="hero-eyebrow">`. |
| `test_rates_display_free_trial_pill_pinned` | `RATES_DISPLAY.FREE_TRIAL_PILL` literal equals `"Free until July 31, 2026"`. |
| `test_rates_display_pill_and_banner_share_trial_end_date` | Both `RATES_DISPLAY.FREE_TRIAL_PILL` and `RATES_DISPLAY.FREE_TRIAL_BANNER` contain the `"July 31, 2026"` substring (single source of truth). |

Per-section acceptance criteria match the §X "Acceptance" blocks above.

---

## Acceptance Criteria

- [ ] `(cd ui/packages/app && bun run typecheck && bun run lint && bun test)` clean.
- [ ] `(cd ui/packages/website && bun run typecheck && bun run lint && bun test)` clean.
- [ ] `make harness-verify` 7/7 green.
- [ ] No new file or modified file in this spec's blast-radius exceeds 350 lines.
- [ ] No `as any` / `!` / `@ts-expect-error` added — `git diff origin/main..HEAD -- 'ui/packages/**/*.ts' 'ui/packages/**/*.tsx' | grep -E "as any|@ts-expect-error|: !" | wc -l` == 0.
- [ ] M68 PR #326 merged into main — `gh pr view 326 --json state -q .state` == `MERGED`.
- [ ] Bundle size for `ui/packages/website` stays under the 140 kB landing-js ceiling pinned in `ui/packages/website/.size-limit.json` (the M68 prose said 220 kB; the actual size-limit config was 140 kB throughout — this spec aligns the prose).

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: app package typecheck + lint + test
(cd ui/packages/app && bun run typecheck && bun run lint && bun test) && echo PASS || echo FAIL

# E2: website package typecheck + lint + test
(cd ui/packages/website && bun run typecheck && bun run lint && bun test) && echo PASS || echo FAIL

# E3: harness
make harness-verify

# E4: no zombiectl files touched
git diff origin/main..HEAD --name-only | grep -c '^zombiectl/'
# expect: 0

# E5: bundle-size check (if website build asserts a ceiling)
(cd ui/packages/website && bun run build) && du -sk dist/

# E6: no silenced strictness
git diff origin/main..HEAD -- 'ui/packages/**/*.ts' 'ui/packages/**/*.tsx' \
  | grep -E "^\\+.*\\b(as any|@ts-expect-error)\\b|^\\+.*: !\\s*[A-Z]" \
  | grep -v "^+++ "
```

---

## Dead Code Sweep

If §5 picks disposition (a) — replace `FeatureFlow` with `OnboardingFlow`:

| Deleted symbol | Grep | Expected |
|----------------|------|----------|
| `FeatureFlow` component | `grep -rn 'FeatureFlow' ui/packages/website/` | Zero matches |
| `FeatureFlow` test file | `ls ui/packages/website/src/components/FeatureFlow.test.tsx 2>/dev/null` | Not found |

If disposition (b) — coexist — both components remain; no dead-code sweep.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does |
|------|-------|--------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage against the Test Specification above. The 18 listed test rows are the floor. |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec + the locked prop signatures + the M68 §13 design blocks. |
| After `gh pr create` | `/review-pr` | Post-merge-diff review on the open PR; comment-resolve before requesting human review. |

---

## Discovery (consult log)

**May 17, 2026 — original P2 spec authored.** Bundled CLI login resilience (§1-§5: countdown, hydration warning, error codes, exp-backoff, blip tolerance) with the M68-deferred dashboard/website work (§6-§11). Categories were `CLI, UI, WEBSITE`.

**May 18, 2026 — rescoped.** Captain consolidated every in-flight CLI auth concern into M74_002 (CLI Browser Authorization Flow). The original §1-§5 CLI dimensions (D22 / D23 / D28 / D29 / D30) and the CLI dimensions originally listed in this spec's Out of Scope (D20 / D21 / D24 / D25 / D26 / D32) all moved into M74_002 §5-§6. This spec was renamed from `M71_001_P2_CLI_LOGIN_RESILIENCE_AND_UX_POLISH.md` to `M71_001_P2_UI_WEBSITE_TRIGGER_PANEL_AND_ONBOARDING_FLOW.md`. Categories trimmed to `UI, WEBSITE`. Sections renumbered (former §6-§11 are now §1-§6).

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| App typecheck + lint + test | `(cd ui/packages/app && bun run typecheck && bun run lint && bun test)` | tsc clean · oxlint 0/0 · 47 files / 504 tests | ✅ |
| App coverage thresholds | `(cd ui/packages/app && bun run test:coverage)` | statements 96.05 · branches 90.15 · functions 95.4 · lines 97.32 (gate: 95/90/95/95) | ✅ |
| Website typecheck + lint + test (post-§7) | `(cd ui/packages/website && bun run typecheck && bun run lint && bun test)` | tsc clean · oxlint 0/0 · **19 files / 146 tests** (+7 over pre-§7 baseline: 3 Hero pill + 3 rates pill + 1 banner-prefix pin) | ✅ |
| Harness | `make harness-verify` | UFS / DESIGN TOKEN / SPEC TEMPLATE / ERROR REGISTRY / LOGGING / LIFECYCLE / CROSS-TIER RATES / MS-ID+UI — ALL GATES GREEN | ✅ |
| Bundle size (landing js, post-§7) | `(cd ui/packages/website && bun run size)` | **132.94 kB gzipped** — under the 140 kB ceiling pinned in `ui/packages/website/.size-limit.json` (7.06 kB headroom; §7 adds ~0.34 kB gz vs pre-§7 baseline) | ✅ |
| Bundle size (landing css) | `(cd ui/packages/website && bun run size)` | 9.89 kB gzipped — under the 20 kB ceiling | ✅ |
| No zombiectl edits | `git diff origin/main..HEAD --name-only \| grep -c '^zombiectl/'` | 0 | ✅ |
| Strictness compliance | E6 grep from "Eval Commands" | 0 `as any` / `!` / `@ts-expect-error` introduced | ✅ |
| Cross-tier rates pin (§7 coupling) | `bash scripts/audit-cross-tier-rates.sh` | `FREE_TRIAL_END_MS` numeric value pins across `src/state/tenant_billing.zig` + 3 TS surfaces; display string `FREE_TRIAL_END_DISPLAY` shared between `RATES_DISPLAY.FREE_TRIAL_BANNER` and `RATES_DISPLAY.FREE_TRIAL_PILL` (rates.test.ts pins the substring) | ✅ |

---

## Out of Scope

- **CLI login resilience and polish (D22 / D23 / D28 / D29 / D30)** — absorbed into **M74_002** (CLI Browser Authorization Flow) §6 "Login UX hardening" on May 18, 2026. Originally §1-§5 of this spec.
- **CLI handshake hardening dimensions D20 / D21 / D24 / D25 / D26 / D32** (idempotency check, `--token-name` flag, `/me` ping, argv-leak warning, TTY-priority env resolution, `logout --all` rename) — absorbed into M74_002 §5 on the same date. Originally listed in this spec's Out of Scope as "deferred to the cli-auth handshake hardening sibling spec."
- **`ApiCard.tsx`** (catch-all `POST /v1/zombies/{id}/events` ingress variant from M68 §E5) — lands with the workspace-API-tokens spec, not here.
- **Server-side handshake redesign, `auth_sessions` endpoint shape, token introspection, expiry semantics, revocation** — all in M74_002.
- **PostHog event-schema changes** — no new analytics emits in this spec.
- **`zombiectl/` modifications** — RULE NLR-forbidden in this spec; anything CLI lives in M74_002.
