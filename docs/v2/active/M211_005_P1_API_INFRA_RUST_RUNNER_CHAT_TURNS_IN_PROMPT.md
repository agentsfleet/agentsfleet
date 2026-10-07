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

# M211_005: A chat follow-up reaches the model with the thread's recent turns, and the prefix it repeats is read from the provider's cache

**Prototype:** v2.0.0
**Milestone:** M211
**Workstream:** 005
**Date:** Oct 05, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — a fleet answers every chat message as if it were the first: the model never sees the question before it, or its own answer
**Categories:** API, INFRA
**Batch:** B2 — folds into M211_002 at that stream's CHORE(open); its Sections run after M211_004's
**Branch:** feat/m211-nested-loops-and-chat-continuity
**Folded-into:** `M211_002`
**Baseline revision:** bb007001545cb97f4dc27c9325235a6a0ebb4fb9
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M211_002 (the nested child run, which starts without the turns) · M213_001 (the Rust runner takes leases; `rustd/crates/agentsfleet_runner/src/main.rs:184-185` refuses them until then, and the Zig runner that serves them meanwhile drops the new field, `src/runner/daemon/control_plane_client_lease.zig:10-12`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 05, 2026) from a source trace of the lease, prompt and provider paths at `b0138d7b3`, recorded in Discovery
**Canonical architecture:** `docs/architecture/runner_execution.md` §Crates (the provider and scrub paragraph, line 92)

---

## Overview

**Goal (testable):** `test_follow_up_lease_carries_the_previous_turn` — a fleet's second chat message is leased with the first message and the fleet's answer to it, and the runner sends that turn to the model ahead of the new message.
**Problem:** A user asks a fleet in chat "which tests failed?", reads the answer, and asks "fix the second one". The runner builds every prompt from the installed instructions and the current message alone (`rustd/crates/afr_agent/src/prompt.rs:24-59`), so the model never saw the first question or its own answer, and asks which test the user means.
**Solution summary:** For a chat event, the daemon reads the fleet's thread before that event, the same rows the dashboard thread shows, and puts up to eight finished turns on the lease, each text cut and the total capped. The runner opens the conversation with them as user and assistant turns, through the same secret scrub as the current message. Because the prompt now repeats across a conversation's leases, the Messages wire marks the tools and the system prompt for caching beside its moving breakpoint, and the Responses wire sends the fleet id as its cache key.

## PR Intent & comprehension handshake

