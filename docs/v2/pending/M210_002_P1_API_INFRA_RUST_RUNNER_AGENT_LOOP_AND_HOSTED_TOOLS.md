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

# M210_002: The Rust runner's agent loop drives a model through today's hosted tools under the lease's policy, ends every call once with its outcome, and runs the four reference bundles end to end against the real daemon

**Prototype:** v2.0.0
**Milestone:** M210
**Workstream:** 002
**Date:** Oct 02, 2026
**Status:** PENDING
**Priority:** P1 — this is the workstream after which the fleets of record (the Continuous Integration (CI) responder and repairer, the Pull Request reviewer, the incident repairer) run on the Rust runner
**Categories:** API, INFRA
**Batch:** B2 — after M210_001 is on `main`; the milestone's follow-up Pull Request. The cutover that deletes the Zig runner is its own milestone
**Branch:** pending — set at CHORE(open)
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M210_001 (supervisor duties, the `AgentEngine` trait, the executor) · M209_001 (`afd_wire::tool_trace`, the outcome fields on `tool_call_completed`) · M209_003 (`afd_wire::tool_detail`, the tool-calls verb)
**Provenance:** LLM-drafted (Claude Fable 5.1, Oct 02, 2026) from a source trace on `main` and the four bundles under `tests/fixtures/fleetbundle/`; Codex at `~/Projects/oss/rs/codex` `2e5fea64e`, IronClaw at `~/Projects/oss/rs/ironclaw` `b0b999d96`, ZeroClaw at `~/Projects/oss/zeroclaw` `74362c2d6`
**Canonical architecture:** `docs/architecture/runner_execution.md` §Process model, §Crates, §Credentials, §Repository writes, §"What comes from where"; `docs/architecture/runner_fleet.md` §"Live activity (the SSE tail)", §"Memory continuity — durable fleet memory rides the trusted plane", §"Egress model — outbound is the only network surface"

---

## Overview

