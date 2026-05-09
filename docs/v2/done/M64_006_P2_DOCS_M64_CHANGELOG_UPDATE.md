<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
-->

# M64_006: Docs-repo changelog `<Update>` for the M64 design-system rollout

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 006
**Date:** May 08, 2026
**Status:** DONE
**Priority:** P2 — the changelog entry is shipped-feature evidence; lands AFTER the underlying work (W1+W2+W3+W4) so the prose matches what's deployed. P2 because the user-facing docs site already auto-deploys on changelog merge; no engineering blocker waiting on this.
**Categories:** DOCS
**Batch:** B5 — depends on M64_002 (W1), M64_003 (W2), M64_004 (W3), M64_007 (W4) all merged AND M64_005 (e2e harness) verifying the dashboard against `api-dev`. The changelog entry's claims must reflect verified shipped behaviour, not aspirational prose.
**Branch:** `chore/m64-changelog` (in `~/Projects/docs/`, branched from `main`)
**Depends on:** M64_002 + M64_003 + M64_004 + M64_005 + M64_007 all DONE.

**Canonical surface:** `~/Projects/docs/changelog.mdx` — the public changelog at `docs.usezombie.com/changelog`. Mintlify renders `<Update>` blocks chronologically newest-first.

---

## Implementing agent — read these first

1. `~/Projects/docs/changelog.mdx` — current state. Read the most recent three `<Update>` blocks for voice, length, and tag conventions.
2. `~/Projects/dotfiles/skills/release-template.md` — the canonical changelog template per `AGENTS.md → CHORE(close)`. Re-source this on every release; do not paraphrase from prior entries.
3. `docs/v2/done/M64_001_*.md` through `docs/v2/done/M64_004_*.md` (in this repo) — the source-of-truth for what shipped in each workstream. The changelog summarises; it does not invent.
4. `docs/DESIGN_SYSTEM.md` — language ground truth. The changelog must use "Operational Restraint" as the named system, "the wake-pulse" as the named signature, and "Commit Mono + Instrument Sans" as the named typeface pair. No "minimal" / "clean" / "modern" filler.
5. The W3 commit (`c036b48d` on `feat/m64-002-design-w1`) and the W2/W4 commit ranges — the implementation truth. If a changelog claim doesn't appear in the diff, it doesn't ship in the changelog.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal.
- `~/Projects/dotfiles/skills/release-template.md` — never paraphrase; re-source on every release.
- File & Function Length Gate — n/a (markdown).
- "Changelog claim challenge" (per AGENTS.md DOCUMENT stage) — every `<Update>` claim must answer "Would this be true if the test file vanished?" Only test evidence (not middleware/handler/CLI) → claim unearned.

---

## Anti-Patterns to Avoid

N/A — spec authoring complete; the implementing agent reads sections below as goal contract, not pseudocode.

---

## Overview

