<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M221_001: The toolbox image shrinks by at least a fifth and keeps every tool

**Prototype:** v2.0.0
**Milestone:** M221
**Workstream:** 001
**Date:** Oct 10, 2026
**Status:** PENDING
**Priority:** P3 — deferrable; a smaller download and disk footprint per runner host, with no lease behaviour change
**Categories:** API, INFRA
**Batch:** B1 — measure, then one image change, its admission list and its docs
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** `M219_002` §6 merged (PR #739) — the toolbox builds from the Debian 13.7 snapshot `20261010T000000Z`, and every size here is measured on it
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 10, 2026) from Indy's Oct 10, 2026 questions on the image's size, with measurements taken that day
**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox

---

## Overview

**Goal (testable):** the toolbox image built from `scripts/toolbox/manifest.txt` is at most 80% of today's bytes for its architecture, every first-use step's p99 stays within 10% of today's, and `make test-runner-kernel` passes with the image admitted.
**Problem:** The toolbox is the read-only Enhanced Read-Only File System (EROFS) image every lease's sandbox mounts as `/`. It is 258,420,736 B (246.4 MiB) on arm64, from about 466 MiB of files. Two causes stack. `erofs -zlz4hc,12 -b4096` compresses in 4 KiB chunks, about 1.9 to 1. And the image carries 64 MiB of locales, docs and man pages that no tool reads.
**Solution summary:** Measure first-use latency for three candidates against today's image, then let Indy pick one and approve its EROFS feature list. Build with the chosen settings, and strip docs, man pages, locales and info at install time while keeping every `copyright` file. Widen admission to exactly the approved features, and record the change in the architecture page.

## PR Intent & comprehension handshake

