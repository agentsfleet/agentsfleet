---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# M243 owner feedback audit

## What it is

This is a source audit of Indy's feedback on draft [PR 819](https://github.com/agentsfleet/agentsfleet/pull/819).
The date records source reading; verification results are recorded separately below.
The resume source has passing Rust lint and focused tests, while the complete repository verification remains unfinished.

Status meanings:

- **Changed in source:** the named edit exists; its required runtime evidence remains due.
- **Partial:** some requested work exists and the row names what remains.
- **Explained:** the answer follows the cited source; this does not certify the implementation.
- **Pending:** the requested change or evidence has not been completed.

No row below means the whole PR is ready.

## Why it exists

Indy requested: "an audit of all my feedback the changes that you did".
Earlier replies blurred existing code, approved decisions and unverified changes.
This audit separates those states and records concrete outstanding work.

## How it behaves

### Report storage, billing and simplicity

| Feedback | State | Source evidence and remaining work |
|---|---|---|
| Remove report spool files | Changed in source | Deleted `afr_supervisor/src/report_spool/` and `src/drainer/`, including fleet and authoring disk storage, startup scans, file replay and quarantine. [`settle`](../../../rustd/crates/runner/afr_supervisor/src/lease_loop/settle.rs) keeps one zeroing report buffer in its worker. |
| Retry only from memory | Changed in source | [`until_acknowledged`](../../../rustd/crates/runner/afr_supervisor/src/report/delivery/mod.rs) retries the caller's operation and returns terminal failures. The worker retains its slot until acknowledgement; no replacement report queue was added. |
| Remove backward-format fallback and the memory-window version guess | Changed in source | Deleted `reshaped`, `fall_back`, `ShapeRefused`, `forget_rejected`, field stripping and the separate settle-time memory push. The current report carries memory and timing segments regardless of whether its lease included a memory window. |
| Billing must not depend on files | Changed in source | Deleted the separate local decision-receipt store and its drainer too. [`verbs/decision.rs`](../../../rustd/crates/runner/afr_supervisor/src/verbs/decision.rs) retains its receipt in the calling task until the daemon acknowledges it. No task-decision receipt directory is created. |
| Keep database authority | Existing server behavior retained | [`task/decision/dispatch.rs`](../../../rustd/crates/daemon/afd_fleet/src/task/decision/dispatch.rs) commits issuance before returning a dispatch. Its locked issued/accounted check returns an existing result on repetition. [`lease/commit.rs`](../../../rustd/crates/daemon/afd_fleet/src/lease/commit.rs) commits completion and usage in Postgres. No SQL change was made for report-file removal. |
| Explain what a report is | Explained | [`ReportRequest`](../../../rustd/crates/protocol/af_wire/src/report/mod.rs) declares the final answer, outcome, identifiers, fencing token, usage, checkpoint, optional tool trace, memory changes and timing. This completes work through the daemon; it is not a new Grafana analytics feed. |
| Is this the previous reports API? | Explained | Fleet report delivery uses [`Up::Report` / `Down::ReportAck`](../../../rustd/crates/protocol/af_wire/src/protocol/up/mod.rs). The previous HTTP report handler is deleted. The daemon retains its report operation and database writes. |
| Why add `authoring/report.rs`? | Corrected in source | The temporary extra file was removed. The existing [`authoring/mod.rs`](../../../rustd/crates/runner/afr_supervisor/src/authoring/mod.rs) owns memory-only authoring delivery. Its current accounting-only form after private-answer expiry is a privacy rule, not an old daemon format. |
| Keep things simple; add features later | Changed in source | Each current worker owns its result and retry future. No disk outbox, recovery scanner, receipt encryption store or separate completed-report drainer remains. |
| Tell Indy when spool removal is done | Source removal complete; unverified | Source and callers are migrated. Regression test sources cover unchanged report retry, refusal without rewriting, no report directories and receipt retry without another reservation. They have not run. |

The selected owner answer was: "Remove runner disk storage; retry from memory".
Indy also required: "No backward compatibility crap, when you remove storage reports."
No migration reads old report files into memory or Postgres.

