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
**Status:** DONE
**Priority:** P1 — operator-facing: the thread is where a person decides whether to trust a fleet, and today it cannot show what the fleet did
**Categories:** UI
**Batch:** B2 — after M209_001 and M209_003 are on `main`; the milestone's follow-up Pull Request
**Branch:** feat/m209-002-chat-tool-calls
**Baseline revision:** b0138d7b3124b871668f07e2361dba923bc774d2
**Test Baseline:** `unit=9824 integration=799` — `make test-unit-all` (Rust 3745 + app 3512 + CLI 1780 + design system 645 + website 142, exit 0) and `make test-integration-rustd` (799 passed / 2 failed in the shared suite, exit 2: two `daemon_suite` timing failures on `main` that pass on this branch) at `b0138d7b3` in a detached worktree. Final: `unit=9948 integration=803` (app 3634, design system 647; 801 shared + 2 exclusive), both exit 0 at `2065fd95d`.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M209_002-b0138d7b3.md`
**Depends on:** M209_001 (frame outcome, saved trace), M209_003 (full call read). Production rows need the Rust runner that emits them (`docs/architecture/runner_execution.md`); every test here drives fixture frames
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 02, 2026) from a source trace on `main` at `93e96897a`; assistant-ui read from the installed `@assistant-ui/react` 0.15.22 / core 0.3.21; Codex TUI rules read at `~/Projects/oss/rs/codex` `2e5fea64e`
**Canonical architecture:** `docs/architecture/user_flow.md` §chat surface; `docs/architecture/runner_fleet.md` §Live activity

---

## Overview

**Goal (testable):** A reply whose fleet read two files, recalled a memory, failed a `POST`, edited a file and was then killed mid-call renders an Explored row ("Read a.md, b.md", "Search deploy window in memory"), a red `Requested POST …` with its error under `└`, `Edited deploy.yaml (+2 −1)` with a coloured diff, a red `Interrupted …` row, and "Worked for 41s · 12.4K tokens · $0.03". After a reload it renders the same rows, and "show all" opens a call's full output.
**Problem:** Every call renders as a glyph, a bare name and a clock (`components/domain/FleetToolCalls.tsx:31-47`). Arguments are hard-coded to `{}` and the result to `null` (`components/domain/fleetReplyMessage.ts:32-35,103-104`). A reload loses every row (`lib/streaming/fleet-stream-frames.ts:296`), and each turn's figures reach the row (`lib/streaming/fleet-stream-row.ts:173-175`) without ever being drawn.
**Solution summary:** Frontend only. The reducer keeps each call's arguments and typed outcome, and settle swaps in the saved trace. Parts carry real `args`, `result` and `isError`. A per-tool copy map renders Codex's cell anatomy on design tokens for every tool in the runner's catalog, and `groupPartByType` folds reads (files, memory, web, schedule lookups) under Explored. Edits render as diffs from their own arguments. "Show all" reads M209_003 through a same-origin proxy into a `Dialog`. A settled reply ends with Codex's "Worked for" line; a running one shows elapsed time.

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
| `ui/packages/app/lib/api/events.ts`, `ui/packages/app/lib/api/events-types.ts`, `ui/packages/app/lib/api/errors.ts` | EDIT | `EventDetail.tool_calls`, the full-call URL, and the not-found status two readers share |
| `ui/packages/app/lib/streaming/fleet-stream-row.ts`, `ui/packages/app/lib/streaming/fleet-stream-tool-frames.ts`, `ui/packages/app/lib/streaming/fleet-stream-frames.ts`, `ui/packages/app/lib/streaming/fleet-stream-detail-recovery.ts`, `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts`, `ui/packages/app/lib/streaming/fleet-stream-entry.ts` | EDIT | Calls keep arguments and outcome; a repeat start merges; settle prefers the saved trace, closes open calls as a guess a late completion still replaces, and re-reads the trace once per event |
| `ui/packages/app/lib/streaming/fleet-stream-tool-trace.ts`, `ui/packages/app/lib/streaming/fleet-tool-call-reader.ts` | CREATE | The saved trace and the full-call read, narrowed at the boundary |
| `ui/packages/app/lib/api/live-json-proxy.ts`, `ui/packages/app/app/live/v1/workspaces/[workspaceId]/events/route.ts`, `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/route.ts`, `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId]/route.ts`, `ui/packages/app/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/[eventId]/tool-calls/[callId]/route.ts`, `docs/AUTH.md` | EDIT / CREATE | Every JSON proxy on one helper, the full-call route on it, and the auth doc naming the seventh route |
| `ui/packages/app/components/domain/tool-call-shape.ts`, `ui/packages/app/components/domain/tool-call-text.ts`, `ui/packages/app/components/domain/tool-call-copy.ts`, `ui/packages/app/components/domain/tool-call-copy-runner.ts`, `ui/packages/app/components/domain/tool-call-diff.ts`, `ui/packages/app/components/domain/tool-call-explore.ts` | CREATE | Tool names and body kinds; argument and output text as the runner writes it; verbs, targets and bodies for every catalog tool; edit and patch diffs; the Explored fold |
| `ui/packages/app/components/domain/FleetToolCalls.tsx`, `ui/packages/app/components/domain/FleetToolCallBody.tsx`, `ui/packages/app/components/domain/FleetExplored.tsx`, `ui/packages/app/components/domain/FleetToolOutputDialog.tsx`, `ui/packages/app/components/domain/FleetScope.tsx`, `ui/packages/app/components/domain/FleetReplyFigures.tsx`, `ui/packages/app/components/domain/FleetTimestamp.tsx` | EDIT / CREATE | The cell, its body, the Explored group, the full-output dialog, the thread's scope, the figures line, and the row timestamp split out at its cap |
| `ui/packages/app/components/domain/FleetReplyBody.tsx`, `ui/packages/app/components/domain/FleetThread.tsx`, `ui/packages/app/components/domain/FleetMessageRow.tsx`, `ui/packages/app/components/domain/fleetMessageRenderers.tsx`, `ui/packages/app/components/domain/fleetReplyMessage.ts`, `ui/packages/app/components/domain/fleetMessageReaders.ts` | EDIT | The group map, the scope provider, turn timestamps, parts with real `args` and `result` keyed by call id, and the figure readers |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.tsx`, `ui/packages/app/lib/events/run-figures-format.ts`, `ui/packages/app/lib/utils.ts` | EDIT / CREATE | Formatters moved out of the status line; the shared time constants exported |
| `ui/packages/design-system/src/tokens.css`, `ui/packages/design-system/src/tokens.css.test.ts` | EDIT | `tool-shimmer` keyframe with a `prefers-reduced-motion` static fallback, pinned as text |
| `ui/packages/app/components/domain/tool-call-copy.test.ts`, `ui/packages/app/components/domain/tool-call-copy-runner.test.ts`, `ui/packages/app/components/domain/tool-call-text.test.ts`, `ui/packages/app/components/domain/tool-call-diff.test.ts`, `ui/packages/app/components/domain/tool-call-explore.test.ts`, `ui/packages/app/components/domain/FleetExplored.test.tsx`, `ui/packages/app/components/domain/FleetToolOutputDialog.test.tsx`, `ui/packages/app/components/domain/FleetReplyFigures.test.tsx`, `ui/packages/app/components/domain/FleetReplyBody.test.tsx`, `ui/packages/app/components/domain/fleetReplyMessage.test.ts`, `ui/packages/app/lib/streaming/fleet-stream-tool-trace.test.ts`, `ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts`, `ui/packages/app/lib/streaming/fleet-stream-frames.settle.test.ts`, `ui/packages/app/lib/streaming/fleet-stream-registry.reply.test.ts`, `ui/packages/app/lib/streaming/fleet-tool-call-reader.test.ts`, `ui/packages/app/lib/events/run-figures-format.test.ts`, `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.test.tsx` | CREATE / EDIT | Unit proofs, and the status line's strings pinned across the move |
| `ui/packages/app/tests/fleet-tool-calls.test.tsx`, `ui/packages/app/tests/fleet-tool-calls.runner.test.tsx`, `ui/packages/app/tests/tool-call-route.test.ts`, `ui/packages/app/tests/stream-proxy-routing.test.ts`, `ui/packages/app/tests/fleet-thread/harness.ts`, `ui/packages/app/tests/fleet-thread/role-reply-parts.test.ts`, `ui/packages/app/tests/fleet-thread/role-reasoning.test.ts`, `ui/packages/app/tests/fleet-thread/role-rows.test.ts`, `ui/packages/app/tests/fleet-thread/role-turns.test.ts`, `ui/packages/app/tests/fleet-thread/malformed-metadata.test.ts`, `ui/packages/app/tests/e2e/acceptance/fleet-reply-parts.spec.ts` | CREATE / EDIT | Thread-level proofs; existing pins move to the new cell (the `{}`/`null`, repeat-start, waiting-verb and operator-time pins change on purpose) |
| `ui/packages/app/package.json`, `bun.lock`, `ui/packages/design-system/package.json`, `ui/packages/website/package.json`, `cli/package.json`, `cli/bun.lock`, `cli/src/**`, `cli/test/command-matrix-parity.unit.test.ts`, `cli/test/entry-help-formatter.unit.test.ts`, `ui/packages/website/src/components/Footer.tsx` | EDIT | Every package to latest (`@assistant-ui/react` 0.15.23, `diff` 9 for edits); effect 4.0.0 moves the CLI module to `effect/cli`; oxlint 1.86's purity rule moves the footer's year out of render |
| `playbooks/operations/acceptance/baselines/M209_002-b0138d7b3.md`, `docs/v2/done/M209_002_P1_UI_CHAT_SHOWS_WHAT_THE_FLEET_DID.md`, `docs/architecture/user_flow.md`, `docs/architecture/data_flow.md`, `docs/architecture/runner_execution.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/scenarios/github-pr-reviewer.md`, `VERSION`, `build.zig.zon`, `rustd/Cargo.toml`, `rustd/Cargo.lock` | CREATE / EDIT | The baseline report `Baseline evidence:` cites, this spec as the work landed, the chat surface as it shipped, the lease, Pull Request and sandbox flows the cells draw (Indy, Oct 05, 2026: fold into this PR), and the minor version bump |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (verbs, glyphs, line caps, group keys are constants), NDC + ORP (`NO_ARGS`, `NO_OUTPUT` and the status line's private formatters leave with every reference), NTP, PJV, HLP, TCF, TST-NAM.
- `dispatch/write_ts_adhere_bun.md` — `TS FILE SHAPE DECISION` at PLAN; design-system primitives (UIS); token utilities, opacity modifiers on named tokens allowed (DTK, precedent `bg-destructive/10`).
- `docs/architecture/web_app.md` — rendering and same-origin data-loading conventions.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI GATE | yes | `List`/`ListItem`, `Accordion`, `Dialog`, `Time` from `@agentsfleet/design-system` |
| DESIGN TOKEN GATE | yes | `text-success`, `text-destructive`, `text-info`, `text-muted-foreground`, `bg-success/10`, `bg-destructive/10`; one new keyframe, no arbitrary values |
| UFS GATE | yes | Copy map, glyphs, `OUTPUT_PREVIEW_ROWS`, group keys are constants |
| LENGTH GATE (≤350/≤50/≤70) | yes | Cell, group, dialog and diff each in their own module |
| MILESTONE-ID GATE | yes | No milestone identifiers in source or test names |
| Architecture consult | yes | `user_flow.md` §chat surface landed with this spec, revised at close to the cells as shipped |

## Prior-Art / Reference Implementations

- **Reference:** Codex TUI (`2e5fea64e`) — `•` bullet green on success, red on failure, shimmering while active (`exec_cell/render.rs:459-466`); dim output under `  └ `; 3 rows then `+N lines` (`tool_output.rs:19`); `(no output)` (`render.rs:558-562`); Explored folding and Read merging (`render.rs:315-428`); `Edited f (+N -M)` diffs (`diff_render.rs:432-497`); "Worked for" (`separators.rs:74-110`). Diverges: verbs come from tool names because hosted tools take structured arguments; the accent and diff colours map to our tokens, not Catppuccin.
- **Reference:** `ui/packages/app/components/domain/FleetThought.tsx` — Accordion disclosure and live-versus-settled labelling.

## Sections (implementation slices)

### §1 — The reducer keeps each call's arguments and outcome

`readToolStep` reads `args_redacted` when it is a JSON object, and `status`, `output_head`, `output_tail`, `output_line_count` and `exit_code` when well-typed; anything else reads as absent. A repeat start under an open call id merges the arguments and keeps the first start's clock.
- **Dimension 1.1** — A start with object arguments stores them on its call → Test `test_started_frame_keeps_arguments` — DONE (`ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts`)
- **Dimension 1.2** — A repeat start under the same id merges arguments and keeps the start clock → Test `test_repeat_start_merges_arguments_keeps_clock` — DONE (`ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts`)
- **Dimension 1.3** — A completion's status and output edges land on the call → Test `test_completed_frame_keeps_outcome` — DONE (`ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts`)
- **Dimension 1.4** — Array arguments, an unknown status, non-string edges or a negative count read as absent → Test `test_malformed_tool_fields_read_absent` — DONE (`ui/packages/app/lib/streaming/fleet-stream-frames.tools.test.ts`)

### §2 — Saved calls replace live ones; no call is left running

`rowToEvent` maps `tool_calls` through one narrowing function. **Implementation default:** a saved call's start is the event's `createdAt`, because only the difference to `duration_ms` renders. At settle, a saved trace replaces the live list; with `null`, live rows stay and any still open become `interrupted`, as Codex closes an unfinished cell at turn end.
- **Dimension 2.1** — A saved trace becomes calls with arguments, outcome and duration → Test `test_saved_trace_becomes_tool_calls` — DONE (`ui/packages/app/lib/streaming/fleet-stream-tool-trace.test.ts`)
- **Dimension 2.2** — A malformed saved trace reads as no calls without throwing → Test `test_malformed_saved_trace_reads_absent` — DONE (`ui/packages/app/lib/streaming/fleet-stream-tool-trace.test.ts`)
- **Dimension 2.3** — Settle prefers a saved trace and keeps live rows when it is `null` → Test `test_settle_prefers_saved_trace` — DONE (`ui/packages/app/lib/streaming/fleet-stream-frames.settle.test.ts`)
- **Dimension 2.4** — A live call still open at settle renders interrupted → Test `test_settle_interrupts_open_live_calls` — DONE (`ui/packages/app/lib/streaming/fleet-stream-frames.settle.test.ts`)
- **Dimension 2.5** — `omitted_call_count` above zero renders "N more calls not recorded" → Test `test_omitted_calls_render_count` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 2.6** — Reloading a settled turn shows the same rows → Test `test_reload_shows_saved_tool_rows` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)

