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

**Status:** IN_PROGRESS

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

**Solution summary:** Remove the named packages and Perl after installation, qualify stock Git for supported workflows, and extend the existing release manifest for scanning without dpkg.

**Design flow:** Install pinned packages → capture provenance and licences → remove named packages → capture remaining records → remove package-manager state → build image → scan and qualify → sign.

The scanner reads a temporary filesystem view containing the remaining package records. The shipped image contains no dpkg database.

## PR Intent & comprehension handshake

- **PR title (eventual):** Remove the toolbox package manager and Perl
- **Intent:** Operators download a smaller image while supported lease tools behave as before.
- **Authoring handshake:** Remove package-management payloads and Perl while qualifying stock Git. ASSUMPTIONS I'M MAKING: package removal must preserve Python, licences and vulnerability detection.
- **Implementer handshake:** Remove the eleven named packages after provenance capture, preserve supported tools, keep only build.sh and manifest.txt in scripts/toolbox/, and prove smaller bytes and unchanged first-use limits on the hosted Linux runner. Record this intent in the implementation PR Session Notes.

## Implementing agent — read these first

1. `scripts/toolbox/build.sh` — pinned installation, provenance capture and image publication.
2. `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox_size.rs` — licence proof currently calls dpkg inside a lease.
3. `.github/actions/build-toolbox/action.yml` — mounted-image scanning and cache identity.
4. `playbooks/operations/acceptance/measure_toolbox_first_use.sh` — eight cold-cache commands, comparison validation and report publication.
5. `docs/v2/done/M221_001_P3_API_INFRA_TOOLBOX_IMAGE_SHRINKS_KEEPING_ITS_TOOLS.md` — preserved behaviour and the original scope exclusions.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| This spec under `active/`, then `done/` | EDIT, MOVE | Record current scope and proof |
| `scripts/toolbox/build.sh` | EDIT | Finalize the disposable root and publish retained inventory |
| `scripts/toolbox/manifest.txt` | EDIT | Name the exact eleven-package removal set |
| `deploy/baremetal/toolbox_test.sh` | DELETE | Owner explicitly requested deletion; remove its discovery entry automatically |
| `.github/actions/build-toolbox/action.yml` | EDIT | Scan a temporary overlay, preserving the read-only image |
| `scripts/toolbox/install-builder-tools.sh` → `.github/actions/build-toolbox/install-tools.sh` | MOVE | Keep the existing builder setup beside its action |
| `scripts/toolbox/measure_first_use.sh` → `playbooks/operations/acceptance/measure_toolbox_first_use.sh` | MOVE | Keep the existing sampler with acceptance evidence |
| `scripts/toolbox/test_measure_first_use.sh` → `playbooks/operations/acceptance/test_toolbox_first_use.sh` | MOVE | Preserve sampler failure and interruption tests |
| `.github/workflows/test-integration-rustd.yml` | EDIT | Qualify matched images in the existing hosted kernel job |
| `deploy/local/orbstack.sh`, `make/test-unit.mk`, `docs/development.md` | EDIT | Update callers of moved scripts |
| `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox_size.rs`, `toolbox.rs`, `tools.rs` | EDIT | Replace the lease-side dpkg licence check and prove Perl-free Git |
| `rustd/crates/afr_sandbox/src/toolbox/manifest/tests.rs` | EDIT | Preserve signed admission with enriched JSON |
| `playbooks/operations/acceptance/baselines/M221_002-first-use-amd64.json` | CREATE | Preserve actual matched samples |
| `playbooks/operations/acceptance/baselines/M221_002-measurements.md` | EDIT | Record build, scan, tool and qualification evidence |
| `docs/architecture/runner_execution.md` | EDIT | Describe runtime tool limits and external package records |
| `/workspace/docs/runners.mdx`, `/workspace/docs/changelog.mdx`, own branch | EDIT | Publish observable runtime limits after product merge |

No new source file belongs in `scripts/toolbox/`. Remove the seven implementation helpers added during the initial build; fold necessary logic into the existing builder, scanner support and tests.

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
| File and function length | Yes | Reuse existing files; keep source files within 350 lines and functions within 50 lines |
| Secrets | Yes | Scan every commit without adding suppressions |

## Prior-Art / Reference Implementations

- `scripts/toolbox/build.sh` remains the only image builder; keep its snapshot, timestamps, compression and atomic publication.
- Debian dpkg performs package removal. Native packaging tools read its records; no new dependency solver or file-ownership deletion algorithm.
- Syft 1.54.1's Debian cataloger reads package status, source versions and optional file metadata. Reuse it instead of writing a software bill of materials serializer.
- Linux OverlayFS gives the scanner a temporary view over the read-only image. Cleanup removes only paths this invocation created.
- `deploy/local/codex-runner.sh` supplies the repository's existing container delegation and kernel-device setup pattern.