**Goal (testable):** A new `<Update>` block lands at the top of `changelog.mdx` on a `chore/m64-changelog` branch in the `~/Projects/docs/` repo, summarising the M64 design-system rollout in three sections — *What's new*, *Why it matters*, *What's next*. The entry is dated to the day W3 (PR #308) merges, tagged with the right Mintlify tags (`["What's new", "Design", "App", "Website", "Docs"]`), and uses voice + structure consistent with the most recent existing `<Update>` blocks. Mintlify deploy preview renders the entry without markdown errors.

**Problem:** M64 ships across five workstreams (W1 token swap → W2 website apply → W3 app apply → W4 docs site apply → W5 zombiectl palette, already shipped) over several PRs. Customers reading the changelog see a sequence of internal-sounding commits with no narrative. Without the consolidated `<Update>` they cannot tell whether "the design changed" is a small visual sweep or a foundational rebrand.

**Solution summary:** One consolidated changelog entry that frames the work as "Operational Restraint applied end to end" — token + typography swap shared by every surface, the wake-pulse signature live in the dashboard, the docs site reading in the same voice, the CLI palette consistent. Three sections, ≤400 words total, with one inline image of the dashboard's pulse-cap shape (the most demonstrable visual evidence). No version bump (changelog-only).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `~/Projects/docs/changelog.mdx` | EDIT | Add new `<Update>` block at the top (newest-first ordering). |
| `~/Projects/docs/images/changelog/m64-pulse-cap.png` | NEW | One screenshot — the dashboard's zombie-list with the 5-simultaneous-pulse cap visible. PNG, ≤200KB, served from the docs CDN. |
| (in this repo, separate commit) `docs/v2/active/M64_006_*.md` → `docs/v2/done/M64_006_*.md` | EDIT | CHORE(close) — move spec to done after the docs PR merges. |

---

## Workstreams

### Workstream A — write the entry

Re-source `~/Projects/dotfiles/skills/release-template.md`. Compose the entry following the template's frontmatter shape:
- Date label (`May DD, 2026` — the day W3 merges)
- Tags: `["What's new", "Design", "App", "Website", "Docs"]`
- H2 headline: "Operational Restraint" or close paraphrase — language anchored in `docs/DESIGN_SYSTEM.md`
- Body sections (use `##` H2 within the Update body): What's new (the four shipped workstreams summarised); Why it matters (one paragraph on what changed for the customer reading the docs / using the dashboard); What's next (one sentence pointing at the e2e harness landing as the verification anchor)
- One `<Frame>` with the pulse-cap screenshot (W3 surface)

### Workstream B — capture the evidence screenshot

Run `app-dev` against a seeded fixture user with 6 active zombies. Browser Playwright captures `/zombies` at 1440×900 viewport. Crop to the list table only (no chrome). Output `~/Projects/docs/images/changelog/m64-pulse-cap.png`.

### Workstream C — Mintlify deploy preview check

Push the branch, open a PR against `usezombie/docs:main`, wait for the Mintlify preview deploy bot. Inspect the rendered `<Update>` block — markdown lint passes, image renders, tags surface in the right colour. Merge once green.

---

## Failure Modes & Invariants

| Mode | What goes wrong | How the spec catches it |
|------|-----------------|-------------------------|
| Aspirational claims | Changelog entry references behaviour that doesn't actually ship in the merged PRs | Changelog claim challenge — every line must trace to a test file or merged commit, not a roadmap intent |
| Voice drift | New entry reads differently from the most recent three `<Update>` blocks | Implementing agent reads the prior three entries first; voice match is a manual check |
| Image bloat | PNG screenshot ships at multi-MB scale and slows the changelog page LCP | ≤200KB hard cap; if screenshot exceeds, re-export with quality reduction |
| Wrong-repo commit | Entry lands in `usezombie/usezombie` instead of `usezombie/docs` | Branch name `chore/m64-changelog` is repo-scoped to `~/Projects/docs/`; AGENTS.md "docs-repo edits on own branch" rule fires |
| Tag drift | Tags use a label not already in use elsewhere in the changelog (Mintlify's tag taxonomy is implicit) | Re-use existing tag values: "What's new", "Design", "App", "Website", "Docs"; do not invent |

**Architectural invariant:** the changelog entry never includes specs, internal milestone IDs, or repo-internal terminology. Customers do not need to know about M64; they need to know that "the dashboard now visibly indicates which agents are alive."

---

## Test Specification

| Test | Asserts |
|------|---------|
| Mintlify preview render | `<Update>` block renders without markdown errors; tags chips surface correctly |
| Image weight | `m64-pulse-cap.png` ≤ 200KB; format PNG |
| Word count | Entry body ≤ 400 words across all three sections |
| Tag re-use | Every tag in the new entry already appears in the existing `changelog.mdx` |
| Voice match | Reads consistent with the most recent three entries (subjective; reviewer eye) |

---

## Acceptance Criteria

- New `<Update>` block at top of `changelog.mdx`, dated W3 merge day.
- One screenshot at `~/Projects/docs/images/changelog/m64-pulse-cap.png`, ≤200KB.
- Mintlify preview deploys cleanly; preview URL inspected and shipped.
- PR opened against `usezombie/docs:main` from `chore/m64-changelog`; merged after `/review` and human review.
- Spec moved `docs/v2/active/` → `docs/v2/done/` in the usezombie repo (separate commit on whichever branch is open at the time).

---

## Out of Scope

- Twitter / LinkedIn / launch posts — separate marketing milestone.
- README updates in any repo (not user-visible enough to warrant changelog cross-promotion here).
- Welcome email template (deferred to M64_005 Workstream-D bonus).
- Mintlify mint.json configuration changes (those are W4's territory).
- Any backfill of older changelog entries.

---

## Discovery (out-of-scope but adjacent observations)

- The `<Update>` template in `~/Projects/dotfiles/skills/release-template.md` should be inspected for staleness during this milestone; if it predates the recent voice shifts in the changelog, propose an update there. Do not silently fork.
- The pulse-cap screenshot is reusable. Consider promoting it to `docs/images/dashboard/` (general-purpose dashboard hero) so future Update blocks can re-reference it without re-capturing.

---

## Implementation Notes

- This spec lives in `~/Projects/usezombie/docs/v2/pending/` BUT the implementation lands in `~/Projects/docs/changelog.mdx` (a separate repo). Per AGENTS.md "Docs-repo edits on own branch": commit on `chore/m64-changelog`, push origin master only after PR merges.
- The release-template.md skill must be re-sourced for the entry shape; do not paraphrase from this spec or prior changelog entries verbatim. The template is the canonical voice.
- The implementing agent should NOT cross-commit usezombie repo files onto the docs branch — those are separate PRs.
