# M18_003 Discovery Handoff

**Date:** Mar 31, 2026
**Purpose:** Personal handoff — continue the discovery session for M18_003
**Spec:** `docs/spec/v1/pending/M18_003_API_BACKED_SPEC_PREVIEW.md` Section 0.0
**Branch:** m18-002-spec-templates-diff-preview (spec lives here until M18_003 branch is cut)

---

## Where We Left Off

M18_003 wants an agent to generate spec templates and predict file impact instead of
the current local heuristics. But we hit the fundamental question:

**How does the agent see the user's local repo?**

Three options are in the spec (Section 0.0). None is chosen yet. This handoff
is for you to research and decide.

---

## The Three Options (brief)

**Option A — Local agent, LLM proxy through zombied**
CLI reads local files, sends context to zombied, zombied calls Anthropic, streams
tokens back. Same pattern as Claude Code. Fastest. No sandbox. Server keeps visibility.

**Option B — File bundle upload to sandbox**
CLI bundles manifest files, uploads before the run. Agent runs in existing worker
sandbox with real file access. Heavyweight — pipeline overhead ~15-40s needs measuring.

**Option C — Preview window with local MCP filesystem**
CLI hosts a local MCP server. Agent (remote LLM) calls `read_file`, `list_dir` tools
that resolve against the user's local disk. Most powerful. Most complex.

---

## Your Discovery Session Plan

Work through these in order. Each produces an answer you record back in the spec.

### Step 1 — Feel it from the user's perspective (30 min)

Before reading any source code, use the tools as a user:

1. Open a repo on your laptop. Run `claude` (Claude Code). Ask it:
   > "What language is this project? List the files most likely to be touched
   > if I add rate limiting to the API."

   Watch: How fast does it respond? What does it show while thinking? How does it
   get file context — did it read files, or did you send them?

2. Open Amp Code (if installed). Same question. Same repo.
   Watch the same things. Note any UX differences.

3. Try Aider (if available): `aider --message "what language is this repo?"`
   Note: does it read files before answering, or just look at the directory?

**Write down:** What felt fast? What felt slow? What did you want to see that you
didn't? That feeling is your product bar for M18_003.

---

### Step 2 — Understand how Claude Code does it (45 min)

Claude Code's filesystem tools are the closest precedent to Option C.

**Read:**
```bash
# Claude Code MCP filesystem tools source
ls ~/.claude/skills/
cat ~/.claude/skills/gstack/ETHOS.md   # philosophy
```

**Research questions:**
- When you ask Claude Code to read a file, what actually happens?
  (Is it a tool call? A pre-sent context? Both?)
- What is the tool call cycle? (model asks → CLI executes → model gets result → model continues)
- How does Claude Code decide WHICH files to read vs. which to ignore?
- What is the latency of one `read_file` tool call round-trip?
  (Time a `cat file.go` vs. asking Claude Code to read the same file)

**Relevant Claude documentation:**
- Tool use / function calling: how the model requests tool execution
- MCP (Model Context Protocol): how MCP servers expose tools to Claude

**Write down:** A one-paragraph description of the Claude Code tool call cycle.
This becomes the basis for Option C's implementation design.

---

### Step 3 — Measure the zombied pipeline (30 min)

Before you can rule out Option B, you need the real numbers.

**Start your local zombied instance** and run:

```bash
# Time a minimal run from submission to first SSE event
time zombiectl run --spec docs/spec/v1/M18_003_API_BACKED_SPEC_PREVIEW.md

# Watch the SSE stream directly
curl -N -H "Authorization: Bearer $(cat ~/.zombie/token)" \
  "$(cat ~/.zombie/api_url)/v1/runs/<run_id>/stream"
```

**Record:**
- Time from `zombiectl run` to first printed output: ___s
- Time from `zombiectl run` to `run_complete` event: ___s
- Subjectively: does it feel instant, acceptable, or slow?

