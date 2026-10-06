# Runner execution — a trusted supervisor, a sandbox per lease, a workspace that outlives the lease

> Scope: how `agentsfleet-runner` executes one lease — where the agent loop runs, what the sandbox is made of, how a fleet's code and files survive from one lease to the next, and how credentials and repository writes cross the sandbox wall. This page describes the Rust runner. The Zig runner it supersedes is in [Runner Fleet](./runner_fleet.md) §"Running one event". The control protocol, renewal, fencing and report semantics in [Runner Fleet](./runner_fleet.md) are shared and unchanged.

## Facts

| Fact | Value |
|---|---|
| Language | Rust, in the `rustd` workspace beside `agentsfleetd`, under the same principles: one error type per crate through `afd_core::error_shell!`, one source per constant, pure logic apart from I/O. The wire is `afd_wire` and nothing else; nothing in the runner refers to the Zig runner. Daemon→runner types decode leniently, so a runner never refuses a field a newer daemon adds |
| Process model | A trusted **supervisor** runs the lease loop and the agent loop; a per-lease **sandbox** executes tool calls and nothing else |
| Hosts | Bare metal or a VM, multi-tenant from the first release |
| Sandbox engine | Firecracker microVMs for code-running leases from many tenants, on hosts that expose `/dev/kvm`. bubblewrap (Landlock, seccomp, cgroup v2, a per-lease network namespace) ships first, and stays as the engine for development, CI and hosts without `/dev/kvm` |
| Toolbox | A signed, read-only Enhanced Read-Only File System (EROFS) root filesystem built once per release from pinned Debian snapshots, downloaded before any lease and admitted by descriptor. No container image is pulled or unpacked on the lease path |
| Workspace disk | Per sandbox, a writable block image sized to the lease's disk limit, attached with direct I/O, holding both `/workspace` and `/tmp`, deleted when its sandbox is destroyed |
| Warm slot | A sandbox already started before any lease arrives: cgroup made, workspace disk mounted, executor idle. It serves one lease |
| Held sandbox | The sandbox of a lease that ended processed, frozen with `/run/creds` emptied and kept for its fleet's next lease for an idle window. Any other ending destroys the sandbox |
| Workspace | Per fleet, restored from and saved to R2 (or any S3-compatible store, such as a self-hosted RustFS). Only the supervisor moves bytes |
| Model keys | Supervisor only; never inside a sandbox |
| Coding engines | Codex first-class, split across the wall; Claude Code in the sandbox, its model calls through a supervisor relay |
| Repository writes | Local commits inside the sandbox; the supervisor pushes under rules `agentsfleetd` compiles; draft Pull Request only |

## The job it is built for

The design centre is a fleet repairing a production outage, not a chat assistant:

```
alert fires
 → a repair fleet gets a lease on a runner (a VM or a bare-metal host)
 → the runner restores the fleet's last workspace (code, notes, artifacts)
 → the fleet reads logs, runs command-line tools, writes Python, edits code, runs tests,
   or hands the coding to Codex on the user's own subscription
 → it proposes a fix → the supervisor pushes a pinned branch and opens a draft Pull Request
 → the runner saves the workspace, and the sandbox is destroyed
 → a follow-up ("also fix the retry") restores that same workspace on the next lease
```

A lease's inputs are the trigger payload, input files (logs, traces, exports), the repository to fix, reference repositories read-only, the toolbox's command-line tools, and the fleet's durable memory. Its outputs are the answer, a proposed change, artifacts the user can download, memory updates, and the workspace snapshot the next lease starts from.

## Process model

```
agentsfleetd ◄── the runner verbs (afd_wire), unchanged, plus workspace and tool-call verbs
   ▲
agentsfleet-runner (one binary; a VM or a bare-metal host)
 ┌ supervisor — trusted, outside every sandbox ─────────────────────────┐
 │ lease loop · renew · report spool · memory hydrate and push          │
 │ agent loop + model providers            (model keys live only here)  │
 │ tool catalog · router · leak scan · events → activity frames         │
 │ workspace restore and save · git clone · credential minting · push   │
 └───────────────┬──────────────────────────────────────────────────────┘
                 │ one executor connection per lease (a Unix socket)
 ┌ sandbox — one per lease ─────────────────────────────────────────────┐
 │ /            toolbox, read-only                                      │
 │ /workspace   restored, writable, disk quota                          │
 │ /run/creds   memory-only, short-lived scoped tokens, never saved     │
 │ executor: processes on pseudo-terminals, files, apply_patch, Chromium│
 │ own network namespace + allowlist · cgroups · seccomp · no caps      │
 └──────────────────────────────────────────────────────────────────────┘
```

**The agent loop runs outside the sandbox.** No model key ever enters a sandbox, so a prompt-injected command cannot read one, and the sandbox is a plain executor, so bubblewrap and a microVM sit behind one interface. A model call starts while the workspace is still restoring, because nothing about the call needs the sandbox.

**The executor is small and ours.** One process per lease inside the sandbox serves spawn, write, read and kill for processes on pseudo-terminals, and the file calls — read, write, append, delete, list — that the file tools and `apply_patch` edit through from the supervisor. Its methods mirror Codex's `exec-server` (`~/Projects/oss/rs/codex/codex-rs/exec-server/README.md`), so the Codex engine and our loop drive the same shapes.

**Every call ends exactly once.** When a run ends for any reason — answer, crash, kill, timeout — the supervisor closes each call still open as `interrupted`, live and in the trace ([Runner Fleet](./runner_fleet.md) §Live activity). A session's process outlives the call that started it, so the run's end also kills every session still open, and none reaches whatever the sandbox serves next.

## Crates

