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

# M164_002: Coverage floors that bind per folder, over a denominator with no harness in it

**Prototype:** v2.0.0
**Milestone:** M164
**Workstream:** 002
**Date:** Aug 14, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — `agentsfleetd/`, `runner/` and `lib/` all sit below the 95% Indy set as the quality bar, and one merged floor cannot say which of them moved.
**Categories:** DOCS, INFRA
**Batch:** B1 — resumed on its own branch after M164_001 merged; one Pull Request (PR).
**Test Baseline:** unit=3907 integration=638
**Branch:** feat/m164-002-coverage-floors — resumed Aug 15, 2026 in `../agentsfleet-m164-002-coverage`; M164_001's branch merged as PR #601 and was pruned.
**Depends on:** M164_001 (merged, PR #601) — its floor is the value this workstream replaces with per-folder floors
**Provenance:** LLM-drafted (Claude Opus 5 (1M context), Aug 14, 2026) — measurements taken by running this branch's own `scripts/check_zig_coverage.py` reader over the reports on disk, not from prose.
**Canonical architecture:** `docs/architecture/testing.md` §Coverage

---

## Overview

**Goal (testable):** The coverage gate publishes and enforces a floor per product folder over a denominator containing no test-support code, and prints each folder's remaining gap to its target, so a daemon that gains coverage while the runner loses it can no longer read as one unchanged number.

**Problem:** Three defects survive on this branch, all of them invisible in the gate's output. First, the floor is a single merged figure — `agentsfleetd/` at 67.48% and `runner/` at 90.10% average into 89.07%, so 22 points of daemon shortfall are masked by better-covered trees, and no floor can bind the folder that needs it. Second, 788 lines of test-support code across 17 files are still counted as product in the daemon lane, because the exclusion catches only `*_test.zig` and two test-root spellings while the tree also holds `test_harness.zig`, `test_fixtures.zig`, `test_support.zig`, `testing.zig`, `test_sse_client.zig`, `test_port.zig`, and `webhook_test_signers.zig`. Third, the merged floor on this branch is 91 against a measured 89.07, so the gate is red with no output distinguishing "a real regression" from "a target nobody has reached yet."

**Solution summary:** Floors become per-folder and gain a second number beside them. Each scope — merged, the `agentsfleetd/` unit lane, the `runner/` unit lane — carries an enforced floor seeded at its measured value and a fixed target, and the gate publishes measured, floor, target, and the remaining gap for all three every run. Test-support sources leave the denominator by naming form rather than by a single suffix, so the daemon lane measures product. A minimum measured-file and measured-line count per component, plus an assertion that every product root is present, replaces the current check that only fires when a component contributes literally nothing. An engineer reads the gate output and knows which folder moved, by how much, and how far it still is from where it must land.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(coverage): bind floors per folder over a product-only denominator
- **Intent (one sentence):** Make coverage enforceable per product folder, over lines that are actually product, with each folder's distance from its target visible on every run.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `scripts/check_zig_coverage.py` — the checker being extended. Its docstring records why the union replaced `kcov --merge`; that work is done and must not be re-litigated. `is_product_source` is the function this workstream widens, and `summarise` is what gains a per-folder view.
2. `scripts/check_zig_coverage_test.py` — the existing self-tests; every new assertion gets a sibling here in the same style.
3. `make/test-unit.mk` (`test-coverage-zig`, the `components=` list) — the component names the gate iterates, and the invocation that gains the new arguments.
4. `docs/architecture/testing.md` §Coverage — canonical for this lane and currently stale on component count, floor value, and assertion strength.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `scripts/check_zig_coverage.py` | EDIT | Widens the test-support filter, adds per-folder rates, per-component denominator floors, required-root assertion, and the target/gap output. |
| `scripts/check_zig_coverage_floors.py` | CREATE | Holds the per-folder floor and target grading plus the key/value emitter, so the checker stays inside the file length cap. |
| `scripts/check_zig_coverage_test.py` | EDIT | Self-tests for every new assertion and its negative path. |
| `make/test-unit.mk` | EDIT | Passes the new floors, targets, roots, and denominator minimums to the checker. |
| `make/test.mk` | EDIT | Defines the per-folder floors and targets and the denominator minimums as named variables, one definition site each. |
| `src/build/shared.zig` | EDIT | `TEST_USE_LLVM` — the single definition site forcing test binaries through LLVM, carrying why. |
| `build.zig`, `build_runner.zig`, `src/build/{s3,daemon_tests,auth_tests,test_list,lib_tests}.zig` | EDIT | Each `addTest` site reads `TEST_USE_LLVM`, so no test binary can be built with unreadable debug info. |
| `src/build/bench_incident.zig` | CREATE | The incident-response bench steps, moved out of `build.zig` when the added line crossed the 350-line cap. |
| `docs/architecture/testing.md` | EDIT | §Coverage records the component set, the per-folder floors and targets, the denominator assertions, and the raise-only ratchet rule. |
| `src/runner/**/*_test.zig` | CREATE/EDIT | Unit-lane tests lifting the runner's four worst files and the tail to the 95% target. |
| `src/agentsfleetd/**/*_test.zig` | CREATE/EDIT | Live-harness unit tests for the zero-percent handlers and near-zero stores, descending dark order. |
| `src/agentsfleetd/tests.zig`, `src/runner/tests.zig` (test roots) | EDIT | New test files registered by explicit import, per the repository's test-discovery rule. |
| `docs/v2/active/M164_002_P0_DOCS_INFRA_COVERAGE_DENOMINATOR_ASSERTION.md` | EDIT | This spec; Dimensions marked DONE alongside their code. |
| `src/lib/**/*_test.zig` | CREATE/EDIT | Unit-lane tests lifting `lib/` to its 95% target. |
| `README.md` | EDIT | The badge row gains one Codecov badge per measured surface — `zig`, `app`, `website`, `cli` (§6). |
| `scripts/publish_coverage_badge.py` | DELETE | Written for the superseded `badges` branch; Codecov hosts the number, so this is dead (RULE NDC). |
| `scripts/publish_coverage_badge_test.py` | DELETE | Its ten tests go with it. |
| `scripts/check_zig_coverage_doc_test.py` | CREATE | Parity self-tests: the architecture doc's floors table, component list and variable names against `make/test.mk` and `make/test-unit.mk` (§5). |
| `scripts/check_zig_test_lanes_test.py` | EDIT | The stubbed kcov emits the lifecycle run marker the recipe now greps; one test asserts a skipped proof fails the lane. |
| `scripts/check_lane_concurrency_test.py` | EDIT | The pinned status-file path moved under `ZIG_COVERAGE_DIR`. |
| `make/bench.mk` | EDIT | The boot-drain lane reads the lifecycle filter, marker and isolation variable from their one definition site. |
| `.github/workflows/test.yml` | EDIT | Four Codecov upload steps, one per flag — **approval-gated, diff shown to Indy before it lands** (§6). |
| `playbooks/operations/m164_free_trial_removal/` | DELETE | One-shot hand-migration for M164_001, already applied; ephemeral by nature and referenced by nothing but M164_001's own record (§7). |
| `docs/v2/done/M155_001_P1_API_OBS_UI_CHARGE_SLICE_BREAKDOWN.md` | RENAME | Carried over from `main`: M155_001 parked, moved `pending/` → `done/`. Bookkeeping only, no code. |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (the single `--min-pct` grading path is replaced, not left beside the per-folder one), **NLR** (the checker is being touched, so its narrow test-support filter is fixed rather than worked around), **UFS** (floors, targets, root names, and naming forms are named variables or module constants with one definition site each), **ORP** (the architecture doc and every caller of the checker's changed argument surface are swept), **FLL** (`scripts/check_zig_coverage.py` is at 231 lines, so the floor grading lands in a sibling module rather than growing it past the cap), **TST-NAM** (self-test identifiers carry no milestone marker), **MSID** (no `M164_002` or section reference in any source file).
- `dispatch/write_python.md` — standard-library parsing, context-managed handles, specific exception classes preserved to the caller, validation at the argument boundary.
- `dispatch/write_shell.md` — the recipe body: quoted expansions, no unquoted argument lists, existing temp-file handling preserved.
- `docs/architecture/testing.md` — the architecture consult for this lane; the doc wins until reconciled in the same diff.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` source changes | N/A |
| PUB / Struct-Shape | no — no Zig pub surface | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — checker at 231 lines, new sibling module | Floor and target grading lands in `check_zig_coverage_floors.py`; every new function stays single-assertion and under the function cap. |
| UFS (repeated/semantic literals) | yes — floors, targets, naming forms, root names | Defined once in `make/test.mk` and passed as arguments; naming forms and root names as module-level frozensets in the checker. |
| UI Substitution / DESIGN TOKEN | no — no UI surface | N/A |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no — Python is outside the logging trigger surface, no `UZ-` codes, no allocator lifecycle, no schema change | N/A |
| MILESTONE-ID | yes — Python sources touched | No milestone marker, section number, or dimension reference in `scripts/*.py` or the make fragments. |

## Prior-Art / Reference Implementations

- **Reference:** `scripts/check_zig_coverage.py` itself, plus `scripts/check_openapi_route_coverage.py` and its `_test.py` sibling — the repository's established shape for a standard-library-only Python checker driven from a make gate, argument-driven with no repository discovery inside the checker, self-tests in a sibling module. This workstream extends the first and mirrors the second's test style. No divergence.

## Sections (implementation slices)

### §1 — The denominator holds product only

788 lines across 17 files in the daemon lane are test harness counted as product, because `is_product_source` recognises only a `_test.zig` suffix and two test-root spellings. A gate satisfiable by writing more harness measures the wrong thing, and at a 95% target the noise is larger than the margin. **Implementation default:** exclude by the naming forms present in the tree, enumerated in one place, rather than by a broad `test` substring that would also swallow product files.

- **Dimension 1.1** — DONE —  Every test-support naming form in the tree is excluded, and the daemon lane's measured-line count drops by the harness total → Test `test_support_naming_forms_excluded`
- **Dimension 1.2** — DONE —  `fleet_runtime/config_helpers.zig`, `http/handlers/auth/session_helpers.zig`, and `http/handlers/memory/helpers.zig` stay in the denominator, because they are product → Test `test_product_helpers_retained`
- **Dimension 1.3** — DONE —  A path matching an excluded form contributes no measured line to any component or to the union → Test `test_excluded_form_absent_from_union`

### §2 — A component that shrinks is caught before it is averaged

The union already fails a component contributing literally nothing. A component contributing a handful of lines still passes, and that is the shape kcov's Linux merge actually produced. This slice puts a floor under the shape of the report, not just its rate, and asserts that every product root is represented. **Implementation default:** absolute minimum counts rather than a percentage of an expected total, because the expected total is exactly what a degraded run gets wrong.

- **Dimension 2.1** — CUT (Indy, Aug 15, 2026 — see Discovery). Per-component minimums were built and removed: fourteen hand-maintained numbers duplicating the `--require-component` assertion that already fails a component contributing nothing, and every one of them turns an honest deletion of dead code into a red gate. Replaced by a single union-level collapse alarm — `--min-files` / `--min-lines`, set near half the measured figures — which catches the failure actually observed (24 files reported where the tree holds 558) without ratcheting against shrinkage → Test `test_collapsed_report_fails_before_any_rate`
- **Dimension 2.2** — DONE —  A union missing any declared product root fails naming the absent root, however high its rate → Test `test_absent_product_root_fails_despite_high_rate`
- **Dimension 2.3** — DONE — The union's own measured-file and measured-line counts are published alongside the rate → Test `test_summary_file_publishes_the_denominator_and_the_component_counts`
- **Dimension 2.5** — DONE — Every test binary compiles through LLVM from one definition site, so kcov can read its line table at all; a component whose debug info regresses fails at the required-component assertion rather than reporting a smaller number → Tests `test_required_component_contributing_nothing_fails_the_gate` (the alarm), plus the measured proof recorded in Discovery: `logging` 0 → 7 and `deadline` 0 → 8 product classes under real kcov
- **Dimension 2.4** — DONE — kcov captures two of the eight components on Linux, so the gate grades the union of those that did collect, states `measured over N of M components` naming every component that captured nothing on success and failure alike, and fails when a component named in `ZIG_COVERAGE_REQUIRED_COMPONENTS` contributes nothing → Tests `test_unrequired_empty_component_is_graded_over_what_collected`, `test_required_component_contributing_nothing_fails_the_gate`, `test_scope_line_names_every_component_that_captured_nothing`, `test_every_component_empty_leaves_nothing_to_grade`

### §3 — Floors bind per folder, targets stay visible

One merged figure cannot bind three trees moving independently: `agentsfleetd/` 89.47%, `runner/` 93.75% and `lib/` 93.77% average into 90.18%, and a floor on that average lets any one of them fall while the number holds. Each scope gains an enforced floor and a fixed target — **95% merged, 95% `agentsfleetd/`, 95% `runner/`, 95% `lib/`**, the bar Indy set for all three folders — with the remaining gap published every run so the distance stays visible while §4 closes it. **Implementation default:** floors seed at the measured value rounded down to the whole point and are raise-only, ratcheting toward 95 in the same commit as the tests that clear each step; targets are literals only Indy changes.

- **Dimension 3.1** — DONE —  Enforced floors and fixed targets exist for merged, `agentsfleetd/`, and `runner/`, each with one definition site → Test `test_floors_and_targets_defined_once`
- **Dimension 3.2** — DONE —  A per-folder floor breach fails naming the folder, its measured rate, and its floor, distinctly from a merged breach → Test `test_folder_breach_names_folder_and_floor`
- **Dimension 3.3** — DONE —  Every enforced floor is at or below its measured value, so a tree that has not regressed stays green → Test `test_enforced_floors_clear_measured_values`
- **Dimension 3.4** — DONE —  Each scope's gap to target is computed and published, and a floor above its own target is a usage error → Test `test_gap_to_target_published_and_bounded`
- **Dimension 3.5** — DONE —  The merged floor on this branch is reconciled to a value the measured figure clears, so a red gate means a regression → Test `test_merged_floor_clears_measured_figure`

### §4 — The lanes rise file by file, lowest first

The coverage work itself, and on the re-measurement it is the bulk of this workstream rather than a tail on it. Walk the ranked dark-line list per folder and add tests to the worst files individually, not merely the files this branch's diff touches. `lib/` (+15 covered lines) and `runner/` (+50) are within reach and go first, which puts two folders at 95 early and leaves one number moving. `agentsfleetd/` needs **+1,450** and is the long pole: its dark mass is not four fat files but roughly 60–100 files in the 20–50 dark-line band, most of them error arms — invalid payloads, refused authorisation, datastore failures — that the suites construct the happy path around and never drive. Each folder's floor ratchets toward 95 in the same commit as the tests that clear the step. **Implementation default:** target files in descending union-dark order within each folder, and drive error arms through the in-process harness rather than reaching for new abstractions; a file is done when its dark remainder is unreachable-by-design (process-fatal paths, operating-system-specific branches) and that remainder is named in the test file.

- **Dimension 4.1** — IN_PROGRESS —  `lib/` clears 95%: `logging/mod.zig` carries 42 of the folder's 76 dark lines and 15 close the gap → Tests per behaviour on the scoped-logger arms
- **Dimension 4.2** — NOT STARTED —  `runner/` clears 95%: `daemon/lease_run.zig` (40 dark), `child_supervisor.zig` (25), `engine/runner.zig` (24), then the tail until the folder rate clears 95 → Tests per file, `test_…` per behaviour
- **Dimension 4.3** — IN_PROGRESS —  the daemon's worst files by union-dark count gain tests in descending order — `http/handlers/tenant_provider.zig` (53), `cmd/serve_webhook_lookup.zig` (48), `http/handlers/tenant_model_entries.zig` (47), `http/handlers/admin/platform_keys.zig` (42), `auth/clerk_backend.zig` (36), `http/handlers/auth/sessions.zig` (35), continuing down the ranking → Tests per verb, success and failure halves
- **Dimension 4.4** — DONE —  `cmd/serve.zig` was 116 dark lines at 0% because nothing drove the boot sequence. The test that drives the real `serve.run` already existed and already skipped: it needs its own process. A `lifecycle` kcov component runs it filtered and isolated, and the file reads **112/116, 96.6%** — the union rose 88.34% → 89.20% (+221 covered lines) with no new test code, `cmd/serve_shutdown.zig`, `cmd/serve_background.zig`, `cmd/serve_qstash.zig` and the three sweepers gaining alongside it → Tests `test_a_skipped_lifecycle_proof_fails_the_lane` (the marker assertion that keeps it honest), plus the measured proof recorded in Discovery
- **Dimension 4.5** — NOT STARTED —  `agentsfleetd/` clears 95% over the product-only denominator → Test `test_enforced_floors_clear_measured_values` (re-graded)
- **Dimension 4.6** — DONE —  every folder floor is raised in the same commit as the tests that clear the new value, never ahead → Test `test_enforced_floors_clear_measured_values` (re-graded per ratchet)

### §5 — The architecture doc describes the instrument that exists

`docs/architecture/testing.md` §Coverage still says five binaries, a 60% floor against a 61.40% baseline, and "each binary must produce a non-empty Cobertura report" — the weakest assertion in the lane and the one §2 replaces. It also predates the union, the `s3` component, and the runner integration component. A stale canonical doc is how the next agent reintroduces all of it.

- **Dimension 5.1** — DONE — §Coverage records the nine-component set by gate name, the union, the kcov Linux capture gap and its evidence, the required-component and required-root assertions, the collapse alarm, the full published key surface, the denominator rule with the figure that motivated it, the per-folder floors table, the raise-only ratchet rule, and `lib/`'s 97.05% ceiling. Three stale claims are gone: the retired `ZIG_COVERAGE_MIN_LINES`, a floor value that never matched the gate, and "per-folder floors cannot be enforced in CI at all", true only before the LLVM repair. A merge conflict marker committed to the default branch mid-sentence is gone with them → Tests `test_architecture_doc_matches_gate_values`, `test_every_product_scope_is_documented`, `test_architecture_doc_lists_every_measured_component`, `test_architecture_doc_names_no_retired_variable`, `test_the_doc_carries_no_conflict_marker` (`scripts/check_zig_coverage_doc_test.py`)

### §6 — The README carries the figure a real run produced

The repository is public and its README opens with a badge row that says nothing about test quality. Indy's requirement is exact: the badge shows **what was executed and run**, not a floor, not a hand-typed number. The gate already writes `zig_line_coverage_pct` to `.tmp/zig-coverage.txt` after grading, so the figure exists; it just has nowhere to go. This publishes it from the run that produced it and points the README at it. **Implementation default:** publish only from a run that graded green on the default branch — a badge fed by a failed or partial run is worse than no badge, because it reports a number over a suite that did not finish. The workflow edit is approval-gated and does not land until Indy has seen the diff.

**Superseded — the repository does not host the number.** The first build wrote
`scripts/publish_coverage_badge.py`, which turned the graded summary into a
shields endpoint payload committed to an orphan `badges` branch. Indy cut it on
two grounds: there is no point carrying that machinery, and the row needs four
numbers, not one — `app`, `website` and `cli` alongside the daemon. Five JSON
files, a publish step, `contents: write` and a commit on every push to main is a
lot of apparatus for four integers. Codecov gives four direct URLs and hosts
nothing in this repository, so the publisher and its ten tests are deleted
(RULE NDC) rather than extended.

- **Dimension 6.1** — DONE —  the README carries one badge per measured surface — `zig`, `app`, `website`, `cli` — each a direct Codecov URL with no repository-hosted state behind it → Test `test_every_readme_flag_has_an_upload` (pending the workflow edit)
- **Dimension 6.2** — BLOCKED (approval-gated `.github/workflows/test.yml` edit + `CODECOV_TOKEN`) —  the Zig badge equals the number the gate enforced, because CI uploads `coverage/zig/merged/cobertura.xml` — the union with this spec's denominator rules already applied — and never the raw per-component kcov reports, which would let Codecov build its own union roughly two points higher → Test `test_zig_upload_names_the_merged_report`
- **Dimension 6.3** — BLOCKED (same edit) —  every flag the README names has an upload step that produces it, so no badge can render `unknown`, and the row stays one coherent row beside the Continuous Integration (CI), Zig, Docs and License badges → Test `test_readme_badge_row_is_well_formed`

### §7 — The one-shot M164 playbook leaves the tree

`playbooks/operations/m164_free_trial_removal/` holds `apply.sql` and `verify.sql`, a hand-migration written for databases that are not rebuilt from the schema slots. M164_001 shipped and the migration has been applied, so the folder is a spent artefact sitting beside durable operator procedures. Nothing references it but M164_001's own Files Changed record, which is history and stays. **Implementation default:** delete both files with the folder; no deprecation note, no archive copy — the content is recoverable from git history and RULE NDC forbids keeping dead material for reassurance.

- **Dimension 7.1** — DONE —  the folder is gone from disk and from git, and `make check-playbooks` still passes → Test `test_no_m164_playbook_remains`

## Interfaces

```
.tmp/zig-coverage.txt — key/value surface read by the CI summary step.
Existing keys preserved verbatim; new keys additive.

  zig_line_coverage_pct=<float>              # existing
  zig_line_coverage_min_pct=<int>            # existing
  zig_line_coverage_target_pct=<int>         # new — 91, fixed
  zig_line_coverage_gap_pts=<float>          # new — target minus measured, 0 when met
  zig_measured_files=<int>                   # new
  zig_measured_lines=<int>                   # new
  zig_folder_pct_<name>=<float>              # new — per product folder
  zig_folder_min_pct_<name>=<int>            # new — enforced floor
  zig_folder_target_pct_<name>=<int>         # new — 95 for agentsfleetd and runner
  zig_folder_gap_pts_<name>=<float>          # new

scripts/check_zig_coverage.py — argument surface gains, all optional so the
existing invocation keeps working:
  --min-files N              union denominator floor: measured files
  --min-lines N              union denominator floor: measured lines
  --component-min NAME=F,L   repeatable; per-component file and line minimums
  --require-root NAME        repeatable; product root that must be present
  --target-pct PCT           merged target (published, not enforced)
  --folder-floor NAME=PCT    repeatable; per-folder enforced floor
  --folder-target NAME=PCT   repeatable; per-folder target (published)
Exit 0 when every assertion passes; exit 1 naming the first breach with
measured and expected values. A floor above its own target is a usage error
and also exits 1. No other exit code.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Component degrades to a handful of lines | Toolchain or path resolution regresses short of total failure | Component minimum trips; failure names component, measured counts, and minimums; no rate is accepted |
| Product root absent from the union | A component drops out or resolves a different tree | Required-root assertion fails naming the absent root, independent of rate |
| Harness re-enters the denominator | A new test-support source with a naming form not yet enumerated | Excluded-form assertion fails naming the path, so the form list is extended rather than the number silently shifting |
| One folder regresses while the average holds | Daemon gains coverage as the runner loses it, or the reverse | Per-folder floor trips for the regressed folder, naming it distinctly from a merged breach |
| Floor set above its target | A floor edited past the destination | Usage error, exit 1, naming floor and target — the ratchet cannot overshoot |
| Target met but floor left behind | Coverage lands and nobody raises the floor | Gap publishes as 0 while the floor stays below; the ratchet rule in the architecture doc names raising it as the follow-up |
| Report unreadable or absent | Component failed before writing output | Existing `FileNotFoundError` path retained; absence is never treated as an empty pass |

## Invariants

1. No rate is graded before its denominator — the checker asserts component minimums, union minimums, and root presence before any percentage is compared to a floor. Enforced by ordering in `main` with an early return, plus a self-test that a degraded report fails on counts rather than on rate.
2. Every declared product root appears in the union — enforced by the required-root assertion; absence exits 1 regardless of rate.
3. No excluded test-support form contributes a measured line — enforced by `is_product_source` and a self-test asserting each enumerated form is absent from the union.
4. Every floor and target has exactly one definition site in `make/test.mk` — enforced by the checker accepting both only as arguments, so no recipe or Python module can hold a second copy.
5. No enforced floor exceeds its own target — enforced by an argument-validation check that exits 1, so the ratchet cannot be set past its destination.
6. Every published rate carries its floor, its target, and its gap — enforced by a single emitter that writes the four keys together or fails; there is no code path writing a rate alone.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | internal verification lane only; no user or operator surface emits anything new | N/A | N/A | N/A |

The CI job summary gains per-folder rows by reading additional keys from the existing `.tmp/zig-coverage.txt` surface, which requires no workflow change. No analytics or funnel playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_support_naming_forms_excluded` | report holding each enumerated form → none contributes a measured line |
| 1.2 | unit | `test_product_helpers_retained` | report holding the three product `*helpers*.zig` paths → all three measured |
| 1.3 | unit | `test_excluded_form_absent_from_union` | two components both reporting `http/test_harness.zig` → union holds zero lines for it |
| 2.1 | unit | `test_component_denominator_floor_enforced` | daemon component at 40 files against a 400-file minimum → exit 1 naming component, 40, and 400 |
| 2.2 | unit | `test_absent_product_root_fails_despite_high_rate` | union at 98.4% holding only `lib/` paths, roots `agentsfleetd` and `runner` required → exit 1 naming both |
| 2.3 | unit | `test_union_denominator_published` | passing run → summary file carries measured files and measured lines |
| 3.1 | unit | `test_floors_and_targets_defined_once` | grep `make/test.mk` → exactly one floor and one target definition per scope |
| 3.2 | unit | `test_folder_breach_names_folder_and_floor` | runner folder at 88% against a 90% floor → exit 1 naming `runner`, 88, and 90, distinct from a merged breach |
| 3.3 | unit | `test_enforced_floors_clear_measured_values` | each seeded floor against its measured value → floor lower or equal in all three scopes |
| 3.4 | unit | `test_gap_to_target_published_and_bounded` | daemon at 67.48% with target 95 → published gap 27.52; floor 96 with target 95 → exit 1 |
| 3.5 | unit | `test_merged_floor_clears_measured_figure` | merged floor against the measured merged rate → floor is the lower value |
| 4.1 | unit | runner file tests (per behaviour) | `runner/` unit-lane rate ≥ 95% measured by the gate over the product-only denominator |
| 4.2 | unit | daemon handler tests (per verb) | each targeted handler leaves 0%; success and failure halves asserted through the in-process harness |
| 4.3 | unit | store tests (per operation) | `session_store_redis.zig` and `fleet_events_store.zig` exercised against live datastores |
| 4.4 | unit | `test_enforced_floors_clear_measured_values` | re-graded after each ratchet: every floor at or below its newly measured value |
| 5.1 | unit | `test_architecture_doc_matches_gate_values` | grep `docs/architecture/testing.md` → zero matches for "five binaries", a 60% floor, or "non-empty Cobertura"; the component set and ratchet rule present |

Regression rows: the guards already on this branch must keep firing — `test_zero_contribution_component_still_fails`, `test_source_roots_still_normalised`, `test_union_report_still_written`. Idempotency: `test_checker_is_pure_over_repeat_invocation` — grading the same reports twice yields identical output and exit code.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Checker self-tests pass (§1–§3) | `python3 -m unittest discover -s scripts -t scripts -p 'check_zig_coverage_test.py'` | exit 0 | P0 | |
| R2 | The gate runs green end to end | `env -u AGENTSFLEET_API_URL make test-coverage-zig` | exit 0 | P0 | |
| R3 | Harness is out of the denominator (§1) | `python3 -c "import xml.etree.ElementTree as E;print(sum(1 for c in E.parse('coverage/zig/merged/cobertura.xml').getroot().iter('class') if 'test_harness' in (c.get('filename') or '') or 'test_fixtures' in (c.get('filename') or '')))"` | `0` | P0 | |
| R4 | Every product root is measured (§2) | `python3 -c "import xml.etree.ElementTree as E;f={c.get('filename') for c in E.parse('coverage/zig/merged/cobertura.xml').getroot().iter('class')};print(sum(any(p in x for x in f) for p in ('agentsfleetd/','runner/','lib/')))"` | `3` | P0 | |
| R5 | Per-folder rates, floors, targets and gaps published (§3) | `grep -cE '^zig_folder_(pct\|min_pct\|target_pct\|gap_pts)_(agentsfleetd\|runner)=' .tmp/zig-coverage.txt` | `8` | P0 | |
| R6 | Targets are 91 / 95 / 95 (§3) | `grep -E '^zig_(line_coverage\|folder)_target_pct' .tmp/zig-coverage.txt \| sed 's/.*=//' \| sort -n \| tr '\n' ' '` | `91 95 95 ` | P0 | |
| R7 | Architecture doc matches the gate (§4) | `grep -ciE 'five binaries\|floor is 60%\|non-empty Cobertura' docs/architecture/testing.md` | `0` | P0 | |
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
| single-floor grading path | `grep -c 'is below threshold' scripts/check_zig_coverage.py` | 1 match, inside the per-scope grader |
| stale architecture text | `grep -ciE 'five binaries\|non-empty Cobertura' docs/architecture/testing.md` | 0 matches |

## Out of Scope

- Raising any floor beyond the value the newly landed tests measurably clear. Floors ratchet with evidence in the same commit, never ahead of it.
- `.github/workflows/test.yml` changes. The summary step already reads `.tmp/zig-coverage.txt`, so new keys surface without touching a workflow, and workflow edits are approval-gated.
- The app, website, agentsfleet, and design-system coverage gates. Different tooling, separate floors, untouched by this diff.
- Re-examining the union that replaced `kcov --merge`. Already landed on this branch and verified; it is read-first context, not scope.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an engineer reads the gate output, sees `agentsfleetd 67.48% (floor 67, target 95, gap 27.52)` beside the runner's own row, and knows which folder to work on without downloading an artifact or parsing Extensible Markup Language (XML) by hand.
2. **Preserved user behaviour** — `make test-coverage-zig` keeps its name, its exit-code meaning, and every existing key in `.tmp/zig-coverage.txt`; the new checker arguments are optional, so the current invocation and the CI summary step keep working unchanged.
3. **Optimal-way check** — the unconstrained-optimal shape floors every file individually, catching one regressed file rather than one regressed folder. Per-folder floors are the direct fix for two trees averaging into one number; per-file floors are a larger design this evidence does not yet justify.
4. **Rebuild-vs-iterate** — iterate. The union, the source-root normalisation, and the zero-contribution guard are all correct and recent; the gap is granularity and a filter that is too narrow. A rebuild would discard working determinism.
5. **What we build** — a widened test-support filter, per-folder rates with floors and targets, per-component denominator minimums, a required-root assertion, one emitter publishing rate/floor/target/gap together, and an architecture doc that matches.
6. **What we do NOT build** — per-file floors (see item 3); a coverage trend database; a PR comment bot; changes to the four package-level coverage gates; the follow-on unit-lane tests themselves.
7. **Fit with existing features** — compounds with the union already on this branch and with the reachability gate that floors test counts. It must not destabilise `make test-unit-all`, which reaches this target through `test-coverage-all`.
8. **Surface order** — N/A — no user surface. The consumers are the make gate and the CI job summary.
9. **Dashboard restraint** — N/A — no user surface. The summary shows only counts the checker measured, never a projected or interpolated figure.
10. **Confused-user next step** — the failure names the scope, the measured value, the floor, and the gap to target, which is the self-serve move: the engineer reads which folder fell short and by how much.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** four Sections ordered so the denominator is trustworthy before anything is graded against it. §1 removes harness from the measurement, §2 puts a floor under the report's shape, §3 makes floors bind per folder with targets visible, §4 stops the canonical doc from reintroducing the earlier state. One workstream on M164's existing branch, because the merged floor being reconciled is M164_001's own.
- **Alternatives considered:** (a) drop the merged floor to a passing value and land #601 with no per-folder signal — rejected, it leaves 22 points of daemon shortfall masked by the average and no way to see it; (b) per-file floors with a trend database — rejected as larger than the evidence supports, and named here rather than silently mud-patched toward.
- **Patch-vs-refactor verdict:** this is a **patch** because the checker's structure, union, and normalisation are correct — the filter is too narrow and the grading too coarse. A refactor would touch the parts that work.

## Resumed (Aug 15, 2026) — supersedes the park below

Resumed on Indy's direction the same day it was parked, with the scope widened:
**every one of `agentsfleetd/`, `runner/` and `lib/` reaches 95%**, and the
README carries the coverage figure an actual run produced.

> Indy (2026-08-15 17:40): "I think i want to be clear here that the quality
> rests on agentsfleetd/ , runner/ , and the lib/ folders so get your rear
> moving to fix them ultrathink to fix them and get us to 95% above for zig"

> Indy (2026-08-15 17:40): "The coverage badge must be what was executed and run
> and what we get in as a badge in README.md"

**The park's per-folder numbers were wrong, and the error mattered.** They were
macOS unit-lane figures taken before the debug-info repair, and they made the
daemon look far worse than it is. Re-measured on this branch over the union the
gate actually grades (unit lanes ∪ the live-datastore integration lane, seven
components on disk):

| Scope | Measured | Dark | Covered lines needed for 95% |
|---|---|---|---|
| union (current filter) | 90.25% — 28797/31907 over 565 files | 3110 | — |
| union (+ §1 filter) | 90.18% — 28334/31419 over 558 files | 3085 | 1515 |
| `agentsfleetd/` | 89.47% — 23452/26212 | 2760 | 1450 |
| `runner/` | 93.75% — 3738/3987 | 249 | 50 |
| `lib/` | 93.77% — 1144/1220 | 76 | 15 |

The spec's `agentsfleetd/ 67.48%` was the unit lane alone; the integration lane
covers the daemon heavily and the union is what the gate enforces. §1's harness
filter removes **488 lines across 7 files**, not the 788 across 17 the spec
claims — the remaining test-support spellings it names are already excluded by
kcov's `--exclude-pattern` or do not exist in the tree.

**What this changes about the plan.** §4 is no longer a tail on the gate work —
it is the bulk of it, and `runner/` and `lib/` are nearly there while the daemon
needs ~1,450 covered lines across a long tail. The dark mass is not four fat
files: the largest single file is `cmd/serve.zig` at 116 dark lines and 0%, and
the rest is ~60–100 files in the 20–50 dark-line band, most of them error arms
the suites never drive.

### The park it supersedes

The spec was parked earlier the same day after the coverage instrument was
repaired and the Pull Request (PR) went green — *"I prefer A"*, choosing the park
over finishing the remaining sections. **3 of 18 Dimensions had landed.** That
decision is superseded by the two quotes above; the record below stays because it
is still the accurate account of what shipped and what each unshipped section
leaves broken.

**What shipped.** The instrument, not the floors. Dimensions 2.3, 2.4 and 2.5:
the union grades what actually collected, states `measured over N of M
components` and names every component that captured nothing, and fails when a
required component contributes zero. Underneath it, the defect that made all of
this necessary — Zig 0.16's self-hosted backend emitting debug info libdw
rejects — is fixed at source, so Linux measures the whole codebase for the first
time. `ZIG_COVERAGE_REQUIRED_COMPONENTS` names all eight, so the six components
that were silently dark cannot go dark again without the gate saying so.

**What did not ship, and what each one leaves broken:**

| Section | Dimensions | What remains broken |
|---|---|---|
| §1 denominator holds product only | 1.1, 1.2, 1.3 | 788 lines of test harness across 17 files still count as product in the daemon lane. The gate is still satisfiable by writing more harness. |
| §2 shrinking component caught | 2.1, 2.2 | A component contributing a handful of lines still passes; only literal zero fails. No assertion that every product root is present. |
| §3 floors bind per folder | 3.1–3.5 | The section this spec is named after. One merged floor still averages `agentsfleetd/` and `runner/` together, so no floor can bind the daemon. |
| §4 lanes rise file by file | 4.1–4.4 | The coverage lifts themselves. Folded in on Indy's direction (quote in Discovery); unstarted. |
| §5 architecture doc | 5.1 PARTIAL | §Coverage records the eight-component set and the union; the per-folder floors and the ratchet rule are unwritten because §3 has not landed. |

**For whoever resumes.** §1, §2.1/2.2, §3 and §5 are one file plus its test file
(`scripts/check_zig_coverage.py`, `scripts/check_zig_coverage_test.py`) — bounded
and self-contained. §4 is a different animal: open-ended test-writing whose size
depends on measured dark lines, and Indy folded it in by direction, so splitting
or dropping it is his call. The per-folder floors this spec cites (67.48% daemon,
90.10% runner) are macOS measurements taken before the repair; re-measure on
Linux before seeding any floor from them.

## Measured outcome (Aug 15, 2026)

**The published rates fell, and coverage did not regress.** The denominator was
holding 5,309 lines of inline `test` blocks written inside product files — 17%
of it — and a test body is ~100% covered by construction (5,280 of the 5,309
were covered), so they lifted every rate. Removing them is the same rule that
already drops `*_test.zig` files; it reaches the blocks that live inside product
sources. It also closes the gate's own gaming vector: before this, adding an
inline test to a product file raised that file's rate directly.

| Scope | Before (flattered) | Honest | Floor | Target | Gap |
|---|---|---|---|---|---|
| merged | 90.14% | **88.24%** — 22744/25775, 531 files | 88 | 95 | 6.76 |
| `agentsfleetd/` | 89.41% | **87.71%** — 19410/22130, 440 files | 87 | 95 | 7.29 |
| `runner/` | 93.74% | **91.18%** — 2532/2777, 66 files | 91 | 95 | 3.82 |
| `lib/` | 94.24% | **92.40%** — 802/868, 25 files | 92 | 95 | 2.60 |

**After the `lifecycle` component** (8 of 8 components, every one collecting;
`make test-coverage-zig` green). Floors ratcheted to these values in the same
commit, each below its measured figure:

| Scope | Measured | Floor | Target | Gap |
|---|---|---|---|---|
| merged | **89.19%** — 22990/25775, 531 files | 89 | 95 | 5.81 |
| `agentsfleetd/` | **88.78%** — 19648/22130, 440 files | 88 | 95 | 6.22 |
| `runner/` | 91.18% — 2532/2777, 66 files | 91 | 95 | 3.82 |
| `lib/` | **93.32%** — 810/868, 25 files | 93 | 95 | 1.68 |

The `runner/` figure is unchanged because the boot sequence is the daemon's; the
`lib/` rise is `logging/mod.zig`, which the booted daemon exercises for real.

**A 99% target is not reachable on this instrument for every folder.** kcov
attributes no instructions to a function signature, a parameter line, a closing
brace or a comment, so those lines can never be marked covered by any test.
`src/lib/logging/mod.zig` is the proof: `mod_test.zig` demonstrably calls all
four scoped levels and asserts the output, yet lines 40-69 — the inline wrapper
signatures — read dark. Measured ceilings if every reachable line were covered:
`runner/engine` 99.38%, `lease` 99.69%, `redis` 99.45%, `postgres/db` 99.51%,
`agentsfleetd` 99.60%, `runner` 99.67%, **`lib` 97.05%**.

**Unit and integration overlap by 34.8%** — 10,814 of 31,079 lines were covered
by both lanes. Neither is redundant: the unit lanes alone measure 69.46%, the
integration lane alone 55.51%, the union 90.18%. Dropping integration would lose
6,439 lines; dropping the unit lanes would lose 10,773. The duplicated third is
the price of two lanes that each reach ~20-35 points nothing else does, and it is
why the lane is slow.

## Discovery (consult log)

- **The daemon's boot sequence was measurable all along.** `cmd/serve.zig` read
  0% of 116 lines, the largest single dark file in the tree by 2.3×, and the
  ranked plan called for writing a boot test. One already existed:
  `serve_lifecycle_integration_test.zig` drives the real `serve.run` against
  live datastores and asserts the whole boot → SIGTERM → drain choreography. It
  skipped, because it needs its own process — it installs signal handlers, binds
  a port and moves process-global state the other ~2000 integration tests read,
  and `make memleak` was the only lane isolating it. Running it as its own kcov
  component costs one rebuild (the integration binary takes its filter at build
  time) and buys **112/116 on `serve.zig`, 0% → 96.6%**, with the union at
  88.34% → 89.20%, +221 covered lines, no new test code. Fourteen files gained:
  `serve_shutdown` (14), `serve_background` (12), `serve_qstash` (7),
  `serve_boot` (5), and the reclaim (18), liveness (16) and repair-verification
  (15) sweepers. **Read the ranked dark list for a test that exists and skips
  before writing one.**

- **The lane self-tests and real runs were reading each other's output.** The
  coverage recipe wrote `.tmp/kcov-<component>.{log,rc}` at a hardcoded relative
  path. `check_zig_test_lanes_test.py` drives the *real* recipe with a stubbed
  kcov and redirects `ZIG_COVERAGE_DIR`, but not those — so a stubbed run
  truncated a real run's logs, and a real run's 57 KB log outlived a stubbed one
  and was read as its output, complete with NUL padding. Both then blamed the
  gate. The logs moved under `ZIG_COVERAGE_DIR`, beside the reports they
  explain; the summary file keeps its own variable because CI reads that exact
  path, so its default cannot move and only a test redirects it. Related and
  still true: that test class shells out to a full `make test-coverage-zig`,
  which is how a lane self-test can sit on a machine for nine hours.

- **A merge conflict marker shipped to the default branch.** `>>>>>>> origin/main`
  was appended to the end of a sentence in `docs/architecture/testing.md`, not
  at the start of a line, so every line-anchored grep and every pre-commit check
  looked straight past it. `test_the_doc_carries_no_conflict_marker` now looks
  for it anywhere in the file.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  > Indy (2026-08-15, this session): chose the live badge — an orphan `badges` branch fed by the graded run — over deleting the publisher, because a floor-backed badge is a hand-typed number and the requirement was the figure a real run produced. The `.github/workflows/test.yml` step remains approval-gated and is shown as a diff before it lands.
  > Indy (2026-08-15 17:52): "I suppose you havent over engineered to measure the floors, and made it stricter to expand and measure it later?" — context: he was right. Per-component denominator minimums (§2 Dimension 2.1) had been built: seven components × two counts, hand-maintained in `make/test.mk`. Two faults. They duplicated `--require-component`, which already fails a component contributing nothing, so the marginal catch was only "a component that half-collected". And being lower bounds on measured lines, they made the gate hostile to deletion — removing dead code shrinks the denominator and would have turned a good change red. Cut to one union-level pair (`--min-files 300`, `--min-lines 18000`, against 558 files / 31,419 lines measured), which still catches the collapse this lane was built for and leaves room to shrink the tree. Floors themselves were checked for the same fault and are not over-tight: every one is seeded below its measured value.
  > Indy (2026-08-14 14:04): "I think you have to go and the check the files in runner/ and agentsfleetd/ individually with lower coverage and improve the tests. You shouldnt just sit and check the mergeable changed or modified new file in the PR" — context: folds the unit-lane coverage lifts (formerly Out of Scope follow-on) into this workstream as §4; targets stay 91/95/95.
  > Indy (2026-08-15): "Make the union script grade honestly" — context: chosen over reverting the gate to the branch point or parking the check, after the measurement below showed the union could not publish at all on Linux.

- **Measurement that changed the design** — every per-folder number in this spec was taken on macOS, and the platform the gate runs on cannot reproduce them. kcov 43 collects the product line tables of only `runner` and `lib` on Linux; the other components yield a Cobertura report with no classes at all. It is a kcov defect, not a filter or path error, on three counts: a kcov run with **no** include or exclude filter returns nothing but `/opt/zig/lib/compiler_rt/*` for the affected binaries; `readelf` shows their product units carrying `DW_AT_comp_dir` values under `src/`, squarely inside the include path; and the same sources measure every component on macOS. The edge of the set also flickers: `deadline` returned nothing on three consecutive Continuous Integration (CI) runs and then 2 files on the fourth, from identical sources, so only `runner` and `lib` are required. The subset **flatters** — the first green run published 91.86% over 89 files where all seven macOS components measure 90.26% over 565 — which is why the pre-existing `kcov --merge` gate could report 93.70% and look healthy.

- **Root cause: Zig, not kcov.** Zig 0.16's self-hosted x86_64 backend emits DWARF 5 line programs libdw rejects — `dwarf_getsrclines` returns `invalid .debug_line section` for every Zig unit, and binutils reports bogus sibling markers over the same bytes. kcov skips failing units silently, so the symptom looked arbitrary. Only `compiler_rt` survived, the one DWARF 4 unit per binary. Isolated on a three-line Zig file: default backend → all units rejected; `-fllvm` → one clean unit, 102,585 lines.

- **Two dead ends, recorded so nobody retries them:** kcov `v44-pre-test3` behaves identically to v43 (already the current release), and elfutils 0.192 refuses the same bytes as 0.190. Neither could work — the defect is in the debug info, not the readers.

- **Fix:** every test binary sets `use_llvm` from one definition site, `shared.TEST_USE_LLVM`. Real kcov on real binaries: `logging` 0 → 7 product classes, `deadline` 0 → 8, `lib` 18 → 18, matching macOS. `build.zig` crossed the 350-line cap, so the incident-response bench block moved to `src/build/bench_incident.zig`.

- **Consequences for this spec:**
  - §2 Dimension 2.2 and §3's `agentsfleetd/` floors are unblocked — the daemon tree is measurable on Linux again. Neither is implemented yet.
  - `ZIG_COVERAGE_REQUIRED_COMPONENTS` **ratcheted to all eight on Linux**, in the commit the evidence arrived: job 94963891177 published `measured over 8 of 8 components — every component collected`, each carrying lines (agentsfleetd 26392, integration 23104, runner 4588, runner_integration 4136, deadline 307, lib 594, logging 276, s3 28). The flicker at the edge of the set is gone because its cause is gone.
  - **The first honest whole-codebase figure Linux has produced: 89.63%, 29089/32456 lines across 565 files.** The 91.86% it published before was 89 files — a flattering subset. The number fell because the denominator grew sixfold, not because coverage regressed.
  - §4's daemon coverage lifts become provable in CI, not macOS-only.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
  > Indy (2026-08-15): "I prefer A, and start a new session" — context: chooses the park over finishing §1, §2.1/2.2, §3, §4 and §5 on this branch. Offered against B (finish everything, including §4's open-ended lifts) and C (finish the gate logic, reassess §4). The spec stays in `active/`; §Parked records what each unshipped section leaves broken.
