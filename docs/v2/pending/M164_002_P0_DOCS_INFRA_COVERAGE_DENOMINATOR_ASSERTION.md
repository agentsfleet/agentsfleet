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

# M164_002: The Zig coverage gate refuses a verdict without its denominator

**Prototype:** v2.0.0
**Milestone:** M164
**Workstream:** 002
**Date:** Aug 14, 2026
**Status:** PENDING
**Priority:** P0 — the Continuous Integration (CI) coverage gate currently passes over 2.7% of the codebase, so every floor it claims to enforce is unenforced.
**Categories:** DOCS, INFRA
**Batch:** B1 — sequential with M164_001 on the same branch; no parallel workstream.
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Branch:** feat/m164-delete-the-free-trial — shared with M164_001; no new worktree, no new Pull Request (PR).
**Depends on:** M164_001 (same branch and PR; its 91% floor is the defect this workstream corrects)
**Provenance:** LLM-drafted (Claude Opus 5 (1M context), Aug 14, 2026) — evidence is a direct comparison of the CI coverage artifact against a local run on the same commit.
**Canonical architecture:** `docs/architecture/testing.md` §Coverage

---

## Overview

**Goal (testable):** `make test-coverage-zig` refuses to print a passing verdict unless the report it graded covers every product root and clears declared minimum measured-file and measured-line counts, so a report over a subset of the tree fails loudly instead of reading green.

**Problem:** The coverage gate reports a number nobody can trust, and its output gives no way to notice. On the last green CI run for `feat/m136` the merged report held 23 files and 853 measured lines and graded 98.36%; a local run on the same commit held 567 files and 31,854 measured lines and graded 88.31%. The two file sets do not overlap at all — all 23 CI files sit under `src/lib/`, so CI measured no daemon file and no runner file. Every floor the gate has ever claimed to enforce (60, then 83, then 91) was cleared by that 23-file subset, which means no coverage floor has been enforced in CI at any point. Work that genuinely raised daemon and runner coverage could not show up in CI, because CI was not measuring those trees.

