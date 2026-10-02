# Runner execution — a trusted supervisor, a sandbox per lease, a workspace that outlives the lease

> Scope: how `agentsfleet-runner` executes one lease — where the agent loop runs, what the sandbox is made of, how a fleet's code and files survive from one lease to the next, and how credentials and repository writes cross the sandbox wall. This page describes the Rust runner. The Zig runner it supersedes is in [Runner Fleet](./runner_fleet.md) §"Running one event". The control protocol, renewal, fencing and report semantics in [Runner Fleet](./runner_fleet.md) are shared and unchanged.

## Facts

| Fact | Value |
|---|---|
| Language | Rust, in the `rustd` workspace beside `agentsfleetd`, under the same principles: one error type per crate through `afd_core::error_shell!`, one source per constant, pure logic apart from I/O. The wire is `afd_wire` and nothing else; nothing in the runner refers to the Zig runner. Daemon→runner types decode leniently, so a runner never refuses a field a newer daemon adds |
| Process model | A trusted **supervisor** runs the lease loop and the agent loop; a per-lease **sandbox** executes tool calls and nothing else |
| Hosts | Bare metal or a VM, multi-tenant from the first release |
| Sandbox engine | Firecracker microVMs for code-running leases from many tenants, on hosts that expose `/dev/kvm`. bubblewrap (Landlock, seccomp, cgroup v2, a per-lease network namespace) ships first, and stays as the engine for development, CI and hosts without `/dev/kvm` |
| Toolbox | A read-only root filesystem built once per release and present on every host. No container image is pulled or unpacked on the lease path |
| Workspace disk | Per lease, a writable block image sized to the lease's disk limit, mounted at `/workspace`, deleted at lease end |
| Warm slot | A sandbox already started before any lease arrives: cgroup made, workspace disk mounted, executor idle. One lease, then destroyed |
| Workspace | Per fleet, restored from and saved to R2 (or any S3-compatible store, such as a self-hosted RustFS). Only the supervisor moves bytes |
| Model keys | Supervisor only; never inside a sandbox |
| Coding engines | Codex first-class, split across the wall; Claude Code with API keys |
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
 │ tool router · leak scan · events → activity frames                   │
 │ workspace restore and save · git clone · credential minting · push   │
 └───────────────┬──────────────────────────────────────────────────────┘
                 │ one executor connection per lease (a Unix socket)
 ┌ sandbox — one per lease ─────────────────────────────────────────────┐
 │ /            toolbox, read-only                                      │
 │ /workspace   restored, writable, disk quota                          │
 │ /run/creds   memory-only, short-lived scoped tokens, never saved     │
 │ executor: processes on pseudo-terminals, files, apply_patch          │
 │ own network namespace + allowlist · cgroups · seccomp · no caps      │
 └──────────────────────────────────────────────────────────────────────┘
