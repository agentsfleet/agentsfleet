# Runner runtime: a trusted supervisor, a sandbox per lease, a workspace that outlives the lease

> Scope: how `agentsfleet-runner` runs one lease, where its agent loop runs and what its sandbox is made of. The page also covers how a fleet's code and files survive from one lease to the next. It covers how credentials and repository writes cross the sandbox wall too.

> `agentsfleet-runner` is the one runner binary, with three entries: `run`, `probe` and `sandbox` (`rustd/crates/runner/main/src/main.rs`). The [runner guide](runner_protocol.md) owns connection, renewal, fencing and report recovery rules.

## Facts

| Fact | Value |
|---|---|
| Language | Rust, in the `rustd` workspace beside `agentsfleetd`, under the same principles: one error type per crate through `af_core::error_shell!`, one source per constant, pure logic apart from I/O. The wire is `af_wire` and nothing else. Native session messages reject unknown variants and fields |
| Process model | A trusted **supervisor** runs the lease loop and the agent loop; a per-lease **sandbox** runs tool calls and nothing else |
| Hosts | Bare metal or a virtual machine (VM), multi-tenant from the first release |
| Sandbox engine | bubblewrap (Landlock, seccomp, cgroup v2, a per-lease network namespace except under `allow_all`) is the one engine built, `BubblewrapEngine`, and stays as the engine for development, Continuous Integration (CI) and hosts without `/dev/kvm`. Firecracker microVMs, for code-running leases from many tenants on hosts that expose `/dev/kvm`, are the target and are not built |
| Sandbox network | The runner's assigned `network_policy`: `allow_all` shares the host's network namespace, `deny_all_egress` gives loopback only, and `allow_list_egress` joins the sandbox to the host by one virtual ethernet (veth) pair whose host-side nf_tables rules admit only the lease's allowlist ([Runner Fleet](./runner_fleet.md) §Egress model) |
| Toolbox | A signed, read-only Enhanced Read-Only File System (EROFS) root filesystem built once per release from pinned Debian snapshots, downloaded before any lease and admitted by descriptor. No container image is pulled or unpacked on the lease path |
| Workspace disk | Per sandbox, a writable block image sized to the lease's disk limit, attached with direct I/O, holding both `/workspace` and `/tmp`, deleted when its sandbox is destroyed |
| Warm slot | A sandbox already started before any lease arrives: cgroup made, workspace disk mounted, executor idle. It serves one lease. Built but unwired: only the kernel lane and tests start one (`afr_sandbox/src/warm_slots/mod.rs`) |
| Held sandbox | The sandbox of a lease that ended processed, frozen through `cgroup.freeze` and kept for its fleet's next lease for an idle window of ten minutes (`af_core::timing::SANDBOX_HOLD_IDLE`). Every other lease's sandbox is destroyed when the lease ends |
| Workspace | Target, not built: per fleet, restored from and saved to R2 (or any S3-compatible store, such as a self-hosted RustFS), with only the supervisor moving bytes. No restore or save exists yet (§"A lease's sandbox today") |
| Model keys | Supervisor only; never inside a sandbox |
| Coding engines | Codex first-class, split across the wall; Claude Code in the sandbox, its model calls through a supervisor relay |
| Repository writes | Target: local commits inside the sandbox; the supervisor pushes under rules `agentsfleetd` compiles; draft Pull Request only. Not built: `propose_change` exists only as the `git` tool's refusal text (`afr_tools/src/sandbox/git/mod.rs`) |

## Lease execution kinds

A tagged native offer carries fleet runtime or private setup work. Fleet runs use their admitted tools and optional sandbox. Setup turns use only daemon-scoped model calls and the fixed proposal tool.

