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

# M210_001: The Rust runner's supervisor leases, renews and reports a fleet's work against the real daemon, and executes every tool call in a hardened per-lease bubblewrap sandbox built from a host toolbox

**Prototype:** v2.0.0
**Milestone:** M210
**Workstream:** 001
**Date:** Oct 02, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the foundation every later runner capability stands on: outage repair, workspaces carried between leases, Codex as an engine
**Categories:** API, INFRA
**Batch:** B1 — the first Rust runner workstream; the agent loop, providers and cutover follow in their own spec
**Branch:** feat/m210-rust-runner-foundation
**Baseline revision:** 0d79b0318e687b8862ee8dbd4f622069bc6ba1a4
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from Indy's in-session decisions and a source trace on `main`; Codex at `~/Projects/oss/rs/codex` `2e5fea64e`, IronClaw at `~/Projects/oss/rs/ironclaw` `b0b999d96`, ZeroClaw at `~/Projects/oss/zeroclaw` `74362c2d6`
**Canonical architecture:** `docs/architecture/runner_execution.md` §Process model, §Sandbox engines, §Toolbox; `docs/architecture/runner_fleet.md` §The control protocol

---

## Overview

**Goal (testable):** Against a real `agentsfleetd`, the Rust runner leases an event, starts a hardened bubblewrap sandbox from the toolbox, executes a scripted turn's tool calls through its in-sandbox executor, streams their frames, renews, pushes memory, spools and posts the report, and destroys the sandbox. A process inside cannot reach the network, gain a capability, write outside its workspace or exceed its memory, process or disk limits.
**Problem:** Outage repair needs a fleet to run real programs — git, builds, tests, Python — inside a boundary that holds against tenant code, with the model key kept out of that boundary, a disk quota that is enforced, and a test lane that proves a lease end to end against the daemon (none exists, `docs/architecture/testing.md:197`). Indy chose a fresh Rust runner, independent of the Zig one, to provide it.
**Solution summary:** New `afr_*` crates and one binary in `rustd`, following its principles (`docs/architecture/runner_execution.md` §Crates). A supervisor speaks the ten runner verbs through `afd_wire` and keeps every duty the daemon relies on: worker pool, renewal, a report spooled before posting, a bounded activity sender, credential minting, memory hydrate and push, bundle fetch, the startup sweep, the capability report. A bubblewrap engine builds each lease's sandbox from a read-only toolbox image, a per-lease workspace disk and a hardened profile, and its `sandbox` sub-mode inside serves process and file calls over a Unix socket. The agent loop is a trait whose only implementation here is a scripted test engine; providers, tools and the cutover come next.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): Rust supervisor, hardened bubblewrap engine and in-sandbox executor
- **Intent (one sentence):** The new runner can run a lease end to end against the real daemon, with every tool call inside a sandbox that stays shut to tenant code, ready for an agent loop to drive.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` — the design this implements; its Decisions table is binding.
2. `docs/architecture/runner_fleet.md` — §The control protocol, §Per-lease renewal, §Failure recovery model: the duties the daemon relies on.
3. `docs/RUST_ERROR_STANDARD.md` and `rustd/crates/afd_runner/src/lib.rs` — the error shape every crate uses, and a `rustd` crate split by concern with pure logic apart from I/O.
4. `rustd/crates/afd_api_runner/src/handler/runner/report.rs` and `rustd/crates/afd_fleet/src/lease/finalize.rs` — the daemon side of the report: what a duplicate, a stale fence and a late post do.
5. https://github.com/openai/codex/tree/2e5fea64eefcaa19f48458b2386011b619f69c70/codex-rs — `linux-sandbox/src/bwrap.rs` (bubblewrap, then re-exec into seccomp) and `exec-server/README.md` (the executor method shapes to mirror).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/Cargo.toml`, `rustd/crates/agentsfleet_runner/` | EDIT / CREATE | Workspace member; one binary whose `run`, `probe` and `sandbox` entries only compose library crates |
| `rustd/crates/afr_supervisor/` (`client.rs`, `config.rs`, `lease_loop.rs`, `turns.rs`, `worker_pool.rs`, `heartbeat.rs`, `renew.rs`, `report.rs`, `report_spool.rs`, `activity.rs`, `credentials.rs`, `memory.rs`, `bundles.rs`, `storage_home.rs`, `capability.rs`, `error.rs`) | CREATE | The daemon-facing duties, one concern per file |
| `rustd/crates/afd_core/src/bundle.rs`, `rustd/crates/afd_library/src/prepare.rs` | CREATE / EDIT | One bundle digest the importer names a bundle by and the runner verifies against |
| `rustd/crates/afr_sandbox/` (`engine.rs`, `probe.rs`, `host.rs`, `bubblewrap.rs`, `bubblewrap_engine.rs`, `harden.rs` (Landlock and seccomp), `cgroup.rs`, `workspace_disk.rs`, `toolbox.rs`, `warm_slots.rs`, `unsandboxed.rs`, `error.rs`) | CREATE | Engine interface, the hardened bubblewrap engine, and a test-only unsandboxed engine release builds refuse |
| `rustd/crates/afr_executor/` (`protocol.rs`, `server.rs`, `client.rs`, `process.rs`, `fs.rs`, `error.rs`) | CREATE | Executor protocol, the in-sandbox server and the supervisor's client |
| `rustd/crates/afr_agent/` (`engine.rs`, `error.rs`, `scripted.rs` behind `test-util`) | CREATE | The agent-engine trait; a scripted test engine as its only implementation here, behind `test-util` so the daemon's integration lane can drive it |
| `rustd/crates/afr_sandbox/examples/kernel_lane/` | CREATE | Real-sandbox proofs on Linux, refusing to skip silently; an example with a `libtest-mimic` harness because `cargo test --test '*'`, the integration lane's selection, runs even a `test = false` target |
| `rustd/crates/agentsfleetd/Cargo.toml`, `rustd/crates/agentsfleetd/tests/daemon_suite.rs`, `rustd/crates/agentsfleetd/tests/integration_rust_runner.rs` | EDIT / CREATE | The runner against the real daemon in the integration lane |
| `rustd/crates/afd_wire/src/{lease,event,policy,runner,memory,credentials,activity,report}.rs`, `rustd/crates/afd_wire/tests/{strictness,memory_shapes}.rs`, `public/openapi.json` | EDIT | Drop `deny_unknown_fields` from the 23 types the runner reads; their published schemas lose `additionalProperties: false` |
| `rustd/crates/afd_core/src/json.rs`, `rustd/crates/afd_http/src/handler/mod.rs`, `rustd/crates/afd_api_operator/src/handler/operator/runner_patch.rs`, `rustd/crates/afd_api_runner/src/handler/runner/{enrolment,memory}.rs` | EDIT | A strict reader for the three daemon requests that embed one of those types |
| `scripts/toolbox/build.sh`, `scripts/toolbox/manifest.txt`, `make/build.mk`, `rustd/Cargo.toml` | CREATE / EDIT | `make toolbox-image`: a pinned, content-addressed, read-only toolbox image |
| `make/test-unit.mk`, `.github/workflows/lint.yml` | EDIT | `make test-runner-kernel` and its Linux CI job. The workflow edit needs Indy's explicit approval (§Hard Safety) |
| `docs/architecture/runner_execution.md`, `docs/RUST_ERROR_STANDARD.md` | EDIT | The supervisor's bounded capability set (landed at authoring) and `afd_observability` as a runner dependency; the runner crates' row in the error conformance table |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (limits, paths, syscall lists and method names are constants), OWN (each sandbox, cgroup, workspace disk and child has one owner and one cleanup), FLS (drain executor output and child pipes on every exit path), TIM (renew window, grace periods and timeouts are explicit), ECL (5xx renew keeps the lease, 4xx ends it), STR (the executor is proven over its real socket), FXS, TCF, TST-NAM, OBS, ERR-RS, NDC.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — one `ErrorKind` per crate through `afd_core::error_shell!`; every refusal keeps its cause.
- `dispatch/write_shell.md` — the toolbox build script: quoted expansions, temp-file cleanup.
- `docs/LOGGING_STANDARD.md` — scoped events with `error_code`; never log a secret, a token or tool output.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | `error_shell!` and `error_lifts!` only; the `Result` alias is the one hand-written line |
| UFS / LOGGING / MILESTONE-ID | yes | Constants per concern; scoped events; no milestone identifiers in source |
| File & Function Length (≤350/≤50/≤70) | yes | One concern per file across the three module directories |
| CI/CD edit guard | yes — `.github/workflows/lint.yml` | Stop at EXECUTE for Indy's explicit approval; the lane cannot ship without it |
| Architecture consult | yes | `runner_execution.md` is the design; drift is fixed in the same commit |