```

**The agent loop runs outside the sandbox.** No model key ever enters a sandbox, so a prompt-injected command cannot read one, and the sandbox is a plain executor, so bubblewrap and a microVM sit behind one interface. A model call starts while the workspace is still restoring, because nothing about the call needs the sandbox.

**The executor is small and ours.** One process per lease inside the sandbox serves spawn, write, read and kill for processes on pseudo-terminals, file reads and writes, and `apply_patch`. Its methods mirror Codex's `exec-server` (`~/Projects/oss/rs/codex/codex-rs/exec-server/README.md`), so the Codex engine and our loop drive the same shapes.

**Every call ends exactly once.** When a run ends for any reason — answer, crash, kill, timeout — the supervisor closes each call still open as `interrupted`, live and in the trace ([Runner Fleet](./runner_fleet.md) §Live activity).

## Crates

```
rustd/crates/
  afd_wire              the one wire, shared with agentsfleetd (exists)
  afd_core              error_shell!, timing constants (exists; no datastore dependency)
  afr_executor          executor protocol, in-sandbox server, supervisor-side client
  afr_sandbox           engine interface; bubblewrap engine now, Firecracker engine next
  afr_agent             agent loop, tool router, events, run trace
  afr_providers         Anthropic Messages, OpenAI Responses, OpenAI-compatible chat
  afr_tools             hosted tools, then exec_command, apply_patch, propose_change
  afr_supervisor        lease loop, renewal, report spool, activity, memory, minting,
                        bundles, storage sweep, capability report, control-plane client
  agentsfleet_runner    binary: composition root only
  agentsfleet_executor  binary: the executor inside a sandbox (a microVM's init, later)
```

Dependencies point one way: the binaries → `afr_supervisor` → `afr_agent` → `afr_providers` and `afr_tools` → `afr_executor`, with `afr_supervisor` → `afr_sandbox` → `afr_executor`. Every runner crate may depend on `afd_wire` and `afd_core` and on nothing else from `agentsfleetd`, and none links a datastore crate. The executor is its own small binary because it lives inside every sandbox and, under Firecracker, inside every guest image.

## Sandbox engines

Fleets on shared hosts run their own programs — build scripts, test suites, package installs, Python the model writes — so the sandbox is the only thing between two tenants' processes and one kernel:

- bubblewrap: a fresh user, PID, IPC, UTS, mount and network namespace per lease; `--cap-drop ALL`, `--disable-userns`, `--clearenv`, `--die-with-parent`, `--new-session`.
- `no_new_privs`, then Landlock, then seccomp. The seccomp filter refuses `io_uring`, `ptrace`, `process_vm_readv`/`writev`, `unshare`, `bpf`, `keyctl` and `perf_event_open`; the last four are calls Codex's own filter leaves open.
- cgroup v2 limits on memory, processor, process count and I/O, with whole-tree kill.
- An enforced disk quota: the workspace disk is sized by the lease's `disk_write_limit_mb`, and a full disk answers `ENOSPC`.
- A supervisor whose capability set is only what sandbox setup needs (mounts, cgroups, namespaces), while tenant code never holds a capability; and a kernel patch cadence for runner hosts.
- The per-lease network allowlist: a network namespace, a virtual ethernet pair and nftables rules, with rendered resolver files ([Runner Fleet](./runner_fleet.md) §Egress model).

**The remaining risk is the shared kernel**, and Firecracker removes it. A kernel privilege-escalation bug escapes every namespace sandbox on the host at once, while a microVM gives each lease its own kernel. Firecracker needs read and write access to `/dev/kvm`: bare metal has it, and cloud VMs expose it only where the provider offers nested virtualization. Both engines sit behind one interface and share everything outside the boundary itself: the supervisor, the executor (a microVM's vsock surfaces on the host as a Unix socket), the toolbox image (a microVM's read-only root disk), the workspace disk (its data disk), cgroups, and the test lanes. Firecracker's jailer applies the same cgroup and namespace barrier around each microVM before dropping privileges. The engine is a host attribute the control plane assigns ([Runner Fleet](./runner_fleet.md) §Assigned policy and reconciliation). Fleets that run processes lease only to runners whose engine allows it; lease assignment carries no such filter today.

## Toolbox

The toolbox is built once per release: a minimal Debian root filesystem with pinned git, the GitHub command-line tool, Python 3 with uv, Node, ripgrep, jq, curl, and the Codex and Claude Code command-line tools. It ships as a compressed read-only EROFS image beside the runner binary, is mounted once per host, and is bound read-only into every lease. Each lease writes to its own workspace disk, mounted at `/workspace`. The capability report states whether the host kernel can mount the toolbox's filesystem, beside whether `/dev/kvm` is present; a host that cannot mount the toolbox cannot build a sandbox, so it refuses every lease.

Speed comes from what the lease path never does: no image pull, no layer unpacking. Each host keeps a few warm slots: sandboxes already started, each with its cgroup made, an empty workspace disk mounted and its executor idle, so a lease start fills the workspace and hands the slot its lease. A slot serves one lease and is destroyed with it. What stays on the lease path is the workspace restore, which costs what the snapshot weighs, so the snapshot is kept small (§"Workspace between leases"). A per-tenant git object cache on each host means a clone fetches only new commits. Start budgets are measured and recorded in the specs, not asserted here.

## Workspace between leases

The workspace is the fleet's, not the lease's. It survives the sandbox:

- **Bytes in R2, the record in Postgres.** Snapshots live in the bucket bundles already use, under their own prefix, named by content hash; a Postgres row points at the latest. This mirrors bundles, where Postgres holds the manifest and R2 the bytes ([Fleet Bundles](./fleet_bundles.md)). Any S3-compatible store works through the same `object_store` client with a different endpoint.
- **Only the supervisor moves bytes.** `agentsfleetd` mints short-lived presigned URLs per lease, fenced the way the memory push is; the `object_store` crate already implements presigning for S3. The sandbox never sees a URL.
- **Small snapshots.** A snapshot holds local commits as a git bundle, uncommitted edits, untracked files, artifacts and the lease transcript. Objects re-fetchable from the origin stay out.
- **Never saved.** `/run/creds` is memory-only, and every snapshot is leak-scanned before upload.
- **One writer.** The fleet's affinity slot and fencing token already make one lease the live holder; a stale holder's snapshot is refused.
- **Retention.** The last three snapshots per fleet, a lifecycle rule on the prefix, and a purge when the fleet is deleted.
- **Continuity.** The checkpoint a report writes reaches the next lease. Today it is saved to `core.fleet_sessions` and loaded with the fleet, but nothing hands it to a lease.

Artifacts a fleet saves for the user live beside the snapshots and are downloaded through a presigned URL the daemon authorises.

## Credentials

- **Model keys** stay in the supervisor.
- **Repository reads.** The supervisor clones with a read-only, one-hour, repository-scoped token from the mint verb (`rustd/crates/afd_credential/src/credential/github/request.rs`). The token never enters the sandbox.
- **Tools that need a login reuse today's mint path.** Fleets already name credentials and never hold them: a `${secrets.github.token}` placeholder resolves through `POST /v1/runners/me/credentials/mint`, and the supervisor calls that verb. What is new is only where a token lands. Today it is substituted into one request inside our own tool and dies with the call. A command-line tool such as the GitHub one needs it as a file or variable, so the supervisor writes it into `/run/creds` for that lease. Each token is bound to the hosts it is for, and the lease's allowlist admits only those hosts.
- **A placeholder-swap proxy can replace the file later without fleets noticing.** The mint path stays the same, and the supervisor writes a placeholder into `/run/creds` instead of the token. Every sandbox connection already leaves through the host-side end of the lease's network namespace, and the toolbox's trust store has an empty slot for a proxy's certificate. Adding the proxy reverses the no-interception rule in [Runner Fleet](./runner_fleet.md) §Traps, and that reversal is recorded when it ships.

## Coding engines

- **Codex is first-class, split across the wall.** `codex app-server` runs in the supervisor and holds the user's ChatGPT subscription or OpenAI key. `codex exec-server` runs in the sandbox and only executes, so the subscription never enters the sandbox. App-server adds a remote exec server as an "environment" (`~/Projects/oss/rs/codex/codex-rs/exec-server/README.md`). Whether every tool call routes to that environment is proven before the engine ships. Codex's JSON Lines events (`codex exec --json`, `~/Projects/oss/rs/codex/codex-rs/exec/src/cli.rs`) map onto the same activity frames as our own loop.
- **Claude Code** runs in the sandbox on an Anthropic API key. Subscription use stays off until its terms are cleared.
- Both run with their own sandbox off. Ours is the boundary, and nested user namespaces are disabled anyway.

## Repository writes

The fleet uses real git inside the sandbox: branches, several commits, the normal flow. `propose_change` hands the commits to the supervisor, which:

1. checks them against the rules `agentsfleetd` compiled for the lease — the pinned branch, no change under `.github/workflows`, size caps;
2. mints a write token for that one call and pushes;
3. opens a draft Pull Request.

When a fleet requires approval, the push runs in the continuation lease after someone approves. The daemon compiles the rules (`rustd/crates/afd_gate/src/policy/egress/write.rs`), and the supervisor enforces them outside the sandbox. A write token inside the sandbox would be readable by every program there, including a dependency's install hook. A repository-scoped write token "can force-push to `main` as easily as it can open a draft Pull Request", in the words of the rules' own module documentation.

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
