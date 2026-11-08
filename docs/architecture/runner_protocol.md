# Runner stream protocol

This page records the M243 implementation design. The transport and gateway are not implemented yet.
The active M243 specification holds the acceptance tests and progress.

## Connection direction and deployment

The runner opens an outbound connection to a configured `agentsfleetd` endpoint.
The daemon sends offers and cancellation over that connection.
The daemon never needs to dial the runner or expose an inbound runner port.

HTTP/3 is preferred. HTTP/2 carries the same application protocol when HTTP/3 cannot connect.
Both transports are required in M243; WebSocket and lease polling are removed.

```text
Fly-hosted runner ---- Fly private network ----------> agentsfleetd runner listener
External runner ----- operator-provided reachability -> configured daemon endpoint
                       HTTP/3: UDP + TLS 1.3
                       HTTP/2: TCP + TLS
```

User Datagram Protocol (UDP) carries HTTP/3 through QUIC.
Transmission Control Protocol (TCP) carries HTTP/2.
Transport Layer Security (TLS) verifies the daemon's identity on both paths.

Cloudflare is not a dependency of either path.
Private Fly routing is the supported hosted deployment path for M243.
External operators supply reachability and permit outbound HTTP/3 over QUIC or HTTP/2 to their configured endpoint.

An external runner needs private network access when its configured daemon endpoint is private to Fly.
Enabling HTTP/3 or HTTP/2 alone does not create that route.
The runner remains an outbound client and needs no inbound HTTP listener.

The deployment requirements are:

- A runner can resolve and reach the configured daemon endpoint.
- The endpoint presents a certificate valid for the configured server name.
- The runner trusts the certificate chain; private deployments may supply their own trusted certificate authority.
- HTTP/3 requires outbound UDP to the daemon's configured listener port.
- HTTP/2 requires outbound TCP to the daemon's configured listener port.
- External operators permit at least one carrier; permitting both allows HTTP/2 fallback when UDP fails.
- Any intermediary must pass bidirectional streaming without buffering the entire request.
- Routing must retain native connection ownership across packets, including during scaling and drain.

The selected port and hostname belong to deployment configuration.
Running both services on Fly does not require a public route.
Fly documents UDP support on its private IPv6 network separately from its public UDP service limitations.

