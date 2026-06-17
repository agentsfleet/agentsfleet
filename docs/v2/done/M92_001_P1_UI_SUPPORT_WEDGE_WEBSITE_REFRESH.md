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
**Status:** DONE
**Priority:** P1 — customer-facing: agentsfleet.net still sells the deploy-failure wake-on-event story while the product positioning moved to a resident engineer that compounds operational knowledge from recurring problem classes; every visitor from the application reads the wrong product
**Categories:** User Interface (UI)
**Batch:** B2 — after M92_002 (the agentsfleet rebrand lands first; every copy string here is authored under the new brand)
**Branch:** feat/m92-website-refresh
**Test Baseline:** unit=1951 integration=189
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

## Pull Request (PR) — PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m92): center website on compounding operational knowledge`
- **Intent (one sentence):** a visitor lands on agentsfleet.net and reads the product as one resident-engineer loop — first signal → recurring problem class → scenario/test → fix Pull Request (PR) → human review → fewer repeats — drawn as a pipeline they can grasp in one glance.
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
10. **Confused-user next step** — a visitor who wants proof clicks through to Docs (existing nav) or follows the loop anchor; a developer who wants to try it copies the install command. The `Get early access` affordance stays visible but disabled for Jun 17, 2026.

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
| `ui/packages/website/src/components/Hero.tsx` | EDIT | escalation headline/lede, persona-aware copy, visible-but-disabled early-access call to action beside the install row |
| `ui/packages/website/src/components/Hero.test.tsx` | EDIT | assertions track new copy + dual call to action |
| `ui/packages/website/src/components/OnboardingFlow.tsx` | DELETE | install steps fold into the `PipelineDiagram` start node; standalone section removed (RULE ORP/NDC) |
| `ui/packages/website/src/components/OnboardingFlow.test.tsx` | DELETE | component removed |
| `ui/packages/website/src/components/PipelineDiagram.tsx` | CREATE | the signal→problem-class→scenario/test→fix PR→human-gate diagram + categorized source strip |
| `ui/packages/website/src/components/PipelineDiagram.test.tsx` | CREATE | structure, fork, reduced-motion, local-asset assertions |
| `ui/packages/website/src/components/OperationalKnowledgeSection.tsx` | CREATE | compounding operational knowledge: problem class → scenario/test → fix PR → fewer repeats |
| `ui/packages/website/src/components/OperationalKnowledgeSection.test.tsx` | CREATE | copy + heading-rank assertions |
| `ui/packages/website/src/components/HowItWorks.tsx` | EDIT | three deploy-era steps become the compounding knowledge loop |
| `ui/packages/website/src/components/HowItWorks.test.tsx` | EDIT | step order assertion |
| `ui/packages/website/src/components/CTABlock.tsx` | EDIT | "Stop chasing failed deploys." → escalation framing |
| `ui/packages/website/src/components/CTABlock.test.tsx` | EDIT | tracks new copy |
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | one new wedge question (what the agent reads / approval posture); rate answers untouched |
| `ui/packages/website/src/components/FAQ.test.tsx` | EDIT | new entry assertion |
| `ui/packages/website/src/components/Pricing.tsx` | EDIT | layout → 3-plan cards (Free trial / Usage / Enterprise-contact) per approved `/design-shotgun` direction; every `RATES_DISPLAY` value byte-identical |
| `ui/packages/website/src/components/Pricing.test.tsx` | EDIT | 3-card structure assertion; rate-value byte-equality regression (Invariant 5) |
| `ui/packages/website/src/pages/Home.tsx` | EDIT | new section order (Hero → OperationalKnowledge → PipelineDiagram → capabilities → HowItWorks → Pricing → FAQ → CTA); OnboardingFlow removed; reframed capability blocks |
| `ui/packages/website/src/pages/Home.test.tsx` | EDIT | new section-order assertion; drop OnboardingFlow four-step + duplicate-install pins (`:49`, `:75`) |
| `ui/packages/website/src/lib/marketing-copy.ts` | CREATE | named constants: pillar tokens, loop titles, source categories, capability copy, llms.txt fields (RULE UFS home) |
| `ui/packages/website/src/marketing-spec.test.ts` | EDIT | pillar tokens for the new era; forbidden unvalidated + autonomy-overclaim strings |
| `ui/packages/website/public/logos/*.svg` | CREATE | vendored monochrome source-logo assets |
| `ui/packages/website/scripts/prebuild.mjs` | EDIT | emit `llms.txt` (convention index) + `llms-full.txt` (full prose) from `lib/marketing-copy.ts` + `config.ts` (`INSTALL_COMMAND`/`DOCS_URL`) + `rates.ts` (pricing pointer) |
| `ui/packages/website/tests/e2e/smoke.spec.ts` | EDIT | new sections render in the dry lane; `/llms.txt` + `/llms-full.txt` reachable (200) |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream — copy repositioning and the diagram ship together because the hero reads coherently only with both; splitting would put a compounding-knowledge headline above a deploy-era diagram on main.
- **Alternatives considered:** (a) full visual redesign now (polsia direction) — rejected: conflates a positioning fix with a taste project; `/design-shotgun` variants come after the message is right. (b) Copy-only, diagram later — rejected: the diagram *is* the positioning claim ("end-to-end") made legible; the onepager's table is prose-shaped without it.
- **Patch-vs-refactor verdict:** this is a **patch** (era-three copy amendment on a proven mechanism) plus one greenfield component. The named follow-ups: architecture-doc reconciliation spec; visual-refresh spec post `/design-shotgun`.