**Goal (testable):** `test_ci_responder_triages_a_failed_run` — against the real `agentsfleetd` and fake GitHub, Grafana and model upstreams, the `ci-responder` bundle's lease runs the loop: the model asks for `http_request` reads and `memory_recall`, every call is checked against the lease's origin rules, its `${secrets.*}` placeholder is substituted in the `Authorization` header at send time, each call emits one `tool_call_started` and one `tool_call_completed` with its outcome, the full outputs post under the fence before the report, and the report carries the diagnosis, the three token counts and a bounded trace. The same lane proves `ci-repairer` opens exactly one draft Pull Request on the daemon-named branch with the write token never in the prompt.
**Problem:** M210_001 ends with a scripted engine. The fleets of record declare only `http_request` and `memory_*` (`tests/fixtures/fleetbundle/*/TRIGGER.md`), and three duties the Zig tool bridge owns have no Rust home: placeholder substitution at the HTTPS boundary, the per-host allowlist and origin rules (`afd_wire::policy::HttpOriginPolicy`), and the repository-write rules the daemon compiles (`rustd/crates/afd_gate/src/policy/egress/write.rs:54-90`). Both repairer bundles also require a "trusted repair context" naming the daemon-issued branch (`ci-repairer/SKILL.md` step 1, `incident-repairer/SKILL.md` "GitHub reconciliation"), and no code renders one: the branch reaches a lease only inside the locked request rules (`rustd/crates/afd_fleet/src/lease/deliver.rs:48-59`), and `instructions` is the SKILL.md body alone (`rustd/crates/afd_fleet_runtime/src/instructions.rs:34`).
**Solution summary:** `afr_agent` runs turns against a provider, routes tool calls, scrubs and bounds every output, emits the M209 frames and trace, and closes open calls once. `afr_providers` speaks Anthropic Messages, OpenAI Responses and OpenAI-compatible chat, chosen by `ExecutionPolicy.provider`. `afr_tools` hosts the thirteen tools the chat renders (M209_002 §3–§4) in the supervisor, with `http_request` enforcing the network policy, the origin rules and the write rules, substituting placeholders only in `Authorization`, and minting on demand through `POST /v1/runners/me/credentials/mint`. The prompt gains a trusted repair context rendered from the repository binding and the locked branch. A lease that declares no file tool starts no sandbox. The integration lane installs the four fixture bundles and drives each against fakes.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(runner): agent loop, providers and hosted tools; the four bundles run on Rust
- **Intent (one sentence):** A fleet written for today's tools runs on the Rust runner with the same policy enforced, every call visible in the thread, and the model key and write token never inside a sandbox or a prompt.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/runner_execution.md` — §Process model (who holds what), §Credentials, §Repository writes; the Decisions table is binding.
2. `rustd/crates/afd_wire/src/policy.rs` — `ExecutionPolicy`, `NetworkPolicy`, `HttpOriginPolicy`, `HttpRequestRule`, `Mintable`, `RepositoryBinding`, `ContextBudget`: the whole of what a run may do.
3. `rustd/crates/afd_gate/src/policy/egress/write.rs` and `rustd/crates/afd_gate/src/policy/repair.rs` — the locked ref and pull-request rules, and why the branch name is exact.
4. `rustd/crates/afd_wire/src/activity.rs`, `rustd/crates/afd_wire/src/report.rs`, `rustd/crates/afd_wire/src/credentials.rs` — the frames, the report with `Outcome` and `FailureClass`, the mint verb.
5. `tests/fixtures/fleetbundle/ci-responder/SKILL.md`, `tests/fixtures/fleetbundle/ci-repairer/SKILL.md`, `tests/fixtures/fleetbundle/github-pr-reviewer/SKILL.md`, `tests/fixtures/fleetbundle/incident-repairer/SKILL.md` — what the loop must make possible, step by step.
6. https://github.com/openai/codex/tree/2e5fea64eefcaa19f48458b2386011b619f69c70/codex-rs — `core/src/codex.rs` (the turn loop and how a turn ends), `protocol/src/protocol.rs` (`ExecCommandEndEvent` and the typed end state per call).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afr_agent/src/` (`loop.rs`, `turn.rs`, `router.rs`, `prompt.rs`, `context.rs`, `events.rs`, `trace.rs`, `records.rs`, `scrub.rs`, `error.rs`) | CREATE | The loop, its prompt, the context budget, frames, trace, full records, the secret scrub |
| `rustd/crates/afr_providers/src/` (`provider.rs`, `anthropic.rs`, `openai_responses.rs`, `openai_chat.rs`, `retry.rs`, `usage.rs`, `error.rs`) | CREATE | One trait, three wires, bounded retry, usage split |
| `rustd/crates/afr_tools/src/` (`registry.rs`, `http_request.rs`, `network.rs`, `origin_rules.rs`, `placeholders.rs`, `mint.rs`, `memory.rs`, `files.rs`, `calculator.rs`, `error.rs`) | CREATE | The thirteen hosted tools and the three policy duties |
| `rustd/crates/afr_supervisor/src/lease_loop.rs`, `rustd/crates/afr_supervisor/src/engine_select.rs` | EDIT / CREATE | Run the real loop; start no sandbox for a lease without file tools |
| `rustd/crates/afr_agent/tests/support/scripted.rs` | EDIT | The scripted engine stays for M210_001's lane |
| `rustd/crates/agentsfleetd/tests/support/fake_github.rs`, `fake_grafana.rs`, `fake_elastic.rs`, `fake_model.rs`, `bundle_install.rs` | CREATE | Fakes speaking the upstream shapes the bundles read; installing a fixture bundle through the seed |
| `rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | CREATE | The four bundles end to end (`#[ignore]`d, run by `make test-integration-rustd`) |
| `rustd/crates/afr_agent/tests/`, `rustd/crates/afr_providers/tests/`, `rustd/crates/afr_tools/tests/` | CREATE | Unit proofs per crate |
| `tests/fixtures/fleetbundle/incident-repairer/TRIGGER.md`, `tests/fixtures/fleetbundle/incident-repairer/SKILL.md`, `rustd/crates/afd_fleet_runtime/tests/frontmatter_corpus.rs` | EDIT | The bundle stops promising the retired approval card; the corpus test pins it |
| `docs/architecture/runner_execution.md` | EDIT | Landed at authoring: the prompt carries the trusted repair context |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (tool names, placeholder grammar, retry bounds, scrub patterns and prompt headings are constants), TFX (tests import the bounds from `afd_wire`), OWN (one owner per provider stream, tool call and record batch), FLS (drain a provider stream and a tool response on every exit path), TIM (retry ceilings, call timeouts and the context cap are explicit), ECL (`Retry-After` and 5xx retry; 4xx ends the call), NTP (every upstream body narrowed at its parse boundary), OBS, ERR-RS, TST-NAM, TCF, NDC.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — one `ErrorKind` per crate through `afd_core::error_shell!`; a tool refusal carries its code to the model, never a bare string.
- `docs/LOGGING_STANDARD.md` — never log a key, a token, a prompt, a tool argument or an output; log ids, codes and counts.
- `docs/architecture/runner_fleet.md` §"Egress model — outbound is the only network surface" — the allowlist is enforced before a connection, never after.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR | yes | `error_shell!` and `error_lifts!` only; the `Result` alias is the one hand-written line |
| UFS / LOGGING / MILESTONE-ID | yes | Constants per concern; scoped events with `error_code`; no milestone identifiers in source |
| File & Function Length (≤350/≤50/≤70) | yes | One concern per file across the three crates; the providers never share a request builder |
| Architecture consult | yes | `runner_execution.md` is the design; the trusted repair context sentence lands with this spec |
| CI/CD edit guard | no | No workflow edit; the bundle lane rides `make test-integration-rustd` |

