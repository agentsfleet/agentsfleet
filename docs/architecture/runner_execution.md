---
type: explanation
audience: contributor
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Runner runtime and sandbox

## What it is

`agentsfleet-runner` is one binary with three entries: `run`, `probe` and `sandbox` ([main](../../rustd/crates/runner/main/src/main.rs)). This page follows a run from the moment the daemon pushes it to the moment its report is acknowledged.

| Part | What it is |
|---|---|
| Supervisor | The trusted `run` process. It holds the stream, the agent loop, model keys and minted tokens |
| Sandbox | One per run, built with bubblewrap. It runs tool calls and nothing else |
| Toolbox | A signed, read-only Debian filesystem image every sandbox mounts as `/` |
| Workspace disk | A per-run writable disk holding `/workspace` and `/tmp` |
| Held sandbox | A finished run's sandbox, frozen and kept for its fleet's next run |
| Network | The runner's assigned policy: the host's network, none, or an allowlist |

## Why it exists

Fleets run tenant programs: build scripts, test suites, package installs and Python the model writes. The sandbox is the only wall between two tenants' processes on one kernel.

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
                 │ one executor connection per run (a socket pair)
 ┌ sandbox: one per run ────────────────────────────────────────────────┐
 │ /            toolbox, read-only                                      │
 │ /workspace   the run's disk, writable                                │
 │ executor: processes on pipes or pseudo-terminals, file calls         │
 │ network per assigned policy · cgroups · seccomp · no capabilities    │
 └──────────────────────────────────────────────────────────────────────┘
```

The executor is a small in-sandbox server: the same binary's `sandbox` entry, bound read-only into the sandbox. It spawns, writes to, reads and kills processes, and it serves the file calls the file tools use.

Every tool call ends exactly once. When a run ends for any reason, the supervisor closes each open call as `interrupted` and kills every command session still open.

### One run, end to end

```text
Down::Run
  ├─ admit      the policy's tools and provider; an unhosted one refuses the run
  ├─ turn       one run per fleet on this runner; a second run for it waits
  ├─ memory     the fleet's memory window, carried on the run
  ├─ admitted   size, egress and checkout plan, before any model call
  ├─ run        the agent loop in the supervisor; sandbox tool calls cross the socket
  │  └─ build   beside it: the fleet's held sandbox, or a fresh one with repositories checked out
  ├─ keep       a processed run's sandbox is frozen and held; any other is killed
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

The runner's crates sit in `rustd/crates/runner/` and import nothing from `daemon/`. No runner crate links a datastore crate ([layer rules](../../rustd/crates/e2e/af_e2e/tests/layers.rs)).

| Crate | Role |
|---|---|
| `guffbolt` | The agent loop as a library: model turns, tool calls, child agents. It knows no run, fleet or task |
| `guffbolt-ai` | The model wires: Chat Completions, Responses, Messages, Gemini and Ollama |
| `af_providers` | Runs a model wire for one run, with its error codes and logs |
| `afr_agent` | Embeds `guffbolt` for a run: prompt, context window, checkpoints, frames and the tool router |
| `afr_tools` | The tool catalog and the verbs that reach `agentsfleetd` |
| `afr_supervisor` | The stream, worker pool, renewals, reports, holds, clones and checkpoint transfer |
| `afr_sandbox`, `afr_executor` | The bubblewrap engine and the in-sandbox server |
| `af_wire` | The one wire format the daemon and runner share |

