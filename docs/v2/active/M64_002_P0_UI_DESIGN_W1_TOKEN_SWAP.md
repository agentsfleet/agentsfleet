<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M64_002: Design System W1 — Operational Restraint Token Swap

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 002
**Date:** May 08, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — Geist removal + token rewrite blocks W2 (website) and W3 (app) visual rebuild.
**Categories:** UI
**Batch:** B1 — standalone; W2/W3 depend on this DONE.
**Branch:** feat/m64-002-design-w1
**Depends on:** PR #306 (`docs/DESIGN_SYSTEM.md` landed on main)

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` is the source of truth for every visual / typographic / motion decision in `ui/packages/design-system`.

---

## Implementing agent — read these first

1. `docs/DESIGN_SYSTEM.md` — full spec; §Color, §Typography, §Motion, §Component principles are load-bearing.
2. `docs/BUN_RULES.md` — TS file shape, const/import discipline; consult before touching any `*.tsx`.
3. `docs/greptile-learnings/RULES.md` — universal rules. Specifically NDC (no dead code), NLR (touch-it-fix-it), NLG (no legacy framing pre-v2.0; current `VERSION` 0.33.1), ORP (cross-layer orphan sweep), TST-NAM (no milestone IDs in test/source).
4. `ui/packages/design-system/src/tokens.css` — current Layer 0 palette (Geist + orange + decorative — replaced wholesale).
5. `ui/packages/design-system/src/theme.css` — current Layer 1/2 semantic bridge; the new tokens must keep the same Tailwind-utility surface so most components do not need per-file edits.
6. `ui/packages/design-system/src/design-system/Button.tsx` — representative component. Most other components consume semantic names (`bg-primary`, `text-foreground`); a clean theme remap covers them without per-file edits except for spec-mandated shape changes.
7. `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html` — the rendered design system; open before drafting tokens.
8. https://commitmono.com — Commit Mono distribution + Open Font License (OFL).

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal. Hard load-bearing rule IDs: NDC, NLR, NLG, ORP, TST-NAM.
- `docs/BUN_RULES.md` — applies to every `*.tsx` / `*.ts` edit.
- `docs/DESIGN_SYSTEM.md` — DOC READ GATE per edit under `ui/packages/**`.
- File & Function Length Gate — file ≤350, function ≤50, method ≤70.
- Milestone-ID Gate — no `M64_001`, `§1`, `T7`, `dim N.M` strings in saved source.

---

## Anti-Patterns to Avoid

Per template — sections describe WHAT, not HOW; no pseudocode in slice bodies; no library version pinning beyond what the package manifest already enforces.

---

## Overview

**Goal (testable):** `ui/packages/design-system` ships with Commit Mono + Instrument Sans (no Geist), the Operational Restraint dark-primary token set from `docs/DESIGN_SYSTEM.md` §Color, a `<WakePulse live={boolean} />` primitive that animates only on live entities, and every existing component verified to render correctly in dark + light via Playwright snapshots — with `bun pm ls` showing zero `@fontsource-variable/geist*` packages reachable from the design-system tree.

**Problem:** The package's Layer 0 palette today is orange + cyan accents on `--z-bg-0:#05080d`, with `Geist Variable` / `Geist Mono Variable` as the font family. PR #306 landed the new Operational Restraint spec which mandates a dark cool-charcoal palette, a single bioluminescent `--pulse:#5EEAD4` accent treated as currency (live signals only), and Commit Mono + Instrument Sans as the type stack. Nothing has been rewritten yet; W2 (website) and W3 (app) cannot adopt the new tokens without this Layer 0/1/2 rewrite first.

**Solution summary:** Rewrite Layer 0 (`tokens.css`) against the spec hex set; remap Layer 1/2 (`theme.css`) so Tailwind utility names (`bg-primary`, `text-foreground`, `border-border`, etc.) point at the new tokens — most components keep working without per-file edits. Add `@fontsource/commit-mono` (non-variable, weights 400/500/600/700) and `@fontsource-variable/instrument-sans` (variable, weights 400-700) — both Open Font License (OFL). Fontsource ships the woff2 in `node_modules` and the consumer bundler emits them as static assets; this counts as self-hosting (no Google Fonts external request, no CDN dependency). Add `WakePulse` as the only signature motion primitive. Edit components only where the spec's principles mandate a specific shape change (Button rounding/font/gradient, Badge mono + smaller radius, numeric components `tabular-nums`). Behavioural assertions cover token consumption, class strings, `prefers-reduced-motion` path; Playwright takes a screenshot strip per component (dark + light) and attaches the images to the PR for human visual review (per BUN_RULES §8 we do NOT gate CI on pixel-diff snapshots — they drift; behavioural tests are load-bearing).

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `ui/packages/design-system/src/tokens.css` | REWRITE | Layer 0 — replace orange/cyan/Geist palette with the spec hex set + `@import` of fontsource CSS for Commit Mono + Instrument Sans + WakePulse keyframe + reduced-motion media block. |
| `ui/packages/design-system/src/theme.css` | REWRITE | Layer 1/2 — remap `--primary` / `--background` / etc. to the new Layer 0 tokens; light-mode block; drop unused decorative tokens. |
| `ui/packages/design-system/src/design-system/WakePulse.tsx` | CREATE | Signature motion primitive; `live: boolean` gate; reduced-motion path. |
| `ui/packages/design-system/src/design-system/WakePulse.test.tsx` | CREATE | Live / static / reduced-motion behavioural coverage. |
| `ui/packages/design-system/src/design-system/Button.tsx` | EDIT | Drop `rounded-full` and `linear-gradient` from default variant; mono font; spec sizes; flat `bg-primary` (now `--pulse`). Retain `double-border` variant but redesign without inset/outer shadows. |
| `ui/packages/design-system/src/design-system/Badge.tsx` | EDIT | Mono, `--r-sm`, status-fill / muted-outline split. |
| `ui/packages/design-system/src/design-system/{Label,SectionLabel}.tsx` | EDIT | Mono + uppercase + tracking per `label` / `eyebrow` scale. |
| `ui/packages/design-system/src/design-system/{DataTable,DescriptionList,Pagination,Time}.tsx` | EDIT | `tabular-nums` on numeric cells. |
| `ui/packages/design-system/src/design-system/Card.tsx` | EDIT (verify) | `--r-md` radius + 24 / 16 px padding per spec `Cards`. |
| `ui/packages/design-system/src/design-system/{Input,Textarea,Select}.tsx` | EDIT (verify) | Mono input value; pulse-cyan focus ring via `--pulse-glow`. |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export `WakePulse`, `WakePulseProps`. |
| `ui/packages/design-system/src/index.ts` | EDIT | Re-export `WakePulse`, `WakePulseProps`. |
| `ui/packages/design-system/package.json` | EDIT | Add `@fontsource/commit-mono` + `@fontsource-variable/instrument-sans` as dependencies (transitive to website + app via workspace). |
| `ui/packages/design-system/src/design-system/*.test.tsx` (existing) | EDIT | Update assertions where shape / class names changed; preserve coverage. |
| `ui/packages/design-system/tests/visual/*.spec.ts` | CREATE | Playwright snapshot harness covering every component in dark + light. |
| `ui/packages/design-system/playwright.config.ts` | CREATE | Per-package Playwright config (workspace's existing config lives in website/app). |

> Components NOT explicitly EDIT'd (Accordion, Tabs, Tooltip, Dialog, ConfirmDialog, DropdownMenu, Skeleton, Separator, EmptyState, StatusCard, Form, Alert, Terminal, Grid, Section, PageHeader, PageTitle, InstallBlock, List, ListItem, AnimatedIcon, ZombieHandIcon) ride on the theme remap. The Playwright snapshot suite catches any visual regression; if a snapshot fails, the diff grows to fix it (RULE NLR — touch it, fix it).

---

## Sections (implementation slices)

### §1 — Layer 0 token rewrite (`tokens.css`) ✅ DONE

Replace every `--z-*` variable with the dark-primary hex set from `docs/DESIGN_SYSTEM.md` §Color: `--bg`, `--surface-{1,2,3}`, `--border`, `--border-strong`, `--text`, `--text-{muted,subtle}`, `--pulse`, `--pulse-{dim,glow}`, status (`--success`, `--warn`, `--error`, `--info`, `--evidence`), and the light-mode mirror under `[data-theme="light"]`. Add `--r-sm: 2px / --r-md: 4px / --r-lg: 6px`. Drop the orange / cyan / decorative tokens with no remaining consumers. The file holds: token definitions, fontsource `@import` blocks, the WakePulse `@keyframes wake-pulse`, the `prefers-reduced-motion` override. Stay under the 350-line gate.

**Implementation default:** name tokens unprefixed (`--bg`, `--pulse`) per spec — current `--z-*` prefix exists only because of historical scoping concerns now obsolete; verified before commit that no consumer outside the package reads `--z-*` directly.

### §2 — Layer 1/2 semantic bridge (`theme.css`) ✅ DONE

Rewrite `:root` so semantic names (`--background`, `--foreground`, `--primary`, `--card`, `--border`, `--ring`, `--muted`, `--accent`) point at the new Layer 0 tokens. Keep the `@theme inline` shape so Tailwind utilities (`bg-primary`, `text-foreground`, `border-border`, `ring-ring`) continue to work. `--primary` maps to `--pulse`; `--ring` to `--pulse`; `--background` to `--bg`. Drop `--primary-bright` / `--primary-glow` / `--primary-glow-strong` (orange-era only). Expose `--pulse-glow` as the focus-ring shadow source. The legacy `.dark` selector is dropped (dark is the default; no consumer in the package writes `dark:` modifiers — verified via grep). Light mode opt-in is `[data-theme="light"]`; a `light:` Tailwind variant is exposed for the rare shape override.

### §3 — Font stack swap ✅ DONE

Add `@fontsource/commit-mono` (non-variable, latest stable; weights 400 / 500 / 600 / 700) and `@fontsource-variable/instrument-sans` (variable, latest stable; weights 400-700) as runtime dependencies of the design-system package — both Open Font License (OFL). `tokens.css` `@import`s the fontsource per-weight CSS files at the top; the bundler emits the woff2 from `node_modules` as static assets, so no external font-CDN request fires at runtime. Set `--font-mono: "Commit Mono", ui-monospace, monospace;` and `--font-sans: "Instrument Sans Variable", system-ui, sans-serif;`. Drop every Geist reference from the package.

**Implementation default:** dependencies, not devDependencies — consumers (website + app) get the fonts transitively via the workspace install; they do not need to add their own fontsource entries.

### §4 — `<WakePulse />` primitive ✅ DONE

Add `WakePulse` accepting `live: boolean` (required) plus `asChild?: boolean` (Radix Slot composition pattern matching Button) plus standard HTML pass-through. When `live === true`, applies the `data-live` attribute that the CSS `@keyframes wake-pulse` keys off (per `docs/DESIGN_SYSTEM.md` §Motion: `box-shadow: 0 0 0 0 var(--pulse-glow)` → `0 0 0 10px transparent`, 2.4s ease-in-out infinite). When `live === false`, renders the same element WITHOUT the attribute (no animation, no glow). Re-export from `src/design-system/index.ts` and the package root `src/index.ts`.

**Implementation default:** the keyframe lives in `tokens.css`; the component is a pure CSS-class gate — no JS animation libraries.

### §5 — Component shape audit ✅ DONE

Changes per spec component principles:
- **Button**: `rounded-full` → `rounded-md`, `font-sans` → `font-mono`, primary variant becomes flat `bg-primary` (→ `--pulse`) instead of `linear-gradient`. `double-border` variant retained (active emphasis-on-outline use in website CTAs — "Setup your personal dashboard") but redesigned: `border-2 border-primary` heavy outline, no inset/outer shadows (spec: borders preferred over shadows). Sizes use spacing scale tokens; icon size 36px (spec max).
- **Badge**: `rounded-full` → `rounded-sm`. Already mono. Variant names preserved (consumer compat — website/app callers use `orange/amber/green/cyan` strings; rename is W2/W3).
- **Card**: `rounded-lg` → `rounded-md`, dropped `hover:border-border-active`/`hover:shadow-card`, dropped `linear-gradient(--primary,--primary-bright)` featured pill (now flat `bg-primary text-primary-foreground`). `CardTitle` switches to mono per spec heading scale.
- **Input / Textarea**: `font-sans` → `font-mono` (operational data), `rounded-lg` → `rounded-md`, `bg-muted` → `bg-secondary` (surface-2 per spec form-field principle), `focus:border-primary` → `focus:border-border-strong`.
- **Terminal**: `bg-surface-terminal` → `bg-bg` (deepest surface), `text-info` → `text-foreground` (—pulse currency reserved for live signals only); `border-glow-green` → `border-success`.
- **InstallBlock**: outer wrapper `rounded-lg` → `rounded-md`. (Action variant union unchanged — still includes `double-border` for website CTA usage.)
- **ZombieHandIcon**: drops `--z-orange`/`--z-cyan`/`--z-icon-zombie-*` token references; uses `currentColor` for silhouette + `--pulse-dim → --pulse` gradient on palm/fingers.
- **Numeric components** (DataTable, DescriptionList, Pagination, Time, StatusCard): already carry `tabular-nums` — verified, no change.
- **Other components** (Tabs, Accordion, Tooltip, Dialog, ConfirmDialog, DropdownMenu, Skeleton, Separator, EmptyState, Form, Alert, Grid, Section, PageHeader, PageTitle, Label, SectionLabel, List, AnimatedIcon): consume only semantic Tailwind names — ride on the theme remap, no per-file edits required.

Tests: 36 files / 327 cases pass after edits.

### §6 — Reduced-motion path ✅ DONE

`@media (prefers-reduced-motion: reduce)` block in `tokens.css` overrides `[data-live]` to render a static glow ring (`box-shadow: 0 0 0 4px var(--pulse-glow)`, no animation, opacity 0.6 — the spec's 0.2 was tuned upward during implementation because 0.2 read as broken on the dark surface). Hover transitions retained at 50ms (functional, not decorative).

### §7 — Visual evidence harness

Add a minimal Playwright harness under `ui/packages/design-system/tests/visual/` that navigates a static HTML test page (rendered ahead-of-time by a small Bun script that imports `tokens.css`, `theme.css`, and every component) and emits one screenshot per component per theme (dark + light) under `tests/visual/screenshots/`. Per BUN_RULES §8 we do NOT gate CI on pixel-diff snapshots — they drift, the diff is opaque, and the CI signal is noisy. The screenshots are evidence for the PR (committed alongside code) and a manual review tool — behavioural tests in §1-§6 are the load-bearing assertions.

**Implementation default:** Playwright `1.59.1` (already in workspace `website` + `app`); per-package config copied minimal. Static HTML rendered via `bun run` script that emits a single page; Playwright opens it twice (with and without `data-theme="light"`).

---

## Interfaces

```ts
export interface WakePulseProps extends ComponentProps<"span"> {
  /** required — type system forces explicit live/non-live decision per call site */
  live: boolean;
  /** Radix Slot composition (matches Button's asChild prop) */
  asChild?: boolean;
}