## Prior-Art / Reference Implementations

- **Reference:** Codex `core/src/codex.rs` (`2e5fea64e`) — the turn loop: a turn is model output plus the tool calls it asks for, fed back until the model answers; every call ends with a typed status. Taken as design, not code.
- **Reference:** ZeroClaw `~/Projects/oss/zeroclaw/crates/zeroclaw-providers` — Anthropic streaming with prompt caching, the OpenAI-compatible chat wire, and the input/cached/output usage split. Mined, with `rustd` error shapes.
- **Reference:** IronClaw `~/Projects/oss/rs/ironclaw/crates/ironclaw_llm` (retry honouring `Retry-After`, a ceiling, no failover inside one lease) and `ironclaw_safety` (leak scan on tool output and model input). Ported.
- **Reference:** `rustd/crates/afd_credential/src/credential/github/request.rs` — the mint client the supervisor already has; `afr_tools::mint` wraps it, caching per lease until `expires_at_ms`.
- **Reference:** `rustd/crates/afd_gate/src/policy/egress/` — the rules are compiled there; the runner only evaluates `HttpRequestRule` as written, exact path and locked fields.

## Sections (implementation slices)

### §1 — The loop runs turns, and every call ends once

A run is turns until the model answers without a tool call, the context cap is reached, the lease ends, or the provider fails. Each tool call gets a call id from a counter starting at 1, one `tool_call_started` (`args_redacted` valid JSON, scrubbed, at most `ARGS_MAX_BYTES`) and one `tool_call_completed` (`status`, `output_head`, `output_tail`, `output_line_count`, `exit_code` only when a process ran). When the run ends for any reason, every open call is closed `interrupted`, live and in the trace. The trace honours `TRACE_MAX_CALLS` and `TRACE_MAX_BYTES`; full records post in batches of at most `DETAIL_POST_MAX_BYTES` to the tool-calls verb before the report. `tool_window` bounds the tool results kept in the window, `memory_checkpoint_every` triggers the mid-run memory push M210_001 §2 provides, and at `context_cap_tokens` the loop asks for a final answer with no tools offered. Answer and reasoning text stream as `fleet_response_chunk` with `text_kind`, `stream_start` and a contiguous `stream_seq`.