## Sections (implementation slices)

### §1 — Remove runtime package management

Keep the current snapshot, compression settings and required packages.
The `purge` input names exactly apt, dpkg, passwd, libapt-pkg7.0, sqv, debian-archive-keyring, perl, libperl5.40, perl-base, perl-modules-5.40 and liberror-perl.
Reject duplicate names, a missing removal package, or overlap with required packages; do not recursively remove dependencies.

Run finalization after provenance capture and existing documentation pruning, inside mmdebstrap's disposable root, before its tar export.
Capture installed records, realized retained-owned files and copyright bytes before removal. The realized tree accounts for existing installation exclusions.

Shell functions use dpkg for removal, GNU tar for retained payload comparison, and jq for JSON.
Capture retained regular files, modes and links in a temporary tar snapshot. Capture copyright bytes separately; compare those bytes after restoring any notice alias.

Shared paths stay when any retained package owns them. Preserve generated passwd/group identity files, live alternatives and shared licence texts.
Materialize a retained package's copyright bytes when removing its documentation owner would break an alias.
Capture these expectations before purge, independently of final status records.

Use dpkg's explicit dependency and essential-package overrides only in the disposable root. Purge the other packages first and dpkg last.
Read final status without invoking the removed manager; require the original installed set minus exactly the approved removal set.
Capture status and retained package info records, including file lists, checksums and control files for export.

Remove only `/var/lib/dpkg`, `/var/lib/apt`, `/var/cache/apt` and `/var/lib/perl` state. Disable mmdebstrap's later apt cleanup because it recreates the removed state.

Keep retained-owned configuration under `/etc/apt` and `/etc/dpkg`. Reconcile the realized retained payload again after cleanup, allowing only preserved-notice materialization and the named state removal.
Verify required tools and identities before writing the inventory commitment and exporting metadata. Any failure prevents both image and release-manifest publication.

- **Dimension 1.1** — Exact removal → `test_finalization_preserves_runtime_and_removes_management` (integration): the pinned root retains Python, Git and account identities while removing Perl; removed payloads and state disappear.
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

The existing build action reconstructs scanner records with GNU tar, coreutils and jq. No scanner program is added to the image and no new scanner helper is introduced.
Image comparison uses the real baseline and candidate with the existing pinned scanners. The existing build action remains the release scanning path.
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

Qualify Debian's existing Git executable without Perl. Use fixed identities, a writable workspace and local bare remotes.
Exercise init, clone, add, commit, diff, branch, switch, merge, rebase, fetch, pull, push, config, status, log, show, tag, worktree, conflict/abort, interactive staging, gc, fsck, Unicode names and executable files.
Assert output and repository/file effects. Do not claim external-network coverage from a local-remote test.

Keep existing uv installation, Python import, Node test, Git commit and version proofs.
General Perl programs and optional Perl helpers such as git send-email, git svn, git request-pull and git filter-branch are unsupported after the owner scope change.
Do not replace Git with an incomplete alternative or introduce a custom Git build when the installed executable passes these workflows.

- **Dimension 3.1** — Qualify Perl-free Git → existing `test_toolbox_can_run_real_tools` and expanded Git fixture (integration): core workflows pass with no Perl executable or installed Perl package.
- **Dimension 3.2** — Remove management commands → expanded existing toolbox trial (integration): apt, apt-get, dpkg, dpkg-query, useradd and chpasswd are absent; root writes fail while workspace writes succeed.
- **Dimension 3.3** — Preserve notices → `test_toolbox_keeps_copyright_and_drops_docs` (integration): every retained notice matches its manifest hash; no broken links or unwanted documentation remain.

### §4 — Measure and publish the result

Use the existing GitHub-hosted Linux kernel job. Add an optional manual-dispatch input, `toolbox_baseline_sha`, accepting only a full 40-character repository commit identifier.
For this work its value is `11883fdaba2197adf8931d334396ba8b3bb9eb5b`. An empty input preserves the ordinary job's complete existing checks.

An explicit qualification run builds that unchanged revision in a disposable detached checkout and builds the candidate on the same runner with identical builder versions.
Set `AFR_TOOLBOX_IMAGE` to that candidate for the full kernel/coverage target. Run scanner equivalence and matched first-use measurement on the same artifacts.
Retain both images, adjacent manifests, raw samples, comparison results, revision and host/builder identity, including failure evidence; no first-use sampling overlaps compilation.
Two candidate builds must have identical image digests. The grader reads both actual images and adjacent manifests, verifies hashes/lengths, and requires candidate bytes below baseline bytes.
The kernel size trial retains an architecture ceiling; tighten each ceiling only after measuring that architecture, preserving an explicit unmeasured status elsewhere.

