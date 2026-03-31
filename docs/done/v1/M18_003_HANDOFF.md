# M18_003 Handoff Notes

**Date:** Mar 31, 2026
**From:** M18_002 session (spec template + preview local implementation)
**To:** M18_003 session (API-backed spec template + SSE preview)

---

## Context

M18_002 shipped `zombiectl spec init` and `zombiectl run --preview` as purely
local CLI features. During review we identified the design problem:

- Language detection was hardcoded extension maps → removed
- Directory ignore lists were hardcoded per-language → removed
- File impact prediction is substring heuristics → should be LLM
- All logic ships in the CLI → server can't improve it without a release

The fix: CLI collects local filesystem facts, server does the thinking.
M18_003 moves both commands to API-backed with SSE streaming for preview.

## What Exists in the CLI (M18_002 state)

Files the new session will modify or remove:

| File | Current role | M18_003 action |
|---|---|---|
| `zombiectl/src/commands/spec_init.js` | local scanner + template generator | keep scanner, remove `generateTemplate()`, add API call |
| `zombiectl/src/commands/run_preview.js` | local ref extractor + matcher | keep `printPreview`/`confIndicator`/`sanitizeDisplay`, remove `extractSpecRefs`/`matchRefsToFiles`, add SSE consumer |
| `zombiectl/src/commands/run_preview_walk.js` | re-exports shared walkDir | keep — CLI still walks local tree |
| `zombiectl/src/lib/walk-dir.js` | shared BFS walker | keep — CLI still needs local file listing |

## Starting Instructions for M18_003 Session

**IMPORTANT: Section 0.0 Discovery must be completed before writing any code.**
The core architecture question — where does the agent run and how does it access
the user's local filesystem — is unresolved. See the spec for 3 candidate options.

1. Mark `docs/spec/v1/M18_003_API_BACKED_SPEC_PREVIEW.md` as IN_PROGRESS,
   add `**Branch:** m18-003-api-spec-preview`

2. Create worktree and branch from main:
   ```bash
   git worktree add .worktrees/m18-003 -b m18-003-api-spec-preview
   ```

3. **Complete Section 0.0 discovery first:**
   - Research how Amp, Claude Code, Cursor, Devin handle local file access
   - Measure pipeline overhead: `start_run → first SSE event` timing
   - Decide between Option A (local agent + remote LLM), Option B (file upload),
     Option C (chat window + MCP filesystem mount)
   - Record the decision in the spec before touching any code

4. Do NOT reference `commandRunWatch` — it does not exist in the CLI.
   There is no existing SSE consumer. `lib/http.js` uses `res.text()` (buffered).
   A new `streamFetch()` helper must be built using `fetch()` + `response.body.getReader()`.

5. Agents go through the full worker pipeline (start_run → sandbox → SSE back).
   nullclaw cannot be called directly from an HTTP handler. Study pipeline
   overhead before assuming it meets the 5-second first-result target.

6. Run full test suite before PR: `bun test zombiectl/test/` — 377 tests
   must pass, new API contract tests must be added.

## API Endpoints to Create on the Server

```
POST /api/spec/template   — returns generated template markdown
POST /api/spec/preview    — SSE stream of { file, confidence } events
```

Both require auth token. Add to server routes before CLI work.

## Current Branch / PR

- Branch: `m18-002-spec-templates-diff-preview`
- PR: usezombie/usezombie#115 (open, all CI passing)
- That PR covers M18_002 only. M18_003 gets its own PR from the new branch.
