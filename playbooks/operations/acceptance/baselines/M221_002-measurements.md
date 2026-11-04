# Toolbox package-removal measurements

These measurements describe the investigation for M221_002. They do not establish completion of its kernel, latency or release gates.

## Unchanged image

Source revision: `11883fdaba2197adf8931d334396ba8b3bb9eb5b`.
The cloud Linux builder runs amd64 on kernel `6.18.57`.

`sudo -E make toolbox-image TOOLBOX_DIR=/audit/baseline` completed successfully in an isolated privileged Docker container.
The source checkout was mounted read-only at `/repo`.

| Property | Observed value |
|----------|----------------|
| Image bytes | 162,004,992 |
| Image digest | `387fa1f1b6b4cbe686d78722e7a4cdb52926ddd26720e67a78ede1e22509f430` |
| Installed packages | 144 |
| Snapshot | `20261031T000000Z` |
| mmdebstrap | `1.5.7-1+deb13u1` |
| apt | `3.0.3` |
| dpkg | `1.22.22` |
| erofs-utils | `1.8.6-1` |
| liblz4-1 | `1.10.0-4` |

The image retains the eight approved filesystem features from M221_001.
No compression or snapshot change was made for this measurement.

## Package sizes

`dpkg-query -W` inside the extracted unchanged image reports the following Installed-Size values.
These are package metadata in kibibytes, not compressed image savings.

| Package | Version | Installed-Size, KiB |
|---------|---------|--------------------:|
| apt | 3.0.3 | 4,480 |
| dpkg | 1.22.22 | 6,587 |
| passwd | 1:4.17.4-2 | 4,758 |
| libapt-pkg7.0 | 3.0.3 | 3,855 |
| sqv | 1.3.0-3+b2 | 1,855 |
| debian-archive-keyring | 2025.1 | 302 |
| perl | 5.40.1-6+deb13u1 | 841 |
| libperl5.40 | 5.40.1-6+deb13u1 | 29,322 |
| perl-modules-5.40 | 5.40.1-6+deb13u1 | 19,993 |
| perl-base | 5.40.1-6+deb13u1 | 7,815 |

## Removal experiment

An ordinary apt purge simulation also removes `python3` and `python3-minimal`.
The latter has an installation dependency on dpkg.

Explicit dpkg removal with dependency and essential-package overrides succeeded inside a disposable extracted root.
The first call removed the seven selected packages other than dpkg; the second removed dpkg itself.
Python imports for hashlib, JSON and TLS still passed, and Perl loaded Git.pm.

The scan of documentation links found no broken links after those package operations.
The experiment still left dpkg state, which requires separate deletion after capturing final records.

## Scanner experiment

Syft `1.54.1` scanned the extracted root before and after package removal.
Its release archive passed verification against the published checksum file before use.

| Scan | Debian packages | Rust crate records from uv | Total package records |
|------|----------------:|--------------------------:|----------------------:|
| Before removal | 144 | 1,054 | 1,198 |
| After removal | 136 | 1,054 | 1,190 |

The removed Debian set was exactly apt, dpkg, passwd, libapt-pkg7.0, sqv, debian-archive-keyring, perl and libperl5.40.

A temporary OverlayFS view restored status, package file lists and checksum records for scanning after deleting the root's dpkg database.
The upper layer used a size-limited temporary filesystem because the container's own filesystem rejected an OverlayFS upper directory.

The view produced identical package records and relationships to the post-removal scan with its original database.
Comparison normalized only the source identifier for the different scan directory; it retained package identities, metadata and file evidence.

Decisive output:

```text
scanner package records identical: 1190
scanner relationships identical: 12803
shipped root has no dpkg database
```

Removing entire `/etc/apt` and `/etc/dpkg` directories lost two files owned by retained packages.
The corrected experiment preserves those files and removes package database/cache state only.
The finalizer must preserve retained-package payload independently of manager removal.

The remaining status, file lists and checksums serialize to 1,120,429 bytes.
Deterministic gzip plus base64 reduces that metadata to 362,756 bytes.
Adding it to the existing release JSON, after filtering the package list, yields 389,723 bytes before licence hashes.

