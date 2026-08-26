//! The cache in front of the mints, and the single-flight in front of the
//! cache.
//!
//! # `moka` is both, in one call
//!
//! The Zig spends three files here — `broker.zig` holds the cache and the
//! dispatch, `broker_key.zig` builds a separator-joined key out of two Wyhash
//! fingerprints, and `broker_flight.zig` is a per-key in-flight set with a
//! mutex, a bounded loser wait, and a poll loop — because `cache.zig` caches
//! and does not single-flight.
//!
//! [`moka::future::Cache`]'s entry API does both at once: exactly one caller
//! per key resolves the init future and the rest park on it. That is not a
//! convenience. Two cold misses on a ROTATING refresh provider both post the
//! same refresh token, and a provider with reuse detection revokes the whole
//! token family — the tenant's connection dies for everyone. The Zig's
//! hand-rolled guard exists for that case, and `or_try_insert_with` is the same
//! guarantee without the poll loop, the timeout, or the residual-key cleanup a
//! minter that died mid-flight leaves behind.
//!
//! `try` is the load-bearing half: a REFUSAL is handed to every waiter and
//! cached by nobody. A cache that stored failures would turn one bad minute at
//! a vendor into a minute of refusals for every fleet on the same key.
//!
//! # What makes two mints the same entry
//!
//! The workspace, the connector, and a digest over the handle's identity and
//! the fleet's binding. Every component is load-bearing:
//!
//! - **The binding.** Two fleets in ONE workspace minting the same integration
//!   from the same installation agree on everything else, so without it a
//!   read-scoped fleet is served the write-scoped token its neighbour cached —
//!   silently undoing the narrowing `ScopedRequest` performed.
//! - **The handle identity, minus the credential fields it rotates.** An
//!   ordinary refresh rotation must stay a cache HIT; a reconnect must be a
//!   miss, which the connect callbacks guarantee by stamping a fresh
//!   `connected_at_ms` on every stored handle.
//!
//! The digest is SHA-256 over the canonical JSON of those two things, and the
//! reason it is a real hash rather than the Zig's seeded Wyhash is what
//! `broker_key.zig` learned the hard way: its first spelling joined repository
//! names on a separator, so `["acme/a","acme/b"]` and the single spliced name
//! `"acme/a<SEP>acme/b"` hashed identically — a deterministic alias, needing no
//! collision, that served one fleet's broad-scope token to another. JSON is
//! self-delimiting, so no framing rule has to be got right; SHA-256 needs no
//! per-process seed to keep a digest from being precomputed.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_fleet_runtime::config::{Recorded, RepositoryBinding};
use moka::Expiry;
use moka::future::Cache;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::credential::outcome::{Minted, Outcome, Retry};
use crate::credential::platform::Platform;
use crate::credential::{github, oauth};
use crate::secrets::connector::{Connector, Connectors, Exchange, FIELD_INTEGRATION};

/// Re-mint this many milliseconds BEFORE the upstream expiry.
///
/// A token handed to a tool call has to outlive the call, and the daemon cannot
/// know how long that is. The slack is the Zig's and the direction is the only
/// safe one: too much costs a re-mint, too little costs a 401 mid-run.
const EXPIRY_SKEW_MS: i64 = 60_000;

/// How many live tokens the process will hold.
///
/// A bound on memory rather than a tuning knob — `moka` evicts least-recently
/// used beyond it, and an evicted entry costs one re-mint.
const MAX_CACHED_TOKENS: u64 = 8192;

/// The vault-handle fields that change on an ordinary rotation.
///
/// Excluded from the cache identity, which is what makes a rotation a HIT: the
/// handle after a refresh names the same installation as the handle before it.
/// A RECONNECT changes something else — the connect callbacks stamp
/// `connected_at_ms` — and correctly misses.
/// The stored expiry field, declared once (RULE UFS).
///
/// It is named twice for different reasons — once as a field a rotation
/// rewrites, once as a field the `Debug` rendering shows — and the two must
/// stay the same word or a rotation would rewrite a key nothing reads.
const FIELD_EXPIRES_AT_MS: &str = "expires_at_ms";

const ROTATING_FIELDS: [&str; 3] = [
    oauth::FIELD_REFRESH_TOKEN,
    "access_token",
    FIELD_EXPIRES_AT_MS,
];

/// The vault-handle field a `static` connector's credential sits in.
const FIELD_TOKEN: &str = "token";

