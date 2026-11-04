<!--
SPEC AUTHORING RULES:
- The spec standard holds the authoring and proof rules.
- Body order is the implementing agent's read order.
- No time or effort estimates, completion percentages or assigned owners.
-->

# M221_002: The toolbox drops its package manager

**Prototype:** v2.0.0

**Milestone:** M221

**Workstream:** 002

**Date:** Nov 04, 2026

**Status:** PENDING

**Priority:** P3, reduce toolbox download and storage bytes

**Categories:** API, INFRA

**Batch:** B1, follows the completed image compression work

**Depends on:** M221_001, preserves its tools, compression settings and first-use limits

**Provenance:** Agent-generated from Indy's request and fresh package experiments

**Canonical architecture:** `docs/architecture/runner_execution.md`, Toolbox

---

## Overview

**Goal (testable):** A smaller toolbox has no apt, dpkg or commands from the passwd package, while Git and every M221 tool retain their first-use budgets.

**Problem:** The read-only toolbox carries programs for installing packages and managing accounts. Leases cannot use them to change the image.

**Solution summary:** Remove those packages after installation, retain Perl's Git dependencies, and extend the existing release manifest for scanning without dpkg.

**Design flow:** Install pinned packages → capture provenance and licences → remove named packages → capture remaining records → remove package-manager state → build image → scan and qualify → sign.

The scanner reads a temporary filesystem view containing the remaining package records. The shipped image contains no dpkg database.

## PR Intent & comprehension handshake

- **PR title (eventual):** Remove the toolbox package manager and qualify smaller Perl
- **Intent:** Operators download a smaller image while supported lease tools behave as before.
- **Authoring handshake:** Remove package-management payloads and qualify Perl for Git. ASSUMPTIONS I'M MAKING: package removal must preserve Python, licences and vulnerability detection.
- **Implementer handshake:** Record the same intent in Pull Request (PR) Session Notes before changing source.

## Implementing agent — read these first

1. `scripts/toolbox/build.sh` — pinned installation, provenance capture and image publication.
2. `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox_size.rs` — licence proof currently calls dpkg inside a lease.
3. `.github/actions/build-toolbox/action.yml` — mounted-image scanning and cache identity.
4. `scripts/toolbox/measure_first_use.sh` — eight cold-cache commands, comparison validation and report publication.
5. `docs/v2/done/M221_001_P3_API_INFRA_TOOLBOX_IMAGE_SHRINKS_KEEPING_ITS_TOOLS.md` — preserved behaviour and the original scope exclusions.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| This spec under `docs/v2/pending/`, then `active/` and `done/` | CREATE, MOVE | Record design, decisions and proof |
| `scripts/toolbox/build.sh` | EDIT | Run final package removal and publish retained inventory |
| `scripts/toolbox/manifest.txt` | EDIT | Pin removal set and retained Perl packages |
| `scripts/toolbox/finalize.py` | CREATE | Remove packages, preserve notices and collect final records |
| `scripts/toolbox/test_finalize.py` | CREATE | Test finalization input validation and reconciliation rules |
| `scripts/toolbox/test_finalize.sh` | CREATE | Prove real package operations and refusal before publication |
| `scripts/toolbox/scan.sh` | CREATE | Give Syft a temporary view containing package records |
| `scripts/toolbox/scan_inventory.py` | CREATE | Validate and materialize remaining package records |
| `scripts/toolbox/test_scan_inventory.py` | CREATE | Reject mismatched, oversized and malformed records |
| `scripts/toolbox/test_scan.sh` | CREATE | Prove scanner completeness and mount cleanup |
| `scripts/toolbox/compare_first_use.py` | CREATE | Assemble and grade matched samples using shared comparison rules |
| `scripts/toolbox/measure_first_use.sh` | EDIT | Import shared commands and comparison rules |
| `scripts/toolbox/test_measure_first_use.sh` | EDIT | Preserve sampler cancellation and publication proofs after extraction |
| `scripts/toolbox/test_compare_first_use.py` | CREATE | Reject missing steps, bad samples and incompatible comparisons |
| `.github/actions/build-toolbox/action.yml` | EDIT | Call the scanner helper and its fixture/cleanup suite |
| `.github/workflows/test-integration-rustd.yml` | EDIT | Qualify matched images in the existing hosted Linux kernel job and retain evidence |
| `make/test-unit.mk` | EDIT | Run Python suites in the runner unit shard and finalization proofs in the kernel target |
| `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox_size.rs` | EDIT | Check licences without dpkg and tighten the size cap |
| `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox_runtime.rs` | CREATE | Prove absent management tools and working Git dependencies |
| `rustd/crates/afr_sandbox/examples/kernel_lane/main.rs` | EDIT | Register the new proof module |
| `rustd/crates/afr_sandbox/examples/kernel_lane/trials.rs` | EDIT | Register runtime trials within the file limit |
| `rustd/crates/afr_sandbox/src/toolbox/manifest/tests.rs` | EDIT | Prove the enriched signed manifest retains admission rules |
| `playbooks/operations/acceptance/baselines/M221_002-first-use-amd64.json` | CREATE | Preserve all new samples and comparison results |
| `playbooks/operations/acceptance/baselines/M221_002-measurements.md` | CREATE | Record build, tool, scan and coverage evidence |
| `docs/architecture/runner_execution.md` | EDIT | Describe removal of package management and reduced Perl support |
| `/workspace/docs/runners.mdx` and `/workspace/docs/changelog.mdx` on their own branch | EDIT | Explain observable runtime limitations after the implementation merges |

