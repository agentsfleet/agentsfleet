<!--
SPEC AUTHORING RULES (load-bearing; the one comment that survives):
- Body order is the implementing agent's read order. The orly-spec skill fills
  this template under the spec standard (spec.md), then deletes every "tpl:"
  guidance comment.
- An EGM (Elephant-Goldfish Model) review checks the structure: leftover tpl
  comments, unfilled fill slots, missing or empty sections and the spec
  standard's rules. orly's gate checks no spec structure.
- No time or effort estimates anywhere: no effort columns, complexity ratings,
  percentage complete, implementation dates or assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Depends on is the only
  sequencing signal. A section that contradicts these rules loses; delete it.
-->

# M237_002: The host caches the toolbox once, and a sandbox starts with no polling or mount program

**Prototype:** v2.0.0

**Milestone:** M237

**Workstream:** 002

**Date:** Nov 02, 2026

**Status:** IN_PROGRESS

**Priority:** P2, host memory and sandbox start time; after M237_001 the start is hidden from users

**Categories:** DOCS, INFRA, OBS

**Batch:** B1, independent of M237_001 and of M236_001; touches only `afr_sandbox` and `afr_executor`

**Branch:** `feat/m237-sandbox-lease-path`, shared with M237_001: one Pull Request carries the milestone

**Baseline revision:** 2f8c878c09e9c4048da1724385ec58746d348ed4

**Test Baseline:** pending, measured before the Pull Request

**Baseline evidence:** pending, set to the kernel lane's budgets and first-use reports before PLAN

**Depends on:** none

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 02, 2026) from Indy's Nov 02, 2026 sandbox review and dev-host checks that day

**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox, §"A lease's sandbox today" and §"Code walkthrough" Parts A, D and E

---

## Overview

**Goal (testable):** `test_toolbox_loop_reads_past_the_host_cache`: the admitted toolbox's loop device reports direct I/O, and reading every toolbox file leaves under 1% of the image file in the host's page cache.

**Problem:** The toolbox's loop device is buffered. Its compressed blocks are cached in the image file's page cache and again in the loop device's own cache, and admission's hash leaves the whole image in the first one. The dev host shows it: `losetup` lists the toolbox on `/dev/loop0` with `DIO 0`. Each sandbox start also polls a socket file every 10 ms and runs `mount(8)` for its workspace disk.

**Solution summary:** The toolbox loop device is configured with direct I/O in the one `LOOP_CONFIGURE` call, and admission drops the hash read's cache. The executor speaks over a socket pair the engine passes into bubblewrap, so nothing polls and no socket directory is bound. The workspace disk is attached and mounted in-process with direct I/O and autoclear set, so `mount(8)` no longer starts. `mke2fs` stays.

**Design flow:**