/// What one runner is asking for.
#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    /// The workspace the lease resolved to. Never taken from the wire.
    pub workspace_id: &'a Uuid7,
    /// The stored handle, as the vault holds it.
    pub handle: &'a Value,
    /// The fleet's declared repository reach, for the connectors that scope by
    /// it. `None` is a fleet that declared none, and the GitHub mint refuses
    /// rather than widening.
    pub binding: Option<&'a RepositoryBinding>,
    /// The instant every expiry decision is measured from.
    pub now_ms: i64,
}

/// What makes two asks the same cache entry.
///
/// `Debug` is derived and safe: the digest is a digest, and a workspace id and
/// a connector name are not secrets. Nothing here is the credential.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    /// Invariant 2: no entry is reachable from another tenant's ask.
    workspace_id: Uuid7,
    /// Which connector minted it.
    connector: Box<str>,
    /// The handle identity and the binding, digested together.
    scope: [u8; 32],
}

/// The two things a digest is taken over, as one serialisable value.
///
/// A struct rather than two hashes concatenated, so the encoding is JSON's and
/// there is no boundary rule of ours to get wrong.
#[derive(Serialize)]
struct Scope<'a> {
    /// The handle's stable fields, in key order — `BTreeMap`, so the parser's
    /// insertion order cannot change the digest.
    ///
    /// Nested objects keep whatever order they were parsed in, which can only
    /// SPLIT one entry into two. That costs a re-mint and never serves the
    /// wrong token, which is the direction every fallback here takes.
    identity: BTreeMap<&'a str, &'a Value>,
    /// The fleet's reach, in the one shape a gate row also records it in.
    binding: Option<Recorded<'a>>,
}

/// A token this process is holding on to.
#[derive(Clone)]
struct Cached {
    /// The credential.
    token: Zeroizing<String>,
    /// When it stops working upstream.
    expires_at_ms: i64,
    /// A refresh token the exchange rotated, held only until the caller that
    /// performed that exchange has taken it — see [`Broker::mint`].
    rotated_refresh_token: Option<Zeroizing<String>>,
    /// How long this entry may stay cached, skew already subtracted.
    live_for: Duration,
}

// Hand-written for [`Minted`]'s reason: a derived one prints the token, and
// `Debug` is what a `tracing` field renders through.
impl std::fmt::Debug for Cached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cached")
            .field(FIELD_EXPIRES_AT_MS, &self.expires_at_ms)
            .field("live_for", &self.live_for)
            .finish_non_exhaustive()
    }
}

impl Cached {
    /// The cache entry for a freshly minted credential.
    fn new(minted: Minted, now_ms: i64) -> Self {
        // Saturating, and then clamped at zero by the conversion: a provider
        // that returned a token expiring inside the skew is cached for no time
        // at all rather than for a negative one. The CALLER still receives it —
        // what it does not get is a second caller a minute later.
        let live_ms = minted
            .expires_at_ms
            .saturating_sub(now_ms)
            .saturating_sub(EXPIRY_SKEW_MS);
        Self {
            token: minted.token,
            expires_at_ms: minted.expires_at_ms,
            rotated_refresh_token: minted.rotated_refresh_token,
            live_for: Duration::from_millis(u64::try_from(live_ms).unwrap_or(0)),
        }
    }

    /// Whether this entry is still worth handing over at `now_ms`.
    ///
    /// Checked against the INJECTED clock rather than left to `moka`'s own
    /// expiry alone, which is what keeps the decision provable without waiting
    /// for wall-clock time to pass. The two agree; this one is the earlier of
    /// them and the one a test can drive.
    const fn is_live(&self, now_ms: i64) -> bool {
        now_ms < self.expires_at_ms.saturating_sub(EXPIRY_SKEW_MS)
    }

    /// This entry with the rotation dropped.
    ///
    /// Written back over the fresh entry as soon as the minting caller has the
    /// replacement, so a refresh token does not sit in a process-wide cache for
    /// the life of the access token beside it.
    fn without_rotation(&self) -> Self {
        Self {
            token: self.token.clone(),
            expires_at_ms: self.expires_at_ms,
            rotated_refresh_token: None,
            live_for: self.live_for,
        }
    }
}

/// Expires each entry by its OWN remaining life.
///
/// A single `time_to_live` cannot: a GitHub installation token lasts an hour, a
/// Zoho access token whatever Zoho said, and a `static` handle forever. Left to
/// an LRU alone, a dead token would sit in memory until something else needed
/// the slot.
struct PerToken;

impl Expiry<Key, Cached> for PerToken {
    fn expire_after_create(
        &self,
        _key: &Key,
        value: &Cached,
        _created_at: std::time::Instant,
    ) -> Option<Duration> {
        Some(value.live_for)
    }
}