## Prior-Art / Reference Implementations

- **Reference:** `rustd` itself — `afd_runner` (split by concern, pure verdicts apart from the store) and `docs/RUST_ERROR_STANDARD.md`; every new crate is shaped the same way.
- **Reference:** Codex `linux-sandbox` (`~/Projects/oss/rs/codex/codex-rs/linux-sandbox/src/bwrap.rs`) — bubblewrap, then re-exec into seccomp, with a zero-capability check; extended with `--disable-userns`, `--clearenv` and the extra seccomp refusals, because Codex's filter allows by default (`linux-sandbox/src/landlock.rs:284-285`).
- **Reference:** Codex `exec-server` — the executor's method names and process lifecycle; IronClaw's `ironclaw-exec` reaper (`~/Projects/oss/rs/ironclaw/docker/sandbox/ironclaw-exec`) — new session, TERM, grace, KILL of every descendant.
- **Reference:** ZeroClaw `zeroclaw-runtime/src/security/` — one sandbox interface over several backends.

## Sections (implementation slices)

### §1 — Crates, one binary with a sandbox sub-mode, the one wire

The `afr_*` crates and the one `agentsfleet-runner` binary, whose `sandbox` sub-mode runs inside each sandbox, follow `docs/architecture/runner_execution.md` §Crates; each crate declares one error type through `afd_core::error_shell!`, and nothing in them refers to the Zig runner. The runner decodes every daemon→runner type without refusing unknown fields, and runner→daemon types stay strict. Three daemon requests embed a runner-bound type — an enrolment's and an operator's assigned policy, a memory push's deltas — and their handlers read through `afd_core::json::strict_object_from_slice`, which refuses an ignored key at any depth, so nothing the daemon checks gets looser. No runner crate depends on anything from `agentsfleetd` beyond `afd_wire`, `afd_core` and `afd_observability`, and none links a datastore crate.

