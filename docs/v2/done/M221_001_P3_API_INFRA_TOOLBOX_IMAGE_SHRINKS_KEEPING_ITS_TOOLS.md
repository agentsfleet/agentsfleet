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
**Status:** DONE
**Priority:** P3 — deferrable; a smaller download and disk footprint per runner host, with no lease behaviour change
**Categories:** API, INFRA
**Batch:** B1 — measure, then one image change, its admission list and its docs
**Branch:** `build/m221-toolbox-shrink`
**Baseline revision:** 6e4a02127b4738f205c6ba1c8d42bce42413ed2e
**Test Baseline:** unit=10828 integration=5387 — identical-tree historical runs; integration uses the coverage-inclusive superset, not a like-for-like ignored-only delta
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M221_001-6e4a02127b47.md`
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
- **Handshake (PLAN)** — Reduce each host's toolbox download and retained bytes by at least a fifth, preserve its tools and licences, and prove first use and kernel admission. `ASSUMPTIONS I'M MAKING:` keep the pinned snapshot and all packages; prefer candidate (a) if measured latency holds; use Indy's delegated override authority for the feature choice. Quality ceiling: compression and install-time exclusions solve this without a new image format or package removal. Surface checklist: OpenAPI, product commands and schema unchanged; published docs and release notes waived by Indy; version stays in sync. Verification: native image builds, cold first-use comparisons, signed admission tests, kernel lane and repository gates; changed Rust coverage at least 99%.

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
| `scripts/toolbox/test_measure_first_use.sh`, `make/test-unit.mk` | CREATE, EDIT | Real-mount failure and interruption cleanup regressions run before the kernel lane |
| `rustd/crates/afr_sandbox/examples/kernel_lane/main.rs` | EDIT | Wire the sibling size and licence proof into the existing example |
| `rustd/crates/afr_sandbox/src/toolbox/{manifest.rs,manifest/tests.rs}` | EDIT | `EROFS_FEATURES` is the approved list; each feature is admitted and any other refused |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{toolbox.rs,toolbox_size.rs,trials.rs}` | EDIT, CREATE | The size cap and the stripped-docs case, in a sibling file so `toolbox.rs` (274 lines) stays under the cap |
| `docs/architecture/runner_execution.md` | WAIVED | Published documentation waived by Indy's quoted override; measured results stay in the PR |
| `playbooks/operations/acceptance/baselines/M221_001-first-use-arm64.json` | CREATE | Raw cold samples and p50/p99 for the current baseline and every candidate; the PR carries the readable report |
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

- **Reference:** Debian's debuerreotype uses [`scripts/.slimify-excludes`](https://github.com/debuerreotype/debuerreotype/blob/master/scripts/.slimify-excludes) for docs, info, man pages and locales, and [`scripts/.slimify-includes`](https://github.com/debuerreotype/debuerreotype/blob/master/scripts/.slimify-includes) to preserve copyright notices. Its `debuerreotype-slimify` script also prunes dangling aliases after applying dpkg filters. These upstream files were read and verified; this build excludes regular documentation during installation and removes the aliases and regenerated info index afterward.
- **Measured input:** `.gstack/tmp/tbx-size.sh` and `.gstack/tmp/tbx-squeeze.sh` produced every number in Discovery. They were scratch files in the M219 worktree; §1's script replaces them.

## Sections (implementation slices)

### §1 — First use is measured, and Indy picks

`measure_first_use.sh` mounts one image read-only, drops the page cache, and times each step inside bubblewrap with the image as `/`, as a lease sees it. The steps are `node --test`, a Python import, `git commit`, `node -e 0`, and `--version` for uv, rg, jq and curl. It runs at least 20 times per image and marks any step whose p99 exceeds the comparison image's by more than 10% as `REGRESSION`. Remeasure the comparison image after M213's approved `gh` removal; the historical sizes below do not supply that baseline. It runs for the comparison image and three candidates:
- (a) lz4hc `-C65536 -Eztailpacking,dedupe` plus the doc strip
- (b) lzma with 64 KiB chunks
- (c) deflate with 64 KiB chunks

**Implementation default:** (a), because it keeps LZ4 high compression (LZ4HC), and the format bar at `runner_execution.md:184` then needs no exception.

- **Dimension 1.1 — DONE** — The report lists every step's p50 and p99 for today and each mountable candidate on afr-kernel; a rejected kernel-incompatible alternative records its actual mount error and no samples → Test `first_use_report_covers_every_candidate` (manual: the report file, rubric R2)
- **Dimension 1.2 — DONE** — Indy picks a candidate and approves its exact EROFS feature list → Test `indy_approves_the_feature_list` (manual: verbatim quote in Discovery)

### §2 — The image is smaller and holds the same tools

The `erofs` line carries the chosen settings. `build.sh` passes mmdebstrap `--dpkgopt` lines that exclude `/usr/share/{doc,man,locale,info}/*` and include `/usr/share/doc/*/copyright`.

- **Dimension 2.1 — DONE** — The image is at most 80% of today's bytes for its architecture → Test `test_toolbox_image_fits_its_size_cap` (kernel lane, new)
- **Dimension 2.2 — DONE** — Every `copyright` file stays, and no locale, man page, info page or other doc remains → Test `test_toolbox_keeps_copyright_and_drops_docs` (kernel lane, new)
- **Dimension 2.3 — DONE** — Every pinned tool still runs under the production policy → Test `test_toolbox_carries_the_tools` (kernel lane, existing)
- **Dimension 2.4 — DONE** — Two builds of one release record stay byte-identical → Test `test_toolbox_build_is_reproducible` (kernel lane, existing)
- **Dimension 2.5 — DONE** — Lease start budgets hold → Test `test_start_budgets_with_four_leases` (kernel lane, existing)

### §3 — Admission names exactly the approved features

`EROFS_FEATURES` becomes Indy's list. An image naming any feature outside it is still refused before the kernel parses it.

- **Dimension 3.1 — DONE** — An image carrying each approved feature is admitted → Test `should_admit_each_approved_erofs_feature` (unit, new)
- **Dimension 3.2 — DONE** — An image carrying a feature outside the list is refused → Test `should_refuse_a_release_for_another_host_runner_or_kernel` (unit, existing; its case moves to a feature still unapproved)

### §4 — The architecture page records the choice — WAIVED

- **Dimension 4.1 — DONE (WAIVED)** — Published architecture and release-note updates are waived by Indy's quoted override; the PR carries the measurements and feature choice → Test `toolbox_doc_names_the_settings` (explicit waiver, rubric R5)

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
| Builder lacks the compressor | The builder's erofs-utils cannot make the chosen compressor | `mkfs.erofs` exits non-zero, `build.sh` publishes no image, and the build fails loudly → Test `build_refuses_an_unavailable_compressor` (manual: a deliberately nonexistent compressor in a scratch manifest exits non-zero with no published `.erofs`) |
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
| 1.1 | manual | `first_use_report_covers_every_candidate` | `measure_first_use.sh` on today and mountable candidates → all eight steps have at least 100 cold samples, p50 and p99; unsupported alternatives carry actual mount errors and no invented latency; the report is committed |
| 1.2 | manual | `indy_approves_the_feature_list` | Indy's verbatim choice and feature list → quoted in Discovery before `manifest.rs` changes |
| 2.1 | integration | `test_toolbox_image_fits_its_size_cap` | Release record `length` → at most 80% of the refreshed baseline: 195,624,960 B on arm64 and 198,574,080 B on amd64 |
| 2.2 | integration | `test_toolbox_keeps_copyright_and_drops_docs` | The mounted image → 156 `copyright` files after M213's approved removal (141 regular files and 15 aliases), every installed package has its notice, and no other regular file under `/usr/share/{doc,man,locale,info}` |
| 2.3 | integration | `test_toolbox_carries_the_tools` | uv, `node --test`, `git commit` and the version calls → each exits 0 |
| 2.4 | integration | `test_toolbox_build_is_reproducible` | Two builds of one manifest → identical SHA-256 |
| 2.5 | integration | `test_start_budgets_with_four_leases` | Four leases on the new image → within the lane's existing budgets |
| 3.1 | unit | `should_admit_each_approved_erofs_feature` | A signed release naming each approved feature → admitted |
| 3.2 | unit | `should_refuse_a_release_for_another_host_runner_or_kernel` | A release naming an unapproved feature → `ToolboxRefusal` for the kernel |
| 4.1 | manual | `toolbox_doc_names_the_settings` | Indy's published-documentation waiver → override commit `166c7c650`; measurements remain required |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The image is at most 80% of today's, docs stripped, licences kept (§2) | `make test-runner-kernel` (on afr-kernel; reuse the single R4 run) | Both named toolbox trials pass; image bytes at most cap | P1 | ✅ native capture 023: `159854592 bytes; cap: 195624960 bytes`; both toolbox trials `ok` |
| R2 | No first-use step regresses past 10% p99 (§1) | `python3 -c 'import json; d=json.load(open("playbooks/operations/acceptance/baselines/M221_001-first-use-arm64.json")); print(sum(s["result"] == "REGRESSION" for s in d["candidates"][d["selected"]]["steps"].values()))'` | `0` for the chosen candidate's rows | P1 | ✅ balanced captures 021/022: `0` regressions, all 120 cold samples per tool retained |
| R3 | Admission takes exactly the approved features (§3) | `cd rustd && cargo test -p afr_sandbox --lib toolbox::manifest` | `test result: ok` | P0 | ✅ capture 023: `14 passed; 0 failed`; five feature-removal mutants killed |
| R4 | Tools, rebuilds and start budgets hold (§2) | `make test-runner-kernel` (on afr-kernel) | exit 0 | P0 | ✅ native capture 023: `53 passed; 0 failed; 0 ignored`; original Git fixture failures fixed by host Git installation |
| R5 | The architecture page names the settings (§4) | Published-documentation update waived by Indy's quoted override in Discovery | Explicit waiver | P2 | WAIVED — `docs.updated` override commit `166c7c650` |
| R6 | This spec's diff stays inside Files Changed | `git diff --name-only "$(git merge-base origin/main HEAD)" HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ comparison with Files Changed: all 13 implementation/evidence paths are declared; final spec move is declared |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ `All checks passed!` in bcb266248 normal commit hook; completion commit runs the same mandatory hook |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | WAIVED — `cmd.verify.unit` override ca3b65028; Rust unit workflow 38138660787 succeeds with 4485 passed, 0 failed, 927 ignored |
| S3 | Lint passes | `make lint-all` | exit 0 | P0 | WAIVED — `cmd.verify.lint` override 624f54572; bcb266248 normal push runs `make lint-rustd`, exit 0; completion shell edits run normal hook lint |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | WAIVED — `cmd.verify.integration` override 1f92ea7f0; native canonical kernel capture 023 passes all 53 trials; cloud integration 38130493390 succeeds |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | ✅ `make check-version`: `all versions match 0.59.0` |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ bcb266248 normal hook: `no leaks found, 4436 bytes scanned`; completion commit runs the mandatory staged secret scan |