/// Why a mint produced no credential.
///
/// The non-`Ok` half of [`Outcome`], as its own type because `moka`'s
/// `try_get_with` needs an error to hand every waiter — and because a value
/// that cannot hold a token is what keeps a refusal out of the cache by
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refused {
    /// A human must connect the integration again.
    Reconnect,
    /// The exchange produced nothing.
    Failed(Retry),
    /// The handle names a connector this registry does not carry.
    Unknown,
    /// This deployment holds no platform credential for the connector.
    Unconfigured,
}

impl From<Refused> for Outcome {
    fn from(refused: Refused) -> Self {
        match refused {
            Refused::Reconnect => Self::ReconnectRequired,
            Refused::Failed(retry) => Self::MintFailed(retry),
            Refused::Unknown => Self::UnknownIntegration,
            Refused::Unconfigured => Self::Unconfigured,
        }
    }
}

/// Performs one exchange, whatever the connector's kind.
///
/// The seam the production dispatch and a test's counting stand-in both sit
/// behind. Boxed rather than an `async fn` in the trait because the broker
/// holds it as `dyn` — one allocation per COLD mint, against an HTTPS round
/// trip.
pub trait Exchanger: std::fmt::Debug + Send + Sync {
    /// Mints through `connector`, or says why it could not.
    fn exchange<'a>(
        &'a self,
        connector: &'a dyn Connector,
        ask: Ask<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Outcome> + Send + 'a>>;
}

/// The production dispatch: this deployment's platform credentials, and one
/// HTTP client.
#[derive(Debug)]
pub struct Vendors {
    /// This deployment's own App and OAuth clients.
    platform: Platform,
    /// Shared, so the connection pool and the timeout are one decision.
    http: reqwest::Client,
}

impl Vendors {
    /// The dispatch over `platform`, posting through `http`.
    #[must_use]
    pub const fn new(platform: Platform, http: reqwest::Client) -> Self {
        Self { platform, http }
    }
}

impl Exchanger for Vendors {
    /// One `match`, and it is the only one in the broker.
    ///
    /// Adding a connector that mints the way an existing one does adds nothing
    /// here — that is what [`Exchange`] being data on the descriptor buys, and
    /// what the three refresh providers sharing one arm demonstrates.
    fn exchange<'a>(
        &'a self,
        connector: &'a dyn Connector,
        ask: Ask<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(async move {
            match connector.exchange() {
                Exchange::Stored => stored(ask.handle),
                Exchange::GithubApp => match self.platform.github() {
                    Some(app) => {
                        github::mint(github::Exchange {
                            app,
                            handle: ask.handle,
                            binding: ask.binding,
                            now_ms: ask.now_ms,
                        })
                        .await
                    }
                    // The tenant connected an App this deployment cannot
                    // authenticate as. An operator's to fix, and no retry
                    // helps — and it is not a failed exchange, because no
                    // exchange was attempted.
                    None => Outcome::Unconfigured,
                },
                Exchange::OAuthRefresh { token_url } => {
                    match self.platform.oauth(connector.name()) {
                        Some(app) => {
                            oauth::mint(oauth::Refresh {
                                app,
                                handle: ask.handle,
                                token_url,
                                http: &self.http,
                                now_ms: ask.now_ms,
                            })
                            .await
                        }
                        None => Outcome::Unconfigured,
                    }
                }
            }
        })
    }
}

/// The `static` connector's whole exchange: there isn't one.
///
/// The stored handle already holds a usable credential, so this hands it back
/// with no upstream expiry. It reaches the broker even though the lease path
/// delivers static credentials inline, because the runner may ask for anything
/// its fleet declared and refusing here would be a refusal the tenant cannot
/// act on.
fn stored(handle: &Value) -> Outcome {
    handle
        .get(FIELD_TOKEN)
        .and_then(Value::as_str)
        .map_or(Outcome::ReconnectRequired, |token| {
            Outcome::Ok(Minted {
                token: Zeroizing::new(token.to_owned()),
                // A stored credential has no upstream expiry this daemon knows
                // of. The far-future sentinel is `integration.zig`'s.
                expires_at_ms: i64::MAX,
                rotated_refresh_token: None,
            })
        })
}

/// The on-demand credential broker.
#[derive(Debug)]
pub struct Broker {
    /// Cache and single-flight, in one structure.
    cache: Cache<Key, Cached>,
    /// The connector set this broker resolves handles through. Injected, so a
    /// test's registry and the shipped one are the same kind of thing.
    connectors: Arc<dyn Connectors>,
    /// What actually performs an exchange.
    vendors: Arc<dyn Exchanger>,
}

