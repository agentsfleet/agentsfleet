<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M259_002: Every run's prompt tells the model the truth about its tools, its speaker and its surroundings, and a person can show a fleet a picture

**Prototype:** v2.0.0

**Milestone:** M259

**Workstream:** 002

**Date:** Nov 26, 2026

**Status:** IN_PROGRESS

**Progress:** 26 of 30 dimensions complete. The image message, model refusal, API bounds, dashboard and command-line paths passed. The hosted Messages model identified the attached image's colour. Dimensions 3.4 and 5.2–5.4 depend on M258 and the M259_001 Firecracker spike. [Implementation PR](https://github.com/agentsfleet/agentsfleet/pull/874) · [Hosted image evidence](https://github.com/agentsfleet/agentsfleet/actions/runs/46129180070/artifacts/15257104912)

**Priority:** P1, every run is told about tools it may not hold, and a webhook body reads as the person's own message

**Categories:** API, CLI, DOCS, INFRA, UI

**Batch:** B2, opened from M259_001 after Dimensions 1.7, 9.1, 9.3 and 9.4 passed. Both workstreams share PR #874.

**Depends on:** The starting branch includes M259_001 search and fetch. The browser and virtual-machine capability work in §§3 and 5 depends on the merged M258 engine and the M259_001 Firecracker spike.

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 26, 2026) from the Nov 26 prompts research, its skeptic review and the pending M258_001. Each reference was re-read at `679002666`.

