---
type: explanation
audience: contributor
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# M243 transport and TOON reference study

## What it is

Use Quiche through its Tokio adapter as the first HTTP/3 candidate.
Use Neqo for interoperability ideas and RustAPI for comparison, subject to the limitations below.
Evaluate Token-Oriented Object Notation (TOON) separately for structured data sent to models.

Indy's [implementation sign-off](implementation-signoff.md) permits the transport proof and production wiring after that proof passes.
This study makes no measured `agentsfleet` throughput, latency or model-quality claim.

### Cloned references

| Repository | Checkout | Revision |
|---|---|---|
| [Tuntii/RustAPI](https://github.com/Tuntii/RustAPI) | `/workspace/oss/RustAPI` | `6d07ddf40cb7ea0d824831573aa4d30c2726b643` |
| [cloudflare/quiche](https://github.com/cloudflare/quiche) | `/workspace/oss/quiche` | `fa0e48072e945ff2da5bb9a21684f86a91f35877` |
| [mozilla/neqo](https://github.com/mozilla/neqo) | `/workspace/oss/neqo` | `4b6d13566f56824bd4e1c161ec32f49bfceb3c7c` |
| [openai/codex](https://github.com/openai/codex) | `/workspace/oss/codex` | `748ab55ca7de0b647d9a7a9d9a8e70e00f28666d` |

Each revision comes from `git rev-parse HEAD` in its checkout.
Clones remain outside the product worktree and introduce no product dependency.
TOON's primary sources are its [format specification](https://github.com/toon-format/spec) and [Rust implementation](https://github.com/toon-format/toon-rust).

## Why it exists

M243 needs native streaming in both directions, bounded recovery and a server implementation suitable for production evaluation.
A crate's adoption history does not prove our adapter handles certificate failures, blocked writes or application backpressure.
The selected stack must pass those tests with the actual runner protocol.

## How it behaves

### Library decisions

| Reference | Evidence from source | Proposed use |
|---|---|---|
| Quiche | README names Cloudflare's edge, Android's resolver and curl; exposes QUIC and HTTP/3 | Select for the native transport proof |
| `tokio-quiche` 0.20.1 | Client/server HTTP/3 drivers, bounded per-stream body queues, explicit native callbacks | Adapt sockets and streaming bodies; retain a private boundary around its API |
| Neqo 0.34.0 checkout | Firefox's production client; README explicitly says its server is experimental and unsuitable for production | Study client state, authentication events, loss tests and interoperability; exclude its server dependency |
| RustAPI | Uses Quinn and Hyperium's h3 for HTTP/3; its Hyper feature selects HTTP/1 | Study shape and observability ideas; retain the existing Axum/Hyper stack |
| Codex | Separate reader/writer supervision, cancellation-aware reconnect and stable-session backoff reset | Apply lifecycle patterns; exclude its WebSocket carrier and forked dependencies |

Quiche is a library, independent of Cloudflare hosting, tunnels or public ingress.
Direct private Fly networking requires no Cloudflare service.
Its [Tokio client library documentation](https://developers.cloudflare.com/privacy-proxy/reference/client-libraries/) provides additional production-use evidence.

RustAPI's `crates/rustapi-core/src/http3.rs` reads the complete upload before invoking its router.
The same handler collects the complete response before sending it.
Copying that adapter would leave a never-ending M243 upload waiting forever for its response.

RustAPI also starts connection and request tasks with detached `tokio::spawn` calls in that adapter.
M243 needs supervised tasks and confirmed cancellation.
Adopting the whole framework would replace existing routing and authentication without solving those requirements.

### Quiche integration details

| Finding | Required treatment |
|---|---|
| Quiche uses BoringSSL; the previous HTTP/2 proposal used Rustls | Use BoringSSL for both new runner carriers under the scoped implementation approval and native proof |
| The repository ships static musl binaries for x86-64 and ARM64 | Prove both targets; the existing Alpine builder has CMake but does not install Clang/libclang |
| `NewClientRequest.body_writer = None` ends the upload | Supply and retain the body writer for every duplex lane |
| Per-connection events and commands use unbounded channels | Keep one active dispatcher, cap request creation and bound our queues by bytes and count |
| Peer-created streams can feed native events | Limit requests, header bytes and streams; reject an extra lane instead of leaving it queued |
| Quiche metadata handles do not own worker cancellation | Wrap the driver with cancellation and completion signals; confirm actual worker exit |
| `QuicConnection::stats()` snapshots primarily update at close | Use live audit values or worker callbacks for live diagnostics; label final snapshots as final |
| Native callbacks must not block | Copy finite numeric samples into the existing telemetry path; no network exporter or database call inside callbacks |
| Library errors can be boxed foreign errors | Preserve their source chain; classify known failures by type and fail closed on unknown security failures |
| Channel send errors can contain non-`Sync` command closures | Report a typed channel-closed condition without retaining the failed command payload |
| README and source signatures differ | Use the published crate source for the pinned version; do not paste README calls without compiling |

The client trust hook only runs when `TlsCertificatePaths` accompanies the hook.
A normal bearer-authenticated runner has no client certificate to supply.
The design must not invent a dummy identity to activate private certificate authority (CA) trust.

Quiche exposes an alternative: construct a verified Quiche configuration, then use `quic::raw::wrap_quiche_conn`.
That interface accepts a bounded inbound packet feed and exposes worker shutdown notification.
The native proof must establish its build, trust and cleanup behavior before selecting this adapter path.

Another supported trust path may use an operator-installed CA bundle shared by both carriers.
That needs an actual certificate test; disabling verification or treating a certificate refusal as network fallback is unacceptable.
No fork is approved by this study.

Disable early data, unused datagrams, optional compressed QUIC logs and TLS key logging.
Disable the dynamic QPACK table and blocked streams for these fixed small headers.
QPACK is HTTP/3's header compression protocol; dynamic table dependencies would add avoidable cross-stream waiting.

### TLS scope recommendation

Transport Layer Security (TLS) verifies the daemon and encrypts the connection.
Use BoringSSL for both HTTP/3 and HTTP/2 on the new runner link under Indy's scoped implementation approval.
Keep Hyper and h2 above a matching `tokio-boring` stream for HTTP/2.

The carriers can share trust-root loading and hostname verification rules while selecting their own negotiated HTTP protocol.
That reduces differences between the primary and fallback certificate paths.
Using one TLS library does not itself make the network more available.

The [Boring bindings](https://github.com/cloudflare/boring) provide a [Tokio adapter](https://github.com/cloudflare/boring/tree/master/tokio-boring).
The native proof must verify compatible dependency versions, certificate handling, duplex I/O, cancellation and static Linux builds.
The review itself changed documentation; native implementation and verification follow the recorded approval.

### Approved hybrid post-quantum key exchange

Indy prefers post-quantum protection for runners connecting over external networks.
The pinned BoringSSL 5.4.0 source lists `X25519MLKEM768` first in `DefaultSupportedGroupIds`.
Quiche 0.30.0 also exposes `Config::set_curves_list` for explicit group preferences.

The implementation requires TLS 1.3 and prefers hybrid X25519 plus ML-KEM-768 on both native transports.
Real HTTP/3 and HTTP/2 tests observed `X25519MLKEM768` after their verified handshakes.
Separate peers forced X25519, P-256 and P-384; their negotiated groups remained correctly identified.

A TLS 1.2-only peer was refused.
The package test command passed eight unit tests, 19 transport tests and one exported-telemetry test.
Evidence: `cargo test -p af_stream --all-features --no-fail-fast`, log `/workspace/scratch/m243-logs/m243-hybrid-af-stream-tests-v2.log`.

BoringSSL's older `mozilla_modern` preset disables TLS 1.3.
The implementation uses `mozilla_intermediate_v5` with an explicit TLS 1.3 minimum instead.
The negotiated-group trace identifies key exchange separately from certificate authentication.

Hybrid key exchange helps protect recorded traffic against future quantum decryption.
The proposed ECDSA certificate signatures still authenticate the server through conventional cryptography.
Fully post-quantum authentication would also require supported post-quantum certificate signatures.

Ephemeral hybrid handshake keys do not need new 1Password entries.
The vault still holds the issuer material and the daemon's certificate and leaf key.

A project-wide Rustls removal reaches active clients beyond M243's runner connection.
The pinned crate feature tables show different backend choices:

| Current dependency | Declared TLS backend support | Effect of a complete BoringSSL migration |
|---|---|---|
| reqwest 0.13.5 | Rustls and native-tls; no BoringSSL feature | Requires a different connector path, client replacement or upstream work |
| SQLx 0.9.0 | Rustls and native-tls; no BoringSSL feature | Requires database TLS integration work beyond changing the workspace feature |
| redis 1.8.1 | Rustls and native-tls; no BoringSSL feature | Requires broker TLS integration work beyond changing the workspace feature |
| lettre 0.11.23 | Rustls, native-tls and BoringSSL, including `tokio1-boring-tls` | A supported adapter exists, but changing email TLS is separate from the runner stream |

The [workspace declarations](../../../rustd/Cargo.toml) currently select Rustls for those clients.
Their pinned Cargo feature tables supplied the support inventory above.
The scoped runner change therefore retains Rustls and its AWS-LC cryptography dependency elsewhere in the binaries.

Rustls implements the TLS protocol; this workspace selects AWS-LC as its cryptographic provider.
AWS-LC derives from BoringSSL, but that does not make a Rustls connection a BoringSSL TLS connection.
A universal backend migration needs its own compatibility and failure evidence before approval.

### Local transport evidence

The scratch HTTP/3 probe passed verified-certificate duplex exchange while its upload remained open.
It echoed 2 MiB byte-identically and answered application pings for 120 seconds.
Explicit close produced the raw client's worker shutdown notification without configuring a client identity.

Command: `cargo run --quiet` in `/workspace/scratch/m243-transport/native`, using the task-local native build dependencies.
Output: `/workspace/scratch/m243-logs/native-h3-proof-v4.log`; source fingerprints: `native-h3-proof-v4.sources.sha256` beside that log.

The HTTP/2 probe uses `cargo run --quiet -- h2` and also passed duplex, 2 MiB payload, 120-second ping and task-exit checks.
Its output is `/workspace/scratch/m243-logs/native-h2-proof.log`.

Two probe corrections mattered before the HTTP/3 success:

- The test CA and leaf inherited the same default name; a distinct CA name corrected the chain rejected as self-signed.
- The pinned driver emits `ServerH3Event::Headers`, while its README example matches a core incoming-headers event that never arrives there.

The production adapter must preserve peer verification and match the pinned event types directly.
These scratch results do not satisfy production adapter, failure-injection, static-target or Fly deployment acceptance criteria.

### Workspace dependency conflict

Published `tokio-quiche 0.20.1` requires `libc < 0.2.190`; the workspace previously requested `0.2.191`.
The [upstream issue](https://github.com/rust-lang/libc/issues/5601) says its iOS regression was fixed in `0.2.191`.
The [pending cap update](https://github.com/cloudflare/quiche/pull/2841) changes one manifest line, but its revision includes 21 commits after the published transport release.

A scratch copy changed only that dependency requirement for local compilation and tests.
The workspace check passed, followed by 14 production adapter and framing tests in `/workspace/scratch/m243-logs/native-cap-proof-tests-v1.log`.
No scratch path is an approved dependency for the pull request.

Indy approved: “Pin workspace libc to 0.2.189 and require full Linux gates (Recommended)”.
The workspace and regenerated lockfile now use released `libc 0.2.189` with published `tokio-quiche 0.20.1`.
No scratch dependency override remains; verification must run against those published packages.

### Enabled QUIC features and telemetry

`tokio-quiche 0.20.1` is pinned with `default-features = false`.
Its published defaults enable qlog compression; these defaults are disabled here.
No handshake key logging is enabled.

| Feature | M243 decision |
|---|---|
| `rpk` | Disabled; runner connections verify certificate chains and endpoint names |
| `gcongestion` | Disabled; retain the published congestion controller until comparative measurements justify changing it |
| `zero-copy` | Deprecated flag; the pinned manifest says zero-copy sends are always enabled; this flag only enables `gcongestion` |
| `perf-quic-listener-metrics` | Disabled; native connection and handshake durations enter the shared telemetry pipeline directly |
| `tokio-task-metrics` | Disabled; enabling native task histograms alone would not export them through our declared producers |

The adapter samples public `quiche::Stats` and `PathStats`, then exports through an owned observation task.
Measurements include round-trip time, sent/lost/retransmission packets, retransmitted bytes and probe timeouts.
HTTP/2 emits no invented QUIC readings.

Each established connection publishes its final sample before the driver drops.
Its owner waits for the observation task during explicit close.
Failed handshakes remain visible through connection outcomes and measured timeouts.

### Exonum review reference

The inspected Exonum revision is `2d2fa22e5f5bc451d08c155c2398956f11dce06e`.
Its `exonum-node/src/events/codec.rs` separates framing from typed messages and network connection state.
The refactor keeps that boundary through `OwnedFrame`, typed HTTP headers and separate native connection owners.

Its historical decoder checks plaintext length after collecting and decrypting the encrypted frame.
Our reader proves the declared length before allocation, then reads directly into one zeroing buffer.
The old dependencies, late bounds and test unwraps are not imported.

### What TOON changes

TOON encodes the JSON data model as text using indentation and table-like rows.
Uniform objects declare their keys once instead of repeating them for every item.
For example, these values carry the same information:

```json
{"workers":[{"name":"builder","idle":true},{"name":"reviewer","idle":false}]}
```

```text
workers[2]{name,idle}:
  builder,true
  reviewer,false
```

The benefit concerns tokens consumed by a model reading those values.
It does not make sockets more available, replace QUIC framing or make arbitrary Rust operations run faster.
Small control messages never reach a model, so their punctuation has no model-token cost.

The [TOON project](https://github.com/toon-format/toon#when-not-to-use-toon) identifies uniform data as its strongest case.
It warns that compact JSON can win for nested or irregular data, and that latency requires measurement.
Its published benchmark uses one tokenizer; other model families may tokenize the same text differently.

### RustAPI's TOON claims and implementation

The README advertises 50–58 percent fewer tokens, while the crate's own pages give different ranges.
Its `LlmResponse` implementation estimates each count as UTF-8 byte length divided by four, rounded up.
Those headers are size estimates, not measurements from a model's tokenizer.

`LlmResponse::into_response` serializes both JSON and TOON even when token headers are disabled.
Its savings calculation clamps negative savings to zero.
Copying those choices would add serialization work and conceal cases where TOON costs more tokens.

The checkout locks `toon-format` at 0.4.6.
The primary Rust repository currently declares 0.7.0 and specification 4.4.
Any experiment needs an explicit crate/specification pair and conformance fixtures rather than copying an older wrapper's defaults.

### Where TOON could help `guffbolt` runs

`guffbolt::Conversation` supplies associated `Turn`, `Results` and `Failure` types without choosing a model format.
Its dependencies contain no model HTTP client or serialization crate.
Source: `rustd/crates/harness/guffbolt/{README.md,src/conversation.rs}`.

The actual runner embedder produces `af_providers::Message::ToolResult` in `afr_agent/src/loop/mod.rs`.
Its context estimator measures the full provider request in `afr_agent/src/loop/context/estimate.rs`.
Those boundaries matter because counting JSON while sending TOON would misbudget compaction.

| Candidate data | Recommendation |
|---|---|
| Search results or inventories containing many objects with the same fields | Strong candidate for a measured TOON projection into model-facing tool text |
| Nested configuration and irregular tool results | Compare with compact JSON; preserve JSON when TOON loses |
| Source code, shell output, logs and ordinary prose | Preserve the original text; do not reinterpret arbitrary output as a table |
| Provider tool definitions, function arguments and required structured outputs | Keep the provider's documented JSON schema and validation path |
| Runner session messages, SQL, report spools and billing records | Keep M243's typed canonical representation; TOON offers no model-token benefit here |

A later experiment should keep canonical tool results and create a model-facing projection from typed values.
Use the format's encoder and strict round-trip checks; never concatenate unescaped table cells.
Treat projected tool output as untrusted data with the same permissions as the original output.

The projection must feed both token estimation and the provider request, including replay after a checkpoint.
A stable formatting choice must preserve prompt-cache behavior across turns.
Introducing TOON into `guffbolt` itself would couple a generic loop to one embedder's data format.

### Measurement before adoption

Compare compact JSON and TOON over representative structured tool results, Unicode, nested data and long strings.
Measure actual tokenizer counts, encoding time, peak allocations, provider-reported input use, cache hits and task success.
Record first-token latency, total latency and total cost, including output and retries.

Use the existing token-counting decision and provider calibration traces rather than a bytes-per-token replacement.
Require lossless round trips, bounded depth and size, unchanged permissions and acceptable task quality before adoption.
A token reduction alone does not prove faster or cheaper completed tasks.

## Limits

No TOON benchmark has run against `agentsfleet` tool output in this study.
No RustAPI or Neqo benchmark ran here.
The scratch Quiche probe passed its local duplex, payload, idle and client-close checks; production transport acceptance tests remain due.

## Related pages

- [Implementation and ownership proposal](implementation-signoff.md)
- [Failure handling and observability](failure-and-observability.md)
- [TOON specification](https://github.com/toon-format/spec/blob/main/SPEC.md)
- [TOON tokenizer and accuracy methodology](https://github.com/toon-format/toon#benchmarks)
- [Existing token counting proposal](../tokenizer-approach-proposal.md)
