# M50_001: ARCHITECHTURE.md Cross-Reference + Post-Launch Reflection

**Prototype:** v2.0.0
**Milestone:** M50
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P2 — packaging hygiene. The architecture doc was rewritten in Apr 25, 2026 ahead of M40-M49 work. This spec keeps it accurate as the substrate ships and adds a post-launch §14 reflection capturing what shipped vs what was planned.
**Categories:** DOCS
**Batch:** B4 — runs after M40-M49 land. Cosmetic + reflective.
**Branch:** feat/m50-architecture-cross-reference (to be created when work starts)
**Depends on:** M40, M41, M42, M43, M44, M45, M46, M47, M48, M49 — substrate + packaging shipped.

**Canonical architecture:** `docs/ARCHITECHTURE.md` itself (the doc being maintained).

---

## Implementing agent — read these first

1. `docs/ARCHITECHTURE.md` — current state (rewritten Apr 25, 2026).
2. Each shipped spec under `docs/v2/done/` (post-implementation) — the source of truth for what actually shipped.
3. The launch tweet, blog post, HN post (if any) — what was claimed publicly.
4. Last 90 days of commits on `feat/m{40-49}-*` branches — actual implementation diffs.

---

## Overview

**Goal (testable):** `docs/ARCHITECHTURE.md` is up-to-date as of post-launch:
- Every reference to `M{N}` substrate spec points at the actually-shipped `docs/v2/done/M{N}_001_*.md` (not the pending version).
- Cross-references are correct: §10 capability rows link to the spec that owns each capability.
- A new §14 ("Ship Reflection") captures: what was planned vs what shipped, what surprised us, what we deferred, what the launch evidence shows.
- Numbering is internally consistent (no `§9 with §8.x subsections` bug, no orphan section references).
- The doc reads cleanly to a new reader: someone joining the project at Day +60 can understand the runtime + wedge framing in one read.

**Problem:** The architecture doc was rewritten BEFORE substrate shipped. Predictions in the doc (e.g., "M34 owns event history, ingest is M43") are guesses. Reality may differ — implementation always reveals constraints. Without a reconciliation pass, the doc stays plausible but slightly wrong forever; future readers can't tell what's spec and what's reality.

**Solution summary:** Three small passes after launch:
1. **Cross-reference correctness pass.** Walk every `(M{N})` mention in the doc; verify the spec exists in `done/`, the spec's title matches what the doc claims it does, the spec's interfaces match what the doc references.
2. **Reflection appendix.** New §14 — a 200-500 word reflection on what shipped vs planned. Decisions that proved right; decisions that didn't survive contact with implementation; what we'd do differently. Brief but honest.
3. **New-reader smoke test.** Have one person who didn't work on the v2 substrate read the doc cold and write down anything confusing. Fix those.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/ARCHITECHTURE.md` | EDIT | Cross-reference + reflection appendix + clarity fixes from cold-read |
| `~/Projects/docs/...` (docs.usezombie.com source — covered by M51) | NO EDIT here | M51 owns docs.usezombie.com sync |

> **Note:** This spec has minimal blast radius. It's a doc-hygiene chore, not a feature.

---

## Sections (implementation slices)

### §1 — Cross-reference correctness pass

For each spec referenced in `docs/ARCHITECHTURE.md` (M40, M41, M42, M43, M44, M45, M46, M47, M48, M49):

1. Confirm the spec is in `docs/v2/done/` (not still pending or active).
2. Read the spec's `## Overview` section; verify the capability description in ARCHITECHTURE.md §10 matches the actual scope.
3. Verify any interfaces the architecture doc names (e.g., "POST /steer", "x-usezombie.context.tool_window") match what shipped.
4. If a spec was renamed or merged with another during implementation, update the architecture doc accordingly.

Output: a one-line note in §14 per spec — either "matches plan" or "deviated: <one line>".

### §2 — Cold-read smoke test

Pick one engineer who did not work on M40-M49. Have them read ARCHITECHTURE.md end-to-end. Capture every place they pause to ask "what does this mean?" or "is this still true?". Fix those without diluting the doc — usually a one-sentence clarification in the offending paragraph.

