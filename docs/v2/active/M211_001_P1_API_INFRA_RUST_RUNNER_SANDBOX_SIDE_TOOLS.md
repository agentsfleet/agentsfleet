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

# M211_001: The sandbox-side tools — shell and exec sessions, git on a supervisor-made clone, the seven file tools and apply_patch, image, browser and screenshot — run inside the lease's sandbox through the executor, on a toolbox that carries Chromium and that the host admits by descriptor

**Prototype:** v2.0.0
**Milestone:** M211
**Workstream:** 001
**Date:** Oct 02, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the half of the harness where a fleet runs code, reads a repository and looks at a page; without it the fleets Indy plans beyond the four bundles cannot run on Rust
**Categories:** API, INFRA
**Batch:** B1 — the milestone's one ready Pull Request; the nested loops (`delegate`, `spawn`, M211_002) fold into it
**Branch:** `feat/m211-sandbox-tools-and-nested-loops`
**Baseline revision:** `b0138d7b3124b871668f07e2361dba923bc774d2`
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M210_001 (the bubblewrap engine, the executor's `process/*` and `fs/*` calls, the toolbox build) · M210_002 (the catalog, the router, the `Sandbox` runtime) · the six toolbox spikes in Discovery, each run and recorded before EXECUTE
**Provenance:** LLM-drafted (Claude Fable 5.1, Oct 02, 2026) from `docs/architecture/runner_execution.md` §"Tool catalog" and Indy's decision that the runner carries every published tool; Codex at `~/Projects/oss/rs/codex` `2e5fea64e`
**Canonical architecture:** `docs/architecture/runner_execution.md` §"Tool catalog", §Process model, §Toolbox, §Repository writes; `docs/architecture/runner_fleet.md` §"The sandbox filesystem contract"

---

## Overview

**Goal (testable):** `test_shell_runs_inside_the_sandbox_with_exit_code` — on the kernel lane, a lease whose policy lists `shell` runs `sh -c 'echo hi; exit 3'` through the executor inside its bubblewrap sandbox; the call completes `failed` with `exit_code: 3`, `output_head` `hi`, the process held no capability and could not reach the network; and the same lease's `git`, `file_edit_hashed`, `apply_patch`, `image`, `browser_open` and `screenshot` calls each complete with the outcome their Dimension names.
**Problem:** M210_002 puts every sandbox-side name in the catalog with a stub handler, so a policy listing `shell`, `git`, a `file_*` tool, `apply_patch`, `image` or a browser tool refuses the lease. Those are the tools the published page promises (`~/Projects/docs/fleets/tools.mdx`) and the ones the Zig runner wires (`src/runner/engine/tool_bridge_registry.zig`). A fleet that must run a test suite, read a repository at a verified head, or look at a dashboard cannot move to Rust without them.
**Solution summary:** `afr_tools::sandbox` implements each sandbox-side handler over the executor connection M210_001 §4 provides: `shell` as one process, `exec_command` and `write_stdin` as Codex's unified-exec sessions on a pseudo-terminal, `git` on a clone the supervisor makes on the host side with the read token before the lease starts, the seven `file_*` tools and `apply_patch` over `fs/*` under `/workspace`, `image` as a file the supervisor attaches to the next model turn, and the three browser tools as headless Chromium from the toolbox driven over the Chrome DevTools Protocol (CDP) through the process's extra pipes. The toolbox manifest gains Chromium, Node, uv, ripgrep, jq and the GitHub command-line tool, and its build gains Debian's updates and security snapshots. The host admits the toolbox by descriptor: a signed manifest, an image staged and published atomically, opened once, and attached read-only through that descriptor, which replaces hashing the loop device after the mount. Every handler inherits the sandbox: no capability, no network beyond loopback, no write outside `/workspace`.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): sandbox-side tools — shell, exec sessions, git, files, apply_patch, image, browser
- **Intent (one sentence):** A fleet can run commands, work a repository, edit files, look at an image and drive a page, all inside its own sandbox, with the same visibility in the thread as every other call.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` — §"Tool catalog" (each tool's runtime and needs), §Toolbox, §Repository writes; the Decisions table is binding.
2. `docs/v2/done/M210_001_P1_API_INFRA_RUST_RUNNER_SUPERVISOR_SANDBOX_FOUNDATION.md` §4 and §5 — the executor's methods and the toolbox build these handlers stand on; Discovery "Sandbox crate choices" for why loop mounts avoided `bindgen`.
3. `rustd/crates/afr_sandbox/src/toolbox.rs` — the path-based `mount -o loop` and the post-mount re-hash that §8 replaces.
4. `docs/v2/done/M210_002_P1_API_INFRA_RUST_RUNNER_AGENT_LOOP_AND_HOSTED_TOOLS.md` §1 — the catalog and router these handlers plug into.
5. `docs/architecture/runner_fleet.md` — §"The sandbox filesystem contract": what a fleet may see and write.
6. https://github.com/openai/codex/tree/2e5fea64eefcaa19f48458b2386011b619f69c70/codex-rs — `core/src/tools/handlers/unified_exec/` (`exec_command`, `write_stdin`, sessions, `yield_time_ms`, `max_output_tokens`), `core/src/tools/handlers/view_image.rs`, `apply-patch/` (the patch grammar and parser, copied at this commit with its NOTICE).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_tools/src/sandbox/` (`shell.rs`, `exec_session.rs`, `git.rs`, `files.rs`, `hashed.rs`, `apply_patch.rs`, `image.rs`, `browser.rs`, `cdp.rs`, `screenshot.rs`, `error.rs`) | CREATE | One handler per tool over the executor connection |
| `rustd/crates/afr_tools/vendor/apply_patch/` + `NOTICE` | CREATE | Codex's patch parser at the pinned commit, with its licence notice |
| `rustd/crates/afr_tools/src/catalog.rs` | EDIT | The sandbox-side entries point at real handlers |
| `rustd/crates/afr_executor/src/protocol.rs`, `rustd/crates/afr_executor/src/process.rs`, `rustd/crates/afr_executor/src/fs.rs` | EDIT | `process/spawn` gains `extra_pipes`; `fs/*` gains `append`, `delete`, `stat` with a content hash |
| `rustd/crates/afr_supervisor/src/workspace_clone.rs` | CREATE | The host-side clone with the read token, into the workspace disk, before the lease starts |
| `rustd/crates/afr_providers/src/image_input.rs` | CREATE | Image content on the next turn for Messages and Responses |
| `scripts/toolbox/manifest.txt`, `scripts/toolbox/build.sh` | EDIT | Chromium as Debian's `chromium-headless-shell` with fonts, Node, uv, ripgrep, jq, the GitHub command-line tool; Debian's updates and security snapshots; pinned EROFS features; the release manifest beside the image |
| `rustd/crates/afr_sandbox/src/toolbox.rs` + `toolbox/` (`manifest.rs`, `stage.rs`, `loop_device.rs`, `adopt.rs`, `holds.rs`), `bubblewrap_engine.rs`, `warm_slots.rs`, `rustd/crates/afr_sandbox/Cargo.toml`, `rustd/Cargo.lock` | EDIT / CREATE | Admission by descriptor, adoption by identity, holds and retention; the path mount and the post-mount re-hash go |
| `rustd/crates/afr_sandbox/tests/kernel_lane.rs`, `rustd/crates/afr_tools/tests/` | EDIT / CREATE | Real-sandbox proofs for every handler; unit proofs for parsers and routing |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | EDIT | One bundle that runs a test suite with `shell` and edits with `apply_patch`, on the unsandboxed engine |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (subcommand allowlists, caps, pipe numbers and CDP method names are constants), OWN (one owner per process, session, Chromium instance and clone), FLS (drain every process stream and CDP pipe on every exit path), TIM (session yields, command timeouts and the kill grace are explicit), NTP (patch text and CDP replies narrowed at their parse boundary), OBS, ERR-RS, TST-NAM, TCF, NDC.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — one `ErrorKind` per crate; a refused subcommand or path carries its code to the model.
- `dispatch/write_shell.md` — the toolbox build script: quoted expansions, temp-file cleanup.
- `docs/LOGGING_STANDARD.md` — never log a command's output, a file's content or a page's text; log ids, codes and counts.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | `error_shell!` and `error_lifts!` only |
| UFS / LOGGING / MILESTONE-ID | yes | Constants per concern; scoped events; no milestone identifiers in source |
| File & Function Length (≤350/≤50/≤70) | yes | One handler per file; the vendored parser is exempt by its own directory and NOTICE |
| Architecture consult | yes | `runner_execution.md` §"Tool catalog" names every runtime here; drift is fixed in the same commit |
| CI/CD edit guard | no | The kernel lane and its job exist from M210_001 |

## Prior-Art / Reference Implementations

- **Reference:** Codex `unified_exec` (`2e5fea64e`) — `exec_command { cmd, workdir, tty, yield_time_ms, max_output_tokens }` opens a session, `write_stdin { session_id, chars, yield_time_ms }` feeds it; output is returned per yield and the session outlives the call. Ported onto the executor's `process/*`.
- **Reference:** Codex `apply-patch` — the patch grammar, parser and UTF-8-safe application. Copied at the pinned commit with its NOTICE; applied through `fs/read` and `fs/write`, never by a process.
- **Reference:** Codex `view_image` — a path becomes image content on the next turn. Ported.
- **Reference:** `rustd/crates/afd_credential/src/credential/github/request.rs` — the read-only, one-hour, repository-scoped token the supervisor clones with.
- **Reference:** loop(4), https://man7.org/linux/man-pages/man4/loop.4.html — `LOOP_CONFIGURE` (Linux 5.8) attaches a backing descriptor read-only in one call, and `LOOP_GET_STATUS64` reports the backing device and inode; both are declared by hand over `libc::ioctl`, so no `bindgen`.
- **Reference:** Chromium's `--remote-debugging-pipe` — CDP over file descriptors 3 and 4, which is what lets the supervisor drive a browser that has no network and no socket of its own.

## Sections (implementation slices)

### §1 — Processes: `shell` runs one command, exec sessions keep one alive

`shell { command, timeout_ms }` runs `sh -c` through `process/spawn` with `cwd` `/workspace`, pipes, and the lease's limits; the call completes with the exit code, `succeeded` on 0 and `failed` otherwise, output edges cut as M210_001 §4 cuts them. `exec_command { cmd, workdir, tty, yield_time_ms, max_output_tokens }` opens a session on a pseudo-terminal and returns what arrived before the yield; `write_stdin { session_id, chars, yield_time_ms }` feeds it. A session ends when its process exits, when the run ends, or when the lease's session cap is reached. A timeout kills the process group, TERM then KILL, and the call completes `failed` with `timed_out`.

- **Dimension 1.1** — `shell` runs inside the sandbox and reports its exit code and edges → Test `test_shell_runs_inside_the_sandbox_with_exit_code`
- **Dimension 1.2** — `exec_command` opens a session; `write_stdin` feeds it; output returns across two yields → Test `test_exec_session_survives_across_calls`
- **Dimension 1.3** — A command past `timeout_ms` is killed with its children and reported `timed_out` → Test `test_shell_timeout_kills_the_group`
- **Dimension 1.4** — A process spawned by `shell` has no capability and cannot connect outside loopback → Test `test_shell_process_inherits_the_sandbox`
- **Dimension 1.5** — Every session still open at run end is closed and its call `interrupted` → Test `test_sessions_close_at_run_end`

### §2 — `git` works a clone the supervisor made

Before a lease with `git` or `shell` in its policy starts, the supervisor clones the bound repository at `repository_base` into the workspace disk on the host side, with the read token from the mint verb and the per-tenant object cache, so the token never enters the sandbox. `git { args }` runs git inside the sandbox on that clone; `push`, `fetch`, `pull`, `remote` and `clone` are refused with a code, because the sandbox has no network and the push is `propose_change` (the outage toolkit spec).

- **Dimension 2.1** — The clone sits at the base branch's head with the cache warm → Test `test_supervisor_clones_at_base_with_cache`
- **Dimension 2.2** — `status`, `log`, `diff`, `checkout -b`, `commit` run inside the sandbox → Test `test_git_tool_runs_local_commands`
- **Dimension 2.3** — `push`, `fetch`, `pull`, `remote add` and `clone` are refused with a code → Test `test_git_tool_refuses_network_subcommands`
- **Dimension 2.4** — The read token appears nowhere inside the sandbox, including the clone's config → Test `test_read_token_never_enters_the_sandbox`

### §3 — The file tools and `apply_patch`

`file_read`, `file_write`, `file_append`, `file_delete`, `file_edit` run over `fs/*` under `/workspace`. `file_read_hashed` returns the content with its SHA-256; `file_edit_hashed` refuses when the file's current hash differs from the one supplied, so a stale edit never lands. `apply_patch { patch }` parses Codex's grammar, applies every hunk through `fs/read` and `fs/write`, refuses any path outside `/workspace`, and completes with `+N −M` for the thread's diff cell.

- **Dimension 3.1** — Each of the seven file tools does its operation under `/workspace` → Test `test_file_tools_operate_under_workspace`
- **Dimension 3.2** — `file_edit_hashed` with a stale hash is refused and changes nothing → Test `test_hashed_edit_refuses_stale_hash`
- **Dimension 3.3** — `apply_patch` applies add, update and delete hunks and reports `+N −M` → Test `test_apply_patch_applies_codex_grammar`
- **Dimension 3.4** — A patch or a file path escaping `/workspace` is refused → Test `test_file_tools_refuse_path_escape`

### §4 — `image` shows the model a file

`image { path }` reads the file through `fs/read`, caps it at `IMAGE_MAX_BYTES`, and attaches it as image content to the next model turn on Messages and Responses; on a provider without image input the call is a tool error with a code. The frame carries the path and the byte count, never the bytes.

- **Dimension 4.1** — An image under the cap reaches the next turn as image content → Test `test_image_attaches_to_next_turn`
- **Dimension 4.2** — An oversize file, a non-image and a text-only provider each refuse with their code → Test `test_image_refusals_carry_codes`

### §5 — Browser: Chromium from the toolbox, driven over its pipes

`browser_open { url }` starts Chromium headless from the toolbox through `process/spawn` with `--remote-debugging-pipe` and `extra_pipes: [3, 4]`, then speaks CDP over those pipes to navigate. `browser { action, selector?, text? }` performs `click`, `type`, `text`, `wait` on the open page. `screenshot` captures the page as PNG and attaches it to the next model turn as image content, as §4 does. Chromium runs inside the sandbox under its limits; with the sandbox's network at loopback only, a page beyond loopback fails until the sandbox allowlist lands, and the kernel lane serves its pages on loopback. Chromium exits with the lease. Chromium keeps its own sandbox: `--no-sandbox` is never passed. Spike S1 settles whether it runs inside ours; if it cannot, Indy decides before §5 starts.

- **Dimension 5.1** — `browser_open` loads a loopback page and `text` returns its content → Test `test_browser_opens_and_reads_a_page`
- **Dimension 5.2** — `click` and `type` drive a form and the page reflects it → Test `test_browser_drives_a_form`
- **Dimension 5.3** — `screenshot` returns a PNG that reaches the next turn → Test `test_screenshot_reaches_next_turn`
- **Dimension 5.4** — Chromium holds no capability and a navigation beyond loopback fails → Test `test_browser_inherits_the_sandbox`
- **Dimension 5.5** — Chromium is gone after the lease → Test `test_browser_exits_with_the_lease`

### §6 — The toolbox carries the tools

The manifest gains Chromium, as Debian's `chromium-headless-shell` rather than the full `chromium` package, with fonts, Node, Python 3 with uv, ripgrep, jq, curl and the GitHub command-line tool, pinned; a binary Debian does not ship is pinned by URL and SHA-256. The build fetches main, updates and security at one snapshot timestamp, pins its EROFS features, and writes the release manifest `runner_execution.md` §Toolbox lists beside the image; it stays reproducible and content-addressed (M210_001 §5). VERIFY records in Discovery the image's size and, separately, host staging, sandbox readiness, the first useful command and the first screenshot, on cold and warm cache, at p95 and p99, with four leases running at once.

- **Dimension 6.1** — Under the production policy, uv installs a locked project, `node --test` passes, git commits and Chromium screenshots a page, and two builds hash the same → Test `test_toolbox_carries_the_tools`

### §7 — A bundle runs code

The integration lane adds a fixture bundle that runs a repository's test suite with `shell`, edits a file with `apply_patch` and commits with `git`, on the unsandboxed engine, so the whole path is proven against the real daemon on any machine; the kernel lane proves the same bundle in the real sandbox.

- **Dimension 7.1** — The bundle runs the suite, applies a patch, commits, and the thread shows `Ran`, `Edited (+N −M)` and the exit codes → Test `test_code_running_bundle_roundtrip`

### §8 — The host admits the toolbox by descriptor

Admission runs in the order `runner_execution.md` §Toolbox gives, before the host reports ready. The manifest's signature is checked against the public key the runner is built with (`TOOLBOX_RELEASE_PUBLIC_KEY`; a fixture key here, the release key in M213_001), then its architecture, length, digest, EROFS features and runner versions. The image is staged under the runner's state, verified, synced and published with `rename`; the published file is opened once with `O_NOFOLLOW`, checked to be a regular file of the manifest's length, hashed through that descriptor, attached read-only with `LOOP_CONFIGURE`, and its loop device is mounted `ro,nosuid,nodev`. A mount found at startup is adopted only when it is EROFS with those flags on a loop device whose backing device and inode equal the admitted descriptor's; anything else is detached. Each image counts the leases and warm slots holding it; the host keeps the current and previous images plus any still held.

- **Dimension 8.1** — Across 1,000 admissions with the image path swapped between steps, only the authorized image is ever mounted → Test `test_toolbox_admission_survives_path_swap`
- **Dimension 8.2** — A bad signature, a wrong length or architecture, and an interrupted stage are each refused before any mount → Test `test_toolbox_admission_refusals`
- **Dimension 8.3** — A mount at the digest's directory is adopted only when its type, flags and backing inode match; otherwise it is detached and remounted → Test `test_toolbox_adoption_checks_identity`
- **Dimension 8.4** — A held image is never unmounted, the current and previous are kept, and a runner killed mid-admission admits cleanly on restart → Test `test_toolbox_holds_and_retention`

## Interfaces

```
shell         { command, timeout_ms }                            → exit_code, edges
exec_command  { cmd, workdir?, tty?, yield_time_ms?, max_output_tokens? } → { session_id, output, exited? }
write_stdin   { session_id, chars, yield_time_ms? }              → { output, exited? }
git           { args: [..] }                                     → exit_code, edges; network subcommands refused
file_read / file_read_hashed { path }  file_write { path, content }  file_append { path, content }
file_delete { path }  file_edit { path, old_text, new_text }  file_edit_hashed { path, hash, old_text, new_text }
apply_patch   { patch }                                          → { added, removed, files }
image         { path }                                           → image content on the next turn
browser_open  { url }  browser { action: click|type|text|wait, selector?, text? }  screenshot {}

Executor additions: process/spawn { …, extra_pipes: [fd] } · fs/append · fs/delete · fs/stat → { size, sha256 }
Constants: SESSIONS_PER_LEASE_MAX · SHELL_TIMEOUT_MS_DEFAULT · IMAGE_MAX_BYTES · GIT_REFUSED_SUBCOMMANDS · TOOLBOX_KEEP_RELEASES (2) · TOOLBOX_RELEASE_PUBLIC_KEY
Toolbox manifest: { arch, length, sha256, erofs_features, runner_versions, packages, vendored } + signature
Admission: verify manifest → stage → fsync → rename → open(O_NOFOLLOW) → fstat → SHA-256(fd) → LOOP_CONFIGURE(fd, read-only) → mount ro,nosuid,nodev → ready
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Command times out | Hung process | Group killed TERM then KILL; call `failed`, `timed_out` (Dimension 1.3) |
| Process tries the network | Tenant code | Connect fails inside the namespace; the command's own error is the output (Dimension 1.4) |
| Stale hashed edit | Two edits raced | Refused; file unchanged (Dimension 3.2) |
| Path escape | `..` or a symlink | Refused with a code; nothing read or written (Dimension 3.4) |
| Network git subcommand | Fleet asks to push | Refused with a code naming `propose_change` (Dimension 2.3) |
| Clone fails | Mint refused, repository gone | Lease refused before any tool runs; capability report untouched |
| Chromium cannot start | Missing from toolbox, limits | Browser tools refuse with a code; the rest of the run continues |
| Image too large or not an image | Fleet picks a wrong file | Refused with a code (Dimension 4.2) |
| Run ends with sessions open | Kill, timeout, lease end | Sessions closed, calls `interrupted` (Dimension 1.5) |
| Image path swapped mid-admission | A buggy installer or host-side tampering | The hashed descriptor is what gets attached; the swapped file is never parsed (Dimension 8.1) |
| Toolbox refused at admission | Bad signature, wrong architecture, short download | Not admitted; the capability report says no toolbox; leases refused (Dimension 8.2) |
| Foreign mount at the digest's directory | A crashed runner or an operator | Detached and remounted from the admitted descriptor (Dimension 8.3) |
| Chromium needs a user namespace or a setuid helper | Our sandbox disables both | Browser tools refuse with a code; `--no-sandbox` is never added; Indy decides from spike S1 |

## Invariants

1. Every sandbox-side handler runs through the executor inside the lease's sandbox; none spawns a process on the host (Dimensions 1.4, 5.4).
2. The read token and every credential stay on the host side; the clone carries none (Dimension 2.4).
3. No handler reads or writes outside `/workspace` (Dimension 3.4).
4. A process, session or Chromium started for a lease is gone when the lease ends (Dimensions 1.5, 5.5).
5. No toolbox image is reopened by path after its descriptor is hashed, and every toolbox mount sits on a loop device backed by an admitted file (Dimensions 8.1, 8.3).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `process_timed_out` (runner log, warn) | ops | A command passes its timeout | lease id, call id, timeout, pid count killed | No command text or output | `test_shell_timeout_kills_the_group` |
| `git_subcommand_refused` (runner log, info) | ops | A network subcommand is asked for | lease id, call id, subcommand | No arguments beyond the subcommand | `test_git_tool_refuses_network_subcommands` |
| `browser_started` / `browser_exited` (runner log, info) | ops | Chromium starts or ends | lease id, milliseconds alive, exit status | No URL, no page text | `test_browser_exits_with_the_lease` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | kernel | `test_shell_runs_inside_the_sandbox_with_exit_code` | `echo hi; exit 3` → `failed`, `exit_code` 3, head `hi` |
| 1.2 | integration | `test_exec_session_survives_across_calls` | `cat` session, two `write_stdin` → both echoed, same `session_id` |
| 1.3 | kernel | `test_shell_timeout_kills_the_group` | `sleep 60 & sleep 60`, 500 ms → `timed_out`, 0 processes left |
| 1.4 | kernel | `test_shell_process_inherits_the_sandbox` | `cat /proc/self/status` → CapEff 0; `curl 1.1.1.1` → fails |
| 1.5 | unit | `test_sessions_close_at_run_end` | 2 open sessions, run ends → 2 `interrupted`, 0 sessions |
| 2.1 | integration | `test_supervisor_clones_at_base_with_cache` | fake GitHub repository → `/workspace` at base head; second clone fetches 0 new objects |
| 2.2 | kernel | `test_git_tool_runs_local_commands` | `status`, `checkout -b x`, `commit` → exit 0 each |
| 2.3 | unit | `test_git_tool_refuses_network_subcommands` | `push`, `fetch`, `pull`, `remote add`, `clone` → refused, code set |
| 2.4 | kernel | `test_read_token_never_enters_the_sandbox` | grep token over `/workspace/.git` and env → 0 hits |
| 3.1 | integration | `test_file_tools_operate_under_workspace` | write, append, read, edit, delete → each observed through `fs/*` |
| 3.2 | unit | `test_hashed_edit_refuses_stale_hash` | hash of old content → refused; current hash → applied |
| 3.3 | unit | `test_apply_patch_applies_codex_grammar` | add + update + delete hunks → files as expected, `+3 −1` |
| 3.4 | unit | `test_file_tools_refuse_path_escape` | `../etc/x`, symlink → refused, nothing touched |
| 4.1 | unit | `test_image_attaches_to_next_turn` | 40 KiB PNG → next request carries image content |
| 4.2 | unit | `test_image_refusals_carry_codes` | 20 MiB file, `.txt`, chat provider → three distinct codes |
| 5.1 | kernel | `test_browser_opens_and_reads_a_page` | loopback page "hello" → `text` returns `hello` |
| 5.2 | kernel | `test_browser_drives_a_form` | `type` into `#q`, `click` `#go` → page shows the query |
| 5.3 | kernel | `test_screenshot_reaches_next_turn` | screenshot → PNG header, image content on next request |
| 5.4 | kernel | `test_browser_inherits_the_sandbox` | Chromium CapEff 0; navigate `http://1.1.1.1` → error |
| 5.5 | kernel | `test_browser_exits_with_the_lease` | lease ends → no chromium process in the cgroup |
| 6.1 | kernel | `test_toolbox_carries_the_tools` | `uv sync` from a loopback index, `node --test`, `git commit`, a loopback screenshot → exit 0 each, policy unchanged; two builds → same SHA-256 |
| 7.1 | integration | `test_code_running_bundle_roundtrip` | fixture bundle → suite ran, patch applied, commit made, trace rows for each |
| 8.1 | kernel | `test_toolbox_admission_survives_path_swap` | 1,000 runs, a racer renames a decoy over the path at each step → backing inode always the admitted one; 0 decoy mounts |
| 8.2 | unit | `test_toolbox_admission_refusals` | wrong key, length off by one, `aarch64` manifest on amd64, half-written stage → four distinct codes, 0 mounts |
| 8.3 | kernel | `test_toolbox_adoption_checks_identity` | ext4 at the digest's directory, EROFS without `nosuid`, a loop of another file → each detached; a matching mount → adopted |
| 8.4 | unit | `test_toolbox_holds_and_retention` | 2 holds, 1 released → still mounted; a third image → oldest unheld unmounted; kill during stage → restart admits, no stage left |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Every sandbox-side tool runs inside the real sandbox, on an admitted toolbox (§1–§6, §8) | `make test-runner-kernel` | exit 0 | P0 | |
| R2 | A code-running bundle round-trips against the real daemon (§7) | `make test-integration-rustd && grep -c "fn test_code_running_bundle_roundtrip(" rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | 1 | P0 | |
| R3 | Parsers and refusals hold (§2–§4) | `cargo test --manifest-path rustd/Cargo.toml -p afr_tools sandbox` | exit 0 | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R5 | Toolbox admission refuses what it must (§8) | `cargo test --manifest-path rustd/Cargo.toml -p afr_sandbox toolbox` | exit 0 | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| grep -v '/vendor/' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted. The stub sandbox-side handlers from M210_002 §1 are replaced in place, not left beside the real ones.

## Out of Scope

- `delegate` and `spawn` as nested loops — M211_002.
- The push: `propose_change`, the supervisor's write under the daemon's rules, and the Codex and Claude Code engines — the outage toolkit spec.
- Network inside the sandbox: the allowlist, a page beyond loopback for the browser, `git fetch` from inside — the sandbox allowlist spec. Until then `http_request` in the supervisor is a fleet's reach.
- Keeping screenshots and other binaries for the thread — the artifacts spec; here an image reaches the model, and the thread's cell says what was captured.
- Workspaces carried between leases in R2 — a later spec; this clone is per lease.
- Signing the release, the SBOM, the scan and the offline bundle — M213_001. Revoking a compromised toolbox — deferred (Discovery). dm-verity and the Firecracker boot wait on spikes S2 and S3; tool packs wait on a fleet that needs one.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet told "the build is red on `dev`" runs the suite in its sandbox, the thread shows `Ran bun test` with the failing line, `Edited retry.ts (+2 −1)` with the diff, and a commit, and the person reads it like a Codex transcript.
2. **Preserved user behaviour** — Tool names and arguments stay as the published page states them; a fleet that never lists a sandbox-side tool still starts no sandbox.
3. **Optimal-way check** — Every handler is a thin client of the executor M210_001 already proves; the one new mechanism is Chromium over pipes, chosen because it needs no socket and no network inside the sandbox.
4. **Rebuild-vs-iterate** — Rebuild, by Indy's decision: "The port is a fresh port".
5. **What we build** — Eleven handlers, three executor methods and one spawn field, the host-side clone, image input for two providers, a larger toolbox admitted by descriptor, one code-running bundle.
6. **What we do NOT build** — Nested loops, the push, network in the sandbox, artifacts, carried workspaces (see Out of Scope).
7. **Fit with existing features** — Plugs into M210_002's catalog and router; the thread's `exec_command` and edit cells (M209_002 §3, §5) render these outcomes unchanged.
8. **Surface order** — API first; the thread already renders command and edit cells.
9. **Dashboard restraint** — N/A — no user surface; an unavailable browser is a red cell with its code.
10. **Confused-user next step** — N/A — no user surface; a refused `git push` names `propose_change`, and a page beyond loopback names the allowlist.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** all sandbox-side handlers in one workstream because they share one mechanism (the executor connection) and one proof lane (the kernel lane), and because a fleet that runs code needs shell, files and git together.
- **Alternatives considered:** running the browser in the supervisor (rejected: tenant-driven navigation belongs inside the boundary); a CDP WebSocket into the sandbox (rejected: a socket the sandbox listens on is a surface, pipes are not); applying patches with a process (rejected: the parser is pure and `fs/*` keeps every write auditable); cloning from inside the sandbox (rejected: the token would enter it); keeping `mount -o loop` behind a second hash (rejected: the kernel parses the file before that hash runs).
- **Patch-vs-refactor verdict:** this is a **patch** because the catalog, router, executor and sandbox exist; each handler is an addition.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "i need all the tools … must be build on the sandbox, the sandbox isnt just a sandbox but a harness that decide to operate like codex so that is critical to realize the fleets i plan to use"; chose "Supervisor loop, sandbox tools". Earlier: "Yes bubblewrap first, and firecracker next."
- **Toolbox review** — Tarzy reviewed the toolbox design on Oct 03, 2026; Indy chose "Approve as classified (Recommended)" for the triage that `runner_execution.md` §Toolbox and its Decisions rows record. Here: the pinned build (§6), the split measurements, admission (§8). M213_001: signing, the SBOM, the scan, the offline bundle.
- **Spikes** — one day each, run and recorded here before EXECUTE; a failed criterion returns to Indy before the Section it gates.
  - **S1 Full toolset under the production policy** — in one lease with today's bubblewrap flags, Landlock and seccomp: uv installs a locked project from a loopback index, `node --test` passes, git commits, `chromium-headless-shell` takes CDP over `--remote-debugging-pipe` and screenshots a loopback page, `codex --version` and `claude --version` run. Pass: all succeed with no `--no-sandbox`, no user namespace, no setuid helper. Gates §5 and §6.
  - **S2 dm-verity beneath EROFS** — on a disposable image with a fixed salt and recorded geometry, corrupt one unread data block and one metadata block. Pass: each read fails with `EIO`, nothing panics, and a healthy image runs S1's workload within 10% of its p99 without dm-verity.
  - **S3 Firecracker boots the exact image** — on a `/dev/kvm` host with a pinned guest kernel: boot, connect over vsock, run a command, write `/workspace`, fail to write `/`, shut down cleanly; record boot latency and memory. Pass: all six, before M213_001 fixes the artifact layout.
  - **S4 Unprivileged build** — `mmdebstrap --mode=unshare` on two clean builders. Pass: both digests equal each other and the root-mode build's.
  - **S5 Sandbox boundary** — inside a lease: count inherited descriptors; call `clone` and `clone3` with namespace flags, `setns` on `/proc/1/ns/*`, and an i386 call on amd64. Pass: only the executor's own descriptors are open and every call is refused.
  - **S6 Writable-state exhaustion** — four leases fill their workspace disks and `/tmp` at once. Pass: each gets `ENOSPC`, the host keeps its free-space reserve, and loop-device I/O shows in each lease's cgroup `io.stat`.
- **Browser** — Indy (in-session, Oct 03, 2026): "Yes stick to chromium then", choosing Debian's `chromium-headless-shell` over the full package and over Lightpanda. Lightpanda renders no pixels (its PNG is a text-layout dump), speaks CDP over WebSocket only, is beta and AGPL-3.0, which the offline bundle would distribute. Headless shell is the same engine without the GTK3 stack: 222.4 MB installed against 319.8 MB for `chromium` on amd64 (packages.debian.org, trixie, 154.0.8037.92); S1 proves its pipe and screenshot, and §6 records the image-size delta.
- **Network inside the sandbox** — M210_001 §3 leaves the sandbox at loopback only, so the browser is proven on loopback here and reaches real pages when the allowlist spec lands; recorded so the limit is read as sequencing, not design.
- **Agent defaults** — `sh -c` for `shell`; a session cap per lease; the five refused git subcommands; SHA-256 for the hashed file tools; PNG for screenshots; Chromium over `--remote-debugging-pipe`; the loop ioctls declared by hand over `libc`; cosign key-pair signatures checked with the `p256` crate as Elliptic Curve Digital Signature Algorithm (ECDSA) P-256 over SHA-256, confirmed against `cosign verify-blob` at PLAN.
- **Metrics review** — No analytics or funnel playbook update required: no user surface; three operator log events added.
- **Skill-chain outcomes** — pending.
- **Deferrals** —
  > Indy (2026-10-03 13:51): "I dont want to focus on revocation of a compromised toolbox, first is to get the toolbox working" — context: revoking a compromised toolbox digest, from Tarzy's review; left out of this spec and M213_001.
