# M69_001: Skills move to `github.com/usezombie/skills`, installed via `npx skills add usezombie/skills`

**Prototype:** v2.0.0
**Milestone:** M69
**Workstream:** 001
**Date:** May 14, 2026
**Status:** PENDING
**Priority:** P1 — unblocks independent skill iteration without main-repo PR ceremony; reverses an M49 decision that has aged out.
**Categories:** SKILL, INFRA, DOCS
**Batch:** B1 — independent of M68 and other M69 workstreams.
**Branch:** {feat/m69-public-skills-repo — added when work begins}
**Depends on:** M49_001 (reverses its "no separate repo" decision; M49 stays in `done/` as historical record).
**Provenance:** LLM-drafted (claude-opus-4-7, 2026-05-14) from Captain Q&A scoping session 2026-05-14.

**Canonical architecture:** `docs/architecture/user_flow.md` §8 (skill install path) — install topology shifts from "in-repo + npm bundle" to "separate repo + `npx skills add`".

---

## Implementing agent — read these first

1. `docs/v2/done/M49_001_P1_SKILL_DOCS_INSTALL_SKILL.md` — the spec this one amends. Read its "Rejected alternatives" section: "Two-repo distribution (rejected)" and "No new GitHub repos are created" are both being undone here. M49 stays in `done/`; its rejected-alternative reasoning is captured here.
2. `skills/usezombie-install-platform-ops/SKILL.md` — the skill body being relocated. Includes the existing 12-step install flow plus M68's in-flight §10c update-in-place branch.
3. `zombiectl/package.json` — the `files:` array that today ships `skills/` inside the npm package. M69_001 removes `skills/` from that array.
4. <https://github.com/resend/resend-cli#agent-skills> — the Resend pattern this install path mirrors. `npx skills add <owner>/<repo>` clones the GitHub repo and symlinks every top-level `<skill-name>/` dir into the host's skill dirs (`~/.claude/skills/`, `~/.codex/skills/`, `~/.amp/skills/`, `~/.opencode/skills/`).
5. `tests/skill-evals/usezombie-install-platform-ops/` — the eval suite that moves with the skill.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline; RULE NLR (touch-it-fix-it on docs that reference the old install path) and RULE ORP (orphan sweep after deletion).
- **`docs/BUN_RULES.md`** — `zombiectl/package.json` edits.
- **`docs/EXECUTE_DOC_READS.md`** — `skills/**` and `zombiectl/package.json` edits trigger doc-read rows.
- Standard set otherwise.

---

## Overview

**Goal (testable):** From a clean `npm install -g @usezombie/zombiectl` followed by `npx skills add usezombie/skills`, the `usezombie-install-platform-ops` skill is reachable as `/usezombie-install-platform-ops` in Claude Code (and the same in Codex/Amp/OpenCode), with `~/.claude/skills/usezombie-install-platform-ops/` resolving to a symlink into `~/.npm/_npx/.../node_modules/@usezombie/skills/` (or wherever `npx skills add` lands the clone). Old install path `npx skills add usezombie/usezombie` returns "skill not found" since the npm package no longer carries `skills/`.

**Problem:** Today, every skill body edit requires a PR against the main `usezombie/usezombie` monorepo, runs the full lint + test gauntlet, and waits on `kishore-babysit-prs` greptile cycles before reaching users. Skills are content, not service code — the ceremony is mismatched. M49 chose "skills live in the npm package" for simplicity; in practice this couples skill iteration cadence to server-release cadence.

**Solution summary:** New public repo `github.com/usezombie/skills` becomes the canonical home for every `usezombie-*` skill body and its eval suite. The npm package `@usezombie/zombiectl` drops `skills/` from its `files:` array. Install becomes a two-command flow (already documented as such): `npm install -g @usezombie/zombiectl` for the CLI, `npx skills add usezombie/skills` for the agent skills. Skill iteration ships independently — push to `main` of the skills repo = ship. No tags, no semver, same model as `dotfiles`.

---

## Files Changed (blast radius)

### In `github.com/usezombie/usezombie` (this repo)

