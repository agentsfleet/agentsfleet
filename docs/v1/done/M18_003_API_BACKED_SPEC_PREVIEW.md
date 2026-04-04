# M18_003: API-Backed Spec Template Generation and SSE Preview

**Prototype:** v1.0.0
**Milestone:** M18
**Workstream:** 003
**Date:** Mar 31, 2026
**Status:** DONE
**Branch:** fix/deploy-executor-skip-worker-restart
**Priority:** P1 — move spec init and run preview from local heuristics to server-side agent with SSE streaming
**Batch:** B2 — depends on M18_002 (local implementation, now the reference spec)
**Depends on:** M18_002

---

## 0.0 Discovery — Architecture Decision (RESOLVED)

**Status:** DONE — Apr 01, 2026

### 0.1 The Problem

When you run `zombiectl spec init` or `zombiectl run --preview`, you want an agent
that actually understands your repo — not regex heuristics. The blocker: the agent
runs server-side but needs to read files on the user's laptop.

### 0.2 Competitive Research

| Tool | Where agent runs | How it gets files | First result latency |
|------|-----------------|-------------------|---------------------|
| Claude Code | CLI → Anthropic API direct | Native tool calls (read_file, list_dir, glob) executed locally by CLI | ~2s |
| OpenCode | CLI → Vercel AI SDK → Provider | Same pattern, AI SDK auto-executes tools locally | ~2s |
| Cursor | IDE process | IDE sends selected context upfront | ~1s |
| Devin | Remote sandbox | Clones from GitHub URL | ~30s |

**Key finding from source review (Claude Code at ~/Projects/claurst/, OpenCode at
~/Projects/opencode/):** Neither sends files upfront. Both send tool definitions with
the API request. The model decides what to read via tool calls. The CLI executes tools
locally and sends results back. The model typically reads 3-8 files total, not the
whole repo.

### 0.3 Architecture: Stateless Tool-Call Relay

zombied acts as a stateless relay between the CLI and the workspace's LLM provider.
This is the same pattern as Claude Code, but with zombied in the middle (because
LLM API keys are server-side secrets managed per-workspace).

```
zombiectl (CLI)                   zombied                     LLM Provider
─────────────────                 ──────                      ────────────
POST /v1/agent/stream
  { mode: "spec_init",
    messages: [user intent],
    tools: [read_file,
            list_dir, glob] } ──→ resolve workspace provider
                                  add system prompt + API key
                                  forward ───────────────────→
                                                           ←── tool_use: list_dir(".")
                              ←── SSE: event: tool_use
CLI runs list_dir locally
POST /v1/agent/stream
  { messages: [user intent,
    asst: tool_use(list_dir),
    user: tool_result(...)] } ──→ forward ───────────────────→
                                                           ←── tool_use: read_file("go.mod")
                              ←── SSE: event: tool_use
CLI reads go.mod locally
POST /v1/agent/stream
  { messages: [...accumulated,
    asst: tool_use(read_file),
    user: tool_result(...)] } ──→ forward ───────────────────→
                                                           ←── text: "# M5_001..."
                              ←── SSE: event: text_delta
                              ←── SSE: event: done { usage }
CLI writes spec to disk
```

**Key properties:**
- **Stateless:** CLI manages conversation history and resends full messages with each
  POST. zombied holds nothing between requests. Same as Anthropic's Messages API.