- **Dimension 1.1** — A lease payload with an extra field decodes in the runner → Test `test_daemon_payload_with_unknown_field_decodes`
- **Dimension 1.2** — A report body with an extra field is still refused by the daemon → Test `test_runner_body_with_unknown_field_refused`
- **Dimension 1.3** — The binary's normal dependency graph names `sqlx`, `redis` or any `afd_*` crate other than `afd_wire`, `afd_core` and `afd_observability` → Test `test_runner_links_no_datastore_crate`

### §2 — The supervisor keeps every duty the daemon relies on

A worker pool runs N leases, one fleet each. Renewal keeps the lease on a 5xx and ends it on a 4xx. The report is spooled to disk before its first POST and replayed at boot. The activity sender holds at most four 64 KiB batches and drops, counts and logs past that. Credential minting, memory hydrate and fenced push, bundle fetch with a hash-verified cache, the boot sweep of orphaned workspaces, and the capability report all keep today's verbs.

- **Dimension 2.1** — Two workers never hold the same fleet → Test `test_worker_pool_runs_distinct_fleets`
- **Dimension 2.2** — A 5xx renewal keeps the lease and a 4xx ends it → Test `test_renew_keeps_on_5xx_ends_on_4xx`
- **Dimension 2.3** — A report spooled before a kill is posted after restart, once → Test `test_spooled_report_replays_once`
- **Dimension 2.4** — A fifth queued batch is dropped, counted and logged → Test `test_activity_sender_drops_past_four_batches`
- **Dimension 2.5** — Memory hydrates at start and pushes with the fencing token before the report → Test `test_memory_push_fenced_before_report`
- **Dimension 2.6** — A bundle whose bytes miss their hash is refused → Test `test_bundle_hash_mismatch_refused`
- **Dimension 2.7** — Boot removes an orphaned lease workspace and keeps foreign directories → Test `test_storage_home_sweeps_orphans_only`
- **Dimension 2.8** — The capability probe states whether `/dev/kvm` is present and usable, so the daemon knows which hosts can take a Firecracker engine, and whether the kernel can mount the toolbox's filesystem (EROFS), without which no sandbox can be built; both reach the daemon as named self-test checks → Test `test_capability_report_states_kvm_and_toolbox_fs`

