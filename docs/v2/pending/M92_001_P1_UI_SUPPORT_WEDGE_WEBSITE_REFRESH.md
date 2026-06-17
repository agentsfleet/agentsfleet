<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M92_001: Marketing site centers compounding operational knowledge with a hero pipeline diagram

**Prototype:** v2.0.0
**Milestone:** M92
**Workstream:** 001
**Date:** Jun 12, 2026
**Status:** PENDING
**Priority:** P1 — customer-facing: usezombie.com still sells the deploy-failure wake-on-event story while the product positioning moved to a resident engineer that compounds operational knowledge from recurring problem classes; every visitor from the application reads the wrong product
**Categories:** User Interface (UI)
**Batch:** B2 — after M92_002 (the agentsfleet rebrand lands first; every copy string here is authored under the new brand)
**Branch:** — added at CHORE(open)
**Depends on:** M92_002 (brand noun + identity surfaces; this workstream's copy and guard tokens say `agentsfleet`)
**Provenance:** agent-generated (website repositioning session, Jun 12, 2026) — grounded in `~/Downloads/usezombie-techstars-onepager.md` (the submitted positioning), `docs/architecture/archive/office_hours_support_wedge_jun2026.md` (competitive grid + Ideal Customer Profile), and a read of every `ui/packages/website/src` component; re-confirm at PLAN.

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` (visual source of truth — mono typography, the pulse, dark-primary, anti-vibes list bans chat bubbles/gradient meshes/mascots) + `docs/architecture/direction.md` §UI surfaces. The compounding operational-knowledge loop itself lives only in `docs/architecture/archive/` (non-canon): this spec changes *marketing positioning*, approved via the TechStars submission + Jun 17 Chief Executive Officer (CEO) review; reconciling `high_level.md`/`user_flow.md` to the wedge is a named follow-up, not this diff.

---

## Implementing agent — read these first

1. `ui/packages/website/src/components/Hero.tsx` — the canonical hero shape (eyebrow + pulse, mono headline, lede, install copy-row, animated `<Terminal>`); every new section mirrors this voice.
2. `docs/DESIGN_SYSTEM.md` — type ramp (`display-xl` hero, `eyebrow` labels), anti-vibes list, dot-grid-on-hero-only rule; the pipeline diagram must pass this doc.
3. `ui/packages/website/src/marketing-spec.test.ts` + `marketing-no-pr-validator-framing.test.ts` + `vocab-guard.test.ts` — the three copy guards this diff amends or must stay clean against.
4. `~/Downloads/usezombie-techstars-onepager.md` — the copy source: problem statement, eight-step loop, competition table, "we build the engineer, not a wrapper".
5. `dispatch/write_ts_adhere_bun.md` — TypeScript (TS) FILE SHAPE verdict, design-system primitive substitution, DESIGN TOKEN gate; read before the first `.tsx` edit.

---

## Pull Request (PR) Intent & comprehension handshake

- **PR title (eventual):** `feat(m92): center website on compounding operational knowledge`
- **Intent (one sentence):** a visitor lands on usezombie.com and reads the product as one resident-engineer loop — first signal → recurring problem class → scenario/test → fix Pull Request (PR) → human review → fewer repeats — drawn as a pipeline they can grasp in one glance.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm against the branch: (a) the current guard-test token lists (they may have moved since authoring), (b) the design-system component inventory actually exported (Terminal, LogLine, WakePulse, SectionLabel, DisplayLG confirmed at authoring), (c) `make lint-website` + `make test-unit-website` + the website dry lane are the canonical verification targets. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a head of support or engineering lead scrolls the hero once and says the sentence back: "it learns the recurring problem class, turns it into a scenario/test and a fix PR, and keeps a human gate before merge or deploy." The diagram did it, not the prose.
2. **Preserved user behaviour** — the curl install row, the animated terminal, the per-second pricing section, the Frequently Asked Questions (FAQ) rate answers, `/pricing` and `/agents` routes, and the promo pill all keep working unchanged. Developers who came to install still install in one copy-paste.
3. **Optimal-way check** — copy + one new diagram component inside the existing design system is the most direct path; the full visual redesign waits for `/design-shotgun` (which MUST include a pricing-section structural variant in the zombieos.polsia.app direction — Indy's named preference).
4. **Rebuild-vs-iterate** — iterate. The site repositioned twice before by copy amendment with guard tests pinning each era; this is era three on the same mechanism.
5. **What we build** — repositioned hero copy + dual call to action, a pipeline diagram component with an approval gate and categorized source strip, a compounding operational-knowledge section, the loop in How-it-works, reframed trust capabilities, a competition table, aligned call-to-action (CTA) / FAQ copy, and amended marketing guard tests.
6. **What we do NOT build** — pricing/rate changes (credit plans are an open product decision; rates stay cross-tier-pinned per-second), architecture-doc reconciliation, docs.usezombie.com updates, a Viktor-style chat-bubble treatment (anti-vibes), connector integrations the diagram alludes to.
7. **Fit with existing features** — compounds with the design system (every new section composes existing primitives) and the guard-test pattern; must not destabilize the rates display (`RATES_DISPLAY` is the only pricing source and this diff never touches it).
8. **Surface order** — UI-only by definition (marketing site). No Command-Line Interface (CLI) / API surface.
9. **Dashboard restraint** — the source strip names categories the agent reads (Signals · Telemetry · Code · Control plane), not partnership claims; no customer logos, no testimonials, no quantitative performance claims until validated; no claim of autonomous merge/deploy while humans sleep.
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

**Goal (testable):** the rendered homepage carries the compounding-operational-knowledge positioning — pillar tokens (`resident engineer`, `human approval`, `replayable log`, `wake.on.event`) in the hero, a signal→problem-class→scenario/test→fix PR→human-review loop, a pipeline diagram with a visible approval gate, and guard tests that reject zero-ticket / autonomous-merge overclaims.

**Problem:** the site sells the previous era ("Your deploy failed. The agent already knows why.") while the product direction moved to compounding operational knowledge: recurring problems become scenarios, tests, approved fixes, and reusable memory. The buyer (engineering leadership) and co-sponsor (head of support) find a support-ticket surface, not the resident-engineer loop that reduces repeat work.

**Solution summary:** reposition the copy across hero, learning loop, how-it-works, capabilities, CTA, and FAQ; add one pipeline-diagram component (Cleric-shaped structure in the house terminal aesthetic: signals/sources → resident engineer core → scenario/test + fix PR → human gate → recurrence reduction); amend the marketing guard tests so the new era is pinned exactly the way the previous two were.

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
| `ui/packages/website/src/components/PipelineDiagram.tsx` | CREATE | the signal→problem-class→scenario/test→fix PR→human-gate diagram + categorized source strip |
| `ui/packages/website/src/components/PipelineDiagram.test.tsx` | CREATE | structure, fork, reduced-motion, local-asset assertions |
| `ui/packages/website/src/components/OperationalKnowledgeSection.tsx` | CREATE | compounding operational knowledge: problem class → scenario/test → fix PR → fewer repeats |
| `ui/packages/website/src/components/OperationalKnowledgeSection.test.tsx` | CREATE | copy + heading-rank assertions |
| `ui/packages/website/src/components/HowItWorks.tsx` | EDIT | three deploy-era steps become the compounding knowledge loop |
| `ui/packages/website/src/components/HowItWorks.test.tsx` | EDIT | step order assertion |
| `ui/packages/website/src/components/CompetitionTable.tsx` | CREATE | the "stops at" + "learns into prevention" table |
| `ui/packages/website/src/components/CompetitionTable.test.tsx` | CREATE | row content assertions |
| `ui/packages/website/src/components/CTABlock.tsx` | EDIT | "Stop chasing failed deploys." → escalation framing |
| `ui/packages/website/src/components/CTABlock.test.tsx` | EDIT | tracks new copy |
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | one new wedge question (what the agent reads / approval posture); rate answers untouched |
| `ui/packages/website/src/components/FAQ.test.tsx` | EDIT | new entry assertion |
| `ui/packages/website/src/pages/Home.tsx` | EDIT | section order + reframed capability blocks |
| `ui/packages/website/src/pages/Home.test.tsx` | EDIT | section-order assertion |
| `ui/packages/website/src/marketing-copy.ts` | CREATE | named constants: pillar tokens, loop titles, source categories (RULE UFS home) |
| `ui/packages/website/src/marketing-spec.test.ts` | EDIT | pillar tokens for the new era; forbidden unvalidated + autonomy-overclaim strings |
| `ui/packages/website/public/logos/*.svg` | CREATE | vendored monochrome source-logo assets |
| `ui/packages/website/scripts/prebuild.mjs` | EDIT | emit `llms.txt` from `marketing-copy.ts` constants |
| `ui/packages/website/tests/e2e/smoke.spec.ts` | EDIT | new sections render in the dry lane; `/llms.txt` reachable |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream — copy repositioning and the diagram ship together because the hero reads coherently only with both; splitting would put a compounding-knowledge headline above a deploy-era diagram on main.
- **Alternatives considered:** (a) full visual redesign now (polsia direction) — rejected: conflates a positioning fix with a taste project; `/design-shotgun` variants come after the message is right. (b) Copy-only, diagram later — rejected: the diagram *is* the positioning claim ("end-to-end") made legible; the onepager's table is prose-shaped without it.
- **Patch-vs-refactor verdict:** this is a **patch** (era-three copy amendment on a proven mechanism) plus one greenfield component. The named follow-ups: architecture-doc reconciliation spec; visual-refresh spec post `/design-shotgun`.

---

## Sections (implementation slices)

### §1 — Hero repositioning

The first screen says the new product. Headline moves to compounding operational knowledge in the house "memorable thing" voice; lede states the resident-engineer claim with the surviving pillar tokens; the install row and animated terminal stay; a design-partner call to action (mailto, prefilled subject) lands beside the install row. **Implementation default:** keep the `LIVE — wake.on.event` eyebrow — a first signal is the wake event, and it preserves a pinned token.

- **Dimension 1.1** — hero carries the era pillar tokens (`resident engineer`, `human approval`, `replayable log`, `wake.on.event`) sourced from `marketing-copy.ts` → Test `test_hero_carries_era_pillar_tokens`
- **Dimension 1.2** — dual call to action: install copy-row preserved verbatim; design-partner mailto present with analytics event → Test `test_hero_dual_cta`

### §2 — Pipeline diagram + categorized logo strip

The Cleric-shaped, house-styled diagram: source categories (Signals · Telemetry · Code · Control plane) → resident engineer core (investigate → identify problem class → generate scenario/test) → fix PR card → visually-dominant ⏸ human-review gate → recurrence-reduced loop-back. Source strip beneath, grouped by category, monochrome, vendored local assets. **Implementation default:** static layout with a single `WakePulse`-driven gate animation; `/design-shotgun` may replace the visual treatment later without changing the component's structural assertions.

- **Dimension 2.1** — diagram renders the four source categories and the resident-engineer stages in order → Test `test_pipeline_renders_sources_and_stages`
- **Dimension 2.2** — scenario/test artifact, fix PR, and human-review gate render in order → Test `test_pipeline_scenario_pr_and_gate`
- **Dimension 2.3** — every logo image resolves from a local `/logos/` asset; zero external URLs in the component → Test `test_pipeline_logos_local_only`
- **Dimension 2.4** — `prefers-reduced-motion` renders the diagram fully static; narrow viewport stacks the three columns vertically → Test `test_pipeline_reduced_motion_and_stacking`

### §3 — Compounding operational knowledge section

New section between hero and capabilities: recurring operational pain is a problem class, not a support artifact. The agent learns from the first signal, captures the failure class, proposes a scenario/test, and turns the approved resolution into reusable operational memory. Qualitative only — no ticket-latency numbers, no percentage claims, no zero-ticket promise.

- **Dimension 3.1** — section renders the compounding knowledge spine with correct heading rank under the hero → Test `test_operational_knowledge_section_renders`

### §4 — How-it-works becomes the eight-step loop

The three deploy-era steps become the compounding loop: first signal → investigate → identify problem class → generate scenario/test → propose fix PR → human review → merge/deploy by humans → learn for the next recurrence. Each step keeps the house pattern (title + one operational sentence naming real surfaces: event stream, allow-listed tools, approvals plane, `core.agent_events`).

- **Dimension 4.1** — the eight steps render in loop order with titles from `marketing-copy.ts` → Test `test_how_it_works_eight_steps_in_order`

### §5 — Trust capabilities + competition table

The four capability blocks reframe from solo-developer features to the trust layer the onepager leads with (sandboxed runtime, vaulted credentials, approval gating, open source + full auditability with replay) — same grid, same components. Below, the competition table: categories and where each stops (answer, route, diagnose, draft, suggest) versus whether they preserve a failure class as scenario/test-backed operational memory. Plain design-system table or definition list — not a feature-comparison checkmark grid.

- **Dimension 5.1** — four reframed capability blocks render with trust-layer copy → Test `test_capabilities_trust_framing`
- **Dimension 5.2** — competition table renders four rows with the "stops at" + "learns into prevention" framing → Test `test_competition_table_rows`

### §6 — CTA + FAQ alignment

`CTABlock` headline moves from "Stop chasing failed deploys." to the escalation claim. FAQ gains one wedge entry (what sources the agent reads and the approval posture); all rate/pricing answers stay byte-identical.

- **Dimension 6.1** — CTA carries escalation framing; no deploy-era copy remains in touched components → Test `test_cta_escalation_framing`
- **Dimension 6.2** — FAQ renders the new wedge entry; rate answers unchanged (regression) → Test `test_faq_wedge_entry_and_rates_regression`

### §7 — Marketing guard amendments

`marketing-spec.test.ts` pins the new era: pillar-token assertion reads the exported constants; a new forbidden-strings block rejects unvalidated quantitative claims (`40%` escalation figures, ticket-latency hour claims), zero-ticket promises, and autonomous merge/deploy claims. Existing `vocab-guard` and `no-pr-validator-framing` guards stay untouched and green.

- **Dimension 7.1** — guard test asserts era pillar tokens via `marketing-copy.ts` imports → Test `test_marketing_spec_pins_new_era`
- **Dimension 7.2** — guard test rejects unvalidated quantitative + autonomy-overclaim strings across rendered copy → Test `test_no_unvalidated_or_autonomy_overclaims`
- **Dimension 7.3** — rendered copy uses the `agentsfleet` product noun; `usezombie` survives only in operational strings (install command, resolving URLs, package/binary names) → Test `test_brand_noun_guard`

### §8 — Large Language Model (LLM)-readable surface

The site ships `public/llms.txt` (llms.txt convention: markdown index at the site root) carrying the compounding operational-knowledge positioning, loop, install command, pricing pointer, and docs links — sourced from `marketing-copy.ts` constants at build time so site copy and the LLM surface cannot drift. **Implementation default:** a prebuild script emits it (the package already has `scripts/prebuild.mjs`); a static hand-edited file is the fallback if build-time generation fights the bundler.

- **Dimension 8.1** — `/llms.txt` is served and carries the era pillar tokens + compounding loop steps → Test `test_llms_txt_present_and_current`

---

## Interfaces

No HTTP/CLI surface. The locked rule is the exported constant module and the guard coupling:

- `marketing-copy.ts` exports: pillar token list, compounding loop titles (ordered), source-category labels, forbidden overclaim strings. Components and guard tests both import from it — the strings exist in exactly one place (RULE UFS).
- Component props for `PipelineDiagram`, `OperationalKnowledgeSection`, `CompetitionTable` are internal; no cross-package exports from the website package.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Reduced motion | `prefers-reduced-motion: reduce` | diagram renders final state, no animation classes applied; clipboard-blocked toast path regression-covered by existing Hero tests |
| Narrow viewport | < tablet breakpoint | diagram columns stack vertically; logo strip wraps; no horizontal scroll |
| Missing logo asset | bad path / asset not vendored | static imports fail the build (not a runtime 404); e2e dry lane catches a broken render |
| Accessibility violation | diagram is image-shaped to a screen reader | diagram carries a text alternative describing the full loop; axe assertions in the dry lane stay green |
| Overclaim copy | ambition section implies zero tickets or autonomous production deploy | guard test fails; primary copy keeps human review before merge/deploy and phrases recurrence reduction directionally |

---

## Invariants

1. No v1 PR-validator framing strings in rendered copy — enforced by the existing `marketing-no-pr-validator-framing.test.ts` (untouched).
2. No standalone "zombie" product noun in rendered copy — enforced by the existing `vocab-guard.test.ts` (untouched).
3. Era pillar tokens present in the hero — enforced by amended `marketing-spec.test.ts` importing `marketing-copy.ts`.
4. No unvalidated quantitative claims (`40%` escalation share, ticket-latency hours), zero-ticket promises, or autonomous merge/deploy claims in rendered copy — enforced by the new forbidden-strings block in `marketing-spec.test.ts`.
5. All pricing display strings originate from `RATES_DISPLAY` — enforced by the existing rates pin tests; this diff adds no pricing copy.
6. Logo assets are local imports only — enforced by `test_pipeline_logos_local_only` asserting zero external URL sources.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_hero_carries_era_pillar_tokens` | rendered hero text contains every token exported by `marketing-copy.ts` |
| 1.2 | unit | `test_hero_dual_cta` | install copy-row and design-partner mailto both present; copy click writes `INSTALL_COMMAND` to clipboard (existing behaviour regression) |
| 2.1 | unit | `test_pipeline_renders_sources_and_stages` | four category labels + investigate/problem-class/scenario-test stages render in document order |
| 2.2 | unit | `test_pipeline_scenario_pr_and_gate` | scenario/test artifact, fix PR card, and human-review gate render in document order |
| 2.3 | unit | `test_pipeline_logos_local_only` | every rendered img/svg source matches `/logos/`; zero `http(s)://` sources |
| 2.4 | unit | `test_pipeline_reduced_motion_and_stacking` | reduced-motion media mock → no animation class; narrow viewport → stacked layout class |
| 3.1 | unit | `test_operational_knowledge_section_renders` | compounding knowledge spine present; heading rank is h2 under the hero h1 |
| 4.1 | unit | `test_how_it_works_eight_steps_in_order` | eight titles render in the exported order, first signal first, learn last |
| 5.1 | unit | `test_capabilities_trust_framing` | four blocks render sandboxed-runtime / vaulted-credentials / approval-gating / open-source-replay copy |
| 5.2 | unit | `test_competition_table_rows` | four rows; each names its category, "stops at" boundary, and prevention-learning posture |
| 6.1 | unit | `test_cta_escalation_framing` | CTA headline matches new copy; "failed deploys" absent from touched components |
| 6.2 | unit | `test_faq_wedge_entry_and_rates_regression` | new entry renders; rate answer strings byte-equal to `RATES_DISPLAY`-derived values |
| 7.1 | unit | `test_marketing_spec_pins_new_era` | guard test sources tokens from `marketing-copy.ts`, fails when a token is removed from the hero |
| 7.2 | unit | `test_no_unvalidated_or_autonomy_overclaims` | seeded forbidden string in a fixture component is detected; live tree has zero hits |
| 7.3 | unit | `test_brand_noun_guard` | rendered copy says `agentsfleet`; `usezombie` only in allowlisted operational strings |
| 8.1 | unit | `test_llms_txt_present_and_current` | emitted `public/llms.txt` contains every pillar token and all compounding loop steps in order |
| all | e2e | website dry-lane smoke | homepage renders every section; `/llms.txt` returns 200; axe assertions green; no console errors |

**Regression:** existing Hero clipboard/toast tests, vocab-guard, no-pr-validator-framing, rates pin tests, `/pricing` + `/agents` route renders — all must pass unmodified except where assertions track intentionally changed copy. **Idempotency/replay:** N/A — static site.

---

## Acceptance Criteria

- [ ] Era pillar tokens, guard suite (incl. brand noun + unvalidated/autonomy-overclaim strings), and `llms.txt` test green — verify: `make test-unit-website`
- [ ] Lint clean — verify: `make lint-website`
- [ ] Homepage dry lane renders all sections, `/llms.txt` 200, axe green — verify: `make dry-smoke`
- [ ] `gitleaks detect` clean · no non-md file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
make test-unit-website && make lint-website && echo "PASS" || echo "FAIL"
make dry-smoke
gitleaks detect 2>&1 | tail -3
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
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
- **Amendment (Indy, Jun 16, 2026):** the wedge positioning was rolled into the org README surfaces ahead of this website implementation — `README.md` (this repo), the `usezombie/.github` profile, `usezombie/docs` `README.md`, and `agentsfleet/skills` `README.md` now lead with the resident-engineer-for-support-escalations copy (hero: "Your hardest support tickets are engineering problems. Now they have an engineer."; pillar tokens: resident engineer, human approval, replayable log). These READMEs are NOT covered by the website `marketing-spec` guards; when this spec's website copy lands it remains canonical and the READMEs reconcile to it. Separately corrected this repo's README CLI install to the ecosystem-standard `npm install -g @agentsfleet/cli` (was a bare `bun install -g agentsfleet`).
- **Amendment (Indy, Jun 17, 2026 `/plan-ceo-review`):** evolved scope collapses to one unique spine: **compounding operational knowledge**. Scenario generation, test generation, resident-engineer workflow, asleep-engineer PRs, and safety boundaries are proof points under that spine — not peer features. The website may say the loop is designed toward scenario/test-backed fix PRs while humans sleep, but must keep human review before merge/deploy and never claim zero tickets.

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
- Any claim of zero tickets or autonomous merge/deploy while humans sleep — this PR may show the direction, not overstate the shipped guarantee.