```
rustd/crates/
  afd_wire              the one wire, shared with agentsfleetd (exists)
  afd_core              error_shell!, timing constants (exists; no datastore dependency)
  afr_executor          executor protocol, in-sandbox server, supervisor-side client
  afr_sandbox           engine interface; bubblewrap engine now, Firecracker engine next;
                        each engine sweeps what a crashed runner's sandboxes left
  afr_agent             agent loop, tool router, events, run trace
  afr_providers         Anthropic Messages, OpenAI Responses, OpenAI-compatible chat
  afr_tools             the catalog: supervisor-side and sandbox-side handlers, each
                        a typed `Handler` whose arguments' type is its JSON Schema
  afr_egress            the outbound guard: admission before any connection, a lease's
                        credentials put in place at send time, the shared transport
  afr_secrets           a held `Secret`, a lease's static secrets, and the one scrub
  afr_memory            one run's memory behind `MemoryBackend`, the backend agentsfleetd
                        binds; `Hydrated` is the Postgres default (runner_fleet.md)
  afr_supervisor        lease loop, renewal, report spool, activity, memory, minting,
                        bundles, storage home, capability report, control-plane client
  agentsfleet_runner    the one binary: composition root; `agentsfleet-runner sandbox`
                        is the sub-mode that runs inside each sandbox (a microVM's init, later)
```

Dependencies point one way: the binary → `afr_supervisor` → `afr_agent` → `afr_providers` and `afr_tools` → `afr_executor`, `afr_memory` and `afr_egress` → `afr_secrets`, with `afr_supervisor` → `afr_sandbox` → `afr_executor`. The supervisor implements `afr_egress::Mint` over its control plane, so the guard mints through the held lease without knowing the daemon's verbs. Every runner crate may depend on `afd_wire`, `afd_core`, `afd_validate` (the bounds the wire's types declare, which depends on garde alone) and `afd_observability` and on nothing else from `agentsfleetd`, and none links a datastore crate. There is one binary. The executor is its `sandbox` sub-mode, re-executed inside the sandbox the way Codex re-executes itself as `codex-linux-sandbox`; that entry constructs only `afr_executor`'s server, never a control-plane client, and the binary is bound read-only into every sandbox. A Firecracker guest gets it inside its root image or on a second read-only disk (§Toolbox).

A lease's `provider` picks the wire through the runner's provider registry, `afr_providers/assets/providers.json`: each name the Zig runner's provider table maps to a wire this runner speaks keeps its wire and base URL, so `anthropic` is Messages, `openai` is Responses and the rest, from `groq` and `mistral` to `deepseek` and `openrouter`, are OpenAI-compatible chat. A `custom:<url>` provider is chat at that URL, taken only as `https` with a host. Any other name, such as one needing request signing, a token exchange, a non-streaming wire or a loopback host, refuses the lease at admission, before a sandbox or a bundle is touched. `rig-core` speaks the three wires, pinned exactly; a registry entry names rig's dialect where its vendor has quirks (its chat path, the fields it rejects, how it hands reasoning back), and perplexity stays a plain gateway because rig's dialect for it drops the tools offered. The base URL is always the registry's. The transport under rig is the runner's: one HTTP client per runner that follows no redirect, so a turn and its key reach the admitted host and no other, and a bounded retry that honours `Retry-After`. A turn that fails before anything visible went out is opened again under the same bound. The reasoning a provider signs goes back with the turn that made it; a call from a turn cut at the output limit is answered `output_limit_reached` and never runs; a call whose arguments are not JSON reaches its tool as the text the model wrote, so the tool refuses it with a reason. rig traces every request and reply whole, before the scrub, so the runner's log filter (`afr_providers::log_filter`) holds rig to its warnings at any level and turns off the one warning that repeats a provider's error message. The model key rides the one header its wire names, and everything the model is sent passes the secret scrub first, the instructions, the event's message and the model's own earlier turns included. A chat lease also carries the fleet's thread before its event, the rows the dashboard thread shows: up to eight finished turns within 64 KiB, each a message and the fleet's answer, which the loop sends as user and assistant turns ahead of the new message, through the same scrub. Webhook, cron and continuation leases carry none. The trusted repair context stays in the system prompt, and no turn enters it. Because a conversation repeats its prefix lease after lease, the Messages wire marks the tools and the system prompt for caching beside the provider's moving breakpoint, at the five-minute default, and the Responses wire sends the fleet id as `prompt_cache_key`. The one-hour lifetime waits for cache writes to be priced: rig counts a write inside input, so the daemon bills it at the input rate. Every lease runs inside a `runner.lease` span under the `agentsfleet-runner` scope, naming the runner by the id the daemon gives its row (read once from `GET /v1/runners/me`), its host, the lease and the fleet; the run, each model turn and each tool call nest inside it as OpenTelemetry `GenAI` `invoke_agent`, `chat` and `execute_tool` spans. With `OTEL_EXPORTER_OTLP_ENDPOINT` set, `run` exports them to the runner collector under a fixed span budget, so a farm of runners is told apart in one trace store ([Observability](./observability.md) §"`agentsfleet-runner` — a collector of its own").

## Tool catalog

The runner is the harness, in Codex's shape: the catalog holds every tool the published tools page names, each with the runtime it executes in; the lease's `ExecutionPolicy.tools` selects which of them the model is offered; the model picks by function calling; the router runs the handler where its runtime says. A policy naming a tool the catalog does not host refuses the lease loudly, the disposition the Zig bridge has today (`src/runner/engine/tool_bridge.zig`), because a bundle running with a quietly different tool set is not the bundle its author wrote. A model call to a name outside the policy is a tool error the model sees, and the run continues. The scheduling tools write rows in `core.fleet_schedules` through the daemon; nothing in the runner sleeps or ticks. They and `message` reach `agentsfleetd` through `afr_tools::LeaseVerbs`, which the supervisor implements over its control plane, adding the lease's id and fencing token to each call, one attempt each: a tool never holds the token, and the model, not a retry loop, decides whether to try again.

