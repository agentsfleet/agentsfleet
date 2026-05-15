# M70_001: Audit scripts default to full-codebase scope

**Prototype:** v2.0.0
**Milestone:** M70
**Workstream:** 001
**Date:** May 15, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — pre-commit gates that silently fail to catch invariants are worse than no gate.
**Categories:** INFRA
**Batch:** B1
**Branch:** feat/m70-audit-scripts-full-codebase
**Depends on:** None — pure harness work; can land independently of any in-flight feature spec.
**Provenance:** LLM-drafted (claude-opus-4-7, 2026-05-15) from the M68 §10b post-mortem after `audit-ufs.sh` slipped a `cross-runtime-orphan` violation past pre-commit.

**Canonical architecture:** N/A — harness/scripts only; no architecture-doc surface.

---

## Implementing agent — read these first

1. `~/Projects/dotfiles/scripts/audit-ufs.sh` — the only script already converted to full-codebase scope (cross-runtime-orphan check). Mirror this pattern: `git ls-files <glob>` to enumerate the working tree (sees the index, so pre-commit-friendly), then `xargs grep` to extract symbols.
2. `~/Projects/usezombie/make/harness.mk` — sets the per-script mode flags (`--diff`, `--staged`, `--all`). The `harness-verify` target is what pre-commit invokes; `harness-verify-all` is the periodic deep variant. **Correction to original draft:** this file lives in the project repo (usezombie), not dotfiles; dotfiles carries no `make/` directory. Each project repo owns its own harness wiring.
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
| `~/Projects/usezombie/make/harness.mk` | EDIT | Drop the `--staged`/`--diff` arguments from `harness-verify` calls; let each script default. Keep `harness-verify-all` for periodic deep audits if it adds anything beyond default. *(Lives in the project repo, not dotfiles.)* |
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

### §1 — `audit-ufs.sh` complete the conversion — DONE

Default mode = full-codebase walk via `git ls-files`. `--diff` retired (RULE NDC; zero callers post-harness.mk update); `--all` kept as back-compat alias. Single awk pass over all files for string-dup-file + numeric-suspect (was 5 forks × ~760 files); cross-runtime grep batched (`xargs -I{} grep` → `xargs grep -h`). Benchmark: 40s → 4s. Latent `while record` subshell bug preserved deliberately — fixing it surfaces ~3019 pre-existing string-dup violations (separate cleanup spec). Landed dotfiles `a55d677`.

### §2 — `audit-design-tokens.sh` flip default — DONE

Default flipped from `--diff` to `--all`-equivalent (full ui/packages walk via `git ls-files`). `--staged` preserved as opt-in narrowing for iterative dev. `--diff` rejected with exit 2 + pointer to gate body. Single grep per pattern across all scoped files (was per-pattern × per-file). Benchmark: 3.5s → 0.8s. Landed dotfiles `a55d677`.

### §3 — `audit-msid-ui.sh` per-check decision — DONE

Renamed by Captain from `audit-combined.sh` to `audit-msid-ui.sh` mid-flow (dotfiles `a2fd057` dropped the PUB clause — `zlint`'s `unused-decls: error` now owns pub-surface parity). Script stays diff-shaped (`--staged` default for pre-commit, `--diff` for periodic deep audit). Header docstring documents per-check rationale: MS-ID flags milestone identifiers added in this commit, UI flags raw HTML primitives added now; legacy raw-HTML cleaned by RULE NLR not by this audit. Landed dotfiles `a55d677` + Captain's `a2fd057`.

### §4 — Harness-mode flip — DONE

`make/harness.mk` `harness-verify` target invokes every audit with no scope flag (default = full-codebase) except `audit-msid-ui.sh --staged` (diff-shaped by construction). `harness-verify-all` similarly: full-codebase for the converted scripts, `--diff` for the diff-shaped one. **Correction to original draft:** `make/harness.mk` lives in the project repo (usezombie), not dotfiles. Each project repo owns its own harness wiring. Landed usezombie `e650cb6e`.

### §5 — Gate-body documentation pass — DONE

`docs/gates/{ufs,design-token,spec-template,error-registry,logging,lifecycle}.md` each carry a "Scope (M70)" section documenting full-codebase semantics + the M68 `02c1f3cf` forcing function (pre-commit `HEAD` is the prior commit; `BASE...HEAD` checks were blind to the index). Landed dotfiles `a55d677`.

### §6 — Invariance suite extension — DONE

`AGENTS_INVARIANCE.md` Scenario 22 added (4 questions covering full-codebase scope, `--diff` retirement, `audit-msid-ui.sh` carve-out, and gate-body Scope-section discipline). Verdict-table row 22 added. Question 4.1c text fix-up — removed stale `--diff` reference. Questionnaire all-YES against AGENTS.md HEAD = `d0f3bf6`, signoff PASS, pushed clean. Landed dotfiles `d0f3bf6` + `a55d677`.

### §7 — Perf bonus (audit-logging + audit-deinit-pairs) — DONE

Surfaced during HARNESS VERIFY budget check. audit-logging: 22s → 4.8s (single-awk Section 2; batched grep Sections 3+5; pre-computed zig/js non-test file subsets). audit-deinit-pairs: 17s → 3.2s (pre-computed `files_with_cleanup` set in one batched grep -lE; per-init body-window awk preserved; Section 3 defer/errdefer awk consolidated with per-file `flush()` on `FNR == 1`). Total `make harness-verify` sequential: 48s → 10.02s. Landed dotfiles `56e578e`. **Acceptance budget set at 15s by Captain (was ≤10s aspirational).**

### §8 — Bonus cleanup surfaced by M70 (A1 + B1) — DONE

Full-codebase scope's first run flagged two clusters on `main` that `BASE...HEAD` had been blind to. Both fixed in usezombie `7671769b`:

**A1 — cross-runtime naming drift** (9 UFS orphans). JS `ERR_*` consts whose UZ-* codes were declared in Zig under different identifiers. Captain's decision: rename JS to match Zig (server is source of truth). 7 renames, 1 delete (`ERR_GRANT_REVOKED`, 0 consumers), 1 add (`pub const ERR_BILLING_UNAVAILABLE = "UZ-BILLING-001"` in Zig — the dead-server / live-CLI case). Callers updated in `zombiectl/src/commands/{billing,core-ops,zombie}.js` + `zombiectl/test/error-codes.unit.test.js`. Also promoted `ERR_WORKSPACE_FREE_LIMIT` from private const to `pub const` for cross-runtime parity.

**B1 — raw-literal map keys** (18 ERROR REGISTRY hits). `zombiectl/src/lib/error-map-presets.js` rewritten to use computed-key `[ERR_X]:` syntax importing from `zombiectl/src/constants/error-codes.js`. Single source of truth: each `UZ-*` literal lives in one JS file (mirroring Zig); typos fail at import time. Captain's framing ("since i need a single source of truth") drove the B1 over B2 choice.

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
