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

# M63_005: Try usezombie CTA shimmer + hand gradient + drop-overflow

**Prototype:** v2.0.0
**Milestone:** M63
**Workstream:** 005
**Date:** May 07, 2026
**Status:** PENDING
**Priority:** P2 — marketing polish on the primary signup CTA; visible on every marketing page render.
**Categories:** DESIGN_SYSTEM, WEBSITE
**Batch:** B1 — independent of M63_004 (CLI resilience); UI-only diff with no server contract.
**Branch:** feat/m63-005-cta-shimmer-hand-drop (to be created at CHORE(open))
**Depends on:** None.

**Canonical architecture:** N/A — pure presentation layer; no architectural surface touched.

---

## Implementing agent — read these first

1. `ui/packages/website/src/styles.css:284-330` — current `.header-mission-control` definition (135deg orange→cyan static gradient, 18px icon slot). Mirror its variable usage (`--z-orange`, `--z-cyan`, `--z-orange-bright`, `--z-amber`) for the shimmer keyframe.
2. `ui/packages/design-system/src/tokens.css:190-240` — `z-icon-drop` keyframe + AnimatedIcon keyframe block. The "drop" animation is the existing pattern to extend; the new on-hover overflow drop must reuse the same easing curve so the motion language stays coherent.
3. `ui/packages/design-system/src/theme.css:166` — `--animate-drop` shorthand. Adding a new "drop-overflow" variant goes alongside, not instead.
4. `ui/packages/design-system/src/design-system/AnimatedIcon.tsx` — wrapper that maps `animation="drop"` to a CSS class. New `animation="drop-overflow"` variant slots in next to `drop`.
5. `ui/packages/design-system/src/design-system/ZombieHandIcon.tsx` — the SVG. The current per-path `fill={colors.handFill}` is what gets replaced with a gradient `<linearGradient>` def. Per-instance gradient id is required to avoid SSR/hydration collisions when more than one icon renders.
6. `ui/packages/website/src/components/domain/background-beams-with-collision.tsx` — pattern reference for the Agents page falling beams. The hand drop-overflow's easing/direction language should mirror this so the two motions feel like one design system.
7. `ui/packages/website/src/App.tsx:128` — only call site of the inline `<AnimatedIcon trigger="parent-hover" animation="drop">` in the CTA. Switching to `animation="drop-overflow"` is the wiring change.

If `prefers-reduced-motion: reduce` is asserted, all three animations (shimmer, drop, drop-overflow) must collapse to a static state. Mirror the existing `@media (prefers-reduced-motion: reduce)` blocks in `tokens.css`.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline. Specifically: RULE FLL (file-length), RULE NLR (no legacy retained — extending an existing animation, not adding a parallel one), RULE TST-NAM (no milestone IDs in test names).
- **`docs/BUN_RULES.md`** — TS file shape, const/import discipline; applies to AnimatedIcon.tsx and ZombieHandIcon.tsx edits.
- **UI Component Substitution Gate** — N/A; this spec edits design-system primitives themselves, not consumers.

Standard set is the floor; no other rule files apply (no Zig, no schema, no HTTP handler, no auth flow).

---

## Overview

**Goal (testable):** The header `Try usezombie` CTA pill animates a continuous orange→amber→cyan→orange gradient shimmer (~6s loop), the embedded `ZombieHandIcon` fills with the same orange→cyan gradient, and on parent-hover the hand drops past the button bottom by ~200px and remains partly visible — three independent presentation behaviors that compose without breaking AA contrast on the label or the existing focus-visible ring.

**Problem:** The CTA gradient currently exists but is static; users don't feel it's "alive" on landing pages. The hand glyph is a flat fill that reads disconnected from the gradient pill it sits in. The drop animation on hover is bounded by the button's `overflow: hidden` ancestors, so the hand barely moves — it doesn't mirror the visual language of the Agents page falling beams that the rest of the marketing site uses.