- **Dimension 1.1** — A turn with two tool calls runs both, feeds the results back and ends on the answer → Test `test_loop_runs_tool_calls_until_answer`
- **Dimension 1.2** — Every call emits one start and one completion with the same call id, numbered from 1 → Test `test_loop_emits_one_start_one_end_per_call`
- **Dimension 1.3** — A kill, a timeout or a provider failure closes each open call `interrupted` exactly once, in the frames and the trace → Test `test_run_end_interrupts_open_calls_once`
- **Dimension 1.4** — The 201st call is counted as omitted, and past the byte cap a call keeps its row without edges → Test `test_trace_bounds_come_from_afd_wire`
- **Dimension 1.5** — Records post before the report in bounded batches; a failed post leaves the report untouched → Test `test_records_post_before_report`
- **Dimension 1.6** — `tool_window` and `memory_checkpoint_every` are honoured; at `context_cap_tokens` the next request offers no tools → Test `test_loop_honours_context_budget`
- **Dimension 1.7** — Answer and reasoning stream as chunks with their kind and a contiguous sequence → Test `test_answer_streams_as_chunks`

### §2 — Three providers, one trait, the key stays in the supervisor

`ExecutionPolicy.provider` selects Anthropic Messages, OpenAI Responses or OpenAI-compatible chat, dialled at `inference_host` or `base_url` with `api_key`. Each speaks its own tool-calling wire and streams. A 429 or 5xx is retried honouring `Retry-After` under a fixed ceiling; a 4xx ends the run `FleetError` with `failure_detail` naming the status and no `failure_reason`; a lost connection ends it `TransportLoss`. Usage sums across turns into `input_tokens`, `cached_input_tokens` and `output_tokens`.

- **Dimension 2.1** — Each provider completes a tool-calling turn against a fake server speaking its wire → Test `test_each_provider_drives_a_tool_turn`
- **Dimension 2.2** — A 429 with `Retry-After: 1` is retried once and succeeds; a 401 ends the run with the status in `failure_detail` → Test `test_provider_retry_honours_retry_after`
- **Dimension 2.3** — The key appears in no log line, frame, trace, record or prompt of a run → Test `test_api_key_never_leaves_the_supervisor`
- **Dimension 2.4** — Three turns' usage sums into the report's three counts → Test `test_report_sums_token_usage`

### §3 — Hosted tools in the supervisor, under the lease's policy

The registry offers the model only the names in `ExecutionPolicy.tools`; a call to any other name is refused with a tool error and the run continues. `http_request` refuses a host outside `network_policy.allow` before any connection; under `read_only` it admits `GET` and `HEAD`, plus `POST` only to `read_post_paths`; for a host with `http_origin_policies` rules, a request matching no rule is refused, and a matching rule's `json_fields` are checked against the body. A `${secrets.NAME.FIELD}` placeholder is substituted only in the `Authorization` header at send time, from `secrets_map` or by minting a `Mintable` through the daemon's verb once per lease and reusing it until `expires_at_ms`; a placeholder in a URL or body refuses the call. Responses are capped at 1 MiB. Every output edge, record and frame passes the scrub: known secret values become `«secret:NAME»`. `memory_store`, `memory_recall`, `memory_list` and `memory_forget` operate on the hydrated store whose deltas M210_001 §2 pushes. `calculator` is pure. The seven `file_*` tools go to the executor's `fs/*` calls under `/workspace`; a lease whose tools include none of them starts no sandbox.

- **Dimension 3.1** — A host outside the allowlist is refused with no connection attempted → Test `test_http_request_refuses_unlisted_host`
- **Dimension 3.2** — Under `read_only`, `POST` is refused except to a listed path → Test `test_http_request_read_only_admits_listed_posts`
- **Dimension 3.3** — A request outside the origin's rules is refused; one inside, with its locked fields, passes → Test `test_http_request_enforces_origin_rules`
- **Dimension 3.4** — A placeholder lands in `Authorization` only; one in a URL or body refuses the call → Test `test_placeholder_substituted_only_in_authorization`
- **Dimension 3.5** — A mintable credential is minted once per lease and reused until expiry → Test `test_mintable_credential_minted_once`
- **Dimension 3.6** — A secret value in a response body is masked in the frame, the trace and the record → Test `test_secret_values_masked_in_outputs`
- **Dimension 3.7** — The four memory tools round-trip through the hydrated store and the push → Test `test_memory_tools_round_trip_through_push`
- **Dimension 3.8** — A lease without file tools starts no sandbox; one with `file_read` does → Test `test_lease_without_file_tools_starts_no_sandbox`
- **Dimension 3.9** — A tool the policy does not list is refused and the run continues → Test `test_unlisted_tool_refused_run_continues`

