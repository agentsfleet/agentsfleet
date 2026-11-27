---
type: explanation
audience: operator
verified: 2026-11-25
product_version: 0.90.0
executable: false
---

# Runner stream protocol

## What it is

Each runner opens one outbound connection to `agentsfleetd`. It carries work, cancellation, lease renewals, progress and results for every run on that host.

The runner prefers HTTP/3 and falls back to HTTP/2. The daemon never dials the runner, so the runner needs no inbound listener.

```text
agentsfleet-runner ── private network ──> agentsfleetd stream listener
                      HTTP/3 over UDP port 4443
                      HTTP/2 over TCP port 4443
```

User Datagram Protocol (UDP) carries HTTP/3 through the QUIC transport. Transmission Control Protocol (TCP) carries HTTP/2.

## Why it exists

The daemon pushes each run to a runner with a free worker, so idle workers send no requests. One authenticated connection replaces a request per verb.

The runner holds a runner token and no datastore credential. Everything it does reaches Postgres through the daemon ([runner fleet](runner_fleet.md)).

## How it behaves

### Settings and certificates

| Process | Setting | Effect and default |
|---|---|---|
| Daemon | `AGENTSFLEET_RUNNER_STREAM_LISTEN_ADDR` | TCP and UDP bind address; default `[::]:4443` |
| Daemon | `AGENTSFLEET_RUNNER_STREAM_TLS_CERT_FILE` | Required Privacy-Enhanced Mail (PEM) leaf certificate followed by its issuer chain; no default |
| Daemon | `AGENTSFLEET_RUNNER_STREAM_TLS_KEY_FILE` | Required PEM private key for that leaf; no default |
| Runner | `AGENTSFLEET_RUNNER_STREAM_URL` | Required HTTPS origin with no credentials, path, query or fragment; no default |
| Runner | `AGENTSFLEET_RUNNER_STREAM_TLS_CA_CERT_FILE` | Optional issuer bundle added to operating-system trust; default operating-system trust |
| Runner | `AGENTSFLEET_RUNNER_TOKEN` | Required `agt_r` runner token; no default |
| Runner | `AGENTSFLEET_API_URL` | Required base URL for the fenced HTTP run verbs; no default |

The client adds `/v1/runners/me/stream` to the origin. In development the origin is `https://agentsfleetd-dev.internal:4443`.

The daemon's file keys, such as `[runner.listener]` and the stream memory budgets, are in the [configuration reference](config_reference.md). The readers are [daemon preflight](../../rustd/crates/daemon/main/src/preflight/runner_listener/mod.rs) and [runner configuration](../../rustd/crates/runner/afr_supervisor/src/config/stream.rs).

The certificate must cover the hostname in the stream URL. Runners need the issuer's public certificate when the operating system does not trust that certificate authority (CA). Runners use their bearer token and need no client certificate.

The runner bootstrap installs that trust at `/etc/agentsfleet/runner-stream-trust.pem`. The [certificate playbook](../../playbooks/operations/runner_stream_tls/001_playbook.md) gives the generation and vault steps.

### Transports and fallback

Both transports require Transport Layer Security (TLS) 1.3 with `X25519MLKEM768` hybrid key exchange. A classical-only peer is refused, during fallback too.

Network failures and transport timeouts allow fallback to HTTP/2. Certificate, credential, protocol and policy refusals fail closed. An explicit HTTP `CONNECT` proxy selects HTTP/2.

A healthy HTTP/2 connection stays on HTTP/2; nothing upgrades it in place. UDP failure history starts only when HTTP/3 fails and HTTP/2 then connects. For the next 30 seconds, a new connection skips HTTP/3.

Each QUIC datagram carries at most 1,232 bytes of payload by default. That fits the 1,280-byte minimum Internet Protocol version 6 (IPv6) maximum transmission unit (MTU) after headers.

`[runner.listener] max_udp_payload_bytes` changes the ceiling, and the peer's advertised limit also applies ([datagram limit](../../rustd/crates/protocol/af_stream/src/mtu.rs)).

Permit both UDP and TCP to the stream port, so HTTP/2 can take over when UDP is blocked. Any intermediary must pass duplex streaming.

A tenant's own runner needs the stream URL, the trust file and a network route to the daemon. The development runner reaches Fly through the `wg-quick@fly-dev` WireGuard tunnel, with split DNS for `*.internal` names.

### Four streams on one connection

The connection authenticates once on the control stream. Three more streams then bind to that confirmed connection, each a duplex `POST /v1/runners/me/stream`.

| Stream | Carries |
|---|---|
| `control` | Hello, credits, heartbeats, renewals, cancellation and drain |
| `work` | Runs (`Down::Run`), folded input, and a chat run's context load and save |
| `reports` | Run completion and its acknowledgement |
| `activity` | Live progress frames, with no acknowledgement |

Each message is a four-byte big-endian length followed by UTF-8 JSON. Wrong-stream messages, unknown fields and conflicting bindings are refused.

Control frames hold at most 2 MiB (`CONTROL_MESSAGE_BYTES_MAX`). Work, report and activity frames hold at most 32 MiB (`MESSAGE_BYTES_MAX`). At most 256 requests wait for answers at once.

Each stream kind draws on its own memory pool, split evenly between sending and receiving. Accepted runs keep a separate allowance of one full frame each, so busy workers cannot block report acknowledgements.

A run whose payload would exceed the frame is never sent. The daemon fails its turn at assignment with `frame_too_large` and `UZ-RUN-032` ([unframed](../../rustd/crates/daemon/afd_gateway/src/connection/unframed.rs)).

### Opening and protocol version

The runner opens with `Hello`, naming the runs it still holds and its idle worker count. The daemon answers with `Welcome` and the runner's assigned policy.