### §3 — A hardened bubblewrap engine

Each lease gets fresh user, PID, IPC, UTS, mount and network namespaces; `--cap-drop ALL`, `--disable-userns`, `--clearenv`, `--die-with-parent`, `--new-session`. Inside, before the executor reads anything: `no_new_privs`, Landlock, then a seccomp filter refusing `io_uring_*`, `ptrace`, `process_vm_readv`, `process_vm_writev`, `unshare`, `bpf`, `keyctl` and `perf_event_open`. The executor refuses to start if any capability remains. Cgroup v2 limits memory, processor, process count and I/O, and ends the whole tree. **Implementation default:** the workspace disk is a per-lease ext4 image sized to the lease's disk limit, loop-mounted at `/workspace` and deleted at lease end, because it works on any host filesystem and costs no memory, unlike XFS project quotas or tmpfs. The network namespace has loopback only; the allowlist arrives with workspaces. The supervisor runs with a bounded capability set (mounts, cgroups, namespaces); tenant code never holds one. **Firecracker-ready:** the engine interface assumes no filesystem shared with the host. The workspace disk is a block image, the toolbox an image file, and the executor a Unix socket on the host side, which is how a microVM's vsock surfaces. A Firecracker engine then attaches the same artifacts unchanged.

- **Dimension 3.1** — A process inside reports zero effective and permitted capabilities → Test `test_sandbox_process_has_no_capabilities`
- **Dimension 3.2** — `unshare`, `bpf`, `keyctl`, `perf_event_open` and `io_uring_setup` fail with `EPERM` → Test `test_seccomp_refuses_listed_syscalls`
- **Dimension 3.3** — A write outside the workspace and `/tmp` is denied → Test `test_landlock_denies_write_outside_workspace`
- **Dimension 3.4** — Writing past the disk limit fails with `ENOSPC`, and the workspace disk is gone after the lease → Test `test_workspace_disk_enforces_limit_and_is_removed`
- **Dimension 3.5** — A fork bomb hits `pids.max` and a memory hog is killed, without touching the supervisor → Test `test_cgroup_limits_contain_runaway`
- **Dimension 3.6** — A TCP connect to any address outside loopback fails → Test `test_sandbox_has_no_network`
- **Dimension 3.7** — A sandbox that cannot be established refuses the lease instead of running it unsandboxed → Test `test_unbuildable_sandbox_refuses_lease`
- **Dimension 3.8** — A release build refuses the unsandboxed engine → Test `test_release_build_refuses_unsandboxed_engine`

### §4 — The executor runs processes and files inside the sandbox

JSON-RPC over a Unix socket bound into the sandbox: spawn (pipes or a pseudo-terminal), write, kill, and a pushed output and exit stream; file read, write and list under `/workspace`. Output keeps the first and last 512 KiB of each process, cut on UTF-8 boundaries. Kill sends TERM to the process group, waits a 2-second grace, then KILLs every descendant. When the executor or sandbox dies, the supervisor ends each open process as `interrupted`.

- **Dimension 4.1** — A spawned `echo` streams its output and exits 0 over the real socket → Test `test_executor_spawn_streams_output`
- **Dimension 4.2** — A pseudo-terminal session accepts input and echoes it → Test `test_executor_pty_accepts_input`
- **Dimension 4.3** — 3 MiB of output keeps its first and last 512 KiB with an omitted count, on character boundaries → Test `test_executor_output_keeps_head_and_tail`
- **Dimension 4.4** — Kill reaps a child that ignores TERM and its grandchildren → Test `test_executor_kill_reaps_process_group`
- **Dimension 4.5** — A path escaping `/workspace` through `..` or a symlink is refused → Test `test_executor_refuses_path_escape`
- **Dimension 4.6** — Killing the sandbox mid-call ends that call `interrupted` exactly once → Test `test_sandbox_death_interrupts_open_calls`

