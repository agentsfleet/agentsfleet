<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M119_001: Dashboard token contrast, a UsageBar primitive, DataTable sticky headers, an OptionCard picker, and a per-user avatar gradient

**Prototype:** v2.0.0
**Milestone:** M119
**Workstream:** 001
**Date:** Jul 07, 2026
**Status:** DONE
**Priority:** P2 — dashboard visual polish, benchmarked against PlanetScale's dashboard; no functional or data-model change.
**Categories:** UI
**Batch:** B1 — standalone UI workstream; independent of M117/M118.
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, this session's design proposal, agreed by Indy)
**Canonical architecture:** no architecture change — UI tokens + presentation only. Model of record: `docs/architecture/billing_and_provider_keys.md` §6 (confirms the credit-exhausted UX contract touched by §2 is unchanged).
**Branch:** feat/m119-dashboard-polish
**Test Baseline:** unit=2377 integration=255 (via `make _lint_zig_test_depth`; UI-only scope — Zig counts are the pin, not expected to move)

---

## Overview

**Goal (testable):** Dashboard cards read defined at rest (no hover needed) via two token-value bumps; the bespoke `.app-meter` markup in `BillingBalanceCard` is replaced by a reusable `UsageBar` design-system primitive; `DataTable` gains an opt-in sticky header and a one-notch-tighter default row density, with zero visual regression to its five existing consumers; `AddRunnerDialog`'s isolation-mode field renders as accessible option cards instead of a plain dropdown; the dashboard's account avatar shows a per-user deterministic gradient instead of one flat color for everyone.

