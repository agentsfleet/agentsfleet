<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M211_003: A lease's sandbox survives its own exhaustion — `/tmp` fills to `ENOSPC` on the workspace disk, memory pressure kills only the tenant's process, the workspace disk writes without a second page cache, and a lease names its sandbox's size

**Prototype:** v2.0.0
**Milestone:** M211
**Workstream:** 003
**Date:** Oct 05, 2026
**Status:** DONE
**Priority:** P1 — spike S6: one command that fills `/tmp` or memory takes `bwrap` and the whole sandbox down, and every lease gets the same size of sandbox
**Categories:** API, INFRA
**Batch:** B2 — folds into M211_002 at that stream's CHORE(open), the milestone's follow-up Pull Request; its Sections run after M211_001's §8
**Branch:** feat/m211-nested-loops-and-chat-continuity
**Folded-into:** `M211_002`
**Baseline revision:** bb007001545cb97f4dc27c9325235a6a0ebb4fb9
**Test Baseline:** unit=4212 integration=4929 — Rust unit 4212 passed, 0 failed, 879 ignored (runner 982 · daemon 132 · daemon libraries 3098); integration through the coverage shards 4929 passed, 0 failed (substrate 4158 · runner 550 · daemon 221); TypeScript app 3642, design-system 647, website 142 passed, cli 1779 passed and 17 skipped, at `bb0070015` via PR #732's identical tree. The branch at `886733be6`: Rust unit 4312 passed, 0 failed, 895 ignored (+100); integration 868 + 2 exclusive passed, 0 failed; kernel lane 34 passed.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M211-bb0070015.md`
**Depends on:** M211_001 (§1's processes and §8's `LOOP_CONFIGURE` attach, which the workspace disk reuses) · spike S6's evidence in `docs/v2/reviews/m211-toolbox-spikes.md`
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 05, 2026) from spike S6's kernel log and runs
**Canonical architecture:** `docs/architecture/runner_execution.md` §"Sandbox engines", §"Workspace between leases"

---

## Overview

**Goal (testable):** `test_writable_state_exhaustion_spares_the_sandbox` — four leases fill `/tmp` and then `/workspace` at once, each writing twice its memory limit; each `dd` ends in `No space left on device`, no process of the sandbox's own leaf is killed, and each lease's executor runs a new command afterwards.
**Problem:** Spike S6 ran four leases that filled their disks and `/tmp`. Filling `/tmp`, an in-memory tmpfs larger than the lease's 2 GiB memory limit, ended every time in the kernel's out-of-memory killer taking `bwrap`, the sandbox's first process, so the whole sandbox died. One lease was killed while writing to its workspace, before the disk filled. Nothing refused leases when four 4 GiB workspaces took the host's disk from 20 GB free to 5.9 GB.
**Solution summary:** `/tmp` moves onto the workspace disk, so it fills to `ENOSPC` and shares the lease's disk limit. The lease cgroup splits into a leaf for `bwrap` and the executor and a leaf for every tenant process, whose memory limit sits just below the lease's, so the killer can only pick a tenant process; the call it killed says `out_of_memory`. The workspace disk attaches with direct I/O, so its writes are not cached twice. A lease names its sandbox's size (processor, memory, disk) within declared bounds, and a lease that names none gets the runner's defaults.

## PR Intent & comprehension handshake