Reuse M221's existing sampler unchanged in substance: eight commands, nearest-rank percentiles and the 1.10 p99 ratio rule.
Run 120 cold-cache samples per command for the fresh baseline, then 120 for the candidate with the baseline report supplied as the existing fourth argument.
Preserve all raw samples, failed reports and attempts; no trimming or favourable-run selection. Check both report identities against actual image hashes, lengths and manifests.
Read historical `selected` from `M221_001-first-use-arm64.json`. A compatible historical host also requires ratios at most 1.10 against that selection.
An amd64 hosted runner is not comparable to the historical arm64 host; record `NOT_COMPARABLE` explicitly and retain the fresh same-host comparison as evidence.

- **Dimension 4.1** — Smaller reproducible image → existing `test_toolbox_rebuild_is_identical` plus matched artifact measurements (integration): two candidate images agree and are smaller than the fresh baseline.
- **Dimension 4.2** — Preserve first use → existing sampler (acceptance): all eight p99 ratios are at most 1.10 with complete raw samples and matching artifact identities.
- **Dimension 4.3** — Complete Linux qualification → `make test-runner-kernel` and `make test-coverage-runner-kernel`: all trials pass on the hosted runner with no weakened prerequisite.

## Interfaces

`manifest.txt` gains one required `purge` line with the eleven names in §1. Existing `packages` entries remain required runtime packages.
The release manifest keeps every existing identity field and filters `packages[]` to the final installed set, retaining each package's archive SHA-256 meaning.
Each package entry adds `copyright: {path, sha256}`, naming its retained notice's absolute path and byte digest.

`package_metadata` is base64 of a deterministic gzip-compressed tar archive containing final `status`, retained `info/` records and `packages.json`. The latter must exactly equal the release JSON package list.
Include complete final status and each retained package's existing info records, including file lists, checksums and control files. Status retains Debian source names and versions; absent Source means the binary identity.
Require exact package/version/architecture agreement with `packages[]`, unique identities, installed status and one file list per retained package.

The tar archive fixes entry ordering, timestamps and numeric ownership. Gzip uses timestamp zero and no filename.
The encoded release stays at most 1,048,576 bytes; decoded metadata stays at most 4,194,304 bytes, 512 packages and 1,024 info entries.
Reject decoded overflow, duplicate archive entries, unsupported entry types and unsafe paths before creating scanner records.

Info keys must be retained package basenames, optionally architecture-qualified, ending in `.list`, `.md5sums`, `.conffiles`, `.postinst`, `.postrm`, `.prerm`, `.preinst`, `.templates`, `.triggers`, `.config`, `.shlibs` or `.symbols`; reject separators and parent traversal.
Materialize only `var/lib/dpkg/status` and `var/lib/dpkg/info/<validated basename>` in a new empty upper directory.
Notice paths must identify `/usr/share/doc/<package>/copyright`; hashes describe resolved bytes, including materialized notices.

The builder writes `/etc/toolbox-inventory.sha256`: lowercase SHA-256 plus newline over the compressed metadata archive. That archive binds both package records and the retained package list.
The scanner checks that file from the read-only image and requires the archived package list to equal the release list before injecting records. A coherent omission from both external representations therefore fails with the original image.
Admission still verifies the existing signed manifest fields and byte limit; it need not parse compressed metadata.

The existing sampler report format and comparison validation remain authoritative. No new measurement framework or comparator module is introduced.

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
| Supported Git workflow fails | failure | A supported workflow unexpectedly needs Perl | Git fixture fails; resolve before shipping | `test_toolbox_can_run_real_tools` |
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
| R2 | Candidate smaller and reproducible | Matched image identity checks plus `make test-runner-kernel` | Actual candidate smaller than fresh baseline; cap and reproducibility trials pass | P0 | |
| R3 | First-use budget preserved | `sudo bash playbooks/operations/acceptance/measure_toolbox_first_use.sh CANDIDATE_IMAGE REPORT 120 BASELINE_REPORT` | Eight passing ratios, exact artifact identities and explicit historical status | P0 | |
| R4 | Scan preserves retained evidence | Existing build-toolbox action and hosted image comparison with pinned scanners | Full records, relationships, vulnerability matches and cleanup cases pass | P0 | |
| R5 | Signed admission unchanged | `make test-unit-rustd-runner` | Runner and existing scanner boundary suites pass | P0 | |
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
2. **Preserved user behaviour** — M221's tools, lease isolation and first-use limits hold; the owner-approved Perl removal changes optional language/helper support.
3. **Optimal-way check** — Reuse dpkg and Syft while moving installation records out of the runtime image.
4. **Rebuild-vs-iterate** — Extend the existing builder and kernel lane.
5. **What we build** — Package removal, inventory-backed scanning, Git qualification and measured image reduction.
6. **What we do NOT build** — A replacement package manager, Git implementation or general dependency solver.
7. **Fit with existing features** — Preserve signed admission and vulnerability gates.
8. **Surface order** — Build and runtime checks first; publish the supported tool limits with the release.
9. **Dashboard restraint** — N/A: no dashboard change.
10. **Confused-user next step** — Consult the runner tool list and use a supported Git workflow or language runtime.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Package finalization, scanning, lease proofs and measurements form four ordered Sections.
- **Ordinary apt purge:** Rejected because the measured removal plan also deletes Python.
- **Hand-deleting package files:** Rejected because shared-file ownership and stale metadata need a second removal implementation.
- **Perl-free stock Git:** Chosen after the owner narrowed support to common Git workflows. A fresh read-only-root probe passed 23 labelled workflows without Perl.
- **Alternative Git implementation/custom build:** Unnecessary for the measured workflows; retain the packaged executable.
- **Many new Python modules:** Rejected by the owner; reuse the existing builder, deployment support and tests.
- **Patch-vs-refactor verdict:** A patch to the existing builder and proof paths. Record any crate reduction or race removal if implementation produces one.

