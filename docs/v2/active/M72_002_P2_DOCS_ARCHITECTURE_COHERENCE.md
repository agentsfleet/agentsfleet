<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`).
-->

# M72_002: Architecture-Directory Coherence Pass

**Prototype:** v2.0.0
**Milestone:** M72
**Workstream:** 002
**Date:** May 17, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — contributor-facing; not blocking any user surface but compounds drift over time.
**Categories:** DOCS
**Batch:** B1 — runs parallel to M72_001 in a separate worktree, different repo.
**Branch:** chore/m72-arch-coherence (worktree at `~/Projects/usezombie-m72-arch-coherence/`)
**Depends on:** None. Sibling to M72_001 under the same milestone. Shares no files.
**Provenance:** human-written (Kishore, May 17, 2026) — surfaced after M72_001 audit by Captain ask to "do the same for the architecture set."

**Canonical architecture:** the directory this spec touches is itself the canonical architecture set — `~/Projects/usezombie/docs/architecture/`.

---

## Implementing agent — read these first

1. `~/Projects/usezombie/docs/architecture/README.md` — the directory's own table of contents and glossary. The proposed installer-signpost and the deduped glossary land here first.
2. `~/Projects/usezombie/docs/architecture/high_level.md` §1–§7 — the contributor's first stop. Houses the §6/§7 why-not-OpenClaw duplication and the one-paragraph summary that duplicates with README.
3. `~/Projects/usezombie/docs/architecture/user_flow.md` §8.7, `billing_and_provider_keys.md` §9–10, `capabilities.md` §4 — the three places where the model-cap origin story is told. Pick the canonical home; the other two narrow to a lens reference.
4. `~/Projects/usezombie/docs/architecture/office_hours_v2.md`, `plan_engg_review_v2.md`, `ship_reflection.md` — historical / pending artifacts that need explicit status headers.
5. `~/Projects/usezombie/docs/v2/pending/M72_001_*.md` — the sibling workstream. Coordinate on shared vocabulary so a docs.usezombie.com page that cites an architecture section still resolves after this cleanup.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal docs discipline. RULE NDC (no dead links after a rename), RULE NLR (touch-it-fix-it on adjacent stale claims).
- `~/Projects/usezombie/AGENTS.md` — repo-level conventions for doc edits and cross-reference hygiene.
- `docs/SCHEMA_CONVENTIONS.md`, `docs/ZIG_RULES.md`, `docs/REST_API_DESIGN_GUIDELINES.md` — N/A. DOCS-only spec; no code paths.

---

## Overview

**Goal (testable):** A contributor or installer landing in `~/Projects/usezombie/docs/architecture/` knows within the first paragraph of README.md (a) whether this directory is what they want and (b) where to go if it is not. Every architectural concept named in the README glossary has exactly one canonical inline definition elsewhere in the directory; the README glossary points at that definition rather than restating it. Every historical or unshipped artifact carries a status header that disambiguates its standing as canon. The model-cap origin story has one canonical home; the other two mentions reduce to lens-only summaries with a pointer to canon — verified by grep + line-count assertions in §Acceptance Criteria.

**Problem:** Eight observable coherence issues for a new contributor or a confused installer who lands in `architecture/`:

1. **Installer signpost missing.** README.md does not tell a user-who-wants-to-install-the-product that they are in the wrong place — should bounce them to `https://docs.usezombie.com`. Two clicks lost on a high-frequency confusion.
2. **Glossary duplication.** README glossary re-defines Zombie / NullClaw / Steer / Webhook trigger / Trigger panel / Free-trial pricing / Cron trigger / Stage / Tool bridge / Self-managed provider keys / Bastion. Most are also defined inline where they are used. Drift risk: over time the README glossary goes stale because the canonical definition lives elsewhere.
3. **README one-paragraph summary duplicates `high_level.md` §1.** Same risk.
4. **`high_level.md` §3.2 (Why not OpenClaw — initial pass) and §6/§7 (Open Product Question + full Why-not-OpenClaw treatment)** make the same argument twice. §6 is a one-paragraph version of §7's full exploration. Merge.
5. **Model-cap origin story documented 3×.** `user_flow.md` §8.7 (with ASCII diagram), `billing_and_provider_keys.md` §9–10 (provider routing + endpoint shape), `capabilities.md` §4 (defaults). Each lens is legitimate, but a contributor reading top-down sees the same story repeated. Pick one canonical home; trim the other two to lens-only ("origin lives in X §Y; this section covers the Z lens").
6. **`office_hours_v2.md` and `plan_engg_review_v2.md` lack a status header.** README labels them "session notes" / "review pass" externally, but inside the files there is no `> Status: Historical — preserved as design record; not enforceable canon` banner. New contributors read them as current authority.
7. **`ship_reflection.md` is still 41 lines of PENDING SHIP placeholders** while M68 done and most of v2 shipped. Either fill from evidence or move to an `_unshipped/` subfolder so the architecture set does not look incomplete.
8. **`_v2` suffix inconsistency.** Only `office_hours_v2.md` and `plan_engg_review_v2.md` carry the suffix; everything else does not. The directory itself is implicitly v2. Drop the suffix (lighter touch) and update inbound links.