**Problem:** A design proposal benchmarked against two PlanetScale dashboard screenshots found the `design-system` already has most of the right bones (`StatusCard`, `DataTable`, `DashboardRow`, `Badge`/`StatusPill`) but five concrete gaps: (1) card/table borders only become legible on hover — `--border` sits ~4% luminance above `--surface-1`, so the resting state reads flat; (2) there is no reusable usage/quota-meter primitive — `BillingBalanceCard.tsx` hand-rolls one via a bespoke `.app-meter` CSS class in `globals.css`, used nowhere else; (3) `DataTable`'s `<thead>` scrolls away with the body on any list longer than a screenful, losing column context; (4) the reference's bordered "choice card" picker (icon + label + description + selected-state ring) has no equivalent — `design-system` already ships an unused `RadioGroup`/`RadioGroupItem` Radix primitive (zero consumers anywhere in the app) while `AddRunnerDialog`'s isolation-mode field uses a plain `Select` dropdown instead; (5) the dashboard's account avatar (Clerk's `UserButton` fallback) renders every signed-in user against the same flat `--surface-2` fill, with no per-account distinctiveness.

**Solution summary:** Bump two dark-mode token values (`--border`, `--surface-1`) in `tokens.css` so every existing `border-border`/`bg-card` consumer gets a crisper resting state for free — no per-component migration. Extract a `UsageBar` primitive from the existing `.app-meter` pattern into `design-system`, migrate `BillingBalanceCard` to it, and delete the now-orphaned CSS. Add an opt-in `stickyHeader` prop to `DataTable` (bounded-height scroll container, sticky `<thead>`) and tighten the default cell padding one notch — both default-off/minimal so existing consumers (`ApiKeyList`, `CatalogueList`, `RunnerList`, `SecretsList`, `BillingUsageTab`) are unaffected except the padding change, which is uniform and intentional. Add an `OptionCard` primitive built on the existing (previously unconsumed) `RadioGroup`, and migrate `AddRunnerDialog`'s isolation-mode `Select` to it. Add a small deterministic `avatarGradient(seed)` helper and wire it into the dashboard's `UserButton` fallback avatar via the existing `useCurrentUser()` hook, so each signed-in user's initials render on a distinct, stable gradient.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(m119): bump token contrast, extract UsageBar/OptionCard primitives, add DataTable sticky header + avatar gradient
- **Intent (one sentence):** Five dashboard visual-polish deltas — crisper resting-state borders, a reusable usage-meter component replacing a one-off, a table that keeps its header in view, an accessible option-card picker replacing a dropdown, and a per-user avatar gradient.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/design-system/src/tokens.css` (dark block, ~line 25-33) — the two token values to bump; `theme.css` maps them into Tailwind's `@theme inline` so no component edits are needed for §1.
2. `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` (~line 42-60) + `ui/packages/app/app/globals.css` (~line 130-146, `.app-meter`/`.app-meter > span`/`@keyframes meter-fill`) — the exact bespoke pattern §2 extracts and replaces; the mount-fill animation + reduced-motion carve-out already exists here and should move with the primitive, not be dropped.
3. `ui/packages/design-system/src/design-system/StatusCard.tsx` — reference shape for a small, presentational, RSC-safe `design-system` primitive (props in, no state, no `asChild`).
4. `ui/packages/design-system/src/design-system/DataTable.tsx` (~line 46-73) — the existing optional-prop pattern (`isLoading`, `hideOnMobile`) to mirror when adding `stickyHeader`; confirm the wrapper's `overflow-x-auto` interaction with a new bounded-height scroll container before wiring `position: sticky`.
5. `ui/packages/design-system/src/theme.css` (~line 91-99) — `--color-pulse`/`--color-pulse-dim` are already mapped Tailwind tokens (used by the existing meter fill); `--accent` maps to `--surface-3` for the track. No new tokens needed for `UsageBar`.
6. `ui/packages/design-system/src/design-system/RadioGroup.tsx` — the existing, currently-unconsumed Radix `RadioGroup`/`RadioGroupItem` primitive `OptionCard` builds on; do not invent a second radio implementation.
7. `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` (~line 159-183, `sandbox_tier` field) — the `Select` this migrates to `RadioGroup` + `OptionCard`; `Select`'s imports (lines 26-30) become dead in this file once migrated and must be dropped.
8. `ui/packages/app/lib/auth/client.ts` (`useCurrentUser()`) — the existing hook exposing `userId`/`emailAddress`; the avatar-gradient seed comes from here, not a new Clerk call.
9. `ui/packages/app/lib/clerkAppearance.ts` (~line 9-17 comment, ~line 83-86 `userButtonAvatarBox`) — states the project's "no decorative gradient on chrome" / "`--pulse` is currency, not decoration" rules; §5's gradient is a deliberate, commented exception applied at the call site (`ClientOnlyAuthUserButton.tsx`), not a change to this file's own rule or its `AUTH_APPEARANCE` export.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/tokens.css` | EDIT | Bump dark-mode `--border` and `--surface-1` for resting-state card/border contrast (§1) |
| `ui/packages/design-system/src/design-system/UsageBar.tsx` | CREATE | New primitive extracted from the existing `.app-meter` pattern (§2) |
| `ui/packages/design-system/src/index.ts` | EDIT | Export `UsageBar` |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | Replace hand-rolled label/percentage/`.app-meter` markup with `<UsageBar>` (§2) |
| `ui/packages/app/app/globals.css` | EDIT | Remove `.app-meter`/`.app-meter > span`; retarget the `meter-fill` keyframe + reduced-motion carve-out to the primitive's class hooks (§2) |
| `ui/packages/design-system/src/design-system/DataTable.tsx` | EDIT | Add opt-in `stickyHeader` prop (keyboard-reachable scroll region); tighten default cell padding one notch (§3) |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | Wire `stickyHeader` on the paginated usage-history table — its real consumer (§3, added at EXECUTE per RULE HLP — see Discovery) |
| `ui/packages/design-system/src/design-system/OptionCard.tsx` | CREATE | New primitive wrapping `RadioGroupPrimitive.Item` as a bordered choice card (§4) |
| `ui/packages/design-system/src/design-system/index.ts` / `ui/packages/design-system/src/index.ts` | EDIT | Export `OptionCard` (alongside the existing `RadioGroup`/`RadioGroupItem`, `UsageBar`) |
| `ui/packages/app/lib/api/runners.ts` | EDIT | Add a `SANDBOX_TIER_DESCRIPTIONS` map (one line per tier) alongside the existing `SANDBOX_TIER_LABELS`, for `OptionCard`'s description slot (§4) |
| `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` | EDIT | Replace the `sandbox_tier` `Select` with `RadioGroup` + `OptionCard`; drop the now-unused `Select*` imports (§4) |
| `ui/packages/app/lib/avatarGradient.ts` | CREATE | Pure deterministic seed → CSS gradient helper (§5) |
| `ui/packages/app/components/layout/ClientOnlyAuthUserButton.tsx` | EDIT | Compute the per-user gradient via `useCurrentUser()` + `avatarGradient()`, override `userButtonAvatarBox.background` for this render only (§5) |
| `docs/DESIGN_SYSTEM.md` | EDIT | Correct the dark-mode token table (§1), rename "Balance meter" → "Usage bars" + add "Option cards" under Component principles (§2, §4), document the avatar-gradient exception (§5), append 3 Decisions log rows |
| `**/*.test.{ts,tsx}` (co-located) | EDIT/CREATE | Cover the new token values, `UsageBar`, `OptionCard`, `avatarGradient`, the migrated `BillingBalanceCard`/`AddRunnerDialog`/`ClientOnlyAuthUserButton`, and `DataTable`'s sticky-header behaviour |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE HLP (don't ship `UsageBar`/`OptionCard` without a consumer — `BillingBalanceCard` / `AddRunnerDialog` are the consumers, in the same diff; `OptionCard` additionally gives the previously-zero-consumer `RadioGroup` its first real caller, resolving a latent HLP risk rather than creating one); RULE NDC (the orphaned `.app-meter` CSS and the now-unused `Select*` imports in `AddRunnerDialog.tsx` are fully deleted, not left dead); RULE NLR (touch-it-fix-it — while migrating `BillingBalanceCard`/`AddRunnerDialog`, confirm no other bespoke meter/picker markup exists elsewhere); RULE UFS (no new string-literal duplication; `ui/` carve-out means this is a manual check, not automated).
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE DECISION for the new `UsageBar.tsx`/`OptionCard.tsx`/`avatarGradient.ts` (`UsageBar`/`OptionCard`: component, no bound state → functions-module/component verdict, mirrors `StatusCard.tsx`; `avatarGradient.ts`: pure function, no state → functions-module verdict); DESIGN TOKEN GATE (no arbitrary hex/utility in `UsageBar`/`OptionCard` — resolve only to mapped tokens; the avatar gradient's runtime HSL values are an explicit, commented override — see §5 — because a per-user hash has no static token equivalent by definition); UI Component Substitution Gate (reuses existing primitives; no raw-HTML alternative introduced).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI Substitution / DESIGN TOKEN | yes | `UsageBar`/`OptionCard` track/fill/border resolve only to existing mapped tokens; no arbitrary Tailwind values. `avatarGradient`'s runtime HSL string is an explicit `// DESIGN TOKEN: SKIPPED per user override (reason: ...)`-commented exception — a per-user hash is inherently dynamic, no static token can represent it |
| File & Function Length (≤350/≤50/≤70) | yes | All edits are small, in-place; no file approaches the cap |
| UFS | yes | No repeated literal strings introduced; manual `ui/` carve-out check |
| ZIG / SCHEMA / LOGGING (Zig) / ERROR REGISTRY | no | No backend, no schema, no Zig |

## Prior-Art / Reference Implementations

- **Reference (the exact pattern to extract):** `BillingBalanceCard.tsx` + `globals.css` `.app-meter` — the current bespoke meter is the direct source for `UsageBar`'s visual contract (8px track, full-pill radius, mint gradient fill, mount-fill animation, reduced-motion carve-out); this is extraction, not new design.
- **Reference (primitive shape):** `StatusCard.tsx` — small presentational component, `HTMLAttributes` passthrough, no `asChild`, RSC-safe.
- **Reference (opt-in prop precedent):** `DataTable.tsx`'s existing `isLoading`/`hideOnMobile` optional props — `stickyHeader` follows the same additive, default-off shape.
- **Reference (radio semantics to build on, not reinvent):** `RadioGroup.tsx`'s existing Radix `RadioGroupPrimitive.Root`/`Item` composition — currently zero consumers in the app; `OptionCard` styles `Item` as a full card rather than wrapping it in a second abstraction.
- **Reference (identity source):** `lib/auth/client.ts`'s `useCurrentUser()` — the only sanctioned way app code reads the signed-in user's id/email; `avatarGradient`'s seed comes from here, never a direct `@clerk/nextjs` import.

## Sections (implementation slices)

### §1 — Resting-state border and surface contrast ✅ DONE (all Dimensions)

Bumps two dark-mode token values so cards and tables read defined before any hover/focus interaction, matching the reference screenshots' crispness. **Implementation default:** `--border: #23292e → #2b333a` and `--surface-1: #11161a → #141a1f` in `tokens.css`'s dark block only — value-only edits, no new token names, no component changes, because every `border-border`/`bg-card` consumer already inherits from these two tokens via `theme.css`'s `@theme inline` mapping. Light-mode tokens are untouched (`ThemeToggle.tsx` forces dark; light mode is vestigial — out of scope).

- **Dimension 1.1** — dark-mode `--border` is `#2b333a` → Test `test_border_token_bumped`
- **Dimension 1.2** — dark-mode `--surface-1` is `#141a1f` → Test `test_surface1_token_bumped`
- **Dimension 1.3** — light-mode `--border`/`--surface-1` values are byte-identical to before this spec → Test `test_light_tokens_unchanged`

### §2 — A `UsageBar` primitive, replacing the bespoke `.app-meter` ✅ DONE (all Dimensions)

Extracts the existing meter markup in `BillingBalanceCard` into a reusable `design-system` component so any future usage/quota surface has a primitive to reach for instead of hand-rolling another one-off. **Implementation default:** `UsageBar` accepts an optional label, a 0-100 percentage (rendered `tabular-nums`, shown only when `label` is supplied), and an optional sub-caption node; track renders with `bg-accent` (maps to `--surface-3`), fill with the existing pulse gradient, height/radius matching the current `.app-meter` (8px, full pill) exactly — this is a lift-and-shift of a proven pattern, not a redesign. The `meter-fill` keyframe and its reduced-motion override move with the component (renamed generically, no longer billing-specific) rather than being dropped. **Amended at EXECUTE:** `label` is optional, not required (see Interfaces + Discovery) — `BillingBalanceCard`'s existing meter has no visible label/percentage (it's `aria-hidden`, and the dollar headline above it already states the value), so making `label` mandatory would have added new visible text to a card the spec commits to leaving visually unchanged.