### §5 — The toolbox is an image already on the host

`make toolbox-image` builds a minimal Debian root with git, Python 3, CA certificates and core utilities from a pinned manifest, into a compressed read-only EROFS image named by its SHA-256. The runner verifies the hash before mounting it once per host and binds it read-only into every lease. Later specs extend the manifest.

- **Dimension 5.1** — The same manifest builds a byte-identical image → Test `test_toolbox_build_is_reproducible`
- **Dimension 5.2** — An image whose hash does not match is never mounted → Test `test_toolbox_hash_mismatch_refused`
- **Dimension 5.3** — A lease sees the toolbox read-only: `git --version` runs and writing `/usr` fails → Test `test_lease_sees_toolbox_read_only`

### §6 — Warm slots and a measured start budget

Each host keeps a configured number of warm slots: sandboxes already started, each with its cgroup made, an empty workspace disk mounted and its executor idle, so a lease start fills the workspace and hands the slot its lease. A slot serves one lease and is destroyed with it; the host starts a new one in its place. The kernel lane measures lease-accept to executor-ready, cold and warm, and VERIFY records both figures in Discovery as the budget later specs guard.

- **Dimension 6.1** — A lease taken from a warm slot reaches executor-ready faster than a cold one, and both figures are reported → Test `test_warm_start_beats_cold_start`
- **Dimension 6.2** — A warm slot is never reused after a lease → Test `test_warm_slot_single_use`

### §7 — Two lanes prove it

The integration lane runs the runner against the real daemon with compose Postgres and Dragonfly, the scripted engine and the unsandboxed engine (so it runs on any developer machine). The kernel lane runs §3–§6 on Linux with the real engine and fails, never skips, when bubblewrap, Landlock, user namespaces or cgroup delegation are missing.

- **Dimension 7.1** — A scripted lease runs end to end: frames on the channel, memory pushed, report settled, workspace removed → Test `test_rust_runner_lease_roundtrip`
- **Dimension 7.2** — The kernel lane fails when a required kernel feature is absent → Test `test_kernel_lane_refuses_to_skip`

## Interfaces