- **PR title (eventual):** folded into M211_001's Pull Request
- **Intent (one sentence):** A chat follow-up is answered with the conversation in view, and the part of the prompt it repeats is read from the provider's cache.
- **Handshake** (PLAN, Oct 07, 2026) — restated: a follow-up chat message reaches the model with the conversation's last few exchanges in front of it, scrubbed like the message itself, and the part of the request that repeats from lease to lease is marked so the provider serves it from cache. Matches the Intent. `ASSUMPTIONS I'M MAKING:` (1) The turns are read through `History::thread_page` (`afd_events/src/history/mod.rs:184`) behind a `Thread` trait, so fail-open is tested without a database. (2) `LeasePayload.history` decodes with a default and always serializes, `[]` when empty, by `afd_wire`'s no-`skip_serializing_if` rule; the Zig runner ignores the key (`control_plane_client_lease.zig:12`). (3) rig 0.43.0 provides both Messages switches (`anthropic/wire.rs:359` and `:378`), so no `cache_control` is written by hand; the Responses key rides rig's `additional_params`. (4) The caps are the stated defaults: eight turns, 65,536 bytes, 16,384 bytes a text, chat events only. (5) The runner's prompt (`afr_agent/src/prompt.rs:23-30`) and the conversation's first message are where the turns join; the system prompt is unchanged. (6) This spec executes before M211_004: it needs neither the kernel lane nor a schema change. Production effect waits on M213_001, as the Depends line says. **Quality ceiling:** a server-side conversation store would avoid re-sending turns at all; the lease carries them because the daemon already holds the thread and the runner holds no state between leases. **Surface checklist:** OpenAPI yes (`LeasePayload.history`; regenerate) · the product CLI no · user docs: the chat page says a fleet sees its recent turns, docs repo branch · release/version at close · schema no · spec vs rules: none found.

## Implementing agent — read these first

1. `rustd/crates/afd_fleet/src/lease/answer.rs` — `render`, where the issued lease is serialized; the turns join it here.
2. `rustd/crates/afd_events/src/history/mod.rs` — `History::thread_page` (line 183), the reader the dashboard thread uses; the turns reuse it rather than a new statement.
3. `rustd/crates/afr_agent/src/prompt.rs` and `rustd/crates/afr_agent/src/loop.rs` (lines 107-130) — the prompt and the conversation's first message.
4. `rustd/crates/afr_providers/src/wire.rs` (lines 36-45) and `rustd/crates/afr_providers/src/request.rs` (lines 31-50) — the Messages caching switch and the request rig sends.
5. https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching — breakpoints, the lookback before a breakpoint, and the five-minute default lifetime.
6. https://platform.openai.com/docs/guides/prompt-caching — automatic prefix caching and `prompt_cache_key`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_wire/src/lease.rs` | EDIT | `Turn`, `LeasePayload.history` decoded with a default, the three caps and the two fixed answers |
| `rustd/crates/afd_wire/src/event.rs`, `rustd/crates/afd_wire/src/event/message.rs` | EDIT / CREATE | `message_of`, the one reading of an event's message, used by the daemon for turns and the runner for the current message |
| `rustd/crates/afd_fleet/src/lease/history.rs`, `rustd/crates/afd_fleet/src/lease/history_tests.rs`, `rustd/crates/afd_fleet/src/lease/mod.rs`, `rustd/crates/afd_fleet/Cargo.toml` | CREATE / EDIT | The `Thread` trait, the read before the event, the finished-row filter, the cuts, fail open, the metric calls |
| `rustd/crates/afd_fleet/src/lease/answer.rs`, `rustd/crates/afd_fleet/src/lease/deliver.rs`, `rustd/crates/afd_fleet/src/lease/pull.rs` | EDIT | `issue_ready` reads the turns for a chat event and `render` puts them on the lease; `Plane` holds the thread as `Arc<dyn Thread>` |
| `rustd/crates/agentsfleetd/src/plane.rs`, `rustd/crates/afd_bench/src/lane/lease/drain/stage.rs`, `rustd/crates/afd_fleet/src/lease/test_dead.rs`, `rustd/crates/afd_fleet/tests/support/fleet_report_seed.rs` | EDIT | Every `Plane` gets a `History`-backed thread |
| `rustd/crates/afd_wire/tests/validation_lease.rs`, `rustd/crates/afd_fleet/tests/fleet_suite.rs`, `public/openapi.json` | EDIT | The wire default proof, the suite registration, the regenerated document |
| `rustd/crates/afr_agent/src/prompt.rs`, `rustd/crates/afr_agent/src/loop.rs`, `rustd/crates/afr_agent/src/loop/history_tests.rs`, `rustd/crates/afr_agent/src/nested/run_tests.rs` | EDIT / CREATE | The turns lead the conversation, scrubbed; the message comes from `message_of` |
| `rustd/crates/afr_agent/src/spans.rs` | EDIT | The `chat` span records cache read and cache write tokens |
| `rustd/crates/afr_providers/src/provider.rs`, `rustd/crates/afr_providers/src/request.rs`, `rustd/crates/afr_providers/src/wire.rs`, `rustd/crates/afr_providers/src/turn.rs` | EDIT | Prompt caching markers on Messages; the cache key on Responses; cache-written tokens kept for the span |
| `rustd/crates/afd_observability/src/semconv.rs`, `rustd/crates/afd_observability/src/metrics/declared/fleet.rs`, `rustd/crates/afd_observability/src/producers/fleet.rs`, `rustd/crates/afd_observability/src/producers/fleet/history.rs`, `rustd/crates/afd_observability/src/metrics/label/fleet.rs`, `rustd/crates/afd_observability/src/metrics/label/tests.rs`, `docs/metrics.census.tsv` | EDIT / CREATE | Two span attributes; three daemon families with their producer |
| `rustd/crates/afd_fleet/tests/integration_lease_history.rs`, `rustd/crates/afr_providers/tests/providers/turns.rs`, `rustd/crates/afr_providers/tests/providers/caching.rs`, `rustd/crates/afr_providers/tests/providers.rs`, `rustd/crates/afr_providers/src/request/tests.rs` | CREATE / EDIT |
| `rustd/crates/afr_agent/src/loop/shared.rs`, `rustd/crates/afr_agent/src/loop/model_turn.rs`, `rustd/crates/afr_agent/src/engine.rs`, `rustd/crates/afr_agent/src/fixture.rs`, `rustd/crates/afr_agent/src/fixture/model.rs`, `rustd/crates/afr_supervisor/src/{renew/tests.rs,report/tests.rs,test_support.rs,lease_telemetry_tests.rs}`, `rustd/crates/agentsfleetd/tests/support/fake_model.rs` | EDIT | The fleet id reaches the request as its cache key; `Usage.cache_written` in every literal | The integration proofs and the request-body proofs |
| `docs/architecture/runner_execution.md` | EDIT | §Crates: a chat lease carries the thread's recent turns, and the stable prefix is marked for caching |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, UFS (the three caps and the two fixed answers are named constants in `afd_wire::lease`), PRI (earlier turns are tenant text: they stay in user and assistant messages, never the system prompt), PSR (`message_of` parses with `serde_json`), KYS and NSQ (the read reuses the thread's composite keyset statement), LOG, ERR-RS, MSID, ARCH, FLL, TST-NAM, ITF.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — the read's failure keeps its cause in the log and never fails the lease.
- `docs/LOGGING_STANDARD.md` — `lease_history_unavailable` carries ids and `error_code`, never message text.
- `dispatch/name_architecture.md` — the prompt gains the thread's turns; `runner_execution.md` §Crates says so in the docs commit with this spec.
- Indy's Rust bar (Oct 04 and Oct 05, 2026): the read sits behind a `Thread` trait that `History` implements, so fail open is tested with a failing implementation and no mocked database; the turns borrow from the read rows as `Cow`, the way `LeasePayload` already borrows; the runner builds each message once, from the scrub's output; `message_of` is one function in `afd_wire` that both sides call; the cut reuses `truncate` (`rustd/crates/afd_fleet/src/lease/verdict.rs:130`); the metrics are `afd_observability` declared families with a producer; nothing walks bytes or writes JSON by hand.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — Rust | Caps and fixed answers named once in `afd_wire::lease`; daemon and runner read the same constants |
| LOGGING GATE | yes — one daemon event | `docs/LOGGING_STANDARD.md` fields; `error_code` on the warning |
| MILESTONE-ID GATE | yes | No milestone identifiers in code, tests or comments |
| Architecture consult | yes | The `runner_execution.md` edit lands in the docs commit with this spec (landing a) |
| File & Function Length (≤350/≤50/≤70) | yes | `lease/history.rs` alone owns the read, the filter and the cuts |
| SCHEMA GUARD | no | No schema change: the read reuses the thread statement and its index (`schema/800_fleet_events.sql:65-66`) |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_events/src/history/mod.rs` `thread_page` — the dashboard thread's reader; the model reads the rows the user sees.
- **Reference:** Codex `codex-rs/core/src/client.rs:356-392` (`~/Projects/oss/rs/codex`) — one `prompt_cache_key` per conversation, compared request to request. Applied: one key per fleet. Diverged: Codex keeps the whole conversation and compacts it; a fleet keeps a window and leaves durable facts to memory.
- **Reference:** rig-core 0.43.0 `Messages::with_prompt_caching` beside `with_automatic_caching` — markers on the last tool and the system prompt while the provider moves the conversation's breakpoint; used as shipped, with no `cache_control` written by hand.
- **Reference:** the span names `gen_ai.usage.cache_read.input_tokens` and `gen_ai.usage.cache_creation.input_tokens`, as OpenTelemetry trace stores read them (`~/Projects/oss/go/signoz-otel-collector/cmd/signozschemamigrator/schema_migrator/traces_migrations.go`). Diverged from Codex's `cache_write` spelling (`codex-rs/core/src/session/turn.rs:2627-2628`).

## Sections (implementation slices)

### §1 — A chat lease carries the thread's recent turns — DONE

When the claimed event is `chat`, `pull` reads the fleet's thread before the event through `Thread`, which `History::thread_page` implements with the event's `(created_at, event_id)` as the cursor and one row more than `HISTORY_TURNS_MAX`, so a cut is known. Rows that ended `processed` or `fleet_error` become turns, oldest first. A turn's message is `message_of(request_json)`. Its answer is `response_text`, `[no reply]` when a processed row has none, and `[the run failed: <failure_label>]` for `fleet_error`. Each text is cut to `TURN_TEXT_BYTES_MAX` on a character boundary, and the oldest turns drop until the total fits `HISTORY_BYTES_MAX`. Any other event type carries an empty list. A read that fails issues the lease with an empty list, logs `lease_history_unavailable` with its `error_code`, and counts it, because a follow-up answered without context beats one refused. **Implementation default:** eight turns, 65,536 bytes, 16,384 bytes per text, and chat events only, because a webhook delivery is self-contained and would otherwise carry up to 64 KiB of unrelated runs. Indy may change any of them.

- **Dimension 1.1** DONE — A chat event's lease carries the finished turns before it, oldest first → Test `test_follow_up_lease_carries_the_previous_turn`
- **Dimension 1.2** DONE — Only finished rows become turns; a failure answers with its label; a silent run answers `[no reply]` → Test `test_history_keeps_finished_turns_only`
- **Dimension 1.3** DONE — At most eight turns, each text cut on a character boundary, the oldest dropped until the bytes fit → Test `test_history_caps_turns_and_bytes`
- **Dimension 1.4** DONE — Webhook, cron and continuation leases carry no turns → Test `test_non_chat_lease_carries_no_history`
- **Dimension 1.5** DONE — Another fleet's rows never appear → Test `test_history_stays_in_its_fleet`
- **Dimension 1.6** DONE — A failed read issues the lease with no turns, logged and counted → Test `test_history_read_failure_fails_open`
- **Dimension 1.7** DONE — A lease without the field decodes with no turns → Test `test_lease_history_defaults_empty`
- **Dimension 1.8** DONE — The bytes histogram observes every chat lease, and each cap that cut counts once → Test `test_history_metrics_recorded`

### §2 — The model reads them as the conversation — DONE

`Prompt` gains the lease's turns. The harness opens the conversation with each turn as a user message and an assistant message with no calls, then the current message (`rustd/crates/afr_agent/src/loop.rs:128-129`); every text passes the lease's `Scrub` first, as the current message does. The current message and a turn's message both come from `message_of`, so a message reads the same as a turn as it read when it was current. The system prompt is unchanged by the turns. A nested child run starts from its task alone. `Budget::evict` rewrites tool results only (`rustd/crates/afr_agent/src/context.rs:48-61`), so turns are never evicted.

- **Dimension 2.1** DONE — The turns lead the conversation, ahead of the current message, and leave the system prompt unchanged → Test `test_history_leads_the_conversation`
- **Dimension 2.2** DONE — Every turn passes the lease's secret scrub → Test `test_history_is_scrubbed`
- **Dimension 2.3** DONE — A message reads the same as a turn as it read when it was current → Test `test_history_message_matches_its_first_reading`
- **Dimension 2.4** DONE — A nested child run's first request holds its task alone → Test `test_child_run_carries_no_history`
- **Dimension 2.5** DONE — Eviction leaves every turn intact → Test `test_eviction_leaves_history_intact`

### §3 — The prefix a conversation repeats is cached — DONE

The Messages wire adds rig's `with_prompt_caching()` beside `with_automatic_caching()` (`rustd/crates/afr_providers/src/wire.rs:40-44`): markers on the last tool and the system prompt, plus the provider's moving breakpoint on the conversation, all at the five-minute default. A follow-up within five minutes then reads the tools, the system prompt and every turn before its own message from the cache, and a webhook lease reads the tools and the system prompt. The Responses wire sends `prompt_cache_key` set to the fleet id through rig's `additional_params`, and a nested child run sends its fleet's key; the Chat wire sends none, because its gateways refuse fields they do not know. The trusted repair context stays in the system prompt. It names the event (`rustd/crates/afd_gate/src/policy/repair.rs:66`), so a write-bound lease writes its system prompt and turns to the cache each event and still reads its tools. The `chat` span records cache read and cache write tokens. Billing is unchanged, because rig counts writes inside input (`rustd/crates/afr_providers/src/turn.rs:262-272`).

- **Dimension 3.1** DONE — A Messages request marks the last tool and the system prompt, keeps the top-level breakpoint, and sets no lifetime → Test `test_messages_cache_marks_the_static_prefix`
- **Dimension 3.2** DONE — `prompt_cache_key` rides the Responses wire only → Test `test_cache_key_rides_responses_only`
- **Dimension 3.3** DONE — A follow-up's request equals the previous lease's through that lease's message → Test `test_history_prefix_is_byte_identical_across_leases`
- **Dimension 3.4** DONE — The `chat` span carries the turn's cache read and cache write tokens → Test `test_cache_tokens_recorded_on_the_chat_span`
- **Dimension 3.5** DONE — A write-bound lease keeps its repair context in the system prompt and out of every message → Test `test_history_leaves_the_repair_context_in_the_system_prompt`

## Interfaces

```
afd_wire::lease::Turn<'a>            message: Cow<str> · answer: Cow<str>
afd_wire::lease::LeasePayload        + history: Vec<Turn>   oldest first; empty unless the event is chat; #[serde(default)]
afd_wire::lease                      HISTORY_TURNS_MAX = 8 · HISTORY_BYTES_MAX = 65_536 · TURN_TEXT_BYTES_MAX = 16_384
afd_wire::lease                      NO_REPLY = "[no reply]" · RUN_FAILED_PREFIX = "[the run failed: "
afd_wire::event::message_of(&str)    -> Cow<str>: the request's `message` string, else the whole request
afd_fleet::lease::history::Thread    rows before an event, newest first; afd_events::history::History implements it
afr_providers::provider::Request     + cache_key: Option<&str>   the fleet id; sent on the Responses wire only
afr_providers::provider::Usage       + cache_written: u64        part of `input`; recorded on the span, billed as today
afd_observability::semconv           ATTR_USAGE_CACHE_READ_INPUT_TOKENS · ATTR_USAGE_CACHE_CREATION_INPUT_TOKENS
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Read fails | Postgres unavailable, or a row this build cannot read | Lease issued with no turns; `lease_history_unavailable` warning with `error_code`; the failure counter; the model answers the message alone, as today |
| Long turn | An answer over 16,384 bytes | Cut on a character boundary and counted `text`; the model sees the cut text |
| Long conversation | More than eight finished turns, or more than 65,536 bytes | Oldest dropped and counted by cap; the shifted window writes that lease's turns to the cache, and its tools and system prompt stay a read |
| Zig runner serves the lease | M213_001 has not landed | The field is ignored (`src/runner/daemon/control_plane_client_lease.zig:10-12`); the prompt is today's |
| Older daemon | A lease without the field | Decodes empty and the prompt is today's (`test_lease_history_defaults_empty`) |
| Injected text in an earlier turn | A webhook digest or an earlier answer carrying instructions | Stays in a user or assistant message, never the system prompt; tools, egress and writes stay governed by `ExecutionPolicy`, as for the current message |
| Secret in an earlier turn | A stored answer that echoed a credential | The lease's scrub masks it before send (`test_history_is_scrubbed`) |
| Prompt below the cache minimum | A short fleet prompt | The provider skips caching and the request succeeds uncached |
| Write-bound fleet | The repair context names the event | Its system prompt and turns are written to the cache each event; its tools still read |
| Two messages at once | A second steer before the first finishes | The affinity slot admits one holder, so the second is leased after the first's report and reads it as a finished turn (`test_follow_up_lease_carries_the_previous_turn`) |