### Disk-code history and approval evidence

Git follows the original report-file module to commit `830cb0c4b8711c9bfd97aae81d8a450cc56e1eb6`, dated Oct 03, 2026.
The file-writing path also exists in the published M243 checkpoint `a0a8cf3b766d74fb7984d332cbe5cc49010bc0a0`.
That original commit is already an ancestor of fetched `origin/main` at `61820b66a765a2aa87684251b07aebebd2c20955`.

It is not a commit introduced by PR 819.
The report-spool module is identical between that main revision and published checkpoint `a0a8cf3b7`.
`git merge-base --is-ancestor 830cb0c origin/main` returned zero; the PR-range file log and diff both returned no changes.

The mistake in M243 was retaining that mechanism as an agent decision and presenting retention as agreed.
Commit authorship does not establish who proposed the behavior or whether Indy approved it.
No explicit owner approval for retaining disk report storage in M243 was found.

The earlier specification labeled retention as an agent decision; this session rejected and removed that decision.

### Pending-memory ownership

The local `bytes` value in `lease_loop/settle.rs` owns the serialized fleet report through `Zeroizing<Vec<u8>>`.
Returning from delivery, returning a terminal error, or dropping the future drops that wrapper and zeroes its allocation.
The separate `CapturedMemory` owner also zeroes its owned text through `Drop`.

Native `OwnedFrame` payloads use `Zeroizing<Vec<u8>>` too.

The `Pending` value in `authoring/mod.rs` explicitly zeroes its owned private answer in `Drop`.
These are source paths, not evidence that every serializer, HTTP, TLS or operating-system copy is wiped.
Whole-PR ownership review and the approved memory tests remain due.

### Removed code and test coverage

Disk replay, file-permission, disk-clock recovery and old-shape fallback tests were deleted because their production behavior was deleted.
This is a removal record, not an approved feature, deferred implementation or permission to restore disk storage.
Root lifecycle tests coupled to polling and disk replay still need replacement through the native session fixture.

```text
worker finishes
      |
      v
current result in worker memory
      |
      +---- send ----> daemon ----> Postgres commit
      |                                  |
      +<----------- acknowledgement -----+
      |
release worker and pending memory

disconnect before acknowledgement -> same worker retries
process loss before commit        -> unsent result or usage can be lost
commit succeeds; reply is lost    -> retry reads saved result without another charge
```

Acknowledged usage stays in Postgres.
A process lost before recording final usage can leave missing or unknown provider usage.
Neither a trace nor a fabricated successful report resolves that uncertainty.

### Original diff comments

The numbers below match the 23 numbered comments Indy supplied.

