# M82_002: Name the product "agent" in user-facing copy (retire user-facing "zombie")

**Prototype:** v2.0.0
**Milestone:** M82
**Workstream:** 002
**Date:** Jun 03, 2026
**Status:** DONE
**Priority:** P1 — customer-facing; the product's core noun is presented three ways ("zombie" / "agent" / "runtime") with no definition, and the dashboard ("Zombies") disagrees with the public API + `/agents` page ("agent"). Newcomers must infer the central concept.
**Categories:** UI
**Batch:** B1 — standalone; no concurrent workstream.
**Branch:** feat/m82-002-zombie-concept-definition
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, in-session user-facing "zombie" terminology audit, Jun 03, 2026 — 3-surface grep + classification). **Re-scoped Jun 03, 2026** after an Indy voice-review decision inverted the original intent — see the RE-SCOPE banner and Discovery.

> **RE-SCOPE (Jun 03, 2026, Indy-directed).** The original spec aimed to *define "zombie"* as the user-facing product noun. At voice review Indy chose the opposite: **user-facing copy uses "agent" as the product noun, and "zombie" is retired from user-facing copy** — it survives only in the brand (`usezombie`, `usezombie.sh`), the CLI/daemon names (`zombiectl`, `zombied`, `zombie-runner`), routes/API paths (`/zombies/...`), and code identifiers (`ZOMBIE_STATUS`, `listZombies`, …). This aligns the dashboard with the already-shipped public `/agents` page + API reference, which call them "agents". The original "do NOT rename the ~36 usages" stance is reversed for user-facing product-noun usages; brand/CLI/route/code usages are still NOT renamed.

> **Provenance is load-bearing.** Agent-generated from a grep+classify audit of `ui/packages/website` and `ui/packages/app`. Every cited file:line below was returned by the audit; cross-check the rendered copy against the live components before EXECUTE. The keep-vs-rename classification in **Files Changed** is the contract — a usage in the KEEP set is a same-spelling-different-meaning trap (brand/CLI/route/code), not a rename target.

**Canonical architecture:** `docs/architecture/direction.md:11` — the source-of-truth definition ("one [agent] = a durable runtime, not a one-shot prompt"). This workstream surfaces that definition to users under the noun "agent"; it does NOT redefine the concept or touch architecture docs (which keep their internal "zombie" vocabulary).

---

## Implementing agent — read these first

