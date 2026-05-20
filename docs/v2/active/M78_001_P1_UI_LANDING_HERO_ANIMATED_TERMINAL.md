# M78_001: Curl-led hero CTA, animated install terminal, and landing copy/pricing polish

**Prototype:** v2.0.0
**Milestone:** M78
**Workstream:** 001
**Date:** May 21, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the public marketing landing page is the first-touch conversion surface; the flagged bugs degrade it directly.
**Categories:** UI
**Batch:** B1 — runs alongside M75 (`usezombie.sh` installer) via a parallel agent; the two share the install-command surface.
**Branch:** feat/m78-001-landing-hero-animated-terminal
**Depends on:** M75_001 (`usezombie.sh` installer + DNS) — **merge-ordering, not code.** The hero prints `curl -fsSL https://usezombie.sh | bash`; that domain must resolve by the time this PR merges. Do not merge this ahead of M75/DNS being live.
**Provenance:** agent-generated (pre-spec) — Indy chat, May 20–21, 2026 (seven flagged landing bugs + "one command installs both" + pioneer.ai-style animated terminal).

**Canonical architecture:** `docs/architecture/user_flow.md` §8 (cold-machine bootstrap names `https://usezombie.sh` as the human entrypoint) and `docs/DESIGN_SYSTEM.md` (operational-restraint motion principle — this spec adds the system's **second** sanctioned animation and reconciles that doc).

---

## Implementing agent — read these first

1. `ui/packages/website/src/components/Hero.tsx` — current hero (`Button` label that copies+scrolls, separate `Terminal`, `Toast`); the surface §1/§2 reshape.
2. `ui/packages/design-system/src/design-system/WakePulse.tsx` + `ui/packages/design-system/src/tokens.css` (`@keyframes wake-pulse`, the `prefers-reduced-motion` block) — the system's motion convention: animation lives in a CSS keyframe + a data-attribute, reduced-motion is honoured **entirely in CSS**. The animated terminal mirrors this — no JS animation loop.
3. `ui/packages/design-system/src/design-system/Terminal.tsx` — the component §2 extends; note the `copyText`-vs-string-child clipboard resolution and the existing copy-flash timer cleanup.
4. `ui/packages/website/src/lib/rates.ts` + `ui/packages/website/src/components/Pricing.tsx` — `FREE_TRIAL_END_MS` / `FREE_TRIAL_STAGE_NANOS` gate the trial; §5's trial-aware cards read it.
5. M75 spec (`docs/v2/pending/M75_001_…USEZOMBIE_SH_INSTALL_DOMAIN.md`, recover via `git show 9b7978da:` if absent on main) — settles the canonical one-liner `curl -fsSL https://usezombie.sh | bash` (bare root) and owns `marketing-spec.test.ts` banned-string evolution + the `~/Projects/docs` sweep.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Curl-led hero, animated install terminal, landing copy + pricing polish
- **Intent (one sentence):** A visitor sees one copyable install command and a live, colored terminal showing the install actually running, and the pricing/closing/footer copy speaks to a human operator — not to a machine API.
- **Handshake (agent fills at PLAN):** restate intent + `ASSUMPTIONS I'M MAKING: …`; reconcile against Intent before any edit.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — always.
- **RULE UFS** — the install one-liner (`curl -fsSL https://usezombie.sh | bash`) and the slash command (`claude /usezombie-install-platform-ops`) are shared across Hero, OnboardingFlow, and the animated-terminal transcript; pin each once as a named const in `config.ts` and reuse verbatim. (ui/ UFS is manual — extract by hand; the audit skips `ui/`.)
- **RULE NDC** — delete the duplicate Home `InstallBlock` cleanly; no commented-out markup.
- **RULE NLR** — touch-it-fix-it: when the terminal animation lands, update the now-false "the only animation" comments in `WakePulse.tsx` + `tokens.css`.
- **RULE NLG** — pre-2.0: no "legacy"/"V2" framing for the removed npm-led hero.
- **RULE ORP** — orphan sweep after the npm one-liner leaves `config.ts` and the Home block is removed.
- **RULE TNM / TST** — no milestone IDs (`M78`, `§2`) in test names or test-file names.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| DESIGN TOKEN | **yes** | Terminal colors + copy-row use existing tokens (`text-pulse`/`info`/`success`/`muted`); the reveal keyframe uses `tokens.css` motion vars. No `text-[Npx]`, no raw hex. |
| UI Substitution | **yes** | New hero markup composes design-system primitives (`asChild` for HTML semantics); no raw `<button>`/`<section>`. |
| PUB / Struct-Shape | **yes** | Terminal gains an opt-in `animate` prop — a presentation variant of the same component, **not** a new pub type. Shape verdict: single default export keeps its shape; no new public surface. |
| File & Function Length (≤350/≤50/≤70) | **yes** | `Terminal.tsx` is ~160 lines; keep the reveal CSS-driven (keyframe in `tokens.css`) so the component grows by a prop + a class, not a JS loop. Split a helper if any fn nears 50. |
| UFS | **yes** | install one-liner + slash command as named consts (see Rules). |
| LIFECYCLE | **no** | CSS-driven animation → no JS timer to clean up. If a JS reveal proves unavoidable, clear timers on unmount (mirror Terminal's existing copy-flash cleanup) and the gate fires. |
| Architecture Consult & Update | **yes** | Adding the system's second animation departs from the documented restraint principle; Indy's explicit request is the consult (record in Discovery), and `docs/DESIGN_SYSTEM.md` + the two "only animation" comments get updated in the same diff. |
| LOGGING / ERROR REGISTRY / SCHEMA / ZIG | no | no such surface touched. |

---

## Overview

**Goal (testable):** The hero renders `curl -fsSL https://usezombie.sh | bash` as a copy-only command (no scroll/navigation on click), an animated colored terminal reveals the install transcript line-by-line (all lines shown statically under `prefers-reduced-motion`) whose Copy yields exactly `claude /usezombie-install-platform-ops`, the duplicate Home install block is gone, the closing CTA + footer read in human-outcome voice, and the pricing billing cards show stages **free** during the trial window — each asserted by a unit or e2e test.

**Problem (user-facing):** clicking the long install command flings the visitor down the page (feels like navigation); the terminal's Copy hands back a 5-line transcript instead of a runnable command; the closing block and footer pitch the OpenAPI machine surface to a human; the "how a run is billed" cards show `$0.001`/stage in writing while the headline says "free until July 31."

**Solution summary:** Reshape the hero CTA into a copy-row (copy-only) with the replay link relocated beneath it; add a CSS-driven animated colored `Terminal` variant in the design system and use it for the hero install demo with a corrected Copy payload; delete the redundant Home `InstallBlock`; rewrite the closing CTA + footer in human-outcome voice; make the pricing billing cards trial-aware. `INSTALL_COMMAND` becomes the bare-root curl one-liner, coordinated with M75.

---

## Prior-Art / Reference Implementations

- **UI motion** → `WakePulse.tsx` + `tokens.css` `@keyframes wake-pulse`: animation as a CSS keyframe gated by a data-attribute, reduced-motion handled in CSS. **Alignment:** the terminal reveal mirrors this exactly. **Divergence:** WakePulse is an infinite pulse; the terminal is a one-shot staggered line reveal (per-line `animation-delay`, optional `steps()` typing) — same philosophy, different keyframe.
- **UI primitives** → `Terminal.tsx` (copy-payload resolution, chrome strip) reused as the base; `Toast` for the copy confirmation; `theme`/`tokens.css` for color + motion vars.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/website/src/config.ts` | EDIT | `INSTALL_COMMAND` → `curl -fsSL https://usezombie.sh | bash`; add a named const for the `claude /usezombie-install-platform-ops` slash command. |
| `ui/packages/website/src/components/Hero.tsx` | EDIT | curl copy-row (copy-only, no scroll), relocate replay link below it, use the animated Terminal, fix Copy payload to the slash-command const. |
| `ui/packages/design-system/src/design-system/Terminal.tsx` | EDIT | opt-in `animate` prop: CSS-driven line-by-line reveal; reduced-motion shows all lines. |
| `ui/packages/design-system/src/tokens.css` | EDIT | add the line-reveal keyframe + `prefers-reduced-motion` guard; update "only animation" comment. |
| `ui/packages/design-system/src/design-system/WakePulse.tsx` | EDIT | update the now-false "the only animation" comment. |
| `ui/packages/website/src/pages/Home.tsx` | EDIT | remove the duplicate `InstallBlock` section below pricing. |
| `ui/packages/website/src/components/OnboardingFlow.tsx` | EDIT | step 01 caption follows `INSTALL_COMMAND` (no longer "one npm"). |
| `ui/packages/website/src/components/CTABlock.tsx` | EDIT | human-outcome closing copy (drop the OpenAPI machine pitch). |
| `ui/packages/website/src/components/Footer.tsx` | EDIT | footer tagline rewrite + capitalization fix. |
| `ui/packages/website/src/components/Pricing.tsx` | EDIT | trial-aware billing cards: stages show "free" during the window, `$0.001` labelled post-trial. |
| `ui/packages/website/src/lib/rates.ts` | EDIT | a trial-window display helper / post-trial label const, if the cards need one (single source). |
| `docs/DESIGN_SYSTEM.md` | EDIT | record the second sanctioned animation + reconcile the restraint principle. |
| `*.test.tsx` / `*.spec.ts` (Hero, Home, OnboardingFlow, CTABlock, Footer, Pricing, Terminal; e2e home/navigation) | EDIT | re-assert curl one-liner, copy-only behaviour, animated terminal, new copy, trial-aware cards. |

> **Out of this PR's scope (M75 owns):** `infra/install/install.sh|ps1`, the `~/Projects/docs` sweep, and the `marketing-spec.test.ts` banned-string evolution. This PR leaves `marketing-spec.test.ts` untouched — the bare-root curl form never trips the `usezombie.sh/install.sh` ban, and `Agents.tsx` retains the `npm install` bootstrap so the "≥1 npm hit" assertion still holds.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five value slices — hero CTA, animated terminal (design-system), Home block removal, closing copy, pricing — each independently testable and shippable.
- **Alternatives considered:** (a) a JS typing engine for the terminal — rejected: contradicts the CSS-motion convention and adds a lifecycle/cleanup surface; (b) a brand-new `AnimatedTerminal` component — rejected: the animation is a presentation variant, a new pub type would duplicate the chrome/copy logic and widen the public surface.
- **Patch-vs-refactor verdict:** **patch** — extends one component and edits website copy/markup; no architecture rewrite. The curl install path itself is the M75 refactor, tracked separately.

---

## Sections (implementation slices)

### §1 — Hero curl CTA (items 1, 2)
Replace the long command-as-button (which copies *and* smooth-scrolls) with a copy-row: the curl one-liner + a Copy affordance that copies only and surfaces the existing `Toast`, no scroll/navigation. The "view a real wake (replay)" ghost link (→ `/agents`) moves to its own line beneath the command.

- **Dimension 1.1** — hero shows `INSTALL_COMMAND` (the curl one-liner) as a copy-row → Test `test_hero_shows_curl_install_command`
- **Dimension 1.2** — clicking Copy writes the command to the clipboard and shows the toast, and does **not** scroll/navigate → Test `test_hero_copy_is_copy_only_no_scroll`
- **Dimension 1.3** — the replay link renders below the command and points at `/agents` → Test `test_hero_replay_link_below_command`

### §2 — Animated colored Terminal + Copy fix (item 3 + pioneer.ai ask)
Add an opt-in `animate` prop to `Terminal` that reveals its children line-by-line via a CSS keyframe (color comes from the existing `LogLine` severity tokens — pioneer.ai-style liveliness, design-token only). Under `prefers-reduced-motion: reduce` every line is visible immediately. The hero's terminal demonstrates the curl install flow ending in `→ next: claude /usezombie-install-platform-ops`, and its Copy payload is **exactly** that slash-command const.

- **Dimension 2.1** — `Terminal animate` reveals lines via CSS (keyframe + per-line delay), no JS timer → Test `test_terminal_animate_reveals_lines_via_css`
- **Dimension 2.2** — under reduced-motion, all lines render visible immediately → Test `test_terminal_animate_static_under_reduced_motion`
- **Dimension 2.3** — hero terminal Copy yields exactly `claude /usezombie-install-platform-ops` → Test `test_hero_terminal_copy_is_slash_command`
- **Dimension 2.4** — `WakePulse`/`tokens.css` "only animation" comments + `DESIGN_SYSTEM.md` updated to reflect two animations → Test `test_design_system_doc_records_terminal_animation` (grep assertion)

### §3 — Remove duplicate Home install block (item 5)
Delete the `InstallBlock` section below pricing on `Home.tsx`; the 4-step `OnboardingFlow` at the top already covers install + slash command.

- **Dimension 3.1** — Home renders no second standalone install block; OnboardingFlow remains → Test `test_home_has_no_duplicate_install_block`

### §4 — Human-voice closing CTA + footer (items 6, 7)
Rewrite `CTABlock` to a human/outcome message (not the OpenAPI machine pitch — that lives on `/agents`). Rewrite the footer tagline + fix the lowercase "self-managed".

- **Dimension 4.1** — closing CTA copy is human-outcome voice; no "OpenAPI 3.1 / machine surface" string → Test `test_cta_block_human_voice`
- **Dimension 4.2** — footer tagline rewritten, capitalization fixed → Test `test_footer_tagline_rewrite`

### §5 — Trial-aware pricing billing cards (item 4)
Make the "how a run is billed" cards read the trial window: during the free trial the stage cells show **free** (matching the headline), with `$0.001` presented as the post-July-31 rate. The struck rate line and the cards stop contradicting each other.

- **Dimension 5.1** — within the trial window, stage cells render "free" → Test `test_billing_cards_free_during_trial`
- **Dimension 5.2** — outside the trial window, stage cells render `$0.001` → Test `test_billing_cards_paid_after_trial`

---

## Interfaces

```
Terminal (design-system) — additive, backward-compatible:
  animate?: boolean         // opt-in line-by-line reveal; default false = current static render
  // copyable / copyText / children / label / green unchanged.
  // animate has NO effect under prefers-reduced-motion: reduce (all lines visible at once).

config.ts:
  INSTALL_COMMAND            = "curl -fsSL https://usezombie.sh | bash"   // single source: Hero + OnboardingFlow
  INSTALL_SKILL_COMMAND      = "claude /usezombie-install-platform-ops"   // hero terminal Copy payload + transcript tail
```

Contract: `animate` is purely presentational; existing `Terminal` call-sites are unaffected (default false). The clipboard payload is governed by `copyText`/string-child exactly as today.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Clipboard write rejected | denied permission / non-secure context | Hero shows the "manual — select and copy" toast (existing path); no unhandled rejection. |
| `prefers-reduced-motion: reduce` | user setting | terminal renders all lines immediately, no reveal; copy/labels intact. |
| JS disabled / SSR snapshot | static render | curl command + all transcript lines are present in the DOM (animation is progressive enhancement only). |
| usezombie.sh not yet live at merge | M75/DNS lag | **process control, not code:** PR body flags the merge-ordering dependency; reviewer blocks merge until DNS resolves. |

---

## Invariants

1. The install one-liner and slash command each appear from a single named const in `config.ts` — enforced by reuse (no inline duplicate string in Hero/OnboardingFlow/transcript). 
2. `animate` never hides content under reduced-motion — enforced by `test_terminal_animate_static_under_reduced_motion`.
3. The hero terminal Copy payload equals `INSTALL_SKILL_COMMAND` exactly — enforced by `test_hero_terminal_copy_is_slash_command`.
4. The animation is CSS-driven (no `setInterval`/`setTimeout` reveal loop) — enforced by code review against the WakePulse pattern + the absence of a new timer in `Terminal.tsx`.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_hero_shows_curl_install_command` | hero renders `curl -fsSL https://usezombie.sh | bash` |
| 1.2 | unit | `test_hero_copy_is_copy_only_no_scroll` | Copy writes clipboard + toast; `scrollIntoView` not called |
| 1.3 | unit | `test_hero_replay_link_below_command` | replay link present, href `/agents`, ordered after the command |
| 2.1 | unit | `test_terminal_animate_reveals_lines_via_css` | `animate` adds the reveal class/keyframe hooks; lines carry staggered delay |
| 2.2 | unit | `test_terminal_animate_static_under_reduced_motion` | with reduced-motion matched, all lines visible, no reveal class effect |
| 2.3 | unit | `test_hero_terminal_copy_is_slash_command` | clicking terminal Copy writes exactly `claude /usezombie-install-platform-ops` |
| 2.4 | unit | `test_design_system_doc_records_terminal_animation` | `DESIGN_SYSTEM.md` mentions the terminal animation; no surviving "only animation" claim |
| 3.1 | unit | `test_home_has_no_duplicate_install_block` | Home renders one onboarding flow, zero standalone InstallBlock |
| 4.1 | unit | `test_cta_block_human_voice` | CTA copy present; absent: "OpenAPI 3.1", "machine surface" |
| 4.2 | unit | `test_footer_tagline_rewrite` | footer tagline = new string; "Self-managed" capitalized |
| 5.1 | unit | `test_billing_cards_free_during_trial` | with `now < FREE_TRIAL_END_MS`, stage cells = "free" |
| 5.2 | unit | `test_billing_cards_paid_after_trial` | with `now ≥ FREE_TRIAL_END_MS`, stage cells = `$0.001` |
| e2e | e2e | `home page install + replay` | rendered Home: curl command visible, Copy copies it, replay link → /agents (Playwright) |

**Regression:** existing Hero/Home/OnboardingFlow/CTABlock/Footer/Pricing/Terminal tests updated, not duplicated; `marketing-spec.test.ts` must still pass **unchanged**. **Idempotency:** N/A.

---

## Acceptance Criteria

- [ ] Hero shows the curl one-liner; Copy copies only (no scroll) — verify: `vitest run Hero`
- [ ] Animated terminal reveals lines, static under reduced-motion, Copy = slash command — verify: `vitest run Terminal Hero`
- [ ] Home has no duplicate install block — verify: `vitest run Home`
- [ ] CTA + footer in human voice — verify: `vitest run CTABlock Footer`
- [ ] Billing cards trial-aware — verify: `vitest run Pricing`
- [ ] `marketing-spec.test.ts` passes unchanged — verify: `vitest run marketing-spec`
- [ ] `make lint` clean · website `make test` passes · e2e home spec passes
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: website unit tests
(cd ui/packages/website && bun run test) && echo PASS || echo FAIL
# E2: design-system tests (Terminal animate)
(cd ui/packages/design-system && bun run test) && echo PASS || echo FAIL
# E3: lint
make lint 2>&1 | grep -E "✓|FAIL"
# E4: marketing-spec untouched + green
git diff --name-only origin/main | grep -q marketing-spec.test.ts && echo "TOUCHED-INVESTIGATE" || echo "untouched OK"
# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E6: gitleaks
gitleaks detect 2>&1 | tail -3
# E7: orphan sweep — old npm one-liner gone from hero/config
grep -rn "npm install -g @usezombie/zombiectl && npx skills" ui/packages/website/src | head
```

---

## Dead Code Sweep

**1. Orphaned files** — N/A — no files deleted (the Home `InstallBlock` *usage* is removed; the design-system component stays, still used by `Agents.tsx`).

**2. Orphaned references**

| Removed | Grep | Expected |
|---------|------|----------|
| old npm-led `INSTALL_COMMAND` value | `grep -rn "npm install -g @usezombie/zombiectl && npx skills" ui/packages/website/src` | 0 matches |
| Home `InstallBlock` import (if now unused on Home) | `grep -rn "InstallBlock" ui/packages/website/src/pages/Home.tsx` | 0 matches |

---

## Discovery (consult log)

> Empty at creation. Append consults, skill outcomes, Indy-acked deferrals as work proceeds.

- **Architecture consult (pre-recorded):** adding the design system's second animation departs from the documented operational-restraint / "single animation" principle. Indy explicitly requested a pioneer.ai-style animated, colored terminal (chat May 20–21, 2026). Resolution: implement CSS-driven (mirroring WakePulse), update `DESIGN_SYSTEM.md` + the two "only animation" comments in the same diff.
- **Coordination:** M75 (`usezombie.sh` installer + DNS) ships in parallel via another agent. Boundary: M75 owns scripts + docs sweep + `marketing-spec.test.ts`; this PR owns website source and leaves that test file untouched. Merge-ordering: this PR must not merge before M75/DNS is live.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Clean; iteration count + coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Comments addressed before human review. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Website unit | `cd ui/packages/website && bun run test` | {paste} | |
| Design-system unit | `cd ui/packages/design-system && bun run test` | {paste} | |
| Lint | `make lint` | {paste} | |
| marketing-spec untouched | `git diff --name-only origin/main \| grep marketing-spec` | {paste} | |
| e2e home | website Playwright home spec | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- `usezombie.sh` installer scripts, DNS/TLS/hosting, and the `~/Projects/docs` sweep — **M75**.
- Evolving `marketing-spec.test.ts` banned/positive strings to the curl form — **M75**.
- The `Agents.tsx` machine-surface page copy — unchanged (the npm bootstrap there is the deliberate "I have a node toolchain" path).
- A general JS animation framework for the design system — rejected; CSS-driven only.
