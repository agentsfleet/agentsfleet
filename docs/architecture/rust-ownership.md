# Rust ownership and crate reuse

This decision record follows the September 2026 audit of `rustd/` at
`685725d7f95b0bb3bf2d98ccdb59f3df75616b4b`. The inventory contained 1,139 Rust
source files across 36 crates. Every file was read for inventory, declaration,
pattern and clone analysis; detailed manual review concentrated on security
boundaries and the resulting findings. Generated build output was excluded.

## Review rubric

Score a function, type or cohesive module on three ordinal axes. The resulting
27-cell cube records a decision context, not a numerical quality grade.

| Axis | 1 | 2 | 3 |
|---|---|---|---|
| Consequence | Local maintenance | Availability or ordinary data | Authentication, secrets, isolation or schema integrity |
| Replacement fit | Application policy | Partial library fit requiring adaptation | Existing dependency or internal owner covers the mechanism |
| Ownership burden | Bounded implementation with tests | Duplicated mechanism or substantial protocol code | Demonstrated divergence or missing security boundary |

Consequence 3 with burden 3 is first priority. Other demonstrated defects and
duplicated mechanisms covered by an existing owner follow. Keep domain policy
whose replacement has no demonstrated benefit. Record confidence and migration
effort separately; multiplying ordinal scores would imply precision they lack.

Each assessment needs a trust boundary, invariant, source evidence, other
implementations, a specific replacement API, preserved behavior, and meaningful
verification. A keyword hit or repeated test fixture is not itself a defect.

## Ownership decisions

| Area | Owner and decision |
|---|---|
| JWT signatures | `jsonwebtoken` verifies RS256 signatures. `afd_identity` retains injected time, exact expiry, issuer, audience, subject, scope and workspace policy. Signature verification remains mandatory before claim decisions. |
| JWK parsing | `jsonwebtoken::jwk::Jwk` reads each raw key. The adapter requires RSA and compatible `use`, `alg` and `key_ops` metadata, rejects ambiguous usable identifiers, and skips individually unusable siblings. Raw-key deserialization preserves duplicate-member refusal. |
| JWKS availability | `afd_identity::jwks::cache` owns six-hour freshness, thirty-second refresh throttling and fifteen minutes of outage grace. Failed refreshes never renew the last successful confirmation. After the ceiling, verification is unavailable. |
| Capability caching | Moka owns storage and coalesced initialization. Separate bounded fresh and stale caches preserve repeated outage fallback without refreshing its age. Application code owns freshness, the hard stale ceiling and unknown-subject semantics. |
| Secret text | `afd_crypto::secret::SecretString` centralizes the existing `zeroize` wrapper, explicit exposure and redacted formatting. Scheduler credentials and connector response bodies use it. Domain credential types remain distinct. |
| Secret JSON | `SecretObject` guards owned JSON keys and string values, including displaced refresh tokens; canonical output enters `SecretBytes` immediately. Serde owns parsing and serialization. |
| Encrypted rows | `afd_vault::StoredEnvelope`, using SQLx `FromRow`, owns named-column reconstruction for vault load, credential load and activation. The crypto crate validates envelope lengths and encryption metadata. Workspace AAD remains mandatory. |
| Migration grammar | PostgreSQL parses complete migration batches delivered through `sqlx::raw_sql`. The local statement scanner is removed. `afd_db` still owns the migration lock, version history, transaction, failure records and schema-ahead refusal. |
| URL escapes | `percent-encoding` owns decoding. A small shared adapter preserves strict malformed-escape refusal, path-byte semantics and form `+`/UTF-8 behavior. Query selection remains application policy. |
| Bundle versions | Import and runtime installation share `afd_fleet_runtime::Version`, backed by `semver`. Accepted versions are stable numeric triples with `u64` components; prerelease and build suffixes are refused at import. |
| Calendar dates | Jiff replaces local Gregorian arithmetic. Nonpositive times display the epoch date. Positive times outside Jiff's supported range display the original millisecond value as text. |
| UUIDs | The existing `uuid` implementation and `afd_core::id::Uuid7` remain. Repeated audit-ID adapters share `Entropy::uuid_randomness`; callers retain their original error conversion and injected clock. |
| Generic and Svix delivery | One authenticated-delivery helper owns pause acknowledgement, JSON parsing and dispatch. Each route retains its signature verification and event-ID selection. Specialized GitHub and approval routing stays separate. |
| RBAC and the datastore | Keep domain scope, tenant-confinement and authorization policy. Datastore connections and commands already use `redis`; atomic Lua scripts encode application transitions and are not replaced merely because they are custom. |
| Svix | Keep the documented vendored verification fork and its compatibility tests. The audit found no basis for an unreviewed dependency switch. |

The principal regression proofs cover repeated provider failures, 100 concurrent
capability callers, independent subjects, narrowing, cancellation, same-cache key
removal/republication, ambiguous JWK fields, redacted scheduler diagnostics,
malformed percent escapes, import/install version agreement and paused webhooks.
The live migration regression fails after successful DDL, checks rollback and
failure recording, then checks a corrected retry and subsequent idempotence.

Secret guards cover the allocations they own. They do not claim to erase Serde
scratch storage, parser error-path allocations, borrowed input, or copies held by
transport libraries. Tests must not read freed memory to infer zeroization.

## October 2026 audit: owners added and replacements refused

