# M82_002: Define "zombie" for users at first-touch surfaces

**Prototype:** v2.0.0
**Milestone:** M82
**Workstream:** 002
**Date:** Jun 03, 2026
**Status:** PENDING
**Priority:** P1 — customer-facing; the product's core noun is undefined for users at every entry point, so newcomers must infer the central concept.
**Categories:** UI
**Batch:** B1 — standalone; no concurrent workstream.
**Branch:** {feat/mNN-name — added when work begins}
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, in-session user-facing "zombie" terminology audit, Jun 03, 2026 — 3-surface grep + classification).

> **Provenance is load-bearing.** Agent-generated from a grep+classify audit of `ui/packages/website`, `ui/packages/app`, and `docs/architecture`. Every cited file:line below was returned by the audit; cross-check the rendered copy against the live components before EXECUTE.

**Canonical architecture:** `docs/architecture/direction.md:11` — the source-of-truth definition ("one zombie = a durable runtime, not a one-shot prompt"), reinforced by `high_level.md:13/69` and `capabilities.md:5`. This workstream surfaces that definition to users; it does NOT redefine the concept.

---

## Implementing agent — read these first

1. `docs/architecture/direction.md:11` (+ `high_level.md`, `capabilities.md:5`) — the canonical, contributor-facing definition of a zombie. The user-facing gloss must be faithful to this ("durable, autonomous, event-triggered runtime"), not a new invention.
2. `ui/packages/website/src/components/CTABlock.tsx:17` — the closest existing *implicit* gloss ("it wakes on the failure, reads the logs, and posts the diagnosis"). Mirror this voice; upgrade it from a behavioral hint to an explicit definition.
3. `ui/packages/app/app/(dashboard)/zombies/page.tsx:59` and `ui/packages/app/app/(dashboard)/page.tsx:67` — the two app first-touch surfaces (empty state, dashboard first-run card) that must carry the definition.
4. The app design-system primitives + `theme.css` tokens — any new tooltip/callout uses primitives (UI Substitution Gate) and tokens (DESIGN TOKEN Gate), never raw HTML or arbitrary values.
5. `docs/CHANGELOG_VOICE.md` — voice discipline (no "seamless"/"powerful"; lead with the change/meaning). The definition copy follows it.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Define "zombie" for users at first-touch surfaces
- **Intent (one sentence):** A first-time user understands what a zombie is the moment they meet the term — on the marketing site and in the app — instead of inferring it from install mechanics or a "zombie / agent / runtime" synonym soup.
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent and lists `ASSUMPTIONS I'M MAKING: …`, and confirms the final definition wording with Indy (voice review) before editing copy. A mismatch with the Intent above → STOP and reconcile.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (always applies). Specifically **UFS** (the definition string is a repeated semantic literal → a named constant; ui/ UFS is extracted by hand, not by the auditor) and **NLG** (no "legacy"/compat framing in new copy; pre-2.0).
- **No `*.zig` / `src/http/handlers/**` / `schema/*` surfaces are touched** — `ZIG_RULES.md`, `REST_API_DESIGN_GUIDELINES.md`, `SCHEMA_CONVENTIONS.md` do not apply.
- This is UI copy only; the binding constants are the design-system primitives + `theme.css` tokens (see Applicable Gates).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` touched |
| PUB / Struct-Shape | no | no Zig pub surface |
| File & Function Length (≤350/≤50/≤70) | maybe | edits to existing `.tsx`; keep each file ≤350 — split a component before crossing the cap |
| UFS (repeated/semantic literals) | **yes** | the definition copy lives in ONE named constant per UI package (website, app); spots reference the constant, never an inline duplicate. Extracted by hand (ui/ UFS is manual). |
| UI Substitution / DESIGN TOKEN | **yes** | any new tooltip/callout uses a design-system primitive (`asChild` for HTML semantics) + `theme.css` tokens; no raw `<div>`/`<section>`, no `*-[...]` arbitraries |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no logs, lifecycle, error codes, or schema touched |

---

## Overview

**Goal (testable):** A rendered first-touch surface on the website (Hero or the "What is usezombie?" FAQ) and in the app (the `zombies` empty state + the dashboard first-run card) each contains an explicit one-sentence definition of "zombie" (a durable, autonomous agent that wakes on an event, runs a skill, and reports back), sourced from a single named copy constant per package; the website's sole primary product noun is "zombie", with "agent"/"runtime" appearing only as gloss — all asserted by rendered (acceptance/render) tests.

**Problem:** A newcomer meets the product's core noun "zombie" ~36 times across the marketing site (6 user-facing uses, 5 unexplained) and the app (30 user-facing uses, all unexplained) and is never told what it is. The only definition lives in `docs/architecture/direction.md` — internal contributor docs the README explicitly routes real users away from. Compounding it, the website names the same thing three ways ("zombie", "the agent", "a long-lived runtime") without connecting them, so a reader must infer that zombie == agent == runtime.

**Solution summary:** Add a single faithful one-line definition at each user entry point — website Hero or "What is usezombie?" FAQ; the app "No zombies yet" empty state; the app dashboard first-run card; optionally the sidebar `Zombies` nav as a tooltip — sourced from one named copy constant per package, and reconcile the website vocabulary so "zombie" is primary and "agent"/"runtime" read as its gloss. The ~36 existing usages are NOT renamed; once the term is defined at first touch, the bare noun is legible everywhere else.

---

## Prior-Art / Reference Implementations

- **UI** → design-system primitives + `theme.css` tokens for any tooltip/callout. **Voice reference:** `ui/packages/website/src/components/CTABlock.tsx:17` already lists what an installed zombie *does* — this workstream promotes that behavioral hint to an explicit definition in the same voice. The canonical *concept* is `docs/architecture/direction.md:11`; the gloss is a faithful user-facing restatement, not a new claim.
- No new architecture; the shape is "inline definition at first-touch", a copy pattern, not a structural one.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/website/src/components/FAQ.tsx` | EDIT | the "What is usezombie?" answer states the explicit definition and connects zombie == agent == runtime |
| `ui/packages/website/src/components/Hero.tsx` | EDIT | (if the Hero is the primary first-touch) carries or links the one-line gloss; else demote "agent"/"runtime" to gloss-of-zombie |
| `ui/packages/website/src/{content,lib}/copy.ts` (or nearest existing copy/constants module) | CREATE/EDIT | single named constant holding the website definition string (UFS) |
| `ui/packages/app/app/(dashboard)/zombies/page.tsx` | EDIT | the "No zombies yet" empty-state description carries the definition |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | the dashboard first-run / "First wake" card carries the definition |
| `ui/packages/app/lib/copy.ts` (or nearest existing app copy/constants module) | CREATE/EDIT | single named constant holding the app definition string (UFS) |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT (optional) | sidebar `Zombies` nav gets a tooltip/`title` with the short gloss |

