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

# M165_001: The docs set says what the code does, in words a reader gets first time

**Prototype:** v2.0.0
**Milestone:** M165
**Workstream:** 001
**Date:** Aug 14, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — the contributor docs describe deleted tables and renumbered schema slots as current, and an agent planning from them plans against a system that no longer exists
**Categories:** DOCS
**Batch:** B1 — one document set, one reviewer, one pass
**Branch:** feat/m165-docs-truth-brevity
**Test Baseline:** unit=3743 integration=630
**Depends on:** none — the set is self-contained and no code changes
**Provenance:** LLM-drafted (Claude Opus 5, Aug 14, 2026), from a mechanical citation audit of all 23 files plus a read of the set
**Canonical architecture:** `docs/architecture/README.md` — the index this workstream keeps true

---

## Overview

**Goal (testable):** Every repository path, table name, make target and cross-document section anchor cited in `docs/architecture/**`, `docs/AUTH*.md` and `docs/development.md` resolves against the tree, no page describes a dropped table or a retired slot number as current, and no page carries a design that is recorded elsewhere as deferred.

**Problem:** The set is 7,287 lines across 23 files and nothing checks it. A citation audit finds tables that were dropped still documented as live storage — `fleet.metering_periods` in two pages, `core.credit_purchases` and `core.fleet_bundles` in three more, `core.tenant_billing` where the column now lives on `billing.tenant_wallet`. Schema slots were renumbered and the pages still cite the old numbers, `schema/013_memory_entries.sql` five times in one page and `schema/033_hot_path_indexes.sql` in two others. Rule pages that moved into the operating-model checkout are still cited as if they sat in this repository. The index points at a section title that the target page no longer uses. Separately, roughly a quarter of the canonical authentication page is a Backend-for-Frontend design the same page marks DEFERRED, so a reader looking up how authentication works today reads two hundred and fifty lines about something nobody is building. None of this is a small-diff problem: an agent reads these pages before planning, and a page that is wrong is worse than a page that is missing.

**Solution summary:** One pass over the set with four passes' worth of intent. Correct every citation against the tree and delete or rewrite every claim the code contradicts. Cut content that belongs somewhere else — deferred design to the roadmap page, duplicated invariant lists to the one page that owns them. Rewrite for a reader who has not read the rest of the set: shorter sentences, plain words, the answer before the reasoning. Keep the index and its anchors true, and leave behind a check that fails when a future edit breaks a citation, so the audit that found this runs again without a person remembering to run it.

## PR Intent & comprehension handshake

- **PR title (eventual):** Make the architecture docs true, shorter, and readable in one pass
- **Intent (one sentence):** A contributor or agent opening any page in the set gets a current, quickly scannable answer, and does not plan against a table that was dropped.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/README.md` — the index and the file table. Every structural decision in this workstream ends up reflected here, so read it before and after.
2. `schema/embed.zig` — the live slot list. It is the authority for which schema files exist and what number each carries; every page citing a slot is checked against it.
3. `schema/` — the live table set. It is the authority for which tables exist; every page naming a table is checked against it.
4. `docs/AUTH.md` §Roadmap — Flow 2 dashboard cleanup — the largest single block of deferred design in the set, and the judgement call this workstream has to get right.
5. `~/Projects/dotfiles/docs/DOCUMENTATION_RULES.md` — the voice these pages are rewritten into.
6. `docs/architecture/scenarios/README.md` — shows the pattern to follow: it already records why retired pages are still cited by shipped specs, which is the shape every deletion here needs.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/architecture/README.md` | EDIT | Index anchors, file table, and any row whose target changed |
| `docs/architecture/memory.md` | EDIT | Cites a renumbered schema slot five times |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Describes two tables that no longer exist; largest topic page after the flow |
| `docs/architecture/roadmap.md` | EDIT | Names a dropped table; receives deferred design cut from elsewhere |
| `docs/architecture/scaling.md` | EDIT | Cites a retired slot number |
| `docs/architecture/observability.md` | EDIT | Cites a rule page that moved out of this repository; section title drifted from the index |
| `docs/architecture/concurrency.md` | EDIT | Cites a dispatch page that moved out of this repository, and a make target that does not exist |
| `docs/architecture/data_flow.md` | EDIT | Largest page in the set; sequencing and brevity |
| `docs/architecture/runner_fleet.md` | EDIT | Second largest; brevity and citation repair |
| `docs/architecture/fleet_bundles.md` | EDIT | Names a table that does not exist |
| `docs/architecture/direction.md`, `high_level.md`, `capabilities.md`, `connectors.md`, `user_flow.md`, `web_app.md`, `testing.md`, `product_analytics.md` | EDIT | Anchor repair, duplication removal, readability |
| `docs/architecture/scenarios/*.md` | EDIT | One page names a table that does not exist; the index duplicates invariants the topic pages own |
| `docs/AUTH.md` | EDIT | Deferred design leaves the canonical reference; shorthand paths resolved |
| `docs/AUTH_DEVICE_LOGIN.md` | EDIT | Citation repair and readability |
| `docs/development.md` | EDIT | Citation repair and readability |
| `audits/` or `make/*.mk` | CREATE or EDIT | The citation check, wired to a target so it runs without being remembered |

