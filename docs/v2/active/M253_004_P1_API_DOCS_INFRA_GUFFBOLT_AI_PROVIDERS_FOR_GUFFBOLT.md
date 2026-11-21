<!--

SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M253_004: guffbolt-ai gives guffbolt its own providers, so a plain chat needs no embedder model code

**Prototype:** v2.0.0

**Milestone:** M253

**Workstream:** 004

**Date:** Nov 21, 2026

**Status:** IN_PROGRESS

**Priority:** P1, guffbolt is published for others to build on, and today nobody can run it without writing a provider

**Categories:** API, DOCS, INFRA

**Batch:** B4, after M253_003

**Depends on:** M253_002, which made guffbolt a standalone crate; M253_003, whose §8 set the limits rule this crate inherits and which edits the same provider crates

**Provenance:** LLM-drafted (Claude Opus 5.5, Nov 21, 2026) from Indy's decision of Nov 21, 2026

**Canonical architecture:** `docs/architecture/runner_execution.md` §Crates; this spec adds `guffbolt-ai` beside `guffbolt` there

---

## Overview

**Goal (testable):** A 16-line program streams a MiniMax answer through `guffbolt::Chat` on a `guffbolt-ai` model read from `MINIMAX_API_KEY`.

**Problem:**
- **guffbolt cannot talk to a model.** A user implements `Model` by hand. The README's plain chat runs from `rustd/guffbolt/README.md:46` to `:215`, and its model is a stub (`README.md:93-120`).
- **The real providers are agentsfleet's.** Every wire lives in `af_inference` and `af_providers`, which depend on `af_wire`, `af_egress`, `af_crypto` and `af_observability` (`af_providers/Cargo.toml`).
- **pi already solved this shape.** `pi-agent-core` depends on `pi-ai` (`pi/packages/agent/package.json:26`). Its Quick Start is one provider registration and one `new Agent` (`pi/packages/agent/README.md:13-39`).

**Solution summary:**
- **A sibling crate.** `rustd/guffbolt-ai` holds the provider-facing types, the wires, the provider factories and a static model catalogue. It depends on no `af_*` crate and not on `guffbolt`.
- **guffbolt depends on it.** guffbolt re-exports the moved types, so every existing `guffbolt::…` path still compiles.
- **A ready chat.** `guffbolt::Chat` is a `Conversation` over any `Model`, as pi's `Agent` is over `streamFn`.
- **agentsfleet becomes an adapter.** `af_providers` builds a `guffbolt-ai` route from the run's saved `ModelInference`, injects the egress transport, and keeps its telemetry and codes. `af_inference` is deleted.

**Design flow:**
1. **Register.** The user builds `Models::new().with(providers::minimax())`. The factory carries the provider id, the Anthropic Messages wire, the base `https://api.minimax.io/anthropic`, the key variable `MINIMAX_API_KEY` and its catalogue rows.
2. **Connect.** `models.connect("minimax", id)` finds the row, reads the key from the process environment, and builds a `Route`. The route is the API, base URL, rig dialect, output limit and context window. `Connected` holds the route, the key and the default transport.
3. **Chat.** `Chat::new(model, instructions).prompt(text)` pushes the message and calls `guffbolt::drive` with `Plain`.
4. **Call.** Each step calls `Model::measure`, then `Model::stream`. The wire encodes the request through rig, sends it through the `Send` transport, and decodes the stream into `Chunk`s.
5. **Answer.** The loop keeps the step in its `Transcript` and returns `Ending::Answered(text)`.
6. **agentsfleet.** The runner's `Connector::connect` maps `ExecutionPolicy` to a `Route` with `Route::new`, passes its guarded transport and the run's key, and wraps `Connected` in `af_providers::Turns`.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(m253): guffbolt-ai, the providers guffbolt runs on
- **Intent (one sentence):** Someone who adds `guffbolt` and `guffbolt-ai` to a project chats with a hosted model in a few lines, and agentsfleet runs exactly as before.
- **Authoring handshake:**
  - **What Indy asked for:** guffbolt "self sufficient", built the way pi-agent-core runs on pi-ai, as "Sibling crate, as pi-ai". Bounds stay only "if it has a reason".
  - **ASSUMPTIONS I'M MAKING:**
    1. **Types follow the dependency.** guffbolt depends on guffbolt-ai, so the types a provider reads and writes move down, and guffbolt re-exports them.
    2. **rig stays.** rig-core `=0.47.0` stays the wire engine inside guffbolt-ai, optional behind the `wire` feature (Q1).
    3. **No behaviour change for agentsfleet.** Every bound, retry, timeout and log line a run sees today keeps its value. Moved tests keep their assertions.
    4. **Key-based providers only.** OAuth, Bedrock and Vertex sign-in are out of scope.
    5. **`Chat` is in scope.** Without it, the 16-line goal cannot hold (Q7).
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/guffbolt/src/model.rs` — the seam that moves: `Model`, `Request`, `Chunk`, `End`, `Hosted`, `TextKind`, `Usage`.
2. `rustd/crates/harness/af_inference/src/lib.rs` — `Configured`: route checks (`:64-108`) and the rig wire per dialect (`:159-178`), the generic half of `Route`.
3. `rustd/crates/harness/af_providers/src/turn/mod.rs` — `Turns`, today's `impl Model` (`:188-207`), and the reopen rule.
4. `/Users/kishore/Projects/oss/js/pi/packages/ai/README.md` — Quick Start (`:108-241`), provider factories (`:249-265`), `builtinModels` (`:267-277`), environment keys (`:488-532`).
5. `docs/RUST_ERROR_STANDARD.md` — the `guffbolt` row (`:161`): a standalone crate hand-writes one error type per surface.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/guffbolt-ai/Cargo.toml` | CREATE | the crate, `version.workspace = true` as guffbolt (Q4), features per provider |
| `rustd/guffbolt-ai/README.md` | CREATE | Quick Start, providers, catalogue, transport; doctests |
| `rustd/guffbolt-ai/src/lib.rs`, `src/types/**` | CREATE | the seam types moved from guffbolt |
| `rustd/guffbolt-ai/src/wire/**` | CREATE | route, request, stream, reopen, usage, image, replay, count, budget, document, json, logs, retry, timeouts, transport |
| `rustd/guffbolt-ai/src/error.rs` | CREATE | the crate's one hand-written error type and its `Result` |
| `rustd/guffbolt-ai/src/models/**`, `src/providers/**` | CREATE | `Models`, `Provider`, `Spec`, key reading, one factory per provider |
| `rustd/guffbolt-ai/catalog/*.json` | CREATE | one catalogue file per provider, read with `include_str!` |
| `rustd/guffbolt-ai/src/testing/**`, `tests/**` | CREATE | the fake TLS provider behind `test-util`; the moved and new suites |
| `rustd/guffbolt/Cargo.toml` | EDIT | depends on `guffbolt-ai` with default features off |
| `rustd/guffbolt/src/lib.rs` | EDIT | re-exports the moved types; exports `Chat` |
| `rustd/guffbolt/src/model.rs` | DELETE | its types live in guffbolt-ai |
| `rustd/guffbolt/src/transcript/mod.rs`, `src/tool/mod.rs`, `src/tool/output.rs` | EDIT | `Call`, `Message`, `Replay`, `ToolSpec`, `Image`, `ImageKind` leave; `CallRef`, `Transcript`, tools and outputs stay |
| `rustd/guffbolt/src/chat/**` | CREATE | `Chat`, the ready `Conversation` |
| `rustd/guffbolt/examples/minimax_chat.rs`, `rustd/guffbolt/tests/public_api/{main,example}.rs` | CREATE, EDIT | the README program, and 4.4's suite running it |
| `rustd/guffbolt/README.md` | EDIT | the MiniMax chat first; the hand-written `Model` becomes "Your own provider" |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | the member and its dependency entry; `af_inference` removed; the layer note at `:18-21` names both standalone crates; the rig note at `:624-631` names no other harness |
| `rustd/crates/harness/af_inference/**` | DELETE | generic modules moved; agentsfleet remainder folded into `af_providers` (Q3) |
| `rustd/crates/harness/af_providers/**` | EDIT | the adapter over guffbolt-ai; generic modules and their tests leave |
| `rustd/crates/harness/af_providers/src/settings/**` | CREATE | `ModelInference` to `Route`, endpoint policy, decision checks, startup timeout and retry settings, `log_filter` re-export |
| `rustd/crates/daemon/afd_admin/{Cargo.toml,src/model_validation.rs,src/error/mod.rs,src/error/tests.rs}` | EDIT | validate through `af_providers::settings` |
| `rustd/crates/daemon/afd_credential/{Cargo.toml,src/provider/eligibility.rs,src/provider/registered/mod.rs}` | EDIT | same |
| `rustd/crates/daemon/main/{Cargo.toml,src/logs.rs,src/serve/observations.rs}` | EDIT | same; images from the mapped `Api` |
| `rustd/crates/runner/main/{Cargo.toml,src/main.rs,src/host.rs,src/config/mod.rs,src/config/tests.rs}` | EDIT | timeouts and the log filter from `af_providers::settings` |
| `rustd/crates/runner/afr_agent/{Cargo.toml,tests/providers/**}` | EDIT | imports; `ends.rs` names `af_wire::protocol::MESSAGE_BYTES_MAX` |
| `rustd/crates/protocol/af_wire/src/inference.rs`, `tests/inference_images.rs` | EDIT, DELETE | `carries_images` leaves for guffbolt-ai's `Api` |
| `.ast-grep/rules/rust/{rust-daemon-no-connector,rust-provider-no-harness-name}.yml`, `.ast-grep/rule-tests/*-test.yml` | CREATE | no daemon crate names `Connect` or `Connector`; provider code names no harness (lints, as tests may not read the repository) |
| `make/quality.mk` | EDIT | the standalone publish dry run covers both crates |
| `make/test-integration-rustd.mk`, `scripts/rustd_coverage.py`, `scripts/rustd_coverage_test.py` | EDIT | guffbolt-ai joins the guffbolt shard and its floor |
| `codecov.yml` | EDIT | the new path; a CI config edit, so Indy approves it in the PR |
| `docs/RUST_ERROR_STANDARD.md` | EDIT | a `guffbolt-ai` row; the `af_inference` and response-bound rows go |
| `docs/architecture/{runner_execution,billing_and_provider_keys,context_lifecycle,runner_checkpoint_recovery,shared_chat_verification,README}.md` | EDIT | crate names; `billing_and_provider_keys.md:628` still says rig 0.46.0 |
| `rustd/guffbolt/src/tools/env/{mod.rs,search.rs,local/mod.rs,local/search.rs}`, `src/tools/files/listing/{mod.rs,grep.rs}`, `tests/public_api/{files,process,network}/**` | EDIT, CREATE | §9: `Env::search`; `LocalEnv`'s search; `find` and `grep` over it |
| `rustd/crates/runner/afr_executor/{Cargo.toml,src/lib.rs,src/api/**,src/protocol/mod.rs,src/client/mod.rs,src/server/session.rs,src/server/session/search.rs,src/server/files/**,tests/search.rs,tests/executor_suite.rs}` | EDIT, CREATE | §9: `fs/search` |
| `rustd/crates/runner/afr_tools/src/generic/env/{mod.rs,lazy.rs,search.rs,search_tests.rs}` | EDIT, CREATE | §9: the sandbox's `Env::search` sends `fs/search` |
| `docs/v2/pending/M253_004_P1_API_DOCS_INFRA_GUFFBOLT_AI_PROVIDERS_FOR_GUFFBOLT.md` | CREATE | this spec, moved to `active/` then `done/` |
| `~/Projects/docs` changelog | EDIT, other repository | the entry for the new crate, on its own branch after merge |