The protocol has one version, `af_wire::protocol::VERSION`, today `agentsfleet.runner.v7`. A runner naming any other version is refused at hello with `UZ-RUN-023` and gets no run.

### Run lease and renewal

Each run holds a lease: a deadline after which another runner may take its turn. A heartbeat round renews every live run on the host at once.

| Timing | Value | Meaning |
|---|---|---|
| Round | Every heartbeat interval, 10 seconds by default | Sends the heartbeat and one renewal frame naming every live run; each answer has a 5-second deadline |
| Run lease | 38 seconds | One round (10) plus the answer (5), the reconnect budget (7), the renewal window (15) and one second |
| Silence | Two heartbeat intervals | Closes the connection |
| Offline | 114 seconds | Three leases of silence; the host reads `offline` |
| Reconnect budget | 7 seconds | HTTP/3 setup 2 seconds, HTTP/2 setup 2 seconds, opening and rebuild 3 seconds |
| Reconnect delay | Jittered exponential backoff, capped at 30 seconds | The first answered request resets it |

[The run clock](../../rustd/crates/daemon/afd_runner/src/timing/mod.rs) and [the shared spans](../../rustd/crates/foundation/af_core/src/timing/mod.rs) assert these relationships at compile time.

A renewal checks the fence, a person's Stop and, every third frame, the payer's balance (`FUNDS_CHECK_TICKS`). A refused renewal answers an error code, a detail and a retryable flag.

A durable task's run is never renewed past its task deadline; a renewal past it answers `UZ-RUN-011`. No other clock ends a run.

The runner starts stopping fleet work nine seconds before its last confirmed deadline. That leaves time to stop the sandbox and reap its processes.

A reconnect or a report retry cannot extend a deadline. Reconnect starts renewal at once, without waiting for the next round.

### Reconnect, ownership and drain

Reconnect rebuilds live work from Postgres. One database marker names the connection that owns the runner, and it survives network loss.

A fresh process using the same token replaces an older one. The older process receives `UZ-RUN-021` and halts.

Archive and delete save the cancellation before sending it. Renewal and reconnect read that saved decision if the message is lost. Revoking or rotating a token fences new grants before the old socket notices.

During a daemon drain, the daemon stops assigning runs and answers accepted requests. It then sends `go_away` and closes. The runner reconnects with its pending results still in memory.

### Results and acknowledgements

Postgres owns saved results, acknowledgements and billing records. A worker retries an unacknowledged report from memory under its original identity.

A result holds its worker until acknowledgement or a terminal refusal. A lost acknowledgement never causes a second charge.

On shutdown the runner gives accepted work and result delivery 35 seconds. Workers then get a five-second cleanup grace before forced cancellation.

### Run payload and fenced verbs

Every `Down::Run` carries one turn of a fleet's thread. Its payload holds the fleet's name, unfinished tasks, memory window, thread history and network policy.

The field `inputs_waiting` says whether a later message is already queued. When false, the run skips the fold request before its first model call.

A chat run reads and saves its thread's compacted context on the work stream, under its fence ([context lifecycle](context_lifecycle.md)). The runner makes every model call; the daemon makes none.

Some run verbs stay HTTP routes under `/v1/runners/me/runs/{run_id}`, fenced by the run's token. They include access requests, trigger rows, task bindings and full tool records ([API resources](api_resources.md)).

A run sends one `POST /searches` or `POST /extracts` under that same run URL for each web tool call. Both carry `fencing_token` and `call_number`. Only a public fleet may use them. Each physical vendor dispatch has a committed reservation. The daemon has a 12-second budget across all attempts. A lost response has an unknown outcome and the runner does not resend it.

A temporary extract failure permits one credential-free local read only when its 502 or 503 problem carries both `UZ-WEB-009` and `local_read_permitted: true`. The runner uses the remaining tool deadline and checks network policy on every hop.

### Refusals

| Code | Cause | Next action |
|---|---|---|
| [UZ-RUN-001](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-001) | Missing, changed or invalid runner token | Set the runner's current token; re-register if needed |
| [UZ-RUN-009](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-009) | Operator state blocks access | Check cordon, drain or revocation |
| [UZ-RUN-021](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-021) | A newer connection owns the runner | Stop the duplicate process; run one process per token |
| [UZ-RUN-022](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-022) | The fleet was archived or deleted | Stop the run and report its final result |
| [UZ-RUN-023](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-023) | Runner and daemon speak different protocol versions | Deploy the runner and daemon from one release |
| [UZ-RUN-024](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-024) | The stream ended or stopped answering | None; the runner reconnects. If it repeats, check UDP and TCP reach to the stream URL |
| [UZ-RUN-032](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-032) | A run's payload exceeds the frame | None; the turn fails at assignment and is not retried |

Connection signals, such as `agentsfleet_runner_stream_connect_total` and the fallback counter, are listed in [observability](observability.md). Logs never carry tokens, private keys, payloads or tenant text.

## Limits

- The runner keeps no report spool. A process that dies before an acknowledgement can lose that report and its usage.
- Repeated outages can outlast the 7-second reconnect budget. The run then stops at its confirmed deadline, and another runner may take the turn.
- Cordon and drain refuse the runner's authentication, renewals included ([runner fleet](runner_fleet.md#limits)).
- Repository tests prove the protocol, not the deployed Fly route. After a deploy, the development runner gate checks private DNS, both transports and a recent heartbeat.

## Related pages

- [Generate and store listener certificates](../../playbooks/operations/runner_stream_tls/001_playbook.md)
- [Runner fleet](runner_fleet.md): registration, policy, state and failure recovery
- [Runner runtime](runner_execution.md): what the runner does with a run
- [Configuration reference](config_reference.md): every daemon and runner key
- [Authentication rules](../AUTH.md)
