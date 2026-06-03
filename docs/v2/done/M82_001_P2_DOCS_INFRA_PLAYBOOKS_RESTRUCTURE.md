# M82_001: Restructure playbooks into founding/ + operations/, fix stale references, add check-playbooks gate

**Prototype:** v2.0.0
**Milestone:** M82
**Workstream:** 001
**Date:** Jun 02, 2026
**Status:** DONE
**Priority:** P2 — operator/agent-facing runbooks; wrong paths break the founding flow but no customer surface.
**Categories:** DOCS, INFRA
**Batch:** B1 — standalone; no concurrent workstream.
**Branch:** feat/m82-001-playbooks-restructure
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, CTO review of `playbooks/` requested by Indy, Jun 02, 2026).

> **Provenance is load-bearing.** Agent-generated from a live audit; every claim below was grep-verified against the tree, not assumed. Cross-check the blast-radius table against `git grep` before EXECUTE.

**Canonical architecture:** `playbooks/README.md` (Playbooks-vs-Gates model) + `playbooks/ARCHITECTURE.md` (tunnel-first rationale). This workstream makes the README an accurate index of the tree and keeps it that way by code.

---

## Implementing agent — read these first

1. `playbooks/README.md` — the Playbooks-vs-Gates contract and gate-script convention this restructure must preserve verbatim (only the inventory/tree changes, not the philosophy).
2. `make/quality.mk` §`check-gh-actions-valid` (line ~238) — the exact sibling pattern `check-playbooks` mirrors: a `check-*` validator that shellchecks a dir, checks make-target refs, and wires into `lint-all`.
3. `playbooks/lib/common.sh` §`playbooks_require_vault_read_approval` — the `ALLOW_VAULT_READS` guard whose README convention §5 narrows (doc-only) to interactive gates.
4. CTO review findings (this turn's conversation) — the phantom `playbooks/gates/m{2,4,7}_00N/run.sh` refs and `001_gate.sh`→`00_gate.sh` corrections.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** restructure playbooks into founding/ + operations/, fix stale refs, add check-playbooks gate
- **Intent (one sentence):** A stranger cloning the repo can read `playbooks/` top-to-bottom and tell the sequential founding path from on-demand ops runbooks, and broken references can never silently reappear.
- **Handshake (agent fills at PLAN, before EXECUTE):** Restructure 15 flat numbered dirs into `founding/01..07` (sequential, gated) + `operations/<name>` (on-demand, categorical, teardowns under `operations/teardown/`); re-point every live reference; rename `ARCHITECHTURE.md`→`ARCHITECTURE.md`; fix phantom gate paths; narrow the `ALLOW_VAULT_READS` doc convention; add `check-playbooks` to enforce reference integrity + README/tree parity. `ASSUMPTIONS I'M MAKING:` (1) `docs/v2/done/*` and `CHANGELOG.md` are frozen historical records — left untouched even though they cite old paths [Indy: "1,2,3 take the call"]; (2) runner bootstrap belongs in `founding/` [Indy approved]; (3) `.github/workflows/**` edits are in-scope [Indy: "you are cleared to edit .github/workflows"]; (4) `git mv` preserves history for every moved dir; (5) `config.template` portability is OUT of scope [Indy-acked deferral, see Discovery].

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (always applies); NDC (no dead refs left at write time), NLR (touch-it-fix-it — fix stale `006_worker_*` drift in any file we already edit, but NOT frozen done/ specs).
- **No language rule files apply** — no `*.zig` logic, schema, or REST surface changes. The only `.zig`/`.ts` edits are path-string updates inside existing comments/fixtures (no behavior change).
- Shell discipline for the new `check-playbooks` body: match `make/quality.mk` `_shell_lint` conventions (`set -euo pipefail`, shellcheck-clean).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | `.zig` edits are path strings in comments only; no logic, no build-graph change. Cross-compile still run as a smoke check. |
| File & Function Length (≤350) | no | README grows but `.md` is exempt; `make/quality.mk` net-add for `check-playbooks` stays under cap (extract helper if it approaches). |
| UFS (repeated literals) | yes | The `playbooks/` root path and `founding`/`operations` segment names become named vars in the `check-playbooks` shell body, not repeated literals. |
| LOGGING | no | No application log emits changed. |
| MILESTONE-ID GATE | yes | Spec lives under `docs/` (exempt); no `M82_001` string lands in any moved script/playbook body. |
| Invariance Suite Gate | no | No edit to `AGENTS.md`/`docs/gates/`/audit fixtures. (Dotfiles `AGENTS.md` path-string update is a separate dotfiles commit, not this repo.) |

---

## Overview

**Goal (testable):** `make check-playbooks` exits 0 on a correct tree and exits non-zero when (a) any `playbooks/<path>` reference in the repo fails to resolve, (b) the README tree and the on-disk dir set diverge, or (c) any `playbooks/**/*.sh` trips shellcheck.

**Problem:** The flat `001`–`015` numbering implies a single sequence that does not exist — `011`/`015` are destructive teardowns, `013` is CI tooling, `012`/`014` are post-deploy setup, all masquerading as founding steps after `010`. The README indexes only `001`–`010`, references a non-existent `architecture/` directory, and several playbooks tell the operator to run gate scripts (`playbooks/gates/m2_001/run.sh`, `002_preflight/001_gate.sh`) that do not exist. Nothing prevents this drift from recurring.

**Solution summary:** Split the tree into `founding/` (the real sequential, gated spine: `01_bootstrap`…`07_runner_bootstrap_prod`) and `operations/` (on-demand runbooks, named not numbered, teardowns isolated under `operations/teardown/`). Re-point every live reference across CI, source, ops scripts, docs, and dotfiles. Rename the misspelled `ARCHITECHTURE.md`. Fix the phantom gate-path prose to point at the real `00_gate.sh`. Narrow the `ALLOW_VAULT_READS` README convention to match reality (interactive gates only). Add a `check-playbooks` make target so reference integrity and README/tree parity are enforced by code, wired into `lint-all`.

---

## Prior-Art / Reference Implementations

- **Make target** → `make/quality.mk` §`check-gh-actions-valid` — same shape: validate a directory (actionlint + run-shellcheck + make-target-ref check), `.PHONY`, listed in `lint-all`, documented in the `help` block. `check-playbooks` mirrors it: shellcheck + reference-integrity + README/tree parity over `playbooks/`. Divergence: it adds a tree-vs-doc parity check (no analog in the actions gate).
- **Gate-script convention** → `playbooks/README.md` "Gate Script Convention" — preserved verbatim; only directory paths move.
- No new architecture; the founding/operations taxonomy is defined in this spec and reflected into `playbooks/README.md`.

---

## Files Changed (blast radius)

> Verified via `git grep -lE 'playbooks/[0-9]{3}_'` (34 files) + the `git mv` set. Done-specs and CHANGELOG are intentionally EXCLUDED (frozen — see Discovery).

| File | Action | Why |
|------|--------|-----|
| `playbooks/001_bootstrap/` … `007_runner_bootstrap_prod/` | MOVE → `playbooks/founding/0N_*` | `git mv` the sequential spine, history-preserving |
| `playbooks/008` … `010`, `012`, `013`, `014` | MOVE → `playbooks/operations/<name>` | on-demand runbooks, renamed categorical |
| `playbooks/011_database_teardown/`, `playbooks/015_redis_teardown/` | MOVE → `playbooks/operations/teardown/{database,redis}` | destructive ops isolated; `015` is currently untracked — `git add` then place |
| `playbooks/ARCHITECHTURE.md` | RENAME → `playbooks/ARCHITECTURE.md` | fix misspelling; update 2 README links |
| `playbooks/README.md` | EDIT | tree → `founding/`+`operations/`; fix `architecture/`→`ARCHITECTURE.md`; narrow `ALLOW_VAULT_READS` §; add restructure-date note |
| `playbooks/founding/0{1,4,5}/001_playbook.md` | EDIT | `002_preflight/001_gate.sh` → `founding/02_preflight/00_gate.sh` (real file) |
| `playbooks/founding/0{2,3,6}/001_playbook.md` + `008/001_playbook.md` | EDIT | phantom `playbooks/gates/m{2,4,7}_00N/run.sh` → real `00_gate.sh`; sibling path refs |
| `make/quality.mk` | EDIT | add `check-playbooks` target + `.PHONY` + `lint-all` dep + `help` line |
| `.github/workflows/deploy-dev.yml` | EDIT | `002_preflight`→`founding/02_preflight`; `006_runner_bootstrap_dev`→`founding/06_…` (Indy-cleared) |
| `.github/workflows/release.yml` | EDIT | `002_preflight`→`founding/02_preflight` (Indy-cleared) |
| `deploy/baremetal/deploy.sh` | EDIT | `006_runner_bootstrap_dev`→`founding/06_…` |
| `make/test-integration.mk` | EDIT | `011_database_teardown`→`operations/teardown/database` |
| `src/zombied/http/handlers/admin/platform_keys.zig`, `…/api_keys/tenant.zig` | EDIT | comment path `012_…`→`operations/admin_bootstrap` (string only) |
| `zombiectl/test/acceptance/fixtures/constants.ts` | EDIT | `008_…`→`operations/credential_rotation`, `004_…`→`founding/04_deploy_dev` |
| `README.md`, `docs/AUTH.md`, `docs/AUTH_DEVICE_LOGIN.md`, `docs/architecture/billing_and_provider_keys.md`, `docs/architecture/scenarios/03_balance_gate.md`, `ui/usezombie.sh/README.md` | EDIT | active-doc path refs → new layout |
| `~/Projects/dotfiles/AGENTS.md` + global `CLAUDE.md` | EDIT (separate dotfiles commit) | priming-gate rule paths `001/002/003`→`founding/0{1,2,3}`; committed+pushed to dotfiles, NOT this PR |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, sectioned by concern (move → re-point → docs → gate). The move and the re-point must land in the same commit or CI breaks between them; `check-playbooks` is the durable guard that makes the one-time fix permanent.
- **Alternatives considered:** (a) *keep flat dirs, add a manifest declaring sequential-vs-ondemand* — rejected: Indy explicitly wants the folder breakup; a manifest doesn't stop the visual lie. (b) *fold `config.template` portability in* — rejected/deferred: that's a separate de-milestoning effort with its own blast radius (Indy-acked, Discovery).
- **Patch-vs-refactor verdict:** **refactor** — the directory taxonomy is the problem; a patch (fix only the README) leaves the misleading numbering. Bounded because it's path-mechanical, not behavioral.

---

## Sections (implementation slices)

### §1 — Founding/operations directory split (history-preserving) — ✅ DONE
`git mv` the 15 dirs into the two-tier layout. Founding keeps two-digit ordinals where order is real; operations are named; teardowns nest under `operations/teardown/`. `015_redis_teardown` is untracked — `git add` it as part of the move so it stops living only in a local tree.
- **Dimension 1.1** — every founding dir lands at `playbooks/founding/0N_<name>` with git history intact → Test `test_founding_layout_present`
- **Dimension 1.2** — every operations runbook lands at `playbooks/operations/<name>` (teardowns under `teardown/`) → Test `test_operations_layout_present`
- **Dimension 1.3** — no `playbooks/NNN_*` numeric-prefixed dir remains at the root → Test `test_no_legacy_numbered_dirs`

### §2 — Reference re-pointing across the live blast radius — ✅ DONE
Update every reference in CI, source, ops scripts, Makefile, active docs, and internal playbook cross-links to the new paths. Frozen records (done-specs, CHANGELOG) excluded by design.
- **Dimension 2.1** — every `playbooks/<path>` reference in non-frozen files resolves on disk → Test `test_all_refs_resolve` (this is the core `check-playbooks` assertion)
- **Dimension 2.2** — CI workflows reference the moved gate/script paths → Test `test_ci_paths_resolve`

### §3 — README + ARCHITECTURE accuracy — ✅ DONE
Rename `ARCHITECHTURE.md`→`ARCHITECTURE.md`; rewrite the README tree to the new layout; fix the `architecture/` directory reference to the real file; narrow the `ALLOW_VAULT_READS` convention to interactive gates (call #5, doc-only — adding the guard to CI gates would break unattended runs); fix phantom `playbooks/gates/...` and `001_gate.sh` prose to the real `00_gate.sh`.
- **Dimension 3.1** — README tree exactly matches the on-disk founding+operations dir set → Test `test_readme_matches_tree`
- **Dimension 3.2** — no playbook prose references a non-existent gate path → Test `test_no_phantom_gate_refs`
- **Dimension 3.3** — `ALLOW_VAULT_READS` README convention scoped to interactive gates; CI/destructive gates documented as exempt → Test `test_vault_read_convention_narrowed` (grep assertion)

### §4 — `check-playbooks` make target — ✅ DONE
New `check-*` validator in `make/quality.mk` (sibling to `check-gh-actions-valid`): shellcheck `playbooks/**/*.sh`; assert reference integrity (§2.1); assert README/tree parity (§3.1). Wire into `lint-all`; add a `help` line under "Quality Gates".
- **Dimension 4.1** — `make check-playbooks` exits 0 on the corrected tree → Test `test_check_playbooks_passes_clean`
- **Dimension 4.2** — it exits non-zero on a planted broken ref, a tree/README mismatch, and a shellcheck violation → Test `test_check_playbooks_catches_each_failure`
- **Dimension 4.3** — `lint-all` invokes `check-playbooks` → Test `test_lint_all_includes_check_playbooks`

---

## Scope Addendum — folded-in workstreams (Indy-directed)

Two workstreams outside the original playbooks restructure were folded into this PR by explicit Indy direction (ack-quotes in Discovery). Each is independently verified; neither touches CI-workflow files.

### §5 — Hermetic zombiectl test-infra fix — ✅ DONE
The pre-push hook's `make test-unit-zombiectl` flaked on the CLI's PostHog telemetry flush (network-dependent, ~5s per-test timeouts) — surfaced because a zombiectl file in this branch makes the lane run. Fix (commit `2aa2eb3b`): `composeEnv` injects `ZOMBIE_TELEMETRY_DISABLED=1` into every spawned-CLI child env (children do not inherit `process.env`); a bun preload (`test/setup.ts`) defaults the runner telemetry-off for in-process `runCli` tests and raises `setDefaultTimeout` to 15s for subprocess specs under `--coverage`. Telemetry-consent tests self-manage the env var, so the default is invisible to them.
- **Dimension 5.1** — `make test-unit-zombiectl` is deterministically green offline → verified 5/5 `bun test --coverage` runs, 0 timeouts
- **Dimension 5.2** — coverage floor unaffected by telemetry-off default → enforce-coverage PASS

### §6 — zombiectl zombie/* command + workspaces coverage — ✅ DONE
Removed the Category B entries from `coveragePathIgnorePatterns` (added in M74-001, "deferred to a follow-up coverage spec") and added test files bringing 6 command files from ~0–80% to ~100% and `workspaces.ts` real logic to 100% line (commit `dd5c2bd8`). `workspaces.ts` function coverage caps at 90% — the bun `Context.Service` class-factory artifact, same limitation as the kept Context.Tag ignores, absorbed by the aggregate. Adversarial `/review` removed one coverage-gaming unit file (redundant: integration tests cover `handlers-bind-zombie.ts` 100%/100%) and strengthened weak/root-fragile assertions (commit `406e6372`).
- **Dimension 6.1** — every Category B file driven by behavioral tests (wire-request / error-path assertions), not vacuous coverage-padding → adversarial review clean
- **Dimension 6.2** — aggregate floor enforces the un-ignored files → function 96.54% ≥ 96, line 98.25% ≥ 97

---

## Interfaces

```
make check-playbooks   # exit 0 = clean; exit 1 = at least one of: broken playbooks/ ref,
                       #          README/tree drift, shellcheck failure. Prints per-check ✓/✗, fail-all.
lint-all: … check-playbooks   # umbrella dependency

playbooks/
  founding/0{1..7}_<name>/         # sequential spine
  operations/<name>/               # on-demand
  operations/teardown/{database,redis}/
```

The founding ordinals (`01`–`07`) and the operations names are the contract `check-playbooks` and the README both read; changing one without the other is the drift the gate exists to catch.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Broken reference after move | a `playbooks/<old>` path missed during re-pointing | `check-playbooks` §2.1 prints `✗ <file>:<path>` and exits 1; CI red before merge |
| README/tree drift | a future dir add/remove not mirrored in README | `check-playbooks` §3.1 prints the symmetric-difference dir set and exits 1 |
| CI path break | workflow still points at old gate path | `test_ci_paths_resolve` + the live `deploy-dev`/`release` preflight job fails fast |
| Untracked `015` lost in move | `git mv` on an untracked dir | EXECUTE `git add` the dir first; §1.2 asserts it landed tracked |
| History lost on move | `cp`+`rm` instead of `git mv` | use `git mv`; `git log --follow` on a moved file confirms history (manual VERIFY check) |
| Frozen record falsely "fixed" | editing a `done/` spec or CHANGELOG | excluded from Files-Changed scope; editing them is an out-of-scope violation |

---

## Invariants

1. Every `playbooks/<path>` reference in a non-frozen tracked file resolves to an existing path — enforced by `check-playbooks` §2.1 in `lint-all` (CI), not review.
2. The README tree and the on-disk `founding/`+`operations/` dir set are identical — enforced by `check-playbooks` §3.1.
3. No numeric-prefixed `playbooks/NNN_*` directory exists at the playbooks root post-move — enforced by `check-playbooks` (glob assertion) + §1.3 test.
4. Every `playbooks/**/*.sh` is shellcheck-clean — enforced by `check-playbooks` shellcheck pass.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_founding_layout_present` | `founding/01_bootstrap`…`07_runner_bootstrap_prod` all exist |
| 1.2 | unit | `test_operations_layout_present` | each ops runbook + `operations/teardown/{database,redis}` exist |
| 1.3 | unit | `test_no_legacy_numbered_dirs` | glob `playbooks/[0-9][0-9][0-9]_*` → empty |
| 2.1 | integration | `test_all_refs_resolve` | every `playbooks/<path>` in non-frozen files resolves; planted bad ref → exit 1 |
| 2.2 | integration | `test_ci_paths_resolve` | `deploy-dev.yml`/`release.yml` gate paths resolve |
| 3.1 | integration | `test_readme_matches_tree` | README dir list == on-disk set; planted mismatch → exit 1 |
| 3.2 | unit | `test_no_phantom_gate_refs` | grep for `playbooks/gates/`/`001_gate.sh` → 0 matches |
| 3.3 | unit | `test_vault_read_convention_narrowed` | README convention text scopes `ALLOW_VAULT_READS` to interactive gates |
| 4.1 | e2e | `test_check_playbooks_passes_clean` | `make check-playbooks` exit 0 on corrected tree |
| 4.2 | e2e | `test_check_playbooks_catches_each_failure` | 3 planted faults (bad ref / drift / shellcheck) each → exit 1 |
| 4.3 | unit | `test_lint_all_includes_check_playbooks` | `lint-all` prereqs contain `check-playbooks` |

These tests ARE the `check-playbooks` script's own behaviour exercised against fixtures; the "test" is a planted-fault run asserting non-zero, since the gate is the deliverable. **Regression:** the live `make/test-integration.mk` teardown path and CI preflight must still run green post-move. **Idempotency:** N/A — one-time move.

---

## Acceptance Criteria

- [ ] `make check-playbooks` exits 0 — verify: `make check-playbooks; echo $?`
- [ ] Planted broken ref caught — verify: `sed -i 's#founding/02_preflight#founding/99_nope#' <tmp> && make check-playbooks; echo $?` (expect 1)
- [ ] No phantom gate refs remain — verify: `git grep -nE 'playbooks/gates/|001_gate\.sh' playbooks/ || echo CLEAN`
- [ ] No legacy numbered dirs — verify: `ls -d playbooks/[0-9][0-9][0-9]_* 2>/dev/null || echo CLEAN`
- [ ] `ARCHITECTURE.md` exists, `ARCHITECHTURE.md` gone — verify: `test -f playbooks/ARCHITECTURE.md && test ! -e playbooks/ARCHITECHTURE.md`
- [ ] History preserved — verify: `git log --follow --oneline playbooks/founding/01_bootstrap/001_playbook.md | tail -1`
- [ ] `make lint` clean (includes `check-playbooks` via `lint-all`)
- [ ] CI preflight green on the branch — verify: `gh pr checks`
- [ ] `gitleaks detect` clean · no non-`.md` file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: gate passes clean
make check-playbooks && echo "PASS" || echo "FAIL"
# E2: every playbooks/ ref resolves (excluding frozen done/ + CHANGELOG)
git grep -hoE 'playbooks/[A-Za-z0-9_/]+' -- ':!docs/v2/done/' ':!CHANGELOG.md' | sort -u \
  | while read -r p; do [ -e "$p" ] || echo "MISSING: $p"; done
# E3: phantom gate refs gone
git grep -nE 'playbooks/gates/|001_gate\.sh' -- ':!docs/v2/done/' || echo "CLEAN"
# E4: legacy numbered dirs gone
ls -d playbooks/[0-9][0-9][0-9]_* 2>/dev/null && echo "OVER" || echo "CLEAN"
# E5: shellcheck the moved scripts
find playbooks -name '*.sh' -print0 | xargs -0 shellcheck && echo "PASS"
# E6: lint umbrella includes it
make lint 2>&1 | grep -iE 'check-playbooks|FAIL'
# E7: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

**1. Orphaned files — renamed, not deleted.**

| File to delete | Verify |
|----------------|--------|
| `playbooks/ARCHITECHTURE.md` | `test ! -e playbooks/ARCHITECHTURE.md` |
| `playbooks/[0-9][0-9][0-9]_*/` (all flat numbered dirs) | `test -z "$(ls -d playbooks/[0-9][0-9][0-9]_* 2>/dev/null)"` |

**2. Orphaned references — zero remaining old paths (outside frozen records).**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `playbooks/0NN_<oldname>` | `git grep -nE 'playbooks/0[0-9][0-9]_' -- ':!docs/v2/done/' ':!CHANGELOG.md'` | 0 matches |
| `ARCHITECHTURE` | `git grep -n ARCHITECHTURE` | 0 matches |

---

## Discovery (consult log)

- **Indy decisions (Jun 02, 2026):** layout (founding/ + operations/, teardowns nested) approved; calls 1–3 delegated ("yes 1, 2, 3 take the call") → done-specs + CHANGELOG frozen, runners in `founding/`, CI-edit dimension in-scope; `.github/workflows` edits explicitly cleared.
- **Indy decisions (Jun 03, 2026) — folded-in workstreams + investigations (§5/§6):**
  > Indy (2026-06-03): "Fold full fix into #361" — context: the hermetic zombiectl test-infra fix (§5) that unblocked the pre-push hook's flaky telemetry-flush timeouts; chosen over a separate test-infra PR.
  > Indy (2026-06-03): "Fold Category B into #361" — context: covering the zombie/* command surface + workspaces.ts (§6), chosen over a standalone M82_002 coverage spec.
  > Indy (2026-06-03): "Keep all three" — context: three branches the test agents flagged as "dead code" (renderOutcome sse_error/else, onTurnError non-CliError, loadBundle non-SkillLoadError catch) proved on analysis to be defensive handling of `unknown`/union types; removing them needs unsafe casts and the floor passes with them present. Kept; a subagent's rogue removal of the renderOutcome branches was reverted before any commit.
- **Deferral (Indy-acked):**
  > Indy (2026-06-02): "Well i only deferred config.template" — context: the portability / de-milestoning config-template pass (abstracting vault names `ZMB_CD_*`, Clerk URLs, domains `usezombie.com/.sh`, hostnames `zombie-*-worker-*`, and milestone IDs `M{N}_{NNN}` out of the playbooks for a public "start a startup" repo) is a follow-up spec, NOT this workstream.
- **Pre-existing drift noted (NLR, fix-on-touch only):** several files cite `006_worker_bootstrap_dev` / `007_worker_bootstrap_prod` (pre-M80 "worker" name; dirs are `runner_*`). Fix to the correct `runner` name only in files we already edit for the move; do NOT touch frozen `done/` specs or CHANGELOG to chase it.
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`: appended during VERIFY/CHORE(close).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | `check-playbooks` fault-injection coverage confirmed (each failure mode planted → non-zero). Result in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial review of the move + re-point completeness vs the blast-radius table. Findings dispositioned. |
| After `gh pr create` | `/review-pr` | Post-rebase path-ref races + squash drift checked; comments addressed. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Gate clean | `make check-playbooks` | shellcheck ✓ · all refs resolve ✓ · README parity ✓ | ✅ |
| Gate catches faults | planted bad-ref / drift / shellcheck | all 3 → exit non-zero | ✅ |
| Refs resolve | full residual scan | CLEAN — no `playbooks/[0-9]{3}_` or `gates/` refs (excl frozen) | ✅ |
| CI paths resolve | grep workflows | `founding/02_preflight/00_gate.sh`, `founding/06_…/03_deploy_readiness.sh` ✓ | ✅ |
| ARCHITECHTURE gone | `git grep ARCHITECHTURE` (playbooks) | CLEAN (docs/ ref is a separate pre-existing file) | ✅ |
| History preserved | `git status` rename detection | 46 renames detected (R), redis added (A — was untracked) | ✅ |
| Gitleaks | pre-commit `gitleaks protect` | confirmed at commit | ✅ |
| §5 suite hermetic | `make test-unit-zombiectl` | 961 tests, 0 fail, no telemetry flake | ✅ |
| §6 coverage floor | `bun run test` (enforce-coverage) | function 96.54% ≥ 96, line 98.25% ≥ 97 | ✅ |
| §6 review clean | `/review` (3-lens adversarial) | scope/rules clean; test-quality findings fixed | ✅ |

---

## Out of Scope

- **`config.template` portability / de-milestoning** — abstracting vault names, Clerk URLs, domains, hostnames, and milestone IDs for a public reusable repo. Indy-acked deferral (Discovery); follow-up spec.
- **Rewriting frozen records** — `docs/v2/done/*` specs and `CHANGELOG.md` keep their historical (now-old) path references; this PR does not touch them.
- **Renumbering/renaming the founding playbooks' internal `001_playbook.md` / `0N_*.sh` files** — only the parent directory paths change; intra-dir filenames stay.