The admission implementation needs no new filesystem feature or signature format. Its existing byte, digest, architecture and feature checks remain authoritative.

## Applicable Rules

- `spec.md`: SPEC-GOAL, SPEC-FILES, SPEC-PIN, SPEC-DIM, SPEC-TESTS, SPEC-INVARIANT, SPEC-GRADE.
- `tests.md`: TEST-OUTCOME, TEST-ERROR, TEST-NEG, TEST-REDGREEN, TEST-MUTATE.
- `code.md`: GRD, HLP, NLG, ORP.
- `languages/shell.md`: SH-QUOTE, SH-ARRAY, SH-TRAP, SH-EVAL.
- `languages/python.md`: PY-PARSE, PY-WITH, PY-BOUNDARY, PY-EXCEPT.
- `docs.md`: DOC-S6, DOC-02, DOC-03, DOC-43.
- Owner directives: Rust patch coverage at least 99%; TypeScript patch coverage 100%; no single-field tests.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `orly doctor` | Yes | Installed rules and review tools must match the repository |
| `orly gate work`, `verify`, `pr` | Yes | Preserve declared commands; record decisive output |
| Linux kernel lane | Yes | Run the canonical target, including systemd startup checks |
| Coverage | Yes | Measure changed Rust lines with the Linux kernel lane; no TypeScript source change planned |
| File and function length | Yes | Separate finalization, scanning and runtime proofs; keep trial registration within 350 lines |
| Secrets | Yes | Scan every commit without adding suppressions |

## Prior-Art / Reference Implementations

- `scripts/toolbox/build.sh` remains the only image builder; keep its snapshot, timestamps, compression and atomic publication.
- Debian dpkg performs package removal. Python's standard-library parsers read its records; no new dependency solver or file-ownership deletion algorithm.
- Syft 1.54.1's Debian cataloger reads package status, source versions and optional file metadata. Reuse it instead of writing a software bill of materials serializer.
- Linux OverlayFS gives the scanner a temporary view over the read-only image. Cleanup removes only paths this invocation created.
- `deploy/local/codex-runner.sh` supplies the repository's existing container delegation and kernel-device setup pattern.

## Sections (implementation slices)

### §1 — Remove runtime package management

Keep the current snapshot and compression settings. Add `perl-base` and `perl-modules-5.40` to the retained `packages` input.
The new `purge` input names exactly apt, dpkg, passwd, libapt-pkg7.0, sqv, debian-archive-keyring, perl and libperl5.40.
Reject duplicate names, a missing removal package, or overlap with required packages; do not recursively remove dependencies.

Run finalization after provenance capture and existing documentation pruning, inside mmdebstrap's disposable root, before its tar export.
Capture installed records, realized retained-owned files and copyright bytes before removal. The realized tree accounts for existing installation exclusions.
For existing regular files capture content hashes and modes; for links capture targets and resolved content where required by retained tools.