Setup leases carry no runtime credentials, repository checkout or tenant tools. The [runner guide](runner_protocol.md) owns connection authority, capacity and recovery. [API resources](api_resources.md#scoped-model-invocation) owns dispatch authorization.

### Proposal correction and failure

Rejected proposal arguments and invalid edits return tool feedback to the model (`afr_supervisor/src/authoring/proposal.rs`). They do not apply a candidate or advance the activity sequence. A correction spends another model call and its measured output from the same attempt budget.

The turn stops at eight calls or the admitted output allowance (`afr_supervisor/src/authoring/turn.rs`). Exhausted corrections report `failed` with `provider_failed`, retain known usage, and supply no successful answer.

When every proposal in the response is accepted, the turn returns the review-ready answer. Save remains a separate human action.

Unknown provider usage ends the turn without another dispatch. Renewal runs every ten seconds, retries retryable failures after 250 milliseconds, and stops at the live deadline (`afr_supervisor/src/authoring/renew.rs`). The attempt also has a 300-second maximum.

## Private content and report retention

`DELETE` on a draft returns 204 after the daemon's transaction fences every attempt and removes private text and candidates. The same transaction writes a content-free tombstone and makes further private reads return 404. It does not delete the shared fleet or billing records.

Remove the private transcript from server-rendered and browser caches, and close its stream. The target stores no unsent text in browser local storage. In-memory text is scoped to account, workspace and subject, and cleared on logout, revocation or delete.

| Copy | Owner / removal | Disclosed residual bound |
|---|---|---|
| Draft/message/candidate rows and private replay bodies | Daemon DELETE/expiry, content-free receipts remain | Gone from ordinary reads at 204; database backups follow infrastructure retention, not an instant physical erase. |
| Private progress channel / buffered deltas | Daemon drops draft-specific buffer and closes subscribers | No new private read after 204; buffered or already rendered client text cannot be recalled. |
| Runner memory / in-flight request | Delete generation at renewal, report, activity; runner cancels the turn and drops its local buffers | Online runner notices at next ten-second renewal; disconnected runner lease expires within 30 seconds. Upstream completion may outlive local abort. |
| Pending authoring report | Worker memory retains the current report; no report files are written (`afr_supervisor/src/authoring/mod.rs`) | After 24 hours, delivery zeroes private answer text and retries content-free accounting. Process loss discards pending memory. |
| Logs / traces / metrics / crash diagnostics | Do not capture prompts, answers, proposal content, tokens, or secret values | No private text is intentionally written; test redaction and disabling automatic body/crash capture. |
| Provider requests | Provider/service account policy, disclosed on delete | Local abort cannot establish upstream erasure. Provider-specific retention is shown, never claimed zero. |
| Published revision / applied fleet setup | Deliberately shared definition, independent authority | Draft deletion does not unpublish or delete shared setup; disclose this before Save/publication. |
| Backups / legal accounting | Infrastructure backup schedule; billing domain content-free ledger | Follow published retention; no invented physical-purge deadline. |

Accounting replay keeps its original report identity and can settle against daemon receipts after draft deletion.
Changed accounting content conflicts.
The [runner guide](runner_protocol.md#results-and-billing) owns delivery and shutdown limits.

An offline runner does not prevent DELETE from returning 204 at the server text boundary. A tombstone rejects stale generations before accepting text. Content-free settlement can acknowledge known usage without recreating messages.

Closed private history expires 30 days after close, only when no active work exists.

## The job it is built for

The design centre is a fleet repairing a production outage, not a chat assistant:

```
alert fires
 → a repair fleet gets a lease on a runner (a VM or a bare-metal host)
 → the runner restores the fleet's last workspace (code, notes, artifacts)
 → the fleet reads logs, runs command-line tools, writes Python, edits code, runs tests,
   or hands the coding to Codex on the user's own subscription
 → it proposes a fix → the supervisor pushes a pinned branch and opens a draft Pull Request
 → the runner holds the sandbox frozen for ten minutes, then destroys it
 → a follow-up ("also fix the retry") within the window thaws that same sandbox on the same runner
```

A lease's inputs are the trigger payload, input files (logs, traces, exports) and the repository to fix. Its other inputs are reference repositories (read-only), the toolbox's command-line tools and the fleet's durable memory.

Its outputs are the answer, a proposed change, files the user can download and memory updates. They also include the workspace snapshot the next lease starts from.

## Process model

```
agentsfleetd ◄── native runner session (af_wire) and retained HTTP operations
   ▲
agentsfleet-runner (one binary; a VM or a bare-metal host)
 ┌ supervisor — trusted, outside every sandbox ─────────────────────────┐
 │ lease loop · renew · pending reports in memory · fleet memory        │
 │ agent loop + model providers            (model keys live only here)  │
 │ tool catalog · router · leak scan · events → activity frames         │
 │ workspace restore and save · git clone · credential minting · push   │
 └───────────────┬──────────────────────────────────────────────────────┘
                 │ one executor connection per lease (a Unix socket)
 ┌ sandbox — one per lease ─────────────────────────────────────────────┐
 │ /            toolbox, read-only                                      │
 │ /workspace   restored, writable, disk quota                          │
 │ executor: processes on pseudo-terminals, files, apply_patch, Chromium│
 │ network per assigned policy · cgroups · seccomp · no caps            │
 └──────────────────────────────────────────────────────────────────────┘
```

Workspace restore and save, and the supervisor's push, are drawn as the target; neither is built (the Workspace and Repository writes rows above).

**The agent loop runs outside the sandbox.** No model key ever enters a sandbox, so a prompt-injected command cannot read one.

The sandbox is a plain in-sandbox server, so bubblewrap and a microVM sit behind one interface. A model call needs nothing from the sandbox, so once workspace restore exists (not built yet), a call will start during the restore.

**The in-sandbox server is small and ours.** One process per lease serves spawn, write, read and kill for processes on pseudo-terminals. It also serves the file calls (read, write, append, delete and list) that the supervisor's file tools and `apply_patch` edit through.

Its methods mirror Codex's `exec-server` (`~/Projects/oss/rs/codex/codex-rs/exec-server/README.md`), so the Codex engine and our loop drive the same shapes.

**Every call ends exactly once.** A run can end by answer, crash, kill or timeout. Whatever the reason, the supervisor closes each call still open as `interrupted`, live and in the trace ([Runner Fleet](./runner_fleet.md) §Live activity).

A session's process outlives the call that started it. So the run's end also kills every session still open, and none reaches whatever the sandbox serves next.

## One lease, end to end

`agentsfleet-runner run` probes the host, admits the staged toolbox and builds one bubblewrap engine (`rustd/crates/runner/main/src/main.rs`).
The native session, heartbeat policy and worker pool run together until shutdown (`afr_supervisor/src/lib.rs`).

Idle workers wait for pushed offers while the current assignment permits work.
Each worker keeps its lease and pending report until acknowledgement or a terminal refusal (`afr_supervisor/src/worker_pool/mod.rs`).

```
 native offer
   │
   ├─ admit       the policy's tools and provider; an unhosted one refuses the lease here
   ├─ turn        one run per fleet on this runner; a second lease for it waits
   ├─ memory      the fleet's memory window, carried on the lease; read through
   │              agentsfleetd when the lease has no window
   ├─ admitted    only when a tool runs in a sandbox: size, egress and checkout plan
   ├─ run         the agent loop in the supervisor; tool calls cross the executor socket
   │  └─ build    beside it: the fleet's held sandbox, or a fresh one on the assigned
   │              network, bound repositories checked out; a sandbox call waits for it
   ├─ keep        a processed lease's sandbox is frozen and held; any other is killed
   ├─ settle      HTTP tool records, then a native report with memory and timing segments
   │              retained in this worker until acknowledgement or terminal refusal
   └─ release     a killed sandbox's disk, beside the live tail's drain of at most 5 s
        renewal ticks alongside from the lease's start until the report is answered
```

**One run per fleet per runner.** `FleetTurns` is a coordinator task that owns which fleets are busy (`afr_supervisor/src/turns/mod.rs`).

A worker waits for its fleet's turn without a lock, and dropping the turn frees the fleet. The daemon's affinity slot already admits one live lease per fleet. It does so across runners ([Runner Fleet](./runner_fleet.md) §"Memory continuity: durable fleet memory rides the trusted plane").

**A refusal ends the lease with a report.** It ends as `startup_posture` before the turn runs (`afr_supervisor/src/lease_loop/refusal.rs`). Refusals cover an unhosted tool or provider, unreadable memory, a size past the wire's bounds, unresolvable egress and a binding no checkout serves.

A sandbox that cannot be built leaves the turn going, and its sandbox-side calls answer `sandbox_unavailable`. Nothing runs unsandboxed in a sandbox's place.

**The report carries memory and timing.** The runner sends the current report with its memory deltas and timing segments (`afr_supervisor/src/lease_loop/settle.rs`).
The daemon captures memory under the lease fence before committing settlement.

**Lease frames and memory deltas zero on drop.** Native offer frames retain zeroing allocation owners through dispatch.
`CapturedMemory` overwrites the strings it owns when dropped (`afr_supervisor/src/memory/mod.rs`).

**The report is the record.** The worker retries the same report from memory after connection loss (`afr_supervisor/src/report/delivery/mod.rs`).
A terminal refusal ends delivery; process loss can discard results or usage the daemon has not accepted.
See [results and billing](runner_protocol.md#results-and-billing) for saved acknowledgements and shutdown limits.

A renewal the daemon refuses, or a runner told to stop, cuts the run. The cut keeps the tokens and memory the run handed back, and reports why the run ended (`afr_supervisor/src/lease_loop/mod.rs`).

**Usage rides renewals and the report, at most 512 lines a body.** Both carry provider requests the daemon has not yet recorded. Each request is one line, oldest first, capped at 512 (`af_wire::usage::LINES_MAX`, taken in `afr_agent/src/engine/mod.rs`).

The runner caps each batch at the wire limit.
Each renewal answer acknowledges recorded lines through `recorded_through`; the report carries the remaining lines.

[Billing §3](./billing_and_provider_keys.md#3-one-debit-point-usage-settled-through-the-biller) has the full table, under **Request lines**.

**A backlog past one body is renewed away before the report.** Failed renewals or a request burst can leave over 512 unrecorded lines.

Before its report, the settle renews with the oldest 512 until one body holds the rest (`flush` in `afr_supervisor/src/renew/mod.rs`). Each answer's `recorded_through` forgets the lines that renewal recorded.

A renewal that fails after its retries, or records nothing, stops the flush. The report then carries the oldest lines left, and the newest are logged `usage_lines_unflushed`, never billed. A line sent twice is billed once, because the daemon keys lines on `(work, seq)`.

## Crates

The runner's crates sit in `rustd/crates/runner/`, beside the shared `foundation/`, `protocol/` and `harness/` layers. `af_wire` is the one wire `agentsfleetd` and the runner both speak.

A crate ships with its own layer or a lower one, and `runner/` and `daemon/` never with each other (`rustd/Cargo.toml`). Shared crates take the `af_` prefix; a layer's binary sits in `main/`.

The supervisor implements credential minting over the control plane for `af_egress`, and uses `af_crypto` for encrypted private reports. No runner crate links a datastore crate or `afd_api_wire`.

`guffbolt` is a library that knows no lease, fleet or task. `drive` takes input before every model call and once more after an answer. A message that arrives while the model writes its answer runs one more turn.

The embedder implements `Conversation`: `afr_agent`'s `Harness` is the input, the window, the model call and the tools. The turn, tool results and failure are the embedder's own types, read through `ModelTurn`. So `guffbolt` depends on `tokio` and `tracing` only, and links no model client or secret store.

What a run of one kind adds is an `Extension`: a durable task is `afr_agent`'s `Task`, and every other loop installs `Plain`. A message's media parts reach the model through a `MediaReader`; until one reads a kind, `Unread` names the part to the model.

`afd_api_wire` owns the daemon's tenant, operator and ingress shapes. Changes there do not rebuild the runner protocol.

One binary supplies both the host supervisor and its in-sandbox `sandbox` entry. The sandbox entry builds only the executor server, and each sandbox binds the binary read-only. The planned Firecracker guest receives it inside the root image or a second read-only disk (§Toolbox).

A lease carries the selected model's saved inference settings: API format, base URL, optional dialect and maximum output tokens. `af_inference` validates them and builds the codec, and missing settings refuse admission. [Billing and provider keys](./billing_and_provider_keys.md#9-provider-routing-saved-model-settings) owns configuration and credential matching.

`rig-core` and `rig-http` speak the three supported API formats, at the version `rustd/Cargo.toml` pins. The transport under rig is the runner's: one HTTP client per runner that follows no redirect. So a turn and its key reach the admitted host and no other.

A bounded retry honours `Retry-After`. A turn that fails before anything visible went out is opened again under the same bound.

The reasoning a provider signs goes back with the turn that made it. A call from a turn cut at the output limit is answered `output_limit_reached` and never runs.

A call whose arguments are not JSON reaches its tool as the text the model wrote. The tool refuses it with a reason.

rig traces every request and reply whole, before the scrub. So the shared log filter (`af_inference::log_filter`), installed by daemon and runner, holds rig to its warnings at any level. It also turns off the one warning that repeats a provider's error message.

The model key rides the one header its wire names. Everything the model is sent passes the secret scrub first: the instructions, the event's message and the model's own earlier turns.

A chat lease also carries the fleet's thread before its event: up to eight finished turns within 64 KiB. The rows are the ones the dashboard thread shows.

The loop sends each turn as a user and an assistant turn ahead of the new message, through the same scrub. Webhook, cron and continuation leases carry none.

The trusted repair context stays in the system prompt, and no turn enters it. A conversation repeats its prefix lease after lease. So the Messages wire marks the tools and the system prompt for caching at the five-minute default.

The Responses wire sends the fleet id as `prompt_cache_key`.

A turn reports its cache writes apart from its input, split by the lifetime the provider names (`af_providers::usage`). Messages sends five minutes and one hour; Responses and Chat send a write with no lifetime. The daemon prices each class at its tier's write rate ([billing §11](./billing_and_provider_keys.md#11-one-billing-lifecycle-behind-a-biller-trait)).

The wire still asks for the five-minute default, and asking for one hour is not decided. Spans and their export are in [Observability](./observability.md) §"`agentsfleet-runner` — a collector of its own".

## Tool catalog

The runner is the harness, in Codex's shape. The catalog holds every tool the published tools page names, each with the runtime it runs in. The lease's `ExecutionPolicy.tools` selects which of them the model is offered.

The model picks by function calling, and the router runs the handler where its runtime says. A policy naming a tool the catalog does not host refuses the lease loudly. A quietly different tool set would not be the setup the fleet's author wrote.

A model call to a name outside the policy is a tool error the model sees, and the run continues. The scheduling tools write rows in `fleet.schedules` through the daemon; nothing in the runner sleeps or ticks.

The scheduling tools and `message` reach `agentsfleetd` through `afr_tools::LeaseVerbs`, which the supervisor implements over its control plane. The supervisor adds the lease's id and fencing token to each call, one attempt each. A tool never holds the token, and the model decides whether to try again; no retry loop does.

| Tool | Runtime | Needs |
|---|---|---|
| `http_request`, `web_fetch`, `pushover` | supervisor | the network policy, the origin rules, placeholders in `Authorization` only |
| `web_search` | the provider, as a hosted tool spec | a provider that offers one; a tool error with a code otherwise |
| `memory_store`, `memory_recall`, `memory_list`, `memory_forget` | supervisor | the loaded store and the fenced push |
| `update_plan` | supervisor | nothing |
| `message` | supervisor, through a runner verb, to the event's thread | the messages verb |
| `schedule`, `cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs` | supervisor, through a runner verb onto the daemon's plane that QStash fires | the verb; QStash keeps the clock and the runner owns no timer |
| `delegate`, `spawn`, `wait_agent`, `send_input`, `list_agents`, `interrupt_agent` | the loop itself, beside the turn: a child is a nested loop sharing the lease's sandbox, memory, key, budget and call counter, polled inside the run so it ends with it | Codex's `spawn_agent`, `wait_agent` and `send_input` shape; depth 2, four running and sixteen started per run |
| `shell`, `exec_command`, `write_stdin` | sandbox, through the executor | `shell`: one process on pipes, its group killed at its timeout; `exec_command` and `write_stdin`: a session per process, on a pseudo-terminal when asked, at most 64 per lease. Every byte of output is forwarded as it arrives, behind a bounded queue a flooding process waits on, and drained for a bounded grace once the leader ends; what a call has not read is kept as a 512 KiB head and tail, the middle counted, and each call drains it. A call says when the output was still open at the end, a gap's marker sits where it fell, a write to a process gone by then says it was not delivered and answers its ending, or runs on until it lands, and a write the process would not take says so and runs on; the executor's own sentence stays on the host, and whatever an executor failure says to a model is rendered to at most 4 KiB (`WIRE_MESSAGE_MAX_BYTES`), a refusal's message and a result that would not decode alike, since the socket's other end is not trusted with the length of what a model reads |
| `git` | sandbox, on a clone the supervisor made on the host side | the token it was fetched with stays in the supervisor; the push is `propose_change` |
| `file_read`, `file_read_hashed`, `file_write`, `file_append`, `file_delete`, `file_edit`, `file_edit_hashed`, `apply_patch` | sandbox, through the executor's file calls | `/workspace`; `file_read` pages by line, `offset` and `limit`, and is cut to the same 10,000-token budget as a command's output |
| `image` | supervisor reads the file through the executor and attaches it to the next model turn | a provider that takes images |
| `browser`, `browser_open`, `screenshot` | sandbox under the Firecracker engine: Chromium from the toolbox, driven over the Chrome DevTools Protocol (CDP) through the process's pipes; the bubblewrap engine answers each with a code | the Firecracker engine; Chromium in the toolbox; the sandbox allowlist for any host beyond loopback |

Shell and Git commands bound their ending wait in `afr_tools/src/sandbox/oneshot/mod.rs`.
The allowance combines native kill/drain grace with a named reply slack; compile-time checks relate it to lease and engine bounds.
Spawn requests retain the native client's separate 30-second reply deadline.

A missing ending returns `interrupted`, retains available output and leaves the command's outcome unknown.
Expiry invalidates the existing executor link; pending and subsequent calls refuse without waiting on that peer again.
The tool performs no automatic repeat and does not claim remote death.

The ending wait consumes Tokio cooperative budget when output notifications are ready.
Output reads take one finite buffer snapshot, including its ending and channel state, under the existing lock.
Later arrivals cannot extend that read; zero-byte frames consume no buffer entries.

The warning `process_ending_deadline_exceeded` records lease/process identities and both timeout durations, without command text.
The existing `agentsfleet_runner_tool_call_duration_seconds` family records this returned tool failure and its elapsed time through the agent ledger.

A lease whose tools are all supervisor-side starts no sandbox. A lease offered any sandbox tool gets its bound repositories checked out before its turn. So a lease of file tools alone reads the repository, never an empty workspace.

git runs programs of its own accord, through hooks, `!` aliases and commands named in configuration. So a lease offered `git` can run, inside the same sandbox, what a lease offered `shell` can. The `git` tool refuses remote subcommands as an answer to the model; the sandbox is the boundary.

## Sandbox engines

Fleets on shared hosts run their own programs: build scripts, test suites, package installs and Python the model writes. So the sandbox is the only thing between two tenants' processes and one kernel. Its layers are:

- bubblewrap: a fresh user, PID, IPC, UTS and mount namespace per lease. A fresh network namespace too, unless the policy is `allow_all`. It runs with `--cap-drop ALL`, `--disable-userns`, `--clearenv`, `--die-with-parent` and `--new-session`.

  `--new-session` detaches the sandbox from any terminal, which closes terminal-input injection (`TIOCSTI`). The entry runs as uid and gid 1000 in `/workspace`. It is bound read-only at `/opt/agentsfleet/agentsfleet-runner` and started by that path, never through `$PATH`.

  The entry is told the tenant leaf's descriptors and its end of the executor's socket pair. It also gets the log level, when one is set (`afr_sandbox/src/bubblewrap/mod.rs`).

- `no_new_privs`, then Landlock, then seccomp. The seccomp filter refuses 12 calls: the three `io_uring` calls, `ptrace`, `process_vm_readv`/`writev`, `unshare`, `bpf`, `keyctl`, `add_key`, `request_key` and `perf_event_open` (`afr_sandbox/src/harden/linux.rs`). Codex's own filter leaves `unshare`, `bpf`, `keyctl` and `perf_event_open` open.

- cgroup v2 limits on memory, processor, process count and I/O, with whole-tree kill. The lease cgroup has two leaves. `sandbox` holds bubblewrap and the executor, and `tenant` holds every process the fleet starts.

  bubblewrap is born in the `sandbox` leaf: its spawn writes itself into `cgroup.procs` before `exec` (`afr_sandbox/src/bubblewrap_engine/parts/mod.rs`). Each tenant process moves itself in before `exec`. It writes the tenant leaf's `cgroup.procs`, which the engine opened as the runner and handed in.

  The kernel checks that move against the opener's credentials and cgroup namespace. So no cgroup file system is mounted inside, and a move that fails refuses the spawn.

  The tenant leaf's memory limit sits `SANDBOX_MEMORY_RESERVE_BYTES` below the lease's. So the out-of-memory killer picks only a tenant process, and the call it ended reads `out_of_memory`.

  Its throttle, `tenant/memory.high`, sits an eighth below that limit and never more than 128 MiB below it. A tenant writing past its disk is slowed while its pages are written back, and reads `ENOSPC` rather than being killed first. The cap keeps the slow band narrow on a large lease: with no swap, everything inside it is throttled.

- An enforced disk quota. The lease's `limits.disk_bytes`, or the runner's 4 GiB default, sizes the workspace disk. The disk holds `/tmp` as well as `/workspace`, and answers `ENOSPC` when full.

  `/tmp` is never a tmpfs, because tmpfs pages stay charged to the lease's memory until the sandbox ends. The disk attaches with direct I/O, so the host does not cache its writes a second time.

  Its image file is `0600`, formatted ext4 with no journal, and its root is owned by the sandbox user (`afr_sandbox/src/workspace_disk/mod.rs`, `afr_sandbox/src/host/mod.rs`).

- A lease's size. `LeasePayload.limits` names its processor share, memory and disk within declared bounds (`af_wire::lease::SandboxLimits`). A lease naming none gets the runner's defaults, and a size past a bound refuses the lease before any sandbox exists.

  The process cap stays the host's. Nothing yet reserves host disk. No check holds live sandboxes' disk limits against the state disk, so leases can fill it.

- A supervisor whose capability set is only what sandbox setup needs (mounts, cgroups, namespaces). Tenant code never holds a capability.

- A kernel patch cadence for runner hosts.

- The network the assigned `network_policy` names ([Runner Fleet](./runner_fleet.md) §Egress model).

Chromium does not start inside a bubblewrap lease. Its own sandbox needs a nested user namespace or a setuid helper.

`--disable-userns` and the seccomp `unshare` refusal deny the first, and `nosuid` with no capabilities denies the second. `--no-sandbox` is never passed, so the browser tools run only under the Firecracker engine.

**The remaining risk is the shared kernel**, and Firecracker removes it. A kernel privilege-escalation bug escapes every namespace sandbox on the host at once, while a microVM gives each lease its own kernel.

Firecracker does not remove the host. Host root, the host kernel and the virtual machine monitor stay inside the trust boundary under either engine.

Firecracker needs read and write access to `/dev/kvm`. Bare metal has it, and cloud VMs expose it only where the provider offers nested virtualization.

Both engines sit behind one interface and share everything outside the boundary itself:

- the supervisor;

- the in-sandbox server, which a microVM's vsock surfaces on the host as a Unix socket;

- the toolbox image, a microVM's read-only root disk;

- the workspace disk, a microVM's data disk;

- cgroups and the test lanes.

Firecracker's jailer applies the same cgroup and namespace barrier around each microVM before dropping privileges. The engine is a host attribute the control plane assigns ([Runner Fleet](./runner_fleet.md) §Assigned policy and reconciliation). Fleets that run processes lease only to runners whose engine allows it; lease assignment carries no such filter today.

## Toolbox

The toolbox is a minimal Debian root filesystem. Its pinned packages are `ca-certificates`, `coreutils`, git, Python 3, Node, ripgrep, jq, and curl. Vendored uv and uvx come from the same pinned uv release.

It excludes Go and the Go-built GitHub command-line tool (`gh`); GitHub API calls use the supervisor's `http_request`. Chromium, Codex and Claude Code await their engine integrations.

It ships as a compressed read-only EROFS image, mounted once per host and bound read-only into every lease. Each lease writes to its own workspace disk, mounted at `/workspace`. The capability report states whether the host kernel can mount the toolbox's filesystem and whether `/dev/kvm` is present.

A host that cannot mount the toolbox cannot build a sandbox, so it refuses every lease.

**Built from pinned inputs.** `make toolbox-image` runs mmdebstrap over `scripts/toolbox/manifest.txt` on a native builder for each architecture. `mkfs.erofs` compresses the result with LZ4 high compression (LZ4HC).

The release record pins:

- The snapshot.debian.org inputs for main, updates and security, and every package's version and hash.

- The builder's mmdebstrap, apt, dpkg, erofs-utils and compression-library versions.

- The upstream uv archive containing uv and uvx, the only vendored binaries, by URL and SHA-256.

- The EROFS features enabled and the runner versions the image serves.

- The digests of the image and its Software Bill of Materials (SBOM).

Two builds of one record are byte-identical. A changed dependency is a new release, never an edit to an old one. Rebuilding proves we can build a release again; it does not prove an upstream binary came from its claimed source.

**Signed and scanned.** The CI host installs Syft, Grype and cosign after the immutable image is built.

Syft, Grype and cosign never enter the toolbox. Syft scans a read-only mount and writes the SBOM outside it.

Syft and Grype use separate caches under the job's temporary directory, so a privileged scan cannot block the unprivileged database reader. Findings in Debian packages are reconciled against Debian's security tracker. A backported fix keeps a version number that a version-only scanner calls vulnerable.

A fixable Critical blocks signing and deployment. cosign signs the release manifest. The runner checks that signature offline against a public key it is built with, so no lease waits on Sigstore.

The amd64 build refuses an image above 262,848,512 bytes, the existing image measured in dev run 38072785909. Vulnerabilities are evaluated daily and the snapshot is refreshed weekly. An actively exploited vulnerability with a usable fix gets a new release within 24 hours.

**Downloaded before any lease.** The image is published apart from the runner binary, with an offline bundle carrying both.

A host downloads it at install or by background reconciliation, never at lease time. The host keeps the current and previous images, plus any image a lease still holds.

A 1 GiB image at 100 Mbit/s is about 86 seconds of transfer. That cost lands at host setup. Every byte a lease needs is local before the host takes the lease.

**Admitted by descriptor.** A host becomes ready in this order:

1. the release manifest is authenticated: architecture, length, digest, EROFS features, and the runner versions it serves;

2. the image is staged in a private directory, verified, synced, and published with an atomic rename;

3. the published image is opened once without following symbolic links, checked to be a regular file of the manifest's length, and that descriptor is hashed;

4. the same descriptor is attached read-only to a buffered loop device with `LOOP_CONFIGURE` (Linux 5.8), then mounted `ro,nosuid,nodev`;

5. only then does the host report ready.

The toolbox device stays buffered, though direct I/O kept the host from caching the image twice: one image of at most 160 MB a host. Over 360 cold first uses a mode, direct I/O was 3 to 12% faster at p50 and 1.12 to 2.98 times slower at p99. Every first-use step regressed ([M237_002](../v2/done/M237_002_P2_DOCS_INFRA_OBS_SANDBOX_START_DROPS_POLLING_AND_DOUBLE_CACHE.md) Discovery).

Nothing reopens the image by path after its hash is taken. A file substituted at that path therefore never reaches the kernel's filesystem parser.

A mount found at startup is adopted only when it is EROFS, mounted `ro,nosuid,nodev`, on a loop device backed by the admitted file. A directory named after a digest proves nothing.

Each lease holds the image it runs on. Past the current and previous releases, an image nothing holds is unmounted at the next admission or retention pass. The startup sweep removes whatever an interrupted stage left.

Host root is inside the trust boundary. It controls the runner, the kernel and the mounts, and no integrity mechanism protects a tenant from it.

**Runtime package set.** The image contains neither apt, dpkg, passwd-package commands nor Perl. The pinned Debian Git executable supports common repository workflows without Perl. Optional Perl helpers, including send-email, svn, request-pull and filter-branch, are unsupported.

Python and Node remain available, and project dependencies install into the writable workspace.

The existing signed release JSON records retained package versions, archive digests and copyright hashes. Its bounded compressed `package_metadata` preserves the final Debian status and package info records, including file lists, checksums and control files. The image commits to those records in `/etc/toolbox-inventory.sha256`.

The build scanner verifies image identity and that commitment, reconstructs records in a temporary OverlayFS upper layer, and scans the complete view. The shipped image has no package database; runner admission keeps its existing signature, byte, digest, architecture and filesystem-feature checks (`scripts/toolbox/build.sh`, `.github/actions/build-toolbox/action.yml`).

**Tools a fleet adds.** A self-contained tool ships as a prebuilt pack at `/opt/tools/<digest>`, mounted before the sandbox is made.

The pack carries its own dependency closure, an explicit `PATH` entry and the toolbox digests it works with. A tool that changes system libraries gets a complete image profile instead.

Project dependencies install into `/workspace` with locked versions and tenant-scoped caches, charged to the lease's time and disk. No package is installed at lease admission. Packs are built when a fleet first needs one.

Trees that must merge use a read-only OverlayFS with several `lowerdir` entries and no `upperdir` or `workdir`. Two Debian installations assembled apart are never overlaid, because apt has not resolved the combination.

**Format.** EROFS with LZ4HC stays the default, with its features pinned at build.

A switch to zstd (Linux 6.10 and later), an uncompressed image or SquashFS must make the image at least 20% smaller. It may cost at most a 10% p99 regression in first use, on the same package tree and workload.

**Under Firecracker.** The same EROFS bytes become the guest's read-only virtio root disk.

The host's bind of `agentsfleet-runner` cannot reach a guest. So the binary and a small init travel inside the image or on a second read-only disk.

The workspace disk is a separate ext4 data disk, never mounted by the host and a guest at once; ownership passes between them. A pack becomes another guest disk or is flattened into a profile at build.

A guest keeps its own page cache and decompressed pages. So bubblewrap's page-cache sharing through one host mount does not carry over, and neither does its memory density.

**Not taken.** fs-verity ties integrity to the host's filesystem. composefs shares content across image versions, and a host keeps only two. Nydus and eStargz fetch on first read, which puts the network in a command's path.

A delta service waits until three consecutive releases show deltas cut transferred bytes by half. The apko, Nix, BuildKit, Guix, Buildroot and Yocto builders produce the same image and shorten no lease start.

The lease path is fast because it pulls no image and unpacks no layer. Warm slots are sandboxes started ahead of a lease, with an empty workspace disk mounted and the in-sandbox server idle. They are built but unwired (§"A lease's sandbox today").

A lease that ends processed leaves its sandbox held for the fleet's next lease, and every other lease's sandbox is destroyed. The workspace restore stays on the lease path and costs what the snapshot weighs, so the snapshot is kept small. Both are in §"Workspace between leases".

A per-tenant git object cache on each host means a clone fetches only new commits. Hashing a whole image warms the host's cache, so a cold sandbox does not mean cold storage.

The specs record measured start budgets, and this page asserts none. The specs measure host staging, sandbox readiness, the first useful command and the first browser screenshot separately. Each runs on cold and warm cache, at p95 and p99, with leases running concurrently.

## A lease's sandbox today

This section records what the code builds, against the design above. Paths below start with their crate, which sits in its layer folder under `rustd/crates/`.

`agentsfleet-runner run` builds one `BubblewrapEngine` per host through `BubblewrapConfig::for_host` (`rustd/crates/runner/main/src/main.rs`). Its inputs are the cgroup the service manager delegated, the admitted toolbox and the runner binary itself. The kernel lane builds its engine the same way (`afr_sandbox/examples/kernel_lane/lane.rs`).

**Only a lease that needs a sandbox gets one.** A lease whose tools all run in the supervisor starts none (`afr_agent/src/engine/mod.rs`, `afr_supervisor/src/lease_loop/mod.rs`). `http_request` and the memory tools run in the supervisor, so a Pull Request reviewer built from them never builds a sandbox. Shell, git and file tools run in one (`afr_tools/src/catalog/mod.rs`).

**A processed lease holds its sandbox; other outcomes kill it before reporting and release its disk after.**

```
 lease start                                                 lease end
   ├─ mkdir <state>/<lease_id>          0711, never reused        │
   ├─ sparse workspace.img 4 GiB → mke2fs ext4 → mount(2), loop   │
   ├─ cgroup: 2 GiB, no swap, 2 cores, 512 pids, 200 MiB/s io     │
   ├─ bwrap: new user/pid/ipc/uts/cgroup ns, cap-drop ALL         │
   │     net         the host's, its own, or its own + a veth     │
   │     /           toolbox EROFS image, read-only, shared       │
   │     /workspace  the loop-mounted ext4                        │
   │     /tmp        a directory on the same ext4 disk            │
   │     /run /dev/shm        tmpfs: RAM, charged to the cgroup   │
   ├─ child: no_new_privs → Landlock → seccomp → executor         │
   ├─ allow_list_egress: host-side nf_tables table for the veth   │
   │              … the turn runs …                               ▼
   └─ processed: frozen and held for the fleet, else killed, the report posted, then released
      before the report: cgroup.kill, then populated 0 within 5 s
      after the report:  the egress table and veth, when built
                       → rm cgroup → umount → rm image → rm dir
```

The executor answers over one end of a socket pair the engine makes for the lease, and no path names it. The supervisor waits at most 10 s for the executor to list the workspace over it before it refuses the lease (`afr_sandbox/src/bubblewrap_engine/mod.rs`).

A lease's sandbox is killed before settle and torn down once the report posts (`afr_supervisor/src/lease_loop/hold.rs`, `afr_sandbox/src/bubblewrap_engine/parts/teardown.rs`). A processed lease's sandbox is frozen and held instead (§"Workspace between leases").

Building the engine sweeps every leftover lease directory in its state directory (`afr_sandbox/src/bubblewrap_engine/sweep.rs`), and every egress table and veth a crashed run left. So a host builds one engine per state directory: an engine per worker tears down its siblings' leases.

Warm slots (`afr_sandbox/src/warm_slots/mod.rs`) are sandboxes started ahead of a lease, one lease each; only the kernel lane uses them so far.

**Cold start is milliseconds; the waits are elsewhere.**

In the kernel lane, a debug build went from lease accept to executor ready in 23.6 ms cold, median of five. From a warm slot it took 0.66 ms ([M210_001](../v2/done/M210_001_P1_API_INFRA_RUST_RUNNER_SUPERVISOR_SANDBOX_FOUNDATION.md) Discovery). That lane kept its state under a tmpfs `/tmp`.

On a disk under `/var/tmp`, with 100 leases 4 at once, executor ready took 100.8 ms cold at p50 before M237. The in-process disk took that under 50 ms. The specs' Discovery holds every figure ([lease waits](../v2/done/M237_001_P1_API_DOCS_INFRA_OBS_SANDBOX_START_HIDES_BEHIND_MODEL.md), [start](../v2/done/M237_002_P2_DOCS_INFRA_OBS_SANDBOX_START_DROPS_POLLING_AND_DOUBLE_CACHE.md)).

Work arrives through a pushed offer when the gateway can claim it for an available worker.
Memory preparation and the first model call follow the offer.
The host fetches the toolbox before taking work.

**Reading the lease path's speed.** A regression should show as a moved percentile on a signal the runner exports. A fleet watching Grafana can then find it ([durable repair](scenarios/durable-repair.md)). Each question below has one:

| Question | Signal |
|---|---|
| How long does a sandbox take to build? | `agentsfleet_runner_sandbox_start_duration_seconds`, by outcome |
| Did a tool call wait on the build, and for how long? | `agentsfleet_runner_sandbox_wait_duration_seconds`; `sandbox_wait_ms` on `lease_completed` |
| How often does a build fail, and at which step? | `agentsfleet_runner_sandbox_builds_total`, by outcome |
| How long is the release after the report? | `agentsfleet_runner_sandbox_release_duration_seconds` |
| Where did one lease's setup time go? | `egress_ms`, `prepare_ms` and `checkout_ms` on `lease_completed` |
| How fast is the model? | `agentsfleet_runner_provider_turn_duration_seconds` |

Three questions have no signal yet. Nothing times an event's wait for its lease, or a lease's wait for its first model request. Nothing times each step inside a prepare either; the follow-up to M237 adds all three.

**Where the bytes live.**

| Thing | Lives in | Bound | Lifetime | Source |
|---|---|---|---|---|
| Toolbox image | a file on disk, loop-mounted read-only once per host, shared by every sandbox | the image's size | the host's | `afr_sandbox/src/toolbox/mod.rs` |
| Toolbox blocks in use | the kernel page cache in RAM, shared and evictable | none of its own; memory pressure evicts it | while hot | kernel |
| Workspace disk, with `/tmp` | `<state>/<lease_id>/workspace.img`, sparse; `/tmp` is its `tmp/` directory | the lease's `disk_bytes`, 4 GiB by default; a full disk answers `ENOSPC` | one lease, or until its hold ends | `afr_sandbox/src/workspace_disk/mod.rs`, `afr_sandbox/src/engine/mod.rs` |
| `/run`, `/dev/shm` | tmpfs in RAM, charged to the lease's memory cgroup | the cgroup's 2 GiB | one lease | `afr_sandbox/src/bubblewrap/mod.rs` |
| Processes | the lease's memory cgroup, no swap | 2 GiB | one command, unless it leaves the shell's process group; exec sessions close at run end | `afr_sandbox/src/cgroup/mod.rs`, `afr_executor/src/server/process/mod.rs`, `afr_agent/src/loop/finish.rs` |
| Repository mirrors and lease sandboxes | The storage home's `git/` and `sandboxes/`, under `/var/lib/agentsfleet-runner` unless `RUNNER_STORAGE_HOME` says otherwise | none found | Across leases; sandbox cleanup follows lease end or the engine's boot sweep | `afr_supervisor/src/config/mod.rs`, `afr_supervisor/src/storage_home/mod.rs` |

**What bounds a runner host.** The image does not, because its blocks sit in shared page cache that memory pressure evicts.

The lease limits bound memory, summed over the host's workers. A sandbox gets 2 GiB by default and up to 64 GiB when its lease names a size.

The worker count defaults to 1 and caps at 64 (`af_core/src/limits.rs`). So default limits reach 128 GiB at the cap, and nothing checks that sum against the host's memory. Disk holds 4 GiB per lease by default and up to 256 GiB for a sized one, sparse, with no headroom reserved.

**Where the code and this page differ today.**

- In practice, every lease gets the runner's defaults (`afr_sandbox/src/engine/mod.rs`, set in `afr_supervisor/src/lib.rs`). They are 2 GiB of memory, 2 cores, 512 processes and a 4 GiB disk. The runner builds the size a lease names (`afr_supervisor/src/lease_loop/workspace.rs`), but no fleet carries a size yet, so the daemon sends `limits: null` (`afd_fleet/src/lease/answer/mod.rs`).

- A held sandbox keeps its disk's bytes, and nothing reserves host disk for it: holds are bounded only by the runner's worker count.

- No workspace restore exists yet, so the first model call waits for the sandbox and its repository checkout.

- The operator's extra binds on the assigned policy are stored, validated and shown, but no runner binds them. The sandbox's host paths are the fixed set above (`afr_sandbox/src/bubblewrap/mod.rs`).

The storage home is `/var/lib/agentsfleet-runner` unless `RUNNER_STORAGE_HOME` names another root (`afr_supervisor/src/config/mod.rs`). The bare-metal unit makes that directory writable under `ProtectSystem=strict` and delegates the `cpu io memory pids` controllers the probe requires (`deploy/baremetal/agentsfleet-runner.service`).

`run` defaults to bubblewrap; `run --engine bubblewrap` selects it explicitly, and unsupported engine names refuse argument parsing.

Before probing or admitting a toolbox, `afr_sandbox::boot` enables those controllers in the delegated service root (`afr_sandbox/src/cgroup/delegated.rs`). `DelegateSubgroup=runner` keeps the runner in its own leaf so that the root can distribute controllers to lease children.

Discovery refuses a process outside the expected `runner` subgroup. Activation writes only positive controller commands in that root; it never changes ancestors or moves processes. Unavailable controllers and kernel activation errors stop startup, with the original error cause preserved.

The probe reports host facts, and `Backend::validate` interprets them before `Backend::start` admits resources. `BubblewrapBackend` keeps every shipping capability check through `BubblewrapRequirements`, and admits the signed toolbox.

`Started` couples the engine to a `Resources` guard. The runner keeps that guard until serving and the runtime have ended, and the kernel lane closes it explicitly. The supervisor still receives `Box<dyn Engine>`.

A new backend supplies startup policy and resources through these interfaces. No other backend ships.

The kernel lane keeps its state under `/var/tmp`, on a disk as a host's is (`afr_sandbox/examples/kernel_lane/lane.rs`). Its delegation trials boot from disabled controllers (`afr_sandbox/examples/kernel_lane/delegation.rs`). They prove restart, child limits, missing-controller refusal and a populated-root failure.

`make test-runner-startup` also runs the shipping binary under a copy of the deployed systemd unit (`scripts/test-runner-startup.sh`). Both engine selections must reach absent-toolbox admission after host preparation, and refuse invalid delegation, activation failures and missing capabilities. This matrix keeps the release key; healthy service startup with the signed release is a deployment acceptance proof.

## Code walkthrough

This section traces one runner from boot to a tool call inside bubblewrap.
Paths start with their crate under `rustd/crates/`.
Parts A to G follow the order the code runs in.

Two facts frame the whole walk. Bubblewrap starts once per lease, never once per tool call. A lease gets a sandbox only when its policy offers a tool whose runtime is `Sandbox`.

### Part A: host boot, once per runner process

`agentsfleet-runner run` checks the host and admits the toolbox before it dials the daemon. A host that fails any check exits with the reason and never runs a lease unsandboxed.

```
systemd ─► /usr/local/bin/agentsfleet-runner run
 main.rs     main ─► run ─► supervise
               ├─ afr_supervisor::boot(env)          config and storage home
               ├─ EngineChoice::Bubblewrap.boot      engine.rs; the only engine today
               │    └─ Host::production              host.rs
               │         ├─ release = Release::signed_by_release(VERSION)
               │         ├─ entry   = current_exe()  this same binary, bound into each sandbox
               │         └─ afr_sandbox::boot(BubblewrapBackend, paths)      boot/mod.rs
               │              ├─ enable_controllers  cpu io memory pids in the delegated root
               │              ├─ probe               landlock, seccomp, bwrap, erofs, controllers
               │              ├─ validate            Requirements::missing ─► refuse boot
               │              └─ start               bubblewrap_engine/boot.rs
               │                   ├─ Toolboxes::open + admit_incoming     see A.1
               │                   ├─ BubblewrapConfig::for_host           bubblewrap_engine/config.rs
               │                   │    entry_args ["sandbox"], host ids 65534, ready 10 s
               │                   └─ BubblewrapEngine::new                bubblewrap_engine/mod.rs
               │                        admissible ─► own the host egress ─► sweep leftovers
               ├─ host::agent        afr_agent::Loop over the tool catalog and providers
               └─ tokio runtime ─► serve                                   main.rs
                    └─ afr_supervisor::run ─► compose ─► Lessee { engine, agent, plane }
                         native session, heartbeat policy and worker pool run together
```

The probe checks that `/usr/bin/bwrap` exists and is executable. It does not start a trial sandbox; the first lease's `prepare` is the first real start.

**A.1, toolbox admission.** It runs once per host, never per lease.

```
admit_incoming                                                toolbox/incoming/mod.rs
  ─► Release::verify (signature over the manifest bytes)
  ─► stage, verify, sync, rename into images/
  ─► KernelMounter::mount                                      toolbox/kernel_mounter.rs
  ─► Toolbox::admit_now                                        toolbox/admit.rs
       open_image(O_NOFOLLOW)  ─► fstat: regular file, manifest length
       sha256 over that same descriptor
       adopt an existing mount only if it is this file, read-only     toolbox/adopt/mod.rs
       loop_device::attach(fd, ReadOnly): LOOP_CTL_GET_FREE ─► LOOP_CONFIGURE(ro, autoclear,
         buffered, 4096-byte blocks)
       mount /dev/loopN at <mounts>/<digest> as erofs, ro,nosuid,nodev
       sandbox_toolbox_admission_completed names caching: buffered
  ─► Arc<Toolbox> kept by the config; every sandbox clones the Arc
```

**Where the bytes sit on the host.** Paths below are under the storage home, `/var/lib/agentsfleet-runner` by default.

```
toolbox/images/toolbox-<digest>.erofs   one file per release; current and previous kept
toolbox/mounts/<digest>/                a mount point; the kernel reads the image on demand
sandboxes/<lease_id>/workspace.img      one ext4 image per lease, mode 0600
sandboxes/<lease_id>/workspace/         that image's mount; bound as /workspace and /tmp
git/<workspace_id>/<owner>/<repo>.git   one mirror per workspace and repository
```

Nothing from the toolbox is unpacked to disk; mounting it adds a directory and no copy. Every sandbox on the host binds the one mount as `/`, whatever its fleet or tenant. The image holds Debian packages and vendored tools only, so tenants share code pages and never data.

A lease's own files live only on its workspace disk, which no other lease can see. Fleets of one workspace share a mirror; fleets of different workspaces never do.

### Part B: a lease arrives, and the sandbox decision

```
native offer ─► worker_pool ─► LeasePayload
Lessee::run ─► run_lease ─► LeaseRun::lease        lease_loop/mod.rs
  renewal and the activity pump start alongside
work
  ├─ needs = agent.admit(policy)                   afr_agent/src/loop/mod.rs
  │     catalog.select(policy.tools)?              an unhosted tool refuses the lease
  │     .needs_sandbox()                           afr_tools/src/selection.rs
  ├─ turns.claim(fleet)                            one run per fleet per runner
  ├─ memory::hydrate(fleet)
  └─ needs.sandbox ? sandboxed(…) : drive(slot = NoSandbox)
```

The decision reads what the policy offers, before the model runs. The model's later choices never change it.

| Runtime | Tools | Runs in |
|---|---|---|
| `Supervisor` | `http_request`, `web_fetch`, `pushover`, memory tools, `update_plan`, `message`, schedule tools, child-loop tools | the runner process |
| `Provider` | `web_search` | the model provider |
| `Sandbox` | `shell`, `exec_command`, `write_stdin`, `git`, file tools, `apply_patch`, `image`, browser tools | the executor inside bubblewrap |

### Part C: `sandboxed`, the build beside the first model call

```
sandboxed                                          lease_loop/workspace.rs
  ├─ admitted, before any model request            refused ─► startup_posture
  │     sized(lease limits, host defaults)
  │     bind egress                                allow_list resolves its host names
  │     plan: checkouts(policy), workspace id      lease_loop/checkout.rs
  └─ beside, one task, both futures boxed
       ├─ build                                    lease_loop/build.rs
       │    acquire: revive(hold key) or prepare   lease_loop/hold.rs, Part D
       │      resume_hold, reallow, thaw, executor answers within 2 s ─► reuse
       │    check_out unless held                  mints the token, Mirrors::check_out
       │    a stop ends the mint, a wait on the mirror and a fetch between git's steps
       │    fill the ExecutorSlot: the executor, or Unavailable(prepare | checkout | stopped)
       │    a failure or a stop fills the slot first, then destroys what it made
       │    count it: sandbox_builds_total{outcome = ready | prepare | checkout | stopped}
       └─ drive(slot)                              lease_loop/drive.rs; first model call
            a processed, uncut turn waits up to 20 s for the build; then, or on
            any other ending, the build is stopped
```

The first model request waits for the size, egress and plan checks alone. A sandbox-side call waits on the slot for what is left of the build, and `lease_completed` logs that wait as `sandbox_wait_ms`. A build that fails leaves the turn going: its sandbox-side calls answer `sandbox_unavailable` naming the class, and `sandbox_build_failed` is logged.

A build still running 20 s after its turn answered is stopped and logged as `sandbox_build_outlasted_turn`. A stalled token mint, a wait on another lease's mirror or a slow fetch cannot hold the lease. The report goes with nothing held.

A runner that stops leasing waits for no build, since nothing would hold its sandbox.

### Part D: `prepare`, where bubblewrap is spawned

```
BubblewrapEngine::prepare ─► start ─► build         bubblewrap_engine/mod.rs
  1 mkdir <state>/<lease_id> 0711, never reused
  2 WorkspaceDisk::create                           workspace_disk/mod.rs
      sparse workspace.img ─► mke2fs (host program)
      on the blocking pool: loop_device::attach(ReadWrite), direct I/O asked for,
      autoclear ─► mount(2) ext4 nosuid,nodev ─► close the device handle
      a start dropped mid-mount: the mount lands unheard and undoes itself
      workspace/ 0755 and tmp/ 1777, owned by 65534
  3 LeaseCgroup::create: sandbox and tenant leaves, memory, cpu and pids limits;
    then cgroup.limit_io on the disk's loop device
  4 TenantFiles::open: the tenant leaf's cgroup.procs and memory.events, opened by the runner
  5 Handover::with_link: a socket pair, both ends close-on-exec       tenant/files.rs
  6 Names::render: hosts and resolv.conf for an allow_list lease
  7 bubblewrap::arguments                           bubblewrap/mod.rs
  8 Parts::spawn                                    bubblewrap_engine/parts/mod.rs
      Command bwrap, env cleared, uid and gid 65534
      pre_exec: write "0" to sandbox/cgroup.procs, clear close-on-exec on the
        tenant fds and the entry's end of the pair, in this child alone
      spawn ─► the bwrap process exists; the runner's copy of the entry's end closes
  9 ready: list_dir(".") over the runner's end, for up to 10 s, nothing polled
    or bwrap's exit with its error stream
 10 allow_list only: veth pair and host nf_tables table
```

The argument list for an isolated lease:

```
bwrap --unshare-user --unshare-pid --unshare-ipc --unshare-uts --unshare-cgroup --unshare-net
      --disable-userns --cap-drop ALL --clearenv --die-with-parent --new-session
      --ro-bind <mounts>/<digest> /            --proc /proc   --dev /dev
      --perms 1777 --size <memory/4> --tmpfs /dev/shm
      --bind <disk>/tmp /tmp     --tmpfs /run   --bind <disk>/workspace /workspace
      --ro-bind <runner binary> /opt/agentsfleet/agentsfleet-runner
      --uid 1000 --gid 1000 --chdir /workspace
      -- /opt/agentsfleet/agentsfleet-runner sandbox
         --tenant-procs <fd> --tenant-events <fd> --executor <fd>
```

### Part E: inside the sandbox

```
bwrap ─► namespaces and the mount tree above ─► pid 1 reaper
  └─ exec /opt/agentsfleet/agentsfleet-runner sandbox     main.rs, serve/mod.rs
       adopt the tenant leaf's descriptors and the link, close-on-exec again
       harden: one thread only ─► no_new_privs ─► Landlock ─► seccomp ─► no capabilities
       current-thread runtime ─► serve /workspace over the link, one connection
```

```
agentsfleet-runner run       root           runner cgroup
 └─ bwrap                    65534          <lease>/sandbox
     └─ bwrap reaper         pid 1 inside
         └─ agentsfleet-runner sandbox      the executor, <lease>/sandbox
             └─ sh -c "…"    <lease>/tenant, moved in before exec
```

### Part F: one tool call

```
model tool call ─► Router::dispatch                 afr_agent/src/router/mod.rs
  Supervisor ─► tool.call(executor None)            http_request: egress guard, then HTTP
  Sandbox    ─► slot.get(): waits for the build, then the executor, or
               SandboxUnavailable naming why; a wait is recorded
               shell ─► oneshot::run_to_end            afr_tools/src/sandbox/oneshot/mod.rs
               ─► Client::spawn
               "process/spawn" over the link ─► executor launcher
               pre_exec joins the tenant leaf ─► sh -c
               "process/output" chunks, then "process/exited"
  Provider   ─► HostedToolUnavailable
```

### Part G: lease end

```
settle_and_drain                                    lease_loop/mod.rs
  keep ─► Fate                                      lease_loop/hold.rs
    held when: ended processed, not cut, runner still leasing, bwrap still running
      ─► freeze ─► park for up to ten minutes ─► Held
      a failed freeze, or a park the keeper refuses ─► kill
    otherwise ─► kill: cgroup.kill, then populated 0 within 5 s ─► Killed
                 an unsettled kill posts anyway and logs sandbox_kill_unsettled
  settle(&Fate)                                     lease_loop/settle.rs
    tool records posted over HTTP ─► usage flushed by native renewal
    ─► native report with memory + segments
    ─► retain the same report in worker memory until acknowledgement or terminal refusal
  release, beside the live tail's drain
    destroy: egress removed, cgroup removed, umount, image and directory removed
    run_lease returns after it, so a worker never holds two leases' disks
```

A cut lease is never held, because `keep` requires the run to end on its own. Only a kill that settles proves no sandbox process remains.

### Built by M237, and decided only

M237_001 and M237_002 built the Nov 02, 2026 decision's five steps, and four stand. They are Parts C and G, and Part D's disk and socket pair. The fifth, a direct-I/O toolbox, was measured and reverted (§Toolbox).

What follows is decided and not built.

Workspace snapshots, when they are built, carry a workspace disk's contents and never the toolbox. Every host of a release already holds the toolbox by digest.

A restore writes into the new lease's disk through the host's mount, inside the background build. It takes checkout's place for each repository it restores, so checkout never overwrites restored work. A save reads the disk only after a kill that settled.

An engine whose disk the host cannot mount, such as Firecracker, reports no host workspace. Its restore and checkout must run inside the guest, or write into the disk image before it attaches.

## Workspace between leases

The [durable repair scenario](scenarios/durable-repair.md) separates trusted task continuation from compatible conversation/file/Git recovery. Neither semantic memory nor context compaction implements workspace snapshots; a new Firecracker backend is not assumed for API-only continuation.

The workspace is the fleet's, not the lease's. Holding a sandbox between messages is built. Workspace snapshots are the target: nothing saves or restores one yet, and the bullets that describe them are marked *Target, not built*.

- **Bytes in R2, the record in Postgres**. *Target, not built.* Snapshots live in an R2 bucket under their own prefix, named by content hash.

  A Postgres row points at the latest. Any S3-compatible store would work through one `object_store` client with a different endpoint.

- **Only the supervisor moves bytes.** *Target, not built.* `agentsfleetd` mints short-lived presigned URLs per lease, fenced the way the memory push is. The `object_store` crate already implements presigning for S3, and the sandbox never sees a URL.

- **Small snapshots.** *Target, not built.* A snapshot holds local commits as a git bundle. It also holds uncommitted edits, untracked files, output files and the lease transcript. Objects re-fetchable from the origin stay out.

- **Never saved.** No credential enters a sandbox. *Target, not built:* every snapshot is leak-scanned before upload.

- **Read as hostile.** The supervisor treats `/workspace` as the tenant's. Restore and save never follow a symbolic link out of it. No git command the supervisor runs uses the workspace's configuration, hooks or helpers, including when `propose_change` reads the fleet's commits.

  A read-only toolbox does nothing against a privileged restore that follows a link.

- **One writer.** *Target, not built.* The fleet's affinity slot and fencing token already make one lease the live holder. A stale holder's snapshot is refused.

- **Held between messages.** A lease that ends processed leaves its sandbox held (`afr_supervisor/src/holds/mod.rs`). The lease cgroup is frozen through `cgroup.freeze` and settled on `frozen 1`. The hold is keyed by fleet, workspace, limits and policy (network policy plus repository binding).

  No credential is in the held sandbox to empty, because none enters a sandbox. The fleet's next lease within ten minutes thaws it and continues with the files the last lease left.

  The host never writes into the held clone, because a tenant-written `.git` under a privileged copy is a symbolic-link escape.

  Processes are not promised. The in-sandbox server kills a command's process group when the command ends. The run's end closes every exec session, so only a process that left the group with `setsid` resumes.

  Any of these destroys the hold:

  - a key mismatch, or a lease the daemon says does not resume it;

  - a failed thaw, or the deadline;

  - the cap of one hold per worker (oldest first), or the runner's last free worker taking a lease;

  - the runner stopping leasing for any cause, or shutting down;

  - a report refused or rejected for good, or a sandbox no longer running;

  - a fleet the daemon names halted, deleted or leased elsewhere.

  The daemon's side of a hold, `held_until` and the skip other runners make, is in [Runner Fleet](./runner_fleet.md) §"Cold and warm execution".

```
lease ends processed ──► freeze ──► held (≤ 10 min, ≤ 1 per worker)
                                      │
   next lease, same fleet + key ──────┼──► thaw ──► runs in the same sandbox
   other key / thaw fails ────────────┼──► destroy ──► fresh sandbox
   deadline · cap · saturation ·      │
   shutdown · refused report ·        └──► destroy
   fleet halted or deleted
```

- **Retention.** *Target, not built.* The last three snapshots per fleet, a lifecycle rule on the prefix, and a purge when the fleet is deleted.

- **Continuity.** Reports save a checkpoint to `fleet.sessions`, but no lease carries it. Chat leases carry recent thread turns (§Crates).

Files a fleet saves for the user live beside the snapshots. The user downloads them through a presigned URL the daemon authorises.

## Credentials

- **Model keys** stay in the supervisor.

- **Repository reads.** The supervisor clones with the lease's own one-hour, repository-scoped token from the mint verb (`rustd/crates/daemon/afd_credential/src/credential/github/request.rs`). The token reads on a read binding and writes on a write binding.

  The token never enters the sandbox. The fetch presents it in an in-memory header, and the clone's configuration names the plain URL (`rustd/crates/runner/afr_supervisor/src/workspace_clone/mod.rs`).

- **Tools that need a login reuse today's mint path.** Fleets already name credentials and never hold them. A `${secrets.github.token}` placeholder resolves through `POST /v1/runners/me/credentials/mint`, which the supervisor calls.

  Only where a token lands is new. Today the supervisor mints it once per lease and keeps it in that lease's egress guard until shortly before it expires. The guard substitutes it only into a request's `Authorization` header, and masks it in whatever the upstream sends back.

  A command-line tool such as the GitHub one needs the token as a file or variable. The design writes it into a memory-only `/run/creds` for that lease. That is not built, so a token stays in the egress guard and no sandbox command receives one.

  Each token is bound to the hosts it is for, and the lease's allowlist admits only those hosts.

- **Thread lines and schedule messages are masked twice**. The `message`, `cron_add`, `cron_update` and `schedule` tools mask lease-minted tokens before text leaves the runner.

  `agentsfleetd` then masks the fleet's declared static secrets with the same `af_secrets::Scrub`. It does so before it posts a line through the outbound worker's Slack poster, or stores a schedule.

  The runner never holds the channel credential.

- **A placeholder-swap proxy can later replace the file without fleets noticing.** The mint path stays the same. The supervisor writes a placeholder into the planned `/run/creds` instead of the token.

  Every connection an `allow_list_egress` sandbox makes already leaves through the host-side end of its veth pair. The toolbox's trust store has an empty slot for a proxy's certificate.

  Adding the proxy reverses the no-interception rule in [Runner Fleet](./runner_fleet.md) §Traps, and that reversal is recorded when it ships.

## Coding engines

- **Codex is first-class, split across the wall.** `codex app-server` runs in the supervisor and holds the user's ChatGPT subscription or OpenAI key. `codex exec-server` runs in the sandbox and only runs commands, so the subscription never enters the sandbox.

  App-server adds a remote exec server as an "environment" (`~/Projects/oss/rs/codex/codex-rs/exec-server/README.md`). Whether every tool call routes to that environment is proven before the engine ships. Codex's JSON Lines events (`codex exec --json`, `~/Projects/oss/rs/codex/codex-rs/exec/src/cli.rs`) map onto the same activity frames as our own loop.

- **Claude Code** runs in the sandbox and reaches its model through a relay the supervisor serves. Its `ANTHROPIC_BASE_URL` names a loopback endpoint, and the supervisor adds the Anthropic API key outside the sandbox, so the key never enters it. Subscription use stays off until its terms are cleared.

- Both run with their own sandbox off. Ours is the boundary, and nested user namespaces are disabled anyway.

## Repository writes

The fleet uses real git inside the sandbox: branches, several commits, the normal flow. `propose_change` hands the commits to the supervisor, which:

1. checks them against the rules `agentsfleetd` compiled for the lease: the pinned branch, no change under `.github/workflows`, and size caps;

2. mints a write token for that one call and pushes;

3. opens a draft Pull Request.

When a fleet requires approval, the push runs in the continuation lease after someone approves. The daemon compiles the rules (`rustd/crates/daemon/afd_gate/src/policy/egress/write.rs`), and the supervisor enforces them outside the sandbox.

The loop renders the same rules into the prompt as a trusted repair context. That context names the one repository, the daemon-named repair branch and the trusted base. A write-bound fleet's instructions require that input before writing, and nothing else supplies it.

A write token inside the sandbox would be readable by every program there, including a dependency's install hook. The rules' own module documentation says this about a repository-scoped write token. That token "can force-push to `main` as easily as it can open a draft Pull Request".

## Why Rust

- **Memory safety is checked by the compiler.** The runner this one replaced relied on rules a reviewer enforced by hand. They were one owner per resource, init and deinit pairing, idempotent cleanup, and draining before deinit. OWN is still in the [code standard](../../.orly/standards/code.md); DEINIT, DIDEM and DRAIN retired with that runner, which needed a memory-leak lane of its own.

- **The language is stable.** The runner this one replaced was written in a pre-1.0 language. A standard-library list change cost the rulebook a rule (ZAL), and a process I/O change cost its engine a patched fork.

- **One wire.** The runner speaks `af_wire`, the daemon's own types, so a mismatch between the two sides fails to compile.

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
| Oct 02, 2026 | A fresh Rust runner, designed for this platform rather than ported from an earlier runner's structure | Indy: "The port is a fresh port, since we always have the last binary with us and running." |
| Oct 02, 2026 | Fleets clone and work in a real repository; supersedes M157_001's "No working tree, no git binary" | Indy: "a fleet will have to clone the copy of code and so on and mug around with it" |
| Oct 02, 2026 | No Docker on the lease path; start time is a requirement | Indy: "preferrably avoiding Docker since its layer takes a while to load... I will need faster results as well" |
| Oct 02, 2026 | Workspaces and artifacts carried in R2 or an S3-compatible store | Indy: "we could have the S3 compatible R2 pointing to code and artifacts if need be for the user" |
| Oct 02, 2026 | Subscriptions and API keys; Codex first-class | Indy: "Subscripts and API keys, but support Codex as first class citizen, anthropic seems to have issues on terms and requests via API keys" |
| Oct 02, 2026 | Multi-tenant on bare metal or VMs from the first release; Firecracker is an additional engine | Indy: "everyone can run isolated multiple tenants with fleets must use a baremetal host or have a runner run from a VM from day 1. To me firecracker is an additional approach to think on isolation." |
| Oct 02, 2026 | Scoped short-lived tokens now; the placeholder-swap proxy later | Indy: "I would go for 1, with the focus on move to 2 later" |
| Oct 02, 2026 | The supervisor pushes repository writes | Indy chose "Supervisor pushes (Recommended)" |
| Oct 02, 2026 | Code-running leases from different tenants may share a host; hardening and kernel patching are the boundary until Firecracker | Indy chose "No, share freely" when asked whether a host should refuse a second tenant's code-running lease |
| Oct 02, 2026 | Firecracker is the production engine for code-running leases; bubblewrap ships first and stays for development, CI and hosts without `/dev/kvm` | Indy: "i think we must shoot for firecracker then", then "Yes bubblewrap first, and firecracker next." Firecracker is installed on production hosts later; every runner reports whether `/dev/kvm` exists |
| Oct 02, 2026 | The Rust runner is independent: one copy of the wire, shared with the daemon, and it follows `rustd` principles | Indy: "A second copy of the wire will not be existing" and "the rust code is independent and follows our current rustd/ principles" |
| Oct 02, 2026 | The runner carries every published tool; the loop stays in the supervisor and routes the code-running tools into the sandbox | Indy: "i need all the tools … the sandbox isnt just a sandbox but a harness that decide to operate like codex so that is critical to realize the fleets i plan to use"; chose "Supervisor loop, sandbox tools" |
| Oct 02, 2026 | One binary: the in-sandbox entry is `agentsfleet-runner sandbox`, a sub-mode, not a second artifact | Indy: "why do we need two ? agentsfleet-runner, agentsfleet-executor … i thought its just one binary?" then "agentsfleet-runner"; chose `sandbox` for the sub-mode |
| Oct 02, 2026 | `cron_*` and `schedule` become fleet tools through a runner verb onto the daemon's schedule plane; supersedes "Scheduled wakes are not a child tool" in [Capabilities](./capabilities.md) §"2. The platform tools the fleet can call" | Indy chose "Yes, runner verb onto daemon schedules" |
| Oct 03, 2026 | Keep Debian, mmdebstrap and EROFS with LZ4HC. The toolbox is built from pinned snapshots, signed, scanned, downloaded before any lease and admitted by descriptor, which replaces hashing the loop device after the mount. Fleet tools arrive as prebuilt packs or image profiles, never as packages installed at admission (§Toolbox) | Tarzy's review of the toolbox design; Indy chose "Approve as classified (Recommended)" |
| Oct 03, 2026 | Not taken: fs-verity, composefs, Nydus and eStargz, a delta service, and the apko, Nix, BuildKit, Guix, Buildroot and Yocto builders | The same review and choice |
| Oct 03, 2026 | Claude Code reaches its model through a supervisor relay, so no model key enters a sandbox; supersedes "Claude Code runs in the sandbox on an Anthropic API key" | Indy: "1 - if our current design spells that way". Four lines on this page already kept model keys out of the sandbox |
| Oct 03, 2026 | Revoking a compromised toolbox is deferred; the toolbox works first, and rollback stays the previous release | Indy: "I dont want to focus on revocation of a compromised toolbox, first is to get the toolbox working" |
| Oct 03, 2026 | Named model providers resolve through a local, versioned registry of name, wire and base URL, embedded as data the way IronClaw embeds its `providers.json`; unknown names are refused at admission; a `custom:<url>` provider is `https` only and the transport follows no redirect. No LLM framework's provider catalogue decides admission | Indy: "How does ironclaw does this? Can we review and steal from here", with Tarzy's recommendation to keep refusing unknown names and support every named provider whose wire was already implemented |
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
| Oct 08, 2026 | The Zig runner is retired: this Rust runner is the only runner, and nothing in the repository builds, lints or tests Zig | M213_001, PR #738. Indy: "Yes remove zig related or mentioned its sunset after this PR#..." |
| Oct 08, 2026 | The provider registry gains `glm` (Z.ai's general endpoint), `minimax` and `qwen` (DashScope's international endpoint) on the chat wire, and `kimi` dials Moonshot's international endpoint, where the model catalogue prices it. `gemini` stays out because its OpenAI-compatible endpoint wants Gemini 3's thought signatures back on every tool call, which the chat wire does not carry; `bedrock` stays out because it needs AWS Signature Version 4 and event-stream framing. The catalogue prices only names the registry routes | Indy: "Yes reconcile, and produce a table on what is supported. Ensure the api-dev is trimmed to the models we need." |
| Oct 10, 2026 | Remove Go-built `gh` without replacement; scanners and signing tools run only on the CI host. Refuse an amd64 toolbox above the measured 262,848,512-byte image | Indy: "Well i dont want Go in the toolbox or grype in the toolbox image, that is stupid crap aiwa"; "I want the toolbox image size to be in limits as its now, not bump it with the crap you have". Dev run 38072785909 confirms the cache fix, the prior image size and the embedded Go Critical findings |
| Oct 17, 2026 | Saved model inference settings replace the embedded provider routing JSON. Workspace-owner registrations and the active platform default supply credentialed choices; model output limits come from their saved catalogue row. Both inference hosts filter private SDK content. | Indy: "Also replace the JSON routing table with persistent provider configuration"; "have the persisted model details as the source of truth." |
| Oct 19, 2026 | The runner reports cache writes apart from input, by the lifetime the provider names, and the daemon bills each class at its own rate instead of at the input rate (M229 Dimension 8.6) | Indy (2026-10-18): cache-write tokens are reported and billed by lifetime, "Bill them now" |
| Nov 02, 2026 | A sandbox build that fails while the run is live leaves the run going; sandbox tools answer `sandbox_unavailable` naming the failure, and egress checks stay before the model call (M237_001) | Indy chose "B: model carries on", asking whether setup and the model are independent |
| Nov 02, 2026 | Built by M237_001 and M237_002: the sandbox builds beside the first model call, the report posts before the disk is released, the toolbox loop device reads with direct I/O (reverted Nov 03), the executor speaks over an inherited socket pair, and the workspace disk mounts in-process | Indy: "1-5 seemed good fixes to do" and "Yes i am happy with it, along with the spec" |
| Nov 03, 2026 | The toolbox loop device stays buffered, superseding Nov 02's direct I/O. A build waits at most 20 s past its answered turn. `sandbox_builds_total` counts builds by outcome | Direct I/O failed the 10% p99 rule on all 8 first-use steps. Indy chose "Revert to buffered", "Counter in this PR" and "Fold the build deadline in here" |
