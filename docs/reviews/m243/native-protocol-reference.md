---
type: explanation
audience: contributor
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Native runner protocol reference

## What it is

This page owns the native message mapping, framing, correlation and implementation boundaries.
The [runner guide](../../architecture/runner_protocol.md) owns operator settings, trust, durable authority and recovery behavior.

Read this page when changing `af_wire::protocol`, `af_stream` or the gateway and runner session owners.
The active M243 specification holds acceptance criteria; the handoff records verification results.

## Why it exists

HTTP/3 and HTTP/2 carry the same application protocol.
Transport-specific code handles bounded byte streams; existing domain services retain work, authorization and accounting decisions.

## How it behaves

### Session protocol

Every session opens four duplex `POST /v1/runners/me/stream` requests on one native connection.
Each request carries `agentsfleet-runner-protocol: agentsfleet.runner.v1` and its `agentsfleet-runner-kind` value.

| Kind | Runner to daemon | Daemon to runner |
|---|---|---|
| `control` | Hello, confirmation, credits, heartbeat, ping, renewal batch, authoring renewal | Prepared, welcome, assignment, heartbeat answer, pong, renewal answers, cancellation, `go_away` |
| `work` | Folded-input request, authoring model call, proposal or progress receipt | Work offer, folded input, authoring model result, proposal acknowledgement |
| `reports` | Fleet report, authoring completion or accounting report | Matching report acknowledgement |
| `activity` | Fleet activity | No application acknowledgement |

The control request carries the bearer token and opens first.
`prepared` identifies the observed owner and proposed owner; the client records both before sending `confirm`.
The host-lock comparison commits ownership before `welcome` confirms it.

Secondary requests carry that owner in `agentsfleet-runner-session`.
The connection-local typed context binds runner, credential digest, owner and cancellation.
Secondary requests carry no bearer token.

Duplicate kinds, missing context, cancelled bindings and identifiers from another physical connection are refused.
One shared `StreamPermit` covers all four requests until cleanup completes.
The [opening owner](../../../rustd/crates/daemon/afd_gateway/src/session/opening/mod.rs) and [connection context](../../../rustd/crates/daemon/afd_gateway/src/connection.rs) enforce that binding.

### Framing and correlation

An application message starts with a four-byte unsigned big-endian length, followed by exactly that many UTF-8 JSON bytes.
The codec checks the length before reserving memory or reading the payload.
Native HTTP frame boundaries carry no application-message meaning.

`Up` refuses unknown variants and fields.
Each message must use its declared stream kind.
The [wire definitions](../../../rustd/crates/protocol/af_wire/src/protocol/mod.rs) own fixed bounds and header spellings.

Incoming payloads retain zeroing allocation owners through dispatch.
Receive and send reservations are independent, so an answer cannot wait for the memory its request holds.
Drop releases reservations after completion, failure, timeout or cancellation.

Requests expecting answers carry a positive session-local `request_id`.
Each answer echoes that identifier; the counter never repeats within a session.
Counter exhaustion ends the session.

Cancellation, deadline expiry and disconnect remove pending entries and fail their callers.
Late or duplicate answers have no waiting owner and are ignored with bounded observations.
Reconnect starts a new request map, so old answers cannot complete new callers.

### Credit and ordering rules

`ready { credits, revision, observed_offers }` replaces the runner's capacity snapshot.
The daemon ignores older revisions and subtracts offers sent but not yet observed.
Each offer consumes one worker; remaining free workers stay eligible.

The counters restart for each session.
Running leases still count toward the host's occupied capacity after reconnect.
The database capacity lock decides whether an issue fits.

Renewals and pings use the priority control queue ahead of ordinary heartbeat updates.
Work, reports and activity have separate bounded writers and flow control.
Activity beyond its cap drops; reports retain their caller's memory owner while delivery retries.

Data transfers use chunks of at most 16 KiB.
Native stream windows are 64 KiB; the connection receive window is 512 KiB.
The combined data-stream windows leave room for control progress.

QUIC streams can avoid ordering stalls between streams.
Messages within one stream still wait for earlier bytes, and all streams share bandwidth and congestion limits.

A control cancellation can arrive before its work offer.
The runner retains that cancellation in bounded session state and refuses to start the matching offer.
Exhausting that state closes the session.

### Usage and private authoring

Each renewal entry retains its complete `RenewResponse`, including that lease's `recorded_through` acknowledgement.
The runner advances usage acknowledgements independently for each lease and preserves final batching.

Authoring model dispatch, proposal replay, renewal and completion use the current connected authority under the host lock.
After physical provider HTTP, the daemon rechecks authority before exposing or saving private output.
Known provider usage retains its accounting path when private publication is fenced.

An accepted model task remains tracked beyond native disconnect, within its existing provider deadline and daemon shutdown.
Its frame charge, operation permit and request registration remain owned until completion.
An uncertain physical call is never repeated by connection recovery.

