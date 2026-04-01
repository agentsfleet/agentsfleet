# M18_003: API-Backed Spec Template Generation and SSE Preview

**Prototype:** v1.0.0
**Milestone:** M18
**Workstream:** 003
**Date:** Mar 31, 2026
**Status:** PENDING
**Priority:** P1 — move spec init and run preview from local heuristics to server-side agent with SSE streaming
**Batch:** B2 — depends on M18_002 (local implementation, now the reference spec)
**Depends on:** M18_002

---

## 0.0 Discovery Required — Unresolved Architecture

**Status:** PENDING — must be resolved before implementation begins

Do not start sections 2.0-3.0 until 0.4 Decision Gate is filled in.

### 0.1 The Problem

When you run `zombiectl spec init` or `zombiectl run --preview`, you want an agent
that actually understands your repo — not regex heuristics. The blocker:

```
User's laptop                    Server worker sandbox
─────────────────                ─────────────────────
/repos/my-project/               agent runs here
  go.mod                    ??   cannot read these files
  src/main.go
  ...10,000 files
```

The agent can't think about files it can't see. Three ways to close this gap:

---

### Option A — Send Context in the Request (Local reads, server thinks)

```
CLI reads local FS
  → packs manifest file contents + file tree into POST body
  → sends to zombied
  → zombied calls Anthropic, proxies the SSE stream back
  → CLI renders tokens as they arrive
```

The CLI decides what to include (manifest files: go.mod, Cargo.toml, package.json;
file tree: paths only). The model gets that context in one prompt and responds.

**Feels like:** fast. One round trip. No sandbox boot. First token in ~2s.

**Tradeoff:** CLI decides upfront what the agent sees. If it picks the wrong files
to include, the agent works with incomplete context. No way to ask for more.

**Precedent:** How Claude Code, Amp, Cursor, Aider all work. The model is remote,
the context selection is local, the rendering is local.

---

### Option B — Send the Folder (Upload, then agent runs)

Three mechanisms for sending the folder to the sandbox:

**B1 — Git bundle**
```bash
git archive HEAD | gzip | POST /v1/spec/bundle
```
CLI packs the whole repo as a git archive (~1-5MB), uploads to server, sandbox
unpacks it. Agent gets the real tree with real file contents.

**B2 — Rsync / file sync endpoint**
CLI syncs the working directory to a server-side path. Like Replit's workspace sync.
Agent has a persistent copy it can browse and re-use between sessions.

**B3 — Selective upload**
CLI walks the tree, uploads only manifest files + files referenced in the spec.
Smaller payload, but CLI still decides what to include.

In all B variants the agent runs in the full worker pipeline:
```
upload → start_run → job queue → worker pickup → sandbox mounts bundle
→ agent reads files natively → SSE stream results back
```

**Feels like:** slow first run (15-40s pipeline overhead + upload time).
Agent has full file access once running — no context limits.

**Tradeoff:** Pipeline overhead may miss the "feels instant" bar.
Files leave the user's machine. Requires new upload endpoint + bundle mount.

---

### Option C — MCP Filesystem Mount (Agent pulls files on demand)

MCP (Model Context Protocol) is a standard for how an AI model requests data
from outside its context window. Instead of sending files upfront, the CLI
exposes local files as callable tools. The agent asks for what it needs.

```
CLI starts a local tool server that exposes:
  read_file(path)   → reads from local disk, returns contents
  list_dir(path)    → lists directory, returns entries
  glob(pattern)     → finds matching files, returns paths

Model calls tools as needed:
  → read_file("go.mod")          CLI reads locally, returns content
  → list_dir("src/")             CLI reads locally, returns listing
  → read_file("src/server.go")   CLI reads locally, returns content
  → "I've seen enough. High confidence: src/server.go"
```

The CLI sits in a loop: send message → get tool call request → execute locally →
send result back → repeat until the model streams its final answer.

**What the user sees:**
```
$ zombiectl run --spec feature.md --preview

  🧟 analyzing your repo...

  → read go.mod  (Go 1.21, github.com/acme/api)
  → listed src/  (23 files)
  → read src/http/server.go

  ● src/http/server.go    high
  ● src/http/handler.go   high
  ◆ src/config/config.go  medium

  3 file(s) matched
```

Agent reads exactly what it needs. Files never leave the laptop unless the agent
asks for them. User sees the agent's work as it happens.

**Feels like:** a chat window in the terminal. Like Claude Code browsing your repo.

**Tradeoff:** Most complex to build. Requires the local tool call loop (~200 lines),
a security boundary (only allow reads inside the repo path), and latency per tool
call (~200-400ms per read_file round trip). Best UX. Most work.