This proves the proposed metadata fits the existing one-mebibyte manifest limit.
Grype `0.120.1` scanned both post-removal software bills of materials with one downloaded database and `--only-fixed`.
All eight vulnerability matches were identical, including package identity, severity and fixed versions.

Neither scan reported a Critical match.

This does not yet prove the implementation's validation, interruption cleanup, cache-hit path or promotion.

## Perl experiments

Git `2.47.3` needs Perl for request-pull and some shell helpers.
Removing the interpreter makes request-pull fail with `/usr/bin/perl: not found`.
Interactive add, commit, diff and log still work in that exploratory root.

Keeping perl-base alone loses File::Find from the exploratory import set; that probe did not run a bundled hook.
Keeping perl-base and perl-modules-5.40 passes the targeted request-pull, stateful filter-branch, Git.pm and hook-module checks.
Those early checks reuse a repository, so final qualification must use isolated equivalent fixtures and assert their effects.

The smaller Perl selection removes Digest::SHA while JSON::PP remains available.
Indy authorized reduced general Perl support; this does not authorize narrowing supported Git behaviour.

## Package-manager reference audit

At revision `11883fdab`, this tracked-file search produced 45 matching lines:

```sh
git grep -n -w -E 'apt|apt-get|dpkg|useradd|chpasswd'
```

| Classification | Files and disposition |
|----------------|-----------------------|
| Executed inside a lease | `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox_size.rs`: one `dpkg-query` invocation; replace it with manifest-supplied licence expectations |
| Image construction | `scripts/toolbox/build.sh`: capture package provenance before removal; retain host-tool inventory and architecture detection |
| Host preparation | `.github/actions/build-toolbox/install-tools.sh`, `deploy/local/codex-runner.Dockerfile`, `playbooks/lib/runner/prepare.sh`, `.github/workflows/test-integration-rustd.yml`: retain host package installation |
| Builder identity | `.github/actions/build-toolbox/action.yml`, `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox.rs`, `rustd/crates/afr_sandbox/src/toolbox/fixtures/release.json`: retain host fingerprints and assertions |
| Host-only test messages | `deploy/baremetal/deploy_test.sh`, `playbooks/lib/egress_host_deps.sh`, `playbooks/lib/runner/runner_test.sh`: retain prerequisite diagnostics and setup tests |
| Documentation | `docs/architecture/runner_execution.md`, `docs/development.md`, completed M75/M211/M220/M221/M238 specs, `docs/v2/reviews/m211-toolbox-spikes.md`, `playbooks/operations/acceptance/baselines/M221_001-6e4a02127b47.md`, `scripts/toolbox/manifest.txt`: retain historical and builder references; update current toolbox behaviour |

There were no useradd or chpasswd matches. The only identified lease invocation is the licence test above.
Repeat the sweep after implementation and classify new removal and absence-test references.

User-supplied shell programs can request absent commands; supported repository workflows must not depend on them.

## Linux host prerequisites

The cloud Docker host has `# CONFIG_SECURITY_LANDLOCK is not set` in `/proc/config.gz`.
The Landlock version syscall returns `-1`, errno `38` (function not implemented).
Therefore its privileged container cannot pass the required kernel lane.

The canonical target first failed because container loop-device nodes were absent; creating nodes for its existing loop devices resolved that setup error.
The sampler's cancellation tests then failed because `CONFIG_CHECKPOINT_RESTORE` is also disabled, removing `/proc/<pid>/task/<pid>/children`.

No kernel test is marked passing or skipped on that basis.
The separate shared dev runner's kernel inventory has not been read, and no host kernels have been removed.

## Evidence still required

The final image needs independent payload reconciliation, complete Git and licence proofs, two reproducible builds and measured compressed savings.
The canonical kernel lane, matched cold-cache samples and Rust patch coverage remain required.
No first-use comparison or merged release is claimed here.

## Simplified Perl-free candidate

