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

**Solution summary:** Rewrite Layer 0 (`tokens.css`) against the spec hex set; remap Layer 1/2 (`theme.css`) so Tailwind utility names (`bg-primary`, `text-foreground`, `border-border`, etc.) point at the new tokens — most components keep working without per-file edits. Self-host Commit Mono (OFL) and add `@fontsource-variable/instrument-sans` (also OFL). Add `WakePulse` as the only signature motion primitive. Edit components only where the spec's principles mandate a specific shape change (Button rounding/font/gradient, Badge mono + smaller radius, numeric components `tabular-nums`). Lock the result in with Playwright dark+light snapshot tests.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `ui/packages/design-system/src/tokens.css` | REWRITE | Layer 0 — replace orange/cyan/Geist palette with the spec hex set + Commit Mono / Instrument Sans `@font-face` declarations + WakePulse keyframe + reduced-motion media block. |
| `ui/packages/design-system/src/theme.css` | REWRITE | Layer 1/2 — remap `--primary` / `--background` / etc. to the new Layer 0 tokens; light-mode block; drop unused decorative tokens. |
| `ui/packages/design-system/src/assets/fonts/commit-mono/*.woff2` | CREATE | Self-host Commit Mono weights 400 / 500 / 600 / 700 (OFL). |
| `ui/packages/design-system/src/assets/fonts/commit-mono/OFL.txt` | CREATE | License attribution. |
| `ui/packages/design-system/src/design-system/WakePulse.tsx` | CREATE | Signature motion primitive; `live: boolean` gate; reduced-motion path. |
| `ui/packages/design-system/src/design-system/WakePulse.test.tsx` | CREATE | Live / static / reduced-motion behavioural coverage. |
| `ui/packages/design-system/src/design-system/Button.tsx` | EDIT | Drop `rounded-full` / gradient / `double-border` variant; mono font; spec sizes; flat `bg-primary` (now `--pulse`). |
| `ui/packages/design-system/src/design-system/Badge.tsx` | EDIT | Mono, `--r-sm`, status-fill / muted-outline split. |
| `ui/packages/design-system/src/design-system/{Label,SectionLabel}.tsx` | EDIT | Mono + uppercase + tracking per `label` / `eyebrow` scale. |
| `ui/packages/design-system/src/design-system/{DataTable,DescriptionList,Pagination,Time}.tsx` | EDIT | `tabular-nums` on numeric cells. |
| `ui/packages/design-system/src/design-system/Card.tsx` | EDIT (verify) | `--r-md` radius + 24 / 16 px padding per spec `Cards`. |
| `ui/packages/design-system/src/design-system/{Input,Textarea,Select}.tsx` | EDIT (verify) | Mono input value; pulse-cyan focus ring via `--pulse-glow`. |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export `WakePulse`, `WakePulseProps`. |
| `ui/packages/design-system/src/index.ts` | EDIT | Re-export `WakePulse`, `WakePulseProps`. |
| `ui/packages/design-system/package.json` | EDIT | Add `@fontsource-variable/instrument-sans`. |
| `ui/packages/design-system/src/design-system/*.test.tsx` (existing) | EDIT | Update assertions where shape / class names changed; preserve coverage. |
| `ui/packages/design-system/tests/visual/*.spec.ts` | CREATE | Playwright snapshot harness covering every component in dark + light. |
| `ui/packages/design-system/playwright.config.ts` | CREATE | Per-package Playwright config (workspace's existing config lives in website/app). |

> Components NOT explicitly EDIT'd (Accordion, Tabs, Tooltip, Dialog, ConfirmDialog, DropdownMenu, Skeleton, Separator, EmptyState, StatusCard, Form, Alert, Terminal, Grid, Section, PageHeader, PageTitle, InstallBlock, List, ListItem, AnimatedIcon, ZombieHandIcon) ride on the theme remap. The Playwright snapshot suite catches any visual regression; if a snapshot fails, the diff grows to fix it (RULE NLR — touch it, fix it).

---

## Sections (implementation slices)

### §1 — Layer 0 token rewrite (`tokens.css`)

Replace every `--z-*` variable with the dark-primary hex set from `docs/DESIGN_SYSTEM.md` §Color: `--bg`, `--surface-{1,2,3}`, `--border`, `--border-strong`, `--text`, `--text-{muted,subtle}`, `--pulse`, `--pulse-{dim,glow}`, status (`--success`, `--warn`, `--error`, `--info`, `--evidence`), and the light-mode mirror under `[data-theme="light"]`. Add `--r-sm: 2px / --r-md: 4px / --r-lg: 6px`. Drop the orange / cyan / decorative tokens with no remaining consumers. The file holds: token definitions, `@font-face` blocks, the WakePulse `@keyframes pulse`, the `prefers-reduced-motion` override. Stay under the 350-line gate.

**Implementation default:** name tokens unprefixed (`--bg`, `--pulse`) per spec — current `--z-*` prefix exists only because of historical scoping concerns now obsolete; verified before commit that no consumer outside the package reads `--z-*` directly.

### §2 — Layer 1/2 semantic bridge (`theme.css`)

Rewrite the `:root` and `.dark` blocks so semantic names (`--background`, `--foreground`, `--primary`, `--card`, `--border`, `--ring`, `--success`, `--warning`, `--info`, `--muted`, `--accent`) point at the new Layer 0 tokens. Keep the `@theme inline` shape so Tailwind utilities (`bg-primary`, `text-foreground`, `border-border`, `ring-ring`) continue to work. `--primary` maps to `--pulse`; `--ring` to `--pulse`; `--background` to `--bg`. Drop `--primary-bright` / `--primary-glow` / `--primary-glow-strong` (orange-era only). Expose `--pulse-glow` as the focus-ring shadow source.

### §3 — Font stack swap

Self-host Commit Mono (OFL) under `src/assets/fonts/commit-mono/` with `@font-face` declarations in `tokens.css` (weights 400, 500, 600, 700). Add `@fontsource-variable/instrument-sans` to `package.json` devDependencies; consumers import via `import "@fontsource-variable/instrument-sans"` from their entry — design-system itself only declares the family. Set `--font-mono: "Commit Mono", ui-monospace, monospace;` and `--font-sans: "Instrument Sans Variable", system-ui, sans-serif;`. Drop every Geist reference from the package.

**Implementation default:** Commit Mono v1.143 (latest stable available at https://commitmono.com); fetch `.woff2` directly per the OFL. License file copied to `src/assets/fonts/commit-mono/OFL.txt`.

### §4 — `<WakePulse />` primitive

Add `WakePulse` accepting `live: boolean` (required) plus `as?: keyof JSX.IntrinsicElements`, `children?: ReactNode`, plus standard HTML pass-through. When `live === true`, applies the `data-live` attribute that the CSS `@keyframes pulse` keys off (per `docs/DESIGN_SYSTEM.md` §Motion: `box-shadow: 0 0 0 0 var(--pulse-glow)` → `0 0 0 10px transparent`, 2.4s ease-in-out infinite). When `live === false`, renders the same element WITHOUT the attribute (no animation, no glow). Re-export from package root.

**Implementation default:** the keyframe lives in `tokens.css`; the component is a pure CSS-class gate — no JS animation libraries.

### §5 — Component shape audit

For each file in `Files Changed` marked EDIT: apply the minimal change to satisfy the spec's component principles. Button drops `rounded-full` for `--r-md` and `font-sans` for `font-mono`; primary variant becomes a flat `bg-primary` (which now maps to `--pulse`); `double-border` and `linear-gradient` variants are removed (RULE NDC — no consumer post-rewrite, verified by grep). Badge switches to mono + `--r-sm`. Numeric-table components add `font-variant-numeric: tabular-nums` (Tailwind utility `tabular-nums`) on every number cell. Form fields verified to render the pulse-cyan focus ring via the existing `ring-ring` utility (it now resolves to `--pulse`).

### §6 — Reduced-motion path

Add `@media (prefers-reduced-motion: reduce)` block in `tokens.css` that overrides `[data-live]` to render a static glow ring at 0.2 opacity (`box-shadow: 0 0 0 4px var(--pulse-glow)`, no animation). Hover transitions retained at 50ms (functional, not decorative).

### §7 — Visual regression harness

Add a Playwright snapshot suite under `ui/packages/design-system/tests/visual/`. Entry: a single Vite-mounted test page that renders every component twice (dark + light). The suite asserts pixel-stable snapshots, attaches them to the PR, and runs in CI.

**Implementation default:** Playwright's existing version (`1.59.1`) already pinned in workspace `website` + `app`; baseline images committed under `tests/visual/__snapshots__/`.

---

## Interfaces

```ts
export interface WakePulseProps extends HTMLAttributes<HTMLElement> {
  live: boolean;
  as?: keyof JSX.IntrinsicElements;
  children?: ReactNode;
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

@keyframes pulse { 0% { box-shadow: 0 0 0 0 var(--pulse-glow); }
                   50% { box-shadow: 0 0 0 10px transparent; }
                   100% { box-shadow: 0 0 0 0 transparent; } }

[data-live] { animation: pulse 2.4s ease-in-out infinite; }

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
| `wake_pulse_live_animates` | When `live=true`, the rendered element carries `data-live`; computed style includes `animation-name: pulse`. |
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
| `double-border` Button variant | `grep -rn 'double-border' ui/packages/design-system/src/` | 0 |
| `--primary-bright`, `--primary-glow`, `--primary-glow-strong` | `grep -rn 'primary-bright\|primary-glow' ui/packages/design-system/src/` | 0 |
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
