---
type: explanation
audience: operator
verified: 2026-11-09
product_version: 0.75.0
executable: false
---

# Runner stream protocol

## What it is

Each runner opens one outbound connection to `agentsfleetd` for work, cancellation, progress and results.
The runner prefers HTTP/3 and uses HTTP/2 when an eligible transport failure prevents HTTP/3.

Read this page to connect a runner, set certificate trust or understand reconnect behavior.
The [certificate guide](../../playbooks/operations/runner_stream_tls/001_playbook.md) gives the generation and vault steps.

## Why it exists

The daemon pushes work to available workers, so idle workers send no lease requests.
One authenticated session carries both fleet runs and authoring operations.
The browser setup chat keeps its existing API and user authentication.

## How it behaves

### Certificates and configuration

Use separate certificates in development and production.
The certificate must cover the hostname in `AGENTSFLEET_RUNNER_STREAM_URL`, including a private hostname when using Fly networking.

| Environment | 1Password vault | Certificate hostname |
|---|---|---|
| Development | `ZMB_CD_DEV` | `agentsfleetd-dev.internal` |
| Production | `ZMB_CD_PROD` | `agentsfleetd-prod.internal` |

The daemon needs its leaf certificate chain and matching private key as protected files.
Runners need the issuer's public certificate when their operating system does not trust that issuer.
Runners keep their existing bearer tokens and need no client certificates.

| Item | Fields | Runtime use |
|---|---|---|
| `agentsfleet-runner-stream` | `ca-certificate`, concealed `ca-private-key`, `certificate-chain`, concealed `private-key`, `server-name` | `ca-certificate`: public runner trust; `ca-private-key`: signing only, never runtime; `certificate-chain` and `private-key`: daemon files; `server-name`: hostname metadata |

Each environment's vault holds one `agentsfleet-runner-stream` item.
The CA signing key sits beside runtime material, where the Continuous Integration (CI) service account can read it.
Indy accepted this access on Nov 09, 2026.

The DEV item also holds concealed `wireguard-config`, used only to restore the runner host's private route.
PROD has no tunnel configuration or machines yet.

Certificate files use Privacy-Enhanced Mail (PEM) encoding.
The issuer is a certificate authority (CA).

The application reads file paths, never 1Password references, for certificate material.
The development and release workflows load these fields through `op://<vault>/agentsfleet-runner-stream/<field>` references.
They stage the daemon chain and leaf key as Fly file secrets before deployment; Fly writes them under `/etc/agentsfleet/runner-stream/` at boot.

The runner bootstrap installs public trust at `/etc/agentsfleet/runner-stream-trust.pem` and sets the stream URL in its existing protected environment file.
No deployment reads `ca-private-key`.

| Process | Setting | Effect and default |
|---|---|---|
| Daemon | `AGENTSFLEET_RUNNER_STREAM_LISTEN_ADDR` | TCP and UDP bind address; default `[::]:4443`; valid IP address and port |
| Daemon | `AGENTSFLEET_RUNNER_STREAM_TLS_CERT_FILE` | Required PEM leaf certificate followed by its issuer chain; no default |
| Daemon | `AGENTSFLEET_RUNNER_STREAM_TLS_KEY_FILE` | Required PEM private key matching the leaf certificate; no default |
| Runner | `AGENTSFLEET_RUNNER_STREAM_URL` | Required HTTPS origin; no credentials, path, query or fragment; no default |
| Runner | `AGENTSFLEET_RUNNER_STREAM_TLS_CA_CERT_FILE` | Optional public CA bundle added to operating-system trust; default operating-system trust |
| Runner | `AGENTSFLEET_RUNNER_TOKEN` | Required existing `agt_r` runner credential; no default |
| Runner | `AGENTSFLEET_API_URL` | Required HTTP or HTTPS base URL for the remaining HTTP operations; no default |

For development, the native origin is `https://agentsfleetd-dev.internal:4443`.
The client adds `/v1/runners/me/stream`; configure only the origin.

The configuration readers are [daemon preflight](../../rustd/crates/daemon/main/src/preflight/runner_listener/mod.rs) and [runner configuration](../../rustd/crates/runner/afr_supervisor/src/config/stream.rs).

### Connection direction and deployment

The runner needs outbound access to the configured daemon endpoint.
The daemon never dials the runner, and the runner needs no inbound listener.

```text
Runner ── private network ──> agentsfleetd listener
         HTTP/3: UDP 4443
         HTTP/2: TCP 4443
```

