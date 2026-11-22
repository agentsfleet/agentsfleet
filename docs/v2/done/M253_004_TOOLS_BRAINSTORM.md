# M253_004 §10: the tools brainstorm

Prepared Nov 22, 2026 for Indy's sign-off. Every proposal here is **Aiwa's call**; Indy decides each one.
His verbatim sign-off goes in the spec's Discovery. Signed-off changes land in this milestone's follow-up Pull Request (PR). MCP below is the Model Context Protocol.

Sources were read on branch `feat/m253-one-harness-for-setup-and-fleets`. Paths are shortened as follows:

- `tools/…` is `rustd/guffbolt/src/tools/…`.
- `afr_tools/…` is `rustd/crates/runner/afr_tools/src/…`.
- `pi:…` is `~/Projects/oss/js/pi/packages/coding-agent/src/core/tools/…`, unless it names another pi package.
- `codex:…` is `~/Projects/oss/rs/codex/codex-rs/…`.

## Decisions for Indy

Each one is a yes or a no. Aiwa's call is yes on all ten.

1. **`file_edit` refuses ambiguous text and takes several edits**, as pi's `edit` does. Today it silently replaces the first match.
2. **guffbolt ships two named default sets**, `coding` and `read_only`, as pi's `DEFAULT_TOOL_NAMES` and `createReadOnlyTools` do.
3. **agentsfleet publishes `ls`, `find` and `grep`.** §9 already runs their search on the executor, yet no fleet can name them.
4. **`grep` gains `context` lines and a 500-character line cap**, and every listing notice names the next `limit` to ask for, as pi's do.
5. **Drop `file_read_hashed` and `file_edit_hashed`.** Neither pi nor Codex ships line-hash editing, and `file_edit` plus `apply_patch` cover it.
6. **Drop `file_append` and `file_delete`.** Neither reference has them; `file_write`, `apply_patch` (`*** Delete File`) and `shell` cover both.
7. **The browser tools get a typed argument schema, and `browser_open` and `screenshot` fold into `browser` as actions.**
8. **`web_fetch` gets a default `max_chars`**, the command output budget, instead of the whole page.
9. **Drop the `ToolErrorCode` mirror.** agentsfleet keeps an enum for its 11 own codes and uses guffbolt's codes for the other 26.
10. **agentsfleet derives its catalog from guffbolt's registry.** guffbolt makes its list public; agentsfleet spells only its own 3 tools.

One standing default needs no answer: **no published name is renamed.** A fleet's saved `ExecutionPolicy.tools` names tools.
A name with no handler refuses the run (`afr_tools/catalog/mod.rs:4-6`), so a rename breaks saved fleets.
Drops 5 and 6 carry the same cost: they land with a migration of saved policies, or wait.
Unverified: how many saved fleets name those four tools. A count over `ExecutionPolicy.tools` in Postgres settles it.

## The three harnesses at a glance

```text
 family        guffbolt (49 named + 2 families)        pi (8 built-in, 4 default)    Codex (current HEAD)
 ────────────  ──────────────────────────────────────  ────────────────────────────  ──────────────────────────────
 one-shot cmd  shell                                   bash (default)                exec_command, one-shot variant
 kept process  exec_command, write_stdin               none                          exec_command, write_stdin
 git           git (remote verbs refused)              none (bash)                   none (exec_command)
 read          file_read, file_read_hashed             read (default; images too)    none (read_file removed)
 write         file_write, file_append                 write (default)               none (apply_patch)
 edit          file_edit, file_edit_hashed             edit (default; many, fuzzy)   none (apply_patch)
 patch         apply_patch (JSON string)               none                          apply_patch (freeform grammar)
 delete        file_delete                             none                          apply_patch Delete File
 image         image                                   read                          view_image
 list/search   ls, find, grep                          ls, find, grep (opt-in)       none (exec_command + rg)
 browser       browser_open, browser, screenshot       none                          none
 web           http_request, web_fetch                 none                          web_search (hosted)
 plan          update_plan                             none (example only)           update_plan (config)
 ask           ask_user                                none                          request_user_input
 subagents     spawn_agent … interrupt_agent (7)       none (example only)           spawn_agent … (v1 5, v2 6)
 deferred      tool_search                             tool_search (opt-in)          tool_search
 MCP           mcp__<server>__<tool>, 3 resource tools same shape (extension)        same shape
 clock         current_time, sleep                     none                          clock namespace, sleep
 product       memory_* (4), cron_* (6), schedule,     none                          none
               message, notify, propose
```

pi's set is `pi:index.ts:95-105`, its default `packages/coding-agent/src/core/settings-manager.ts:85`.
Codex's plan is `codex:core/src/tools/spec_plan.rs:1168-1188`, its feature defaults `codex:features/src/lib.rs:1225-1282`.

## What "default" and "feature" mean here