## Applicable Rules

- **`~/Projects/dotfiles/docs/DOCUMENTATION_RULES.md`** — the voice for every rewritten sentence, read before the narrower guides.
- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (a page describing a design nobody is building is dead weight in prose form; delete or relocate rather than annotate), **NLR** (a page touched for one fix gets its other stale claims fixed in the same diff), **NLG** (no "formerly" or "pre-M154" framing while pre-launch; describe the design as the design), **ORP** (a deleted page and every inbound link retire together, including the index).
- **`dispatch/write_shell.md`** — the citation check is a shell script: quoted expansions, no untrusted evaluation, deterministic exit.
- **`~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md`** — read to confirm which slot numbers and table names are current before correcting a page that cites them.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no Zig changes | N/A |
| SCHEMA GUARD | no — schema is read as the authority, never edited | N/A |
| UI Substitution / DESIGN TOKEN | no — no user interface surface | N/A |
| LENGTH GATE | yes — the check script | keep it under the file cap; one function per concern |
| LOGGING / ERROR REGISTRY | no — no runtime surface | N/A |
| MILESTONE ID | yes — new script carries its milestone marker | marker in the script header |
| DOC READ GATE | yes — published-voice edits | `DOCUMENTATION_RULES.md` read and cited before the first prose edit |

## Prior-Art / Reference Implementations

- **Reference:** `docs/architecture/scenarios/README.md` — already records *why* retired pages are still named by shipped specs. Every deletion in this workstream leaves the same kind of note rather than a silent gap.
- **Reference:** `docs/architecture/scaling.md` — the house page shape: a Facts table that front-loads the answer, then Traps, Topology, Decisions, Detail. Pages that wander get pulled toward this shape rather than a new one.
- **Reference:** `audits/spec-template.sh` in the operating-model checkout — a documentation check that reads the tree, prints one line per file, and exits non-zero on a finding. The citation check mirrors its output shape.

## Sections (implementation slices)

### §1 — Every citation resolves, and the check keeps it that way

The audit that motivated this workstream is mechanical: extract every backticked repository path, table name, make target and cross-page section anchor, and resolve each against the tree. It found dropped tables described as live storage, renumbered slots cited by their old numbers, rule pages cited at paths inside this repository after they moved out of it, and an index anchor naming a section title its target no longer uses. Fixing them by hand fixes today; landing the extractor as a check fixes tomorrow. **Implementation default:** the check reads the tree and never a list of expected values, so adding a page or renaming a table cannot leave it silently passing.

- **Dimension 1.1** — every backticked repository path in the set resolves to a tracked file, or is written in a form the check recognises as shorthand → Test `test_every_cited_path_resolves`
- **Dimension 1.2** — every schema-qualified table name in the set exists in `schema/` → Test `test_every_cited_table_exists`
- **Dimension 1.3** — every cross-page section anchor names a heading its target actually carries → Test `test_every_section_anchor_resolves`
- **Dimension 1.4** — every `make` target named in the set exists in the makefile set → Test `test_every_cited_make_target_exists`
- **Dimension 1.5** — the check runs from a repository target and fails the build on a finding → Test `test_citation_check_fails_on_a_planted_break`

