<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M139_001: Fleet event failures name their cause and repeats collapse

**Prototype:** v2.0.0
**Milestone:** M139
**Workstream:** 001
**Date:** Jul 22, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — customer-facing: repeated webhook failures render as dozens of identical, cause-less rows in both the fleet chat and the Events table; the operator scrolls a full page and still cannot see which check failed
**Categories:** API, UI
**Batch:** B1 — standalone; no sibling workstreams
**Branch:** feat/m139-event-legibility
**Test Baseline:** unit=2809 integration=371
**Depends on:** none — M138_001 (console chat fidelity) is in `done/` and merged on `main`
**Provenance:** Large Language Model (LLM)-drafted (claude-fable-5, Jul 22, 2026) — authored from Indy's live session screenshots (Chat + Events tabs, fleet `github-pr-reviewer`, workspace `gentle-mesa-130`) and a code read of the render path (`fleetMessageRenderers.tsx`, `event-summary.ts`, `EventsList.tsx`) and the report path (`execution_result.zig`, `event_rows.zig`, `service_report.zig`)
**Canonical architecture:** `docs/architecture/data_flow.md` §"Two streams + one pub/sub channel"; `docs/architecture/runner_fleet.md` for the runner→report path

---

## Overview

**Goal (testable):** A `startup_posture` failure renders its cause sentence — from a new durable `failure_detail` — in the chat row, the Events table, and the live completion frame; ten identical consecutive webhook failures render as one persistent banner plus one collapsed group row, never ten equal rows.

**Problem:** The fleet chat reads like a table and the Events table reads like a stuck record. Every webhook delivery renders at the same visual weight as an operator message, each with a second outcome row, so a burst of GitHub deliveries floods the thread and the operator scrolls to reach the composer. Every failure says only its class ("Failed a startup safety check") — never which check or why — because `core.fleet_events` stores only `failure_label`, several `startup_posture` classification sites return an empty `content`, and the live completion frame carries only `status` (the chat shows a generic "The run failed." until reload). The remediation `guidance` hook in `event-summary.ts` is authored but rendered nowhere.

**Solution summary:** One additive column and a presentation overhaul. The runner names its failure cause at every classification site and carries it on the existing `result` wire frame (defaulted field — wire-compatible both directions); the report verb persists it, the envelope and the completion frame surface it, and the shared vocabulary module renders it under the plain-language failure sentence with its remediation guidance. In the chat, integration events demote to compact one-line rows, consecutive repeats coalesce into one expandable "×N" group, and a repeating failure pins one banner (count, last seen, cause, guidance), so the operator reaches their own conversation without scrolling. The Events table groups consecutive identical failures and dims zero-value metrics on failed rows. Design variants come first via `/design-shotgun` (board committed under `docs/design/`, Indy picks); `/design-review` runs as the designer QA pass after implementation.

## PR Intent & comprehension handshake

- **PR title (eventual — what the merged Pull Request (PR) is called):** feat: name fleet failure causes and collapse repeated events
- **Intent (one sentence):** An operator opening a struggling fleet sees at a glance what is failing, why, and how often — instead of scrolling a wall of identical cause-less rows.
- **Handshake** (filled at PLAN) — Restatement: make a struggling fleet self-explaining at a glance — every failure carries its cause from the runner all the way to the row, and repetition renders as one loud banner plus one collapsed group instead of a wall of identical lines. ASSUMPTIONS I'M MAKING: (1) the cause is a new durable column + defaulted wire field, never an overload of `response_text` — that field is the fleet's reply, and a failure cause is not a reply; (2) coalescing is presentation-only — a pure grouping function over the already-ordered event array; the stream registry, storage, and pagination are not restructured; (3) the banner threshold (≥2 consecutive identical terminal failures) and the truncation cap are named constants the design board may tune, not architecture; (4) `feat/data-table-tanstack` is in flight in a sibling worktree with zero commits — §7 is sequenced last and rebases over `main` before touching `EventsList.tsx`, building on whichever table engine has landed; (5) the design board variants are committed as static pages under `docs/design/` following the `models-creds-*` precedent, and Indy's pick gates EXECUTE on §3–§7 (backend §1–§2 are variant-independent and may start first).

