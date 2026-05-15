# M70_001: Audit scripts default to full-codebase scope

**Prototype:** v2.0.0
**Milestone:** M70
**Workstream:** 001
**Date:** May 15, 2026
**Status:** PENDING
**Priority:** P1 — pre-commit gates that silently fail to catch invariants are worse than no gate.
**Categories:** INFRA
**Batch:** B1
**Branch:** feat/m70-audit-scripts-full-codebase (to be created)
**Depends on:** None — pure harness work; can land independently of any in-flight feature spec.
**Provenance:** LLM-drafted (claude-opus-4-7, 2026-05-15) from the M68 §10b post-mortem after `audit-ufs.sh` slipped a `cross-runtime-orphan` violation past pre-commit.

**Canonical architecture:** N/A — harness/scripts only; no architecture-doc surface.

---

## Implementing agent — read these first

1. `~/Projects/dotfiles/scripts/audit-ufs.sh` — the only script already converted to full-codebase scope (cross-runtime-orphan check). Mirror this pattern: `git ls-files <glob>` to enumerate the working tree (sees the index, so pre-commit-friendly), then `xargs grep` to extract symbols.
2. `~/Projects/dotfiles/make/harness.mk` — sets the per-script mode flags (`--diff`, `--staged`, `--all`). The `harness-verify` target is what pre-commit invokes; `harness-verify-all` is the periodic deep variant.
3. `~/Projects/dotfiles/AGENTS.md` (Action-Triggered Guards table, rows 9–18 — every gate body lives in `docs/gates/<slug>.md` and the table cites the script). Update each gate body when its script's scope changes.
4. The post-mortem in M68_001's commit `<hash to be filled>` "fix(zombiectl): rename ERR_AUTH_* JS exports …" — explains *why* the slip happened (pre-commit `HEAD` is the prior commit; staged content lives in the index, not in `BASE...HEAD`).

---

## Applicable Rules

- `~/Projects/dotfiles/AGENTS.md` — Action-Triggered Guards table is the index. Every script change must keep the table row + gate body in sync (Rule Extension Protocol — same diff lands all four parts).
- `docs/greptile-learnings/RULES.md` — RULE NDC (no dead code at write time): if a script's `--diff` mode is no longer the default, decide whether `--diff` survives at all or becomes a vestigial flag worth deleting.
- `docs/gates/ufs.md` — already reflects the new full-codebase semantics for the cross-runtime-orphan check; mirror that doc shape into the other gate bodies as their scripts flip.

No `*.zig` / HTTP / SCHEMA touches expected.

---

## Overview

**Goal (testable):** Every `scripts/audit-*.sh` script's default mode scans the entire working tree (via `git ls-files`), so pre-commit catches an invariant violation regardless of whether the fix is staged-but-not-yet-committed. `--diff` and `--staged` survive only as opt-in narrowing for fast iterative loops; `harness-verify` no longer invokes them.

**Problem:** Three scripts currently default to a partial scope:

- `audit-ufs.sh` — was `--diff` (now hot-fixed for cross-runtime-orphan only; the other checks in the same script are still diff-shaped).
- `audit-design-tokens.sh` — defaults to `--diff` (`BASE...HEAD`); blind to staged-not-committed when invoked outside pre-commit.
- `audit-combined.sh` — defaults to `--staged` (`git diff --cached`); pre-commit-safe because the index *does* include staged content, but the MS-ID/PUB/UI checks are inherently diff-shaped (assert on *added* lines, not file state) so converting them is a redesign, not a flag flip.

The four `--all`-default scripts (`audit-deinit-pairs`, `audit-error-codes`, `audit-logging`, `audit-spec-template`) are already full-codebase by default — but pre-commit invokes them with `--staged`, so they share the partial-scope blindspot in pre-commit. The orphan-cleanup commit (`02c1f3cf` on `feat/m68-trigger-dx-and-free-trial`) added 9 cross-runtime mismatches that the UFS pre-commit gate did not see, because at the moment the hook ran there was nothing in `BASE...HEAD` to chew on.

**Solution summary:** Two layers — (1) every script's *default* mode walks `git ls-files` so direct invocation always scans the full codebase; (2) `harness.mk`'s `harness-verify` target stops passing `--staged`/`--diff` and instead lets each script use its full-codebase default. Pre-commit gets slower (acceptable — these are bash + grep, not compilers) and catches everything every commit. Iterative `--diff`/`--staged` modes stay as opt-in flags for hot-loop development. Each gate body in `docs/gates/<slug>.md` gets a one-paragraph "scope" note documenting the change.

