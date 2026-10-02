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

# M209_002: The fleet chat renders tool calls the way Codex does — a status bullet, a verb and target, dim output under a rail, Explored folds, diffs for edits, "show all" for the rest — and keeps them after a reload

**Prototype:** v2.0.0
**Milestone:** M209
**Workstream:** 002
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing: the thread is where a person decides whether to trust a fleet, and today it cannot show what the fleet did
**Categories:** UI
**Batch:** B2 — after M209_001 and M209_003 are on `main`; the milestone's follow-up Pull Request
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M209_001 (frame outcome, saved trace), M209_003 (full call read)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main` at `93e96897a`; assistant-ui read from the installed `@assistant-ui/react` 0.15.22 / core 0.3.21; Codex TUI rules read at `~/Projects/oss/rs/codex` `2e5fea64e`
**Canonical architecture:** `docs/architecture/user_flow.md` §chat surface; `docs/architecture/runner_fleet.md` §Live activity

---

## Overview

**Goal (testable):** A reply whose fleet read two files, recalled a memory, failed a `POST`, edited a file and was then killed mid-call renders an Explored row ("Read a.md, b.md", "Search deploy window in memory"), a red `Requested POST …` with its error under `└`, `Edited deploy.yaml (+2 −1)` with a coloured diff, a red `Interrupted …` row, and "Worked for 41s · 12.4k tokens · $0.03". After a reload it renders the same rows, and "show all" opens a call's full output.
**Problem:** Every call renders as a glyph, a bare name and a clock (`components/domain/FleetToolCalls.tsx:31-47`). Arguments are hard-coded to `{}` and the result to `null` (`components/domain/fleetReplyMessage.ts:32-35,103-104`). A reload loses every row (`lib/streaming/fleet-stream-frames.ts:296`), and each turn's figures reach the row (`lib/streaming/fleet-stream-row.ts:173-175`) without ever being drawn.
**Solution summary:** Frontend only. The reducer keeps each call's arguments and typed outcome, and settle swaps in the saved trace. Parts carry real `args`, `result` and `isError`. A per-tool copy map renders Codex's cell anatomy on design tokens, and `groupPartByType` folds reads under Explored. Edits render as diffs from their own arguments. "Show all" reads M209_003 through a same-origin proxy into a `Dialog`. A settled reply ends with Codex's "Worked for" line; a running one shows elapsed time.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(app): render fleet tool calls like Codex, with full output on demand
- **Intent (one sentence):** A person reading a fleet's thread sees what each call touched, how it ended, what came back, and what the turn cost, live or after a reload, and can open anything the thread abbreviates.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/components/domain/FleetReplyBody.tsx` — the `GroupedParts` switch and the group map this extends.
2. `ui/packages/app/lib/streaming/fleet-stream-tool-frames.ts` — ingress, call-id pairing, and the repeat-start rule that now merges arguments.
3. `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId]/route.ts` — the proxy the full-output route copies.
4. `docs/v2/done/M207_001_P1_UI_CHAT_REPLY_ON_ASSISTANT_UI_PARTS.md` — the parts model and the tests that pin today's row.
5. https://github.com/openai/codex/tree/2e5fea64eefcaa19f48458b2386011b619f69c70/codex-rs/tui/src — `exec_cell/render.rs` (bullet, rail, Explored, `:340-428`), `tool_output.rs` (`PREVIEW_LINES = 3`), `history_cell/dynamic.rs` (`Calling/Called/Failed/Interrupted`), `diff_render.rs` (`:432-497`), `separators.rs` ("Worked for").

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/api/events.ts`, `ui/packages/app/lib/api/events-types.ts` | EDIT | `EventDetail.tool_calls`, the full-call type and its same-origin URL |
| `ui/packages/app/lib/streaming/fleet-stream-row.ts`, `ui/packages/app/lib/streaming/fleet-stream-tool-frames.ts`, `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | `FleetToolCall` gains arguments and outcome; repeat start merges; settle prefers the saved trace and closes open calls |
| `ui/packages/app/lib/streaming/fleet-stream-tool-trace.ts` | CREATE | Narrows a saved trace at the parse boundary |
| `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId]/tool-calls/[callId]/route.ts` | CREATE | Same-origin proxy for the full-call read |
| `ui/packages/app/components/domain/fleetReplyMessage.ts` | EDIT | Parts carry `args`, `argsText`, `result`, `isError`; the reply's custom bag carries its figures |
| `ui/packages/app/components/domain/tool-call-copy.ts`, `ui/packages/app/components/domain/tool-call-diff.ts` | CREATE | Verbs, targets and explore class per tool; line diff from edit arguments |
| `ui/packages/app/components/domain/FleetToolCalls.tsx`, `ui/packages/app/components/domain/FleetExplored.tsx`, `ui/packages/app/components/domain/FleetToolOutputDialog.tsx`, `ui/packages/app/components/domain/FleetReplyBody.tsx` | EDIT / CREATE | Cell, Explored group, full-output dialog, group map |
| `ui/packages/app/components/domain/FleetReplyFigures.tsx`, `ui/packages/app/lib/events/run-figures-format.ts` | CREATE | "Worked for" line; formatters moved out of the status line |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx`, `ui/packages/app/components/domain/FleetMessageRow.tsx`, `ui/packages/app/components/domain/fleetMessageReaders.ts` | EDIT | Shared formatters; turn timestamps; figure readers |
| `ui/packages/design-system/src/tokens.css` | EDIT | `tool-shimmer` keyframe with a `prefers-reduced-motion` static fallback |
| `ui/packages/app/tests/fleet-tool-calls.test.tsx`, `ui/packages/app/components/domain/fleetReplyMessage.test.ts`, `ui/packages/app/components/domain/FleetReplyBody.test.tsx`, `ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts`, `ui/packages/app/tests/fleet-thread/role-reply-parts.test.ts`, `ui/packages/app/tests/e2e/acceptance/fleet-reply-parts.spec.ts` | EDIT | Existing pins move to the new cell (the `{}`/`null` and repeat-start pins change on purpose) |
| `ui/packages/app/components/domain/tool-call-copy.test.ts`, `ui/packages/app/components/domain/tool-call-diff.test.ts`, `ui/packages/app/components/domain/FleetExplored.test.tsx`, `ui/packages/app/components/domain/FleetToolOutputDialog.test.tsx`, `ui/packages/app/lib/streaming/fleet-stream-tool-trace.test.ts`, `ui/packages/app/components/domain/FleetReplyFigures.test.tsx`, `ui/packages/app/lib/events/run-figures-format.test.ts` | CREATE | Unit proofs |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (verbs, glyphs, line caps, group keys are constants), NDC + ORP (`NO_ARGS`, `NO_OUTPUT` and the status line's private formatters leave with every reference), NTP, PJV, HLP, TCF, TST-NAM.
- `dispatch/write_ts_adhere_bun.md` — `TS FILE SHAPE DECISION` at PLAN; design-system primitives (UIS); token utilities, opacity modifiers on named tokens allowed (DTK, precedent `bg-destructive/10`).
- `docs/architecture/web_app.md` — rendering and same-origin data-loading conventions.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI GATE | yes | `List`/`ListItem`, `Accordion`, `Dialog`, `Time` from `@agentsfleet/design-system` |
| DESIGN TOKEN GATE | yes | `text-success`, `text-destructive`, `text-accent`, `text-muted-foreground`, `bg-success/10`, `bg-destructive/10`; one new keyframe, no arbitrary values |
| UFS GATE | yes | Copy map, glyphs, `OUTPUT_PREVIEW_ROWS`, group keys are constants |
| LENGTH GATE (≤350/≤50/≤70) | yes | Cell, group, dialog and diff each in their own module |
| MILESTONE-ID GATE | yes | No milestone identifiers in source or test names |
| Architecture consult | yes | `user_flow.md` §chat surface landed with this spec |

## Prior-Art / Reference Implementations

- **Reference:** Codex TUI (`2e5fea64e`) — `•` bullet green on success, red on failure, shimmering while active (`exec_cell/render.rs:459-466`); dim output under `  └ `; 3 rows then `+N lines` (`tool_output.rs:19`); `(no output)` (`render.rs:558-562`); Explored folding and Read merging (`render.rs:315-428`); `Edited f (+N -M)` diffs (`diff_render.rs:432-497`); "Worked for" (`separators.rs:74-110`). Diverges: verbs come from tool names because hosted tools take structured arguments; the accent and diff colours map to our tokens, not Catppuccin.
- **Reference:** `ui/packages/app/components/domain/FleetThought.tsx` — Accordion disclosure and live-versus-settled labelling.

## Sections (implementation slices)

### §1 — The reducer keeps each call's arguments and outcome

`readToolStep` reads `args_redacted` when it is a JSON object, and `status`, `output_head`, `output_tail` and `output_line_count` when well-typed; anything else reads as absent. A repeat start under an open call id merges the arguments and keeps the first start's clock.

- **Dimension 1.1** — A start with object arguments stores them on its call → Test `test_started_frame_keeps_arguments`
- **Dimension 1.2** — A repeat start under the same id merges arguments and keeps the start clock → Test `test_repeat_start_merges_arguments_keeps_clock`
- **Dimension 1.3** — A completion's status and output edges land on the call → Test `test_completed_frame_keeps_outcome`
- **Dimension 1.4** — Array arguments, an unknown status, non-string edges or a negative count read as absent → Test `test_malformed_tool_fields_read_absent`

### §2 — Saved calls replace live ones; no call is left running

`rowToEvent` maps `tool_calls` through one narrowing function. **Implementation default:** a saved call's start is the event's `createdAt`, because only the difference to `duration_ms` renders. At settle, a saved trace replaces the live list; with `null`, live rows stay and any still open become `interrupted`, as Codex closes an unfinished cell at turn end.

- **Dimension 2.1** — A saved trace becomes calls with arguments, outcome and duration → Test `test_saved_trace_becomes_tool_calls`
- **Dimension 2.2** — A malformed saved trace reads as no calls without throwing → Test `test_malformed_saved_trace_reads_absent`
- **Dimension 2.3** — Settle prefers a saved trace and keeps live rows when it is `null` → Test `test_settle_prefers_saved_trace`
- **Dimension 2.4** — A live call still open at settle renders interrupted → Test `test_settle_interrupts_open_live_calls`
- **Dimension 2.5** — `omitted_call_count` above zero renders "N more calls not recorded" → Test `test_omitted_calls_render_count`
- **Dimension 2.6** — Reloading a settled turn shows the same rows → Test `test_reload_shows_saved_tool_rows`

### §3 — Each call renders as a Codex cell

Bullet `•`: shimmer while running (static dim under reduced motion), `text-success` on `succeeded`, `text-destructive` on `failed` and `interrupted`, dim with no outcome. Header: the bold verb (`-ing` while running, past tense when done; `Interrupted` replaces it), the target in mono, then dim ` · 1.2s`. Copy: `file_write` Wrote ·path· (+N), `file_append` Appended, `file_delete` Deleted, `http_request` Requesting/Requested ·method· ·url·, `memory_store` Remembered ·key·, `memory_forget` Forgot ·key·, `calculator` Calculated ·operation·, unknown tools Called ·name·(·compact arguments·). Output: dim mono under `└`, three rows, then dim "+N lines" and a "show all" control. Empty output on success reads `(no output)`; a call with no recorded outcome reads `(output unavailable)`. The full arguments sit behind a collapsed Accordion.

- **Dimension 3.1** — Each hosted tool renders its verb and target, running and done → Test `test_tool_cell_names_verb_and_target`
- **Dimension 3.2** — An unknown tool renders `Called name(arguments)` → Test `test_unknown_tool_cell_calls_by_name`
- **Dimension 3.3** — Bullet colour follows status; `Interrupted` replaces the verb → Test `test_tool_cell_bullet_follows_status`
- **Dimension 3.4** — Eight output lines render three plus "+5 lines" → Test `test_tool_cell_previews_three_rows`
- **Dimension 3.5** — Empty output reads `(no output)`; no outcome reads `(output unavailable)` → Test `test_tool_cell_names_empty_and_unknown_output`
- **Dimension 3.6** — The shimmer stops under `prefers-reduced-motion` → Test `test_running_bullet_respects_reduced_motion`
- **Dimension 3.7** — A live run in the real page shows a shimmering cell turning into a green "Requested GET …" with its output → Test `test_live_tool_cell_settles_green`

### §4 — Reads fold under Explored

The group map sends `tool-call:file_read`, `tool-call:file_read_hashed`, `tool-call:memory_recall` and `tool-call:memory_list` to an explore group, and every other `tool-call` to the tool group. A bold "Exploring" with a shimmer reads "Explored" with a dim bullet once all are done. Under `└`, consecutive successful reads merge into one de-duplicated "Read a, b" line; `memory_recall` reads "Search ·query· in memory"; `memory_list` reads "List memory ·category·". Verbs use `text-accent`, and a failed line ends in red "(failed)".

- **Dimension 4.1** — Read, read, recall render one Explored group with "Read a, b" and "Search q in memory" → Test `test_consecutive_reads_fold_under_explored`
- **Dimension 4.2** — A write between reads yields two Explored groups around it → Test `test_non_read_call_splits_explored`
- **Dimension 4.3** — The header reads Exploring while any folded call runs → Test `test_explored_header_tracks_running`
- **Dimension 4.4** — A failed read keeps the group and marks its line "(failed)" → Test `test_failed_read_marks_its_explored_line`

### §5 — Edits render as diffs, and "show all" opens the rest

`file_edit` and `file_edit_hashed` render `Edited ·path· (+N −M)`, with +N in `text-success` and −M in `text-destructive`. A line diff built from `old_text` and `new_text` uses `bg-destructive/10` and `bg-success/10` rows, indented four. "Show all" opens a `Dialog` that reads the full call through the proxy, with numbered lines, full arguments and the full diff. A 404 shows the saved head and tail with "Full output wasn't kept for this call."

- **Dimension 5.1** — An edit renders its header counts and coloured diff rows → Test `test_edit_cell_renders_diff`
- **Dimension 5.2** — The diff of `a\nb` → `a\nc\nd` counts +2 −1 → Test `test_edit_diff_counts_lines`
- **Dimension 5.3** — "Show all" renders every line of the full output → Test `test_output_dialog_shows_full_output`
- **Dimension 5.4** — A 404 falls back to head and tail with the not-kept note → Test `test_output_dialog_falls_back_when_not_kept`
- **Dimension 5.5** — The proxy forwards the call id percent-encoded and passes the status through → Test `test_tool_call_proxy_forwards_encoded_id`

### §6 — Every turn says when it ran, how long and what it cost

`toReplyMessage` carries the turn's tokens, wall time and cost in the reply's custom bag, never the trigger's, because `convertEvent`'s output is compared to detect trigger changes. A settled reply ends with dim "Worked for 41s · 12.4k tokens · $0.03", using formatters moved from `FleetStatusLine.tsx` into `lib/events/run-figures-format.ts`. A running reply's indicator reads "Working (1m 05s)". Turn rows show their time through `FleetMessageRow`'s `Timestamp`.

- **Dimension 6.1** — A settled reply shows its figures line → Test `test_settled_reply_shows_figures`
- **Dimension 6.2** — An unreported figure is left out, never shown as zero → Test `test_unknown_figure_left_out`
- **Dimension 6.3** — A running reply shows elapsed time and no figures line → Test `test_running_reply_shows_elapsed`
- **Dimension 6.4** — The status line renders the same strings after the move → Test `test_status_line_formats_unchanged`
- **Dimension 6.5** — Operator and reply rows show their timestamp → Test `test_turn_rows_show_timestamp`

## Interfaces

```
FleetToolCall += { args?: Record<string, unknown>; status?: "succeeded"|"failed"|"interrupted";
                   outputHead?: string; outputTail?: string; outputLineCount?: number }