## Implementing agent — read these first

1. `ui/packages/app/lib/events/event-summary.ts` — the shared vocabulary all three surfaces (thread, summary strip, events table) read; extend it, never fork per-surface copies.
2. `src/agentsfleetd/fleet/event_rows.zig` + `src/agentsfleetd/state/fleet_events_store.zig` — the durable write/read of `failure_label`; `failure_detail` follows the identical shape end to end.
3. `src/lib/contract/execution_result.zig` — the shared runner↔daemon wire type; additive defaulted fields are the established compatibility pattern (see the token-split fields).
4. `docs/SCHEMA_CONVENTIONS.md` + `schema/031_fleet_runners_delete_grant.sql` — migration conventions and the most recent migration to mirror.
5. `~/.gstack/projects/agentsfleet-agentsfleet/designs/fleet-workspace-20260721/` and `docs/design/models-creds-board.html` — the approved variant-A row grammar this spec extends, and the prior shotgun-board artifact pattern M139's variants follow.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/032_fleet_events_failure_detail.sql` | CREATE | Additive `ALTER TABLE … ADD COLUMN failure_detail` — first slot under Indy's additive-migration model; shipped slots frozen. |
| `schema/embed.zig` | EDIT | Register slot 32 (single-source migration registry). |
| `make/quality.mk` | EDIT | Remove `_schema_gate_check`/`check-schema-gate` per Indy (Discovery D3) — the teardown-era gate contradicts the new model. |
| `docs/SCHEMA_CONVENTIONS.md` | EDIT | §Migration Model rewritten: additive migrations, frozen slot files, destructive changes owner-gated. |
| `src/lib/contract/execution_result.zig` | EDIT | `failure_detail` field; §8 tagged `outcome` replacing `exit_ok` + `failure`. |
| `src/lib/contract/report_mapping.zig` | CREATE | §8: the single result↔report conversion pair (`protocol.zig` is at its length cap). |
| `src/runner/daemon/loop.zig` | EDIT | §8: outcome derivation moves onto the tagged type. |
| `src/runner/child_exec.zig` | EDIT | Startup checks populate the detail (two sites already carry readable messages in `content`). |
| `src/runner/child_supervisor.zig` | EDIT | `failed(.startup_posture)` sites name the failing step. |
| `src/runner/engine/runner.zig` | EDIT | Error→class mapping carries the cause. |
| `src/runner/pipe_proto.zig` | EDIT | `result` frame serialises the field. |
| `src/runner/child_supervisor_result.zig` | EDIT | Parses the field; absent ⇒ empty. |
| `src/agentsfleetd/fleet/event_rows.zig` | EDIT | Report write persists `failure_detail`. |
| `src/agentsfleetd/state/fleet_events_store.zig` | EDIT | Row read + envelope carry the field. |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | Completion publish passes label + detail. |
| `src/agentsfleetd/fleet/service_activity.zig` | EDIT | Completion frame gains `failure_label` + `failure_detail`. |
| `ui/packages/app/lib/api/events.ts` | EDIT | `EventRow` mirrors the envelope verbatim. |
| `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | Frame parse + merge render the live cause without reload. |
| `ui/packages/app/lib/events/event-summary.ts` | EDIT | Outcome renders detail under the failure sentence; guidance presentation exported. |
| `ui/packages/app/components/domain/fleetMessageRenderers.tsx` | EDIT | Compact integration rows; failure rows render cause + guidance. |
| `ui/packages/app/components/domain/FleetMessageRow.tsx` | EDIT | Compact row variant beside the full skeleton. |
| `ui/packages/app/components/domain/FleetThread.tsx` | EDIT | Mounts banner and grouping over the event array. |
| `ui/packages/app/components/domain/FleetEventGroup.tsx` | CREATE | Collapsed "×N" group row (name indicative — match local style). |
| `ui/packages/app/components/domain/FleetFailureBanner.tsx` | CREATE | Persistent-failure banner (name indicative). |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | Groups consecutive identical failures; dims zero metrics on failed rows. |
| `ui/packages/app/components/domain/EventDetailsDialog.tsx` | EDIT | Inspect shows the full untruncated cause. |
| `ui/packages/app/tests/` + colocated `*.test.tsx` / Zig test blocks | EDIT/CREATE | One test per Dimension (Test Specification). |
| `docs/design/fleet-events-*.html` | CREATE | Shotgun board + variants; Indy's pick recorded in Discovery. |
| `docs/architecture/data_flow.md` | EDIT | Completion frame + event row carry the failure cause. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TGU (§8: the tagged outcome is the rule applied to the result type itself), UFS (every new label/sentence is a named constant), NDC + NLR (the unused `guidance` hook gets a consumer in this diff — no latent surface remains), ESC (`failure_detail` is escaped at JSON emission), STS + NSQ (no schema defaults; schema-qualified named-constant SQL), TST-NAM (milestone-free test identifiers), XCC (cross-compile both linux targets), FLS (new store reads drain), IMS (`[]const u8` for the detail), ORP (any renamed export swept cross-layer).
- `dispatch/write_zig.md` — runner + daemon edits (lifecycle, tagged results, length caps).
- `dispatch/write_ts_adhere_bun.md` — all dashboard edits (file-shape verdicts, design-system substitution).
- `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — migration 032 (additive, single-concern, ≤100 lines).
- `docs/DESIGN_SYSTEM.md` §Operational Restraint — banner and group rows stay quiet, token-only styling.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — runner, daemon, wire type | Cross-compile x86_64-linux + aarch64-linux; colocated tests. |
| PUB / Struct-Shape | yes — new field on the shared `pub` wire struct | Defaulted field on the existing struct; FILE SHAPE DECISION per touched pub surface. |
| File & Function Length (≤350/≤50/≤70) | yes — `fleetMessageRenderers.tsx` and `FleetThread.tsx` are near caps | Grouping, banner, and filter land as the two new components + a pure grouping module, not inline growth. |
| UFS (repeated/semantic literals) | yes — labels, frame keys | Named constants; the frame key spelling shared runner↔daemon↔dashboard verbatim. |
| UI Substitution / DESIGN TOKEN | yes — banner, group row, filter control | Design-system primitives (`Badge`, `Alert`, existing segmented patterns); theme tokens only. |
| SCHEMA GUARD | model changed by Indy (Discovery D3) | Additive migration in a new numbered slot; the `check-schema-gate` lint target is removed in this diff; prior slots untouched. |
| LOGGING / LIFECYCLE / ERROR REGISTRY | no — no new log lines, error codes, or init/deinit surfaces planned | Re-verdict at PLAN if a new allocation-owning component appears. |