## Discovery (consult log)

- **Scope change:** M221_001 excluded apt and dpkg removal. Indy now requests: "Goal: the toolbox Enhanced Read-Only File System (EROFS) image drops the package manager."
- **Database rationale:** M221 rejected deleting installed files while retaining records naming them. This work removes the manager and database together, retaining remaining-package records beside the image.
- **Initial Perl decision:** Indy: "Qualify a smaller Perl runtime for Git; allow reduced general Perl support (Recommended)".
- **Superseding Perl direction:** Indy: "since i donot see a point to use perl, as its only for git", then supplied Git's official `NO_PERL=YesPlease` build documentation. Qualify the packaged Git executable without Perl before considering a custom build.
- **Scanner simplification:** Indy: "also delete the toolbox_test.sh as well, and i still see your old python crap in toolbox.sh why". Delete the existing test file and remove all added scanner functions from the deployment helper. Keep the existing release scan in its action using native tools; do not create a replacement helper.
- **Local state:** Indy: "and make it smaller modular", "as opposed to giant functions that repeat", and "i do undrestand that the build.sh will be used only in the pipeline once, but these fixes can make this simplere". Keep state local to main, scope epoch environment to the commands consuming it, and share package queries and hashing.
- **Builder shape:** Indy: "well build.sh simple have bunch of function that gets called, and assembles the image. manifest.txt is the feeder to it?", followed by "nothing else" and "Donot  complicate other". Use named shell functions and declarative manifest data.
- **File scope:** Indy: "I feel manifest.txt and build.sh in toolbox/ folder are sufficient" and "I dont want those gazillion scripts/toolbox/*.py? what can we do better here?". Keep that folder to those two files and reuse existing proof locations.
- **Delivery:** Indy: "Yes update and push the PR"; update existing PR #800 with code and keep it in draft until qualification finishes.
- **Source revision:** `11883fdab`; fresh origin/main fetch found M221_002 unused.
- **Fresh baseline:** The unchanged amd64 build produces 162,004,992 bytes, digest `387fa1f1b6b4cbe686d78722e7a4cdb52926ddd26720e67a78ede1e22509f430`.
- **Measured package behaviour:** Apt's purge simulation removes Python. Explicit dpkg removal of perl and libperl5.40 preserves four targeted Git checks but removes Digest::SHA.
- **Evidence:** [M221_002-measurements.md](../../../playbooks/operations/acceptance/baselines/M221_002-measurements.md) records package sizes, the unchanged build, full scanner equivalence, Perl experiments and the classified reference sweep.
- **Perl-free build:** 135,979,008 bytes, 133 retained packages, digest `a3e4660a0f78c1b91261ee9051105f70b6e1cae08294b026e4dae2f0efb75279`; local build evidence only.
- **Remaining proof:** The final implementation still needs image, Git, latency, reproducibility, full kernel and coverage qualification.
- **Model reviews:** Six brainstorm lenses and nine formal reviewers completed. Formal review produced eight Major actions and five Nits; all were accepted and incorporated into interfaces, proofs and wiring.
- **Qualification host decision:** Indy selected "GitHub-hosted Linux runner (Recommended)" after the cloud host failed kernel prerequisites. The existing kernel workflow will supply full qualification; OrbStack access is unnecessary for this route.
- **Open blockers:** Complete the hosted qualification and all remaining proofs. No failed prerequisite is skipped or recorded as passing.
- **Deferrals and transfers:** None.