### §3 — Each call renders as a Codex cell

Bullet `•`: shimmer while running (static dim under reduced motion), `text-success` on `succeeded`, `text-destructive` on `failed` and `interrupted`, dim with no outcome. Header: the bold verb (`-ing` while running, past tense when done; `Interrupted` replaces it), the target in mono, then dim ` · 1.2s`. Copy: `file_write` Wrote ·path· (+N), `file_append` Appended, `file_delete` Deleted, `http_request` Requesting/Requested ·method· ·url·, `memory_store` Remembered ·key·, `memory_forget` Forgot ·key·, unknown tools Called ·name·(·compact arguments·). Output: dim mono under `└`, three rows, then dim "+N lines" and a "show all" control. Empty output on success reads `(no output)`; a call with no recorded outcome reads `(output unavailable)`. A failed call ends its header in red "(failed)", so status never rests on colour alone (`docs/DESIGN_SYSTEM.md` §Color). The full arguments sit behind a collapsed Accordion. `exec_command` and `shell` render Codex's command cell: `Running`, then `Ran`, with the command on the header line, at most two dim `│` rail lines of a longer command and then `… +N lines`, and a red ` (exit N)` on a non-zero exit.
- **Dimension 3.1** — Each hosted tool renders its verb and target, running and done → Test `test_tool_cell_names_verb_and_target` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 3.2** — An unknown tool renders `Called name(arguments)` → Test `test_unknown_tool_cell_calls_by_name` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 3.3** — Bullet colour follows status; `Interrupted` replaces the verb → Test `test_tool_cell_bullet_follows_status` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 3.4** — Eight output lines render three plus "+5 lines" → Test `test_tool_cell_previews_three_rows` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 3.5** — Empty output reads `(no output)`; no outcome reads `(output unavailable)` → Test `test_tool_cell_names_empty_and_unknown_output` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 3.6** — The shimmer stops under `prefers-reduced-motion` → Test `test_running_bullet_respects_reduced_motion` — DONE (`ui/packages/design-system/src/tokens.css.test.ts`)
- **Dimension 3.7** — A live run in the real page shows a shimmering cell turning into a green "Requested GET …" with its output → Test `test_live_tool_cell_settles_green` — DONE (`ui/packages/app/tests/e2e/acceptance/fleet-reply-parts.spec.ts`)
- **Dimension 3.8** — A four-line command renders `Ran` with its first line, two rail lines and `… +1 lines`; exit code 2 adds a red ` (exit 2)` → Test `test_command_cell_renders_like_codex` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)

