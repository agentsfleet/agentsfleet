<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M258_001: Fleets run in our own Firecracker microVMs on bare metal, and one conformance suite proves every engine

**Prototype:** v2.0.0

**Milestone:** M258

**Workstream:** 001

**Date:** Nov 26, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every run's tool calls share the runner host's kernel, and a VM engine cannot plug in while the seam leaks host paths

**Categories:** API, CLI, DOCS, INFRA, OBS

**Batch:** B1. One Pull Request carries M258_001, M258_002 and M258_003, built in that order (O1)

**Depends on:** none. The web and harness milestone, M259, follows this one (assumption O5)

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 26, 2026) from Indy's rulings of Nov 26, 2026 and the research in `~/.local/state/agentsfleet-m254/` (`vm-engines/`, `space/`, `harness/engine-policy-placement.md`, `harness/skeptic-review.md`). Each repository citation was re-read at `679002666`.

**Canonical architecture:** `docs/architecture/runner_execution.md` §Sandbox engines and §Limits; this spec rewrites both for two engines

---

## Overview

**Goal (testable):** On the dev runner host every sandbox is a Firecracker microVM, and the kernel lane passes one conformance suite on bubblewrap and Firecracker.

**The compute layer.** M258 is the compute layer, and Indy ruled it the first milestone, so bubblewrap is sunset before anything else. Its three Workstreams ship in one Pull Request (O1).

- **M258_001, this spec:** the engine seam in its own crates, the conformance suite, our Firecracker engine with its inside-out guest, and the dev host switched.
- **M258_002:** checkpoint and snapshot rebuilt on a live chunk map, written separately once the storage research lands.
- **M258_003:** production switched to Firecracker, tiers named for engines, and bubblewrap deleted.
- **M260_001:** the Sprites engine, the engine policy, burst and a Sprites rate, moved out of M258 by Indy (O2).

**Terms.**

- A virtual machine (VM) runs its own kernel. Firecracker is the virtual machine monitor (VMM) that runs a small one, a microVM.
- Firecracker needs the Kernel-based Virtual Machine (KVM) device, `/dev/kvm`. The toolbox is our signed Enhanced Read-Only File System (EROFS) image of Debian 13.
- Rust paths that start with a crate name are under `rustd/crates/<layer>/`. Line references are at `679002666`; "working tree" marks a file another branch is editing.

**Problem.** Read from source.

- **One engine, fused into one crate.** `EngineChoice` has one variant (`main/src/engine.rs:9-14`), and `--engine firecracker` is refused (`main/src/main_tests.rs:120-124`). `afr_sandbox` holds the traits, the bubblewrap engine and every shared piece in 17,579 lines.
- **The seam leaks the host.** Checkout, capture and both restores need a host path (`afr_supervisor/src/run_loop/checkout.rs:94-96`, `recovery_capture.rs:121-123`, `turn_recovery.rs:16-18`, `recovery.rs:46-51`). `freeze` promises the cgroup freezer's semantics (`afr_sandbox/src/engine/mod.rs:229-236`). `reallow` swaps an IPv4 set in one transaction (`:257-268`).
- **Defaults answer nothing.** `toolbox_digest` and `workspace` default to `None`, `is_running` to `true`, and `recovery_quiescent` to an error (`engine/mod.rs:207-247`). An engine that forgets one still compiles.
- **The daemon is not told the engine.** The capability report is five Linux booleans (`af_wire/src/runner/capability.rs:25-52`, working tree). A host without usable KVM fails its self-test, though bubblewrap needs none (`afr_supervisor/src/capability/tests.rs:39-65`).
- **Nothing proves two engines alike.** The kernel lane builds a concrete `BubblewrapEngine` (`main/examples/kernel_lane/lane.rs:96-99`).

**Solution summary.** The seam is split by the concerns Indy named: lifecycle and identity, resources, storage, the executor's transport, networking and credentials. Files cross through the executor, the egress plan is in force before the first command, and every engine states its facts. The seam moves to its own crate, and each engine to a crate of its own.

One conformance suite runs over `&dyn Engine`. Bubblewrap passes it, then a Firecracker engine passes it, and the dev host switches.

**Design flow:**

```text
agentsfleet-runner run --engine firecracker          (main: the one crate naming an engine)
  boot: afr_engine_firecracker admits the host        Kvm::Usable, host kernel, pinned binaries
  heartbeat: engine=firecracker, kvm=usable, sandboxes_max=13
supervisor (names afr_engine only)
  prepare(SandboxSpec{id: Uuid7, granted, network gen N})
     jailer ─► firecracker --config-file, own uid, chroot, cgroup leaf, netns
        drive vda  toolbox EROFS, read-only, from the verified descriptor
        drive vdb  workspace ext4 image, read-write, never host-mounted
        tap  ─► host table afegress<slot>: listed IPv4 set, reserved ranges dropped
        vsock ─► CONNECT <port> ─► executor (the runner's own binary as guest entry)
     returns once egress generation N is in force and the executor answers
  checkout: fetch in the host mirror with the token ─► tar stream ─► executor import
  capture:  executor export ─► today's encrypted manifest ─► R2
  hold:     quiesce, guest sync, VMM stopped, disk kept; resume boots a fresh VM on it
  restore:  the executor (the VM's root service) tears down the inner environment,
            swaps the workspace, builds it again; the VM never reboots
```

**What is unverified.**

- No VM has booted on the dev host. §1's spike settles boot, the guest entry, the drives and a stop without loss.
- The host kernel, 6.12.111+deb13-amd64, is outside Firecracker's validated 5.10, 6.1 and 6.18 (D6). Firecracker's boot times and overhead are vendor-stated.
- Whether the acceptance machine exposes `/dev/kvm` is unchecked (A1).

**What changes for a fleet.** Each line is a behaviour Indy accepts by approving this spec.

