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
The preceding published checkpoint was `a0a8cf3b766d74fb7984d332cbe5cc49010bc0a0`.

**The implementation is incomplete and has no passing full compilation, lint or test lane.**
Authoring daemon dispatch is written; kernel examples and remaining fixtures still need migration.
Do not present this source checkpoint as usable, code complete, ready to undraft or a benchmark baseline.

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
- Require `X25519MLKEM768` on both ends and carriers. Transport fallback must not weaken cryptography.
- Keep private Fly routing. External runners need an explicit route to the private listener.
- Remove obsolete polling, old report shapes and compatibility retries. Add no aliases or legacy delivery path.
- Postgres owns saved results and billing. Unacknowledged results stay only in caller memory.
- Preserve M244's work and coordinate overlapping interfaces on issue 818.

### Completed-report storage removal

The source deletes `afr_supervisor/src/report_spool/` and `src/drainer/`, including authoring and task-decision receipt files.
It also removes startup replay, quarantine, encryption helpers, directory creation and old-shape field-stripping retries.
Do not restore these to repair unfinished fixtures.

`lease_loop/settle.rs` retains one `Zeroizing<Vec<u8>>` report buffer in its worker until delivery ends.
`report/delivery.rs` retries the current operation from that caller.
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

The latest complete unit-lane attempt stops during compilation of sandbox kernel examples.
They still import or use the deleted `RUNNER_REPORTS` constant:

- `afr_sandbox/examples/kernel_lane/supervisor_checks.rs`.
- `afr_sandbox/examples/kernel_lane/supervisor_plane.rs`.
- `afr_sandbox/examples/kernel_lane/supervisor_recovery/mod.rs`.

Migrate those examples to native sessions; do not restore polling/report-route constants to silence the compiler.
Continue any remaining runner test signature fixes after that build reaches them.
The ignored `.tmp/rustd-unit-runner.log` contains this machine's latest attempt; reproduce it in a fresh checkout.

A test-only `afr_supervisor::test_util::Session` now uses the production downlink, correlation and recovery owners.
It receives offers, retains registrations until report acknowledgement and exposes typed native operations.
`main/tests/support/runner_stream.rs::client` shares that session through the scenario's `OnceCell`.

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

1. Finish the kernel example and end-to-end fixture migration above, then rerun the complete Rust build and unit lanes.
2. Migrate remaining `LeaseResponse`, `Verb::Lease`, `.lease()`, `.claim()` and deleted path-constant callers in tests, examples and benchmarks.
3. Port HTTP-only fixture constructors through the native test seam. Supply required stream configuration in external fixtures.
4. Replace held-heartbeat end-to-end tests and stale downlink policy tests with session behavior.
5. Replace drain and saturation benchmark polling. Dense idle workloads, fairness and fault matrices still need source coverage.
6. Add and run authoring native integration cases, connection fencing, provider-dispatch replay, cancellation, renewals, folds and report retry cases.
7. Review worker policy changes while waiting for offers and shutdown bounds during a report outage.
8. Complete the whole-PR Rust review, including RS-SENTINEL, RS-UNWRAP, public docs, function sizes and causes.
9. Reconcile manifest changes and Cargo.lock only when the permitted dependency-generation step is clear.
10. Reconcile canonical and public docs; generate OpenAPI through its generator after approval, never by hand.

Tool-trace parsing now returns its error, and invalid authoring call limits no longer become zero-call runs.
Remaining examples include `report_workers` saturation and listener failure handling that drops typed causes.
Main's terminal `BootFailure` and `MigrateFailure` are deliberate exceptions in `RUST_ERROR_STANDARD.md`; do not migrate them unnecessarily.

`claim_selected` now returns a direct optional tagged lease in test utilities.
Its encoder closure may require explicit argument types when the daemon libraries compile.
Reconcile all rewritten test modules with their test-only unwrap policy.

### TLS deployment and certificates still open

The source does not prove where production TLS ends.
Define the private hostname, direct TCP/UDP exposure and native acceptor termination before claiming the hybrid policy applies in production.
No deployment or workflow file was changed for this checkpoint.

Pinned Quiche 0.30.0 sets SNI and `X509_VERIFY_PARAM_set1_host` in `src/tls/mod.rs::set_host_name`.
Wrong-host rejection test sources exist for both carriers, without runtime results.
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
Newly oversized files include `downlink/mod.rs`, `main/src/serve/mod.rs` and `main/src/serve/drain/mod.rs`.
Several earlier touched domain files exceed 350 lines too; a green Clippy run would not prove this review rule satisfied.

### Cloud build tools used here

This cloud image lacked cmake and libclang.
A user-owned virtual environment at `/workspace/.cloud-tools/native-build` supplies cmake 4.4.4 and libclang 18.1.1.
The library path is `/workspace/.cloud-tools/native-build/lib/python3.12/site-packages/clang/native`.

Rust lanes used that environment's `bin` directory on PATH and the library path through `LIBCLANG_PATH`.
`BINDGEN_EXTRA_CLANG_ARGS` included `-I/usr/lib/gcc/x86_64-linux-gnu/14/include` for builtin C headers.
Keep two Cargo jobs, zero development/test debug info and incremental compilation disabled, as `.orly/LOCAL.md` requires.

These tools are machine setup outside the repository; a fresh cloud session must inspect its own installed tools.
Root and CLI Bun dependencies were installed from their frozen lockfiles.
Remaining UI package installs, CLI build and live-datastore setup may still be needed before the full repository lanes.

## Limits

Indy initially held verification, then approved publication scans and explicitly authorized compilation, lint and tests.
His latest steering prioritizes committing, pushing and handing off the actual work.
This checkpoint records current failures and must remain draft.

| Command | Actual result |
|---|---|
| `orly doctor` | Passed: local rules match and Codex tools are installed. |
| `orly gate work --accept-dirty` | Passed: four configured commands declared. This does not compile or test the source. |
| `gitleaks protect --staged --redact --verbose --no-banner` | Passed on the final staged checkpoint before commit. |
| `make lint-source` | Passed: 44 rule tests and source scan after the source changes. |
| `cargo fmt --all` | Completed successfully after the last source edit. |
| `make lint-rustd` | Failed at rustfmt before the formatting pass. No completed Clippy/shear result. |
| `make test-unit-rustd` | Fails during compilation; latest recorded blockers are kernel example references to removed report routes. No completed unit lane. |
| Full unit, integration, version and shipping gates | Not passed for this checkpoint. |

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
