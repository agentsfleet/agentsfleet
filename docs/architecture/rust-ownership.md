---
type: explanation
audience: contributor
verified: 2026-11-27
product_version: 0.90.0
executable: false
---

# Rust ownership: which crate owns which mechanism

> Read this before you write a parser, cache, retry, token check, secret wrapper or URL guard in `rustd/`. Parent: [`README.md`](./README.md) · Rule: [the Rust standard](../../.orly/standards/languages/rust.md).

## What it is

Each mechanism below has one owner, either a well-maintained crate or one module in this workspace.
New code calls the owner; it never writes a second copy.

| Mechanism | Owner | What stays ours |
|---|---|---|
| JSON Web Token (JWT) signatures | `jsonwebtoken`, RS256 | time, expiry, issuer, audience, subject, scope and workspace checks in `afd_identity` |
| JSON Web Key (JWK) parsing | `jsonwebtoken::jwk::Jwk` | refusing non-RSA keys, mismatched `use`, `alg` or `key_ops`, and ambiguous key identifiers |
| Key set freshness | `afd_identity::jwks::cache` | 6-hour freshness, 30-second refresh throttle, 15 minutes of outage grace |
| Caches | `moka` | freshness rules and the hard stale ceiling |
| Retries | `backon` | which errors are worth retrying |
| Secret text | `af_crypto::secret::SecretString`, over `zeroize` | explicit exposure and redacted formatting |
| Secret JSON | `af_crypto`'s `SecretObject` and `SecretBytes`; `serde` parses | guarding keys and string values, refresh tokens included |
| Encrypted rows | `afd_vault::StoredEnvelope`, via SQLx `FromRow` | workspace additional authenticated data, always required |
| Migrations | SQLx `Migrator`, with checksums | `afd_db`'s bounded advisory lock |
| URL escapes | `percent-encoding` | refusing malformed escapes; form `+` handling |
| Calendar dates | `jiff` | how out-of-range times display |
| Identifiers | `uuid` through `af_core::id::Uuid7`; randomness from `Entropy::uuid_randomness` | each caller's error type and clock |
| Configuration knobs | `af_core::env::optional`: trimmed, blank as unset | the runner's untrimmed token reader, which refuses a padded token |
| HTTP clients that ignore a proxy | `afd_identity::direct(timeout)` | redirect bounds, set by each caller |
| Hex and entity tags | `hex::encode`; `af_core::lower_hex`; `af_core::etag::compute` | constant-time tag comparison in `af_crypto` |
| A coded failure's log fields | `Coded::logged()` and `Coded::told()` in `af_core` | which code a line names |
| Egress slots | `afr_firewall`'s `Kernel::slots()` | one pool per running kernel; a fake per test |
| Sandbox engines | `afr_engine`'s `Engine`, `Sandbox` and `Parked` traits | one `afr_engine_*` crate per engine; only the runner binary and `af_e2e` name one |
| Egress plan read-back | `afr_engine::enforce`, retried through `backon` | each engine's `Enforcer` and the stale-plan refusal |
| Trees into and out of a sandbox | `afr_executor::tree`, over `tar` and `cap-std` | the confinement checks, run by both the executor and the host stage |
| Toolbox and workspace images | `afr_image` | verify, stage, adopt and holds; loop devices and mounts |
| Cgroup limits and the freezer | `afr_cgroup` | tenant attribution |
| Sandbox-side hardening | `afr_guest`, over `landlock` and `seccompiler` | the refused system calls and the inner environment |
| Available memory | `procfs-core`'s `Meminfo` | the overcommit ratio and the per-microVM reservation |
| Path-segment and `LIKE` escaping | `af_validate::rules`; `afd_db` | nothing |
| Approval status words | `afd_api_wire::approval` | nothing |
| `If-None-Match` | `headers::IfNoneMatch`, through `precondition_passes` | the task read and the model list |
| Datastore clients | `sqlx`, `async-nats` | every statement and authorization rule |
| Svix webhook signatures | `af_webhook::vendor::svix`, a vendored copy with its compatibility tests | which route accepts the scheme |

## Why it exists

A second copy of a mechanism drifts from the first, and the drift is usually in a security check.
The workspace rule is to use an existing or well-maintained crate before writing a limiter, retry, cache, parser, pool or protocol piece.
This page records where each one already lives, so a reviewer can point at it.

## How it behaves

A replacement that loses a refusal the current code makes is not a replacement.
The table below lists proposals that failed that test, so they are not raised again without new evidence.

| Proposed replacement | Why it was refused |
|---|---|
| `url::Url` for the connector token pin, the vault projection guard and the runner allowlist | WHATWG URL parsing reads `\` as `/`, collapses `///`, accepts `scheme:host` and empty userinfo; each hand guard refuses these |
| `backon` for regenerating a name on collision | not a retry: no transient fault and no wait, just a new draw inside one held connection |
| `headers::IfMatch` for write preconditions | `*` and tag lists would pass where these routes answer 412, and a malformed member would be skipped instead of refused |
| `garde` or `af_validate::Limit` for a raw body cap or a clamping `?limit` | a byte cap is checked before parsing, with its own code; `Limit` refuses where these routes clamp |
| `oauth2` for `afd_connector`'s exchange | the shared part is one five-field form; provider policy would still need its adapters |

Secret guards cover only the memory they own.
They make no claim about `serde` scratch space, parser error paths, borrowed input or copies inside transport libraries.

## Limits

- **The list is not exhaustive.** A mechanism missing here may still have an owner; search the workspace's `Cargo.toml` and crates first.
- **Refusals can be revisited.** New evidence, such as a crate that keeps every refusal, reopens one.

## Related pages

- [`docs/RUST_ERROR_STANDARD.md`](../RUST_ERROR_STANDARD.md): how errors are declared
- [Rust standard](../../.orly/standards/languages/rust.md)
- [Postgres schemas](./postgres_schemas.md): migrations and layout
- [Testing](./testing.md)