| Comment | Request | State | Change, source and remaining work |
|---|---|---|---|
| 1 | Explain `self.0` and initialization | Changed in source | [`Gateway`](../../../rustd/crates/daemon/afd_gateway/src/lib.rs) now names its shared field `inner: Arc<Inner>`. `new` initializes it; `in_flight` reads its task tracker. Provisional-task counting still needs shutdown tests. |
| 2 | Explain `preflight::runner_stream` | Changed in source | Renamed the preflight module to [`runner_listener`](../../../rustd/crates/daemon/main/src/preflight/runner_listener.rs) and its type to `RunnerListenerConfig`. This parses daemon listener settings. `af_stream` owns the HTTP/2 and HTTP/3 transport abstraction. |
| 3 | Are certificate inputs post-quantum safe? | Changed in source; evidence pending | [`key_exchange`](../../../rustd/crates/protocol/af_stream/src/tls/key_exchange/mod.rs) now requires the hybrid group on both transports and ends. Classical certificate signatures remain a separate concern. Required tests have not run; production route evidence remains due. |
| 4 | Standard human-readable and agent-readable errors | Partial | [`runner_listener.rs`](../../../rustd/crates/daemon/main/src/preflight/runner_listener.rs) now hoists actionable address, certificate and private-key messages into constants. Preflight retains the knob. Full error-code, cause-chain and wire-refusal review remains due. |
| 5 | Tracing across listener startup | Partial | [`serve/runner_stream.rs`](../../../rustd/crates/daemon/main/src/serve/runner_stream.rs) logs startup, stop and failure. Failure causes, connection correlation and negotiated-group observations still need completion. |
| 6 | Copy and study Exonum and Codex without `.git` | Partial | Sparse source copies exist under ignored `.tmp/reference-sources/`. Exonum revision: `2d2fa22e5f5bc451d08c155c2398956f11dce06e`. Codex revision: `8703d32bd0ea5cf26768859a3dd476df906ba053`. Exonum cleanup ownership was read. A complete named comparison and application to listener supervision is still due. |
| 7 | Why `STARTUP_DRAGONFLY_CONNECT`? | Explained; integration pending | [`main/src/error.rs`](../../../rustd/crates/daemon/main/src/error.rs) maps the existing `Queue` boot error to that code. Runner listener errors are separate. M244 owns the NATS replacement; this branch has not integrated that work yet. Renaming the old code alone would misdescribe the backend. |
| 8 | What is the proxy? | Explained | [`af_stream/src/proxy/mod.rs`](../../../rustd/crates/protocol/af_stream/src/proxy/mod.rs) handles operator-configured outbound proxy settings. HTTP CONNECT carries end-to-end daemon TLS over HTTP/2. Direct private routing does not need that proxy. Comments 11–13 record the written simplification and unrun tests. |
| 9 | Tracing throughout the PR and meaningful comments | Partial | Transport, reconnect and worker events exist in [`af_stream`](../../../rustd/crates/protocol/af_stream/src/telemetry.rs) and [`downlink`](../../../rustd/crates/runner/afr_supervisor/src/downlink/mod.rs). This is not a whole-PR tracing audit. Failure pairs, span context, sensitive-data exclusions and bounded labels remain due. |
| 10 | `Zeroizing` coverage and Miri proof | Partial | Fleet pending report bytes now use `Zeroizing<Vec<u8>>`; authoring pending answers are zeroed on drop. Proxy credentials already have zeroing temporary buffers. `HeaderValue::set_sensitive` is not memory wiping. [The existing Miri job](../../../.github/workflows/test-unit-rustd.yml) targets `afd_authoring --test secret_wiping`; it does not prove wiping across this PR. Full ownership audit remains due; no workflow edit was made. |
| 11 | Simplify proxy bypass and environment parsing | Changed in source | [`proxy.rs`](../../../rustd/crates/protocol/af_stream/src/proxy/mod.rs) now reuses hyper-util Matcher for standard NO_PROXY host/network matching. Injected EnvSource remains the only input. Proxy URLs are validated before matching so invalid configuration cannot select direct routing. Unrecognized bypass entries keep the explicit proxy. |
| 12 | Use `TryFrom` for proxy conversion | Changed in source | `Proxy::try_from(&str)` validates bounds and parses the supported proxy URL. The retained URI excludes credentials. Conversion and failure tests have not run. |
| 13 | Separate proxy conversion from socket creation | Changed in source | `Request<Outgoing>::try_from(&Endpoint)` constructs CONNECT headers synchronously. The named asynchronous `socket` function owns connection, upgrade and transfer failures. No asynchronous From conversion was invented. |
| 14 | Share repeated client request/decode computation | Changed in source | [`client/stream.rs`](../../../rustd/crates/runner/afr_supervisor/src/client/stream.rs) now uses `StreamHandle::compute` for correlation, request and decode. Heartbeat, renewal batches, reports, folds and ping declare their typed response. Tests remain unrun. |
| 15 | Typed client command abstraction | Changed in source | Each request implements `Command`; `Outcome::try_from(Answer)` resolves refusal codes once. Both periodic renewal batches and final renewal use this computation. Activity remains an unacknowledged send. |
| 16 | Explain `ENV_STREAM_CA` and `ENV_STREAM_URL` | Explained | [`config/stream.rs`](../../../rustd/crates/runner/afr_supervisor/src/config/stream.rs) names the daemon HTTPS origin and optional extra root-certificate file. The CA file is neither a runner client identity nor a CA signing key. |
| 17 | Follow the existing environment/TOML standard | Explained; configuration review pending | The implementation uses injected `af_core::env::EnvSource`, matching [`config/mod.rs`](../../../rustd/crates/runner/afr_supervisor/src/config/mod.rs). No TOML configuration layer was added. Typed validation and operator documentation still need final review. |
| 18 | Simplify connection opening | Partial | [`Opening`](../../../rustd/crates/runner/afr_supervisor/src/downlink/connect.rs) collects opening inputs and separates request construction, hello, confirmation, secondary attachment and response parsing. Cleanup and opening-failure tests remain due. |
| 19 | Distinguish connect from dispatch | Explained | [`connect.rs`](../../../rustd/crates/runner/afr_supervisor/src/downlink/connect.rs) establishes and authenticates a connection. [`dispatch.rs`](../../../rustd/crates/runner/afr_supervisor/src/downlink/dispatch.rs) routes established-session messages to replies, offers, cancellation and policy. |
| 20 | Prove performance and absence of races | Pending | Bounded queues, correlation cleanup and cancellation ownership are source mechanisms, not proof. Contention, lost wakeups, credit changes, disconnect races, fairness and benchmark evidence are still due. No race-free or optimized claim is made. |
| 21 | Why `parking_lot::Mutex`? | Changed in source | [`Running`](../../../rustd/crates/runner/afr_supervisor/src/downlink/running.rs) now keeps capacity and registrations in one `State` under a short lock. Duplicate detection, length bound and insertion remain atomic together. Cancellation callbacks run after releasing the lock. Tests remain due. |
| 22 | Look for existing DashMap and use it appropriately | Changed in source | The existing [`afd_outbound` pattern](../../../rustd/crates/daemon/afd_outbound/src/lanes/mod.rs) was read. [`Renewals`](../../../rustd/crates/runner/afr_supervisor/src/uplink/renewals.rs) now uses named DashMap entries and an independent permit bound. It releases shard guards before meter reads and watch updates. |
| 23 | Simplify renewal functions | Changed in source | The ticker, one-batch exchange, snapshot and reply application have separate methods. Snapshots retain native UUID values without reparsing local strings. Reply decoding and registered refusal conversion use the shared command computation. Runtime evidence remains due. |