- **guffbolt has one feature, `tools`**, and every tool on this page sits behind it (`rustd/guffbolt/Cargo.toml`, `[features] tools`).
- **guffbolt has no default set.** The embedder's `ToolsConfig` names tools; `*` offers every tool whose backend was given (`tools/builder/config.rs:62-68`).
- **A name whose backend is missing stops the run before its first step**, and a pattern skips it (`tools/builder/mod.rs:299-320`).
- **agentsfleet has no default set either.** A fleet's `ExecutionPolicy.tools` selects from the published list (`afr_tools/catalog/mod.rs:4-6`).
- **pi turns 4 of 8 on by default**: `read`, `bash`, `edit` and `write` (`settings-manager.ts:85`).
- **Codex turns on** `exec_command`, `write_stdin`, `view_image`, `request_user_input`, the v1 subagent tools and `apply_patch` where the model asks for it.

## How each harness reports output and failure

| | guffbolt | pi | Codex |
|---|---|---|---|
| Failure shape | text `[code] detail`, with `error_code` set (`rustd/guffbolt/src/tool/output.rs:98-107`) | a thrown error becomes `isError: true` text (`packages/agent/src/agent-loop.ts:848-855`) | `RespondToModel(String)` becomes output with `success: false` (`codex:core/src/tools/parallel.rs:497-521`) |
| Failure vocabulary | named codes, such as `text_not_found` (`tools/code.rs:10-67`) | free text | free text |
| Unknown tool | `[tool_not_offered] <name> is not one of this run's tools` (`tool/output.rs:10,138`) | validation error text | `unsupported call: <name>` (`codex:core/src/tools/registry.rs:1290`) |
| Command output cut | head and tail halves of 10,000 tokens × 4 bytes, `... N bytes omitted ...` (`tools/process/output.rs:22-24,133-152`) | the tail of 2,000 lines or 50 KB; full output saved to a temp file (`pi:bash.ts:378`, `pi:truncate.ts:11-12`) | head and tail of `max_output_tokens`, 10,000 default, `... N bytes omitted ...` (`codex:core/src/unified_exec/mod.rs:68-77`) |
| File read cut | `limit` lines (2,000) within the output budget; last line names the next `offset` (`tools/files/read.rs:175-192`) | 2,000 lines or 50 KB; `[Showing lines X-Y of Z. Use offset=N to continue.]` (`pi:read.ts:329`) | no read tool |

Aiwa's read: guffbolt's coded failures are the strongest of the three. Keep them. A model can branch on `[text_not_found]`; pi and Codex make it parse prose.

## Every tool

This is the table Dimension 10.1's test reads. "Published" is whether agentsfleet offers it (`afr_tools/catalog/mod.rs:127-164,207-218`).
The 45 built-ins are `TOOLS` at `tools/builder/catalog.rs:38-84`. The other names are built outside the builder.