EventDetail   += { tool_calls: { calls: SavedToolCall[]; omitted_call_count: number } | null }
tool-call part : { toolName, args, argsText, result?: { head, tail, lineCount }, isError, timing }
Group keys     : "group-reasoning" · "group-explore" · "group-tool"
Proxy          : /live/v1/workspaces/{ws}/fleets/{fleet}/events/{event}/tool-calls/{call} → M209_003
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Older runner | Frames without arguments or outcome | Bare name, dim bullet, `(output unavailable)` |
| Malformed frame field | Wrong type from any producer | Field absent; cell renders without it (Dimension 1.4) |
| Malformed saved trace | Corrupt body | No saved calls; live rows stay (Dimension 2.2) |
| Call left open | Runner host lost | Interrupted at settle (Dimension 2.4) |
| Full output not kept | Budget spent or post failed | Head, tail and a not-kept note (Dimension 5.4) |
| Figures missing | Run reported none | That figure left out (Dimension 6.2) |

## Invariants

1. A cell never claims success it was not told — green only for `status === "succeeded"` (Dimension 3.3).
2. Explored never hides a non-read call — only read-class tools map to the explore group (Dimension 4.2).
3. Tool output renders as text, never markup — text nodes in a monospace block, never the markdown renderer (Dimension 3.4).
4. No call is drawn running after its turn settles (Dimension 2.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product or operator signal changes | not applicable | — | — | — | `test_tool_cell_names_verb_and_target` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_started_frame_keeps_arguments` | start `{path:"a.md"}` → call `args.path === "a.md"` |
| 1.2 | unit | `test_repeat_start_merges_arguments_keeps_clock` | start `{}` at t0, start `{path}` at t1 → one call, path set, start t0 |
| 1.3 | unit | `test_completed_frame_keeps_outcome` | `status:"failed"`, head `denied`, count 1 → stored |
| 1.4 | unit | `test_malformed_tool_fields_read_absent` | `args:[1]`, `status:"ok"`, `output_head:7`, count −1 → absent |
| 2.1 | unit | `test_saved_trace_becomes_tool_calls` | 2 saved calls → 2 done calls with `duration_ms` |
| 2.2 | unit | `test_malformed_saved_trace_reads_absent` | `tool_calls: "x"` → no calls, no throw |
| 2.3 | unit | `test_settle_prefers_saved_trace` | live 1 + saved 3 → 3; live 1 + `null` → 1 |
| 2.4 | unit | `test_settle_interrupts_open_live_calls` | open live call, settle with `null` → `interrupted` |
| 2.5 | unit | `test_omitted_calls_render_count` | `omitted_call_count: 4` → "4 more calls not recorded" |
| 2.6 | e2e | `test_reload_shows_saved_tool_rows` | settled turn, reload → same cells |
| 3.1 | unit | `test_tool_cell_names_verb_and_target` | each non-read tool, running and done → expected header |
| 3.2 | unit | `test_unknown_tool_cell_calls_by_name` | `fly_status {app:"x"}` → `Called fly_status({"app":"x"})` |
| 3.3 | unit | `test_tool_cell_bullet_follows_status` | succeeded → success; failed, interrupted → destructive; interrupted header |
| 3.4 | unit | `test_tool_cell_previews_three_rows` | 8 lines → 3 rows + "+5 lines" |
| 3.5 | unit | `test_tool_cell_names_empty_and_unknown_output` | empty success → `(no output)`; no status → `(output unavailable)` |
| 3.6 | unit | `test_running_bullet_respects_reduced_motion` | reduced-motion media → no shimmer animation class |
| 3.7 | e2e | `test_live_tool_cell_settles_green` | fixture frames → shimmer, then green "Requested GET …" and output |
| 4.1 | unit | `test_consecutive_reads_fold_under_explored` | read a, read b, recall q → "Read a, b", "Search q in memory" |
| 4.2 | unit | `test_non_read_call_splits_explored` | read, write, read → Explored, Wrote, Explored |
| 4.3 | unit | `test_explored_header_tracks_running` | one running → "Exploring"; all done → "Explored" |
| 4.4 | unit | `test_failed_read_marks_its_explored_line` | second read failed → its line "(failed)", group stays |
| 5.1 | unit | `test_edit_cell_renders_diff` | edit arguments → header "(+2 −1)", coloured rows |
| 5.2 | unit | `test_edit_diff_counts_lines` | `a\nb` → `a\nc\nd` → +2 −1 |
| 5.3 | unit | `test_output_dialog_shows_full_output` | 224-line response → 224 numbered lines |
| 5.4 | unit | `test_output_dialog_falls_back_when_not_kept` | 404 → head, tail and the not-kept note |
| 5.5 | unit | `test_tool_call_proxy_forwards_encoded_id` | id `7:3` → upstream path ends `/tool-calls/7%3A3`; 404 passes through |
| 6.1 | unit | `test_settled_reply_shows_figures` | tokens 12400, wall 41000 → "Worked for 41s · 12.4k tokens · $…" |
| 6.2 | unit | `test_unknown_figure_left_out` | cost `null` → no cost segment, no "$0" |
| 6.3 | unit | `test_running_reply_shows_elapsed` | running 65 s → "Working (1m 05s)", no figures line |
| 6.4 | unit | `test_status_line_formats_unchanged` | fixed figures → same strings as before the move |
| 6.5 | unit | `test_turn_rows_show_timestamp` | operator and reply rows → `time` element with `dateTime` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Cells, Explored and diffs render like Codex (§3–§5) | `cd ui/packages/app && bunx vitest run tests/fleet-tool-calls.test.tsx components/domain/tool-call-copy.test.ts components/domain/FleetExplored.test.tsx components/domain/tool-call-diff.test.ts` | exit 0 | P0 | |
| R2 | Live cells settle and saved rows survive a reload in the real page (§2, §3) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts tests/e2e/acceptance/fleet-reply-parts.spec.ts` | exit 0 | P0 | |
| R3 | Every turn shows its time and cost (§6) | `cd ui/packages/app && bunx vitest run components/domain/FleetReplyFigures.test.tsx lib/events/run-figures-format.test.ts` | exit 0 | P0 | |
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

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `NO_ARGS` | `grep -rn "NO_ARGS" ui/packages/app --include='*.ts' --include='*.tsx' \| grep -v node_modules` | 0 matches |
| `NO_OUTPUT` | `grep -rn "NO_OUTPUT" ui/packages/app --include='*.ts' --include='*.tsx' \| grep -v node_modules` | 0 matches |
| status line's private formatters | `grep -n "function format" "ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx"` | 0 matches |

## Out of Scope

- Producing arguments, outcomes, the saved trace and full outputs — M209_001, M209_003.
- Inline approval lines, budget warnings and an interrupt control — the next milestone (the approval gate is per event, and no turn interrupt exists).
- Codex's footer (model, context left) — no runner signal carries context use.
- Syntax highlighting inside outputs — outputs are files and HTTP bodies of unknown type.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator scrolls back through last night's run and reads it like a Codex transcript: Explored reads, a red failed `POST` with its error, a green diff, an Interrupted call, "Worked for 41s · $0.03", and opens the 224-line log with one click.
2. **Preserved user behaviour** — Thought chip, live clocks, streaming answer, Copy, Resend and the status line work as before.
3. **Optimal-way check** — Rendering on assistant-ui's grouping and part fields is the direct path; Codex's cell anatomy ports with tool-name verbs.
4. **Rebuild-vs-iterate** — Iterate: M207_001 built the parts model this fills.
5. **What we build** — Reducer and settle changes, a trace parser, a copy map, a diff, a cell, an Explored group, a dialog, a proxy, a figures line.
6. **What we do NOT build** — Approvals, warnings, interrupt, footer, output highlighting (see Out of Scope).
7. **Fit with existing features** — Compounds with the detail re-read and call ids; must not slow streaming (M207_001's long-task budget stays).
8. **Surface order** — UI only.
9. **Dashboard restraint** — No green without `succeeded`, no zero cost for an unreported figure, "show all" only where more exists.
10. **Confused-user next step** — "Full output wasn't kept for this call" and "N more calls not recorded" state each gap in the thread itself.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** UI as its own workstream and Pull Request after the backend lands, so each gets its own review profile.
- **Alternatives considered:** thirteen `makeAssistantToolUI` registrations (rejected: one copy map and one cell express them, and `GroupedParts` already owns layout); parsing HTTP status from output text in the browser (rejected: Codex's rule is never to parse text for outcomes; the first output line already shows it).
- **Patch-vs-refactor verdict:** this is a **patch** because the parts model, grouping and disclosure primitives already exist.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "Codex-style tool rows and others ensure we are able to show more information"; "The tool call preview like we see in codex nothing must be hidden, isnt codex displaying all / i need the visuals as well really cool"; chose "Full output on click (Recommended)"; then "just explore the rust code of codex as well and make our agentsfleet robust." The ASCII sample shown in-session is the target look. Agent defaults: current Codex's 3-row preview (the screenshots mix builds), per-turn figures and timestamps as the extras, approvals and warnings moved to the next milestone.
- **Metrics review** — No analytics or funnel playbook update required: no new tracked user action.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