1. `docs/architecture/direction.md:11` (+ `high_level.md`, `capabilities.md:5`) — the canonical concept ("durable, autonomous, event-triggered runtime"). The user-facing definition must be faithful to this; only the *noun* changes (zombie → agent) in user copy.
2. `ui/packages/website/src/pages/Agents.tsx` — the **already-shipped precedent**: the public `/agents` page + API reference call them "agents" while the paths stay `/zombies`. This workstream brings the rest of the site + the app into line with it.
3. `ui/packages/website/src/components/CTABlock.tsx:17` — the closest existing *implicit* gloss ("it wakes on the failure, reads the logs, and posts the diagnosis"). Mirror this voice; upgrade it from a behavioral hint to an explicit definition under the noun "agent".
4. `ui/packages/app/app/(dashboard)/zombies/page.tsx` + `ui/packages/app/app/(dashboard)/page.tsx` — the two app first-touch surfaces (empty state, first-run card) that carry the definition and the renamed noun.
5. The app design-system primitives + `theme.css` tokens — any new tooltip/callout uses primitives (UI Substitution Gate) and tokens (DESIGN TOKEN Gate), never raw HTML or arbitrary values.
6. `docs/CHANGELOG_VOICE.md` — voice discipline (no "seamless"/"powerful"; lead with the change/meaning). The definition copy follows it.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Name the product "agent" in user-facing copy + define it at first-touch
- **Intent (one sentence):** A first-time user meets one consistent product noun — **"agent"** — on the marketing site and in the app, with an explicit definition at first touch, instead of a "zombie / agent / runtime" synonym soup that disagrees with the public API.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent, lists `ASSUMPTIONS I'M MAKING: …`, and (done Jun 03) confirmed the noun decision + final definition wording with Indy at voice review. A mismatch with the Intent above → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline. Specifically **UFS** (the definition string is a repeated semantic literal → a named constant; ui/ UFS is extracted by hand, not by the auditor) and **NLG** (no "legacy"/compat framing in new copy; pre-2.0). **Blast-radius grep** (a rename touches many files; separate user-facing product-noun "zombie" from same-spelling brand/CLI/route/code "zombie" — git grep from repo root, classify before editing).
- **No `*.zig` / `src/http/handlers/**` / `schema/*` surfaces are touched** — `ZIG_RULES.md`, `REST_API_DESIGN_GUIDELINES.md`, `SCHEMA_CONVENTIONS.md` do not apply.
- This is UI copy only; the binding constants are the design-system primitives + `theme.css` tokens (see Applicable Gates).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` touched |
| PUB / Struct-Shape | no | no Zig pub surface |
| File & Function Length (≤350/≤50/≤70) | maybe | edits to existing `.tsx`; keep each file ≤350 — split before crossing the cap |
| UFS (repeated/semantic literals) | **yes** | the definition copy lives in ONE named constant per UI package (website, app); spots reference the constant, never an inline duplicate. Extracted by hand (ui/ UFS is manual). |
| UI Substitution / DESIGN TOKEN | **yes** | any new tooltip/callout uses a design-system primitive (`asChild` for HTML semantics) + `theme.css` tokens; no raw `<div>`/`<section>`, no `*-[...]` arbitraries |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no logs, lifecycle, error codes, or schema touched |
| MILESTONE-ID | yes (source files) | no `M82_002`/`§X.Y` IDs in `.tsx`/`.ts` bodies (RULE TST-NAM / milestone-free code) |

---

## Overview

**Goal (testable):** Every user-facing surface on the website and in the app names the product **"agent"** (singular) / **"agents"** (plural); the bare noun "zombie" appears in user-visible copy only as part of the brand/CLI names (`usezombie`, `usezombie.sh`, `zombiectl`, `zombied`). A first-touch surface on the website (the "What is usezombie?" FAQ) and in the app (the `zombies`-route empty state + the dashboard first-run card) each carries an explicit one-sentence definition ("An agent is a long-lived runtime you install once…"), sourced from a single named copy constant per package — all asserted by rendered/render-equivalent tests plus a vocab-guard test that fails on any user-facing standalone product-noun "zombie".

**Problem:** A newcomer meets the product's core concept named three different ways — "zombie", "the agent", "a long-lived runtime" — and is never told they are the same thing, and never told what it is. Worse, the surfaces disagree: the public `/agents` page and the API reference already say **"agent"**, while the dashboard nav, empty states, and buttons say **"Zombie"**. The only real definition lives in internal contributor docs (`direction.md:11`). Net: the product's primary noun is both undefined and inconsistent at first touch.

**Solution summary:** Standardize the user-facing product noun on **"agent"**, retiring user-facing "zombie" (brand/CLI/route/code keep it), and add one faithful, voice-reviewed definition at each first-touch surface — sourced from one named copy constant per package. Where "agent" already meant the *coding tool* hosting the install skill (Claude Code/Amp/Codex/OpenCode), disambiguate to "coding agent" / "host". The ~30 user-facing product-noun usages are renamed; the brand/CLI/route/code usages (KEEP set) are not.

---

## Prior-Art / Reference Implementations

- **UI** → design-system primitives + `theme.css` tokens for any tooltip/callout. **Noun precedent:** `ui/packages/website/src/pages/Agents.tsx` already calls them "agents". **Voice reference:** `CTABlock.tsx:17` lists what an installed agent *does* — promote that behavioral hint to an explicit definition in the same voice. The canonical *concept* is `direction.md:11`; the gloss is a faithful user-facing restatement under the noun "agent".
- No new architecture; the shape is "rename the user-facing noun + inline definition at first-touch", a copy pattern, not a structural one.

---

## Files Changed (blast radius)

> **KEEP set (NOT renamed — same spelling, different meaning):** `usezombie` / `usezombie.sh` brand, `@usezombie/*` packages, page `<title>`, `BRAND_NAME`; `zombiectl` / `zombied` / `zombie-runner`; `/zombies/...` routes + `/v1/.../zombies/...` API paths + `zombie_id` + `x-usezombie:` frontmatter; code identifiers (`ZOMBIE_STATUS`, `listZombies`, `Zombie[]`, `ZombiesList`, `core.zombie_events`, component/function/file names). Editing any of these is out of scope and a regression.

| File | Action | Why (user-facing product-noun rename unless noted) |
|------|--------|-----|
| `ui/packages/website/src/lib/copy.ts` | CREATE | single named constant: `AGENT_DEFINITION` + `AGENT_SHORT_GLOSS` (UFS) |
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | "What is usezombie?" answer states the explicit definition; billing answers' product-noun "zombie" → "agent" |
| `ui/packages/website/src/components/Hero.tsx` | EDIT | lede names "agent" as the primary noun (currently "A long-lived runtime…") |
| `ui/packages/website/src/components/CTABlock.tsx` | EDIT | "Install one zombie" → "Install one agent" |
| `ui/packages/website/src/components/Pricing.tsx` | EDIT | product-noun "zombie" → "agent" (3 spots) |
| `ui/packages/website/src/components/OnboardingFlow.tsx` | EDIT | "Steer your zombie" → "Steer your agent" |
| `ui/packages/website/src/pages/Agents.tsx` | EDIT | snippet comments "platform-ops zombie" / "steer the zombie" → "agent" |
| `ui/packages/app/lib/copy.ts` | CREATE | single named constant: `AGENT_DEFINITION` + `AGENT_SHORT_GLOSS` (UFS), same identifiers as website |
| `ui/packages/app/app/(dashboard)/zombies/page.tsx` | EDIT | page title "Zombies"→"Agents", "No zombies yet"→"No agents yet" + definition, "installing zombies"→"agents" |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | first-run card: product-noun "zombie"→"agent" + definition |
| `ui/packages/app/app/(dashboard)/zombies/new/page.tsx` | EDIT | "Install Zombie" title → "Install Agent"; "installing zombies"→"agents" |
| `ui/packages/app/app/(dashboard)/zombies/new/InstallZombieForm.tsx` | EDIT | button "Install Zombie"→"Install Agent"; prose product-noun → "agent" (KEEP `name: my-zombie` slug example? → change heading "# My Zombie"→"# My Agent", keep `name:` slug) |
| `ui/packages/app/app/(dashboard)/zombies/loading.tsx` | EDIT | "Loading zombies…" → "Loading agents…" |
| `ui/packages/app/app/(dashboard)/zombies/components/ZombiesList.tsx` | EDIT | search/empty labels: "zombies"→"agents" (KEEP all `Zombie`/`zombie` code identifiers) |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | sidebar NAV label "Zombies"→"Agents" (KEEP href `/zombies`) |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | "zombie events"→"agent events" (2 spots) |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | "Run a zombie event…"→"agent event" (KEEP code comment) |
| `ui/packages/app/app/(dashboard)/settings/provider/components/ProviderSelector.tsx` | EDIT | "Zombie credits cover everything."→"Agent credits…" |

> The exact module path for each copy constant mirrors each package's nearest existing constants module (`website/src/lib/`, `app/lib/`); FILES + ROLES are fixed here.

> **Scope-expansion surfaces (added Jun 03, 2026 — see Discovery).** Beyond the UI: `docs/architecture/**` prose (14 files), root `README.md`, `zombiectl/README.md`, `Dockerfile` (OCI labels → GHCR `zombied` page points to docs.usezombie.com), the app agent-detail page (`zombies/[id]/components/KillSwitch.tsx`, `ZombieConfig.tsx`), and tests (`copy.test.ts` ×2, `vocab-guard.test.ts`). Separate-repo PRs: `usezombie/skills#2`, `usezombie/docs#78`.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a copy rename + inline definition. Decomposed into "canonical wording" (§1), "app surfaces" (§2), "website surfaces + rename" (§3), "vocab guard" (§4 — was optional nav; now the guard test is mandatory) so each lands and verifies independently.
- **Alternatives considered:** (a) keep "zombie", just define it — **rejected by Indy at voice review** (the original spec's plan). (b) drop both "zombie" and "agent" for a third noun — **rejected** (Indy chose "agent"). (c) rename only first-touch surfaces, leave the rest "zombie" — **rejected**: a half-rename is worse than either extreme; it *is* the synonym-soup problem. Consistency across user-facing copy is the goal.
- **Patch-vs-refactor verdict:** a **patch** — copy + one constant per package; no data model, component architecture, or routing changes (routes/API stay `/zombies`).

---

## Sections (implementation slices)

### §1 — Canonical user-facing definition (single source per package) — DONE
Establish the one faithful, voice-reviewed definition of an "agent" (sourced from `direction.md`), stored as a named copy constant in each UI package. Wording fixed in **Interfaces**. **Implementation default:** one constant per package referencing the same agreed wording, because website and app are separate packages with no shared content module; mirror each package's nearest existing constants module.

- **Dimension 1.1** — a named constant (`AGENT_DEFINITION`) holds the definition string in each touched package; no spot inlines a second copy → Test `test_agent_definition_constant_single_source`
- **Dimension 1.2** — the rendered definition is faithful to `direction.md` (durable, autonomous, event/wake, "not a one-shot prompt") → Test `test_agent_definition_matches_canonical`

### §2 — App first-touch definitions + rename — DONE
The two surfaces a newcomer lands on first render the definition under the noun "agent". **Implementation default:** the definition goes in the empty-state *description* and the first-run card *body*, beside the existing install-mechanics copy.

- **Dimension 2.1** — the `zombies`-route "No agents yet" empty state (`zombies/page.tsx`) renders the definition and uses "agent" → Test `test_app_empty_state_defines_agent`
- **Dimension 2.2** — the dashboard first-run / "First wake" card (`page.tsx`) renders the definition and uses "agent" → Test `test_app_first_run_defines_agent`

### §3 — Website definition + full user-facing rename — DONE
The website primary entry point ("What is usezombie?" FAQ) states the definition explicitly under "agent", and every user-facing product-noun "zombie" across the site becomes "agent".

- **Dimension 3.1** — the FAQ "What is usezombie?" answer renders the explicit "An agent is …" definition → Test `test_website_defines_agent`
- **Dimension 3.2** — user-facing rename: no website component renders a standalone product-noun "zombie" (brand/CLI/route excepted) → Test `test_website_no_user_facing_zombie_noun`

### §4 — Vocab guard (mandatory) + optional nav affordance — DONE
A CI-runnable guard fails on any user-facing standalone product-noun "zombie" in either package's components (allowlisting brand/CLI/route/code). The app sidebar `Zombies` nav becomes `Agents`; an optional tooltip carries the short gloss.

- **Dimension 4.1** — the vocab-guard test scans user-facing copy in both packages and reports 0 standalone product-noun "zombie" violations (allowlist: `usezombie`, `zombiectl`, `zombied`, `zombie-runner`, `/zombies`, code identifiers) → Test `test_no_user_facing_zombie_noun`
- **Dimension 4.2** — the sidebar nav label reads "Agents" (href unchanged `/zombies`); optional tooltip exposes the short gloss → Test `test_sidebar_label_is_agents`

---

## Interfaces

> The contract is the **copy wording** (user-visible strings), not a function signature. The strings below are the **Indy-voice-reviewed finals** (Jun 03); both packages render them verbatim from their constant.

```
AGENT_SHORT_GLOSS  (nav tooltip / one-liner):
  "An agent wakes on an event, runs your skill, and reports back."

AGENT_DEFINITION  (FAQ answer / empty-state / first-run card):
  "An agent is a long-lived runtime you install once. It sleeps until an
   event wakes it, runs your skill against that event, and reports back with
   evidence — durable and autonomous, not a one-shot prompt."

Noun rules (user-facing copy):
  - product noun = "agent" / "agents"
  - "zombie" allowed ONLY inside: usezombie, usezombie.sh, zombiectl, zombied,
    zombie-runner, /zombies route paths, and code identifiers.
  - the coding tools that host the install skill (Claude Code, Amp, Codex,
    OpenCode) = "coding agent" / "host" (disambiguates the overloaded "agent").
```

Same identifier (`AGENT_DEFINITION`, `AGENT_SHORT_GLOSS`) verbatim in both packages (UFS cross-package).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Definition drift | a spot inlines the wording instead of the constant | the named constant is the single source; the single-source test fails the build |
| Rename regression (over-rename) | a KEEP-set usage (brand/CLI/route/code) gets renamed | the internal-surface guard (`git diff` over routes/code/CLI) flags it; KEEP set is the contract |
| Rename regression (under-rename) | a user-facing product-noun "zombie" survives | the §4 vocab-guard test fails, naming the file:line |
| "agent" overload | copy says "agent" for both the product and the coding tool | coding tools are "coding agent"/"host" per the noun rules |
| Runtime failures (timeout/auth/network/race/replay/quota) | — | **N/A** — static rendered copy only; no new fetch, handler, auth path, or async behavior |

---

## Invariants

1. The user-facing definition renders from exactly one named constant per touched package — no inline duplicate. Enforced by the single-source test, not review discipline.
2. In user-facing copy, "agent" is the sole product noun; "zombie" appears only inside the KEEP allowlist (brand/CLI/route/code). Enforced by the §4 vocab-guard test (a grep/assertion over component copy), not reviewer vigilance.
3. Routes, API paths, and code identifiers are unchanged — `/zombies` still routes, `listZombies`/`ZOMBIE_STATUS` still compile. Enforced by the internal-surface `git diff` guard + a green build/test.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_agent_definition_constant_single_source` | each package exports one definition constant; no second inline copy |
| 1.2 | unit | `test_agent_definition_matches_canonical` | constant contains canonical markers (durable, autonomous, event/wake, "not a one-shot prompt") |
| 2.1 | render | `test_app_empty_state_defines_agent` | empty state renders the definition + "No agents yet" |
| 2.2 | render | `test_app_first_run_defines_agent` | first-run card renders the definition + "agent" |
| 3.1 | render | `test_website_defines_agent` | FAQ "What is usezombie?" renders the explicit "An agent is …" definition |
| 3.2 | unit | `test_website_no_user_facing_zombie_noun` | scan website component copy → 0 standalone product-noun "zombie" (allowlist applied) |
| 4.1 | unit | `test_no_user_facing_zombie_noun` | scan both packages' component copy → 0 violations |
| 4.2 | render/unit | `test_sidebar_label_is_agents` | sidebar NAV label === "Agents"; href === "/zombies" |

**Regression:** routes/API/code identifiers unchanged — `test_internal_surface_unchanged` greps the diff to confirm no `/zombies` route, `zombie_id`, `ZOMBIE_STATUS`, `zombiectl`, or `usezombie` brand string moved. **Idempotency/replay:** N/A — static copy.

UI is a user-facing Category, so §2/§3 carry render-tier tests; §4 vocab guard is a unit-tier grep. Render tier is satisfied by the app/website `vitest` render tests already in each package.

---

## Acceptance Criteria

- [x] FAQ "What is usezombie?" renders an explicit "An agent is …" definition — website render test green
- [x] App `zombies`-route empty state + dashboard first-run card render the definition under "agent" — `make test-unit-app` green
- [x] Definition sourced from a single named constant per package — `copy.test.ts` green
- [x] No user-facing standalone product-noun "zombie" remains (allowlist: brand/CLI/route/code) — `vocab-guard.test.ts` green
- [x] Routes/API/code unchanged — keep-token integrity gate: `/zombies`, `zombie_id`, `ZOMBIE_STATUS`, `listZombies`, `usezombie` brand all preserved; tests green
- [x] `make lint-{app,website}` clean · `make test-unit-{app,website}` pass · gitleaks clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: definition present in app first-touch surfaces
git grep -l "long-lived runtime you install once" ui/packages/app && echo "PASS" || echo "FAIL"
# E2: Build  — (cd ui/packages/app && bun run build) && (cd ui/packages/website && bun run build)
# E3: Tests  — (cd ui/packages/app && bun run test) && (cd ui/packages/website && bun run test)
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Vocab guard — the §4 test must report 0 user-facing product-noun "zombie"
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: Internal-surface untouched (KEEP set) — these must still be present/routing
git grep -c "ZOMBIE_STATUS\|listZombies\|/zombies" ui/packages/app | head
```

---

## Dead Code Sweep

N/A — no files deleted; this workstream renames/adds copy and constants.

---

## Discovery (consult log)

- **Indy decisions (Jun 03, 2026):**
  > Indy (2026-06-03): "Yes create a spec and push that spec in this PR" — context: the spec was created in `pending/` and committed onto the M82_001 branch (PR #361). Implementation is this separate PR.
  > Indy (2026-06-03): "leave out the internals like api /zombies/... zombiectl, zombied zombie-runner or errors that are sent which is a larger change" — context: internal references are out of scope (KEEP set).
  > Indy (2026-06-03, voice review): "i only want to use agent, dont use zombie." — context: **inverts the original "define zombie" intent**. User-facing product noun becomes "agent"; "zombie" is retired from user-facing copy (brand/CLI/route/code excepted). Confirmed via a follow-up disambiguation question: "Use 'agent', drop 'zombie' in copy."
- **Re-scope blast-radius grep (Jun 03, 2026):** repo-root grep of `ui/packages/{website,app}` classified ~30 user-facing product-noun "zombie" usages (rename) vs the KEEP set (brand/CLI/route/code). Notable same-spelling trap: `"Which agent hosts work for the install skill?"` — "agent" there = the *coding tool* (Claude Code/Codex), not the product; resolved to "coding agent"/"host".
- **CHORE(open) (Jun 03, 2026):** also removed the stale `docs/v2/pending/M80_004_…md` twin (orphan left by M80_004's CHORE(close); the `done/` copy is canonical) — folded into this branch per Indy.
- **Scope expansion (Jun 03, 2026, Indy-directed).** After the UI rename, Indy widened scope to a full product-wide terminology sweep, in this PR:
  > Indy (2026-06-03): "Ensure the docs/architecture/** is scoured … README.md … the zombiectl/README.md must have the term zombie removed. Ensure the ~/Projects/skills repo is updated with agents term and the ui/usezombie.sh/ is updated … Also ensure the comments and text in playbooks are renamed."
  > Indy (2026-06-03): "We leave out the folder names zombiectl, zombie/ zombied binary, zombie-runner for now, and the api responses and the api resources." + "preserve anything that will have a code impact, any static text changes can be done." + "No updates to historical [changelog], but add a change log to this effect."
  Resolved surfaces: `docs/architecture/**` prose (14 files), root + `zombiectl` READMEs, the `Dockerfile` OCI labels (GHCR `zombied` package → `docs.usezombie.com`), the `skills` repo README, and a `docs` changelog entry. **No change required:** `playbooks/**` (every "zombie" is the `zombie` DB / `zombied` / `zombie-runner` / vault refs / datasource = code impact), `ui/usezombie.sh/**` (brand/CLI only), `.github/profile/README.md` (brand/URLs only; line 9 already reads "Durable agent runtime").
- **Rename method + integrity (Jun 03, 2026).** A guarded regex first pass on the architecture docs wrongly renamed schema/module identifiers (`core.zombies`→`core.agents`, `redis_zombie`→`redis_agent`) — reverted. Redone with judgment-based sub-agents per file, then verified with a keep-token integrity gate: every identifier token (`zombiectl`, `zombied`, `core.zombie*`, `redis_zombie`, `parseZombie`, `src/zombie/`, `/zombies`, brand) matches HEAD count exactly; zero `core.agents`/`redis_agent`/`/agents/`-style wrong renames.
- **Cross-repo PRs (separate repos, same effort):** `usezombie/skills#2` (README prose) and `usezombie/docs#78` (changelog entry). The in-repo PR is this branch.
- **Skill chain outcomes:** `/write-unit-test` intent satisfied by the new copy-constant + FAQ-definition tests and the `vocab-guard.test.ts` regression guard (website 146, app 667 green). `/review` performed inline (keep-set integrity gate + per-file sub-agent verification of the architecture rename).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification — every Dimension has its render/unit test; the vocab + single-source invariants are tested. | Clean; iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `direction.md`, UI Substitution + DESIGN TOKEN gates, voice (`CHANGELOG_VOICE.md`), and the KEEP-set (no over-rename). | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR (copy accuracy, no internal-surface drift). | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| App unit/render tests | `make test-unit-app` | 667 passed | ✅ |
| Website unit/render tests | `make test-unit-website` | 146 passed | ✅ |
| Single-source / vocab guard | `vocab-guard.test.ts` + `copy.test.ts` (both pkgs) | green (guard caught + fixed detail-page copy) | ✅ |
| Lint | `make lint-app` · `make lint-website` | both passed (Oxlint + tsc) | ✅ |
| Internal-surface unchanged (arch docs) | keep-token HEAD-vs-WORK count gate | all tokens match; 0 wrong renames | ✅ |
| Gitleaks | pre-commit `gitleaks protect --staged` | no leaks (every commit) | ✅ |

---

## Out of Scope

- **The KEEP set** — `usezombie`/`usezombie.sh` brand, `@usezombie/*`, `zombiectl`/`zombied`/`zombie-runner`, `/zombies` routes + `/v1/.../zombies/...` API paths + `zombie_id` + `x-usezombie:` frontmatter, and all code identifiers (`ZOMBIE_STATUS`, `listZombies`, `Zombie[]`, component/function/file names). [Indy-acked, Discovery] — "a larger change".
- **Renaming routes / API / the `/agents` page structure** — paths stay `/zombies`; the product noun in copy is "agent".
- **`docs/architecture` changes** — internal docs keep "zombie"; this workstream is user-facing copy only.
- **A standalone glossary / docs-site page** — the definition is inline at first-touch; a `docs.usezombie.com` glossary entry is future work.
