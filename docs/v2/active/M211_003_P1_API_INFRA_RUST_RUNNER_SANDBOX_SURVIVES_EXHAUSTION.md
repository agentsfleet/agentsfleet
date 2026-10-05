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

# M211_003: A lease's sandbox survives its own exhaustion — `/tmp` fills to `ENOSPC` on the workspace disk, memory pressure kills only the tenant's process, the workspace disk writes without a second page cache, and a host keeps a disk reserve

**Prototype:** v2.0.0
**Milestone:** M211
**Workstream:** 003
**Date:** Oct 05, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — spike S6: one command that fills `/tmp` or memory takes `bwrap` and the whole sandbox down, and nothing stops leases from filling the host's disk
**Categories:** API, INFRA
**Batch:** B1 — folded into M211_001 and shipped in its Pull Request; its Sections run after M211_001's §8
**Branch:** `feat/m211-sandbox-tools-and-nested-loops`
**Folded-into:** `M211_001`
**Baseline revision:** `b0138d7b3124b871668f07e2361dba923bc774d2`
**Test Baseline:** pending — measured before the Pull Request; shared with M211_001
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M211_001 (§1's processes and §8's `LOOP_CONFIGURE` attach, which the workspace disk reuses) · spike S6's evidence in `docs/v2/reviews/m211-toolbox-spikes.md`
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 05, 2026) from spike S6's kernel log and runs
**Canonical architecture:** `docs/architecture/runner_execution.md` §"Sandbox engines", §"Workspace between leases"

---

## Overview

**Goal (testable):** `test_writable_state_exhaustion_spares_the_sandbox` — four leases fill `/workspace` and `/tmp` at once under the default limits; each `dd` ends in `No space left on device`, no process of the sandbox's own leaf is killed, and each lease's executor runs a new command afterwards.
**Problem:** Spike S6 ran four leases that filled their disks and `/tmp`. Filling `/tmp`, an in-memory tmpfs larger than the lease's 2 GiB memory limit, ended every time in the kernel's out-of-memory killer taking `bwrap`, the sandbox's first process, so the whole sandbox died. One lease was killed while writing to its workspace, before the disk filled. Nothing refused leases when four 4 GiB workspaces took the host's disk from 20 GB free to 5.9 GB.
**Solution summary:** `/tmp` moves onto the workspace disk, so it fills to `ENOSPC` and shares the lease's disk limit. The lease cgroup splits into a leaf for `bwrap` and the executor and a leaf for every tenant process, whose memory limit sits just below the lease's, so the killer can only pick a tenant process; the call it killed says `out_of_memory`. The workspace disk attaches with direct I/O, so its writes are not cached twice. A worker takes a lease only while the runner's state disk can hold every live sandbox at its full disk limit plus a reserve.

## PR Intent & comprehension handshake