**Solution summary:** Three additive presentation changes on the existing CTA, all in design-system + website CSS, no React structural changes. (1) New `z-cta-shimmer` keyframe driving a 600%-wide gradient image position-shift on `.header-mission-control`. (2) Per-instance `<linearGradient>` def in `ZombieHandIcon` SVG, used as `fill` on hand/finger paths — wrist + nails + line work stay token-driven for contrast. (3) New `drop-overflow` AnimatedIcon variant with a longer translateY (settles ~200px below origin) and `overflow: visible` carve-out on the icon slot ancestor chain.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/website/src/styles.css` | EDIT | Add `z-cta-shimmer` keyframe, attach to `.header-mission-control`; pause on `:hover`; carve `overflow: visible` on `.header-mission-control` + `.header-mission-control-icon` to let the hand drop past the pill. |
| `ui/packages/design-system/src/tokens.css` | EDIT | Add `z-icon-drop-overflow` keyframe (longer translateY, settle below baseline). |
| `ui/packages/design-system/src/theme.css` | EDIT | Register `--animate-drop-overflow` shorthand alongside `--animate-drop`. |
| `ui/packages/design-system/src/design-system/AnimatedIcon.tsx` | EDIT | Add `"drop-overflow"` to the animation union and class-mapping. |
| `ui/packages/design-system/src/design-system/ZombieHandIcon.tsx` | EDIT | Add `<linearGradient>` defs + per-instance id; switch hand/finger/thumb path `fill` to the gradient ref. Wrist, nails, and stroke work stay token-driven. |
| `ui/packages/website/src/App.tsx` | EDIT | Flip the CTA's `animation="drop"` to `animation="drop-overflow"`. |
| `ui/packages/design-system/src/design-system/AnimatedIcon.test.tsx` | EDIT | Cover the new `drop-overflow` variant + reduced-motion fallback. |
| `ui/packages/design-system/src/design-system/ZombieHandIcon.test.tsx` | EDIT (or CREATE) | Assert per-instance gradient id uniqueness across two render calls. |
| `ui/packages/website/src/App.test.tsx` | EDIT | Assert the CTA renders with `animation="drop-overflow"` and the shimmer class is wired. |

---

## Sections (implementation slices)

### §1 — Shimmer on the CTA gradient

The `.header-mission-control` background becomes a 600%-wide multi-stop gradient (`var(--z-orange) 0%, var(--z-amber) 25%, var(--z-cyan) 50%, var(--z-amber) 75%, var(--z-orange) 100%`) with `background-size: 600% 100%`. The shimmer keyframe pans `background-position` from `0% 50%` to `100% 50%` over ~6s linear, infinite. On `:hover` the animation pauses (so the user sees the moment they engage). On `:focus-visible` the animation continues — the focus ring is the engagement signal there. Honors `prefers-reduced-motion: reduce` by collapsing to the existing 135deg static gradient.

**Implementation default:** 6s loop because a faster pan reads as nervous and a slower pan stops feeling alive. The agent picks the exact step count (multi-stop) that delivers a smooth pan without visible banding.

### §2 — Orange→cyan gradient fill on the hand

Add a `<defs><linearGradient id={…}>` to `ZombieHandIcon` with two stops (`var(--z-orange)` at 0%, `var(--z-cyan)` at 100%) and apply it as `fill="url(#…)"` on the palm + finger + thumb paths (the `colors.handFill` sites). The wrist (`colors.wristFill`), nails (`colors.nailFill`), and stroke work (`colors.line`) stay token-driven — those carry the silhouette and must remain readable against any background the icon is dropped into.

**Implementation default:** per-instance id derived from `useId()` (React 18+) so multiple `ZombieHandIcon` instances in the same DOM don't collide. The agent picks the prefix string and decides whether to memoize the defs.

### §3 — Drop-overflow on hover

Define `z-icon-drop-overflow` as the existing `z-icon-drop` keyframe extended: same start (translateY(-110%) opacity 0), same easing (`cubic-bezier(0.32, 0.72, 0, 1)`), but the end frame translates to `translateY(200%)` (well past the button's natural bottom) at opacity 0.45 — leaving the hand partly visible below the pill. The settle is held with `forwards` so the hand stays parked there until the hover ends.

**Hover-out is a snap-back, not a smooth reverse.** When the hover/focus class detaches, the animation class detaches with it; the element returns to its resting state in the same frame. We deliberately do not run a mirrored reverse keyframe — Tailwind's hover utility already removes the class on `pointerleave`, so a "smooth reverse" would require either a separate hover-out keyframe wired by JS or a CSS `transition` on transform/opacity that fights the animation curve. Both add complexity without earning real polish (the hand had already drifted past the pill bottom at low opacity; the snap reads as the cue ending, not as a missed reversal). Tests assert the resting state on `pointerleave` — not a smooth motion path.

`.header-mission-control` and `.header-mission-control-icon` get `overflow: visible` so the dropped hand isn't clipped. The hand's z-index stays below the label so the gradient pill text is never occluded.

**Implementation default:** trigger remains `parent-hover` (existing AnimatedIcon machinery) so no new event wiring is needed. The agent decides whether to debounce the reverse animation if rapid hover-toggling looks twitchy.

---

## Interfaces

No public API changes. The only contract change is internal to design-system:

```
AnimatedIcon.animation: "drop" | "drop-overflow" | …existing variants
```

Adding `"drop-overflow"` to the union is the new export surface. Consumers that already pass `"drop"` are untouched.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Reduced-motion request | User has `prefers-reduced-motion: reduce` | All three animations collapse to a static state — gradient holds at the 135deg start, hand renders in slot, no drop. |
| Multiple `ZombieHandIcon` instances on the same page | Two icons mounted simultaneously (e.g., header CTA + footer CTA) | Per-instance gradient id from `useId()` prevents `<defs>` collision. Test asserts ids differ across renders. |
| SSR hydration mismatch | `useId()` produces a deterministic id; the marketing site is a Vite SPA (CSR), so hydration mismatch is not a concern here. | Test asserts id stability across React strict-mode double-render. |
| `overflow: visible` regression | A future CSS edit re-adds `overflow: hidden` on the header-actions container | Visual test catches the clipped drop. Document the requirement inline as a `WHY` comment. |
| Animation jank on low-power devices | `background-size: 600%` shimmer can be expensive | Use `will-change: background-position` only on `:hover` where it's already stationary; otherwise rely on the browser's default compositor handling. Profile if regression is observed. |

---

## Invariants

1. **AA contrast on the CTA label.** The dark charcoal label color (`#0b0b10`) must clear AA against every shimmer frame. Enforced by Playwright visual diff at three intermediate frames + the existing focus-ring contrast assertion.
2. **Per-instance gradient id uniqueness.** Two `ZombieHandIcon` mounts on the same page yield two distinct `<linearGradient>` ids. Enforced by a unit test that mounts twice and queries the DOM.
3. **Reduced-motion compliance.** Under `prefers-reduced-motion: reduce`, the CTA gradient is static (no animation) and the hand does not drop on hover. Enforced by a CSS unit test that toggles the media query and asserts computed-animation = "none".
4. **Existing `drop` animation untouched.** Other consumers of `AnimatedIcon animation="drop"` (any) keep their current motion. Enforced by leaving `z-icon-drop` and `--animate-drop` exactly as they are; the new variant is additive.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `cta_shimmer_class_wired` (unit, JSDOM) | The CTA element has the class hook the shimmer rule binds to (`header-mission-control`); JSDOM does not parse external stylesheets so this is the only enforceable class-presence claim at unit level. The actual `animation-name`/`animation-duration` are pinned by the Playwright test below. |
| `cta_shimmer_animation_runs_in_browser` (Playwright) | On a real Chromium page render, computed `animation-name` is `z-cta-shimmer` and `animation-duration` is non-zero on `.header-mission-control`. |
| `cta_shimmer_pauses_on_hover` (Playwright) | After hovering the CTA in Chromium, computed `animation-play-state` resolves to `paused`. |
| `cta_shimmer_static_under_reduced_motion` (Playwright) | With `prefers-reduced-motion: reduce` (`page.emulateMedia({ reducedMotion: 'reduce' })`), computed `animation-name` is `none` and the static fallback `linear-gradient(135deg, ...)` background is applied. |
| `hand_gradient_fill_paints_paths` | The palm + each finger + thumb path's `fill` attribute resolves to `url(#…)` referencing a `<linearGradient>` with two stops (orange 0%, cyan 100%). |
| `hand_gradient_id_unique_across_renders` | Mounting two `ZombieHandIcon` instances yields two `<linearGradient>` defs with different ids. |
| `animated_icon_drop_overflow_class` | `<AnimatedIcon animation="drop-overflow">` produces a class that maps to `--animate-drop-overflow`. |
| `cta_drop_overflow_clears_button_bottom` | Playwright: hover the CTA, assert the hand SVG's bounding rect's `top` is below `.header-mission-control`'s `bottom` after the animation settles. |
| `cta_drop_overflow_partially_visible` | Playwright: post-settle, the hand has computed `opacity > 0` and is in the viewport (not clipped to zero). |
| `cta_drop_overflow_snaps_back_on_hover_out` | Playwright: hover the CTA, wait for settle, dispatch `pointerleave`, then on the next animation frame assert the hand SVG's bounding rect matches the resting position (snap-back, not smooth reverse — see §3). |
| `app_cta_uses_drop_overflow` | Unit: `App.test.tsx` finds the CTA AnimatedIcon and asserts `animation === "drop-overflow"`. |