**Solution summary:** The percentage stops being the only thing graded. A report checker reads the Cobertura output and asserts the denominator before any rate is accepted — minimum measured files, minimum measured lines, and the presence of every product root — then grades the merged rate and per-folder unit-lane rates against floors that live in exactly one file. Test-support sources that the current `_test.zig` pattern misses leave the denominator. The gate prints the counts beside the percentage so a shrinking denominator is visible in CI output rather than inferable only from an artifact download. Operators and reviewers see a coverage number that is either measured over the whole tree or absent.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(coverage): grade the denominator, not just the rate
- **Intent (one sentence):** Make the coverage gate incapable of reporting a passing number over a subset of the codebase, so the floors it enforces mean what they say.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `make/test-unit.mk` (`test-coverage-zig`) — the recipe being changed; its comment block already records why each existing guard exists, and none of them may be dropped.
2. `scripts/check_openapi_route_coverage.py` + `scripts/check_openapi_route_coverage_test.py` — the repository's established shape for a Python checker invoked from a make gate with its own self-tests; mirror it.
3. `docs/architecture/testing.md` §Coverage — the canonical description of the lane, currently stale on component count, floor value, and the strength of the report assertion.
4. Commit `9143b13c2` — the prior coverage fix on this branch. Its three guards (test bodies excluded, per-component directory removal, zero-pass and failure detection) stay; this workstream adds to them.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `scripts/check_coverage_report.py` | CREATE | Reads a Cobertura report and asserts denominator floors, required product roots, forbidden harness paths, and merged plus per-folder rates. |
| `scripts/check_coverage_report_test.py` | CREATE | Self-tests over reports built in-test, covering each assertion and its negative path. |
| `make/test-unit.mk` | EDIT | The coverage recipe delegates every report assertion to the checker and passes floors in rather than grading inline. |
| `make/test.mk` | EDIT | Denominator floors, per-folder unit-lane floors, and the exclusion pattern list become named variables; the merged floor is reset to a value the honest figure clears. |
| `docs/architecture/testing.md` | EDIT | §Coverage records six components, the denominator assertions, the per-folder floors, and the ratchet rule. |
| `docs/v2/active/M164_002_P0_DOCS_INFRA_COVERAGE_DENOMINATOR_ASSERTION.md` | CREATE | This spec, moved from `pending/` at CHORE(open). |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (the inline rate-grading and non-empty-report check the checker replaces are deleted, not left beside it), **NLR** (the coverage recipe is being touched, so its exclusion pattern is fixed rather than worked around), **UFS** (every floor, pattern, and root name is a named variable or module constant, never a repeated literal in a recipe), **ORP** (the architecture doc and any renamed make variable are swept for stale references), **FLL** (`make/test-unit.mk` is already 237 lines and its recipe body is the longest in the repository; moving assertion logic into Python must reduce it, not grow it), **TST-NAM** (checker self-test identifiers carry no milestone marker), **MSID** (no `M164_002` or `§x.y` marker in any source file).
- `dispatch/write_python.md` — the new checker: standard-library parsing (`xml.etree`), context-managed file handles, specific exceptions over bare `except`, validation at the argument boundary.
- `dispatch/write_shell.md` — the recipe body: quoted expansions, no unquoted pattern lists, temp-file cleanup preserved.
- `docs/architecture/testing.md` — the architecture consult for this lane; the doc wins until reconciled in the same diff.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` source changes | N/A |
| PUB / Struct-Shape | no — no new Zig pub surface | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — `make/test-unit.mk` at 237 lines, new Python module | Assertion logic moves out of the recipe into the checker, so the recipe shrinks; checker functions stay one-assertion-each and under the function cap. |
| UFS (repeated/semantic literals) | yes — floors, root names, exclusion patterns | Floors and patterns defined once in `make/test.mk`; root names and report key names as module constants in the checker; the recipe passes them as arguments. |
| UI Substitution / DESIGN TOKEN | no — no UI surface | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — no daemon logging, no allocator lifecycle, no error registry entry, no schema change | N/A |
| MILESTONE-ID | yes — new source files | No milestone marker, section number, or dimension reference in `scripts/*.py` or the make fragments. |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/check_openapi_route_coverage.py` and its `_test.py` sibling — a standard-library-only Python checker driven by a make gate, with self-tests run from the same target before the assertion. This workstream mirrors that shape exactly: argument-driven, no repository discovery inside the checker, self-tests first. The one justified divergence is input format — that checker reads an OpenAPI bundle, this one reads Cobertura Extensible Markup Language (XML).

## Sections (implementation slices)

### §1 — The denominator is graded before the rate

The load-bearing slice. A rate computed over an unknown file set carries no information, and the current gate accepts one. The checker refuses to grade a rate until the report's shape clears declared floors, and the gate prints that shape so a shrinking denominator is visible without downloading an artifact. **Implementation default:** floors are absolute counts rather than a percentage of some expected total, because the expected total is exactly what a broken run gets wrong.

- **Dimension 1.1** — A report whose measured-file count is below the declared floor fails, naming measured and floor → Test `test_file_floor_breach_fails_with_counts`
- **Dimension 1.2** — A report whose measured-line count is below the declared floor fails, naming measured and floor → Test `test_line_floor_breach_fails_with_counts`
- **Dimension 1.3** — A report missing any declared product root fails, naming the absent root, regardless of how high its rate is → Test `test_absent_product_root_fails_despite_high_rate`
- **Dimension 1.4** — The measured file and line counts are written beside the percentage in the gate's key/value output and echoed in the success line → Test `test_counts_emitted_beside_percentage`
- **Dimension 1.5** — A report with no measured classes at all fails as an empty report rather than dividing by zero → Test `test_empty_report_fails_before_rate`

### §2 — CI measures the same tree the developer does

§1 makes the defect loud; this slice makes CI correct. The include-pattern resolution that yields 567 files locally and 23 in the container is single-sourced so host and container agree by construction, and the assertion from §1 is what proves it rather than a reviewer's eye. **Implementation default:** assert root presence and count floors rather than exact equality between platforms, because platform-gated sources legitimately differ between a macOS and a Linux run.

- **Dimension 2.1** — The kcov include root is derived from one make variable used by every component invocation, so no component can resolve a different tree → Test `test_include_root_single_sourced`
- **Dimension 2.2** — Each per-component report clears its own denominator floor, so a single component silently resolving a subset fails without waiting for the merge → Test `test_per_component_floor_enforced`
- **Dimension 2.3** — The merged report produced under CI clears the same product-root assertion the local report clears → Test `test_merged_report_carries_every_product_root`

### §3 — Test-support code leaves the denominator

`--exclude-pattern=_test.zig` catches only names ending that way, leaving 788 lines of harness across 17 files counted as product. That is tolerable noise against an 83% floor and material against the 95% target this milestone's follow-on work aims at. **Implementation default:** exclude by the naming forms actually present in the tree, enumerated in one variable, rather than by a broad `test` substring that would also swallow product files.

- **Dimension 3.1** — Every harness naming form present in the tree is excluded, and the daemon component's measured-line count drops by the harness total → Test `test_harness_naming_forms_excluded`
- **Dimension 3.2** — `fleet_runtime/config_helpers.zig`, `http/handlers/auth/session_helpers.zig`, and `http/handlers/memory/helpers.zig` remain in the denominator, because they are product → Test `test_product_helpers_retained`
- **Dimension 3.3** — A report containing any excluded harness path fails, so a newly added test-support file cannot quietly re-enter the denominator → Test `test_harness_path_in_report_fails`

### §4 — Declared targets, enforced floors, and the gap between them

The targets are fixed: **91% merged overall, 95% on the `agentsfleetd/` unit lane, 95% on the `runner/` unit lane**. None is cleared today — measured values are 89.07%, 67.48%, and 90.14%. Enforcing a target the tree does not meet produces a permanently red build rather than a gate, and enforcing only what the tree already meets loses the target. So each scope carries two numbers: the target, which is fixed and printed every run, and the enforced floor, seeded at the measured value and raise-only. The printed gap is what keeps the target from being forgotten. **Implementation default:** floors seed at the measured value rounded down to the whole point; targets are literals that only Indy changes.

- **Dimension 4.1** — Separate unit-lane floors and targets exist for `agentsfleetd/` and `runner/`, plus a merged pair, each defined once → Test `test_folder_floors_and_targets_defined_once`
- **Dimension 4.2** — A per-folder floor breach fails naming the folder, its measured rate, and its floor, distinctly from a merged-rate breach → Test `test_folder_breach_names_folder_and_floor`
- **Dimension 4.3** — Every enforced floor is at or below its measured value, so a green tree stays green → Test `test_enforced_floors_clear_measured_values`
- **Dimension 4.4** — Each scope's remaining gap to target is computed and published every run, and a floor may never be set above its target → Test `test_gap_to_target_published_and_bounded`

### §5 — The architecture doc describes the instrument that exists

`docs/architecture/testing.md` §Coverage still says five binaries, a 60% floor against a 61.40% baseline, and "each binary must produce a non-empty Cobertura report" — the weakest possible assertion and the one this workstream replaces. A stale canonical doc is how the next agent reintroduces the defect.

- **Dimension 5.1** — §Coverage records six components, the denominator assertions, the per-folder floors, the 91/95/95 targets, and the raise-only ratchet rule, with no surviving reference to five binaries or a 60% floor → Test `test_architecture_doc_matches_gate_values`

## Interfaces

```
.tmp/zig-coverage.txt — key/value surface read by the CI summary step.
Existing keys are preserved verbatim; new keys are additive.

  zig_line_coverage_pct=<float>            # existing
  zig_line_coverage_min_pct=<int>          # existing
  zig_measured_files=<int>                 # new
  zig_measured_lines=<int>                 # new
  zig_measured_files_min=<int>             # new
  zig_measured_lines_min=<int>             # new
  zig_line_coverage_target_pct=<int>       # new — 91, fixed
  zig_line_coverage_gap_pts=<float>        # new — target minus measured
  zig_folder_pct_agentsfleetd=<float>      # new
  zig_folder_min_pct_agentsfleetd=<int>    # new
  zig_folder_target_pct_agentsfleetd=<int> # new — 95, fixed
  zig_folder_gap_pts_agentsfleetd=<float>  # new
  zig_folder_pct_runner=<float>            # new
  zig_folder_min_pct_runner=<int>          # new
  zig_folder_target_pct_runner=<int>       # new — 95, fixed
  zig_folder_gap_pts_runner=<float>        # new

scripts/check_coverage_report.py — argument-driven; no repository discovery.
  --report PATH             Cobertura file to grade (required)
  --min-files N             denominator floor: measured files
  --min-lines N             denominator floor: measured lines
  --require-root NAME       repeatable; product root that must be present
  --forbid-path-pattern P   repeatable; harness form that must be absent
  --min-line-rate PCT       merged rate floor (enforced)
  --line-rate-target PCT    merged rate target (published, not enforced)
  --folder-floor NAME=PCT   repeatable; per-folder rate floor (enforced)
  --folder-target NAME=PCT  repeatable; per-folder rate target (published)
  --emit-keys PATH          append the key/value surface above
Exit 0 on all assertions passing; exit 1 naming the first breach with
measured and expected values. A floor above its own target is a usage
error and also exits 1. No other exit code.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Report over a subset of the tree | kcov include-pattern resolves fewer files than the tree holds (CI today: 23 of 567) | Gate exits 1 naming measured file and line counts against their floors; no percentage is accepted or printed as passing |
| Product root absent entirely | Module path resolution differs between host and container | Gate exits 1 naming the absent root; a high rate over the remaining roots does not rescue it |
| Suite ran but produced no report | Component binary exits 0 without executing tests | Existing zero-pass detection retained; checker additionally fails an empty class set before computing a rate |
| Stale kcov output rejoins the merge | Rebuilt binary hashes to a new directory beside its predecessor | Existing per-component directory removal retained; a merge that shrinks the denominator now trips the file and line floors |
| Harness file re-enters the denominator | A new test-support source with a naming form not yet enumerated | Gate exits 1 naming the offending path, so the exclusion list is extended rather than the number quietly shifting |
| Floor set above the truth | A floor edited without the tests that clear it | Breach names folder, measured rate, and floor; the raise-only ratchet rule in the architecture doc makes the intended direction explicit |
| Checker invoked with a missing report | Component failed before writing output | Gate exits 1 naming the unreadable path; absence is never treated as an empty pass |

## Invariants

1. No rate is graded before its denominator — the checker computes and asserts file and line counts before computing any percentage, so an unreadable or undersized report cannot yield a passing verdict. Enforced by argument-order-independent assertion sequence in the checker plus a self-test that a subset report fails.
2. Every declared product root appears in the merged report — enforced by the `--require-root` assertion; absence is exit 1, independent of rate.
3. No excluded harness path appears in any graded report — enforced by the `--forbid-path-pattern` assertion, which fails on the first match.
4. Every floor has exactly one definition site in `make/test.mk` — enforced by the checker accepting floors only as arguments, so a recipe cannot hold a second copy.
5. The gate's printed verdict always carries its denominator — enforced by the checker emitting counts and rate together to `--emit-keys` or failing; there is no code path that writes a rate alone.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | internal verification lane only; no user or operator surface emits anything new | N/A | N/A | N/A |

The CI job summary gains denominator columns by reading additional keys from the existing `.tmp/zig-coverage.txt` surface, which requires no workflow change. No analytics or funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_file_floor_breach_fails_with_counts` | 23-file report against a 400-file floor → exit 1, message carries both 23 and 400 |
| 1.2 | unit | `test_line_floor_breach_fails_with_counts` | 853-line report against a 25,000-line floor → exit 1, message carries both numbers |
| 1.3 | unit | `test_absent_product_root_fails_despite_high_rate` | report at 98.4% holding only `src/lib/` paths, roots `agentsfleetd` and `runner` required → exit 1 naming both absent roots |
| 1.4 | unit | `test_counts_emitted_beside_percentage` | passing report with `--emit-keys` → file contains rate, both counts, and both count floors |
| 1.5 | unit | `test_empty_report_fails_before_rate` | report with zero class elements → exit 1 as empty, no ZeroDivisionError |
| 2.1 | unit | `test_include_root_single_sourced` | grep the coverage recipe → exactly one definition of the include root, referenced by every component invocation |
| 2.2 | unit | `test_per_component_floor_enforced` | daemon component report below its own floor → exit 1 before the merge is graded |
| 2.3 | integration | `test_merged_report_carries_every_product_root` | `make test-coverage-zig` on a clean tree → merged report holds `agentsfleetd`, `runner`, and `lib` paths |
| 3.1 | unit | `test_harness_naming_forms_excluded` | report containing each enumerated harness form → every one reported as forbidden |
| 3.2 | unit | `test_product_helpers_retained` | report containing the three product `*helpers*.zig` paths → passes; none is treated as harness |
| 3.3 | unit | `test_harness_path_in_report_fails` | report containing `http/test_harness.zig` → exit 1 naming that path |
| 4.1 | unit | `test_folder_floors_and_targets_defined_once` | grep `make/test.mk` → exactly one floor and one target definition for each of `agentsfleetd`, `runner`, and merged |
| 4.2 | unit | `test_folder_breach_names_folder_and_floor` | runner folder at 88% against a 90% floor → exit 1 naming `runner`, 88, and 90, distinct from a merged breach |
| 4.3 | unit | `test_enforced_floors_clear_measured_values` | each enforced floor against its measured value → floor is the lower or equal value in all three scopes |
| 4.4 | unit | `test_gap_to_target_published_and_bounded` | daemon at 67.48% with target 95 → published gap is 27.52; a floor argument above its target → exit 1 |
| 5.1 | unit | `test_architecture_doc_matches_gate_values` | grep `docs/architecture/testing.md` → zero matches for "five binaries" or a 60% floor; six components and the ratchet rule present |

Regression rows: the three guards from `9143b13c2` must keep firing — `test_zero_pass_suite_still_fails`, `test_failing_suite_still_fails`, `test_test_bodies_still_excluded`. Idempotency: `test_checker_is_pure_over_repeat_invocation` — grading the same report twice yields identical output and exit code.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A subset report cannot pass (§1) | `python3 -m unittest discover -s scripts -t scripts -p 'check_coverage_report_test.py'` | exit 0 | P0 | |
| R2 | The gate grades the whole tree (§1, §2) | `env -u AGENTSFLEET_API_URL make test-coverage-zig` | exit 0 | P0 | |
| R3 | Every product root is measured (§2) | `python3 -c "import xml.etree.ElementTree as E;f={c.get('filename') for c in E.parse('coverage/zig/merged/cobertura.xml').getroot().iter('class')};print(sum(any(p in x for x in f) for p in ('agentsfleetd/','runner/','lib/')))"` | `3` | P0 | |
| R4 | Harness code is out of the denominator (§3) | `python3 -c "import xml.etree.ElementTree as E;print(sum('test_harness' in (c.get('filename') or '') for c in E.parse('coverage/zig/merged/cobertura.xml').getroot().iter('class')))"` | `0` | P0 | |
| R5 | Denominator is published beside the rate (§1) | `grep -cE '^zig_measured_(files\|lines)=' .tmp/zig-coverage.txt` | `2` | P0 | |
| R6 | Per-folder floors are enforced (§4) | `grep -cE '^zig_folder_min_pct_(agentsfleetd\|runner)=' .tmp/zig-coverage.txt` | `2` | P0 | |
| R9 | Targets 91/95/95 are published with their gaps (§4) | `grep -E '^zig_(line_coverage\|folder)_target_pct' .tmp/zig-coverage.txt \| sort` | `91`, `95`, `95` across three lines | P0 | |
| R7 | Architecture doc matches the gate (§5) | `grep -ciE 'five binaries\|floor is 60%' docs/architecture/testing.md` | `0` | P0 | |
| R8 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit lanes pass | `env -u AGENTSFLEET_API_URL make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `env -u AGENTSFLEET_API_URL make lint-all` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | N/A — no files deleted |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| inline merged-rate `awk` grading in the recipe | `grep -n 'is below threshold' make/test-unit.mk` | 0 matches |
| "produced no Cobertura report" non-empty check | `grep -c 'produced no Cobertura report' make/test-unit.mk` | 0 matches |
| stale architecture text | `grep -ciE 'five binaries\|non-empty Cobertura' docs/architecture/testing.md` | 0 matches |

## Out of Scope

- The unit-lane coverage work itself — roughly 7,100 daemon lines and 200 runner lines to reach the 95% targets Indy set. Follow-on workstreams under a later milestone; this workstream only makes the measurement trustworthy enough to direct that work and publishes the remaining gap every run.
- `.github/workflows/test.yml` changes. The CI summary step already reads `.tmp/zig-coverage.txt`, so the new keys surface without touching a workflow, and workflow edits are approval-gated.
- The app, website, agentsfleet, and design-system coverage gates. Different tooling, separate floors, unaffected by this diff.
- Raising any floor above its seeded value. The ratchet is a follow-on activity, gated on the tests that clear it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer reads `✓ [zig] merged line coverage passed` in a CI log and can act on it, because the same line states how many files and lines that verdict covered. Today that line is printed over 23 files and nobody can tell.
2. **Preserved user behaviour** — `make test-coverage-zig` keeps its name, its key/value output surface, its existing guards from `9143b13c2`, and its exit-code meaning. Every current caller, including the CI summary step, keeps working unchanged.
3. **Optimal-way check** — the unconstrained-optimal shape asserts coverage per file with a per-file floor, which would catch a single regressed file rather than a shrunken tree. Absolute denominator floors plus root presence are the direct fix for the observed defect; per-file floors are a larger design that this evidence does not yet justify.
4. **Rebuild-vs-iterate** — iterate. The lane's structure is right and its prior guards are sound; the defect is a missing assertion, not a wrong instrument. Replacing kcov or the merge model would trade away run-to-run determinism for no gain against this problem.
5. **What we build** — one argument-driven report checker with self-tests, floors and patterns named once in `make/test.mk`, a recipe that delegates grading to the checker, and an architecture doc that matches.
6. **What we do NOT build** — per-file floors (see item 3); a coverage trend database; a pull-request comment bot; any change to the four package-level coverage gates; the follow-on unit-lane tests themselves.
7. **Fit with existing features** — compounds with `9143b13c2`'s guards and with the reachability gate that floors test counts. It must not destabilise `make test-unit-all`, which runs this target as part of `test-coverage-all`.
8. **Surface order** — N/A — no user surface. The only consumers are the make gate and the CI job summary.
9. **Dashboard restraint** — N/A — no user surface. The CI summary shows only counts the checker actually measured, never a projected or interpolated figure.
10. **Confused-user next step** — the failure message names the measured value, the expected floor, and the offending root or path, which is the whole self-serve move: the engineer reads what was measured and either fixes the resolution or extends the exclusion list.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five Sections ordered so the loud failure lands before the fix that quiets it. §1 makes the defect impossible to miss, §2 corrects the resolution it exposes, §3 cleans the denominator, §4 sets floors the truth clears, §5 stops the canonical doc from reintroducing all four. Grouped as one workstream on M164's existing branch because the defect being corrected is M164_001's own 91% floor.
- **Alternatives considered:** (a) drop the floor to a passing value and land #601 now, deferring the instrument — rejected, it preserves a gate that cannot fail and leaves the next agent measuring 2.7% of the tree; (b) a full per-file coverage floor with a trend database — rejected as a larger build than the evidence supports, and named here rather than silently mud-patched toward.
- **Patch-vs-refactor verdict:** this is a **patch** because the lane's structure, component split, and merge model are all correct — the gate is missing one assertion and carrying one leaky pattern. A refactor would touch the parts that work.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