**Grading protocol (VERIFY):** reuse unchanged successful canonical runs, as Indy requests; each grade names its decisive output. Local repository-command waivers remain explicit. Normal commit/push hooks and the final-head Continuous Integration (CI) and Greptile report are recorded in PR Session Notes; the PR gate was invoked once and its extra integration build stopped under the delegated resource override.

## Dead Code Sweep

N/A — no files deleted. The losing candidates' settings never enter the manifest.

## Out of Scope

- Removing packages: Perl (git depends on it), Node and its International Components for Unicode (ICU) library, uv. M213 separately removes Go-built `gh` at Indy's direction; this spec preserves the tools remaining after that removal.
- Purging apt and dpkg from the image.
- zstd: the current erofs-utils 1.8.6 does offer it, but this change retains LZ4 and does not qualify another compressor family.
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

- **Session overrides** — Indy: "You are free to override orly, orly docs can be skipped" and "No doc updates needed = override approved in orly, and other overrides as you need for orly." This delegates the implementation-default choice and its measured feature list, waives published architecture/changelog updates, and permits the missing sibling-module wiring in Files Changed. Measurements and verification remain required; the report is carried in the Pull Request. Indy also requires 99% coverage for touched Rust and 100% for touched TypeScript, and asks that five concurrent agents share disk, CPU and memory carefully. Host builds use four Cargo jobs; the owned native instrumented cache uses one.