### §4 — Reads fold under Explored

The group map sends `tool-call:file_read`, `tool-call:file_read_hashed`, `tool-call:memory_recall` and `tool-call:memory_list` to an explore group, and every other `tool-call` to the tool group. A bold "Exploring" with a shimmer reads "Explored" with a dim bullet once all are done. Under `└`, consecutive successful reads merge into one de-duplicated "Read a, b" line; `memory_recall` reads "Search ·query· in memory"; `memory_list` reads "List memory ·category·". Verbs use `text-info`, the light blue Codex draws them in, and a failed line ends in red "(failed)".
- **Dimension 4.1** — Read, read, recall render one Explored group with "Read a, b" and "Search q in memory" → Test `test_consecutive_reads_fold_under_explored` — DONE (`ui/packages/app/components/domain/FleetExplored.test.tsx`)
- **Dimension 4.2** — A write between reads yields two Explored groups around it → Test `test_non_read_call_splits_explored` — DONE (`ui/packages/app/components/domain/FleetExplored.test.tsx`)
- **Dimension 4.3** — The header reads Exploring while any folded call runs → Test `test_explored_header_tracks_running` — DONE (`ui/packages/app/components/domain/FleetExplored.test.tsx`)
- **Dimension 4.4** — A failed read keeps the group and marks its line "(failed)" → Test `test_failed_read_marks_its_explored_line` — DONE (`ui/packages/app/components/domain/FleetExplored.test.tsx`)