| Tool | Runtime | Needs |
|---|---|---|
| `http_request`, `web_fetch`, `pushover` | supervisor | the network policy, the origin rules, placeholders in `Authorization` only |
| `web_search` | the provider, as a hosted tool spec | a provider that offers one; a tool error with a code otherwise |
| `memory_store`, `memory_recall`, `memory_list`, `memory_forget` | supervisor | the hydrated store and the fenced push |
| `update_plan` | supervisor | nothing |
| `message` | supervisor, through a runner verb, to the event's thread | the messages verb |
| `schedule`, `cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs` | supervisor, through a runner verb onto the daemon's plane that QStash fires | the verb; QStash keeps the clock and the runner owns no timer |
| `delegate`, `spawn` | supervisor: a nested loop sharing the lease's sandbox and budget | Codex's `spawn_agent`, `wait_agent` and `send_input` shape |
| `shell`, `exec_command`, `write_stdin` | sandbox, through the executor | `shell`: one process on pipes, its group killed at its timeout; `exec_command` and `write_stdin`: a session per process, on a pseudo-terminal when asked, at most 64 per lease. Every byte of output is forwarded as it arrives, behind a bounded queue a flooding process waits on, and drained for a bounded grace once the leader ends; what a call has not read is kept as a 512 KiB head and tail, the middle counted, and each call drains it. A call says when the output was still open at the end, a gap's marker sits where it fell, a write to a process gone by then says it was not delivered and answers its ending, or runs on until it lands, and a write the process would not take says so and runs on; the executor's own sentence stays on the host, and whatever an executor failure says to a model is rendered to at most 4 KiB (`WIRE_MESSAGE_MAX_BYTES`), a refusal's message and a result that would not decode alike, since the socket's other end is not trusted with the length of what a model reads |
| `git` | sandbox, on a clone the supervisor made on the host side | the token it was fetched with stays in the supervisor; the push is `propose_change` |
| `file_read`, `file_read_hashed`, `file_write`, `file_append`, `file_delete`, `file_edit`, `file_edit_hashed`, `apply_patch` | sandbox, through the executor's file calls | `/workspace`; `file_read` pages by line, `offset` and `limit`, and is cut to the same 10,000-token budget as a command's output |
| `image` | supervisor reads the file through the executor and attaches it to the next model turn | a provider that takes images |
| `browser`, `browser_open`, `screenshot` | sandbox under the Firecracker engine: Chromium from the toolbox, driven over the Chrome DevTools Protocol (CDP) through the process's pipes; the bubblewrap engine answers each with a code | the Firecracker engine; Chromium in the toolbox; the sandbox allowlist for any host beyond loopback |

A lease whose tools are all supervisor-side starts no sandbox. A lease offered any sandbox tool gets its bound repositories checked out before its turn, so a lease of file tools alone reads the repository rather than an empty workspace. git runs programs of its own accord, through hooks, `!` aliases and commands named in configuration, so a lease offered `git` can run what one offered `shell` can, inside the same sandbox; the sandbox is the boundary, and the `git` tool's refusal of remote subcommands is an answer to the model, not a wall.

## Sandbox engines

Fleets on shared hosts run their own programs — build scripts, test suites, package installs, Python the model writes — so the sandbox is the only thing between two tenants' processes and one kernel:

- bubblewrap: a fresh user, PID, IPC, UTS, mount and network namespace per lease; `--cap-drop ALL`, `--disable-userns`, `--clearenv`, `--die-with-parent`, `--new-session`.
- `no_new_privs`, then Landlock, then seccomp. The seccomp filter refuses `io_uring`, `ptrace`, `process_vm_readv`/`writev`, `unshare`, `bpf`, `keyctl` and `perf_event_open`; the last four are calls Codex's own filter leaves open.
- cgroup v2 limits on memory, processor, process count and I/O, with whole-tree kill. The lease cgroup has two leaves: `sandbox` holds bubblewrap and the executor, and `tenant` holds every process the fleet starts, each moving itself in before `exec`. The tenant leaf's memory limit sits `SANDBOX_MEMORY_RESERVE_BYTES` below the lease's, so the out-of-memory killer picks only a tenant process, and the call it ended reads `out_of_memory`.
- An enforced disk quota: the workspace disk is sized by the lease's `disk_write_limit_mb`, holds `/tmp` as well as `/workspace`, and answers `ENOSPC` when full. `/tmp` is never a tmpfs, because tmpfs pages stay charged to the lease's memory until the sandbox ends. The disk attaches with direct I/O, so the host does not cache its writes a second time.
- A host reserve: a worker takes a lease only while the runner's state disk can hold every live sandbox (leases, warm slots and held sandboxes) at its remaining disk limit, one more sandbox at its full limit, and `STATE_DISK_RESERVE_BYTES`.
- A supervisor whose capability set is only what sandbox setup needs (mounts, cgroups, namespaces), while tenant code never holds a capability; and a kernel patch cadence for runner hosts.
- The per-lease network allowlist: a network namespace, a virtual ethernet pair and nftables rules, with rendered resolver files ([Runner Fleet](./runner_fleet.md) §Egress model).

Chromium does not start inside a bubblewrap lease. Its own sandbox needs a nested user namespace or a setuid helper; `--disable-userns` and the seccomp `unshare` refusal deny the first, and `nosuid` with no capabilities denies the second. `--no-sandbox` is never passed, so the browser tools run only under the Firecracker engine.

**The remaining risk is the shared kernel**, and Firecracker removes it. A kernel privilege-escalation bug escapes every namespace sandbox on the host at once, while a microVM gives each lease its own kernel. It does not remove the host: host root, the host kernel and the virtual machine monitor stay inside the trust boundary under either engine. Firecracker needs read and write access to `/dev/kvm`: bare metal has it, and cloud VMs expose it only where the provider offers nested virtualization. Both engines sit behind one interface and share everything outside the boundary itself: the supervisor, the executor (a microVM's vsock surfaces on the host as a Unix socket), the toolbox image (a microVM's read-only root disk), the workspace disk (its data disk), cgroups, and the test lanes. Firecracker's jailer applies the same cgroup and namespace barrier around each microVM before dropping privileges. The engine is a host attribute the control plane assigns ([Runner Fleet](./runner_fleet.md) §Assigned policy and reconciliation). Fleets that run processes lease only to runners whose engine allows it; lease assignment carries no such filter today.