Shared paths stay when any retained package owns them. Preserve generated passwd/group identity files, live alternatives and shared licence texts.
Materialize a retained package's copyright bytes when removing its documentation owner would break an alias.
Capture these expectations before purge, independently of final status records.

Use dpkg's explicit dependency and essential-package overrides only in the disposable root. Purge the other packages first and dpkg last.
Read final status without invoking the removed manager; require the original installed set minus exactly the approved removal set.
Capture status, `.list` and `.md5sums` records for export, then remove only `/var/lib/dpkg`, `/var/lib/apt` and `/var/cache/apt` state.

Keep retained-owned configuration under `/etc/apt` and `/etc/dpkg`. Reconcile the realized retained payload again after cleanup, allowing only preserved-notice materialization and the named state removal.
Verify required tools and identities before writing the inventory commitment and exporting metadata. Any failure prevents both image and release-manifest publication.

- **Dimension 1.1** — Exact removal → `test_finalization_preserves_runtime_and_removes_management` (integration): the pinned root retains Python, Git, required Perl and account identities; removed payloads and state disappear.
- **Dimension 1.2** — Refuse incomplete finalization → `test_finalization_refuses_incomplete_removal` (integration): inject a failing dpkg operation through the builder hook; require its diagnostic and neither published artifact.
- **Dimension 1.3** — Preserve retained payload → `test_finalization_preserves_shared_licences_and_owned_files` (integration): compare hashes, modes and links; preserve notices and shared paths.
- **Dimension 1.4** — Refuse payload loss → `test_finalization_refuses_missing_retained_payload` (integration): remove Python, a retained-owned configuration file or a shared notice; require a named-path failure and no publication.

### §2 — Preserve scanning and admission

Reuse and filter the existing release JSON's `packages[]`; add the bounded scanner metadata described in Interfaces. No new release sidecar is permitted.
The trusted builder checks actual payload independently, then embeds one inventory digest in the image. The scanner verifies image identity and that commitment before reconstruction.
This protects against changed external records after building; it does not prove the builder trustworthy.

Mount the image read-only. Create a 32-mebibyte temporary filesystem containing OverlayFS upper, work and view directories, and reconstruct dpkg records only in that upper layer.
Syft scans the complete view, including vendored binaries and language packages. Keep the existing pinned scanners and fixable-Critical vulnerability gate.

Compare the unchanged and finalized trees: their Debian identities differ by exactly the approved removal set; retained source versions and vendored detections agree.
Compare the finalized tree with original records against its reconstructed view: all package records, file evidence and relationships agree.
Normalize only the scan-directory source identifier and relationship ordering; with one Grype database, vulnerability identities, severities, package identities and fixed versions must match.

The build action runs Python suites and the scanner fixture/cleanup suite after its existing scanner-install step, on cache hits too.
The runner unit shard runs Python suites without privileges; the kernel target runs `test_finalize.sh` with its existing builder prerequisites.
The explicit hosted qualification run uses the real baseline and candidate for full scanner equivalence, with the same pinned scanner-install action.
Scanner tests require Linux root, EROFS, OverlayFS, tmpfs, Syft and Grype; missing prerequisites fail explicitly.

Reuse the sampler's signal-forwarding and child-reaping pattern. After success or interruption, unmount the view, temporary filesystem and image before deleting invocation-owned paths.
A failed unmount returns failure, reports the retained path and preserves the mounted tree. Failed scans never publish a completed bill of materials.
Promotion later signs the same release JSON bytes through the existing process.

- **Dimension 2.1** — Preserve scanner results → `test_scan_preserves_remaining_package_identities` (integration): require both comparisons above, including equal Grype results with one database.
- **Dimension 2.2** — Refuse false inventory → `test_scan_refuses_missing_or_changed_inventory` (integration): malformed, duplicate, oversized or coherently omitted records fail before scanning and publication.
- **Dimension 2.3** — Refuse lost attribution → `test_scan_refuses_changed_source_or_file_evidence` (integration): unchanged package names with altered source versions or file records fail comparison.
- **Dimension 2.4** — Clean up safely → `test_scan_cleans_up_interrupted_mounts_and_children` (integration): inject signals after lower mount, overlay mount and scanner start; assert child reaping, loop detachment and path cleanup.
- **Dimension 2.5** — Preserve failed mounts → `test_scan_retains_tree_when_unmount_fails` (integration): inject unmount failure; require nonzero exit, retained mounted paths, diagnostic and no completed output.
- **Dimension 2.6** — Preserve admission → `should_admit_inventory_without_relaxing_identity_checks` (unit): enriched signed JSON passes; tampering, excessive bytes and unsupported features retain their existing refusals.