> **Implementation default:** if no engineer outside M40-M49 work is available, ask Codex CLI (or another LLM in a fresh context) to read the doc and flag confusing passages. Treat as supplementary; humans catch things LLMs miss.

### §3 — §14 Ship Reflection

Add a new section at the end of ARCHITECHTURE.md (after §13 Path to Bastion):

```markdown
## 14. Ship Reflection (post-launch, Q2 2026)

### What shipped vs planned

[Brief — 1-3 paragraphs. Cover: did the wedge ship as designed (GH Actions
trigger + chat steer + Slack post)? Did the substrate (M40-M45) hold up?
Did context layering (M41) avoid the embarrassment Codex predicted?]

### What surprised us

[1-2 paragraphs. Decisions that didn't survive contact with implementation.
Bugs we didn't see in the design doc. Operational learnings.]

### What we deferred

[1 paragraph. Specifically: did self-host validation slip? Did BYOK ship
with launch or after? Did the install-skill cover every host or just
Claude Code?]

### Evidence

- Launch date: <YYYY-MM-DD>
- First external install: <YYYY-MM-DD>, <operator> at <company>
- Public artifacts: <URLs to the launch post, HN thread, Loom demo>
- First real external incident the zombie diagnosed: <YYYY-MM-DD, brief>
```

> **Implementation default:** keep this section under 600 words. Don't repeat content from §0-§13. Reflection is what's NEW post-ship — surprises, deferred items, evidence.

### §4 — Numbering and anchor sanity

Run a markdown-anchor audit: `grep -E "§[0-9]+|\\[.*\\]\\(#[a-z-]+\\)"` and verify every reference resolves to an actual section. Fix any orphans.

### §5 — Out-of-scope follow-up specs

If the cross-reference pass surfaces real architectural questions that the post-ship reality created (e.g., "we shipped without bastion; the path-to-bastion section in §13 needs a meaningful update"), file those as follow-up specs in pending/. Don't bloat M50 with new architectural changes.

---

## Interfaces

```
N/A — this is a documentation chore, no API surface changes.
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Cross-reference pass finds a spec that didn't ship | Substrate work descoped mid-implementation | Update ARCHITECHTURE.md to remove the reference; file a new pending spec capturing the gap |
| Cold-reader finds a fundamental architecture confusion | Doc was wrong from the start | Refactor the offending section. Don't just paper over with one-line clarifications. |
| §14 reflection drifts into roadmap | Common drift — reflection becomes "what's next" | Reject. §14 is what HAPPENED, not what's next. Future work goes in pending specs. |

---

## Invariants

1. **Every `(M{N})` reference in ARCHITECHTURE.md points at a real, shipped spec.** Verified by grep + ls of `docs/v2/done/`.
2. **§14 is reflective, not prescriptive.** Contains evidence (dates, names, URLs), not plans.
3. **Cold-read smoke test is a real read by a real human.** Not skipped.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_all_M_references_resolve` | grep + ls — every `M{N}` mentioned has a corresponding `docs/v2/done/M{N}_*.md` file |
| `test_anchor_links_resolve` | grep `(#anchor)` style links in the doc → all targets exist as headers |
| `test_§14_present_and_under_600_words` | After M50 ships, §14 exists with non-empty content under word cap |
| `test_no_orphan_TODO_in_doc` | grep `TODO\|TKTK\|FIXME` in the doc → 0 hits |

> **Implementation default:** these tests can be a single shell script run as part of the M50 PR's CI; no need for a Zig integration test.

---

## Acceptance Criteria

- [ ] Cross-reference pass complete; every `M{N}` reference verified
- [ ] §14 Ship Reflection added with real evidence (launch date, first external install, URLs)
- [ ] Cold-read smoke test done; resulting clarity fixes applied
- [ ] No orphan TODOs / FIXMEs in the doc
- [ ] Doc passes a final read by author + at least one outside reader

---

## Out of Scope

- Re-architecting based on post-launch learnings. If the substrate decisions look wrong in retrospect, file follow-up specs; don't rewrite the doc.
- Sync with `~/Projects/docs/` (docs.usezombie.com) — that's M51's responsibility.
- Marketing-tone polish. The architecture doc stays technical; marketing copy lives on docs.usezombie.com.
