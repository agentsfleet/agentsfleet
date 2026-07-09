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

# M125_001: Model-details dialog reads like a summary, timestamps go relative, and icon-only row actions become a named design-system standard

**Prototype:** v2.0.0
**Milestone:** M125
**Workstream:** 001
**Date:** Jul 09, 2026
**Status:** PENDING
**Priority:** P2 — presentation polish plus one accessibility-hardening primitive; no data-model, route, or wire-shape change. The icon-action standard removes a whole defect class (nameless icon buttons), which lifts it above pure cosmetics.
**Categories:** UI
**Batch:** B2 — no code dependency, but M120_003 (pending) renames the `settings/models` client imports this spec's §1 file already uses (`@/lib/api/model_caps` → `model_library`). Ordering risk only: whichever lands second rebases one import line in `ModelDetailsDialog.tsx`. Do NOT block on it; note the collision at PLAN and take the trivial rebase.
**Branch:** {added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none.
**Provenance:** human-directed — Indy's screenshot feedback on the model-entry details dialog, the Secrets "Created" column, and the Manage-runners row actions (Jul 09, 2026 session); agent-drafted against the code as merged at PR #496.
**Canonical architecture:** none — presentation-layer only; no flow, channel, or schema is defined or changed. The design-system primitive set (`ui/packages/design-system/src/index.ts`) is the surface of record for §3.

---

## Overview

**Goal (testable):** the model-details dialog drops the `Kind` and `Has key` rows in favour of a header vault Badge and a relative "Added" time; every list timestamp (Secrets "Created", runner enrolled/last-seen) renders through the existing `Time` component with an absolute-timestamp tooltip; and a new `IconAction` design-system primitive makes every icon-only row action carry a type-required accessible name, adopted first by the runner admin list.

**Problem:** three screenshot complaints. (1) The details dialog labels the vault-key reference "Name" — so both `Name` and `Provider` read "pioneer" — carries a noisy `Kind` row, states key presence as a `Has key: Yes/No` row, and shows an absolute `Created` timestamp buried in the row list. (2) The Secrets "Created" column and the runner host cell print raw locale-pinned absolute timestamps ("Jul 09, 2026, 04:12 PM") where a relative "… ago" label reads faster. (3) The Manage-runners actions are wide text buttons; Indy asked for standardized icon-only actions — but the app has no icon-only row-action pattern, so a first mover would invent one and likely ship an icon with no accessible name.

**Solution summary:** the fix is adoption, not invention. `Time` already exists and already supports `format="relative"`, defaulting its tooltip ON to render the absolute timestamp on hover — the call sites simply never pass `format="relative"`. `Tooltip`, `Badge`, and `Button` (`size="icon"`, `size="icon-sm"`) already exist. §1 restructures `ModelDetailsDialog` (relabel, drop two rows, header Badge + relative added-time). §2 points the Secrets "Created" cell at `Time` and deletes its bespoke `DATE_FORMATTER`/`formatCreatedAt` (RULE NDC/NLR). §3 adds one new primitive, `IconAction` = `Button(size="icon-sm")` + `Tooltip` + a **required** `label` prop feeding both the tooltip body and the `aria-label`, so an icon-only action cannot compile without a name. §4 adopts `IconAction` in the runner list for Activity/Cordon/Drain/Revoke (destructive intent preserved for Revoke), gives `ACTION_CONFIG` an icon per action, moves the host-cell timestamps to `Time`, and deletes the list's own `fmt()`.

## PR Intent & comprehension handshake

- **PR title (eventual):** Relative timestamps, a cleaner model-details header, and a named icon-action primitive
- **Intent (one sentence):** operators read model-entry details and list timestamps at a glance, and every icon-only action announces itself to sighted and assistive-technology users alike — enforced by the type system, not by review.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch against the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/design-system/src/design-system/Time.tsx` + `time-utils.ts` — `Time` renders `<time dateTime>`; `format="relative"` defaults `tooltip` ON (absolute form on hover) and sets `suppressHydrationWarning` because the label depends on `Date.now()`. No new date component is needed — pass the prop.
2. `ui/packages/design-system/src/design-system/Button.tsx` + `Tooltip.tsx` + `Badge.tsx` — the three primitives `IconAction` composes; note `size="icon-sm"` (`h-6 w-6`) on Button and `TooltipTrigger asChild`. Do NOT re-implement any of them.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx` — §1's target; its `Name` row renders `target.secret_ref` (the vault key ref), the source of the duplicated "pioneer".
4. `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` — §4's target: `ACTION_CONFIG`, `actionsFor`, `ActionsCell`, `HostCell`, and the locale-pinned `fmt()`.
5. `ui/packages/app/app/(dashboard)/layout.tsx` — already mounts `TooltipProvider` at the dashboard root, so both surfaces have the ancestor `Time` and `IconAction` tooltips require; no new provider.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/design-system/IconAction.tsx` | CREATE | the new primitive — `Button(size="icon-sm")` + `Tooltip` + required `label` (aria-label + tooltip body) |
| `ui/packages/design-system/src/design-system/IconAction.test.tsx` | CREATE | accessible-name, tooltip-body, variant/passthrough, fixed-size cases (colocated, per design-system convention) |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | export `IconAction` + `IconActionProps` from the barrel |
| `ui/packages/design-system/src/index.ts` | EDIT | re-export `IconAction` + `IconActionProps` from the package root |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx` | EDIT | relabel `Name`→`Secret ref`; drop `Kind` + `Has key` rows; reorder rows; header vault Badge + relative added-time |
| `ui/packages/app/tests/model-details-dialog.test.tsx` | CREATE | row-order, relabel, vault-badge (both branches), header relative-time cases (`tests/` kebab-case convention) |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` | EDIT | Created cell → `Time format="relative"`; delete `DATE_FORMATTER` + `formatCreatedAt` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.test.tsx` | CREATE | relative Created cell + no-bespoke-formatter cases (colocated, per neighbouring secrets tests) |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | EDIT | Activity/Cordon/Drain/Revoke → `IconAction`; `ACTION_CONFIG` gains `icon`; `HostCell` → `Time`; delete `fmt()` |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.test.tsx` | CREATE | every-row-action-has-a-name, Revoke-destructive, host-cell-uses-Time cases (colocated) |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (delete `DATE_FORMATTER`/`formatCreatedAt`/`fmt` outright — no third bespoke formatter is added; `Time` is the one home), **NLR** (touch-it-fix-it: editing `SecretsList.tsx`/`RunnerList.tsx` removes their private formatters in the same diff, not a shim), **ORP** (grep the removed formatter symbols to zero after deletion), **UFS** (badge label strings "In vault"/"Keyless endpoint", the "Added" prefix, and each runner action label live as named consts in the ui/ manual-UFS pass — `audits/ufs.sh` skips ui/ string-dups), **TST-NAM** (new test names milestone-free).
- **`dispatch/write_ts_adhere_bun.md`** — every `.ts`/`.tsx` edit: `const`/import discipline, `import type`, no default export for the new primitive, colocated `bun`/vitest tests. Carries the **UI GATE** and **DESIGN TOKEN GATE** below.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no Zig touched |
| PUB / Struct-Shape | no | TypeScript only; no Zig pub surface. `IconAction` is a named-export function component (TS FILE SHAPE: functions-module — passive render, no bound state) |
| File & Function Length (≤350/≤50/≤70) | yes | all four touched files stay well under 350; `IconAction.tsx` is a single small component. Deleting `fmt`/`DATE_FORMATTER` shrinks the app files |
| UFS (repeated/semantic literals) | yes | manual ui/ pass: badge/label/prefix strings as named consts; runner action labels stay in `ACTION_CONFIG` (single home) |
| **UI GATE (UIS)** | yes | compose only design-system primitives — `IconAction` wraps `Button`+`Tooltip`; callers use `IconAction`/`Badge`/`Time`. No raw `<button>`/`<time>` added; the runner actions stop being bare `<Button>` text and become the named primitive |
| **DESIGN TOKEN GATE (DTK)** | yes | no arbitrary `*-[...]` values introduced; `size="icon-sm"` and existing Badge/Time tokens carry all sizing. If the header layout needs spacing, use `gap-*`/`p-*` token utilities, never `gap-[…]` |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | presentation only; `ui/packages/app` is outside the RULE OBS trigger surface (`src/**/*.zig`, `agentsfleet/src/**/*.js`) |

## Prior-Art / Reference Implementations

- **Reference:** `ui/packages/design-system/src/design-system/Time.tsx` — the `format="relative"` + default tooltip already ships; §1/§2/§4 consume it verbatim, adding no formatting code.
- **Reference:** `ui/packages/app/components/layout/Shell.tsx` (`size="icon"`) — the only existing icon-only Button use in the app, sidebar chrome, not a row action. `IconAction` establishes the row-action standard the app lacks; divergence: it fixes `size="icon-sm"` and requires `label`, which Shell's ad-hoc use does not.
- **Reference:** `ui/packages/app/.../secrets/components/SecretsList.tsx` `SecretActions` — existing per-row action buttons already pass an `aria-label`; `IconAction` makes that discipline unskippable by type rather than by author memory.

## Sections (implementation slices)

### §1 — Model-details dialog reads as a summary

The dialog's job is a fast read of one registry entry. Relabel the `Name` row to **Secret ref** (keep the value — it is the vault key ref, e.g. "pioneer", which is why `Name` and `Provider` both showed "pioneer"). Remove the `Kind` row entirely and the `Has key` Yes/No row. Row order becomes **Provider, Model, Secret ref, then Endpoint (when `base_url` present)**. Move key presence and creation into the header: a status **Badge** reading "In vault" when `has_key`, "Keyless endpoint" when not; and the creation time as `<Time format="relative">` phrased "Added <relative>" (renders e.g. "Added … ago"; the absolute timestamp is one hover away). **Target header layout:** title = `model_id`; beneath it the relative added-time on the left and the vault Badge on the right. **Implementation default:** vault Badge uses `variant="green"` for "In vault" and `variant="default"` for "Keyless endpoint" — the muted default reads as informational, not alarming.

- **Dimension 1.1** — `Kind` and `Has key` rows are gone; remaining rows render in order Provider, Model, Secret ref, Endpoint(when present) → Test `test_details_row_order_no_kind`
- **Dimension 1.2** — the former `Name` row is labelled "Secret ref" and still renders `target.secret_ref` → Test `test_secret_ref_row_label`
- **Dimension 1.3** — header Badge reads "In vault" when `has_key` is true and "Keyless endpoint" when false → Test `test_vault_badge_reflects_has_key`
- **Dimension 1.4** — the header shows an "Added <relative>" `Time` (`format="relative"`) and `created_at` no longer appears as a row in the description list → Test `test_added_time_relative_in_header`

### §2 — Secrets "Created" goes relative

The Secrets list "Created" column prints a bespoke locale-pinned absolute string. Point it at `<Time value={new Date(created_at)} format="relative">` so it reads a relative "… ago" label with the absolute timestamp in the tooltip, and delete `DATE_FORMATTER` and `formatCreatedAt` — no bespoke formatter survives (RULE NDC). **Implementation default:** pass the millisecond `created_at` through `new Date(...)` at the call site, matching `Time`'s `value: string | Date` input; `Time`'s own NaN guard renders "—" for a bad timestamp.

- **Dimension 2.1** — the Created cell renders a `Time` with `format="relative"` whose tooltip carries the absolute timestamp → Test `test_secrets_created_relative`
- **Dimension 2.2** — `DATE_FORMATTER` and `formatCreatedAt` are removed from `SecretsList.tsx`; zero bespoke date-formatting code remains in the file → Test `test_secretslist_no_bespoke_formatter`

### §3 — `IconAction`: the named icon-only row-action primitive

The app has no icon-only row-action pattern, and Indy asked for a *standardized* one. Add `IconAction` to the design system: it composes `Button(size="icon-sm")` inside a `Tooltip`, and its `label: string` prop is **required by the type** and feeds BOTH the visible tooltip body AND the button's `aria-label`. An icon-only action therefore cannot ship without an accessible name — the compiler rejects it. Size is fixed to `icon-sm` internally (not a prop), so the standard is uniform; `variant` passes through (`outline` default, `destructive` for terminal actions), as do `onClick`, `disabled`, `type`, `ref`. Export from both design-system barrels. **Implementation default:** the icon is `children`; `IconAction` does not re-implement `Tooltip`/`Button` — it arranges them.

- **Dimension 3.1** — `IconAction` (imported from `@agentsfleet/design-system`) renders a `size="icon-sm"` button whose `aria-label` equals `label`; omitting `label` is a TypeScript error → Test `test_icon_action_accessible_name`
- **Dimension 3.2** — hovering the trigger surfaces a tooltip whose text equals `label` → Test `test_icon_action_tooltip_shows_label`
- **Dimension 3.3** — `variant="destructive"` and passthrough props (`onClick`, `disabled`) reach the underlying Button unchanged → Test `test_icon_action_variant_and_passthrough`

### §4 — Runner list adopts the standard

Replace the runner list's text action buttons (Activity, Cordon, Drain, Revoke) with `IconAction`, keeping Revoke's `destructive` intent. `ACTION_CONFIG` gains an `icon` field per action (a `lucide-react` glyph; the existing `label` becomes the accessible name); Activity keeps its `ActivityIcon`. `HostCell`'s enrolled/last-seen timestamps adopt `<Time format="relative">` (deleting the locale-pinned `fmt()`), so "enrolled <relative> · last seen <relative>" with absolute tooltips. **Implementation default:** the agent picks recognizable glyphs for cordon/drain/revoke; correctness rides on the `label`, not the glyph choice.

- **Dimension 4.1** — every rendered runner-row action (Activity + the state-appropriate Cordon/Drain/Revoke) is an `IconAction` with a non-empty accessible name → Test `test_runner_row_actions_have_accessible_names`
- **Dimension 4.2** — the Revoke action renders with the `destructive` variant/intent → Test `test_revoke_destructive_intent`
- **Dimension 4.3** — `HostCell` renders enrolled/last-seen via `Time` and `fmt()` is deleted from the file → Test `test_hostcell_uses_time_no_fmt`

## Interfaces

```
NEW primitive — ui/packages/design-system/src/design-system/IconAction.tsx
  export function IconAction(props: IconActionProps): JSX.Element
  export interface IconActionProps
    extends Omit<ButtonProps, "size" | "aria-label" | "children"> {
    label: string;          // REQUIRED — sets aria-label AND tooltip body (no default)
    children: ReactNode;     // the icon glyph
    // variant?: ButtonVariant   (inherited; defaults "outline")
    // onClick/disabled/type/ref (inherited from ButtonProps)
  }
  // renders: <Tooltip><TooltipTrigger asChild>
  //            <Button size="icon-sm" aria-label={label} {...rest}>{children}</Button>
  //          </TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>
  // size is fixed to "icon-sm" — NOT overridable.

CHANGED — ModelDetailsDialog row set (no prop/signature change):
  rows: Provider, Model, Secret ref, Endpoint(when base_url)   // Kind + Has key removed
  header: DialogTitle=model_id · <Time format="relative"> "Added …" · Badge("In vault"|"Keyless endpoint")

CHANGED — ACTION_CONFIG[action] gains: icon: ReactNode        // label stays the accessible name
```

No API route, request/response shape, `TenantModelEntry`/`RunnerListItem` field, or `Time`/`Button`/`Badge` signature changes.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Invalid timestamp | `created_at`/`last_seen_at` is NaN or unparseable | `Time`'s existing NaN guard renders "—"; no crash, no thrown RangeError → covered by `test_time_invalid_timestamp_dash` |
| Nameless icon action | author omits `label` on `IconAction` | TypeScript compile error (required prop); the build fails before merge — asserted by the type in `test_icon_action_accessible_name` |
| Keyless endpoint entry | `has_key` is false | header Badge reads "Keyless endpoint" (not "In vault"); no `Has key: No` row → negative branch of `test_vault_badge_reflects_has_key` |
| Never-connected runner | `last_seen_at` is 0 | host cell keeps the existing "never connected" string; `Time` is used only for the real enrolled/last-seen values → `test_hostcell_uses_time_no_fmt` |
| Tooltip provider absent | a future caller mounts `IconAction` outside `TooltipProvider` | the `aria-label` is on the Button independent of the provider, so the accessible name survives even if the floating tooltip does not render → asserted by `test_icon_action_accessible_name` |

## Invariants

1. Every `IconAction` has an accessible name — `label: string` is a required prop with no default; TypeScript rejects omission. Enforced by the type and asserted by `test_icon_action_accessible_name` + `test_runner_row_actions_have_accessible_names`, never by review.
2. Zero bespoke date formatters remain in the touched app files — `Time` is the single formatting home. Enforced by grep in `test_secretslist_no_bespoke_formatter` + `test_hostcell_uses_time_no_fmt` (assert `DATE_FORMATTER`/`formatCreatedAt`/`fmt` absent).
3. `IconAction` size is always `icon-sm` — `size` is not exposed as a prop; asserted by the rendered class in `test_icon_action_accessible_name`.
4. Relative `Time` labels stay hydration-safe — `Time` sets `suppressHydrationWarning` for `format="relative"` and locale-pins its absolute default, so the Server-Side Rendering (SSR) markup and the first Client-Side Rendering (CSR) paint never mismatch. Adopting `Time` inherits this; the deleted `fmt()`/`DATE_FORMATTER` pinned the locale by hand for exactly this reason, and `Time` subsumes that pin.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | pure presentation-layer restructuring; adds no analytics event, renames none | unchanged | unchanged — no new data leaves the client | existing UI suites stay green |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_details_row_order_no_kind` | entry with `base_url` set → description terms are exactly [Provider, Model, Secret ref, Endpoint] in order; no "Kind"/"Has key" term present |
| 1.2 | unit | `test_secret_ref_row_label` | term "Secret ref" renders and its value equals `target.secret_ref` ("pioneer") |
| 1.3 | unit | `test_vault_badge_reflects_has_key` | `has_key:true`→header Badge "In vault"; `has_key:false`→"Keyless endpoint" (both branches) |
| 1.4 | unit | `test_added_time_relative_in_header` | header contains a `<time>` with "Added …ago" text and a `dateTime` attr; no "Created" row in the list |
| 2.1 | unit | `test_secrets_created_relative` | Created cell renders a `<time>` with relative text; its title/tooltip carries the absolute `formatTimeAbsolute` string |
| 2.2 | unit (grep) | `test_secretslist_no_bespoke_formatter` | `grep -nE "DATE_FORMATTER\|formatCreatedAt" SecretsList.tsx` → 0 matches |
| 3.1 | unit | `test_icon_action_accessible_name` | `<IconAction label="Cordon"><Icon/></IconAction>` → button with `aria-label="Cordon"` and the `icon-sm` size class; omitting `label` fails typecheck |
| 3.2 | unit | `test_icon_action_tooltip_shows_label` | hover/focus the trigger → a tooltip with text "Cordon" appears |
| 3.3 | unit | `test_icon_action_variant_and_passthrough` | `variant="destructive"` + `onClick` + `disabled` → destructive classes; click on enabled fires handler; disabled blocks it |
| 4.1 | unit | `test_runner_row_actions_have_accessible_names` | active-state row → Activity+Cordon+Drain+Revoke each an `IconAction`; every action button has a non-empty accessible name |
| 4.2 | unit | `test_revoke_destructive_intent` | Revoke action button carries the destructive variant class |
| 4.3 | unit (render+grep) | `test_hostcell_uses_time_no_fmt` | host cell renders `<time>` for enrolled/last-seen; `grep -nE "\bfmt\b" RunnerList.tsx` → 0 matches |
| FM-NaN | unit | `test_time_invalid_timestamp_dash` | `created_at:NaN` → cell shows "—", no throw (Failure Mode row 1) |

Regression: the existing model/secrets/runner render suites must stay green after the row and formatter changes (no behaviour beyond presentation moved). Idempotency/replay: N/A — no retry semantics.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Details dialog: no Kind/Has-key rows, Secret-ref label, header badge + relative added-time (§1) | `make test-unit-app` | exit 0 incl. the four `test_details_*`/`test_secret_ref_*`/`test_vault_badge_*`/`test_added_time_*` cases | P0 | |
| R2 | Secrets Created renders relative (§2) | `make test-unit-app` | exit 0 incl. `test_secrets_created_relative` | P0 | |
| R3 | `IconAction` requires an accessible name (§3) | `make test-unit-design-system` | exit 0 incl. `test_icon_action_accessible_name` | P0 | |
| R4 | Every runner row action has a name; Revoke stays destructive (§4) | `make test-unit-app` | exit 0 incl. `test_runner_row_actions_have_accessible_names` + `test_revoke_destructive_intent` | P0 | |
| R5 | No bespoke date formatter survives (§2/§4) | `grep -rnE "DATE_FORMATTER\|formatCreatedAt\|function fmt\(" ui/packages/app/app --include=*.tsx \| grep -v node_modules` | no output | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | App unit tests pass | `make test-unit-app` | exit 0 | P0 | |
| S2 | Design-system unit tests pass | `make test-unit-design-system` | exit 0 | P0 | |
| S3 | Lint clean (incl. UI + design-token audits) | `make lint` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted (all app files are edited; the design-system files are additive).

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `DATE_FORMATTER`, `formatCreatedAt` (SecretsList) | `grep -rnE "DATE_FORMATTER\|formatCreatedAt" ui/packages/app` | 0 matches |
| `fmt` (RunnerList locale-pinned formatter) | `grep -n "function fmt(" ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | 0 matches |
| `Kind` / `Has key` row terms + `target.kind` in the dialog | `grep -nE "Kind\|Has key\|target\.kind" ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx` | 0 matches |

## Out of Scope

- **A runner Delete action.** Requested in the screenshot feedback, but there is no Delete: `RUNNER_ADMIN_ACTION = {cordon, drain, revoke}` and the daemon serves no `DELETE /v1/runners` route. Adding one is a new destructive endpoint (daemon handler, admin authorization, audit event) — a separate spec, not a UI batch. Recorded here so the omission is deliberate.
- **`RunnerActivityDialog`'s inline `occurred_at` timestamp** (`RunnerDialogs.tsx`) — a separate inline `toLocaleString()`, not the `fmt()` this spec deletes, and in a different file (NLR does not reach across files). Left for a follow-up timestamp sweep to keep this diff precise.
- Any change to `Time`, `Button`, `Badge`, or `Tooltip` themselves — they already carry every capability this spec consumes.
- Migrating other icon buttons (Secrets edit/rename/delete, `Shell.tsx`) to `IconAction` — `IconAction` is introduced here and proven on the runner list; a broader adoption sweep is its own low-risk follow-up.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator opens a model entry and instantly sees "pioneer · gpt-4o · In vault · Added <relative>" instead of two rows both saying "pioneer" and a raw timestamp; and on the runners page the actions are compact icons that name themselves on hover and to a screen reader.
2. **Preserved user behaviour** — every action still works (Activity opens the log, Cordon/Drain/Revoke still confirm and mutate state), every value shown is unchanged, and the absolute timestamp is still reachable — now in a tooltip instead of inline.
3. **Optimal-way check** — adopting the existing `Time`/`Badge`/`Button`/`Tooltip` primitives is the most direct route; the one new artifact (`IconAction`) exists only because the app genuinely lacks an icon-action standard and Indy asked for one. No gap to the unconstrained-optimal shape.
4. **Rebuild-vs-iterate** — iterate: four contained presentation slices plus one small additive primitive; nothing here wants a redesign, and none of it trades away run-to-run determinism (relative labels are deterministic given `now`, and hydration-guarded).
5. **What we build** — the dialog restructure (§1), the Secrets relative Created cell (§2), the `IconAction` primitive (§3), and the runner-list adoption (§4).
6. **What we do NOT build** — a runner Delete action, an activity-dialog timestamp change, any `Time`/`Button` capability, or a full icon-button migration — see Out of Scope.
7. **Fit with existing features** — compounds the design-system primitive set and the admin runners surface; must not destabilize the runner confirm/mutate flow (`ACTION_CONFIG` label/intent stay the confirm-dialog source; only the trigger presentation changes).
8. **Surface order** — UI-first by nature: all three complaints are dashboard-render concerns; no CLI or public API reads any of them. `IconAction` lands in the shared design system so the standard is reusable, not app-local.
9. **Dashboard restraint** — the relative label shows only a computed delta over a real `created_at`/`last_seen_at`; nothing is fabricated, the absolute truth stays one hover away, and a bad timestamp degrades to "—" rather than a guess.
10. **Confused-user next step** — an operator unsure what an icon does hovers it (tooltip) or navigates with a screen reader (aria-label) — both now guaranteed by `IconAction`; the absolute time is in the same tooltip. No ticket, no doc lookup needed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections — three independent presentation edits plus one shared primitive that §4 immediately consumes — each independently testable and DONE-markable. The primitive is separated from its first adopter so the design-system test proves the accessibility invariant in isolation.
- **Alternatives considered:** (a) inline the icon+tooltip+aria-label at each runner action without a primitive — rejected: it reproduces the nameless-icon risk at every future call site and makes the invariant a review habit, not a type. (b) add a relative-time helper local to each list — rejected: `Time` already does it and a local helper is exactly the bespoke formatter RULE NDC/NLR removes.
- **Patch-vs-refactor verdict:** this is a **patch** — additive primitive plus presentation edits that *shrink* the surface (two formatters deleted, two dialog rows removed) rather than restructuring any data flow or component interface.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: empty at creation.
- **Metrics review** — empty at creation.
- **Skill-chain outcomes** — empty at creation.
- **Deferrals** — empty at creation.