- **Provider-agnostic:** zombied resolves the LLM provider from workspace config (could
  be Anthropic, OpenAI, Google, or user's own key). CLI never knows or cares.
- **Files never leave the laptop** unless the model explicitly asks via tool calls.
  Model reads 3-8 files total, not the whole repo.
- **Protocol:** Multi-request loop over HTTP/1.1 keep-alive. httpz only supports
  HTTP/1.1; Cloudflare Tunnel handles HTTP/2 at the edge.

### 0.4 Decision Gate

```
CHOSEN: Stateless tool-call relay (Option C-relay via zombied)

RATIONALE:
- Matches how Claude Code and OpenCode work (verified from source).
- Model decides what to read, not the CLI. No upfront context guessing.
- Files stay on the user's laptop. Only sent one-at-a-time when model asks.
- zombied is a pure pass-through: adds API key + system prompt, forwards,
  streams back. No server state, no sandbox, no job queue.
- Provider-agnostic: workspace config determines which LLM provider to call.

REJECTED:
- Option A (CLI packs context upfront): CLI must guess what to include.
  If it guesses wrong, model works with incomplete context. No way to ask
  for more. Neither Claude Code nor OpenCode does this for exploration tasks.
- Option B (upload bundle to sandbox): 15-40s pipeline overhead for a 5-second
  operation. Files leave the user's machine. Massive overkill.
- Option C-direct (CLI calls LLM API directly): LLM API keys are server-side
  secrets managed per-workspace. CLI has no access to key material.
```

### 0.5 Dimensions

- 0.1 DONE Researched Claude Code source (~/Projects/claurst/) — tool loop in query/src/lib.rs:140-416, tools in crates/tools/src/
- 0.2 DONE Researched OpenCode source (~/Projects/opencode/) — tool loop in session/prompt.ts runLoop(), Vercel AI SDK handles tool execution
- 0.3 DONE Architecture decided: stateless tool-call relay through zombied
- 0.4 DONE Protocol decided: multi-request HTTP/1.1 keep-alive (httpz constraint, Cloudflare handles HTTP/2 at edge)

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

### 1.1 Alternatives to Track

| Tool | What it does | Relevant to |
|------|-------------|-------------|
| fission.ai | ? — research needed | spec generation from intent |
| gstack `/office-hours` | YC-style problem framing + plan | spec problem statement |
| GitHub Copilot Workspace | intent → plan → code | closest end-to-end analog |

---

## 2.0 API Design

Both endpoints use the same **stateless tool-call relay** mechanism. zombied is a
pass-through: adds system prompt + workspace API key, forwards to the configured
provider, streams SSE back. CLI manages conversation history and resends with each POST.

### 2.1 Spec Template Generation

```
POST /v1/spec/template
Authorization: Bearer <token>
Content-Type: application/json

{
  "messages": [
    {
      "role": "user",
      "content": "Generate a spec template for: Add rate limiting per API key with Redis backend"
    }
  ],
  "tools": [
    {
      "name": "read_file",
      "description": "Read a file from the user's repo",
      "input_schema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }
    },
    {
      "name": "list_dir",
      "description": "List directory contents",
      "input_schema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }
    },
    {
      "name": "glob",
      "description": "Find files matching a glob pattern",
      "input_schema": { "type": "object", "properties": { "pattern": { "type": "string" } }, "required": ["pattern"] }
    }
  ]
}

→ 200 text/event-stream
```

**System prompt (server-side, not sent by CLI):** "You are a spec generation agent.
Explore the repo to understand language, ecosystem, structure. Generate a milestone
spec using the canonical format."

### 2.2 Impact Preview

```
POST /v1/spec/preview
Authorization: Bearer <token>
Content-Type: application/json

{
  "messages": [
    {
      "role": "user",
      "content": "Which files will this spec touch?\n\n# M5_001: Rate Limiting..."
    }
  ],
  "tools": [ ... same tool definitions ... ]
}

→ 200 text/event-stream
```

**System prompt (server-side):** "You are a blast radius analyzer. Read the spec,
explore the repo, and predict which files the agent will touch. Output each match
with confidence (high, medium, low)."

### 2.3 Shared Relay Protocol

Both endpoints use identical SSE event types and the same stateless round-trip loop.

**SSE event types:**

```
event: tool_use
data: {"id":"tu_01","name":"read_file","input":{"path":"go.mod"}}

event: text_delta
data: {"text":"# M5_001: Rate Limiting\n\n"}

event: done
data: {"usage":{"input_tokens":12450,"output_tokens":3200,"cost_usd":0.085,"provider":"anthropic","model":"claude-sonnet-4-6","round_trips":4}}

event: error
data: {"message":"provider timeout after 30s"}
```

**Stateless round trip (same for both endpoints):**
1. CLI POSTs `{ messages, tools }` → zombied adds system prompt + API key → forwards to provider
2. Provider returns `tool_use` → zombied streams SSE `event: tool_use` back to CLI
3. CLI executes tool locally (reads file from laptop)
4. CLI appends assistant `tool_use` + user `tool_result` to message history
5. CLI POSTs updated `{ messages }` to same endpoint → goto 2
6. Provider returns text → zombied streams `event: text_delta`
7. Provider finishes → zombied streams `event: done` with usage

**Provider resolution:** zombied resolves the LLM provider from workspace config
(Anthropic, OpenAI, Google, or user's own key). The CLI never sees or specifies
the provider. `RuntimeProviderBundle` in `agents_runner.zig` handles this.

**Dimensions:**
- 2.1.1 DONE POST /v1/spec/template — relay handler with spec generation system prompt
- 2.2.1 DONE POST /v1/spec/preview — relay handler with blast radius system prompt
- 2.3.1 DONE Shared relay logic: resolve provider, forward messages + tools, stream SSE back
- 2.3.2 DONE SSE events: tool_use, text_delta, done (with usage), error
- 2.3.3 DONE Provider-agnostic: workspace config determines which LLM to call, cost calculated server-side

### 2.4 What the User Sees

**spec init:**
```
$ zombiectl spec init --describe "Add rate limiting with Redis"

  🧟 analyzing your repo...
  → listed ./             (root structure)
  → read go.mod           (Go 1.21, github.com/acme/api)
  → read Makefile          (lint, test, build)
  → listed src/            (4 dirs, 23 files)

  🧟 drafting spec...

  ✓ wrote docs/spec/v1/pending/M5_001_RATE_LIMITING.md
    4.8s | 4 reads | 15.6K tokens | $0.09
```

**run --preview:**
```
$ zombiectl run --spec M5_001_RATE_LIMITING.md --preview

  🧟 analyzing your repo against spec...
  → read M5_001_RATE_LIMITING.md
  → listed src/http/       (middleware, handlers)
  → read src/http/middleware.go
  → listed src/redis/      (client)
  → grep "docker-compose" .

  ● src/http/middleware.go       high
  ● src/redis/client.go          high
  ◆ src/config/config.go         medium
  ◆ docker-compose.yml           medium
  ○ src/http/handler.go          low

  5 file(s) in blast radius
    5.2s | 5 reads | 18.3K tokens | $0.11
```

Each `→` line appears in real-time as the model makes tool calls. The user sees the
agent exploring their repo, not a blind spinner.

---

## 3.0 CLI Implementation

### 3.1 Tool Call Loop

New core primitive: the agent loop. CLI sends messages, receives SSE events, executes
tool calls locally, sends results back, repeats until done.

```js
// Pseudo-code for the agent loop
async function agentLoop(endpoint, userMessage, repoRoot, ctx) {
  const tools = [readFileTool, listDirTool, globTool];
  let messages = [{ role: "user", content: userMessage }];
  let toolCalls = 0;
  const startTime = Date.now();

  while (toolCalls < MAX_TOOL_CALLS && (Date.now() - startTime) < MAX_WALL_MS) {
    const events = await streamFetch(endpoint, { messages, tools }, ctx);

    for (const event of events) {
      if (event.type === "tool_use") {
        toolCalls++;
        renderToolCall(event);  // → read go.mod
        const result = executeLocally(event.name, event.input, repoRoot);
        messages.push({ role: "assistant", content: [{ type: "tool_use", ...event }] });
        messages.push({ role: "user", content: [{ type: "tool_result", tool_use_id: event.id, content: result }] });
        break;  // POST again with accumulated messages
      }
      if (event.type === "text_delta") { renderText(event.text); }
      if (event.type === "done") { renderUsage(event.usage); return; }
      if (event.type === "error") { renderError(event.message); return; }
    }
  }
}
```

**Guardrails (CLI-enforced):**
- Max 10 tool calls per session (spec init needs 3-5, preview needs 4-8)
- Max 30s total wall time
- If either limit hit: render partial result + warning message

**Dimensions:**
- 3.1.1 DONE agentLoop(): POST → SSE → tool_use → execute locally → POST again → repeat
- 3.1.2 DONE Guardrails: max 10 tool calls + 30s wall time, partial result on limit
- 3.1.3 DONE Real-time rendering: each tool call shown as → line, text streamed as it arrives

### 3.2 Local Tool Executors

Three read-only tools executed by the CLI on the user's laptop:

**read_file(path):** Read file contents. Resolve path against repo root. Reject
if resolved path escapes repo root (path traversal prevention). Return file
contents as string, or error if file not found or path rejected.

**list_dir(path):** List directory entries. Same path validation. Return entries
as newline-separated list with trailing `/` for directories.

**glob(pattern):** Find files matching glob pattern within repo root. Return
matching paths as newline-separated list. Limit to 500 results.

**Security boundary (CLI-side only):**
```js
function validatePath(inputPath, repoRoot) {
  const resolved = path.resolve(repoRoot, inputPath);
  if (!resolved.startsWith(repoRoot + path.sep) && resolved !== repoRoot) {
    return { error: "path outside repo root" };
  }
  return { resolved };
}
```

zombied has no path awareness — it's a relay. Only the CLI knows the repo root and
can enforce path safety. This matches how Claude Code and OpenCode handle it.

**Dimensions:**
- 3.2.1 DONE read_file: read file, validate path against repo root, reject traversal
- 3.2.2 DONE list_dir: list directory entries within repo root
- 3.2.3 DONE glob: match files within repo root, limit 500 results
- 3.2.4 DONE Path traversal prevention: resolve + startsWith check on all tools

### 3.3 streamFetch Helper

New SSE consumer in `zombiectl/src/lib/http.js`. The CLI has no existing SSE
consumption capability.

```js
export async function streamFetch(url, payload, headers, onEvent) {
  const res = await fetch(url, {
    method: "POST",
    headers: { ...headers, "Content-Type": "application/json", "Accept": "text/event-stream" },
    body: JSON.stringify(payload),
  });
  if (!res.ok) { throw new ApiError(...); }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    let boundary;
    while ((boundary = buf.indexOf("\n\n")) !== -1) {
      const frame = buf.slice(0, boundary);
      buf = buf.slice(boundary + 2);
      const event = parseSseFrame(frame);  // { type, data }
      if (event) onEvent(event);
    }
  }
}
```

**Dimensions:**
- 3.3.1 DONE streamFetch(): POST + getReader + SSE line protocol parser
- 3.3.2 DONE Handle multi-chunk reads (data split across TCP segments)
- 3.3.3 DONE Non-200 response → throw ApiError before streaming

### 3.4 Command Migration

**spec init:**
- `commandSpecInit` now calls `agentLoop("/v1/spec/template", userIntent, repoRoot, ctx)`
- Remove `generateTemplate()` and local template string — server agent generates it
- `spec init` becomes auth-required: remove `spec.init` from `AUTH_EXEMPT_ROUTES`
- Output: writes agent-generated spec to disk, displays usage stats

**run --preview:**
- `runPreview` now calls `agentLoop("/v1/spec/preview", specContent, repoRoot, ctx)`
- Delete `extractSpecRefs()`, `matchRefsToFiles()`, `scoreMatch()`, `walkDirForPreview()`
  — clean break, no @internal retention
- Retain `printPreview()`, `confIndicator()`, `sanitizeDisplay()` for output formatting
- Preview collects matches from the agent's text output and renders with confidence indicators

**Dimensions:**
- 3.4.1 DONE commandSpecInit: use agentLoop with --describe flag, local fallback without it
- 3.4.2 DONE runPreview: use agentLoop when workspaceId available, local fallback for offline
- 3.4.3 DONE Retain output formatting functions (printPreview, confIndicator, sanitizeDisplay)

---

## 4.0 Verification

**Status:** DONE

**Gates:**
- `make lint`
- `make test`

**Dimensions:**
- 4.1 DONE POST /v1/spec/template returns valid SSE stream with tool_use + text events
- 4.2 DONE POST /v1/spec/preview returns SSE stream with tool_use + match results
- 4.3 DONE CLI tool call loop: POST → tool_use → execute locally → continue → done
- 4.4 DONE CLI path traversal: read_file("../../.ssh/id_rsa") returns error, not file contents (13 tests)
- 4.5 DONE CLI guardrails: max 10 tool calls enforced, partial result rendered
- 4.6 DONE CLI guardrails: 30s timeout enforced, partial result rendered
- 4.7 DONE spec init without token falls back to local template (auth enforced server-side on relay endpoints)
- 4.8 DONE Provider timeout → SSE error event emitted before stream closes
- 4.9 DONE Usage stats (tokens, round trips) displayed in CLI output

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 `zombiectl spec init --describe "..."` calls `/v1/spec/template`, agent explores repo via tool calls, writes generated spec — local fallback without --describe
- [x] 5.2 `zombiectl run --spec FILE --preview` calls `/v1/spec/preview`, agent reads spec + explores repo, outputs file matches with confidence
- [x] 5.3 Each tool call (read_file, list_dir, glob) renders as a `→` line in real-time
- [x] 5.4 `spec init` without auth falls back to local template; relay endpoints are auth-gated server-side
- [x] 5.5 Path traversal attempts are rejected by CLI before any file read (13 unit tests)
- [x] 5.6 Agent sessions respect max 10 tool calls + 30s wall time; partial results shown on limit
- [x] 5.7 Usage stats (tokens, provider, model, round trips) shown after completion
- [x] 5.8 All existing tests pass; 57 new tests for tool loop, SSE parser, path validation, streamFetch
- [x] 5.9 Provider timeout/error → SSE error event, CLI shows clear error message

---

## 6.0 Out of Scope

- Write tools (agent can only read files, not modify via relay)
- Full agent execution (`zombiectl run`) — stays on the pipeline/sandbox path
- Provider selection UI (workspace config is pre-existing)
- HTTP/2 support in httpz (separate infra milestone if needed)
- WebSocket upgrade path (defer until needed)
- Retry on LLM provider failure (zombied returns error event, CLI surfaces it)
- Background/async agent sessions (spec init + preview are short-lived, synchronous)
- Server-side session state (CLI manages conversation history, zombied is stateless)

---

## 7.0 Implementation Notes

### Server (Zig)

New routes in `router.zig`:
```
spec_template,    // POST /v1/spec/template
spec_preview,     // POST /v1/spec/preview
```

Both routes dispatch to a shared `handleAgentRelay()` function with different system prompts.

Handler flow:
1. Parse JSON body: extract `messages`, `tools`
2. Resolve workspace provider via `RuntimeProviderBundle` (same as `agents_runner.zig`)
3. Attach the route-specific system prompt (spec generation or blast radius)
4. Forward `{ system, messages, tools }` to provider API
6. Stream provider response back as SSE via `res.chunk()`:
   - Provider `tool_use` blocks → `event: tool_use\ndata: {...}\n\n`
   - Provider text deltas → `event: text_delta\ndata: {...}\n\n`
   - Provider finish → `event: done\ndata: {usage}\n\n`
   - Provider error → `event: error\ndata: {message}\n\n`

SSE headers (same pattern as `runs/stream.zig`):
```
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
X-Accel-Buffering: no
```

**Cost tracking:** Each provider API response includes usage (input_tokens, output_tokens).
zombied knows the provider and model pricing. The `done` event includes accumulated cost
across all round trips for the session. zombied records total usage against workspace billing.

### CLI (JS)

New files:
- `zombiectl/src/lib/agent-loop.js` — agentLoop() orchestrator
- `zombiectl/src/lib/tool-executors.js` — read_file, list_dir, glob with path validation
- `zombiectl/src/lib/sse-parser.js` — SSE line protocol parser (or inline in streamFetch)

Modified files:
- `zombiectl/src/lib/http.js` — add streamFetch()
- `zombiectl/src/commands/spec_init.js` — replace generateTemplate() with agentLoop()
- `zombiectl/src/commands/run_preview.js` — replace regex heuristics with agentLoop()
- `zombiectl/src/commands/core.js` — update run --preview integration
- `zombiectl/src/cli.js` — remove spec.init from AUTH_EXEMPT_ROUTES

Deleted code:
- `generateTemplate()` in spec_init.js
- `extractSpecRefs()`, `matchRefsToFiles()`, `scoreMatch()`, `walkDirForPreview()` in run_preview.js

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 2 | ISSUES RESOLVED | 8 issues, 0 critical gaps — full rewrite to relay architecture |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |

**UNRESOLVED:** 0
**DECISIONS MADE:**
1. Option A → relay architecture (zombied as stateless tool-call proxy)
2. Protocol: multi-request HTTP/1.1 keep-alive (httpz constraint)
3. Session state: stateless, CLI manages conversation history
4. Endpoints: two routes POST /v1/spec/template + POST /v1/spec/preview, shared relay logic
5. Provider: workspace config, not hardcoded Anthropic
6. Security: CLI-side path validation only (matches Claude Code/OpenCode)
7. Guardrails: max 10 tool calls + 30s total timeout
8. Dead code: delete regex heuristics immediately (no @internal retention)
**VERDICT:** ENG REVIEW CLEAR — spec fully rewritten to relay architecture.
**NOTE:** DONE — Architecture doc (docs/contributing/architecture.md) agent relay section added.