---

## Sections (implementation slices)

### §1 — Hero repositioning

The first screen says the new product. Headline moves to compounding operational knowledge in the house "memorable thing" voice; lede states the resident-engineer claim with the surviving pillar tokens; the install row and animated terminal stay; `Get early access` remains visible but disabled for Jun 17, 2026 with no link target or signup analytics. **Implementation default:** keep the `LIVE — wake.on.event` eyebrow — a first signal is the wake event, and it preserves a pinned token.

- **Dimension 1.1** — hero carries the era pillar tokens (`resident engineer`, `human approval`, `replayable log`, `wake.on.event`) sourced from `marketing-copy.ts` → Test `test_hero_carries_era_pillar_tokens`
- **Dimension 1.2** — dual call to action: install copy-row preserved verbatim; `Get early access` renders disabled without `href` or signup analytics; secondary `See the loop` anchor remains present → Test `test_hero_dual_cta`

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

### §5 — Trust capabilities

The four capability blocks reframe from solo-developer features to the trust layer the onepager leads with (sandboxed runtime, vaulted credentials, approval gating, open source + full auditability with replay) — same grid, same components. Capability copy moves to `lib/marketing-copy.ts` (UFS). *(Competition table removed per `/plan-eng-review` Jun 17 — see Discovery.)*

- **Dimension 5.1** — four reframed capability blocks render with trust-layer copy → Test `test_capabilities_trust_framing`

### §6 — CTA + FAQ alignment

`CTABlock` headline moves from "Stop chasing failed deploys." to the escalation claim. FAQ gains one wedge entry (what sources the agent reads and the approval posture); all rate/pricing answers stay byte-identical.

- **Dimension 6.1** — CTA carries escalation framing; no deploy-era copy remains in touched components → Test `test_cta_escalation_framing`
- **Dimension 6.2** — FAQ renders the new wedge entry; rate answers unchanged (regression) → Test `test_faq_wedge_entry_and_rates_regression`

### §7 — Marketing guard amendments

`marketing-spec.test.ts` pins the new era: pillar-token assertion reads the exported constants; a new forbidden-strings block rejects unvalidated quantitative claims (`40%` escalation figures, ticket-latency hour claims), zero-ticket promises, and autonomous merge/deploy claims. Existing `vocab-guard` and `no-pr-validator-framing` guards stay untouched and green.

- **Dimension 7.1** — guard test asserts era pillar tokens via `marketing-copy.ts` imports → Test `test_marketing_spec_pins_new_era`
- **Dimension 7.2** — guard test rejects unvalidated quantitative + autonomy-overclaim strings across rendered copy → Test `test_no_unvalidated_or_autonomy_overclaims`
- **Dimension 7.3** — rendered copy uses the `agentsfleet` product noun; **zero** `usezombie`/`zombie` matches anywhere in `src/` (the rebrand has landed — install is `agentsfleet.dev` per `config.ts`, no operational-string exception remains) → Test `test_brand_noun_guard`

### §8 — Large Language Model (LLM)-readable surface