- **PR title (eventual):** folded into M211_001's Pull Request
- **Intent (one sentence):** A tenant command that exhausts its lease's disk or memory gets an error it can read, and the sandbox, its other processes and the host all carry on.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/reviews/m211-toolbox-spikes.md` §S6 — what failed, the kernel log's victims, and the two harness traps.
2. `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` — how `bwrap` enters its cgroup through a `cgroup.procs` descriptor the engine opened; the tenant move mirrors it.
3. `rustd/crates/afr_executor/src/server/launch.rs` — the two launchers (pipes, terminal) every tenant process starts from.
4. `rustd/crates/afr_sandbox/src/cgroup.rs`, `rustd/crates/afr_sandbox/src/workspace_disk.rs` — the lease cgroup and the workspace disk this spec reshapes.
5. https://docs.kernel.org/admin-guide/cgroup-v2.html — "no internal process" rule, `memory.max`, `memory.events`, delegation and migration permission.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_sandbox/src/workspace_disk.rs` | EDIT | The disk holds `workspace/` and `tmp/`; attached with `LOOP_CONFIGURE` and direct I/O |
| `rustd/crates/afr_sandbox/src/bubblewrap.rs` | EDIT | `/tmp` binds the disk's `tmp/` instead of a tmpfs |
| `rustd/crates/afr_sandbox/src/cgroup.rs`, `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` | EDIT | The `sandbox` and `tenant` leaves; `bwrap` enters `sandbox`; the tenant `cgroup.procs` descriptor is handed in, close-on-exec |
| `rustd/crates/afr_executor/src/server/launch.rs`, `rustd/crates/afr_executor/src/server/launch/terminal.rs` | EDIT | Each tenant process moves itself into `tenant` before `exec`; a failed move refuses the spawn |
| `rustd/crates/afr_executor/src/api.rs`, `rustd/crates/afr_tools/src/runtime.rs` | EDIT | An ending killed for memory is `out_of_memory`, a `ToolErrorCode` the model reads |
| `rustd/crates/afr_sandbox/src/capacity.rs` | CREATE | The state-disk rule: live sandboxes' remaining limits plus one more plus the reserve |
| `rustd/crates/afr_supervisor/src/worker_pool.rs`, `rustd/crates/afr_sandbox/src/warm_slots.rs` | EDIT | Poll and refill only with room |
| `rustd/crates/afr_sandbox/examples/kernel_lane/trials.rs` | EDIT | The kernel proofs, S6 among them |
| `docs/architecture/runner_execution.md` | EDIT | §"Sandbox engines": the two leaves, `/tmp` on the disk, the reserve |
| `docs/v2/pending/M214_001_P1_DOCS_OBS_RUST_RUNNER_EXPORTS_ITS_TELEMETRY.md` | EDIT | The runner census gains this workstream's two counters |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (reserves and directory names are constants), OWN (one owner per descriptor; the tenant descriptor is close-on-exec), LOG, ERR-RS, TGU (an ending is one enum), FLL, TST-NAM, NDC.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — a refused move keeps its `io::Error` cause; `pre_exec` stays async-signal-safe (one `write`, no allocation).
- `docs/LOGGING_STANDARD.md` §8A — `sandbox_capacity_short` logs bytes, never paths a tenant chose; `error_code` on every warning.
- Indy's Rust bar (Oct 04 and Oct 05, 2026): traits and trait objects where behaviour varies (the capacity rule is a trait the worker pool holds as `&dyn`, so tests swap the disk reading), structs that own their behaviour, borrows over clones, ownership over `Mutex`, `Fn`/`FnMut`/`FnOnce` where behaviour is passed, `afd_core::error_shell!`, `afd_observability` for spans and metrics, established crates over hand-rolled code (`rustix` for `statvfs`, the `write` in `pre_exec` and the loop ioctls; `nix` nowhere new), no duplicated logic (the workspace attach reuses M211_001 §8's `LOOP_CONFIGURE` helper), `afd_core` reused for shared constants.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | `error_shell!` per crate, causes preserved |
| UFS / LOGGING / MILESTONE-ID | yes | Named reserves and leaf names; scoped events; no milestone identifiers in source |
| File & Function Length (≤350/≤50/≤70) | yes | `capacity.rs` alone owns the rule; `cgroup.rs` splits leaf handling out if it nears the cap |
| Architecture consult | yes | `runner_execution.md` §"Sandbox engines" edited in the docs commit that lands this spec |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afr_sandbox/src/bubblewrap_engine/parts.rs` — a process entering its cgroup through a descriptor the engine opened; the tenant move is the same write, made by each tenant process before it execs.
- **Reference:** systemd's delegation model (`Delegate=` units keep the manager's own processes in a leaf apart from the delegated subtree) — the reason the sandbox's own processes and the tenant's never share a leaf.

## Sections (implementation slices)

### §1 — `/tmp` lives on the workspace disk

A fresh workspace disk gets two directories, `workspace/` (mode 0755) and `tmp/` (mode 1777), owned by the sandbox user; bubblewrap binds them at `/workspace` and `/tmp`, so `mke2fs`'s `lost+found` leaves `/workspace` too. Both share the lease's disk limit, so a full `/tmp` answers `ENOSPC` like a full workspace. **Implementation default:** the disk, not a sized tmpfs, because tmpfs pages stay charged to the lease's memory until every process in its mount namespace is gone, which is how S6's sandboxes died.

- **Dimension 1.1** — Filling `/tmp` ends in `ENOSPC`, and the executor runs a new command afterwards → Test `test_full_tmp_answers_enospc`
- **Dimension 1.2** — `/workspace` and `/tmp` draw on one disk limit, and `/workspace` holds no `lost+found` → Test `test_workspace_and_tmp_share_the_disk`

### §2 — Memory pressure kills the tenant's process, never the sandbox

The lease cgroup becomes an inner node with two leaves: `sandbox` (bubblewrap and the executor, entered as today) and `tenant`, whose `memory.max` is the lease's limit minus `SANDBOX_MEMORY_RESERVE_BYTES`, so the tenant leaf runs out first and the killer chooses only inside it. The engine opens `tenant/cgroup.procs` write-only and passes the descriptor into the sandbox close-on-exec; each tenant process writes `0` to it in `pre_exec`, before `exec`, through the descriptor's open-time credentials, so no cgroup file system is mounted inside. A move that fails refuses the spawn: no tenant process ever runs in `sandbox`. An ending whose process the killer took, read from `tenant/memory.events`, completes the call `failed` with `out_of_memory`. **Implementation default:** a 64 MiB reserve, measured against the executor's resident memory on the kernel lane at PLAN.

- **Dimension 2.1** — A process allocating past the limit is killed, and the executor runs a new command afterwards → Test `test_oom_kills_only_the_tenant`
- **Dimension 2.2** — The killed call completes `failed` with `out_of_memory` → Test `test_oom_ending_reads_out_of_memory`
- **Dimension 2.3** — A tenant process holds descriptors 0–2 and sits in `tenant` → Test `test_tenant_process_holds_no_cgroup_descriptor`
- **Dimension 2.4** — A move that fails refuses the spawn, and nothing runs in `sandbox` but bubblewrap and the executor → Test `test_failed_tenant_move_refuses_the_spawn`
- **Dimension 2.5** — Destroy and the boot sweep remove both leaves before the lease cgroup → Test `test_sweep_removes_both_leaves`

### §3 — The workspace disk writes past one page cache

The workspace image attaches through §8's `LOOP_CONFIGURE` with `LO_FLAGS_DIRECT_IO`, read-write, then mounts as today; the loop device reads and writes the image without caching it a second time on the host. A backing file system without direct I/O falls back to buffered, which the capability report states.

- **Dimension 3.1** — The workspace loop device reports direct I/O on → Test `test_workspace_disk_uses_direct_io`
- **Dimension 3.2** — Writing 4 GiB to `/workspace` under the 2 GiB memory limit ends in `ENOSPC` with zero out-of-memory kills → Test `test_disk_fill_under_memory_limit_ends_in_enospc`

### §4 — The host keeps a disk reserve

A worker polls for a lease only while the state file system's available bytes cover every live sandbox's remaining disk limit (limit minus the blocks its image holds), one more sandbox at its full limit, and `STATE_DISK_RESERVE_BYTES`. Live means leases, warm slots and held sandboxes alike. Short of room, the worker waits, and `sandbox_capacity_short` logs once when the host goes short and once when it recovers; warm slots refill under the same rule. **Implementation default:** a 2 GiB reserve, room for the next toolbox release and the runner's logs.

- **Dimension 4.1** — The rule admits at exactly the boundary and refuses one byte short → Test `test_capacity_rule_counts_remaining_limits`
- **Dimension 4.2** — A short host takes no lease and polls again once space returns, logging each transition once → Test `test_short_host_waits_and_resumes`
- **Dimension 4.3** — On a state disk sized for three filled sandboxes, a fourth is never prepared and the reserve stays free → Test `test_reserve_survives_concurrent_fills`

### §5 — S6, kept

Spike S6's scenario becomes a kernel trial on one shared engine, with lease state on disk: four leases fill `/workspace` and `/tmp` at once.

- **Dimension 5.1** — Each lease gets `ENOSPC` on both, no `sandbox` leaf process is killed, and each executor runs a command afterwards → Test `test_writable_state_exhaustion_spares_the_sandbox`

## Interfaces

```
workspace disk        <lease>/disk/{workspace (0755), tmp (1777)}   → /workspace, /tmp
lease cgroup          <lease>/{sandbox, tenant}   tenant/memory.max = limit − SANDBOX_MEMORY_RESERVE_BYTES
tenant move           pre_exec: write(fd, "0")   fd = tenant/cgroup.procs, opened by the engine, close-on-exec
ending                Exited | Signaled | OutOfMemory (tenant memory.events oom_kill rose)
ToolErrorCode         + OutOfMemory   wire "out_of_memory"
capacity              available ≥ Σ live (limit − used) + DEFAULT_DISK_BYTES + STATE_DISK_RESERVE_BYTES
constants             SANDBOX_MEMORY_RESERVE_BYTES (64 MiB) · STATE_DISK_RESERVE_BYTES (2 GiB) · WORKSPACE_DIR · TMP_DIR
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| `/tmp` full | A tenant writes until the disk is gone | `ENOSPC` to the writer; the sandbox lives (Dimension 1.1) |
| Memory exhausted | A tenant allocates past the limit | The killer takes a tenant process; its call reads `out_of_memory` (Dimensions 2.1, 2.2) |
| Tenant move refused | A kernel without open-time migration checks, or a lost descriptor | Spawn refused with a code; nothing runs unshielded (Dimension 2.4) |
| Workspace writes outrun reclaim | Dirty pages under the memory limit | Direct I/O removes the second cache; the writer ends in `ENOSPC` (Dimension 3.2) |
| Host disk short | Live sandboxes could grow past the free space | No lease taken; one log line; resumes on its own (Dimension 4.2) |
| Crash mid-split | Runner dies between creating leaves and entering them | The boot sweep removes both leaves (Dimension 2.5) |