| File | Action | Why |
|------|--------|-----|
| `skills/usezombie-install-platform-ops/**` | DELETE | Moves to new repo. |
| `skills/README.md` | DELETE | Moves to new repo as the public README. |
| `tests/skill-evals/usezombie-install-platform-ops/**` | DELETE | Co-locates with the skill in the new repo. |
| `zombiectl/package.json` | EDIT | Drop `skills` from `files:` array. |
| `docs/v2/done/M49_001_*.md` | NO EDIT | Stays as-is; historical record. This spec's "Solution summary" explains the amendment. |
| `docs/architecture/user_flow.md` | EDIT | Update §8 to document **both install paths by audience**: humans → `https://usezombie.sh` (current copy-paste flow); agents → `npm install -g @usezombie/zombiectl && npx skills add usezombie/skills`. Architecture doc currently names one path; reality post-M69_001 is two. RULE NLR applies — the canonical doc is touched, not just `plan_engg_review_v2.md`. |
| `docs/architecture/plan_engg_review_v2.md` | EDIT | Line 35 reference: `usezombie/usezombie` → `usezombie/skills`. |
| `docs/quickstart` references (cross-repo `~/Projects/docs/`) | EDIT | `quickstart.mdx`, `cli/install.mdx` flip to `npx skills add usezombie/skills`. |
| `samples/platform-ops/**` | NO EDIT | Zombie templates stay here; the npm package still bundles `samples/`. |

### In `github.com/usezombie/skills` (new repo)

| File | Action | Why |
|------|--------|-----|
| `README.md` | CREATE | Top-level explainer: what skills exist, install command, symlink-fallback. |
| `usezombie-install-platform-ops/SKILL.md` | CREATE | Migrated from this repo verbatim (post-M68 §10c version). |
| `usezombie-install-platform-ops/evals/**` | CREATE | Migrated from `tests/skill-evals/` here. Test runner: **Bun** (`bun test`) — matches lead repo's `zombiectl/` and `ui/packages/` runners. No vitest, no node:test. |
| `LICENSE` | CREATE | Match this repo's license. |
| `.github/workflows/eval.yml` | CREATE | CI runs `npm test` in `evals/` on PRs to main. |

---

## Sections (implementation slices)

### §1 — New repo bootstrap

Create `github.com/usezombie/skills` (public). Add minimal `README.md` pointing at install command + per-skill subdirs.

**License:** MIT — locked. Lead repo's root `LICENSE` is MIT (`Copyright (c) 2026 usezombie`). Copy verbatim, swap the copyright year if the move happens in a new calendar year.

**Implementation default:** repo visibility is public from day 1 because `npx skills add` resolves the public GitHub URL.

### §2 — Skill + eval migration

Move `skills/usezombie-install-platform-ops/` and `tests/skill-evals/usezombie-install-platform-ops/` to the new repo as `usezombie-install-platform-ops/` and `usezombie-install-platform-ops/evals/`. Verbatim move — no body edits in this section. Eval `package.json` adjusts paths but the test bodies are byte-identical.

### §3 — Main-repo deletion