Model routing and saved model settings belong to [models](models.md#routing-from-saved-settings). The prompt belongs to [capabilities](capabilities.md), pinned memory to [memory](memory.md), and the context window to [context lifecycle](context_lifecycle.md).

### Tool catalog

Every run is offered every published tool and the seven child-agent tools. A run's `ExecutionPolicy.tools` lists them; a tool the catalog does not host refuses the run.

A call to a name outside the policy returns a tool error, and the run continues. A call the daemon refuses returns as `[kind] sentence`, so the model can correct it.

| Tool | Runs in | Notes |
|---|---|---|
| `http_request` | Supervisor, through the run's egress guard | Credentials go only in `Authorization` |
| `web_fetch` | Supervisor or daemon | Hosts named in the run policy are read locally. A public fleet sends other HTTPS pages to Parallel Extract. |
| `notify` | Supervisor | Needs an approved `pushover` secret whose `host` is `api.pushover.net` |
| `web_search` | Daemon, through a fenced run verb | Every model uses the same function tool. A public fleet searches through Parallel Fast first, with Exa as fallback. |
| `memory_store`, `memory_recall`, `memory_list`, `memory_forget` | Supervisor | Only an explicit `core` category is pinned into later prompts |
| `request_access` | Supervisor, through a fenced run verb | A new request pauses the asking turn. An existing pending request or an already approved grant lets the turn continue. |
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

Successful `web_search` and `web_fetch` calls record their sources with the tool result. The dashboard can reload them from the saved turn. Source URLs omit credentials, query strings and fragments. A local read that follows a redirect can also cite the confirmed final origin. A vendor read cites the requested page.

Unread command output is kept as a 512 KiB head and tail, and the middle is counted. A command that never ends returns `interrupted` with its output so far.

A tool that changes the fleet runs only when the person's newest message asks for it. For the two task tools, the daemon also checks who sent the turn.

`git` runs hooks and aliases, so offering `git` offers what `shell` does. The sandbox is the boundary for both.

### Sandbox engines

bubblewrap is the one engine built (`BubblewrapEngine`). Its layers are:

- A fresh user, process, inter-process communication, hostname and mount namespace per run. It adds a network namespace unless the policy is `allow_all`.
- The flags `--cap-drop ALL`, `--disable-userns`, `--clearenv`, `--die-with-parent` and `--new-session`. The last one blocks terminal-input injection.
- `no_new_privs`, then Landlock, then a seccomp filter refusing 12 system calls, including `io_uring`, `ptrace`, `unshare`, `bpf` and `keyctl` ([hardening](../../rustd/crates/runner/afr_sandbox/src/harden/linux.rs)).
- Control group (cgroup) v2 limits on memory, processor, process count and disk I/O, with whole-tree kill.
- A workspace disk that answers `ENOSPC` when full, so `/tmp` never fills the run's memory.

The run's cgroup has two leaves. `sandbox` holds bubblewrap and the executor, and `tenant` holds every process the fleet starts.

The tenant leaf's memory limit sits below the run's, so the out-of-memory killer picks only a tenant process. The call it ended reads `out_of_memory`.

The executor runs as uid and gid 1000 in `/workspace`, started by its fixed path, never through `$PATH`. The supervisor keeps only the capabilities sandbox setup needs.

Every sandbox gets the runner's defaults, and the wire carries no per-run size. The defaults are 2 GiB of memory with no swap, 2 cores and 512 processes. Disk I/O is capped at 200 MiB/s, and the disk holds 4 GiB.

### Sandbox filesystem

bubblewrap builds the sandbox's mounts from a fixed set, in one order ([bubblewrap](../../rustd/crates/runner/afr_sandbox/src/bubblewrap/mod.rs)):

| Inside | From | Mode |
|---|---|---|
| `/` | The verified toolbox image's mount | Read-only |
| `/proc`, `/dev` | A fresh `/proc` and a minimal device tree | As bubblewrap mounts them |
| `/dev/shm` | A private in-memory filesystem, a quarter of the run's memory | Read-write |
| `/tmp`, `/workspace` | Directories on the run's workspace disk | Read-write |
| `/run` | A private in-memory filesystem | Read-write |
| `/opt/agentsfleet/agentsfleet-runner` | The runner binary | Read-only |
| `/etc/hosts`, `/etc/resolv.conf` | The run's rendered files under `allow_list_egress`, or the host's under `allow_all` | Read-only |

No other host file reaches a sandbox. Landlock then narrows writes to `/workspace`, `/tmp`, `/dev/pts` and `/dev/shm`, plus the few devices a shell opens.

### Network egress

Each runner is assigned one of three network policies:

| Policy | What a sandbox can reach |
|---|---|
| `allow_all` | Everything the host reaches; the sandbox shares the host's network namespace |
| `deny_all_egress` | Nothing; its own namespace holds only loopback. A missing assignment gets this too |
| `allow_list_egress` | Only the run's allowlist, through one virtual ethernet pair `afv<slot>` on `10.69.<slot>.0/30` |

Under `allow_list_egress`, the runner builds one nf_tables table, `inet afegress<slot>`, in the host namespace, over netlink. Its forward chain drops port 53, accepts the allowed IPv4 set and drops the rest.

The table lives outside the sandbox, so the sandbox cannot flush it. Each table matches only its own link, so one sandbox's rules never touch another's traffic.

The allowlist starts with the runner's `registry_allowlist`, or the default package registries when that list is empty. It adds the hosts of the fleet's approved secrets.

The supervisor resolves those names to IPv4 and writes them into the sandbox's `/etc/hosts`. Its `/etc/resolv.conf` names no server, so a sandbox cannot tunnel data through Domain Name System (DNS) lookups.

The run is refused when a name fails to resolve, resolves only to IPv6, or the netlink build fails. It is refused too when a secret's host resolves to a private or reserved address.

At boot the runner builds one table and tears it down. A host that fails reports `egress_enforcement: false`, and the daemon marks an `allow_list_egress` runner degraded.

A fleet's two network toggles decide what its HTTP tools reach under that ceiling. `http_egress` is `restricted` or `public`, and `package_domains` adds the package registries; both start off.

Restricted mode admits approved web hosts, approved secrets' hosts, configured package registries and GitHub under an approved GitHub grant. A fleet can ask for a new web host with `request_access {host}`. Approval adds that host to future policies. Credentials require a separate grant. Restricted fleets cannot search or use vendor extraction.

Public mode reaches public HTTPS hosts. Both modes refuse private and reserved addresses on local reads and repeat admission at every redirect. A restricted fleet that encounters a redirect to an unapproved host can request that hostname and retry the page after approval. Search and extraction credentials belong to the daemon and are read from `PLATFORM_ADMIN_WORKSPACE_ID`.

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

`http_request` follows up to 20 redirects by default, and only for a bodyless GET ([redirects](../../rustd/crates/harness/af_egress/src/redirect/mod.rs)). Each hop repeats the egress check, and a change of origin drops every credential header.

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

The supervisor clones repositories with the run's one-hour, repository-scoped token. The fetch sends it in an in-memory header, and the clone's configuration names the plain URL ([clone](../../rustd/crates/runner/afr_supervisor/src/workspace_clone/mod.rs)).

A `${secrets.github.token}` placeholder resolves through `POST /v1/runners/me/credentials/mint`, which reads the fleet's current grants. The egress guard puts the token only into a request's `Authorization` header and masks it in the response.

A revoked grant leaves a live run the token it already has. The next run and the next mint lack it.

`message`, `cron_add`, `cron_update` and `schedule` mask minted tokens before text leaves the runner. The daemon then masks the fleet's static secrets before posting to Slack or storing a schedule.

### Workspace between runs

A run that ends processed leaves its sandbox held ([holds](../../rustd/crates/runner/afr_supervisor/src/holds/mod.rs)). The supervisor freezes its cgroup through `cgroup.freeze`.

The hold is keyed by fleet, workspace, limits, network policy and repository binding. The fleet's next run within the hold window thaws it and continues with the files the last run left.

```text
run ends processed ──► freeze ──► held (≤ 10 min, ≤ 1 per worker)
                                      │
   next run, same fleet and key ──────┼──► thaw ──► runs in the same sandbox
   other key or a failed thaw ────────┼──► destroy ──► fresh sandbox
   deadline · worker needed ·         │
   shutdown · refused report ·        └──► destroy
   fleet archived, deleted or held elsewhere
```

Processes do not survive a hold, because the run's end closes every command session. The host never writes into a held clone, because a tenant-written `.git` could redirect a privileged write.

A turn that moves to another runner restores its files from a [checkpoint](checkpoints.md), not from a hold. The daemon's hold rules are in [runner fleet](runner_fleet.md#capacity-and-sandbox-holds).

### Toolbox

The toolbox is a minimal Debian root filesystem with git, Python 3, Node, ripgrep, jq, curl and uv. It holds no package manager, no Perl and no Go.

`make toolbox-image` builds it with mmdebstrap from pinned Debian snapshots ([manifest](../../scripts/toolbox/manifest.txt)). It ships as a read-only Enhanced Read-Only File System (EROFS) image with LZ4HC compression.

Two builds of one release record are byte-identical. The amd64 build refuses an image above 262,848,512 bytes.

The Continuous Integration (CI) host scans the image with Syft and Grype, and a fixable Critical finding blocks signing. The tool cosign then signs the release manifest, and the runner checks that signature offline.

A host downloads the image at install, never at run time, and keeps the current and previous images. Each run holds the image it runs on.

Before a host reports ready, it authenticates the manifest and opens the image once without following links. It hashes that open file and mounts the same file read-only, so a swapped path never reaches the kernel.

### Sandbox lifecycle

```text
run start
  ├─ mkdir <state>/<run_id>            mode 0711, never reused
  ├─ sparse workspace.img 4 GiB → ext4 → loop mount with direct I/O
  ├─ cgroup: sandbox and tenant leaves with the run's limits
  ├─ bwrap: namespaces, mounts, then no_new_privs → Landlock → seccomp → executor
  ├─ allow_list_egress only: veth pair and host nf_tables table
  │            … the turn runs …
  └─ processed: frozen and held; otherwise killed before the report
     after the report: egress removed → cgroup removed → unmount → image and directory removed
```

The supervisor waits at most 10 seconds for the executor to answer before refusing the run. Building the engine sweeps every leftover run directory, egress table and virtual link a crashed runner left.

The storage home is `/var/lib/agentsfleet-runner` unless `RUNNER_STORAGE_HOME` names another root. It holds the toolbox images, per-run sandboxes and one git mirror per workspace and repository.

| Question | Signal |
|---|---|
| How long does a sandbox take to build? | `agentsfleet_runner_sandbox_start_duration_seconds` |
| Did a tool call wait on the build? | `agentsfleet_runner_sandbox_wait_duration_seconds`; `sandbox_wait_ms` on `run_completed` |
| How often does a build fail, and where? | `agentsfleet_runner_sandbox_builds_total`, by outcome |
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

- bubblewrap is the only engine. The Firecracker microVM engine is not built, and its review lives in [the sandbox evaluation](../reviews/space-sandbox/sandbox_evaluation.md).
- The browser tools (`browser`, `browser_open`, `screenshot`) answer with a code. Chromium's own sandbox cannot start inside bubblewrap.
- Tenants share the host kernel. A kernel privilege-escalation bug escapes every namespace sandbox on the host.
- Nothing checks the sum of sandbox limits against the host. At 64 workers the default limits reach 128 GiB of memory, and no host disk is reserved for holds.
- The operator's extra binds on the assigned policy are stored and shown, but no runner binds them.
- Warm slots, sandboxes started before a run arrives, are built but used only by tests.
- An ordinary run cannot push to a repository; only a durable task's `propose_change` publishes.
- The address set matches destinations only. The default registries share front ends with other sites, so a hard boundary needs a mirror the operator owns.

## Related pages

- [Runner fleet](runner_fleet.md): registration, policy, tiers and failure recovery
- [Runner protocol](runner_protocol.md): the stream, leases and report delivery
- [Checkpoints](checkpoints.md): what a replacement runner restores
- [Access](user_flow.md#access): where a grant and its secret hosts come from
- [Configuration reference](config_reference.md): runner keys such as `[sandbox]`, `[tools]` and `[egress]`
- [Observability](observability.md): runner metrics and logs