**Canonical architecture:** `docs/architecture/run_flow.md` (the system prompt), `docs/architecture/runner_execution.md` §Tool catalog and `docs/architecture/thread_model.md` (a turn's input)

---

## Overview

**Goal (testable):** Each loop's prompt names only its tools, the turn's sender, the date, network and workspace, and a message's images reach models that read images.

**Terms.** Rust paths that start with a crate name are under `rustd/crates/<layer>/`; `guffbolt/` and `guffbolt-ai/` are under `rustd/`. F is `/v1/workspaces/{workspace_id}/fleets/{fleet_id}` and W is `/v1/workspaces/{workspace_id}`. Line references are at `679002666`.

A **loop** is the root agent or one child it spawned. **Guidance** is the prompt text a tool contributes when a loop holds it. The **fixed text** is `Prompt.fixed`, the only text a saved summary's digest covers.

**Problem:** Read from source.

- **The prompt names tools the run lacks.** One fixed paragraph names `memory_store`, the cron tools, `trigger_add`, `task_start` and `request_access` to every run (`afr_agent/src/prompt/mod.rs:43-56`). A child given fewer tools reads the root's prompt word for word (`guffbolt/src/agents/seated.rs:82-84`).
- **The model cannot tell who is speaking.** A webhook body, a schedule firing, another fleet's request and the harness's "continue" line are all plain user messages (`guffbolt-ai/src/types/message.rs:54`; `guffbolt/src/drive/mod.rs:290`). Yet the prompt gates side effects on "the person's newest message" (`prompt/mod.rs:46-51`).
- **No date, network or environment.** The prompt carries none, though the payload holds the network setting (`af_wire/src/policy/mod.rs:35`) and the turn its `created_at` (`af_wire/src/turn/mod.rs:114`).
- **Every judged turn breaks the cached prefix.** The task block, after pinned memory in the same system block, embeds the newest turn's id and length (`prompt/mod.rs:236-243`).
- **Offered tools that always refuse.** `image` is offered on the Chat wire (`guffbolt-ai/src/wire/route/mod.rs:43-48`), where every call answers `image_input_unavailable` (`guffbolt/src/tools/files/image.rs:161-163`).
- **Thin descriptions.** 53 tool descriptions hold 8,993 characters, median 146, and none gives an example call.
- **A person cannot show a fleet a picture.** A run's input is the text of `message` alone (`af_wire/src/turn/message.rs:14-24`). The composer's preview upload never enters the durable turn (`ui/packages/app/components/domain/conversation/composer/useFleetUploadPreviews.ts:24`).

**Solution summary:**

- **Guidance from the tools a loop holds**, root and child, with the digest kept on text no run fact changes.
- **A source line on each turn** that a person did not type, and one tag for harness text.
- **An environment block** with the date, the network setting, the working directory and what the engine provides.
- **One stable prefix.** Per-turn facts leave the system block, and blocks are ordered by how often they change.
- **Only tools that can work are offered,** and every withheld capability is named in one line.
- **Descriptions in one shape,** each with its sibling and an example a test parses, plus three harness-fact sentences.
- **The two approved tool fixes**, and **images in a message** through the API, the web app and the CLI.

**What is unverified.** Whether a daemon verb refuses a side effect by the turn's sender is unread. Whether any wording makes a model choose better needs a model evaluation, which this spec does not run. Which field states that a run has a browser is `[?]`: M258_001's `Capabilities` hold none.

**Design flow:**

```
 today: one string, one system block               after
 ┌───────────────────────────────────┐            ┌──────────────────────────────────────────┐
 │ fixed: who the fleet is           │ digest     │ who the fleet is · harness facts (§7)     │ digest
 │  + 5 tool sentences, always       │            ├──────────────────────────────────────────┤
 ├───────────────────────────────────┤            │ guidance, one per tool this loop holds    │ per tool set
 │ ## Pinned memory                  │            │ ## Environment: workspace, network, engine│ per fleet, per engine
 │ ## Trusted repair context         │            │ ## Trusted repair context · ## Workspace  │ per binding
 │ ## Workspace                      │            │ ## Pinned memory                          │ when a person asks
 │ ## Durable tasks: turn id, bytes  │ every turn │ Today is Thursday, November 26, 2026 (UTC)│ daily
 └───────────────────────────────────┘            │ ## Durable tasks: unfinished only         │ when a task changes
                                                  └──────────────────────────────────────────┘
 turn message: a source line when a person did not type it ("a webhook delivery from …")
 POST F/messages {message, images?} ─► the turn keeps them ─► the run's first user message carries them
```

## Rulings

**Ruled by Indy on Nov 26, 2026** (quotes in Discovery).

| # | Ruling |
|---|---|
| Scope | This milestone improves guffbolt's tools "with the system prompts and such proposed like opencode". |
| Tool fixes | `tool_search` answers 8 when no `limit` is named; `grep` gains `context` and a 500-character line cut (approved Nov 22, 2026). |
| Images | A person attaches images through the API, and both the web app and the CLI send them. |

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(prompt): guidance from held tools, speaker and environment lines, one cached prefix; images in messages
- **Intent (one sentence):** A fleet's model hears only true things about its tools, its speaker and its run, and sees what a person shows it.
- **Authoring handshake:** Indy wants guffbolt's tools and prompts improved the way opencode's are, in the web milestone. ASSUMPTIONS I'M MAKING:
  1. The engine milestone is M258 and this is M259; this numbering was asked and not yet ruled.
  2. M258_001's `EgressFacts` state outbound network, and M259_001's browser source states whether a run has a browser.
  3. Items 1 to 7 of the research ship on deterministic tests; items 8 to 10 wait for a measurement.
  4. Nothing is backward compatible, and each fleet's saved summary is ignored once when the fixed text changes.
- **Implementer handshake, Nov 28.** Build guidance from each loop's actual offered tools, preserve the fixed prompt digest across tool-set changes, and carry sender and image capabilities through their existing boundaries. Start with deterministic prompt and tool cases on the web-tools base. Report VM and browser capabilities only once their real sources land.

## Implementing agent — read these first

1. `rustd/crates/runner/afr_agent/src/prompt/mod.rs` — `Prompt::new` (`:73-90`), the builder every prompt Section changes.
2. `rustd/crates/runner/afr_agent/src/loop/history_tests.rs` — `test_history_prefix_is_byte_identical_across_runs` (`:188`), the cache test §4 extends.
3. `rustd/guffbolt/src/tools/mod.rs` — `Handler` (`:131`), where a tool's guidance and examples sit beside its description.
4. `rustd/crates/runner/afr_agent/src/offer/mod.rs` — `names` (`:19-26`), the seam §5 narrows.
5. `rustd/guffbolt-ai/src/wire/route/mod.rs` — `carries_images` (`:43-48`), the wires that take an image.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M259_002_P1_API_CLI_DOCS_INFRA_UI_TRUTHFUL_PROMPTS_AND_IMAGES_IN_MESSAGES.md` | CREATE | this spec; moves to `active/`, then `done/` at close |
| `rustd/crates/runner/afr_agent/src/{prompt,loop,offer,engine,fixture}/**`, `afr_agent/tests/golden/offered_tools.json` | EDIT/CREATE | §1 to §5 and §7; §6's golden file |
| `rustd/crates/e2e/af_e2e/tests/integration_rust_runner_nested.rs` | EDIT | §1.2 and §2.2 verify narrowed guidance and tagged tasks in the delegation journey |
| `rustd/guffbolt/src/{tools/mod.rs,tool/mod.rs,drive/mod.rs,tools/model_text_tests.rs}`, `rustd/guffbolt/README.md` | EDIT | guidance and examples beside each description; the harness tag |
| `rustd/guffbolt/src/tools/{plan.rs,message/**,notify/**,memory/**,web/**,files/**,agents/**,schedule/**}` | EDIT | §6: descriptions in one shape |
| `rustd/guffbolt/src/tools/{search.rs,files/listing/grep.rs}` | EDIT | §8: the two approved fixes |
| `rustd/crates/runner/afr_tools/src/verbs/{tasks,access,triggers}/**` | EDIT | §1 guidance; §4: the span becomes optional |
| `rustd/crates/protocol/af_wire/src/{run/payload.rs,turn/**,protocol/mod.rs}` | EDIT | §2: a history `Turn` gains `event_type` and `actor`; §9: images; `VERSION` moves once |
| `rustd/crates/daemon/afd_fleet/src/run/history.rs` | EDIT | §2: fills those two fields |
| `rustd/guffbolt-ai/src/{types/message.rs,wire/request/**,wire/image.rs}` | EDIT | §9: a user message carries images on the wires that take them |
| `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/**`, `afd_api_wire/src/**`, `afd_turns/src/**` | EDIT | §9: `POST F/messages` and `POST W/fleets` take `images` |
| `rustd/crates/daemon/afd_http/src/route/route_meta_total.rs` | EDIT | if the request shapes change its counts |
| `rustd/crates/foundation/af_core/src/{error_code,problem}/**` | EDIT | §9's two codes, each with its `REGISTRY` row and problem entry |
| `ui/packages/app/components/domain/conversation/composer/**`, `ui/packages/app/lib/**` | EDIT | §9: the composer attaches images |
| `cli/src/commands/fleet/{create,steer}.ts`, `cli/test/**` | EDIT | §9: `--image <path>` |
| `tests/fixtures/fleets/keen-hawk.json`, `tests/fixtures/fleets/README.md`, `ui/packages/app/tests/e2e/acceptance/prime-keen-hawk.spec.ts` | CREATE/EDIT | §9's end-to-end fleet |
| `public/openapi.json` | EDIT | regenerated with `--no-banner` |
| `docs/architecture/{run_flow,runner_execution,thread_model,runner_protocol}.md` | EDIT | the prompt's blocks, the tool rows, a turn's images, the payload's turn fields |
| `~/Projects/docs` message, CLI and tool pages, `changelog.mdx` | EDIT, on that repository's own branch after merge | the published behaviour |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE. **`languages/typescript.md`** — TS-SCHEMA, TS-ERRSTYLE. **`code.md`** — PRI for text that reaches a prompt; NDC and ORP for the old fixed text; CODE-SEAM.
- **`domains/http.md`, `domains/http-responses.md`, `domains/http-surface.md`** — HTTP-GARDE, HTTP-VALID, HTTP-STATUS, HTTP-NEWCODE, HTTP-REGISTRY, HTTP-OPENAPI.
- **`tests.md`** — TEST-NEG, TEST-REDGREEN, TEST-GOLDEN, TCF. **`domains/architecture.md`** — ARCH-WINS, ARCH-LAND.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | fix the code; guffbolt stays standalone |
| `make test-unit-all` | yes | every prompt, tool, app and CLI Dimension |
| `make test-integration-rustd` | yes | §9's daemon Dimensions, against a fake Messages provider |
| OpenAPI drift | yes | regenerate with `--no-banner` |
| `make check-version` | yes | the runner protocol `VERSION` and the published crates move as the gate requires |
| `guffbolt_tool_specs_name_no_host_product` | yes | guffbolt's guidance stays product-neutral; product sentences live in `afr_agent` and `afr_tools` |
| `make acceptance` (live, Linux, on demand) | not in a green PR | `prime-keen-hawk.spec.ts` and the prime set run on a burst machine before the PR opens |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | the prompt splits into one module per block |

## Prior-Art / Reference Implementations

- **Reference:** the history prefix test (`afr_agent/src/loop/history_tests.rs:188`) already proves a byte-identical prefix for an unjudged run. §4 extends it to a judged one.
- **Reference:** guffbolt derives each schema from its argument type (`guffbolt/src/tools/schema.rs`). Guidance and examples sit beside the description the same way, so text and type cannot drift.
- **Reference:** the `image` tool's gate and bounds (`guffbolt/src/tools/files/image.rs:160-201`); §9's images take the same kinds and caps.
- **Divergence:** none from the HTTP standard. The CLI keeps its six practices (SPEC-PRIOR).

## Sections (implementation slices)

Build order: §7 with §1, then §2 to §5; §6 and §8 anywhere; §9 last. §3 and §5 need M259_001's search and browser source.

### §1 — Each loop is told only about the tools it holds

The fixed text splits into the identity sentence, §7's harness facts, and one guidance entry per tool it names today. Each tool declares its guidance beside its description: a `GUIDANCE` constant on guffbolt's `Handler`, empty by default, surfaced through `Describe` (`guffbolt/src/tool/mod.rs:25`). The runner's verbs do the same.

A loop's guidance is rendered from the specs it is offered. `request(window, tools)` already receives them, narrowed for a child (`guffbolt/src/agents/seated.rs:76-84`). The digest stays on the identity and the harness facts (`afr_agent/src/loop/context/mod.rs:58-71`), so a tool-set change keeps a fleet's saved summary.

guffbolt's guidance names no host product, which its text test enforces (`guffbolt/src/tools/model_text_tests.rs:29-30`). Product sentences stay in `afr_agent` and `afr_tools`.

- **Dimension 1.1** — DONE. a run hears only about tools it holds → Test `a_run_is_told_only_about_the_tools_it_holds` (unit, `afr_agent`): one case per guidance entry. Each entry's sentence is present exactly when its tool is offered; an unjudged run hears no `task_start`.
- **Dimension 1.2** — DONE. a child hears only about its own tools → Test `a_child_given_fewer_tools_is_told_about_fewer` (unit, `afr_agent`, the scripted model in `fixture/model.rs`): a child given `shell` alone. Its instructions carry no `memory_store` guidance, and the root's still do.
- **Dimension 1.3** — DONE. a tool-set change keeps the summary → Test `a_tool_set_change_keeps_the_saved_summary` (unit, `afr_agent`): two runs differing only in offered tools → the same pinned digest. A changed identity sentence → a new digest.
- **Dimension 1.4** — DONE. guidance names no host product → Test `guffbolt_tool_specs_name_no_host_product` (unit, guffbolt, extended to guidance): a guidance line naming `fleet` fails the test.

### §2 — The model is told who is speaking, and what is data

A turn a person did not type gets one source line, rendered from its `event_type` and `actor` (`af_wire/src/turn/mod.rs:105-108`). A webhook delivery, a schedule firing and another fleet's request each have their own line; a person's chat message has none.

A history `Turn` (`af_wire/src/run/payload.rs:18-33`) gains both fields, so an earlier turn renders the same bytes next run. Harness text carries one tag: where guffbolt pushes `Message::User(why)` (`guffbolt/src/drive/mod.rs:290`), and where a child's task opens. §7's sentences say what the tag means and that tool output, pages, files and memories grant no permission.

**Implementation default:** the line states the source and says the body is data to act on under standing guidance. Wording too strong makes a webhook-fed fleet refuse its own job, so §2 changes advice, not enforcement.

- **Dimension 2.1** — DONE. a delivered turn names its source → Test `a_delivered_turn_is_told_its_source` (unit, `afr_agent`): a webhook, a schedule firing and another fleet's request each carry their line. A person's chat message carries none.
- **Dimension 2.2** — DONE. harness text is tagged → Test `a_continue_reason_reaches_the_model_tagged_as_the_harness` (unit, guffbolt): a durable task's "continue" line → wrapped in the harness tag, never bare user text.
- **Dimension 2.3** — DONE. an earlier turn renders the same bytes → Test `an_earlier_webhook_turn_renders_the_same_bytes_next_run` (unit, `afr_agent`): a webhook turn as the head, then as history → identical text both times.

### §3 — An environment block, fed by the run's facts

One `## Environment` block follows the guidance. It names the working directory and that commands run through `sh -c` (`guffbolt/src/tools/process/shell.rs:21`) when the run holds a sandbox tool. It states the fleet's network from `http_egress` (`af_wire/src/policy/mod.rs:35`): search on, or search off with "Allow all" named.

Outbound network from commands comes from M258_001's `EgressFacts`; a browser or none comes from M259_001's browser source `[?]`. The prompt renders them, so a changed engine changes one source, not a sentence.

The date line reads "Today is Thursday, November 26, 2026 (UTC).", from the turn's `created_at` (`af_wire/src/turn/mod.rs:114`). It is day-grained and sits outside the fixed text, so the prefix changes at most once a day and a test needs no clock.

- **Dimension 3.1** — DONE. the date is there and stays out of the digest → Test `a_runs_prompt_carries_the_turns_date_outside_the_digest` (unit, `afr_agent`): `created_at` 2026-11-26T09:00Z → the date line in the instructions. `Prompt.fixed` does not hold it.
- **Dimension 3.2** — DONE. a private fleet is told → Test `a_private_fleets_prompt_says_search_is_off` (unit): `restricted` → the network line names "Allow all". `public` → it says search is on.
- **Dimension 3.3** — DONE. commands' home is named only with a sandbox → Test `a_run_with_a_sandbox_is_told_where_commands_run` (unit): a sandbox tool held → the working directory and `sh -c`. None held → no such line.
- **Dimension 3.4** — a missing browser is named → Test `a_run_whose_engine_has_no_browser_is_told_so` (unit): a run with no browser source → one line saying no browser is available. With one → the browser line.

### §4 — One stable cached prefix

`task_start` and `task_input` take `source_turn_id`, `source_start` and `source_end` as optional (`afr_tools/src/verbs/tasks/mod.rs:68-94`). Absent means the newest person's message, whole, which the runner fills before the verb call. The `## Durable tasks` block keeps only unfinished tasks, so it changes when a task does.

Blocks follow the order in the design flow, most shared first. On a Messages request the conversation tail is the top-level breakpoint the provider moves (`guffbolt-ai/src/wire/request/mod.rs:63-65`), not a marker on the last block. No test asserts such a marker.

- **Dimension 4.1** — DONE. a judged follow-up repeats the prefix → Test `a_judged_follow_up_repeats_the_first_runs_prefix` (unit, `afr_agent`): the history fixture gains a decision profile and the task tools. The follow-up's first request repeats the system prompt, whole tool specs and earlier message byte for byte.
- **Dimension 4.2** — DONE. a task needs no copied offsets → Test `task_start_without_a_span_takes_the_whole_newest_message` (unit, `afr_tools`): no span → the verb posts the newest turn with 0 and its byte length. A span past the message's end → `invalid_arguments`, and nothing is posted.

### §5 — Only tools that can work are offered

`offer::names` (`afr_agent/src/offer/mod.rs:19-26`) already withholds the task tools from an unjudged run. It also withholds the three browser tools while the run has no browser source, and `image` when the wire takes none (`afr_agent/src/loop/shared.rs:139`).

§3's block names each withheld capability in one line, so the model can tell the person. A withheld tool's earlier call still replays as history. The daemon's policy does not change.

- **Dimension 5.1** — DONE. no `image` on a text-only wire → Test `a_run_on_a_text_only_wire_is_not_offered_image` (unit, `afr_agent`): a Chat-wire model → `image` absent. A Messages-wire model → present.
- **Dimension 5.2** — no browser where the engine has none → Test `a_run_whose_engine_has_no_browser_is_not_offered_the_browser_tools` (unit): a run with no browser source → none of the three offered.
- **Dimension 5.3** — every offered tool can run → Test `no_offered_tool_refuses_every_call` (unit): walking the offered set against the run's backends → no tool answers `browser_unavailable`, `image_input_unavailable` or `tool_not_offered` by construction.
- **Dimension 5.4** — history survives a withheld tool → Test `a_withheld_tools_earlier_call_still_replays` (unit): a history `screenshot` call on a run that withholds it → the call and its result replay, with no refusal.

### §6 — Descriptions in one shape, with examples a test parses

Each description that leaves a choice open states what it does and returns, when to use it, when not, and the sibling to use instead. Examples sit in an `EXAMPLES` list beside the description, rendered into it, so a test reads them without parsing prose.

| Tool | What to add |
|---|---|
| `update_plan` | when a plan is worth recording, and that a person watches it |
| `message`, `notify` | which one, against simply answering |
| `memory_recall`, `memory_list` | only `core` entries are shown at the start; anything else must be recalled |
| `web_fetch`, `http_request` | a page to read, against an API call with a credential |
| `apply_patch` | one worked patch beside the prose grammar |
| `file_edit`, `file_write` | edit for a change, write for a new or replaced file |
| `spawn_agent` | what a child cannot do |
| `cron_remove`, `cron_update` | the id comes from `cron_list` |

- **Dimension 6.1** — DONE. every example parses → Test `every_example_a_tool_gives_parses_as_its_arguments` (unit, guffbolt): each example parses with its tool's `Arguments` (`guffbolt/src/tools/mod.rs:196-204`), and `apply_patch`'s with the patch parser. A planted malformed example fails.
- **Dimension 6.2** — DONE. siblings are named → Test `every_tool_names_the_sibling_it_is_confused_with` (unit, guffbolt): over the declared pairs in the table, each description names its sibling.
- **Dimension 6.3** — DONE. wording changes show in review → Test `the_offered_tool_specs_match_their_golden_file` (unit, `afr_agent`): the offered specs serialise to `tests/golden/offered_tools.json` byte for byte. The test prints their size, so token cost is visible.

### §7 — A short harness-facts section

Three sentences join the identity inside the fixed text. The final answer is what the person reads in the thread. Independent calls can go in one step and run in order (`guffbolt/src/drive/call.rs:36`). §2's two sentences name the harness tag and say data grants no permission.

The sentences promise no Markdown, because what Slack and email render is unverified.

- **Dimension 7.1** — DONE. the facts sit in the stable prefix → Test `the_harness_facts_sit_inside_the_fixed_text` (unit, `afr_agent`): each sentence is in `Prompt.fixed` exactly once. None appears in the per-run blocks.

### §8 — guffbolt's two approved tool fixes

`tool_search` answers 8 matches when no `limit` is named, in place of `usize::MAX` (`guffbolt/src/tools/search.rs:138`). `grep` gains `context`, the lines around each match, 0 when absent, and cuts each line at 500 characters with a note. Indy named both numbers (Discovery).

Neither fix changes a fleet today. `grep` is not in the runner's catalog (`afr_tools/src/catalog/mod.rs:138-177`), and nothing in `rustd/crates` wires `tool_search`; both serve guffbolt's standalone embedders until then.

- **Dimension 8.1** — DONE. the default limit is 8 → Test `tool_search_answers_eight_when_no_limit_is_named` (unit, guffbolt): 12 deferred matches and no `limit` → 8.
- **Dimension 8.2** — DONE. a long line is cut → Test `grep_cuts_a_long_line_at_500_characters` (unit, guffbolt): a 2,000-character matching line → 500 characters and the note.
- **Dimension 8.3** — DONE. context is shown → Test `grep_shows_context_lines_around_a_match` (unit, guffbolt): `context: 1` → the line before and after each match. Absent → the match alone.

### §9 — A person's message carries images

`POST F/messages` and `POST W/fleets` take `images`, and the turn's request body keeps them beside `message`, so Postgres holds them. The web composer, `agentsfleet fleet create --image <path>` and `fleet steer --image <path>` send them. The runner puts them in the run's first user message on Messages, Responses, Gemini and Ollama.

A fleet whose model is on the Chat wire refuses the message, since Chat takes text alone. The composer sends supported image kinds with the message. Other files keep the preview upload. An image message queued during a run starts a turn of its own. The browser keeps image bytes in memory for retry, while the pending-send record retains the operation and the attachment marker across reloads.

**Bounds, each forced by a provider or a transport.** Each image is PNG, JPEG, GIF or WebP, the kinds the wires take (`guffbolt/src/tools/files/image.rs`). Each is at most 5 MiB decoded, the Messages wire's cap (`guffbolt/src/tools/limits.rs:43-45`). The runner scales a side past 2048 pixels, and the route's body limit is the runner frame, 32 MiB (`af_wire/src/protocol/mod.rs:29`).

- **Dimension 9.1** — DONE. The daemon stores the original PNG in the acknowledged turn in `integration_message_images`. The runner's `a_message_with_an_image_reaches_the_model` checks the first HTTP request to a fake Messages provider for the same PNG in a user image block.
- **Dimension 9.2** — DONE. `integration_message_images` selects a registered Chat-wire model. A fresh image request returns 409 `UZ-AGT-027` and names the model. The ledger keeps one turn, and an exact retry of the earlier successful operation still returns that turn.
- **Dimension 9.3** — DONE. The full HTTP harness rejects a 6 MiB PNG, TIFF and corrupt PNG with 400 `UZ-AGT-026` before admission. Test `a_message_image_past_the_wire_bounds_is_refused` in the tenant plane.
- **Dimension 9.4** — DONE. Desktop and mobile Playwright check the first-message and existing-fleet composers, the 6 MiB refusal before sending, and a retry with the same attachment bytes and operation ID. The app's steer route forwards the attachment.
- **Dimension 9.5** — DONE. `fleet-steer.integration.test.ts` checks that `--image picture.png` sends the original image in base64 and that a missing file exits 4 before making a request. `fleet create --image` uses the same file reader.
- **Dimension 9.6** — DONE. `prime-keen-hawk.spec.ts` passed on `96ce588de` in 8.911 seconds in hosted acceptance. The Messages-wire model named red, the stored turn held the original PNG, and the question and answer survived a dashboard reload. The full suite reported 115 passed and two failures, in credential-denial context and durable-task completion. [Playwright evidence](https://github.com/agentsfleet/agentsfleet/actions/runs/46129180070/artifacts/15257104912)

## Interfaces

**Prompt.** Blocks in this order: identity and harness facts, guidance per held tool, `## Environment`, repair context, workspace, pinned memory, the date line, unfinished tasks. Only the first is `Prompt.fixed`.

**guffbolt.** `Handler` gains `GUIDANCE: &str` and `EXAMPLES: &[&str]`, both empty by default; `Describe` surfaces them. Harness text carries one tag, named in §7's sentences.

**Runner wire.** A history `Turn` gains `event_type` and `actor`. The task verbs' span fields become optional. `VERSION` (`af_wire/src/protocol/mod.rs:15`) moves once.

**Messages.** `POST F/messages` and `POST W/fleets` take `images?: [{media_type, data}]`, `data` in base64. The media types are PNG, JPEG, GIF and WebP; each image is at most 5 MiB decoded.

**Codes.** Two new codes, numbered at PLAN from the next free in the family. 409: the fleet's model cannot see an image. 400: an image is past the wire's bounds.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| A webhook body reads as the person | security | a delivered body asks for a side effect | its source line names the delivery; enforcement stays the daemon's `[?]` | `a_delivered_turn_is_told_its_source` |
| Summary lost on a tool change | failure | guidance inside the digest | the digest covers only the identity and harness facts | `a_tool_set_change_keeps_the_saved_summary` |
| Prefix rebuilt every judged turn | capacity | per-turn facts in the system block | the task block holds only unfinished tasks | `a_judged_follow_up_repeats_the_first_runs_prefix` |
| A withheld tool in history | failure | an earlier call to a tool now withheld | the call replays as history | `a_withheld_tools_earlier_call_still_replays` |
| A broken example | failure | an example no longer parses | the test fails before merge | `every_example_a_tool_gives_parses_as_its_arguments` |
| Product name in guffbolt | failure | guidance names the host product | the text test fails | `guffbolt_tool_specs_name_no_host_product` |
| A span past the message | failure | the model sends an offset past the end | `invalid_arguments`; nothing posted | `task_start_without_a_span_takes_the_whole_newest_message` |
| An image a model cannot see | failure | an image for a Chat-wire fleet | 409 naming the model; no turn | `an_image_for_a_model_that_takes_none_is_refused` |
| An oversized or unknown image | capacity | past 5 MiB, another kind, or bytes that do not decode | 400 naming the bound; no turn | `a_message_image_past_the_wire_bounds_is_refused` |
| A missing file in the CLI | failure | `--image` names no file | exit 4; no request | `fleet steer --image sends the file` |

## Invariants

1. guffbolt's guidance names no host product — `guffbolt_tool_specs_name_no_host_product` reads every tool's text against `HOST_WORDS` (Dimension 1.4).
2. A saved summary survives a tool-set change — `digests` hashes `Prompt.fixed`, which holds only the identity and harness facts (Dimension 1.3).
3. A loop hears only about tools it holds — guidance renders from the specs passed to `request`, never from the run's whole list (Dimension 1.2).
4. No image reaches a wire that cannot carry it — the route refuses by the fleet's model wire before any turn exists (Dimension 9.2).

## Metrics & Observability

N/A: no product or operator signal changes. An image count rides no event until a funnel needs one.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Each loop hears only about its own tools, and its summary survives (§1, §7) | `make test-unit-all` | exit 0; `a_run_is_told_only_about_the_tools_it_holds ... ok`, `a_child_given_fewer_tools_is_told_about_fewer ... ok`, `a_tool_set_change_keeps_the_saved_summary ... ok`, `the_harness_facts_sit_inside_the_fixed_text ... ok` | P0 | |
| R2 | A delivered turn names its source, and harness text is tagged (§2) | `make test-unit-all` | exit 0; `a_delivered_turn_is_told_its_source ... ok`, `a_continue_reason_reaches_the_model_tagged_as_the_harness ... ok` | P1 | |
| R3 | The date, network, workspace and engine facts are stated (§3) | `make test-unit-all` | exit 0; `a_runs_prompt_carries_the_turns_date_outside_the_digest ... ok`, `a_private_fleets_prompt_says_search_is_off ... ok` | P1 | |
| R4 | A judged follow-up repeats its prefix (§4) | `make test-unit-all` | exit 0; `a_judged_follow_up_repeats_the_first_runs_prefix ... ok` | P1 | |
| R5 | No offered tool refuses every call (§5) | `make test-unit-all` | exit 0; `no_offered_tool_refuses_every_call ... ok`, `a_withheld_tools_earlier_call_still_replays ... ok` | P1 | |
| R6 | Every example parses, and the offered specs match their golden file (§6) | `make test-unit-all` | exit 0; `every_example_a_tool_gives_parses_as_its_arguments ... ok`, `the_offered_tool_specs_match_their_golden_file ... ok` | P1 | |
| R7 | guffbolt's approved fixes hold (§8) | `make test-unit-all` | exit 0; `tool_search_answers_eight_when_no_limit_is_named ... ok`, `grep_cuts_a_long_line_at_500_characters ... ok` | P2 | |
| R8 | A person's image reaches a model that sees, and is refused for one that does not (§9) | `make test-integration-rustd` | exit 0; `a_message_with_an_image_reaches_the_model ... ok`, `an_image_for_a_model_that_takes_none_is_refused ... ok`, `a_message_image_past_the_wire_bounds_is_refused ... ok` | P1 | |
| R9 | The app and the CLI send images (§9) | `make test-unit-all` | exit 0; `the composer sends an attached image` and `fleet steer --image sends the file` passed | P1 | |
| R10 | The live paths hold: the image fleet and every prime fleet pass (§9, Behaviour evals) | `make acceptance SUITE=all` | exit 0; `prime-keen-hawk.spec.ts` and every `prime-*.spec.ts` passed | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

### Behaviour evals

- **Grounding rule:** an answer claims no tool, capability, date or network setting that its prompt did not state.
- **Golden set:** the follow-ups of the prime fleets in `tests/fixtures/fleets/` and `keen-hawk.json`. The worst known case is a webhook body asking for a trigger; each failure found in use becomes a follow-up.
- **Ship threshold:** grounding holds in every case, and every prime spec passes as it did before this change; R10 computes both.
- **Fallback:** the wording of the failing Section reverts, and its deterministic tests stay. A fabricated capability fails R10.

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| the fixed text's tool sentences | `git grep -n -e "start work that must keep going with task_start" -- rustd` | no output |
| the per-turn offsets in the task block | `git grep -n -e "into it; 0 and" -- rustd/crates/runner/afr_agent` | no output |

The first phrase has one production hit at `679002666`, `afr_agent/src/prompt/mod.rs:50`; the second has one, `:242`. Their tests are `afr_agent/src/prompt/tests/mod.rs:77-82` and `prompt/tests/tasks.rs:57-61`, which change with them.

## Out of Scope

- Per-model prompt additions, argument repair and deferring tools behind `tool_search`, the research's items 8 to 10. Each needs a measurement or a model evaluation first.
- A daemon verb refusing a side effect by the turn's sender: unread today, recorded `[?]` in Discovery.
- Reading a checkout's `AGENTS.md` into the prompt, which needs a trust decision first; a title prompt.
- Files other than images in a message, and audio or video.
- Search, page reading and the browser: M259_001.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a person drops a screenshot of a broken page into the chat and the fleet describes it. A fleet asked about "next weekend" answers from today's date.
2. **Preserved user behaviour** — every message, every tool call and every fleet's chat keep working; the preview upload still works for files that are not images.
3. **Optimal-way check** — prompt text rendered from what the run holds, and images on the message route the wires already read. The gap is a fixed paragraph and a text-only route.
4. **Rebuild-vs-iterate** — iterate: the prompt builder, the offer seam and the message route all exist.
5. **What we build** — guidance per tool, a source line, an environment block and a stable prefix. Also a narrower offer, reshaped descriptions, three harness facts, two tool fixes and images.
6. **What we do NOT build** — per-model prompts, argument repair or tool deferral until a measurement asks. Model limits in the prompt, which the request already carries.
7. **Fit with existing features** — it compounds with M259_001's search and browser, the access card and the `image` gate. It must not destabilize the history prefix test.
8. **Surface order** — runner first; then the message route; then the app and the CLI, which both send the new field.
9. **Dashboard restraint** — the composer shows an image's thumbnail and nothing new besides; the prompt is never shown.
10. **Confused-user next step** — a refused image names the fleet's model and that it reads no images; the fleet page's model picker is the fix.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the prompt's text first (§1, §7), then each fact it states (§2, §3), then the cache and the offer (§4, §5). The words, the fixes and images follow.
- **Alternatives considered:**
  - A name-to-sentence table inside `afr_agent/src/prompt`: smaller, but a narrowed child stays misinformed.
  - Per-model prompt files that replace the base: each must repeat the base by hand, and every variant multiplies what an evaluation covers.
  - Sending a changed fact as a mid-conversation message: needs a system role some wires lack; a day-grained line costs one cache rebuild a day.
- **Patch-vs-refactor verdict:** a **patch** because every seam exists; the prompt's one string becomes ordered blocks.

## Discovery (consult log)

- > Owner (Indy, Nov 26, 2026): "4. I want to replace the engine first 5. Then the web search with parallel + other providers like opencode and the browse decision 6. so 2 milestones, in the same milestone i need the tools of guffbolt to be improved with the system prompts and such proposed like opencode" — context: this spec is the tools and prompts half of that milestone.
- > Owner (Indy, Nov 26, 2026): "Essentially add missing features for guffbolt agent harness", "max 2 in 1 milestone for the guffbolt tool updates" and "I need the final scope to fix" — context: two Workstreams in this milestone.
- > Owner (Indy, Nov 22, 2026): "3. ... keep" (the tools page's proposed grep line cap, web_fetch default output cap and tool_search limit 8 are approved for §10) — context: §8's numbers, recorded at `docs/v2/done/M253_004_P1_API_DOCS_INFRA_GUFFBOLT_AI_PROVIDERS_FOR_GUFFBOLT.md:508`; the 500-character cut is at `docs/v2/done/M253_004_TOOLS_BRAINSTORM.md:20,236`.
- > Owner (Indy, Nov 26, 2026): "In web and the cli, since if its api then allow both" — context: §9's three surfaces.
- **Open owner decisions (`[?]`), each an agent default until ruled:**
  - Numbering: the engine as M258 and this as M259 was asked and timed out unanswered.
  - Whether "like opencode" includes per-model prompts. This spec builds none until a measurement shows one model family failing one behaviour.
  - How strongly the source line words a delivery. Too strong, and a webhook-fed fleet refuses its own job.
  - The composer attaches images to the message and keeps the preview upload for other files.
  - Where a run's browser fact lives: M259_001's browser source, or a field M258_001's `Capabilities` gains.
- **Unverified (`[?]`), each settled by a named step:**
  - Whether `trigger_add`, `cron_add` or `task_start` refuse by the turn's sender; reading their daemon handlers settles it.
  - The serialised size of the offered specs; Dimension 6.3 prints it.
  - The cached-token saving of §4 on a live judged fleet; `Usage.cached_input` before and after settles it.
  - What Slack and email render, and whether a continuation turn's `created_at` is its re-queue time.
- **Corrections from the skeptic review** — on a Claude request the cache tail is the top-level breakpoint, not a last-block marker. The two tool fixes change no fleet until wired, and the catalogue holds 21 models.
- **Consults** — the prompts research (Nov 26, 2026) read opencode at `44344bd22d`, Codex at `af719423` and pi at `db7971285`. Borrowed: guidance per held tool, ordered sources, a speaker mark and an environment block. Copied: nothing, so no licence notice is owed and no code names another harness.
- **Restructure record** — §1 to §5 and §7 carry the research's items 1 to 7. §8 and §9 are §7 and §9 of the web search spec drafted on Nov 26, 2026, which the Pull Request adding this spec removes. Its §6 folds into §1 and §3, with the turn's `created_at` in place of a clock.
- **Model reviews** — none yet.
- **Initial implementation evidence, Nov 28.** The agent, runner tool and standalone guffbolt library suites passed 775 tests. Another 59 file-tool API cases passed. The suites exercised the root and narrowed child request, stable summary identity, source labels on real continuations and child task messages, eight default matches, Unicode line shortening, and grep context through one executor call.
- **Opening decision, Nov 28.** The owner requested this workstream start once M259_001 Dimensions 1.7, 9.1, 9.3 and 9.4 were complete. All four passed on the web-tools branch. The independent prompt and tool changes can proceed there. The engine-dependent dimensions still need merged M258 and the browser capability evidence. [Web-tools PR](https://github.com/agentsfleet/agentsfleet/pull/874) · [Dashboard acceptance](https://github.com/agentsfleet/agentsfleet/actions/runs/46114949902/job/140886570326)
- **Sender, environment and task input evidence, Nov 28.** 263 runner agent tests, 15 daemon history unit tests, five PostgreSQL history cases and seven task tool tests passed. The delegation journey also passed on the consolidated branch. A later run presents the same webhook message it presented on delivery. The date stays outside the saved summary digest. A judged follow-up preserves its instructions and fills a missing task span from the latest human message.
- **Tool description evidence, Nov 28.** The tool library passed 268 tests. Examples parsed against the tools’ argument types, including the patch grammar. The golden render matched 45 offered tools and measured 33,628 JSON bytes.
- **Fleet chat review, Nov 28.** Desktop and mobile Chromium passed 151 Playwright checks. The desktop navigation assertion was skipped on mobile. The review covered the seven Conversation samples, image attachment in the first message and existing fleets, and Send with a tall draft while reconnecting in a 360 px window. The reference composer shows the same attachment control as the dashboard. [UX reference](../../ux.html#conversation) · [Reference checks](../../../ui/packages/app/tests/e2e/ux-review.spec.ts) · [Short-window checks](../../../ui/packages/app/tests/e2e/fleet-summary-layout.spec.ts)
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.