```
toolbox   open ─► hash ─► fadvise(DONTNEED) ─► LOOP_CONFIGURE(ro, autoclear, direct, 4096) ─► mount erofs
executor  socketpair(CLOEXEC) ─► child end cleared in pre_exec ─► entry serves it ─► client.over(parent end)
disk      sparse image ─► mke2fs ─► LOOP_CONFIGURE(rw, autoclear, direct, 4096) ─► mount(2) ext4
```

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(sandbox): cache the toolbox once and start without polling
- **Intent (one sentence):** A runner host keeps one cached copy of its toolbox, and a sandbox starts without polling or a mount program.
- **Authoring handshake:** Indy approved fixes 3 to 5 of the Nov 02 review. ASSUMPTIONS I'M MAKING: the loop device stays; the file-backed EROFS mount waits for a later spec; per-lease `mke2fs` stays until a measurement shows its cost.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit. A restatement that differs from the Intent stops the work until the two are reconciled.

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` — §"Code walkthrough" Parts A.1, D and E show each call this spec changes.
2. `rustd/crates/afr_sandbox/src/toolbox/loop_device/mod.rs` — `attach`, `backing` and `direct_io`, the ioctls every Section reuses.
3. `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` — the `pre_exec` hook that already passes the tenant descriptors into bubblewrap.
4. `rustd/crates/afr_executor/src/server/mod.rs` — the listener that accepts one connection today, and `Client::over` in `afr_executor/src/client/mod.rs`.
5. https://third-party-mirror.googlesource.com/systemd/+/refs/heads/master/src/shared/loop-util.c — requesting direct I/O in `LOOP_CONFIGURE` and confirming it through `LOOP_GET_STATUS64`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_sandbox/src/toolbox/loop_device/mod.rs` and tests | EDIT | Configure flags for direct I/O, block size and read-write attach; confirm through status |
| `rustd/crates/afr_sandbox/src/toolbox/{admit.rs,adopt/mod.rs}` and tests | EDIT | Drop the hash read's cache; adopt a direct-I/O loop device |
| `rustd/crates/afr_sandbox/src/bubblewrap/mod.rs` and tests | EDIT | No socket-directory bind; the executor's descriptor joins the entry's arguments |
| `rustd/crates/afr_sandbox/src/bubblewrap_engine/{mod,parts}.rs` and tests | EDIT | Socket pair made and passed; no `run/` directory; ready probe |
| `rustd/crates/afr_sandbox/src/{serve,tenant}/` and tests | EDIT | The entry adopts and serves the inherited socket |
| `rustd/crates/afr_executor/src/{server,client,lib.rs}` and tests | EDIT | Server serves a connected stream, inherited or accepted |
| `rustd/crates/afr_sandbox/src/workspace_disk/mod.rs` and tests | EDIT | In-process attach and mount; one direct-I/O path |
| `rustd/crates/afr_sandbox/src/host/mod.rs`, `src/lib.rs` | EDIT | `mount` leaves `HostTools` and the crate's exports |
| `rustd/crates/afr_sandbox/src/probe/mod.rs` | EDIT | Reports whether the toolbox device took direct I/O |
| `rustd/crates/afr_sandbox/examples/kernel_lane/` | EDIT | New trials; `filesystems.rs` stops using `MOUNT_PATH` |
| `scripts/toolbox/measure_first_use.sh`, `scripts/toolbox/test_measure_first_use.sh` | EDIT | An attach mode: buffered `-o loop`, or a direct-I/O loop device |
| `rustd/crates/afr_sandbox/tests/confine.rs`, `rustd/crates/afr_executor/tests/` | EDIT | Callers of the socket-path API |
| `docs/architecture/runner_execution.md` | EDIT | Toolbox, Parts A, D and E after the change |
| `docs/v2/active/M237_002_P2_DOCS_INFRA_OBS_SANDBOX_START_DROPS_POLLING_AND_DOUBLE_CACHE.md` | EDIT | This spec, opened at CHORE(open); moves to `done/` at close |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE for the new attach and mount errors; RS-RACE for descriptor inheritance across concurrent spawns.
- **`tests.md`** — TEST-REDGREEN, TEST-OUTCOME, TEST-ERROR, TEST-RESIDUAL for loop devices and mounts left behind.
- **`domains/architecture.md`** — ARCH-LAND: the toolbox and start facts change in `runner_execution.md` in the same Pull Request.
- **`domains/logging.md`** — named events with an `event` field.
- **`docs.md`** — DOC-02, DOC-03, DOC-05, DOC-14b and DOC-33 for changed architecture prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | Linux-only code is linted on Linux as well |
| `make test-unit-all` coverage gates | yes | Kernel-only ioctls stay in the excused kernel files |
| `make test-runner-kernel` | yes, every Section is kernel behaviour | One lane run and one run on the Debian 13 dev host, each recording `uname -r` |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `parts.rs` is near the cap | Socket-pair setup gets its own module |
| `orly gate verify` docs language | yes | Short sentences, no banned words |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afr_sandbox/src/workspace_disk/mod.rs` — the workspace disk already switches its loop device to direct I/O and falls back to buffered on `EINVAL`.
- **Reference:** systemd's `loop-util.c` — sets `LO_FLAGS_DIRECT_IO` in `LOOP_CONFIGURE` and reads the result back.
- **Reference:** `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` and `afr_executor/src/server/launch/tenant/mod.rs` — descriptors cross into bubblewrap in `pre_exec`, and the entry sets close-on-exec again on adoption.

## Sections (implementation slices)

### §1 — The toolbox is read past the host cache

Admission hashes the image through its descriptor, then calls `posix_fadvise(DONTNEED)` on it to ask the kernel to drop the hash's cache. That call is advice, not a promise, so Dimension 1.1 measures the pages that stay resident.

The attach sets `LO_FLAGS_DIRECT_IO` with `LO_FLAGS_READ_ONLY` and `LO_FLAGS_AUTOCLEAR`, and a 4096-byte block size, in the one `LOOP_CONFIGURE` call. Admission reads `LOOP_GET_STATUS64` back and logs whether direct I/O took.

A host whose file system refuses direct I/O keeps a buffered device and logs `caching: buffered`, as the workspace disk does. Adoption accepts a loop device with or without the direct-I/O flag, since it checks only the read-only bit.

`scripts/toolbox/measure_first_use.sh` mounts with buffered `-o loop` today. It gains `TOOLBOX_LOOP_DIRECT_IO=on`, which attaches with `losetup --direct-io=on --read-only` before mounting. One image is measured both ways on the dev host, buffered as the baseline, under the §Toolbox rule of no more than a 10% p99 regression.

- **Dimension 1.1** — The toolbox's loop device reports direct I/O and reading every file leaves the image under 1% cached → Test `test_toolbox_loop_reads_past_the_host_cache`
- **Dimension 1.2** — A refused direct I/O request leaves a buffered device and logs it → Test `test_toolbox_buffered_fallback_is_logged`
- **Dimension 1.3** — A toolbox mount from the previous runner, buffered or direct, is adopted → Test `test_direct_io_toolbox_is_adopted`
- **Dimension 1.4** — No first-use step regresses past 10% at p99 → Test `manual_first_use_holds_after_direct_io`
- **Dimension 1.5** — The harness attaches with direct I/O when asked and refuses a device that did not take it → Test `test_sampler_attaches_with_direct_io_when_asked`

### §2 — The executor speaks over an inherited socket pair

The engine makes a connected Unix socket pair with `SOCK_CLOEXEC` before it spawns bubblewrap. Only the child's end loses close-on-exec, inside the `pre_exec` hook, so no other spawn on the host inherits it. The runner drops its copy of the child's end once bubblewrap starts. The descriptor number joins the entry's arguments after the tenant descriptors.

The entry sets close-on-exec on its end again before hardening, so no tenant process inherits the link. The server serves a connected stream, whether inherited here or accepted from a listener as the unsandboxed engine and a future vsock engine need. The ready check is a `list_dir` of the workspace within the same ten seconds, or bubblewrap's exit with its error stream. The `run/` directory, its ownership change and its read-write bind leave the sandbox.

- **Dimension 2.1** — A new sandbox answers with no socket file on the host or inside → Test `test_executor_answers_over_the_inherited_pair`
- **Dimension 2.2** — A tenant process holds no socket descriptor → Test `test_tenant_process_holds_no_executor_socket`
- **Dimension 2.3** — Another lease's concurrent spawn never inherits the socket → Test `test_concurrent_spawn_inherits_no_other_socket`
- **Dimension 2.4** — An entry that exits before answering is refused with its error stream → Test `test_dead_entry_is_refused_with_its_reason`
- **Dimension 2.5** — The sandbox holds no writable bind beside `/workspace` and `/tmp` → Test `test_sandbox_has_no_socket_directory_bind`

### §3 — The workspace disk is attached and mounted in-process

`WorkspaceDisk::create` keeps the sparse image and `mke2fs`. It then attaches the image read-write through `LOOP_CONFIGURE` with `LO_FLAGS_AUTOCLEAR`, `LO_FLAGS_DIRECT_IO` and a 4096-byte block size. It mounts ext4 `nosuid,nodev` with `mount(2)` while it still holds the device descriptor, then closes it.

Autoclear means unmounting frees the device, so the release and the boot sweep keep their order. The separate `LOOP_SET_DIRECT_IO` call goes. `mount(8)` and `MOUNT_PATH` leave `HostTools` and the crate's exports.

- **Dimension 3.1** — Building a lease disk starts no `mount` program → Test `test_workspace_disk_starts_no_mount_program`
- **Dimension 3.2** — Releasing the disk leaves no loop device attached → Test `test_released_disk_leaves_no_loop_device`
- **Dimension 3.3** — A refused mount leaves no device and no files → Test `test_refused_mount_leaves_nothing`
- **Dimension 3.4** — A buffered fallback is reported when the host refuses direct I/O → Test `test_disk_reports_buffered_fallback`

### §4 — The architecture page tells the new start

`runner_execution.md` §Toolbox, §"A lease's sandbox today" and Parts A.1, D and E change to the built calls. The Section records the budgets trial's cold start before and after, with each host's kernel.

- **Dimension 4.1** — Parts A.1, D and E match the code at merge → Test `manual_walkthrough_matches_code_after_engine_changes`

## Interfaces

```
afr_executor server           serve a connected stream; the path listener stays for the
                              unsandboxed engine, the tests and a future vsock engine