Replay uses the original dispatch, activity or report identity.
Changed replay content is refused.
Content-free accounting remains available after private deletion, and unsettled accounting retains its worker owner.

### Session events and confirmed targets

`SessionOpening` owns a proposal; `ConfirmedSession` owns database-issued authority.
`SessionSupervisor` consumes typed events and owns its operation task set.
Independent request permits preserve control capacity under data pressure.

`RequestRegistration` releases correlation identity through `Drop`.
`LeaseCache::refresh` validates a complete snapshot before applying it.
A revision comparison prevents stale reads from restoring a reported lease or shortening a newer renewal.

Durable cancellation still applies if a concurrent local update wins that comparison.
Activity targets expire at the confirmed lease deadline and clear on refusal, cancellation or report completion.
Ordinary chat activity uses those targets; durable task observations retain their fenced writes and daemon-owned sequences.

Issue returns the stored `created_at`; capacity retains the database clock's full precision.
Reconnect reads use native UUIDv7 bindings and owning-table indexes.
Query shape alone establishes no latency claim.

### Scheduling and the bus

The [gateway scheduler](../../../rustd/crates/daemon/afd_gateway/src/scheduler/mod.rs) owns one notification pass at a time.
A pass spends one lease per wake generation and visits locality-first candidates in waiting order.
An empty claim advances to the next host.

Each claim travels through the session's bounded queue and returns its observed outcome.
An uncertain outcome ends the pass because its transaction may already have issued work.
Wakes received during a pass form one subsequent pass.

Only increased capacity or recovery from degraded status starts an independent session pass.
Successful issue updates waiting order.
The replica probe keeps its independent clock during notification work and broker reconnects.

One boot-selected `afd_bus::Handle` reaches all publishers and subscribers.
The adapter translates backend events into `Message`, `Lagged` and `Gap`.
Subscription owners retain native release behavior, and message consumers share payload allocations.

`Bus::shutdown` asynchronously closes subscriptions and joins adapter work under the caller's drain deadline.
The bus carries disposable signals; durable replay and runner broker credentials are absent.
The [datastore design](../../architecture/datastore_scaling.md) owns broker selection and replacement.

### Rust boundaries and file owners

| Responsibility | Current source |
|---|---|
| Message variants, headers and bounds | `af_wire/src/protocol/{mod,down}.rs`, `up/` |
| Native byte budgets and framing | `af_stream/src/framing/` |
| HTTP/2, HTTP/3 and shared BoringSSL trust | `af_stream/src/{h2,h3,tls}/` |
| Connection binding and HTTP adaptation | `afd_gateway/src/{connection,service}.rs` |
| Opening, dispatch, cache and cleanup | `afd_gateway/src/session/` |
| Registry, bounded passes and recovery probe | `afd_gateway/src/{registry,scheduler}/` |
| Runner connection, request map and cancellation | `afr_supervisor/src/downlink/` |
| Runner priority queues and writers | `afr_supervisor/src/uplink/` |
| Native session metrics and finite labels | `af_observability/src/producers/fleet/runner/stream.rs`, `metrics/{declared,label}/fleet.rs` |

Crates live under their existing `protocol/`, `daemon/` or `runner/` layer in `rustd/crates/`.
Domain services never branch on HTTP versions.
Typed errors distinguish retryable transport loss, terminal refusal, drain and supersession.

Each mutable session state has one owner.
Bounded channels transfer messages; watch channels carry the latest assignment or availability snapshot.
Cancellation stops sibling tasks, and supervised task sets observe their completion.

### Libraries and reference code

The HTTP/3 dependency is `tokio-quiche` 0.20.1 with default features disabled.
HTTP/2 uses Hyper 1.12.0, h2 0.4.20 and `tokio-boring` 5.4.0.
Both native transports use BoringSSL; existing database and outbound clients retain their selected TLS stacks.

The [transport study](transport-and-toon-research.md) records source comparisons and native probe results.
Its Codex reference is revision `748ab55ca7de0b647d9a7a9d9a8e70e00f28666d`.
Independent reader/writer ownership and delayed backoff reset apply here; its WebSocket framing does not.

## Limits

State-changing operations wait for the completed handshake; replayable TLS early data is disabled.
Transport retries alone establish no exactly-once guarantee.
The [runner guide](../../architecture/runner_protocol.md#results-and-billing) states what survives acknowledgement loss and process loss.

### Transport evidence

Native verification must cover both carriers, open upload bodies, large frames, refusal, pressure, blocked I/O and complete cleanup.
Fault cases must cover reconnect, saved-result replay, cancellation, token changes and supersession.
The [current handoff](session-handoff.md) records actual results; historical scratch probes do not certify the current production path.

## Related pages

- [Runner setup and runtime behavior](../../architecture/runner_protocol.md)
- [Failure and observability design](failure-and-observability.md)
- [Implementation sign-off](implementation-signoff.md)
- [Current session handoff](session-handoff.md)