### §3 — Prove tools and licences inside leases

Read licence expectations from the adjacent release JSON, following the existing kernel toolbox helper. Replace the lease's dpkg-query call and fixed copyright count.
Check each retained package's actual copyright hash, broken links, empty notices and unwanted documentation inside the lease.

Use a separate fresh repository for each Git fixture and equivalent fixed inputs for baseline and candidate:

- Request-pull: create a base commit and a topic commit adding `note.txt`, publish to a local bare repository, and request that range. Assert the base/topic identities, topic subject and one-file summary.
- Filter-branch: rewrite two commits from author Before to After and remove `private.txt`, saving mappings in a state branch. Add a third commit and reload that state; assert changed identities, preserved old mappings, the new mapping, three After authors and no private file in rewritten trees.
- Git.pm: use `Git->repository` to read HEAD and tracked files; compare with the fixture's expected commit and file set.
- Bundled hook: execute `prepare-commit-msg.sample` on a message containing Git's help block; assert the help is removed, the message retained and the backup preserves the input. Load `JSON::PP` and `IPC::Open2`, used by `fsmonitor-watchman.sample`, without claiming Watchman itself is installed.

Keep existing uv installation, Python import, Node test, Git commit and version proofs. General Perl programs may lose native modules such as Digest::SHA.
A Git dependency failure blocks qualification and requires restoring its dependency; it cannot silently narrow supported Git behaviour.

- **Dimension 3.1** — Keep Git's Perl paths → `test_toolbox_git_uses_reduced_perl` (integration): execute all isolated fixtures above and assert outputs plus repository/file effects.
- **Dimension 3.2** — Remove management commands → `test_toolbox_has_no_package_or_account_manager` (integration): absolute and PATH invocations of apt, apt-get, dpkg, dpkg-query, useradd and chpasswd fail; root writes fail while workspace writes succeed.
- **Dimension 3.3** — Preserve notices → `test_toolbox_keeps_copyright_and_drops_docs` (integration): every retained notice matches its hash; no broken links or unwanted documentation remain.

### §4 — Measure and publish the result

Use the existing GitHub-hosted Linux kernel job. Add an optional manual-dispatch input, `toolbox_baseline_sha`, accepting only a full 40-character repository commit identifier.
For this work its value is `11883fdaba2197adf8931d334396ba8b3bb9eb5b`. An empty input preserves the ordinary job's complete existing checks.

An explicit qualification run builds that unchanged revision in a disposable detached checkout and builds the candidate on the same runner with identical builder versions.
Set `AFR_TOOLBOX_IMAGE` to that candidate for the full kernel/coverage target. Run scanner equivalence and matched first-use measurement on the same artifacts.
Retain both images, adjacent manifests, raw samples, comparison results, revision and host/builder identity, including failure evidence; no first-use sampling overlaps compilation.
Two candidate builds must have identical image digests. The grader reads both actual images and adjacent manifests, verifies hashes/lengths, and requires candidate bytes below baseline bytes.
The kernel size trial retains an architecture ceiling; tighten each ceiling only after measuring that architecture, preserving an explicit unmeasured status elsewhere.

Share the sampler's eight command definitions, nearest-rank percentile calculation and 1.10 p99 ratio rule with `compare_first_use.py`.
Its `measure` command declares twelve 20-sample blocks before execution: baseline, candidate, candidate, baseline, repeated three times.
Run each block through M221's sampler, retaining all 120 cold samples per command and image.

Write the envelope before sampling and update it atomically after every block. Preserve failures, incomplete blocks and previous attempts; no trimming or favourable-run selection.
The `grade` command recomputes percentiles from finite positive raw samples, checks actual counts and exact commands, and rejects summaries that disagree.
Both image identities must match the supplied artifacts; their manifests must agree on architecture, snapshot and builder versions.