- **Dimension 2.1** — `UsageBar` renders track+fill always, and a label + tabular-nums percentage row only when `label` is supplied; an optional sub-caption always renders when given; exported from `@agentsfleet/design-system` → Test `test_usage_bar_renders`
- **Dimension 2.2** — `BillingBalanceCard` renders its meter via `<UsageBar>`, no bespoke inline meter markup remaining → Test `test_billing_balance_uses_usage_bar`
- **Dimension 2.3** — `.app-meter` and `.app-meter > span` are fully removed from `globals.css`, zero remaining references → Test `test_app_meter_removed`
- **Dimension 2.4** — `UsageBar`'s track/fill classes resolve only to existing mapped Tailwind tokens (`bg-accent`, `pulse`/`pulse-dim`); no arbitrary hex or one-off utility → Test `test_usage_bar_no_arbitrary_colors`

### §3 — `DataTable` sticky header and row density ✅ DONE (all Dimensions)

Keeps a long table's column labels in view while scrolling, and tightens default row padding one notch for a denser, PlanetScale-like read. **Implementation default:** an opt-in `stickyHeader` boolean prop wraps the table body in a bounded-height (`overflow-y-auto`) scroll container with `<thead>` set `sticky top-0` inside it — a separate scroll axis from the existing `overflow-x-auto` wrapper, so horizontal scroll on narrow viewports is unaffected. Default (`stickyHeader` unset) renders exactly as today. Cell padding drops one notch (`py-2` → `py-1.5`) unconditionally — small, uniform, no consumer-side change needed.

