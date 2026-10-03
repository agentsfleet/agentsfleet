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

# M210_002: The Rust runner is the harness — a tool catalog the model picks from, a router that runs each tool in the supervisor or the sandbox, three providers, every supervisor-side tool, and the four reference bundles end to end against the real daemon

**Prototype:** v2.0.0
**Milestone:** M210
**Workstream:** 002
**Date:** Oct 02, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the workstream after which the fleets of record (the Continuous Integration (CI) responder and repairer, the Pull Request reviewer, the incident repairer) run on the Rust runner, and the one every tool workstream plugs into
**Categories:** API, INFRA
**Batch:** B2 — after M210_001 is on `main`; the milestone's follow-up Pull Request. The sandbox-side tools, the runner verbs for schedules and messages, the nested loops and the cutover are later milestones
**Branch:** `feat/m210-agent-loop-hosted-tools`
**Baseline revision:** 4339afb59fe83a20fb643004e432b9755e1b14a7
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M210_001 (supervisor duties, the `AgentEngine` trait, the executor) · M209_001 (`afd_wire::tool_trace`, the outcome fields on `tool_call_completed`) · M209_003 (`afd_wire::tool_detail`, the tool-calls verb)
**Provenance:** LLM-drafted (Claude Fable 5.1, Oct 02, 2026) from a source trace on `main`, the four bundles under `tests/fixtures/fleetbundle/`, and Indy's in-session decisions; Codex at `~/Projects/oss/rs/codex` `2e5fea64e`, IronClaw at `~/Projects/oss/rs/ironclaw` `b0b999d96`, ZeroClaw at `~/Projects/oss/zeroclaw` `74362c2d6`
**Canonical architecture:** `docs/architecture/runner_execution.md` §Process model, §"Tool catalog", §Crates, §Credentials, §Repository writes; `docs/architecture/runner_fleet.md` §"Live activity (the SSE tail)", §"Memory continuity — durable fleet memory rides the trusted plane", §"Egress model — outbound is the only network surface"

---

## Overview