agentsfleet-runner sandbox    <tenant procs fd> <tenant events fd> <executor fd>
afr_sandbox::HostTools        { bwrap, mke2fs }          mount removed
Wire, schema, OpenAPI and CLI: no change.
```

## Failure Modes

| Mode | Kind | Cause | Handling (system response + what the caller observes) | Test |
|------|------|-------|--------------------------------------------------------|------|
| Direct I/O refused | capacity | Backing file system rejects direct I/O | Device stays buffered; admission logs `caching: buffered` | `test_toolbox_buffered_fallback_is_logged` |
| Socket leaks to tenant | security | Close-on-exec left clear on the entry's end | Entry sets it before hardening; tenant listing shows no socket | `test_tenant_process_holds_no_executor_socket` |
| Socket leaks across leases | security | Close-on-exec cleared in the parent | Cleared only in `pre_exec`; concurrent spawns see none | `test_concurrent_spawn_inherits_no_other_socket` |
| Entry dies early | failure | Hardening refused or binary missing | Sandbox refused with its error stream within ten seconds | `test_dead_entry_is_refused_with_its_reason` |
| Mount refused | failure | Kernel rejects the ext4 mount | Device freed by closing its descriptor, image removed, lease refused | `test_refused_mount_leaves_nothing` |
| Device pinned after release | capacity | Autoclear missing on a read-write device | Autoclear set at configure; release check finds no device | `test_released_disk_leaves_no_loop_device` |
| First use slower | capacity | Cold reads go to disk instead of cache | p99 measured; a step past 10% fails the row | `manual_first_use_holds_after_direct_io` |

## Invariants

1. The kernel parses only hashed bytes — `attach` takes the `File` that was hashed, as today.
2. No loop device outlives its mount — every attach sets `LO_FLAGS_AUTOCLEAR`, enforced by `attach` building the flags itself.
3. The executor link reaches no tenant process — the entry's adopt step sets close-on-exec before `harden` runs, and `harden` refuses once a second thread exists.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|----------|------------|--------------------|---------------|------------|
| `sandbox_toolbox_admission_completed` field `caching` | operator | Toolbox mounted | digest, `direct` or `buffered` | none needed | `test_toolbox_loop_reads_past_the_host_cache` |
| `sandbox_workspace_buffered` | operator | A disk stays buffered | lease id | none needed | `test_disk_reports_buffered_fallback` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | kernel | `test_toolbox_loop_reads_past_the_host_cache` | Drop caches, read every file → status shows direct I/O, image resident pages under 1% |
| 1.2 | unit | `test_toolbox_buffered_fallback_is_logged` | Status without the flag → `caching: buffered` logged, admission succeeds |
| 1.3 | unit | `test_direct_io_toolbox_is_adopted` | Backing flags read-only plus direct → adopted |
| 1.4 | manual | `manual_first_use_holds_after_direct_io` | Buffered run, then a `TOOLBOX_LOOP_DIRECT_IO=on` run against it on the dev host → no step past 1.10 p99 ratio |
| 1.5 | unit | `test_sampler_attaches_with_direct_io_when_asked` | Mode on, `losetup` reports `DIO 0` → sampler exits non-zero with no report |
| 2.1 | kernel | `test_executor_answers_over_the_inherited_pair` | New sandbox → no `executor.sock` on host or inside |
| 2.2 | kernel | `test_tenant_process_holds_no_executor_socket` | `ls -l /proc/self/fd` in `shell` → no socket entries |
| 2.3 | kernel | `test_concurrent_spawn_inherits_no_other_socket` | Two sandboxes start at once → each entry holds one executor socket |
| 2.4 | kernel | `test_dead_entry_is_refused_with_its_reason` | Entry exits 1 with a line → refusal carries that line |
| 2.5 | kernel | `test_sandbox_has_no_socket_directory_bind` | Mount table inside → no `/run/agentsfleet` |
| 3.1 | kernel | `test_workspace_disk_starts_no_mount_program` | Program audit during prepare → no `mount` started |
| 3.2 | kernel | `test_released_disk_leaves_no_loop_device` | Release → no loop device names the deleted image |
| 3.3 | kernel | `test_refused_mount_leaves_nothing` | Corrupt superblock → lease refused, no device, no files |
| 3.4 | unit | `test_disk_reports_buffered_fallback` | Status without the flag → `Caching::Buffered` |
| 4.1 | manual | `manual_walkthrough_matches_code_after_engine_changes` | Reviewer follows Parts A.1, D and E at the merge commit |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Toolbox cached once, socket pair and in-process disk proven on the lane (§1 to §3) | `make test-runner-kernel` | exit 0 | P0 | |
| R2 | The same trials pass on the Debian 13 dev host | `make test-runner-kernel` on `zombie-dev-worker-ant`, output in Session Notes | exit 0 | P0 | |
| R3 | Direct-I/O first use within 10% p99 of buffered (§1); `IMAGE` is the admitted toolbox; baseline from the same command without the variable | `sudo TOOLBOX_LOOP_DIRECT_IO=on bash scripts/toolbox/measure_first_use.sh "$IMAGE" first-use-direct.json 120 first-use-buffered.json` | `0` steps marked `REGRESSION` | P1 | |
| R4 | Start figures recorded (§4) | `test -s playbooks/operations/acceptance/baselines/M237_002-start.md` | exit 0 | P2 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

The spec standard (`spec.md`) defines grading, the ship rule and transfers to a successor spec (`MOVED` rows) in SPEC-GRADE, SPEC-SHIP and SPEC-MOVE.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A: no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `MOUNT_PATH` | `git grep -n -w "MOUNT_PATH" rustd` | no output |
| `WORKSPACE_OPTIONS` | `git grep -n -w "WORKSPACE_OPTIONS" rustd` | no output |
| `RUN_DIR_MODE` | `git grep -n -w "RUN_DIR_MODE" rustd` | no output |
| `LOOP_SET_DIRECT_IO` | `git grep -n -w "LOOP_SET_DIRECT_IO" rustd` | no output |

## Out of Scope

- The file-backed EROFS mount. Debian's `6.12.111` makes its direct I/O opt-in, and the hosted kernel lane may lack it. A later spec revisits it once every lane and host runs a kernel with `CONFIG_EROFS_FS_BACKED_BY_FILE` and `directio`.
- Template workspace disks. They need a lock, an atomic publish, unique file-system identities and bounded sizes. A later spec, if §4's figures show `mke2fs` matters.
- Raising the Landlock ABI above V3 and scoping abstract Unix sockets: a hardening spec of its own.
- dm-verity under the toolbox: deferred in M224_001.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator reads one toolbox-sized cache footprint on a host running many sandboxes, where two were before.
2. **Preserved user behaviour** — Every tool, result, refusal and egress rule stays as it is.
3. **Optimal-way check** — One flag fixes the double cache; the file-backed mount does too but raises the kernel floor.
4. **Rebuild-vs-iterate** — Iterate on the existing loop and spawn code.
5. **What we build** — Configure flags, a cache drop, a socket pair and an in-process mount.
6. **What we do NOT build** — File-backed mounts, template disks and Landlock changes; reasons in Out of Scope.
7. **Fit with existing features** — Must not weaken admission by descriptor or the tenant's isolation from the executor link.
8. **Surface order** — N/A: no user surface.
9. **Dashboard restraint** — N/A: one log field.
10. **Confused-user next step** — An operator reads `caching` on the admission event, or `losetup -l -O NAME,DIO`.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three engine changes behind the unchanged `Engine` trait, apart from M237_001's lease order.
- **Alternatives considered:** The file-backed EROFS mount removes the loop device but needs 6.12 with `directio` set and a probe by trial mount. Template disks remove `mke2fs` but add shared state with unique-identity and race work.
- **Patch-vs-refactor verdict:** this is a **patch** because each change stays inside one engine and keeps its failure classes.

## Discovery (consult log)

- **Consults** — Nov 02, 2026, revision `77147e0d2`. Indy: "1-5 seemed good fixes to do".
- **Dev host (Nov 02, `zombie-dev-worker-ant`)** — Debian 13.7, kernel `6.12.111+deb13-amd64`, bare metal, `/dev/kvm` present. `losetup` shows the toolbox on `/dev/loop0`, `DIO 0`. State on `/dev/md3`, ext4. bubblewrap 0.12.0, mke2fs 1.47.2. `CONFIG_EROFS_FS_BACKED_BY_FILE=y`.
- **Kernel source** — Upstream `v6.12` `fs/erofs/fileio.c` reads direct whenever the backing file allows. Stable `v6.12.111` and `v6.12.113` read direct only with the `directio` option. The draft first claimed the upstream behaviour for Debian; review corrected it.
- **Shape change** — The draft's file-backed mount and template disks were replaced after review: the loop flag gives the same cache result on every kernel the lanes use.
- **Model reviews** — EGM review, Claude Opus 5.5, Nov 02: zoom-out 8, comprehension 8, readiness 10, design 10 findings across the draft that held both Workstreams. Accepted for this Workstream: loop direct I/O over file-backed; cache drop after hashing; descriptor rules for the socket pair; listener kept for vsock; autoclear on the read-write attach; templates out of scope.
- **Greptile on PR #797** — one P2 finding, fixed: the first-use harness mounted only with buffered `-o loop`. It gains a direct-I/O mode, and R3 compares the two on one image.
- **One Pull Request (Indy, Nov 02)** — Both Workstreams ship in one Pull Request, which is the milestone's one ready Pull Request. Measurement runs locally through the make targets: the OrbStack kernel lane (Debian 13.7 on OrbStack's `7.2.12` kernel) stands in for the dev host in R2 and R3. The dev host stays on `6.12.111` until M237 ships.
- **Same-task build (M237_001 §2)** — The build now shares a task with the model's stream. §3's in-process attach and mount therefore run on `spawn_blocking`, so a slow ioctl never stalls the stream.
- **Codex review (Nov 02, baseline `2f8c878c0`)** — one P2 finding, fixed: §1 claimed the cache drop leaves no cache behind. `posix_fadvise` is advice, so §1 now leans on the flag readback, the resident-page test and the logged buffered fallback.
- **Open blockers** — none.
- **Metrics review** — `caching` added to the admission event.
- **Skill-chain outcomes** — pending until VERIFY and REVIEW.
- **Deferrals and transfers** — none.

> Owner (Indy, Nov 02, 2026 10:52 PM): "coudl we do both specs in this PR, if you need to measure do it in local? in the docker compose using make targets" — context: whether M237_001 and M237_002 ship together, and where the start figures are measured.
