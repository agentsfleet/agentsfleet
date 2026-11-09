---
type: explanation
audience: contributor
verified: 2026-11-10
product_version: 0.75.0
executable: false
---

# M243 owner feedback audit

## What it is

Source closure for Indy's feedback on [PR 819](https://github.com/agentsfleet/agentsfleet/pull/819).
The 23 numbers match the original comments. Implementation findings are closed in source; runtime verification remains due.
This table claims no unrun test passed.

## Why it exists

Indy requires owner-feedback fixes first, the foundation/crate reuse audit next, then integration, coverage, memory, benchmark and deployment checks.
The previous audit mixed obsolete findings with later corrections; this replaces that history with current source dispositions.

## How it behaves

### Original comments

Rust paths below are relative to `rustd/crates/`.

| # | Feedback | Source disposition | File |
|---|---|---|---|
| 1 | Explain `self.0` and initialization | Named `Gateway.inner`; explicit shared dependencies, registry, budgets and tracker. Provisional openings count during drain. | `daemon/afd_gateway/src/lib.rs` |
| 2 | Explain listener preflight | Named `RunnerListenerConfig` parses deployment inputs; `af_stream` owns both carriers. | `daemon/main/src/preflight/runner_listener/mod.rs` |
| 3 | Post-quantum safety | Both ends/carriers require TLS 1.3 and `X25519MLKEM768`. Certificate signatures remain classical ECDSA, as documented. TLS-library verification covers DNS and literal IP identities. | `protocol/af_stream/src/tls/key_exchange/mod.rs`, `h3/socket.rs` |
| 4 | Standard errors | Foundation error macros, registered codes, actionable configuration sentences and original typed causes are retained. Opening refusals preserve status/code. | `protocol/af_stream/src/error/`, `runner/afr_supervisor/src/error/` |
| 5 | Listener tracing | Startup, stop, failure and negotiated-group events exist; unexpected component termination cancels serving. | `daemon/main/src/serve/mod.rs`, `serve/runner_stream.rs`, `protocol/af_stream/src/telemetry.rs` |
| 6 | Study Exonum and Codex | Applied receiver ownership/priority, child cancellation, bounded joining and abort cleanup through Tokio primitives. Named comparison below. | `daemon/afd_gateway/src/session/{streams,supervisor}.rs`, `runner/afr_supervisor/src/downlink/mod.rs` |
| 7 | Explain Dragonfly startup code | Existing code describes M243's actual backend. Native listener failures retain their own path; M244's replacement is not imported. | `daemon/main/src/error.rs` |
| 8 | Explain proxy | Explicit HTTP CONNECT carries end-to-end TLS over HTTP/2; mandatory proxy policy blocks direct HTTP/3. Private direct routing needs no proxy. | `protocol/af_stream/src/proxy/mod.rs` |
| 9 | Tracing and comments | Connection/dispatch spans retain identifiers and finite operation labels; observations finish on cancellation. Runner exporter admits the reviewed transport span. Payloads and credentials stay out. | `protocol/af_stream/src/telemetry.rs`, `daemon/afd_gateway/src/session/dispatch/mod.rs`, `runner/afr_telemetry/src/telemetry/mod.rs` |
| 10 | Wiping and Miri | Fleet bytes use `Zeroizing`; authoring endings/reports and decision receipts wipe their private fields on drop. Debug hides receipt data. Miri remains due; parser/header/TLS/kernel copies are outside this ownership claim. | `runner/afr_supervisor/src/lease_loop/settle.rs`, `authoring/mod.rs`, `verbs/decision/pending/receipt/mod.rs` |
| 11 | Simplify bypass and environment parsing | Reused Hyper `Matcher`, `EnvSource`, `garde` and `url`. Invalid proxy inputs fail before bypass matching. | `protocol/af_stream/src/proxy/mod.rs` |
| 12 | Use `TryFrom` | Synchronous proxy conversion validates once; retained URI excludes credentials. | `protocol/af_stream/src/proxy/mod.rs` |
| 13 | Separate conversion/socket I/O | CONNECT headers use synchronous conversion; the async socket function owns connect/handshake/upgrade. | `protocol/af_stream/src/proxy/mod.rs` |
| 14 | Share request/decode work | `StreamHandle::compute` shares correlation, deadline, decode and observations across typed commands. | `runner/afr_supervisor/src/client/stream.rs` |
| 15 | Typed commands | `Command` declares expected output; `Outcome::try_from` resolves refusal codes once. Activity remains unacknowledged. | `runner/afr_supervisor/src/client/stream.rs` |
| 16 | Explain URL/CA | Required HTTPS origin and optional public trust file are documented. No runner client certificate or signing key is required. | `runner/afr_supervisor/src/config/stream.rs`, `docs/architecture/runner_protocol.md` |
| 17 | Configuration convention | Injected `EnvSource` and typed validation are retained; no second TOML layer. | `runner/afr_supervisor/src/config/stream.rs`, `protocol/af_stream/src/endpoint.rs` |
| 18 | Simplify opening | `Opening` separates hello, confirmation, secondary attachment and refusal parsing under one absolute deadline; failure closes the native owner. | `runner/afr_supervisor/src/downlink/connect.rs`, `terminal_tests.rs` |
| 19 | Connect versus dispatch | Opening authenticates/attaches; dispatch handles established-session replies, offers, cancellation and policy. | `runner/afr_supervisor/src/downlink/{connect,dispatch}.rs` |
| 20 | Performance and races | Bounded ownership, coalesced scheduling, transactional guards and independent lanes have pressure/contention/lost-command/reconciliation/cost test sources. Measurements remain due. | `daemon/afd_gateway/src/scheduler/`, `daemon/main/tests/integration_runner_stream/`, `runner/afr_supervisor/src/downlink/pressure_tests.rs` |
| 21 | Appropriate mutex use | Capacity and registrations share one short `parking_lot` lock; no await under its guard. Cancellation runs after release. | `runner/afr_supervisor/src/downlink/running.rs` |
| 22 | Reuse DashMap patterns | Named entries and a separate semaphore follow existing outbound lanes; shard guards are released before meter reads and watch updates. | `runner/afr_supervisor/src/uplink/renewals/mod.rs`, `daemon/afd_outbound/src/lanes/mod.rs` |
| 23 | Simplify renewals | Tick, snapshot, partitioning, exchange and reply application are separated. UUID values remain typed; shared commands decode replies. | `runner/afr_supervisor/src/uplink/renewals/{mod,batches}.rs`, `renew/mod.rs` |