1. On a Firecracker runner, `allow_all` means the public internet with reserved ranges refused. Today the sandbox shares the host's network (`runner_execution.md:171`).
2. A held sandbox keeps its disk, not its memory. Resume boots a fresh VM on the kept disk (O3).
3. A Firecracker host overcommits memory 4 to 1, so the dev host runs about 55 runs of 2 GiB at once, not 64 (O3). If guests touch more memory than the host has, the host ends one VM, and its run resumes from its last checkpoint.
4. A sandbox starts slower than bubblewrap's "tens of milliseconds" (`afr_sandbox/src/engine/mod.rs:30-32`). §1 records by how much.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): an engine seam split by concern, and our own Firecracker engine on the dev host
- **Intent (one sentence):** A fleet's tool calls run behind a hardware VM wall on our hosts, and a suite decides when an engine is good enough.
- **Authoring handshake:** Indy wants the engine replaced first and bubblewrap retired once a VM engine runs fleets. He wants SPACE's traits studied, storage and networking abstracted, better crates, and Fly's Sprites design adopted where it fits. ASSUMPTIONS I'M MAKING:
  1. O1, ruled: one Pull Request for all of M258. M258_002 designs the chunk map; this spec only carries its snapshot interface.
  2. O2, ruled: Sprites, the engine policy and burst are M260_001. Nothing here touches Sprites.
  3. O3, ruled: admission overcommits VM memory 4 to 1, and holds keep the disk only.
  4. O4, ruled: SPACE's `fc_api.rs` is copied under Apache-2.0 only when a pause, snapshot or balloon first needs the API (§7).
  5. O5, ruled: the compute layer is M258, and web plus harness is M259.
  6. Nothing is backward compatible. The wire, the self-test and the crates change in place, with one runner protocol `VERSION` move, in §2.
  7. One commit per Section after §1, in the one M258 Pull Request.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/runner/afr_sandbox/src/engine/mod.rs` — the seam §2 to §4 reshape, with its four defaults that answer nothing.
2. `rustd/crates/runner/main/examples/kernel_lane/lane.rs` — the lane whose trials become the conformance suite in §6.
3. `rustd/crates/runner/afr_supervisor/src/run_loop/checkout.rs` — the host-path checkout §3 moves onto the executor.
4. `docs/reviews/space-sandbox/sandbox_evaluation.md` — the Oct 25, 2026 review: the first wrapper, its build order, and what a copied file must name.
5. https://github.com/firecracker-microvm/firecracker/blob/v1.18.0/docs/prod-host-setup.md — the jailer, host settings and the egress filtering Firecracker leaves to us.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M258_001_P1_API_CLI_DOCS_INFRA_OBS_FLEETS_RUN_IN_OUR_FIRECRACKER_MICROVMS.md` | CREATE | this spec; moves to `active/`, then `done/` |
| `docs/v2/reviews/m258-firecracker-spike.md` | CREATE | §1's record of commands, outputs and measurements; spike helpers stay outside the repository |
| `rustd/crates/runner/afr_sandbox/**` | DELETE | §5 splits it; `git grep -w afr_sandbox` counts 404 hits in 141 files outside the crate, each renamed in §5 |
| `rustd/crates/runner/afr_engine/**` | CREATE | §2 to §6: the seam traits and types, the error, boot admission, warm slots, and the suite behind `test-util` |
| `rustd/crates/runner/{afr_image,afr_firewall,afr_cgroup,afr_guest}/**` | CREATE | §5: images; the host firewall table; cgroups and tenant attribution; the sandbox-side entry |
| `rustd/crates/runner/{afr_engine_bubblewrap,afr_engine_unsandboxed}/**` | CREATE | §5: today's engine as one crate; the debug engine and the recovery-drill engine |
| `rustd/crates/runner/afr_engine_firecracker/**` | CREATE | §7 and §8 |
| `rustd/crates/runner/afr_executor/**` | EDIT | §3: a transport over any byte stream, a redialling client, `import` and `export`; §7: the guest listens on vsock |
| `rustd/crates/runner/afr_supervisor/**` | EDIT | §2 to §4, §8: holds by mode, files through the executor, the egress plan, the engine's evidence, credits capped by memory |
| `rustd/crates/runner/main/**` | EDIT | §5, §7: features per engine, `EngineChoice::Firecracker`, the kernel lane over `&dyn Engine` |
| `rustd/crates/protocol/af_wire/src/runner/capability.rs`, `af_wire/src/protocol/mod.rs` | EDIT | §2: the report names engine, isolation, KVM and the memory bound; `VERSION` moves once |
| `rustd/crates/daemon/afd_runner/src/reconcile.rs` | EDIT | §2: `proven_by` reads hardware-VM evidence |
| `rustd/crates/e2e/af_e2e/Cargo.toml`, `af_e2e/tests/layers.rs`, `af_e2e/tests/layers/rules.rs` | EDIT | §5: the engine-crate rule and its planted twin |
| `rustd/crates/foundation/af_core/tests/backtrace.rs` | EDIT | §5: names the moved crate |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | members; `tokio-vsock` for the guest; `procfs-core`, already pinned, for available memory |
| `make/test-unit.mk` | EDIT | §6, §7: `test-runner-kernel` takes `ENGINE`; no new target |
| `scripts/toolbox/**` | EDIT | §7: the 6.18 guest kernel build with EROFS; a new script file there needs Indy's sign-off |
| `deploy/baremetal/{agentsfleet-runner.service,toolbox.sh}`, `playbooks/lib/runner/{prepare.sh,common.sh}` | EDIT | §7, §9: pinned binaries, the jail root, KSM and swap off, the engine flag |
| `deploy/local/{linux.sh,linux_test.sh,linux-runner.sh,linux-runner.Dockerfile}` | EDIT | §9: the acceptance runner gets `/dev/kvm` and `--engine firecracker` |
| `.github/workflows/{build-candidate.yml,acceptance-on-demand.yml}`, `.github/actions/sign-toolbox/action.yml`, `scripts/release_candidate/signature.py` | EDIT, with Indy's approval at build | the guest kernel ships signed with the toolbox; the acceptance runner opens `/dev/kvm` |
| `cli/src/commands/platform/runners.ts`, `cli/test/platform-output.unit.test.ts` | EDIT | §2: print the reported engine; `DEFAULT_ENGINE` goes |
| `public/openapi.json` | EDIT | regenerated with `--no-banner` for the report's new fields |
| `docs/architecture/{runner_execution,runner_fleet,checkpoints,concurrency,rust-ownership,README}.md`, `docs/development.md`, `docs/AUTH.md`, `docs/RUST_ERROR_STANDARD.md`, `SECURITY.md` | EDIT | two engines, the crate map, the KVM lane; `checkpoints.md:164` wrongly says no deployment sets `[checkpoint.storage]` |
| `.orly/LOCAL.md` | EDIT, with Indy's approval | its Linux-only line names `afr_sandbox` |
| `~/Projects/docs` self-hosting pages and `changelog.mdx` | EDIT, on that repository's own branch after merge | a tenant runner host needs usable KVM to run Firecracker |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR and RS-SOURCE for the seam's new kinds; RS-FEATURE and RS-COMBOS for the engine features; RS-UNSAFE; RS-RACE.
- **`code.md`** — NLG, ORP for the crate split, OWN for one cleanup per jail, tap and drive, FLS, TIM, ECL, CODE-MSID, FLL, HLP.
- **`tests.md`** — TCF, TEST-NEG, TEST-CONTEND for one sandbox per id, TEST-KILL, TEST-INJECT, TEST-RESIDUAL, TEST-LANE, TEST-SKIP.
- **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND. **`domains/logging.md`** — LOG-EVENT, LOG-SECRET. **`domains/http-surface.md`** — HTTP-OPENAPI.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` with `cargo shear` | yes | each move drops the dependencies it leaves behind |
| `make test-unit-all` | yes | new crates sit under `crates/runner/`, which both shard globs select (`make/test-unit.mk:9`, `make/test-integration-rustd.mk:120`) |
| `af_e2e/tests/layers.rs` | yes | every new crate is `afr_` in `runner/`; the engine-crate rule arrives with its twin |
| `make test-runner-kernel`, and `ENGINE=firecracker` on a KVM host | not in a green Pull Request | both run before the Pull Request opens; counts go in the Session Notes |
| `make acceptance` on a KVM machine | not in a green Pull Request | §9 runs it before the Pull Request opens |
| OpenAPI drift | yes | regenerate with `--no-banner` |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the Firecracker engine splits into launch, drives, network, vsock and sweep modules |

## Prior-Art / Reference Implementations

- **Reference:** SPACE (`Sandboxed Platform for Agentic Code Execution`) at `~/Projects/oss/rs/SPACE`, `0aa39485e`, Apache-2.0, read for design.
  - A required capability statement, and no default that answers `Unsupported` (`crates/l40_api/api/src/controllers/substrate/AGENTS.md:82-90`).
  - A caller-chosen sandbox identifier, and an ambiguous mutation reconciled, never replayed (`crates/l40_api/node/runtime/core/src/control.rs:30-31`).
  - Pause, suspend and stop kept apart; network policy as numbered data, with start waiting until it is in force (`proto/net/gateway/v1/gateway.proto:44-52`).
  - The jailer's flag order and a one-file boot (`crates/l40_api/node/runtime/core/src/launch/os/linux.rs:651-723`, `boot/firecracker.rs:1-63`).
  - The vsock `CONNECT` handshake with peer pinning (`crates/l40_api/spaced/client/src/vsock.rs:64-158`).
- **Divergence:** none of SPACE's services, gRPC, SQLite node state, operation queue or guest agent. Our executor is the guest agent, and the daemon's run fence stays the authority. No identifier, comment or test names SPACE; only a copied file's licence notice may.
- **Reference:** `docs/reviews/space-sandbox/sandbox_evaluation.md`, "First Firecracker wrapper" and "Selective SPACE reuse". §1 follows its build order, and a copied file names the six things it lists.
- **Reference:** Fly.io's "The Design & Implementation of Sprites" (https://fly.io/blog/design-and-implementation/), adopted on Indy's word. Its pools of empty VMs become §8's gated pool, and its object-store disk is M258_002's, behind §3's snapshot interface. Its inside-out design becomes §7's guest layout.
- **CLI practices:** the one CLI change keeps all six. Layered command, handler and error; handlers that neither print nor exit; one renderer. Structured JSON errors; three test tiers; JSON when stdout is not a terminal.

## Sections (implementation slices)

Build order: §1 is the spike, committing only its record. §2 to §4 reshape the seam inside `afr_sandbox` with bubblewrap underneath, so every commit stays green.

§5 splits the crates once the supervisor names no host type. §6 lifts the suite, §7 adds Firecracker, §8 bounds admission, and §9 switches the dev host.

### §1 — Spike: one VM booted by hand on the dev host

Boot one VM on the dev runner host before any code, with Firecracker and the jailer 1.18.x. The guest kernel is 6.18 from Firecracker's CI configuration, plus `CONFIG_EROFS_FS` with LZ4. The toolbox is a read-only drive and a workspace image the data drive; the record lands in `docs/v2/reviews/m258-firecracker-spike.md`.

- **Dimension 1.1** — ✅ DONE (Nov 26, 2026; `docs/v2/reviews/m258-firecracker-spike.md`) — the toolbox boots as a VM's root → Test `firecracker_boots_the_toolbox_by_hand` (manual). Aiwa records `uname -r`, threads per core, the toolbox's SHA-256, idle resident set size (RSS), and create-to-ready as a median over a stated count of boots. A root other than our toolbox's digest is a false pass.
- **Dimension 1.2** — ✅ DONE (Nov 26, 2026; `docs/v2/reviews/m258-firecracker-spike.md`) — the executor answers over vsock → Test `the_executor_answers_through_the_vsock_handshake` (manual). The runner's own binary is the guest entry, as an initramfs or a third drive, whichever boots. `CONNECT <port>` answers `OK` and `list_dir` answers; a wrong port answers no `OK`.
- **Dimension 1.3** — ✅ DONE (Nov 26, 2026; `docs/v2/reviews/m258-firecracker-spike.md`) — a stopped VM loses no byte → Test `a_signal_stopped_vm_keeps_its_synced_disk` (manual). With the data drive's `cache_type` `Writeback`: write, sync in the guest, stop the VMM by signal, boot, and the file reads back. With `Unsafe` the record states what was lost, and no boot made an API call.

### §2 — Lifecycle, identity and resources, named on every heartbeat

**(5) Lifecycle and identity.** `Engine` gains a required `kind()` and `capabilities()`. `Sandbox` gains a required `id()`, `environment()` and `granted()`, and its four defaults go.

`environment()` replaces `toolbox_digest()`, which defaulted to none. It names the root, our toolbox by its SHA-256 or a vendor's image by name and version, beside the executor's digest. The toolbox is the root on our hosts only, bubblewrap's and Firecracker's, and today's check refuses a restore across roots (`docs/architecture/checkpoints.md:90`).

`prepare` takes a `SandboxSpec` whose `id` the supervisor mints with `af_core::id`, so a retried build names the same sandbox; `RunName` goes. `freeze`, `thaw` and `recovery_quiescent` become `hold(mode)`, `resume()` and `quiesce() -> Witness`. An engine's `sweep(keep)` destroys what a crash or an ambiguous build left.

**State authority:** each engine's own per-id state, a directory, jail or remote name. **Guarded transition:** a second `prepare` for a live id answers that sandbox. **Failure assumption:** a build of unknown outcome answers `Ambiguous`, and the caller sweeps it, never repeating it.

**(3) Resources.** `admit(limits) -> Granted` states what the engine will enforce before anything is built, never less than asked. It refuses a cap the engine has no mechanism for, and `Granted`, not `Limits`, is what a run reports. **Implementation default:** bubblewrap grants exactly what it is asked, through cgroup files, because that is today's behaviour.

**Heartbeat.** The capability report gains `engine`, `isolation`, `kvm` and `sandboxes_max`, edited in place. The self-test reads its checks from the engine's boot evidence, so only an engine that needs KVM judges it. `proven_by` maps a hardware-VM report onto the guarantees Landlock proves today (`afd_runner/src/reconcile.rs:6-26`), and the CLI prints the reported engine.

- **Dimension 2.1** — every engine states its facts → Test `every_engine_states_its_capabilities` (unit, `afr_sandbox`). Each engine's `capabilities()` equals its stated table, and its `environment()` names a root. A hold mode outside the table answers `Unsupported` naming `hold`.
- **Dimension 2.2** — one id, one sandbox → Test `a_second_prepare_for_one_id_answers_the_same_sandbox` (unit, unsandboxed engine). 100 callers released from a barrier with one id get one sandbox directory, and all 100 answers name it.
- **Dimension 2.3** — an unknown outcome is swept, never retried → Test `an_ambiguous_prepare_is_swept_never_retried` (unit, the supervisor's fake engine). A build that times out answers `Ambiguous`, one `sweep` destroys it, no second `prepare` follows, and the run reads `sandbox_unavailable`.
- **Dimension 2.4** — a cap the engine cannot meet is refused → Test `admit_refuses_a_limit_the_engine_cannot_enforce` (unit). A fake engine with no process cap, asked for 512 processes, refuses naming `pids`. Bubblewrap's `Granted` equals its `Limits`.
- **Dimension 2.5** — only an engine that needs KVM judges it → Test `a_bubblewrap_host_without_kvm_passes_its_selftest` (unit, `afr_supervisor`). Bubblewrap evidence with `Kvm::Absent` reads `all_ok`. Firecracker evidence with `Kvm::Denied` does not, and its `kvm` check names the open failure.
- **Dimension 2.6** — a VM report proves the cage → Test `a_hardware_vm_report_proves_the_cage_guarantees` (unit, `afd_runner`). `engine: firecracker` with `isolation: hardware_vm` under `landlock_full` reads healthy. `engine: unsandboxed` under it reads degraded with its own reason.
- **Dimension 2.7** — the CLI prints what the runner reported → Test `platform runners show prints the reported engine` (unit, handler). A row whose report names `firecracker` prints it, and a row with no report prints `unknown`. Neither prints `Bubblewrap` from a constant.

### §3 — Storage and the executor's transport

**(1) Storage.** The executor gains `import(at, tree)` and `export(paths)`: a tar stream, confined to `/workspace`, each entry owned by the sandbox user. It is chunked under the 64 MiB frame (`afr_executor/src/protocol/mod.rs:48`), so a repository of any size crosses.

Checkout fetches into the host mirror with the token, packs the selected tree and imports it. `HostWorkspace` and `Sandbox::workspace()` go; the images stay engine-internal, loop-mounted by bubblewrap and attached as drives by a VM.

**The snapshot interface.** The storage concern carries `Snapshots`: take a snapshot under a caller-minted id, restore one, forget one. Both VM engines implement it later: ours on M258_002's chunk map, Sprites on Fly's own checkpoints in M260_001. §6 holds every implementation to one observable behaviour, and this spec designs no mechanism behind it.

**Today's file checkpoint path keeps working, unchanged, until M258_002 replaces it.** It scans the workspace, encrypts 8 MiB chunks, uploads them to R2 once each, and keeps manifests in Postgres (`docs/architecture/checkpoints.md:42-43,111,119-121,129`). Only its byte source moves, from the host path to `export` and `import`; §7's Dimension 7.6 proves it on Firecracker.

**(4) Transport.** The executor client speaks the same JSON lines over any byte stream a `Transport` dials. It redials once when a call finds the link closed. Bubblewrap dials its socket pair once, Firecracker dials vsock after every boot, and a process the old link carried reads `Interrupted` (`afr_executor/src/client/mod.rs:1-8`).

- **Dimension 3.1** — files cross with no host path → Test `import_then_export_returns_the_same_tree` (unit, unsandboxed engine). A fixture tree with modes, relative links and an empty directory is imported. The exported tree's SHA-256 digest equals the imported one.
- **Dimension 3.2** — nothing lands outside the workspace → Test `an_import_entry_leaving_its_root_is_refused` (unit, `afr_executor`). `../x`, an absolute link and a link out of `/workspace` are each refused, and nothing is written.
- **Dimension 3.3** — checkout and recovery need no host path → Test `checkout_fills_a_workspace_the_host_cannot_see` (integration, `afr_supervisor`). An engine with no host path checks out the fixture with today's tree digest. The recovery drill engine captures with today's checkpoint path through `export` and restores it byte for byte.
- **Dimension 3.4** — the link redials after a hold → Test `the_executor_link_redials_after_a_hold` (unit, over `tokio::io::duplex`). The far end closes, the client redials once, and the next call answers. A process the old link carried reads `Interrupted`.

### §4 — Networking and credentials

**(2) Networking.** The supervisor compiles one egress plan per run: no network, listed hosts with their resolved IPv4 addresses, or the public internet. Every plan carries a generation and the ranges our address rule blocks (`af_core/src/net/mod.rs:16-22`). `Sandbox::egress()` applies a plan and reads back the generation the enforcer reports; `reallow` goes.

**State authority:** the enforcer's own read-back, never a remembered value. **Guarded transition:** a generation older than the one in force is refused. **Commit before release:** `prepare` returns only once the plan reads back in force, so no command runs before it.

**(6) Credentials.** One rule, with no engine trait: no secret enters a sandbox. Checkout keeps the fetch token in the host mirror, and model keys and minted tokens stay in the supervisor (`docs/architecture/runner_execution.md:26-28`). §6's planted-token trial proves it on every engine.

- **Dimension 4.1** — nothing runs before the plan is in force → Test `prepare_returns_only_after_the_plan_is_in_force` (unit, a fake enforcer). When read-back lags two calls, `prepare` waits and the executor sees no command. When the enforcer fails, `prepare` refuses and destroys the sandbox.
- **Dimension 4.2** — an old plan cannot replace a newer one → Test `a_stale_generation_is_refused` (unit). With generation 4 in force, applying 3 is refused, and 4 reads back.
- **Dimension 4.3** — every plan refuses the reserved ranges → Test `every_plan_refuses_the_reserved_ranges` (unit, the plan compiler). No-network compiles to no device. A listed or public plan drops private, `169.254.0.0/16`, `100.64.0.0/10`, loopback and IPv6 destinations before any accept.
- **Dimension 4.4** — the fetch token never crosses → Test `the_checkout_stream_carries_no_fetch_token` (unit, `afr_supervisor`). A canary token fetches the fixture. No byte of the imported stream holds it, `.git/config` included.

### §5 — Crate layout: one seam crate, building blocks, one crate per engine

Indy asked for better crate management, with the runner using an engine crate. Every crate below sits in `runner/` with the `afr_` prefix, so `layers.rs`'s folder and prefix rule holds. Both shard globs select them unchanged; Indy approved the names (D8).

- **`afr_engine`**, the seam only: the traits, the per-concern traits for storage, egress, transport and resources, and capability and limit types. It holds the error type through `error_shell!`, boot admission, warm slots and the suite behind `test-util`. It has no operating-system code, so the supervisor depends on it alone.
- **Building blocks:** `afr_image` (toolbox verify, stage, adopt and holds; the workspace image; loop devices and mounts), `afr_firewall` (today's `egress`, slots and links) and `afr_cgroup` (limits, the freezer, tenant attribution).
- **`afr_guest`**, `harden` and `serve`, the sandbox-side entry, plus §7's inner environment. It departs from the coordinator's proposal, which put both in the bubblewrap crate: the Firecracker guest hardens and serves too.
- **Engines:** `afr_engine_bubblewrap`, deleted whole in M258_003; `afr_engine_unsandboxed`, the debug engine with the recovery-drill engine behind `test-util`, used by `main` and `af_e2e`; `afr_engine_firecracker` (§7).
- **Next:** `afr_engine_sprites` arrives in M260_001 (O2). `afr_engine_bubblewrap` lives only inside this branch, and M258_003 deletes it before the Pull Request opens.

`probe` splits by owner: each building block probes its own mechanism, and each engine's boot admission assembles its evidence. Only `main` names a concrete engine, behind one cargo feature per engine, all on by default. Each engine crate builds on every OS with its Linux code `cfg`-gated, so `--all-features` builds on a Mac.

`layers.rs` gains one rule. No crate but `agentsfleet_runner` and `af_e2e` depends on an `afr_engine_` crate, and no engine crate depends on another.

- **Dimension 5.1** — the layout passes the layer rules → Test `only_the_runner_binary_and_e2e_name_an_engine_crate` (unit, `af_e2e/tests/layers.rs`). The workspace's manifests pass it. They also pass `every_crate_depends_on_its_own_layer_or_lower` and `every_crate_runs_in_exactly_one_unit_shard`.
- **Dimension 5.2** — a planted violation is named → Test `an_engine_crate_named_by_the_supervisor_or_another_engine_is_refused` (unit, `layers.rs`). Planted manifests have `afr_supervisor` depend on `afr_engine_x`, and `afr_engine_a` on `afr_engine_b`. Both edges are named.
- **Dimension 5.3** — the supervisor names the seam alone → Test `the_supervisor_names_the_seam_alone` (unit, `layers.rs`). `afr_supervisor`'s manifest lists `afr_engine`. It lists no `afr_engine_` crate and none of `afr_image`, `afr_firewall`, `afr_cgroup` or `afr_guest`.

### §6 — The conformance suite, which bubblewrap passes

`afr_engine::conformance`, behind `test-util`, is one list of named trials over `&dyn Engine`, in the kernel lane's shape. Today's trials join it once they take `&dyn Engine`, and `make test-runner-kernel` takes `ENGINE`, defaulting to bubblewrap. A trial for a mechanism an engine does not claim asserts `Unsupported`, so the lane fails and never skips (`make/test-unit.mk:81`).

**What must match, and what may differ.** Network verdicts, granted caps, file transfer, command streams, exit codes, output bounds and the planted-token search must match on every engine. The root image, tool versions, start time and declared hold modes may differ. The suite prints that list beside each engine's environment, so a difference is stated, never hidden.

**Snapshot trials.** Every engine that declares snapshots passes four trials. A save is taken and a restore made; no process of the old environment survives, and the files equal the save's. No engine declares snapshots in this spec, so the trials assert `Unsupported` until M258_002 implements them on our engine.

- **Dimension 6.1** — bubblewrap passes the suite → Test `make test-runner-kernel` (integration, a real Linux kernel). Every trial passes, among them confinement, budgets, kill, hold, toolbox, files, git, shared memory and §2 to §4's trials. The lane prints each name and a zero failure count.
- **Dimension 6.2** — a false claim fails the suite → Test `a_false_capability_fails_the_suite` (unit, `afr_engine`). A fake claiming `Pause` that answers `Unsupported` fails that trial by name, and no trial reads skipped. A fake whose network verdict differs fails, though a differing root alone passes.
- **Dimension 6.3** — only listed destinations connect → Test `the_verdict_matrix_refuses_every_unlisted_destination` (integration, kernel lane). A listed host connects. An unlisted host or literal, `10.0.0.5`, `169.254.169.254`, `100.64.0.1`, `::ffff:10.0.0.1`, an unlisted lookup and a QUIC dial on UDP 443 fail.
- **Dimension 6.4** — no secret enters a sandbox → Test `no_secret_in_sandbox` (integration, kernel lane). A checkout runs with a canary fetch token, then `search` covers the guest's files, environment and `/proc`. It finds zero hits, and finds a canary planted on purpose.
- **Dimension 6.5** — every snapshot holds to one behaviour → Test `the_snapshot_trials_hold_every_engine_to_one_behaviour` (unit, `afr_engine`, fakes). A fake that saves, restores, ends the old environment and returns equal files passes all four trials. A fake that leaves one old process alive fails by name, and one that differs by a byte fails by name.

### §7 — Our Firecracker engine

`afr_engine_firecracker` launches each sandbox through the stock jailer. Each gets its own user and group, from a range sized `af_core::limits::MAX_WORKERS`, and a chroot under the runner's `StateDirectory`. It also gets a cgroup leaf under the delegated subtree and the slot's network namespace.

Firecracker boots from one `--config-file`. The toolbox drive is the loop device today's verification attached from the hashed descriptor, and the workspace image is a writable drive. That drive has `cache_type` `Writeback` and a rate limit of today's I/O cap.

**Guest layout, inside out.** The guest entry is the runner's own binary, attached as §1 found, and the executor is the VM's root service on vsock. Tenant code runs in an inner environment the executor builds, with its own mount and process namespaces. Inside it, today's hardening holds: `no_new_privs`, Landlock, and seccomp with the `unshare` refusal.

The executor can tear the inner environment down and build it again without rebooting the VM. A restore swaps the workspace under it, and a plan's names reach the inner `/etc/hosts` when the plan is applied. The VM is the tenant wall; inside it, confinement applies to the inner environment, not to the executor.

**Implementation default:** the toolbox drive is the verified loop device, because it keeps "nothing reopens the image by path" with no new release format; in-guest dm-verity waits. Memory is the guest's size and processors are whole virtual CPUs, rounded up. The process cap is a declared `None`, because the VM is the wall.

**Holds** are `Stop` only: quiesce, guest sync, VMM stopped, disk kept, and resume boots a fresh VM on it (O3). No API call is needed for this, so O4's copy waits until a pause, snapshot or balloon is ruled in.

**Egress** reuses `afr_firewall`'s per-slot table on a tap, dropping the reserved ranges ahead of the accept, with no IPv6 on the tap. No-network attaches no device. Firecracker filters nothing itself, so this table is the only enforcer.

**One VM per workspace image.** §1's first run let two VMMs mount one ext4 image, and the guest read `checksum invalid` (`docs/v2/reviews/m258-firecracker-spike.md`). The engine takes an exclusive lock on a workspace image before it attaches it and holds it while the VMM runs, so a second attach is refused before any VMM starts.

**Only the host reaches the executor.** The guest kernel is built without `CONFIG_VSOCKETS_LOOPBACK`, so no guest process can dial the executor's port, as §1 found. The executor also refuses a connection whose peer is not the host's context identifier, 2. The host socket lives in the jail, owned by that jail's user.

- **Dimension 7.1** — Firecracker passes the same suite → Test `make test-runner-kernel ENGINE=firecracker` (integration, the dev host). Every trial passes or asserts `Unsupported` as declared, the verdict matrix, planted token and toolbox digest included. Counts go in the Session Notes.
- **Dimension 7.2** — no KVM, no Firecracker, no fallback → Test `firecracker_refuses_a_host_without_usable_kvm` (unit, `afr_engine_firecracker`). With `Kvm::Denied` or `Kvm::Absent`, boot refuses naming `kvm`, and no engine starts. `--engine firecracker` parses.
- **Dimension 7.3** — a crash leaves nothing → Test `a_runner_killed_mid_build_leaves_no_jail_tap_or_table` (integration, kernel lane, Firecracker). `SIGKILL` reaches the runner during `prepare`. The next boot's sweep leaves no jail directory, tap, table, VMM process or drive.
- **Dimension 7.4** — a hold keeps the disk, not the VM → Test `a_stopped_hold_resumes_on_its_own_disk` (integration, kernel lane, Firecracker). A file is written, then a hold leaves no VMM running. On resume the file reads back, no earlier process remains, and the plan reads back in force.
- **Dimension 7.5** — a restore needs no reboot → Test `a_restore_inside_a_running_vm_replaces_the_workspace` (integration, kernel lane, Firecracker). A background process runs and a file is written; then a different file checkpoint is restored in place. The workspace equals that checkpoint's tree, no process of the old inner environment survives, and the VMM's process identifier is unchanged.
- **Dimension 7.7** — one VM per workspace image → Test `a_second_vmm_cannot_attach_a_held_workspace_image` (integration, kernel lane, Firecracker). With a sandbox running, a second `prepare` naming its workspace image is refused naming the image's lock, and no second VMM process starts.
- **Dimension 7.8** — only the host reaches the executor → Test `a_guest_process_cannot_reach_the_executor` (integration, kernel lane, Firecracker). From the inner environment, dialing the executor's vsock port at context identifiers 1, 2 and 3 fails. A connection whose peer identifier is not 2 is refused, in a unit test over a fake listener.
- **Dimension 7.6** — today's checkpoints still work on Firecracker → Test `test_supervisor_repository_durable_task_retains_base_then_incremental` (integration, `make test-runner-kernel ENGINE=firecracker`). This existing trial passes unchanged in what it asserts: a base, then an incremental checkpoint, saved and restored. So does `test_supervisor_api_durable_task_captures_frozen_workspace`, renamed only if its "frozen" no longer holds.

### §8 — Memory admission overcommitted 4 to 1, and a pool of four pre-booted VMs

A VM reserves address space for its guest memory, and Firecracker backs that space only as the guest touches it. Indy ruled overcommit from the start, at a ratio he named: 4 (O3).

At boot the engine reads `MemAvailable` from `/proc/meminfo` with `procfs-core`. `sandboxes_max` is the ratio times that, divided by one sandbox's reservation: guest memory plus the VMM overhead §1 measured, named beside its constant.

The ratio is `memory_overcommit` in the runner's configuration file, default 4, with Indy's ruling cited beside the default. The supervisor's capacity becomes the lesser of `sandboxes_max` and `worker_count` (`afr_supervisor/src/downlink/running.rs:54-73`), and a `Stop` hold reserves nothing.

**When guests outgrow the host.** Under overcommit, the guests' touched memory can pass the host's, and the host kernel then ends one VMM. The engine reports that sandbox lost. Its run ends `sandbox_unavailable` and resumes from its last checkpoint, as a killed runner's run does today, and the other sandboxes run on.

**A pool of pre-booted VMs**, four per host, as Indy ruled (D15). The size is `warm_pool` in the runner's configuration file, default 4. Every VM boots the one standard image, our toolbox, so a pooled VM serves any fleet. Today's `WarmSlots` wraps the engine (`runner_execution.md:317`). A claim applies the run's plan and imports its files, and each pooled VM counts against `sandboxes_max`.

- **Dimension 8.1** — credits stop at the overcommitted bound → Test `credits_stop_at_the_overcommitted_memory_bound` (unit). `MemAvailable` of 28 GiB, 2 GiB runs, 16 MiB overhead, ratio 4 and `worker_count` 64 give `sandboxes_max` 55. The runner offers at most 55 credits, and ratio 1 gives 13.
- **Dimension 8.2** — nothing is admitted past the bound → Test `a_sandbox_past_the_memory_bound_is_refused` (unit). With 55 admitted, the next `admit` refuses naming memory, and no VM starts. Once one sandbox goes to a `Stop` hold, the next `admit` succeeds.
- **Dimension 8.3** — a pooled VM serves one run and leaves nothing behind → Test `a_claimed_pool_vm_serves_one_run_with_its_own_plan` (integration, kernel lane, Firecracker). A pool of four serves a run whose plan and files reach it, and the pool refills to four. The next claimant finds none of the first run's files or processes, and the pool plus live runs never pass `sandboxes_max`.
- **Dimension 8.4** — a VM the host ends takes only its own run → Test `a_vmm_ended_by_the_host_ends_only_its_own_run` (integration, kernel lane, Firecracker). `SIGKILL` reaches one of two running VMMs. Its run ends `sandbox_unavailable` and `sandbox_lost` is logged, while the other run's next command answers.

### §9 — The dev host runs Firecracker

The dev runner host restarts with `--engine firecracker` once §7 passes there. Acceptance runs on GitHub's hosted runner, which exposes `/dev/kvm` to root only (A1). The workflow opens it to the runner user with a udev rule, and the acceptance runner takes the same flag.

- **Dimension 9.1** — the dev host reports Firecracker → Test `dev_host_reports_firecracker` (manual). Aiwa restarts the unit. `GET /v1/admin/runners/{id}` shows `engine: firecracker`, `kvm: usable`, §8's `sandboxes_max` and a self-test `all_ok`; evidence goes in the Session Notes.
- **Dimension 9.2** — the real path on Firecracker → Test `make acceptance SUITE=all` (e2e). Every prime fixture spec passes with the acceptance runner on `--engine firecracker`. A run on bubblewrap fails it, because the runner row must report `firecracker`.

## Interfaces

**The seam** (`afr_engine`), in outline.

```text
Engine:  kind() · capabilities() · admit(&Limits) -> Granted · prepare(&SandboxSpec) -> Box<dyn Sandbox> · sweep(keep) -> Swept
Sandbox: id() · environment() · granted() · state() · executor() · egress() · snapshots() -> Option
         quiesce() -> Witness · hold(HoldMode) · resume() · kill() -> Killed · destroy(self)