**Word-boundary hits of `af_inference` outside `docs/v2`, from a repository-wide search:**
- **Production:** `af_providers/src/{turn/mod.rs,turn/progress.rs,connect/mod.rs,error/mod.rs,error/raise.rs,transport/mod.rs,testing/mod.rs}`, `afd_admin/src/{error/mod.rs,model_validation.rs}`, `afd_credential/src/provider/{eligibility.rs,registered/mod.rs}`, `daemon/main/src/{logs.rs,serve/observations.rs}`, `runner/main/src/{main.rs,host.rs,config/mod.rs}`, seven `Cargo.toml` files.
- **Tests:** `af_providers/src/{turn/tests.rs,transport/tests.rs,error/tests.rs}`, `afd_admin/src/error/tests.rs`, `runner/main/src/config/tests.rs`, `afr_agent/tests/providers/{privacy,logging,limits,limits_flow,ends}.rs`.
- **Docs:** the six architecture pages above and `docs/RUST_ERROR_STANDARD.md`.
- **Same spelling, other meaning:** `af_wire::decision::REPLY_MAX_BYTES` (64 KiB, `af_wire/src/decision/mod.rs:29`) is the decision reply bound and stays.

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE; guffbolt-ai follows guffbolt's hand-written shape, and `af_providers` lifts its error with `error_lifts!`
- **`.orly/LOCAL.md`** — No invented limits; Code names no other harness (moved comments at `af_providers/src/turn/mod.rs:9-13` and `request/mod.rs:5-8` cite IronClaw and are rewritten to state the fact); garde stays on agentsfleet's startup settings
- **`code.md`**, **`tests.md`** — a negative test per failure mode; moved tests keep their assertions
- **`domains/logging.md`** — the moved stream keeps its content-free records; no key, URL path or prompt in a log line
- **`domains/auth.md`** — the key reaches only its named host; agentsfleet's key still comes from the run's policy, never the environment
- **`docs.md`** — both READMEs and the architecture pages in short sentences

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` (clippy, rustfmt, cargo shear, standalone publish dry run) | yes | guffbolt-ai lists no unused dependency, and its dry run builds from crates.io only |
| `make test-unit-all` | yes | moved suites, the new suites and the README doctests |
| `make test-integration-rustd` | yes | the runner and daemon suites prove agentsfleet is unchanged |
| coverage floor (`RUSTD_GUFFBOLT_FLOOR`) | yes | guffbolt-ai is graded beside guffbolt |
| `docs.language` | yes | changed pages in short sentences |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | `Route` splits from its wires as `af_inference/src/lib.rs` (283 lines) already nears the cap |

## Prior-Art / Reference Implementations

- **Reference:** `pi/packages/ai` at `ee6f36496`. Mirror the shape. A provider is a factory with its id, wire, base URL, key variable and catalogue (`src/providers/minimax.ts:6-15`). `Models` holds providers and answers `getModel(provider, id)` (`src/models.ts:266-280`), and `builtinModels()` registers every one (`src/providers/all.ts:186`). pi-ai owns `Message`, `Context`, `Tool`, `Usage` and `Model`, and pi-agent-core imports them (`pi/packages/agent/src/types.ts:1-16`).
  - **Diverge:** Rust features replace pi's per-provider imports. Wires stay rig's, where pi writes its own (`src/api/anthropic-messages.ts`, `openai-completions.ts`, `openai-responses.ts`, `google-generative-ai.ts`). Catalogue rows are checked in, where pi generates them from models.dev (`catalog/README.md:3-8`).
- **Reference:** `pi/packages/agent/README.md:13-39` — `new Agent({ initialState: { systemPrompt, model }, streamFn: models.streamSimple })`, then `agent.prompt("Hello!")`. `guffbolt::Chat` mirrors it.
- **Reference:** `rustd/crates/harness/af_inference` and `af_providers` — the code that moves, with its tests.

## Sections (implementation slices)

### §0 — Three config fixes from M253_003's key audit, built first (Indy, Nov 22, 2026)

1. **No fleet budget maxima.** A fleet's own daily and monthly budget is the limit; `af_fleet_runtime` `MAX_DAILY_NANOS` / `MAX_MONTHLY_NANOS` and their check (authoring and readback) are deleted, and no `[billing]` maximum key exists.
2. **`[server] read_timeout_seconds = 300`.** The time a client gets to send a request's headers and body; a slow client past it is dropped. It never cuts a response, so an open live stream (SSE) is unaffected. Built on hyper's/tower-http's own timeout, not hand-rolled.
3. **The runner tells the executor its sizes.** The file read size, the command output kept per end and the frame size become runner TOML keys (`[tools] max_file_read_mib`, `[tools] output_edge_kib`, `[executor] max_frame_mib`); the runner passes them to the in-sandbox executor when it starts it, and the executor uses what it was given. The compiled constants survive only as defaults.

- Dimension 0.1 — DONE → Test `a_fleet_budget_has_no_ceiling_but_its_own` (unit): a definition with a $50,000 daily budget parses and is read back. Expected: accepted.
- Dimension 0.2 — DONE → Test `a_request_slower_than_the_read_timeout_is_dropped` and `a_live_stream_outlives_the_read_timeout` (integration). Expected: the slow request's connection closes; the stream keeps sending.
- Dimension 0.3 — DONE → Test `the_executor_uses_the_sizes_the_runner_sent` (unit): a runner config with a 1 MiB read size; the executor refuses a 2 MiB read. Expected: refused at 1 MiB.

### §1 — guffbolt-ai owns the provider-facing types, and guffbolt re-exports them

`rustd/guffbolt-ai` joins the workspace beside `rustd/guffbolt`. It depends on no workspace crate. Its default build compiles the types alone, on `serde`, `serde_json`, `bytes` and `futures-util`.

These move unchanged in shape, serde form and docs:
- **From `guffbolt/src/model.rs`:** the `Model` trait, `Request`, `Chunk`, `End`, `Hosted`, `TextKind` and `Usage`.
- **From `guffbolt/src/transcript/mod.rs`:** `Call`, `Message` and `Replay`.
- **From `guffbolt/src/tool/mod.rs` and `tool/output.rs`:** `ToolSpec`, `Image` and `ImageKind`.

`CallRef`, `Transcript`, `Describe`, `Tool`, `Tools`, `ToolOutput` and `ErrorCode` stay in guffbolt, because only the loop reads them. guffbolt's `lib.rs:98-104` re-exports the moved names from guffbolt-ai.

**Implementation default:** the `Model` trait moves too. A third-party provider crate then implements it with guffbolt-ai alone, as a pi provider needs only pi-ai.

- **Dimension 1.1** — guffbolt's paths still resolve → Test `guffbolt_reexports_every_seam_type` (unit, guffbolt): the README doctests and a test naming each of the 13 moved types by its `guffbolt::` path. Expected: compiles and passes. — DONE
- **Dimension 1.2** — a saved conversation still reads → Test `a_drive_state_saved_before_the_move_resumes` (unit, guffbolt): a `DriveState` JSON fixture written before the move, with an assistant replay and an image result. Expected: deserializes and round-trips byte-equal. — DONE
- **Dimension 1.3** — the types build without a wire → the `guffbolt-ai default build` step of `make lint-rustd` (lint; a test may not read a repository file): `cargo tree` over guffbolt-ai with no default features. Expected: only `bytes`, `futures-util`, `serde` and `serde_json`. — DONE
- **Dimension 1.4** — the crate stands alone → Test the `guffbolt standalone` step of `make lint-rustd` (unit): the publish dry run of guffbolt-ai, then guffbolt. Expected: both build from crates.io only. — DONE

### §2 — The wires move into guffbolt-ai behind the `wire` feature

The generic code of both crates moves with its tests, names unchanged. Each module's target:

| Module today | Kind | Target |
|---|---|---|
| `af_inference` `Configured` checks and wire choice (`lib.rs:41-280`) | GENERIC, split | `guffbolt_ai::Route`; the garde check, `provider_endpoint` and `accepts_endpoint` stay in `af_providers::settings` |
| `af_inference` `budget.rs`, `count/**`, `document.rs`, `json.rs`, `stream.rs` | GENERIC | `guffbolt_ai::wire` |
| `af_inference` `stream/trace.rs` | split | the observation hook moves; its `model_turn_*` lines with registry codes stay as `af_providers`' observer |
| `af_inference` `timeouts.rs` | split | `Timeouts` moves; garde `TimeoutSettings` stays in `af_providers::settings` |
| `af_inference` `transport.rs` | GENERIC | `guffbolt_ai::transport`; `REPLY_MAX_BYTES` becomes an option (§2 bounds) |
| `af_inference` `logs/mod.rs` | GENERIC | `guffbolt_ai::log_filter`, behind feature `log-filter`; only the crate pinning rig keeps rig's module list true |
| `af_inference` `endpoint.rs`, `error.rs` codes | AGENTSFLEET | `af_providers::settings` and `af_providers::error` |
| `af_providers` `request/**`, `usage/**`, `image_input.rs`, `turn/progress.rs`, the reopen rule in `turn/mod.rs` | GENERIC | `guffbolt_ai::wire` |
| `af_providers` `retry/**` | split | the loop and defaults move; garde `RetrySettings` stays |
| `af_providers` `testing/tls.rs` | GENERIC | `guffbolt_ai::testing`, behind `test-util` |
| `af_providers` `connect/**`, `transport/**`, `observe/**`, `error/**`, `decision/**`, `provider.rs`, `testing/{scripted,observed}.rs` | AGENTSFLEET | stay |

`guffbolt_ai::transport::Send` is the transport seam, renamed from `af_inference::transport::SendRequest`. It takes an `http::Request<Bytes>` and answers a streamed `http::Response`, never rig's types, so a rig upgrade does not break an embedder's transport. A send the embedder's guard refused answers `SendError::Refused`, which no retry or reopen repeats.

The default transport is a `reqwest` client behind feature `reqwest`. It follows no redirect, because the key must reach only the host the route names.

**Bounds, each with its reason or none:**
- **Reply size:** guffbolt-ai has no reply bound of its own. A search of `pi/packages/ai/src` found no response-byte cap, and Codex's hits for one are tool-call results. `Connected::with_reply_bound` takes one, and agentsfleet passes `af_wire::protocol::MESSAGE_BYTES_MAX` (32 MiB, `af_wire/src/protocol/mod.rs:29`), the runner frame.
- **Stream idle:** 300 s by default, Codex's `DEFAULT_STREAM_IDLE_TIMEOUT_MS` (`codex-rs/model-provider-info/src/lib.rs:73`).
- **Connect:** none of its own. Codex and pi set a connect timeout only for websockets (`model-provider-info/src/lib.rs:78`, `pi/packages/ai/src/types.ts:271`), so the operating system's applies. agentsfleet keeps its 10 s startup setting.
- **Retries:** 3 attempts and 3 openings, as the SDKs' 2 retries (`pi/packages/ai/src/types.ts:207-210`). A `Retry-After` past 60 s ends the turn, as pi's `maxRetryDelayMs` (`types.ts:212-218`).
- **Count call:** `COUNT_TIMEOUT` (10 s) and `COUNT_REPLY_BYTES` (16 KiB) (`af_inference/src/count/native.rs:12-13`) move unchanged, so runs behave as before (Q5).

- **Dimension 2.1** — the moved suites pass → Test the `af_inference` and `af_providers` generic suites (integration, fake TLS provider, unit lane), names and assertions unchanged. Expected: every one passes under guffbolt-ai. — DONE
- **Dimension 2.2** — no reply bound unless asked → Test `with_no_bound_a_reply_past_32_mib_is_read` (integration): a 33 MiB streamed reply. Expected: the turn ends answered. — DONE
- **Dimension 2.3** — the embedder's bound holds → Test `a_reply_past_the_bound_the_embedder_set_is_refused` (integration): bound 1 MiB, reply 1 MiB + 1 event. Expected: `Error::is_reply_too_large()`, the turn not reopened. — DONE
- **Dimension 2.4** — no redirect carries the key → Test `a_redirect_is_not_followed_with_the_key` (integration): the provider answers 307 to a second listener. Expected: the turn fails with status 307, and the second listener sees no request. — DONE
- **Dimension 2.5** — a refused host is not retried → Test `a_send_the_guard_refused_is_not_retried` (unit): a transport answering `SendError::Refused`. Expected: one send, the error kept. — DONE
- **Dimension 2.6** — a long wait is not slept → Test `a_retry_after_past_sixty_seconds_ends_the_turn` (integration, paused clock): 429 with `Retry-After: 120`. Expected: one send, and the turn ends with status 429. — DONE
- **Dimension 2.7** — a silent provider ends → Test `a_silent_provider_ends_at_the_idle_timeout` (integration, paused clock): headers, then silence. Expected: a deadline failure at 300 s. — DONE

### §3 — Providers, keys and a static catalogue

`guffbolt_ai::Models` is a collection of `Provider`s. `Models::new().with(provider)` registers one, `Models::builtin()` registers every provider compiled in, and `get(provider, id)` answers the catalogue row.

`connect(provider, id)` reads the key from the provider's variable, and `connect_with_key` takes one. `Route::new(api, base_url, dialect, max_output_tokens, context_tokens)` builds a model with no catalogue row, as agentsfleet does.

A `Spec` row holds the id, context window, maximum output tokens, and whether results may carry images. Each row is data in `catalog/<provider>.json`. The first rows come from rig 0.47's bundled catalogue (`rig-core-0.47.0/src/catalog/models.json`), which gives MiniMax-M3.1-Flash a 1,000,000-token window and 524,288 output tokens (Q2).

| Feature | Factory | Wire and rig dialect | Base URL | Key variable |
|---|---|---|---|---|
| `anthropic` | `providers::anthropic()` | Messages, `anthropic` | `https://api.anthropic.com` | `ANTHROPIC_API_KEY` |
| `openai` | `providers::openai()` | Responses, `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| `gemini` | `providers::gemini()` | Gemini native | `https://generativelanguage.googleapis.com` | `GEMINI_API_KEY` |
| `ollama` | `providers::ollama()` | Ollama native | `http://localhost:11434` | `OLLAMA_API_KEY`, optional |
| `openrouter` | `providers::openrouter()` | Chat, `openrouter` | rig's `OPENROUTER` | `OPENROUTER_API_KEY` |
| `fireworks` | `providers::fireworks()` | Chat, gateway | `https://api.fireworks.ai/inference/v1` | `FIREWORKS_API_KEY` |
| `minimax` | `providers::minimax()` | Messages, `minimax` | `https://api.minimax.io/anthropic` | `MINIMAX_API_KEY` |
| `deepseek`, `groq`, `mistral`, `xai`, `together` | one each | Chat, rig's dialect of that name | rig's | `DEEPSEEK_API_KEY`, `GROQ_API_KEY`, `MISTRAL_API_KEY`, `XAI_API_KEY`, `TOGETHER_API_KEY` |

Every provider feature turns on `wire`. Feature `all` turns on every provider. rig-core 0.47 has no per-provider features (its `Cargo.toml` lists `derive`, `rustls`, `reqwest` and a few others), so a provider feature compiles out only our factory and its rows.

af_inference reaches every rig dialect today through a saved dialect name; `Route::new` keeps that. pi adds Cerebras, Baseten, NVIDIA, Vercel AI Gateway, Cloudflare, Bedrock, Vertex, Copilot and OAuth providers; none is in this spec (Q6).

- **Dimension 3.1** — a registered provider answers → Test `a_chat_on_minimax_answers_through_the_messages_wire` (integration, fake TLS provider): the minimax factory pointed at the fake host with `with_base_url`. Expected: `POST /v1/messages` with `x-api-key` (rig's gateway auth, `anthropic/wire.rs:141,464`), and the answer streamed back. — DONE
- **Dimension 3.2** — a missing key names its variable → Test `connect_without_the_key_names_its_variable` (unit): `MINIMAX_API_KEY` unset. Expected: an error whose text names `MINIMAX_API_KEY` and holds no value. — DONE
- **Dimension 3.3** — an unknown model is refused → Test `an_unknown_model_id_is_refused_with_its_provider` (unit): `connect("minimax", "no-such-model")`. Expected: an error naming both, and no request sent. — DONE
- **Dimension 3.4** — an unregistered provider is refused → Test `an_unregistered_provider_is_refused` (unit): `Models::new().connect("minimax", …)`. Expected: an error naming `minimax`. — DONE
- **Dimension 3.5** — the catalogue holds together → Test `every_catalogue_row_parses_and_fits` (unit): every compiled-in file. Expected: each row parses, its output limit is at most its window, and no id repeats within a provider. — DONE
- **Dimension 3.6** — a row's image flag reaches the loop → Test `accepts_images_follows_the_row_and_the_wire` (unit): MiniMax-M3.1-Flash on Messages, and a Chat route. Expected: true, then false. — DONE

### §4 — `guffbolt::Chat` runs a plain chat in a few lines

`Chat` is a `Conversation` over any `M: Model`. It holds the model, the instructions, an optional `Tools` set lent nothing, a `Transcript`, a stop token and an inbox. `prompt(text)` joins the text and drives the loop with `Plain`. A chat that needs more, such as a lent context or an observer, writes its own `Conversation` as today.

The target user code, which both READMEs carry as a `no_run` doctest:

```rust
use guffbolt::{Chat, Ending};
use guffbolt_ai::{Models, providers};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), guffbolt_ai::Error> {
    // MINIMAX_API_KEY is read from the environment.
    let models = Models::new().with(providers::minimax());
    let model = models.connect("minimax", "MiniMax-M3.1-Flash")?;

    let mut chat = Chat::new(model, "Answer in one sentence.");
    match chat.prompt("Which crate parses TOML in Rust?").await {
        Ending::Answered(text) => println!("{text}"),
        Ending::Failed(why) => eprintln!("the model failed: {why}"),
        Ending::Stopped | Ending::Waiting(_) => {}
    }
    Ok(())
}
```

The id is `MiniMax-M3.1-Flash`, MiniMax's own name (platform.minimax.io text-generation guide); there is no plain `MiniMax-M3.1`.

- **Dimension 4.1** — a chat answers → Test `a_chat_answers_one_prompt` (unit, guffbolt): a scripted `Model` answering "toml". Expected: `Ending::Answered("toml")`, and the transcript holds both messages. — DONE
- **Dimension 4.2** — a chat runs its tools → Test `a_chat_runs_a_tool_and_answers_with_its_output` (unit): a model that calls `note`, then answers. Expected: the tool ran once, and the answer follows its result. — DONE
- **Dimension 4.3** — a model failure ends the chat → Test `a_chat_whose_model_fails_ends_failed` (unit): a model whose stream errs. Expected: `Ending::Failed` carrying that error. — DONE
- **Dimension 4.4** — the README program runs on the real path → Test `the_minimax_example_answers_against_a_fake_provider` (e2e, subprocess, `guffbolt/tests/public_api/example.rs`): the `minimax_chat` binary `cargo test` built, run with a fake provider's base URL as its argument and `MINIMAX_API_KEY` a fixture key. Expected: exit 0 and the fake's answer on stdout. — DONE
- **Dimension 4.5** — live MiniMax answers → Test `live_minimax_chat` (manual): Indy runs `examples/minimax_chat` with his `MINIMAX_API_KEY`, from `rustd/`: `MINIMAX_API_KEY="$(op read <the MiniMax key's reference>)" cargo run --locked -p guffbolt --example minimax_chat` (no argument, so it reaches `https://api.minimax.io/anthropic`). Expected: one sentence printed; the command and its output land in the PR Session Notes, the key in neither.
- 4.4 and 4.5 follow §3: the example connects through `Models` and `providers::minimax()`, which §3 builds.

### §5 — agentsfleet runs on guffbolt-ai with no behaviour change

`af_providers` becomes the adapter. `Connector::connect` (`connect/mod.rs:108-124`) maps `af_wire::InferenceApi` to `guffbolt_ai::Api` in one `match`. It builds `Route::new` from the run's `ModelInference` and context window, then calls `connect_with_key` with the policy's key and its guarded `Transport`. The reply bound is `MESSAGE_BYTES_MAX`.

`Turns` wraps `Connected` and implements `guffbolt::Model<Error = af_providers::Error>`. It maps errors to registry codes, records `af_observability` measurements and writes the `run_id` log lines. The billing meter in `afr_agent` reads `Chunk::Usage` as before.

**Implementation default:** `af_inference` is deleted and its agentsfleet remainder becomes `af_providers::settings`. One crate then owns what a saved `ModelInference` means. The daemon's three users switch to it, and `layers.rs` proves they name no `Connect` (Q3).

`af_wire::InferenceApi::carries_images` goes. guffbolt-ai's `Api` is the one source, and `af_wire/tests/inference_images.rs` moves there.

- **Dimension 5.1** — runs are unchanged → Test the `afr_agent` `tests/providers` suite and the `af_e2e` suites (integration, `make test-integration-rustd`), assertions unchanged. Expected: every one passes. — DONE (the unit-lane `tests/providers` suite passes; the integration half is compiled and runs in §8's `make test-integration-rustd`)
- **Dimension 5.2** — the runner frame bounds a reply → Test `a_reply_past_the_cap_ends_the_run_as_the_fleets_error_and_is_sent_once` (integration, `afr_agent/tests/providers/ends.rs:111`, its import renamed only): a reply one event past 32 MiB. Expected: the failure detail names 33554432, and the request went once. — DONE
- **Dimension 5.3** — a guarded host stays refused → Test `should_name_a_blocked_endpoint_as_the_fleets_configuration_and_never_as_unhosted` (unit, `af_providers/src/error/tests.rs:215`), plus `a_send_the_guard_refused_is_not_retried` through the adapter. Expected: the fleet's configuration error naming the provider, after one send. — DONE
- **Dimension 5.4** — the daemon makes no model call → the ast-grep rule `rust-daemon-no-connector` (lint, `make lint-source`; a test may not read a repository file), with its rule test seeding each spelling: every `rustd/crates/daemon/**` source file. Expected: no `af_providers::Connect`, `Connector` or `Turns`. — DONE
- **Dimension 5.5** — saved settings still validate → Test the `afd_admin` `model_validation` and `afd_credential` eligibility suites (unit), assertions unchanged. Expected: an `http://` base, an unknown dialect and an output past the window are refused as before. — DONE
- **Dimension 5.6** — moved code names no harness → the ast-grep rule `rust-provider-no-harness-name` (lint, `make lint-source`; a test may not read a repository file), with its rule test: guffbolt-ai's Rust sources and `af_providers`'. Expected: no `IronClaw`, `ironclaw_llm`, `codex-rs`, `pi-ai` or `pi-agent-core`, in any case. — DONE

### §6 — The pages describe two standalone crates

`runner_execution.md` §Crates names `guffbolt-ai` beside `guffbolt`, both outside every layer. `af_providers` is the adapter, and `af_inference` is gone. `docs/RUST_ERROR_STANDARD.md` gains a `guffbolt-ai` row: one hand-written `Error` with its `Result`, because the crate cannot call `error_shell!`. The guffbolt README leads with the MiniMax chat.

- **Dimension 6.1** — the READMEs compile → Test the guffbolt and guffbolt-ai README doctests (unit, `make test-unit-all`). Expected: pass.
- **Dimension 6.2** — no page names the deleted crate → Test the R3 search. Expected: no output.

### §7 (from M253_003 §6) — M253_001's acceptance and benchmarks run at the end of M253

Carried from M253_001, by the owner's instruction. They run once on E2E Networks after M253_003's CI is green, in parallel, and the virtual machines are deleted after.

- **Dimension 6.1** (was M253_001 7.3) — the CLI reads turns → Test `fleet_turns_lists_turns` (acceptance, `cli/test/acceptance/logs-turns-live.spec.ts`): `agentsfleet fleet turns <fleet_id>` against a live daemon. Expected: JSON rows with `turn_id`.
- **Dimension 6.2** (was M253_001 9.1) — recovery is measured → Test `bench_thread_recovery_api_and_files` (manual): Indy reviews `bench/baselines/thread-recovery.rig.json`. Expected: p50 and last-fleet times for each size, with source hashes.
- **Dimension 6.3** (was M253_001 9.2) — each changed statement has a plan → Test `explain_plans_for_thread_statements` (manual): Indy reviews `bench/baselines/thread-sql.rig.json`. Expected: one plan per statement, none a sequential scan on `thread.turns` or `thread.runs`.

Moved here from M253_003 (Indy, Nov 21, 2026) so the end-to-end run covers guffbolt-ai.

## Interfaces

- **guffbolt (unchanged paths):** every name in `guffbolt/src/lib.rs:95-105` still resolves. New: `guffbolt::Chat<M>` with `new(model, instructions)`, `with_tools(tools)`, `prompt(text) -> Ending<ChatFailure<M::Error>>` and `stop() -> &CancellationToken`; `guffbolt::ChatFailure<E>` is `Model(E)` or `Silent { cut }`.
- **guffbolt-ai types:** `Model`, `Request`, `Chunk`, `End`, `Hosted`, `TextKind`, `Usage`, `Call`, `Message`, `Replay`, `ToolSpec`, `Image`, `ImageKind`.
- **guffbolt-ai models:** `Models::{new, builtin, with, get, connect, connect_with_key}`, `Provider` with `with_base_url`, `Spec { id, context_tokens, max_output_tokens, images }`, `providers::<name>()` per feature.
- **guffbolt-ai wire:** `Api { Chat, Responses, Messages, Gemini, Ollama }` with `carries_images`, `Route::new`, `Route::supports_tools`, `Route::host`, `Connected` implementing `Model`, `Connected::{with_transport, with_reply_bound, with_retry, with_timeouts, with_observer}`, `transport::{Send, SendError}`, `Retry`, `Timeouts`, `log_filter`, `Error`, `Result`.
- **Features:** default none; `wire`; `reqwest` (default transport); one per provider; `all`; `log-filter`; `test-util`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Missing key | auth failure | the provider's variable is unset | `connect` fails naming the variable, no value | `connect_without_the_key_names_its_variable` |
| Unknown model | malformed input | an id no row holds | refused before any request | `an_unknown_model_id_is_refused_with_its_provider` |
| Unregistered provider | malformed input | its feature is off or it was never added | refused naming the provider | `an_unregistered_provider_is_refused` |
| Rate limited | exceeded quota | 429 with a long `Retry-After` | not slept; the turn ends with 429 | `a_retry_after_past_sixty_seconds_ends_the_turn` |
| Silent provider | timeout | no event for 300 s | a deadline failure | `a_silent_provider_ends_at_the_idle_timeout` |
| Redirect | security | the provider answers 3xx | not followed; the key goes nowhere else | `a_redirect_is_not_followed_with_the_key` |
| Guard refusal | security | the embedder's transport refuses the host | one send, not retried; agentsfleet names the policy | `a_send_the_guard_refused_is_not_retried`, `should_name_a_blocked_endpoint_as_the_fleets_configuration_and_never_as_unhosted` |
| Oversize reply | capacity | a reply past the embedder's bound | refused, not reopened | `a_reply_past_the_bound_the_embedder_set_is_refused`, `a_reply_past_the_cap_ends_the_run_as_the_fleets_error_and_is_sent_once` |
| Model failure in a chat | dependency unavailable | the stream errs | `Ending::Failed` with the error | `a_chat_whose_model_fails_ends_failed` |
| Old checkpoint | replay | a `DriveState` saved before the move | reads and resumes | `a_drive_state_saved_before_the_move_resumes` |
| Bad catalogue row | malformed input | output past window, or a repeated id | the unit lane fails | `every_catalogue_row_parses_and_fits` |
| Daemon gains a model call | security | a daemon crate names the connector | `make lint-source` fails | `rust-daemon-no-connector` |

## Invariants

1. guffbolt-ai depends on no workspace crate — `cargo publish --dry-run` in `make lint-rustd` builds it from crates.io alone, and fails on any path dependency.
2. guffbolt's default build links no wire — guffbolt depends on guffbolt-ai with `default-features = false`, the wire crates are optional, and Dimension 1.3's lint step checks what the default build links.
3. The key reaches only the route's host — the default transport's client is built with `redirect::Policy::none()`, and Dimension 2.4 proves it.
4. No daemon crate drives a model — the `rust-daemon-no-connector` ast-grep rule fails on a daemon source naming `Connect`, `Connector` or `Turns`.

## Metrics & Observability

N/A: no product or operator signal changes. The provider metric families and the `model_turn_*`, `provider_retry` and `provider_turn_reopened` lines keep their names and fields, now written by `af_providers`' observer. The existing `af_providers` `observe/tests.rs` suite proves it unchanged.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | guffbolt-ai and guffbolt build alone from crates.io (§1) | `make lint-rustd` | exit 0, with the standalone dry run for both crates | P0 | |
| R2 | A plain chat on MiniMax answers through the public API (§3, §4) | `make test-unit-all` | `a_chat_on_minimax_answers_through_the_messages_wire` and `the_minimax_example_answers_against_a_fake_provider` pass | P0 | |
| R3 | No reference to the deleted crate remains (§5, §6) | `git grep -n -w af_inference -- . ':!docs/v2'` | no output | P0 | |
| R4 | guffbolt's public paths are unchanged (§1) | `make test-unit-all` | `guffbolt_reexports_every_seam_type` and the guffbolt README doctests pass | P0 | |
| R5 | agentsfleet runs are unchanged (§5) | `make test-integration-rustd` | exit 0 | P0 | |
| R6 | No bound without a reason (§2) | `make test-unit-all` | `with_no_bound_a_reply_past_32_mib_is_read` passes | P1 | |
| R7 | Moved code names no other harness (§5) | `git grep -n -i -E "ironclaw|codex-rs|pi-ai|pi-agent-core" -- rustd/guffbolt-ai rustd/crates/harness/af_providers rustd/Cargo.toml` | no output | P1 | |
| R8 | Live MiniMax answers (§4) | manual: Dimension 4.5 | the output in the PR Session Notes | P1 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit lanes | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration lane | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `af_inference` | `git grep -n -w af_inference -- . ':!docs/v2'` | no output |
| `SendRequest` | `git grep -n -w SendRequest -- rustd` | no output |
| `carries_images` on `af_wire::inference::InferenceApi` | `git grep -n -w carries_images -- rustd/crates/protocol` | no output |
| `rustd/guffbolt/src/model.rs` | `git grep -n "mod model;" -- rustd/guffbolt/src/lib.rs` | no output |

## Out of Scope

- OAuth, Bedrock, Vertex and Copilot sign-in: not planned in this spec (Q6).
- guffbolt-ai's own wire code in place of rig: Q1.
- A catalogue refresh tool: a new script needs Indy's sign-off, so rows are edited by hand.
- Publishing both crates to crates.io: a release, which waits for Indy's ask.
- Image generation and classifier models, which pi-ai also serves: not planned.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A Rust developer adds two dependencies, sets `MINIMAX_API_KEY`, runs the 16-line program, and reads MiniMax's answer.
2. **Preserved user behaviour** — Every `guffbolt::` path compiles. A hand-written `Model` still works. Every agentsfleet run, bound, retry and log line is unchanged.
3. **Optimal-way check** — The most direct route moves code that already passes into a crate with no agentsfleet dependency. The gap is `Chat` and the catalogue, which nothing has today.
4. **Rebuild-vs-iterate** — Iterate: the wires are proven, and moving them keeps their tests. Rewriting them off rig is Q1, not this spec.
5. **What we build** — guffbolt-ai: types, wires, the transport seam, `Models`, factories and catalogue rows; `guffbolt::Chat`; `af_providers` as the adapter.
6. **What we do NOT build** — OAuth sign-in, a catalogue fetcher, cost estimates per call (agentsfleet prices from its own catalogue), and image or classifier models.
7. **Fit with existing features** — It compounds with M253_002's standalone guffbolt. The run path it must not destabilize is the runner's `Connector::connect`.
8. **Surface order** — Library first; no CLI or UI surface changes.
9. **Dashboard restraint** — No UI change.
10. **Confused-user next step** — The error names the missing variable, the unknown id or the unregistered provider. The README lists every provider, feature and key.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** types (§1), wires (§2), providers (§3), chat (§4), the adapter (§5), pages (§6). Each lands green on its own, in that order.
- **Alternatives considered:**
  - **Providers inside guffbolt behind features.** Indy chose the sibling crate, and a provider crate could not then implement `Model` without the loop.
  - **guffbolt-ai depends on guffbolt.** Then guffbolt cannot ship a ready chat over a real provider without a cycle; rejected.
  - **Keep the types in guffbolt, implement `Model` there for guffbolt-ai's model.** guffbolt-ai would need guffbolt's `Request` to encode one, a cycle; rejected.
- **Patch-vs-refactor verdict:** a **refactor** because code moves across crates while behaviour holds.

## Discovery (consult log)

- > Owner (Indy, Nov 21, 2026), on the seven open questions: "Take all seven (Recommended)" — context: Q1 rig 0.47 stays the wire engine inside guffbolt-ai; Q2 the model catalogue is checked-in rows seeded from rig's; Q3 af_inference is deleted and what remains folds into af_providers; Q4 the workspace version; Q5 the count timeout and reply size move unchanged (they are TOML keys now); Q6 the 12 key-based providers first, OAuth/Bedrock/Vertex later; Q7 guffbolt::Chat is in this spec.
- > Owner (Indy, Nov 21, 2026): "since after M253_003 you will have to fix M253_004 and perfor the spec using orly-build".

- **Consults** — Indy's words, quoted:
  - > Owner (Indy, Nov 21, 2026): "we must move the guffbolt to be self sufficient. Can you refer how pi-core is invoked from the ~/Projects/oss/js/pi/ so we know how to deal with this" — context: the request; pi read at `ee6f36496`.
  - > Owner (Indy, Nov 21, 2026): "Sibling crate, as pi-ai" — context: his answer to "How should guffbolt become self-sufficient, the way pi-agent-core works on top of pi-ai?" A crate `guffbolt-ai` holds providers and a model catalogue, and `guffbolt` depends on it.
  - > Owner (Indy, Nov 21, 2026): "I need bounds if it has a reason" — context: §2's bounds. Each one names its forcing limit or its Codex or pi source, and the reply bound is the embedder's.
- **Agent defaults, listed in the PR for Indy:** the `Model` trait moves (§1); the transport seam speaks `http` types (§2); no reply bound by default (§2); `Chat` lends tools nothing (§4); `af_inference` folds into `af_providers` (§5).
- **Open questions for the owner:**
  - **Q1, the wire engine.** Recommend rig `=0.47.0` behind `wire`: both crates already pass on it, and it carries the dialect tables, tool-call assembly and reasoning replay. Alternative: our own wires per API, as pi writes them.
  - **Q2, catalogue data.** Recommend checked-in rows seeded from rig's catalogue, so a wire change keeps them. Alternative: read `rig_core::catalog` at run time.
  - **Q3, af_inference's remainder.** Recommend folding it into `af_providers::settings`, with the layer test keeping the daemon off the connector. Alternative: keep a slim settings-only `af_inference`.
  - **Q4, the version.** guffbolt takes the workspace version (`guffbolt/Cargo.toml:4`, `0.78.0` at `rustd/Cargo.toml:111`). Recommend the same; alternative: both crates get their own.
  - **Q5, the count bounds.** `COUNT_TIMEOUT` and `COUNT_REPLY_BYTES` move unchanged. pi-ai calls no count endpoint, so the rule says cap nothing; dropping them changes agentsfleet runs.
  - **Q6, the first providers.** Recommend the twelve key-based providers in §3; pi's others wait.
  - **Q7, `Chat` in this spec.** Recommend yes: without it the shortest MiniMax chat is the README's 170 lines.
- **Settled (Nov 22, 2026):**
  - Fireworks' Chat base is `https://api.fireworks.ai/inference/v1`; the Anthropic-style base drops `/v1` (docs.fireworks.ai/tools-sdks/openai-compatibility).
  - MiniMax's model id is `MiniMax-M3.1-Flash`; there is no plain `MiniMax-M3.1` (platform.minimax.io/docs/guides/text-generation).
  - `guffbolt-ai` and `guffbolt` are free on crates.io (`GET /api/v1/crates/<name>` → 404 for both).
  - `cargo publish -p guffbolt-ai -p guffbolt --dry-run --allow-dirty --locked` exits 0 with guffbolt-ai unpublished. Cargo packages guffbolt-ai, verifies it, then unpacks it into a local registry (`target/package/tmp-registry`) and verifies guffbolt against that copy. guffbolt's dependency is a path plus `version = "0.78.0"`, declared as `[workspace.dependencies.guffbolt-ai]` so `make sync-version` moves the version with the workspace's. The `guffbolt standalone` step of `make lint-rustd` runs this two-crate dry run.
  - §1 adds no `guffbolt_ai::Error`: the moved types have no fallible function. The hand-written `Error` and its `Result` land with §2's first fallible one.
  - M253_004 is free on a fetched `origin/main` (`git ls-tree` → 0 hits).
- **Settled while building §2 (Nov 22, 2026):**
  - The wires log nothing. `guffbolt_ai::wire::Observer` hears a turn's start, first text, end, input calibration, each retry and each reopening; `af_providers`' `RunObserver` writes the `model_turn_*`, `model_input_calibration`, `provider_retry` and `provider_turn_reopened` lines and the measurements, fields unchanged.
  - `af_inference` is a shim until §5: `Configured` (garde, `provider_endpoint`, `accepts_endpoint`) over `guffbolt_ai::wire::Route`, `Timeouts` (agentsfleet's 10 s connect beside the wire's `Timeouts`), `TimeoutSettings`, and `transport::REPLY_MAX_BYTES` = `af_wire::protocol::MESSAGE_BYTES_MAX`, so `runner/main` builds unchanged.
  - `af_providers::RetryPolicy` wraps `guffbolt_ai::wire::Retry`; `Connector::connect` builds a `Connected` over its guarded client (`Reqwest::refusing(af_egress::blocked_address)`) with the frame as its reply bound, and `Turns` maps its failures to the same kinds, codes and classes.
  - `estimate`, `context_document` and `retry_after` are public in `guffbolt_ai::wire`: the scripted model, the context index and the decision client use them.
- **Settled while building §5 (Nov 22, 2026):**
  - `af_providers::settings` holds `Configured` (the one `InferenceApi` to `Api` match, `Route::new`, `accepts_endpoint`, `api()`), `provider_endpoint`, `validate_decision`, `Timeouts`, `TimeoutSettings`, `CONNECT_TIMEOUT`, `RetryPolicy`, `RetrySettings` and `log_filter`. `RetryPolicy` and `RetrySettings` left the crate root, so each has one public path.
  - `log_filter` is a function over `guffbolt_ai::log_filter`, not a `pub use`, as `layers.rs` refuses a re-export of another workspace crate's item.
  - The settings refusals join `af_providers::Error` as kinds under `UZ-REQ-001`, as `af_inference`'s were, since a crate has one error type. The runner keeps one as the cause of `Configuration`, still `AGENTSFLEET_INVALID_CONFIG`, so connect's codes and lines are unchanged.
  - The reply bound is `af_wire::protocol::MESSAGE_BYTES_MAX`, named once in `af_providers::transport`. `afr_agent`'s limit suites count input with `guffbolt_ai::wire::input_tokens`, a dev-dependency, because a count is not a setting.
  - The adapter connects as `policy.context.model`, the id the loop names in every `Request`, so production sends what it sent before. `afr_agent/tests/providers/native.rs` asked for `configured-model` on a run whose policy said `model-1`; its fixture now sets the policy's model, assertions unchanged.
  - 5.4 and 5.6 are ast-grep rules, not tests: `docs/architecture/testing.md` routes a rule about how source reads to a lint, and the `tests-rust-repo-read` rule refuses a test that reads the repository.
- **Settled while building §4 (Nov 22, 2026):**
  - `Chat` fails with `ChatFailure<M::Error>`, not `M::Error`. The loop asks the conversation for a blank first step's failure (`Conversation::silent`), and no `M::Error` can be built for any `M`. `ChatFailure::Model(E)` displays as `E` and answers `E`'s source; `Silent { cut }` holds no cause.
  - The loop calls the chat's model as `dyn Model<Error = ChatFailure<M::Error>>`, over a private wrapper that lifts the model's error, so no wrapper type joins the public API.
  - `Chat` keeps its `DriveState`: each prompt's ending answers that prompt alone, calls number after earlier prompts, and a prompt sent while a question waits is its answer.
  - `Chat::request` sends `Request::model` empty, because `Model` names no id. `Connected` counts input with `request.model` (`guffbolt-ai/src/wire/connected/mod.rs:138,143,168`), so §3 has `Connected` count with its own id before 4.4 runs.
- **Settled while building §4.4 (Nov 22, 2026):**
  - `examples/minimax_chat.rs` belongs to guffbolt, which dev-depends on guffbolt-ai with `minimax` and `test-util`. It is the README program; an optional first argument is passed to `Provider::with_base_url`.
  - The fake serves plain HTTP on loopback, not TLS. The example's default `reqwest` client trusts the system's roots through `rustls-platform-verifier`; on macOS that is the Security framework, which no environment variable extends, and the fake's `fake.provider.test` resolves only inside a client built with `testing::tls::client`. The fake admits only the fixture key, so the key read from the environment is proven on the wire.
  - Cargo sets `CARGO_BIN_EXE_*` for binaries alone, so the suite finds `examples/minimax_chat` under an ancestor of its own path: `deps/` in the old layout, `build/<crate>/<hash>/out/` in Cargo 1.100's. The child runs with an empty environment but for the key and `LLVM_PROFILE_FILE`, as `runner/main/tests/support.rs` runs the runner.
  - The Messages answer a fake streams is `guffbolt_ai::testing::messages::answer`, shared by guffbolt-ai's `minimax` suite and this one.
  - guffbolt-ai's README doctest is compiled by guffbolt (`ProviderReadmeDoctests`, `cfg(doctest)`), because the program runs on guffbolt and guffbolt-ai never depends on guffbolt (`rustd/Cargo.toml:19-22`).
- **Settled while building §3 (Nov 22, 2026):**
  - Each provider feature turns on `wire` and `reqwest`: `connect` answers a `Connected` on the default transport, so a provider without `reqwest` could not connect. `Models`, `Provider`, `Spec` and `providers` compile under `reqwest`.
  - The catalogue rows are rig-core 0.47's tool-calling models with a context window and an output limit, deprecated ones left out: 470 rows over 11 files. rig has no Fireworks rows, so `catalog/fireworks.json` is empty and `Provider::with_spec(Spec)` adds a model; a row added in place of a catalogue row replaces it.
  - Base URLs and key variables are read from rig's dialect constants where rig names them; Ollama's and Fireworks' are written out, as rig keeps Ollama's private and has no Fireworks dialect.
  - `connect` finds the provider, then the row, then reads the key, so an unknown provider or model is refused whatever the environment holds. An empty variable is no key.
  - `Connected` counts, measures and sends as the id it was connected to, whatever `Request::model` names: test `a_request_naming_no_model_is_counted_and_sent_as_the_connected_one`.
  - `Connected::accepts_images` is the row's `images` and the `Api`'s `carries_images`; `Connected::new` keeps the `Api` alone, as before.
- **Settled while building §9 (Nov 22, 2026):**
  - `Env::search(&Search) -> Found` is one required method: `Search` is a path, an optional glob, an optional `Pattern` (text, literal, ignore case) and the model's `limit`; `Found` is the hits in path order (`Hit::File` or `Hit::Line`), `limited` and `cut`. No default walk is kept: guffbolt is unpublished, so no third-party `Env` needs one.
  - `find` and `grep` call it; `ls` keeps `list_dir`, already one call. Names, arguments and output lines are unchanged; both descriptions now say `.gitignore` paths are skipped. A `grep` path that is not there still answers empty.
  - `fs/search` runs `ignore`'s walker (`.gitignore` and `.git/info/exclude`, `require_git(false)`, dotfiles kept, `.git` and links skipped, no global ignore file) and `grep-searcher` over `grep-regex`, on the session's blocking pool like every file call. `LocalEnv` runs the same crates on `spawn_blocking`. No new cap: the frame (`Sizes::max_frame_bytes`) is the only bound, and a cut answer sets `cut`. The executor opens each file through its `cap_std` handle.
  - Two changes from the old walk. A file past the read size (8 MiB) is now searched whole. A file is skipped only when a matched line is not UTF-8, not when any byte is. A pattern holding a literal line break is refused as `invalid_arguments`.
  - `globset`'s `Glob::compile_matcher` panics on a glob too large to compile, so both ends build a one-glob `GlobSet`, whose build is fallible.
- **Runner↔executor transport review (§9, Nov 22, 2026): recommendation, not built.**
  - Client: fits. `jsonrpsee-core`'s `async-client` takes any `TransportSenderT`/`TransportReceiverT` through `ClientBuilder::build_with_tokio(sender, receiver)` (`jsonrpsee-core-0.26.2/src/client/async_client/mod.rs:306`), so the two halves of the Unix socket can be line-framed under it. `process/output` and `process/exited` would arrive through `subscribe_to_method` (`client/mod.rs:148`), keyed by method rather than process. Demuxing by process and the head and tail bound per process stay ours.
  - Server: does not fit as a server. `jsonrpsee-server` speaks HTTP and WebSocket only: its listener is TCP (`jsonrpsee-server-0.26.2/src/server.rs:57,71`) and `serve(io, service)` runs hyper over a stream (`src/utils.rs:137`). `RpcModule::raw_json_request` (`jsonrpsee-core-0.26.2/src/server/rpc_module.rs:351`) answers one raw request with no transport, so it could replace the session's hand-written `route` while the line codec stays.
  - Recommend a follow-up: the client over `build_with_tokio`, and `RpcModule` on the executor behind the existing line codec. Unverified: whether `raw_json_request` keeps the rule that a spawn's answer is queued before its first output, and whether it bounds a response by the frame.
- **Unverified (`[?]`):**
  - Whether Codex calls a token-count endpoint; Q5 settled the bounds either way, so it blocks nothing.
- **Model reviews** — none yet; the EGM review runs before this spec leaves `pending/`.
- > Owner (Indy, Nov 22, 2026), on §7's Linux-only acceptance run: "Yes, one E2E run at the end (Recommended)" — context: one `orly burst acceptance --provider e2e` after §1–§9 and `make test-integration-rustd` are green, the machine deleted when the run ends.
- > Owner (Indy, Nov 22, 2026): "now once you are done with the sibling create i want the guffbolt/README.md to have the usage like pi-core/README.md has similar one ELI20 (no cryptic, no marketing) but to an engineer who can use the harness and build on top of it. Then you have to go back to your own code and see if the guffbolt is intergrated in the way like you have written or better way that suites us. Net net - just like `pi` is built on top of `pi-core` our agentsfleet is built on top of guffbolt must be proven. There must be some live tests added with the fireworks key and tested manual for a 2$ budget max,  dont test beyond that. The testing i mean is the guffbolt-ai, guffbolt using guffbot-ai on read model. The coverage of rust code must be 99%, with all the files> 99% the patch coverage 99% so lets move the notch up in the respective coverage configuration and ensure we meet that. Ensure the CI jobs are green, greptile feedback is resolved and all green and good to merge. Donot merge, Indy will eyeball and merge." — context: adds §11–§15 below; §10 (tools brainstorm) stays last for his sign-off.
- > Owner (Indy, Nov 22, 2026): "1. I think when you set the fleet you give a budget so this flag is pointless 2. Yes add the read_timout_seconds 3. Yes signed on the fix to pass the runner to pass the values to the executor." — context: §0 below; (1) the fleet budget maxima, (2) the HTTP API request read timeout, (3) the sizes the runner and the in-sandbox executor share.
- > Owner (Indy, Nov 22, 2026), on CI workflow edits overnight: "Yes, coverage/test lines only (Recommended)" — context: `.github/workflows/**` lines that pass the coverage floors or select tests (keeping the manual live tests out of CI) may change; deploy jobs, secrets, permissions and triggers may not; every edit is listed in the morning summary.
- > Owner (Indy, Nov 22, 2026): "donot send any keys or spill any keys in the PR" and "run gitleaks prior to push the PR" — context: before every push, `gitleaks detect` runs over the commits being pushed and must report no leaks; no key, token, vault path or key-derived value goes into a commit, a PR body, a PR comment, a log pasted in the PR or a test fixture.
- > Owner (Indy, Nov 22, 2026), on §14's "all the files > 99%": "Files the PR touches (Recommended)" — context: the judge's existing `--per-file-floor` (his Oct 15 rule) turns on at 99 for every `.rs` file the branch adds, edits or moves; project and patch floors go to 99; guffbolt-ai is graded as its own crate.
- **Open blockers** — none; the one `[?]` left blocks nothing. Q1 to Q7 are answered ("Take all seven").
- > Owner (Indy, Nov 22, 2026): "Indy is off to bed, so you need to keep going on M253_004 and move the tools brainstorm at the end.  I cant sit and wait for you to complete to do a brain storm" — context: the tools brainstorm moves from before §1 to §10, the last Section; §1–§9 are built without waiting for it.
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.

### §8 — Carried from M253_003 (Indy, Nov 21, 2026)

- `make test-integration-rustd` over everything M253_003 built: its integration tests are written and compiled but not yet run (§1–§4 Dimensions, the effect items, the thread page of 100, `recovery_process` reordered for the deleted fallback tick).
- Regenerate `public/openapi.json` with `--no-banner` (page limits, the `x-request-id` header, effect item fields, removed caps).
- `orly gate pr` on the final tree, then CI and review.

### §9 — Search runs where the files are (from M253_003, Nov 22, 2026)

guffbolt's grep, glob and tree listing walk the sandbox through `Env` today: one `fs/list` per directory and one `fs/read` per file over the executor socket, with no `.gitignore`. Add one `Env::search(pattern, root, globs)` and one executor method `fs/search` that runs the `ignore` walker and `grep-searcher` on the sandbox's own disk and answers once; `LocalEnv` runs the same two crates on the host path. The guffbolt tools call it instead of walking.

- Dimension 9.1 — DONE → Test `a_search_over_a_tree_is_one_executor_call` (unit): a 1,000-file tree. Expected: one `fs/search` request, matches equal to today's walk.
- Dimension 9.2 — DONE → Test `a_search_skips_gitignored_paths` (unit): `target/` listed in `.gitignore`. Expected: no match under it.

Also review the runner↔executor JSON-RPC transport: it uses `jsonrpsee-types` only and frames messages itself; use `jsonrpsee`'s own client and server if they fit the Unix socket, else record why not.

### §11 — guffbolt's README teaches an engineer to build on it (Indy, Nov 22, 2026)

`rustd/guffbolt/README.md` follows `~/Projects/oss/js/pi/packages/agent/README.md`'s shape: install, the shortest working chat over a real provider through guffbolt-ai, then the loop, tools, events, sessions and how to add a provider or a tool. Plain words for an engineer building a harness on it; no marketing, no internal agentsfleet names. Every code block compiles.

- Dimension 11.1 — the README's code runs → Test `the_readme_examples_compile` (unit, doc-test): each Rust block in the README is included as a doc-test. Expected: all compile; the chat example runs against a stub provider.

### §12 — agentsfleet is built on guffbolt, as pi is on pi-core (Indy, Nov 22, 2026)

Re-read how agentsfleet's runner (`afr_tools`, `afr_supervisor`, `af_providers`) uses guffbolt after §1–§5, against how `pi/packages/coding-agent` uses `pi/packages/agent`. Every place agentsfleet re-implements loop, tool, event or provider behaviour guffbolt already has is moved onto guffbolt's public API, or the reason is recorded here. agentsfleet reaches guffbolt only through its public API.

- Dimension 12.1 — one boundary → Test `agentsfleet_reaches_guffbolt_only_through_its_public_api` (unit): a dependency check finds no `guffbolt` item outside its published API used by agentsfleet crates, and no second agent loop. Expected: none.

### §13 — Live tests on Fireworks, $2 at most (Indy, Nov 22, 2026)

`#[ignore]`d live tests prove guffbolt-ai alone and guffbolt over guffbolt-ai on a real Fireworks model: one chat, one streamed reply, one tool call round trip, one token-usage read. The key is `FIREWORKS_API_KEY` from the machine environment file; it is never printed. They run by hand only, on the cheapest Fireworks chat model, with a running spend tally from the provider's reported usage that stops the run at $2.

- > Owner (Indy, Nov 22, 2026): "yes donot add any live tests of fireworks into the CI" — the live tests are `#[ignore]`d with a reason naming the manual run, no CI job or `--include-ignored` lane runs them, and no CI secret holds the Fireworks key.
- The coverage lane runs `--all-features --include-ignored`, so `#[ignore]` alone would put the live tests in CI. They live as a cargo example, `cargo run --example fireworks_live -p guffbolt-ai`, which no test lane runs.
- Dimension 13.2 — never in CI → Test `no_ci_lane_runs_the_fireworks_live_tests` (unit): no test target, workflow or make recipe runs the `fireworks_live` example, and the tests read the key only at run time. Expected: none found.
- Dimension 13.1 — live proof → Test `a_fireworks_chat_round_trips_through_guffbolt_ai` and `guffbolt_runs_a_tool_call_on_fireworks` (live, manual). Expected: both pass; the recorded spend in Discovery is under $2.

### §14 — Rust coverage 99% overall, per file and per patch (Indy, Nov 22, 2026)

Raise `codecov.yml`'s `rust-afd` project and patch targets to 99%, `RUSTD_COVERAGE_FLOOR` and `RUSTD_PATCH_FLOOR` to 99, and add a per-file floor of 99 to the coverage judge, so every Rust file under the flag holds at least 99% lines. Tests reach the numbers; nothing is excluded to reach them.

- Dimension 14.1 — the floors hold → Test `make test-coverage-rustd` (integration). Expected: overall ≥ 99%, patch ≥ 99%, no file under 99%.

### §15 — The PR is green and ready, unmerged (Indy, Nov 22, 2026)

Every CI job on PR #855 is green, every greptile finding is resolved or answered, and the PR is left for Indy to review and merge.

### §10 — Tools brainstorm with Indy, last (Indy, Nov 22, 2026)

Moved here from the spec's opening on Indy's word (Discovery). The agent prepares it while §1–§9 are built; Indy reviews it when he is back.

1. List every guffbolt tool as the model sees it: name, description, arguments, limits, output shape.
2. Compare each with the pi and Codex equivalent, read from their source under `~/Projects/oss`, and the experience each gives the model: naming, argument shape, output trimming, errors.
3. Propose per tool: keep, change (to match pi or Codex, or better), merge, or drop; and which tools ship by default or behind a feature.
4. Indy decides per tool and his verbatim sign-off goes in Discovery. Tool changes he signs off land after that, in this milestone's follow-up.

- Dimension 10.1 — the comparison exists → Test: `docs/v2/active/M253_004_TOOLS_BRAINSTORM.md` lists every tool registered in `guffbolt::tools` (a unit test reads the registry's names and the page's table and finds no tool missing).
