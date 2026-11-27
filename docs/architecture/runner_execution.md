---
type: explanation
audience: contributor
verified: 2026-11-27
product_version: 0.90.0
executable: false
---

# Runner runtime and sandbox

## What it is

`agentsfleet-runner` is one binary with two entries, `run` and `probe` ([main](../../rustd/crates/runner/main/src/main.rs)). Inside each microVM runs a second binary, `agentsfleet-guest`, the guest agent ([guest](../../rustd/crates/guest/main/src/main.rs)); it ships only inside the guest's initramfs and is never installed on a host. This page follows a run from the moment the daemon pushes it to the moment its report is acknowledged.

| Part | What it is |
|---|---|
| Supervisor | The trusted `run` process. It holds the stream, the agent loop, model keys and minted tokens |
| Engine | What builds each sandbox: a Firecracker microVM with its own kernel |
| Sandbox | One per run, built by the runner's engine. It runs tool calls and nothing else |
| Toolbox | A signed, read-only Debian filesystem image every sandbox runs on |
| Workspace disk | A per-run writable disk holding `/workspace` and `/tmp` |
| Parked sandbox | A finished run's sandbox, frozen and kept for its fleet's next run |
| Network | The runner's assigned policy: the public network, none, or an allowlist |

## Why it exists

Fleets run tenant programs: build scripts, test suites, package installs and Python the model writes. The sandbox is the only wall between two tenants' processes on one host.

Each sandbox is a Firecracker virtual machine (VM) with a kernel of its own.

The agent loop runs outside that wall. A prompt-injected command therefore cannot read a model key, a runner token or a minted credential, because none enters a sandbox.

## How it behaves

### Process model

```text
agentsfleetd ◄── native runner stream (af_wire) and fenced HTTP run verbs
   ▲
agentsfleet-runner (one binary on a VM or a bare-metal host)
 ┌ supervisor: trusted, outside every sandbox ──────────────────────────┐
 │ stream · renewals · pending reports in memory · fleet memory         │
 │ agent loop and model providers         (model keys live only here)   │
 │ tool router · secret scrub · activity frames · git clone · minting   │
 └───────────────┬──────────────────────────────────────────────────────┘
                 │ one agent link per run: vsock to the microVM
 ┌ sandbox: one per run ────────────────────────────────────────────────┐
 │ /            toolbox, read-only                                      │
 │ /workspace   the run's disk, writable                                │
 │ guest agent: processes on pipes or pseudo-terminals, file calls      │
 │ network per assigned policy · Landlock · seccomp · no capabilities   │
 └──────────────────────────────────────────────────────────────────────┘
```

The guest agent is a small in-sandbox server. It spawns, writes to, reads and kills processes, and it serves the file calls the file tools use.

In a microVM, `agentsfleet-guest` is the guest kernel's first process and serves the agent on vsock. The runner links none of its crates: the two speak only `af_guest_wire`. The shipped runner is built with `--no-default-features`, so it carries neither the debug engine nor the agent that engine serves in-process ([layer rules](../../rustd/crates/e2e/af_e2e/tests/layers.rs)).

Every tool call ends exactly once. When a run ends for any reason, the supervisor closes each open call as `interrupted` and kills every command session still open.

### One run, end to end

```text
Down::Run
  ├─ admit      the policy's tools and provider; an unhosted one refuses the run
  ├─ turn       one run per fleet on this runner; a second run for it waits
  ├─ memory     the fleet's memory window, carried on the run
  ├─ admitted   size, egress and checkout plan, before any model call
  ├─ run        the agent loop in the supervisor; sandbox tool calls cross the socket
  │  └─ build   beside it: the fleet's parked sandbox, or a fresh one with repositories checked out
  ├─ keep       a processed run's sandbox is parked; any other is killed
  ├─ settle     tool records, then the report with memory changes and timing
  └─ release    a killed sandbox's disk, beside at most 5 s of live-activity drain
       renewal runs alongside from the start until the report is acknowledged
```

A failed admission check ends the run as `startup_posture` before the turn starts. The checks cover an unhosted tool or provider, unreadable memory, a size past the wire's bounds, unresolvable egress and an unserved repository binding.

The sandbox builds beside the first model call. A sandbox call waits for the build, and `run_completed` logs that wait as `sandbox_wait_ms`.

A failed build leaves the turn going: its sandbox calls answer `sandbox_unavailable`, and nothing runs unsandboxed instead. A build still running 20 seconds after the turn answered is stopped.

The report carries the run's memory changes and timing segments. The worker keeps it in memory and retries it until acknowledgement ([runner protocol](runner_protocol.md#results-and-acknowledgements)).