Read historical `selected` from `M221_001-first-use-arm64.json`. A compatible historical host also requires each candidate p99 ratio against its selected image to be at most 1.10.
Incompatible history is explicitly `NOT_COMPARABLE`, with the architecture/kernel reason; it does not fail an otherwise complete fresh comparison.
Incompatible fresh baseline/candidate metadata always fails qualification.

Run `make test-runner-kernel` in full. Preserve four-lease observations and every existing asserted limit; observational percentiles do not imply an enforced threshold.
Document reduced Perl support. Close only with green Continuous Integration (CI), a successful explicit qualification run, resolved Greptile findings and the authorized merge.
Record the qualification run URL and tested image digest. Download its raw report into the acceptance baseline; verify later documentation-only commits leave the image digest unchanged.

- **Dimension 4.1** — Preserve latency → `first_use_report_preserves_m221_budgets` (integration): all eight fresh p99 ratios are at most 1.10; compatible historical ratios also pass, or history states its incompatibility.
- **Dimension 4.2** — Reject false evidence → `test_first_use_refuses_incomplete_or_mismatched_evidence` (unit): reject missing blocks/commands, nonfinite or nonpositive samples, false summaries, differing fresh hosts and wrong image identities.
- **Dimension 4.3** — Shrink reproducibly → `test_first_use_requires_smaller_candidate`, `test_toolbox_image_fits_its_size_cap` and `test_toolbox_build_is_reproducible`: compare actual baseline/candidate bytes, enforce the architecture cap and match two candidate digests.
- **Dimension 4.4** — Keep the lease path → `test_start_budgets_with_four_leases` (integration): the full canonical kernel lane passes and records its existing four-lease timing observations.

## Interfaces

`manifest.txt` gains one required `purge` line with the eight names in §1. Existing `packages` entries remain required runtime packages.
The release manifest keeps every existing identity field and filters `packages[]` to the final installed set, retaining each package's archive SHA-256 meaning.
Each package entry adds `copyright: {path, sha256}`, naming its retained notice's absolute path and byte digest.

`package_metadata` is base64 of deterministic gzip over UTF-8 JSON: `{"status":"Debian status text", "info":{"git.list":"file paths", "git.md5sums":"checksums"}}`.
Include complete final status and each retained package's existing `.list`/`.md5sums` records. Status retains Debian source names and versions; absent Source means the binary identity.
Require exact package/version/architecture agreement with `packages[]`, unique identities, installed status and one file list per retained package.

Canonical JSON uses Python's standard encoder with sorted keys, compact separators, ASCII escaping and no trailing newline. Gzip uses timestamp zero and no filename.
The encoded release stays at most 1,048,576 bytes; decoded metadata stays at most 4,194,304 bytes, 512 packages and 1,024 info entries.
Reject overflow while decompressing, duplicate keys/records, invalid text, unsupported names or paths before creating files.

Info keys must be retained package basenames, optionally architecture-qualified, ending only in `.list` or `.md5sums`; reject separators and parent traversal.
Materialize only `var/lib/dpkg/status` and `var/lib/dpkg/info/<validated basename>` in a new empty upper directory.
Notice paths must identify `/usr/share/doc/<package>/copyright`; hashes describe resolved bytes, including materialized notices.

The finalizer writes `/etc/toolbox-inventory.sha256`: lowercase SHA-256 plus newline over canonical JSON containing only `packages` and `package_metadata`.
The scanner checks that file from the read-only image before injecting records. A coherent omission from both external representations therefore fails with the original image.
Admission still verifies the existing signed manifest fields and byte limit; it need not parse compressed metadata.

The first-use envelope contains `baseline_revision`, artifact digests/lengths, and `attempts[]` with predeclared `order`, completed `blocks[]` and failure details.
Each block holds its role and the sampler's unmodified report; attempts are append-only. The current candidate must have exactly one complete qualifying attempt, with no failed attempt for that same digest hidden by selection.
The shared module exposes `measure BASELINE_IMAGE CANDIDATE_IMAGE REPORT HISTORICAL` and `grade REPORT HISTORICAL BASELINE_IMAGE CANDIDATE_IMAGE`.
`test_scan.sh fixtures` runs the bounded scanner and cleanup cases; `test_scan.sh images BASELINE_IMAGE CANDIDATE_IMAGE` additionally proves full image equivalence.
The hosted kernel workflow's explicit qualification input selects the second mode and retains its outputs with the measurement artifacts.