Regression: existing CTA tests (`header-mission-control` rendering, `Try usezombie` label visibility, focus-visible outline, mobile collapse) must keep passing.

---

## Acceptance Criteria

- [ ] `make lint` clean — verify: `make lint`
- [ ] design-system unit tests pass — verify: `cd ui/packages/design-system && bun run test`
- [ ] website unit tests pass — verify: `cd ui/packages/website && bun run test`
- [ ] Playwright e2e on the home + agents pages still green — verify: `cd ui/packages/website && bun run test:e2e -- home agents`
- [ ] No file over 350 lines added — verify: `git diff --name-only origin/main | grep -v -E '\.md$|^vendor/' | xargs wc -l 2>/dev/null | awk '$1 > 350'`
- [ ] Cross-layer orphan sweep clean for any old `drop` shorthand removed — verify: `grep -rn 'animation="drop"' ui/packages/website/src/ | grep -v drop-overflow` must be zero hits.
- [ ] Manual eyeball pass at three viewports (390, 768, 1280) — shimmer pans smoothly, hand drops cleanly, reduced-motion static state holds.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: Lint
make lint 2>&1 | grep -E "✓|FAIL" | tail -5

# E2: Design-system unit
cd ui/packages/design-system && bun run test 2>&1 | tail -5

# E3: Website unit
cd ui/packages/website && bun run test 2>&1 | tail -5