**Solution summary:** One PR against `~/Projects/usezombie/` on a worktree-isolated branch. Each fix is a small targeted edit with grep-verifiable acceptance. No prose is rewritten where it is correct; reorganisation only. Cross-references are checked end-to-end; any link this PR breaks is fixed in the same diff. No changes to `docs/v2/`, code, or external user-facing surfaces.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/architecture/README.md` | EDIT | Add installer-signpost banner. Replace glossary entries with one-line pointers to canonical inline definitions. Trim the one-paragraph product summary or replace with `> see high_level.md §1`. |
| `docs/architecture/high_level.md` | EDIT | Merge §3.2 and §6 into §7 (or restructure so the OpenClaw argument lives in one place). Update internal anchors. |
| `docs/architecture/user_flow.md` | EDIT | §8.7 remains the canonical home for the model-cap origin story (it owns the most reader-context). Add a one-line note saying so. |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | §9–10 trim to "model-caps endpoint shape + provider routing only — origin in `user_flow.md` §8.7." Keep the endpoint payload reference. |
| `docs/architecture/capabilities.md` | EDIT | §4 defaults table stays; the origin-story narrative trims to "where these defaults come from: `user_flow.md` §8.7." |
| `docs/architecture/office_hours_v2.md` | EDIT (rename) | Add status header. Rename to `office_hours.md`. |
| `docs/architecture/plan_engg_review_v2.md` | EDIT (rename) | Add status header. Rename to `plan_engg_review.md`. |
| `docs/architecture/ship_reflection.md` | EDIT | Either fill from real evidence (preferred — v2 has effectively shipped) or move to `docs/architecture/_unshipped/ship_reflection.md` with a README note. Implementing agent decides after checking ship state. |
| `docs/architecture/_unshipped/` | CREATE (conditional) | Only if `ship_reflection.md` cannot be filled. |
| `docs/v2/pending/M72_002_*.md` | CREATE | This spec. |

Inbound link audit (separate grep pass): every `office_hours_v2.md` / `plan_engg_review_v2.md` reference under `docs/`, `~/Projects/docs/`, and the spec history gets rewritten in the same diff.

---

## Sections (implementation slices)

### §1 — Installer signpost

Add a `> **If you are trying to USE usezombie**, this directory is not for you — go to [docs.usezombie.com](https://docs.usezombie.com). This directory documents the architecture for contributors to the runtime.` banner immediately after the README title. One block, no further structure changes.

### §2 — Glossary deduplication

Walk the README glossary entries. For each: confirm the canonical inline definition exists elsewhere (almost all do), then replace the glossary body with a one-line pointer: `**Zombie** — see [`high_level.md` §1](./high_level.md#what-we-are)` (or similar). Definitions that have no canonical home stay in the glossary as the canonical home.

**Implementation default:** Pointers, not deletion. The glossary table stays as the lookup surface; the *body* of each entry collapses to a link. Saves the contributor a search.

### §3 — README one-paragraph summary collapse

Replace `## What we are, in one paragraph` with a one-line pointer to `high_level.md` §1. The README is not the place to maintain a parallel summary that drifts.

### §4 — `high_level.md` OpenClaw consolidation

Merge §3.2 (initial pass) and §6 (Open Product Question) into §7 (Why Not Just Use OpenClaw). The merged §7 covers: existing argument, the pass/fail test from §6, the practical implication. §3.2 trims to a one-sentence forward-pointer ("see §7"). Update any incoming anchor links from other architecture files.

### §5 — Model-cap origin story consolidation

`user_flow.md` §8.7 stays the canonical home — it has the ASCII diagram and the install-skill walkthrough context. The other two:

- `billing_and_provider_keys.md` §9–10 narrows to "model-caps endpoint payload + provider routing." Origin pointer at top: "for the install-time vs trigger-time resolution flow, see `user_flow.md` §8.7."
- `capabilities.md` §4 keeps the defaults table and the failure-mode-escalation diagram but trims the prose explaining *where* the cap comes from — replace with the same pointer.

The §-numbers do not change; only the bodies trim.

### §6 — Historical-artifact status headers

Add at the top of each of `office_hours_v2.md` and `plan_engg_review_v2.md`, immediately after the title:

```markdown
> **Status: Historical** — preserved as the v2-design record. Not enforceable canon; current canon lives in `direction.md` and the topic files in this directory. Read for persona context and decisions-not-taken.
```

Then rename both files to drop the `_v2` suffix (`office_hours.md`, `plan_engg_review.md`). Update inbound links in the same diff.

### §7 — `ship_reflection.md` decision

Check the v2 launch state. If launched (M68 done + M70 done + external installs landed), fill the placeholders from real evidence — launch date, first external install, deferred-item status. If not launched (still pre-public-release), move the file to `docs/architecture/_unshipped/ship_reflection.md` and add a one-line README note. The placeholder skeleton does not stay in the canonical set indefinitely.

**Implementation default:** Fill from evidence. M68 ships free-trial + trigger DX; M70 ships infra audit. Material exists. If the implementer judges the launch hasn't happened in the relevant sense, the move path is the fallback.

---

## Interfaces

DOCS-only spec — no programmatic interfaces touched. The interface this spec exposes to readers:

```
README.md
  Title
  > Installer signpost banner   (NEW — §1)
  ## Why the doc is split this way
  ## Read in this order …       (unchanged)
  | file table |                 (unchanged structurally; `office_hours_v2` / `plan_engg_review_v2` renames flow through)
  > see high_level.md §1         (REPLACED — §3)
  ## Glossary                    (entries collapsed to pointers — §2)
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Broken inbound link after `_v2` rename | A `docs/v2/` spec or a `docs/` user-doc page references `office_hours_v2.md` by old name. | `grep -rn 'office_hours_v2\|plan_engg_review_v2' .` across both repos in the same diff; update every hit. |
| Glossary pointer points at moved anchor | The canonical inline definition was on a heading the implementer also touched in §4 or §5. | Re-grep anchor targets after §4/§5 land; fix pointers in the same diff. |
| `ship_reflection.md` fill is wrong | Implementer fills evidence the team has not actually validated. | Default to the move path (`_unshipped/`) when in doubt. Filling is the cleaner outcome but only if the evidence is real and Captain-acked. |
| Glossary deletion regret | A glossary entry that turned out to have no canonical home elsewhere gets deleted instead of kept. | Confirm canonical home exists *before* trimming. Pre-grep each entry's terms; if there is no inline definition, that entry stays in the glossary as the canonical home. |
| Worktree drift | Implementer edits files outside the worktree by mistake. | CWD check (`pwd` + `git worktree list`) on every Bash invocation; refuse edits outside `~/Projects/usezombie-m72-arch-coherence/`. |

---

## Invariants

1. **Every architectural concept has exactly one canonical inline definition.** Verified by grep — for each glossary term, exactly one file outside README contains a block-level definition; everywhere else (including README) is a pointer. Enforceable by `scripts/audit-arch-glossary.sh` (if it exists; otherwise grep manually as part of CHORE(close)).
2. **No internal architecture link is broken.** Verified by `markdown-link-check` (or equivalent) across the directory after every commit on the worktree.
3. **Historical artifacts carry a Status header.** Verified by grep — every file under `architecture/` either (a) has a `> Status:` line in the first 5 lines, (b) is `README.md`, or (c) is a canonical topic file (high_level / direction / user_flow / data_flow / capabilities / billing_and_provider_keys / bastion).

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_installer_signpost_present` | `head -10 docs/architecture/README.md` contains "docs.usezombie.com" with the bounce-out language. |
| `test_glossary_pointers_only` | Every entry in the README glossary (between the `## Glossary` heading and the next `##`) is one line and contains either `see ` or a markdown link `[…](./…)`. No multi-line definitions. |
| `test_high_level_openclaw_single` | `grep -c "Why Not Just Use OpenClaw\|why-not-OpenClaw" docs/architecture/high_level.md` returns `1`, not `2`. |
| `test_model_cap_origin_single` | `grep -rln "resolved at install time from the model-caps endpoint\|resolved at install time (platform-managed posture) or at provider-set time" docs/architecture/` lists exactly one file: `user_flow.md`. The other two mention the cap but pointer to user_flow.md. |
| `test_historical_status_headers` | `head -10 docs/architecture/office_hours.md` and `head -10 docs/architecture/plan_engg_review.md` each contain `> **Status: Historical**`. |
| `test_v2_suffix_purged` | `ls docs/architecture/*_v2.md 2>/dev/null` empty. `grep -rn 'office_hours_v2\|plan_engg_review_v2' .` returns zero hits across both repos. |
| `test_ship_reflection_resolved` | Either `docs/architecture/ship_reflection.md` contains no `PENDING SHIP` markers, or the file is at `docs/architecture/_unshipped/ship_reflection.md` and README mentions the move. |
| `test_no_broken_links` | A markdown-link-check pass over the touched files completes with zero broken links. |

---

## Acceptance Criteria

- [ ] Installer signpost present at top of README. Verify: `head -10 docs/architecture/README.md | grep -q 'docs.usezombie.com'`
- [ ] Glossary entries are pointers, not definitions. Verify: `awk '/^## Glossary/{flag=1; next} /^## /{flag=0} flag' docs/architecture/README.md | grep -c '^| \*\*' | tee /dev/stderr | awk '{exit ($1 > 0) ? 0 : 1}'` plus a manual review that each row has a pointer body.
- [ ] OpenClaw argument lives in one §. Verify: `grep -cE 'why.{0,4}not.{0,4}OpenClaw|Why Not Just Use OpenClaw' docs/architecture/high_level.md` returns `1`.
- [ ] Model-cap origin canonical in user_flow.md only. Verify: `grep -l 'install-skill writes resolved-or-sentinel into frontmatter' docs/architecture/*.md` lists only `user_flow.md`.
- [ ] `_v2` suffix gone, status headers added. Verify: `ls docs/architecture/*_v2.md 2>/dev/null` empty AND `head -10 docs/architecture/office_hours.md docs/architecture/plan_engg_review.md | grep -c 'Status: Historical'` returns `2`.
- [ ] No file under `docs/architecture/` exceeds 350 lines as a result of these edits.
- [ ] `ship_reflection.md` either filled or moved — no PENDING SHIP placeholders in the canonical set.
- [ ] All inbound links resolved. Verify: `grep -rn 'office_hours_v2\|plan_engg_review_v2' . 2>/dev/null` empty across `~/Projects/usezombie/` and `~/Projects/docs/`.
- [ ] Worktree clean before PR. Verify: `git status` in `~/Projects/usezombie-m72-arch-coherence/` empty.

---

## Eval Commands (Post-Implementation Verification)

```bash
# Run inside the worktree: ~/Projects/usezombie-m72-arch-coherence/

# E1: installer signpost
head -10 docs/architecture/README.md | grep -q 'docs.usezombie.com' && echo PASS || echo FAIL

# E2: glossary pointer discipline (manual: every row body has `see` or a markdown link)
awk '/^## Glossary/{flag=1; next} /^## /{flag=0} flag' docs/architecture/README.md | grep -E '^\| \*\*' | wc -l

# E3: OpenClaw section uniqueness
grep -cE 'Why Not Just Use OpenClaw' docs/architecture/high_level.md

# E4: model-cap origin uniqueness
grep -l 'install-skill writes resolved-or-sentinel into frontmatter' docs/architecture/*.md

# E5: v2-suffix purge
ls docs/architecture/*_v2.md 2>/dev/null
grep -rn 'office_hours_v2\|plan_engg_review_v2' . 2>/dev/null

# E6: status headers on historical
head -10 docs/architecture/office_hours.md docs/architecture/plan_engg_review.md | grep -c 'Status: Historical'

# E7: ship_reflection state
grep -c 'PENDING SHIP' docs/architecture/ship_reflection.md 2>/dev/null \
  || ls docs/architecture/_unshipped/ship_reflection.md

# E8: no file over 350 lines
wc -l docs/architecture/*.md docs/architecture/scenarios/*.md | awk '$1 > 350 && $2 != "total" { print "OVER: " $2 ": " $1 " lines" }'

# E9: link-check (if markdown-link-check installed)
which markdown-link-check && find docs/architecture -name '*.md' -exec markdown-link-check {} \;
```

---

## Dead Code Sweep

Two files renamed: `office_hours_v2.md` → `office_hours.md`, `plan_engg_review_v2.md` → `plan_engg_review.md`. After rename:

| Deleted reference | Grep command | Expected |
|-------------------|--------------|----------|
| `office_hours_v2` | `grep -rn 'office_hours_v2' ~/Projects/usezombie/ ~/Projects/docs/` | 0 matches |
| `plan_engg_review_v2` | `grep -rn 'plan_engg_review_v2' ~/Projects/usezombie/ ~/Projects/docs/` | 0 matches |
| Anchors changed in §4/§5 | Audit anchors on each touched §. | All inbound pointers resolve. |

If `ship_reflection.md` moves to `_unshipped/`, do the same grep for inbound references.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | DOCS audit pass: confirms every test in §Test Specification is a runnable grep/check; surfaces missing assertions. | Audit report in PR Session Notes. |
| After audit, still before CHORE(close) | `/review` | Adversarial pass against `direction.md` constants and the README's read-in-this-order claim. Does the directory still onboard a cold contributor cleanly? | Findings dispositioned (fix / defer / reject). |
| After `gh pr create` | `/review-pr` | Re-runs review against the immutable PR diff. Catches anchor drift introduced by squash. | Comments addressed before human review. |

---

## Discovery (consult log)

- **2026-05-17 — `ship_reflection.md` deletion (Captain decision).** The §7 plan was to move the placeholder to `docs/architecture/_unshipped/ship_reflection.md` with a README explaining the convention. After review, Captain decided to **delete** both files outright rather than hold the placeholder. The post-launch reflection content can be re-authored from real evidence when v2 actually ships; carrying the skeleton (or even a `_unshipped/` convention) adds clutter without payoff. Discovery: the move path was over-engineered for what amounts to one file's worth of pre-launch placeholder. Bastion's `Next:` pointer dropped; README's `_unshipped/` row dropped. Test `test_ship_reflection_resolved` in this spec now passes via "file deleted" rather than "file moved."

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Signpost (E1) | see Eval Commands | | |
| Glossary pointers (E2) | see Eval Commands | | |
| OpenClaw single (E3) | see Eval Commands | | |
| Model-cap canonical (E4) | see Eval Commands | | |
| v2-suffix purged (E5) | see Eval Commands | | |
| Status headers (E6) | see Eval Commands | | |
| ship_reflection (E7) | see Eval Commands | | |
| 350L gate (E8) | `wc -l` | | |
| Link-check (E9) | `markdown-link-check` | | |

---

## Out of Scope

- **Rewriting any canonical topic file.** This is a coherence pass, not a content rewrite. If a topic file's prose is correct but verbose, leave it.
- **`docs/v2/` spec corpus.** This spec touches `docs/architecture/` only. The spec corpus has its own coherence story; that is a separate milestone.
- **Code-comment ↔ architecture cross-references.** Some code points at `docs/architecture/<file>#<anchor>` in comments. Auditing those is in-scope only when the anchor changes (§4, §5); a broader sweep is its own milestone.
- **External user-facing docs.** Sibling workstream M72_001 owns `~/Projects/docs/`. This spec does not edit there except for inbound-link fixes after the `_v2` rename.
- **New architecture topics.** No new files added beyond the conditional `_unshipped/` folder. Adding topics is a content milestone, not this coherence pass.