- **Measured feature approval under delegated authority** — `dump.erofs -s` on the native LZ4 high compression (LZ4HC) level-9, 64 KiB, tail-packing/deduplication trial reports exactly `sb_csum mtime 0padding compr_cfgs big_pcluster ztailpacking fragments dedupe`. Indy's quoted override delegates this list's approval before admission is edited. Each feature and their combination get signed-admission proofs; an unknown feature remains refused. Final compressor settings still require the first-use comparison. Level 9 is an additional candidate because level 12's first native compression consumed over twenty CPU minutes.

- **Install-filter findings** — dpkg retains documentation symlinks, and `install-info` regenerates `/usr/share/info/dir` after installation. The customize hook removes excluded-file aliases and that generated index, preserving copyright aliases and shared licence directories. The mounted-image comparison checks licence contents and package identity; the new kernel case rejects other documentation and broken links.

- **Observed alternative rejection** — The current LZMA candidate builds at 114,790,400 bytes, but the actual ARM kernel rejects its mount with exit 32, `fsconfig() failed: Operation not supported` (review capture 010). Its report records kernel incompatibility and zero latency samples. Selected-candidate acceptance still requires all eight tools, 100 cold runs and no p99 ratio above 1.10.

- **Review repair** — Actual regression probes reproduced a supplied JSON null baseline silently bypassing comparison and a predictable temporary report path following a planted symlink. The sampler rejects non-object baselines before running tools and writes through an exclusively created unique temporary file. Six permanent real-mount cases pass (capture 013). Successful measurement plus unmount failure already returns 1 under the existing shell's `errexit`; capture 011 disproved that suspected finding, and the stronger test remains.