# E4: Playwright (home + agents only — CTA is in the header, visible everywhere)
cd ui/packages/website && bun run test:e2e -- home agents 2>&1 | tail -10

# E5: 350-line gate
git diff --name-only origin/main | grep -v -E '\.md$|^vendor/' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E6: Gitleaks
gitleaks detect 2>&1 | tail -3

# E7: Orphan sweep — no stale `animation="drop"` in website App.tsx
grep -rn 'animation="drop"' ui/packages/website/src/ | head -5
echo "E7: orphan sweep (empty = pass)"
```

---

## Dead Code Sweep

N/A — no files deleted. The new `drop-overflow` variant is additive; the existing `drop` variant remains for any other consumer (none today, but leaving it costs nothing and preserves the design-system contract).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage of the diff against this spec's Test Specification. Iterate until clean. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review against this spec, BUN_RULES.md, accessibility (AA contrast on shimmer frames), and the existing CTA component contract. |
| After `gh pr create` | `/review-pr` | Address greptile feedback inline; runs via `kishore-babysit-prs`. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit (design-system) | `cd ui/packages/design-system && bun run test` | _filled at VERIFY_ | |
| Unit (website) | `cd ui/packages/website && bun run test` | _filled at VERIFY_ | |
| Lint | `make lint` | _filled at VERIFY_ | |
| Playwright | `cd ui/packages/website && bun run test:e2e -- home agents` | _filled at VERIFY_ | |
| Gitleaks | `gitleaks detect` | _filled at VERIFY_ | |
| 350L gate | `wc -l` over diff | _filled at VERIFY_ | |
| Orphan sweep | `grep -rn 'animation="drop"' ui/packages/website/src/` | _filled at VERIFY_ | |

---

## Out of Scope

- Touching the Agents-page `background-beams-with-collision` itself — its falling motion is the reference, not the target.
- Adding new color tokens — the spec uses existing `--z-orange`, `--z-amber`, `--z-cyan`, `--z-orange-bright`. Any new token would belong in a separate design-token spec.
- Replacing the dashboard's CTA (different component, different surface). This spec is marketing-website-only.
- Mobile-specific drop tuning beyond the existing media-query block. If the drop-overflow looks off on small viewports, that's a follow-up spec, not bundled here.