### §5 — Edits render as diffs, and "show all" opens the rest

`file_edit` and `file_edit_hashed` render `Edited ·path· (+N −M)`, with +N in `text-success` and −M in `text-destructive`. A line diff that jsdiff (`diff`) builds from `old_text` and `new_text` uses `bg-destructive/10` and `bg-success/10` rows, indented four. "Show all" opens a `Dialog` that reads the full call through the proxy, with numbered lines, full arguments and the full diff. A 404 shows the saved head and tail with "Full output wasn't kept for this call."
- **Dimension 5.1** — An edit renders its header counts and coloured diff rows → Test `test_edit_cell_renders_diff` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 5.2** — The diff of `a\nb` → `a\nc\nd` counts +2 −1 → Test `test_edit_diff_counts_lines` — DONE (`ui/packages/app/components/domain/tool-call-diff.test.ts`)
- **Dimension 5.3** — "Show all" renders every line of the full output → Test `test_output_dialog_shows_full_output` — DONE (`ui/packages/app/components/domain/FleetToolOutputDialog.test.tsx`)
- **Dimension 5.4** — A 404 falls back to head and tail with the not-kept note → Test `test_output_dialog_falls_back_when_not_kept` — DONE (`ui/packages/app/components/domain/FleetToolOutputDialog.test.tsx`)
- **Dimension 5.5** — The proxy forwards the call id percent-encoded and passes the status through → Test `test_tool_call_proxy_forwards_encoded_id` — DONE (`ui/packages/app/tests/tool-call-route.test.ts`)