The second audit covered all 54 crates, 18 more than September's, with a finder and an independent
verifier per crate group; the per-site record is
`docs/v2/reviews/m218-rust-duplication-ledger.md`. It added these owners:

| Area | Owner and decision |
|---|---|
| Configuration knobs | `afd_core::env::optional` reads a knob trimmed, and blank as unset. Every daemon-side configuration knob reader calls it. The daemon's terminal sensing (`NO_COLOR`, `TERM`, the service-manager marker) stays a direct presence check, where a set-but-blank value still counts as set. The runner supervisor keeps its own untrimmed reader on purpose: it refuses a padded runner token today, and a trimming reader would accept one. |
| Direct HTTP clients | `afd_identity::direct(timeout)` is the one builder for a client that must ignore an environment proxy: the key-set fetch, the claim read, the metadata write, the GitHub library fetch, the vendor token exchange and the scheduler. It returns a builder, so a caller that also bounds redirects says so before building, and maps a build failure into its own error. A vendor client that cannot build refuses boot rather than falling back to a proxy-honouring default. |
| Egress slots | `afr_sandbox`'s slot pool is reached through `Kernel::slots()`: the running kernel's pool is one static, because every engine in a process names its links in the same host namespace; a fake kernel holds its own, so tests need no process-wide lock. |
| Log lines of a coded failure | `Coded::logged()` binds `error_code` and `reason`; `reason` is `Coded::told()` — the sentence and its causes, without the code a second time or a captured backtrace. A line that logs a fixed code keeps it and takes only `told()`. |
| Hex, MAC tags and entity tags | `hex::encode` renders bytes; `afd_crypto`'s tag type compares a presented hex tag in constant time over its hex form, so no caller decodes untrusted hex first; `afd_core::etag::compute` is the one entity-tag encoding. |
| Small shared rules | `afd_validate::rules` owns the path-segment rule two crates wrote; `afd_db` owns the query type alias and the escaping of untrusted text inside a `LIKE` pattern; `afd_api_wire::approval` owns the approval-gate status vocabulary both the gate and the inbox read. |

Replacements the verifiers refused, kept here so they are not proposed again without new evidence:

| Proposed replacement | Why it was refused |
|---|---|
| `url::Url` for the connector token pin, the vault projection guard and the runner allowlist | Each hand guard is stricter exactly where its comment says it must be: WHATWG parsing reads `\` as `/`, collapses `///`, accepts `scheme:host` with no `//`, accepts an empty userinfo before `@`, and reads `host:443` as a scheme. Keeping every refusal means keeping the hand split. |
| `backon` for regenerating a workspace or signup name on collision | Not a retry: no transient fault and no wait. Each pass draws a new name because the last was taken, inside one held connection. |
| `headers::IfMatch` / `IfNoneMatch` for the fleet and entity-tag preconditions | Both directions change: `*` and tag lists would pass where they answer 412 today, a malformed member would be skipped where the header is refused today, and a quoted tag with a space trips a debug assertion. |
| garde, or `afd_validate::Limit`, for a raw body byte cap or a clamping `?limit` | A byte cap is checked before parsing, with its own code and order; `Limit` refuses where these routes clamp. |
| A shared trimming reader for the runner's token | See configuration knobs above. |

## OAuth library evaluation

`afd_connector::oauth` already delegates encoding to `url` and request encoding
to reqwest. Its remaining shared exchange is a five-field authorization-code
form. Provider policy includes extra authorization parameters, absent scopes for
app installs, provider-specific response bodies, data-centre endpoints, and a
single endpoint override for test isolation.

The [`oauth2` crate](https://docs.rs/oauth2/latest/oauth2/) can own authorization
URLs, typed exchange requests and token responses. Adopting it would still require
the provider adapters and grant parsing, plus translation into the existing secret
and error types. Retain the current bounded composition for this change. Revisit
when PKCE, discovery or additional grant types create shared protocol machinery;
that evaluation should compare deleted code and preserved provider contracts.

## Migration checksum upgrade design

The existing ledger has no SQL checksum. Replacing its executor does not provide
drift detection, and switching to SQLx's separate migration ledger would not
preserve deployed history automatically. Keep that limitation explicit.

A checksum upgrade needs these steps:

1. Reconcile `.orly/dispatch/write_sql.md`'s pre-2.0 prohibition on `ALTER TABLE` with
   `.orly/docs/SCHEMA_CONVENTIONS.md`'s post-rebuild additive-migration policy. This
   implementation changes neither the schema nor those rules.
2. Introduce a real new migration version for the ledger upgrade, so older
   binaries refuse the newer database through the existing schema-ahead check.
3. Hash exact embedded SQL bytes using the existing `sha2` dependency. Under the
   migration advisory lock, check schema-ahead status and all applied checksums
   before clearing failures or changing history. A mismatch refuses startup.
4. Commit each new SQL batch, version, timestamp and checksum in one transaction.
5. Adopt existing rows explicitly against a known deployed artifact. Never fill
   missing hashes silently from the new checkout: that would approve potentially
   edited SQL. Record baseline provenance and the inability to prove earlier
   history from a newly adopted baseline.
6. Exercise fresh databases, adoption, mismatch without mutation, numbering gaps,
   retired versions, failure/retry and older-binary refusal against PostgreSQL.

This is a separate data-history transition. No destructive rebuild or silent
adoption is part of the crate-reuse implementation.