impl Broker {
    /// A broker over `connectors`, minting through `vendors`.
    #[must_use]
    pub fn new(connectors: Arc<dyn Connectors>, vendors: Arc<dyn Exchanger>) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(MAX_CACHED_TOKENS)
                .expire_after(PerToken)
                .build(),
            connectors,
            vendors,
        }
    }

    /// Resolves one ask to a credential.
    ///
    /// A cache hit answers without an exchange and reports no rotation — it
    /// performed none. A miss mints exactly once per key however many callers
    /// arrive at once, and the ROTATED refresh token, when there is one, is
    /// handed to that one caller alone: it is the caller whose exchange
    /// consumed the old value, and the only one that may write the replacement
    /// back.
    pub async fn mint(&self, ask: Ask<'_>) -> Outcome {
        let Some(connector) = self.resolve(ask.handle) else {
            return Outcome::UnknownIntegration;
        };
        let Some(key) = Key::of(&ask, connector.name()) else {
            // The handle would not serialise, so no entry can stand for it.
            return Outcome::MintFailed(Retry::Permanent);
        };

        if let Some(cached) = self.cache.get(&key).await {
            if cached.is_live(ask.now_ms) {
                return Outcome::Ok(Minted {
                    token: cached.token.clone(),
                    expires_at_ms: cached.expires_at_ms,
                    // A hit performed no exchange, so it rotated nothing.
                    rotated_refresh_token: None,
                });
            }
            // Within the skew of its expiry. Dropped so the next step mints
            // rather than being handed a token that dies mid-call.
            self.cache.invalidate(&key).await;
        }

        let minted = self
            .cache
            .entry_by_ref(&key)
            .or_try_insert_with(async {
                match self.vendors.exchange(connector, ask).await {
                    Outcome::Ok(minted) => Ok(Cached::new(minted, ask.now_ms)),
                    Outcome::ReconnectRequired => Err(Refused::Reconnect),
                    Outcome::MintFailed(retry) => Err(Refused::Failed(retry)),
                    Outcome::UnknownIntegration => Err(Refused::Unknown),
                    Outcome::Unconfigured => Err(Refused::Unconfigured),
                }
            })
            .await;

        match minted {
            Ok(entry) => {
                // `is_fresh` is true for the ONE caller whose future ran, and
                // false for every waiter it served — which is exactly the
                // caller that owes the vault a write-back.
                let fresh = entry.is_fresh();
                let cached = entry.into_value();
                let rotated = if fresh {
                    cached.rotated_refresh_token.clone()
                } else {
                    None
                };
                if rotated.is_some() {
                    // Taken out of the cache now that its one reader has it.
                    self.cache.insert(key, cached.without_rotation()).await;
                }
                Outcome::Ok(Minted {
                    token: cached.token,
                    expires_at_ms: cached.expires_at_ms,
                    rotated_refresh_token: rotated,
                })
            }
            // Handed to every waiter, cached by none.
            Err(refused) => (*refused).into(),
        }
    }

    /// The connector this handle names, if the registry carries it.
    fn resolve(&self, handle: &Value) -> Option<&dyn Connector> {
        let name = handle.get(FIELD_INTEGRATION)?.as_str()?;
        self.connectors.resolve(name)
    }
}

impl Key {
    /// The entry `ask` reads and writes.
    ///
    /// `None` only when the handle cannot be serialised at all, which a value
    /// that came out of `serde_json` cannot be — it is refused rather than
    /// collapsed into a shared key, because every ask that failed to digest
    /// would otherwise share one cache entry.
    fn of(ask: &Ask<'_>, connector: &str) -> Option<Self> {
        let identity = ask
            .handle
            .as_object()
            .map(|handle| {
                handle
                    .iter()
                    .filter(|(field, _)| !ROTATING_FIELDS.contains(&field.as_str()))
                    .map(|(field, value)| (field.as_str(), value))
                    .collect()
            })
            .unwrap_or_default();
        let scope = Scope {
            identity,
            binding: ask.binding.map(RepositoryBinding::recorded),
        };
        let canonical = serde_json::to_vec(&scope).ok()?;
        Some(Self {
            workspace_id: ask.workspace_id.clone(),
            connector: connector.into(),
            scope: Sha256::digest(&canonical).into(),
        })
    }
}

#[cfg(test)]
mod tests;
