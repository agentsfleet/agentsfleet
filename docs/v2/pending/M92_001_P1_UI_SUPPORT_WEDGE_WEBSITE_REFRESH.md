<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M92_001: Marketing site repositions to the support-escalation wedge with a hero pipeline diagram

**Prototype:** v2.0.0
**Milestone:** M92
**Workstream:** 001
**Date:** Jun 12, 2026
**Status:** PENDING
**Priority:** P1 — customer-facing: usezombie.com still sells the deploy-failure wake-on-event story while the product positioning (TechStars onepager, Jun 11, 2026) moved to the resident engineer for support escalations; every visitor from the application reads the wrong product
**Categories:** UI
**Batch:** B2 — after M92_002 (the agentsfleet rebrand lands first; every copy string here is authored under the new brand)
**Branch:** — added at CHORE(open)
**Depends on:** M92_002 (brand noun + identity surfaces; this workstream's copy and guard tokens say `agentsfleet`)
**Provenance:** agent-generated (website repositioning session, Jun 12, 2026) — grounded in `~/Downloads/usezombie-techstars-onepager.md` (the submitted positioning), `docs/architecture/archive/office_hours_support_wedge_jun2026.md` (competitive grid + Ideal Customer Profile), and a read of every `ui/packages/website/src` component; re-confirm at PLAN.

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` (visual source of truth — mono typography, the pulse, dark-primary, anti-vibes list bans chat bubbles/gradient meshes/mascots) + `docs/architecture/direction.md` §UI surfaces. The support-escalation loop itself lives only in `docs/architecture/archive/` (non-canon): this spec changes *marketing positioning*, approved via the TechStars submission; reconciling `high_level.md`/`user_flow.md` to the wedge is a named follow-up, not this diff.

---

## Implementing agent — read these first

1. `ui/packages/website/src/components/Hero.tsx` — the canonical hero shape (eyebrow + pulse, mono headline, lede, install copy-row, animated `<Terminal>`); every new section mirrors this voice.
2. `docs/DESIGN_SYSTEM.md` — type ramp (`display-xl` hero, `eyebrow` labels), anti-vibes list, dot-grid-on-hero-only rule; the pipeline diagram must pass this doc.
3. `ui/packages/website/src/marketing-spec.test.ts` + `marketing-no-pr-validator-framing.test.ts` + `vocab-guard.test.ts` — the three copy guards this diff amends or must stay clean against.
4. `~/Downloads/usezombie-techstars-onepager.md` — the copy source: problem statement, eight-step loop, competition table, "we build the engineer, not a wrapper".
5. `dispatch/write_ts_adhere_bun.md` — TS FILE SHAPE verdict, design-system primitive substitution, DESIGN TOKEN gate; read before the first `.tsx` edit.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m92): reposition marketing site to support-escalation wedge`
- **Intent (one sentence):** a visitor from the TechStars application (or any support-org buyer) lands on usezombie.com and reads the product they were pitched — a resident engineer that takes a ticket through investigation, approval-gated remediation, and a customer reply — drawn as a pipeline they can grasp in one glance.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm against the branch: (a) the current guard-test token lists (they may have moved since authoring), (b) the design-system component inventory actually exported (Terminal, LogLine, WakePulse, SectionLabel, DisplayLG confirmed at authoring), (c) `make lint-website` + `make test-unit-website` + the website dry lane are the canonical verification targets. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a head of support or engineering lead scrolls the hero once and says the sentence back: "it reads the ticket, investigates with our logs and code, a human approves, then it ships the fix or replies to the customer." The diagram did it, not the prose.
2. **Preserved user behaviour** — the curl install row, the animated terminal, the per-second pricing section, the FAQ rate answers, `/pricing` and `/agents` routes, and the promo pill all keep working unchanged. Developers who came to install still install in one copy-paste.
3. **Optimal-way check** — copy + one new diagram component inside the existing design system is the most direct path; the full visual redesign waits for `/design-shotgun` (which MUST include a pricing-section structural variant in the zombieos.polsia.app direction — Indy's named preference).
4. **Rebuild-vs-iterate** — iterate. The site repositioned twice before by copy amendment with guard tests pinning each era; this is era three on the same mechanism.
5. **What we build** — repositioned hero copy + dual call to action, a pipeline diagram component with an approval gate and a categorized logo strip, a problem section, the eight-step loop in How-it-works, reframed trust capabilities, a competition table, aligned CTA/FAQ copy, and amended marketing guard tests.
6. **What we do NOT build** — pricing/rate changes (credit plans are an open product decision; rates stay cross-tier-pinned per-second), architecture-doc reconciliation, docs.usezombie.com updates, a Viktor-style chat-bubble treatment (anti-vibes), connector integrations the diagram alludes to.
7. **Fit with existing features** — compounds with the design system (every new section composes existing primitives) and the guard-test pattern; must not destabilize the rates display (`RATES_DISPLAY` is the only pricing source and this diff never touches it).
8. **Surface order** — UI-only by definition (marketing site). No CLI/API surface.
9. **Dashboard restraint** — the logo strip names *source categories the agent reads* (Tickets · Telemetry · Code · Control plane), not partnership claims; no customer logos, no testimonials, no quantitative performance claims until validated.
10. **Confused-user next step** — a visitor who wants proof clicks through to Docs (existing nav) or the design-partner call to action (mailto with a prefilled subject); a developer who wants to try it copies the install command — both affordances are in the hero.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NDC (no dead code: removed copy blocks leave no orphaned components), RULE NRC (no redundant comments), RULE NLR (touch-it-fix-it: stale wake-on-event copy in touched files goes, not lingers), RULE UFS (repeated copy strings → named constants; the pillar tokens are shared verbatim between component and guard test), RULE TST-NAM (no milestone IDs in test names), RULE ORP (orphan sweep on every removed string/component).
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE DECISION at PLAN for each new component; design-system primitive substitution (no raw-HTML where a primitive exists); DESIGN TOKEN gate (no `*-[...]` arbitrary utilities).
- **`docs/DESIGN_SYSTEM.md`** — binding visual rules: anti-vibes list, type ramp, dark-primary, dot-grid hero-only.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` in scope | — |
| PUB / Struct-Shape | no — TS only | — |
| File & Function Length (≤350/≤50/≤70) | yes — new `.tsx` components | each new component is single-purpose; the diagram splits its logo strip into a child component if it approaches the cap |
| UFS (repeated/semantic literals) | yes — pillar tokens + step titles shared between components and guard tests | export named constants from one module; tests import them rather than re-typing |
| UI Substitution / DESIGN TOKEN | yes — every `.tsx` edit | compose `@agentsfleet/design-system` primitives; theme tokens only, no arbitrary values |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — static marketing site, no logging surface | — |

---

## Overview

**Goal (testable):** the rendered homepage carries the support-escalation positioning — pillar tokens (`resident engineer`, `human approval`, `replayable log`, `wake.on.event`) in the hero, an eight-step ticket-to-learn loop, a pipeline diagram with a visible approval gate forking to a Pull Request (PR) card and a customer-reply card — with all three marketing guard tests green and zero unvalidated quantitative claims.

**Problem:** the site sells the previous era ("Your deploy failed. The agent already knows why.") to an audience that was just pitched a resident engineer for support escalations. The buyer (engineering leadership) and co-sponsor (head of support) from the office-hours Ideal Customer Profile find nothing addressed to them; the differentiators that survive the pivot (approval gating, replayable log, open source, self-managed keys) are framed for solo developers.

**Solution summary:** reposition the copy across hero, problem, how-it-works, capabilities, CTA, and FAQ; add one pipeline-diagram component (Cleric-shaped structure in the house terminal aesthetic: inputs → agent core → approval gate → forked outcomes, categorized logo strip beneath); amend the marketing guard tests so the new era is pinned exactly the way the previous two were.

---

## Prior-Art / Reference Implementations

- **UI** → design-system primitives + `theme.css` tokens. The hero comment block in `Hero.tsx` names the canonical "Mockup A" shape — extend it, don't replace it.
- **Diagram structure** → cleric.io homepage (inputs → agent → outputs with categorized integration logos beneath): mirror the *structure*; render in the house mono/log-line aesthetic — explicitly NOT their light-card visual style, NOT viktor.com's chat bubbles (anti-vibes), NOT fin.ai's serif editorial. zombieos.polsia.app is the cleanliness bar for section rhythm.
- **Guard-test amendment** → `marketing-no-pr-validator-framing.test.ts` is the existing pattern for retiring an era's copy; the pillar-token assertion in `marketing-spec.test.ts` is the pattern for pinning the new era.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/website/src/components/Hero.tsx` | EDIT | escalation headline/lede, persona-aware copy, design-partner call to action beside the install row |
| `ui/packages/website/src/components/Hero.test.tsx` | EDIT | assertions track new copy + dual call to action |
| `ui/packages/website/src/components/PipelineDiagram.tsx` | CREATE | the input→agent→gate→fork diagram + categorized logo strip |
| `ui/packages/website/src/components/PipelineDiagram.test.tsx` | CREATE | structure, fork, reduced-motion, local-asset assertions |
| `ui/packages/website/src/components/ProblemSection.tsx` | CREATE | "escalations are engineering investigations disguised as support" |
| `ui/packages/website/src/components/ProblemSection.test.tsx` | CREATE | copy + heading-rank assertions |
| `ui/packages/website/src/components/HowItWorks.tsx` | EDIT | three deploy-era steps become the eight-step ticket→learn loop |
| `ui/packages/website/src/components/HowItWorks.test.tsx` | EDIT | step order assertion |
| `ui/packages/website/src/components/CompetitionTable.tsx` | CREATE | the "stops at" table from the onepager |
| `ui/packages/website/src/components/CompetitionTable.test.tsx` | CREATE | row content assertions |
| `ui/packages/website/src/components/CTABlock.tsx` | EDIT | "Stop chasing failed deploys." → escalation framing |
| `ui/packages/website/src/components/CTABlock.test.tsx` | EDIT | tracks new copy |
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | one new wedge question (what the agent reads / approval posture); rate answers untouched |
| `ui/packages/website/src/components/FAQ.test.tsx` | EDIT | new entry assertion |
| `ui/packages/website/src/pages/Home.tsx` | EDIT | section order + reframed capability blocks |
| `ui/packages/website/src/pages/Home.test.tsx` | EDIT | section-order assertion |
| `ui/packages/website/src/marketing-copy.ts` | CREATE | named constants: pillar tokens, step titles, source categories (RULE UFS home) |
| `ui/packages/website/src/marketing-spec.test.ts` | EDIT | pillar tokens for the new era; forbidden unvalidated-claim strings |
| `ui/packages/website/public/logos/*.svg` | CREATE | vendored monochrome source-logo assets |
| `ui/packages/website/scripts/prebuild.mjs` | EDIT | emit `llms.txt` from `marketing-copy.ts` constants |
| `ui/packages/website/tests/e2e/smoke.spec.ts` | EDIT | new sections render in the dry lane; `/llms.txt` reachable |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream — copy repositioning and the diagram ship together because the hero reads coherently only with both; splitting would put a support-wedge headline above a deploy-era diagram on main.
- **Alternatives considered:** (a) full visual redesign now (polsia direction) — rejected: conflates a positioning fix with a taste project; `/design-shotgun` variants come after the message is right. (b) Copy-only, diagram later — rejected: the diagram *is* the positioning claim ("end-to-end") made legible; the onepager's table is prose-shaped without it.
- **Patch-vs-refactor verdict:** this is a **patch** (era-three copy amendment on a proven mechanism) plus one greenfield component. The named follow-ups: architecture-doc reconciliation spec; visual-refresh spec post `/design-shotgun`.

---

## Sections (implementation slices)

### §1 — Hero repositioning

The first screen says the new product. Headline moves to the escalation moment in the house "memorable thing" voice; lede states the resident-engineer claim with the surviving pillar tokens; the install row and animated terminal stay; a design-partner call to action (mailto, prefilled subject) lands beside the install row. **Implementation default:** keep the `LIVE — wake.on.event` eyebrow — a ticket arriving *is* the wake event, and it preserves a pinned token.

- **Dimension 1.1** — hero carries the era pillar tokens (`resident engineer`, `human approval`, `replayable log`, `wake.on.event`) sourced from `marketing-copy.ts` → Test `test_hero_carries_era_pillar_tokens`
- **Dimension 1.2** — dual call to action: install copy-row preserved verbatim; design-partner mailto present with analytics event → Test `test_hero_dual_cta`

### §2 — Pipeline diagram + categorized logo strip

The Cleric-shaped, house-styled diagram: input categories (Tickets · Telemetry · Code · Control plane) → agent core (triage → root cause → proposal) → a visually-dominant ⏸ human-approval gate → fork to a PR-opened card and a customer-reply card → a learn loop-back. Logo strip beneath, grouped by the four categories, monochrome, vendored local assets. **Implementation default:** static layout with a single `WakePulse`-driven gate animation; `/design-shotgun` may replace the visual treatment later without changing the component's structural assertions.

- **Dimension 2.1** — diagram renders the four input categories and the agent-core stages in order → Test `test_pipeline_renders_inputs_and_stages`
- **Dimension 2.2** — approval gate renders between proposal and outcomes; both outcome cards (PR, customer reply) render → Test `test_pipeline_gate_and_fork`
- **Dimension 2.3** — every logo image resolves from a local `/logos/` asset; zero external URLs in the component → Test `test_pipeline_logos_local_only`
- **Dimension 2.4** — `prefers-reduced-motion` renders the diagram fully static; narrow viewport stacks the three columns vertically → Test `test_pipeline_reduced_motion_and_stacking`

### §3 — Problem section

New section between hero and capabilities: support escalations are engineering investigations disguised as customer support; the answer lives in three places at once (code, internal docs, live production state); support sees only part of the picture. Qualitative only — no ticket-latency numbers, no percentage claims.

- **Dimension 3.1** — section renders the three-places framing with correct heading rank under the hero → Test `test_problem_section_renders`

### §4 — How-it-works becomes the eight-step loop

The three deploy-era steps become the onepager loop: ticket → investigate → root cause → propose remediation → human approval → execute → reply → learn. Each step keeps the house pattern (title + one operational sentence naming real surfaces: event stream, allow-listed tools, approvals plane, `core.zombie_events`).

- **Dimension 4.1** — the eight steps render in loop order with titles from `marketing-copy.ts` → Test `test_how_it_works_eight_steps_in_order`

### §5 — Trust capabilities + competition table

The four capability blocks reframe from solo-developer features to the trust layer the onepager leads with (sandboxed runtime, vaulted credentials, approval gating, open source + full auditability with replay) — same grid, same components. Below, the competition table: three categories and where each stops (answering tickets / diagnosing systems / generating fixes) versus resolving escalations end-to-end. Plain design-system table or definition list — not a feature-comparison checkmark grid.

- **Dimension 5.1** — four reframed capability blocks render with trust-layer copy → Test `test_capabilities_trust_framing`
- **Dimension 5.2** — competition table renders four rows with the "stops at" framing → Test `test_competition_table_rows`

### §6 — CTA + FAQ alignment

`CTABlock` headline moves from "Stop chasing failed deploys." to the escalation claim. FAQ gains one wedge entry (what sources the agent reads and the approval posture); all rate/pricing answers stay byte-identical.

- **Dimension 6.1** — CTA carries escalation framing; no deploy-era copy remains in touched components → Test `test_cta_escalation_framing`
- **Dimension 6.2** — FAQ renders the new wedge entry; rate answers unchanged (regression) → Test `test_faq_wedge_entry_and_rates_regression`

### §7 — Marketing guard amendments

`marketing-spec.test.ts` pins the new era: pillar-token assertion reads the exported constants; a new forbidden-strings block rejects unvalidated quantitative claims (`40%` escalation figures and ticket-latency hour claims) until the bucket-labeling validation lands. Existing `vocab-guard` and `no-pr-validator-framing` guards stay untouched and green.

- **Dimension 7.1** — guard test asserts era pillar tokens via `marketing-copy.ts` imports → Test `test_marketing_spec_pins_new_era`
- **Dimension 7.2** — guard test rejects the unvalidated-claim strings across rendered copy → Test `test_no_unvalidated_quantitative_claims`
- **Dimension 7.3** — rendered copy uses the `agentsfleet` product noun; `usezombie` survives only in operational strings (install command, resolving URLs, package/binary names) → Test `test_brand_noun_guard`

### §8 — LLM-readable surface

The site ships `public/llms.txt` (llms.txt convention: markdown index at the site root) carrying the positioning, the eight-step loop, install command, pricing pointer, and docs links — sourced from `marketing-copy.ts` constants at build time so site copy and the LLM surface cannot drift. **Implementation default:** a prebuild script emits it (the package already has `scripts/prebuild.mjs`); a static hand-edited file is the fallback if build-time generation fights the bundler.

- **Dimension 8.1** — `/llms.txt` is served and carries the era pillar tokens + loop steps → Test `test_llms_txt_present_and_current`

---

## Interfaces

No HTTP/CLI surface. The locked contract is the exported constant module and the guard coupling:

- `marketing-copy.ts` exports: pillar token list, loop step titles (ordered), source-category labels. Components and guard tests both import from it — the strings exist in exactly one place (RULE UFS).
- Component props for `PipelineDiagram`, `ProblemSection`, `CompetitionTable` are internal; no cross-package exports from the website package.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Reduced motion | `prefers-reduced-motion: reduce` | diagram renders final state, no animation classes applied; clipboard-blocked toast path regression-covered by existing Hero tests |
| Narrow viewport | < tablet breakpoint | diagram columns stack vertically; logo strip wraps; no horizontal scroll |
| Missing logo asset | bad path / asset not vendored | static imports fail the build (not a runtime 404); e2e dry lane catches a broken render |
| Accessibility violation | diagram is image-shaped to a screen reader | diagram carries a text alternative describing the full loop; axe assertions in the dry lane stay green |

---

## Invariants

1. No v1 PR-validator framing strings in rendered copy — enforced by the existing `marketing-no-pr-validator-framing.test.ts` (untouched).
2. No standalone "zombie" product noun in rendered copy — enforced by the existing `vocab-guard.test.ts` (untouched).
3. Era pillar tokens present in the hero — enforced by amended `marketing-spec.test.ts` importing `marketing-copy.ts`.
4. No unvalidated quantitative claims (`40%` escalation share, ticket-latency hours) in rendered copy — enforced by the new forbidden-strings block in `marketing-spec.test.ts`.
5. All pricing display strings originate from `RATES_DISPLAY` — enforced by the existing rates pin tests; this diff adds no pricing copy.
6. Logo assets are local imports only — enforced by `test_pipeline_logos_local_only` asserting zero external URL sources.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_hero_carries_era_pillar_tokens` | rendered hero text contains every token exported by `marketing-copy.ts` |
| 1.2 | unit | `test_hero_dual_cta` | install copy-row and design-partner mailto both present; copy click writes `INSTALL_COMMAND` to clipboard (existing behaviour regression) |
| 2.1 | unit | `test_pipeline_renders_inputs_and_stages` | four category labels + triage/root-cause/proposal stages render in document order |
| 2.2 | unit | `test_pipeline_gate_and_fork` | approval-gate element renders between proposal and outcomes; PR card and customer-reply card both present |
| 2.3 | unit | `test_pipeline_logos_local_only` | every rendered img/svg source matches `/logos/`; zero `http(s)://` sources |
| 2.4 | unit | `test_pipeline_reduced_motion_and_stacking` | reduced-motion media mock → no animation class; narrow viewport → stacked layout class |
| 3.1 | unit | `test_problem_section_renders` | three-places copy present; heading rank is h2 under the hero h1 |
| 4.1 | unit | `test_how_it_works_eight_steps_in_order` | eight titles render in the exported order, ticket first, learn last |
| 5.1 | unit | `test_capabilities_trust_framing` | four blocks render sandboxed-runtime / vaulted-credentials / approval-gating / open-source-replay copy |
| 5.2 | unit | `test_competition_table_rows` | four rows; each names its category and its "stops at" boundary |
| 6.1 | unit | `test_cta_escalation_framing` | CTA headline matches new copy; "failed deploys" absent from touched components |
| 6.2 | unit | `test_faq_wedge_entry_and_rates_regression` | new entry renders; rate answer strings byte-equal to `RATES_DISPLAY`-derived values |
| 7.1 | unit | `test_marketing_spec_pins_new_era` | guard test sources tokens from `marketing-copy.ts`, fails when a token is removed from the hero |
| 7.2 | unit | `test_no_unvalidated_quantitative_claims` | seeded forbidden string in a fixture component is detected; live tree has zero hits |
| 7.3 | unit | `test_brand_noun_guard` | rendered copy says `agentsfleet`; `usezombie` only in allowlisted operational strings |
| 8.1 | unit | `test_llms_txt_present_and_current` | emitted `public/llms.txt` contains every pillar token and all eight loop steps in order |
| all | e2e | website dry-lane smoke | homepage renders every section; `/llms.txt` returns 200; axe assertions green; no console errors |

**Regression:** existing Hero clipboard/toast tests, vocab-guard, no-pr-validator-framing, rates pin tests, `/pricing` + `/agents` route renders — all must pass unmodified except where assertions track intentionally changed copy. **Idempotency/replay:** N/A — static site.

---

## Acceptance Criteria

- [ ] Era pillar tokens, guard suite (incl. brand noun + unvalidated-claims), and `llms.txt` test green — verify: `make test-unit-website`
- [ ] Lint clean — verify: `make lint-website`
- [ ] Homepage dry lane renders all sections, `/llms.txt` 200, axe green — verify: `make dry-smoke`
- [ ] `gitleaks detect` clean · no non-md file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1+E2: Website unit tests + lint
make test-unit-website && make lint-website && echo "PASS" || echo "FAIL"
# E3: Dry-lane smoke (website renders, axe)
make dry-smoke
# E4: Gitleaks
gitleaks detect 2>&1 | tail -3
# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E6: Deploy-era orphan sweep (empty = pass)
grep -rn "failed deploys\|deploy failed" ui/packages/website/src --include='*.tsx' --include='*.ts' | grep -v test | head
```

---

## Dead Code Sweep

No files deleted. Removed-copy sweep:
| Removed copy/symbol | Grep | Expected |
|---------------------|------|----------|
| deploy-era headline strings in touched components | E6 above | 0 matches outside tests |
| any capability-block constant orphaned by the §5 reframe | `grep -rn "<old constant name>" ui/packages/website/src` | 0 matches |

## Discovery (consult log)

- **Authoring-time decisions (Indy, Jun 12, 2026 session):** pricing copy stays per-second (credit-plan migration is a separate product decision, not deferred scope of this spec); the curl install motion stays alongside the new design-partner call to action; Cleric = structure reference, polsia = cleanliness bar, Fin = logo treatment, Viktor = tone only (chat bubbles are anti-vibes); the `≥40%` figure stays off the site until ticket bucket-labeling validates it.
- **Amendment (Indy, Jun 12, 2026):** copy authored under the `agentsfleet` brand (M92_002 dependency); `/llms.txt` added (§8); the `/design-shotgun` follow-up MUST include a pricing-section structural variant in the zombieos.polsia.app direction; install command stays on `usezombie.sh` verbatim.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification above | Clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/DESIGN_SYSTEM.md`, `dispatch/write_ts_adhere_bun.md` | Clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Comments addressed before human review |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-website` | | |
| Lint | `make lint-website` | | |
| Dry lane (e2e + axe) | `make dry-smoke` | | |
| Gitleaks | `gitleaks detect` | | |
| Orphan sweep | Eval E6 | | |

---

## Out of Scope

- Pricing-model change (credit plans from the onepager) — open product decision; rates remain cross-tier-pinned per-second across `tenant_billing.zig` / `rates.ts` / `rates.mdx`.
- Architecture-doc reconciliation (`high_level.md`, `user_flow.md` still describe the wake-on-event framing) — follow-up spec when the wedge graduates from positioning to canon.
- Full visual redesign / `/design-shotgun` variant selection — follow-up after this positioning diff lands; the diagram's structural tests survive a re-skin. The shotgun run carries a standing requirement: a pricing-section structural variant in the zombieos.polsia.app direction.
- docs.usezombie.com content updates — separate repo, separate spec.
- Any connector implementation (Zoho Desk, Jira, Datadog ingestion) — the logo strip names source categories, not shipped integrations.
