---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# M243 session handoff

## What it is

This file accompanies Indy's requested context-save checkpoint on draft [PR 819](https://github.com/agentsfleet/agentsfleet/pull/819).
Resume the feature branch at the commit containing this file, or a later explicitly published checkpoint.
The latest incorporated published checkpoint is Claude's `3e3d8f1b0`, following Codex's authoring checkpoint `9c2803f91`.

**M243 remains incomplete. Finish code and review first, then run the combined final verification. Integration is paused until code completion.**

The authoring checkpoint passed workspace compilation and full `make lint-all`.
The full Rust unit lane passed 5,380 tests before its guard simplification; the affected library shard then passed all 3,531 tests again.
The later scheduler checkpoint has its own narrow results in [bounded scheduling passes](#nov-09-bounded-scheduling-passes).
Claude's three previously failing tests and latest musl dependency fix are included.

Kernel compilation blockers are fixed. Native fixture migration and the remaining implementation still need verification.
Do not present M243 as code complete, ready to undraft or a benchmark baseline.

| Context | Value |
|---|---|
| Repository | `agentsfleet/agentsfleet` |
| Branch | `feat/m243-runner-push-stream` |
| Worktree | `/workspace/agentsfleet-m243` |
| Existing draft | [PR 819](https://github.com/agentsfleet/agentsfleet/pull/819) |
| M244 coordination | [Issue 818](https://github.com/agentsfleet/agentsfleet/issues/818), Claude's subsequent PR 820 |
| Fetched main | `61820b66a765a2aa87684251b07aebebd2c20955` |
| Resume prompt | [Copy-ready M243 prompt](resume-prompt.md) |
| Separate M246 preparation | [M246 parallel prompt](m246-parallel-prompt.md) |

Reuse this worktree if available.
Otherwise fetch the published feature branch into an isolated worktree.
Do not restart M243 from main, discard this source, rebase the published branch or force-push it.

## Why it exists

Indy requested a context save before continuing in a new cloud session.
He also requested an audit tying every supplied comment to source changes and unfinished work.
The [owner feedback audit](owner-feedback-audit.md) distinguishes source edits from runtime evidence.

The [implementation guide](implementation-signoff.md) contains historical evidence from earlier revisions.
Those results do not verify this checkpoint.

The date above records source reading. The check results below state exactly what ran.

## How it behaves

### Read first

1. `AGENTS.md`, `.orly/LOCAL.md`, `.orly/AGENTS.md` and `.orly/skills/orly-build/SKILL.md`.
2. This handoff, the [owner feedback audit](owner-feedback-audit.md) and the latest corrections in the [implementation guide](implementation-signoff.md).
3. [Runner protocol](../../architecture/runner_protocol.md) and [failure design](failure-and-observability.md).
4. [Active M243 specification](../../v2/active/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md).
5. Orly Rust, errors, code, tests, authentication, logging, HTTP and documentation standards for changed files.
6. Issue 818 and its current comments; read issue 813 before every push.

### Owner instructions that still apply

- Indy explicitly authorized compilation, formatting, lint, tests and gates on Nov 09 so this checkpoint can become green. Do not ask again for those routine checks.
- Keep PR 819 draft. Hold merge for Indy's listener, certificate and 1Password review.
- Tell Indy when the code is complete. Do not reuse earlier results to make that claim.
- Ask before new repository scripts, workflow or deployment edits, unlisted tables or SQL casts.
- Keep native UUIDv7 database bindings, `event_id`, owning table files and the shared Bus abstraction.
- Keep HTTP/3 preference with HTTP/2 fallback after eligible transport failures. Do not add hot transport switching.
- Require `X25519MLKEM768` on both ends and transports. Transport fallback must not weaken cryptography.
- Keep private Fly routing. External runners need an explicit route to the private listener.
- Remove obsolete polling, old report shapes and compatibility retries. Add no aliases or legacy delivery path.
- Postgres owns saved results and billing. Unacknowledged results stay only in caller memory.
- Preserve M244's work and coordinate overlapping interfaces on issue 818.

### Completed-report storage removal

The source deletes `afr_supervisor/src/report_spool/` and `src/drainer/`, including authoring and task-decision receipt files.
It also removes startup replay, quarantine, encryption helpers, directory creation and old-shape field-stripping retries.
Do not restore these to repair unfinished fixtures.

`lease_loop/settle.rs` retains one `Zeroizing<Vec<u8>>` report buffer in its worker until delivery ends.
`report/delivery/mod.rs` retries the current operation from that caller.
`authoring/mod.rs::Pending` clears its private answer on drop; no separate `authoring/report.rs` remains.

```text
worker finishes -> result in caller memory -> daemon -> Postgres commit
                            ^                   |
                            +------ retry ------+  if acknowledgement is lost

acknowledgement -> return from delivery -> drop pending memory
process loss before commit -> an unsent result or usage can be lost
```

Task-decision receipts likewise stay in their calling future until acknowledgement.
Postgres issuance precedes provider dispatch; receipt delivery retries must not dispatch another provider call.
Named zeroing owners do not prove wiping every serializer, TLS or operating-system copy; whole-PR ownership review remains due.

The audit records storage provenance: original commit `830cb0c4b8711c9bfd97aae81d8a450cc56e1eb6` is already an ancestor of main.
The old report-spool module was identical on main and checkpoint `a0a8cf3b7`.
No approval for retaining it in M243 was established; Indy explicitly rejected that approach.

### First resume tasks: compile the complete native path and migrate its fixtures

The four authoring daemon handlers are now written in `afd_gateway/src/session/dispatch/authoring.rs`.
Gateway composition shares `ModelCalls` and `Creations` alongside `Drafts`.
The four authoring HTTP adapters, route variants and OpenAPI source entries are deleted.

| Operation | Wire exchange | Stream |
|---|---|---|
| Daemon-owned model invocation | `AuthoringModel` / `AuthoringModelResult` | work |
| Proposal and progress receipt | `AuthoringActivity` / `AuthoringActivityAck` | work |
| Authoring lease extension | `AuthoringRenew` / `AuthoringRenewed` | control |
| Final authoring answer and usage | `AuthoringReport` / `AuthoringReportAck` | reports |

This supersedes the original M243 decision to retain those four HTTP calls.
M245, M246 and M247 do not already own this migration.
Browser fleet-setup chat remains its browser API; its runner job uses the native channel.

`Drafts::check_connection` checks the current credential and owner under the host lock after the existing draft/attempt guard.
Model reservation, proposal receipt replay and completion accounting use that check.
`ModelCalls::for_connection` and `Creations::for_connection` retain it through their existing domain operations.

The model answer deadline preserves 310 seconds, while writes retain the shorter transport deadline.
Work operation slots now permit the protocol's worker maximum while preserving independent control capacity and the total pending bound.
Do not repeat an uncertain physical provider call automatically.

Runner commands share a lifetime-independent `Command: Send + Sync` computation with bounded verb observations.
Authoring requests and fleet reports borrow their caller's payload through `Cow` during encoding.
The gateway decodes native definition types and consumes owned payloads; no HTTP tunnel or old-shape retry was added.

The kernel example's supervisor, report checks and recovery checks now use native frames through `NativeDaemon`.
The removed `RUNNER_REPORTS` constant was not restored.
Runner binary fixtures also use this native daemon; their remaining HTTP handlers serve only retained operations.

The shared test issuer now has a distinct subject from its leaf. The regression authenticates both transports and requires `X25519MLKEM768`.
It failed before the issuer correction and passed after it.

Boot fixtures explicitly supply the required native URL before testing later startup checks.

The resume run reached supervisor unit tests and found a real report-decoding defect.
Serde's tagged-enum buffering could not decode the report's borrowed raw tool trace.
`af_wire/src/protocol/up/decode.rs` now selects the current report schema from its discriminator and borrows its raw data from the charged frame.

It preserves the JSON shape, strict fields and malformed-trace handling.
The previously stuck telemetry test passed in 0.55 seconds after this change.

The scripted native fixture now starts production renewal batching and supplies the missing healthy folds answer.
Batching tests control the activity writer boundary; native progress tests wait for their terminal frame.
Lease cancellation and soft shutdown preserve final delivery; the hard-stop signal ends pending delivery.

No report files, old format, retired route or compatibility alias was restored.

The library run also found stale route counts, missing native payload schemas and a missing key-exchange vocabulary entry.
The source fixes register current payloads, document the existing 413 refusal and count all ten removed HTTP routes explicitly.
The transport telemetry fixture uses one literal loopback address, preserving its exact physical-attempt assertions.

A test-only `afr_supervisor::test_util::Session` now uses the production downlink, correlation and recovery owners.
It receives offers, retains registrations until report acknowledgement and exposes typed native operations.
`main/tests/support/e2e.rs::Scenario::native_session` shares that session through the scenario's `OnceCell`.

Native heartbeat/report calls were ported in fifteen end-to-end fixture files.
`e2e_wire.rs` now waits for pushed offers and durable refusals without issuing polling requests.
That conversion is unfinished: finish remaining assertions, imports, helper signatures and direct old-route calls before claiming runtime coverage.

Other per-lease APIs still use HTTP: task decisions, checkpoints, memory, tools, schedules, messages and credentials.
This checkpoint does not claim every runner operation has migrated.
Do not restore the four authoring routes as compatibility fallback.

### Other source changes since the preceding checkpoint

| Area | Written source and remaining evidence |
|---|---|
| Startup | Native listener and scheduler join daemon supervision; preflight is named `runner_listener`. Structured startup observations exist. |
| Drain | `Listener::serve(make, accepting, abort)` separates stopping accepts from hard cancellation. Main drain waits for gateway work and native owner closure under one deadline. |
| Native cleanup | H2 uses graceful shutdown; H3 drops the acceptor while keeping existing routing, then closes the controller before awaiting native completion. Final flush and cleanup remain unproved. |
| Runner session | Downlink owns opening/recovery; uplink owns bounded writes and typed request computation. Four streams, reply permits, cancellation and policy delivery have source wiring. |
| Renewals | One host ticker snapshots live meters. A bounded DashMap registry releases guards before meter reads and watch updates. |
| Credit race | Ready includes a revision and observed-offer count. The daemon deducts unseen offers and ignores stale revisions. Other free workers remain immediately eligible. |
| TLS | Connectors and acceptors require the hybrid group. Source tests cover classical rejection and wrong hostnames. Bounded daemon-side group observations are added. |
| Proxy | Standard hyper-util bypass matching, `TryFrom` conversions and a named async CONNECT operation replace ad hoc parsing. Explicit proxy selection uses H2. |
| Polling | Six fleet handlers and runtime polling methods are deleted. `LeaseRequest`, `LeaseResponse`, held-heartbeat knobs, doorbell modules and obsolete wake-answer metrics are removed. |
| Lease values | `TaggedLease` lives in `af_wire/src/lease/mod.rs`. The temporary `lease/tagged.rs` does not remain. |
| Wake | Placement and scheduler share the Bus-owned bounded wake queue. A live two-adapter wake test is written, without a passing result. |
| Worker tests | Pushed-offer sources replace polling cases. They include two free workers and B starting while A awaits its report acknowledgement. |

### Remaining source migration and review

1. Finish the full Rust lint/unit reruns and fix any further caller failures. The kernel example compilation is fixed; end-to-end fixtures still need the live lane.
2. Migrate remaining `LeaseResponse`, `Verb::Lease`, `.lease()`, `.claim()` and deleted path-constant callers in tests, examples and benchmarks.
3. Port HTTP-only fixture constructors through the native test seam. Supply required stream configuration in external fixtures.
4. Replace held-heartbeat end-to-end tests and stale downlink policy tests with session behavior.
5. Replace drain and saturation benchmark polling. Dense idle workloads, fairness and fault matrices still need source coverage.
6. Add and run authoring native integration cases, connection fencing, provider-dispatch replay, cancellation, renewals, folds and report retry cases.
7. Review worker policy changes while waiting for offers and shutdown bounds during a report outage.
8. Complete the whole-PR Rust review, including RS-SENTINEL, RS-UNWRAP, public docs, function sizes and causes.
9. Review manifest changes and Cargo.lock against their current callers.
10. Reconcile canonical and public docs; keep generated OpenAPI matched to its source, never edit it by hand.

Tool-trace parsing now returns its error, and invalid authoring call limits no longer become zero-call runs.
Remaining examples include `report_workers` saturation and listener failure handling that drops typed causes.
Main's terminal `BootFailure` and `MigrateFailure` are deliberate exceptions in `RUST_ERROR_STANDARD.md`; do not migrate them unnecessarily.

`claim_selected` now returns a direct optional tagged lease in test utilities.
Its encoder closure may require explicit argument types when the daemon libraries compile.
Reconcile all rewritten test modules with their test-only unwrap policy.

### TLS deployment and certificates still open

The source does not prove where production TLS ends.
Define the private hostname, direct TCP/UDP exposure and where native acceptors end TLS before claiming the hybrid policy applies in production.
No deployment or workflow file was changed for this checkpoint.

Pinned Quiche 0.30.0 sets SNI and `X509_VERIFY_PARAM_set1_host` in `src/tls/mod.rs::set_host_name`.
Wrong-host rejection test sources exist for both transports, without runtime results.
HTTP/2 pre-negotiation rejection telemetry remains incomplete.

Certificate issuer, root delivery and reload before expiry still need a concrete lifecycle design and Indy's review.
The extra-root file option does not establish live fleet rotation.
Do not write Fly secrets or 1Password items without that review.

### Coordination and research

Claude's M244 is PR 820, subsequent to M243; preserve its NATS startup, live-tail and benchmark work.
Shared overlap includes Bus connection observations, publication results, stream admission names, startup composition and Cargo files.
Do not import M244's entire deployment migration to close this checkpoint.

Read issue 818 for newer revisions and agreed ownership.
The last ownership question concerned wake-benchmark dimensions: one-second misses, at most 64 unanswered requests and one/two/four replicas.
Those extra dimensions were not silently accepted into M243; check the issue and ask Indy before assigning them.

Ignored reference exports are not repository files and will not exist in a fresh cloud checkout.
They were copied without `.git` under `.tmp/reference-sources/` from these revisions:

- Exonum: `2d2fa22e5f5bc451d08c155c2398956f11dce06e`.
- OpenAI Codex: `8703d32bd0ea5cf26768859a3dd476df906ba053`.

Exonum informed event selection/handler separation and cleanup on every exit path.
Codex informed explicit cancellation and distinguishing a queued write from a completed write.
Pinned tokio-quiche 0.20.1, BoringSSL bindings 5.4.0 and Quiche 0.30.0 informed native ownership and TLS configuration.

### M246 can prepare independently

The M246 spec depends on M243's once-per-connection authentication, periodic recheck and revoke-driven closure.
Its token overlap and reload behavior extends that interface.
Enrollment, tenant ownership, placement policy and management UI can prepare in another worktree.

Final stream integration and merge still depend on the completed M243 interface.
M245 is not an explicit dependency in M246.
M246 also retains two owner decisions in Discovery; its separate prompt preserves those questions without choosing defaults for Indy.

### Standards-review corrections written in this checkpoint

The Orly standards reviewer read the moving PR source and reported concrete failures.
This is an initial source review, without final sign-off or runtime proof.
The following fixes are written:

- Registry insertion and superseding hold the database host lock while proving the confirmed owner.
- A live regression models A committing, B registering, then delayed A attempting registration.
- Candidate sorting captures keys once, avoiding changing comparison keys during concurrent policy updates.
- A known-fleet wake now notifies every eligible candidate, rather than only as many hosts as fleet hints.
- Proxy CONNECT preserves its status; 429 and server failures remain retryable, while authentication/policy refusals remain terminal.
- Frame growth copies into another zeroing owner before dropping the previous allocation.
- Authoring command payloads borrow private data instead of making an extra ordinary owned clone.
- Worker panic observations record bounded flags and an error code, without formatting arbitrary panic text.
- Overflow of late-cancellation tombstones requests recovery instead of permanently halting the runner.
- Tool-trace parse errors propagate, and invalid call allowances no longer become zero.
- Waiting workers recheck policy changes while awaiting offers.

These regression sources have not passed a test lane.
The scheduler's positive-pass issuance budget and coalescing in M243 Dimension 4.4 remain unfinished.
Notifying all candidates closes the wrong-host delay but does not establish that acceptance result.

File-length review also remains due.
Remaining oversized files include `af_stream/src/error/mod.rs`, `main/src/serve/mod.rs` and `main/src/serve/drain/mod.rs`.
Several earlier touched domain files exceed 350 lines too; a green Clippy run would not prove this review rule satisfied.

### Cloud build tools used here

This cloud image lacked cmake and libclang.
A user-owned virtual environment at `/workspace/.cloud-tools/native-build` supplies cmake 4.4.4 and libclang 18.1.1.
The library path is `/workspace/.cloud-tools/native-build/lib/python3.12/site-packages/clang/native`.

Rust lanes used that environment's `bin` directory on PATH and the library path through `LIBCLANG_PATH`.
`BINDGEN_EXTRA_CLANG_ARGS` included `-I/usr/lib/gcc/x86_64-linux-gnu/14/include` for builtin C headers.
Keep two Cargo jobs, zero development/test debug info and incremental compilation disabled, as `.orly/LOCAL.md` requires.

These tools are machine setup outside the repository; a fresh cloud session must inspect its own installed tools.
Root, CLI and every UI package were installed from their frozen lockfiles, and the CLI build completed.
Live-datastore setup still remains before the integration lane.

This container runs `tail -f /dev/null` as process 1 and does not reap orphaned children.
The unchanged executor process-cleanup tests failed because killed children remained zombies.
They passed under a temporary child-subreaper wrapper outside the repository (`/tmp/m243-subreaper.py` on this machine).

It sets `PR_SET_CHILD_SUBREAPER` and waits for adopted children; no repository script was added.
Run Rust tests with `umask 022` scoped to the lane: the cloud shell's `0077` mask also caused the published-image mode assertion to fail.
These are execution-environment requirements, with no relaxed assertion or coverage floor.

## Limits

Indy initially held verification, then approved publication scans and explicitly authorized compilation, lint and tests.
His latest steering prioritizes committing, pushing and handing off the actual work.
This checkpoint records current failures and must remain draft.

| Command | Actual result |
|---|---|
| `orly doctor` | Passed: local rules match and Codex tools are installed. |
| `orly gate work --accept-dirty` | Passed: four declared commands. This does not compile or test the source. |
| `gitleaks protect --staged --redact --verbose --no-banner` | Passed for the published checkpoint. The resume edits are not staged or scanned yet. |
| `make lint-source` | Passed: 44 rule tests and source scan after the source changes. |
| `cargo fmt --all --check` | Passed after the latest source changes. |
| `cargo check --workspace --all-targets --all-features` | Passed from `rustd/`: development profile completed in 1 minute 5 seconds after the latest fixes. |
| `make lint-rustd` | Passed before OpenAPI regeneration: source checks, formatting, all-target/all-feature Clippy and cargo shear. The regenerated JSON now has one source-prose finding below. |
| `make test-unit-rustd` | Runner: 1,685 passed. Daemon: 164 passed. The final library run has three failing tests; see the handoff below. |
| `cargo test -p afr_supervisor --all-features --lib` | Passed: 396 tests in the full runner shard, zero failures. This is an inner-loop result, not the repository unit lane. |
| `cargo test -p af_wire --all-features` | Passed: 79 library tests and 103 suite tests, zero failures. Includes native report-trace and strict-field regressions. |
| Generated OpenAPI | Rebuilt from the current daemon with `--no-banner openapi`: 115 paths; all ten retired paths absent; native body and 413 response present. |
| `make test-unit-all` | Failed in the Rust library shard on the three cases below. Frontend coverage did not run. |
| Integration and final shipping gates | Still due. PR remains draft. |

There are no active Git hooks in this checkout; only sample hooks exist.
Manual publication checks are required; do not claim hooks ran.
No deployment, workflow, Fly secret or 1Password write was made.

Keep the repository lanes and coverage floors unchanged.
Earlier changed-line coverage failed and does not verify this checkpoint.
Production private routing, certificate lifecycle and handshake evidence remain open for Indy's review.

## Related pages

- [Resume M243 prompt](resume-prompt.md)
- [Separate M246 preparation prompt](m246-parallel-prompt.md)
- [Owner feedback audit](owner-feedback-audit.md)
- [Implementation guide and historical evidence](implementation-signoff.md)
- [File inventory](files.tsv)
- [Runner protocol](../../architecture/runner_protocol.md)


### Current checkpoint for Claude: three remaining unit failures

Indy requested this checkpoint pushed before these three tests are fixed.
Cargo check and formatting pass. Rust Clippy and dependency checks passed before OpenAPI regeneration.

The regenerated document now fails the source-prose check below.
The runner shard passes 1,685 tests; the daemon shard passes 164.

The final `make test-unit-all` run failed in the library shard.
It used Cargo's `--no-fail-fast` option and collected these remaining failures:

| Test and source | Actual failure and next action |
|---|---|
| `runner_plane::test_runner_bearer_state_matrix`, `afd_api/tests/runner_plane/mod.rs:78` | The response is `UZ-INTERNAL-001`; the assertion mistakenly expects `UZ-INTERNAL-003`. Codex changed the wrong matching assertion. Restore the database-query expectation here. |
| `runner_plane::the_native_stream_opening_is_guarded_like_every_other_runner_verb`, same file, line 244 | The fixture response is `UZ-INTERNAL-003`; the assertion still expects `UZ-INTERNAL-001`. `NoWork::open` deliberately returns `INTERNAL_OPERATION_FAILED` without a native connection. Correct this assertion instead. |
| `blackhole_fallback_cancellation_and_redaction_reach_exported_telemetry`, `af_stream/tests/transport_observability.rs:306` | The negotiated TLS log assertion fails. Exact connection, fallback and timeout metric assertions now pass with the single-address fixture. Determine which transport observation is missing; preserve both hybrid-group log assertions and redaction checks. |

The complete unit lane is still red; no full repository or code-completion claim applies.
Indy will assign these fixes to another Claude Code agent.
Codex will coordinate on issue 818 and continue outside those files.

The current generated OpenAPI has 115 paths and omits all ten retired HTTP operations.
Its native stream body remains binary; current lease/fold schemas and the existing 413 refusal are documented.
No retired route, compatibility alias, report file or backward-format retry was restored.


### Publication gate result

`orly gate work` and the staged secret scan pass.
`orly gate verify` passes configuration, documentation language and version 0.75.0 checks.
Its `make lint-all` command fails the source scan on one generated OpenAPI description.

The finding is `docs-api-internal-detail` at `public/openapi.json:8741`.
The protocol header description contains `agentsfleet.runner.v1`; its source is `afd_api_runner/src/handler/runner/stream.rs`.
Codex owns correcting that annotation and regenerating the document; Claude owns the three unit failures above.

No repository gate, test assertion or coverage floor was weakened to publish this owner-requested checkpoint.


### OpenAPI prose follow-up

The protocol header now says "Required runner protocol revision".
Its example comes from the existing protocol-version constant and remains `agentsfleet.runner.v1`.
The current daemon generator rebuilt OpenAPI successfully; the targeted `docs-api-internal-detail` scan passes.

`make lint-all` passed, including app, design-system and CLI checks.
The gate found two paragraph-formatting issues in these notes; those are fixed and the documentation scan passes.
No unit suite was restarted for this documentation correction.

Indy assigned the three remaining unit failures to Claude.
Codex will continue M243 and pull Claude's updates only when Indy signals.

### Claude's fixes for the three unit failures

Claude fixed all three in its own worktree, on top of `5c03ead30`.

| Test | Cause | Fix |
|---|---|---|
| `runner_plane::test_runner_bearer_state_matrix` | `25f5239e5` changed the wrong one of two matching assertions. | `743a64e89` expects `UZ-INTERNAL-001` again: a live runner reaches its handler and fails on the absent datastore. |
| `runner_plane::the_native_stream_opening_is_guarded_like_every_other_runner_verb` | `NoWork::open` answers `INTERNAL_OPERATION_FAILED` with a 500. | `743a64e89` expects `UZ-INTERNAL-003`; the 401 and `UZ-RUN-001` checks are unchanged. |
| `blackhole_fallback_cancellation_and_redaction_reach_exported_telemetry` | `h3/telemetry.rs` spawned the HTTP/3 security recorder as a bare task, which inherits neither the caller's span nor its subscriber. The HTTP/3 `runner_stream_tls_negotiated` record lost the session's fields and never reached the suite's scoped subscriber. | `e510404ba` runs that task in the current span and subscriber, as the HTTP/2 path's inline record does. No assertion changed. |

Results, run from `rustd/` with `--all-features`, no debug info, no incremental compilation and two Cargo jobs:

- `cargo test -p af_stream --test transport_observability`: 1 passed.
  With `e510404ba` removed it fails at its negotiated-TLS assertion, so the fix is the cause.
- `cargo test -p afd_api runner_plane::`: the runner plane suite passed 17, with 3 ignored.
- `cargo fmt --check` and `cargo clippy --all-targets -D warnings` pass for `af_stream` and `afd_api`.

The full `make test-unit-all` lane was not rerun here; one full Rust verification is still due on the combined branch.
M243 remains incomplete.

### Nov 09 native authoring source checkpoint

Indy prioritized authoring model calls, proposals, renewals and completion on one native session.
The runner commands and daemon dispatch keep those four operations on the existing work, control and reports streams.
No authoring HTTP route, compatibility alias or disk report storage was restored.

The source now rechecks connection authority before publishing previews or retaining a provider result.
It checks expiry again after waiting for the host lock.
A fenced result preserves authoritative provider usage while withholding private output and its continuation.

Accepted work requests remain in the daemon task tracker after their native connection closes.
They retain their charged input frame and request permit until completion, their bounded deadline or daemon shutdown.
This lets a physical provider call record its receipt even when its native reply cannot be delivered.

An unsettled authoring acknowledgement retains both the daemon lease cache entry and the test runner's worker registration.
Only settlement releases those owners.
Postgres continues to own saved results, receipt replay and billing; runner report retries remain memory-only.

`main/tests/integration_runner_stream/authoring/` contains twelve ignored integration cases.
They cover the four operations on one session, saved-result replay, changed replay content, disconnect recovery and one report identity under 100 contending callers.
They also cover cancellation, replacement during provider HTTP, stale connection and credential authority, wrong fences, pending accounting, private deletion and expiry during a host-lock wait.

`AuthoringFixture` is explicitly test-only.
It boots the real daemon and native runner, uses the real stores, and substitutes only provider HTTP with a local server.
The provider boundary has no environment option and leaves the guarded production client unchanged.

Indy stopped integration testing until code completion.
The first focused run was terminated by process ID after three cases printed `ok`; it has no complete suite verdict.
No integration pass is claimed, and no further integration run has started.

At Indy's subsequent signal, Codex fast-forwarded from `5c03ead30` to Claude's `b5e971269`, preserving the authoring changes.
That includes the HTTP/3 observer fix and the two corrected API fixture expectations.
The combined Rust unit lane and final lint are recorded below when they finish.

The browser setup chat keeps its existing draft API and event stream.
No UI source change is required for the daemon-to-runner authoring transport migration.

Combined unit result: `make test-unit-rustd` passed with 5,380 tests and zero failures.
The runner shard passed 1,685, daemon 164 and daemon libraries 3,531.
The library result includes Claude's three formerly failing tests.

`make check-version` passed: all versions match 0.75.0.
Integration remains paused, so the twelve new native authoring cases have no complete execution verdict.
The full `make test-unit-all` frontend coverage lane, integration, benchmarks and final shipping review remain due.

### Nov 09 authoring guard simplification

Indy asked Codex to study `openai/codex` and `exonum/exonum` and simplify the repeated authoring checks.
Both repositories were cloned outside this worktree for reference.
No code from them was executed or added as a dependency.

The relevant examples are [Codex ProviderAuthContext](https://github.com/openai/codex/blob/e54a5713caf331f42a6eef0166186df5f7563c29/codex-rs/model-provider/src/auth.rs#L40), [Codex NetworkPermit](https://github.com/openai/codex/blob/e54a5713caf331f42a6eef0166186df5f7563c29/codex-rs/http-client/src/network_policy.rs#L345) and [Exonum ExecutionContext](https://github.com/exonum/exonum/blob/2d2fa22e5f5bc451d08c155c2398956f11dce06e/exonum/src/runtime/execution_context.rs#L68).
They keep caller authority with the operation context and expose a checked boundary to callers.
The implementation applies that pattern to the existing `Drafts` owner.

`afd_authoring/src/lease/guard/connected.rs` now returns the complete guard for invocation, private publication or accounting.
Model dispatch and proposal callers each request one guard instead of mutating its timestamp and repeating connection checks.
The shared boundary retains the draft/attempt-before-host lock order and checks database time after the host wait.

The same boundary now protects native token counting and setup edit-intent preparation, reservation and result recording.
Late physical model results still retain known usage when private publication is refused.
Accounting keeps its independent path after private deletion or expiry.

The expiry test's `pg_backend_pid()` identifies only the test transaction holding the host lock.
Its observer proves the native request is waiting on that transaction before the test releases it after expiry.
No production authoring code queries backend process identifiers.

The full Rust unit pass recorded above preceded this simplification.
Compilation, final lint and the affected library unit shard are being checked again for the changed guard boundary.
Integration remains paused at Indy's instruction.

Post-simplification compilation passed: `cargo check --workspace --all-targets --all-features` completed in 44.42 seconds.
`make lint-all` passed, including source checks, rustfmt, workspace Clippy, app/design-system/CLI lint and repository safety checks.
OpenAPI was regenerated with the current daemon: 115 paths, the native stream and 413 response present, all authoring lease HTTP paths absent.
The generated bytes match the published document.

The post-simplification `make test-unit-rustd-daemon-libs` recheck passed: 3,531 tests, zero failures.
The first attempt was stopped unnecessarily after a misleading progress observation; the completed rerun used the unchanged Make target and assertions.
No test, gate, repository script or coverage floor was weakened.

Authoring implementation is ready for source review, with integration execution still pending.
M243's remaining scheduling, recovery/drain work, full review, benchmarks and final repository verification are not complete.
Keep PR 819 draft and retain Indy's listener, certificate and 1Password merge hold.

Before publication, Codex fast-forwarded to Claude's CI builder tip `292424604` without changing its files.
Those commits contain no Rust source changes; the authoring index was preserved.
Their workflow/Make-reference checks, playbook references, shellcheck and all 21 image-builder unit cases pass locally.
Claude still owns the full musl static builds and r2 image publication; no image was published by Codex.

### Nov 09 bounded scheduling passes

Authoring checkpoint `9c2803f918b8202b99b9b7bb534a0578e7ef2cea` was pushed successfully and PR 819 remains draft.
The PR body and issue 818 comment 6743278302 record that checkpoint's actual checks.

The next source change completes the scheduler mechanism for Dimensions 4.4 and 4.5.
`afd_gateway/src/scheduler/mod.rs` now runs one notification pass, with one lease of budget per observed wake generation.
It awaits each session's claim outcome, continues a host while it issues work and advances after an empty claim.
Notifications received during that pass form one subsequent pass; an uncertain outcome ends the current pass.

`session/offers.rs` transfers each request through a bounded session queue and a one-shot reply.
The existing supervisor still owns offer execution and keeps control, reports and authoring progress independent.
An abandoned queued request is skipped before issue.
Local capacity passes start on a credit increase or recovery from degraded state, instead of every heartbeat or unchanged Ready update.

Successful issue updates waiting order; merely requesting an offer does not.
Draining and superseded sessions are excluded from candidate selection.
The five-second probe starts after its first full interval and retains its own clock during notification work.

The focused unit check `cargo test -p afd_gateway --all-features --lib` passed 20 tests, with zero failures and one live-database test ignored.
Ten cases were added: eight pass/registry tests, one credit-rise test and one replica-probe test.
Increasing the production pass budget by one made the one-ring test fail at its extra-claim assertion; the mutation was removed before the green run.

The first probe test attempt assumed equal-priority hosts would be visited in insertion order and was stopped without a suite verdict.
The corrected fixture answers all 50 real session request queues concurrently, under a deadline; no scheduler check was weakened.
The test proves probe frequency and pass dispatch with a controlled probe result, without running a live datastore test.

A full changed-file count found 14 first-party Rust files still over the 350-line ceiling.
Those corrections, whole-PR review, remaining native fault scenarios, benchmarks and final repository lanes remain due.
Integration stays paused until code completion; the twelve authoring cases still have no complete runtime verdict.

### Latest GitHub pull and next-session order

Indy requested a checkpoint push and context save, then explicitly asked to pull the latest GitHub work and push again.
Codex fetched both main and the feature branch, then fast-forwarded from `9c2803f91` to `3e3d8f1b0` with all local scheduler work preserved.
The pulled commit adds Claude's licensed `datagram-socket` musl patch under `rustd/patches/`, its workspace exclusion and lockfile resolution.
Codex did not edit those incoming files or Claude's CI-builder files.

Claude reports the arm64 static daemon build passed at that incoming checkpoint.
In [issue 818 comment 6743710519](https://github.com/agentsfleet/agentsfleet/issues/818#issuecomment-6743710519), Claude reports the r2 builder image published for both `linux/amd64` and `linux/arm64`.
The three Linux build jobs on `3e3d8f1b0` ran before publication and failed to pull that image.
Claude stopped the local amd64 emulation builds and will rerun those jobs on native CI runners after run 42641151625 completes.
Those static-build results and any upstream report remain open; image publication itself is complete according to that coordination update.
Local tooling reports an unused `IoSlice` import in the vendored dependency; retain that fact when reviewing the fork.
Vendored upstream files are outside the first-party length count below.

The checkpoint lint initially found the new recovery module's directory layout and test-only reply/error assertions.
The module moved to `scheduler/recovery/mod.rs`, including its relative SQL path, and the assertions now retain their actual outcomes.
No lint rule, test assertion, floor or gate was disabled.

Continue in this order:

1. Fetch the feature branch again and incorporate any newer published work while preserving this checkpoint.
2. Finish the source review and fixes for drain, recovery, cancellation, renewals, folds and report retries. Reconcile actual tests against the active specification rather than treating old handoff lists as new compiler failures.
3. Review `SessionSupervisor::drain` specifically: it cancels readers and joins dispatched jobs. The treatment of frames already queued by readers still needs a source decision and a native test; this is a review question, not a confirmed runtime verdict.
4. Finish first-party file and function length corrections, full Rust ownership/error review, native failure/fairness test sources, benchmark source and documentation/OpenAPI reconciliation.
5. Tell Indy when code is complete. Then run the combined repository verification: lint, unit/coverage, native integration including authoring, version, benchmarks and applicable Orly shipping gates.

Routine formatting, compilation, lint, unit checks and gates remain authorized.
Do not repeat unchanged full shards merely for status, and do not start integration before code completion.
An existing green command is evidence for its recorded revision and scope only.

| First-party Rust file | Lines at this checkpoint |
|---|---:|
| `afd_fleet/src/error/classify.rs` | 658 |
| `afd_fleet/src/task/mod.rs` | 574 |
| `afd_fleet/src/lease/mint/mod.rs` | 406 |
| `afd_authoring/src/error/mod.rs` | 404 |
| `afd_api/src/openapi/mod.rs` | 395 |
| `afr_supervisor/src/lease_loop/mod.rs` | 386 |
| `afd_events/src/steer/mod.rs` | 381 |
| `main/src/serve/mod.rs` | 367 |
| `afd_fleet/tests/support/fleet_report_seed.rs` | 364 |
| `afd_fleet/src/lease/report/mod.rs` | 362 |
| `af_stream/src/error/mod.rs` | 357 |
| `afd_fleet/tests/integration_renew_coverage.rs` | 353 |
| `main/tests/cli.rs` | 352 |
| `main/src/serve/drain/mod.rs` | 352 |

Paths above are relative to each crate's layer under `rustd/crates/`; `main` is the daemon binary.
No completion or ready-for-review claim accompanies this context save.
PR 819 stays draft, with merge held for Indy's listener, certificate and 1Password review.

### Checkpoint checks after pulling `3e3d8f1b0`

| Command | Actual result |
|---|---|
| `make lint-rustd` | Passed: 44 source-rule tests and clean scan, 16 coverage-harness tests, rustfmt, workspace all-target/all-feature Clippy and cargo shear |
| `cargo check --workspace --all-targets --all-features --locked` | Passed in 20.65 seconds |
| `cargo test -p afd_gateway --all-features --lib --locked` | Passed: 20 tests, zero failures, one live-database test ignored; build finished in 4.92 seconds |
| `make check-version` | Passed: all versions match 0.75.0 |
| `orly gate work` | Passed: repository configuration declares four commands; this work gate does not replace final verification |
| `gitleaks protect --staged --redact --no-banner` | Passed for the checkpoint index; no leaks found |

The last two logs are `/tmp/m243-scheduler-check.log` and `/tmp/m243-scheduler-unit.log` when reusing this environment.
Cargo reports the vendored unused import described above; cargo shear retains its existing five unlinked `afd_api` fixture warnings and exits successfully.
No full Rust shard or frontend coverage lane was rerun for this scheduler context save.
The full `make lint-all` and Rust shard results earlier in this file belong to their recorded authoring revisions.
No integration test restarted; full repository and shipping verification remain due after code completion.