- **PR title (eventual):** folded into M211_001's Pull Request
- **Intent (one sentence):** A tenant command that exhausts its lease's disk or memory gets an error it can read, and the sandbox, its other processes and the host all carry on.
- **Handshake** (PLAN, Oct 07, 2026) — restated: a command that fills its lease's disk or memory reads an error it can act on, and the sandbox, the other processes in it and the host carry on. Matches the Intent. `ASSUMPTIONS I'M MAKING:` (1) `/tmp` is the disk's `tmp/` directory bound at `/tmp`, never a sized tmpfs; the disk's root holds `workspace/` and `tmp/`, so `mke2fs`'s `lost+found` leaves `/workspace`, and the host-side clone target (`HostWorkspace::root`) becomes `<mount>/workspace`. (2) The lease cgroup becomes an inner node: `cgroup.subtree_control` gains `+memory +pids +cpu +io`, the limits stay on the node as today, and two leaves hang under it, `sandbox` (bubblewrap enters it, since a node with children may hold no process) and `tenant` with `memory.max = memory_bytes − SANDBOX_MEMORY_RESERVE_BYTES`. (3) The engine opens `tenant/cgroup.procs` write-only and `tenant/memory.events` read-only, keeps both open across bubblewrap's exec, and names their numbers to the sandbox entry as `sandbox --tenant-procs N --tenant-events M`; the executor marks both close-on-exec on arrival and each launcher's pre-exec hook writes `0` to the first, so no tenant process inherits either. `portable_pty` exposes no hook (`SlavePty` is `spawn_command` alone, `portable-pty-0.9.0/src/lib.rs:165`), so the terminal launcher opens its pair through `rustix::pty` and spawns with the same hook, and the crate leaves `afr_executor`. Both launchers check the descriptor before spawning, so a lost one refuses with its cause and starts nothing. (4) After a `Signaled(SIGKILL)` ending the executor reads `oom_kill` from the events descriptor and answers `Ending::OutOfMemory` when it rose since its last read; `afr_tools` renders it as `ToolErrorCode::OutOfMemory`, exit code 137, status `out of memory`. (5) The workspace image attaches through M211_001's `loop_device::attach` given flags, `LO_FLAGS_DIRECT_IO | LO_FLAGS_AUTOCLEAR`, and mounts the device node; a kernel that refuses direct I/O on the backing file system (`EINVAL`) gets a buffered attach, and the probe reports `workspace_direct_io` from one attach at boot. (6) The capacity rule needs no registry: `StateDisk` reads `statvfs` of the state directory and walks `<state>/*/workspace.img`, each image's `st_size` being its limit and `st_blocks × 512` what it holds, so leases, warm slots and held sandboxes count alike and the rule survives a restart; the worker pool and the warm keeper hold it as `Arc<dyn Admission>`, a worker short of room sleeps `MIN_POLL_PAUSE` and asks again, and `sandbox_capacity_short` logs once per transition. (7) Kernel proofs run on `afr-kernel` (OrbStack, running) through `make test-runner-kernel`; the unit lanes on this Mac cover 2.4, 4.1, 4.2, the argument builder and the capacity arithmetic. (8) M214_001 is in `done/`, so the two counters go into `docs/metrics.runner.census.tsv` and `afr_telemetry::record`, not into a done spec. **Quality ceiling:** the walk of sparse images is leaner than a registry and cannot drift from what is on disk; dropping `portable_pty` removes a dependency for forty lines of `rustix`; direct I/O on the loop device is the kernel's own answer to the double cache. **Surface checklist:** OpenAPI no · the product CLI no (the `sandbox` entry's two flags are internal) · user docs: the runners page's limits paragraph, checked at DOCUMENT · release/version at close · schema no · spec vs rules: Files Changed gains `afr_executor/src/server/launch/tenant.rs`, `loop_device.rs`, `probe.rs`, `docs/metrics.runner.census.tsv` and `afr_telemetry`, amended with the first Section.

## Implementing agent — read these first

1. `docs/v2/reviews/m211-toolbox-spikes.md` §S6 — what failed, the kernel log's victims, and the two harness traps.
2. `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` — how `bwrap` enters its cgroup through a `cgroup.procs` descriptor the engine opened; the tenant move mirrors it.
3. `rustd/crates/afr_executor/src/server/launch.rs` — the two launchers (pipes, terminal) every tenant process starts from.
4. `rustd/crates/afr_sandbox/src/cgroup.rs`, `rustd/crates/afr_sandbox/src/workspace_disk.rs` — the lease cgroup and the workspace disk this spec reshapes.
5. https://docs.kernel.org/admin-guide/cgroup-v2.html — "no internal process" rule, `memory.max`, `memory.events`, delegation and migration permission.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_sandbox/src/workspace_disk.rs` | EDIT | The disk holds `workspace/` and `tmp/`; its loop device switched to direct I/O after the mount |
| `rustd/crates/afr_sandbox/src/toolbox.rs`, `rustd/crates/afr_sandbox/src/toolbox/loop_device.rs`, `rustd/crates/afr_sandbox/src/toolbox/adopt.rs` | EDIT | `direct_io` and `node` join the loop-device module, which the workspace disk now shares with adoption |
| `rustd/crates/afr_sandbox/src/probe.rs`, `rustd/crates/afr_sandbox/src/probe/tests.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine/tests.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine/tests/support.rs`, `rustd/crates/afr_supervisor/src/capability.rs`, `rustd/crates/afr_supervisor/src/capability/tests.rs`, `rustd/crates/afr_supervisor/src/lib_tests.rs`, `rustd/crates/afr_supervisor/src/heartbeat/tests.rs`, `rustd/crates/agentsfleetd/tests/integration_rust_runner.rs`, `rustd/crates/agentsfleetd/tests/support/bundle_run.rs` | EDIT | The probe's `state_dir` and `workspace_direct_io`; the `workspace_direct_io` self-test check; every stated probe names the new fact |
| `rustd/crates/afr_sandbox/src/bubblewrap.rs` | EDIT | `/tmp` binds the disk's `tmp/` instead of a tmpfs |
| `rustd/crates/afr_sandbox/src/cgroup.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine.rs`, `rustd/crates/afr_sandbox/src/cgroup/tests.rs` | EDIT | The `sandbox` and `tenant` leaves; `bwrap` enters `sandbox`; the tenant leaf's two descriptors are opened by the engine and let through bubblewrap's exec |
| `rustd/crates/afr_sandbox/src/tenant.rs`, `rustd/crates/afr_sandbox/src/tenant/files.rs`, `rustd/crates/afr_sandbox/src/tenant/tests.rs` | CREATE | The descriptors from engine to entry: `TenantFiles` opens and lets them through; `TenantDescriptors` names them as `--tenant-procs`/`--tenant-events` and the entry adopts them |
| `rustd/crates/afr_sandbox/src/serve.rs`, `rustd/crates/afr_sandbox/src/serve/tests.rs`, `rustd/crates/afr_sandbox/src/bubblewrap.rs`, `rustd/crates/afr_sandbox/src/lib.rs`, `rustd/crates/afr_sandbox/src/error.rs`, `rustd/crates/afr_sandbox/src/error/raise.rs`, `rustd/crates/afr_sandbox/Cargo.toml` | EDIT | The entry adopts the tenant leaf before it hardens; bubblewrap names the descriptors; `NotInherited` refuses a number the entry does not hold |
| `rustd/crates/agentsfleet_runner/src/main.rs`, `rustd/crates/agentsfleet_runner/src/main_tests.rs`, `rustd/crates/afr_sandbox/examples/kernel_lane/lane.rs`, `rustd/crates/afr_sandbox/examples/kernel_lane/main.rs` | EDIT | Both sandbox entries take the tenant flags; the runner's refuses to start without them |
| `rustd/crates/afr_executor/src/server/launch.rs`, `rustd/crates/afr_executor/src/server/launch/terminal.rs`, `rustd/crates/afr_executor/src/server/process.rs`, `rustd/crates/afr_executor/src/server/session.rs`, `rustd/crates/afr_executor/src/server.rs`, `rustd/crates/afr_executor/src/lib.rs`, `rustd/crates/afr_executor/src/server/process/tests.rs` | EDIT | Each tenant process moves itself into `tenant` before `exec` through the session's `Placement`; a failed move refuses the spawn; the terminal opens its pair through `rustix::pty` so it takes the same hook |
| `rustd/crates/afr_executor/src/server/launch/tenant.rs`, `rustd/crates/afr_executor/src/server/launch/tenant/tests.rs` | CREATE | `Placement` (`Tenant`, `Inherit`): the move, the descriptor check, and an ending judged `OutOfMemory` when the leaf's `oom_kill` rose |
| `rustd/crates/afr_executor/src/error.rs`, `rustd/crates/afr_executor/src/error/raise.rs`, `rustd/crates/afr_executor/src/error/tests.rs`, `rustd/crates/afr_executor/Cargo.toml`, `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | `TenantUnavailable` keeps the kernel's reason; `ProgramUnavailable`, raised only by `portable_pty`'s lookup, leaves with it |
| `rustd/crates/afr_executor/src/api.rs`, `rustd/crates/afr_executor/tests/link.rs`, `rustd/crates/afr_tools/src/runtime.rs`, `rustd/crates/afr_tools/src/runtime/tests.rs`, `rustd/crates/afr_tools/src/sandbox/output.rs`, `rustd/crates/afr_tools/src/sandbox/output/tests.rs`, `rustd/crates/afr_tools/src/sandbox/oneshot.rs`, `rustd/crates/afr_tools/src/sandbox/shell/tests.rs` | EDIT | An ending killed for memory is `out_of_memory`, a `ToolErrorCode` the model reads, exit code 137, logged `sandbox_out_of_memory` |
| `rustd/crates/afd_wire/src/lease.rs`, `rustd/crates/afd_wire/tests/validation_lease.rs`, `rustd/crates/afd_wire/tests/wire_suite.rs`, `rustd/crates/afd_fleet/src/lease/answer.rs`, `public/openapi.json` | EDIT / CREATE | `SandboxLimits` and its bounds; `LeasePayload.limits`, null from the daemon until a fleet carries a size; the regenerated document |
| `rustd/crates/afr_supervisor/src/lease_loop/workspace.rs`, `rustd/crates/afr_supervisor/src/lease_loop/workspace_tests.rs`, `rustd/crates/afr_supervisor/src/error.rs`, `rustd/crates/afr_supervisor/src/error/raise.rs`, `rustd/crates/afr_supervisor/src/test_support/sandbox.rs`, `rustd/crates/afr_supervisor/Cargo.toml` | EDIT | The runner builds the size a lease names, or its own; a size past the bounds refuses the lease (`LeaseSize`) |
| `rustd/crates/afr_sandbox/examples/kernel_lane/trials.rs`, `rustd/crates/afr_sandbox/examples/kernel_lane/run.rs` | EDIT | The kernel proofs, S6 among them; `in_sandbox_each` runs several scripts on one sandbox, since what the executor does after an exhaustion is only seen there |
| `rustd/crates/afr_sandbox/examples/kernel_lane/exhaustion.rs`, `rustd/crates/afr_sandbox/examples/kernel_lane/exhaustion_concurrent.rs`, `rustd/crates/afr_sandbox/examples/kernel_lane/shared_memory.rs` | CREATE | The exhaustion trials, S6 and the `/dev/shm` fill, apart from `trials.rs` so none passes the length cap |
| `rustd/crates/afr_sandbox/src/engine.rs`, `rustd/crates/afr_sandbox/src/bubblewrap.rs`, `rustd/crates/afr_sandbox/src/bubblewrap/tests.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine.rs`, `rustd/crates/afr_sandbox/src/workspace_disk.rs`, `rustd/crates/afr_sandbox/src/toolbox/loop_device.rs`, `rustd/crates/afr_sandbox/src/lib.rs` | EDIT | `/dev/shm` sized to a quarter of memory; `Caching` says when a disk ran buffered, and the engine logs it with its lease |
| `rustd/crates/afr_executor/src/server/launch/tenant.rs`, `rustd/crates/afr_executor/src/server/launch/tenant/tests.rs` | EDIT | A shell's exit 137 is judged like a kill; an unreadable `memory.events` is logged |
| `rustd/crates/afr_sandbox/src/bubblewrap/tests.rs`, `rustd/crates/afr_sandbox/src/workspace_disk/tests.rs` | EDIT | The argument builder binds `tmp/`; the disk's two directories and their modes |
| `docs/architecture/runner_execution.md` | EDIT | §"Sandbox engines": the two leaves, `/tmp` on the disk, the lease's size |
| `docs/metrics.runner.census.tsv`, `rustd/crates/afr_telemetry/src/{families.rs,record.rs,testing.rs}`, `rustd/crates/afr_telemetry/src/families/tests.rs` | EDIT | The runner census gains `agentsfleet_runner_tool_out_of_memory_total`; the tool family's ceiling follows the catalog's 40 labels (39 published plus `_other`, after the nested-loop tools) |
| `rustd/crates/afr_agent/src/spans.rs`, `rustd/crates/afr_agent/src/ledger.rs`, `rustd/crates/afr_agent/src/ledger/tests.rs` | EDIT | `execute_tool` carries `error.type` from the call's closed code; a call killed for memory is counted |
| `rustd/crates/afr_sandbox/src/{cgroup.rs,cgroup/tests.rs,error.rs,error/raise.rs,tenant/files.rs,tenant/tests.rs,workspace_disk.rs,workspace_disk/tests.rs,bubblewrap_engine/tests/prepare.rs,unsandboxed/tests.rs}`, `rustd/crates/afr_executor/tests/{orphans,terminal}.rs` | EDIT | The tenant leaf's `memory.high` sits an eighth below its `memory.max`; every failed workspace disk build unmounts before it removes files; a cgroup file that cannot be read is named as unread, and `Error::detail()` gives its sentence; the unit-test ledger's sandbox and executor gaps |
| `rustd/crates/afr_sandbox/examples/kernel_lane/{filesystems,forked_kill}.rs`, `rustd/crates/afr_sandbox/examples/kernel_lane/{admission,confinement}.rs` | CREATE / EDIT | The failed-mount, buffered-disk, direct-I/O probe, short-disk and shell-reported-kill trials; `confinement.rs` finds `/workspace` under the disk's `workspace/` directory |
| `rustd/crates/afr_tools/src/sandbox/exec_session/{tests,write_tests}.rs` | EDIT | A session killed for memory, in `exec_command` or `write_stdin`, logs the lease's own id |
| `docs/v2/done/M211_003_P1_API_INFRA_RUST_RUNNER_SANDBOX_SURVIVES_EXHAUSTION.md`, `docs/v2/pending/M211_003_P1_API_INFRA_RUST_RUNNER_SANDBOX_SURVIVES_EXHAUSTION.md` | CREATE / DELETE | This spec, moved from `pending/` at CHORE(open) and to `done/` at CHORE(close) and to `done/` at CHORE(close) |
| `rustd/crates/afr_executor/src/server/tests.rs` | EDIT | A listener with a tenant places its processes in the leaf, so a counted kill reads as out of memory |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (reserves, size bounds and directory names are constants), OWN (one owner per descriptor; the tenant descriptor is close-on-exec), LOG, ERR-RS, TGU (an ending is one enum), FLL, TST-NAM, NDC.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — a refused move keeps its `io::Error` cause; `pre_exec` stays async-signal-safe (one `write`, no allocation).
- `docs/LOGGING_STANDARD.md` §8A — `sandbox_size_refused` carries the lease id and `error_code`, never a path a tenant chose.
- `docs/REST_API_DESIGN_GUIDELINES.md` §6 — the document is regenerated, never hand-edited. `limits` writes `null` when absent, as `bundle` does: `afd_wire`'s rule (no `skip_serializing_if`) governs the runner protocol over §3's omit-the-key.
- Indy's Rust bar (Oct 04 and Oct 05, 2026): traits and trait objects where behaviour varies (the sandbox engine stays the `&dyn Engine` the lease loop holds, so tests read the size it was asked for), structs that own their behaviour, borrows over clones, ownership over `Mutex`, `Fn`/`FnMut`/`FnOnce` where behaviour is passed, `afd_core::error_shell!`, `afd_observability` for spans and metrics, established crates over hand-rolled code (`garde` for the size bounds, `rustix` for the `write` in `pre_exec` and the loop ioctls; `nix` nowhere new), no duplicated logic (the workspace attach reuses M211_001 §8's `LOOP_CONFIGURE` helper), `afd_core` reused for shared constants.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | `error_shell!` per crate, causes preserved |
| UFS / LOGGING / MILESTONE-ID | yes | Named reserves, size bounds and leaf names; scoped events; no milestone identifiers in source |
| File & Function Length (≤350/≤50/≤70) | yes | `workspace.rs` owns the sizing; `cgroup.rs` splits leaf handling out if it nears the cap |
| Architecture consult | yes | `runner_execution.md` §"Sandbox engines" edited in the docs commit that lands this spec |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` — a process entering its cgroup through a descriptor the engine opened; the tenant move is the same write, made by each tenant process before it execs.
- **Reference:** systemd's delegation model (`Delegate=` units keep the manager's own processes in a leaf apart from the delegated subtree) — the reason the sandbox's own processes and the tenant's never share a leaf.

## Sections (implementation slices)

### §1 — `/tmp` lives on the workspace disk — DONE

A fresh workspace disk gets two directories, `workspace/` (mode 0755) and `tmp/` (mode 1777), owned by the sandbox user; bubblewrap binds them at `/workspace` and `/tmp`, so `mke2fs`'s `lost+found` leaves `/workspace` too. Both share the lease's disk limit, so a full `/tmp` answers `ENOSPC` like a full workspace. **Implementation default:** the disk, not a sized tmpfs, because tmpfs pages stay charged to the lease's memory until every process in its mount namespace is gone, which is how S6's sandboxes died.

- **Dimension 1.1** DONE · PARKED for production (M213_001 Dimension 1.4) — Filling `/tmp` ends in `ENOSPC`, and the executor runs a new command afterwards → Test `test_full_tmp_answers_enospc`
- **Dimension 1.2** DONE · PARKED for production (M213_001 Dimension 1.4) — `/workspace` and `/tmp` draw on one disk limit, and `/workspace` holds no `lost+found` → Test `test_workspace_and_tmp_share_the_disk`
- **Dimension 1.3** DONE · PARKED for production (M213_001 Dimension 1.4) — `/dev/shm` holds a quarter of the sandbox's memory: filling it ends in `ENOSPC`, and the next command still allocates beside it → Test `test_full_shared_memory_spares_the_tenant`

### §2 — Memory pressure kills the tenant's process, never the sandbox — DONE

The lease cgroup becomes an inner node with two leaves: `sandbox` (bubblewrap and the executor, entered as today) and `tenant`, whose `memory.max` is the lease's limit minus `SANDBOX_MEMORY_RESERVE_BYTES`, so the tenant leaf runs out first and the killer chooses only inside it. The engine opens `tenant/cgroup.procs` write-only and passes the descriptor into the sandbox close-on-exec; each tenant process writes `0` to it in `pre_exec`, before `exec`, through the descriptor's open-time credentials, so no cgroup file system is mounted inside. A move that fails refuses the spawn: no tenant process ever runs in `sandbox`. An ending whose process the killer took, read from `tenant/memory.events`, completes the call `failed` with `out_of_memory`. **Implementation default:** a 64 MiB reserve, measured against the executor's resident memory on the kernel lane at PLAN.

- **Dimension 2.1** DONE · PARKED for production (M213_001 Dimension 1.4) — A process allocating past the limit is killed, and the executor runs a new command afterwards → Test `test_oom_kills_only_the_tenant`
- **Dimension 2.2** DONE · PARKED for production (M213_001 Dimension 1.4) — The killed call completes `failed` with `out_of_memory` → Test `test_oom_ending_reads_out_of_memory`
- **Dimension 2.3** DONE · PARKED for production (M213_001 Dimension 1.4) — A tenant process holds descriptors 0–2 and sits in `tenant` → Test `test_tenant_process_holds_no_cgroup_descriptor`
- **Dimension 2.4** DONE · PARKED for production (M213_001 Dimension 1.4) — A move that fails refuses the spawn, and nothing runs in `sandbox` but bubblewrap and the executor → Test `test_failed_tenant_move_refuses_the_spawn`
- **Dimension 2.5** DONE · PARKED for production (M213_001 Dimension 1.4) — Destroy and the boot sweep remove both leaves before the lease cgroup → Test `test_sweep_removes_both_leaves`
- **Dimension 2.6** DONE · PARKED for production (M213_001 Dimension 1.4) — A forked child killed for memory, which the shell reports as exit 137, reads `out_of_memory` when the leaf counted the kill, and keeps its exit when it did not → Test `a_shell_reporting_its_childs_kill_reads_as_out_of_memory_when_the_leaf_counted_it`

### §3 — The workspace disk writes past one page cache — DONE

The workspace image mounts through the host's `mount -o loop` as today, then `LOOP_SET_DIRECT_IO` switches the loop device the mount made, found by the mount point's device number through M211_001's `loop_device::node`; the device then reads and writes the image without caching it a second time on the host. A backing file system without direct I/O answers `EINVAL` and stays buffered. The probe, given the engine's state directory, opens an unnamed `O_DIRECT` file there and reports `workspace_direct_io`, a self-test check beside the toolbox file system's. **Implementation default:** switch after the mount rather than attach with `LOOP_CONFIGURE`, because the engine's unit tests stand a fake `mount` in for the real one and run without root.

- **Dimension 3.1** DONE · PARKED for production (M213_001 Dimension 1.4) — The workspace loop device reports direct I/O on → Test `test_workspace_disk_uses_direct_io`
- **Dimension 3.2** DONE · PARKED for production (M213_001 Dimension 1.4) — Writing 4 GiB to `/workspace` under the 2 GiB memory limit ends in `ENOSPC` with zero out-of-memory kills → Test `test_disk_fill_under_memory_limit_ends_in_enospc`

### §4 — A lease names its sandbox's size — DONE

`LeasePayload.limits` is a nullable `SandboxLimits` (`cpu_millis`, `memory_bytes`, `disk_bytes`), each field `garde`-bounded and its bounds published in the document. The runner proves a size before it builds anything: a lease past a bound ends at startup (`startup_posture`, logged `sandbox_size_refused`) with no sandbox prepared, a lease naming none gets the runner's own `Limits`, and the process cap stays the host's for every lease. A warm slot serves a lease only when its size is the slot's (`warm_slots.rs:110`); any other size is built fresh. The daemon sends `null` until a fleet carries a size; the Zig runner ignores the key (`control_plane_client_lease.zig:12`). **Bounds:** 250–32 000 thousandths of a core, 256 MiB–64 GiB of memory, 1–256 GiB of disk.

- **Dimension 4.1** DONE · PARKED for production (M213_001 Dimension 1.4) — A sized lease's sandbox enforces that size, with the host's process cap → Test `a_sized_lease_builds_the_size_it_asked_for`
- **Dimension 4.2** DONE · PARKED for production (M213_001 Dimension 1.4) — A lease naming no size builds the runner's defaults → Test `a_lease_without_a_size_builds_the_hosts_defaults`
- **Dimension 4.3** DONE — A lease from a daemon without the field decodes, sizeless, and re-encodes `null` → Test `a_lease_without_a_size_decodes_as_none`
- **Dimension 4.4** DONE · PARKED for production (M213_001 Dimension 1.4) — A size past a bound refuses the lease before any sandbox is prepared → Test `a_size_past_its_bounds_refuses_the_lease_before_any_sandbox`
- **Dimension 4.5** DONE · PARKED for production (M213_001 Dimension 1.4) — Each bound admits its limit and refuses one past it → Test `a_size_one_past_any_bound_is_refused`
- **Dimension 4.6** DONE — The published bounds are the enforced ones → Test `the_published_bounds_are_the_enforced_ones`

### §5 — S6, kept — DONE

Spike S6's scenario becomes a kernel trial on one shared engine, with lease state on disk: four leases fill `/tmp` and then `/workspace` at once, in `examples/kernel_lane/exhaustion_concurrent.rs`. Each lease has 1 GiB of disk and 512 MiB of memory, so every fill writes twice the memory limit, the ratio that killed S6's sandboxes through a tmpfs `/tmp`. S6's own 4 GiB per lease would need 16 GiB free on a lane host. The lane refuses to start with under 6 GiB free under `/var/tmp`, naming any state earlier runs left: on Oct 07, 2026 a host disk at 97% turned s6a's fill into `Killed` rather than `ENOSPC`, and the same trial passed once space was freed.

- **Dimension 5.1** DONE · PARKED for production (M213_001 Dimension 1.4) — Each lease gets `ENOSPC` on both, no `sandbox` leaf process is killed, and each executor runs a command afterwards → Test `test_writable_state_exhaustion_spares_the_sandbox`

## Interfaces

```
workspace disk        <lease>/disk/{workspace (0755), tmp (1777)}   → /workspace, /tmp
lease cgroup          <lease>/{sandbox, tenant}   tenant/memory.max = limit − SANDBOX_MEMORY_RESERVE_BYTES · tenant/memory.high = memory.max − min(memory.max / 8, TENANT_HIGH_HEADROOM_MAX_BYTES)
tenant move           pre_exec: write(fd, "0")   fd = tenant/cgroup.procs, opened by the engine, close-on-exec
ending                Exited | Signaled | OutOfMemory (Signaled(SIGKILL) or a shell's Exited(128+SIGKILL), when tenant memory.events oom_kill rose)
shared memory         /dev/shm  tmpfs --size Limits::shared_memory_bytes() = memory_bytes / 4
workspace caching     WorkspaceDisk::create → (disk, Caching::{Direct, Buffered})
ToolErrorCode         + OutOfMemory   wire "out_of_memory"
lease size            LeasePayload.limits: SandboxLimits { cpu_millis, memory_bytes, disk_bytes } | null → host Limits
constants             SANDBOX_MEMORY_RESERVE_BYTES (64 MiB) · SANDBOX_{CPU_MILLIS,MEMORY_BYTES,DISK_BYTES}_{MIN,MAX} · WORKSPACE_DIR · TMP_DIR
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| `/tmp` full | A tenant writes until the disk is gone | `ENOSPC` to the writer; the sandbox lives (Dimension 1.1) |
| `/dev/shm` full | A tenant writes shared memory, whose pages outlive their writer | `ENOSPC` at a quarter of memory; later commands keep the rest (Dimension 1.3) |
| Forked child killed for memory | A compound command's child is killed; the shell exits 137 | Read as `out_of_memory` when the leaf counted it (Dimension 2.6) |
| Lane host short of disk | Earlier runs' state or a full host disk | The kernel lane refuses to run and names the free space and the leftover state |
| Memory exhausted | A tenant allocates past the limit | The killer takes a tenant process; its call reads `out_of_memory` (Dimensions 2.1, 2.2) |
| Tenant move refused | A kernel without open-time migration checks, or a lost descriptor | Spawn refused with a code; nothing runs unshielded (Dimension 2.4) |
| Workspace writes outrun reclaim | Dirty pages under the memory limit | Direct I/O removes the second cache; the writer ends in `ENOSPC` (Dimension 3.2) |
| Tenant writes past its disk faster than write-back drains | Pages under write-back fill the tenant leaf before the disk refuses the write | `memory.high`, an eighth below the leaf's `memory.max` and never more than 128 MiB below it, slows the writer while write-back drains; it reads `ENOSPC` and is not killed (Dimension 5.1, `test_the_tenant_leaf_throttles_an_eighth_below_its_limit`) |
| Disk build fails after the mount | The mount helper mounts and then fails, or the layout is refused | Undone as a release is: unmounted, then removed; the lease sees the step's own failure, and no loop device stays on a deleted image (Dimension 3.1) |
| Size past a bound | A daemon sends a size outside the declared bounds | The lease ends at startup, `startup_posture`, before any sandbox is prepared (Dimension 4.4) |
| Crash mid-split | Runner dies between creating leaves and entering them | The boot sweep removes both leaves (Dimension 2.5) |

## Invariants

1. No tenant process runs in the `sandbox` leaf — the move happens before `exec`, and a failed move fails the spawn; `test_failed_tenant_move_refuses_the_spawn`.
2. The tenant leaf's memory limit is below the lease's — the engine writes both at create; `test_oom_kills_only_the_tenant`.
3. No tenant process holds the cgroup descriptor — it is close-on-exec; `test_tenant_process_holds_no_cgroup_descriptor`.
4. No sandbox is built at a size outside the declared bounds — `sized` proves it before `prepare`; `a_size_past_its_bounds_refuses_the_lease_before_any_sandbox`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `error.type = out_of_memory` on the call's `execute_tool` span (`afd_observability::semconv::ATTR_ERROR_TYPE`) | ops | A tenant process is killed for memory; any other closed code a call ends with rides it the same way | the span's existing ids | No command text | `a_call_killed_for_memory_is_counted_and_typed_on_its_span` |
| `sandbox_out_of_memory` (runner log, warn) | ops | Same | lease id, `error_code`; the call's ids ride its span | No command text | `test_oom_ending_reads_out_of_memory` |
| `sandbox_size_refused` (runner log, warn) | ops | A lease names a size past a bound | lease id, `error_code` | No paths | `a_size_past_its_bounds_refuses_the_lease_before_any_sandbox` (asserts the event, its code and lease) |
| `sandbox_workspace_buffered` (runner log, debug) | ops | A lease's workspace disk runs without direct I/O | lease id | No paths | `test_workspace_disk_uses_direct_io` (the direct case) |
| `executor_memory_events_unread` (executor log, warn) | ops | The leaf's `memory.events` cannot be read when judging a kill | `error_code` | No paths | `a_kill_whose_count_cannot_be_read_stays_a_sigkill_and_is_logged` |
| `agentsfleet_runner_tool_out_of_memory_total` (runner census) | ops | A tenant process is killed for memory | none — closed label set | — | `a_call_killed_for_memory_is_counted_and_typed_on_its_span`, `test_every_runner_census_family_has_a_producer` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | kernel | `test_full_tmp_answers_enospc` | `dd` into `/tmp` → `No space left on device`; next `echo ok` → `ok` |
| 1.2 | kernel | `test_workspace_and_tmp_share_the_disk` | 64 MiB disk: 40 MiB into `/workspace` fits, and `ls -a /workspace` → no `lost+found`; 40 MiB more into `/tmp` → `No space left on device` |
| 2.1 | kernel | `test_oom_kills_only_the_tenant` | 256 MiB memory: a 2 GiB allocation → that process ends `OutOfMemory`; `echo ok` → `ok`; the `sandbox` leaf's `memory.events` reads `oom_kill 0` |
| 2.2 | unit | `test_oom_ending_reads_out_of_memory` | an `OutOfMemory` ending through `shell` → code `out_of_memory`, exit 137, one `sandbox_out_of_memory` warn with no command text; the executor's own `OutOfMemory` on a real kernel is 2.1's |
| 2.3 | kernel | `test_tenant_process_holds_no_cgroup_descriptor` | `ls /proc/$$/fd` from the tenant's shell → `0 1 2`; `/proc/$$/cgroup` → `0::/../tenant`, the leaf beside the namespace's `sandbox` root |
| 1.3 | kernel | `test_full_shared_memory_spares_the_tenant` | 256 MiB memory; `dd` 128 MiB into `/dev/shm` → `No space left on device`; then a 64 MiB allocation → `ok` |
| 1.3 | unit | `test_shared_memory_is_a_quarter_of_the_sandboxs_memory` | 2 GiB memory → 512 MiB; bubblewrap gets `--perms 1777 --size <bytes> --tmpfs /dev/shm` |
| 2.6 | unit | `a_shell_reporting_its_childs_kill_reads_as_out_of_memory_when_the_leaf_counted_it` | `Exited(137)`, leaf count risen → `OutOfMemory`; unchanged → `Exited(137)` (`a_shell_reporting_a_kill_the_leaf_never_counted_keeps_its_exit`); `Exited(1)` → unjudged |
| 2.4 | unit | `test_failed_tenant_move_refuses_the_spawn` | descriptor closed before spawn → refused with its cause; 0 processes started |
| 2.5 | kernel | `test_sweep_removes_both_leaves` | runner killed after the split → restart sweeps `sandbox`, `tenant`, then the lease cgroup |
| 3.1 | kernel | `test_workspace_disk_uses_direct_io` | `/sys/dev/block/<major>:<minor>/loop/dio` of the mounted disk, read before destroy → `1` |
| 3.1 | kernel | `test_a_disk_whose_mount_helper_failed_is_unmounted` | a real mount helper that mounts, then exits 1 → the build fails, the lease's directory is empty, nothing stays mounted |
| 3.1 | unit | `test_a_mount_that_fails_after_mounting_leaves_nothing_behind` | a fake mount that writes into the mount point, then fails → the build fails; image and mount point removed |
| 3.1 | unit | `test_a_disk_that_cannot_be_laid_out_leaves_nothing_behind` | a fake mount that leaves `workspace/` in the mount point → the layout's `AlreadyExists` reaches the caller; image and mount point removed, whichever user runs the test |
| 3.2 | kernel | `test_disk_fill_under_memory_limit_ends_in_enospc` | 4 GiB `dd` under 2 GiB memory → `ENOSPC`; `tenant` `oom_kill` 0 |
| 4.1 | unit | `a_sized_lease_builds_the_size_it_asked_for` | 4 000 / 8 GiB / 20 GiB → the engine is asked for exactly that, with the default process cap |
| 4.2 | unit | `a_lease_without_a_size_builds_the_hosts_defaults` | no `limits` → the engine is asked for `Limits::default()` |
| 4.3 | unit | `a_lease_without_a_size_decodes_as_none` | no `limits` key, and `null` → `None`; re-encodes `null` |
| 4.4 | unit | `a_size_past_its_bounds_refuses_the_lease_before_any_sandbox` | 256 MiB − 1 of memory → `startup_posture`, 0 prepared, 0 turns |
| 4.5 | unit | `a_size_one_past_any_bound_is_refused` | each bound's limit admitted, one past it refused, six rows |
| 4.6 | unit | `the_published_bounds_are_the_enforced_ones` | `SandboxLimits`'s schema minimum and maximum per field → the `SANDBOX_*` constants |
| 5.1 | kernel | `test_writable_state_exhaustion_spares_the_sandbox` | four leases at 1 GiB disk and 512 MiB memory, `/tmp` then `/workspace` filled with 2 GiB each → eight `ENOSPC`, `oom_kill 0` in every `sandbox` leaf, four `ok` |
| 5.1 | unit | `test_the_tenant_leaf_throttles_an_eighth_below_its_limit` | a new lease cgroup → the tenant leaf's `memory.high` reads its `memory.max` less an eighth |
| 5.1 | unit | `test_a_large_tenant_leaf_throttles_a_capped_headroom_below_its_limit` | a 4 GiB lease cgroup → the tenant leaf's `memory.high` reads its `memory.max` less 128 MiB, not an eighth |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Exhaustion ends in errors the tenant reads, never in a dead sandbox (§1–§3, §5) | `make test-runner-kernel 2>&1 \| grep -c "test_writable_state_exhaustion_spares_the_sandbox ... ok"` | 1 | P0 | ✅ 1 — `test_writable_state_exhaustion_spares_the_sandbox ... ok`; lane `34 passed; 0 failed` in 142.79s, `/dev/shm` trial included (Oct 07, 2026) |
| R2 | A lease's size is built or refused as declared (§4) | `cd rustd && cargo test --all-features -p afr_supervisor -p afd_wire size` | exit 0 | P0 | ✅ 10 passed, 0 failed (§4) |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed tables | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears above verbatim. **Grading protocol (VERIFY):** Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results in Session Notes.

## Dead Code Sweep

N/A — no files deleted. The sandbox's `/tmp` tmpfs flag goes in place.

## Out of Scope

- A host memory reserve: the sum of leases' memory limits against the host's memory, which S6 did not test.
- Per-tenant quotas across leases.
- A host disk reserve: a worker waiting until the state disk can hold every live sandbox at its full limit. Deferred by Indy (see Discovery); a host's disk may fill meanwhile.
- Where a lease's size comes from: the fleet API, its storage, filling it into the lease, placement by a host's ceiling, billing by size, and warm slots per size.
- The Firecracker engine's own exhaustion behaviour.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet's build fills `/tmp`; the thread shows the command failed with `No space left on device`, and the fleet's next command cleans up and carries on in the same sandbox.
2. **Preserved user behaviour** — Every tool and path is where it was; `/tmp` is still `/tmp`.
3. **Optimal-way check** — A microVM per lease isolates memory and disk entirely; until the Firecracker engine, cgroups and one disk give the same answer a tenant can act on.
4. **Rebuild-vs-iterate** — Iterate: four changes to the engine M210_001 built.
5. **What we build** — The disk layout, the two leaves and the tenant move, direct I/O on the disk, the lease's size, the S6 trial.
6. **What we do NOT build** — A host memory or disk reserve, cross-lease quotas, the fleet API for sizes (Out of Scope).
7. **Fit with existing features** — The hold (M211_004) freezes the whole lease cgroup, both leaves, at whatever size the lease named.
8. **Surface order** — N/A — no user surface; the thread shows the codes.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; `out_of_memory` and `No space left on device` name the limit that was hit.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section per S6 failure, then S6 as a permanent trial, because each failure has its own mechanism and its own proof.
- **Alternatives considered:** a sized tmpfs for `/tmp` (rejected: its pages stay charged to memory until the sandbox dies); `oom_score_adj` on bubblewrap and the executor (rejected: children inherit it, and Landlock forbids a tenant process from raising its own through `/proc`); preallocating every workspace image (rejected: costs the full limit up front, for leases that mostly write little).
- **Patch-vs-refactor verdict:** this is a **patch** because the engine, the disk and the cgroup exist; each change is local to them.

## Discovery (consult log)

- **Consults** — §3 on the kernel lane (Oct 07, 2026): the lane made its state under `/tmp`, a tmpfs on `afr-kernel` (`findmnt`), so every workspace image was memory and the 4 GiB fill was killed whatever the loop device cached; on a disk (`/var/tmp`, btrfs) `losetup --direct-io=on` reads back `dio` 1 and 3.2 ends in `ENOSPC`. The lane's state now lives under `/var/tmp`, as a host's does on its disk. Indy (in-session, Oct 05, 2026): "Fix all fixes in this PR", approving D3's four fixes in a new workstream of this Pull Request. Source and evidence: spike S6 (`docs/v2/reviews/m211-toolbox-spikes.md`); Landlock grants writes only beneath `WRITABLE` (`rustd/crates/afr_sandbox/src/harden/linux.rs:50-57`); bubblewrap enters its cgroup through an engine-opened descriptor (`bubblewrap_engine/parts.rs:89,282`).
- **Metrics review** — Two runner log events; no analytics or funnel playbook change.
- **Production run parked on M213_001** — > Indy (2026-10-07 ~16:08, AskUserQuestion): "Move them the dimenstions as parked and the spec as DONE. Mention in a prompt to me so i can ask the other agent in M213 to deploy and test this." — context: `agentsfleet_runner/src/main.rs:184-185` refuses every lease, and the production probe passes no `state_dir`, so the engine runs only in the kernel lane and tests until M213_001 Dimension 1.4. 4.3 and 4.6 are the wire the daemon already encodes.
- **Gaps closed at REVIEW (Oct 07, 2026)** — an unsized, writable `/dev/shm` was S6's failure in another directory: now a quarter of memory (1.3). A forked child killed for memory read as a plain exit 137: now judged (2.6). The lane failed on a full host disk without saying so: it now refuses to start. Dimension 4.4's test now asserts the `sandbox_size_refused` warn it is cited for.
- **Published docs** — sandbox defaults, bounds and what happens at each limit are on `runners.mdx` in the docs repo, branch `chore/m211-sandbox-tools-changelog`, commit `07a4cb1`.
- **Unit-test ledger at the boundary (Oct 07, 2026)** — three findings. The Linux-only engine test `test_a_lease_runs_through_the_engine_and_keeps_a_disk_it_cannot_unmount` failed from `60b37efd9` on: it read the lease cgroup's own `cgroup.procs` after processes moved to the sandbox leaf. macOS lanes never compile `bubblewrap_engine`, so no local lane caught it. It now reads `sandbox/cgroup.procs`, and the Linux suites ran green on `afr-kernel` at the boundary: `afr_sandbox` 140, `afr_executor` 96 and 55. A workspace disk whose build failed after the mount (direct I/O or the layout) removed its files and unlinked its image while still mounted; it now unmounts first, as a release does (`test_a_disk_that_cannot_be_laid_out_leaves_nothing_behind`). The leaf's `oom_kill` count is shared by every tenant process: with two running at once, a shell that exits 137 for its own reason after the other's kill can claim that kill, and the other's ending reads as plain. Known and left as built: commands in one lease run one at a time unless the model opens concurrent `exec_command` sessions.
- **Kernel trial S6 root cause (Oct 08, 2026)** — The S6 trial `test_writable_state_exhaustion_spares_the_sandbox` failed intermittently on base `886733be6` and head alike: S6 alone passed 0 of 3 runs on base and 2 of 3 on head. The kernel's out-of-memory report showed the tenant leaf full of pages under write-back, `file_writeback 468451328` against a 448 MiB limit, so `dd` was killed before it read `ENOSPC`. The tenant leaf now carries `memory.high` an eighth below its `memory.max` (`afr_sandbox/src/cgroup.rs`, unit test `test_the_tenant_leaf_throttles_an_eighth_below_its_limit`). S6 alone then passed 5 of 5 runs, and the full lane 40 of 40, on a scratch tree (`585f8c29f`).
- **Review fixes (Oct 08, 2026)** — Every failed workspace disk build, before or after the mount, now goes through one undo that unmounts whatever is mounted before it removes files (`585f8c29f`). The kernel trial `test_a_disk_whose_mount_helper_failed_is_unmounted` fails a real mount helper after it mounts. The unit tests `test_a_mount_that_fails_after_mounting_leaves_nothing_behind` and `test_a_disk_that_cannot_be_laid_out_leaves_nothing_behind` prove the undo without root, and the second no longer depends on the user that runs it. A cgroup file that cannot be read is now named as unread, not as a refused write.
- **Skill-chain outcomes** — `/orly-write-unit-test` at the boundary (Oct 08, 2026): the review round's ledger over `657ae5807..df3b1f52d` (44 production files) found 8 gaps, closed in `190012f01`, with two won't-test rows (a refused `memory.high` write needs a cgroup file system; an unencodable report cannot occur); a coverage pass closed the remaining testable arms in `b9e282233`. `make test-coverage-rustd` at `b9e282233`: patch coverage 99.3078% (2726 of 2745 changed lines) against the 99 floor, line coverage 98.8582%; the 19 unhit lines are pre-exec hooks and the sandbox entry, which run in a forked child or inside bubblewrap where no profile is written, plus arms no input reaches. gstack `/review`: ten readers over `e2242b5b4..657ae5807`, fixes in `585f8c29f`, `6905beb74` and `d0afd22a0`; a second pass over the fixes found four more, fixed in `e96879f57` and `a414fbc13`; the findings left open are listed for Indy in the Pull Request. `make test-runner-kernel` on afr-kernel at `a414fbc13`: 41 passed, 0 failed. Mutation testing over the review round runs after the Pull Request opens and its result goes in the Pull Request's Session notes; `orly-babysit-prs` follows the push.
- **§5 sizes** (Oct 07, 2026: 1:10 PM) — the trial runs at 1 GiB of disk and 512 MiB of memory per lease, a quarter of S6's, because four default leases need 16 GiB the lane host does not have. Indy (in-session, Oct 07, 2026) approved the scaled sizes: "yes go ahead", answering the recommendation of 1 GiB / 512 MiB over S6's 4 GiB / 2 GiB.
- **Deferrals** — §4's host disk reserve, as written at PLAN (`capacity.rs`, the worker's wait, `sandbox_capacity_short`, `agentsfleet_runner_capacity_short_total`), is not built. Indy (in-session, Oct 07, 2026): "for now the host disk size can get maxed, that is fine, its a separate think to solve disk pressure." §4 became the lease's size in its place, Indy choosing "Lease field": the size rides `LeasePayload`, the daemon sends null for now, and tests inject it.