### §6 — Every turn says when it ran, how long and what it cost

`toReplyMessage` carries the turn's tokens, wall time and cost in the reply's custom bag, never the trigger's, because `convertEvent`'s output is compared to detect trigger changes. A settled reply ends with dim "Worked for 41s · 12.4k tokens · $0.03", using formatters moved from `FleetStatusLine.tsx` into `lib/events/run-figures-format.ts`. A running reply's indicator adds its elapsed time ("Pondering… (1m 5s)") and its status stays named "Working". The row that opens a turn shows its time through `FleetMessageRow`'s `Timestamp`, once per turn.
- **Dimension 6.1** — A settled reply shows its figures line → Test `test_settled_reply_shows_figures` — DONE (`ui/packages/app/components/domain/FleetReplyFigures.test.tsx`)
- **Dimension 6.2** — An unreported figure is left out, never shown as zero → Test `test_unknown_figure_left_out` — DONE (`ui/packages/app/components/domain/FleetReplyFigures.test.tsx`)
- **Dimension 6.3** — A running reply shows elapsed time and no figures line → Test `test_running_reply_shows_elapsed` — DONE (`ui/packages/app/components/domain/FleetReplyFigures.test.tsx`)
- **Dimension 6.4** — The status line renders the same strings after the move → Test `test_status_line_formats_unchanged` — DONE (`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetStatusLine.test.tsx`)
- **Dimension 6.5** — The operator row shows the turn's time, and its reply shows no second copy → Test `test_turn_rows_show_timestamp` — DONE (`ui/packages/app/tests/fleet-thread/role-turns.test.ts`)

### §7 — Every runner tool reads in words

The copy map covers the runner's whole catalog. Argument names come from the runner: `afr_tools` for the tools it hosts (`pushover`, `update_plan`, `web_fetch`), the provider for `web_search`, and the specs that will host the rest (M211_001 sandbox tools, M211_002 nested loops, M212_001 verbs). A plan draws Codex's checklist (✔ done, ◐ in progress, □ to do, each also in words); a patch draws its diff with its files and counts; a hashed edit that names its line by tag (`target`, no `old_text`) names that line and draws no diff. `web_fetch`, `web_search`, `cron_list` and `cron_runs` change nothing, so they fold under Explored. Where a spec names only a verb's body (`cron_*`, `message`), the cell tries each spelling it knows and otherwise shows the arguments as they came. Output drops terminal control sequences and keeps a carriage-return progress line at its last state.
- **Dimension 7.1** — Each runner tool names its verb and target, running and done, and shows unknown argument names as they came → Test `test_runner_tools_name_verb_and_target` — DONE (`ui/packages/app/components/domain/tool-call-copy-runner.test.ts`)
- **Dimension 7.2** — A plan renders its checklist, a patch its diff, a hashed edit its line, and web and schedule lookups fold under Explored → Test `test_runner_cells_render_plan_patch_and_line` — DONE (`ui/packages/app/tests/fleet-tool-calls.runner.test.tsx`)
- **Dimension 7.3** — Output drops terminal control sequences and keeps a progress line at its last state → Test `test_output_drops_terminal_control` — DONE (`ui/packages/app/components/domain/tool-call-text.test.ts`)