**Goal (testable):** `test_ci_responder_triages_a_failed_run` — against the real `agentsfleetd` and fake GitHub, Grafana and model upstreams, the `ci-responder` bundle's lease is offered exactly its policy's tools, the model picks `http_request` reads and `memory_recall`, every call is checked against the origin rules, its `${secrets.*}` placeholder is substituted in the `Authorization` header at send time, each call emits one `tool_call_started` and one `tool_call_completed` with its outcome, the full outputs post under the fence before the report, and the report carries the diagnosis, the three token counts and a bounded trace. The same lane proves `ci-repairer` opens exactly one draft Pull Request on the daemon-named branch with the write token never in the prompt.
**Problem:** M210_001 ends with a scripted engine and no tools. The runner has to be the harness Codex is: a catalog, a model that picks, a router, and handlers that run where their runtime says (`docs/architecture/runner_execution.md` §"Tool catalog"). Three duties the Zig tool bridge owns have no Rust home: placeholder substitution at the HTTPS boundary, the per-host allowlist and origin rules (`afd_wire::policy::HttpOriginPolicy`), and the repository-write rules the daemon compiles (`rustd/crates/afd_gate/src/policy/egress/write.rs:54-90`). Both repairer bundles also require a "trusted repair context" naming the daemon-issued branch (`ci-repairer/SKILL.md` step 1, `incident-repairer/SKILL.md` "GitHub reconciliation"), and no code renders one: the branch reaches a lease only inside the locked request rules (`rustd/crates/afd_fleet/src/lease/deliver.rs:48-59`), and `instructions` is the SKILL.md body alone (`rustd/crates/afd_fleet_runtime/src/instructions.rs:34`).
**Solution summary:** `afr_tools` holds the catalog: every published tool with its JSON schema and its runtime (`Supervisor` or `Sandbox`); `afr_agent` offers the policy's subset, runs turns against a provider, routes each call to its handler, scrubs and bounds every output, emits the M209 frames and trace, and closes open calls once. `afr_providers` speaks Anthropic Messages, OpenAI Responses and OpenAI-compatible chat, chosen by `ExecutionPolicy.provider`, and sends `web_search` as the provider's hosted tool spec. The supervisor-side handlers land here: `http_request` with the three policy duties, `web_fetch`, `pushover`, the four memory tools, `calculator`, `update_plan`. Sandbox-side names are in the catalog with their runtime and routed through the executor; their handlers are the sandbox-side tools workstream. The prompt gains a trusted repair context rendered from the repository binding and the locked branch. A lease that declares no sandbox-side tool starts no sandbox. The integration lane installs the four fixture bundles and drives each against fakes.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): the harness — catalog, router, loop, providers, supervisor-side tools; the four bundles run on Rust
- **Intent (one sentence):** A fleet written for today's tools runs on the Rust runner with the same policy enforced, the model choosing tools the way Codex's does, every call visible in the thread, and the model key and write token never inside a sandbox or a prompt.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` — §"Tool catalog" (every tool and its runtime), §Process model, §Credentials, §Repository writes; the Decisions table is binding.
2. `rustd/crates/afd_wire/src/policy.rs` — `ExecutionPolicy`, `NetworkPolicy`, `HttpOriginPolicy`, `HttpRequestRule`, `Mintable`, `RepositoryBinding`, `ContextBudget`: the whole of what a run may do.
3. `rustd/crates/afd_gate/src/policy/egress/write.rs` and `rustd/crates/afd_gate/src/policy/repair.rs` — the locked ref and pull-request rules, and why the branch name is exact.
4. `rustd/crates/afd_wire/src/activity.rs`, `rustd/crates/afd_wire/src/report.rs`, `rustd/crates/afd_wire/src/credentials.rs` — the frames, the report with `Outcome` and `FailureClass`, the mint verb.
5. `src/runner/engine/tool_bridge.zig` — the one disposition carried over: a policy naming a tool the runner cannot host fails the lease, never runs with a quieter tool set.
6. `tests/fixtures/fleetbundle/ci-responder/SKILL.md`, `tests/fixtures/fleetbundle/ci-repairer/SKILL.md`, `tests/fixtures/fleetbundle/github-pr-reviewer/SKILL.md`, `tests/fixtures/fleetbundle/incident-repairer/SKILL.md` — what the loop must make possible, step by step.
7. https://github.com/openai/codex/tree/2e5fea64eefcaa19f48458b2386011b619f69c70/codex-rs — `core/src/tools/registry.rs` and `spec_plan.rs` (the catalog per config), `router.rs` (dispatch), `sandboxing.rs` (`Sandboxable`, `ToolRuntime`), `hosted_spec.rs` (`web_search` as a provider spec), `core/src/codex.rs` (the turn loop).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/afr_memory/`, `rustd/crates/afd_wire/src/memory.rs`, `rustd/crates/afd_fleet/` (`Cargo.toml`, `src/memory/`), `rustd/crates/afd_api_tenant/src/handler/fleet/memory_request*`, `rustd/crates/afr_tools/` (`Cargo.toml`, `src/catalog.rs`, `schema.rs`, `runtime.rs`, `handler.rs`, `lease.rs`, `testing.rs`, `http_request.rs`, `network.rs`, `origin_rules.rs`, `placeholders.rs`, `mint.rs`, `web_fetch.rs`, `pushover.rs`, `memory.rs`, `calculator.rs`, `plan.rs`, `error.rs`) | CREATE | The catalog with every published tool, its schema and runtime; the supervisor-side handlers, each a typed `Handler`; `afr_memory` is the run's memory, its bounds declared once in `afd_wire`; `network.rs` is the guarded transport the tools share |
| `rustd/crates/afr_agent/` (`Cargo.toml`, `src/lib.rs`, `engine.rs`, `loop.rs`, `turn.rs`, `router.rs`, `prompt.rs`, `context.rs`, `events.rs`, `trace.rs`, `records.rs`, `scrub.rs`, `json.rs`, `ledger.rs`, `spans.rs`, `error.rs`, `fixture.rs`) | CREATE / EDIT | The loop, routing, prompt, context budget, frames, trace, full records, the secret scrub and the one JSON walk it shares with the trace, and the ledger that ends each call once; `AgentRun` gains the runner verbs the loop calls |
| `rustd/crates/afr_providers/` (`Cargo.toml`, `src/provider.rs`, `connect.rs`, `registry.rs`, `request.rs`, `transport.rs`, `turn.rs`, `wire.rs`, `logs.rs`, `retry.rs`, `error.rs`, `assets/providers.json`), and `rustd/crates/agentsfleet_runner/` (`Cargo.toml`, `src/main.rs`, `src/main_tests.rs`) | CREATE / EDIT | One trait and a `Connect` seam; the named-provider registry with rig dialects; rig-core speaks each wire over the runner's transport (no redirect, bounded retry), with hosted specs; the runner logs through `afr_providers::log_filter`, so no level journals rig's raw replies (Indy, Oct 03) |
| `rustd/crates/afr_supervisor/` (`Cargo.toml`, `src/lease_loop.rs`, `src/lease_loop/`, `src/client.rs`, `src/records.rs`, `src/report.rs`, `src/test_support/`), `src/identity.rs`, `rustd/crates/afd_wire/src/paths.rs`, `rustd/crates/afd_observability/src/semconv.rs`, `rustd/crates/afd_core/src/test_util/trace.rs`, `rustd/crates/afr_agent/tests/support/scripted.rs` | EDIT / CREATE | Run the real loop; refuse a lease naming an unhosted tool; start no sandbox for a supervisor-only lease; `lease_loop.rs` (352 lines) splits; records post before the report on the `tool-calls` path, and the report carries the trace; the scripted engine stays for M210_001's lane |
| `rustd/crates/agentsfleetd/tests/support/fake_github.rs`, `fake_grafana.rs`, `fake_elastic.rs`, `fake_model.rs`, `bundle_install.rs` | CREATE | Fakes speaking the upstream shapes the bundles read; installing a fixture bundle through the seed |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | CREATE | The four bundles end to end (`#[ignore]`d, run by `make test-integration-rustd`) |
| `rustd/crates/afr_agent/tests/`, `rustd/crates/afr_providers/tests/`, `rustd/crates/afr_tools/tests/` | CREATE | Unit proofs per crate |
| `tests/fixtures/fleetbundle/incident-repairer/TRIGGER.md`, `tests/fixtures/fleetbundle/incident-repairer/SKILL.md`, `rustd/crates/afd_fleet_runtime/tests/frontmatter_corpus.rs` | EDIT | The bundle stops promising the retired approval card; the corpus test pins it |
| `docs/architecture/runner_execution.md`, `docs/architecture/capabilities.md` | EDIT | Landed at authoring: the tool catalog, the trusted repair context, the schedules reversal |
| `docs/v2/pending/M211_001_P1_API_INFRA_RUST_RUNNER_SANDBOX_SIDE_TOOLS.md`, `docs/v2/pending/M211_002_P1_API_INFRA_RUST_RUNNER_NESTED_LOOPS.md`, `docs/v2/pending/M213_001_P0_API_DOCS_INFRA_RUST_RUNNER_CUTOVER_ZIG_RETIRED.md` | EDIT | The toolbox triage (`4339afb59`, never pushed, rides this Pull Request by Indy's call); M211_001's browser becomes `chromium-headless-shell` (Indy, Oct 03); read-first links follow this spec to `active/` |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (tool names, schemas, runtimes, placeholder grammar, retry bounds, scrub patterns and prompt headings are constants), TFX (tests import the bounds from `afd_wire`), OWN (one owner per provider stream, tool call and record batch), FLS (drain a provider stream and a tool response on every exit path), TIM (retry ceilings, call timeouts and the context cap are explicit), ECL (`Retry-After` and 5xx retry; 4xx ends the call), NTP (every upstream body narrowed at its parse boundary), OBS, ERR-RS, TST-NAM, TCF, NDC.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — one `ErrorKind` per crate through `afd_core::error_shell!`; a tool refusal carries its code to the model, never a bare string.
- `docs/LOGGING_STANDARD.md` — never log a key, a token, a prompt, a tool argument or an output; `docs/architecture/runner_fleet.md` §"Egress model — outbound is the only network surface" — the allowlist is enforced before a connection, never after.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | `error_shell!` and `error_lifts!` only; the `Result` alias is the one hand-written line |
| UFS / LOGGING / MILESTONE-ID | yes | Constants per concern; scoped events with `error_code`; no milestone identifiers in source |
| File & Function Length (≤350/≤50/≤70) | yes | One concern per file across the three crates; the providers never share a request builder |
| Architecture consult | yes | `runner_execution.md` §"Tool catalog" is the design; drift is fixed in the same commit |
| CI/CD edit guard | no | No workflow edit; the bundle lane rides `make test-integration-rustd` |

## Prior-Art / Reference Implementations

- **Reference:** Codex `core/src/tools/` (`2e5fea64e`) — the catalog is built per config (`registry.rs`, `spec_plan.rs`), the router dispatches (`router.rs`), each handler declares its runtime (`sandboxing.rs`), and `web_search` reaches the model as a hosted spec (`hosted_spec.rs`); the turn loop is `core/src/codex.rs`. Taken as design, not code.
- **Reference:** ZeroClaw `~/Projects/oss/zeroclaw/crates/zeroclaw-providers` — Anthropic streaming with prompt caching, the OpenAI-compatible chat wire, and the input/cached/output usage split. Mined, with `rustd` error shapes.
- **Reference:** IronClaw `~/Projects/oss/rs/ironclaw/crates/ironclaw_llm` (retry honouring `Retry-After`, a ceiling, no failover inside one lease) and `ironclaw_safety` (leak scan on tool output and model input). Ported.
- **Reference:** `rustd/crates/afd_credential/src/credential/github/request.rs` (the mint client `afr_tools::mint` wraps, caching per lease until `expires_at_ms`) and `rustd/crates/afd_gate/src/policy/egress/` (the rules are compiled there; the runner only evaluates `HttpRequestRule` as written, exact path and locked fields).

## Sections (implementation slices)

### §1 — The catalog and the router are the harness

The catalog names every tool in `docs/architecture/runner_execution.md` §"Tool catalog" with its runtime; each handler carries its JSON schema, so a tool with no handler yet has none to drift. A lease is offered exactly the names in `ExecutionPolicy.tools`; a policy naming a tool the catalog has no handler for refuses the lease with a logged code, never a quieter tool set. A model call to a name outside the policy is a tool error and the run continues. The router runs a `Supervisor` handler in-process and a `Sandbox` handler through the executor connection; in this workstream the sandbox-side handlers are a stub that proves the route, and a lease whose tools are all supervisor-side starts no sandbox.

- **Dimension 1.1** — The model is offered exactly the policy's tools, each with its schema → Test `test_catalog_offers_policy_tools` — DONE (`rustd/crates/afr_tools/src/catalog/tests.rs`)
- **Dimension 1.2** — A policy naming a tool without a handler refuses the lease before any model call → Test `test_unhosted_tool_refuses_lease` — DONE (`rustd/crates/afr_supervisor/src/lease_loop/admit_tests.rs`)
- **Dimension 1.3** — A model call to a name outside the policy is a tool error; the next turn runs → Test `test_unlisted_tool_refused_run_continues` — DONE (`rustd/crates/afr_agent/src/loop/turn_tests.rs`)
- **Dimension 1.4** — A supervisor-side call never touches the executor; a sandbox-side call crosses it → Test `test_router_sends_each_tool_to_its_runtime` — DONE (`rustd/crates/afr_agent/src/router/tests.rs`)
- **Dimension 1.5** — A lease with only supervisor-side tools starts no sandbox; one with `file_read` does → Test `test_lease_without_sandbox_tools_starts_no_sandbox` — DONE (`rustd/crates/afr_supervisor/src/lease_loop/admit_tests.rs`)

### §2 — The loop runs turns, and every call ends once

A run is turns until the model answers without a tool call, the context cap is reached, the lease ends, or the provider fails. Each call gets a call id from a counter starting at 1, one `tool_call_started` (`args_redacted` valid JSON, scrubbed, at most `ARGS_MAX_BYTES`) and one `tool_call_completed` (`status`, `output_head`, `output_tail`, `output_line_count`, `exit_code` only when a process ran). When the run ends for any reason, every open call is closed `interrupted`, live and in the trace. The trace honours `TRACE_MAX_CALLS` and `TRACE_MAX_BYTES`; full records post in batches of at most `DETAIL_POST_MAX_BYTES` to the tool-calls verb before the report. `tool_window` bounds the tool results kept in the window, `memory_checkpoint_every` triggers the mid-run memory push M210_001 §2 provides, and at `context_cap_tokens` the loop asks for a final answer with no tools offered. Answer and reasoning text stream as `fleet_response_chunk` with `text_kind`, `stream_start` and a contiguous `stream_seq`.

- **Dimension 2.1** — A turn with two tool calls runs both, feeds the results back and ends on the answer → Test `test_loop_runs_tool_calls_until_answer` — DONE (`rustd/crates/afr_agent/src/loop/tests.rs`)
- **Dimension 2.2** — Every call emits one start and one completion with the same call id, numbered from 1 → Test `test_loop_emits_one_start_one_end_per_call` — DONE (`rustd/crates/afr_agent/src/loop/tests.rs`)
- **Dimension 2.3** — A kill, a timeout or a provider failure closes each open call `interrupted` exactly once, in the frames and the trace → Test `test_run_end_interrupts_open_calls_once` — DONE (`rustd/crates/afr_agent/src/loop/tests.rs`)
- **Dimension 2.4** — The 201st call is counted as omitted, and past the byte cap a call keeps its row without edges → Test `test_trace_bounds_come_from_afd_wire` — DONE (`rustd/crates/afr_agent/src/trace/tests.rs`)
- **Dimension 2.5** — Records post before the report in bounded batches; a failed post leaves the report untouched → Test `test_records_post_before_report`
- **Dimension 2.6** — `tool_window` and `memory_checkpoint_every` are honoured; at `context_cap_tokens` the next request offers no tools → Test `test_loop_honours_context_budget`
- **Dimension 2.7** — Answer and reasoning stream as chunks with their kind and a contiguous sequence → Test `test_answer_streams_as_chunks` — DONE (`rustd/crates/afr_agent/src/loop/turn_tests.rs`)

### §3 — Three providers, one trait, the key stays in the supervisor

`ExecutionPolicy.provider` selects the wire with `api_key` through a provider registry (`assets/providers.json`): every name the Zig runner speaks over these wires keeps its base URL, `custom:<url>` is chat at that `https` URL, no redirect is followed, and any other name refuses the lease at admission. Everything the model is sent passes the scrub. `rig-core` speaks each tool-calling wire and streams it over the runner's own transport; the reasoning a provider signs goes back with its turn, and a turn cut at the output limit runs none of its calls. `web_search` is sent as the provider's hosted tool spec on Responses and Messages; on compatible chat the call is a tool error with a code. A 429 or 5xx is retried honouring `Retry-After` under a fixed ceiling; a 4xx ends the run `FleetError` with `failure_detail` naming the status and no `failure_reason`; a lost connection ends it `TransportLoss`. Usage sums across turns into `input_tokens`, `cached_input_tokens` and `output_tokens`.

- **Dimension 3.1** — Each provider completes a tool-calling turn against a fake server speaking its wire → Test `test_each_provider_drives_a_tool_turn` — DONE (`rustd/crates/afr_providers/tests/providers.rs`)
- **Dimension 3.2** — A 429 with `Retry-After: 1` is retried once and succeeds; a 401 ends the run with the status in `failure_detail` → Test `test_provider_retry_honours_retry_after` — DONE (`rustd/crates/afr_providers/tests/providers.rs`)
- **Dimension 3.3** — The key appears in no log line, frame, trace, record or prompt of a run → Test `test_api_key_never_leaves_the_supervisor` — DONE (`rustd/crates/afr_providers/tests/providers.rs`)
- **Dimension 3.4** — Three turns' usage sums into the report's three counts → Test `test_report_sums_token_usage` — DONE (`rustd/crates/afr_agent/src/loop/budget_tests.rs`)
- **Dimension 3.5** — `web_search` reaches Responses and Messages as a hosted spec and is a coded tool error on compatible chat → Test `test_web_search_is_a_hosted_spec` — DONE (`rustd/crates/afr_providers/tests/providers.rs`)
- **Dimension 3.6** — A call from a turn stopped at the output limit is answered `output_limit_reached` and its handler never runs → Test `a_call_cut_at_the_output_limit_is_answered_and_never_run` — DONE (`rustd/crates/afr_providers/tests/providers/ends.rs`)
- **Dimension 3.7** — Thinking a provider signed goes back ahead of the call it preceded on the next turn → Test `a_turns_signed_thinking_goes_back_ahead_of_its_call` — DONE (`rustd/crates/afr_providers/tests/providers/ends.rs`)

### §4 — Supervisor-side tools under the lease's policy

`http_request` refuses a host outside `network_policy.allow` before any connection; under `read_only` it admits `GET` and `HEAD`, plus `POST` only to `read_post_paths`; for a host with `http_origin_policies` rules, a request matching no rule is refused, and a matching rule's `json_fields` are checked against the body. A `${secrets.NAME.FIELD}` placeholder is substituted only in the `Authorization` header at send time, from `secrets_map` or by minting a `Mintable` through the daemon's verb once per lease and reusing it until `expires_at_ms`; `${secrets.NAME.host}` may also name the URL's host, and a credential is sent only to its own host (`src/runner/engine/runtime/credential_placement.zig`); any other placeholder in a URL, or one in a body, refuses the call. Responses are capped at 1 MiB. `web_fetch` is `GET` only, carries no placeholder and no credential, and is capped the same way. `pushover` posts to its one host with the `pushover` secret's `token` and `user` fields taken from `secrets_map` by the handler itself, never from the model. Every output edge, record and frame passes the scrub: known secret values become `«secret:NAME»`. The four memory tools operate on the hydrated store whose deltas M210_001 §2 pushes; the daemon only upserts (`rustd/crates/afd_fleet/src/memory/sql.rs:32-38`), so `memory_forget` holds for the run. `calculator` is pure. `update_plan` records the model's plan steps as a tool outcome the thread renders.

- **Dimension 4.1** — A host outside the allowlist is refused with no connection attempted → Test `test_http_request_refuses_unlisted_host`
- **Dimension 4.2** — Under `read_only`, `POST` is refused except to a listed path → Test `test_http_request_read_only_admits_listed_posts`
- **Dimension 4.3** — A request outside the origin's rules is refused; one inside, with its locked fields, passes → Test `test_http_request_enforces_origin_rules`
- **Dimension 4.4** — A placeholder lands in `Authorization`, or as `.host` in the URL; any other in a URL or body refuses the call → Test `test_placeholder_substituted_only_in_authorization`
- **Dimension 4.5** — A mintable credential is minted once per lease and reused until expiry → Test `test_mintable_credential_minted_once`
- **Dimension 4.6** — A secret value in a response body is masked in the frame, the trace and the record → Test `test_secret_values_masked_in_outputs` — DONE (`rustd/crates/afr_agent/src/loop/budget_tests.rs`)
- **Dimension 4.7** — `web_fetch` is `GET` only, refuses a placeholder, and caps the body → Test `test_web_fetch_is_get_only_and_credential_free`
- **Dimension 4.8** — `pushover` sends the secret's two fields from `secrets_map` and refuses a model-supplied token → Test `test_pushover_takes_credentials_from_secrets_map`
- **Dimension 4.9** — The four memory tools round-trip through the hydrated store; stores reach the push, a forget holds for the run → Test `test_memory_tools_round_trip_through_push` — DONE (`rustd/crates/afr_agent/src/loop/memory_tests.rs`)
- **Dimension 4.10** — `update_plan` records steps with their status as a succeeded call → Test `test_update_plan_records_steps` — DONE (`rustd/crates/afr_tools/src/plan/tests.rs`)

### §5 — Repository writes stay inside the daemon's rules, and the prompt says which branch

For a write-bound lease the prompt carries a trusted repair context: the one repository, the repair branch read from the locked `refs` rule, and the trusted base from `repository_binding`; a read-bound lease gets none. The runner evaluates the write rules as compiled: a ref may be created only at `refs/heads/<branch>`, and a pull request only with that head, that base and `draft: true`.

- **Dimension 5.1** — A write-bound lease's prompt names repository, branch and base; a read-bound one has no such block → Test `test_prompt_carries_trusted_repair_context`
- **Dimension 5.2** — Creating any other ref is refused; the named one passes → Test `test_write_rules_admit_only_repair_branch`
- **Dimension 5.3** — A pull request with `draft: false` or another base is refused → Test `test_write_rules_require_draft_against_base`

### §6 — The four bundles run end to end against the real daemon

The integration lane installs each fixture bundle through the seed (`rustd/crates/afd_fleet_runtime/tests/support/mod.rs` already reads the corpus), delivers an event through the lease verb, and runs the Rust runner against fake GitHub (Actions runs, jobs, the job-log 302, the Git Data API, pulls, reviews), fake Grafana (datasources, the Loki proxy, annotations, alerts), fake Elasticsearch (`_query`) and a fake model whose scripted turns ask for the calls each SKILL.md names.

- **Dimension 6.1** — `ci-responder` reports a diagnosis citing the run, the failed job and step, the job-log gap the 302 leaves, a Loki line and the commits checked, and stores a memory → Test `test_ci_responder_triages_a_failed_run`
- **Dimension 6.2** — `ci-repairer` reconciles, re-reads the head, writes blob, tree, commit, the named ref and one draft; a second run finding the draft ends with its link and writes nothing → Test `test_ci_repairer_opens_one_draft_pull_request`
- **Dimension 6.3** — `github-pr-reviewer` posts one `COMMENT` review with a path and line per finding; an operator steer posts nothing → Test `test_pr_reviewer_posts_one_review`
- **Dimension 6.4** — `incident-repairer` ships five writes after a full read, and ends diagnosis-only with no write after a partial one → Test `test_incident_repairer_ships_or_stops`
- **Dimension 6.5** — Each run's trace and a call's full record are readable through the tenant routes → Test `test_bundle_run_trace_is_readable`

### §7 — The incident repairer stops promising a gate the daemon retired

`incident-repairer/TRIGGER.md` and `SKILL.md` say every wake parks behind a repository-write approval card. The daemon retired that card: the standing integration grant authorises the write (`rustd/crates/afd_fleet_runtime/src/config/raw/predicate.rs:97-99`). The prose changes to the grant; the corpus test pins that no fixture names the card.

- **Dimension 7.1** — No fixture bundle names an approval card for a repository write → Test `test_fixture_corpus_names_no_retired_gate`

## Interfaces

```
Catalog entry: { name, runtime: Supervisor | Sandbox | Provider }
Tool (trait):  name() · schema() · runtime() · call(arguments, ctx) → ToolOutput { text, exit_code?, error_code? }
AgentEngine (M210_001 trait) + admit(policy) → Needs { sandbox } ← afr_agent::Loop { provider, catalog ∩ policy.tools, budget }
Connect (trait): admit(policy) · connect(lease) → Box<dyn Provider>; Provider: stream(request) → chunks { text(kind) | tool_call(id, name, arguments) | usage | end { replay, cut } }; web_search rides as the wire's own spec
Trusted repair context (rendered into the system prompt, write-bound leases only)
  ## Trusted repair context
  repository: agentsfleet/linkwarden
  repair branch: agentsfleet-repair/<daemon-named>
  trusted base: dev
Frames afd_wire::activity · trace afd_wire::tool_trace (200 calls, 64 KiB, edges ≤ 1 KiB) · records ≤ DETAIL_POST_MAX_BYTES per post
Mint:   POST /v1/runners/me/credentials/mint { lease_id, integration, scope? } → { token, expires_at_ms }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Policy names an unhosted tool | Bundle lists a tool this runner lacks | Lease refused with a logged code before any model call (Dimension 1.2) |
| Provider 429 / 5xx | Rate limit, upstream fault | Retried under the ceiling honouring `Retry-After`; then `FleetError` naming the status (Dimension 3.2) |
| Provider connection lost mid-stream | Network | Open calls closed `interrupted`; run ends `TransportLoss` (Dimensions 2.3, 3.2) |
| Tool refused: unlisted name, or host, method, path or field outside the rules | Model fault, injection | Tool error with a code to the model; no connection; run continues (Dimensions 1.3, 4.1–4.3) |
| Placeholder outside `Authorization` | Prompt-injected or careless request | Call refused before send (Dimension 4.4) |
| Mint refused | Grant missing or revoked | The daemon's refusal body reaches the model unchanged; no retry (Dimension 4.5) |
| Context cap reached | Long run | Final answer requested with no tools; the answer names what was not done (Dimension 2.6) |
| Record post fails | Daemon unavailable | Report posts regardless; "show all" has nothing for those calls (Dimension 2.5) |
| Secret in a response body | Upstream echoes a token | Masked in frame, trace and record (Dimension 4.6) |

## Invariants

1. The model key and every minted token exist only in the supervisor; none reaches a sandbox, a frame, a trace, a record, a log or a prompt (Dimensions 3.3, 4.4).
2. A placeholder is substituted only in the `Authorization` header, at send time (Dimension 4.4).
3. Every call ends exactly once, as `succeeded`, `failed` or `interrupted` (Dimensions 2.2, 2.3).
4. No connection opens to a host outside the allowlist, and no ref other than the daemon-named one can be created (Dimensions 4.1, 5.2).
5. The model is offered the policy's tools and nothing else, and a tool the runner cannot host refuses the lease rather than running without it (Dimensions 1.1, 1.2).
6. The runner never exceeds a bound `afd_wire` declares; the daemon re-checks and is the authority (Dimension 2.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `tool_refused_not_hosted` (runner log, error; the Zig bridge's spelling, `docs/LOGGING_STANDARD.md` §8A), and `provider_refused_not_hosted` for a provider | ops | A policy names a tool without a handler, or a provider with no wire | lease id, tool `name`, `error_code` | No policy content beyond the name | `test_unhosted_tool_refuses_lease` |
| `provider_retry` (runner log, warn) | ops | A provider call is retried | lease id, provider, status, attempt, wait | No request or response body | `test_provider_retry_honours_retry_after` |
| `tool_refused` (runner log, info) | ops | A call fails the policy | lease id, call id, tool, `error_code` | No arguments, no host beyond its name | `test_http_request_refuses_unlisted_host` |
| `credential_minted` (runner log, info) | ops | A mintable is minted | lease id, integration, `expires_at_ms` | Never the token | `test_mintable_credential_minted_once` |
| `context_cap_reached` (runner log, info) | ops | The loop stops offering tools | lease id, turn count, tokens | No prompt content | `test_loop_honours_context_budget` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_catalog_offers_policy_tools` | tools `[http_request, memory_recall]` → 2 specs, each with its schema, nothing else |
| 1.2 | unit | `test_unhosted_tool_refuses_lease` | tools `[http_request, browser]` before the sandbox-side workstream → lease refused, 0 model calls |
| 1.3 | unit | `test_unlisted_tool_refused_run_continues` | model calls `shell`, policy lacks it → tool error, next turn runs |
| 1.4 | unit | `test_router_sends_each_tool_to_its_runtime` | `calculator` → executor untouched; stub `file_read` → one executor call |
| 1.5 | unit | `test_lease_without_sandbox_tools_starts_no_sandbox` | tools `[http_request]` → engine never prepared; `[file_read]` → prepared |
| 2.1 | unit | `test_loop_runs_tool_calls_until_answer` | scripted provider: 2 calls then answer → 2 tool outputs fed back, answer returned |
| 2.2 | unit | `test_loop_emits_one_start_one_end_per_call` | 3 calls → ids `1`,`2`,`3`, one start and one completion each |
| 2.3 | unit | `test_run_end_interrupts_open_calls_once` | kill during call 2 → one `interrupted` frame and trace row for it, none for call 1 |
| 2.4 | unit | `test_trace_bounds_come_from_afd_wire` | 201 calls → `omitted_call_count` 1; 70 KiB of edges → later rows edge-less |
| 2.5 | integration | `test_records_post_before_report` | 3 calls → records stored before the report settles; daemon 503 on records → report still 2xx |
| 2.6 | unit | `test_loop_honours_context_budget` | `tool_window` 2 after 4 calls → 2 results in window; cap hit → next request has no tools |
| 2.7 | unit | `test_answer_streams_as_chunks` | reasoning then answer → `text_kind` each, `stream_seq` 0..n contiguous |
| 3.1 | unit | `test_each_provider_drives_a_tool_turn` | fake Messages, Responses, chat servers → one tool call parsed and answered each |
| 3.2 | unit | `test_provider_retry_honours_retry_after` | 429 + `Retry-After: 1` then 200 → success; 401 → `FleetError`, detail `401` |
| 3.3 | unit | `test_api_key_never_leaves_the_supervisor` | key `sk-test-…` → absent from logs captured through the runner's own filter at `trace`, frames, trace, records, prompt |
| 3.4 | unit | `test_report_sums_token_usage` | turns (10,2,5),(20,4,6),(5,0,1) → 35, 6, 12 |
| 3.5 | unit | `test_web_search_is_a_hosted_spec` | Responses, Messages → request carries the hosted spec; chat → tool error with code |
| 3.6 | unit | `a_call_cut_at_the_output_limit_is_answered_and_never_run` | Messages `stop_reason: max_tokens` with a call → result `[output_limit_reached] …`, call `Failed`, run answers |
| 3.7 | unit | `a_turns_signed_thinking_goes_back_ahead_of_its_call` | Messages thinking + `signature_delta`, then a call → next request's assistant turn is `thinking` (same text and signature), then `tool_use` |
| 4.1 | unit | `test_http_request_refuses_unlisted_host` | `evil.example` → refused, 0 connections on the fake |
| 4.2 | unit | `test_http_request_read_only_admits_listed_posts` | `POST /_query` listed → sent; `POST /other` → refused |
| 4.3 | unit | `test_http_request_enforces_origin_rules` | `POST …/git/refs` with other `ref` → refused; locked `ref` → sent |
| 4.4 | unit | `test_placeholder_substituted_only_in_authorization` | header → real token on the wire; `${secrets.grafana.host}` URL → its host; `.token` in URL or body → refused |
| 4.5 | integration | `test_mintable_credential_minted_once` | 3 calls needing `github` → 1 mint; expired → re-mint |
| 4.6 | unit | `test_secret_values_masked_in_outputs` | body echoes the token → `«secret:github.token»` in edge, trace, record |
| 4.7 | unit | `test_web_fetch_is_get_only_and_credential_free` | `POST` → refused; `${secrets…}` in header → refused; 2 MiB body → cut at 1 MiB |
| 4.8 | unit | `test_pushover_takes_credentials_from_secrets_map` | call with `{message}` → body carries `token`,`user` from the map; call passing `token` → refused |
| 4.9 | unit | `test_memory_tools_round_trip_through_push` | hydrated `incident:41`; store, recall, forget, list through the loop → recall reads the store first, list lacks the forgotten key, the push carries exactly the stored delta |
| 4.10 | unit | `test_update_plan_records_steps` | 3 steps → succeeded call whose output lists each step and status |
| 5.1 | unit | `test_prompt_carries_trusted_repair_context` | write binding → block with repository, branch, base; read binding → no block |
| 5.2 | unit | `test_write_rules_admit_only_repair_branch` | `refs/heads/main` → refused; the locked ref → sent |
| 5.3 | unit | `test_write_rules_require_draft_against_base` | `draft:false` → refused; base `main` with base `dev` locked → refused |
| 6.1 | integration | `test_ci_responder_triages_a_failed_run` | fixture bundle + fakes → report cites run, job, step, log gap, Loki line; memory row stored |
| 6.2 | integration | `test_ci_repairer_opens_one_draft_pull_request` | run 1 → blob, tree, commit, ref, draft on the named branch; run 2 → link, 0 writes |
| 6.3 | integration | `test_pr_reviewer_posts_one_review` | `pull_request` digest → 1 review, `COMMENT`, N comments; steer → 0 reviews |
| 6.4 | integration | `test_incident_repairer_ships_or_stops` | full reads → 5 writes, 1 draft; `contents` 403 → 0 writes, diagnosis-only |
| 6.5 | integration | `test_bundle_run_trace_is_readable` | settled event → `tool_calls` rows; one call id → full record |
| 7.1 | unit | `test_fixture_corpus_names_no_retired_gate` | corpus grep "approval card" → 0 matches |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The four bundles run end to end against the real daemon (§6) | `make test-integration-rustd && grep -cE "fn test_(ci_responder_triages_a_failed_run\|ci_repairer_opens_one_draft_pull_request\|pr_reviewer_posts_one_review\|incident_repairer_ships_or_stops)\(" rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | 4 | P0 | |
| R2 | The catalog, loop, providers and tools hold their invariants (§1–§5) | `cargo test --manifest-path rustd/Cargo.toml -p afr_agent -p afr_providers -p afr_tools` | exit 0 | P0 | |
| R3 | The key never leaves the supervisor (§3) | `cargo test --manifest-path rustd/Cargo.toml -p afr_providers test_api_key_never_leaves_the_supervisor` | exit 0 | P0 | |
| R4 | No fixture promises the retired card (§7) | `grep -rc "approval card" tests/fixtures/fleetbundle \| grep -v ':0$'` | no output | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from this Files Changed table or M210_003's | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file (`dispatch/write_any.md` §LENGTH GATE's extensions) | `git diff --name-only origin/main...HEAD \| grep -E '\.(zig\|jsx?\|tsx?\|py\|rs\|go\|sh\|sql\|ya?ml)$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted. The scripted engine stays as M210_001's lane driver; the Zig runner stays until the cutover milestone deletes it with its lanes.

## Out of Scope

- Sandbox-side handlers (`shell`, `exec_command`, `write_stdin`, `git`, the seven `file_*` tools, `apply_patch`, `image`, the three browser tools, Chromium in the toolbox) — M211_001; until it lands a policy naming one refuses the lease (§1) and the Zig runner keeps serving those fleets. `delegate` and `spawn` as nested loops — M211's follow-up. `message`, `cron_*` and `schedule` — M212_001.
- `propose_change`, the supervisor's push, the Codex and Claude Code engines (the outage toolkit); workspaces in R2, artifacts, the allowlist inside the sandbox (later specs); the cutover and the published tools page (M213). Every handler here runs in the supervisor and needs none of them.
- `stage_chunk_threshold` and the checkpoint's reader: `context_json` has no reader today (`docs/architecture/runner_execution.md` §"Workspace between leases"); the loop carries the checkpoint fields through unchanged.
- Rendering — M209_002 draws what this emits; `update_plan`, `web_fetch` and `pushover` need their copy-map verbs added to M209_002 §3 when it opens.

---
## Product Clarity (authoring record)

1. **Successful user moment** — Someone posts a failed run in the channel, mentions the CI responder, and reads a cited diagnosis whose every `http_request` shows in the thread as a green `Requested GET …` with its first lines; they type the repairer line and a draft Pull Request link comes back, with the one `POST …/pulls` call visible and nothing else written.
2. **Preserved user behaviour** — Bundles keep their tool names, placeholders, allowlists and budgets; the Zig runner keeps serving until the cutover; the daemon's verbs are unchanged; a bundle naming a tool this runner cannot host yet is refused loudly, as today.
3. **Optimal-way check** — Codex's four parts (catalog, model choice, router, runtime per handler) with the loop in the supervisor, so the model key never crosses a boundary and the sandbox-side tools plug into a router that already exists; the rules are evaluated as the daemon compiled them, never re-derived.
4. **Rebuild-vs-iterate** — Rebuild, by Indy's decision: "The port is a fresh port".
5. **What we build** — The catalog, the router, the loop, three providers with hosted specs, nine supervisor-side tools with the three policy duties, the trusted repair context, four bundle proofs, one fixture fix.
6. **What we do NOT build** — Sandbox-side handlers, the two runner verbs, nested loops, pushes, coding engines, workspaces, the cutover (see Out of Scope).
7. **Fit with existing features** — Consumes M209_001's frame fields and M209_003's verb from the start; must not change a bundle's observable behaviour beyond fixing the retired-card prose.
8. **Surface order** — API first; the thread renders it through M209_002; the published tools page changes at cutover.
9. **Dashboard restraint** — N/A — no user surface; a refused call is a red cell with its code, never a silent skip.
10. **Confused-user next step** — N/A — no user surface; an operator reads `tool_refused` or `lease_refused_unhosted_tool` with its `error_code`, and the thread's cell shows the same code.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the harness core and every supervisor-side tool as one workstream, proven by the four bundles the product ships, so the first Rust-run fleets are the ones people already use, and every later tool workstream adds handlers to a catalog and router that already exist.
- **Alternatives considered:** embedding `codex-core` as the loop (rejected: Responses API only, no Anthropic, weekly surface churn); running the whole harness inside the sandbox (rejected by Indy: "Supervisor loop, sandbox tools"); forwarding `http_request` to a proxy that substitutes placeholders (rejected for now: `docs/architecture/runner_execution.md` §Credentials records it as the later move, behind three seams); re-deriving the write rules in the runner from the binding (rejected: the daemon compiles them, and two compilers drift).
- **Patch-vs-refactor verdict:** this is a **refactor** because it replaces the runner's execution core; the daemon changes by nothing but two fixture files.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "i need all the tools, since cron has a scheduler i think, web_search and so that the sandbox decides to use or how will codex harness decide to use to those tools? must be build on the sandbox, the sandbox isnt just a sandbox but a harness that decide to operate like codex so that is critical to realize the fleets i plan to use." Chose "Supervisor loop, sandbox tools" (Codex's shape: the loop outside, the code-running tools inside) and "Yes, runner verb onto daemon schedules". Earlier the same day: "Subscripts and API keys, but support Codex as first class citizen"; "I would go for 1, with the focus on move to 2 later" (scoped tokens now, the placeholder-swap proxy later); "The port is a fresh port, since we always have the last binary with us and running." Oct 03, 2026, on the refactor: "Do a large refactor by thinking on abstraction based on purpose", "ensure you follow SOLID principles and keep it simple", "with testable, performant, concurrent and scalable approach/abstraction", and Exonum "must be used". On runner telemetry Indy chose "Add spans now" with "clearly define so we know its AFR emitted of a specific runner, assume that we would have runners farm", and chose "Heartbeat reply carries it" for the runner's id; the agent then found `GET /v1/runners/me` already returns that id from the daemon and took it instead, to change no wire. On named providers: "How does ironclaw does this? Can we review and steal from here", with Tarzy's "Keep refusal for unknown names, but support every Zig named provider whose wire is already implemented" and "treat `custom:<url>` as the dangerous one"; then "Do not re invent our own. provider layer", then "yes use rig_core". rig 0.43 traces every request and reply whole at `trace`, before the scrub (`rig-core-0.43.0/src/providers/internal/mod.rs:38`); `test_api_key_never_leaves_the_supervisor` caught the key in a captured line. Indy chose "Cap rig at warn (recommended)". One rig warning repeats a provider's error message (`providers/internal/openai_chat_completions_compatible.rs:34`); asked again, Indy gave no answer within 60 seconds and the agent took the recommended option, warn with that one module off. An entry names a rig dialect only where its base URL's path equals rig's, the host free to differ (moonshot's `.cn`); perplexity stays a gateway; zai, minimax and xiaomimimo are not added, a public change `registry/tests.rs` still pins refused. rig ends the stream on a call whose arguments are not JSON, so that call goes out with its raw text and the turn ends on what arrived before it. Oct 03, on §4: Indy chose "Obey the host list" for `web_fetch`, "Share via afd_core" for the private-address rules and "Move to afd_wire" for the memory bounds, and asked for smaller crates (M-SMALLER-CRATES), the latest crate versions, a workspace crate before any new one, and no mutex; `memory_recall` matches a key substring, the ceiling `docs/architecture/direction.md` sets.
- **Trusted repair context** — Both repairer bundles require it and nothing renders it: `grep -rn -i "trusted repair context" rustd src` finds only the fixtures. The branch reaches a lease inside the locked rules (`rustd/crates/afd_fleet/src/lease/deliver.rs:48-59`, `rustd/crates/afd_gate/src/policy/egress/write.rs:54-90`), so §5 renders it from those rules rather than inventing a second source.
- **Retired card; unhosted tools** — `rustd/crates/afd_fleet_runtime/src/config/raw/predicate.rs:97-99`: "the standing integration grant now authorises a repository write, no daemon path raises that card"; `incident-repairer` promises that card in its wake rule, and §7 corrects the fixture rather than the daemon. The unhosted-tool disposition is carried from `src/runner/engine/tool_bridge.zig`: a bundle asking for a tool it may not have is misconfigured or hostile, so the lease is refused, never trimmed.
- **Agent defaults** — a retry ceiling of three attempts; the 1 MiB response cap from the published tools page; the context-cap behaviour (final answer, no tools) in place of a failure; the trusted repair context's three lines; `pushover` reading its two fields from `secrets_map` rather than through the placeholder grammar, because its API takes them in the body.
- **Metrics review** — No analytics or funnel playbook update required: no user surface; five operator log events added.
- **Skill-chain outcomes** — pending.
- **Deferrals** — Dimension 6.3's review post: no `afd_gate` rule admits `POST …/pulls/{number}/reviews`, so the post is refused today. > Indy (2026-10-03 15:32): "you just tell me crap, increase the scope, so the refusal of review must be ignored for now. If that blocks the spec to move to done, then record Indys wording and move it to done." — context: no review-post rule in this spec; a refused 6.3 does not block `done/`. Folded workstream: M210_003 (the Rust unit lane in parallel shards, Indy's Oct 03 request), on this branch and in this Pull Request; its own Discovery says why it is not §8 here.