---

## Files Changed (blast radius)

> All script edits are in `~/Projects/dotfiles/scripts/`; the project repo carries symlinks. Same applies to `make/harness.mk` and `docs/gates/*.md`.

| File | Action | Why |
|------|--------|-----|
| `~/Projects/dotfiles/scripts/audit-ufs.sh` | EDIT | Make string-dup-file + numeric-suspect checks full-codebase too (cross-runtime-orphan already done). Default mode → full scan. |
| `~/Projects/dotfiles/scripts/audit-design-tokens.sh` | EDIT | Switch default from `--diff` to full-codebase walk; preserve `--diff`/`--staged` as opt-in. |
| `~/Projects/dotfiles/scripts/audit-combined.sh` | EDIT | Decide per-check: PUB / MS-ID / UI raw-text checks may need to stay diff-shaped (they assert on additions). Document the decision in the script header; convert what can be converted. |
| `~/Projects/dotfiles/scripts/audit-deinit-pairs.sh` | EDIT | Already `--all` default; remove the `--staged` mode OR document why it remains. |
| `~/Projects/dotfiles/scripts/audit-error-codes.sh` | EDIT | Same as audit-deinit-pairs. |
| `~/Projects/dotfiles/scripts/audit-logging.sh` | EDIT | Same. |
| `~/Projects/dotfiles/scripts/audit-spec-template.sh` | EDIT | Same. |
| `~/Projects/dotfiles/make/harness.mk` | EDIT | Drop the `--staged`/`--diff` arguments from `harness-verify` calls; let each script default. Keep `harness-verify-all` for periodic deep audits if it adds anything beyond default. |
| `~/Projects/dotfiles/docs/gates/ufs.md` | EDIT | Document the cross-runtime-orphan + string-dup-file + numeric-suspect scope changes. |
| `~/Projects/dotfiles/docs/gates/design-token.md` | EDIT | Document the scope change. |
| `~/Projects/dotfiles/docs/gates/spec-template.md` | EDIT | Same. |
| `~/Projects/dotfiles/docs/gates/error-registry.md` | EDIT | Same. |
| `~/Projects/dotfiles/docs/gates/logging.md` | EDIT | Same. |
| `~/Projects/dotfiles/docs/gates/lifecycle.md` | EDIT | Same. |
| `~/Projects/dotfiles/AGENTS_INVARIANCE.md` | EDIT | Add a scenario question: "When pre-commit invokes an audit script, does the script see staged content?" Expected answer: yes — full-codebase scan via `git ls-files` includes the index. |

Project repos pick this up via the next `bin/sync-agents` run; no per-project file changes needed.

---

## Sections (implementation slices)

### §1 — `audit-ufs.sh` complete the conversion

The cross-runtime-orphan check is already full-codebase. Convert string-dup-file and numeric-suspect to the same shape: enumerate files via `git ls-files`, walk every source file, accumulate the per-file violation list. Drop the `--diff` default; keep it as an opt-in mode if iterative use justifies it (decide based on actual dev workflow — if no one uses it, delete the mode entirely per RULE NDC).

### §2 — `audit-design-tokens.sh` flip default

Change default from `--diff` to a new `--all`-equivalent mode. Same `git ls-files`-driven file enumeration. Iterative `--diff`/`--staged` stay as opt-in.

### §3 — `audit-combined.sh` per-check decision

This script's three sub-checks (MS-ID, PUB, UI substitution) all assert on *added* content (`^\+` lines from a unified diff). Converting to full-codebase means asserting on file *state*, which changes the rule semantics. Implementation default: leave the diff-shaped checks alone; document in the script header *why* they are diff-shaped (the rule itself is "don't introduce X", not "X must not exist anywhere"). If any sub-check has a state-shaped equivalent, add it as a sibling check rather than rewriting the diff one.

### §4 — Four `--all`-default scripts harness-mode flip

`audit-deinit-pairs`, `audit-error-codes`, `audit-logging`, `audit-spec-template` already default to `--all`. The change here is in `make/harness.mk`: `harness-verify` calls them with `--staged` today, which limits the scope to staged files. Drop the `--staged` argument so they use their own default. Keep the `--staged` mode in each script for opt-in narrowing during iterative loops.

### §5 — Gate-body documentation pass

