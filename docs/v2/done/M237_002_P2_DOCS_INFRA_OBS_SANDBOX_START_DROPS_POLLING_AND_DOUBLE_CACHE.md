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

# M237_002: A sandbox starts with no polling or mount program, and a direct-I/O toolbox is measured

**Prototype:** v2.0.0

**Milestone:** M237

**Workstream:** 002

**Date:** Nov 02, 2026

**Status:** DONE

**Priority:** P2, host memory and sandbox start time; after M237_001 the start is hidden from users

**Categories:** DOCS, INFRA, OBS

**Batch:** B1, independent of M237_001 and of M236_001; touches only `afr_sandbox` and `afr_executor`

**Branch:** `feat/m237-sandbox-lease-path`, shared with M237_001: one Pull Request carries the milestone

**Baseline revision:** 2f8c878c09e9c4048da1724385ec58746d348ed4

**Test Baseline:** base `2f8c878c0`, measured in an isolated checkout: runner unit 1382 and daemon unit 139 passed. Its unit lane stopped at the macOS `base64` test this Pull Request fixes; its integration lane stopped at 630 passed, 2 failed, both `afd_authoring` contention timeouts. Branch: unit 4911 Rust and 5289 TypeScript passed; integration 1458 passed.

**Baseline evidence:** this spec's Discovery, **Measurements**

**Depends on:** none

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 02, 2026) from Indy's Nov 02, 2026 sandbox review and dev-host checks that day

**Canonical architecture:** `docs/architecture/runner_execution.md` §Toolbox, §"A lease's sandbox today" and §"Code walkthrough" Parts A, D and E

---

## Overview

**Goal (testable):** `test_executor_answers_over_the_inherited_pair`: a new sandbox answers over the pair it inherited, with no socket file on the host or inside.

**Problem:** The toolbox's loop device is buffered. Its compressed blocks are cached in the image file's page cache and again in the loop device's own cache, and admission's hash leaves the whole image in the first one. The dev host shows it: `losetup` lists the toolbox on `/dev/loop0` with `DIO 0`. Each sandbox start also polls a socket file every 10 ms and runs `mount(8)` for its workspace disk.

**Solution summary:** A direct-I/O toolbox device was built and measured against the 10% p99 rule; it failed, so the toolbox stays buffered. The executor speaks over a socket pair the engine passes into bubblewrap, so nothing polls and no socket directory is bound. The workspace disk is attached and mounted in-process with direct I/O and autoclear set, so `mount(8)` no longer starts. `mke2fs` stays.

**Design flow:**

