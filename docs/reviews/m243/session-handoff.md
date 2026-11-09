---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# M243 session handoff

## What it is

Resume M243 from the commit introducing this file, on `feat/m243-runner-push-stream`.
The existing draft is [PR 819](https://github.com/agentsfleet/agentsfleet/pull/819).
Coordination with Claude's M244 is [issue 818](https://github.com/agentsfleet/agentsfleet/issues/818).

This is an unverified source checkpoint requested by Indy for review and a new session.
M243 is not code complete, ready for merge or a final benchmark baseline.
Do not infer a pass from source changes or reuse older gate results for this checkpoint.

## Why it exists

Indy requested code completion before local verification, followed by a review checkpoint and context handoff.
The source review uses Orly's Rust and error standards, including RS-SENTINEL and RS-UNWRAP.
A completed formal review and verification pass remain due.

## How to resume

Read `AGENTS.md`, `.orly/LOCAL.md`, the `orly-build` skill and the active M243 spec before editing.
Reuse the existing worktree when available; otherwise fetch the published feature branch into an isolated worktree.
Do not restart from main or rebase and force-push this published branch.

| Context | Value |
|---|---|
| Existing worktree | `/workspace/agentsfleet-m243` |
| Branch | `feat/m243-runner-push-stream` |
| Refreshed main | `61820b66a765a2aa87684251b07aebebd2c20955` |
| Previous pushed checkpoint | `0b87ec9ca597ac8909f76c6ef2ae90e0c2c554d8` |
| Active spec | `docs/v2/active/M243_001_P1_API_DOCS_INFRA_OBS_RUNNERS_HOLD_ONE_PUSH_STREAM.md` |
| Review guide | `docs/reviews/m243/implementation-signoff.md`, Nov 09 review checkpoint |
| Exact inventory | `docs/reviews/m243/files.tsv` |
| Canonical design | `docs/architecture/runner_protocol.md` |

### Persistent owner instructions

- Finish the implementation before starting local verification.
- Ask Indy before lint, unit tests, Cargo checks, gates or other verification commands.
- Commit and push source checkpoints to the existing draft; keep them clearly unverified.
- Hold merge for Indy's listener, certificate and 1Password review.
- Ask before repository scripts, workflow or deployment edits, unlisted tables or SQL casts.
- Use native UUIDv7 bindings and rows, `event_id`, and owning table schema files.
- Remove legacy polling paths in the final implementation; add no compatibility aliases.
- Keep HTTP/3 preferred on a new connection, with HTTP/2 fallback and no hot transport switching.
- Keep private Fly routing and document external runner prerequisites; do not add Cloudflare dependence.
- Read issue 813 before every push and keep Claude informed on issue 818.

### Written in this checkpoint

- `LeaseService`, `AdmittedFleetWork`, `GatewayDependencies`, `StreamLimit` and `StreamPermit` replace ambiguous names without aliases.
- `http_services.rs` and `lease_service.rs` replace the old service-trait and adapter filenames.
- `StreamKind` replaces the protocol enum; the selector header is `agentsfleet-runner-kind`.
- Route-selected `RequestAuthentication` returns `Result<Option<Principal>, Refusal>` and preserves the common scope check.
- Native control proves its bearer once; secondary streams require the same physical connection's server-created binding.
- `prepared`, `confirm` and `welcome` separate proposed from committed ownership and allow lost-welcome recovery.
- `SessionEvent`, `EventSource`, `EventHandler` and `SessionSupervisor` separate event selection, decisions and supervised effects.
- Request identifiers and operation slots return through `Drop`; each request frame is decoded once.
- `LeaseCache` validates complete snapshots and protects concurrent updates with a short nonpoisoning lock and revision check.
- Memory budgets use `AGENTSFLEET_RUNNER_CONTROL_MEMORY_MIB` and `AGENTSFLEET_RUNNER_DATA_MEMORY_MIB`.
- Startup rejects invalid settings; wake capacity uses `AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY`.
- The independent recovery probe runs once per replica every five seconds; subscription retries do not multiply database passes.
- Typed `Cancel` confirms its durable lease marker; gaps and overflow request a complete `Recheck`.
- Offer serialization borrows an encoder while policy inputs live and carries its exact target without another target query.
- Lease insertion returns the saved `created_at` in its existing statement; census uses the unchanged full-precision database clock.
- `Handle::publish_frame` and completion publication return `Result<()>`; post-commit callers explicitly observe notification outcomes.
- Invalid saved usage and benchmark concurrency conversions remain errors rather than fabricated zero or maximum values.
- New test sources cover cache races, request ownership, control isolation and memory configuration.

`parking_lot 0.12.5` was already locked transitively and is now a direct gateway dependency.
`cargo update --workspace --offline` added the gateway's lockfile entry without compiling Rust.
This was dependency-artifact generation, not a verification result.

### Next implementation work

1. Finish the standards review of the full PR diff and inspect new generic and ownership boundaries for compile issues.
2. Wire and supervise the private native listener, gateway scheduler and graceful drain in the daemon runtime.
3. Implement runner downlink and uplink ownership, four streams, pending replies, reconnect and proposal retention.
4. Feed worker offers, batch renewals, cancellation, folds and durable report replay through that session.
5. Respect configured proxies and trusted certificates; exercise HTTP/3 failure, HTTP/2 fallback and address failover later.
6. Remove the six polling routes, old heartbeat holding, obsolete client methods, fields and tests.
7. Complete failure, fairness, observability and dense idle-workload benchmark sources.
8. Reconcile canonical architecture pages, generated OpenAPI and the final exact file inventory.
9. Tell Indy when code is complete; seek approval before running the required verification lanes.

Do not restore old held-heartbeat wiring to fix the current failed integration test.
Replace it with the native-session path and port its assertions.
The UUIDv7 fixture correction is written but has no passing result for this checkpoint.

### Remaining verification and review

The required repository lanes remain `make lint-all`, `make test-unit-all`, `make test-integration-rustd` and `make check-version`.
Final Rust coverage requires 98% overall and 97.5% changed lines; TypeScript gates require their declared 100% floors.
Do not lower floors, add exclusions, suppress findings or claim these gates passed on this checkpoint.

The last completed coverage report was 98.2726% overall and 87.1031% changed lines at an earlier revision.
The changed-line gate failed then and has no newer completed graded result.
Query plans, new gateway behavior, actual linked musl transports and private Fly reachability still need evidence.

### Coordination with Claude

Claude owns M244, PR 820, NATS and the tail benchmark behavior.
He was told to commit his benchmark changes; preserve them when merging the mechanical public-name edits.
Issue 818 records that Indy approved NATS and accepted one benchmark miss; do not repeat superseded speed claims.

The newly published `Handle::publish_frame` result type is an interface change for M244 callers.
Preserve connection authority and `for_connection` when adopting M244's constructor changes.
Claude now proceeds with NATS startup and consumer changes without waiting for M243.
He holds Dragonfly deletion until this checkpoint is published.
Preserve the new wake and frame-memory preflight fields when merging his startup changes.
A requested round-one review of PR 820 remains outstanding.

### Environment and operations

The current cloud worktree's build target was cleaned after disk exhaustion.
No local test or lint lane is running.
Use two Cargo jobs, no development or test debug information, and no incremental compilation when verification is approved.

Kernel tests need umask 022 and the existing Docker environment setup.
Do not treat a datastore environment-variable failure as a product test result.
Use Make's exported test environment and reset discipline for any later integration claim.

No Fly secret or 1Password item has been written by this task.
The certificate instructions remain draft in `playbooks/operations/runner_stream_tls/001_playbook.md`.
Finish exact listener configuration and delivery instructions before asking Indy to provision anything.

## Limits

This handoff preserves task state and decisions; it is not a merge recommendation or a completed milestone report.
Keep the draft description and issue 818 current after each material push.
Retain `Closes #818` in the PR description.

## Related pages

- [Implementation review](implementation-signoff.md)
- [Transport research](transport-and-toon-research.md)
- [Failure and observability](failure-and-observability.md)
- [Runner protocol](../../architecture/runner_protocol.md)
