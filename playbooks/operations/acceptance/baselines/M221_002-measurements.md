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
| Host preparation | `scripts/toolbox/install-builder-tools.sh`, `deploy/local/codex-runner.Dockerfile`, `playbooks/lib/runner/prepare.sh`, `.github/workflows/test-integration-rustd.yml`: retain host package installation |
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