The owner requested only `build.sh` and `manifest.txt` in `scripts/toolbox/`.
The first consolidation emitted a temporary program from the builder. The owner rejected that design; the current builder uses named shell functions.

Scanner reconstruction lives in the existing deployment helper and its existing test suite.
The existing installer moved beside the build action, and the existing sampler and its test moved under acceptance.

`sudo -E make toolbox-image TOOLBOX_DIR=/audit/perl-free` completed with 133 retained packages and an image of 135,979,008 bytes.
Its digest is `a3e4660a0f78c1b91261ee9051105f70b6e1cae08294b026e4dae2f0efb75279`; the adjacent release JSON is 322,909 bytes.
The exact removal set is the manifest's eleven packages, including all Perl packages.

This is 26,025,984 fewer bytes than the fresh unchanged image. It is local build evidence, not full qualification.

A read-only bubblewrap root with a writable workspace passed 23 labelled common Git workflows using Debian Git 2.47.3 without Perl.
The probe exercised local bare remotes, conflict/abort, rebase, interactive staging, worktrees, garbage collection, object verification and Unicode/executable payloads.
It did not test external network transport or the complete production lease boundary.

The consolidated scanner reconstruction passed on the mounted Perl-free image:
`full scan: 1187 packages, 133 Debian packages, 10719 relationships` (Syft 1.54.1).
The 1,054 vendored Rust package detections remain present.

Full original-record comparison and same-database vulnerability parity still require qualification for this final candidate.
`bash deploy/baremetal/toolbox_test.sh` reports `5 passed, 0 failed`, including successful reconstruction and malformed/tampered/bounded-input refusal cases.

The complete original-record comparison caught missing control-file attribution in the first reconstruction.
The builder now exports retained package control files as well as lists and checksums; the scanner treats them as data.
The corrected image remains 135,979,008 bytes, with digest `0d8ef1a73c70b21faf9173d0b3d7751a95cd8aa6180e8398882c5c4ac9cd211f`.

Its release JSON is 915,521 bytes, below admission's 1,048,576-byte bound.

The independent baseline extraction was purged with dpkg, retaining its original database for scanning.
Its complete Syft package records exactly match the reconstructed final image: `1187 packages; 10829 relationships`.
Only the scan-directory source identifier and relationship ordering were normalized.

Grype 0.120.1 with the same cached database and `--only-fixed` reports identical match records: `8 matches; 0 Critical`.

## Native shell builder

The owner rejected embedding the finalizer in `build.sh`. The builder now consists of named shell functions and reads its inputs from `manifest.txt`.
It uses dpkg for removal, GNU tar for payload comparison and jq for the existing release JSON.

`sudo -E make toolbox-image TOOLBOX_DIR=/audit/shell-candidate` completed successfully.
The image is 135,979,008 bytes with digest `acfe072d46762caae77c6dbe897c20206f3300add6c85704f3b2f68704b5bd0f`.
The adjacent release JSON is 923,937 bytes and contains 133 package entries.

Syft 1.54.1 over the reconstructed mounted image reports `1187 packages, 133 Debian packages, 10829 relationships`.
The compressed metadata is now a deterministic tar archive containing status, package info records and the retained package list.
The image commits to the compressed archive hash; the scanner requires its package list to equal the release list.

Hosted qualification for source `0e0de728350471a68217501dc86f16b6e6df17ed` was dispatched as run `41779197114`.
The final review and all required proof remain pending. Indy requested: "once done, let me check"; leave PR #800 unmerged for inspection.

## Owner-requested scanner cleanup

Indy requested: "also delete the toolbox_test.sh as well, and i still see your old python crap in toolbox.sh why".
The added Python scanner functions are removed from `deploy/baremetal/toolbox.sh`, restoring its original staging-only contents.
`deploy/baremetal/toolbox_test.sh` is deleted, including its four original staging tests and the added reconstruction case.

The deployment test runner discovers files by glob; there is no explicit invocation to remove.
The existing build action restores records using GNU tar, coreutils and jq, with bounded decoding, image commitment, entry validation and package-list agreement.
No replacement helper or test file is introduced.