**The bar:** `zombiectl spec init` needs to feel like running `git status`.
Not like waiting for a CI run. If the pipeline is >3s to first output, Option B
is unlikely to meet the bar without a purpose-built lightweight path.

---

### Step 4 — Research Amp's local agent pattern (30 min)

Amp Code is the closest product to zombiectl's design (CLI-first, agent-native).

**Find:**
- Does Amp open source any of their agent loop code?
- How does Amp handle `amp --preview` or similar local analysis commands?
- Does Amp run the agent locally (in the CLI process) or remotely (via their server)?
- What does the Amp streaming UX look like in the terminal?

**Sources to check:**
- Amp's public documentation and changelog
- Any open source repos from the Amp team
- GitHub issues / Discord if available

**Key question:** Does Amp have a "preview" or "analyze" command that works without
pushing to their server? If yes, that's Option A/C territory. If no, that's Option B.

---

### Step 5 — Write your decision (30 min)

After steps 1-4, fill in the decision gate in the spec:
`docs/spec/v1/M18_003_API_BACKED_SPEC_PREVIEW.md` → Section 0.4

The decision format is already there. Fill it in. Then update sections 2.0-3.0
to reflect the chosen option's architecture.

If you're torn between options, here's the forcing question:

> **"Should zombiectl spec init feel like git status (instant, local)
>   or like submitting a CI run (queued, server-side)?"**

If the answer is "git status": Option A or C.
If the answer is "CI run is fine, I want the server to have all the context": Option B.

---

## What's Already Built (don't rebuild)

| Component | Location | Status |
|---|---|---|
| BFS directory walker | `zombiectl/src/lib/walk-dir.js` | Done |
| Makefile parser | `zombiectl/src/commands/spec_init.js:parseMakeTargets` | Done |
| Test pattern detector | `zombiectl/src/commands/spec_init.js:detectTestPatterns` | Done |
| Confidence renderer | `zombiectl/src/commands/run_preview.js:confIndicator` | Done |
| ANSI sanitizer | `zombiectl/src/commands/run_preview.js:sanitizeDisplay` | Done |
| Spinner | `zombiectl/src/ui-progress.js` | Done |
| SSE server (runs) | `src/http/handlers/runs/stream.zig` | Done — reference for Option B/C |
| HTTP client (CLI) | `zombiectl/src/lib/http.js` | Done — needs streaming extension for Option A/C |
| Auth guard | `zombiectl/src/program/auth-guard.js` | Done |

The local scanner (spec_init.js:scanRepo) is kept from M18_002. It's the right
starting point regardless of which option is chosen — it produces the metadata
(file_paths, makefile_targets, test_patterns) that gets sent or used locally.

---

## Questions to Answer Before Cutting the M18_003 Branch

When you're done with discovery, you should be able to answer:

1. Which option (A/B/C)?
2. Where does the agent run? (CLI process / zombied HTTP handler / worker sandbox)
3. How does the agent get file contents? (tool calls / POST body / mounted bundle)
4. What does the user see while waiting? (spinner / streaming tokens / step-by-step)
5. What is the SSE pattern? (zombied proxy / run stream / direct Anthropic stream)
6. Does spec init require auth? (yes — spec says so, all three options agree on this)

Once those 6 are answered, the M18_003 implementation spec writes itself.

---

## If You Want a Recommendation Now

**Option A is my recommendation** for v1.

It's the simplest path, solves the filesystem problem without any new infrastructure,
gives the agent real context (manifest file contents + file tree), and matches how
every successful local-first AI tool works. The key design point: zombied proxies the
LLM call — the Anthropic API key stays on the server, the server logs what was
generated, and the CLI gets a streaming response it renders.

Option C is where this ends up in v2 — once you want the agent to interactively
browse files, call `glob`, and reason step by step. Build Option A first, then
upgrade the agent call to a tool-call loop later.

Option B is the right answer only if the pipeline overhead is <2s, which is unlikely.

But you've used these tools. Your instinct about the UX is more important than my
architectural preference. Step 1 of this plan will tell you which direction to go.