## Invariants

1. No tenant process runs in the `sandbox` leaf — the move happens before `exec`, and a failed move fails the spawn; `test_failed_tenant_move_refuses_the_spawn`.
2. The tenant leaf's memory limit is below the lease's — the engine writes both at create; `test_oom_kills_only_the_tenant`.
3. No tenant process holds the cgroup descriptor — it is close-on-exec; `test_tenant_process_holds_no_cgroup_descriptor`.
4. A worker never takes a lease the state disk cannot hold at its full limit with the reserve intact — `capacity.rs` is the one gate before a poll; `test_capacity_rule_counts_remaining_limits`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `error.type = out_of_memory` on the call's `execute_tool` span (`afd_observability::semconv::ATTR_ERROR_TYPE`) | ops | A tenant process is killed for memory | the span's existing ids | No command text | `test_oom_ending_reads_out_of_memory` |
| `sandbox_out_of_memory` (runner log, warn) | ops | Same | lease id, call id, memory limit | No command text | `test_oom_ending_reads_out_of_memory` |
| `sandbox_capacity_short` (runner log, warn / info on recovery) | ops | The state disk goes short, and when it recovers | available, required, reserve bytes | No paths | `test_short_host_waits_and_resumes` |
| `agentsfleet_runner_tool_out_of_memory_total`, `agentsfleet_runner_capacity_short_total` (runner census, M214_001) | ops | The two events above | none — closed label sets | — | M214_001's producer-coverage test |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | kernel | `test_full_tmp_answers_enospc` | `dd` into `/tmp` → `No space left on device`; next `echo ok` → `ok` |
| 1.2 | kernel | `test_workspace_and_tmp_share_the_disk` | 3 GiB in `/workspace` → `/tmp` takes under 1 GiB; `ls -a /workspace` → no `lost+found` |
| 2.1 | kernel | `test_oom_kills_only_the_tenant` | a 3 GiB allocation → that process killed; `echo ok` → `ok`; `sandbox` leaf processes unchanged |
| 2.2 | kernel | `test_oom_ending_reads_out_of_memory` | same → call `failed`, code `out_of_memory` |
| 2.3 | kernel | `test_tenant_process_holds_no_cgroup_descriptor` | `ls /proc/self/fd` → `0 1 2`; `/proc/self/cgroup` ends in `tenant` |
| 2.4 | unit | `test_failed_tenant_move_refuses_the_spawn` | descriptor closed before spawn → refused with its cause; 0 processes started |
| 2.5 | kernel | `test_sweep_removes_both_leaves` | runner killed after the split → restart sweeps `sandbox`, `tenant`, then the lease cgroup |
| 3.1 | kernel | `test_workspace_disk_uses_direct_io` | `/sys/block/loopN/loop/dio` → `1` |
| 3.2 | kernel | `test_disk_fill_under_memory_limit_ends_in_enospc` | 4 GiB `dd` under 2 GiB memory → `ENOSPC`; `tenant` `oom_kill` 0 |
| 4.1 | unit | `test_capacity_rule_counts_remaining_limits` | exact boundary → admit; one byte short → refuse; a held sandbox counts |
| 4.2 | unit | `test_short_host_waits_and_resumes` | fake disk short then roomy → no poll, then a poll; two log lines |
| 4.3 | kernel | `test_reserve_survives_concurrent_fills` | 14 GiB state disk, four fills → three prepared; reserve free throughout |
| 5.1 | kernel | `test_writable_state_exhaustion_spares_the_sandbox` | four leases, `/workspace` and `/tmp` filled → eight `ENOSPC`, 0 `sandbox` kills, four `ok` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Exhaustion ends in errors the tenant reads, never in a dead sandbox (§1–§3, §5) | `make test-runner-kernel 2>&1 \| grep -c "test_writable_state_exhaustion_spares_the_sandbox ... ok"` | 1 | P0 | |
| R2 | The capacity rule holds (§4) | `cargo test --manifest-path rustd/Cargo.toml -p afr_sandbox capacity` | exit 0 | P0 | |
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
- Per-tenant quotas across leases, and disk limits other than the defaults.
- The Firecracker engine's own exhaustion behaviour.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet's build fills `/tmp`; the thread shows the command failed with `No space left on device`, and the fleet's next command cleans up and carries on in the same sandbox.
2. **Preserved user behaviour** — Every tool and path is where it was; `/tmp` is still `/tmp`.
3. **Optimal-way check** — A microVM per lease isolates memory and disk entirely; until the Firecracker engine, cgroups and one disk give the same answer a tenant can act on.
4. **Rebuild-vs-iterate** — Iterate: four changes to the engine M210_001 built.
5. **What we build** — The disk layout, the two leaves and the tenant move, direct I/O on the disk, the capacity rule, the S6 trial.
6. **What we do NOT build** — A host memory reserve, cross-lease quotas (Out of Scope).
7. **Fit with existing features** — The hold (M211_004) counts as a live sandbox under §4 and freezes the whole lease cgroup, both leaves.
8. **Surface order** — N/A — no user surface; the thread shows the codes.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; `out_of_memory` and `No space left on device` name the limit that was hit.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one Section per S6 failure, then S6 as a permanent trial, because each failure has its own mechanism and its own proof.
- **Alternatives considered:** a sized tmpfs for `/tmp` (rejected: its pages stay charged to memory until the sandbox dies); `oom_score_adj` on bubblewrap and the executor (rejected: children inherit it, and Landlock forbids a tenant process from raising its own through `/proc`); preallocating every workspace image (rejected: costs the full limit up front, for leases that mostly write little).
- **Patch-vs-refactor verdict:** this is a **patch** because the engine, the disk and the cgroup exist; each change is local to them.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 05, 2026): "Fix all fixes in this PR", approving D3's four fixes in a new workstream of this Pull Request. Source and evidence: spike S6 (`docs/v2/reviews/m211-toolbox-spikes.md`); Landlock grants writes only beneath `WRITABLE` (`rustd/crates/afr_sandbox/src/harden/linux.rs:50-57`); bubblewrap enters its cgroup through an engine-opened descriptor (`bubblewrap_engine/parts.rs:89,282`).
- **Metrics review** — Two runner log events; no analytics or funnel playbook change.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