## Invariants

1. A lease carries only its own fleet's turns — the read is scoped by workspace and fleet; `test_history_stays_in_its_fleet`.
2. Only a chat event carries turns — `pull` reads them for `EventType::Chat` alone; `test_non_chat_lease_carries_no_history`.
3. No turn reaches a provider unscrubbed — the harness builds every turn's messages from `Scrub::clean`; `test_history_is_scrubbed`.
4. Tenant text never enters the system prompt — turns become user and assistant messages only; `test_history_leads_the_conversation`.
5. A lease's turns fit `HISTORY_BYTES_MAX` — the cut runs before render; `test_history_caps_turns_and_bytes`.
6. A failed read never refuses a lease — `test_history_read_failure_fails_open`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `agentsfleet_lease_history_bytes` (daemon histogram, `afd_observability` declared + producer) | ops | A chat lease is issued | none | no ids or text in labels | `test_history_metrics_recorded` |
| `agentsfleet_lease_history_cuts_total` (daemon counter) | ops | A cap cuts a chat lease's turns | `cap`: `turns`, `bytes` or `text` | closed label set | `test_history_metrics_recorded` |
| `agentsfleet_lease_history_read_failures_total` (daemon counter) | ops | The thread read fails | none | none to guard | `test_history_read_failure_fails_open` |
| `lease_history_unavailable` (daemon log, warn) | ops | The thread read fails | lease id, fleet id, `error_code` | no message or answer text | `test_history_read_failure_fails_open` |
| `gen_ai.usage.cache_read.input_tokens`, `gen_ai.usage.cache_creation.input_tokens` on the `chat` span | ops | Every model turn | token counts | counts only | `test_cache_tokens_recorded_on_the_chat_span` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_follow_up_lease_carries_the_previous_turn` | chat `m1` leased and reported processed with answer `a1`; chat `m2` queued → `m2`'s lease has `history = [{message: "m1", answer: "a1"}]` |
| 1.2 | unit | `test_history_keeps_finished_turns_only` | rows processed `a`, fleet_error `timeout`, processed with no text, and a queued row → turns `a`, `[the run failed: timeout]`, `[no reply]`; no queued row |
| 1.3 | unit | `test_history_caps_turns_and_bytes` | nine finished rows → eight turns, oldest first; a 20,000-byte answer ending in a multi-byte character → at most 16,384 bytes, on a boundary; turns totalling 70,000 bytes → oldest dropped until at most 65,536 |
| 1.4 | integration | `test_non_chat_lease_carries_no_history` | a webhook event after a finished chat turn → its lease has `history = []` |
| 1.5 | integration | `test_history_stays_in_its_fleet` | fleets `f1` and `f2` each with a finished turn → `f1`'s follow-up carries `f1`'s turn only |
| 1.6 | unit | `test_history_read_failure_fails_open` | a `Thread` that errors → the lease renders with `history = []`, `lease_history_unavailable` logged with its `error_code`, the failure counter at 1 |
| 1.7 | unit | `test_lease_history_defaults_empty` | lease JSON without `history` → decodes with an empty list; with it → round-trips unchanged |
| 1.8 | unit | `test_history_metrics_recorded` | a read cut by turns and by text → the histogram observes the total bytes, `cuts_total` reads 1 for `turns` and 1 for `text` |
| 2.1 | unit | `test_history_leads_the_conversation` | turn `(m1, a1)` and message `m2` → messages `User m1`, `Assistant a1` with no calls, `User m2`; instructions equal those of the same lease without turns |
| 2.2 | unit | `test_history_is_scrubbed` | a turn whose answer holds the lease's secret value → the request carries the masked form |
| 2.3 | unit | `test_history_message_matches_its_first_reading` | request `{"message":"m1"}` read as lease A's current message and as lease B's turn → identical strings |
| 2.4 | unit | `test_child_run_carries_no_history` | a lease with turns spawns a child run → the child's first request holds its task as the only message |
| 2.5 | unit | `test_eviction_leaves_history_intact` | a run past the tool window → older tool results evicted, every turn's messages unchanged |
| 3.1 | unit | `test_messages_cache_marks_the_static_prefix` | a Messages turn on the provider fake → `cache_control` on the last tool and on the system block, a top-level `cache_control`, no `ttl` |
| 3.2 | unit | `test_cache_key_rides_responses_only` | one request with cache key `f1` on each wire → the Responses body has `prompt_cache_key = "f1"`; the Messages and Chat bodies have none |
| 3.3 | unit | `test_history_prefix_is_byte_identical_across_leases` | lease A with message `m1`; lease B with turn `(m1, a1)` and message `m2` → B's first request equals A's in instructions, tools and the first message |
| 3.4 | unit | `test_cache_tokens_recorded_on_the_chat_span` | provider usage with 900 cache-read and 100 cache-write tokens → the `chat` span carries 900 and 100 |
| 3.5 | unit | `test_history_leaves_the_repair_context_in_the_system_prompt` | a write-bound lease with turns → the repair context is in the instructions and in no message |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A chat follow-up's lease carries the previous turn; other events and fleets carry none (§1) | `make test-integration-rustd 2>&1 \| grep -c -E "test_(follow_up_lease_carries_the_previous_turn\|non_chat_lease_carries_no_history\|history_stays_in_its_fleet) \.\.\. ok"` | 3 | P0 | |
| R2 | The daemon keeps finished rows, cuts to the caps, fails open and measures it; the wire decodes leniently (§1) | `cargo test --manifest-path rustd/Cargo.toml -p afd_fleet -p afd_wire history 2>&1 \| grep -c -E "test_(history_keeps_finished_turns_only\|history_caps_turns_and_bytes\|history_read_failure_fails_open\|history_metrics_recorded\|lease_history_defaults_empty) \.\.\. ok"` | 5 | P0 | |
| R3 | The runner leads with the turns, scrubbed, read as first read, and a child run starts clean (§2, §3) | `cargo test --manifest-path rustd/Cargo.toml -p afr_agent history 2>&1 \| grep -c -E "test_(history_leads_the_conversation\|history_is_scrubbed\|history_message_matches_its_first_reading\|child_run_carries_no_history\|eviction_leaves_history_intact\|history_prefix_is_byte_identical_across_leases\|history_leaves_the_repair_context_in_the_system_prompt) \.\.\. ok"` | 7 | P0 | |
| R4 | The stable prefix is marked, the key rides Responses, and the span counts cache tokens (§3) | `cargo test --manifest-path rustd/Cargo.toml -p afr_providers -p afr_agent cache 2>&1 \| grep -c -E "test_(messages_cache_marks_the_static_prefix\|cache_key_rides_responses_only\|cache_tokens_recorded_on_the_chat_span) \.\.\. ok"` | 3 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears above verbatim. **Grading protocol (VERIFY):** Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results in Session Notes.

## Dead Code Sweep

N/A — no files deleted. `Prompt::new`'s message parse (`rustd/crates/afr_agent/src/prompt.rs:36-45`) moves to `afd_wire::event::message_of` in the same commit, so no second copy remains.

## Out of Scope

- The one-hour cache lifetime. Anthropic bills a one-hour cache write at twice the input price, and the daemon bills every write at the input rate because rig counts writes inside input (`rustd/crates/afr_providers/src/turn.rs:262-272`); the longer lifetime waits for a spec that prices cache writes.
- OpenAI's `prompt_cache_retention`: only some models accept it, and a model that does not refuses the request.
- Summarising a long conversation. Turns past the window drop; facts a fleet must keep belong in memory, which its tools pull.
- Turns on webhook, cron and continuation leases.
- A conversation identifier: the fleet is the conversation (`rustd/crates/afd_events/src/history/statement.rs:258-270`).
- Moving the trusted repair context out of the system prompt to keep a write-bound fleet's prefix stable.

---

## Product Clarity (authoring record)

1. **Successful user moment** — The user asks "which tests failed?", then "fix the second one", and the fleet fixes the second failing test without asking which one.
2. **Preserved user behaviour** — Webhook, cron and continuation runs see exactly today's prompt; a fleet's thread, memory and billing are unchanged.
3. **Optimal-way check** — The unconstrained shape keeps the whole conversation and compacts it as it grows, as Codex does. A fleet's conversation is long-lived and shared by every trigger, so a bounded window plus memory keeps each lease's cost bounded.
4. **Rebuild-vs-iterate** — Iterate: one read through the thread reader, one wire field, the conversation's first messages and two cache switches (Decomposition).
5. **What we build** — The read and its caps, the wire field, the leading turns, the cache markers and key, two span attributes, three daemon families, one architecture edit.
6. **What we do NOT build** — Compaction, the one-hour lifetime, turns for other event types, conversation identifiers (Out of Scope).
7. **Fit with existing features** — Compounds with M211_004's held sandbox: a follow-up keeps its files and now its words; memory keeps durable facts past the window.
8. **Surface order** — No new surface; the change shows in chat answers.
9. **Dashboard restraint** — No indicator in the user interface (UI); the thread already shows what the model sees.
10. **Confused-user next step** — The chat page of the published docs says how many earlier messages a fleet sees, so "it forgot what I said much earlier" has its answer.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections by authority. The daemon owns what a lease carries (§1), the runner owns the conversation (§2), and the provider layer owns the cache (§3). §1 and §2 alone answer follow-ups; §3 makes the repeat cheap.
- **Alternatives considered:** the runner reading the thread itself (rejected: the runner links no datastore, `docs/architecture/runner_execution.md:90`, and a verb per lease adds a round trip to every chat start); the session checkpoint (rejected: one answer cut to 2,048 bytes, `rustd/crates/afd_fleet/src/lease/sql/session.rs:17`, and no messages); turns on every event type (rejected as the default: each delivery to a pull request reviewer would carry up to 64 KiB of unrelated reviews); moving per-event trusted text after the turns to keep write-bound prefixes stable (rejected: trusted policy would sit in a user message).
- **Patch-vs-refactor verdict:** this is a **patch** because the lease, the loop and the providers keep their shape; the turns ride ahead of the current message.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 05, 2026) chose "Add M211_005, cached (recommended)". Source trace (Oct 05, 2026, at `b0138d7b3`), each claim read from source: the prompt holds the instructions and the current message only (`afr_agent/src/prompt.rs:24-59`), and the conversation starts with that one message (`afr_agent/src/loop.rs:128-129`); `core.fleet_events` keeps each run's `request_json` and whole `response_text` (`schema/800_fleet_events.sql:30-57`), written at report (`afd_fleet/src/lease/finalize.rs:99-112`); the thread reader with bodies exists (`afd_events/src/history/mod.rs:183-214`); the Messages wire caches automatically at the five-minute default (`afr_providers/src/wire.rs:40-44`); rig counts cache writes inside input (`afr_providers/src/turn.rs:262-272`); the repair branch names the event (`afd_gate/src/policy/repair.rs:66`); the Zig runner ignores unknown lease fields (`src/runner/daemon/control_plane_client_lease.zig:10-12`); a chat message is capped at 8,192 bytes (`afd_wire/src/event/steer.rs:111`), under `TURN_TEXT_BYTES_MAX`. Three changes from the design drafted before this spec, each an implementation default Indy may change: only chat events carry turns; the five-minute lifetime stays, because a one-hour write costs twice the input price and the daemon bills writes at the input rate; the trusted repair context stays in the system prompt. Architecture consult: `ARCH: grounded in runner_execution.md:92 | proposal: a chat lease carries the thread's recent turns, sent ahead of the message, with the stable prefix marked for caching | status: extends | landing: a` — the doc edit lands in the docs commit that lands this spec.
- **Metrics review** — Three daemon families, one daemon log event and two span attributes; no analytics or funnel playbook change, because no product event changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