> The exact module path for each copy constant is the implementer's call (mirror the nearest existing constants/content module in each package); the FILES + ROLES are fixed here.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a copy patch — add an explicit definition at the handful of first-touch surfaces + reconcile the website vocabulary. Decomposed into "canonical wording" (§1), "app surfaces" (§2), "website surfaces + vocab" (§3), "optional nav affordance" (§4) so each lands and verifies independently.
- **Alternatives considered:** (a) rename "zombie" → "agent" across all 36 usages — **rejected**: "zombie"/`usezombie` is the brand and product name; the goal is to *define* the term, not abandon it. (b) a standalone glossary / docs-site page — **rejected for now**: users need the definition *inline at first touch*, not behind a click; a docs-site glossary is named as future work.
- **Patch-vs-refactor verdict:** this is a **patch** — the problem is "the core noun is undefined at first touch", which an inline gloss solves; no data model, component architecture, or routing changes. No larger refactor is hiding here.

---

## Sections (implementation slices)

### §1 — Canonical user-facing definition (single source per package)
Establish the one faithful, voice-reviewed definition of "zombie" (sourced from `direction.md`), stored as a named copy constant in each UI package. The exact wording is fixed in **Interfaces** (pending Indy voice review) so the website and app render identical phrasing. **Implementation default:** one constant per package referencing the same agreed wording, because website and app are separate Next.js packages with no shared content module today — the implementer mirrors each package's nearest existing constants module rather than introducing a new shared package.

- **Dimension 1.1** — a named constant holds the definition string in each touched package; no spot inlines a second copy of the wording → Test `test_zombie_definition_constant_single_source`
- **Dimension 1.2** — the rendered definition is faithful to `direction.md` (durable, autonomous, event-triggered; "not a one-shot prompt") → Test `test_zombie_definition_matches_canonical`