```
agentsfleet-runner run           supervisor (systemd unit)
agentsfleet-runner probe         capability report: /dev/kvm, toolbox filesystem mountable
agentsfleet-runner sandbox       the in-sandbox entry; the binary is bound read-only into each sandbox

Executor (JSON-RPC 2.0, Unix socket bound at /run/agentsfleet/executor.sock inside the sandbox)
  process/spawn  { argv, cwd, env, pty: bool, timeout_ms }   → { process_id }
  process/write  { process_id, data }
  process/kill   { process_id }                              TERM, 2 s grace, KILL the group
  notifications  process/output { process_id, stream, data } · process/exited { process_id,
                 exit_code, signal, timed_out, omitted_bytes }
  fs/read { path, max_bytes } · fs/write { path, content } · fs/list { path }

Engine (Rust trait): prepare(lease) → Sandbox; Sandbox: executor(), destroy()
AgentEngine (Rust trait): run(lease, executor, events) → Outcome   (scripted only, here)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Sandbox cannot be built | Missing kernel feature, failed mount | Lease refused before any tool runs; capability report says why (Dimension 3.7) |
| Supervisor killed after the run | `SIGKILL`, OOM, reboot | Spooled report posted once at boot (Dimension 2.3) |
| Renewal answered 5xx / 4xx | Daemon fault / revoked lease | Keep / kill the sandbox and end the lease (Dimension 2.2) |
| Executor or sandbox dies | Crash, cgroup kill | Open calls end `interrupted`; the run reports its outcome (Dimension 4.6) |
| Runaway process | Fork bomb, memory hog, disk fill | Cgroup and disk limits contain it; the supervisor is untouched (Dimensions 3.4, 3.5) |
| Tampered toolbox | Wrong image on disk | Not mounted; leases refused until a verified image exists (Dimension 5.2) |
| Host cannot mount the toolbox | Kernel lacks the toolbox's filesystem | Capability report says so; no sandbox can be built, so every lease is refused (Dimensions 2.8, 3.7) |
| Live-tail backlog | Slow daemon | Batches past four dropped and counted; the report is unaffected (Dimension 2.4) |
| Kernel lane host lacks a feature | CI image drift | Lane fails loudly (Dimension 7.2) |

## Invariants

1. No tool call runs outside a sandbox in a release build — the unsandboxed engine is refused at startup (Dimension 3.8).
2. Tenant code never holds a capability — the executor checks before serving (Dimension 3.1).
3. Every spawned process ends exactly once, as exited, killed or `interrupted` (Dimension 4.6).
4. A finished run's report is never lost to a crash — it is on disk before its first POST (Dimension 2.3).
5. The runner links no datastore crate (Dimension 1.3).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `sandbox_refused` (runner log, error) | ops | A sandbox cannot be established | lease id, missing feature, `error_code` | No workspace content | `test_unbuildable_sandbox_refuses_lease` |
| `sandbox_start_ms` (runner log, info) | ops | A lease reaches executor-ready | lease id, warm or cold, milliseconds | None needed | `test_warm_start_beats_cold_start` |
| `activity_batch_dropped` (runner log, warn) | ops | A batch exceeds the queue | lease id, dropped count | No frame content | `test_activity_sender_drops_past_four_batches` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_daemon_payload_with_unknown_field_decodes` | lease JSON + `"future": 1` → decodes |
| 1.2 | integration | `test_runner_body_with_unknown_field_refused` | report + `"future": 1` → daemon refuses |
| 1.3 | unit | `test_runner_links_no_datastore_crate` | normal graph: no `sqlx`, `redis`, or `afd_*` beyond `afd_wire`, `afd_core`, `afd_observability` |
| 2.1 | unit | `test_worker_pool_runs_distinct_fleets` | 2 workers, 1 fleet with 2 events → never concurrent |
| 2.2 | unit | `test_renew_keeps_on_5xx_ends_on_4xx` | fake daemon 503 → keep; 409 → kill and end |
| 2.3 | unit | `test_spooled_report_replays_once` | spool, kill before POST, restart → one POST |
| 2.4 | unit | `test_activity_sender_drops_past_four_batches` | stalled daemon, 5 full batches → 1 dropped, counted |
| 2.5 | unit | `test_memory_push_fenced_before_report` | run end → push with token, then report |
| 2.6 | unit | `test_bundle_hash_mismatch_refused` | tampered bytes → refused, not cached |
| 2.7 | unit | `test_storage_home_sweeps_orphans_only` | orphan lease dir + foreign dir → only orphan removed |
| 2.8 | unit | `test_capability_report_states_kvm_and_toolbox_fs` | fake `/dev/kvm` present and absent, `erofs` listed and missing in a fake `/proc/filesystems` → probe and self-test checks say each |
| 3.1 | kernel | `test_sandbox_process_has_no_capabilities` | `/proc/self/status` CapEff and CapPrm all zero |
| 3.2 | kernel | `test_seccomp_refuses_listed_syscalls` | each listed call → `EPERM` |
| 3.3 | kernel | `test_landlock_denies_write_outside_workspace` | write `/opt/x` → denied; `/workspace/x` → ok |
| 3.4 | kernel | `test_workspace_disk_enforces_limit_and_is_removed` | 64 MiB limit, write 80 MiB → `ENOSPC`; workspace disk gone after |
| 3.5 | kernel | `test_cgroup_limits_contain_runaway` | fork bomb → `pids.max`; 2 GiB hog → killed; supervisor alive |
| 3.6 | kernel | `test_sandbox_has_no_network` | connect `1.1.1.1:443` → fails |
| 3.7 | kernel | `test_unbuildable_sandbox_refuses_lease` | Landlock unavailable → lease refused, nothing executed |
| 3.8 | unit | `test_release_build_refuses_unsandboxed_engine` | release profile + unsandboxed engine → startup refusal |
| 4.1 | integration | `test_executor_spawn_streams_output` | `echo hi` over the socket → `hi`, exit 0 |
| 4.2 | integration | `test_executor_pty_accepts_input` | `cat` on a pseudo-terminal, write `x` → `x` echoed |
| 4.3 | unit | `test_executor_output_keeps_head_and_tail` | 3 MiB with multi-byte characters → 512 KiB + 512 KiB, valid UTF-8, omitted count |
| 4.4 | integration | `test_executor_kill_reaps_process_group` | TERM-ignoring parent + grandchild → both gone after grace |
| 4.5 | unit | `test_executor_refuses_path_escape` | `../etc/passwd`, symlink to `/etc` → refused |
| 4.6 | integration | `test_sandbox_death_interrupts_open_calls` | kill sandbox mid-call → one `interrupted` frame |
| 5.1 | kernel | `test_toolbox_build_is_reproducible` | two builds → same SHA-256 |
| 5.2 | unit | `test_toolbox_hash_mismatch_refused` | flipped byte → not mounted, leases refused |
| 5.3 | kernel | `test_lease_sees_toolbox_read_only` | `git --version` ok; `touch /usr/x` fails |
| 6.1 | kernel | `test_warm_start_beats_cold_start` | warm p50 < cold p50; both logged |
| 6.2 | unit | `test_warm_slot_single_use` | lease ends → its sandbox destroyed, a new slot started |
| 7.1 | integration | `test_rust_runner_lease_roundtrip` | scripted lease → frames, memory push, settled report, workspace removed |
| 7.2 | kernel | `test_kernel_lane_refuses_to_skip` | feature probe fails → lane exits non-zero |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A scripted lease runs end to end against the real daemon (§2, §7) | `make test-integration-rustd && grep -c "fn test_rust_runner_lease_roundtrip(" rustd/crates/agentsfleetd/tests/integration_rust_runner.rs` | 1 | P0 | |
| R2 | The sandbox holds against tenant code on Linux (§3–§6) | `make test-runner-kernel` | exit 0 | P0 | |
| R3 | The runner links no datastore or daemon-plane crate (§1) | `cargo tree --manifest-path rustd/Cargo.toml -p agentsfleet_runner -e normal \| grep -cE "sqlx\|redis\|afd_(db\|dragonfly\|fleet\|events\|api)"` | 0 | P0 | |
| R4 | Kernel lane runs in CI after Indy's approval of the workflow edit | manual — Indy approves the `.github/workflows/lint.yml` change; evidence: the run URL in Session Notes | approval quote and a green run URL | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted. The Zig runner stays until the cutover spec deletes it with its lanes.