**Security boundary (required):** The CLI must reject any tool call that resolves
outside the repo root. `read_file("../../.ssh/id_rsa")` must return an error.

---

### 0.2 Pipeline Overhead Study

Options B and C use the full worker pipeline. Measure before committing:

- `start_run` POST → first SSE event: ___s (cold worker), ___s (warm)
- Sandbox boot time alone: ___s
- **Target:** user sees first output within 5s of running the command

If overhead > 3s, Option B likely fails the bar without pipeline changes.
Option A and C have no pipeline overhead (no sandbox, no job queue).

---

### 0.3 Competitive Research

| Tool | Where agent runs | How it gets files | First result latency |
|------|-----------------|-------------------|---------------------|
| Claude Code | CLI process | MCP tool calls (Option C) | ~2s |
| Amp Code | ? | ? | ? |
| Cursor | IDE process | IDE sends selected context (Option A) | ~1s |
| Aider | CLI process | sends diff + relevant files (Option A) | ~2s |
| Devin | Remote sandbox | clones from GitHub URL (Option B1) | ~30s |
| GitHub Copilot | IDE process | LSP sends open files (Option A) | ~1s |
| Claude.ai Projects | API call | user uploads files explicitly (Option B3) | ~3s |

Fill in the Amp row during discovery. That's the closest analog to zombiectl.

---

### 0.4 Decision Gate

Fill this in before writing any implementation code. Update sections 2.0-3.0 to match.

```
CHOSEN OPTION: [ A / B1 / B2 / B3 / C ]

RATIONALE:
- Pipeline overhead measured at: ___s cold, ___s warm
- Chosen because: ___

REJECTED:
- Option ___ because: ___

IMPACT ON SECTIONS 2.0-3.0:
- ___
```

### 0.5 Dimensions

- 0.1 PENDING Measure pipeline overhead (start_run → first SSE event, cold + warm)
- 0.2 PENDING Research Amp Code — where does their agent run, how does it get files
- 0.3 PENDING Decide Option A / B / C, fill in 0.4, update sections 2.0-3.0

---

## 1.0 Problem

In zombiectl's model, **developers don't write code — agents do.** Developers
dictate intent. Specs are the contract between developer intent and agent execution.

That means two things must be agent-powered:

1. **Spec creation** — the developer describes what they want to build. The agent
   reads the repo and produces a complete spec: right milestone structure, right
   acceptance criteria, right gate commands for this language and project. Not a
   blank template with `{placeholders}` the developer fills in. A real spec draft
   the developer reviews and approves.

2. **Impact preview** — before executing a spec, the developer wants to know the
   blast radius: which files will the agent touch? Today this is regex + substring
   matching. An agent that reads the spec and understands the codebase can predict
   this correctly — including files implied but not named.

**Why M18_002 is wrong:** `spec init` produces a blank template. The developer still
writes the spec. That's backwards — the agent should draft it. `run --preview` uses
substring heuristics. That misses the point — the agent should reason about intent.

**What M18_003 delivers:** Agent drafts the spec from developer intent. Agent predicts
impact from spec content. Developer reviews, edits, approves. Agent executes.

The mechanism for giving the agent access to the local repo is the open question —
see Section 0.0.

### 1.1 Alternatives to Track

Other tools that generate or assist with spec/plan creation. Track these for
competitive awareness and to avoid reinventing solved problems.

| Tool | What it does | Relevant to |
|------|-------------|-------------|
| fission.ai | ? — research needed | spec generation from intent |
| gstack `/office-hours` | YC-style problem framing + plan | spec problem statement |
| gstack `/plan-eng-review` | architecture + test plan from spec | spec validation |
| gstack `/autoplan` | full CEO + eng + design review pipeline | spec review pipeline |
| Linear | issue → spec translation | spec structure |
| GitHub Copilot Workspace | intent → plan → code | closest end-to-end analog |

**Discovery task:** Research fission.ai and GitHub Copilot Workspace specifically.
Both claim to go from developer intent to actionable plan. What does their output
look like? How does it compare to the zombiectl spec format? What can be learned
or integrated?

---

## 2.0 API Design

### 2.1 Spec Template Generation

```
POST /v1/spec/template
Authorization: Bearer <token>
Content-Type: application/json

{
  "file_paths": ["src/main.go", "tests/main_test.go", ...],
  "makefile_targets": ["lint", "test", "build"],
  "test_patterns": ["*_test.*", "*.test.*"],
  "project_structure": ["src/", "tests/", "docs/"],
  "manifest_files": {
    "go.mod": "module github.com/foo/bar\n\ngo 1.21\n",
    "package.json": "{\"name\":\"zombiectl\",\"version\":\"1.0.0\"}"
  }
}

→ 200 application/json
{
  "template": "# M{N}_001: {Feature Title}\n\n**Prototype:** v1.0.0\n..."
}
```