## Prior-Art / Reference Implementations

- **Reference:** M138_001 + `designs/fleet-workspace-20260721/` variant-A — the approved row grammar; this spec adds a compact variant and a group variant inside that grammar, it does not replace the skeleton.
- **Reference:** `EventsList.tsx` sibling data surfaces (API keys, runners, billing) — the grouped table stays the standard `DataTable`; grouping is row-model preprocessing, not a new table component.
- **Reference:** the `failure_label` column in the fleet-events DDL — the exact in-file shape (`TEXT NULL` + provenance comment) `failure_detail` mirrors.
- **Greenfield:** chat coalescing and the failure banner have no in-repo prior art; their shape is defined by the `/design-shotgun` board committed under `docs/design/` and Indy's recorded pick.

## Sections (implementation slices)

### §1 — Durable failure cause, end to end

A failure that cannot say why it failed is noise. The runner names the cause at classification time; the cause survives the wire, the row, the envelope, and the live frame. **Implementation default:** truncate at a named byte cap at the report write (chat renders one line; Inspect shows the full stored value) because a runaway child must not bloat rows.

- **Dimension 1.1** — A new additive migration adds nullable `failure_detail` (shipped slots frozen — Indy's migration-model decision, Discovery D3); the store roundtrips the field → Test `test_event_row_failure_detail_roundtrip`
- **Dimension 1.2** — DONE — `ExecutionResult` gains defaulted `failure_detail`; a frame without the field parses to empty (old runner ⇒ new daemon, and inverse) → Test `test_result_frame_absent_detail_parses_empty`
- **Dimension 1.3** — DONE — Every `startup_posture` classification site emits a human-readable cause (`child_exec` reuses its existing messages; `child_supervisor` names the failing step; `engine/runner` maps its error set) → Test `test_startup_posture_sites_carry_detail`
- **Dimension 1.4** — Report verb persists label + detail; the events envelope returns both verbatim → Test `test_report_persists_and_envelope_returns_detail`
- **Dimension 1.5** — DONE — Completion frame carries `failure_label` + `failure_detail`; the dashboard merge renders the real sentence live, no reload → Test `test_stream_merge_renders_failure_detail`
- **Dimension 1.6** — DONE — Over-cap and control-character detail is truncated and escaped at the report write; the stored value is what Inspect shows → Test `test_detail_truncated_and_escaped_at_write`

### §2 — The vocabulary renders cause and guidance

`event-summary.ts` stays the single voice. The failure sentence keeps its plain-language label; when a detail exists it renders beneath it; the authored-but-unrendered `guidance` presentation gets its consumer (startup guidance points the operator at the fleet's configuration surface). Unknown labels keep the render-the-tag fallback.

- **Dimension 2.1** — DONE — Outcome presentation includes the detail when present and exactly the canned sentence when absent → Test `test_outcome_includes_detail_when_present`
- **Dimension 2.2** — Startup failures render the guidance line in the chat row and the details dialog → Test `test_guidance_renders_for_startup_failures`

### §3 — Conversation dominates; activity compacts

Operator and fleet turns keep the full row skeleton. Integration (system-role) events render a compact single-line variant — muted, headline + time, no separate outcome row when the outcome is the only content. Recognized change-proposal headlines render the action as a `Badge`; when the stored payload carries a link field the headline links out.

- **Dimension 3.1** — System-role rows render the compact variant; payload disclosure remains reachable → Test `test_system_rows_render_compact`
- **Dimension 3.2** — Operator sends, optimistic/failed states, and fleet replies render exactly as before (regression) → Test `test_conversation_rows_unchanged`
- **Dimension 3.3** — Change-proposal rows render an action `Badge` and link out only when the payload carries a link → Test `test_change_proposal_badge_and_conditional_link`

### §4 — Consecutive repeats coalesce

Runs of system-role events with the same actor, headline, and outcome collapse into one expandable group row — "headline ×N · first–last time" — with every delivery (and its payload disclosure) inside. Any operator or fleet row breaks the group. **Implementation default:** a pure, memoized grouping function over the event array at render time, because the stream registry already owns ordering and identity.

- **Dimension 4.1** — Grouping coalesces qualifying runs and never groups user/assistant rows or differing outcomes → Test `test_grouping_rules`
- **Dimension 4.2** — Group row renders count + time range; expansion restores individual rows → Test `test_group_row_expands`
- **Dimension 4.3** — A live frame matching the newest group joins it and updates the count → Test `test_live_frame_extends_group`

### §5 — A repeating failure is one banner, not N rows

The same `failure_label` on ≥2 consecutive terminal failures pins one banner above the viewport: count, last-seen time, cause sentence, guidance. The banner is exempt from the §6 filter and clears when a non-failure terminal event lands.

- **Dimension 5.1** — Banner appears at the threshold with count, last-seen, and cause → Test `test_banner_appears_with_cause`
- **Dimension 5.2** — A single failure shows no banner; a processed event clears it → Test `test_banner_threshold_and_clear`

### §6 — All / Conversation / Activity filter — DEFERRED

Cut by Indy at the design lock (verbatim quote in Discovery): the thread always shows all rows. Reactivation condition: usage evidence that operators need to isolate conversation from activity after the §3–§5 demotion/coalescing ships. No Dimensions; no tests.

### §7 — The Events table stops repeating itself

Consecutive rows with identical actor, status, and failure cause collapse into one row with a "×N" `Badge`, expandable in place; Inspect on the group opens the latest delivery. Zero-value Cost/Tokens/Duration on failed rows render dimmed. **Implementation default:** grouping within the fetched page only — no cross-page stitching over keyset pagination.

- **Dimension 7.1** — Consecutive identical failures collapse with a count; expansion restores rows → Test `test_table_groups_identical_failures`
- **Dimension 7.2** — Failed rows dim zero metrics; processed rows render unchanged → Test `test_zero_metrics_dimmed_on_failures`
- **Dimension 7.3** — Grouping never crosses a page boundary; pagination behaviour unchanged (regression) → Test `test_grouping_respects_page_boundary`

### §8 — The result type stops permitting illegal states

Folded in by Indy at the §1 review (Discovery D4). Three findings surfaced by §1's cost: the execution result encodes its verdict as `exit_ok: bool` beside `failure: ?FailureClass` (four representable states for two meanings, the "null iff processed" invariant hand-written at three consumers); the same type is disassembled into the report wire and rebuilt field-by-field on the far side; and the live chat merge rescans the whole event array per streaming chunk. **Implementation default:** the domain type becomes precise (tagged outcome) while the report wire stays flat and defaulted, because the report is the genuine cross-version boundary and the child frame is not (the daemon forks its own binary).

- **Dimension 8.1** — DONE — The result carries a tagged `outcome` (`completed` | `failed` with its classified cause); a result that claims success while naming a failure does not compile → Test `test_outcome_union_forbids_success_with_failure`
- **Dimension 8.2** — DONE — One conversion pair owns result↔report translation, including the trust-boundary guard that a cause never accompanies a clean outcome → Test `test_report_round_trip_preserves_every_wire_field`
- **Dimension 8.3** — DONE — The completion frame takes a named failure-cause struct, not adjacent same-typed strings → Test `test_completion_frame_cause_is_named`
- **Dimension 8.4** — DONE — Chunk and completion merges locate their event once and copy the array once, matching the tool-call helper → Test `test_chunk_merge_single_pass`

## Interfaces

```
core.fleet_events        — gains nullable text column `failure_detail` (additive;
                           semantics app-enforced, no schema default)
result wire frame        — runner→daemon: + failure_detail (string; absent ⇒ "")
EventRow envelope        — + failure_detail: string | null (dashboard mirrors verbatim)
completion frame (live)  — + failure_label?: string, failure_detail?: string
Dashboard                — no route or navigation changes; chat row grammar gains
                           compact + group variants inside the approved skeleton
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Old runner omits the field | version skew across the wire | Parse defaults to empty; every surface renders the pre-M139 canned sentence. |
| Oversized / multiline detail | runaway or hostile child output | Truncated at the named cap at report write; chat renders one line; Inspect shows the stored value. |
| Control characters in detail | hostile child output | Escaped at JSON emission (RULE ESC); rendered only as text nodes — never interpreted as markup. |
| Group swallows a distinct failure | same label, different cause | Grouping key includes the outcome sentence; differing causes never merge. |
| Banner outlives recovery | stale client state | Any non-failure terminal event clears it; negative test proves the clear. |
| Coalescing hides a failed send | operator's own optimistic/failed rows | User/assistant roles are never grouped (Invariant 1). |
| Cross-page truncation lies about counts | keyset pagination | Groups form within one fetched page; the count claims only what the page holds. |

## Invariants

1. Operator and fleet rows never coalesce — role guard in the grouping function; unit test asserts no user/assistant row ever enters a group.
2. The `EventRow` envelope is mirrored verbatim daemon↔dashboard (no shim, no rename) — integration test asserts the envelope field names.
3. `failure_detail` is render-safe: escaped at emission, rendered only as text nodes — component test asserts literal rendering of markup-shaped input.
4. A frame or row without `failure_detail` renders exactly the pre-M139 sentence — wire-compatibility unit test.
5. No opaque identifier renders in the banner or group rows — both reuse `senderLabelFor`; test with an opaque-actor fixture.
6. Every new user-facing string is a named constant — UFS gate, enforced at edit time.
7. A result cannot claim success and name a failure — the outcome is a tagged union, so the illegal pairing is unrepresentable and the compiler replaces the hand-written guard at every consumer (§8).
8. The report wire and the domain result never drift — one conversion pair, proven by a round-trip test asserting every wire field survives (§8).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

This workstream renders existing durable data; it adds, renames, and removes no analytics events, and no funnel changes → no analytics/funnel playbook update (Discovery records the no-change reason).

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_event_row_failure_detail_roundtrip` | Report write with detail → row read returns the same bytes; migration applied. |
| 1.2 | unit | `test_result_frame_absent_detail_parses_empty` | Frame without the field → empty detail; frame with it → verbatim value. |
| 1.3 | unit | `test_startup_posture_sites_carry_detail` | Each classification site's result carries a non-empty, distinct cause. |
| 1.4 | integration | `test_report_persists_and_envelope_returns_detail` | Report → events endpoint returns `failure_label` + `failure_detail` verbatim. |
| 1.5 | unit | `test_stream_merge_renders_failure_detail` | Completion frame with label+detail → merged event's outcome carries the cause without refetch. |
| 1.6 | unit | `test_detail_truncated_and_escaped_at_write` | Over-cap / control-character detail → stored truncated + JSON-escaped (negative). |
| 2.1 | unit | `test_outcome_includes_detail_when_present` | Row with detail → sentence + cause; row without → canned sentence only. |
| 2.2 | unit | `test_guidance_renders_for_startup_failures` | `startup_posture` row → guidance line present in row and dialog; other labels → absent. |
| 3.1 | unit | `test_system_rows_render_compact` | Webhook event → single-line row; payload disclosure reachable. |
| 3.2 | unit | `test_conversation_rows_unchanged` | Steer + optimistic + failed + fleet reply fixtures → pre-M139 render (regression). |
| 3.3 | unit | `test_change_proposal_badge_and_conditional_link` | Payload with action → `Badge`; link only when a link field exists (incl. malformed-payload negative). |
| 4.1 | unit | `test_grouping_rules` | Runs coalesce; role boundary, differing outcome, and interleaved steer all break groups. |
| 4.2 | unit | `test_group_row_expands` | 5-event run → "×5" + time range; expansion yields 5 rows. |
| 4.3 | unit | `test_live_frame_extends_group` | Frame matching newest group → count increments; non-matching → new row. |
| 5.1 | unit | `test_banner_appears_with_cause` | 2 consecutive identical failures → banner with count, last-seen, cause. |
| 5.2 | unit | `test_banner_threshold_and_clear` | 1 failure → no banner; failures then processed → banner gone (negative). |
| 7.1 | unit | `test_table_groups_identical_failures` | 15 identical failure rows → 1 group row "×15"; expansion restores. |
| 7.2 | unit | `test_zero_metrics_dimmed_on_failures` | Failed row with 0/0/0 → dimmed; processed row → unchanged (regression). |
| 7.3 | unit | `test_grouping_respects_page_boundary` | Identical rows across two pages → two groups; cursors unchanged (regression). |
| 8.1 | unit | `test_outcome_union_forbids_success_with_failure` | A completed outcome exposes no cause; a failed one always carries its `Failure`. |
| 8.2 | unit | `test_report_round_trip_preserves_every_wire_field` | result→report→result returns the original; a cause on a processed outcome is dropped at the boundary (negative). |
| 8.3 | unit | `test_completion_frame_cause_is_named` | Frame built from a named cause struct emits label+detail; a clean completion emits both empty. |
| 8.4 | unit | `test_chunk_merge_single_pass` | Chunk and completion merges update only the target event and preserve every other reference. |
| end-to-end (e2e) | e2e | `acceptance-e2e` console walk | Failing fleet → operator sees banner + collapsed group + cause sentence on the real rendered console. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Failure rows name their cause; repeats collapse; banner pins (§2–§5) | `cd ui/packages/app && bun test tests/fleet-thread.test.ts components/domain/FleetEventGroup.test.tsx components/domain/FleetFailureBanner.test.tsx` | exit 0 | P0 | |
| R2 | Design board committed and a variant chosen (Discovery quote) | `ls docs/design/ \| grep -c "fleet-events"` | count ≥ 1 | P0 | |
| R3 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes (schema + envelope touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the console path | `make acceptance-e2e` | exit 0 | P0 | |
| S5 | No leaks (store read/write touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. R1's package-scoped run is focused evidence only — package-scoped runners are not verification; S1's `make test-unit-all` is the gate. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted. The one latent surface (`guidance` in `event-summary.ts`, exported but unrendered) gains its consumer in §2 rather than deletion (RULE NDC/NLR resolution).

## Out of Scope

- Webhook redelivery/replay controls and retention/pruning of failure rows — separate backend spec if wanted.
- Cross-page group stitching or server-side aggregation of repeats — page-local grouping only until evidence demands more.
- Re-threading the event model (turn-based conversation storage) — refactor larger than the problem (Decomposition).
- Analytics events for filter/banner usage — dashboard restraint; no counters before evidence.
- Fixing `github-pr-reviewer`'s underlying startup failure in `gentle-mesa-130` — operational configuration, not code.
- Rich cards for connectors beyond the two recognized payload shapes — new shapes arrive with their connectors.
- User-docs (`docs.agentsfleet.net`) + changelog updates ride CHORE(close) per the lifecycle, not a Section here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens the struggling `github-pr-reviewer` fleet and, without scrolling or clicking, reads one banner: the failing check's name, "×15", last seen 12:03 — and the chat below it is three quiet lines plus his own conversation.
2. **Preserved user behaviour** — steering from the composer, optimistic sends and retry, payload disclosure, Inspect dialog, keyset pagination, live Server-Sent Events (SSE) updates, Jump to latest: all unchanged.
3. **Optimal-way check** — the unconstrained shape re-threads the event model into conversation turns; one additive column plus presentation grouping delivers the moment without storage upheaval. Acceptable now; the re-thread stays a named non-goal.
4. **Rebuild-vs-iterate** — iterate. The approved variant-A skeleton and the frame/envelope pipeline stay; determinism of the render path is preserved (pure grouping function over the same ordered array).
5. **What we build** — one migration, one wire field, cause text at classification sites, envelope + frame plumbing, compact/group/banner chat presentation, grouped events table, a shotgun board.
6. **What we do NOT build** — see Out of Scope; headline rejections: no event-model re-thread (too big), no server aggregation (page-local suffices), no new analytics (no evidence yet).
7. **Fit with existing features** — compounds with `RunMetricsStrip` and `EventDetailsDialog` through the shared vocabulary module; must not destabilize the optimistic-steer reconciliation path in `useFleetEventStream`.
8. **Surface order** — User Interface (UI)-first, diverging from the repo's Command-Line Interface (CLI)-first default: the defect is a console legibility failure; the API change is subordinate plumbing the console consumes, and the CLI inherits the richer envelope for free.
9. **Dashboard restraint** — the banner needs ≥2 consecutive identical failures before it speaks; zero metrics dim rather than hide; no health scores anywhere.
10. **Confused-user next step** — the banner's guidance line names the failing check and points at the fleet's configuration surface; every failure row carries its cause. No ticket surface needed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream — the cause column and the presentation overhaul ship together because the banner and failure rows are only legible with the cause present; splitting would ship a prettier "Failed a startup safety check" fifteen times.
- **Alternatives considered:** (a) UI-only polish — rejected: half the complaint is missing content, which no amount of grouping supplies; (b) full event-model re-thread with server-side aggregation — rejected: a refactor larger than the problem, trading determinism of the merged-on-`main` stream pipeline for structure nothing yet demands.
- **Patch-vs-refactor verdict:** this is a **patch** (with one additive schema evolution) because every touched layer keeps its shape — the wire type, the row, the envelope, and the row grammar all extend rather than change.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision. Jul 22, 2026 (spec-vs-rules, **superseded by D3 below**): the authored spec prescribed a new numbered migration file; `docs/SCHEMA_CONVENTIONS.md` then forbade that pre-v2.0.0, so the spec was briefly amended to an inline `schema/015` edit. **D3 (Jul 22, 2026, harness ack)** — Indy, asked explicitly about `_schema_gate_check` in `make/quality.mk`: "Drop the gate entirely, and all migration will need the alter or add column and so on hence forth, the old schema should nt be touched." — schema model moved to additive migrations; gate removed; `SCHEMA_CONVENTIONS.md` §Migration Model rewritten; the 015 inline edit reverted; `failure_detail` lands as slot 032. Follow-up owed: the orly-side rule set (`dispatch/write_sql.md` §Pre-v2.0.0, AGENTS.md Schema Guard prose) still describes the teardown model and needs an `edit_rules`-flow sync. Jul 22, 2026: Indy confirmed Full scope (chat + table + backend cause) and the design bracket — `/design-shotgun` before implementation, `/design-review` after. Jul 22, 2026: variant board (`docs/design/fleet-events-board.html`) — Indy picked **variant B "Timeline Rail"** over the authoring recommendation (A): activity as inline rail ticks, floating failure card, pill segmented filter, leading Runs count column in the Events table. Jul 22, 2026: Indy's chronology question answered and pinned in the variant page's sequence-behaviour demo — the rail is inline treatment in ONE chronological column; groups are consecutive-only and break on any success or conversation row (already pinned by §4 Dimension 4.1 — no spec change needed).
- **Harness note (Jul 22, 2026)** — raw `zig build test` reports `agentsfleetd-tests` failed under the build-runner's `--listen` protocol while the same binary passes directly (including with the failing run's seed) and the canonical `make test-unit-agentsfleetd` lane is green. Off-diff quirk, surfaced for follow-up; verification stands on the make lanes per `docs/VERIFY_TIERS.md`.
- **Metrics review** — no analytics/funnel playbook update required: no product events added, renamed, or removed (see Metrics & Observability).
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close)); `/design-shotgun` variant pick and `/design-review` findings land here.
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
  > Indy (2026-07-22 14:05): "go (I dont need the All, Conversation, Activity option) we just show all for now." — context: §6 chat filter deferred at the design lock; the thread always shows all rows; reactivate on usage evidence after §3–§5 ship.
- **D4 (Jul 22, 2026, scope)** — asked whether the three §1-surfaced refactor findings should be their own milestone (authoring recommendation) or land here: > Indy: "I want all the finding refactor to be fixed in this PR." — §8 added; blast-radius grep first (8 production + 6 test files touch `exit_ok`; 5 touch the failure fields).