### §2 — No page states something the code contradicts

A broken link is visible; a wrong sentence is not. Pages describe storage that was dropped, and the prose around those names asserts behaviour that went with them. This slice reads each claim about storage, endpoints and lifecycle against the tree and rewrites what the code disagrees with — not by appending a correction, which leaves both readings on the page, but by making the page say one true thing. **Implementation default:** where a claim cannot be settled from the tree, the sentence goes rather than being hedged; an unverifiable claim in a reference page is worse than its absence.

- **Dimension 2.1** — no page describes a dropped table as current storage → Test `test_no_page_describes_a_dropped_table`
- **Dimension 2.2** — no page cites a schema slot number that `schema/embed.zig` does not carry → Test `test_no_page_cites_a_retired_slot`
- **Dimension 2.3** — no page cites a rule or dispatch page at a repository-local path after it moved to the operating-model checkout → Test `test_relocated_rule_pages_cite_their_real_home`
- **Dimension 2.4** — each page's opening claim about what it covers matches what it now contains → Test `test_page_scope_lines_match_contents`

### §3 — Deferred design and duplicated facts leave the reference pages

The canonical authentication page carries a Backend-for-Frontend design across six sections that the same page marks deferred, which is roughly a quarter of it. The scenarios index restates five invariants the topic pages own, immediately after saying scenarios link rather than copy. Both are the same failure: a reference page carrying content whose home is elsewhere, so a reader has to work out which copy is current. **Implementation default:** deferred design moves to the roadmap page in compressed form with its reasoning intact, duplicated facts collapse to a link, and the page that owned the fact keeps it in full.

- **Dimension 3.1** — the authentication page describes what authentication does now; the deferred direction lives in the roadmap page → Test `test_auth_page_carries_no_deferred_design`
- **Dimension 3.2** — no invariant is stated in full on two pages; the second is a link → Test `test_invariants_are_stated_once`
- **Dimension 3.3** — every relocation leaves the reasoning intact at its new home rather than dropping it → Test `test_relocated_content_keeps_its_reasoning`

### §4 — The set reads in plain words, and the index still finds everything

A reference page is read under pressure by someone who has not read the rest of the set. The pages that fail are the ones with sentences carrying three clauses and a parenthetical, compressed phrasing that needs a second read, and answers that arrive after their justification. This slice rewrites for first-read comprehension and confirms the index still routes a question to the section that answers it. **Implementation default:** the answer opens the paragraph, one idea per sentence, and a term is expanded where it is first used rather than assumed.

- **Dimension 4.1** — every page opens with what it answers, before any mechanism → Test `test_pages_lead_with_their_answer`
- **Dimension 4.2** — the index routes every listed question to a section that exists and answers it → Test `test_index_questions_reach_their_answers`
- **Dimension 4.3** — the file table lists every page in the set and no page is absent from it → Test `test_file_table_is_complete`
- **Dimension 4.4** — no page uses the banned marketing vocabulary or unexpanded uncommon abbreviations → Test `test_documentation_voice_holds`

## Interfaces