The CLI reads the contents of manifest files that exist in the repo root
(go.mod, Cargo.toml, package.json, pyproject.toml, mix.exs — whichever are present)
and includes them in the POST body. The server agent uses these contents to detect language
and ecosystem. All other source file contents stay local — only manifest file contents
are sent.

The server calls nullclaw agent directly from the HTTP handler — it does NOT create a run
or spawn a sandbox process. Same pattern as the proposals agent in
`src/pipeline/agents_runner.zig`. Handler timeout: 30 seconds.

The CLI shows a `ui-progress.js` spinner while awaiting the response (template generation
is a single LLM call, 5-15s). No streaming of the template itself is required for v1.

**Dimensions:**
- 2.1.1 PENDING POST /v1/spec/template endpoint — validates input, calls nullclaw agent, returns template
- 2.1.2 PENDING Agent reads manifest_files contents to detect language + ecosystem; falls back to file_paths names if manifest_files is empty
- 2.1.3 PENDING Returns structured template matching the canonical spec format from CLAUDE.md

### 2.2 SSE Preview Stream

```
POST /v1/spec/preview
Authorization: Bearer <token>
Content-Type: application/json

{
  "spec_content": "# Feature\n\nEdit `src/foo.go` and `lib/bar.ts`...",
  "file_paths": ["src/foo.go", "lib/bar.ts", "src/util.go", ...]
}

→ 200 text/event-stream
event: match
data: {"file":"src/foo.go","confidence":"high"}

event: match
data: {"file":"lib/bar.ts","confidence":"medium"}

event: match
data: {"file":"src/util.go","confidence":"low"}

event: done
data: {}
```

Agent semantically matches spec intent to file paths. Streams results as each
file is scored — gives real-time feedback for large repos without waiting for
full completion.

If the agent errors mid-stream, the server emits an error event before closing:
```
event: error
data: {"message": "agent timed out after 60s"}
```

The CLI uses fetch() + response.body.getReader() to consume the stream (standard
browser-compatible POST + SSE pattern). EventSource is NOT used — it only supports GET.
The CLI does NOT have an existing SSE consumer; a new streamFetch() helper must be
added to zombiectl/src/lib/http.js.

Handler stall deadline: 60 seconds between events. File path limit: CLI sends at most
2000 relative paths (BFS order, truncated after limit). Server does NOT create a run.

**Dimensions:**
- 2.2.1 PENDING POST /v1/spec/preview endpoint — SSE response, nullclaw agent streams matches
- 2.2.2 PENDING Agent uses spec content semantically (not substring) to score each file
- 2.2.3 PENDING Each SSE event: `event: match\ndata: { file, confidence }` where confidence ∈ high|medium|low
- 2.2.4 PENDING `event: done\ndata: {}` signals stream end; `event: error\ndata: {message}` on failure

---

## 3.0 CLI Migration

### 3.1 spec init

`commandSpecInit` sends collected local facts to the server, writes the returned template.

**Before (M18_002):** scans repo locally, generates template with string interpolation.
**After (M18_003):** scans repo locally for paths + Makefile + test patterns + manifest file
contents, shows spinner, POSTs to `/v1/spec/template`, writes returned template to disk.

- Remove `generateTemplate()` from the CLI — server owns template generation
- Add manifest file content reading: read go.mod, Cargo.toml, package.json, pyproject.toml,
  mix.exs from repoPath (whichever exist) and include in POST body
- `spec init` becomes auth-required: add `spec.init` to routes.js, remove from `AUTH_EXEMPT_ROUTES`
- Show spinner (ui-progress.js) while awaiting server response
- `commandSpecInit` JSON output: `detected.*` fields come from server response

**Dimensions:**
- 3.1.1 PENDING commandSpecInit: POST collected facts + manifest file contents to /v1/spec/template
- 3.1.2 PENDING Remove generateTemplate() and local template string from CLI
- 3.1.3 PENDING Move spec.init out of AUTH_EXEMPT_ROUTES — requires token; show spinner during request

### 3.2 run --preview

`runPreview` sends spec content + local file tree to `/v1/spec/preview` SSE stream,
prints matches as they arrive.

**Before (M18_002):** extractSpecRefs() regex + matchRefsToFiles() substring scoring.
**After (M18_003):** sends spec_content + file_paths (max 2000), renders SSE events as they stream.