### TLS findings supplied after the diff comments

Transport Layer Security (TLS) protects the runner channel.
HTTP/2 fallback after an eligible HTTP/3 transport failure remains approved.
Classical cryptographic fallback is a different decision and is now rejected by Indy.

| Finding | State | Required change or evidence |
|---|---|---|
| Hybrid-only `X25519MLKEM768` on both ends and both transports | Changed in source | [`key_exchange`](../../../rustd/crates/protocol/af_stream/src/tls/key_exchange/mod.rs) restricts both HTTP/2 contexts and each actual QUIC TLS session before handshaking. Negotiated groups are checked again. Classical-only rejection sources cover the connector and native acceptors; none have run. |
| Define and prove the production TLS endpoint | Pending | Document exact private listener hostname, TCP/UDP reachability and direct TLS termination. Prove negotiated group on that route. No deployment files changed; a code-only test cannot prove production routing. |
| Verify certificate hostname on HTTP/3 | Source path established; runtime evidence pending | Pinned Quiche 0.30.0 `src/tls/mod.rs::set_host_name` sets SNI and `X509_VERIFY_PARAM_set1_host`. The connector supplies `endpoint.host()`. [`failures.rs`](../../../rustd/crates/protocol/af_stream/tests/failures.rs) contains wrong-host rejection cases for both transports; none have run. |
| Issuance, root delivery and rotation before expiry | Pending | Extra roots come from a file. No approved issuer or completed live certificate-reload lifecycle is established by this branch. Prepare a concrete design for Indy's listener/certificate/1Password review. |
| Daemon negotiated-group telemetry | Partial | `runner_stream_tls_handshakes_total` uses closed transport, peer, group and outcome labels. Both acceptors observe the negotiated group; QUIC observations run outside the native callback. A failed QUIC handshake with no group records `other/failed`. HTTP/2 pre-negotiation rejection telemetry and runtime evidence remain due. |