Usage rides both renewals and the report, one line per provider request, with no count cap. The daemon bills a line sent twice only once ([billing](billing.md#how-a-run-is-charged)).

### Crates

The runner's crates sit in `rustd/crates/runner/` and import nothing from `daemon/`. No runner crate links a datastore crate. The guest agent's crates sit in `rustd/crates/guest/` with the `afg_` prefix and link only `foundation/` and `protocol/` crates; a runner crate links one only in a dev table or through the debug engine ([layer rules](../../rustd/crates/e2e/af_e2e/tests/layers.rs)).

| Crate | Role |
|---|---|
| `guffbolt` | The agent loop as a library: model turns, tool calls, child agents. It knows no run, fleet or task |
| `guffbolt-ai` | The model wires: Chat Completions, Responses, Messages, Gemini and Ollama |
| `af_providers` | Runs a model wire for one run, with its error codes and logs |
| `afr_agent` | Embeds `guffbolt` for a run: prompt, context window, checkpoints, frames and the tool router |
| `afr_tools` | The tool catalog and the verbs that reach `agentsfleetd` |
| `afr_supervisor` | The stream, worker pool, renewals, reports, holds, clones, host stages and checkpoint transfer |
| `afr_engine` | The seam every engine meets: its traits and facts, egress plans, boot admission, warm slots and the conformance suite |
| `afr_engine_firecracker`, `afr_engine_unsandboxed` | One crate per engine; the second is the debug and recovery-drill engine |
| `afr_image`, `afr_firewall`, `afr_cgroup` | Building blocks: toolbox and workspace images, the host egress tables, and a run's cgroup limits |
| `afr_guest_client` | The host's client for the guest agent: the `GuestAgent` seam the agent loop calls, the redialling link and the slot a run's agent is reached through |
| `af_guest_wire` | What the runner and the guest agent both speak: the link's messages and framing, the refusals both name, the tree's tar stream and the guest kernel's command line |
| `af_cgroup` | The cgroup v2 file names and the freezer both ends write |
| `afg_agent`, `afg_confine`, `afg_entry`, `agentsfleet_guest` | The guest agent: the in-sandbox server, the hardening and inner environment each tenant process runs in, the guest's first-process entry, and its binary |
| `af_wire` | The one wire format the daemon and runner share |

`afr_supervisor` reaches every engine through `afr_engine` and names no engine or building-block crate. Only the runner binary, `agentsfleet_runner`, and the `af_e2e` crate name an engine crate, and no engine crate names another ([layer rules](../../rustd/crates/e2e/af_e2e/tests/layers.rs)).

Model routing and saved model settings belong to [models](models.md#routing-from-saved-settings). The prompt belongs to [capabilities](capabilities.md), pinned memory to [memory](memory.md), and the context window to [context lifecycle](context_lifecycle.md).

### Tool catalog

Every run is offered every published tool and the seven child-agent tools. A run's `ExecutionPolicy.tools` lists them; a tool the catalog does not host refuses the run.

A call to a name outside the policy returns a tool error, and the run continues. A call the daemon refuses returns as `[kind] sentence`, so the model can correct it.

| Tool | Runs in | Notes |
|---|---|---|
| `http_request`, `web_fetch` | Supervisor, through the run's egress guard | The network policy applies; credentials go only in `Authorization` |
| `notify` | Supervisor | Needs an approved `pushover` secret whose `host` is `api.pushover.net` |
| `web_search` | The model provider | Needs a provider that offers it |
| `memory_store`, `memory_recall`, `memory_list`, `memory_forget` | Supervisor | Only an explicit `core` category is pinned into later prompts |
| `request_access` | Supervisor, through a fenced run verb | Ends the run waiting on an approval card unless a grant already covers it |
| `ask_user` | Supervisor | Ends the run waiting on a question; a checkpoint is saved first |
| `trigger_add`, `trigger_list`, `trigger_remove` | Supervisor, through fenced run verbs | Needs a connected provider and a signing secret ([wakes on](user_flow.md#wakes-on)) |
| `task_start`, `task_input` | Supervisor, through the task binding verb | Only on a task-capable runner, and only from a person's message |
| `propose_change` | Supervisor, inside a durable task | Publishes to the task's existing pull request; no merge, no new pull request |
| `message`, `fleet_activity`, `fleet_steer` | Supervisor, through runner verbs | Post to the turn's thread, read the workspace's fleets, or message another fleet |
| `schedule`, `cron_*` | Supervisor, through runner verbs | Schedules are daemon rows, and the runner owns no timer ([schedules](data_flow.md#schedules)) |
| `update_plan` | Supervisor | Needs nothing |
| Seven child-agent tools | Supervisor, through `guffbolt::agents` | Depth 2 and six open children per run |
| `shell`, `exec_command`, `write_stdin` | Sandbox | `shell` times out at 10 seconds unless the model names a limit; at most 64 sessions per run |
| `git` | Sandbox, on a clone the supervisor made | Remote subcommands (`push`, `fetch`, `pull`, `remote`, `clone`) are refused |
| `file_read`, `file_write`, `file_edit`, `apply_patch` | Sandbox | Confined to `/workspace`; `file_edit` writes nothing when one edit fails |
| `image` | Sandbox file read, attached to the next model turn | Source at most 8 MiB, shown at most 5 MiB, scaled to 2,048 pixels |

Unread command output is kept as a 512 KiB head and tail, and the middle is counted. A command that never ends returns `interrupted` with its output so far.

A tool that changes the fleet runs only when the person's newest message asks for it. For the two task tools, the daemon also checks who sent the turn.

`git` runs hooks and aliases, so offering `git` offers what `shell` does. The sandbox is the boundary for both.

### Sandbox engines

A release build runs one engine, Firecracker, which `agentsfleet-runner run` picks with no flag ([engine choice](../../rustd/crates/runner/main/src/engine.rs)). Only a debug build accepts `--engine unsandboxed`, and a refused engine never starts another in its place.

| Engine | Wall, as `isolation` | Park mode | How it meets the caps |
|---|---|---|---|
| `firecracker` | A microVM with its own guest kernel, `hardware_vm` | `Stop` | Memory, processor, process count and disk enforced; the process count is the guest's tenant leaf's `pids.max` |
| `unsandboxed` | None, `unconfined`; a release build refuses it | None | None |

#### The seam

The supervisor sees an engine only through `afr_engine`'s `Engine`, `Sandbox` and `Parked` traits ([seam](../../rustd/crates/runner/afr_engine/src/engine/mod.rs)). No method has a default, so an engine that leaves a fact out does not compile.

- `capabilities()` names the engine, its isolation, its park modes, whether it offers snapshots, and how it meets each cap.
- `grant(limits)` states what the engine will enforce before anything is built. It never grants less than asked, and it refuses a cap it has no mechanism for by name.
- `prepare(spec)` builds a sandbox under a `Uuid7` the caller mints. A second `prepare` for a live identifier is refused naming that sandbox.
- A build whose outcome the engine cannot tell answers `Ambiguous`. The supervisor sweeps that identifier once and never builds it again ([build](../../rustd/crates/runner/afr_supervisor/src/run_loop/build.rs)).
- `environment()` names the engine and the root: the toolbox by its SHA-256 digest, or the host's own files for the debug engine. A checkpoint restore compares that digest.
- `quiesce()` reads from the engine's own mechanism whether a tenant process remains. A checkpoint is captured only when it reads quiet. On Firecracker the guest's agent answers: quiet when no call of its own runs and the tenant leaf lists no process, so one a call left behind reads busy ([leaf](../../rustd/crates/guest/afg_agent/src/server/launch/tenant/mod.rs)). `kill()` ends the monitor and returns once it has exited, and a guest whose monitor has ended reads quiet.

Each engine sweeps what a crashed runner left when the engine is built.

#### Parking between runs

A processed run's sandbox parks in the first mode its engine offers, and a sandbox whose engine offers none is killed ([park](../../rustd/crates/runner/afr_supervisor/src/run_loop/park.rs)). A parked sandbox is its own type, `Parked`, which runs nothing until it resumes.

- `Stop` is Firecracker's. The guest syncs, its monitor stops, and only the disk stays. Resume boots a fresh microVM on that disk. The run waits `[sandbox] park_resume_timeout_seconds` (2) for its guest agent to answer, then gives the park up for a fresh sandbox.
- `Pause` keeps memory and disk while every process stops, and no engine offers it.

#### Egress plans

The supervisor gives every sandbox a numbered egress plan: no network, an allowlist, or the public network ([plans](../../rustd/crates/runner/afr_engine/src/engine/egress/mod.rs)). Plan numbers rise across the runner process.

`afr_engine::enforce` applies a plan for every engine. It refuses a plan older than the one in force, applies the new one, then reads the mechanism back until it holds the new number.

`prepare` returns only once the plan reads back in force, so no command runs before it. A parked sandbox takes its next run's plan the same way. Firecracker reads an allowlist back by dumping the kernel's address set and comparing it with the plan's.

A new allowlist on a running microVM refills the tap's set, then replaces the guest's `/etc/hosts` before the plan applies ([re-plan](../../rustd/crates/runner/afr_engine_firecracker/src/engine/vm.rs)). A warm slot or a resumed microVM therefore resolves its new run's names.

#### Files in and out

No engine offers a host path into its sandbox. Files cross only through the guest agent's `import` and `export`: a tar stream confined to `/workspace`, sent in chunks under the 64 MiB frame ([tree](../../rustd/crates/protocol/af_guest_wire/src/tree/mod.rs), [pack](../../rustd/crates/protocol/af_guest_wire/src/tree/pack.rs), [unpack](../../rustd/crates/protocol/af_guest_wire/src/tree/unpack.rs)).

An import is spooled on the workspace disk and checked whole before its first write. One entry leaving its root refuses the whole stream, and set-id bits are dropped. An export streams as it is packed.

On the host, each step's files wait in a stage, `<storage home>/staging/<sandbox id>-<random>/`, mode `0700` and owned by the runner ([staging](../../rustd/crates/runner/afr_supervisor/src/staging/mod.rs)). A stage is removed when its step ends, and the runner empties `staging/` at boot.

- Checkout fetches into the host mirror with the run's token, checks the tree out into a stage, and imports the stage.
- Checkpoint capture exports the workspace into a stage, which the checkpoint reads ([checkpoints](checkpoints.md#when-the-runner-saves)).
- Restore writes the checkpoint into a stage and imports it into an empty workspace.

The host unpacks an export with the same `af_guest_wire::tree` checks the guest agent applies to an import.

#### The conformance suite

`afr_engine::conformance`, behind the `test-util` feature, is one list of 16 named trials over `&dyn Engine` ([suite](../../rustd/crates/runner/afr_engine/src/conformance/mod.rs)). `make test-runner-kernel` holds Firecracker to the suite on a real Linux kernel with a usable `/dev/kvm`, then runs its own trials. `ENGINE` defaults to `firecracker`, and any other name fails the lane.

The trials cover the grant, command streams, file transfer, write walls, a full `/tmp` and `/dev/shm`, no network, and the verdict matrix. They also cover the planted-token search, kill, each park mode and four snapshot trials.

A trial for a mechanism the engine does not declare asserts `Unsupported`, so the lane passes or fails and never skips. No engine declares snapshots, so the snapshot trials assert `Unsupported` until M258_002 builds them.

The lane prints what may differ between engines beside each run: the root image, tool versions, start time and declared park modes.

#### Firecracker

`afr_engine_firecracker` builds one microVM per run through the stock jailer ([engine](../../rustd/crates/runner/afr_engine_firecracker/src/lib.rs)). Its boot admits the host only when `/dev/kvm`, the Kernel-based Virtual Machine (KVM) device, opens for reading and writing. Otherwise the runner exits naming `kvm` ([device check](../../rustd/crates/runner/afr_engine_firecracker/src/kvm.rs)). A host whose packet filter cannot hold a guest to its egress is refused the same way, naming `nftables` ([admission](../../rustd/crates/runner/afr_engine_firecracker/src/engine/backend.rs)).

Each boot first makes a jail user, the image lock, and a network namespace with its tap and host table. It then makes the chroot and a cgroup leaf, and the jailer starts Firecracker from one `--config-file`, with no API socket ([boot](../../rustd/crates/runner/afr_engine_firecracker/src/engine/build.rs)).

| Device | What it carries |
|---|---|
| `vda` | The toolbox: the verified loop device, read-only |
| `vdb` | The workspace: the run's ext4 image, read-write, `cache_type` `Writeback`, capped at `[sandbox] io_mib_per_second` |
| `vdc` | The plan's `/etc/hosts` and `/etc/resolv.conf`, read-only; absent under a public plan |
| `eth0` | The tap, only when the plan reaches a network |
| vsock | Guest context identifier 3; the host's end is a Unix socket in the jail |

The guest's memory is the grant rounded up to whole mebibytes. Its processors are the grant rounded up to whole virtual CPUs, at most 32 ([grant](../../rustd/crates/runner/afr_engine_firecracker/src/capabilities.rs)). The monitor's cgroup sets `memory.high` at the guest memory plus `[firecracker] monitor_overhead_mib` (64 MiB, the spike's measured monitor overhead), and admission reserves that much per microVM. Its `memory.max` sits `[firecracker] write_cache_mib` higher (256 MiB, Indy's number). The host caches a guest's unsaved workspace writes in that cgroup, so past `memory.high` the kernel slows the monitor and writes the pages back before `memory.max` kills it ([cgroup](../../rustd/crates/runner/afr_cgroup/src/cgroup/mod.rs)). The monitor's cgroup sets no `pids.max`: its threads are Firecracker's own, and the process cap is the tenant's, enforced inside the guest.

`[firecracker]` in the runner's file names `firecracker`, `jailer`, `kernel`, `initrd`, `first_jail_id`, `memory_overcommit`, `disk_overcommit`, `warm_pool`, `monitor_overhead_mib` and `write_cache_mib` ([section](../../rustd/crates/runner/main/src/config/firecracker.rs)). The program defaults are `/usr/local/bin/firecracker` and `/usr/local/bin/jailer`. The guest defaults are `/usr/local/lib/agentsfleet/vmlinux` and `/usr/local/lib/agentsfleet/guest.cpio`.

#### Inside a microVM

The guest kernel starts `agentsfleet-guest` as its first process ([entry](../../rustd/crates/guest/afg_entry/src/lib.rs)). It refuses to start unless it is process 1 with a vsock device, naming which it is not, so it never mounts over a host ([admission](../../rustd/crates/guest/afg_entry/src/admit.rs)). Its death ends the microVM, and the run resumes from its checkpoint on a fresh one. It mounts the toolbox drive's `/usr` and `/etc`, and the workspace drive at `/workspace` and `/tmp`. It brings loopback up, so a process inside reaches its own `127.0.0.1` with or without a network device. It sets the guest kernel to overcommit memory always, so a process asking for more than the guest holds is ended by the out-of-memory killer and its run reads `out_of_memory`, instead of failing on a refused allocation. Then it serves the guest agent on vsock port 1024.

Every tenant process runs in the guest's tenant cgroup leaf, whose `pids.max` is `[sandbox] pids_max`, passed on the guest's kernel command line as `--pids-max` ([guest entry](../../rustd/crates/guest/afg_entry/src/vm/linux.rs)). A fork bomb stops there with `EAGAIN`, and the guest agent, outside the leaf, still answers. When a run ends, the supervisor reads the leaf's `pids.events` `max` count through the guest agent; above zero, it logs `sandbox_pids_limit_hit` with `refused` and `pids_max` and counts `agentsfleet_runner_sandbox_pids_limit_hits_total`, the data `pids_max` is tuned from ([park](../../rustd/crates/runner/afr_supervisor/src/run_loop/park.rs)).

The guest agent is the VM's root service. Tenant code runs in an inner environment the guest agent builds: a new process namespace, a new mount namespace and a new inter-process communication (IPC) namespace, whose init reaps orphans ([inner environment](../../rustd/crates/guest/afg_confine/src/inner/mod.rs)).

Each tenant process enters it as uid and gid 1000, under `no_new_privs`, Landlock and seccomp with `unshare` refused. The guest agent stands outside that confinement, and the VM is the wall around both. Which confinement a process applies is the guest agent's choice, behind one seam ([confinement](../../rustd/crates/guest/afg_confine/src/harden/confinement.rs)); a tenant's request names none.

A process is born into the inner process namespace because the thread that forks it was readied first. Once the fork is done the guest agent puts that thread back, since a thread readied that way can start no thread of its own process ([entry](../../rustd/crates/guest/afg_agent/src/server/launch/inner.rs)).

The guest agent's `renew` ends the inner environment, empties `/workspace` and builds it again while the VM runs on. Ending the namespace's init ends every process in it, so no tenant process outlives a renewal. The IPC namespace goes with its last process, so no System V object or POSIX message queue a tenant made outlives one either.

The host reaches the guest agent through the monitor's socket with a `CONNECT 1024` line ([vsock](../../rustd/crates/runner/afr_engine_firecracker/src/vsock.rs)). The guest agent client dials again after every boot, and a process the old link carried reads `Interrupted`.

#### The console

Each microVM's serial console, `ttyS0`, carries the guest kernel's messages and the guest agent's own log lines; Firecracker writes it to its standard output. The engine pipes the monitor's standard output and error and reads each in a task of its own, one line at a time, logging every line as `sandbox_console` with `sandbox_id`, `stream` and `line` through the runner's own logging and export. A line longer than Linux's default pipe capacity, 64 KiB, is logged in pieces of that size, each but the last marked `continues` ([console](../../rustd/crates/runner/afr_engine_firecracker/src/console.rs)). The task ends when the pipe closes, and it reads as fast as the monitor writes, so the guest never waits on it.

No tenant process writes a console line. Landlock grants a tenant writes beneath `/workspace`, `/tmp`, `/dev/pts` and `/dev/shm` and to `/dev/null`, `/dev/zero`, `/dev/full`, `/dev/tty` and `/dev/ptmx` alone, so `/dev/console`, `/dev/ttyS0` and `/dev/kmsg` refuse it ([walls](../../rustd/crates/guest/afg_confine/src/harden/mod.rs)). A kernel message a tenant provokes, such as an out-of-memory kill, is the kernel's line.

#### Only the host reaches the guest agent

- The guest agent drops a vsock peer whose context identifier is not the host's, 2, before it reads a byte.
- The guest kernel has no vsock loopback, so no guest process can dial its own guest agent.
- The host's end of the vsock is a socket inside the jail, owned by that jail's user.

#### Walls between microVMs

Each microVM has its own kernel. Between two microVMs on one host, these walls also hold:

- Each jail has its own user and group, counted up from `[firecracker] first_jail_id`. Every jail file is mode `0600` for that user, so one monitor cannot open another jail's image, socket or chroot ([jail](../../rustd/crates/runner/afr_engine_firecracker/src/jail.rs)).
- An exclusive `flock` on each workspace image refuses a second attach before a second monitor starts ([lock](../../rustd/crates/runner/afr_engine_firecracker/src/lock.rs)).
- Each tap's host table admits only its plan. A public plan drops every private, link-local and carrier-grade range before its accept, so a guest cannot dial another guest's address.
- Each microVM has its own vsock device, which reaches only its own host socket.
- The toolbox drive is shared read-only, and workspace drives are never shared.
- Kernel same-page merging (KSM) stays off, so no page is merged between guests. The kernel lane fails when `/sys/kernel/mm/ksm/run` reads on.
- A pooled microVM, once claimed, belongs to that run and never returns to the pool.
- Simultaneous multithreading (SMT) stays on, so two microVMs may share a physical core. [`SECURITY.md`](../../SECURITY.md) records that risk as accepted.

#### Memory and disk admission, and the pool

A microVM reserves its guest memory, and the host backs it only as the guest touches it. Its workspace image is sparse, and the host backs it only as the guest writes it. A Firecracker host therefore admits more guests than its memory or its disk holds at full size ([bound](../../rustd/crates/runner/afr_engine/src/engine/bound.rs)).

At boot, after its sweep, the engine reads `MemAvailable` from `/proc/meminfo` and the free space of the file system holding its state directory. Memory admits `[firecracker] memory_overcommit`, 4 by default, times `MemAvailable`, divided by one microVM's cgroup reservation. Disk admits `[firecracker] disk_overcommit`, 8 by default, times the free space, divided by one sandbox's granted disk. `sandboxes_max` is the lesser of the two.

A build past the bound is refused naming the resource that made it, memory or disk, before any monitor starts ([admission](../../rustd/crates/runner/afr_engine_firecracker/src/admission.rs)). A `Stop` park or a destroy frees its reservation, so a parked sandbox holds none.

The runner offers the daemon the least of `worker_count`, `sandboxes_max` and the frame's run limit as credits ([credits](../../rustd/crates/runner/afr_supervisor/src/downlink/running.rs)). When the bound is the smallest, the runner logs `runner_capacity_bounded` with `bound_by`, `memory` or `disk`, and what each resource had and admits.

If guests touch more memory than the host has, the host kernel ends one monitor. The engine logs `sandbox_lost`, and that sandbox answers `lost` to every call. Its run ends as a transport loss, and the other sandboxes run on.

A Firecracker runner keeps `[firecracker] warm_pool` microVMs booted ahead, 4 by default, through `WarmSlots` ([warm slots](../../rustd/crates/runner/afr_engine/src/warm_slots/mod.rs)). Each pooled microVM holds a reservation, so the heartbeat's `sandboxes_max` is the engine's bound less the pool. A slot serves only a run with no network and the default limits; every other run builds cold.

#### Sizes

Every sandbox gets the runner's `[sandbox]` values, and the wire carries no per-run size. The defaults are 2 GiB of memory with no swap, 2 cores (`cpu_millicores`, 2,000) and 512 processes (`pids_max`). Disk I/O is capped at 200 MiB/s, and the disk holds 4 GiB.

### Sandbox filesystem

The guest agent mounts a fixed set of file systems before it serves the host ([guest mounts](../../rustd/crates/guest/afg_entry/src/vm/linux.rs)). The drives behind them are listed under [Firecracker](#firecracker).

| Inside | From | Mode |
|---|---|---|
| `/usr`, `/etc` | The toolbox drive, `vda`, mounted at `/toolbox` and bound in place | Read-only |
| `/proc`, `/sys`, `/dev`, `/dev/pts` | The guest kernel's own | As the guest mounts them |
| `/dev/shm` | A private in-memory filesystem, a quarter of the run's memory | Read-write |
| `/tmp`, `/workspace` | Directories on the workspace drive, `vdb` | Read-write |
| `/run` | A private in-memory filesystem | Read-write |
| `/etc/hosts`, `/etc/resolv.conf` | The names drive, `vdc`, under `deny_all_egress` and `allow_list_egress`; the toolbox's own under `allow_all` | Read-only |

No host file reaches a microVM except through its drives. Inside the guest, Landlock narrows each tenant process's writes to `/workspace`, `/tmp`, `/dev/pts` and `/dev/shm`, plus the few devices a shell opens.

### Network egress

Each runner is assigned one of three network policies:

| Policy | What a sandbox can reach |
|---|---|
| `allow_all` | Every public IPv4 address through the microVM's tap, with reserved ranges dropped |
| `deny_all_egress` | Nothing; the microVM gets no network device. A missing assignment gets this too |
| `allow_list_egress` | Only the run's allowlist, through one virtual ethernet pair `afv<slot>` on `10.69.<slot>.0/30` |

Under `allow_list_egress`, the runner builds one nf_tables table, `inet afegress<slot>`, in the host namespace, over netlink. Its forward chain drops port 53, accepts the allowed IPv4 set and drops the rest.

A microVM reaches that pair through a tap in its own namespace, `tap0` on `172.16.0.0/30`, which has no IPv6 ([guest network](../../rustd/crates/runner/afr_firewall/src/egress/guest.rs)). The namespace's table only translates addresses, so every verdict is the host table's, which no guest reaches.

The table lives outside the sandbox, so the sandbox cannot flush it. Each table matches only its own link, so one sandbox's rules never touch another's traffic.

The allowlist starts with the runner's `registry_allowlist`, or the default package registries when that list is empty. It adds the hosts of the fleet's approved secrets.

The supervisor resolves those names to IPv4 and writes them into the sandbox's `/etc/hosts`. Its `/etc/resolv.conf` names no server, so a sandbox cannot tunnel data through Domain Name System (DNS) lookups.

The run is refused when a name fails to resolve, resolves only to IPv6, or the netlink build fails. It is refused too when a secret's host resolves to a private or reserved address.

At boot the runner builds one table and tears it down, and a host where that fails is refused naming `nftables`: it runs no sandbox it could not hold to its plan. The runner then takes the host's egress lock, so a second runner is refused before it removes anything ([lock](../../rustd/crates/runner/afr_firewall/src/egress/lock.rs)), and sweeps the tables a crashed runner left.

A fleet's two network toggles decide what its HTTP tools reach under that ceiling. `http_egress` is `restricted` or `public`, and `package_domains` adds the package registries; both start off.

Restricted mode reaches only approved secrets' hosts, plus GitHub under an approved GitHub grant. Public mode reaches any public HTTPS host. Both modes refuse private and reserved addresses.

The toggles never widen the runner's policy. Only a signed-in person changes them; an API key gets `403`.

At `api.github.com`, the run's rules admit only requests that name their method, path and, where it matters, field values ([GitHub rules](../../rustd/crates/daemon/afd_gate/src/policy/egress/mod.rs)).
A read grant admits `GET` and `HEAD` under `/repos/<owner>/<repo>/`. The trailing slash keeps a neighbour such as `<repo>-private` out.

A write grant that publishes an event draft binds one repository and one branch, `agentsfleet-repair/<base64 of the turn ID>`. It adds these rules:

| Request | What it may carry |
|---|---|
| `POST …/git/blobs`, `…/git/trees` | Any body; an object nothing points at changes nothing |
| `POST …/git/commits` | `message`, `tree` and `parents`; no author or committer |
| `POST …/git/refs` | `ref` locked to `refs/heads/<branch>`, plus `sha` |
| `POST …/pulls` | `head` locked to the branch, `base` to the grant's base, `draft: true`; plus `title`, `body`, `maintainer_can_modify` |
| `PATCH …/git/refs/heads/<branch>` | `force: false` and `sha`, so it only fast-forwards |

A locked rule admits no other key and no query string. A write grant with no branch or several repositories refuses the run.

`http_request` follows at most three redirects, and only for a bodyless GET ([redirects](../../rustd/crates/harness/af_egress/src/redirect/mod.rs)). Each hop repeats the egress check, and a change of origin drops every credential header.

### Model calls

`af_providers` opens each model turn through `guffbolt_ai::wire::Connected`, which streams it as a `StreamTurn` over Rig.
Bytes leave through `HttpTransport` and the runner's pooled client. Its guard refuses a private or reserved address, and no retry repeats that ([transport](../../rustd/crates/harness/af_providers/src/transport/mod.rs)).

One reply may carry at most one runner frame, since a larger one could not reach the daemon.

| Bound | Default | Setting |
|---|---|---|
| Connect | 10 s | `connect_timeout_seconds`, 1 to 120 |
| Silence between stream events | 300 s | `stream_idle_timeout_seconds`, 1 to 3,600 |

`TurnControl` carries the caller's absolute deadline and cancellation; neither starts a second attempt.
A turn may reopen only before spend is known or text is shown, so a retry never bills or prints twice.
When a provider response omits usage, the runner decodes its raw document for the counts alone; the failed answer stays failed.

### Credentials

Model keys stay in the supervisor. The run carries them and the fleet's approved secret references; nothing is written to the runner's disk.

The supervisor fetches repositories with the run's one-hour, repository-scoped token. The fetch sends it in an in-memory header, and the clone's configuration names the plain URL ([clone](../../rustd/crates/runner/afr_supervisor/src/workspace_clone/mod.rs)).

The token stays with the host mirror. The checkout lands in a host stage and crosses into the sandbox by `import`, so no byte of the imported stream carries the token ([checkout](../../rustd/crates/runner/afr_supervisor/src/run_loop/checkout.rs)).

A `${secrets.github.token}` placeholder resolves through `POST /v1/runners/me/credentials/mint`, which reads the fleet's current grants. The egress guard puts the token only into a request's `Authorization` header and masks it in the response.

A revoked grant leaves a live run the token it already has. The next run and the next mint lack it.

`message`, `cron_add`, `cron_update` and `schedule` mask minted tokens before text leaves the runner. The daemon then masks the fleet's static secrets before posting to Slack or storing a schedule.

### Workspace between runs

A run that ends processed leaves its sandbox parked ([parks](../../rustd/crates/runner/afr_supervisor/src/parks/mod.rs)). The supervisor parks it in its engine's mode: Firecracker stops the microVM and keeps its disk.

The park is keyed by fleet, workspace, limits, network policy and repository binding. The fleet's next run within the park window resumes it and continues with the files the last run left.

```text
run ends processed ──► park ──► parked (≤ 10 min, ≤ 1 per worker)
                                      │
   next run, same fleet and key ──────┼──► resume ──► runs in the same sandbox
   other key or a failed resume ──────┼──► destroy ──► fresh sandbox
   deadline · worker needed ·         │
   shutdown · refused report ·        └──► destroy
   fleet archived, deleted or parked elsewhere
```

Processes do not survive a park, because the run's end closes every command session. The host never writes into a parked clone, because a tenant-written `.git` could redirect a privileged write.

A durable task's sandbox is never parked, because a durable task restores a fresh workspace from its own checkpoint.

A turn that moves to another runner restores its files from a [checkpoint](checkpoints.md), not from a park. The daemon's park rules are in [runner fleet](runner_fleet.md#capacity-and-parked-sandboxes).

### Toolbox

The toolbox is a minimal Debian root filesystem with git, Python 3, ripgrep, jq, curl, uv and Bun, and a small set of everyday tools: `ps` and `pgrep` (procps), `unzip`, `xz`, `bzip2`, `zstd`, `file`, `patch`, `make` and `tree`. Every Debian package in it is the newest its pinned snapshot serves, and it holds no C compiler. Bun is its only JavaScript runtime and package manager, as `bun`, `bunx` and `node`; no Node.js is installed. Under the name `node`, Bun runs a script and answers `node -p process.version`, and it does not answer `node --version`. The toolbox holds no system package manager, no Perl and no Go.

`make toolbox-image` builds it with mmdebstrap from pinned Debian snapshots ([manifest](../../scripts/toolbox/manifest.txt)). It ships as a read-only Enhanced Read-Only File System (EROFS) image with LZ4HC compression.

Two builds of one release record are byte-identical. The amd64 build refuses an image above 159,999,999 bytes, under 160 MB (Indy, Nov 27, 2026), and the kernel lane's `test_toolbox_image_fits_its_size_cap` holds it to the same number.

The Continuous Integration (CI) host scans the image with Syft and Grype, and a fixable Critical finding blocks signing. The tool cosign then signs the release manifest, and the runner checks that signature offline.

A host downloads the image at install, never at run time, and keeps the current and previous images. Each run holds the image it runs on.

Before a host reports ready, it authenticates the manifest and opens the image once without following links. It hashes that open file and mounts the same file read-only, so a swapped path never reaches the kernel.

A microVM's toolbox drive is the loop device behind that verified mount, so Firecracker never reopens the image by path either.

### Sandbox lifecycle

```text
run start
  ├─ admit      one reservation under sandboxes_max, or refused naming memory
  ├─ mkdir <state>/<sandbox id>         mode 0700, never reused
  ├─ workspace.img 4 GiB → ext4, owned by the guest user, never host-mounted
  ├─ jail user · image lock · netns, tap and host table · chroot · cgroup leaf
  ├─ jailer → firecracker --config-file --no-api
  └─ ready once the agent answers over vsock and the plan reads back in force
            … the turn runs …
  processed: guest sync → monitor stopped → disk kept; otherwise killed
  release: monitor → cgroup → host table and pair → namespace → chroot → lock, jail user
  destroy: release, then the directory and its image
```

The supervisor waits at most 10 seconds for the guest agent to answer before refusing the run. Building the engine sweeps every leftover run directory, egress table and virtual link a crashed runner left.

The storage home is `/var/lib/agentsfleet-runner` unless `RUNNER_STORAGE_HOME` names another root. It holds the toolbox images, per-run sandboxes, one git mirror per workspace and repository, and `staging/`.

| Question | Signal |
|---|---|
| How long does a sandbox take to build? | `agentsfleet_runner_sandbox_start_duration_seconds` |
| Did a tool call wait on the build? | `agentsfleet_runner_sandbox_wait_duration_seconds`; `sandbox_wait_ms` on `run_completed` |
| How often does a build fail, and where? | `agentsfleet_runner_sandbox_builds_total`, by outcome |
| Do runs meet the process cap? | `agentsfleet_runner_sandbox_pids_limit_hits_total`; `sandbox_pids_limit_hit` with `refused` and `pids_max` |
| How long is the release after the report? | `agentsfleet_runner_sandbox_release_duration_seconds` |
| Where did one run's setup time go? | `egress_ms`, `prepare_ms` and `checkout_ms` on `run_completed` |

### What a fleet delete leaves

Deleting a fleet stops its live runs and purges its rows ([data flow](data_flow.md)). These copies follow their own rules:

| Copy | What happens |
|---|---|
| Usage lines | Kept; they name the run, never its content |
| A live run on a runner | Stops at the Cancel or its next renewal, which finds no run |
| Logs, traces and metrics | Never hold prompts, answers or secret values |
| Provider requests | Follow the provider's retention; a local stop cannot erase them upstream |
| Database backups | Follow the infrastructure's backup retention |

## Limits

- The browser tools (`browser`, `browser_open`, `screenshot`) answer with a code. Chromium's own sandbox needs `unshare`, which seccomp refuses inside the microVM.
- The guest kernel must be built without vsock loopback; `scripts/toolbox/guest.sh` refuses a kernel configuration that has it.
- Under a public plan, the guest keeps the toolbox's own `/etc/hosts` and `/etc/resolv.conf`.
- No engine offers snapshots; M258_002 builds them.
- The operator's extra binds on the assigned policy are stored and shown, but no runner binds them.
- A warm slot serves only a run with no network and the default limits.
- An ordinary run cannot push to a repository; only a durable task's `propose_change` publishes.
- The address set matches destinations only. The default registries share front ends with other sites, so a hard boundary needs a mirror the operator owns.

## Related pages

- [Runner fleet](runner_fleet.md): registration, policy, tiers and failure recovery
- [Runner protocol](runner_protocol.md): the stream, leases and report delivery
- [Checkpoints](checkpoints.md): what a replacement runner restores
- [Access](user_flow.md#access): where a grant and its secret hosts come from
- [Configuration reference](config_reference.md): runner keys such as `[sandbox]`, `[tools]` and `[egress]`
- [Observability](observability.md): runner metrics and logs