Every `docs/gates/<slug>.md` for the affected scripts gets a one-paragraph "Scope" section explaining what the script scans (full working tree via `git ls-files`) and when the iterative modes are appropriate. Plus the rationale: pre-commit `HEAD` is the prior commit, so partial-scope checks based on `BASE...HEAD` are blind to the index.

### §6 — Invariance suite extension

Add one Scenario question to `AGENTS_INVARIANCE.md`: "When pre-commit invokes an audit script, does the script see staged content?" — expected YES. The scenario verifies the change persists; if a future edit reverts a script to diff-only default, the questionnaire surfaces it.

---

## Interfaces

```
# Script CLI surface (unchanged for opt-in modes; default mode flips to full-codebase)

scripts/audit-ufs.sh                     # default: full codebase (was --diff)
scripts/audit-ufs.sh --diff              # opt-in: BASE...HEAD scope
scripts/audit-ufs.sh --staged            # opt-in: index scope (NEW — currently absent)
scripts/audit-ufs.sh --all               # alias for default

scripts/audit-design-tokens.sh           # default: full codebase (was --diff)
scripts/audit-design-tokens.sh --diff    # opt-in
scripts/audit-design-tokens.sh --staged  # already exists; keep
scripts/audit-design-tokens.sh --all     # alias for default

# audit-combined.sh — defaults stay --staged because each sub-check is diff-shaped by design.
# Header docstring explains the per-check rationale.

# audit-deinit-pairs / audit-error-codes / audit-logging / audit-spec-template — already --all default.
# No CLI change. The harness.mk call site changes.
```

`make/harness.mk` `harness-verify` target: drop the explicit `--staged`/`--diff` argument for every script except `audit-combined.sh`. Each script uses its own default.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `harness-verify` slows past developer tolerance | Full-codebase scans take >5s for the largest scripts on the largest worktrees | Profile with `time make harness-verify`. Acceptable budget: total ≤10s. If over, parallelize the script invocations via `make -j`. |
| Script flags pre-existing violation that was never triggered before | Full-codebase scan reveals latent debt that the partial-scope check missed | Surface the violation. Either fix in this PR (if mechanical) or land a follow-up chore commit before merging this spec. The orphan-cleanup commit precedent (M68 `02c1f3cf`) is the model. |
| Existing CI run breaks on the wider scope | A pipeline call site that relied on the partial scope now fires on the full scope | Fix the violations the wider scope reveals; do not narrow the scope back. |
| `audit-combined.sh` sub-check accidentally converted to state-shape | Author misreads the per-check rationale | Tests in §6 invariance scenario catch the regression next time the questionnaire fires. |
| Symlink-resolution drift between project and dotfiles | A project repo has a stale `scripts/audit-*.sh` regular file instead of a symlink | `bin/sync-agents` per-project run replaces stale files with symlinks. Spec acceptance includes running it on at least one project repo and confirming. |

---

## Invariants