User Datagram Protocol (UDP) carries HTTP/3 through QUIC.
Transmission Control Protocol (TCP) carries HTTP/2.
The port above is the default; both listeners use the configured address.

Each UDP datagram that carries QUIC holds at most 1232 bytes of payload.
That size fits the 1280-byte minimum Internet Protocol version 6 (IPv6) maximum transmission unit (MTU), after the IPv6 and UDP headers.
No setting changes the [ceiling](../../rustd/crates/protocol/af_stream/src/mtu.rs).

[Fly documents](https://fly.io/docs/networking/app-services/) that `[::]` also accepts private IPv6 traffic.
Direct private access needs no public `[[services]]` entry.

The DEV OVH runner reaches Fly through `wg-quick@fly-dev`.
Preparation installs WireGuard; deployment restores `/etc/wireguard/fly-dev.conf` from the DEV vault through SSH input, with root ownership and mode `0600`.

The tunnel carries only the Fly organization prefix; split DNS sends `*.internal` lookups to Fly.
An external runner needs equivalent routing, name resolution and permitted TCP/UDP traffic.
Certificate delivery alone creates no route.

Permit both transports to allow HTTP/2 fallback when UDP is unavailable.
Any intermediary must pass duplex streaming and retain native connection ownership across packets.
Cloudflare is not required.

### One session and two transports

Both transports require Transport Layer Security (TLS) 1.3 and `X25519MLKEM768` hybrid key exchange.
Classical-only peers are refused, including during fallback.
Certificate signatures remain conventional; handshake keys need no vault entry.

Network failures and bounded transport timeouts permit fallback.
Certificate, credential, protocol and policy refusals fail closed.
An explicit HTTP CONNECT proxy selects HTTP/2; HTTP/3 never bypasses that policy.

A healthy HTTP/2 session stays on HTTP/2.
The runner tries HTTP/3 again during a later permitted connection setup, after recent UDP failure history allows another attempt.
There is no periodic transport upgrade or hot switch.

UDP failure history starts only when HTTP/3 fails and HTTP/2 then carries the session.
If neither carrier connects, no new history starts, so a daemon restart alone does not send the runner to HTTP/2.

### Recovery and availability

Reconnect rebuilds live work from Postgres.
Each running lease keeps its last confirmed expiry and stops when that deadline expires.
A reconnect or report retry cannot extend that deadline.

Fleet work starts stopping nine seconds before expiry.
This leaves time to stop the engine and reap its processes.

| Timing | Value | Meaning |
|---|---|---|
| Heartbeat | Every 10 seconds | No heartbeat for two intervals closes the session |
| Ping | Every 15 seconds | An unanswered ping fails after 5 seconds; idle detection can take 20 seconds |
| Renewal | Every 5 seconds | Each answer has its own 5-second deadline |
| Lease grant | 35 seconds | Includes recovery, another renewal answer and physical cleanup |
| Offline threshold | 105 seconds | Three missed lease lifetimes mark the host offline |
| Recovery attempt | 7 seconds total | HTTP/3 setup 2 seconds, HTTP/2 setup 2 seconds, authenticated opening and rebuild 3 seconds |
| Reconnect delay | Jittered exponential backoff, capped at 30 seconds | Failed openings and short sessions retain retry history |
| Stable reset | 60 seconds | A stable session resets reconnect backoff |
| UDP failure history | 30 seconds | How long a new connection skips HTTP/3 after HTTP/3 failed and HTTP/2 carried the session |
| Replica work probe | Every 5 seconds | Finds work after a lost notification |

[Timing constants](../../rustd/crates/foundation/af_core/src/timing.rs) assert the renewal and recovery relationship.
Repeated outages can exceed the recovery window; confirmed lease expiry still stops the running lease.
Reconnect starts renewal immediately; it never waits for the next periodic tick.

A kill saves cancellation before sending its notification.
Renewal and reconnect read that saved decision if the notification is lost.
Revocation and token rotation fence new grants before an old socket detects the change.

During drain, the daemon stops offers, answers accepted requests, then sends `go_away` and closes the session.
The runner reconnects with its pending results still in memory.

### Ownership during reconnect

One database marker identifies the current connection owner.
Preparation and confirmation compare the previous owner and credential under the host lock before a new session becomes authoritative.
The marker survives network disconnects.

A fresh process using the same token can replace an older process.
The older process receives UZ-RUN-021 and halts; stop the duplicate process before restarting the intended runner.

### Refusals

| Code | Cause | Next action |
|---|---|---|
| [UZ-RUN-001](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-001) | Missing, changed or invalid runner token | Set the intended runner's current token; re-register if necessary |
| [UZ-RUN-009](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-009) | Administrative state blocks access | Check cordon, drain or revocation; re-enroll when required |
| [UZ-RUN-021](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-021) | A newer connection owns the runner | Stop the duplicate process; keep one intended process per token |
| [UZ-RUN-022](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-022) | The fleet was killed | Stop that lease and report its final result |
| [UZ-RUN-023](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-023) | Runner and daemon use different protocols | Upgrade the runner and daemon together |
| [UZ-RUN-024](https://docs.agentsfleet.net/api-reference/error-codes#UZ-RUN-024) | The stream ended or stopped answering | None; the runner reconnects. If it repeats, check UDP and TCP reachability of the stream URL |

### Results and billing

Postgres owns saved results, acknowledgements and billing records.
Workers retry unacknowledged reports and task-decision receipts from memory using their original identities.
Caller shutdown allows accepted work and result delivery thirty-five seconds, then stops remaining work.

Workers receive the existing five-second cleanup grace before forced cancellation and joining.
Process exit after that grace can lose reports or usage that the daemon has not acknowledged.
A lost acknowledgement does not authorize another charge or physical provider call.

An unacknowledged result occupies its worker until acknowledgement or a terminal refusal.
Process loss before the database commit can lose that unsent result and usage.
The runner writes no report spool and replays no files from an earlier process.

### Authoring operations on the native session

Authoring model calls, proposals, renewals and completion share the authenticated runner session.
The daemon checks the current connection owner and credential at each authority boundary.
Model credentials and saved results remain in the daemon.

An accepted physical provider call stays tracked after disconnect so known usage can reach Postgres.
A fenced result retains that usage while withholding private text.
Uncertain physical model calls are never automatically redispatched.

### Session protocol

The session authenticates once, then binds four independently bounded streams to the same physical connection.
Control stays available while reports, model results or activity are waiting.
Wrong-stream messages, unknown fields and conflicting session bindings are refused.

Each stream is a duplex `POST /v1/runners/me/stream` on the same physical connection.
The control stream authenticates and confirms ownership; secondary streams bind to that confirmed session.

| Kind | Carries |
|---|---|
| `control` | Opening, credits, heartbeat, ping, renewals, cancellation and drain |
| `work` | Offers, folded input, authoring model calls and proposals |
| `reports` | Fleet and authoring completion with matching acknowledgements |
| `activity` | Fleet progress without application acknowledgements |

Messages use a four-byte big-endian length followed by UTF-8 JSON.
Session-local request identifiers correlate answers; cancellation and disconnect remove waiting callers, so late answers cannot reach another request.

The [wire types](../../rustd/crates/protocol/af_wire/src/protocol/mod.rs) define messages and bounds.
The [gateway](../../rustd/crates/daemon/afd_gateway/src/session/mod.rs) and [runner](../../rustd/crates/runner/afr_supervisor/src/downlink/mod.rs) own task cleanup and session state.

#### Memory configuration

| Setting | Default | Bounds |
|---|---|---|
| `AGENTSFLEET_RUNNER_CONTROL_MEMORY_MIB` | 16 MiB per replica | 12–512 MiB |
| `AGENTSFLEET_RUNNER_DATA_MEMORY_MIB` | 48 MiB per replica | 12–512 MiB |
| `AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY` | 64 fleet identifiers | 1–65,536 identifiers |

Receive and send pools are separate; each receives half the configured totals.
Invalid values refuse startup before allocation.

Accepted offers use a separate allowance of up to 128 MiB for 64 leases, so workers holding offers cannot block report acknowledgements.
The 2 MiB frame and 256 pending-request limits remain fixed protocol bounds.

### Observability

Connection records identify transport, failure reason, fallback and recovery duration.
Queue-pressure and dropped-activity counters expose congestion; ping and renewal timings expose a delayed application path.
Metric labels remain finite; tokens, private keys, payloads and tenant text never enter logs.

## Limits

Repository tests cover native behavior; they cannot prove the deployed Fly route.
After deployment, the DEV runner gate checks private DNS, private API readiness, separate HTTP/2 and HTTP/3 TLS connections, and a recent native heartbeat.

## Related pages

- [Generate and store listener certificates](../../playbooks/operations/runner_stream_tls/001_playbook.md)
- [Authentication rules](../AUTH.md)
- [Work scheduling](push_scheduler.md)
