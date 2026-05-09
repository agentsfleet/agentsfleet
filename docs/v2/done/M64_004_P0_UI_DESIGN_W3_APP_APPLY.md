<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
-->

# M64_004: Apply Operational Restraint to ui/packages/app

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 004
**Date:** May 08, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the authenticated dashboard is the canonical screenshot. Every demo, doc image, social post, and onboarding video starts here. The wake-pulse signature only earns its memorable thing when the dashboard actually pulses on live signals — until W3 lands the brand promise is unrealised.
**Categories:** UI
**Batch:** B3 — depends on W1 (M64_002) DONE; runs after W2 (M64_003) but does not depend on its merge. Stacks on the same `feat/m64-002-design-w1` branch as the bundled milestone PR #308.
**Branch:** feat/m64-002-design-w1 (stacked on W1 + W2)
**Depends on:** M64_002 (token swap + WakePulse + design-system font self-hosting). M64_003 not a hard dependency — both consume W1 in parallel — but W2's component-rebuild patterns are the closest precedent and should be mirrored.

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` — locked direction "Operational Restraint" (memorable thing: "It wakes"). Mockup B is the dashboard canonical shape; the wake-pulse currency rule lives here for real.

---

## Implementing agent — read these first

1. `docs/DESIGN_SYSTEM.md` — full spec, end to end. Sections of particular weight for W3: §Layout (Layout · App / dashboard — 12-col strict, sidebar+main shell, stat-card row, zombie list), §Component principles (borders > shadows, tabular-nums, mono labels/values), §Color (the `--pulse` currency rule and the 5-simultaneous cap), §Motion (wake-pulse is the *only* signature animation; everything else functional or absent; `prefers-reduced-motion` is a hard requirement). DOC READ GATE fires on every `ui/packages/**` edit.
2. `docs/AUTH.md` — mandatory before touching `ui/packages/app/lib/auth/**` (which exports the workspace/auth helpers consumed by every authenticated page). Visual-layer rebuilds of `app/(auth)/sign-in/**` and `app/(auth)/sign-up/**` (Clerk catch-all routes) configure Clerk's `appearance` prop via `lib/clerkAppearance.ts` — no token-minting or session logic changes.
3. `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html` — Mockup B (App dashboard) is the North Star. Read the topbar shape, sidebar nav posture (surface-2 background, mono labels at 12px, active item is a surface-3 fill — never a coloured bar), stat-card row (mono uppercase label + mono display value + mono delta, all tabular-nums), zombie list row (label/state/last-wake/cost columns, dot indicator on the left, single brand-mark pulse in the topbar). Mockup A (marketing hero) — read it once for taste calibration; the dashboard is its operational sibling, not a different system.
4. `ui/packages/design-system/src/index.ts` + `tokens.css` + `theme.css` — W1 surface. The 50+ exports cover ≈95% of W3's primitive needs: `Section`, `Grid`, `Card{Header,Title,…}`, `PageHeader`, `PageTitle`, `Button`, `Input`, `Form`, `Label`, `Badge`, `StatusCard`, `DataTable`, `EmptyState`, `Skeleton`, `Tabs`, `Tooltip`, `Dialog`, `ConfirmDialog`, `DropdownMenu`, `DescriptionList`, `List`, `ListItem`, `Time`, `Terminal`, `InstallBlock`, `Pagination`, `Separator`, `WakePulse`. Anything not on this list is page-shell layout (rebuilt with utilities) or a small in-scope follow-up primitive (see Discovery).
5. `ui/packages/app/components/layout/Shell.tsx` (213L) + `ui/packages/app/app/globals.css` (155L) — current page-shell. Today: `mc-shell` / `mc-header` / `mc-sidebar` / `mc-nav-item` / `mc-content` legacy classes (≈80L of the CSS file) define the topbar+sidebar+main grid. W3 collapses these into design-system primitives + utility classes the same way W2 collapsed `.cta` / `.feature-section` / `.pricing-card` for the website.
6. The W2 spec at `docs/v2/done/M64_003_P1_UI_DESIGN_W2_WEBSITE_APPLY.md` — the closest precedent. Same shape: drop Geist, audit page-by-page, rebuild around design-system primitives + utilities, align tests. Mirror its Failure Modes / Invariants / Eval Commands structure.
7. `ui/packages/app/package.json` — still carries `@fontsource-variable/geist` + `@fontsource-variable/geist-mono` from the W1 carve-out. Fonts arrive transitively via `@usezombie/design-system/tokens.css` (already imported by `app/globals.css`).
8. `core.zombies` schema + `src/http/handlers/zombies/list.zig` (or whatever the GET `/workspaces/{id}/zombies` handler is named in this branch) — the truth function for `live`. The pulse is data-driven: `<WakePulse live={zombie.state === 'live'} />` only. No prop drift, no derived "feels live" booleans.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal. Specifically **NDC** (no dead code at write time), **NLR** (touch-it-fix-it), **NLG** (no legacy framing pre-v2.0), **ORP** (cross-layer orphan sweep across schema/Zig/JS/tests/docs), **TST-NAM** (no milestone IDs in source — extends to component class names, comments, test names).
- `docs/BUN_RULES.md` — TS/TSX file shape, const/import discipline, anti-patterns. Doc-read gate fires per `*.tsx` / `*.ts` edit.
- `docs/DESIGN_SYSTEM.md` — doc-read gate fires per `ui/packages/**` edit.
- `docs/AUTH.md` — doc-read gate fires per `ui/packages/app/lib/auth/**` edit (visual layer of auth flows in scope; token-minting/session logic out of scope).
- File & Function Length Gate — file ≤ 350L, fn ≤ 50L, method ≤ 70L.
- UI Component Substitution Gate — extended to `ui/packages/app/` already (W2 widened the gate). Raw `<section>` / `<button>` / `<input>` / `<dialog>` / `<nav>` / `<header>` / `<form>` / `<article>` / `<table>` / `<aside>` in `*.tsx` is a violation when a primitive exists; use `asChild` for HTML semantics.
- Standard set otherwise.

---

## Anti-Patterns to Avoid (read this BEFORE drafting the spec)

N/A — spec authoring complete; the implementing agent reads sections below as goal contract, not pseudocode.

---

## Overview

**Goal (testable):** Every authenticated page under `ui/packages/app/app/(dashboard)/**` and every auth route under `ui/packages/app/app/(auth)/**` renders against `docs/DESIGN_SYSTEM.md` Mockup B shape — zero `mc-*` legacy classes remain in source, zero Geist references remain in source or `package.json`, the wake-pulse fires only on data-driven live signals (zombie rows where `state === 'live'` + the brand-mark in the topbar) capped at 5 simultaneous in viewport with consolidation beyond, every numeric column renders with tabular-nums, every primary surface uses 12-col grid + borders > shadows, light-mode parity holds, and `qa-app` Playwright smoke passes against the new shape.

**Problem:** W1 swapped the design-system tokens but the app shell still loads `@fontsource-variable/geist`, still ships `mc-shell` / `mc-header` / `mc-sidebar` / `mc-nav-item` legacy class chrome (Shell.tsx 213L wired to ≈80L of pre-Operational-Restraint CSS), still composes its dashboard around shadcn-default radii and shadow stacks rather than the spec's borders+restraint posture. Most critically: today *nothing pulses*. The brand promise — "it wakes" — is words on a page until the dashboard renders a WakePulse on every actually-live zombie row and the topbar brand-mark.

**Solution summary:** Drop Geist from `app/package.json`, rebuild the page shell (`Shell.tsx` + `app/globals.css` `mc-*` block) around design-system primitives + utility classes, audit every dashboard route + auth route for token compliance and pulse-currency, and wire `<WakePulse live={zombie.state === 'live'} />` on the zombie list view as the spec's operational payoff. Apply the strict 12-col grid, mono labels/display values with `tabular-nums`, cyan focus rings (no decorative pulse), restrained status-badge palette (parked → `--text-subtle`, failed → `--error`), and ensure mobile dashboard at 375px renders a designed mobile layout — not a broken desktop grid. Light mode is the polite afterthought per spec but must work end to end.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/package.json` | EDIT | Remove `@fontsource-variable/geist` + `@fontsource-variable/geist-mono` deps. Fonts arrive transitively via design-system. |
| `ui/packages/app/app/globals.css` | REWRITE | ≈155L → ≈80-120L. Drop the `mc-*` block (`.mc-shell`, `.mc-header`, `.mc-sidebar`, `.mc-content`, `.mc-nav-section`, `.mc-nav-label`, `.mc-nav-item`, `.mc-nav-item:hover`, `.mc-nav-item.active`). Keep only the pieces utilities can't express: `@import "@usezombie/design-system/tokens.css"` + `theme.css`, `body` dot-grid bg with light-mode mirror, `prefers-reduced-motion` overrides, any `@layer base` Tailwind escape hatches. Zero pre-W1 class chrome. |
| `ui/packages/app/components/layout/Shell.tsx` | REBUILD | Mockup B page shell: design-system `<Section asChild><header>`-style topbar with brand-mark `<WakePulse live size={12}>` + workspace switcher + user menu; sidebar nav as a column of mono items, active item rendered with `data-active` + surface-3 fill; main content area a `<Section>` per page. ≤200L; if it grows, factor sidebar/topbar/userMenu into co-located components. |
| `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` | EDIT | Token swap + design-system `<DropdownMenu>` if not already; mono labels. |
| `ui/packages/app/app/(dashboard)/page.tsx` | REBUILD | Workspace landing → Mockup B stat-card row (live count, wakes-24h, evidence count, cost) using `<StatusCard>` (or new `<Stat>` primitive — see Discovery). All numbers tabular-nums. |
| `ui/packages/app/app/(dashboard)/layout.tsx` | EDIT | Wire rebuilt `Shell` if shape changed; otherwise minimal token sweep. |
| `ui/packages/app/app/(dashboard)/zombies/page.tsx` + `loading.tsx` | REBUILD | The zombie list view — Mockup B's canonical row pattern. Each row: dot indicator (live → `<WakePulse live size={6}>`, parked → static dot in `--text-subtle`, failed → static dot in `--error`), label, last-wake `<Time>`, cost-24h tabular-nums. Strict 12-col grid. Loading skeleton uses `<Skeleton>` bars in `--border` — never a fake live indicator. Empty state via `<EmptyState>`. |
| `ui/packages/app/app/(dashboard)/zombies/components/ZombiesList.tsx` | REBUILD | The pulse currency lives here. `<WakePulse live={zombie.state === 'live'} />` per row, with the visible-viewport cap (≤5 concurrent pulses; rows beyond fall back to static glow). Header reads "N live" with a single brand-pulse next to the count when consolidation is active. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/page.tsx` + `components/ZombieConfig.tsx` / `KillSwitch.tsx` / `TriggerPanel.tsx` | EDIT | Detail/steer view — `<PageHeader>` + tabs (Config / Steer / Trigger). Mono operational data via `<DescriptionList>`. KillSwitch is a destructive `<Button variant="destructive">` with a `<ConfirmDialog>`. Trigger panel uses `<Form>` + `<Input>` + `<Button>`. Status indicator at top renders the pulse if live. |
| `ui/packages/app/app/(dashboard)/events/page.tsx` | EDIT | Event log / replay view — mono `<DataTable>` (timestamp, type, zombie, payload preview), tabular-nums on timestamp, expand-row uses `<Dialog>` for full payload. No `--pulse` here — events are historical, not live. |
| `ui/packages/app/app/(dashboard)/approvals/**` (`page.tsx`, `[gateId]/page.tsx`, `ResolveButtons.tsx`, `components/ApprovalsList.tsx`) | EDIT | Approval inbox — `<DataTable>` for the list, `<Card>` for individual gate view, `<Button>` pair (Approve / Deny) with `<ConfirmDialog>` on each. Pending state is *not* live — no pulse. |
| `ui/packages/app/app/(dashboard)/credentials/**` (`page.tsx`, `components/CredentialsList.tsx`, `components/AddCredentialForm.tsx`) | EDIT | Vault view — `<DataTable>` for credential list (name, type, last-used `<Time>`), `<Form>` for add. Credential values never rendered (already enforced by handler; spec just preserves the ban visually — no clipboard-icon affordance for secret values). |
| `ui/packages/app/app/(dashboard)/settings/**` (`page.tsx`, `provider/page.tsx`, `provider/components/*`, `billing/page.tsx`, `billing/components/*`, `billing/lib/*`) | EDIT | Settings — `<Tabs>` for sub-pages (Workspace / Provider / Billing). `BillingUsageTab` + `BillingBalanceCard` keep their data shape, swap to design-system primitives + tabular-nums. `ProviderSelector` / `ModeRadio` / `ByokFields` use `<Form>` + `<Label>` + `<Input>` + `<Select>`. |
| `ui/packages/app/app/(auth)/sign-in/[[...sign-in]]/page.tsx` + `(auth)/sign-up/[[...sign-up]]/page.tsx` + `(auth)/layout.tsx` | EDIT | Clerk catch-all routes — visual layer only. Auth-shell layout: centred `<Card>` on dot-grid bg, brand-mark `<WakePulse live>` in the corner (the brand is always alive). Configure Clerk via `lib/clerkAppearance.ts`. |
| `ui/packages/app/lib/clerkAppearance.ts` | REBUILD | Clerk `appearance` config wired to design-system tokens (Commit Mono for buttons/labels, Instrument Sans for body, `--surface-2` cards, `--pulse` for primary CTA, `--error` for error states). No `--pulse` on focus rings — focus uses `--pulse-glow` (or Clerk's `.cl-formButtonPrimary:focus` mapped to it). No `--pulse` on signed-out states. |
| `ui/packages/app/app/(dev)/ds-button-rsc/page.tsx` | EDIT (or DELETE if gallery moved to design-system) | Dev-route token-sweep; if it's a dead gallery surface duplicated by design-system, delete per RULE NDC. |
| `ui/packages/app/components/domain/**` | EDIT | Token sweep + raw-HTML → primitive substitution. UI Substitution Gate fires per file. |
| `ui/packages/app/components/analytics/**` | EDIT | Visual-layer-only audit; analytics literals (e.g. `mode: "humans"`) preserved per the W1+W2 inline-callsite precedent. |
| `ui/packages/app/tests/e2e/*.spec.ts` | EDIT | Update assertions to new shape (mono `<h1>`, `<WakePulse>` only on live rows, no `mc-*` selectors, dot-grid bg present, no `linear-gradient` on CTAs). |
| `ui/packages/app/tests/*.test.ts` + per-component `.test.tsx` files | EDIT | Re-align unit-test class-string + computed-style assertions to rebuilt shape. |
| `ui/packages/app/tests/e2e/wake-pulse.spec.ts` | CREATE | New Playwright spec asserting the pulse fires only on `live` rows and consolidates above 5 visible. |
| `ui/packages/app/tests/e2e/mobile-dashboard.spec.ts` | CREATE | New Playwright spec at 375px viewport asserting the dashboard renders a designed mobile layout (not a broken desktop grid). |
| `docs/v2/pending/M64_004_P0_UI_DESIGN_W3_APP_APPLY.md` | CREATE | This spec. CHORE(open) moves it to `active/`. |

> **Anti-pattern caveat**: this table lists every directory expected to be touched as a *role*. The implementing agent reads each file before editing and may discover sub-files not enumerated here (e.g., a test file inside a component directory). Adding to scope is permitted under the same role; opportunistic refactors outside the dashboard/auth surface require explicit user override.

---

## Sections (implementation slices)

### §1 — Font + dependency surface

Remove `@fontsource-variable/geist` and `@fontsource-variable/geist-mono` from `ui/packages/app/package.json`. Run `bun install` so the lockfile drops the entries. Verify `bun pm ls --filter usezombie-app | grep -i geist` returns no rows. Fonts arrive transitively via the existing `@import "@usezombie/design-system/tokens.css"` in `app/globals.css`.

**Implementation default:** the app does not add its own fontsource imports. All font loading is design-system's responsibility (W1).

### §2 — `globals.css` rewrite + `mc-*` teardown

Drop the `mc-*` block (`.mc-shell` … `.mc-nav-item.active`) wholesale. Author a fresh `globals.css` containing only:
- `@import "@usezombie/design-system/tokens.css"` + `theme.css` (preserved).
- `@tailwindcss/postcss` directive (preserved).
- The body dot-grid background (radial-gradient at 8% opacity, fixed; light-mode mirror under `[data-theme="light"]`).
- `@media (prefers-reduced-motion: reduce)` overrides — wake-pulse becomes a static glow, no other motion in scope.
- Any minimal `@layer base` escape hatches Next.js / Clerk needs for proper SSR token resolution.

No `.mc-*`, no orange-era classes, no `linear-gradient` outside the dot-grid radial, no `border-radius: 9999px`.

**Implementation default:** keep the file ≤ 350L; aim for ≈100L. If it drifts higher, the surplus is page-chrome that should be utility classes on JSX, not CSS.

### §3 — Page shell (Shell.tsx) rebuild

`Shell.tsx` ships the Mockup B chrome:
- **Topbar**: `<WakePulse live size={12}>` brand-mark + Commit Mono "usezombie" wordmark + workspace switcher + user menu (Clerk's `<UserButton>` configured via `clerkAppearance`). 1px bottom border in `--border`. No coloured accent strip.
- **Sidebar**: surface-2 background, mono nav items at 12px in `--text-muted`. Active item: `data-active` + surface-3 fill, **never a coloured bar or `--pulse` accent**. Sidebar grouped by section labels (Operations / Settings) per Mockup B.
- **Main**: `<main>` slot rendering the routed page; respects 12-col grid via design-system `<Grid>` consumed by each page.

Drop `<div className="mc-shell">` wrapper and the legacy class chrome. Use design-system primitives + Tailwind utilities. Mobile (375px): sidebar collapses to a top-anchored hamburger sheet (design-system `<Dialog>` with side-anchor); topbar shrinks to brand-mark + user menu.

**Implementation default:** dark is canonical; light mode works via OS preference; no in-page toggle this spec (Mockup A's `⌘ toggle mode` button stays out of scope until DESIGN_SYSTEM.md commits to the affordance — same deferral as W2).

### §4 — Workspace dashboard (`app/(dashboard)/page.tsx`)

Mockup B canonical landing:
- `<PageHeader>` with workspace name + small action group on the right (Invite / Settings link).
- Stat-card row: 4 stats — **LIVE NOW** (count, with single brand-pulse), **WAKES · 24H** (count + delta), **EVIDENCE** (count + delta), **COST · 24H** (currency + delta). Each renders mono uppercase label + mono display value (clamp-sized to spec's `display-md` 28px) + mono delta (tabular-nums; sign coloured only when delta is informational — no decorative `--pulse`).
- Below: a condensed `<ZombiesList>` preview (top N rows) + a "view all" link, OR a recent-events strip — pick one per Mockup B and document in Discovery.

Numbers everywhere render with `tabular-nums`. Delta arrows are mono glyphs (▲/▼), never coloured `--pulse` accents.

### §5 — Zombie list (`app/(dashboard)/zombies/page.tsx` + `ZombiesList.tsx`)

The operational payoff. Each row in a strict 12-col grid:
- Col 1 (dot): live → `<WakePulse live size={6}>`; parked → static `<span>` dot at `--text-subtle`; failed → static dot at `--error`. The `live` prop is **data-driven** off `zombie.state === 'live'` only — no derived booleans, no "feels live" signals.
- Cols 2–4 (label + last-wake `<Time>`): left-aligned, mono.
- Cols 5–9 (state + recent activity): mono badges via `<Badge>`; restrained palette per spec (live → no badge, the pulse is the signal; parked → muted; failed → `--error` badge with error code).
- Cols 10–12 (cost-24h, wakes-24h): right-aligned tabular-nums.

**Pulse currency cap:** when more than 5 rows with `live === true` are in the viewport, rows beyond the first 5 render with a static glow (no animation) and the list header reads "N live" with a single brand-mark `<WakePulse live>` next to the count. Implementation default: use `IntersectionObserver` (or Mockup B's documented "first-5-in-DOM-order" heuristic — pick one and document in Discovery). Reduced-motion: every pulse becomes a static glow regardless of count.

Loading state: `<Skeleton>` bars in `--border`. **Never a fake live indicator on a skeleton row.**

Empty state: `<EmptyState>` with a "→ install your first zombie" `<Button>` linking to `/zombies/new`.

### §6 — Zombie detail / steer (`zombies/[id]/**`)

Tabs (`<Tabs>`) for Config / Steer / Trigger / Events. Top-of-page status row: pulse + state label + last-wake `<Time>`. `<DescriptionList>` for operational metadata (created, owner, hooks, vault refs as opaque names). `<KillSwitch>` is a destructive `<Button variant="destructive">` behind a `<ConfirmDialog>` ("This stops the daemon. Type `kill` to confirm."). `<TriggerPanel>` is a `<Form>` with `<Input>` for the event payload + `<Button>` to fire — mono input values per spec.

### §7 — Events / Approvals / Credentials / Settings

- **Events**: `<DataTable>` with mono columns (ts, type, zombie, payload preview). Expand uses `<Dialog>` rendering full payload in a `<Terminal>`-styled mono block. No pulse — historical data.
- **Approvals**: list via `<DataTable>`, individual gate via `<Card>` with Approve / Deny pair behind `<ConfirmDialog>`. Pending ≠ live; no pulse.
- **Credentials**: `<DataTable>` (name, type, last-used `<Time>`); add via `<Form>`. Credential values **never** rendered — this spec preserves that ban visually (no copy/reveal affordance).
- **Settings**: top-level `<Tabs>` (Workspace / Provider / Billing). Provider page uses `<Form>` + `<Select>` + `<Input>`; ByokFields preserves its existing data shape, swaps chrome. Billing renders `<StatusCard>` for balance + `<DataTable>` for usage charges; all currency values tabular-nums.

### §8 — Auth flows (`app/(auth)/**`)

Clerk's catch-all routes get a calm auth shell: centred `<Card>` on the dot-grid bg, brand-mark `<WakePulse live size={12}>` in the corner (the brand is always alive). The page itself feels *quiet* — auth is not where the metaphor goes loud. No additional pulses, no decorative motion.

`lib/clerkAppearance.ts` wires Clerk's `appearance` prop to design-system tokens:
- `formButtonPrimary` → `--pulse` background, `--surface-1` text.
- `formButtonPrimary:hover` → `--pulse-hover`.
- `formButtonPrimary:focus` → `--pulse-glow` ring (no decorative pulse on focus).
- `formField` → `--surface-2` bg, `--border`, mono input value.
- Error states → `--error`, **never** `--pulse`. (Failed ≠ live.)
- Card → `--surface-1` bg, 1px `--border`, no shadow.

Read `docs/AUTH.md` before touching `lib/auth/{client,server}.ts`. The token-minting / session logic stays out of scope; only `lib/clerkAppearance.ts` and the page-level chrome change.

### §9 — Mobile dashboard (375px)

Desktop 12-col grid does not survive at 375px. Design + ship the mobile layout:
- Topbar: brand-mark + user menu only; workspace switcher behind a dropdown.
- Sidebar: collapses to a top-anchored sheet (`<Dialog>` with side-anchor) triggered by a hamburger.
- Stat row: 2×2 grid on mobile (≤768px), 4×1 on desktop (≥1024px).
- Zombie list: rows collapse to 2-line layout — line 1 (dot + label + state badge), line 2 (last-wake + cost-24h, tabular-nums). Pulse cap still applies.
- Detail tabs: horizontally scrollable.

Document the breakpoints in `globals.css` (or a co-located `.module.css` if it stays small) and assert via Playwright at 375px.

### §10 — Test alignment + new specs

- Update existing per-component `.test.tsx` and `tests/*.test.ts` for the rebuilt shape.
- Update existing `tests/e2e/*.spec.ts` smoke specs for the new selectors (no `mc-*`, mono `<h1>`, dot-grid bg present).
- New `tests/e2e/wake-pulse.spec.ts` — render the dashboard with a fixture of 7 zombies (5 live, 1 parked, 1 failed); assert exactly 5 animating pulses + 1 brand-mark pulse + 1 consolidation pulse on the list header (= 7 total, of which 6 are decorative-cap consolidation candidates and the 7th is brand). Then add a 6th live zombie via fixture; assert the 6th renders with a static glow and the consolidation count reads "6 live".
- New `tests/e2e/mobile-dashboard.spec.ts` — at 375px viewport: assert the sidebar is hidden, hamburger is visible, stat row is 2×2, zombie row is 2-line.
- Reduced-motion test (vitest): assert that with `prefers-reduced-motion: reduce`, `<WakePulse live>` renders without `animation-name: pulse` (uses static glow shadow only).

RULE TST-NAM: no `M64_004` / `§9` / `T7` / `dim 5.2` in any test name, file name, or comment.

---

## Interfaces

No HTTP / RPC surface changes. Public component interfaces preserved or extended:

- `<Shell />` — page-shell composition; props preserved.
- `<ZombiesList zombies={...} />` — input shape preserved (consumes the existing list-handler response). Behaviour change: each row reads `zombie.state` to drive the `<WakePulse live={...}>` prop.
- `<KillSwitch zombieId={...} />`, `<TriggerPanel zombieId={...} />`, `<ZombieConfig zombie={...} />` — props preserved; chrome rebuilt.
- `lib/clerkAppearance.ts` — exports `appearance: Appearance` (Clerk type). Re-exported into `app/layout.tsx`'s `<ClerkProvider>` (existing wiring; only the object contents change).

A new internal hook may emerge in §5: `useVisiblePulseCap(rows, max=5)` returning a boolean `shouldPulse` per row. Implementation choice (IntersectionObserver vs DOM-order heuristic) documented in Discovery.

The design-system surface (W1) is **read-only** for W3. New primitives needed mid-stream pause work and consult the user — do not modify `ui/packages/design-system/**` without explicit override (Discovery captures the consult).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Geist still loads | Browser cache or missed import | `bun pm ls --filter usezombie-app \| grep -i geist` returns 0 rows; CI verified. |
| `mc-*` class leaks back | Author copy-pastes pre-W3 snippet | `grep -rn 'mc-shell\|mc-header\|mc-sidebar\|mc-nav-' ui/packages/app/` returns 0 hits at HARNESS VERIFY. |
| `--pulse` used decoratively | Author drops it on a non-live element, hover, or focus ring | Pre-PR grep: every `var(--pulse)` / `#5EEAD4` / `--pulse-glow` callsite is either (a) a `<WakePulse live={...}>` whose `live` is data-driven, (b) the brand-mark in the topbar, (c) a focus ring using `--pulse-glow` (allowed). Anything else = violation. Discovery captures every callsite at CHORE(close). |
| Pulse fires on skeleton/loading row | Loading state misuses live indicator | Skeleton rows render `<Skeleton>` only — no `<WakePulse>` import in any `loading.tsx`. Asserted in `tests/e2e/wake-pulse.spec.ts`. |
| Pulse fires on parked/failed row | Author bypasses `state === 'live'` check | `live` prop value computed inline at the JSX call site as `zombie.state === 'live'` (single source of truth). Test fixture covers parked + failed cases. |
| Pulse cap not enforced | Viewport with N>5 live zombies animates all N | `tests/e2e/wake-pulse.spec.ts` fixture renders 6 live zombies, asserts only 5 animate + consolidation-count is shown. |
| 60fps drop on dashboard with N live zombies | Box-shadow animation multiplied | Cap at 5 (above) handles the common case; documented in Discovery if a stricter cap is needed after Chrome DevTools profiling. |
| Reduced-motion ignored | Author uses `animation` directly | Every motion site routes through `<WakePulse>`; the primitive owns reduced-motion handling. Vitest asserts the static-glow path. |
| Mobile dashboard broken | Desktop 12-col grid leaks to 375px | `tests/e2e/mobile-dashboard.spec.ts` asserts the designed mobile layout. |
| Buttons go pill | `rounded-full` survives a copy-paste | `grep -rn 'rounded-full\|border-radius: 9999' ui/packages/app/` returns 0 hits. |
| Aurora gradient sneaks in | Mistaken decoration | `grep -rn 'linear-gradient\|radial-gradient' ui/packages/app/` audited; every hit is the dot-grid radial OR a single-stop overlay. Multi-stop forbidden. |
| Auth visual rebuild touches token-minting | Clerk appearance change strays into `lib/auth/{client,server}.ts` logic | Spec scope: `lib/clerkAppearance.ts` only. Any edit to `lib/auth/{client,server}.ts` requires `docs/AUTH.md` re-read + explicit user override. |
| Test asserts old `mc-*` shape | Smoke spec runs against new shell and fails | §10 alignment captured in same commit; `qa-app` green is acceptance criterion. |

---

## Invariants

1. **Zero `mc-*` legacy classes in `ui/packages/app/`.** Enforced by `grep -rn 'mc-shell\|mc-header\|mc-sidebar\|mc-content\|mc-nav-' ui/packages/app/` returning 0 in CI's `qa-app` job.
2. **Zero Geist in `ui/packages/app/`.** Enforced by `grep -rn -i geist ui/packages/app/ --include="*.json" --include="*.css" --include="*.tsx" --include="*.ts"` returning 0 non-historical matches; `bun pm ls --filter usezombie-app | grep -i geist` returns 0 rows.
3. **`--pulse` only on data-driven live signals OR the brand-mark OR `--pulse-glow` focus rings.** Enforced by code-review grep at CHORE(close); every callsite triaged in Discovery. The `<WakePulse>` primitive owns the `live` prop; rendering is data-driven and cannot accidentally animate non-live entities (W1 invariant carries through).
4. **No `border-radius: 9999px` on buttons.** Enforced by `grep -rn 'rounded-full\|border-radius: 9999' ui/packages/app/` returning 0.
5. **Tabular-nums on every numeric column.** Enforced by code-review of every `<DataTable>` numeric column + every stat-card value (Tailwind `tabular-nums` or `font-feature-settings: "tnum"`). Vitest asserts on stat-card rendered class strings.
6. **Pulse cap (≤5 simultaneous in viewport).** Enforced by `useVisiblePulseCap` implementation; asserted in `tests/e2e/wake-pulse.spec.ts`.
7. **`prefers-reduced-motion` honoured.** Enforced by `<WakePulse>` primitive (W1) + a vitest assertion that the static-glow path renders when the media query matches.
8. **File length gate.** Every file ≤ 350L; functions ≤ 50L; methods ≤ 70L. Enforced by File & Function Length Gate.
9. **Design-system package read-only.** Enforced by Files Changed scope — `ui/packages/design-system/**` is not in the table; edits there require explicit override + Discovery entry.
10. **No raw HTML elements where a primitive exists.** Enforced by UI Component Substitution Gate per `*.tsx` edit.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `app-smoke / dashboard / mono headline` | `getComputedStyle(h1).fontFamily` contains "Commit Mono" on the dashboard landing. |
| `app-smoke / dashboard / dot-grid bg` | `getComputedStyle(body).backgroundImage` contains "radial-gradient". |
| `app-smoke / topbar / brand-mark pulses` | The topbar's `<WakePulse>` element resolves `animation-name` to "pulse". |
| `app-smoke / sidebar / no coloured active bar` | Active sidebar item resolves to a surface-3 fill — no `--pulse` border, no coloured stripe. |
| `wake-pulse / live row pulses` | A row whose fixture has `state === 'live'` resolves `<WakePulse>` `animation-name` to "pulse". |
| `wake-pulse / parked row static` | A row whose fixture has `state === 'parked'` renders the static dot at `--text-subtle`, no animation. |
| `wake-pulse / failed row error` | A row whose fixture has `state === 'failed'` renders the static dot at `--error`, no animation. |
| `wake-pulse / cap consolidation` | With 6 live rows in viewport, exactly 5 animate; the 6th renders static glow; list header reads "6 live" with a single brand-mark pulse. |
| `wake-pulse / loading skeleton has no pulse` | Loading state DOM contains zero `<WakePulse>` elements. |
| `wake-pulse / reduced motion → static glow` | Under `prefers-reduced-motion: reduce`, every `<WakePulse live>` renders without `animation-name`; box-shadow glow only. |
| `mobile-dashboard / sidebar hidden at 375px` | At 375px, sidebar is `display: none` (or off-screen); hamburger is visible. |
| `mobile-dashboard / stat row 2×2 at 375px` | Stat row resolves to a 2-column grid at 375px, 4-column at ≥1024px. |
| `mobile-dashboard / zombie row 2-line at 375px` | Each zombie row stacks into 2 lines at 375px. |
| `auth / sign-in chrome` | Sign-in page renders the centred `<Card>` shape; brand-mark pulse present in corner; `formButtonPrimary` resolves background to `var(--pulse)`. |
| `auth / error state uses --error not --pulse` | Triggered error state in the form resolves the error message colour to `var(--error)`; no `--pulse` references in error subtree. |
| `stat-card / tabular-nums` | Every stat-card value element class string contains `tabular-nums` (or computed `font-feature-settings` contains "tnum"). |
| `data-table / numeric columns tabular-nums` | Every numeric `<DataTable>` column has tabular-nums on the cell. |
| `qa-app-smoke / routes render` | `/`, `/zombies`, `/zombies/[id]`, `/events`, `/approvals`, `/credentials`, `/settings`, `/sign-in`, `/sign-up` each return 200 + render `<main>` (auth routes return 200 with the auth shell). |

---

## Acceptance Criteria

- [ ] `grep -rn 'mc-shell\|mc-header\|mc-sidebar\|mc-content\|mc-nav-' ui/packages/app/` returns 0 matches.
- [ ] `grep -rn -i geist ui/packages/app/ --include="*.json" --include="*.css" --include="*.tsx" --include="*.ts"` returns 0 non-historical matches.
- [ ] `bun pm ls --filter usezombie-app | grep -i geist` returns 0 rows.
- [ ] `grep -rn 'rounded-full\|border-radius: 9999' ui/packages/app/` returns 0 matches.
- [ ] `grep -rn 'linear-gradient\|radial-gradient' ui/packages/app/` returns only the dot-grid radial + audited single-stop overlays.
- [ ] Every `var(--pulse)` / `#5EEAD4` / `--pulse-glow` callsite in `ui/packages/app/` is dispositioned in Discovery (data-driven live, brand-mark, or focus ring).
- [ ] `bun --filter usezombie-app run test` passes.
- [ ] `bun --filter usezombie-app run test:e2e` passes (incl. new wake-pulse + mobile specs).
- [ ] `bun --filter usezombie-app run lint` passes.
- [ ] `bun --filter usezombie-app run build` produces a successful Next.js build.
- [ ] `qa-app` and `qa-app-smoke` CI jobs pass.
- [ ] `make lint` clean.
- [ ] No file over 350 lines added or edited.
- [ ] Lighthouse on a deployed preview's `/zombies` (authed): LCP < 2s.
- [ ] PR description includes 4-6 screenshots (dashboard / zombies-list / detail / mobile-dashboard / sign-in — dark canonical; one light-mode parity shot).
- [ ] `gitleaks detect` clean.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Zero mc-* legacy classes in app source
grep -rn 'mc-shell\|mc-header\|mc-sidebar\|mc-content\|mc-nav-' ui/packages/app/ && echo "FAIL" || echo "PASS"

# E2: Zero Geist in app package
grep -rn -i geist ui/packages/app/ --include="*.json" --include="*.css" --include="*.tsx" --include="*.ts" && echo "FAIL" || echo "PASS"

# E3: No pill buttons
grep -rn 'rounded-full\|border-radius: 9999' ui/packages/app/ && echo "FAIL" || echo "PASS"

# E4: No multi-stop gradients
grep -rn 'linear-gradient' ui/packages/app/ && echo "audit each hit" || echo "PASS"

# E5: Build
bun --filter usezombie-app run build 2>&1 | tail -5

# E6: Unit tests
bun --filter usezombie-app run test 2>&1 | tail -5

# E7: E2E (incl. new wake-pulse + mobile specs)
bun --filter usezombie-app run test:e2e 2>&1 | tail -10

# E8: Lint
bun --filter usezombie-app run lint 2>&1 | tail -3

# E9: 350-line gate
git diff --name-only origin/main -- ui/packages/app/ | grep -v '\.md$' | xargs -I{} sh -c 'wc -l "{}"' | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E10: Pulse currency audit (manual disposition required)
grep -rn 'var(--pulse)\|#5EEAD4\|--pulse-glow' ui/packages/app/ | tee /tmp/w3-pulse-audit.txt
echo "Disposition each line in PR Discovery."

# E11: Gitleaks
gitleaks detect --redact 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Files deleted:**

| File to delete | Verify deleted |
|----------------|----------------|
| `ui/packages/app/app/(dev)/ds-button-rsc/page.tsx` (only if confirmed dead per RULE NDC) | `test ! -f ui/packages/app/app/(dev)/ds-button-rsc/page.tsx` |

> **Note**: Most deletions in W3 are CSS rules (the `mc-*` block in `globals.css`) and dead component files exposed by audit, not yet enumerable. The implementing agent runs the orphan sweep below at HARNESS VERIFY and adds rows here in the same commit as the deletion.

**2. Orphaned references:**

| Deleted symbol or import | Grep command | Expected |
|--------------------------|--------------|----------|
| `mc-shell` / `mc-header` / `mc-sidebar` / `mc-content` / `mc-nav-*` | `grep -rn 'mc-shell\|mc-header\|mc-sidebar\|mc-content\|mc-nav-' ui/packages/app/` | 0 matches |
| Geist font ref | `grep -rn -i geist ui/packages/app/ --include="*.ts" --include="*.tsx" --include="*.css" --include="*.json"` | 0 matches |
| Pre-W3 class chrome (whatever the per-component audit surfaces) | `grep -rn '"<class>"' ui/packages/app/` per row | 0 matches |

---

## Discovery (consult log)

- **Stat-card primitive vs `<StatusCard>`.** Resolved — existing `<StatusCard>` (W1 surface) was a fit: mono uppercase label, tabular-nums display value, hover/focus already wired to `--primary`. No new `<Stat>` primitive needed; design-system stays read-only.
- **Dashboard landing composition.** Resolved — kept the existing 4-stat row + `RecentActivity` event-strip below. Added a third path: when `zombies.length === 0`, replace the stat row with `<FirstInstallCard>` rendering an `<InstallBlock>` with `zombiectl install --from <skill>` + a docs CTA. Day-zero UX gap (operator with $5 balance and no zombies sees nothing actionable) closed.
- **Dot-grid bg on app body.** Resolved — DESIGN_SYSTEM.md §Aesthetic permits the dot-grid on the marketing hero only. Initially leaked into `app/globals.css` body, then stripped. App body stays flat `--bg`.
- **`<Time>` primitive vs hand-rolled formatRelative.** Resolved — the design-system already ships a `<Time value={...} format="relative">` primitive that does Invalid-Date soft-fail and ISO `dateTime` attribute. UI Substitution Gate violation caught at /review; swapped to the primitive. Tooltip disabled (`tooltip={false}`) on list rows because dense rows × tooltip context = noisy; absolute time is one click into the row.
- **shadcn idiomatic className over arbitrary-value escapes.** Initially used `bg-[color:var(--surface-2)]` etc.; W1's `theme.css` `@theme inline` block exposes these as first-class utilities (`bg-muted`, `bg-pulse`, `border-border`, etc.). Migrated all callsites to the canonical utility names per shadcn theming guide. The W1 docstring explicitly forbids `var(--*)` arbitrary values in component files.
- **ZOMBIE_STATUS const consolidation.** Resolved in-scope. Status string literals were scattered across page.tsx / ZombiesList / KillSwitch / detail. Consolidated to `ZOMBIE_STATUS = { ACTIVE, PAUSED, STOPPED, KILLED, ERRORED } as const`. `setZombieStatus` takes the narrower `ZombieStatusSettable = "active" | "stopped" | "killed"` (PATCH-accepted subset). `Zombie.status` stays loose `string` for forward-compat; consumers narrow with `ZOMBIE_STATUS`.
- **ESLint Tier A + Tier B.** Hardened the rule set: `no-explicit-any: error`, `consistent-type-imports`, `no-non-null-assertion` (source only), `prefer-as-const`, plus typed-lint Tier B (`no-floating-promises`, `no-misused-promises`, `await-thenable`, `prefer-nullish-coalescing`, `prefer-optional-chain`, `no-unnecessary-type-assertion`, `switch-exhaustiveness-check`, `restrict-template-expressions`) + `react/no-unstable-nested-components`, `react/jsx-no-leaked-render`. parserOptions.project added for type info. Fixed every violation in the same diff (no `any` left in source; misused-promises wrapped via `(e) => { void form.handleSubmit(onSubmit)(e); }`).
- **Pulse cap implementation.** `IntersectionObserver` vs DOM-order heuristic. Default: DOM-order (first 5 live rows in render order pulse; the rest get static glow + the consolidation count). Switch to IntersectionObserver only if DOM-order produces visibly wrong behaviour on long lists with virtualisation.
- **Pulse cap implementation.** `IntersectionObserver` vs DOM-order heuristic. Default: DOM-order (first 5 live rows in render order pulse; the rest get static glow + the consolidation count). Switch to IntersectionObserver only if DOM-order produces visibly wrong behaviour on long lists with virtualisation.
- **Marketing-display typography from W2 carryover.** `<DisplayTitle>`/`<DisplayHeading>`/`<DisplaySubhead>` were tracked as a follow-up in W2's Discovery. They are likely **not** needed in W3 — dashboard typography uses `<PageTitle>` (already shipping in design-system) for H1 and stat-card display values are clamp-sized inline. If any dashboard surface wants display-scale headings, treat it as an out-of-spec deviation and consult.
- **Auth shell + Clerk appearance.** First time the spec wires a third-party (Clerk) component to design-system tokens. Document any token gaps surfaced (e.g., a Clerk surface that needs a token we don't have) here.
- **Mobile breakpoints.** DESIGN_SYSTEM.md does not currently commit to specific breakpoints. Implementing agent picks `sm: 640px / md: 768px / lg: 1024px / xl: 1280px` (Tailwind defaults) and documents here; if a sharper cutover is needed for the dashboard, capture it.

- **Coverage gap (functions).** Statements 96.02% ✅ / Branches 90.56% ✅ / Lines 96.93% ✅ / Functions 91.35% 🔴 (target 95%). Drag is from `Shell.tsx` (79% functions) — the extracted `MobileNav` / `SidebarNav` / `NavItem` / `NavGroup` sub-components have no direct unit tests; the existing `Shell({ children })` static-render test only walks the top-level tree. Captured in `M64_005` follow-up.
- **`@next/eslint-plugin-next` + `eslint-plugin-jsx-a11y` integration (in-scope).** Both plugins added to `eslint.config.js` with `recommended` + `core-web-vitals` rule sets. Surfaced 4 real violations, all fixed: `tabIndex` on non-interactive span (BillingBalanceCard — disabled-button-tooltip pattern; suppressed with justified comment), unassociated `<label>` (TriggerPanel — replaced with `<span>` since the value is a display, not a form control), redundant `role="list"` on `<ul>` (ZombiesList — removed), raw `<a href="/">` for internal nav (ds-button-rsc dev fixture — load-bearing for the Button asChild RSC test, suppressed with justified comment).
- **Raw `<input type="radio">` in ModeRadio.** design-system does not yet ship a `<RadioGroup>` primitive. Out of W3 scope (design-system is read-only). Follow-up: add `<RadioGroup>` to `ui/packages/design-system/src/design-system/` mirroring shadcn's pattern.
- **Deferred follow-ups (M64_005 spec).** Function-coverage gap (91.35% vs 95% target — drag is Shell.tsx's MobileNav/NavItem/NavGroup/SidebarNav sub-components); add direct `render()` tests for those (the static-tree test only walks the top-level Shell). Website + zombiectl coverage scripts (currently only app reports). Cross-repo changelog `<Update>` entry on `~/Projects/docs/changelog.mdx`. Authenticated Playwright e2e harness (signup → install zombie → events → kill — requires `@clerk/testing` + Clerk dev fixture users). Welcome-email on signup (currently no transactional email; pick provider — Resend / Postmark — and wire after `signup_bootstrap`).

Other consults fire here as they happen during implementation.

---

## Notes

- **Voice / copy:** preserved. This spec changes chrome, not prose. Page titles, table column headers, error messages all survive unchanged unless the existing copy contradicts spec voice ("operational, not SaaS-marketing").
- **Light mode:** parity holds via `[data-theme="light"]` from W1. Auth flows + dashboard both render correctly. Dark is canonical for screenshots; light is the polite secondary per spec §Color.
- **Bundle size:** the `mc-*` teardown drops a small amount of CSS; the page-shell rebuild may net out neutral or slightly heavier (more design-system primitive composition). Not a goal; not a regression target.
- **Performance:** wake-pulse is GPU-cheap (box-shadow only) but multiplies. The 5-cap above is the primary lever. If Chrome DevTools Performance shows < 60fps on 50+ live zombies even with the cap, drop the cap further and document in Discovery — do not introduce JS animation as a fallback.
- **Inline analytics literals:** the `mode: "humans"` analytics callsites in `App.tsx`/`Hero.tsx`/`Pricing.tsx` precedent (W2) carries to app: app analytics callsites stay inline, type-pinned by `Record<string, AnalyticsPrimitive>`, with tests covering each callsite. No Constants module.
- **Stacking on PR #308:** W3 commits land on `feat/m64-002-design-w1` (same branch as W1 + W2). PR #308 title gets extended to "M64 Operational Restraint: W1 (token swap) + W2 (website apply) + W3 (app apply)" at CHORE(close). Branch name stays as-is — branch name is a label, PR title is the canonical scope marker.