## Failure Modes

| Mode | Kind | Cause | Handling and observable result | Test |
|------|------|-------|--------------------------------|------|
| Required payload lost | failure | Purge or cleanup removes a retained file | Named-path refusal; neither release artifact published | `test_finalization_refuses_missing_retained_payload` |
| Partial purge | failure | A package operation fails | Failed operation reported; neither artifact published | `test_finalization_refuses_incomplete_removal` |
| Broken notice | failure | Shared copyright owner disappears | Restore captured bytes or fail without publication | `test_finalization_refuses_missing_retained_payload` |
| False inventory | security | Missing, altered or malformed external records | Commitment/schema refusal before scanning or output | `test_scan_refuses_missing_or_changed_inventory` |
| Lost attribution | failure | Source or file records are omitted | Full scan comparison fails despite matching names | `test_scan_refuses_changed_source_or_file_evidence` |
| Interrupted scan | failure | Signal at mount or scanner boundary | Reap child, unmount, detach loop, remove owned paths; no output | `test_scan_cleans_up_interrupted_mounts_and_children` |
| Failed unmount | failure | Mount remains busy | Fail, report retained path and preserve mounted tree | `test_scan_retains_tree_when_unmount_fails` |
| Git module unavailable | failure | Reduced Perl loses a reachable dependency | Git fixture fails; restore dependency before shipping | `test_toolbox_git_uses_reduced_perl` |
| Invalid measurement | failure | Missing samples, false summary or mismatched fresh host/image | Grader refuses qualification with the mismatch named | `test_first_use_refuses_incomplete_or_mismatched_evidence` |

## Invariants

1. Required tools survive: pre-removal realized payload and post-cleanup files agree before publication.
2. Retained records describe checked payload and remain bound to the image by its embedded commitment.
3. The release stays within admission's encoded bound; reconstruction enforces separate decoded bounds.
4. Qualification rejects first-use p99 ratios above 1.10 for any of the eight commands, using complete artifact-bound samples.

## Metrics & Observability

N/A: no product event or metric changes. Release length, digest and existing kernel-lane measurements provide operator evidence.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|-----------|--------|----------|----------|-----------------|
| R1 | Management payload absent; tools, Git and licences work | `make test-runner-kernel` | All trials pass; no skips | P0 | |
| R2 | Candidate smaller and reproducible | R3 grader plus `make test-runner-kernel` | Actual candidate smaller than fresh baseline; cap and reproducibility trials pass | P0 | |
| R3 | First-use budget preserved | `python3 scripts/toolbox/compare_first_use.py grade REPORT HISTORICAL BASELINE_IMAGE CANDIDATE_IMAGE` | Eight passing ratios, exact artifact identities and explicit historical status | P0 | |
| R4 | Scan preserves retained evidence | `sudo -E bash scripts/toolbox/test_scan.sh images BASELINE_IMAGE CANDIDATE_IMAGE` with pinned scanners | Full records, relationships, vulnerability matches and cleanup cases pass | P0 | |
| R5 | Signed admission unchanged | `make test-unit-rustd-runner` | Runner and Python unit suites pass | P0 | |
| R6 | Rust patch coverage | `make test-coverage-runner-kernel` and existing coverage merge job | Same revision; Rust patch at least 99%; no TypeScript source diff | P0 | |
| S1 | Repository lint | `make lint-all` | exit 0 | P0 | |
| S2 | Repository unit tests | `make test-unit-all` | exit 0 | P0 | |
| S3 | Repository integration tests | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Versions agree | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

`REPORT` is `playbooks/operations/acceptance/baselines/M221_002-first-use-amd64.json`; `HISTORICAL` is the existing M221_001 arm64 report.
Record both actual image paths, manifests, revision and decisive outputs in `M221_002-measurements.md`; placeholders are command arguments, never substitute evidence.

## Dead Code Sweep