### §4 — Repository writes stay inside the daemon's rules, and the prompt says which branch

For a write-bound lease the prompt carries a trusted repair context: the one repository, the repair branch read from the locked `refs` rule, and the trusted base from `repository_binding`; a read-bound lease gets none. The runner evaluates the write rules as compiled: a ref may be created only at `refs/heads/<branch>`, and a pull request only with that head, that base and `draft: true`.

- **Dimension 4.1** — A write-bound lease's prompt names repository, branch and base; a read-bound one has no such block → Test `test_prompt_carries_trusted_repair_context`
- **Dimension 4.2** — Creating any other ref is refused; the named one passes → Test `test_write_rules_admit_only_repair_branch`
- **Dimension 4.3** — A pull request with `draft: false` or another base is refused → Test `test_write_rules_require_draft_against_base`

### §5 — The four bundles run end to end against the real daemon

The integration lane installs each fixture bundle through the seed (`rustd/crates/afd_fleet_runtime/tests/support/mod.rs` already reads the corpus), delivers an event through the lease verb, and runs the Rust runner against fake GitHub (Actions runs, jobs, the job-log 302, the Git Data API, pulls, reviews), fake Grafana (datasources, the Loki proxy, annotations, alerts), fake Elasticsearch (`_query`) and a fake model whose scripted turns ask for the calls each SKILL.md names.

- **Dimension 5.1** — `ci-responder` reports a diagnosis citing the run, the failed job and step, the job-log gap the 302 leaves, a Loki line and the commits checked, and stores a memory → Test `test_ci_responder_triages_a_failed_run`
- **Dimension 5.2** — `ci-repairer` reconciles, re-reads the head, writes blob, tree, commit, the named ref and one draft; a second run finding the draft ends with its link and writes nothing → Test `test_ci_repairer_opens_one_draft_pull_request`
- **Dimension 5.3** — `github-pr-reviewer` posts one `COMMENT` review with a path and line per finding; an operator steer posts nothing → Test `test_pr_reviewer_posts_one_review`
- **Dimension 5.4** — `incident-repairer` ships five writes after a full read, and ends diagnosis-only with no write after a partial one → Test `test_incident_repairer_ships_or_stops`
- **Dimension 5.5** — Each run's trace and a call's full record are readable through the tenant routes → Test `test_bundle_run_trace_is_readable`

### §6 — The incident repairer stops promising a gate the daemon retired

`incident-repairer/TRIGGER.md` and `SKILL.md` say every wake parks behind a repository-write approval card. The daemon retired that card: the standing integration grant authorises the write (`rustd/crates/afd_fleet_runtime/src/config/raw/predicate.rs:97-99`). The prose changes to the grant; the corpus test pins that no fixture names the card.

- **Dimension 6.1** — No fixture bundle names an approval card for a repository write → Test `test_fixture_corpus_names_no_retired_gate`

## Interfaces