SandboxSpec { id: Uuid7, run_id, granted: Granted, network: &NetworkSpec }      HoldMode: Pause | Suspend | Stop
Capabilities { isolation, location, holds, snapshots, quiescence, egress: EgressFacts, resources: ResourceFacts }
Environment { engine, root: Toolbox { digest } | Vendor { name, version }, executor: digest, kernel: Option<digest> }
NetworkSpec { mode: None | Listed | Public, allow, generation: u64 }   Egress: apply(&NetworkSpec) · in_force()
Executor: today's ten verbs + import(at, tree) · export(paths)          Transport: dial() -> byte stream
Snapshots: take(id: Uuid7) -> SnapshotRef · restore(&SnapshotRef) · forget(&SnapshotRef)   (implemented in M258_002; Sprites in M260_001)
Error kinds added: Unsupported{engine, operation} · Remote{engine, operation, status} · Ambiguous{sandbox, operation}
```

**Wire.** `CapabilityReport` gains `engine` (`bubblewrap`, `firecracker` or `unsandboxed`), `isolation` (`shared_kernel` or `hardware_vm`), `kvm` (`usable`, `denied` or `absent`) and `sandboxes_max`. `sandboxes_max` is absent where the engine reserves no memory. The five Linux booleans stay until M258_003, and the runner protocol `VERSION` moves once.

**Runner.** `agentsfleet-runner run --engine bubblewrap|firecracker`; `make test-runner-kernel ENGINE=bubblewrap|firecracker`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| No usable KVM | failure | the device is absent or denied | boot refuses naming `kvm`; no engine starts | `firecracker_refuses_a_host_without_usable_kvm` |
| Ambiguous build | failure | a build times out mid-way | `Ambiguous`; one sweep; never repeated; `sandbox_unavailable` | `an_ambiguous_prepare_is_swept_never_retried` |
| Runner killed mid-build | failure | `SIGKILL` or a crash | the next boot's sweep removes jail, tap, table and drives | `a_runner_killed_mid_build_leaves_no_jail_tap_or_table` |
| Plan not in force | security | the enforcer lags or fails | `prepare` waits, or refuses and destroys | `prepare_returns_only_after_the_plan_is_in_force` |
| Stale plan | race | an older plan applied after a newer one | refused; the newer stays | `a_stale_generation_is_refused` |
| Hostile tree | security | an import entry escapes `/workspace` | refused; nothing written | `an_import_entry_leaving_its_root_is_refused` |
| Link dropped | failure | a hold or a closed socket | one redial; old processes `Interrupted` | `the_executor_link_redials_after_a_hold` |
| Memory bound reached | capacity | one more VM than the overcommitted bound | `admit` refuses; credits stop | `a_sandbox_past_the_memory_bound_is_refused` |
| Guests outgrow the host | capacity | overcommitted guests touch more memory than the host has | the host ends one VMM; its run ends `sandbox_unavailable` and resumes from its last checkpoint; others run on | `a_vmm_ended_by_the_host_ends_only_its_own_run` |
| Secret in a sandbox | security | a token in the tree or environment | the planted-token trial fails the lane | `no_secret_in_sandbox` |
| Private or metadata dial | security | a tenant dials a reserved address | dropped before the accept | `the_verdict_matrix_refuses_every_unlisted_destination` |
| False capability | failure | an engine claims a mode it lacks | the suite fails by name | `a_false_capability_fails_the_suite` |
| Hold loses bytes | failure | the VMM stops with writes unflushed | guest sync and a `Writeback` drive | `a_stopped_hold_resumes_on_its_own_disk` |
| Workspace attached twice | failure | a leftover or second VMM names a live image | the image's exclusive lock refuses the second attach; no VMM starts | `a_second_vmm_cannot_attach_a_held_workspace_image` |
| Guest dials the executor | security | tenant code opens a vsock connection | no loopback in the guest kernel; the executor refuses a non-host peer | `a_guest_process_cannot_reach_the_executor` |
| Old process outlives a restore | security | a tenant process escapes the teardown | the inner process namespace ends with its init; the restore refuses if any remains | `a_restore_inside_a_running_vm_replaces_the_workspace` |

## Invariants

1. A refused engine starts no other — `EngineChoice::boot` maps one choice to one backend and returns its refusal (Dimension 7.2).
2. No command reaches a sandbox before its plan is in force — `prepare` hands back the sandbox only after `in_force()` reads the spec's generation (Dimension 4.1).
3. One identifier builds at most one sandbox — `SandboxSpec.id` is a caller-minted `Uuid7` that each engine keys its state by (Dimension 2.2).
4. Every engine states its capabilities — `capabilities()` has no default, so an engine without it does not compile (Dimension 2.1).
5. Only the runner binary and the e2e crate name an engine — `layers.rs` reads every manifest (Dimensions 5.1, 5.2).
6. A Firecracker host never admits past its overcommitted bound — `admit` refuses past `sandboxes_max` (Dimension 8.2).
7. No tenant process outlives its inner environment — the environment is a process namespace, and ending its init ends every process in it (Dimension 7.5).

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `sandbox_engine_booted` (log) | operator | the engine admits the host at boot | `engine`, `kvm`, `sandboxes_max`, `worker_count` | no path, token or tenant identifier | `firecracker_refuses_a_host_without_usable_kvm` |
| `runner_capacity_bounded` (log) | operator | `sandboxes_max` is below `worker_count` | `worker_count`, `sandboxes_max`, `mem_available_bytes`, `memory_overcommit` | host facts only | `credits_stop_at_the_overcommitted_memory_bound` |
| `sandbox_lost` (log) | operator | the host ends a VMM the engine did not stop | `engine`, `reason` | no tenant identifier | `a_vmm_ended_by_the_host_ends_only_its_own_run` |
| `sandbox_swept` (log) | operator | a sweep destroys leftovers | `engine`, `destroyed`, `left` | no tenant identifier | `a_runner_killed_mid_build_leaves_no_jail_tap_or_table` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every open owner decision is ruled (Discovery) | `git grep -n -E "^  - \*\*[A-Z][0-9]+\*\* .\[\?\]" -- "docs/v2/*/M258_001_*"` | no output | P0 | |
| R2 | The spike's record is committed (§1) | `git ls-files docs/v2/reviews/m258-firecracker-spike.md` | one line | P0 | |
| R3 | The seam states its facts, builds once per id, sweeps what it cannot tell, and holds every snapshot to one behaviour (§2, §6) | `make test-unit-all` | exit 0; `every_engine_states_its_capabilities`, `an_ambiguous_prepare_is_swept_never_retried`, `the_snapshot_trials_hold_every_engine_to_one_behaviour` pass | P0 | |
| R4 | Files and the plan cross with no host path and no secret (§3, §4) | `make test-integration-rustd` | exit 0; `checkout_fills_a_workspace_the_host_cannot_see ... ok` | P0 | |
| R5 | The layout keeps engines behind the seam (§5) | `make test-unit-all` | exit 0; `only_the_runner_binary_and_e2e_name_an_engine_crate`, `the_supervisor_names_the_seam_alone` pass | P0 | |
| R6 | Bubblewrap passes the suite (§6) | `make test-runner-kernel` | exit 0; zero failed trials | P0 | |
| R7 | Firecracker passes the same suite on the dev host, today's checkpoints included (§7) | `make test-runner-kernel ENGINE=firecracker` | exit 0; zero failed trials; `test_supervisor_repository_durable_task_retains_base_then_incremental` passed | P0 | |
| R8 | Admission stops at the overcommitted bound (§8) | `make test-unit-all` | exit 0; `credits_stop_at_the_overcommitted_memory_bound` passes | P0 | |
| R9 | The real path runs on Firecracker (§9) | `make acceptance SUITE=all` | exit 0 | P1 | |
| R10 | The old crate is gone (§5) | `git ls-files rustd/crates/runner/afr_sandbox` | no output | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the old crate | `git grep -n -w afr_sandbox -- rustd cli deploy make scripts playbooks .github` | no output |
| the host-path seam | `git grep -n -w -e HostWorkspace -e recovery_quiescent -e reallow -e RunName -e run_id_unsafe -- rustd` | no output |
| the method that defaulted the root to none | `git grep -n -e "fn toolbox_digest" -- rustd` | no output; the checkpoint wire field of that name stays |
| the CLI's constant engine | `git grep -n -w DEFAULT_ENGINE -- cli` | no output |

## Out of Scope

- Production hosts and deleting bubblewrap: M258_003, in this Pull Request.
- Fly.io Sprites, the engine policy, burst and a Sprites rate: M260_001 (O2).
- The browser, Chromium in a guest, and lifting the `unshare` refusal it needs: M259. "Chromium's own sandbox starts in a Firecracker guest" stays a hypothesis until its spike runs.
- Checkpoint and snapshot rebuilt on a live chunk map, with R2 chunks and Postgres manifests as the workspace's truth: M258_002. This spec carries only the snapshot interface and its trials.
- Memory snapshots, pause holds and balloon reporting: not planned until a measurement asks.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator reads `engine: firecracker` on the dev runner, and a fleet's run answers as it did on bubblewrap.
2. **Preserved user behaviour** — every tool, hold, checkpoint and network policy works as today, and a fleet sees no new setting.
3. **Optimal-way check** — implement the existing trait once more. The gap is the host path, which §3 closes for every engine.
4. **Rebuild-vs-iterate** — iterate on the trait and the executor. Rebuild the crate layout, which Indy asked for.
5. **What we build** — the split seam, the crate layout, the suite, the Firecracker engine, overcommitted memory admission, a pool of four, the heartbeat fields and the dev host switch.
6. **What we do NOT build** — memory snapshots, a second VMM, a Mac engine or SPACE's services. None is needed to retire bubblewrap.
7. **Fit with existing features** — compounds with checkpoints in R2, holds and tenant runners. It must not destabilize the egress verdicts.
8. **Surface order** — runner first, then the daemon's reconcile, then the operator CLI, because the report drives both.
9. **Dashboard restraint** — the app shows nothing new. The operator CLI shows only what the heartbeat reported.
10. **Confused-user next step** — a refused boot names `kvm` and the probe's reason. `agentsfleet-runner probe` and the self-test print the same.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** a spike, three seam Sections by concern, the crate split, the suite, the engine, admission and the switch. Each Section leaves bubblewrap green.
- **Alternatives considered:**
  - Run SPACE as the engine. Refused: its own Postgres schema, SQLite on each node and nine node services break "Postgres holds every durable fact".
  - Depend on SPACE's crates, which are `publish = false` with no API promise, or `fctools`, alpha and validated on host kernels 5.10 and 6.1 only.
  - Cloud Hypervisor, a larger device surface with no GPU need, or gVisor, which gives a run no kernel of its own.
  - Keep bubblewrap as a dev-only engine. Code that passes locally would fail deployed.
- **Patch-vs-refactor verdict:** a **refactor** because the seam's shape, the crate graph and the supervisor's file paths change, with a second engine behind them.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "bubblewrap must be retired if we do 3rd party sprites based firecraker or any sandbox or firecracker engine our own." and "CAn you move our spec to 258_001, 258_002 then?" — context: retirement and numbering.
- > Owner (Indy, Nov 26, 2026): "I also need the sprites firecracker in fly as either burst or sole one. So customers can pick. In our case we pick 1. our hardware + burst as sprites 2. an user can pick sprintes fully So the policy must be configurable on the engine choice. I think our code is abstracted that way. 3. Is headless chromium the simplest or anyothers equivalent to chromium 4. I want to replace the engine first 5. Then the web search with parallel + other providers like opencode and the browse decision 6. so 2 milestones, in the same milestone i need the tools of guffbolt to be improved with the system prompts and such proposed like opencode" — context: the milestone shape and the engine-first order.
- > Owner (Indy, Nov 26, 2026): "Also while building our own I think should we refer and use ~/Projects/oss/rs/SPACE ? from perplexity which build a firecracker gRPC layer?" and "OUR ABSTRACTION OF THE ENGINE MUST BE IMPROVED BY LOOKING A THE ~/PROJECTS/OSS/RS/SPACE? MAY BE? ON TEH TRAITS AND SUCH" — context: the Prior-Art adoptions.
- > Owner (Indy, Nov 26, 2026): "Okay where will the engine abstraction live and all these changes i want a better crate management now, since the runner should use another engine crate or so is my thinking" and "So i can't burst to fly sprites if i had my own host with kvm running firecracker ?" — context: §5; burst belongs to M258_003.
- > Owner (Indy, Nov 26, 2026): "does afr_engine_sprites take the erofs type of toolbox or?" and "afr_engine_sprites - root = Fly's ubuntu 26.04 - can you switch to Debian 13. if sprites offers theirs example fly debian 13 then why mount our toolbox inside, out toolbox sticks to our host only?" — context: the toolbox is the root on our hosts only, and §2's `environment()` names each sandbox's root.
- > Owner (Indy, Nov 26, 2026): "also read this sprite design https://fly.io/blog/design-and-implementation/ so you know if you prefer to redesign anything", then "i am thinking we must do that" and "even in our checkpoint and restore to make them faster" — context: §3's storage verb, §7's guest layout and §8's gated pool. The post, saved at `~/.local/state/agentsfleet-m254/vm-engines/fly-sprites-design-post.md`, is a cited source, not a mechanism to copy.
- > Owner (Indy, Nov 26, 2026): "I prefer the milestone where ever we do the firecracker and sprites engine abstractions move (to me this is a compute layer) must  fix the checkpoint and snapshot, this must be the first milestone so we sunset bubblewrap and move on to firecracker and sprites" — context: ruled; M258 is the compute layer and lands first. Its packing into three Workstreams is O1.
- **Record, Oct 25, 2026:** "The owner requested a small Firecracker wrapper with measured startup behavior and control over future changes" (`docs/reviews/space-sandbox/README.md`). That concept review left 12 Major findings and one Nit.
- **Owner decisions, ruled Nov 26, 2026** in AskUserQuestion dialogs; each quote is Indy's, verbatim.
  - **O1** one Pull Request for all of M258. > Owner (Indy, Nov 26, 2026): chose "One PR for all of M258" — "the reason i prefer 1 is cause your memory is fad, you will keep stale dead code of bubblewrap still in. That is the reason i need 1 PR supporting all." — context: the three Workstreams build in order on one branch.
  - **O2** Sprites leave M258. > Owner (Indy, Nov 26, 2026): "Also i think i prefer we move the sprites to M260 milestone_001 But the M259_003 becomes what ever we had sans sprites as a new engine" and "since sprites as a new engine is a separate scope doesnt yield any benefit"; asked again, chose "Production switch + delete bubblewrap (Recommended)" — context: M258_003 is production and the deletion; the engine policy, burst and a Sprites rate go to M260_001. On Fly's terms: "well customers will feed their own secrets of fly. they wont use our fly account."
  - **O3** overcommit 4 to 1. > Owner (Indy, Nov 26, 2026): chose "Overcommit from the start", then "A fixed ratio you name" — "who will handle this?" — then "Ratio 4 (your earlier pick)" — context: `memory_overcommit = 4` in the runner's configuration file (§8). Holds keep the disk only.
  - **O4** > Owner (Indy, Nov 26, 2026): chose "Approve, copy when needed (Recommended)" — context: SPACE's `fc_api.rs` is copied with its notices only when a pause, snapshot or balloon first needs the API.
  - **O5** > Owner (Indy, Nov 26, 2026): "M258 is the compute layer and lands first. M259 (web search, browse, guffbolt tools and prompts) follows." — context: the start message.
  - **D6** > Owner (Indy, Nov 26, 2026): chose "Stock 6.12, suite as proof (Recommended)" — context: the dev host keeps Debian 13's 6.12. For production: "in M258_003 just make assumptions around how you saw in dev machine = same kernel debian 13, kvm that you made it work with may be better ram, cpu cores and nvme disks."
  - **D7** > Owner (Indy, Nov 26, 2026): chose "On for the dev host (Recommended)" — context: SMT stays on in dev.
  - **D8** > Owner (Indy, Nov 26, 2026): chose "These names (Recommended)" — context: §5's crates; `afr_engine_sprites` moves to M260_001.
  - **D15** > Owner (Indy, Nov 26, 2026): chose "Build now, pool of 4" — context: `warm_pool = 4` (§8, Dimension 8.3).
  - **A1** settled by probe, Nov 26, 2026. GitHub Actions run 45864550316 on `ubuntu-24.04` (Ubuntu 24.04.5, kernel 7.0.0-1017-azure) and `ubuntu-latest` (Ubuntu 26.04.1, kernel 7.0.0-1020-azure) printed `dev_kvm=present`, `crw-rw---- 1 root kvm 10, 232`, openable by root and not by the runner user. The E2E Networks probe answered `401 … "The Auth Token provided is expired."` and created nothing. > Owner (Indy, Nov 26, 2026): "okay try usin gh for burst then" and "and stop probing e2enetworks" — context: Firecracker acceptance runs on GitHub's hosted runner (§9).
  - **Spike** > Owner (Indy, Nov 26, 2026): "yeah use the latest firecraker and so on. what is jailer 1.18 is that the latest" — context: §1 approved on the dev runner host; v1.18.0 is the latest release (`gh release list --repo firecracker-microvm/firecracker`, published Nov 09, 2026), and the jailer ships in it.
- **Dev host facts,** read over Secure Shell (SSH) on Nov 26, 2026: x86_64, Debian 13, kernel 6.12.111+deb13-amd64, ext4 root. `/dev/kvm` and `/dev/net/tun` are present and btrfs is in the kernel. It has 8 CPUs, 31 GiB, 409 GB free, no Docker or Kubernetes, and 64 bubblewrap workers.
- **Consults** — the research reports named in Provenance. Skeptic corrections applied: SPACE's worker gRPC API is external (SPACE's `proto/node/api/v1/worker.proto:11-13`), and no maintained Firecracker client exists. Both deployments store file checkpoints in R2 and only resume is off (`deploy/fly/agentsfleetd-prod/agentsfleetd.toml:85,132-135`).
- **Model reviews** — none yet. The Elephant-Goldfish Model (EGM) review runs at `orly-spec` before the docs Pull Request.
- **Open blockers** — none. How the chunk map learns which chunks changed belongs to M258_002; this spec invents no mechanism for it.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