- **Dimension 3.1** — `stickyHeader={true}` bounds the table height and keeps `<thead>` visually pinned while the body scrolls; `stickyHeader` unset renders identically to the pre-change markup → Test `test_data_table_sticky_header_optin`
- **Dimension 3.2** — default cell padding is `py-1.5` (was `py-2`) with no other markup change → Test `test_data_table_row_density`
- **Dimension 3.3** — all five existing `DataTable` consumers (`ApiKeyList`, `CatalogueList`, `RunnerList`, `SecretsList`, `BillingUsageTab`) still render their rows and remain clickable/keyboard-navigable where they were before → Test `test_data_table_consumers_unaffected`

### §4 — An `OptionCard` picker, replacing `AddRunnerDialog`'s isolation-mode dropdown ✅ DONE (all Dimensions)

Gives `design-system` a bordered "choice card" primitive (icon/label/description, ring-highlighted when selected) — the picker idiom the reference screenshots use for Database engine / Cluster configuration / Storage options — and uses it for the one existing picker in the app that's a plain dropdown today. **Implementation default:** `OptionCard` wraps the existing `RadioGroupPrimitive.Item` (via `RadioGroup.tsx`, currently unconsumed anywhere) as a full-card button: label, an optional one-line description, an optional leading icon slot, `data-[state=checked]` → a `border-primary` ring treatment mirroring `StatusCard`'s existing hover/focus border-accent idiom. `AddRunnerDialog`'s `sandbox_tier` field swaps `Select`/`SelectTrigger`/`SelectItem` for a `RadioGroup` of four `OptionCard`s (one per `SANDBOX_TIERS` value), stacked in a single column — four cards with description text read better stacked than in a cramped 2-column grid at dialog width. Per-tier one-line descriptions are new, added to `lib/api/runners.ts` alongside the existing `SANDBOX_TIER_LABELS` (same keyed-by-tier shape, so a new tier can't be added without one). `Select`'s imports are dropped from `AddRunnerDialog.tsx` once unused — the `Select` primitive itself is untouched (used elsewhere in the app).

- **Dimension 4.1** — `OptionCard` renders as an accessible radio item (Radix `role="radio"`) with label + optional description + optional icon; `data-state="checked"` styling applies only to the selected card → Test `test_option_card_renders_and_selects`
- **Dimension 4.2** — `AddRunnerDialog`'s isolation-mode field renders four `OptionCard`s (one per `SANDBOX_TIERS` value) inside a `RadioGroup`; selecting one calls the form field's `onChange` with that tier's value, matching the pre-migration binding behaviour → Test `test_add_runner_isolation_mode_option_cards`
- **Dimension 4.3** — `OptionCard`'s selected/unselected styling resolves only to existing mapped tokens; no arbitrary hex or one-off utility → Test `test_option_card_no_arbitrary_colors`

### §5 — A per-user deterministic avatar pattern ✅ DONE (all Dimensions)