### §2 — App first-touch definitions
The two surfaces a newcomer lands on first render the definition. **Implementation default:** the definition goes in the empty-state *description* and the first-run card *body*, beside the existing install-mechanics copy — not as a separate modal — because first-touch context is where the term is first met.

- **Dimension 2.1** — the `zombies` "No zombies yet" empty state (`zombies/page.tsx`) renders the definition → Test `test_app_empty_state_defines_zombie`
- **Dimension 2.2** — the dashboard first-run / "First wake" card (`page.tsx`) renders the definition → Test `test_app_first_run_defines_zombie`

### §3 — Website definition + vocabulary reconciliation
The website's primary entry point (Hero or the "What is usezombie?" FAQ) states the definition explicitly, and the site stops presenting "agent"/"runtime" as competing undefined names — they appear only as gloss of "zombie".

- **Dimension 3.1** — the website primary entry point renders the explicit "a zombie is …" definition → Test `test_website_defines_zombie`
- **Dimension 3.2** — vocabulary consistency: in user-facing website copy, "agent"/"runtime" appear only adjacent to "zombie" as gloss, never as a standalone undefined product noun → Test `test_website_single_primary_noun`

### §4 — Sidebar nav affordance (optional)
The app sidebar `Zombies` nav (`Shell.tsx`) exposes the short gloss via a design-system tooltip/`title`, so the central nav item is self-explanatory. Optional — include only if §2/§3 land cleanly within scope.

- **Dimension 4.1** — the sidebar `Zombies` nav exposes the short gloss on hover/focus → Test `test_sidebar_zombies_tooltip`

---

## Interfaces

> The contract here is the **copy wording** (the user-visible strings), not a function signature. The strings below are **DRAFTS for Indy voice review** at PLAN — the agent confirms the final wording before EXECUTE, then both packages render it verbatim from their constant.

```
SHORT_GLOSS  (nav tooltip / one-liner):
  "A zombie is an autonomous agent that wakes on an event, runs your skill, and reports back."

LONG_DEFINITION  (FAQ answer / empty-state / first-run card):
  "A zombie is a durable, autonomous agent you install once. It sleeps until an
   event wakes it, runs your skill against that event, and reports back with
   evidence — a long-lived runtime, not a one-shot prompt."

Constraint: the website FAQ "What is usezombie?" answer must make the
zombie == agent == runtime equivalence explicit (one primary noun, the others
as gloss).
```

Wording is provisional; faithfulness to `direction.md` and Indy's voice is the gate, not the literal draft above.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Definition drift | a spot edits the wording inline instead of the constant | the named constant is the single source; a duplicate-literal test/grep fails the build, so the rendered copy can only change in one place |
| Synonym regression | new website copy reintroduces a bare "agent"/"runtime" as a standalone product noun | the vocab-consistency test (§3.2) fails, surfacing the un-glossed noun |
| Tooltip/callout overflow | the short gloss is too long for the nav affordance | the design-system tooltip truncates/wraps; the long form lives only on the larger surfaces |
| Runtime failures (timeout / auth / network / race / replay / quota) | — | **N/A** — this workstream changes static rendered copy only; no new data fetch, handler, auth path, or async behavior |

---

## Invariants