### §8 — A cell never claims what the fleet did not do

The pre-landing review found where a cell could say something untrue; each is closed. A call the turn closed itself still takes a completion that lands later, and settle reads the saved trace once when it closed a call. "Show all" waits for the turn to end, since the runner posts full records at settle, and output kept only at its edges says it continues. A known tool whose arguments the runner dropped reads "(arguments not recorded)", never a default; text it may have cut at 256 bytes ends in "…". Explored keeps same-named files apart and shows a failed read's error. Rows are keyed by the runner's call id, and a finished call keeps its part fields while the reply streams. Readable text uses `text-subtle` (5.99:1 on the light page), not `text-dim` (4.41:1), and elapsed time needs no `Intl.DurationFormat`.
- **Dimension 8.1** — A completion that lands after its turn ended replaces the settle guess → Test `test_late_completion_replaces_the_settle_guess` — DONE (`ui/packages/app/lib/streaming/fleet-stream-frames.settle.test.ts`)
- **Dimension 8.2** — Settle reads the saved trace once after closing a call → Test `test_settle_rereads_the_trace_after_closing_a_call` — DONE (`ui/packages/app/lib/streaming/fleet-stream-registry.reply.test.ts`)
- **Dimension 8.3** — "Show all" appears only where the full read can answer, dropped arguments are never invented, and cut text ends in "…" → Test `test_cells_claim_only_what_they_hold` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 8.4** — Explored keeps same-named files apart and shows a failed read's error → Test `test_explored_keeps_paths_and_errors` — DONE (`ui/packages/app/components/domain/FleetExplored.test.tsx`)
- **Dimension 8.5** — A finished call keeps its part fields while the reply streams → Test `test_streamed_word_keeps_a_finished_call_s_part_fields` — DONE (`ui/packages/app/components/domain/fleetReplyMessage.test.ts`)
- **Dimension 8.6** — A cell's words reach 4.5:1, and elapsed time renders without `Intl.DurationFormat` → Test `test_cell_text_is_readable` — DONE (`ui/packages/app/tests/fleet-tool-calls.test.tsx`)
- **Dimension 8.7** — The dialog says when the arguments were not kept, and tool output stays literal text → Test `test_output_dialog_notes_dropped_arguments` — DONE (`ui/packages/app/components/domain/FleetToolOutputDialog.test.tsx`)

## Interfaces