```
AgentEngine (M210_001 trait) ← afr_agent::Loop { provider, tools, budget }
Provider (trait): stream(request) → chunks { text(kind) | tool_call(id, name, arguments) | usage }
Tool (trait): name() · schema() · call(arguments, ctx) → ToolOutput { text, exit_code?, error_code? }

Trusted repair context (rendered into the system prompt, write-bound leases only)
  ## Trusted repair context
  repository: agentsfleet/linkwarden
  repair branch: agentsfleet-repair/<daemon-named>
  trusted base: dev

Frames: afd_wire::activity (started · progress · completed with outcome · chunk)
Trace:  afd_wire::tool_trace (TRACE_MAX_CALLS 200 · TRACE_MAX_BYTES 65536 · edges ≤ 1 KiB)
Records: POST /v1/runners/me/leases/{lease_id}/tool-calls, batches ≤ DETAIL_POST_MAX_BYTES
Mint:   POST /v1/runners/me/credentials/mint { lease_id, integration, scope? } → { token, expires_at_ms }
Placeholder grammar: ${secrets.<name>.<field>} — Authorization header only
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Provider 429 / 5xx | Rate limit, upstream fault | Retried under the ceiling honouring `Retry-After`; then `FleetError` naming the status (Dimension 2.2) |
| Provider connection lost mid-stream | Network | Open calls closed `interrupted`; run ends `TransportLoss` (Dimensions 1.3, 2.2) |
| Tool refused by policy | Host, method, path or field outside the rules | Tool error with a code to the model; no connection; run continues (Dimensions 3.1–3.3, 3.9) |
| Placeholder outside `Authorization` | Prompt-injected or careless request | Call refused before send (Dimension 3.4) |
| Mint refused | Grant missing or revoked | The daemon's refusal body reaches the model unchanged; no retry (Dimension 3.5) |
| Context cap reached | Long run | Final answer requested with no tools; the answer names what was not done (Dimension 1.6) |
| Run killed mid-call | Lease end, kill, timeout | Each open call `interrupted` once (Dimension 1.3) |
| Record post fails | Daemon unavailable | Report posts regardless; "show all" has nothing for those calls (Dimension 1.5) |
| Secret in a response body | Upstream echoes a token | Masked in frame, trace and record (Dimension 3.6) |

## Invariants

1. The model key and every minted token exist only in the supervisor; none reaches a sandbox, a frame, a trace, a record, a log or a prompt (Dimensions 2.3, 3.4).
2. A placeholder is substituted only in the `Authorization` header, at send time (Dimension 3.4).
3. Every call ends exactly once, as `succeeded`, `failed` or `interrupted` (Dimensions 1.2, 1.3).
4. No connection opens to a host outside the allowlist, and no ref other than the daemon-named one can be created (Dimensions 3.1, 4.2).
5. The runner never exceeds a bound `afd_wire` declares; the daemon re-checks and is the authority (Dimension 1.4).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `provider_retry` (runner log, warn) | ops | A provider call is retried | lease id, provider, status, attempt | No request or response body | `test_provider_retry_honours_retry_after` |
| `tool_refused` (runner log, info) | ops | A call fails the policy | lease id, call id, tool, `error_code` | No arguments, no host beyond its name | `test_http_request_refuses_unlisted_host` |
| `credential_minted` (runner log, info) | ops | A mintable is minted | lease id, integration, `expires_at_ms` | Never the token | `test_mintable_credential_minted_once` |
| `context_cap_reached` (runner log, info) | ops | The loop stops offering tools | lease id, turn count, tokens | No prompt content | `test_loop_honours_context_budget` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_loop_runs_tool_calls_until_answer` | scripted provider: 2 calls then answer → 2 tool outputs fed back, answer returned |
| 1.2 | unit | `test_loop_emits_one_start_one_end_per_call` | 3 calls → ids `1`,`2`,`3`, one start and one completion each |
| 1.3 | unit | `test_run_end_interrupts_open_calls_once` | kill during call 2 → one `interrupted` frame and trace row for it, none for call 1 |
| 1.4 | unit | `test_trace_bounds_come_from_afd_wire` | 201 calls → `omitted_call_count` 1; 70 KiB of edges → later rows edge-less |
| 1.5 | integration | `test_records_post_before_report` | 3 calls → records stored before the report settles; daemon 503 on records → report still 2xx |
| 1.6 | unit | `test_loop_honours_context_budget` | `tool_window` 2 after 4 calls → 2 results in window; cap hit → next request has no tools |
| 1.7 | unit | `test_answer_streams_as_chunks` | reasoning then answer → `text_kind` each, `stream_seq` 0..n contiguous |
| 2.1 | unit | `test_each_provider_drives_a_tool_turn` | fake Messages, Responses, chat servers → one tool call parsed and answered each |
| 2.2 | unit | `test_provider_retry_honours_retry_after` | 429 + `Retry-After: 1` then 200 → success; 401 → `FleetError`, detail `401` |
| 2.3 | unit | `test_api_key_never_leaves_the_supervisor` | key `sk-test-…` → absent from captured logs, frames, trace, records, prompt |
| 2.4 | unit | `test_report_sums_token_usage` | turns (10,2,5),(20,4,6),(5,0,1) → 35, 6, 12 |
| 3.1 | unit | `test_http_request_refuses_unlisted_host` | `evil.example` → refused, 0 connections on the fake |
| 3.2 | unit | `test_http_request_read_only_admits_listed_posts` | `POST /_query` listed → sent; `POST /other` → refused |
| 3.3 | unit | `test_http_request_enforces_origin_rules` | `POST …/git/refs` with other `ref` → refused; locked `ref` → sent |
| 3.4 | unit | `test_placeholder_substituted_only_in_authorization` | header → real token on the wire; URL placeholder → refused |
| 3.5 | integration | `test_mintable_credential_minted_once` | 3 calls needing `github` → 1 mint; expired → re-mint |
| 3.6 | unit | `test_secret_values_masked_in_outputs` | body echoes the token → `«secret:github.token»` in edge, trace, record |
| 3.7 | integration | `test_memory_tools_round_trip_through_push` | store, recall, forget → push carries the deltas; next hydrate reflects them |
| 3.8 | unit | `test_lease_without_file_tools_starts_no_sandbox` | tools `[http_request]` → engine never prepared; `[file_read]` → prepared |
| 3.9 | unit | `test_unlisted_tool_refused_run_continues` | model calls `shell` → tool error, next turn runs |
| 4.1 | unit | `test_prompt_carries_trusted_repair_context` | write binding → block with repository, branch, base; read binding → no block |
| 4.2 | unit | `test_write_rules_admit_only_repair_branch` | `refs/heads/main` → refused; the locked ref → sent |
| 4.3 | unit | `test_write_rules_require_draft_against_base` | `draft:false` → refused; base `main` with base `dev` locked → refused |
| 5.1 | integration | `test_ci_responder_triages_a_failed_run` | fixture bundle + fakes → report cites run, job, step, log gap, Loki line; memory row stored |
| 5.2 | integration | `test_ci_repairer_opens_one_draft_pull_request` | run 1 → blob, tree, commit, ref, draft on the named branch; run 2 → link, 0 writes |
| 5.3 | integration | `test_pr_reviewer_posts_one_review` | `pull_request` digest → 1 review, `COMMENT`, N comments; steer → 0 reviews |
| 5.4 | integration | `test_incident_repairer_ships_or_stops` | full reads → 5 writes, 1 draft; `contents` 403 → 0 writes, diagnosis-only |
| 5.5 | integration | `test_bundle_run_trace_is_readable` | settled event → `tool_calls` rows; one call id → full record |
| 6.1 | unit | `test_fixture_corpus_names_no_retired_gate` | corpus grep "approval card" → 0 matches |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The four bundles run end to end against the real daemon (§5) | `make test-integration-rustd && grep -cE "fn test_(ci_responder_triages_a_failed_run\|ci_repairer_opens_one_draft_pull_request\|pr_reviewer_posts_one_review\|incident_repairer_ships_or_stops)\(" rustd/crates/agentsfleetd/tests/integration_rust_runner_bundles.rs` | 4 | P0 | |
| R2 | The loop, providers and tools hold their invariants (§1–§4) | `cargo test --manifest-path rustd/Cargo.toml -p afr_agent -p afr_providers -p afr_tools` | exit 0 | P0 | |
| R3 | The key never leaves the supervisor (§2) | `cargo test --manifest-path rustd/Cargo.toml -p afr_providers test_api_key_never_leaves_the_supervisor` | exit 0 | P0 | |
| R4 | No fixture promises the retired card (§6) | `grep -rc "approval card" tests/fixtures/fleetbundle \| grep -v ':0$'` | no output | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted. The scripted engine stays as M210_001's lane driver; the Zig runner stays until the cutover milestone deletes it with its lanes.

