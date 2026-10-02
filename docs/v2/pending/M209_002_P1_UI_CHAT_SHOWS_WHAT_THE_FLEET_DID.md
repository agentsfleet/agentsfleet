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

# M209_002: The fleet chat shows each tool call as a verb and a target with ✓ or ✗ and a five-line preview, folds reads under Explored, keeps the rows after a reload, and puts time and cost under every reply

**Prototype:** v2.0.0
**Milestone:** M209
**Workstream:** 002
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: the thread is where a person decides whether to trust a fleet, and today it cannot show what the fleet did
**Categories:** UI
**Batch:** B2 — starts once M209_001's wire and detail shapes are on `main`; the milestone's follow-up Pull Request
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M209_001 — the frame outcome fields and `EventDetail.tool_calls` this spec renders
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main` at `93e96897a`; assistant-ui claims read from the installed `@assistant-ui/react` 0.15.22 / core 0.3.21
**Canonical architecture:** `docs/architecture/user_flow.md` §chat surface — amended in the authoring commit; `docs/architecture/runner_fleet.md` §Live activity

---

## Overview

**Goal (testable):** A reply whose fleet read two files, wrote one and failed an HTTP call renders "Explored · Read a.md, b.md", "Wrote c.md ✓", "POST https://… ✗" with its error line, then the answer and a "time · tokens · cost · wall" line; after a reload the same rows render from the saved trace.
**Problem:** Every call renders as a glyph, a bare tool name and a clock (`components/domain/FleetToolCalls.tsx:31-47`). Arguments are hard-coded to `{}` and the result to `null` (`components/domain/fleetReplyMessage.ts:32-35,103-104`). A reload loses every row (`lib/streaming/fleet-stream-frames.ts:296` keeps only live ones). Each turn's tokens, cost and wall time reach the row (`lib/streaming/fleet-stream-row.ts:173-175`) but only the newest run's show, in the status line.
**Solution summary:** Frontend only. The live reducer keeps each call's arguments and outcome; the settle path swaps in the saved trace from the detail read. Tool-call parts carry real `args`, `result` and `isError`. A per-tool copy map renders verb and target, and assistant-ui's `groupPartByType` folds read-class calls under one Explored row. A settled reply gains a figures line built from the status line's formatters, moved to a shared module.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(app): show each tool call's target, outcome and output in the fleet chat
- **Intent (one sentence):** A person reading a fleet's thread can see what each tool call touched, whether it worked, what came back, and what the turn cost, live or after a reload.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/components/domain/FleetReplyBody.tsx` — the `GroupedParts` switch and the module-constant group map this spec extends.
2. `ui/packages/app/lib/streaming/fleet-stream-tool-frames.ts` — frame ingress, call-id pairing, and the repeat-start rule that now merges arguments.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx` — the token, cost and duration formatters the reply line shares.
4. `docs/v2/done/M207_001_P1_UI_CHAT_REPLY_ON_ASSISTANT_UI_PARTS.md` — the parts model, the tool row's clock and the tests that pin it.
5. https://github.com/assistant-ui/assistant-ui/tree/%40assistant-ui/react%400.15.22/packages/core/src/react/utils — `groupParts` (`tool-call:<name>` outranks `tool-call`).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/api/events.ts` | EDIT | `EventDetail.tool_calls` and its trace type, mirroring M209_001's interface |
| `ui/packages/app/lib/streaming/fleet-stream-row.ts`, `ui/packages/app/lib/streaming/fleet-stream-tool-frames.ts`, `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | `FleetToolCall` gains `args`, `ok`, `preview`, `outputLines`; repeat start merges; settle prefers the saved trace |
| `ui/packages/app/lib/streaming/fleet-stream-tool-trace.ts` | CREATE | Narrows a saved trace into tool calls at the parse boundary |
| `ui/packages/app/components/domain/fleetReplyMessage.ts` | EDIT | Parts carry `args`, `argsText`, `result`, `isError`; reply custom bag carries the turn's figures |
| `ui/packages/app/components/domain/tool-call-copy.ts` | CREATE | Verb, target field and explore class per hosted tool, as named constants |
| `ui/packages/app/components/domain/FleetToolCalls.tsx`, `ui/packages/app/components/domain/FleetReplyBody.tsx` | EDIT | Verb-and-target row, outcome glyph, preview, argument disclosure, Explored group |
| `ui/packages/app/components/domain/FleetReplyFigures.tsx`, `ui/packages/app/lib/events/run-figures-format.ts` | CREATE | The settled reply's figures line; formatters moved out of the status line |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx`, `ui/packages/app/components/domain/FleetMessageRow.tsx`, `ui/packages/app/components/domain/fleetMessageReaders.ts` | EDIT | Status line imports the shared formatters; turn rows show their timestamp; figure readers |
| `ui/packages/app/tests/fleet-tool-calls.test.tsx`, `ui/packages/app/components/domain/fleetReplyMessage.test.ts`, `ui/packages/app/components/domain/FleetReplyBody.test.tsx`, `ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts`, `ui/packages/app/tests/fleet-thread/role-reply-parts.test.ts`, `ui/packages/app/tests/e2e/acceptance/fleet-reply-parts.spec.ts` | EDIT | Existing pins move to the new rows (the `{}`/`null` and repeat-start no-op pins change on purpose) |
| `ui/packages/app/components/domain/tool-call-copy.test.ts`, `ui/packages/app/lib/streaming/fleet-stream-tool-trace.test.ts`, `ui/packages/app/components/domain/FleetReplyFigures.test.tsx`, `ui/packages/app/lib/events/run-figures-format.test.ts` | CREATE | Unit proofs for the new modules |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (verbs, glyphs, line caps and group keys are named constants), NDC + ORP (`NO_ARGS`, `NO_OUTPUT` and the status line's private formatters leave with every reference), NTP (the saved trace is narrowed once at ingress), PJV, TCF, TST-NAM, HLP (the shared formatter module ships with both consumers).
- `dispatch/write_ts_adhere_bun.md` — `TS FILE SHAPE DECISION` at PLAN for the three new modules; design-system primitives over raw HTML (UIS); token utilities over arbitrary values (DTK).
- `docs/architecture/web_app.md` — the dashboard's rendering and data-loading conventions the thread follows.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI GATE | yes — new row and figures markup | `List`/`ListItem`, `Accordion`, `Time` from `@agentsfleet/design-system`; no raw `<details>` |
| DESIGN TOKEN GATE | yes | `text-success`, `text-destructive`, `text-pulse`, `text-muted-foreground`, `font-mono text-label`; no arbitrary values |
| UFS GATE | yes | Copy map, glyphs (`◐ ✓ ✗ •`), `PREVIEW_MAX_LINES` and group keys are constants |
| LENGTH GATE (≤350/≤50/≤70) | yes — `FleetReplyBody.tsx`, `FleetToolCalls.tsx` | Row and group render in `FleetToolCalls.tsx`; copy and parsing in their own modules |
| MILESTONE-ID GATE | yes | No milestone identifiers in source or test names |
| Architecture consult | yes | `user_flow.md` §chat surface landed with this spec |

## Prior-Art / Reference Implementations

- **Reference:** Codex `codex-rs/tui/src/exec_cell/model.rs:233-244` and `render.rs:313-466` (`~/Projects/oss/rs/codex`) — a call folds into Explored when it only reads, lists or searches; the group flushes on any other call or on text; a non-zero exit turns the bullet red. Diverges: verbs come from the tool name, since hosted tools take structured arguments and no shell command exists to parse.
- **Reference:** `ui/packages/app/components/domain/FleetThought.tsx` — the Accordion disclosure and live-versus-settled labelling the Explored row mirrors.

## Sections (implementation slices)

### §1 — The live reducer keeps each call's arguments and outcome

`readToolStep` reads `args_redacted` when it is a JSON object, and `ok`, `preview`, `output_lines` when well-typed; anything else reads as absent. A repeat start under an open call id merges the arguments and keeps the first start's clock: the runner re-emits the start once it has the arguments (M209_001 §1).

- **Dimension 1.1** — A start with object arguments stores them on its call → Test `test_started_frame_keeps_arguments`
- **Dimension 1.2** — A repeat start under the same call id merges arguments and keeps the start clock → Test `test_repeat_start_merges_arguments_keeps_clock`
- **Dimension 1.3** — A completion's `ok`, `preview` and `output_lines` land on the call → Test `test_completed_frame_keeps_outcome`
- **Dimension 1.4** — Array arguments, non-boolean `ok`, non-string `preview` or a negative count read as absent → Test `test_malformed_tool_fields_read_absent`

### §2 — Saved calls replace live ones at settle and after a reload

`rowToEvent` maps `tool_calls` into `FleetToolCall[]` through one narrowing function. **Implementation default:** a saved call's start is the event's `createdAt`, because only the difference to its `ms` renders. At settle the saved list replaces the live one when present; `null` keeps the live rows.

- **Dimension 2.1** — A detail row's saved trace becomes calls with arguments, outcome and duration → Test `test_saved_trace_becomes_tool_calls`
- **Dimension 2.2** — A malformed saved trace reads as no calls without throwing → Test `test_malformed_saved_trace_reads_absent`
- **Dimension 2.3** — Settle prefers a saved trace and keeps live rows when the trace is `null` → Test `test_settle_prefers_saved_trace`
- **Dimension 2.4** — `omitted` above zero renders "N more calls not recorded" → Test `test_omitted_calls_render_count`
- **Dimension 2.5** — Reloading a settled turn shows the same rows → Test `test_reload_shows_saved_tool_rows`

### §3 — Each call reads as a verb and a target, with its outcome

`tool-call-copy.ts` maps all thirteen hosted tools: `file_read`/`file_read_hashed` → Read ·path·, `file_write` → Wrote, `file_edit`/`file_edit_hashed` → Edited, `file_append` → Appended, `file_delete` → Deleted, `http_request` → ·method· ·url·, `memory_store` → Remembered ·key·, `memory_recall` → Recalled ·query·, `memory_list` → Listed memory ·category·, `memory_forget` → Forgot ·key·, `calculator` → Calculated ·operation·. Glyphs: ◐ running, ✓ `ok`, ✗ failed (`text-destructive`), • done with no recorded outcome. Up to five preview lines render monospace; `output_lines` beyond them renders "… +N lines". The full arguments sit behind a collapsed Accordion.

- **Dimension 3.1** — Each hosted tool renders its verb and target → Test `test_tool_row_names_verb_and_target`
- **Dimension 3.2** — An unknown tool renders its name and a compact argument summary → Test `test_unknown_tool_row_falls_back_to_name`
- **Dimension 3.3** — `ok` renders ✓, failure ✗ with its error line, absent outcome • → Test `test_tool_row_marks_outcome`
- **Dimension 3.4** — Twelve output lines render five plus "… +7 lines" → Test `test_tool_row_previews_five_lines`
- **Dimension 3.5** — Arguments stay collapsed until the disclosure opens → Test `test_tool_row_discloses_arguments`
- **Dimension 3.6** — A live run in the real page shows "Read README.md ✓" and its preview → Test `test_live_tool_row_reads_verb_target_preview`

### §4 — Reads fold under Explored

The group map sends `tool-call:file_read`, `tool-call:file_read_hashed`, `tool-call:memory_recall` and `tool-call:memory_list` to an explore group, and every other `tool-call` to the tool group, so adjacent reads fold and any other call breaks the run. The row reads "Exploring" while a folded call runs and "Explored" after; consecutive file reads share one "Read a, b" line.

- **Dimension 4.1** — Three consecutive reads render one Explored row naming all three → Test `test_consecutive_reads_fold_under_explored`
- **Dimension 4.2** — A write between reads yields two Explored rows around it → Test `test_non_read_call_splits_explored`
- **Dimension 4.3** — The label reads Exploring while any folded call runs → Test `test_explored_label_tracks_running`
- **Dimension 4.4** — A failed read inside the group marks its own line ✗ → Test `test_failed_read_marks_its_explored_line`

### §5 — Each settled reply shows when it ran and what it cost

`toReplyMessage` carries the turn's tokens, wall time and cost in the reply's custom bag, never the trigger's, because `convertEvent`'s output is compared to detect trigger changes. `FleetReplyFigures` renders "time · tokens · cost · wall" with the formatters moved from `FleetStatusLine.tsx` into `lib/events/run-figures-format.ts`. Each turn row shows its timestamp through `FleetMessageRow`'s `Timestamp`.

- **Dimension 5.1** — A settled reply shows its time, tokens, cost and wall time → Test `test_settled_reply_shows_figures`
- **Dimension 5.2** — A figure the daemon did not report is left out, never shown as zero → Test `test_unknown_figure_left_out`
- **Dimension 5.3** — A running reply shows no figures line → Test `test_running_reply_hides_figures`
- **Dimension 5.4** — The status line renders the same strings after the move → Test `test_status_line_formats_unchanged`
- **Dimension 5.5** — Operator and reply rows show their timestamp → Test `test_turn_rows_show_timestamp`

## Interfaces

```
FleetToolCall += { args?: Record<string, unknown>; ok?: boolean;
                   preview?: string; outputLines?: number }
EventDetail   += { tool_calls: { calls: SavedToolCall[]; omitted: number } | null }
tool-call part : { toolName, args, argsText, result?: { preview, outputLines },
                   isError: ok === false, timing }
Group keys    : "group-reasoning" · "group-explore" · "group-tool"
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Older runner | Frames without arguments or outcome | Row shows the bare name with •; nothing invented |
| Malformed frame field | Wrong type from any producer | Field reads absent; the row renders without it (Dimension 1.4) |
| Malformed saved trace | Corrupt or unexpected body | No saved calls; live rows stay if present (Dimension 2.2) |
| Trace not recorded | Row predates the column, or the daemon dropped it | Live rows if the tab saw them, else none |
| Trace truncated | More than fifty calls | "N more calls not recorded" (Dimension 2.4) |
| Figures missing | Run reported no tokens or cost | That figure is left out (Dimension 5.2) |

## Invariants

1. A row never claims success it was not told — ✓ renders only for `ok === true` (Dimension 3.3).
2. Explored never hides a non-read call — the group map lists only read-class tools; everything else maps to the tool group (Dimension 4.2).
3. Preview text renders as text, never markup — rendered as a text node in a monospace block, not through the markdown renderer (Dimension 3.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | — | — | — | `test_tool_row_names_verb_and_target` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_started_frame_keeps_arguments` | start `{path:"a.md"}` → call `args.path === "a.md"` |
| 1.2 | unit | `test_repeat_start_merges_arguments_keeps_clock` | start `{}` at t0, start `{path}` at t1 → one call, `args.path` set, start t0 |
| 1.3 | unit | `test_completed_frame_keeps_outcome` | completion `ok:false, preview:"denied", output_lines:1` → stored |
| 1.4 | unit | `test_malformed_tool_fields_read_absent` | `args:[1]`, `ok:"yes"`, `preview:7`, `output_lines:-1` → all absent |
| 2.1 | unit | `test_saved_trace_becomes_tool_calls` | detail with 2 saved calls → 2 calls, `done`, `ms` kept |
| 2.2 | unit | `test_malformed_saved_trace_reads_absent` | `tool_calls: "x"` → no calls, no throw |
| 2.3 | unit | `test_settle_prefers_saved_trace` | live 1 call + saved 3 → 3; live 1 + saved `null` → 1 |
| 2.4 | unit | `test_omitted_calls_render_count` | `omitted: 4` → "4 more calls not recorded" |
| 2.5 | e2e | `test_reload_shows_saved_tool_rows` | settled turn, page reload → same verb-and-target rows |
| 3.1 | unit | `test_tool_row_names_verb_and_target` | each of 13 tools → expected verb and target text |
| 3.2 | unit | `test_unknown_tool_row_falls_back_to_name` | `fly_status {app:"x"}` → `fly_status app=x` |
| 3.3 | unit | `test_tool_row_marks_outcome` | ok → ✓; failed → ✗ + error line; absent → • |
| 3.4 | unit | `test_tool_row_previews_five_lines` | 5 preview lines, `outputLines` 12 → "… +7 lines" |
| 3.5 | unit | `test_tool_row_discloses_arguments` | collapsed → JSON hidden; opened → JSON shown |
| 3.6 | e2e | `test_live_tool_row_reads_verb_target_preview` | fixture frames → row "Read README.md" with ✓ and preview |
| 4.1 | unit | `test_consecutive_reads_fold_under_explored` | read, read, recall → one Explored row, three names |
| 4.2 | unit | `test_non_read_call_splits_explored` | read, write, read → Explored, Wrote, Explored |
| 4.3 | unit | `test_explored_label_tracks_running` | one running read → "Exploring"; all done → "Explored" |
| 4.4 | unit | `test_failed_read_marks_its_explored_line` | second read `ok:false` → its line ✗, group stays |
| 5.1 | unit | `test_settled_reply_shows_figures` | tokens 12400, cost, wall 8200 → "12.4k tokens · $… · 8.2s" |
| 5.2 | unit | `test_unknown_figure_left_out` | cost `null` → no cost segment, no "$0" |
| 5.3 | unit | `test_running_reply_hides_figures` | running reply → no figures line |
| 5.4 | unit | `test_status_line_formats_unchanged` | fixed figures → status line strings equal the pre-move strings |
| 5.5 | unit | `test_turn_rows_show_timestamp` | operator and reply rows → `time` element with ISO `dateTime` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Tool rows read as verb, target, outcome and preview (§3) | `cd ui/packages/app && bunx vitest run tests/fleet-tool-calls.test.tsx components/domain/tool-call-copy.test.ts` | exit 0 | P0 | |
| R2 | Reads fold under Explored and saved rows survive a reload (§2, §4) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts tests/e2e/acceptance/fleet-reply-parts.spec.ts` | exit 0 | P0 | |
| R3 | Each settled reply shows its figures (§5) | `cd ui/packages/app && bunx vitest run components/domain/FleetReplyFigures.test.tsx lib/events/run-figures-format.test.ts` | exit 0 | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green (applies only if the diff carries Rust) | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `NO_ARGS` | `grep -rn "NO_ARGS" ui/packages/app --include='*.ts' --include='*.tsx' \| grep -v node_modules` | 0 matches |
| `NO_OUTPUT` | `grep -rn "NO_OUTPUT" ui/packages/app --include='*.ts' --include='*.tsx' \| grep -v node_modules` | 0 matches |
| `function formatTokens` in the status line | `grep -n "function format" "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx"` | 0 matches |

## Out of Scope

- Producing arguments, outcome and the saved trace — M209_001.
- Inline approval rows ("Approval requested", "Approved by …") — needs `event_id` on gate rows and a gate read; a follow-up spec.
- A full-output view behind "… +N lines" — the runner keeps five lines by design (M209_001 §1).
- Context-window and model indicators like Codex's footer — no runner signal carries them.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator scrolls back through yesterday's run and reads "Explored · Read deploy.yaml, runbook.md", "POST https://api.fly.io/… ✗ 401", then the answer and "14:02 · 12.4k tokens · $0.03 · 41s".
2. **Preserved user behaviour** — Thought chip, live clocks, streaming answer, Copy, Resend and the status line work as before.
3. **Optimal-way check** — Rendering on assistant-ui's own grouping and part fields is the direct path; the gap to Codex is the missing full transcript, which the runner does not keep.
4. **Rebuild-vs-iterate** — Iterate: M207_001 built the parts model this fills.
5. **What we build** — One reducer change, one trace parser, one copy map, one row, one group, one figures line.
6. **What we do NOT build** — Approval rows, full-output view, footer indicators (see Out of Scope).
7. **Fit with existing features** — Compounds with the detail re-read and call ids; must not slow streaming (M207_001's long-task budget stays).
8. **Surface order** — UI only; the CLI's `memory` and event verbs are unaffected.
9. **Dashboard restraint** — No ✓ without `ok`, no zero cost for an unreported figure, no expansion that promises output the runner did not keep.
10. **Confused-user next step** — A settled turn with no rows either predates saved traces or had its trace dropped; the daemon's `report_tool_trace_dropped` log (M209_001) names which, and a truncated trace says so in the thread (Dimension 2.4).

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** UI as its own workstream and Pull Request after the backend lands, so each gets its own review profile.
- **Alternatives considered:** per-tool `makeAssistantToolUI` registrations (rejected: thirteen registrations for what one copy map and one row express, and the `GroupedParts` render switch already owns layout); parsing arguments in the browser to classify reads (rejected: the tool name already classifies).
- **Patch-vs-refactor verdict:** this is a **patch** because the parts model, grouping and disclosure primitives already exist.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "Codex-style tool rows and others ensure we are able to show more information." Agent defaults while Indy was away: the extras are per-turn figures and timestamps (data already in the browser); inline approval rows move to a follow-up because they need server work. Output visibility follows M209_001's pending decision.
- **Metrics review** — No analytics or funnel playbook update required: no new user action is introduced.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