- **PR title (eventual):** build: the toolbox image is a fifth smaller and carries the same tools
- **Intent (one sentence):** Each runner host downloads and keeps a smaller toolbox per release, and every tool a lease runs behaves and starts as it does today.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` §Toolbox, lines 164–186 — what the image holds, how it is built and admitted, and the format bar at line 184.
2. `scripts/toolbox/build.sh` — the mmdebstrap hooks, `BUILDER_PACKAGES` (line 63), and the release record's `length` and `erofs_features` (lines 177–213).
3. `rustd/crates/afr_sandbox/src/toolbox/manifest.rs` — `EROFS_FEATURES` (line 33), the admission allowlist.
4. `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox.rs` — the cases that run the toolbox's tools in a real sandbox.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `scripts/toolbox/manifest.txt` | EDIT | The `erofs` line carries the chosen compressor, chunk size and features |
| `scripts/toolbox/build.sh` | EDIT | Docs, man pages, locales and info are excluded at install, `copyright` files included; `BUILDER_PACKAGES` names a new compressor's library |
| `scripts/toolbox/measure_first_use.sh` | CREATE | First-use p50 and p99 per step for one image, cold cache, in a sandbox like a lease's |
| `rustd/crates/afr_sandbox/src/toolbox/{manifest.rs,manifest/tests.rs}` | EDIT | `EROFS_FEATURES` is the approved list; each feature is admitted and any other refused |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{toolbox.rs,toolbox_size.rs,trials.rs}` | EDIT, CREATE | The size cap and the stripped-docs case, in a sibling file so `toolbox.rs` (274 lines) stays under the cap |
| `docs/architecture/runner_execution.md` | EDIT | §Toolbox names the settings and features; a decision row records the choice and its numbers |
| `docs/v2/reviews/m221-toolbox-first-use.md` | CREATE | The first-use report for today's image and each candidate |
| `docs/v2/{pending,active,done}/M221_001_P3_API_INFRA_TOOLBOX_IMAGE_SHRINKS_KEEPING_ITS_TOOLS.md` | EDIT | This spec, moved by CHORE(open) and CHORE(close) |
| `playbooks/operations/acceptance/baselines/M221_001-<revision>.md` | CREATE | The baseline evidence the header promises, named for the comparison revision |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS (the size cap and feature names are named constants, never literals in a check); NDC (no candidate's settings survive beside the chosen one); NLR (the refused-feature test that names `ztailpacking` is rewritten if that feature is approved); FLL (the kernel-lane file split).
- `.orly/dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md` — admission keeps its existing refusal; no new error kind is expected.
- `.orly/dispatch/write_shell.md` — the measurement script quotes expansions and cleans its mount on every exit path.
- `.orly/docs/DOCUMENTATION_RULES.md` — the architecture page is published prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes — `kernel_lane/toolbox.rs` is 274 lines | New cases go in `kernel_lane/toolbox_size.rs` |
| UFS | yes | The byte cap, the baseline `copyright` count and each feature name are constants beside the test or allowlist that reads them |
| Security boundary (admission allowlist) | yes | Indy's explicit yes on the exact feature list, quoted in Discovery before `manifest.rs` changes |
| Continuous Integration (CI) workflow edit (`.github/workflows/**`) | no | The kernel job already builds from the manifest |

## Prior-Art / Reference Implementations

- **Reference:** Debian's official container images, built by debuerreotype, exclude docs, man pages and locales through dpkg `path-exclude` and keep each `copyright` file through `path-include`. The strip follows that list rather than deleting files after install, so dpkg's database matches the tree. unverified: the exact upstream file name for that list; reading debuerreotype's repository at PLAN settles it.
- **Measured input:** `.gstack/tmp/tbx-size.sh` and `.gstack/tmp/tbx-squeeze.sh` produced every number in Discovery. They were scratch files in the M219 worktree; §1's script replaces them.

## Sections (implementation slices)

### §1 — First use is measured, and Indy picks

`measure_first_use.sh` mounts one image read-only, drops the page cache, and times each step inside bubblewrap with the image as `/`, as a lease sees it. The steps are `node --test`, a Python import, `git commit`, `node -e 0`, and `--version` for uv, rg, jq and curl. It runs at least 20 times per image and marks any step whose p99 exceeds the comparison image's by more than 10% as `REGRESSION`. Remeasure the comparison image after M213's approved `gh` removal; the historical sizes below do not supply that baseline. It runs for the comparison image and three candidates:
- (a) lz4hc `-C65536 -Eztailpacking,dedupe` plus the doc strip
- (b) lzma with 64 KiB chunks
- (c) deflate with 64 KiB chunks

**Implementation default:** (a), because it keeps LZ4 high compression (LZ4HC), and the format bar at `runner_execution.md:184` then needs no exception.

- **Dimension 1.1** — The report lists every step's p50 and p99 for today and each candidate on afr-kernel → Test `first_use_report_covers_every_candidate` (manual: the report file, rubric R2)
- **Dimension 1.2** — Indy picks a candidate and approves its exact EROFS feature list → Test `indy_approves_the_feature_list` (manual: verbatim quote in Discovery)

### §2 — The image is smaller and holds the same tools

The `erofs` line carries the chosen settings. `build.sh` passes mmdebstrap `--dpkgopt` lines that exclude `/usr/share/{doc,man,locale,info}/*` and include `/usr/share/doc/*/copyright`.

- **Dimension 2.1** — The image is at most 80% of today's bytes for its architecture → Test `test_toolbox_image_fits_its_size_cap` (kernel lane, new)
- **Dimension 2.2** — Every `copyright` file stays, and no locale, man page, info page or other doc remains → Test `test_toolbox_keeps_copyright_and_drops_docs` (kernel lane, new)
- **Dimension 2.3** — Every pinned tool still runs under the production policy → Test `test_toolbox_carries_the_tools` (kernel lane, existing)
- **Dimension 2.4** — Two builds of one release record stay byte-identical → Test `test_toolbox_build_is_reproducible` (kernel lane, existing)
- **Dimension 2.5** — Lease start budgets hold → Test `test_start_budgets_with_four_leases` (kernel lane, existing)

### §3 — Admission names exactly the approved features

`EROFS_FEATURES` becomes Indy's list. An image naming any feature outside it is still refused before the kernel parses it.

- **Dimension 3.1** — An image carrying each approved feature is admitted → Test `should_admit_each_approved_erofs_feature` (unit, new)
- **Dimension 3.2** — An image carrying a feature outside the list is refused → Test `should_refuse_a_release_for_another_host_runner_or_kernel` (unit, existing; its case moves to a feature still unapproved)

### §4 — The architecture page records the choice

- **Dimension 4.1** — §Toolbox names the compressor, chunk size, features and doc strip, and a decision row carries the sizes and first-use numbers → Test `toolbox_doc_names_the_settings` (grep, rubric R5)

## Interfaces

```
No interface changes: no route, command, flag or wire shape moves.
The release record keeps its fields; `erofs_features` lists more names.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| First use regresses | A candidate's p99 for any step exceeds today's by more than 10% | The candidate is not offered as a default; the report marks the step `REGRESSION` → Test `first_use_report_covers_every_candidate` |
| Unapproved feature | A build's image names a feature outside `EROFS_FEATURES` | Admission refuses the release before mounting it, and the host keeps its last admitted image → Test `should_refuse_a_release_for_another_host_runner_or_kernel` |
| Builder lacks the compressor | The builder's erofs-utils cannot make the chosen compressor (zstd failed this way on Oct 10) | `mkfs.erofs` exits non-zero, `build.sh` writes no image, and the build fails loudly → Test `build_refuses_an_unavailable_compressor` (manual: `erofs -zzstd` in a scratch manifest exits non-zero with no `.erofs` written) |
| Host kernel cannot mount | A runner host's kernel lacks the chosen feature or compressor | The mount fails, the host builds no sandbox and refuses every lease, per `runner_execution.md:164` → Test `test_unbuildable_sandbox_refuses_lease` (kernel lane, existing) |
| Licence notices lost | A strip pattern removes `copyright` files | The kernel-lane case fails on the count → Test `test_toolbox_keeps_copyright_and_drops_docs` |

## Invariants

1. **Admission accepts only the approved features** — enforced by the `EROFS_FEATURES` constant and the existing refusal in `manifest.rs`.
2. **The image stays under its cap** — enforced by a byte-cap constant the kernel lane checks on every run, so a later package addition that undoes the saving fails the lane.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes; the release record already carries `length` | not applicable | not applicable | not applicable | not applicable | `not_applicable` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | manual | `first_use_report_covers_every_candidate` | `measure_first_use.sh` on today and (a), (b), (c) → each step has p50 and p99 per image; the report is committed |
| 1.2 | manual | `indy_approves_the_feature_list` | Indy's verbatim choice and feature list → quoted in Discovery before `manifest.rs` changes |
| 2.1 | integration | `test_toolbox_image_fits_its_size_cap` | Release record `length` → at most 80% of today's: 206,736,588 B on arm64; amd64's cap set from CI's current record at §1 |
| 2.2 | integration | `test_toolbox_keeps_copyright_and_drops_docs` | The mounted image → 157 `copyright` files (today's arm64 count) and no other regular file under `/usr/share/{doc,man,locale,info}` |
| 2.3 | integration | `test_toolbox_carries_the_tools` | uv, `node --test`, `git commit` and the version calls → each exits 0 |
| 2.4 | integration | `test_toolbox_build_is_reproducible` | Two builds of one manifest → identical SHA-256 |
| 2.5 | integration | `test_start_budgets_with_four_leases` | Four leases on the new image → within the lane's existing budgets |
| 3.1 | unit | `should_admit_each_approved_erofs_feature` | A signed release naming each approved feature → admitted |
| 3.2 | unit | `should_refuse_a_release_for_another_host_runner_or_kernel` | A release naming an unapproved feature → `ToolboxRefusal` for the kernel |
| 4.1 | unit | `toolbox_doc_names_the_settings` | `git grep -c` for the chosen `erofs` line in `runner_execution.md` → at least 1 |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The image is at most 80% of today's, docs stripped, licences kept (§2) | `make test-runner-kernel 2>&1 \| grep -cE 'test_toolbox_(image_fits_its_size_cap\|keeps_copyright_and_drops_docs) \.\.\. ok'` (on afr-kernel) | `2` | P1 | |
| R2 | No first-use step regresses past 10% p99 (§1) | `grep -c REGRESSION docs/v2/reviews/m221-toolbox-first-use.md` | `0` for the chosen candidate's rows | P1 | |
| R3 | Admission takes exactly the approved features (§3) | `cd rustd && cargo test -p afr_sandbox --lib toolbox::manifest` | `test result: ok` | P0 | |
| R4 | Tools, rebuilds and start budgets hold (§2) | `make test-runner-kernel` (on afr-kernel) | exit 0 | P0 | |
| R5 | The architecture page names the settings (§4) | `git grep -c -e 'Eztailpacking' -e 'path-exclude' -- docs/architecture/runner_execution.md` | at least 1 per term for candidate (a) | P2 | |
| R6 | This spec's diff stays inside Files Changed | `git diff --name-only $(git log --diff-filter=A --format=%H -- 'docs/v2/*/M221_001_*.md' \| tail -1)..HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ plus one decisive output line. Repository-command rows point at the final `orly gate pr` results in PR Session Notes.

## Dead Code Sweep

N/A — no files deleted. The losing candidates' settings never enter the manifest.

## Out of Scope

- Removing packages: Perl (git depends on it), Node and its International Components for Unicode (ICU) library, uv. M213 separately removes Go-built `gh` at Indy's direction; this spec preserves the tools remaining after that removal.
- Purging apt and dpkg from the image.
- zstd: the builder's erofs-utils does not offer it, and it needs Linux 6.10 or later.
- Moving the snapshot: `M219_002` §6 owns the pin.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator's runner host pulls the next toolbox release in a third less time and keeps a third less on disk, and no lease notices.
2. **Preserved user behaviour** — Every tool, path, version and start budget a lease sees stays the same.
3. **Optimal-way check** — Compression settings and an install-time strip remove bytes no tool reads; cutting packages would remove tools, so it waits for Indy.
4. **Rebuild-vs-iterate** — Iterate: one manifest line, one build option set, one allowlist.
5. **What we build** — A first-use measurement, a smaller image, a widened admission list and its tests.
6. **What we do NOT build** — Package removals, a new compressor beyond erofs-utils' set, CI changes.
7. **Fit with existing features** — Compounds with the signed release record and admission; must not destabilize lease start.
8. **Surface order** — N/A — no user surface; operators see only the release's size.
9. **Dashboard restraint** — N/A — no dashboard change.
10. **Confused-user next step** — The architecture page's decision row states the sizes, the features and why.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Measure first (§1), then the image (§2), then admission (§3) and docs (§4), so no allowlist widens before Indy has the numbers.
- **Alternatives considered:**
  - lzma, up to 48% smaller: a format switch held to the bar at `runner_execution.md:184`, offered in §1 with its latency.
  - Deleting docs after install: rejected, because it leaves dpkg's database naming files that are gone.
  - Removing packages: out of scope above.
- **Patch-vs-refactor verdict:** this is a **patch** because the build, admission and lane are right in shape; only settings and one list change.

## Discovery (consult log)

- **Consults** — Oct 10, 2026: Indy asked "Why is the image size big? How can we bring down the size", then "write me a follow on prompt so let me decide to do it or not in reducing the image size", then "i recommend you to push this spec as well". Measured on afr-kernel, arm64, rebuilding today's tree:

  | Settings | Bytes | vs today |
  |---|---|---|
  | today, `-zlz4hc,12 -b4096` | 258,027,520 | — |
  | lz4hc, 64 KiB chunks | 220,008,448 | −14.7% |
  | today, docs stripped | 214,396,928 | −16.9% |
  | lz4hc, 64 KiB, `ztailpacking,dedupe` | 205,897,728 | −20.2% |
  | deflate, 64 KiB | 190,193,664 | −26.3% |
  | candidate (a) | 169,865,216 | −34.2% |
  | lzma, 64 KiB | 152,621,056 | −40.9% |
  | lzma, 1 MiB, `ztailpacking,dedupe` | 133,877,760 | −48.1% |

  The image held 157 `copyright` files and 4 symlinked doc directories. The largest contents were libnode, git, libicu, uv and gh, at 34 to 48 MiB each.
- **Fold or stand alone** — `M220_001` also touches the kernel-lane machine; this spec stands alone because its change ships in the image. Indy may fold it at CHORE(open).
- **Metrics review** — No analytics or funnel playbook update required: no product or operator signal changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