The site ships `public/llms.txt` to the [llmstxt.org](https://llmstxt.org) convention — an H1 title, a blockquote one-line positioning summary, then `##` link sections — plus a `public/llms-full.txt` carrying the full positioning prose for deep crawls. Both are emitted by `scripts/prebuild.mjs` from `lib/marketing-copy.ts` (positioning + loop), `config.ts` (`INSTALL_COMMAND`, `DOCS_URL`, `GITHUB_URL`), and `rates.ts` (`RATES_DISPLAY` pricing pointer) so the LLM surface cannot drift from site copy or the pinned rate model. The index links the already-bundled `/openapi.json`. **Required shape:**

```
# agentsfleet

> Resident engineer that compounds operational knowledge: signal → recurring
> problem class → scenario/test → fix PR → human approval → fewer repeats.

## Product
- [How it works](https://agentsfleet.net/#how-it-works): the compounding loop
- [Pricing](https://agentsfleet.net/#pricing): $0.0001/sec run, $5 starter credit, events free

## Resources
- [Docs](https://docs.agentsfleet.net)
- [OpenAPI](/openapi.json)
- [Source](https://github.com/agentsfleet/agentsfleet)
- Install: `curl -fsSL https://agentsfleet.dev | bash`
```

**Implementation default:** prebuild-script generation; a static hand-edited file is the fallback only if generation fights the bundler.

- **Dimension 8.1** — `/llms.txt` is served, passes the convention shape (H1 + blockquote summary + ≥1 `##` link section), and carries the pillar tokens + ordered loop steps + docs/openapi/install/pricing links → Test `test_llms_txt_present_and_current`
- **Dimension 8.2** — `/llms-full.txt` is served and carries the full positioning prose; both files derive from the shared constants (no hardcoded drift) → Test `test_llms_full_txt_present`

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
| 1.2 | unit | `test_hero_dual_cta` | install copy-row present; disabled `Get early access` has no `href` and does not fire signup analytics; copy click writes `INSTALL_COMMAND` to clipboard (existing behaviour regression) |
| 2.1 | unit | `test_pipeline_renders_sources_and_stages` | four category labels + investigate/problem-class/scenario-test stages render in document order |
| 2.2 | unit | `test_pipeline_scenario_pr_and_gate` | scenario/test artifact, fix PR card, and human-review gate render in document order |
| 2.3 | unit | `test_pipeline_logos_local_only` | every rendered img/svg source matches `/logos/`; zero `http(s)://` sources |
| 2.4 | unit | `test_pipeline_reduced_motion_and_stacking` | reduced-motion media mock → no animation class; narrow viewport → stacked layout class |
| 3.1 | unit | `test_operational_knowledge_section_renders` | compounding knowledge spine present; heading rank is h2 under the hero h1 |
| 4.1 | unit | `test_how_it_works_eight_steps_in_order` | eight titles render in the exported order, first signal first, learn last |
| 5.1 | unit | `test_capabilities_trust_framing` | four blocks render sandboxed-runtime / vaulted-credentials / approval-gating / open-source-replay copy |
| 6.1 | unit | `test_cta_escalation_framing` | CTA headline matches new copy; "failed deploys" absent from touched components |
| 6.2 | unit | `test_faq_wedge_entry_and_rates_regression` | new entry renders; rate answer strings byte-equal to `RATES_DISPLAY`-derived values |
| 7.1 | unit | `test_marketing_spec_pins_new_era` | guard test sources tokens from `marketing-copy.ts`, fails when a token is removed from the hero |
| 7.2 | unit | `test_no_unvalidated_or_autonomy_overclaims` | seeded forbidden string in a fixture component is detected; live tree has zero hits |
| 7.3 | unit | `test_brand_noun_guard` | rendered copy says `agentsfleet`; **zero** `usezombie`/`zombie` matches across `src/` (no exception) |
| 8.1 | unit | `test_llms_txt_present_and_current` | emitted `public/llms.txt` passes convention shape (H1 + blockquote + ≥1 `##` link section) and carries every pillar token, ordered loop steps, and docs/openapi/install/pricing links |
| 8.2 | unit | `test_llms_full_txt_present` | emitted `public/llms-full.txt` carries the full positioning prose; both files derive from shared constants |
| all | e2e | website dry-lane smoke | homepage renders every section; `/llms.txt` + `/llms-full.txt` return 200; axe assertions green; no console errors |

**Regression:** existing Hero clipboard/toast tests, vocab-guard, no-pr-validator-framing, rates pin tests, `/pricing` + `/agents` route renders — all must pass unmodified except where assertions track intentionally changed copy. **Idempotency/replay:** N/A — static site.

---

## Acceptance Criteria

- [x] Era pillar tokens, guard suite (incl. brand noun + unvalidated/autonomy-overclaim strings), and `llms.txt` test green — verify: `make test-unit-website`
- [x] Lint clean — verify: `make lint-website`
- [x] Homepage dry lane renders all sections, `/llms.txt` 200, axe green — verify: `make dry-smoke`
- [x] `gitleaks detect` clean · no non-md file over 350 lines added

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
- **Amendment (Indy, Jun 17, 2026 `/plan-eng-review`):** steer = "I like the polsia (zombieos.polsia.app) format, less cluttered, and the site must be LLM-friendly (llms.txt)." Decisions, all supersede the conflicting body text above:
  - **Scope:** keep the full spec (diagram + operational-knowledge section + reframed capabilities + 8-step loop) rather than reducing to copy-only. The less-cluttered intent is satisfied within full scope by the two cuts below + deferring visual treatment to `/design-shotgun`.
  - **Competition table removed** — §5.2, `CompetitionTable.tsx`/`.test.tsx`, Dimension 5.2, `test_competition_table_rows` all dropped. §5 is now trust-capabilities only.
  - **OnboardingFlow merged into the diagram** — `OnboardingFlow.tsx`/`.test.tsx` DELETED; its install steps fold into the `PipelineDiagram` start node (install → first signal → …). Final section order: **Hero → OperationalKnowledge → PipelineDiagram → capabilities → HowItWorks → Pricing → FAQ → CTA** (~8 sections, down from ~10). `Home.test.tsx` `:49`/`:75` OnboardingFlow pins are removed; hero copy-row is the canonical `InstallBlock`. *Risk logged:* OnboardingFlow's 4-step setup content must survive in the diagram start node or a docs link, not vanish.
  - **Brand:** the rebrand already landed in code (zero `usezombie` in `src/`; install is `agentsfleet.dev`). §7.3 now asserts **zero** `usezombie`/`zombie` (no operational-string exception); the obsolete "install stays on `usezombie.sh` verbatim" premise from the Jun 12 amendment is **void**. Stale `usezombie.com` prose in Priority/Intent corrected to `agentsfleet.net`.
  - **Pricing:** confirmed adhered — `lib/rates.ts` is the single cross-tier-pinned source ($5 credit, free events, $0.0001/sec ≈ $0.36/hr, free-trial to Jul 31 2026); diff leaves it untouched (Invariant 5). Because §6 edits `FAQ.tsx` (a rate consumer), the verification block must run `scripts/audit-cross-tier-rates.sh`.
  - **llms.txt:** ship the full llmstxt.org convention (H1 + blockquote + `##` link sections, linking docs/`openapi.json`/install/pricing) **plus** `llms-full.txt`; both generated from shared constants (§8 rewritten, Dimension 8.2 added).
  - **`marketing-copy.ts` → `lib/marketing-copy.ts`** for consistency with `rates.ts`/`copy.ts`/`contact.ts`.
  - **Follow-up:** `/design-shotgun` for the website (requested Jun 17) — must include a pricing-section structural variant in the polsia direction (standing requirement); the diagram's structural tests survive any re-skin.
- **Amendment (Indy, Jun 17, 2026 `/design-shotgun` — APPROVED visual direction):** ran the shotgun (code-authored HTML mockups; AI-image path skipped — no OpenAI key, and the token-precise design system is better served by exact CSS). Three variants generated (Single Spine / Terminal Ledger / Diagram-Forward); artifacts at `~/.gstack/projects/agentsfleet-usezombie/designs/homepage-refresh-20260617/` (`chosen.html` is the approved merge). Approved direction:
  - **Base = "Terminal Ledger"** — the loop (§2) renders as a **replayable terminal session** of timestamped evidence log-lines (`[wake] signal → [work] investigate → [EVIDENCE] problem class → scenario/test → fix PR → ⏸ human approval (pulse) → ✓ merged by human · recurrence reduced`), in `Terminal`/`LogLine` primitives — NOT the Cleric horizontal node-diagram. The four source categories (Signals · Telemetry · Code · Control plane) are still required (Dimension 2.1) but presented in the ledger aesthetic.
  - **Hero headline:** "A resident engineer that compounds operational knowledge." Keep the `LIVE — wake.on.event` eyebrow + pulse.
  - **CTA labels (§1.2):** primary **"Get early access"** (was "Become a design partner" — aligns with `RATES_DISPLAY.HEADLINE`); secondary **"See the loop"** (anchor to §2). The design-partner mailto label is retired.
  - **Jun 17 disable:** Indy asked to disable `Get early access` for today. Header, hero, and usage-card early-access affordances render as disabled buttons with no `href`; the copy remains visible, and signup analytics do not fire from disabled controls.
  - **Pricing presentation (new scope consequence):** approved as a **3-plan card layout** (Free trial / Usage [featured] / Enterprise-contact) with copy "Start free. Pay only while it runs." + "Usage is metered per second — no seats, no tiers tax. Enterprise adds the controls big teams need." The **billed model stays metered** (`$5` credit · free events · `$0.0001/sec`); tiers are a **later product+billing decision**, Enterprise is contact-only (no fabricated price). This means **`Pricing.tsx` is now edited** (layout → 3 cards) while every `RATES_DISPLAY` value stays byte-identical — added to Files Changed; Invariant 5 + the rates pin still hold.

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
| Unit coverage | `cd ui/packages/website && npm run test:coverage` | 24 files / 164 tests passed; statements 100 percent, branches 100 percent, functions 100 percent, lines 100 percent | yes |
| Typecheck | `cd ui/packages/website && npm run typecheck` | passed | yes |
| Lint | `cd ui/packages/website && npm run lint` | passed | yes |
| End-to-End (E2E) + axe | `cd ui/packages/website && npm run test:e2e` | 98 passed | yes |
| Design audit | `/private/tmp/agentsfleet-design-audit.mjs` against local dev server | no console errors; no horizontal overflow; zero small touch targets; early-access controls disabled/no href; screenshots saved under `/private/tmp/agentsfleet-design-review/` | yes |
| Gitleaks | `gitleaks detect` | pending | |
| Orphan sweep | Eval E6 | pending | |

---

## Out of Scope

- Pricing-model change (credit plans from the onepager) — open product decision; rates remain cross-tier-pinned per-second across `tenant_billing.zig` / `rates.ts` / `rates.mdx`.
- Architecture-doc reconciliation (`high_level.md`, `user_flow.md` still describe the wake-on-event framing) — follow-up spec when the wedge graduates from positioning to canon.
- Full visual redesign / `/design-shotgun` variant selection — follow-up after this positioning diff lands; the diagram's structural tests survive a re-skin. The shotgun run carries a standing requirement: a pricing-section structural variant in the zombieos.polsia.app direction.
- docs.usezombie.com content updates — separate repo, separate spec.
- Any connector implementation (Zoho Desk, Jira, Datadog ingestion) — the logo strip names source categories, not shipped integrations.
- Any claim of zero tickets or autonomous merge/deploy while humans sleep — this PR may show the direction, not overstate the shipped guarantee.

---

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | clean | spine collapsed to compounding operational knowledge (Jun 17) |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | clean | 5 findings raised, all dispositioned; 0 unresolved, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | superseded by requested `/design-shotgun` |

**Eng Review findings (all resolved):**
1. **Scope vs. stated direction** (arch) — spec was maximalist (~10 sections) vs. "less cluttered" steer → kept full scope, trimmed CompetitionTable + merged OnboardingFlow into diagram (~8 sections); visual cleanup → `/design-shotgun`.
2. **Brand guard premise stale** (code-quality, conf 9/10) — `config.ts:25` install is `agentsfleet.dev`, zero `usezombie` in `src/` → §7.3 now asserts zero usezombie; stale prose corrected.
3. **Section order unspecified + OnboardingFlow collision** (arch, conf 8/10) — `Home.tsx:35-69` + `Home.test.tsx:75` → order locked, OnboardingFlow deleted, install folds into diagram.
4. **llms.txt under-specified** (test/coverage, conf 8/10) — §8 ignored the llmstxt.org convention + bundled `openapi.json` → full convention + `llms-full.txt`, shape pinned.
5. **`marketing-copy.ts` placement** (arch, conf 7/10) — convention is `lib/` → moved to `lib/marketing-copy.ts`.

**Pricing confirmed adhered:** `lib/rates.ts` single source, cross-tier-pinned; diff untouched; `audit-cross-tier-rates.sh` added to verification (FAQ.tsx is a rate consumer).

- **CODEX:** outside voice deferred (CODEX_MODE was ready) — user pivoted to `/design-shotgun`; re-run independent pass with `/codex review` before CHORE(close).
- **VERDICT:** CEO + ENG CLEARED — ready to implement (or run `/design-shotgun` first per Indy's request).

NO UNRESOLVED DECISIONS