## Out of Scope

- The agent loop, model providers, the hosted tools and the cutover that deletes the Zig runner and NullClaw — the next spec in M210.
- The network allowlist, workspaces carried in R2, artifacts, leak scanning — a later spec; this sandbox has no network.
- Coding engines (Codex, Claude Code), `apply_patch`, `propose_change` and the supervisor's push — later specs.
- Firecracker microVMs — an additional engine behind the same interface, later.
- Deploying the Rust runner to hosts — the cutover spec.

---

## Product Clarity (authoring record)

1. **Successful user moment** — No user sees this yet; the moment is an engineer watching a scripted lease run through the real daemon in a sandbox that refuses a fork bomb, a network connect and a capability.
2. **Preserved user behaviour** — Every fleet keeps running on the Zig runner until the cutover; the daemon's verbs are unchanged.
3. **Optimal-way check** — The supervisor/executor split is Codex's shape; the sandbox hardens itself before the executor reads anything, and closes what Codex's filter leaves open.
4. **Rebuild-vs-iterate** — Rebuild, by Indy's decision: "The port is a fresh port".
5. **What we build** — One crate: supervisor, bubblewrap engine, executor, toolbox image, two test lanes.
6. **What we do NOT build** — Agent loop, providers, network, workspaces, coding engines, microVMs (see Out of Scope).
7. **Fit with existing features** — Speaks the existing runner verbs through `afd_wire`; must not change daemon behaviour beyond loosening decode on runner-bound types.
8. **Surface order** — N/A — no user surface; internal runner substrate.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; an operator reads `sandbox_refused` and the capability report.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** foundation first, as M175 did for the daemon: the supervisor, the sandbox and the executor are proven before an agent loop depends on them, and a scripted engine drives the integration lane so the provider work cannot hide sandbox faults.
- **Alternatives considered:** porting the Zig runner's structure (rejected by Indy); a strangler with a Rust supervisor launching the Zig child (rejected: pre-production, the last Zig binary is the rollback); Docker per lease (rejected: image start latency, and Indy asked to avoid it); embedding `codex-core` (rejected: Responses API only, weekly churn).
- **Patch-vs-refactor verdict:** this is a **refactor** because it replaces the runner wholesale; the daemon side changes only by loosening decode on runner-bound types.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "The port is a fresh port, since we always have the last binary with us and running." and "ensure we dont hoadwink and follow the runner zig as opposed to a fresh plate"; "preferrably avoiding Docker since its layer takes a while to load... I will need faster results as well"; multi-tenant on bare metal or VMs from the first release, Firecracker additional; "No, share freely" for code-running leases from different tenants. These supersede "the src/runner will be on zig no action needed there" (Sep 02, `M187_001`). Agent defaults: the per-lease ext4 workspace disk, 512 KiB output edges, a 2-second kill grace.
- **Naming** — Indy (Oct 02, 2026): "i think keep toolbox that is fine", reversing an earlier "base" pick. The read-only image is the toolbox, the per-lease writable image is the workspace disk, and a pre-started sandbox is a warm slot; `docs/architecture/runner_execution.md` §Facts carries all three.
- **Engine sequencing** — Indy (Oct 02, 2026): "i think we must shoot for firecracker then", conditioned on "if its throwawy work", and asked for the quickest end-to-end path. With both engines behind one interface only Dimensions 3.1–3.3 are bubblewrap-specific, and bubblewrap stays as the development, CI and no-`/dev/kvm` engine. Indy confirmed: "Yes bubblewrap first, and firecracker next." A Firecracker spec follows this one; Dimension 2.8 makes every host report whether it can run it.
- **Required human decision** — Indy's explicit approval of the `.github/workflows/lint.yml` edit that runs the kernel lane (`AGENTS.orly.md` §Hard Safety); R4 records it.
- **Independence** — Indy (Oct 02, 2026): "A second copy of the wire will not be existing, none of the rust code will point to the zig." and "the rust code is independent and follows our current rustd/ principles". Daemon→runner types decode leniently so a runner never refuses a field a newer daemon adds; real-sandbox proofs once skipped silently everywhere (`M170_001`), hence Dimension 7.2.
- **One binary** — Indy (Oct 02, 2026): "why do we need two ? agentsfleet-runner, agentsfleet-executor … i thought its just one binary?" then "agentsfleet-runner". The in-sandbox entry is the `sandbox` sub-mode of the one binary (Indy chose the name), as Codex re-executes itself as `codex-linux-sandbox`; the sub-mode constructs only `afr_executor`'s server.
- **Metrics review** — No analytics or funnel playbook update required: no user surface; three operator log events added.
- **PLAN decisions** — Indy (Oct 02, 2026) chose "Lenient + guarded PATCH" for runner-bound decoding; on pushing `main`: "No fast forward in your worktree and keep moving , let it go in the PR"; and set patch coverage at 99% for Rust and 100% for TypeScript, which `codecov.yml` already enforces. Agent defaults, flagged for Indy in the Pull Request: runner errors reuse registry codes (the `afd_bench` precedent in `docs/RUST_ERROR_STANDARD.md`, since no client reads them); `/dev/kvm` and EROFS reach the daemon as self-test checks until the Firecracker spec adds the wire field it reads; Linux-only crates sit under `cfg(target_os = "linux")` dependencies, as Codex's `linux-sandbox` does.
- **Sandbox crate choices (agent)** — loop mounts go through the host's `mount -o loop`, because the maintained loop-device crates (`loopdev-3`, `sys-mount`) build with `bindgen` and would put libclang on every Linux workspace build; cgroups are typed writes of named files, because `cgroups-rs` supports v2 and `cgroup.kill` but pulls in `zbus`. The toolbox build is reproducible: two builds hashed `c4e5f5bb23d5683d1c30e8656675a1734f1ea2f0ae586515812220c4477a0ad6`.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