Sources: [Fly UDP networking](https://docs.fly.io/networking/udp-and-tcp/),
[HTTP/3 connection establishment](https://www.rfc-editor.org/rfc/rfc9114.html#section-3.2).

## Certificates and configuration

The daemon presents one certificate chain and private key on both native listeners.
The certificate's subject alternative names include the hostname in the runner stream URL.
Runners verify that name and chain, then authenticate with their existing bearer token; they need no client certificate.

For private Fly routing, use a private certificate authority (CA) or another issuer valid for the configured hostname.
Distribute the CA's public certificate to runners when their operating system does not already trust it.
The CA signing key stays outside daemon and runner runtime configuration.

The implementation uses these configuration names:

| Process | Environment variable | Purpose |
|---|---|---|
| Daemon | `AGENTSFLEET_RUNNER_STREAM_LISTEN_ADDR` | Private TCP and UDP listen address; proposed default `[::]:4443` |
| Daemon | `AGENTSFLEET_RUNNER_STREAM_TLS_CERT_FILE` | PEM leaf certificate followed by its issuer chain |
| Daemon | `AGENTSFLEET_RUNNER_STREAM_TLS_KEY_FILE` | PEM private key matching that leaf certificate |
| Runner | `AGENTSFLEET_RUNNER_STREAM_URL` | HTTPS daemon origin; the client opens `/v1/runners/me/stream` there |
| Runner | `AGENTSFLEET_RUNNER_STREAM_TLS_CA_CERT_FILE` | Trusted CA bundle for a private issuer; omitted for operating-system trust |
| Runner | Existing `AGENTSFLEET_RUNNER_TOKEN` | Runner bearer credential, independent of certificate trust |
| Runner | Existing `AGENTSFLEET_API_URL` | Endpoint for the remaining HTTP operations |

These names are recorded before their configuration readers land; this page's implementation status still applies.
A runner stream URL such as `https://agentsfleetd-dev.internal:4443` requires that exact hostname in the certificate.
An external runner also needs a route to that private endpoint.

Use the existing 1Password vault selected by `VAULT_DEV` for development and `VAULT_PROD` for production.
The deployment workflow already selects those variables; never hardcode vault names into transport code.
The proposed item fields are:

| Item | Fields | Runtime delivery |
|---|---|---|
| `runner-stream-ca` | `certificate`, `private-key` | Distribute only the public certificate to runner trust bundles; keep the signing key out of runtime |
| `agentsfleetd-runner-stream-tls` | `certificate-chain`, `private-key`, `server-name` | Stage the chain and leaf key as protected daemon files; the hostname is configuration metadata |

Use separate CA and leaf items in each environment's vault.
The existing runner item continues to hold the bearer credential.
Creating items, delivering protected files and changing Fly configuration require their concrete reviewed deployment patch.

Rotate leaf files through a daemon rollout, which drains sessions and lets runners reconnect using the current carrier preference.
When rotating the CA, distribute the new trust anchor before serving certificates issued by it.
Overlapping trust bundles allow that sequence without disabling certificate verification.

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

The next normal connection setup tries HTTP/3 again when network policy permits it.
That setup follows a connection failure, daemon drain or restart, or runner restart.
A healthy HTTP/2 session can therefore remain on HTTP/2 indefinitely; this milestone adds no periodic upgrade or hot switch.

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

Each carrier opens four duplex POST requests on one native connection, all at `/v1/runners/me/stream`.
The `agentsfleet-runner-lane` header selects `control`, `work`, `reports` or `activity`.
The protocol header is `agentsfleet-runner-protocol: agentsfleet.runner.v1`.

The control request presents the runner bearer token and opens first.
After authentication and guarded ownership acquisition, `welcome` returns the connection identifier.
The other three requests present that identifier in `agentsfleet-runner-session`.

The server supplies a connection-local context as a typed request extension.
That context binds the proven runner, credential digest, connection identifier and cancellation token.
A secondary lane must match that context, its current session and its declared lane exactly.

The server rejects duplicate lanes, missing context, another connection's identifier, cancelled bindings and secondary bearer-token authentication.
One session reserves one ceiling permit, shared by its lanes.
Closing the native connection cancels all four requests and their pending operations.

The control lane carries hello, credits, heartbeat, ping, renewal, cancellation and drain.
The work lane carries offers and folded-input requests and answers.
The reports lane carries reports and their acknowledgements; the activity lane carries activity without acknowledgements.

Wrong-lane messages are protocol errors.

Each message is a four-byte, unsigned big-endian length followed by UTF-8 JSON.
The length covers only the JSON bytes, with a maximum of 2 MiB.
The reader validates that prefix before reserving memory and reading into one zeroing payload buffer.

Its reservation returns automatically on success, cancellation, timeout and failed I/O.

Requests expecting answers carry a positive, session-local `request_id` that every answer echoes.
The counter never repeats within a session; exhaustion ends that session.
At most 256 requests may be outstanding, and every pending entry has an answer deadline.

Cancellation, timeout and disconnect remove the pending entry and fail its waiting caller.
Late or duplicate answers have no pending owner and are ignored with bounded telemetry.
A new session starts a new request map; old answers cannot reach its callers.

Each successful renewal entry carries the complete `RenewResponse`, including `recorded_through`.
Usage acknowledgements advance independently for each lease, including after a repeated renewal.
Final usage flushing keeps its existing batching behavior.

Renewals, ping and cancellation receive control service ahead of ordinary heartbeat updates.
Large offers and folded inputs remain on the independently flow-controlled work lane.
Reports use a separate bounded writer and cannot queue ahead of control.

Activity drops beyond its cap, while reports remain durably spooled.

Data writes use chunks of at most 16 KiB and bounded per-stream flow-control windows.
The connection receive window exceeds the combined data-lane windows, preserving control credit.
Native tests must prove control progress with continuously ready reports and activity.

Multiple QUIC streams can avoid transport ordering stalls between lanes.
Messages sharing one ordered stream still wait for missing earlier bytes.
HTTP/3 does not remove shared bandwidth, congestion or application queue limits.

State-changing messages cannot use replayable TLS early data.
Reports retain their lease-keyed idempotency regardless of carrier.
Transport retries alone never establish exactly-once processing.

## Rust boundaries and file owners

Indy approved implementation and continuation to one pull request on Nov 08, 2026.
The [sign-off packet](../reviews/m243/implementation-signoff.md) records the shared `af_stream` carrier crate and exact file paths.
This page owns the approved runtime design; the review packet supplies supporting detail and evidence.

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
| Shared framing and byte budgets | `rustd/crates/protocol/af_stream/src/framing/mod.rs`, `reader.rs`, `writer.rs`, `budget.rs` |
| Native HTTP/2 carrier | `rustd/crates/protocol/af_stream/src/h2/mod.rs`, `client.rs`, `server.rs` |
| Native HTTP/3 carrier | `rustd/crates/protocol/af_stream/src/h3/mod.rs`, `client.rs`, `server.rs`, `driver.rs`, `socket.rs`, `trust.rs`, `telemetry.rs` |
| Shared BoringSSL trust policy | `rustd/crates/protocol/af_stream/src/tls/mod.rs`, `boring.rs` |
| HTTP runner entry | `rustd/crates/daemon/afd_api_runner/src/handler/runner/stream.rs` |
| Client connection and recovery | `rustd/crates/runner/afr_supervisor/src/downlink/mod.rs`, `connect.rs`, `dispatch.rs`, `reconnect.rs`, `pending.rs` |
| Client priority queues and writer | `rustd/crates/runner/afr_supervisor/src/uplink/mod.rs`, `queues.rs`, `writer.rs` |
| Stream metrics and finite labels | `rustd/crates/protocol/af_observability/src/producers/fleet/runner/stream.rs`, `metrics/label/fleet.rs`, `metrics/declared/fleet.rs` |
| Native transport and failure evidence | `rustd/crates/daemon/main/tests/integration_runner_stream/mod.rs`, `transport.rs`, `reconnect.rs`, `cancel.rs`, `reports.rs`, `observability.rs` |

Directory prefixes in each row apply to the following short filenames.
The active specification inventories the existing files whose callers also change.
The HTTP stream mapping may require refining this plan during the spike.

`afd_bus::Bus` is the runtime-selected broker boundary.
Its `publish`, `publish_batch` and `subscribe` methods use typed `Subject` values.
An `Arc<dyn Bus>` handle permits the later broker replacement without changing publishers.

Its synchronous `shutdown` closes the adapter's subscriptions during drain.
Subscription streams own native release behavior, and message payloads share one allocation across consumers.

The adapter translates backend-specific values into `Message`, `Lagged` and `Gap` outcomes.
M244 supplies core NATS behind this interface and moves Dragonfly's durable responsibilities into Postgres before deleting Dragonfly.
The bus carries signals without adding durable replay or broker credentials to runners.

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

The HTTP/3 candidate uses `tokio-quiche` 0.20.1 with default compression features disabled.
BoringSSL serves both carriers on the new runner connection.
HTTP/2 keeps Hyper and h2, using a matching `tokio-boring` stream for TLS.

Existing database, broker and outbound HTTP clients retain their current Rustls integrations.
The [reference study](../reviews/m243/transport-and-toon-research.md) records their backend support and the broader migration cost.

The native spike verifies certificate handling, build targets, cancellation and metric integration before production wiring.
It never disables peer verification to obtain a passing test.
State-changing operations wait for the completed handshake; early data is disabled.

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

Ownership acquisition updates the runner row only if the authenticated credential digest still matches and the administrative state permits connection.
Each issue and streamed renewal checks the expected connection identifier and credential digest while holding the host row lock.
A stale in-memory session cannot authorize new work, even when a notification is lost.

Those checks widen the existing capacity lock and its callers, rather than adding a second authority resolver.
The claim ticket carries the expected session authority into both fleet and authoring issue transactions.
Per-request authoring renewals retain their existing bearer authentication and do not use a cached session identity.

Only the database owner can grant or renew; an old socket may remain open until its next recheck.
Supersession tests include lost notifications, token rotation and old claims already waiting on the host lock.
Socket closure is bounded detection, not the execution-authority fence.

The activity cache eliminates target reads for ordinary chat chunks.
Durable task observations retain their existing fenced database writes and daemon-owned sequence values.
Cached timing eligibility expires at the confirmed lease deadline and is cleared on refusal, cancellation or report completion.

Heartbeat and application ping deadlines measure session responsiveness.
Transport acknowledgement alone cannot prove that the gateway can answer a renewal.
With a 15-second ping interval and 5-second answer timeout, ping-only detection can take 20 seconds.

An unanswered renewal fails within its own answer timeout.
Tests measure the complete detection and reconnect interval from the injected fault.
Recovery succeeds before lease expiry or the runner stops the lease at its confirmed deadline.

One recovery attempt has a seven-second aggregate deadline: two seconds for HTTP/3, two for HTTP/2, and three for authenticated opening and rebuild.
The runner also caps that deadline by each running lease's remaining time before expiry.
Cancellation can interrupt every stage, including name resolution and backoff.

The normal recovery assertion includes renewal tick, answer timeout, maximum initial jitter and the full recovery deadline.
Their sum is 19 seconds, below the 20-second budget left after reserving the renewal window.
Repeated failures may outlast that budget; independent lease expiry remains unconditional.

The bounded recovery test restores connectivity within the first carrier-attempt budget.
A separate sustained-outage test proves that no retry extends an unconfirmed lease.
Backoff resets only after 60 seconds of a stable session.

During drain, the gateway stops offers and finishes already accepted answers before sending `go_away`.
It closes all lanes within the existing drain deadline.
The client may reconnect immediately after `go_away` without losing the answers that preceded it.

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

The local HTTP/3 spike completed verified-certificate duplex exchange, a byte-identical 2 MiB payload and 120 seconds of application pings.
The raw client worker reported exit after explicit close, without a client certificate configured.
The HTTP/2 BoringSSL spike also passed duplex, payload, 120-second ping and worker-exit checks.

These scratch probes establish library behavior, not completion of production adapter or deployment tests.
Fly development checks follow local implementation and repository gates, before the pull request.

The probe commands and findings live in the [transport study](../reviews/m243/transport-and-toon-research.md).

Before runtime implementation, the spike must record:

1. Native HTTP/3 and HTTP/2 duplex exchange while both request bodies remain open.
2. Idle operation for at least 120 seconds, application ping answers, and large-message bounds.
3. HTTP/3 network failure followed by successful HTTP/2 fallback within the recovery deadline.
4. No fallback after authentication, certificate or protocol-version refusal.
5. Renewal and cancellation progress during large reports and activity pressure.
6. Cancellation of blocked reads, writes, handshakes and reconnect backoff.
7. Session rebuild and report retry after server restart, with one settlement.
8. Tracing and metrics that distinguish timeout, fallback, reconnect and sustained outage.

Private Fly routing needs deployment evidence before the hosted rollout.
External operators meet the documented routing, certificate and carrier prerequisites; public ingress deployment is outside this milestone.
Certificate delivery remains a deployment decision requiring a concrete reviewed patch.

No production deployment or configuration change is authorized by this page.
