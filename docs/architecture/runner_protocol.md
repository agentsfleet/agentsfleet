# Runner stream protocol

This page records the M243 implementation design. The transport and gateway are not implemented yet.
The pending M243 specification holds the acceptance tests and progress.

## Connection direction and deployment

The runner opens an outbound connection to a configured `agentsfleetd` endpoint.
The daemon sends offers and cancellation over that connection.
The daemon never needs to dial the runner or expose an inbound runner port.

HTTP/3 is preferred. HTTP/2 carries the same application protocol when HTTP/3 cannot connect.
Both transports are required in M243; WebSocket and lease polling are removed.

```text
Fly-hosted runner ---- Fly private network -----+
                                               +--> agentsfleetd runner listener
External runner ----- directly reachable host -+
                       HTTP/3: UDP + TLS 1.3
                       HTTP/2: TCP + TLS
```

User Datagram Protocol (UDP) carries HTTP/3 through QUIC.
Transmission Control Protocol (TCP) carries HTTP/2.
Transport Layer Security (TLS) verifies the daemon's identity on both paths.

Cloudflare is not a dependency of either path.
An operator may put supported infrastructure in front of the endpoint, provided it passes the deployment tests below.
Private and public connections use the same runner protocol and authentication rules.

The deployment requirements are:

- A runner can resolve and reach the configured daemon endpoint.
- The endpoint presents a certificate valid for the configured server name.
- The runner trusts the certificate chain; private deployments may supply their own trusted certificate authority.
- HTTP/3 requires a reachable UDP listener, normally on port 443.
- HTTP/2 requires a reachable TCP listener, normally on port 443.
- Any intermediary must pass bidirectional streaming without buffering the entire request.
- Public QUIC routing must retain connection ownership across packets, including during scaling and drain.

The selected port and hostname belong to deployment configuration.
Running both services on Fly does not require a public route.
Fly documents UDP support on its private IPv6 network separately from its public UDP service limitations.

