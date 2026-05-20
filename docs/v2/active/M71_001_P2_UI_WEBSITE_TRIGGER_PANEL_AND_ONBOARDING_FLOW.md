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
**Status:** IN_PROGRESS
**Priority:** P2 — completes the M68 trigger DX surface (per-trigger cards, provider guidance table, OnboardingFlow, Hero CTA) that was deferred during M68's CHORE(close), plus §7 hero promo pill (Captain ask, in-PR amendment May 18, 2026), plus §8–§12 post-audit dashboard follow-ups (Captain ask, May 20, 2026 — folds M76_001). Not blocking any other workstream.
**Categories:** UI, WEBSITE, API
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
| `ui/packages/app/lib/clerkAppearance.ts` (+ `.test.ts`) | EDIT / NEW | §8 | `formFieldInput` gets `--surface-1` fill + `--border-strong` so inputs separate from the card. |
| `ui/packages/app/app/(dev)/ds-button-rsc/page.tsx` | DELETE | §9 | Remove public dev route; Button RSC-safety covered by real server-component usage. |
| `ui/packages/app/vitest.config.ts` | EDIT | §9 | Drop the now-dead `**/ds-button-rsc/**` coverage exclude. |
| `ui/packages/design-system/src/design-system/Spinner.tsx` (+ `.test.tsx`, index export) | NEW | §10 | Branded `WakePulse` spinner; swap `Loader2Icon` spinner sites. |
| `ui/packages/app/app/(dashboard)/zombies/loading.tsx` + inline `Loader2Icon` spinner sites | EDIT | §10 | Use `Spinner`. Skeleton placeholders untouched. |
| `ui/packages/app/lib/api/workspaces.ts` | EDIT | §11 | Add `createTenantWorkspace(token, {name?})`. |
| `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` (+ test) | EDIT | §11 | "New workspace" item → create dialog. |
| `ui/packages/app/app/(dashboard)/actions.ts` | EDIT | §11 | Server action wrapping `POST /v1/workspaces` + active-workspace switch. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/{page,actions,loading}.tsx` + `components/*` | NEW | §12 | List / mint / revoke / delete surface (mirrors `/credentials`). |
| `ui/packages/app/app/(dashboard)/settings/page.tsx` | EDIT | §12 | Third `SettingsLink` card → API keys. |
| `ui/packages/app/lib/api/api_keys.ts` (+ types in `lib/types.ts`) | NEW | §12 | Typed client: list/create/revoke/delete. |
| `src/http/handlers/api_keys/tenant.zig` | EDIT (comment only) | §12 | RULE NLR — replace "no first-party UI" block with `/settings/api-keys` pointer. |
| `ui/packages/app/tests/**` + `tests/e2e/acceptance/settings-api-keys.spec.ts` | NEW | §8–§12 | Unit + e2e coverage for the five follow-ups. |

> **Anti-pattern guard:** no file in `zombiectl/`, `src/auth/`, `src/http/handlers/auth/`, or `ui/packages/app/lib/auth/` is touched by this spec — those are M74_002's reserved surface. The sole `src/` (Zig) touch is one **comment-only** edit in `src/http/handlers/api_keys/tenant.zig` (§12, RULE NLR); no Zig logic changes. `docs/AUTH.md` is not touched.

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

## §8–§12 — Post-audit dashboard follow-ups (Captain ask, May 20, 2026)

**Provenance:** during PR #330 review the Captain requested a full dashboard route/navigation audit. The audit surfaced five gaps; the Captain elected to fix all five in this PR rather than spin separate specs ("fix all of them in this PR — I don't want a separate spec"). M76_001 (Tenant API Keys self-service UI, PENDING) is **absorbed** here as §12 and deleted from `docs/v2/pending/`. None of the five touch M74_002's reserved surface (`zombiectl/`, `src/auth/`, `src/http/handlers/auth/`, `ui/packages/app/lib/auth/`).

### §8 — Clerk sign-in input contrast fix

**Problem:** `ui/packages/app/lib/clerkAppearance.ts` themes both the card surface (`cardBox.backgroundColor`) and the input fields (`formFieldInput.backgroundColor`) with the same `var(--surface-2)` token. With no luminance delta between the card and the inputs sitting on it, the text boxes are visually invisible — the operator cannot tell where to click (Captain: "its black and I didn't know where to click").

**What lands:** give `formFieldInput` its own surface + a visible border so the click target reads as an input. Use `var(--surface-1)` for the field fill (one step darker than the `--surface-2` card) plus `borderColor: var(--border-strong)`. Inputs now separate from the card. No new tokens — both already exist in `theme.css`.

**Acceptance:** `clerkAppearance.test.ts` (new or extended) asserts `formFieldInput.backgroundColor !== cardBox.backgroundColor` (the regression that caused the bug) and that `formFieldInput.borderColor` is set.

### §9 — Remove `/ds-button-rsc` dev route

**Problem:** `app/(dev)/ds-button-rsc/page.tsx` is a public, unauthenticated route that ships to production and is reachable by URL.

**Decision (Captain, May 20, 2026):** delete it. An interim gate (`notFound()` in production, keep the build fixture) was considered because `vitest.config.ts` documented the route as a build-time RSC-safe assertion for the design-system `Button`. The Captain directed full removal. Verified the guard is **redundant**: `Button` carries no `"use client"` directive (RSC-safe by construction) and is already rendered in real server components — `settings/billing/components/BillingBalanceCard.tsx` and `approvals/[gateId]/page.tsx` — so the contract stays exercised by production code without a dedicated fixture.

**What lands:** delete the `app/(dev)/` route group, its dev-fixture test, and the now-dead `**/ds-button-rsc/**` coverage exclude in `vitest.config.ts`.

**Acceptance:** `find app/\(dev\)` returns nothing; `grep -rn ds-button-rsc app vitest.config.ts` returns nothing; app tests + typecheck stay green (Button RSC usage in BillingBalanceCard / approvals detail still compiles).

### §10 — Branded loader (`Spinner` via `WakePulse`)

**Problem:** loading affordances are inconsistent. The brand wake-pulse appears only as the header/sign-in "live" dot; in-flight loaders use Lucide's generic `Loader2Icon` (`/zombies` loading, BillingUsageTab "Load more", InstallZombieForm submit, etc.) while `/approvals`, `/settings`, `/events` use `Skeleton`. The generic spinner is off-brand.

**What lands:** add a `Spinner` primitive to `@usezombie/design-system` built on the existing `WakePulse` glow-ring keyframe (a sized, `aria-busy` brand pulse). Swap the `Loader2Icon`-as-spinner usages in the dashboard to `Spinner`. **Skeleton placeholders stay** — they are layout-shape loaders, a different affordance, and are already consistent. The decision (pulse for indeterminate "working", skeleton for "page-shape pending") is documented in the component docstring.

**Acceptance:** `Spinner.test.tsx` asserts it renders the pulse element with `role="status"` / `aria-busy`. No remaining `Loader2Icon` import in dashboard spinner sites (Skeleton untouched). DESIGN TOKEN GATE clean (pulse tokens, no arbitraries).

### §11 — Create-workspace UI

**Problem:** the backend exposes `POST /v1/workspaces` (bearer, `invokeCreateWorkspace` → `ws_lifecycle.innerCreateWorkspace`) but the dashboard has no create affordance — workspaces only ever come from the Clerk signup webhook. A tenant that lands without a workspace, or wants a second, is stuck.

**Backend contract (verified, unchanged):** `POST /v1/workspaces`, body `{ name?: string }` (empty/blank → server picks a Heroku-style name), → `201 { workspace_id, name, request_id }`. Errors: `ERR_INVALID_REQUEST` (malformed JSON), `ERR_UNAUTHORIZED` (missing/unknown tenant on session).

**What lands:** a "New workspace" entry in the existing `WorkspaceSwitcher` dropdown (header) opening a dialog with an optional name field; submit calls a server action wrapping `POST /v1/workspaces`, then switches the active-workspace cookie to the new id and revalidates. `lib/api/workspaces.ts` gains `createTenantWorkspace(token, { name? })`.

**Acceptance:** unit test for the client + server action happy path and the `ERR_UNAUTHORIZED` mapping; WorkspaceSwitcher test asserts the "New workspace" item renders and opens the dialog.

### §12 — Tenant API Keys self-service settings page (absorbs M76_001)

Full plan, contract, failure-mode table, invariants, and test specification are carried verbatim from M76_001 (deleted from `pending/` on absorption). Summary:

**Route:** `/settings/api-keys` under the dashboard shell. **RBAC:** operator/admin only — page guard reads `metadata.role` via the existing `getServerSessionMetadata()` in `lib/auth/server.ts` (consume, do not edit — M74_002 owns `lib/auth/`); `user` role → redirect to `/settings`. Mirrors the `registry.operator()` gating on the backend route.

**Backend contract (verified, unchanged — no new endpoints):**
```
POST   /v1/api-keys        body {key_name, description?}  → 201 {id, key_name, key (raw zmb_t_*, ONCE), created_at}
GET    /v1/api-keys        ?page&page_size&sort           → 200 {items[{id,key_name,active,created_at,last_used_at,revoked_at}], total, page, page_size}
PATCH  /v1/api-keys/{id}   body {active:false}            → 200 {id, active:false, revoked_at}
DELETE /v1/api-keys/{id}   (only when active=false)       → 204
```
Validation: `key_name` `[A-Za-z0-9_-]{1,64}`, `description` ≤256. sort allowlist `created_at|-created_at|key_name|-key_name`; default `-created_at`, page_size 25 (max 100). Error codes: `ERR_APIKEY_NAME_TAKEN`, `ERR_APIKEY_NOT_FOUND`, `ERR_APIKEY_READONLY_FIELD`, `ERR_APIKEY_ALREADY_REVOKED`, `ERR_APIKEY_MUST_REVOKE_FIRST`, `ERR_INVALID_REQUEST`, `ERR_FORBIDDEN`.

**What lands:** server-rendered list with status/created/last-used/revoked columns; "New API key" dialog with one-time raw-secret reveal (copy-to-clipboard, overlay-click locked, single "I've stored it" dismiss); revoke (PATCH `{active:false}`) on active rows; delete (DELETE) on already-revoked rows; a third `SettingsLink` card on `/settings`. The raw key never persists in the DOM after dismiss and is never logged. One Zig **comment-only** edit in `src/http/handlers/api_keys/tenant.zig` (RULE NLR): the "no first-party UI/CLI consumes these routes" block now contradicts shipped reality → point it at `/settings/api-keys`.

**Invariants:** (1) raw key not in DOM after reveal dialog closes; (2) raw key never logged; (3) page unreachable for `user` role; (4) all four mutations re-fetch the list before resolving.

**Acceptance:** the M76_001 Test Specification rows are the floor (settings-card link, role redirect, list, mint-reveal-once, name validation, name-collision keeps-dialog-open, revoke, revoke-already-revoked toast, delete, delete-active toast). E2e round-trip mint→reveal→revoke→delete.

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
| Zombie has zero triggers declared | Edge case from M68 spec | TriggerPanel renders a single "No triggers declared" Card with the bare webhook URL as a fallback ingress (via `CopyUrlFallback` source="none"). The M68 Tabs scaffolding is removed by RULE NLR (touch-it-fix-it) — its only remaining caller was this empty-state branch, and dragging it forward solely for that case is dead-code framing. The "user can still find a webhook URL" intent of the M68 row is preserved. |
| `trigger.source` not in `PROVIDER_GUIDANCE` keyset | Unknown / new provider | TriggerPanel renders `CopyUrlCard` (existing M68 fallback shape). |
| Cron expression unparseable | Bad TRIGGER.md input | CronCard shows the raw expression in the header + a "schedule unparseable — check `TRIGGER.md`" warning line in place of next-fire. |
| `navigator.clipboard.writeText` rejects (insecure context / permission denied) | Browser restricts clipboard access | Fallback to a visible "Copy this command:" prose block with the command selectable; the toast still fires but with prose "Selected — copy manually." |
| `scrollIntoView` no-op (anchor missing because §5 didn't land yet) | §6 lands before §5 | §6 PR must include §5; the dependency is documented. CI test for §6 asserts `#onboarding-flow` exists in the rendered Home page. |
| Bundle-size regression beyond the website's 140 kB landing-js ceiling (`ui/packages/website/.size-limit.json`) | `cron-parser` or other §4 dep | Implementer measures pre-/post-bundle size; if over budget, swap for a smaller cron parser or roll a minimal expression-only formatter. |
| Hero promo pill date drifts from `RATES_DISPLAY.FREE_TRIAL_BANNER` date | Someone edits the pill string without touching the banner (or vice versa) | Both consume a single private `FREE_TRIAL_END_DISPLAY` substring in `rates.ts`. `rates.test.ts` pins the shared substring; drift fails the test. |
| Hero promo pill date drifts from `FREE_TRIAL_END_MS` numeric pin | Someone bumps `FREE_TRIAL_END_MS` (Zig + 3 TS surfaces, audit-cross-tier-rates.sh enforced) but forgets the display string | Out-of-scope automation for now; the rates.ts module-level comment names the coupling, the audit script flags numeric drift, and the human PR review is the catch for the display string until a future spec adds a `FREE_TRIAL_END_MS → display` derivation. |
| §11 create-workspace: empty/blank name submitted | User leaves the name field blank | Backend picks a Heroku-style name (`{}` body path); UI treats blank as "let server name it" — no client-side rejection. |
| §11 create-workspace: `ERR_UNAUTHORIZED` (missing/unknown tenant) | Stale session / unprovisioned Clerk metadata | Server action surfaces a toast "Workspace creation unavailable — refresh and retry"; no cookie switch on failure. |
| §12 name collision | Mint with an existing tenant name | `ERR_APIKEY_NAME_TAKEN` → dialog stays open, name field flagged, no secret minted. |
| §12 network failure during reveal | Mint succeeded server-side, response lost | Recovery message "the key may have been created — refresh the list and revoke if you see an unknown name"; **no auto-retry** (would mint a second key). |
| §12 clipboard blocked | `navigator.clipboard.writeText` refused | Fall back to a selectable read-only field + "Copy failed — select manually"; reveal stays intact. |
| §12 delete-while-active race | Delete clicked on a row still active | `ERR_APIKEY_MUST_REVOKE_FIRST` toast; list refresh shows current state. |
| §12 already-revoked revoke race | Two operators revoke the same key | `ERR_APIKEY_ALREADY_REVOKED` toast; list refresh resolves. |
| §12 non-operator role direct-URL | `user` role hits `/settings/api-keys` | Server component redirects to `/settings`; no API call fires. |
| §12 sort param tampering | URL crafted with `sort=foo` | API `ERR_INVALID_REQUEST` → UI resets to default sort + toast. |

---

## Invariants

1. **No `zombiectl/` file is touched by this spec.** Enforced by RULE NLR (touch-it-fix-it) — anything that asks for CLI changes belongs in M74_002 or a different spec.
2. **No file added or modified by this spec exceeds 350 lines.** Enforced by RULE FLL pre-commit hook. The provider-guidance table splits per-provider if it grows.
3. **The M68 shipped `TriggerPanel` test rows continue to pass** (or their assertions move to the new TriggerPanel.test.ts with equivalent coverage). No regression of M68 acceptance.
4. **No `as any` / `!` / `@ts-expect-error` introduced.** Enforced by `bun run lint` + `bun run typecheck`.
5. **Hero promo pill date string is never hardcoded in `Hero.tsx`.** §7. Pill consumes `RATES_DISPLAY.FREE_TRIAL_PILL` from `rates.ts`. Enforced by code review + rates.test.ts pinning the substring share with `FREE_TRIAL_BANNER`.
6. **`lib/auth/` is consumed, never edited.** §11/§12. Role + token reads go through the pre-existing `getServerSessionMetadata()` / `getServerAuth()`; M74_002 owns that directory.
7. **No new HTTP route appears in `src/http/router.zig` or `route_table.zig`.** §11/§12 consume existing endpoints verbatim; `git diff origin/main -- src/http/router.zig src/http/route_table.zig` stays empty.
8. **§12 raw API key never persists in the DOM after the reveal dialog closes, and is never logged.** Enforced by the dialog's unmount cleanup, an e2e post-close assertion, and a no-`console.log(key/result)` lint in `actions.ts`.
9. **§12 `/settings/api-keys` is unreachable for `user` role** — server-component guard redirects; regression test asserts the redirect.
10. **§12 all four mutations re-fetch the list before resolving** — the server action returns the fresh list payload; no client-only optimistic state that could lie on failure.

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

**§8–§12 post-audit follow-up test rows:**

| Test | Asserts |
|------|---------|
| `test_clerk_appearance_input_distinct_from_card` | `AUTH_APPEARANCE.elements.formFieldInput.backgroundColor !== cardBox.backgroundColor` and `formFieldInput.borderColor` is set (§8 regression pin). |
| `test_dev_route_removed` | `app/(dev)/ds-button-rsc` does not exist; no `ds-button-rsc` reference in `app/` or `vitest.config.ts` (§9). |
| `test_spinner_renders_pulse_status` | `Spinner` renders the WakePulse element with `role="status"` + `aria-busy` (§10). |
| `test_workspace_switcher_new_workspace_item` | WorkspaceSwitcher renders a "New workspace" item that opens the create dialog (§11). |
| `test_create_workspace_action_happy_path` | `createTenantWorkspace` POSTs `{name?}`, returns `{workspace_id,name}`, switches active cookie (§11). |
| `test_create_workspace_unauthorized_maps_toast` | `ERR_UNAUTHORIZED` → toast, no cookie switch (§11). |
| `test_settings_card_links_to_api_keys` | Settings index renders the third card → `/settings/api-keys` (§12). |
| `test_user_role_redirected` | `user`-role principal → redirect to `/settings` (§12). |
| `test_operator_role_lists_keys` | `operator` sees `active`+`revoked` rows ordered `-created_at` default (§12). |
| `test_mint_happy_path_reveals_once` | submit → raw `zmb_t_*` visible once; after close, string no longer in DOM (§12). |
| `test_mint_name_validation_client_side` | invalid `key_name` chars block submit, inline validation (§12). |
| `test_mint_name_collision_keeps_dialog_open` | `ERR_APIKEY_NAME_TAKEN` → dialog stays open, no reveal (§12). |
| `test_revoke_active_key` | active row → revoke → `revoked_at` populated, row inactive (§12). |
| `test_revoke_already_revoked_toast` | `ERR_APIKEY_ALREADY_REVOKED` toast + list refresh (§12). |
| `test_delete_revoked_key` | revoked row → delete → row gone (`204`) (§12). |
| `test_delete_active_key_blocked` | `ERR_APIKEY_MUST_REVOKE_FIRST` toast + refresh (§12). |
| `test_sort_param_invalid_resets` | `sort=foo` → default sort + toast (§12). |
| `test_pagination_bounds` | `page_size=200` rejected client-side before request (§12). |
| `test_e2e_round_trip` (Playwright) | mint → reveal → close → list shows row → revoke → delete → back to original; reveal-secret invariant asserted post-close (§12). |

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

**May 19, 2026 — §7 Option C `/backend` proxy + Redacted + retry hardening dropped, superseded by M74_002 single-token collapse.** A post-merge scope expansion landed on this branch on May 19 ("§7 — API token redaction + retry hardening + Option C /backend proxy", commits `89add737` / `989dda57` / `9e381f2e` / `e8def1ac`). It introduced per-endpoint `/backend/*` route-handler proxies so the browser would never carry the bearer, a `Redacted<string>` wrapper, and an expanded retry layer mirroring `zombiectl/src/lib/http.js`. Two findings forced a reversal:
1. **Build break.** `lib/api/client.ts:serverAuthorizationHeader` dynamically imports `lib/auth/server.ts`, which statically resolves to `@clerk/nextjs/server` and trips Next's `server-only` boundary. `next build` fails on Vercel — `dry-app` / `lint-app` don't catch it because they typecheck rather than bundle. Reproduced locally with `bun run build`.
2. **M74_002 redirected to single-token collapse.** Captain pivoted M74_002 from CLI handshake hardening to the Clerk `sid`-on-custom-template investigation (formerly HANDOFF.md § A.1). If single-token collapse lands, the entire `/backend` BFF + `Redacted` wrapper + `serverAuthorizationHeader` indirection are deleted — the browser carries one cookie-borne JWT and talks to zombied directly.

Rather than fix the `server-only` boundary just to ship code that M74_002 deletes, the §7 commits were dropped from this branch (`git reset --hard b96a153d` then force-push). The hero promo pill §7 (line 242 above) is unaffected and remains DONE. The retry layer reverts to the pre-§7 shape; if M74_002's single-token work needs CLI-parity retries on the dashboard side, it can re-introduce them in its own diff. `tests/e2e/acceptance/events-backfill-proxy.spec.ts` is dropped — the invariant it pinned ("browser carries no bearer") is automatically restored by single-token collapse, where there is no browser-side bearer to leak.

**May 20, 2026 — spec reopened (DONE → IN_PROGRESS) for §8–§12 post-audit follow-ups.** During PR #330 babysitting the Captain asked for a full dashboard route/navigation audit (login → every route, top-right chrome, settings affordances, billing model, Clerk input styling, spinner branding). The audit found five gaps: (§8) Clerk sign-in inputs invisible — card + input both `--surface-2`; (§9) `/ds-button-rsc` public dev route ships to prod; (§10) loaders split between Lucide `Loader2Icon` and the brand pulse; (§11) no create-workspace UI despite a live `POST /v1/workspaces`; (§12) no API-keys self-service UI despite live `operator()`-gated `/v1/api-keys` CRUD. Captain: "fix all of them in this PR — I don't want a separate spec." **M76_001** (Tenant API Keys Settings UI, was PENDING) is absorbed as §12 and deleted from `pending/`; its full plan/contract/failure-modes/invariants/test-spec are the authority for §12. Backend contracts for §11 and §12 were verified against `route_table.zig`, `router.zig`, `workspaces/lifecycle.zig`, `api_keys/{tenant,list}.zig` before reopen — all endpoints already wired, no new HTTP surface introduced. RBAC role read via the pre-existing `getServerSessionMetadata()` so `lib/auth/` (M74_002's) is consumed, not edited.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| App typecheck + lint + test | `(cd ui/packages/app && bun run typecheck && bun run lint && bun test)` | tsc clean · oxlint 0/0 · 47 files / 504 tests | ✅ |
| App coverage thresholds | `(cd ui/packages/app && bun run test:coverage)` | statements 96.05 · branches 90.15 · functions 95.4 · lines 97.32 (gate: 95/90/95/95) | ✅ |
| Website typecheck + lint + test | `(cd ui/packages/website && bun run typecheck && bun run lint && bun test)` | tsc clean · oxlint 0/0 · **19 files / 146 tests** (Hero pill + rates pin coverage included) | ✅ |
| Harness | `make harness-verify` | UFS / DESIGN TOKEN / SPEC TEMPLATE / ERROR REGISTRY / LOGGING / LIFECYCLE / CROSS-TIER RATES / MS-ID+UI — ALL GATES GREEN | ✅ |
| Bundle size (landing js) | `(cd ui/packages/website && bun run size)` | **132.94 kB gzipped** — under the 140 kB ceiling pinned in `ui/packages/website/.size-limit.json` (7.06 kB headroom) | ✅ |
| Bundle size (landing css) | `(cd ui/packages/website && bun run size)` | 9.89 kB gzipped — under the 20 kB ceiling | ✅ |
| No zombiectl edits | `git diff origin/main..HEAD --name-only \| grep -c '^zombiectl/'` | 0 | ✅ |
| Strictness compliance | E6 grep from "Eval Commands" | 0 `as any` / `!` / `@ts-expect-error` introduced | ✅ |
| Cross-tier rates pin (§7 coupling) | `bash scripts/audit-cross-tier-rates.sh` | `FREE_TRIAL_END_MS` numeric value pins across `src/state/tenant_billing.zig` + 3 TS surfaces; display string `FREE_TRIAL_END_DISPLAY` shared between `RATES_DISPLAY.FREE_TRIAL_BANNER` and `RATES_DISPLAY.FREE_TRIAL_PILL` (rates.test.ts pins the substring) | ✅ |

---

## Out of Scope

- **CLI login resilience and polish (D22 / D23 / D28 / D29 / D30)** — absorbed into **M74_002** (CLI Browser Authorization Flow) §6 "Login UX hardening" on May 18, 2026. Originally §1-§5 of this spec.
- **CLI handshake hardening dimensions D20 / D21 / D24 / D25 / D26 / D32** (idempotency check, `--token-name` flag, `/me` ping, argv-leak warning, TTY-priority env resolution, `logout --all` rename) — absorbed into M74_002 §5 on the same date. Originally listed in this spec's Out of Scope as "deferred to the cli-auth handshake hardening sibling spec."
- **§7 Option C `/backend` proxy + Redacted<string> wrapper + retry-layer hardening** — landed on May 19 then dropped before merge. Superseded by M74_002 single-token collapse (the cookie-borne JWT removes the need for a BFF). Investigation memo lives in the Discovery entry above; the four commits (`89add737` / `989dda57` / `9e381f2e` / `e8def1ac`) are preserved on the local backup branch `backup/m71-001-p2-pre-rebase-20260519-234114` and on `origin` until the next force-push, in case M74_002 needs to reference the prior art.
- **`ApiCard.tsx`** (catch-all `POST /v1/zombies/{id}/events` ingress variant from M68 §E5) — lands with the workspace-API-tokens spec, not here.
- **Server-side handshake redesign, `auth_sessions` endpoint shape, token introspection, expiry semantics, revocation** — all in M74_002.
- **PostHog event-schema changes** — no new analytics emits in this spec.
- **`zombiectl/` modifications** — RULE NLR-forbidden in this spec; anything CLI lives in M74_002.