```
NO RUNTIME SURFACE   No endpoint, command, flag or schema changes. The product
                     behaves identically before and after.

NEW CHECK            A repository target runs the citation audit over the docs
                     set and exits non-zero on any unresolved path, table, make
                     target or section anchor. Output is one line per file
                     checked plus a finding list.

INDEX                docs/architecture/README.md remains the single entry point:
                     a question index and a file table, both complete.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Shorthand path read as broken | Pages cite `http/router.zig` for a file under `src/agentsfleetd/` | The check resolves by basename against tracked files before reporting; only genuinely unresolvable paths are findings |
| A shipped spec cites a page this workstream deletes | Historical specs name pages by their ship-time path | Deletions leave a note recording the retirement, following the pattern the scenarios index already uses; shipped specs are never rewritten |
| A correction is itself wrong | The tree is read carelessly and a true sentence is replaced with a false one | Every corrected claim cites the file it was read from, so a reviewer can check the source rather than the assertion |
| Rewriting drops a load-bearing fact | Brevity applied without judgement | A fact removed for brevity must exist in full on the page that owns it; the check for stated-once invariants confirms the survivor |
| The check passes because it finds nothing to check | An extraction pattern silently matches zero lines | The check asserts a non-zero citation count per page before reporting clean |
| A page becomes shorter but no clearer | Compression mistaken for readability | Readability is graded on the opening claim and sentence shape, not on line count |

## Invariants

1. **A citation that does not resolve fails the build.** Enforced by the check running from a repository target, not by review.
2. **A fact is stated in full on exactly one page.** Enforced by the duplicate-invariant check and by every other mention being a link.
3. **The index lists every page and every question reaches a real section.** Enforced by the anchor and file-table checks.
4. **No page describes storage the schema does not define.** Enforced by resolving every schema-qualified name against `schema/`.
5. **Deletions leave a record.** Enforced by the retirement-note pattern, so a shipped spec citing a retired page still leads somewhere.

## Metrics & Observability

No product or operator signal changes. This workstream touches contributor documentation and one repository check; no runtime code, no endpoint, no analytics event, no metric family. The only new signal is the check's own exit status, which is a build outcome rather than telemetry.

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Citation check outcome | repository target | The docs check runs | file count, finding count | No file contents in output beyond the citation line | `test_citation_check_fails_on_a_planted_break` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_every_cited_path_resolves` | Every backticked repository path resolves to a tracked file or a recognised shorthand |
| 1.2 | unit | `test_every_cited_table_exists` | Every schema-qualified table name appears in a `schema/*.sql` definition |
| 1.3 | unit | `test_every_section_anchor_resolves` | Every cross-page anchor names a heading the target page carries |
| 1.4 | unit | `test_every_cited_make_target_exists` | Every `make <target>` named in the set exists in the makefile set |
| 1.5 | unit | `test_citation_check_fails_on_a_planted_break` | A deliberately broken citation makes the check exit non-zero |
| 2.1 | unit | `test_no_page_describes_a_dropped_table` | No page names a table absent from `schema/` as current storage |
| 2.2 | unit | `test_no_page_cites_a_retired_slot` | No page cites a slot number `schema/embed.zig` does not carry |
| 2.3 | unit | `test_relocated_rule_pages_cite_their_real_home` | No page cites a relocated rule page at a repository-local path |
| 2.4 | unit | `test_page_scope_lines_match_contents` | Each page's stated scope matches the sections it now carries |
| 3.1 | unit | `test_auth_page_carries_no_deferred_design` | The authentication page carries no section describing deferred design |
| 3.2 | unit | `test_invariants_are_stated_once` | No invariant paragraph appears in full on two pages |
| 3.3 | unit | `test_relocated_content_keeps_its_reasoning` | Content moved to the roadmap page retains its rationale |
| 4.1 | unit | `test_pages_lead_with_their_answer` | Each page's first paragraph states what it answers |
| 4.2 | unit | `test_index_questions_reach_their_answers` | Every index row resolves to a section that answers its question |
| 4.3 | unit | `test_file_table_is_complete` | The file table lists every page in the set, and only those |
| 4.4 | unit | `test_documentation_voice_holds` | No banned vocabulary; uncommon abbreviations expanded at first use |
| regression | unit | `test_no_runtime_file_changed` | The diff touches documentation and the check only |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every citation in the set resolves | the new docs-check target | exit 0 | P0 | |
| R2 | No dropped table is described as current | `grep -rnE 'metering_periods\|credit_purchases\|fleet_bundles\|core\.tenant_billing' docs/architecture docs/AUTH*.md docs/development.md` | no output outside a recorded retirement note | P0 | |
| R3 | No retired slot number is cited | `grep -rnE 'schema/0[0-9][0-9]_' docs/` | no output | P0 | |
| R4 | Deferred design is out of the reference page | `grep -nE '^#+ .*(Stage 2\|BFF\|Backend-for-Frontend)' docs/AUTH.md` | no output | P0 | |
| R5 | The index is complete | `test_file_table_is_complete` | passes | P0 | |
| R6 | No runtime file changed | `git diff --name-only origin/main... \| grep -vE '^(docs/\|audits/\|make/)'` | no output | P0 | |
| S1 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

**1. Orphaned references — zero remaining live uses.** Each grep drops comment lines, so prose recording a retirement cannot fail the criterion asserting it.