Sources: [Fly UDP networking](https://docs.fly.io/networking/udp-and-tcp/),
[HTTP/3 connection establishment](https://www.rfc-editor.org/rfc/rfc9114.html#section-3.2).

## One session and two carriers

A runner has one serving session, identified by `runner.runners.connection_id`.
Choosing another carrier creates a new session and rebuilds from Postgres.
HTTP/2 and HTTP/3 never run competing lease consumers for one host.

Connection establishment tries HTTP/3 within a bounded recovery deadline.
A network failure or unsupported transport permits HTTP/2 fallback.
Authentication, certificate, protocol-version and policy refusals fail closed instead of causing fallback.

A healthy HTTP/2 session stays on HTTP/2.
The runner does not interrupt active work merely to try HTTP/3 again.
Subsequent attempts retain recent failure history so an unavailable UDP path cannot create a retry loop.

An explicit outbound proxy policy must also govern transport selection.
An HTTP/3 attempt must not bypass a configured proxy requirement.
Ordinary HTTP CONNECT proxy support belongs to the HTTP/2 carrier.

The recovery deadline covers transport attempts, handshake, authentication and state rebuild.
Each running lease also keeps its independent expiry deadline.
A connection attempt or a report retry never extends a lease without a confirmed renewal.

## Session protocol

Both carriers use the same strict, bounded JSON messages from `af_wire::protocol`.
The transport handles HTTP and framing; the gateway handles runner and lease decisions.
The session opening authenticates the runner token and validates the protocol version before accepting work.

Upstream messages carry hello, credits, heartbeats, renewal batches, activity, reports and folded-input requests.
Downstream messages carry welcome, offers, answers, report acknowledgements, folded input, cancellation and drain instructions.
Only declared variants and fields are accepted.

Message size, running-lease count, renewal count and queue bounds apply equally to both carriers.
Native HTTP frame boundaries are not application-message boundaries.
The codec must reject an oversized declared length before allocating that payload.

Control, reports and activity need independently bounded scheduling.
The transport spike must choose and document their HTTP stream mapping and authentication binding before implementation.
Large reports must not prevent cancellation or renewal from making progress.

Multiple QUIC streams can avoid transport ordering stalls between lanes.
Messages sharing one ordered stream still wait for missing earlier bytes.
HTTP/3 does not remove shared bandwidth, congestion or application queue limits.

State-changing messages cannot use replayable TLS early data.
Reports retain their lease-keyed idempotency regardless of carrier.
Transport retries alone never establish exactly-once processing.

## Rust boundaries and file owners

The paths below describe planned files, not existing implementations.
Existing crate owners keep their lease, billing and authentication decisions.
New dependencies land with their first production callers.

| Responsibility | Exact planned source files |
|---|---|
| Strict message variants and bounds | `rustd/crates/protocol/af_wire/src/protocol/mod.rs`, `up.rs`, `down.rs`, `bounds.rs` |
| Bus abstraction and typed subjects | `rustd/crates/daemon/afd_bus/src/lib.rs`, `subject.rs`, `command.rs`, `subscription.rs`, `error.rs` |
| Dragonfly bus adapter | `rustd/crates/daemon/afd_bus/src/dragonfly/mod.rs` |
| Wake notifications | `rustd/crates/daemon/afd_bus/src/wake/mod.rs` |
| Gateway composition and errors | `rustd/crates/daemon/afd_gateway/src/lib.rs`, `error.rs` |
| Session registry and offer passes | `rustd/crates/daemon/afd_gateway/src/registry/mod.rs`, `offers.rs` |
| Session opening, messages and authority checks | `rustd/crates/daemon/afd_gateway/src/session/mod.rs`, `hello.rs`, `frames.rs`, `recheck.rs` |
| Server carriers and bounded framing | `rustd/crates/daemon/afd_gateway/src/transport/mod.rs`, `h2.rs`, `h3.rs`, `codec.rs` |
| HTTP runner entry | `rustd/crates/daemon/afd_api_runner/src/handler/runner/stream.rs` |
| Client connection and recovery | `rustd/crates/runner/afr_supervisor/src/downlink/mod.rs`, `connect.rs`, `h2.rs`, `h3.rs`, `dispatch.rs`, `reconnect.rs` |
| Client priority queues and writer | `rustd/crates/runner/afr_supervisor/src/uplink/mod.rs`, `queues.rs`, `writer.rs` |
| Stream metrics and finite labels | `rustd/crates/protocol/af_observability/src/producers/fleet/runner/stream.rs`, `metrics/label/fleet.rs`, `metrics/declared/fleet.rs` |
| Native transport and failure evidence | `rustd/crates/daemon/main/tests/integration_runner_stream/mod.rs`, `transport.rs`, `reconnect.rs`, `cancel.rs`, `reports.rs`, `observability.rs` |

Directory prefixes in each row apply to the following short filenames.
The pending specification inventories the existing files whose callers also change.
The HTTP stream mapping may require refining this plan during the spike.

`afd_bus::Bus` is the runtime-selected broker boundary.
Its `publish`, `publish_batch` and `subscribe` methods use typed `Subject` values.
An `Arc<dyn Bus>` handle permits the later broker replacement without changing publishers.

The gateway remains a concrete session owner over existing services.
Carrier adapters supply bounded inbound and outbound messages.
Rust's `Stream` and `Sink` traits provide the transport boundary where their semantics fit.

The client chooses a carrier at connection establishment.
Domain services never branch on HTTP versions.
Enums distinguish retryable connection failures, terminal refusals, graceful drain and superseded sessions.

One task owns each mutable session state.
Bounded Tokio channels transfer messages; watch channels carry the latest assignment or credits.
Per-connection cancellation tokens stop sibling tasks when a reader, writer or supervisor ends.

A supervised `JoinSet` collects task results and bounds shutdown.
Owned permits release stream capacity when a task returns, fails or is cancelled.
The writer's deadline also bounds a socket stuck during an in-progress send.

## Libraries and reference code

The HTTP/2 carrier uses the existing Hyper, h2, Tokio and TLS stack.
The HTTP/3 candidate is `tokio-quiche`, using its native HTTP/3 client and server drivers.
The spike must verify certificate handling, supported targets, build requirements, cancellation and metric integration before pinning its version.

Cloudflare documents production use of `tokio-quiche`.
Using that library does not require using Cloudflare's network or hosting.
Source: [tokio-quiche library documentation](https://developers.cloudflare.com/privacy-proxy/reference/client-libraries/).

Codex's transport provides references for independent reader and writer tasks, bounded shutdown, and delayed backoff reset.
The inspected checkout is `openai/codex` revision `748ab55ca7de0b647d9a7a9d9a8e70e00f28666d`.
Its broker-independent lifecycle patterns apply here; its WebSocket framing and forked dependencies do not.

Reference files:

- `codex-rs/app-server-transport/src/transport/remote_control/websocket.rs`
- `codex-rs/websocket-client/src/lib.rs`
- `codex-rs/websocket-client/src/dialer.rs`

## Recovery and availability

Postgres holds live leases, connection ownership and cancellation markers.
Reconnect compares the runner's running leases with that authority.
Missing, cancelled or superseded leases receive cancellation; valid leases retain their confirmed deadlines.

Each report reaches durable runner storage before transmission.
The runner removes it only after the matching acknowledgement or a terminal refusal.
An acknowledgement lost after commit causes an idempotent retry, not another charge.

A kill marks the affected leases in its transaction before publishing cancellation.
Renewal and reconnect read the marker when a notification is lost.
A failed bus publish costs delivery time, not the durable cancellation decision.

Each grant and renewal checks the runner's current authority inside its transaction.
A stale in-memory session cannot authorize new work.
Supersession tests must include lost notifications and concurrent connections across replicas.

Heartbeat and application ping deadlines measure session responsiveness.
Transport acknowledgement alone cannot prove that the gateway can answer a renewal.
With a 15-second ping interval and 5-second answer timeout, ping-only detection can take 20 seconds.

An unanswered renewal fails within its own answer timeout.
Tests measure the complete detection and reconnect interval from the injected fault.
Recovery succeeds before lease expiry or the runner stops the lease at its confirmed deadline.

HTTP/3 may preserve a connection across supported network-address changes.
It does not preserve a session whose daemon process died.
Both carriers use the same reconnect, fencing and database rebuild rules after server failure.

## Observability

Every attempt records its selected transport and classified outcome.
Every established session records one completed or failed exit.
A transition to HTTP/2 records why HTTP/3 was unavailable.

| Signal | Purpose |
|---|---|
| Connection attempts and failures by transport and finite reason | Distinguish unreachable endpoints, timeouts and terminal refusals |
| Fallback count by reason | Show when HTTP/3 availability differs from overall session availability |
| Active sessions and disconnected duration | Distinguish a stable connection from a retry loop or sustained outage |
| Reconnect count, delay and recovery duration | Expose repeated short-lived connections and slow rebuilds |
| Ping and renewal response duration | Expose a responsive socket whose application control path is delayed |
| Queue pressure and dropped activity | Expose congestion before control traffic misses its deadline |
| Stream closes and offer claim outcomes | Preserve M243's operator-facing session and scheduling counters |

Metric labels use closed enums for transport, outcome, reason and message kind.
Runner, connection and lease identifiers belong in tracing fields, not unbounded metric labels.
Tokens, certificates, report bodies, tenant messages and private keys never enter logs.

Start and exit records include connection identifiers, transport, duration and the registered error code where applicable.
Reconnect records include attempt count, selected delay and outage duration.
An HTTP/2 fallback is visible as a transport outcome, not reported as an HTTP/3 success.

## Transport evidence

No HTTP/2 or HTTP/3 runner-stream probe has run for this design.
Tool availability and a successful build do not establish transport or deployment support.

Before runtime implementation, the spike must record:

1. Native HTTP/3 and HTTP/2 duplex exchange while both request bodies remain open.
2. Idle operation for at least 120 seconds, application ping answers, and large-message bounds.
3. HTTP/3 network failure followed by successful HTTP/2 fallback within the recovery deadline.
4. No fallback after authentication, certificate or protocol-version refusal.
5. Renewal and cancellation progress during large reports and activity pressure.
6. Cancellation of blocked reads, writes, handshakes and reconnect backoff.
7. Session rebuild and report retry after server restart, with one settlement.
8. Tracing and metrics that distinguish timeout, fallback, reconnect and sustained outage.

Private Fly routing and public routing need separate deployment evidence.
Public routing details and certificate delivery remain deployment decisions requiring a concrete reviewed patch.
No production deployment or configuration change is authorized by this page.