- Add `streamFetch(url, payload, onEvent, opts)` helper to `lib/http.js`:
  uses `fetch()` + `response.body.getReader()` + SSE line protocol parser
- Remove `extractSpecRefs()` and `matchRefsToFiles()` from the CLI call path
  (keep the functions in the file, marked @internal, until M18_003 SSE is stable;
  delete them in the follow-on cleanup commit)
- `printPreview` is retained — renders the collected match list at stream end
- `confIndicator` / `sanitizeDisplay` retained — output formatting stays client-side
- Preview spinner shown while stream is open; each match rendered as event arrives

**Dimensions:**
- 3.2.1 PENDING runPreview: POST to /v1/spec/preview, consume SSE stream via streamFetch()
- 3.2.2 PENDING Print each match as its SSE event arrives (streaming UX, not batch)
- 3.2.3 PENDING streamFetch() helper: fetch + getReader + SSE line protocol parser in lib/http.js
- 3.2.4 PENDING Handle stream errors and partial results gracefully (event: error → show partial + error message)

---

## 4.0 Verification

**Status:** PENDING

**Gates:**
- `make lint`
- `make test`

**Dimensions:**
- 4.1 PENDING POST /v1/spec/template returns valid spec markdown for Go, Rust, TS, Python repos (using manifest_files content)
- 4.2 PENDING POST /v1/spec/preview streams correct high/medium/low matches for known spec fixtures
- 4.3 PENDING CLI spec init writes server-returned template unchanged to disk; shows spinner during wait
- 4.4 PENDING CLI run --preview renders streamed matches in real-time (each event before done arrives)
- 4.5 PENDING spec init without token exits 1 with AUTH_REQUIRED, not a local template
- 4.6 PENDING LLM timeout on template → 503 with clear error message (not silent hang)
- 4.7 PENDING LLM timeout/error on preview → event: error emitted before stream closes

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 `zombiectl spec init` calls `/v1/spec/template`, writes returned template — no local generation
- [ ] 5.2 `zombiectl run --spec FILE --preview` streams file matches from `/v1/spec/preview` SSE
- [ ] 5.3 Language detection works for Go, Rust, TypeScript, Python, Elixir via manifest file contents
- [ ] 5.4 `spec init` without auth token exits 1 with AUTH_REQUIRED error
- [ ] 5.5 SSE stream renders each match as it arrives — user sees results before stream completes
- [ ] 5.6 All 377+ existing zombiectl tests pass; new API contract tests added
- [ ] 5.7 Template endpoint returns 503 (not hang) on LLM timeout; preview stream emits error event

---

## 6.0 Out of Scope

- Source file contents sent to server — only manifest file contents (go.mod, Cargo.toml etc.)
- Streaming the template token-by-token (batch JSON + spinner is sufficient for v1)
- Workspace-scoped file tree caching on the server
- Preview for repos not yet connected to a workspace (requires worktree walk milestone)
- Agent-driven directory ignore list (tracked separately)
- `extractSpecRefs()`/`matchRefsToFiles()` deletion (retained as @internal; deleted in follow-on cleanup after SSE is stable)
- Retry on LLM provider failure (server returns 503; client surfaces the error)

---

## 7.0 Implementation Notes

### Server (Zig)

New routes in `router.zig`:
```
spec_template,     // POST /v1/spec/template
spec_preview,      // POST /v1/spec/preview
```

Handlers call nullclaw agent directly — same pattern as `pipeline/agents_runner.zig`.
Use `res.chunk()` for SSE streaming (same as `runs/stream.zig`).
SSE headers: `Content-Type: text/event-stream`, `Cache-Control: no-cache`,
`Connection: keep-alive`, `X-Accel-Buffering: no`.

### CLI (JS)

New helper in `zombiectl/src/lib/http.js`:
```js
// streamFetch: POST to URL, consume SSE line protocol, call onEvent per event
export async function streamFetch(url, payload, headers, onEvent) {
  const res = await fetch(url, {
    method: "POST",
    headers: { ...headers, "Content-Type": "application/json", "Accept": "text/event-stream" },
    body: JSON.stringify(payload),
  });
  if (!res.ok) { /* throw ApiError */ }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  // parse SSE line protocol: event: / data: / blank line boundaries
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    // split on double-newline, parse event + data fields, call onEvent
  }
}
```

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | ISSUES OPEN | 10 issues, 4 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |

**UNRESOLVED:** 2 (manifest file resolution, extractSpecRefs removal timing)
**VERDICT:** ENG REVIEW OPEN — address 4 critical gaps before implementation.