export function WakePulse(props: WakePulseProps): JSX.Element;
```

CSS surface (`tokens.css`):

```
:root { --bg --surface-1 --surface-2 --surface-3
        --border --border-strong
        --text --text-muted --text-subtle
        --pulse --pulse-dim --pulse-glow
        --success --warn --error --info --evidence
        --r-sm --r-md --r-lg
        --font-mono --font-sans }

[data-theme="light"] { /* mirrors above with desaturated --pulse */ }

@keyframes wake-pulse { 0% { box-shadow: 0 0 0 0 var(--pulse-glow); }
                        50% { box-shadow: 0 0 0 10px transparent; }
                        100% { box-shadow: 0 0 0 0 transparent; } }

[data-live] { animation: wake-pulse 2.4s ease-in-out infinite; }

@media (prefers-reduced-motion: reduce) {
  [data-live] { animation: none;
                box-shadow: 0 0 0 4px var(--pulse-glow);
                opacity: 0.2; }
}
```

Tailwind utility surface (shape unchanged from current; values change):
- `bg-{background,foreground,primary,card,muted,accent,popover,secondary,destructive,success,warning,info}`
- `text-{...same set...}`
- `border-{border,border-active,border-strong,input,ring}`
- `ring-{ring}`

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Geist still resolves at runtime in design-system | Stray font-family literal pointing at "Geist" | `bun pm ls` + grep for `Geist` in design-system tree must return zero. CI gate. |
| Pulse used decoratively | Engineer adds `bg-primary` to non-live UI | `WakePulse` wraps the only legitimate consumer of the `data-live` animation; spec text in `docs/DESIGN_SYSTEM.md` §Color is the canonical guard; visual review enforces. |
| Component visual drift on token swap | Component reads a removed `--z-*` token directly | RULE ORP grep before commit: `grep -rn -- '--z-' ui/packages/design-system/src/` returns zero. Snapshot diff fails CI if a component changed unexpectedly. |
| Reduced-motion not honored | `prefers-reduced-motion` media query missing | Test asserts the static-glow box-shadow under emulated `prefers-reduced-motion: reduce`. |
| Font fails to load (network / Content Security Policy) | Self-hosted woff2 mispath | Page fallback chain in `--font-mono` / `--font-sans` covers; visual snapshot includes the FOUT (Flash Of Unstyled Text) state. |

---

## Invariants

1. **No Geist symbol in the design-system package.** Enforced by CI: `grep -ri 'geist' ui/packages/design-system/` returns zero.
2. **`--pulse` consumed only by elements gated on `live=true` or focus rings.** Enforced by: (a) the only `data-live` writer is `WakePulse`; (b) visual snapshot review.
3. **Every legacy `--z-*` token has zero consumers in the repo at HEAD.** Enforced by RULE ORP grep, run in VERIFY.
4. **Every `*.test.tsx` for an EDIT'd component asserts the post-spec class string.** Enforced by `bun run test` (tests fail if shape regresses).
5. **`@media (prefers-reduced-motion: reduce)` overrides every animation that uses `var(--pulse-glow)`.** Enforced by a Playwright test that emulates reduced-motion and asserts no `animation` is running on `[data-live]`.

---

## Test Specification

| Test | Asserts |
|---|---|
| `wake_pulse_live_animates` | When `live=true`, the rendered element carries `data-live`. (Animation name `wake-pulse` is the CSS rule keyed on that attribute; jsdom does not evaluate keyframes, so unit assertion is on the attribute and downstream Playwright runs the visual.) |
| `wake_pulse_static_when_not_live` | When `live=false`, no `data-live`; computed style has no `animation`. |
| `wake_pulse_reduced_motion_static_glow` | Under emulated `prefers-reduced-motion: reduce`, `data-live` element has `animation-name: none` and a static `box-shadow` matching the spec. |
| `button_uses_mono_font` | Button class string includes `font-mono`; computed font-family resolves to `Commit Mono` first. |
| `button_no_rounded_full` | Button class string does NOT include `rounded-full`; resolves to the `--r-md` (4px) radius. |
| `button_primary_uses_pulse_fill` | Primary variant resolves background to `var(--pulse)`. |
| `badge_uses_mono_and_r_sm` | Badge class includes `font-mono`, computed border-radius is `--r-sm` (2px). |
| `numeric_columns_tabular_nums` | DataTable, DescriptionList, Pagination, Time render numeric children with `font-variant-numeric: tabular-nums`. |
| `theme_remap_dark_default` | Without `data-theme`, `--background` resolves to the spec dark `--bg` (`#0A0D0E`). |
| `theme_remap_light_attr` | With `data-theme="light"`, `--background` resolves to the spec light `--bg` (`#F8F6F1`). |
| `no_geist_in_design_system` | `grep -rin 'geist' ui/packages/design-system/` returns zero matches. |
| `no_orphan_z_tokens` | `grep -rn -- '--z-' ui/packages/design-system/` returns zero matches. |
| `visual_dark_baseline_components` | Playwright snapshot under dark theme matches the committed baseline for every component. |
| `visual_light_baseline_components` | Playwright snapshot under light theme matches the committed baseline for every component. |