The requested changes to TLS have not been completed by report-file removal.
No certificate, private route or deployed handshake has been exercised in this resumed session.

### Overall code-completion requests

| Owner request | State and remaining work |
|---|---|
| Listener startup, drain and runner session integration | Partial source wiring. Finish transport shutdown ownership and verify accepted-work drain. |
| Reconnect, cancellation, renewals, folds and report retry | Partial source wiring. Memory-only result delivery is written; native failure and replay tests remain due. |
| Remove polling routes and migrate callers/tests | HTTP handlers and runtime methods have removals written. Old polling fixture types, route constants, benches and remaining callers are not all migrated. |
| Whole-PR Rust review, failure cases, fairness, observability and benchmarks | Pending completion. RS-SENTINEL, RS-UNWRAP, function sizes, causes and ownership need full review. |
| Reconcile docs and generated OpenAPI | Report-storage design text updated. Generated OpenAPI now matches the current source; public documentation review remains due. |
| Make PR consumable and merge-ready | Incomplete. PR stays draft; earlier test results do not establish current readiness. |
| Verification authorization | Indy initially held checks, then explicitly approved publication scans and compilation/lint/tests on Nov 09. Source lint passed; compilation and remaining gates have not passed. See the handoff. |
| Preserve M244 and coordinate on issue 818 | Coordination comments posted. M244 remains the subsequent PR; shared Bus, startup, live-stream and Cargo changes must be reconciled without importing its whole deployment migration into M243. |
| Read issue 813 before every push | Issue and comments read during context-save preparation. Read again before a later push if new coordination may have arrived. |
| Save context and push all source and handoff prompts | Handoff, M243 resume prompt and separate M246 preparation prompt are written. Publication status is the actual branch/PR commit; this checkpoint remains incomplete and unverified. |
| Use one runner channel for authoring too | Partial. Four wire exchanges and runner commands are written. Gateway dispatch, connection-authority integration and HTTP route deletion are written. Fixture migration and native integration checks remain unfinished. This belongs to M243. |

## Limits

This section describes limits of the audit evidence, not report-storage policy or approved future features.
Regression sources were added or rewritten without running them.

The scripted runner test seam now answers current frames through the real reply correlator.
That seam does not prove HTTP/2 or HTTP/3 socket behavior.
Several existing HTTP-only fixture constructors and old polling tests still need migration.

Source review found a credit race: a Ready update could restore capacity before an in-flight offer reached the runner.
Ready now carries its revision and the count of offers the runner has observed.
The gateway ignores stale revisions and deducts unseen offers from advertised capacity.

An offer consumes one worker; any other free worker remains eligible immediately.
[`credits_tests.rs`](../../../rustd/crates/daemon/afd_gateway/src/session/credits_tests.rs) covers a crossing update, stale updates, impossible acknowledgements and two workers receiving independent offers.
These regression sources have not run.

The audit does not certify Orly compliance or mark the PR code complete.
Before verification, finish the pending source changes and reconcile each affected test caller.
Indy has now authorized the required commands; do not request that approval again.

## Related pages

- [Current session handoff](session-handoff.md)
- [Implementation sign-off and historical evidence](implementation-signoff.md)
- [Runner protocol](../../architecture/runner_protocol.md)
- [Failure and observability design](failure-and-observability.md)
- [Active M243 specification](../../v2/active/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md)

### Nov 09 authoring clarification

The four retained HTTP calls are active runner operations for fleet setup chat.
Indy requested that this work also use the stream.
M245–M247 do not already own that migration.

Wire and runner changes place model calls and proposals on work, renewals on control, and final reports on reports.
Gateway dispatch and deletion of those HTTP endpoints are written; caller migration and native integration evidence remain unfinished.
Preserve the daemon-owned provider capability, physical dispatch and Postgres accounting rules while finishing this migration.

No completion or verification is claimed for this extension.
The [handoff](session-handoff.md) records current compiler failures, review fixes and remaining migration tasks.
Other per-lease HTTP operations remain outside this four-call correction; this checkpoint does not claim every runner API uses the stream.