The [measurement record](../../../playbooks/operations/acceptance/baselines/M221_002-measurements.md) classifies all 45 initial matches at `11883fdab`.
Only the licence test invokes dpkg inside a lease; no production lease command references apt, dpkg, useradd or chpasswd.
User-provided programs can attempt absent commands; supported repository workflows must not require them.

Remove the lease-side dpkg query and fixed copyright count. Keep builder-host package commands and historical references, whose roles differ.
Repeat repository-wide word-boundary searches for apt, dpkg, useradd and chpasswd. Classify host setup, build, test, documentation and runtime hits separately.

## Out of Scope

- Removing Python, Node, uv, Git, ripgrep, jq, curl, Bash or their required runtime libraries.
- Removing login, mount or shared authentication libraries beyond the named removal set.
- A custom Git build, a new compressor, a snapshot refresh or support for arbitrary Perl programs.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator downloads a smaller toolbox and runs the same supported Git and language-tool workflows.
2. **Preserved user behaviour** — M221's tools, lease isolation and first-use limits hold; only the approved general Perl reduction changes language support.
3. **Optimal-way check** — Reuse dpkg and Syft while moving installation records out of the runtime image.
4. **Rebuild-vs-iterate** — Extend the existing builder and kernel lane.
5. **What we build** — Package removal, inventory-backed scanning, Git qualification and measured image reduction.
6. **What we do NOT build** — A replacement package manager, Git implementation or general dependency solver.
7. **Fit with existing features** — Preserve signed admission and vulnerability gates.
8. **Surface order** — Build and runtime checks first; publish the supported tool limits with the release.
9. **Dashboard restraint** — N/A: no dashboard change.
10. **Confused-user next step** — Consult the runner tool list and use a supported runtime instead of a removed Perl module.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Package finalization, scanning, lease proofs and measurements form four ordered Sections.
- **Ordinary apt purge:** Rejected because the measured removal plan also deletes Python.
- **Hand-deleting package files:** Rejected because shared-file ownership and stale metadata need a second removal implementation.
- **Perl-free Git:** Rejected because the current request-pull command fails without the interpreter.
- **Perl-base alone:** Rejected because it omits module files used by Git helpers; retain the packaged modules while qualifying removal of native extensions.
- **Patch-vs-refactor verdict:** A patch to the existing builder and proof paths. Record any crate reduction or race removal if implementation produces one.

## Discovery (consult log)

- **Scope change:** M221_001 excluded apt and dpkg removal. Indy now requests: "Goal: the toolbox Enhanced Read-Only File System (EROFS) image drops the package manager."
- **Database rationale:** M221 rejected deleting installed files while retaining records naming them. This work removes the manager and database together, retaining remaining-package records beside the image.
- **Perl decision:** Indy: "Qualify a smaller Perl runtime for Git; allow reduced general Perl support (Recommended)".
- **Source revision:** `11883fdab`; fresh origin/main fetch found M221_002 unused.
- **Fresh baseline:** The unchanged amd64 build produces 162,004,992 bytes, digest `387fa1f1b6b4cbe686d78722e7a4cdb52926ddd26720e67a78ede1e22509f430`.
- **Measured package behaviour:** Apt's purge simulation removes Python. Explicit dpkg removal of perl and libperl5.40 preserves four targeted Git checks but removes Digest::SHA.
- **Evidence:** [M221_002-measurements.md](../../../playbooks/operations/acceptance/baselines/M221_002-measurements.md) records package sizes, the unchanged build, full scanner equivalence, Perl experiments and the classified reference sweep.
- **Remaining proof:** The final implementation still needs image, Git, latency, reproducibility, full kernel and coverage qualification.
- **Model reviews:** Six brainstorm lenses and nine formal reviewers completed. Formal review produced eight Major actions and five Nits; all were accepted and incorporated into interfaces, proofs and wiring.
- **Qualification host decision:** Indy selected "GitHub-hosted Linux runner (Recommended)" after the cloud host failed kernel prerequisites. The existing kernel workflow will supply full qualification; OrbStack access is unnecessary for this route.
- **Open blockers:** Complete the hosted qualification and all remaining proofs. No failed prerequisite is skipped or recorded as passing.
- **Deferrals and transfers:** None.