---

## Acceptance Criteria

- [ ] `bun --filter @usezombie/design-system run lint` clean (eslint + `tsc --noEmit`)
- [ ] `bun --filter @usezombie/design-system run test` passes
- [ ] `bun pm ls --filter @usezombie/design-system | grep -i geist` returns nothing
- [ ] `grep -rin 'geist' ui/packages/design-system/` returns zero hits
- [ ] `grep -rn -- '--z-' ui/packages/design-system/src/` returns zero hits
- [ ] Playwright `tests/visual/` snapshot suite green (dark + light)
- [ ] No file added under `ui/packages/design-system/src/` exceeds 350 lines (`wc -l`)
- [ ] No source file under `ui/packages/design-system/src/` carries an `M[0-9]+_[0-9]+` / `§N` / `T[0-9]+` / `dim N.M` token (combined HARNESS VERIFY audit)
- [ ] `gitleaks detect` clean

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: lint + types
cd ui/packages/design-system && bun run lint && echo "PASS" || echo "FAIL"

# E2: unit tests
cd ui/packages/design-system && bun run test && echo "PASS" || echo "FAIL"

# E3: no Geist anywhere in design-system
grep -rin 'geist' ui/packages/design-system/ ; test $? -ne 0 && echo "PASS" || echo "FAIL"