```
toolbox   open ─► hash ─► LOOP_CONFIGURE(ro, autoclear, buffered, 4096) ─► mount erofs
executor  socketpair(CLOEXEC) ─► child end cleared in pre_exec ─► entry serves it ─► client.over(parent end)
disk      sparse image ─► mke2fs ─► LOOP_CONFIGURE(rw, autoclear, direct, 4096) ─► mount(2) ext4
```

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(sandbox): start without polling or a mount program
- **Intent (one sentence):** A sandbox starts without polling or a mount program, and the toolbox goes direct only if its first use holds.
- **Authoring handshake:** Indy approved fixes 3 to 5 of the Nov 02 review. ASSUMPTIONS I'M MAKING: the loop device stays; the file-backed EROFS mount waits for a later spec; per-lease `mke2fs` stays until a measurement shows its cost.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit. A restatement that differs from the Intent stops the work until the two are reconciled.

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` — §"Code walkthrough" Parts A.1, D and E show each call this spec changes.
2. `rustd/crates/afr_sandbox/src/toolbox/loop_device/mod.rs` — `attach`, `backing` and `direct_io`, the ioctls every Section reuses.
3. `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts/mod.rs` — the `pre_exec` hook that already passes the tenant descriptors into bubblewrap.
4. `rustd/crates/afr_executor/src/server/mod.rs` — the listener that accepts one connection today, and `Client::over` in `afr_executor/src/client/mod.rs`.
5. https://third-party-mirror.googlesource.com/systemd/+/refs/heads/master/src/shared/loop-util.c — requesting direct I/O in `LOOP_CONFIGURE` and confirming it through `LOOP_GET_STATUS64`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_sandbox/src/toolbox/loop_device/mod.rs` and tests | EDIT | Configure flags for block size and read-write attach; direct I/O for a read-write device alone; confirm through status |
| `rustd/crates/afr_sandbox/src/toolbox/{admit.rs,adopt/mod.rs}` and tests | EDIT | Log the caching the device took; adopt a direct-I/O loop device |
| `rustd/crates/afr_sandbox/src/bubblewrap/mod.rs` and tests | EDIT | No socket-directory bind; the executor's descriptor joins the entry's arguments |
| `rustd/crates/afr_sandbox/src/bubblewrap_engine/{mod.rs,parts/}` and tests | EDIT | Socket pair made and passed; no `run/` directory; ready probe; the release half in `parts/teardown.rs` |
| `rustd/crates/afr_sandbox/src/{serve,tenant}/` and tests | EDIT | The entry adopts and serves the inherited socket |
| `rustd/crates/afr_executor/src/{server,client,lib.rs}` and tests | EDIT | Server serves a connected stream, inherited or accepted |
| `rustd/crates/afr_sandbox/src/workspace_disk/mod.rs` and tests | EDIT | In-process attach and mount; one direct-I/O path |
| `rustd/crates/afr_sandbox/src/host/mod.rs`, `src/lib.rs` | EDIT | `mount` leaves `HostTools` and the crate's exports |
| `rustd/crates/afr_sandbox/src/probe/mod.rs` | EDIT | Reports whether the toolbox device took direct I/O |
| `rustd/crates/afr_sandbox/examples/kernel_lane/` | EDIT | New trials; `filesystems.rs` stops using `MOUNT_PATH` |
| `scripts/toolbox/measure_first_use.sh`, `scripts/toolbox/test_measure_first_use.sh` | EDIT | An attach mode: buffered `-o loop`, or a direct-I/O loop device |
| `rustd/crates/afr_sandbox/tests/confine.rs`, `rustd/crates/afr_executor/tests/` | EDIT | Callers of the socket-path API |
| `docs/architecture/runner_execution.md` | EDIT | Toolbox, Parts A, D and E after the change |
| `docs/AUTH.md` | EDIT | The `env_clear()` citation follows `parts.rs` to `parts/mod.rs` |
| `rustd/crates/afr_sandbox/src/harden/mod.rs` | EDIT | Its comment named the socket directory the pair removed |
| `rustd/crates/afr_sandbox/src/host/tests.rs` | EDIT | `HostTools.mount` is a function, not a program path |
| `rustd/crates/agentsfleet_runner/src/{main.rs,main_tests.rs}`, `agentsfleet_runner/tests/entries.rs` | EDIT | The entry takes `--executor <fd>` beside the tenant descriptors |
| `docs/architecture/runner_fleet.md` | EDIT | Its `/run/agentsfleet` mount row went with the socket directory |
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
| `make test-runner-kernel` | yes, every Section is kernel behaviour | Runs on the OrbStack machine `afr-kernel`, recording `uname -r`; the dev host waits until M237 ships (Discovery) |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `parts.rs` passed the cap | The release half moves to `parts/teardown.rs` |
| `orly gate verify` docs language | yes | Short sentences, no banned words |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afr_sandbox/src/workspace_disk/mod.rs` — the workspace disk already switches its loop device to direct I/O and falls back to buffered on `EINVAL`.
- **Reference:** systemd's `loop-util.c` — sets `LO_FLAGS_DIRECT_IO` in `LOOP_CONFIGURE` and reads the result back.
- **Reference:** `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts/mod.rs` and `afr_executor/src/server/launch/tenant/mod.rs` — descriptors cross into bubblewrap in `pre_exec`, and the entry sets close-on-exec again on adoption.

## Sections (implementation slices)

### §1 — The toolbox's direct I/O is measured before it ships

The direct-I/O toolbox was built as drafted: `LO_FLAGS_DIRECT_IO` in the one `LOOP_CONFIGURE` call, and the hash read's cache dropped with `posix_fadvise(DONTNEED)`. Dimension 1.4 then measured it. Direct I/O failed the 10% p99 rule on all 8 first-use steps, so Indy reverted it (Discovery).

The toolbox attaches buffered, with `LO_FLAGS_READ_ONLY`, `LO_FLAGS_AUTOCLEAR` and a 4096-byte block size. Admission reads `LOOP_GET_STATUS64` back and logs `caching: buffered`. Adoption accepts a loop device with or without the direct-I/O flag, since it checks only the read-only bit.

`scripts/toolbox/measure_first_use.sh` mounts with buffered `-o loop` today. It gains `TOOLBOX_LOOP_DIRECT_IO=on`, which attaches with `losetup --direct-io=on --read-only` before mounting. One image is measured both ways on the dev host, buffered as the baseline, under the §Toolbox rule of no more than a 10% p99 regression.

- **Dimension 1.1** — The toolbox's loop device reports direct I/O and reading every file leaves the image under 1% cached → Test `test_toolbox_loop_reads_past_the_host_cache` — DROPPED with the direct-I/O toolbox; the owner's decision is in Discovery
- **Dimension 1.2** — A buffered toolbox device admits its toolbox and logs `caching: buffered` → Test `test_toolbox_buffered_fallback_is_logged` — DONE
- **Dimension 1.3** — A toolbox mount from the previous runner, buffered or direct, is adopted → Test `test_direct_io_toolbox_is_adopted` — DONE
- **Dimension 1.4** — The toolbox goes direct only if no first-use step regresses past 10% at p99 → Test `manual_first_use_holds_after_direct_io` — DONE: all 8 steps regressed, so the toolbox stays buffered (Discovery)
- **Dimension 1.5** — The harness attaches with direct I/O when asked → Test `direct_io_when_asked`, a case of `scripts/toolbox/test_measure_first_use.sh` — DONE
- **Dimension 1.6** — The harness refuses a device that did not take direct I/O, with no report and no device left → Test `direct_io_refused`, a case of the same script — DONE

### §2 — The executor speaks over an inherited socket pair

The engine makes a connected Unix socket pair with `SOCK_CLOEXEC` before it spawns bubblewrap. Only the child's end loses close-on-exec, inside the `pre_exec` hook, so no other spawn on the host inherits it. The runner drops its copy of the child's end once bubblewrap starts. The descriptor number joins the entry's arguments after the tenant descriptors.

The entry sets close-on-exec on its end again before hardening, so no tenant process inherits the link. The server serves a connected stream, whether inherited here or accepted from a listener as the unsandboxed engine and a future vsock engine need. The ready check is a `list_dir` of the workspace within the same ten seconds, or bubblewrap's exit with its error stream. The `run/` directory, its ownership change and its read-write bind leave the sandbox.

- **Dimension 2.1** — A new sandbox answers with no socket file on the host or inside → Test `test_executor_answers_over_the_inherited_pair` — DONE
- **Dimension 2.2** — A tenant process holds no socket descriptor → Test `test_tenant_process_holds_no_executor_socket` — DONE
- **Dimension 2.3** — Another lease's concurrent spawn never inherits the socket → Test `test_concurrent_spawn_inherits_no_other_socket` — DONE
- **Dimension 2.4** — An entry that exits before answering is refused with its error stream → Test `test_dead_entry_is_refused_with_its_reason` — DONE
- **Dimension 2.5** — The sandbox holds no writable bind beside `/workspace` and `/tmp` → Test `test_sandbox_has_no_socket_directory_bind` — DONE

### §3 — The workspace disk is attached and mounted in-process

`WorkspaceDisk::create` keeps the sparse image and `mke2fs`. It then attaches the image read-write through `LOOP_CONFIGURE` with `LO_FLAGS_AUTOCLEAR`, `LO_FLAGS_DIRECT_IO` and a 4096-byte block size. It mounts ext4 `nosuid,nodev` with `mount(2)` while it still holds the device descriptor, then closes it.

Autoclear means unmounting frees the device, so the release and the boot sweep keep their order. The separate `LOOP_SET_DIRECT_IO` call goes. `mount(8)` and `MOUNT_PATH` leave `HostTools` and the crate's exports.

- **Dimension 3.1** — Building a lease disk starts no `mount` program → Test `test_workspace_disk_starts_no_mount_program` — DONE
- **Dimension 3.2** — Releasing the disk leaves no loop device attached → Test `test_released_disk_leaves_no_loop_device` — DONE
- **Dimension 3.3** — A refused mount leaves no device and no files → Test `test_refused_mount_leaves_nothing` — DONE
- **Dimension 3.4** — A buffered fallback is reported when the host refuses direct I/O → Test `test_disk_reports_buffered_fallback` — DONE

### §4 — The architecture page tells the new start

`runner_execution.md` §Toolbox, §"A lease's sandbox today" and Parts A.1, D and E change to the built calls. The Section records the budgets trial's cold start before and after, with each host's kernel.

- **Dimension 4.1** — Parts A.1, D and E match the code at merge → Test `manual_walkthrough_matches_code_after_engine_changes` — DONE: walked at `5ea5b8839` by the final standards review, whose two drifts were fixed in `4c091806d`; `make check-architecture-doc`: all 513 cited source paths resolve

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
| Direct I/O refused | capacity | Backing file system rejects direct I/O | The workspace disk stays buffered and logs `sandbox_workspace_buffered` | `test_disk_reports_buffered_fallback` |
| Socket leaks to tenant | security | Close-on-exec left clear on the entry's end | Entry sets it before hardening; tenant listing shows no socket | `test_tenant_process_holds_no_executor_socket` |
| Socket leaks across leases | security | Close-on-exec cleared in the parent | Cleared only in `pre_exec`; concurrent spawns see none | `test_concurrent_spawn_inherits_no_other_socket` |
| Entry dies early | failure | Hardening refused or binary missing | Sandbox refused with its error stream within ten seconds | `test_dead_entry_is_refused_with_its_reason` |
| Mount refused | failure | Kernel rejects the ext4 mount | Device freed by closing its descriptor, image removed, lease refused | `test_refused_mount_leaves_nothing` |
| Device pinned after release | capacity | Autoclear missing on a read-write device | Autoclear set at configure; release check finds no device | `test_released_disk_leaves_no_loop_device` |
| First use slower | capacity | Cold reads go to disk instead of cache | p99 measured; every step regressed past 10%, so the toolbox stays buffered | `manual_first_use_holds_after_direct_io` |

## Invariants

1. The kernel parses only hashed bytes — `attach` takes the `File` that was hashed, as today.
2. No loop device outlives its mount — every attach sets `LO_FLAGS_AUTOCLEAR`, enforced by `attach` building the flags itself.
3. The executor link reaches no tenant process — the entry's adopt step sets close-on-exec before `harden` runs, and `harden` refuses once a second thread exists.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|----------|------------|--------------------|---------------|------------|
| `sandbox_toolbox_admission_completed` field `caching` | operator | Toolbox mounted | digest, `direct` or `buffered` | none needed | `test_toolbox_buffered_fallback_is_logged` |
| `sandbox_workspace_buffered` | operator | A disk stays buffered | lease id | none needed | `test_disk_reports_buffered_fallback` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | kernel | `test_toolbox_loop_reads_past_the_host_cache` | DROPPED with the direct-I/O toolbox |
| 1.2 | unit | `test_toolbox_buffered_fallback_is_logged` | A buffered device → `caching: buffered` logged, admission succeeds |
| 1.3 | unit | `test_direct_io_toolbox_is_adopted` | Backing flags read-only plus direct → adopted |
| 1.4 | manual | `manual_first_use_holds_after_direct_io` | Buffered run, then a `TOOLBOX_LOOP_DIRECT_IO=on` run against it on the OrbStack lane → a step past 1.10 p99 ratio keeps the toolbox buffered |
| 1.5 | unit | `direct_io_when_asked` | Mode on with direct I/O → report says `loop: direct` |
| 1.6 | unit | `direct_io_refused` | Mode on, the device reads `dio 0` → sampler exits non-zero with no report and no device |
| 2.1 | kernel | `test_executor_answers_over_the_inherited_pair` | New sandbox → no `executor.sock` on host or inside |
| 2.2 | kernel | `test_tenant_process_holds_no_executor_socket` | `ls -l /proc/self/fd` in `shell` → no socket entries |
| 2.3 | kernel | `test_concurrent_spawn_inherits_no_other_socket` | Two sandboxes start at once on several workers → no process of one lease holds the other's link |
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
| R1 | Buffered toolbox, socket pair and in-process disk proven on the lane (§1 to §3) | `make test-runner-kernel` | exit 0 | P0 | ✅ `test result: ok. 69 passed; 0 failed` at `c24c4b481` |
| R2 | The same trials pass on Debian 13: the OrbStack machine `afr-kernel` stands in for the dev host (Discovery) | `make test-runner-kernel` on `afr-kernel`, `uname -r` and output in Session Notes | exit 0 | P0 | ✅ on `afr-kernel`, `7.2.12-orbstack-00394-gbecdd6130068`: `69 passed; 0 failed` |
| R3 | The toolbox goes direct only within 10% p99 of buffered (§1); `IMAGE` is the admitted toolbox; baseline from the same command without the variable | `sudo TOOLBOX_LOOP_DIRECT_IO=on bash scripts/toolbox/measure_first_use.sh "$IMAGE" first-use-direct.json 120 first-use-buffered.json` | `0` steps marked `REGRESSION`, or the toolbox attaches buffered | P1 | ✅ 8 of 8 steps `REGRESSION`, so the toolbox attaches buffered (`Access::direct_io`) |
| R4 | Start figures recorded (§4) | `grep -c '^- \*\*Measurements\*\*' docs/v2/done/M237_002_P2_DOCS_INFRA_OBS_SANDBOX_START_DROPS_POLLING_AND_DOUBLE_CACHE.md` | `1` | P2 | ✅ `1` |
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
- **Outside Files Changed, now listed** — the entry's callers, `runner_fleet.md`, the `harden` comment and the host tests are in Files Changed, which Indy approved on Nov 03 (M237_001 Discovery).
- **Probe unchanged** — `probe/mod.rs` reads the host before any toolbox is attached, so it has no device to report on. Admission's `sandbox_toolbox_admission_completed` carries `caching` instead.
- **Read-write device opened read-only** — `LOOP_CONFIGURE` makes a device read-only when the handle configuring it cannot write. Every workspace disk came up read-only and its ext4 mount was refused, which the kernel lane caught (`55cfb02e7`).
- **Small disks had 1 KiB blocks** — the attach gives every loop device 4096-byte sectors, and `mke2fs` picks 1 KiB blocks for a small disk. ext4 refused those mounts ("bad block size 1024"), so `mke2fs` now gets `-b 4096` (`beeacb8b0`).
- **A cancelled mount** — the `spawn_blocking` mount runs to its end even when the start is dropped. Its output is a guard that undoes the disk unless the start took it, so a dropped start leaks no mount (`test_a_mount_nobody_heard_is_undone`).
- **Mount seam** — `HostTools.mount` is a plain `fn`, `loop_mount` in production. The unit tests' fake host mounts nothing through it, as it once ran `/bin/true` as `mount`.
- **Sampler test names** — Dimension 1.5 is proven by the `direct_io_when_asked` and `direct_io_refused` cases of `scripts/toolbox/test_measure_first_use.sh`.
- **Unit tests take the link with `pidfd_getfd`** — the fake bubblewrap notes its pid and the link's number in its lease directory. The in-process executor then takes the descriptor from that child, so no production seam exists for tests.
- **First use, direct against buffered (Nov 03, OrbStack lane)** — three runs of 120 a mode, pooled to 360 a step. Direct I/O was 3 to 12% faster at p50 on all 8 steps and 1.12 to 2.98 times slower at p99 on all 8. The worst were `uv` 27.3 → 81.2 ms, `git_commit` 19.2 → 48.1 ms and `jq` 13.4 → 30.5 ms; `node_test` went 278.1 → 409.5 ms. The double cache direct I/O saved is one image of at most 160 MB a host.
- **The revert** — the toolbox attaches buffered again, and admission drops no cache. The workspace disk keeps direct I/O. The caching trial went with it: it proved only the direct-I/O toolbox, so `caching.rs` was added and removed on this branch.
- **The release half of `parts.rs`** — the socket pair took `parts.rs` past 350 lines. Its release half moves to `parts/teardown.rs`, and `parts.rs` becomes `parts/mod.rs`, so `docs/AUTH.md` cites the new path.
- **Sandbox ready under 10 ms** — a fresh build reads 42.3 ms at p50, cold; a lease that took a warm slot read 1.17 ms (`test_warm_start_beats_cold_start`, p50 of 5). Nothing times the steps inside a prepare yet. Indy chose one follow-up spec after M237 merges: time each step, wire warm slots for default-sized leases, and shorten the lease-poll wait, at least 250 ms today (`worker_pool/mod.rs:30`).
- **Performance a durable task can watch** — every figure the follow-up cuts becomes an exported signal, so a later regression shows as a moved percentile on Grafana rather than a re-run of the kernel lane. Today the runner exports build, wait, release and model durations and the build outcome count; `runner_execution.md` lists which answers which question. Three have no signal yet: admission to lease, lease to first model request, and each prepare step. The follow-up adds them, with a baseline per release that a durable task compares against.
- **Measurements** — the kernel lane's `test_start_budgets_with_four_leases`, 100 leases 4 at once, on the same OrbStack machine, toolbox and trial for every row. Executor ready, p50 / p95 / p99:

  | Revision | What it had | Cold | Warm |
  |---|---|---|---|
  | `937c8d34d` | before M237 | 100.8 / 321.6 / 442.7 ms | 95.7 / 119.3 / 122.3 ms |
  | `beeacb8b0` | the in-process disk, no socket pair | 37.7 / 49.4 / 51.8 ms | 32.2 / 41.4 / 57.9 ms |
  | `e7c03a27d` | the socket pair and a direct-I/O toolbox | 42.1 / 57.4 / 59.4 ms | 30.8 / 39.7 / 43.3 ms |
  | `c24c4b481` | what ships: the toolbox buffered | 42.3 / 54.5 / 107.9 ms | 30.4 / 36.3 / 60.1 ms |

  The drop is the in-process disk. Each row is one run, so a p99 is one lease's figure, not a measured slow-down. The socket pair's gain is the removed poll and socket directory, not a measured speed-up. The first useful command (`git --version`) went from 5.2 / 12.6 / 13.8 ms cold to 4.9 / 6.0 / 6.6 ms, and from 1.4 / 2.0 / 2.1 ms warm to 0.9 / 1.9 / 3.1 ms.
- **The direct-I/O sampler case's bound** — `direct_io_when_asked` runs the sampler's minimum of 20 runs past the host cache: 41 s alone on the OrbStack lane, and past the shared 90 s bound when other cases ran first. That one case now has 300 s; every other case keeps 90 s.
- **The kernel lane's stack overflow (Nov 03)** — `test_start_budgets_with_four_leases` aborted: a tokio blocking-pool thread overflowed its stack. A core dump held 8,392 frames of one loop. `Parts::drop` hands its release to the blocking pool; a stopping runtime's pool refuses by dropping the task on the caller's stack; the task still held the parts, whose drop handed them to the pool again. The loop predates M237 (`parts.rs:281` on `2f8c878c0`) and is in the code a release build runs, so any runtime that stops with such a release still queued meets it. A refused release now runs where it is refused, as it does with no runtime; `test_parts_a_stopped_runtime_refuses_are_released_where_they_drop` overflowed before the fix and passes after.
  The start failure that stopped the lane's runtime came from §3's in-process attach. A device `LOOP_CTL_GET_FREE` creates may have no `/dev` node yet, and the attach opened it once. util-linux, which `mount -o loop` used, retries that open on `ENOENT` or `EACCES` every 25 ms up to 16 times, and a `LOOP_CONFIGURE` answering `EAGAIN` every 250 ms up to 10 times (`lib/loopdev.c`, `loopcxt_setup_device`); the attach now does both. The ext4 `error -4` the machine logged was the abort's own signal reaching a mount in flight.
- **Open blockers** — none.
- **Metrics review** — `caching` added to the admission event.
- **Skill-chain outcomes** — pending until VERIFY and REVIEW.
- **Deferrals and transfers** — Dimension 1.1, with the direct-I/O toolbox it proved:

> Owner (Indy, Nov 03, 2026 08:53 AM): "Revert to buffered (Recommended)" — context: Dimension 1.4 measured direct I/O failing the 10% p99 rule on all 8 first-use steps, so the toolbox loop device stays buffered and Dimension 1.1's direct-I/O trial goes.

> Owner (Indy, Nov 03, 2026 12:07 PM): "300 s for that case (Recommended)" — context: the kernel lane's sampler test timed out `direct_io_when_asked` at 90 s twice; only that case's bound moves.

> Owner (Indy, Nov 03, 2026 11:36 AM): "I want to monitor the observability, logging, metrics across so ensure we can do that to answer the performance questions. I wanna run Durable Task to figure out performance regression later using agentsfleet. so ensure you think in that way and record." — context: the follow-up to M237 and every later performance change.

> Owner (Indy, Nov 03, 2026 10:07 AM): "All three (Recommended)" — context: a follow-up spec after M237 merges to bring sandbox-ready under 10 ms: per-step timers, warm slots for default-sized leases, and a shorter lease-poll wait.

> Owner (Indy, Nov 02, 2026 10:52 PM): "coudl we do both specs in this PR, if you need to measure do it in local? in the docker compose using make targets" — context: whether M237_001 and M237_002 ship together, and where the start figures are measured.