## Out of Scope

- The cutover: deploying the Rust runner, deleting `src/runner`, `src/lib`, the NullClaw fork, the Zig lanes and the CI image, and revising the published tools page (`fleets/tools.mdx` in the docs repository lists tools this runner does not carry: `web_search`, `web_fetch`, `pushover`, `cron_*`, `delegate`, `spawn`, `schedule`, `shell`, `git`, `image`, the browser tools, `message`) — the next milestone.
- `exec_command`, `apply_patch`, `propose_change` and the supervisor's push; the Codex and Claude Code engines — the outage toolkit spec.
- Workspaces in R2, artifacts, the network allowlist inside the sandbox — a later spec; `http_request` runs in the supervisor and needs none of them.
- `stage_chunk_threshold` and the checkpoint's reader: `context_json` has no reader today (`docs/architecture/runner_execution.md` §"Workspace between leases"); the loop carries the checkpoint fields through unchanged.
- Rendering — M209_002 draws what this emits.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Someone posts a failed run in the channel, mentions the CI responder, and reads a cited diagnosis whose every `http_request` shows in the thread as a green `Requested GET …` with its first lines; they type the repairer line and a draft Pull Request link comes back, with the one `POST …/pulls` call visible and nothing else written.
2. **Preserved user behaviour** — Bundles keep their tool names, placeholders, allowlists and budgets; the Zig runner keeps serving until the cutover; the daemon's verbs are unchanged.
3. **Optimal-way check** — The loop and the hosted tools live in the supervisor, so the four bundles need no sandbox at all and the model key never crosses a boundary; the rules are evaluated as the daemon compiled them, never re-derived.
4. **Rebuild-vs-iterate** — Rebuild, by Indy's decision: "The port is a fresh port".
5. **What we build** — The loop, three providers, thirteen hosted tools with the three policy duties, the trusted repair context, four bundle proofs, one fixture fix.
6. **What we do NOT build** — Processes, patches, pushes, coding engines, workspaces, the cutover (see Out of Scope).
7. **Fit with existing features** — Consumes M209_001's frame fields and M209_003's verb from the start; must not change a bundle's observable behaviour beyond fixing the retired-card prose.
8. **Surface order** — API first; the thread renders it through M209_002; the published tools page changes at cutover.
9. **Dashboard restraint** — N/A — no user surface; a refused call is a red cell with its code, never a silent skip.
10. **Confused-user next step** — N/A — no user surface; an operator reads `tool_refused` with its `error_code`, and the thread's cell shows the same code.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the loop, providers and tools as one workstream, proven by the four bundles the product ships, so the first Rust-run fleets are the ones people already use. The sandbox from M210_001 stays off this path: none of the four declares a file tool.
- **Alternatives considered:** embedding `codex-core` as the loop (rejected: Responses API only, no Anthropic, weekly surface churn); forwarding `http_request` to a proxy that substitutes placeholders (rejected for now: `docs/architecture/runner_execution.md` §Credentials records it as the later move, behind three seams); re-deriving the write rules in the runner from the binding (rejected: the daemon compiles them, and two compilers drift).
- **Patch-vs-refactor verdict:** this is a **refactor** because it replaces the runner's execution core; the daemon changes by nothing but two fixture files.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "Subscripts and API keys, but support Codex as first class citizen, anthropic seems to have issues on terms and requests via API keys" (OpenAI Responses is a first-class provider here; the Codex split engine is the outage toolkit's); "I would go for 1, with the focus on move to 2 later" (scoped tokens now, the placeholder-swap proxy later); "The port is a fresh port, since we always have the last binary with us and running." Indy asked the same day that the four bundles be reviewed "from a ticket from slack to triage to a repaire to response"; §5 is that review made executable.
- **Trusted repair context** — Both repairer bundles require it and nothing renders it: `grep -rn -i "trusted repair context" rustd src` finds only the fixtures. The branch reaches a lease inside the locked rules (`rustd/crates/afd_fleet/src/lease/deliver.rs:48-59`, `rustd/crates/afd_gate/src/policy/egress/write.rs:54-90`), so §4 renders it from those rules rather than inventing a second source.
- **Retired card** — `rustd/crates/afd_fleet_runtime/src/config/raw/predicate.rs:97-99`: "the standing integration grant now authorises a repository write, no daemon path raises that card". `incident-repairer` promises that card in its wake rule; §6 corrects the fixture rather than the daemon.
- **Agent defaults** — a retry ceiling of three attempts; the 1 MiB response cap from the published tools page; the context-cap behaviour (final answer, no tools) in place of a failure; the trusted repair context's three lines.
- **Metrics review** — No analytics or funnel playbook update required: no user surface; four operator log events added.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