### Nov 09 publication state

Indy requested the commit, push and handoff while the native fixture migration remains incomplete.
The handoff records the passing setup/work/secret checks and source lint, plus the failing Rust compilation attempts.
This publication is a source checkpoint, with no green-build or code-completion claim.

Initial Orly review found ownership, cancellation, retry classification and zeroing failures.
The corresponding source corrections and unrun regression cases are listed in the handoff.
Positive-pass scheduling/coalescing, file lengths, native integration, benchmarks and final full-PR sign-off remain unfinished.

### Nov 09 resume results

The resume work fixes the kernel example compilation without restoring `RUNNER_REPORTS`.
Native server fixtures replace the retired poll/report handlers in runner examples and binary tests.
Obsolete HTTP assertions are removed or moved to the current wire, native-session and settlement owners.

Tests retain explicit checks that retired HTTP routes are absent.

A native report with a tool trace exposed a decoder defect: Serde's tagged-enum buffer could not deserialize its borrowed raw JSON.
The current report schema is now selected before that buffering, preserving the JSON shape and borrowed raw trace.
Valid and malformed trace tests preserve the answer; extra frame and request fields remain refused.

No backward-format decoding or report storage was added.

| Evidence | Actual result |
|---|---|
| `make lint-rustd` | Passed through source checks, rustfmt, workspace Clippy and cargo shear before OpenAPI regeneration; the current generated JSON has one source-prose finding. |
| `cargo check --workspace --all-targets --all-features` | Passed from `rustd/` in 1 minute 5 seconds after the latest fixes; `cargo fmt --all --check` also passed. |
| `make test-unit-rustd` | Runner: 1,685 passed. Daemon: 164 passed. Three library tests still fail; exact cases and ownership are in the handoff. |
| Supervisor library in the runner shard | 396 passed, zero failed. Covers the migrated report, renewal, cancellation and batching cases; not a full repository result. |
| `cargo test -p af_wire --all-features` | 79 library tests and 103 suite tests passed, zero failed. Includes strict native renewals/folds and raw report-trace regressions. |
| Generated OpenAPI | Current daemon generator completed: 115 paths, no retired operation paths, binary native body and 413 refusal documented. |
| Full repository lanes and shipping gates | Still due. No code-completion claim. |

The scripted fixture starts production renewal batching and waits for unacknowledged activity observations before asserting them.
The report retry cases now lose acknowledgements across native answer deadlines and settle the same in-memory result.
A hard stop ends delivery; stopping a lease or draining the caller leaves its final acknowledgement path available.

The telemetry regression that previously hung passed in 0.55 seconds after the decoder fix.

Cloud tests run under a temporary child subreaper with a lane-scoped `umask 022`.
The executor's process disappearance assertions remain unchanged; process 1 otherwise leaves killed children as zombies.
No repository script, workflow, deployment configuration or coverage floor was changed.

PR 819 remains draft at the published checkpoint until the next verified push.


### Current push authorization and remaining failures

Indy requested: "can you push and keep going i will ask another claude code agent to fix this".
This supersedes waiting for a green unit lane before publishing the checkpoint.
The [handoff](session-handoff.md#current-checkpoint-for-claude-three-remaining-unit-failures) names the three remaining failures and source locations.

Cargo check and formatting pass. Rust lint passed before OpenAPI regeneration.
The regenerated document now fails one source-prose check.

The complete unit lane remains red; frontend coverage, live integration and final shipping verification are still due.
PR 819 remains draft, with merge held for Indy's listener, certificate and 1Password review.


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


### The three unit failures

Claude fixed the three failures the handoff assigned: `e510404ba` for the HTTP/3 telemetry record and `743a64e89` for the two runner guard codes.
No assertion, metric count, hybrid-group check or redaction check was weakened; the telemetry fix is in source.
The [handoff](session-handoff.md#claudes-fixes-for-the-three-unit-failures) lists the targeted results.

The full unit lane and the final Rust verification on the combined branch are still due.
PR 819 remains draft, with merge held for Indy's listener, certificate and 1Password review.