Replaces the dashboard account avatar's flat `--surface-2` fallback fill (identical for every signed-in user) with a per-user two-colour `repeating-conic-gradient` pinwheel, hashed from the user's Clerk id — a pattern reads closer to "distinct per account" than a smooth blend would, while staying inside the existing "no three-or-more-stop gradient" rule (it's two colours, repeated) and Clerk's CSS-only styling hook (no custom child markup, so a true pixel-grid identicon is out of scope — see Out of Scope). **Implementation default:** a pure `avatarGradient(seed: string): string` helper (new `lib/avatarGradient.ts`) hashes the seed to a hue, a second analogous hue, and a starting angle, and returns a `repeating-conic-gradient(from <angle>deg, hsl(h1) 0deg 45deg, hsl(h2) 45deg 90deg)` string — two different users get visibly different pinwheels, not just different colours. `ClientOnlyAuthUserButton.tsx` reads `userId`/`emailAddress` from the existing `useCurrentUser()` hook, picks the seed (`userId ?? emailAddress ??` a constant fallback), and passes Clerk's `UserButton` a per-render `appearance` object that spreads `AUTH_APPEARANCE` and overrides only `elements.userButtonAvatarBox.background` with the computed pattern — `clerkAppearance.ts` itself, and its documented "no decorative gradient on chrome" / "pulse is currency" rules, are untouched; this is one explicit, commented exception at the call site, not a change to the rule. Only affects the initials-fallback state — a user with an uploaded photo is unaffected (the photo covers the background).

- **Dimension 5.1** — `avatarGradient(seed)` is pure: the same seed always returns the same pattern string; different seeds produce visibly different hues/angle (not a constant) → Test `test_avatar_gradient_deterministic`
- **Dimension 5.2** — `ClientOnlyAuthUserButton` passes `UserButton` an `appearance.elements.userButtonAvatarBox.background` derived from the current user's id, not the flat `--surface-2` fill → Test `test_avatar_button_gradient_wired`
- **Dimension 5.3** — with no user id yet available (pre-hydration/loading), the component falls back to a stable, non-empty seed rather than an empty-string hash or a crash → Test `test_avatar_gradient_fallback_seed`

## Interfaces

```
DataTable<T> gains one new optional prop (all existing props unchanged):
  stickyHeader?: boolean   // default false — bounds height + pins <thead> when true

UsageBar (new, exported from @agentsfleet/design-system):
  { label?: string; pct: number; sublabel?: React.ReactNode; className?: string }
  — pct is clamped [0,100] by the component, not the caller. `label` is
  optional (amended at EXECUTE, see Discovery): BillingBalanceCard's meter is
  intentionally unlabeled/aria-hidden today (the dollar headline above it
  already states the value) — the label+percentage row renders only when a
  caller supplies `label`, so migrating BillingBalanceCard adds no visible
  text that wasn't there before.

No HTTP/API/schema change. No change to computeReceiveCharge/computeStageCharge
or any billing data shape — BillingBalanceCard's existing props
(TenantBilling, ChargeSummary) are unchanged; only its internal rendering
of the meter changes.

OptionCard (new, exported from @agentsfleet/design-system):
  { value: string; label: string; description?: React.ReactNode;
    icon?: React.ReactNode; className?: string }
  — must render inside the existing <RadioGroup>; value/checked state flow
  through Radix's RadioGroupPrimitive.Item, same as RadioGroupItem today.

avatarGradient(seed: string): string — pure, deterministic, returns a CSS
repeating-conic-gradient() value. No new HTTP/API/schema surface; no change
to useCurrentUser()'s existing shape.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Token bump breaks a snapshot/visual test pinned to the old hex | Test asserts a literal token value | Update the co-located test to the new value in the same commit |
| `stickyHeader` used without realizing it bounds height | Caller sets the prop on a table meant to grow with the page | Prop is opt-in and documented on the type; default behaviour is unchanged, so an accidental regression requires an explicit opt-in |
| `.app-meter` removed while something else still references it | Assumption that `BillingBalanceCard` was the sole consumer is wrong | Dimension 2.3's grep runs before deletion; a surviving reference blocks the removal, not silently breaks it |
| Mount-fill animation regresses to an instant jump | `meter-fill` keyframe dropped instead of retargeted during extraction | Dimension 2.1's test asserts the animation class hook is present on `UsageBar`'s fill element |
| `OptionCard` renders outside a `RadioGroup` | Consumer forgets the required parent | Radix's `RadioGroupPrimitive.Item` throws a context error in dev — existing Radix behaviour, not new handling; documented in the Interfaces note |
| Avatar seed is empty/undefined pre-hydration | `useCurrentUser()` returns `userId: null` before Clerk finishes loading | `avatarGradient` receives a constant fallback seed (Dimension 5.3), never an empty string, so the gradient is always well-defined |

## Invariants

1. `UsageBar`'s track/fill resolve only to existing mapped design-system tokens — no inline hex, no arbitrary Tailwind value — enforced by Dimension 2.4's grep-based test.
2. `DataTable`'s default (no `stickyHeader`) render path is markup-equivalent to today for all five existing consumers except the intentional padding change — enforced by Dimension 3.3's consumer tests.
3. Light-mode token values are untouched — enforced by Dimension 1.3's byte-comparison test.
4. `OptionCard`'s selected/unselected styling resolves only to existing mapped tokens — enforced by Dimension 4.3's grep-based test.
5. `avatarGradient` is pure — same seed always produces the same output — enforced by Dimension 5.1's repeated-call equality test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_border_token_bumped` | `tokens.css` dark block `--border` equals `#2b333a` |
| 1.2 | unit | `test_surface1_token_bumped` | `tokens.css` dark block `--surface-1` equals `#141a1f` |
| 1.3 | unit | `test_light_tokens_unchanged` | `tokens.css` light block `--border`/`--surface-1` unchanged from baseline |
| 2.1 | unit | `test_usage_bar_renders` | `<UsageBar label="Monthly run budget" pct={62} sublabel="…"/>` renders label, `62%` (tabular-nums), a track, and a fill sized to 62%; `<UsageBar pct={30}/>` (no label) renders track+fill only, no label/percentage text |
| 2.2 | unit | `test_billing_balance_uses_usage_bar` | `BillingBalanceCard` render tree contains `UsageBar`'s `data-slot` (via the overridden `balance-meter` testid); no bespoke `.app-meter` div; fill width matches `summary.meterPct`; no label/percentage text rendered |
| 2.3 | unit | `test_app_meter_removed` | `grep -rn 'app-meter' ui/packages/app/app/globals.css` → 0 matches |
| 2.4 | unit | `test_usage_bar_no_arbitrary_colors` | `UsageBar.tsx` contains no `#`-hex literal and no `[` arbitrary Tailwind value in its class strings |
| 3.1 | unit | `test_data_table_sticky_header_optin` | `stickyHeader` unset → markup matches pre-change baseline; `stickyHeader={true}` → wrapper has bounded height + `<thead>` has `sticky top-0` |
| 3.2 | unit | `test_data_table_row_density` | default `<td>`/`<th>` classes include `py-1.5`, not `py-2` |
| 3.3 | regression | `test_data_table_consumers_unaffected` | `ApiKeyList`, `CatalogueList`, `RunnerList`, `SecretsList`, `BillingUsageTab` each render their existing rows without a thrown error or missing row |
| 4.1 | unit | `test_option_card_renders_and_selects` | `<OptionCard value="a" label="A" description="…"/>` inside a `<RadioGroup>` renders `role="radio"`; selecting it sets `data-state="checked"` on that card only |
| 4.2 | unit | `test_add_runner_isolation_mode_option_cards` | `AddRunnerDialog` renders 4 `OptionCard`s (one per `SANDBOX_TIERS`); clicking one calls `form`'s `sandbox_tier` `onChange` with that tier's value |
| 4.3 | unit | `test_option_card_no_arbitrary_colors` | `OptionCard.tsx` contains no `#`-hex literal and no `[` arbitrary Tailwind value in its class strings |
| 5.1 | unit | `test_avatar_gradient_deterministic` | `avatarGradient("user_1")` called twice returns the identical string; `avatarGradient("user_1")` !== `avatarGradient("user_2")` |
| 5.2 | unit | `test_avatar_button_gradient_wired` | rendering `ClientOnlyAuthUserButton` with a mocked signed-in user passes `appearance.elements.userButtonAvatarBox.background` equal to `avatarGradient(<that user's id>)`, not the flat `--surface-2` string |
| 5.3 | unit | `test_avatar_gradient_fallback_seed` | with `useCurrentUser()` mocked to `{ userId: null, emailAddress: null, isLoaded: false, isSignedIn: false }`, the computed background is still a non-empty `repeating-conic-gradient(...)` string |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Token contrast bumped (§1) | `grep -E '\-\-border: #23292e\|\-\-surface-1: #11161a' ui/packages/design-system/src/tokens.css` | no output (old values gone from the dark block) | P2 | ✅ no output |
| R2 | `UsageBar` exported and consumed (§2) | `grep -n "UsageBar" ui/packages/design-system/src/index.ts "ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx"` | both files match | P2 | ✅ both match |
| R3 | `.app-meter` fully removed (§2) | `grep -rn 'app-meter' ui/packages/app/app/globals.css` | no output | P2 | ✅ no output |
| R4 | `DataTable` sticky header opt-in present, default unchanged (§3) | inspect `DataTable.tsx` for `stickyHeader` prop | prop exists; default path unmodified | P2 | ✅ prop exists; default-path test passes unmodified |
| R6 | `OptionCard` used for AddRunnerDialog isolation mode (§4) | `grep -n "OptionCard" "ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx"` | match found; `grep -c "SelectItem" ` on the same file → 0 | P2 | ✅ match found; `SelectItem` count 0 |
| R7 | Avatar renders a per-user gradient, not flat `--surface-2` (§5) | `grep -n "avatarGradient" ui/packages/app/components/layout/ClientOnlyAuthUserButton.tsx` | match found | P2 | ✅ match found |
| R5 | Diff inside Files Changed | `git diff --name-only main` | 0 paths missing from the table | P0 | ✅ 23 paths — includes the pending→active spec pair (a heavy same-diff edit, so git shows delete+add rather than a detected rename) and every §4/§5 file |
| S1 | UI unit tests pass | `make test-unit-agentsfleet` (or the ui package test runner) | exit 0 | P0 | ✅ design-system 453/453, app 1213/1213 |
| S2 | Lint clean | `make lint` | exit 0 | P0 | ✅ `make lint-apps-ds-ctl` — app/design-system/agentsfleet all clean |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found |
| S4 | No oversize file | `git diff --name-only main \| grep '\.tsx\?$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output |
| S5 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | ✅ `.app-meter` 0 matches; `SelectItem` 0 matches in `AddRunnerDialog.tsx`; `meter-fill` 4 matches (comment + rule + keyframe + reduced-motion mention, not orphaned) |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P2 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `.app-meter` (CSS class) | `grep -rn "app-meter" ui/packages/app/app/globals.css ui/packages/app/app/\(dashboard\)/` | 0 matches |
| `meter-fill` keyframe (renamed, not deleted) | `grep -rn "meter-fill" ui/packages/app/app/globals.css` | ≥1 match — the renamed keyframe still backing `UsageBar`'s fill animation (plus its own comment mentions), not orphaned |
| `Select`/`SelectContent`/`SelectItem`/`SelectTrigger`/`SelectValue` imports (dropped from `AddRunnerDialog.tsx`; the `Select` primitive itself is untouched, used elsewhere) | `grep -n "SelectItem" "ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx"` | 0 matches |

## Out of Scope

- **Light-mode token parity** — `ThemeToggle.tsx` forces dark on mount; light mode is vestigial. No light-mode changes in this spec.
- **The "compact capacity rows" pattern** (Warm runners / Queue depth) from the design proposal — no required change identified; `DashboardRow`'s existing icon-chip shape already covers it if/when a runner-capacity panel is built. Left for a future spec.
- **A true pixel-grid identicon (GitHub-style) instead of a gradient** — Clerk's `appearance.elements` styling API accepts CSS values, not custom child markup; a data-URI SVG background is theoretically possible but would need to be verified live against Clerk's own initials overlay before committing to it (unverified assumption, not a same-PR patch). The gradient is the tested, low-risk equivalent for this spec.
- **Migrating the Models page's existing option-card prose (M98 §3-4) onto the new `OptionCard` primitive** — that page wasn't audited in this session; if it's still ad-hoc markup, that's a small follow-up, not this spec's scope (§4 only touches `AddRunnerDialog`).
- **Any new page, route, or dashboard panel** — this spec touches existing surfaces only.
- **Any backend/schema/analytics change** — presentation-only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user opens any dashboard page and the cards/tables read crisp and legible before touching anything; on the Billing page the balance meter looks and behaves exactly as before, just built on a primitive that can be reused elsewhere; adding a runner picks the isolation mode from clear option cards instead of a dropdown; and the account avatar in the corner is recognizably *theirs*, not the same grey circle everyone else sees.
2. **Preserved user behaviour** — Every existing page, link, click, and data value is unchanged; only resting-state contrast, one component's internal markup, table row density, one form field's picker control, and the avatar fallback fill change.
3. **Optimal-way check** — Yes: token-value edits, extracting two already-proven/spec'd patterns (`.app-meter`, the M98 option-card prose) into primitives, and one small pure hash helper is the minimal lift for five distinct, independently small deltas; no rebuild buys more for a P2 polish pass.
4. **Rebuild-vs-iterate** — Iterate. Presentation-only; no determinism or data-model impact.
5. **What we build** — two token value bumps; one new `UsageBar` primitive + `BillingBalanceCard` migration; one opt-in `DataTable` prop + a uniform padding tighten; one new `OptionCard` primitive (on the existing, previously-unconsumed `RadioGroup`) + `AddRunnerDialog` migration; one pure `avatarGradient` helper + `ClientOnlyAuthUserButton` wiring.
6. **What we do NOT build** — any new page, any new metric/event, light-mode changes, the runner-capacity "bonus" panel, a true pixel-grid identicon, the Models page's option-card migration, any backend/schema change.
7. **Fit with existing features** — Extends `design-system` primitives already used across settings/admin pages; must not regress `BillingBalanceCard`'s credit-exhausted alert, any of `DataTable`'s five existing consumers, `AddRunnerDialog`'s existing form validation/submission, or the account menu's sign-out/settings actions.
8. **Surface order** — UI-only; no CLI/API surface.
9. **Dashboard restraint** — No new card, control, or metric is added; existing surfaces become more legible, two duplicated/undocumented patterns are consolidated into primitives, and the avatar gains identity signal without adding a control — nothing new is exposed before its signal is real.
10. **Confused-user next step** — N/A — no new error/edge state; the existing exhausted-balance alert, "Buy credits" flow, and runner-registration validation are unchanged.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One workstream, five small Sections by surface (tokens / UsageBar extraction / table structure / OptionCard extraction / avatar pattern), each independently verifiable — same shape as M117's dashboard-polish workstream, extended in-session per Indy's follow-up requests rather than split into a second spec (same PR, per explicit instruction).
- **Alternatives considered:** (a) Introduce a parallel "elevated" border/surface token pair used only by select components — rejected: inconsistent adoption surface, larger blast radius than a direct value bump on the base tokens. (b) Design `UsageBar`/`OptionCard` from scratch with new visual language — rejected: `.app-meter` and the M98 §3-4 option-card prose are proven, already-decided patterns; extraction is lower-risk than redesign. (c) A true pixel-grid identicon for §5 — rejected for this patch: Clerk's styling hook has no confirmed way to suppress its own initials overlay under a data-URI background, an unverified assumption not worth taking into a same-PR patch; the conic-gradient pinwheel is the tested, same-risk-profile equivalent.
- **Patch-vs-refactor verdict:** a **patch** — token values, two component extractions (each building on an already-existing, under-used primitive), one opt-in prop, one pure helper. No architecture move.

## Discovery (consult log)

- **Consults** — Indy reviewed a design proposal (artifact) benchmarked against two PlanetScale dashboard screenshots and agreed to proceed; three deltas selected from the proposal's original three (border contrast, `UsageBar`, table structure), the "bonus" capacity-row pattern deferred (no ack needed — it was explicitly optional in the proposal, not dropped scope). Mid-EXECUTE, Indy asked for "a large refactor" for optimization/performance with no target named; flagged that this contradicts the spec's own patch-vs-refactor verdict and that there's no performance concern in scope (presentation-only diff, no data-fetching/compute) — Indy confirmed **stick to the patch**, no refactor.
- **§4/§5 scope addition (same PR, explicit instruction)** — after a second PlanetScale screenshot, Indy asked what else was worth adopting; offered 4 candidates (option cards, live cost rail, tier-picker row, inline settings in the account menu) and named option cards as the one with a real, immediate consumer. Indy: "spec that here and fix it in this PR. There must [be a] section to the current spec" (§4 added, same file/branch, not a new M120). Also asked to adopt the reference's gradient avatar circle — flagged a real conflict first: `clerkAppearance.ts` documents "no decorative gradient on chrome" and `--pulse` as currency-only, and Clerk's `UserButton` appearance object has no per-user data by default. Asked via `AskUserQuestion` (one fixed gradient / per-user hash / skip) — Indy asked "how about github like? is that easy to make", surfacing that `useCurrentUser()` already exposes the per-render identity needed, so a per-user hash is in fact a small change. Answered directly when asked for an opinion on a true pixel-grid identicon: Clerk's `appearance.elements` accepts CSS values only (no child markup), so an identicon needs an unverified data-URI-vs-Clerk's-initials-overlay assumption — recommended the tested, same-risk CSS-only equivalent instead. Indy asked for one more "creative and unique" cheap option; proposed and adopted a `repeating-conic-gradient` pinwheel (hash-derived hue × hue × angle) over a plain linear blend — same integration point, same risk profile, reads more identity-distinct.
- **EXECUTE reconciliations** — §2's `UsageBar.label` was pinned required in the Interfaces section at authoring; discovered at EXECUTE that `BillingBalanceCard`'s existing meter is intentionally unlabeled and `aria-hidden` (the dollar headline above it already states the value), so a required `label` would have added new visible text — amended `label` to optional (Interfaces + §2 updated in the same diff) so the migration adds zero visible change to `BillingBalanceCard`.
- **Metrics review** — no product/operator signal changes; presentation-only.
- **Skill-chain outcomes** — `/write-unit-test`: tests written per Dimension before each review pass (§ Test Specification), design-system 453/453 + app 1213/1213 green. `/review` on §1-§3 (medium effort, 3 finder angles + verify): 2 confirmed findings, both fixed — (1) `stickyHeader` shipped with zero production consumers (RULE HLP) → wired onto `BillingUsageTab`'s paginated usage-history table; (2) the sticky-header scroll region was hardcoded (`max-h-96`, no override) and not keyboard-reachable → added `tabIndex={0}` + `role="region"` + a caption-derived `aria-label`. `/review` on §4-§5 (correctness + cleanup/conventions angles): 3 findings, all fixed — (1) migrating `sandbox_tier` from `Select` to `RadioGroup` left `FormControl`'s injected `id`/`aria-describedby`/`aria-invalid` on a `div[role=radiogroup]` (not a labelable element) while `FormLabel`'s `htmlFor` pointed at it — the group's accessible name now comes from `aria-labelledby` referencing the `FormLabel`'s own `id` (via `useId()`), not an inert `htmlFor` match or a duplicated `aria-label="Isolation mode"` string; (2) the migration silently dropped the "How the host isolates fleet work — self-reported" `FormDescription` disclaimer → restored; (3) §4/§5 section headers were missing their `✅ DONE` markers in this same diff → added. Correctness/cleanup angles on §1-§3 and reuse/conventions on §4-§5 otherwise returned no findings. `kishore-babysit-prs`: polled 12 cycles on PR #491 (~80 min, backoff cadence) — zero greptile activity the whole window (only a Vercel deploy-preview comment); stopped per the skill's 2-consecutive-empty-polls-in-the->60min-bracket condition. PR merged directly.
- **Deferrals** — None. The "compact capacity rows" pattern and a true pixel-grid identicon were scoped out at authoring/EXECUTE as Out of Scope, not deferred mid-work — no Indy-ack quote needed.