Delete `skills/` and `tests/skill-evals/usezombie-install-platform-ops/` from this repo. Orphan-sweep every reference: every `npx skills add usezombie/usezombie` flips to `npx skills add usezombie/skills`. Every code path that read `skills/usezombie-install-platform-ops/` is gone (there shouldn't be any — confirmed via pre-spec grep).

### §4 — npm package surgery

`zombiectl/package.json` `files:` array drops `skills`. `npm pack` post-edit shows no `skills/` in the tarball. Other entries (`samples/`, `dist/`, etc.) unchanged.

**Postinstall audit:** `zombiectl/scripts/postinstall.mjs` exists and **already operates on `samples/` only** — verified at spec time, no `skills/` references in the body. The script copies the bundled `samples/` tree to `~/.config/usezombie/samples/` (sha256-manifested, idempotent). **No edit needed.** Implementing agent: confirm by reading the file once more before §4; if it grew a skills reference between spec and execute, treat as a separate edit. Concrete check: `grep -n "skills" zombiectl/scripts/postinstall.mjs` → expect 0 matches.

### §5 — Cross-repo doc sweep

> ⚠️ **PAUSE before editing `~/Projects/docs/`.** Per CLAUDE.md Operational defaults, cross-repo writes need explicit per-session approval from the Captain naming the files. Lead-repo edits in §3 + this repo's `docs/architecture/user_flow.md` (per row 6 of Files Changed) can proceed in parallel without docs-repo writes. Request approval before touching anything under `~/Projects/docs/`.

Every reference in the lead repo + `~/Projects/docs/` repo flips `usezombie/usezombie` → `usezombie/skills`. Captain's "cross-repo writes need explicit per-session ask" rule applies — this section enumerates the docs-repo edits so the implementing agent has the scope locked.

### §6 — Smoke test

From a clean macOS account (or a fresh container), run the two-command install and reach `/usezombie-install-platform-ops` in Claude Code. The evidence belongs in PR Session Notes.

---

## Interfaces

**Install command (locked):**

```
npm install -g @usezombie/zombiectl
npx skills add usezombie/skills
```

**Skill discovery path (locked):**

```
~/.claude/skills/usezombie-install-platform-ops/   → symlink into npx skills clone dir
~/.codex/skills/usezombie-install-platform-ops/    → same
~/.amp/skills/usezombie-install-platform-ops/      → same
~/.opencode/skills/usezombie-install-platform-ops/ → same
```

**Skill repo top-level layout (locked):**

```
github.com/usezombie/skills/
├── README.md
├── LICENSE
├── .github/workflows/eval.yml
└── usezombie-install-platform-ops/
    ├── SKILL.md
    ├── TRIGGER.md  (if present in source)
    └── evals/
        ├── package.json
        └── *.eval.ts
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `npx skills add usezombie/usezombie` lingers post-merge | User's local docs / shell history / muscle memory | Old command returns "skill not found"; docs sweep ensures every authoritative reference uses the new path. |
| Symlink target stale after `npm install -g` upgrade | npx clone dir moves between runs | `npx skills add` re-runs idempotently; users hitting the issue re-run the command per docs. |
| Skill repo accidentally goes private | Repo owner toggle | `npx skills add` fails with auth error; surfaced to Captain via babysit-prs equivalent on the skills repo. Recovery is a single setting flip. |
| Eval CI fails on a skill PR | Bad skill body | Eval workflow blocks merge to skills repo `main`; same protection model as this repo's CI. |
| `@usezombie/zombiectl` postinstall still tries to find `skills/` | Stale postinstall logic | Section §4 audits every postinstall reference to `skills/`. If a postinstall step exists, it gets removed or rewritten. |

---

## Invariants

1. After §3, `find skills/ -type f` returns empty in this repo — enforced by orphan-sweep CI grep on `skills/` path.
2. After §4, `npm pack --dry-run` in `zombiectl/` does not list any `skills/` file — enforced by the `make` smoke target or a new check.
3. Skill-repo `main` is the only release surface — no tags, no semver. Enforced by convention + docs.
4. Every `npx skills add usezombie/usezombie` reference in the lead repo + `~/Projects/docs/` is dead post-§5 — enforced by `grep -rn "usezombie/usezombie" docs/ ui/ src/ zombiectl/` returning empty.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_npm_pack_excludes_skills` | After §4, `npm pack --dry-run` output in `zombiectl/` contains no path matching `skills/*`. |
| `test_lead_repo_no_skills_dir` | After §3, `test ! -d skills` passes. |
| `test_no_old_install_command_refs` | `grep -rn "npx skills add usezombie/usezombie" docs/ ui/ zombiectl/` returns 0 hits. |
| `test_eval_suite_present_in_new_repo` | (Cross-repo) New repo's `usezombie-install-platform-ops/evals/package.json` exists and `npm test` passes inside it. |
| `test_skill_body_byte_identical_post_move` | (Pre-merge sanity) SHA256 of pre-move `skills/usezombie-install-platform-ops/SKILL.md` matches SHA256 of new repo's `usezombie-install-platform-ops/SKILL.md`. |
| `test_skill_symlinks_in_all_four_host_dirs` | (Smoke, PR Session Notes — not CI) After `npx skills add usezombie/skills` on a clean macOS account, all four host dirs resolve: `~/.claude/skills/usezombie-install-platform-ops/`, `~/.codex/skills/usezombie-install-platform-ops/`, `~/.amp/skills/usezombie-install-platform-ops/`, `~/.opencode/skills/usezombie-install-platform-ops/`. Each is a symlink (or copy) into the npx clone dir; SKILL.md reachable through each. |

---

## Acceptance Criteria

- [ ] New repo `github.com/usezombie/skills` exists, public, with LICENSE + README + the one migrated skill — verify: `gh repo view usezombie/skills --json visibility,defaultBranchRef`.
- [ ] `skills/` and `tests/skill-evals/usezombie-install-platform-ops/` deleted from this repo — verify: `test ! -d skills && test ! -d tests/skill-evals/usezombie-install-platform-ops`.
- [ ] `zombiectl/package.json` `files:` array does not contain `skills` — verify: `cd zombiectl && jq -r '.files | contains(["skills"]) | not' package.json` returns `true`.
- [ ] Smoke install on a clean env reaches `/usezombie-install-platform-ops` — verify: paste evidence in PR Session Notes.
- [ ] `make lint` clean.
- [ ] `make test` passes.
- [ ] `gitleaks detect` clean on both repos.
- [ ] No file over 350 lines added.

---

## Eval Commands

```bash
# E1: skills/ directory gone from lead repo
test ! -d skills && echo "PASS: skills/ removed" || echo "FAIL"

# E2: skill-evals dir gone from lead repo
test ! -d tests/skill-evals/usezombie-install-platform-ops && echo "PASS" || echo "FAIL"

# E3: package.json files: array no longer mentions skills
cd zombiectl && jq -e '.files | contains(["skills"]) | not' package.json && echo "PASS" || echo "FAIL"

# E4: no lingering references to old install path
! grep -rn "npx skills add usezombie/usezombie" docs/ ui/ src/ zombiectl/ && echo "PASS: no stale refs" || echo "FAIL"

# E5: new repo reachable
gh repo view usezombie/skills --json visibility | jq -e '.visibility == "PUBLIC"' && echo "PASS" || echo "FAIL"

# E6: lint
make lint 2>&1 | tail -3

# E7: 350L gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 }'
```

---

## Dead Code Sweep

**Orphaned files:**

| File to delete | Verify deleted |
|----------------|----------------|
| `skills/usezombie-install-platform-ops/SKILL.md` | `test ! -f skills/usezombie-install-platform-ops/SKILL.md` |
| `skills/README.md` | `test ! -f skills/README.md` |
| `tests/skill-evals/usezombie-install-platform-ops/**` | `test ! -d tests/skill-evals/usezombie-install-platform-ops` |

**Orphaned references:**

| Deleted symbol | Grep | Expected |
|---|---|---|
| `npx skills add usezombie/usezombie` | `grep -rn "usezombie/usezombie" docs/ ui/ src/ zombiectl/` | 0 matches |
| `skills/` path import or reference | `grep -rn "skills/usezombie-install-platform-ops" src/ zombiectl/` | 0 matches |

---

## Skill-Driven Review Chain

| When | Skill | Required output |
|------|-------|-----------------|
| Pre-CHORE(close) | `/write-unit-test` | Smoke-install evidence + the test specification's grep checks all green. |
| Pre-CHORE(close) | `/review` | Adversarial diff review against M49's amendment + cross-repo coordination. |
| Post `gh pr create` | `/review-pr` | Greptile pass on both repos (lead + new skills repo). |

---

## Verification Evidence

(Filled during VERIFY.)

---

## Out of Scope

- Third-party skill publishing flow (different goal — community authorship). Future M70+ if needed.
- Skill marketplace UI on `usezombie.com` (browse + discovery surface). Premature with one first-party skill; revisit after >5 skills.
- Automated mirror from lead repo → skills repo (drift surface; killed by §3 deletion).
- Versioning / tagging on the skills repo (Captain explicit: `main` push = ship).