1. The user-facing definition is rendered from exactly one named constant per touched package — no inline duplicate of the wording. Enforced by a duplicate-literal grep/test in CI-runnable test, not by review discipline.
2. In user-facing website copy, "zombie" is the sole primary product noun; "agent"/"runtime" occur only adjacent to "zombie". Enforced by the §3.2 vocab-consistency test (a grep/assertion over the rendered marketing copy), not by reviewer vigilance.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_zombie_definition_constant_single_source` | each package exports one definition constant; grep finds no second inline copy of the wording → 1 source |
| 1.2 | unit | `test_zombie_definition_matches_canonical` | the constant text contains the canonical markers (durable, autonomous, event/wake, "not a one-shot prompt") → all present |
| 2.1 | e2e/render | `test_app_empty_state_defines_zombie` | render `zombies` page with zero zombies → the definition string is visible in the empty state |
| 2.2 | e2e/render | `test_app_first_run_defines_zombie` | render dashboard in first-run state → the definition string is visible on the first-run card |
| 3.1 | e2e/render | `test_website_defines_zombie` | render the website primary entry point → the explicit "a zombie is …" definition is visible |
| 3.2 | unit | `test_website_single_primary_noun` | scan user-facing website copy → no standalone "agent"/"runtime" product noun without an adjacent "zombie" → 0 violations |
| 4.1 | e2e/render | `test_sidebar_zombies_tooltip` | hover/focus the sidebar `Zombies` nav → the short gloss appears (skip if §4 deferred) |

**Regression:** the ~36 existing "zombie" usages and all internal references (`/zombies/`, `zombiectl`, `zombied`, `zombie-runner`, error codes) render unchanged — `test_no_internal_copy_changed` greps the diff to confirm no internal surface moved. **Idempotency/replay:** N/A — static copy.

UI is a user-facing Category, so §2/§3/§4 each carry a rendered (acceptance/render) test, not a unit substitute. Render tier is satisfied by the app/website `vitest` render tests or the Playwright acceptance suite, whichever the package already uses.

---

## Acceptance Criteria

- [ ] Website primary entry point renders an explicit "a zombie is …" definition — verify: `bun run test` (website render test) or the acceptance suite asserting the rendered text
- [ ] App `zombies` empty state + dashboard first-run card render the definition — verify: `cd ui/packages/app && bun run test`
- [ ] Definition sourced from a single named constant per package (no inline duplicate) — verify: `git grep -c "<canonical-phrase>" ui/packages/{app,website}` returns the constant only
- [ ] Website vocab: no standalone "agent"/"runtime" product noun without adjacent "zombie" in user-facing copy — verify: the §3.2 test
- [ ] No internal surface changed — verify: `git diff --name-only origin/main | grep -E "zombiectl|zombied|src/|schema/|openapi"` is empty
- [ ] `make lint` clean · app + website `bun run test` pass · `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: definition present in app first-touch surfaces
git grep -l "wakes on" ui/packages/app/app && echo "PASS" || echo "FAIL"
# E2: Build  — cd ui/packages/app && bun run build ; cd ui/packages/website && bun run build
# E3: Tests  — (cd ui/packages/app && bun run test) && (cd ui/packages/website && bun run test)
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile (Zig only) — N/A (no Zig touched)
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: Internal-surface untouched (empty = pass) —
git diff --name-only origin/main | grep -E "zombiectl|zombied|src/|schema/|openapi" | head
```

---

## Dead Code Sweep

N/A — no files deleted; this workstream only adds/edits copy and constants.

---

## Discovery (consult log)

- **Indy decisions (Jun 03, 2026):**
  > Indy (2026-06-03): "Yes create a spec and push that spec in this PR" — context: this spec is created in `pending/` and committed onto the M82_001 branch (PR #361), diverging from `kishore-spec-new`'s default `main` commit, per explicit instruction. Implementation is a separate later PR.
  > Indy (2026-06-03): "leave out the internals like api /zombies/... zombiectl, zombied zombie-runner or errors that are sent which is a larger change" — context: scope boundary; internal references are out of scope (see Out of Scope).
- **Audit source** — in-session 3-surface grep+classify (website 6 uses / 5 cryptic; app 30 uses / 30 cryptic; docs/architecture has the only definition at `direction.md:11`). Full per-spot list lives in the audit result of this session.
- **Skill chain outcomes** — appended during VERIFY/CHORE(close).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs the Test Specification — every Dimension has its render/unit test; the vocab + single-source invariants are tested. | Clean; iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, the canonical `direction.md` definition, UI Substitution + DESIGN TOKEN gates, voice (`CHANGELOG_VOICE.md`). | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR (copy accuracy, no internal-surface drift). | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| App render tests | `cd ui/packages/app && bun run test` | {paste snippet} | |
| Website render tests | `cd ui/packages/website && bun run test` | {paste snippet} | |
| Single-source / vocab | `git grep` per Acceptance Criteria | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Internal-surface untouched | `git diff --name-only origin/main \| grep -E "zombiectl\|zombied\|src/"` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |

---

## Out of Scope

- **Renaming the ~36 existing "zombie" usages** — the goal is to define the term at first touch, not rename it; "zombie"/`usezombie` is the brand.
- **Internals** — API paths `/zombies/...`, `zombiectl`/`zombied`/`zombie-runner`, error codes/messages sent over the wire. [Indy-acked deferral, Discovery] — "a larger change".
- **A standalone glossary / docs-site page** — the definition is inline at first-touch; a `docs.usezombie.com` glossary entry is future work.
- **`docs/architecture` changes** — the canonical definition already exists at `direction.md:11`; this workstream surfaces it, it does not edit it.