1. **Default mode of every `scripts/audit-*.sh` (except `audit-combined.sh`) is full-codebase.** Enforced by AGENTS_INVARIANCE.md scenario question; the questionnaire fires on every dotfiles edit per the Invariance Suite Gate.
2. **`harness-verify` calls scripts without explicit `--staged`/`--diff` arguments** (except `audit-combined.sh`). Enforced by `scripts/audit-agents-md.sh` — extend it to grep the harness target and assert no narrowing flags survive on the converted scripts.
3. **`docs/gates/<slug>.md` carries a "Scope" section** for every gate whose script scope changed. Enforced by extending `audit-agents-md.sh` to require the section heading on the listed gate bodies.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_ufs_default_full_codebase` | `audit-ufs.sh` (no args) walks `git ls-files`, not `git diff`. Verified by injecting a staged-but-uncommitted ERR_* mismatch and confirming the audit fires without an explicit `--all`. |
| `test_design_tokens_default_full_codebase` | Same shape — stage a token-violating arbitrary, run `audit-design-tokens.sh` (no args), expect violation reported. |
| `test_combined_per_check_documented` | `audit-combined.sh` header docstring contains a "Per-check scope" section explaining why MS-ID/PUB/UI stay diff-shaped. |
| `test_harness_no_narrowing_flags` | Grep `make/harness.mk` `harness-verify` target for `--staged`/`--diff` arguments on the four flipped scripts; expect zero hits. |
| `test_pre_commit_catches_staged_violation` | End-to-end: stage a cross-runtime mismatch, run `make harness-verify`, expect non-zero exit + violation listed. Mirror the M68 `02c1f3cf` slip scenario. |
| `test_iterative_modes_still_work` | `--diff` and `--staged` flags still produce narrower output for the converted scripts. Regression test against the convenience flags. |
| `test_audit_agents_md_enforces_scope_section` | `audit-agents-md.sh` rejects a `docs/gates/<slug>.md` PR that drops the "Scope" section on a converted gate. |

---

## Acceptance Criteria

- [ ] `audit-ufs.sh` (no args) reports the same violations as `audit-ufs.sh --all` — verify: `diff <(scripts/audit-ufs.sh 2>&1) <(scripts/audit-ufs.sh --all 2>&1)` returns empty.
- [ ] `audit-design-tokens.sh` (no args) reports the same violations as `--all` mode — verify: same `diff` shape.
- [ ] `make harness-verify` runs in ≤10 s on the lead repo (`time make harness-verify`).
- [ ] `make harness-verify` catches the M68 `02c1f3cf` cross-runtime mismatch at pre-commit (regression test).
- [ ] Every `docs/gates/<slug>.md` for the affected scripts has a "Scope" section.
- [ ] `AGENTS_INVARIANCE.md` carries the new scenario question; `.agents-invariance-signoff` is fresh.
- [ ] `make harness-verify` clean on the dotfiles + on at least one project repo after `bin/sync-agents`.
- [ ] No file in dotfiles or any project exceeds 350 lines as a result.
- [ ] `gitleaks detect` clean.

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: every converted script's no-arg invocation matches --all
for s in audit-ufs audit-design-tokens audit-deinit-pairs audit-error-codes audit-logging audit-spec-template; do
  diff <(scripts/$s.sh 2>&1) <(scripts/$s.sh --all 2>&1) >/dev/null \
    && echo "PASS: $s" || echo "FAIL: $s"
done

# E2: harness-verify regression — stage a known-bad cross-runtime mismatch, expect non-zero
git stash push -m "audit-test"
echo 'export const ERR_TEST_FAKE = "UZ-TEST-999";' >> zombiectl/src/constants/error-codes.js
git add zombiectl/src/constants/error-codes.js
make harness-verify; rc=$?; git restore --staged zombiectl/src/constants/error-codes.js; git checkout zombiectl/src/constants/error-codes.js; git stash pop
[ $rc -ne 0 ] && echo "PASS: pre-commit caught staged violation" || echo "FAIL: pre-commit missed staged violation"

# E3: harness budget
time make harness-verify

# E4: invariance suite
./scripts/audit-agents-md.sh

# E5: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

> Mandatory if `--diff` modes are deleted from any script.

| File / symbol | Verify | Expected |
|---------------|--------|----------|
| (script) `--diff` mode if removed | `grep -n '"--diff"' scripts/<name>.sh` | 0 matches |
| (script) `--diff` mode if removed | `grep -rn '<name>.sh --diff' make/ scripts/ docs/` | 0 matches |

If `--diff` modes survive (decision per script): write "N/A — modes preserved" with a one-line rationale per script.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does |
|------|-------|--------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits coverage of the test cases listed above. |
| After tests pass, still before CHORE(close) | `/review` | Adversarial review against this spec + AGENTS.md Action-Triggered Guards table + the other audit-script implementations to keep style consistent. |
| After `gh pr create` | `/review-pr` | Comments on PR diff once squashed. |

---

## Verification Evidence

> Filled in during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Default-mode parity | `diff <(scripts/audit-ufs.sh) <(scripts/audit-ufs.sh --all)` | | |
| Pre-commit catches staged violation | E2 above | | |
| Harness budget | `time make harness-verify` | | |
| Invariance suite | `./scripts/audit-agents-md.sh` | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- **The `audit-combined.sh` per-check redesign.** Sub-checks (MS-ID/PUB/UI) are inherently diff-shaped because the rule is "don't *introduce* X". Converting to state-shape changes the semantic and is a separate research spec.
- **Project-repo audit-script forks.** Some projects have local audit scripts not symlinked from dotfiles; cataloguing and flipping those is per-project work, not part of this spec.
- **TS / `ui/packages` cross-runtime parity beyond ERR_*.** The hot-fix scoped the cross-runtime-orphan check to `ERR_*` because it's the canonical cross-runtime contract surface. Extending to other shared symbol categories (constants, type names) is a follow-up if anyone identifies a real category that needs parity.
- **CI-side replication.** `make harness-verify` runs locally + in pre-commit. CI runs its own targets; bringing the same full-codebase semantics to CI workflows is a follow-up spec.