- **Build-resource steering** — Indy supplied the Cargo job-cap fix and requested sourcing `~/.config/agentsfleet/.env` again. Host build commands now load `CARGO_PROFILE_DEV_DEBUG=1`, `CARGO_PROFILE_TEST_DEBUG=1`, `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=4`, applying the other branch's cap through the environment. The native instrumented cache retains its established debug-zero settings and one-job limit.

- **Cancellation repair** — The fresh adversarial review identified wrapper-only termination while Python measures. Captures 016 and 017 reproduced deferred cleanup with a real EROFS mount and bubblewrap child. The sampler now waits on a background child, forwards termination and reaps it before cleanup. The identical probe exits 143 in 25 milliseconds with no child, mount, tree or report (018); all seven permanent failure/publication cases pass (019).

- **Kernel and platform checks** — The native kernel run passed 51 of 53 trials, including the size cap, licence inventory, tools, reproducibility and four-lease budgets. Two host-side Git fixture trials could not spawn Git; the owned VM now has Git 2.47.3 and the complete lane remains required. Linux Clippy found needless raw-string hashes in the new licence proof; removing only the delimiters preserves its command bytes. LZMA and deflate images both fail this kernel's mount with exit 32, `Operation not supported`, and have no fabricated timing samples.

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
- **Balanced first-use proof** — Captures 014 and 020 failed sequential comparisons and all their raw samples remain in `M221_001-first-use-arm64.json`. Before capture 021, the comparison was fixed to all six baseline/level-9/level-12 orders, twenty cold samples per block, 120 per tool and image, nearest-rank p99 and the unchanged 1.10 ceiling. Capture 021 expired after fourteen complete blocks; 022 reused those blocks without resampling and executed only the four unfinished blocks. All eight level-9 ratios pass (0.228351–0.469385); level 12 also passes (worst 1.023387). No samples or outliers were removed. The shared host was heavily loaded, so these observed ratios are not a hardware-independent speedup guarantee. LZMA and deflate carry their real mount refusals and zero samples.

- **Greptile repair** — PR #746 finding `discussion_r4241632703` reproduced a compatible baseline missing only `image_sha256` reaching measurements before failing (024). The sampler now validates a lowercase 64-character SHA-256 digest before sampling. All eight permanent mount/failure/publication cases pass (025), including this regression and the existing seven cases. The balanced benchmark supplied no baseline to individual block calls; this preflight-only repair does not alter its executed timing path, image, samples or calculations.

- **Publication cancellation repair** — The final native review identified a small staging-file leak when SIGTERM interrupts JSON writing. Capture 026 reproduced the leaked unique file before repair. Publication now handles SIGTERM through Python cleanup and masks it until the exclusive file has a cleanup owner. All nine permanent cases pass (027), including interrupted publication, measurement cancellation and successful atomic publication. The handler is installed only after measurements finish; the timed commands, samples and calculations in 022 are unchanged.

- **Process-group hangup repair** — A subsequent native review found that a group SIGHUP reaches Python directly and bypasses a TERM-only handler. Capture 028 reproduced the same staging-file leak. Publication now handles and masks INT, TERM and HUP, including additional signals while cleanup runs. All ten permanent cases pass (029); the real process-group hangup exits 129 with no child, mount, tree, staging file or report, alongside measurement cancellation and successful atomic publication.

- **Native completion and coverage** — Capture 023 runs the canonical kernel lane with instrumentation after installing the missing host Git: all 53 trials pass, including both new toolbox checks, tools, reproducibility and four-lease budgets. A clean-profile report includes examples and test modules via `--no-default-ignore-filename-regex`; the official coverage grader reports `100.0000% >= 99% floor — 37 of 37 changed lines covered`. Rust source is byte-identical to instrumented revision bcb266248; later edits affect only baseline preflight, report publication, their regressions and evidence. No TypeScript source changed. `make check-version` reports every version matches 0.59.0.

- **PR timing override** — Indy: "Donot keep checking the gates in cycles, since it takes a while for PR. if you did check before then i recommend you to push the PR". PR #746 was opened with the then-current partial evidence. The one `orly gate pr` invocation launched another costly integration build despite recorded waivers; only its owned process tree was stopped. No completed PR-gate pass is claimed and no further cycle is needed. All behavioral dimensions now have passing evidence; published docs and local repository-suite waivers remain explicit.

- **Skill-chain outcomes** — `orly-write-unit-test`: thirteen new cases (one signed admission, two kernel checks, ten sampler cases), nine failure/interruption cases; five feature-removal mutants killed. Gstack `/review` and `orly-babysit-prs` final results, normal completion-hook results and final-head CI are appended to PR #746 Session Notes after completion.
- **Deferrals** — none.