### Other owner requests

| Request | Source disposition |
|---|---|
| Remove report files and compatibility fallback | Spool, replay, scanner, quarantine, receipt drainer and old-format reshaping are deleted. Each worker retains only its current result until acknowledgement/terminal refusal. Process loss can lose an unsent result. |
| Keep billing authority | Existing fenced Postgres issuance/settlement remain authoritative; lost acknowledgements replay saved results without another provider call or charge. |
| Liveness through the stream | Valid control-lane host heartbeats refresh `session/liveness.rs`; pings/unrelated traffic do not extend its two-interval deadline. |
| Preserve accepted frames during drain | Closed queues drain under existing operation permits before `go_away`; Tokio owns readers, writers and operations. |
| Delete unused code | Retired handlers/polling helpers, unused `ActivityAccepted` and forwarding `deadline::left` are removed. Six time callers use `af_core::clock::elapsed` directly. Duplicate runner JSON counters use `af_validate::serialized_len`. |
| Greptile blocked acknowledgements | Retained offers transfer charges into a separate bounded Tokio semaphore allowance. Focused mutation red/green evidence exists; the pushed thread remains open. |
| Keep recovery probe | Indy accepted crash-after-commit/before-wake recovery. One daemon probe per replica every five seconds remains; runners receive pushed offers. |
| Include deployment | Existing flows reference the approved vault item, deliver daemon chain/key files and runner public trust/private Fly origin, and exclude the signing key. DEV restores WireGuard routing. |
| One vault item and ten-year validity | Approved item/fields and joint renewal are documented. Certificates are already provisioned. No operator vault read, key generation or secret printing occurred here. |
| Concise canonical docs | Protocol guide/playbook give operator steps; stale runner polling passages are corrected. Delete this folder after verification and preserve final feedback evidence in PR notes. |

### Reference comparison

Codex `e54a5713caf331f42a6eef0166186df5f7563c29`, `app-server-transport/src/transport/remote_control/websocket.rs`, owns connection workers in a `JoinSet`, cancels them together and bounds joining before aborting.
Exonum `2d2fa22e5f5bc451d08c155c2398956f11dce06e`, `exonum-node/src/events/mod.rs`, owns and prioritizes input receivers.
This gateway additionally drains already accepted input instead of ending aggregation when any receiver closes.
Inspected source copies are under `/tmp/m243-reference-sources`, without Git metadata.

## Limits

The source review mapped all 23 comments to current code and found no remaining implementation finding in its inspected scope.
The reuse audit removed the forwarding time helper and both runner JSON counters; `af_validate::serialized_len` now shares the existing allocation-free bounded writer.
Tokio owns channels, semaphore permits, cancellation and tracked tasks; Hyper owns proxy matching; Boring owns certificate verification. Foundation helpers own identifiers, entropy, errors, environment reads and time conversion. The operator fixture reuses `afd_identity::direct`.
All 717 current candidate test/gate hunks were reviewed. The follow-up restored selected wake and loss-recovery proofs, prioritized forbidden offers over a durable test condition, corrected the settlement query key, and restored native report-to-completion analytics coverage.
Renewal partitioning, request correlation, offer admission and cancellation ordering remain domain logic.
The final size audit found zero violations across 848 changed paths; no suppression was added.
Integration, aggregate coverage, Miri, three reset-rig benchmark comparisons and deployed-route evidence are not certified by this source audit.
The base-branch comparison passed: runner 1,749; daemon 158; daemon libraries 3,482 (`make test-unit-rustd`, `/tmp/m243-base-rust-unit-01.log`). These are base counts, not candidate results.

## Related pages

- [Runner guide](../../architecture/runner_protocol.md)
- [Certificate guide](../../../playbooks/operations/runner_stream_tls/001_playbook.md)
- [Coordination](https://github.com/agentsfleet/agentsfleet/issues/818)