```
FleetToolCall += { args?, status?: "succeeded"|"failed"|"interrupted", outputHead?, outputTail?, outputLineCount?, exitCode?, callId?, closedAtSettle?: true }
EventDetail   += { tool_calls: { calls: SavedToolCall[]; omitted_call_count: number } | null }
tool-call part : { toolCallId: "{event}:tool:{callId|index}", toolName, args?, result?: { status?, outputHead?, outputTail?, outputLineCount?, exitCode?, callId? }, isError, timing }
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
| 2.6 | unit | `test_reload_shows_saved_tool_rows` | three calls through the live reducer, and the same calls as the saved row a reload reads → identical cells |
| 3.1 | unit | `test_tool_cell_names_verb_and_target` | each non-read tool, running and done → expected header |
| 3.2 | unit | `test_unknown_tool_cell_calls_by_name` | `fly_status {app:"x"}` → `Called fly_status({"app":"x"})` |
| 3.3 | unit | `test_tool_cell_bullet_follows_status` | succeeded → success; failed, interrupted → destructive; interrupted header |
| 3.4 | unit | `test_tool_cell_previews_three_rows` | 8 lines → 3 rows + "+5 lines" |
| 3.5 | unit | `test_tool_cell_names_empty_and_unknown_output` | empty success → `(no output)`; no status → `(output unavailable)` |
| 3.6 | unit | `test_running_bullet_respects_reduced_motion` | reduced-motion media → no shimmer animation class |
| 3.7 | e2e | `test_live_tool_cell_settles_green` | fixture frames → shimmer, then green "Requested GET …" and output |
| 3.8 | unit | `test_command_cell_renders_like_codex` | 4-line command, exit 2 → `Ran` + 1 header line + 2 rail lines + `… +1 line` + ` (exit 2)` |
| 4.1 | unit | `test_consecutive_reads_fold_under_explored` | read a, read b, recall q → "Read a, b", "Search q in memory" |
| 4.2 | unit | `test_non_read_call_splits_explored` | read, write, read → Explored, Wrote, Explored |
| 4.3 | unit | `test_explored_header_tracks_running` | one running → "Exploring"; all done → "Explored" |
| 4.4 | unit | `test_failed_read_marks_its_explored_line` | second read failed → its line "(failed)", group stays |
| 5.1 | unit | `test_edit_cell_renders_diff` | edit arguments → header "(+2 −1)", coloured rows |
| 5.2 | unit | `test_edit_diff_counts_lines` | `a\nb` → `a\nc\nd` → +2 −1 |
| 5.3 | unit | `test_output_dialog_shows_full_output` | 224-line response → 224 numbered lines |
| 5.4 | unit | `test_output_dialog_falls_back_when_not_kept` | 404 → head, tail and the not-kept note |
| 5.5 | unit | `test_tool_call_proxy_forwards_encoded_id` | id `7:3` → upstream path ends `/tool-calls/7%3A3`; 404 passes through |
| 6.1 | unit | `test_settled_reply_shows_figures` | tokens 12400, wall 41000 → "Worked for 41s · 12.4K tokens · $…" |
| 6.2 | unit | `test_unknown_figure_left_out` | cost `null` → no cost segment, no "$0" |
| 6.3 | unit | `test_running_reply_shows_elapsed` | running 65 s → the "Working" status shows "(1m 5s)", no figures line |
| 6.4 | unit | `test_status_line_formats_unchanged` | fixed figures → same strings as before the move |
| 6.5 | unit | `test_turn_rows_show_timestamp` | operator row → `time` with `datetime`; its reply row → no `time` |
| 7.1 | unit | `test_runner_tools_name_verb_and_target` | each runner tool, running and done → its header (`git {args:["status"]}` → "Ran git status"); `cron_remove {name}` → its arguments as they came |
| 7.2 | unit | `test_runner_cells_render_plan_patch_and_line` | plan steps → ✔ ◐ □ and "(done)"; `-old +new +more` → "Edited a.ts (+2 −1)"; `{target:"L10:abc"}` → "Edited a.md at L10:abc"; search, fetch, cron_list → one Explored group beside a "Notified" cell |
| 7.3 | unit | `test_output_drops_terminal_control` | `\u001B[32mok\u001B[0m` → "ok"; `10%\r100% done` → "100% done" |
| 8.1 | unit | `test_late_completion_replaces_the_settle_guess` | open call, settle, its completion → status from the completion, no `closedAtSettle` |
| 8.2 | unit | `test_settle_rereads_the_trace_after_closing_a_call` | settle with an open call → one detail read; the saved status replaces the guess |
| 8.3 | unit | `test_cells_claim_only_what_they_hold` | running turn → no button, settled → button; head and tail on one line → "… output continues"; no arguments → "Requested (arguments not recorded)", never "GET"; a 260-byte url → ends in "…" |
| 8.4 | unit | `test_explored_keeps_paths_and_errors` | `src/a/index.ts`, `src/b/index.ts`, a failed read → "Read a/index.ts, b/index.ts" and the error |
| 8.5 | unit | `test_streamed_word_keeps_a_finished_call_s_part_fields` | same calls, new reply text → the same `result` and `timing` objects |
| 8.6 | unit | `test_cell_text_is_readable` | a settled cell → `text-text-subtle`, never `text-text-dim` |
| 8.7 | unit | `test_output_dialog_notes_dropped_arguments` | `truncated_arguments:true, arguments:{}` → the not-kept note, no diff, no arguments disclosure |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Cells, Explored and diffs render like Codex for every runner tool, never claim what did not happen, and a reloaded turn reads as it did live (§2–§5, §7, §8) | `cd ui/packages/app && bunx vitest run tests/fleet-tool-calls.test.tsx tests/fleet-tool-calls.runner.test.tsx components/domain/tool-call-copy.test.ts components/domain/tool-call-copy-runner.test.ts components/domain/tool-call-text.test.ts components/domain/FleetExplored.test.tsx components/domain/tool-call-diff.test.ts` | exit 0 | P0 | |
| R2 | Live cells settle in the real page (§3) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts tests/e2e/acceptance/fleet-reply-parts.spec.ts` | exit 0 | P0 | |
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
- Codex's footer (model, context left), since no runner signal carries context use; syntax highlighting inside outputs, which are files and HTTP bodies of unknown type.

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
- **Metrics review and skill chain** — No analytics or funnel playbook update required: no new tracked user action. Skill-chain outcomes: `/orly-write-unit-test` ran per Section and at the boundary, its ledger in the PR's Session Notes; `/orly-write-integration-test` is N/A, since the diff stays inside the app's reducer and render and every frame is a fixture; gstack `/review` ran twice, every finding resolved (Review decisions).
- **Re-scope (Oct 02, 2026)** — Indy chose a fresh Rust runner ("The port is a fresh port, since we always have the last binary with us and running."). This spec's rendering is unchanged; its frames come from that runner, and Codex engine events map onto the same frames (`docs/architecture/runner_execution.md` §Coding engines).
- **PLAN amendments (Oct 04, 2026)** — Indy (in-session): "ensure we use the altest assistant-ui pacakge. Do we need to write our own assistant-ui? or use the pacakge?" → the package, bumped to 0.15.23; "avoid handrolled code, use it only if need be" → edit diffs from jsdiff 9. Agent defaults stated at PLAN with no correction: `text-info` replaces `text-accent` because `--accent` is `--surface-3` (`ui/packages/design-system/src/theme.css:36`), a surface; a red "(failed)" marks failed calls (`docs/DESIGN_SYSTEM.md` §Color; Codex `history_cell/dynamic.rs:146` swaps the verb to `Failed`); `calculator` leaves the copy map because the Rust runner's catalog has no such tool (`rustd/crates/afr_tools/src/catalog.rs`), and `shell` joins the command cell; the running indicator keeps its existing verb and adds the elapsed; diff rows carry no file line numbers because edit arguments carry no offsets.
- **Proxy shape (Oct 04, 2026)** — Indy (in-session): "IS this the way to go for a proxy and is it like others?", then "just to be clear we are reducing code here? by sharing?"; chose "Share it, move all 3". The four JSON reads share `lib/api/live-json-proxy.ts` as the two streams share `lib/api/event-stream-proxy.ts`; the three existing route tests pass unchanged. Next 16.3.6 decodes route params (`next/dist/shared/lib/router/utils/route-matcher.js:19`), so the call id is encoded once.
- **Package refresh (Oct 04, 2026)** — Indy (in-session): "also check and update all the packages like typescript, oxlint, react, next.js, clerk, tailwind, vitest, pretty much all the pacakges"; chose "Fold into this PR"; "effect needs to move up to 4.0.0"; "If next.js, react are updated then use the latest feature of them." Every workspace moves to latest in one commit. React 19.3.0, Tailwind 4.3.3 and TypeScript 7.0.2 were already latest. `typescript-jsapi` stays on typescript@6.0.2 per Indy's Jul 30, 2026 decision, because typescript@7 ships no compiler API (`typeof ts.createSourceFile` is `undefined` under 7.0.2).
- **EXECUTE defaults (Oct 04, 2026)** — stated here, uncorrected. The runner cuts every argument string at 256 bytes, unmarked (`rustd/crates/afr_agent/src/trace.rs:105`), so a write's `(+N)` shows only for whole content, and an edit that may have been cut draws no diff and offers "show all" (`afd_wire/src/tool_detail.rs:50`). Figures spell "12.4K" (Codex's `format_tokens_compact`) and "1m 5s"; one hidden line reads "+1 line". Interrupted reads "(output unavailable)". "Show all" needs a call id. One timestamp per turn, on the row that opens it, bringing turn times back after `34d973fe9` (Jul 26, 2026). Diffs ignore a missing final line break (`ignoreNewlineAtEof`).
- **Reload proof on the unit tier (Oct 04, 2026)** — A reload reads the thread on the server (`app/(dashboard)/w/[workspaceId]/fleets/[id]/components/view-data.ts:93`), which a browser route cannot stand in for. Indy (in-session, Oct 04, 2026), asked how Dimension 2.6 is proven, chose "Unit tier (Recommended)": the same calls through the live reducer and as the saved row a reload reads must render identical cells. R2 keeps the live half; R1 carries the reload half.
- **Review decisions (Oct 05, 2026)** — `/review` (seven specialists, the adversarial pass, the red team) found where a cell could say something untrue. Indy chose D1 "Fix all 8 (Recommended)", D2 "Fix all (Recommended)" and D3 "Add all 8 (Recommended)", which became §8 with its tests. A second pass over those fixes, in three bounded slices, found six more P2 places a cell or the settle path could claim something untrue or redraw for nothing, and eleven P3s. All are fixed, each behaviour fix with a test that fails without it, except `font-mono` on `ListItem`, which `tests/interface-typography.test.ts:6-9` permits. On D4, whether the runner's newer tools could wait for a follow-up spec: "Can you if the follow on spec have the support for the tools? if not it must be in this PR". No pending spec renders them (M211_001 relies on this spec's cells; M212_001 on a "Scheduled …" cell), so §7 lands here. Arguments were read from source: `afr_tools` `pushover.rs`, `plan.rs`, `web_fetch.rs`; M211_001, M211_002 and M212_001 for the rest; and the earlier runner's `file_edit_hashed`, which names its line by `target` with no `old_text` (NullClaw `file_edit_hashed.zig:89`).
- **Deferrals** — none.