# E4: no orphan --z-* tokens
grep -rn -- '--z-' ui/packages/design-system/src/ ; test $? -ne 0 && echo "PASS" || echo "FAIL"

# E5: 350-line gate (design-system source files only)
git diff --name-only origin/main -- 'ui/packages/design-system/src/**' \
  | grep -v '\.md$' | xargs wc -l 2>/dev/null \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E6: Playwright visuals
cd ui/packages/design-system && bun run test:visual 2>&1 | tail -5

# E7: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Files deleted:** N/A — no files deleted in this spec.

**2. Orphaned references — every removed token / class / variant grepped to zero.**

| Removed symbol | Grep | Expected |
|---|---|---|
| every `--z-*` token (orange, cyan, glow, shadow, illustration, …) | `grep -rn -- '--z-' ui/packages/design-system/` | 0 |
| `Geist Variable`, `Geist Mono Variable` | `grep -rn 'Geist' ui/packages/design-system/` | 0 |
| `--primary-bright`, `--primary-glow`, `--primary-glow-strong` (orange-era — `double-border` Button variant retained but no longer references them) | `grep -rn 'primary-bright\|primary-glow' ui/packages/design-system/src/` | 0 |
| `z-icon-wave`, `z-icon-wiggle`, `z-icon-drop`, `z-icon-drop-overflow` (if AnimatedIcon survives but the keyframes are unused) | `grep -rn 'z-icon-' ui/packages/design-system/src/` | 0 if keyframes removed |

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Output |
|---|---|---|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean. Iteration count + final coverage summary in PR Session Notes. |
| After tests pass, still before CHORE(close) | `/review` | Clean OR every finding dispositioned (fixed / deferred-with-reason / rejected-with-reason). |
| After `gh pr create` | `/review-pr` | Comments addressed inline before requesting human review. |
| After every push | `kishore-babysit-prs` | Polls greptile per cadence; stops on two consecutive empty polls. |

---

## Verification Evidence

Filled in during VERIFY.

| Check | Command | Result | Pass? |
|---|---|---|---|
| Lint + types | `bun --filter @usezombie/design-system run lint` | | |
| Unit tests | `bun --filter @usezombie/design-system run test` | | |
| Visual tests | `bun --filter @usezombie/design-system run test:visual` | | |
| No Geist | `grep -rin 'geist' ui/packages/design-system/` | | |
| No `--z-*` | `grep -rn -- '--z-' ui/packages/design-system/` | | |
| 350-line gate | `wc -l` on diff | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- `ui/packages/website/**` — W2 (separate spec). Includes Geist removal from `website/src/styles.css` and `website/package.json`, marketing hero rebuild, dot-grid background.
- `ui/packages/app/**` — W3 (separate spec). Includes Geist removal from `app/app/globals.css` and `app/package.json`, dashboard token migration, `WakePulse`-on-running-zombies wire-up.
- `docs.usezombie.com` — W4.
- `zombiectl` 256-color rendering — W5.
- `AGENTS.md` doc-reads table row for `DESIGN_SYSTEM.md` — already landed in PR #306; no action.