| Tool | Family | Backend | Published | Proposal (Aiwa's call) |
|---|---|---|---|---|
| `shell` | process | env | yes, sandbox | keep |
| `exec_command` | process | env + sessions | yes, sandbox | keep |
| `write_stdin` | process | env + sessions | yes, sandbox | keep |
| `git` | process | env | yes, sandbox | keep |
| `file_read` | files | env | yes, sandbox | keep |
| `file_read_hashed` | files | env | yes, sandbox | drop (decision 5) |
| `file_write` | files | env | yes, sandbox | keep |
| `file_append` | files | env | yes, sandbox | drop (decision 6) |
| `file_delete` | files | env | yes, sandbox | drop (decision 6) |
| `file_edit` | files | env | yes, sandbox | change (decision 1) |
| `file_edit_hashed` | files | env | yes, sandbox | drop (decision 5) |
| `apply_patch` | files | env | yes, sandbox | keep |
| `image` | files | env | yes, sandbox | keep |
| `ls` | files | env | no | change: publish, notice (decisions 3, 4) |
| `find` | files | env | no | change: publish, notice (decisions 3, 4) |
| `grep` | files | env | no | change: publish, context, cap (decisions 3, 4) |
| `browser_open` | browser | env | yes, sandbox | merge into `browser` (decision 7) |
| `browser` | browser | env | yes, sandbox | change: typed actions (decision 7) |
| `screenshot` | browser | env | yes, sandbox | merge into `browser` (decision 7) |
| `http_request` | web | outbound | yes, supervisor | keep |
| `web_fetch` | web | outbound | yes, supervisor | change: default cap (decision 8) |
| `memory_store` | memory | memory | yes, supervisor | keep |
| `memory_recall` | memory | memory | yes, supervisor | keep |
| `memory_list` | memory | memory | yes, supervisor | keep |
| `memory_forget` | memory | memory | yes, supervisor | keep |
| `cron_add` | schedule | scheduler | yes, supervisor | keep |
| `cron_list` | schedule | scheduler | yes, supervisor | keep |
| `cron_update` | schedule | scheduler | yes, supervisor | keep |
| `cron_remove` | schedule | scheduler | yes, supervisor | keep |
| `cron_run` | schedule | scheduler | yes, supervisor | keep |
| `cron_runs` | schedule | scheduler | yes, supervisor | keep |
| `schedule` | schedule | scheduler | yes, supervisor | keep |
| `message` | conversation | channel | yes, supervisor | keep |
| `notify` | conversation | notifier | yes, supervisor | keep |
| `spawn_agent` | subagents | agents | yes, by guffbolt's name | keep |
| `send_input` | subagents | agents | yes, by guffbolt's name | keep |
| `followup_task` | subagents | agents | yes, by guffbolt's name | keep |
| `wait_agent` | subagents | agents | yes, by guffbolt's name | keep |
| `list_agents` | subagents | agents | yes, by guffbolt's name | keep |
| `close_agent` | subagents | agents | yes, by guffbolt's name | keep |
| `interrupt_agent` | subagents | agents | yes, by guffbolt's name | keep |
| `update_plan` | plain | none | yes, supervisor | keep |
| `ask_user` | plain | none | yes, supervisor | keep |
| `current_time` | plain | none | no | keep, unpublished |
| `sleep` | plain | none | no | keep, unpublished |
| `tool_search` | deferred | its `Deferred` | no | change: default limit 8 |
| `list_mcp_resources` | MCP | a server | no | keep, unpublished |
| `list_mcp_resource_templates` | MCP | a server | no | keep, unpublished |
| `read_mcp_resource` | MCP | a server | no | keep, unpublished |
| `mcp__<server>__<tool>` | MCP family | a server | no | keep, unpublished |
| propose (named by its `Document`) | proposal family | the embedder's `Document` | as `propose_setup` | keep |
| `web_search` | agentsfleet only | provider-hosted | yes, provider | keep |
| `fleet_steer` | agentsfleet only | runner verb | yes, supervisor | keep |
| `fleet_activity` | agentsfleet only | runner verb | yes, supervisor | keep |

Counts:

- guffbolt names 49 tools and 2 families: the 45 built-ins, `tool_search` and the 3 MCP resource tools; then the MCP tools and the proposal tool.
- agentsfleet publishes 36 entries (`afr_tools/catalog/mod.rs:127`): 33 built over guffbolt, plus `web_search`, `fleet_steer` and `fleet_activity`.
- It adds guffbolt's 7 subagent tools under guffbolt's names (`catalog/mod.rs:207-218`), and `propose_setup` on setup runs.
- The harness tools agentsfleet does not offer yet are listed at `docs/architecture/runner_execution.md:296`.
- The §12 audit said "37 `Entry` constants". The file holds 36; 37 is the count of `ToolErrorCode` variants.

## Processes

| Tool | Description, quoted | Arguments | Limits, output, errors |
|---|---|---|---|
| `shell` | "Run one shell command in the working directory and read back its output and exit code. The command is killed once timeout_ms passes. To keep a process running across calls, use exec_command." (`tools/process/shell.rs:58`) | `command` string, required; `timeout_ms` integer | Default timeout 10 s (`tools/limits.rs`, `default_timeout`). The output budget is head and tail of 40,000 bytes. The text ends `Process timed out after N ms` or `Process exited with code N` (`tools/process/oneshot.rs:55-92`). |
| `exec_command` | "Start a shell command in the working directory and keep it running across calls. Answers with the output that arrived within yield_time_ms and, while the process runs, its session ID; …" (`tools/process/exec_session.rs:146`) | `cmd` required; `workdir`; `tty` bool; `yield_time_ms`; `max_output_tokens` (10,000, at most 262,144) | Yield 10 s by default, held between 250 ms and 30 s (`exec_session.rs:30`, `limits.rs`). The text ends `Process running with session ID N` or the exit line. |
| `write_stdin` | "Write to a session exec_command started and read back what it printed within yield_time_ms. Send empty chars to only read. Answers the exit code once the process has ended." (`exec_session.rs:207`) | `session_id` required; `chars`; `yield_time_ms`; `max_output_tokens` | An empty write waits at least 5 s (`exec_session.rs:33`). An unknown session fails `[session_not_found] session N is not an open session` (`exec_session.rs:330-335`). |
| `git` | "Run git on the repository checked out in the working directory and read back its output and exit code. Local commands only: push, fetch, pull, remote and clone are refused, …" (`tools/process/git.rs:94`) | `args` string array, required; `timeout_ms` | Five remote verbs are refused before anything runs (`git.rs:30`). The output budget is `shell`'s. |

**pi:**

- `bash` takes `command` and `timeout` in seconds, with no default timeout (`pi:bash.ts:39-42`).
- It keeps the tail of 2,000 lines or 50 KB and saves the full output to `pi-bash-<hex>.log` (`pi:bash.ts:378`).
- A non-zero exit is `isError: true` (`pi:bash.ts:434-440`).
- pi has no kept-process tool and no `git` tool.

**Codex:**

- `exec_command` takes `cmd` (required), `workdir`, `tty`, `yield_time_ms` and `max_output_tokens`, plus `shell`, `login` and sandbox-escalation fields (`codex:core/src/tools/handlers/shell_spec.rs:15-73`).
- Its description: "Runs a command in a PTY, returning output or a session ID for ongoing interaction." (`shell_spec.rs:83`). PTY means pseudo-terminal.
- Yields are 250 ms to 30 s, an empty write 5 s or more, output 10,000 tokens (`codex:core/src/unified_exec/mod.rs:68-77`).
- `write_stdin` defaults its yield to 250 ms (`codex:core/src/tools/handlers/unified_exec.rs:117-119`).
- The one-shot variant adds `timeout_ms`, 10,000 ms by default (`codex:core/src/tools/handlers/exec_command.rs:275-306`).
- Its header carries `Wall time` and `Original token count` lines (`codex:core/src/tools/context.rs:568-591`).

**Proposals (Aiwa's call):**

- **`shell`: keep.** Its `timeout_ms` and default match Codex's one-shot variant. Head and tail beats pi's tail alone: the command echo and the error both survive.
- **`exec_command`, `write_stdin`: keep.** Names, arguments and bounds already match Codex. guffbolt leaves out Codex's sandbox-escalation fields on purpose, as agentsfleet's sandbox has no approval prompt.
- **`git`: keep.** Neither reference ships one; agentsfleet needs it because the clone's credential stays outside the sandbox (`git.rs:1-14`).
- **Not proposed:** pi's temp file of full output. Codex does without one. A sandbox file the model can `file_read` is a later choice.

## Files

| Tool | Description, quoted | Arguments | Limits, output, errors |
|---|---|---|---|
| `file_read` | "Read a file in the working directory and read back its text: at most `limit` lines from line `offset` (1 when absent), cut to the same output budget as a command's. A read that stops before the file ends says on its last line which offset to read on from." (`tools/files/read.rs:76`) | `path` required; `offset`, `limit` (both 1 or more) | 2,000 lines by default (`limits.rs`), and at most 8 MiB read (`tools/env/mod.rs:44`). The last line reads `... the file continues at line N; read on with offset N` (`read.rs:183`). No line numbers are printed. |
| `file_read_hashed` | "Read a file in the working directory with every line tagged L<line>:<hash>\|<text>, for file_edit_hashed to name lines by. The file must be text and fit one read." (`tools/files/hashed.rs:83`) | `path` | One read whole. |
| `file_write` | "Write a file in the working directory, replacing what was there, and make any missing directories on the way." (`tools/files/mod.rs:170`) | `path`, `content` | Ends with a `Lines changed:` block (`tools/files/changes.rs:22`). |
| `file_append` | "Add text to the end of a file in the working directory, making the file when it does not exist." (`files/mod.rs:203`) | `path`, `content` | as `file_write` |
| `file_delete` | "Delete a file in the working directory. Directories and links are not deleted." (`files/mod.rs:237`) | `path` | |
| `file_edit` | "Find text in a file in the working directory and replace its first occurrence. The file must be text and fit one read." (`files/mod.rs:261`) | `path`, `old_text`, `new_text` | A miss fails `[text_not_found] <path> does not contain old_text`. A match is replaced with `replacen(…, 1)`, so a second match is never reported (`files/mod.rs:279-285`). |
| `file_edit_hashed` | "Replace one line, or the range from target to end_target, … naming the lines by the tags file_read_hashed printed. A tag the file no longer matches is refused: read the file again." (`hashed.rs:106`) | `path`, `target`, `end_target`, `new_text` | `[hash_mismatch]` on a stale tag. |
| `apply_patch` | "Edit files in the working directory with one patch in apply_patch's grammar: add, update and delete hunks, each path relative to the working directory. Every hunk is checked before any file changes; …" (`tools/files/apply_patch.rs:150`) | `patch` string, required | The success text matches Codex's `Success. Updated the following files:` (`apply_patch.rs:38`). A patch that does not apply changes nothing (`apply_patch.rs:1-17`). |
| `image` | "Look at an image file in the working directory: PNG, JPEG, GIF or WebP. The image is shown to you with this call's result on your next step." (`tools/files/image.rs:141`) | `path` | Scaled to 2,048 px and 5 MiB (`limits.rs`). It refuses on a wire with no image input (`image.rs:53`). |
| `ls` | "List a directory in the working directory, sorted by name, a directory marked with a trailing slash. Dotfiles are listed. At most `limit` entries (500 when absent)." (`tools/files/listing/mod.rs:72`) | `path`, `limit` | The notice reads `... limit reached; narrow the path or the pattern` (`listing/mod.rs:32`). |
| `find` | "Find files in the working directory whose path matches a glob, such as `**/*.rs`, … `.git` and what `.gitignore` names are skipped. At most `limit` paths (1000 when absent)." (`listing/mod.rs:114`) | `pattern` required; `path`, `limit` | The same notice. |
| `grep` | "Search the files in the working directory for a pattern, a regular expression or, with `literal`, text, and read back each matching line as `path:line: text`. … At most `limit` lines (100 when absent)." (`tools/files/listing/grep.rs:46`) | `pattern` required; `path`, `glob`, `ignore_case`, `literal`, `limit` | No line-length cap. The frame is the only byte bound (spec §9 Discovery). |

**pi** (`pi:` paths):

- `read` takes `path`, `offset` and `limit`. It cuts at 2,000 lines or 50 KB and also reads images (`read.ts:14-18,239`).
- `write` takes `path` and `content` (`write.ts:10-13`).
- `edit` takes `path` and `edits: [{oldText, newText}]`. Each `oldText` must be unique and not overlap another (`edit.ts:18-38,147`).
- A repeated match fails "Found N occurrences of the text in <path>. The text must be unique. Please provide more context to make it unique." (`edit-diff.ts:310`).
- After an exact miss it retries with whitespace, quote and dash normalising (`edit-diff.ts:250-288`).
- `grep` takes `pattern`, `path`, `glob`, `ignoreCase`, `literal`, `context` and `limit` (100). It is backed by ripgrep and cuts lines to 500 characters (`grep.ts:20-37,226`).
- Its notice reads "100 matches limit reached. Use limit=200 for more, or refine pattern" (`grep.ts:300-317`).
- `find` defaults `limit` to 1,000 and is backed by fd (`find.ts:26-42`). `ls` defaults `limit` to 500 (`ls.ts:10-22`).
- Each notice names the next limit, as in "500 entries limit reached. Use limit=1000 for more" (`ls.ts:147`).

**Codex:**

- Codex has no read, write, edit or listing tool at this HEAD. `read_file`, `list_dir` and `grep_files` are gone; a grep for their handlers finds none.
- `apply_patch` is freeform only, a Lark grammar (`codex:protocol/src/openai_models.rs:339-341`, `codex:core/assets/tools/apply_patch.lark`).
- A failure reads `apply_patch verification failed: …`, such as `Failed to find context '<ctx>' in <path>` (`codex:core/src/tools/handlers/apply_patch.rs:351`, `codex:apply-patch/src/file_update.rs:80`).
- `view_image` takes `path`, and `detail` when the model supports it (`codex:core/src/tools/handlers/view_image_spec.rs:17-46`).

**Proposals (Aiwa's call):**

- **`file_edit`: change toward pi** (decision 1).
  - Take `edits: [{old_text, new_text}]`, each matched against the original file.
  - Refuse a repeated match with a new code `text_not_unique` that names the count.
  - Refuse overlapping edits.
  - Why: today a second match is edited silently in the wrong place. pi's shape also saves a call per extra hunk.
  - Not proposed: pi's fuzzy retry. It edits text the model did not quote, and coded refusals already steer a retry.
- **`file_read`: keep.** It already matches pi's bounds and continuation line. Images stay in `image`, as Codex keeps `view_image` apart.
- **`apply_patch`: keep the JSON string form.** Codex's freeform grammar is a Responses-API custom tool. guffbolt's other wires take JSON function arguments only, so the string form runs everywhere.
- **`file_read_hashed`, `file_edit_hashed`: drop** (decision 5). They are two tools neither reference ships. Dropping them shrinks the published catalog by two.
- **`file_append`, `file_delete`: drop** (decision 6). `apply_patch`'s `*** Delete File` and `file_write` cover both, as Codex and pi show.
- **`image`: keep, no rename.** Codex's name is `view_image`; renaming a published name breaks saved fleets.
- **`ls`, `find`, `grep`: publish in agentsfleet** (decision 3). Today a fleet must shell out to `rg` with quoting; §9 built the executor search for exactly these.
- **`grep`: change toward pi** (decision 4). Add `context` (lines around each match, 0 when absent) and cut each line at 500 characters with a note.
- **The listing notices: change toward pi** (decision 4). For example: `... 100 lines, the limit; ask with limit=200 for more, or narrow the pattern`.

## Browser

| Tool | Description, quoted | Arguments | Output |
|---|---|---|---|
| `browser_open` | "Open a page in the environment's browser. An environment with no browser refuses the call with a code, and the run continues." (`tools/browser.rs:55`, sentence 2 from `browser.rs:16-20`) | any JSON (`type Arguments = Value`, `browser.rs:59`) | text |
| `browser` | "Act on the open page in the environment's browser: click, type, scroll, read." (`browser.rs:79`) | any JSON (`browser.rs:83`) | text |
| `screenshot` | "Take a screenshot of the open page in the environment's browser." (`browser.rs:103`) | any JSON (`browser.rs:107`) | an image, plus `Attached a screenshot (N bytes, <type>) for your next step` (`browser.rs:111-118`) |

Neither pi nor Codex ships a browser tool.

**Proposal (Aiwa's call, decision 7):** give `browser` a typed schema with an `action` field (`open`, `click`, `type`, `scroll`, `read`, `screenshot`).
`browser_open` and `screenshot` then fold into it. Today the model is told no argument names, so every first call is a guess.
Unverified: what arguments the sandbox's browser backend accepts. Reading `afr_tools/sandbox/browser` settles it before the schema is written.

## Web

| Tool | Description, quoted | Arguments | Limits |
|---|---|---|---|
| `http_request` | "Send one HTTPS request to a host the network policy allows, and read back its status and up to 1 MiB of a complete body. Truncated bodies are omitted with an explicit incomplete-source notice. …" (`tools/web/http_request.rs:49`) | `url` required; `method`, `headers`, `body` | 1 MiB body; `${secrets.NAME.FIELD}` only in `Authorization` (`http_request.rs:28-39`) |
| `web_fetch` | "Fetch one https page from a host the network policy allows and read back its text. No credential is sent." (`tools/web/web_fetch.rs:40`) | `url` required; `max_chars` | The whole text when `max_chars` is absent (`web_fetch.rs:29-30,68`) |
| `web_search` (agentsfleet) | provider-hosted; the provider runs it (`afr_tools/catalog/mod.rs:59`) | the provider's | |

**pi** has no web tool. **Codex** sends the hosted `web_search`, `Cached` by default (`codex:core/src/tools/hosted_spec.rs:15-20`, `codex:core/src/config/mod.rs:4204-4205`).

**Proposals (Aiwa's call):**

- **`http_request`: keep.** Credential placement and the egress guard are agentsfleet's product, and neither reference has an equal.
- **`web_fetch`: change** (decision 8). When `max_chars` is absent, cut at the command output budget, 40,000 bytes. Add a closing line naming the characters left.
  - Why: one long page can fill the model's window. Indy's rule is a bound with a reason, and the window is that reason.
- **`web_search`: keep** as the provider-hosted entry, as Codex does.

## Memory, schedules, conversation and proposals

These are agentsfleet's product tools. Neither pi nor Codex has an equal, so the proposal for each is **keep** (Aiwa's call).

| Tool | Description, quoted | Arguments |
|---|---|---|
| `memory_store` | "Remember a fact for later runs under a stable key." (`tools/memory/mod.rs:168`) | `key`, `content` required; `category` (`core` default); `visibility` (`private` or `shared`) |
| `memory_recall` | "Read remembered facts whose key or content holds the query, key matches first." (`memory/mod.rs:199`) | `query` required; `limit` (5) |
| `memory_list` | "List the keys and categories of every remembered fact, newest first." (`memory/mod.rs:224`) | `category` |
| `memory_forget` | "Forget a remembered fact for the rest of this run. The stored copy stays, …" (`memory/mod.rs:246`) | `key` |
| `cron_add` | "Schedule a run on a recurring cron expression, with a message saying what to do each time it fires." (`tools/schedule/cron.rs:81`) | `cron`, `message` required; `timezone` (an Internet Assigned Numbers Authority (IANA) zone, Coordinated Universal Time (UTC) default) |
| `cron_list` | "List the schedules, each with its expression, zone, message and who made it." (`cron.rs:103`) | none |
| `cron_update` | "Change a schedule's expression, zone or message, or pause and resume it." (`cron.rs:138`) | `schedule_id` required; `cron`, `timezone`, `message`, `paused` |
| `cron_remove` | "Delete a schedule." (`cron.rs:119`) | `schedule_id` |
| `cron_run` | "Run a schedule now, as if it had fired; the run is queued behind this one." (`cron.rs:164`) | `schedule_id` |
| `cron_runs` | "List the runs a schedule fired, newest first." (`cron.rs:184`) | `schedule_id` required; `limit`, `starting_after` |
| `schedule` | "Schedule one more run at a given moment, with a message saying what to do then. It is removed after it runs." (`tools/schedule/once.rs:62`) | `at` (Request for Comments (RFC) 3339, within a year), `message` |
| `message` | "Post a short line to the conversation this run was asked from, before the answer, such as progress or what you are waiting on." (`tools/message/mod.rs:62`) | `text` |
| `notify` | "Send a push notification to the person this run works for." (`tools/notify/mod.rs:72`) | `message` required; `title`, `priority` |
| `propose_setup` | "Call this tool only when the person's newest message asks to create or change the setup. …" (`rustd/crates/protocol/af_fleet_runtime/src/setup/mod.rs:31-40`) | the setup document |
| `fleet_steer` | "Ask another fleet in this workspace to do something, by its fleet_id. Returns at once: …" (`afr_tools/verbs/fleet_steer/mod.rs:41`) | `fleet_id`, `text` (at most 1,048,576 characters) |
| `fleet_activity` | "List this workspace's fleets with their status, or, given a fleet_id, that fleet's recent events … Wakes no fleet." (`afr_tools/verbs/fleets/mod.rs:39`) | `fleet_id`, `limit`, `starting_after` |

Considered, not proposed: folding `memory_list` into `memory_recall`, whose empty query already matches every memory (`memory/mod.rs:138-140`).
The gain is one tool. The cost is a published name. Indy can ask for it.

## Subagents, plan, questions and the clock

| Tool | Description, quoted | Arguments |
|---|---|---|
| `spawn_agent` | "Start a child agent on a well-scoped task. It runs alongside you over its own history, with your tools or the subset you name, …" (`tools/agents/mod.rs:172`) | `message` required; `tools`; `fork_turns` (`all`, `none` or a count) |
| `send_input` | "Queue a message for a child agent: a running child reads it on its next step, an idle one with its next task. …" (`agents/mod.rs:203`) | `target`, `message` |
| `followup_task` | "Give a child agent another task. An idle child runs again with its whole history; …" (`agents/mod.rs:222`) | `target`, `message` |
| `wait_agent` | "Wait for a child agent's turn to end and read its answer. …" (`agents/mod.rs:241`) | `target` required; `timeout_ms` (30 s default, 1 h at most; `agents/mod.rs:29`, `limits.rs`) |
| `list_agents` | "List every child agent of this run: its agent_id, status, depth and how many calls it made." (`agents/mod.rs:278`) | none |
| `close_agent` | "Close a child agent, and every child it started, once it is no longer needed. …" (`agents/mod.rs:293`) | `target` |
| `interrupt_agent` | "Interrupt a child agent's current turn, if it runs one, and answer its status before. …" (`agents/mod.rs:311`) | `target` |
| `update_plan` | "Record your plan as steps with their status, and update it as you work." (`tools/plan.rs:52`) | `plan` required, each `{step, status}`; `explanation` |
| `ask_user` | "Ask the person a question you cannot answer from what you can read or run, and wait for their answer, which is this call's result. …" (`tools/ask.rs:39`) | `question` required; `options` |
| `current_time` | "Read the current time, in UTC, as RFC 3339." (`tools/clock.rs:46`) | none |
| `sleep` | "Wait for duration_ms milliseconds before going on; the wait ends early when a new message arrives, and says so." (`clock.rs:89`) | `duration_ms`, at most 12 h (`limits.rs`) |

**Codex:**

- v1 offers `spawn_agent`, `send_input`, `resume_agent`, `wait_agent` and `close_agent`. v1's `wait_agent` takes `targets` (an array) and `timeout_ms` (30,000) (`codex:core/src/tools/handlers/multi_agents_spec.rs:608-645`).
- v2 offers `spawn_agent`, `send_message`, `followup_task`, `wait_agent`, `interrupt_agent` and `list_agents` (`codex:core/src/tools/spec_plan.rs:1452-1563`).
- `update_plan` has the same arguments and statuses as guffbolt's, and is off unless configured (`codex:core/src/tools/handlers/plan_spec.rs:45-49`).
- `request_user_input` takes one to three questions, each with `id`, `header` and `options` of `{label, description}` (`codex:core/src/tools/handlers/request_user_input_spec.rs:16-128`).

**pi** has none of these in its source; `todo` and `subagent` are only examples (`packages/coding-agent/examples/extensions/`).

**Proposals (Aiwa's call):**

- **The seven subagent tools: keep.** guffbolt's set is Codex's v2 set plus `send_input` and `close_agent` from v1. It already covers both ends of a child's life.
- Considered, not proposed: `wait_agent` taking several `targets`, as Codex v1 does. It is new behaviour, not a fix.
- **`update_plan`: keep.** It is Codex's tool, argument for argument.
- **`ask_user`: keep.** One question a call is simpler than Codex's three, and the run pauses on it either way.
- **`current_time`, `sleep`: keep, unpublished.** Codex gates both behind features too.

## Deferred tools and MCP

| Tool | Description, quoted | Arguments | Notes |
|---|---|---|---|
| `tool_search` | "Search the tools this run holds but has not offered you, by what you need them to do, and load the best matches: …" (`tools/search.rs:128`) | `query` required; `limit` | Every match when `limit` is absent (`search.rs:138`) |
| `list_mcp_resources` | "List the resources this run's MCP servers hold: each one's server, URI, name and description. Read one with read_mcp_resource." (`tools/mcp/resources.rs:133`) | `server` | MCP is the Model Context Protocol; URI is a Uniform Resource Identifier. |
| `list_mcp_resource_templates` | "List the resource templates this run's MCP servers offer: …" (`resources.rs:164`) | `server` | |
| `read_mcp_resource` | "Read one resource an MCP server of this run holds, by the server's name and the resource's URI, and read back its text." (`resources.rs:196`) | `server`, `uri` | |
| `mcp__<server>__<tool>` | the server's own | the server's own | Names cut at 64 characters; 300 s per call (`tools/mcp/mod.rs:56,58`) |

**pi:**

- `tool_search` takes `query` and `limit`, 8 by default. It ranks with Best Match 25 (BM25) (`packages/coding-agent/src/extensions/tool-search/tool.ts:21-22,320`).
- The three resource tools have the same names (`packages/coding-agent/src/core/mcp-servers.ts:37-39`).

**Codex:**

- `tool_search` defaults `limit` to 8 (`codex:tools/src/tool_discovery.rs:6-7`).
- MCP names are `mcp__<server>__<tool>`, at most 128 characters, with a hash suffix on a clash (`codex:codex-mcp/src/tools.rs:24,266-281`).
- The resource listings take a `cursor` (`codex:core/src/tools/handlers/mcp_resource_spec.rs:29-101`).

**Proposals (Aiwa's call):**

- **`tool_search`: change, default `limit` 8**, as both references do. "Every match" can load a whole server's tools in one step.
- Unverified: how guffbolt ranks matches. Reading `Deferred::search` in `tools/search.rs` settles it.
- **The MCP tools: keep, unpublished.** Their names match both references. A `cursor` argument waits until a server that pages is in use.

## The two §12 items

### Decision 9: drop the `ToolErrorCode` mirror

- **Today:** `afr_tools/runtime/mod.rs:62-190` holds a 37-variant enum.
  - 26 variants spell guffbolt's codes: `guffbolt::tools::code::*` (`tools/code.rs:10-67`), plus `ErrorCode::{NOT_OFFERED, INTERRUPTED}`.
  - 11 are agentsfleet's own: `hosted_tool_unavailable`, `credential_placement_not_allowed`, `credential_host_not_allowed`, `request_policy_not_allowed`, `credential_mint_refused`, `schedule_cap_reached`, `schedule_not_fleet_owned`, `schedule_not_runnable`, `message_no_channel`, `agentsfleetd_refused` and `agentsfleetd_unreachable`. The §12 audit said 25 and 12; `runtime/mod.rs:150-188` maps 26 onto guffbolt.
- **Proposal (Aiwa's call):** keep a closed enum for the codes agentsfleet owns. Use guffbolt's constants directly for the rest.
  - The `From<ToolErrorCode> for guffbolt::ErrorCode` impl stays for the owned codes.
  - guffbolt's own comment invites this shape: "an embedder that keeps its own closed set of codes converts from these" (`tools/code.rs:1-4`).
- **Size and risk:** about 75 of 130 lines, and about 60 call sites across 14 files (§12 audit).
  - Unverified: whether `afr_telemetry` keys a closed label set on the enum. Reading its label code settles it.
  - If it does, the enum keeps the label role and only the spelling moves.

### Decision 10: derive the catalog from guffbolt's registry

- **Today:** `afr_tools/catalog/mod.rs:53-124` spells 33 of guffbolt's names as string constants, each with a runtime.
  - Only the subagent tools are looked up through `guffbolt::tools::built_in` (`catalog/mod.rs:214-218`).
  - guffbolt's list is `TOOLS: [BuiltIn; 45]`, which is `pub(super)` (`tools/builder/catalog.rs:38`).
- **Proposal (Aiwa's call):**
  - guffbolt makes the list public, as `pub fn built_ins() -> &'static [BuiltIn]`.
  - agentsfleet derives each runtime from `BuiltIn::needs`: `Env` or `Sessions` means the sandbox, anything else the supervisor.
  - agentsfleet spells only `web_search`, `fleet_steer` and `fleet_activity`, plus a list of guffbolt names it does not publish.
  - A unit test pins the published set as literals, so a guffbolt rename fails the build.
- **Why the exclusion list:** without it, every new guffbolt tool would be published on the next bump. Publishing stays a decision agentsfleet makes.
- **Size and risk:** about 60 lines (§12 audit). The pinned test is what keeps a rename from silently renaming a published tool.

## What Indy's answers change

```text
 yes to 1  ─► file_edit: edits[], text_not_unique, overlap refusal        (guffbolt)
 yes to 2  ─► ToolsConfig::coding(), ToolsConfig::read_only()             (guffbolt)
 yes to 3  ─► PUBLISHED gains ls, find, grep; docs page lists them        (agentsfleet, docs repo)
 yes to 4  ─► grep context + 500-char cap; listing notices name a limit   (guffbolt)
 yes to 5  ─► hashed pair leaves TOOLS and PUBLISHED; saved policies move (both)
 yes to 6  ─► file_append, file_delete leave; saved policies move         (both)
 yes to 7  ─► one typed browser tool with actions                         (guffbolt, sandbox)
 yes to 8  ─► web_fetch default max_chars = output budget                 (guffbolt)
 yes to 9  ─► ToolErrorCode keeps 11 owned codes                          (agentsfleet)
 yes to 10 ─► guffbolt::tools::built_ins(); catalog derives runtimes      (both)
 any "no"  ─► that tool stays as it is today
```

Every published change (3, 5, 6, 7) is a public behaviour change. Each needs a branch in `~/Projects/docs`.