## Toolbox

The toolbox is a minimal Debian root filesystem with pinned git, the GitHub command-line tool, Python 3 with uv, Node, ripgrep, jq, curl, Chromium (Debian's `chromium-headless-shell`, for the browser tools under the Firecracker engine), and the Codex and Claude Code command-line tools. It ships as a compressed read-only EROFS image, is mounted once per host, and is bound read-only into every lease. Each lease writes to its own workspace disk, mounted at `/workspace`. The capability report states whether the host kernel can mount the toolbox's filesystem, beside whether `/dev/kvm` is present; a host that cannot mount the toolbox cannot build a sandbox, so it refuses every lease.

**Built from pinned inputs.** `make toolbox-image` runs mmdebstrap over `scripts/toolbox/manifest.txt` on a native builder for each architecture, and `mkfs.erofs` compresses the result with LZ4 high compression (LZ4HC). The release record pins the snapshot.debian.org inputs for main, updates and security; every package's version and hash; the builder's mmdebstrap, apt, dpkg, erofs-utils and compression-library versions; every binary that does not come from Debian (uv, the coding-engine command-line tools) by URL and SHA-256; the EROFS features enabled; the runner versions the image serves; and the digests of the image and its Software Bill of Materials (SBOM). Two builds of one record are byte-identical, and a changed dependency is a new release, never an edit to an old one. Rebuilding a release proves we can build it again; it does not prove an upstream prebuilt binary came from its claimed source.

**Signed and scanned.** Syft writes the SBOM and Grype scans it. Findings in Debian packages are reconciled against Debian's security tracker, because a backported fix keeps a version number a version-only scanner calls vulnerable. cosign signs the release manifest, and the runner checks that signature offline against a public key it is built with, so no lease waits on Sigstore. Vulnerabilities are evaluated daily, the snapshot is refreshed weekly, and an actively exploited vulnerability with a usable fix gets a new release within 24 hours.

**Downloaded before any lease.** The image is published apart from the runner binary, with an offline bundle carrying both. A host downloads it at install or by background reconciliation, never at lease time, and keeps the current and previous images plus any image a lease or warm slot still holds. A 1 GiB image at 100 Mbit/s is about 86 seconds of transfer. That is provisioning time, and it stays there: every byte a lease needs is local before the host takes the lease.

**Admitted by descriptor.** A host becomes ready in this order:

1. the release manifest is authenticated: architecture, length, digest, EROFS features, and the runner versions it serves;
2. the image is staged in a private directory, verified, synced, and published with an atomic rename;
3. the published image is opened once without following symbolic links, checked to be a regular file of the manifest's length, and that descriptor is hashed;
4. the same descriptor is attached read-only to a loop device with `LOOP_CONFIGURE` (Linux 5.8), and the device is mounted `ro,nosuid,nodev`;
5. only then does the host report ready.

Nothing reopens the image by path after its hash is taken, so a file substituted at that path never reaches the kernel's filesystem parser. A mount found at startup is adopted only when it is EROFS, mounted `ro,nosuid,nodev`, on a loop device backed by the admitted file; a directory named after a digest proves nothing. Each lease and warm slot holds the image it runs on; past the current and previous releases, an image nothing holds is unmounted at the next admission or retention pass, and the startup sweep removes whatever an interrupted stage left. Host root is inside the trust boundary: it controls the runner, the kernel and the mounts, and no integrity mechanism protects a tenant from it.

**Tools a fleet adds.** A self-contained tool ships as a prebuilt pack at `/opt/tools/<digest>`, with its own dependency closure, an explicit `PATH` entry and the toolbox digests it works with, mounted before the warm slot is made. A tool that changes system libraries gets a complete image profile instead. Project dependencies install into `/workspace` with locked versions and tenant-scoped caches, charged to the lease's time and disk. No package is installed at lease admission. Trees that must merge use a read-only OverlayFS with several `lowerdir` entries and no `upperdir` or `workdir`. Two Debian installations assembled apart are never overlaid, because apt has not resolved the combination. Packs are built when a fleet first needs one.

**Format.** EROFS with LZ4HC stays the default, with its features pinned at build. A switch to zstd (Linux 6.10 and later), to an uncompressed image or to SquashFS must make the image at least 20% smaller with no more than a 10% p99 regression in first use, on the same package tree and workload.

**Under Firecracker.** The same EROFS bytes become the guest's read-only virtio root disk. The host's bind of `agentsfleet-runner` cannot reach a guest, so the binary and a small init travel inside the image or on a second read-only disk. The workspace disk is a separate ext4 data disk, never mounted by the host and a guest at once; ownership passes between them. A guest keeps its own page cache and decompressed pages, so bubblewrap's page-cache sharing through one host mount does not carry over, and neither does its memory density. A pack becomes another guest disk or is flattened into a profile at build.

**Not taken.** fs-verity ties integrity to the host's filesystem. composefs shares content across image versions, and a host keeps only two. Nydus and eStargz fetch on first read, which puts the network in a command's path. A delta service waits until three consecutive releases show deltas cut transferred bytes by half. apko, Nix, BuildKit, Guix, Buildroot and Yocto produce the same image and shorten no lease start.

Speed comes from what the lease path never does: no image pull, no layer unpacking. Each host keeps a few warm slots: sandboxes already started, each with its cgroup made, an empty workspace disk mounted and its executor idle, so a lease start fills the workspace and hands the slot its lease. A slot serves one lease. When that lease ends processed, its sandbox is held for the fleet (§"Workspace between leases"); any other ending destroys it. What stays on the lease path is the workspace restore, which costs what the snapshot weighs, so the snapshot is kept small (§"Workspace between leases"). A per-tenant git object cache on each host means a clone fetches only new commits. Start budgets are measured and recorded in the specs, not asserted here: host staging, sandbox readiness, the first useful command and the first browser screenshot, each separately, on cold and warm cache, at p95 and p99, with leases running concurrently. Hashing a whole image warms the host's cache, so a cold sandbox does not mean cold storage.

## A lease's sandbox today

This section records what the code on `main` builds, against the design above. The engine is built and tested but not wired in: `agentsfleet-runner run` refuses every lease (`rustd/crates/agentsfleet_runner/src/main.rs:27`), and only the kernel lane and the engine's own tests construct a `BubblewrapConfig` (`afr_sandbox/examples/kernel_lane/lane.rs:154`). Paths below are under `rustd/crates/`.

**Only a lease that needs a sandbox gets one.** A lease whose tools all run in the supervisor starts none (`afr_agent/src/engine.rs:139-141`, `afr_supervisor/src/lease_loop.rs:286-290`). `http_request` and the memory tools run in the supervisor, so a Pull Request reviewer built from them never builds a sandbox. Shell, git and file tools would need one, and until the sandbox-side tools land, a policy listing any of them refuses the lease.

**One sandbox per lease, destroyed before the report.**

```
 lease start                                                 lease end
   ├─ mkdir <state>/<lease_id>          0711, never reused        │
   ├─ sparse workspace.img 4 GiB → mke2fs ext4 → loop mount       │
   ├─ cgroup: 2 GiB, no swap, 2 cores, 512 pids, 200 MiB/s io     │
   ├─ bwrap: new user/pid/ipc/uts/net/cgroup ns, cap-drop ALL     │
   │     /           toolbox EROFS image, read-only, shared       │
   │     /workspace  the loop-mounted ext4                        │
   │     /tmp /run /dev/shm   tmpfs: RAM, charged to the cgroup   │
   ├─ child: no_new_privs → Landlock → seccomp → executor         │
   │              … the turn runs …                               ▼
   └─ destroyed before the report: cgroup.kill → kill → rm cgroup
                                   → umount → rm image → rm dir
```

There is no idle timeout and no reuse. Teardown runs at lease end, before settle (`afr_supervisor/src/lease_loop/workspace.rs:44`, `afr_sandbox/src/bubblewrap_engine/parts.rs`). Building the engine sweeps every leftover lease directory in its state directory (`afr_sandbox/src/bubblewrap_engine/sweep.rs`), so a host builds one engine per state directory: an engine per worker tears down its siblings' leases. Warm slots (`afr_sandbox/src/warm_slots.rs`) are sandboxes started ahead of a lease, one lease each; only the kernel lane uses them so far.

**Cold start is milliseconds; the waits are elsewhere.** Lease accept to executor ready, median of five, debug build, measured in the kernel lane: cold 23.6 ms, from a warm slot 0.66 ms ([M210_001](../v2/done/M210_001_P1_API_INFRA_RUST_RUNNER_SUPERVISOR_SANDBOX_FOUNDATION.md) Discovery). The lane keeps its state under `/tmp`, so a number for a workspace image on a disk is still to be measured. A run's first seconds go to the poll interval (up to 1 s), memory hydration and the first model call. The toolbox download never sits on a lease's path: a host fetches the image when it boots or reconciles.

**Where the bytes live.**

| Thing | Lives in | Bound | Lifetime | Source |
|---|---|---|---|---|
| Toolbox image | a file on disk, loop-mounted read-only once per host, shared by every sandbox | the image's size | the host's | `afr_sandbox/src/toolbox.rs` |
| Toolbox blocks in use | the kernel page cache in RAM, shared and evictable | none of its own; memory pressure evicts it | while hot | kernel |
| Workspace disk | `<state>/<lease_id>/workspace.img`, sparse | 4 GiB; a full disk answers `ENOSPC` | one lease | `afr_sandbox/src/workspace_disk.rs`, `afr_sandbox/src/engine.rs:17` |
| `/tmp`, `/run`, `/dev/shm` | tmpfs in RAM, charged to the lease's memory cgroup | the cgroup's 2 GiB | one lease | `afr_sandbox/src/bubblewrap.rs` |
| Processes | the lease's memory cgroup, no swap | 2 GiB | one lease | `afr_sandbox/src/cgroup.rs` |
| Report spool, bundle cache | the storage home, `/var/lib/agentsfleet-runner` unless `RUNNER_STORAGE_HOME` says otherwise | none found | across leases | `afr_supervisor/src/config.rs:21-24` |

**What bounds a runner host.** The image does not: its blocks sit in shared page cache, which memory pressure evicts. Memory is bounded by the lease limits: at most 2 GiB per sandbox, summed over the host's workers. The worker count defaults to 1 and caps at 64 (`afd_core/src/limits.rs:14-20`), so 128 GiB of limits at the cap, and nothing checks that sum against the host's memory. Disk holds up to 4 GiB per lease, sparse, with no headroom reserved.

**Where the code and this page differ today.**

- Every lease gets `Limits::default()` (`afr_supervisor/src/lib.rs:128`): 2 GiB of memory, 2 cores, 512 processes and a 4 GiB disk (`afr_sandbox/src/engine.rs:10-17`), not a disk sized by the lease's `disk_write_limit_mb`.
- No workspace restore exists yet, so the first model call waits for the sandbox and the bundle's files.
- No runner crate builds the per-lease network allowlist. The sandbox has loopback only, and every outbound call leaves through the supervisor's `afr_egress`.
- The production state directory is not chosen. The kernel lane keeps its state under `/tmp` (`afr_sandbox/examples/kernel_lane/lane.rs:143-145`); where `/tmp` is a tmpfs, as on the kernel lane's host, its workspace images sit in RAM. The bare-metal unit `deploy/baremetal/agentsfleet-runner.service` was written for the Zig runner: it allows writes only under `/run/agentsfleet` and `/tmp` (`:76`) and delegates `cpu memory pids` (`:71`), while the Rust host probe requires `io` as well (`afr_sandbox/src/probe.rs:37`). Wired as it stands, workspace images would land on tmpfs.

## Workspace between leases

The workspace is the fleet's, not the lease's. It survives the sandbox:

- **Bytes in R2, the record in Postgres.** Snapshots live in the bucket bundles already use, under their own prefix, named by content hash; a Postgres row points at the latest. This mirrors bundles, where Postgres holds the manifest and R2 the bytes ([Fleet Bundles](./fleet_bundles.md)). Any S3-compatible store works through the same `object_store` client with a different endpoint.
- **Only the supervisor moves bytes.** `agentsfleetd` mints short-lived presigned URLs per lease, fenced the way the memory push is; the `object_store` crate already implements presigning for S3. The sandbox never sees a URL.
- **Small snapshots.** A snapshot holds local commits as a git bundle, uncommitted edits, untracked files, artifacts and the lease transcript. Objects re-fetchable from the origin stay out.
- **Never saved.** `/run/creds` is memory-only, and every snapshot is leak-scanned before upload.
- **Read as hostile.** The supervisor treats `/workspace` as the tenant's: restore and save never follow a symbolic link out of it, and no git command the supervisor runs, including when `propose_change` reads the fleet's commits, uses the workspace's configuration, hooks or helpers. A read-only toolbox does nothing against a privileged restore that follows a link.
- **One writer.** The fleet's affinity slot and fencing token already make one lease the live holder; a stale holder's snapshot is refused.
- **Held between messages.** A lease that ends processed leaves its sandbox held: `/run/creds` emptied, the cgroup frozen through `cgroup.freeze`, and the hold keyed by fleet, workspace, limits, toolbox digest and network policy. The fleet's next lease within the idle window thaws it and continues with the files and running processes the last lease left. A key mismatch, a failed thaw, the deadline, the runner's last free worker taking a lease, shutdown, or a halted or deleted fleet destroys the hold. The report and every heartbeat tell the daemon which fleets a runner holds, and the holder claims a held fleet first while it heartbeats ([Runner Fleet](./runner_fleet.md) §"Datastore topology").
- **Retention.** The last three snapshots per fleet, a lifecycle rule on the prefix, and a purge when the fleet is deleted.
- **Continuity.** The checkpoint a report writes reaches the next lease. Today it is saved to `core.fleet_sessions` and loaded with the fleet, but nothing hands it to a lease; a chat lease carries the thread's recent turns instead (§Crates).

Artifacts a fleet saves for the user live beside the snapshots and are downloaded through a presigned URL the daemon authorises.

## Credentials

- **Model keys** stay in the supervisor.
- **Repository reads.** The supervisor clones with the lease's own one-hour, repository-scoped token from the mint verb, which reads on a read binding and writes on a write binding (`rustd/crates/afd_credential/src/credential/github/request.rs`). The token never enters the sandbox: the fetch presents it in an in-memory header, and the clone's configuration names the plain URL (`rustd/crates/afr_supervisor/src/workspace_clone.rs`).
- **Tools that need a login reuse today's mint path.** Fleets already name credentials and never hold them: a `${secrets.github.token}` placeholder resolves through `POST /v1/runners/me/credentials/mint`, and the supervisor calls that verb. What is new is only where a token lands. Today it is minted once per lease, kept in that lease's egress guard until shortly before it expires, substituted only into a request's `Authorization` header, and masked in whatever the upstream sends back. A command-line tool such as the GitHub one needs it as a file or variable, so the supervisor writes it into `/run/creds` for that lease. Each token is bound to the hosts it is for, and the lease's allowlist admits only those hosts.
- **A line said to the thread, and a schedule's message, are masked twice.** The `message`, `cron_add`, `cron_update` and `schedule` tools mask every token the lease minted before the text leaves the runner; `agentsfleetd` masks the fleet's declared static secrets with the same `afr_secrets::Scrub` before it posts a line through the outbound worker's Slack poster or stores a schedule. The runner never holds the channel credential.
- **A placeholder-swap proxy can replace the file later without fleets noticing.** The mint path stays the same, and the supervisor writes a placeholder into `/run/creds` instead of the token. Every sandbox connection already leaves through the host-side end of the lease's network namespace, and the toolbox's trust store has an empty slot for a proxy's certificate. Adding the proxy reverses the no-interception rule in [Runner Fleet](./runner_fleet.md) §Traps, and that reversal is recorded when it ships.

## Coding engines

- **Codex is first-class, split across the wall.** `codex app-server` runs in the supervisor and holds the user's ChatGPT subscription or OpenAI key. `codex exec-server` runs in the sandbox and only executes, so the subscription never enters the sandbox. App-server adds a remote exec server as an "environment" (`~/Projects/oss/rs/codex/codex-rs/exec-server/README.md`). Whether every tool call routes to that environment is proven before the engine ships. Codex's JSON Lines events (`codex exec --json`, `~/Projects/oss/rs/codex/codex-rs/exec/src/cli.rs`) map onto the same activity frames as our own loop.
- **Claude Code** runs in the sandbox and reaches its model through a relay the supervisor serves. Its `ANTHROPIC_BASE_URL` names a loopback endpoint, and the supervisor adds the Anthropic API key outside the sandbox, so the key never enters it. Subscription use stays off until its terms are cleared.
- Both run with their own sandbox off. Ours is the boundary, and nested user namespaces are disabled anyway.

## Repository writes

The fleet uses real git inside the sandbox: branches, several commits, the normal flow. `propose_change` hands the commits to the supervisor, which:

1. checks them against the rules `agentsfleetd` compiled for the lease — the pinned branch, no change under `.github/workflows`, size caps;
2. mints a write token for that one call and pushes;
3. opens a draft Pull Request.

When a fleet requires approval, the push runs in the continuation lease after someone approves. The daemon compiles the rules (`rustd/crates/afd_gate/src/policy/egress/write.rs`), and the supervisor enforces them outside the sandbox. The loop renders the same rules into the prompt as a trusted repair context — the one repository, the daemon-named repair branch, the trusted base — because a write-bound fleet's instructions require that input before writing and nothing else supplies it. A write token inside the sandbox would be readable by every program there, including a dependency's install hook. A repository-scoped write token "can force-push to `main` as easily as it can open a draft Pull Request", in the words of the rules' own module documentation.

## Why Rust

- **Memory safety is checked by the compiler.** The Zig runner relies on rules a reviewer enforces by hand: one owner per resource, init and deinit pairing, idempotent cleanup, draining before deinit (`docs/greptile-learnings/RULES.md`, OWN, ZIG, DEINIT, DIDEM, DRAIN). It once needed a memory-leak lane of its own.
- **The language is stable.** Zig is pre-1.0: the rulebook carries a rule for the Zig 0.15 ArrayList change (ZAL), and the NullClaw fork patches around Zig 0.16's process I/O.
- **One wire.** The runner speaks `afd_wire`, the daemon's own types, so a mismatch between the two sides fails to compile.
- **The next work already exists in Rust.** Pseudo-terminals, patch application, the bubblewrap helper, Landlock and seccomp bindings, S3 presigning and Firecracker itself.

The costs are the rewrite, slower compiles, async complexity and larger binaries. Speed is not a reason: both compile to native code.

## What comes from where

| Piece | Source | Form |
|---|---|---|
| Loop shape: turns, tool router, every call ends once, typed events | Codex | Design |
| Process control: groups, TERM then KILL, timeouts, output caps, pseudo-terminals | Codex `utils/pty` and its exec module | Copy the pseudo-terminal crate; port the design |
| `apply_patch` parser, UTF-8-safe cuts, process hardening | Codex | Copy at a pinned commit, with its NOTICE |
| bubblewrap-then-seccomp helper | Codex `linux-sandbox` | Adapt, adding the flags and filters above |
| Anthropic streaming and caching, OpenAI-compatible chat, token usage split | ZeroClaw `zeroclaw-providers` | Mine |
| Retry honouring `Retry-After`, failover, circuit breaker | IronClaw `ironclaw_llm` | Mine |
| Per-fleet grants: effects, mounts, network, secrets, enforced ceilings | IronClaw `ironclaw_host_api` | Design; auto-approve off |
| Leak scanning on tool output, model input and egress | IronClaw `ironclaw_safety` | Port |
| Coding-engine runners | ZeroClaw `zeroclaw-tools` (Claude Code, Codex) | Mine |
| Per-lease transcript as JSON Lines | Codex rollout | Design; saved with the snapshot |
| Docker lanes, WASM tools, `codex-core`, a TLS-intercepting egress proxy | — | Not taken |

`codex-core` speaks only OpenAI's Responses API, so it cannot drive fleets on Anthropic keys, and its public surface changes weekly. ZeroClaw and IronClaw are whole personal-assistant products whose loops are welded to their channels. The loop and the provider layer are ours.

## Decisions

| Date | Decision | Source |
|---|---|---|
| Oct 02, 2026 | A fresh Rust runner, not a port of the Zig runner's structure | Indy: "The port is a fresh port, since we always have the last binary with us and running." and "ensure we dont hoadwink and follow the runner zig as opposed to a fresh plate on focussed on an outage" |
| Oct 02, 2026 | Supersedes "the src/runner will be on zig no action needed there" (Sep 02, M187_001) and "stays Zig permanently" (M181_006) | Indy, the quotes above |
| Oct 02, 2026 | Fleets clone and work in a real repository; supersedes M157_001's "No working tree, no git binary" | Indy: "a fleet will have to clone the copy of code and so on and mug around with it" |
| Oct 02, 2026 | No Docker on the lease path; start time is a requirement | Indy: "preferrably avoiding Docker since its layer takes a while to load... I will need faster results as well" |
| Oct 02, 2026 | Workspaces and artifacts carried in R2 or an S3-compatible store | Indy: "we could have the S3 compatible R2 pointing to code and artifacts if need be for the user" |
| Oct 02, 2026 | Subscriptions and API keys; Codex first-class | Indy: "Subscripts and API keys, but support Codex as first class citizen, anthropic seems to have issues on terms and requests via API keys" |
| Oct 02, 2026 | Multi-tenant on bare metal or VMs from the first release; Firecracker is an additional engine | Indy: "everyone can run isolated multiple tenants with fleets must use a baremetal host or have a runner run from a VM from day 1. To me firecracker is an additional approach to think on isolation." |
| Oct 02, 2026 | Scoped short-lived tokens now; the placeholder-swap proxy later | Indy: "I would go for 1, with the focus on move to 2 later" |
| Oct 02, 2026 | The supervisor pushes repository writes | Indy chose "Supervisor pushes (Recommended)" |
| Oct 02, 2026 | Code-running leases from different tenants may share a host; hardening and kernel patching are the boundary until Firecracker | Indy chose "No, share freely" when asked whether a host should refuse a second tenant's code-running lease |
| Oct 02, 2026 | Firecracker is the production engine for code-running leases; bubblewrap ships first and stays for development, CI and hosts without `/dev/kvm` | Indy: "i think we must shoot for firecracker then", then "Yes bubblewrap first, and firecracker next." Firecracker is installed on production hosts later; every runner reports whether `/dev/kvm` exists |
| Oct 02, 2026 | The Rust runner is independent: no second copy of the wire, nothing in its code, comments or tests refers to the Zig runner, and it follows `rustd` principles | Indy: "A second copy of the wire will not be existing, none of the rust code will point to the zig." and "the rust code is independent and follows our current rustd/ principles" |
| Oct 02, 2026 | The runner carries every published tool; the loop stays in the supervisor and routes the code-running tools into the sandbox | Indy: "i need all the tools … the sandbox isnt just a sandbox but a harness that decide to operate like codex so that is critical to realize the fleets i plan to use"; chose "Supervisor loop, sandbox tools" |
| Oct 02, 2026 | One binary: the in-sandbox entry is `agentsfleet-runner sandbox`, a sub-mode, not a second artifact | Indy: "why do we need two ? agentsfleet-runner, agentsfleet-executor … i thought its just one binary?" then "agentsfleet-runner"; chose `sandbox` for the sub-mode |
| Oct 02, 2026 | `cron_*` and `schedule` become fleet tools through a runner verb onto the daemon's schedule plane; supersedes "Scheduled wakes are not a child tool" in [Capabilities](./capabilities.md) §"2. The platform tools the fleet can call" | Indy chose "Yes, runner verb onto daemon schedules" |
| Oct 03, 2026 | Keep Debian, mmdebstrap and EROFS with LZ4HC. The toolbox is built from pinned snapshots, signed, scanned, downloaded before any lease and admitted by descriptor, which replaces hashing the loop device after the mount. Fleet tools arrive as prebuilt packs or image profiles, never as packages installed at admission (§Toolbox) | Tarzy's review of the toolbox design; Indy chose "Approve as classified (Recommended)" |
| Oct 03, 2026 | Not taken: fs-verity, composefs, Nydus and eStargz, a delta service, and the apko, Nix, BuildKit, Guix, Buildroot and Yocto builders | The same review and choice |
| Oct 03, 2026 | Claude Code reaches its model through a supervisor relay, so no model key enters a sandbox; supersedes "Claude Code runs in the sandbox on an Anthropic API key" | Indy: "1 - if our current design spells that way". Four lines on this page already kept model keys out of the sandbox |
| Oct 03, 2026 | Revoking a compromised toolbox is deferred; the toolbox works first, and rollback stays the previous release | Indy: "I dont want to focus on revocation of a compromised toolbox, first is to get the toolbox working" |
| Oct 03, 2026 | Named model providers resolve through a local, versioned registry of name, wire and base URL, embedded as data the way IronClaw embeds its `providers.json`; unknown names are refused at admission; a `custom:<url>` provider is `https` only and the transport follows no redirect. No LLM framework's provider catalogue decides admission | Indy: "How does ironclaw does this? Can we review and steal from here", with Tarzy's recommendation: "Keep refusal for unknown names, but support every Zig named provider whose wire is already implemented" |
| Oct 04, 2026 | The browser tools run only under the Firecracker engine. The bubblewrap engine answers each with a code and never passes `--no-sandbox`, and Chromium enters the toolbox with the Firecracker engine | Spike S1: Chromium's sandbox cannot start inside bubblewrap's. Indy chose "Refuse browser tools, defer §5 (Recommended)" |
| Oct 05, 2026 | `/tmp` moves onto the workspace disk, the lease cgroup splits into a `sandbox` and a `tenant` leaf, the workspace disk attaches with direct I/O, and a worker leases only with a state-disk reserve (M211_003) | Spike S6: filling `/tmp` ended in the out-of-memory killer taking `bwrap` every time. Indy: "Fix all fixes in this PR" |
| Oct 05, 2026 | A processed lease's sandbox is held, frozen, for its fleet's next lease, and its holder claims that fleet first; supersedes "one lease, then destroyed" (M211_004) | Indy: "I want to provide a seamless faster approach on chat?" (Oct 04), then "Fix all fixes in this PR, may be the 215_001 must be renamed to the next sequence in 211_00X and fix all that is needed" |
| Oct 05, 2026 | A chat lease carries the thread's recent turns, sent ahead of the message; the stable prefix is cached at the five-minute default, and the trusted repair context stays in the system prompt (M211_005) | Indy chose "Add M211_005, cached (recommended)". The one-hour lifetime waits because the daemon bills a cache write at the input rate |
| Oct 06, 2026 | `file_read` pages by line and is cut to the output budget, instead of reading back up to 8 MiB | Indy chose "Budget + paging (Recommended)" |
| Oct 06, 2026 | Offering `git` offers what `shell` does inside the sandbox; the sandbox is the boundary | Indy chose "git counts as shell (Rec.)" |
| Oct 06, 2026 | Every sandbox tool gets the bound repositories checked out, not only the process tools | Indy chose "Clone for file tools (Rec.)" |
| Oct 06, 2026 | The host mirror's hygiene is deferred: refreshing the default branch, dropping deleted branches, size caps and eviction, purging force-pushed objects, redirects with a token header, and transient open errors | Indy chose "Defer all" |
| Oct 06, 2026 | The executor forwards every byte of a process's output live, behind a bounded queue, instead of a 512 KiB live head and a tail held to the end; the supervisor's side keeps what a caller has not read as a 512 KiB head and tail and counts the middle, as Codex's client does, so a session that printed past 512 KiB is still heard | Indy chose "Fix in this PR (recommended)" |
| Oct 06, 2026 | Deferred: the toolbox trust hardening (the fixture key as trust root, a rewrite after hashing, an owner and mode check, a symlinked mount point, manifest expiry, the registry lock held over the hash), image support decided per model, zeroizing token header copies, the lchown ordering warm slots would break, and `file_read_hashed`'s 8 MiB read | Indy chose "Defer all (recommended)" |
| Oct 06, 2026 | The long-session fix keeps its push design for now: a flood's decode cost and the unread store sit on the supervisor host, and the store bounds bytes, not chunks; the pull model (`process/read`, Codex's exec-server shape) waits on testing the user experience | Indy: "I think ignore this, i want test and find the user experience" |
| Oct 06, 2026 | Deferred: a byte offset on `fs/read` for paging, and the checkout for a lease whose only sandbox tools refuse | Indy chose "B: defer both with my quote" |