| Deleted symbol/column | Grep | Expected |
|-----------------------|------|----------|
| retired page names | `grep -rn 'data_lifecycle\.md' docs/` | 0 matches |
| dropped tables | `grep -rn -w 'metering_periods' docs/architecture docs/AUTH.md` | 0 matches outside a retirement note |
| relocated rule pages | `grep -rn 'docs/LOGGING_STANDARD.md\|`dispatch/' docs/architecture` | 0 repository-local cites |

## Out of Scope

- **`docs/v1/` and `docs/v2/` specs** — shipped and pending specs are historical records of what a milestone touched. They are never rewritten, even when they cite a page this workstream changes.
- **`~/Projects/docs`** — the user-facing documentation site is a separate repository with its own branch flow.
- **`playbooks/`** — operational runbooks are a different audience with a different voice; a sweep there is its own workstream.
- **New pages** — this workstream corrects and consolidates what exists. A gap that needs a new page is recorded in Discovery, not filled here.
- **Code changes to match the docs** — where a page and the tree disagree, the tree wins and the page changes. A case where the code looks wrong is recorded, not fixed.
- **The operating-model checkout** — rule and dispatch pages live outside this repository and are edited through their own flow.

## Product Clarity (authoring record)

1. **Successful user moment** — N/A, no user surface. The contributor moment: someone opens a page, gets the answer in the first screen, and it is true.
2. **Preserved user behaviour** — N/A, no user surface. Nothing the product does changes.
3. **Optimal-way check** — the optimal shape is a set where every claim is checkable and checked, so freshness survives the person who cared about it. A one-time cleanup without the check decays back within a few milestones.
4. **Rebuild-vs-iterate** — iterate. The page shapes are sound and the index works; what has drifted is the content and the length.
5. **What we build** — corrected pages, relocated deferred design, collapsed duplication, a plainer voice, and a citation check wired to a target.
6. **What we do NOT build** — new pages, spec rewrites, code changes, or a documentation site generator.
7. **Fit with existing features** — the check joins the existing repository targets and runs where the other documentation gates run.
8. **Surface order** — N/A, no user surface. The index keeps its current ordering; only broken rows move.
9. **Dashboard restraint** — N/A, no dashboard surface.
10. **Confused-user next step** — N/A for end users. A contributor hitting a check failure gets the file, the citation and what it failed to resolve against, which is enough to fix it without reading the script.

## Decomposition & alternatives (patch vs refactor)

- **Chosen — one pass over the whole set, plus a check.** The failures are the same four shapes on every page, so a single pass costs little more than a partial one and leaves the set consistent. The check is what stops this recurring.
- **Rejected — fix only the pages with dropped tables.** It leaves the anchors, the shorthand paths and the deferred design in place, and the next reader hits one of those instead.
- **Rejected — a documentation generator that derives pages from code.** These pages carry reasoning, not just facts; generating them would lose exactly the part that makes them worth reading.
- **Rejected — delete the pages that have drifted.** They are cited by shipped specs and by the operating model, and the content is sound where it is current. Correction is cheaper than reconstruction.
- **Rejected — a new page collecting the corrections.** The set is already at 23 files and adding one is the opposite of the ask.
- **Deferred — a sweep of `playbooks/`.** Different audience, different voice, and no evidence yet that it has drifted the same way.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close)).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.

- **Why this workstream exists (Aug 14, 2026).** Indy asked for the architecture set to be reviewed for duplicates, brevity, agent and human readability, staleness against the code, factual truth, sequencing, and plain language — and for pages that are not needed to be consolidated rather than added to. A mechanical citation audit run before drafting confirmed the staleness is not stylistic: dropped tables, renumbered slots and relocated rule pages are all still cited as current.

> Indy (2026-08-14): "I dont want a new md now. But what i have must be cleaned." — context: the immediate trigger; a proposed new architecture page was withdrawn and this workstream replaces it.

- **Open decision — where the deferred authentication design goes.** Compressing it into the roadmap page keeps one place for direction, which is what that page is for. Deleting it outright is also defensible, since the reasoning is recorded in the shipped specs that produced it. The spec assumes relocation; a decision to delete only shortens the diff.
