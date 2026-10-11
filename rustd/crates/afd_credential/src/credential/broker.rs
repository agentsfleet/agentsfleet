//! The cache in front of the mints, and the single-flight in front of the
//! cache.
//!
//! # `moka` is both, in one call
//!
//! [`moka::future::Cache`]'s entry API caches and single-flights at once:
//! exactly one caller per key resolves the init future and the rest park on
//! it. That is not a convenience. Two cold misses on a ROTATING refresh
//! provider both post the same refresh token, and a provider with reuse
//! detection revokes the whole token family — the tenant's connection dies for
//! everyone. `or_try_insert_with` is the guarantee that case needs, without a
//! hand-rolled in-flight set's poll loop, timeout, or residual-key cleanup a
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
//! The digest is SHA-256 over the canonical JSON of those two things. A key
//! that joined repository names on a separator would make `["acme/a","acme/b"]`
//! and the single spliced name `"acme/a<SEP>acme/b"` hash identically — a
//! deterministic alias, needing no collision, that serves one fleet's
//! broad-scope token to another. JSON is self-delimiting, so no framing rule
//! has to be got right; SHA-256 needs no per-process seed to keep a digest from
//! being precomputed.

use std::collections::BTreeMap;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_fleet_runtime::config::{Recorded, RepositoryBinding};
use moka::future::Cache;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use self::cache::{Cached, PerToken, Refused};
pub use self::vendors::{Exchanger, Vendors};
use crate::credential::outcome::{Minted, Outcome, Retry};
use crate::secrets::connector::{Connector, Connectors, FIELD_INTEGRATION};

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
    afd_core::credential::FIELD_REFRESH_TOKEN,
    "access_token",
    FIELD_EXPIRES_AT_MS,
];

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

    /// The connector set this broker resolves every handle through.
    ///
    /// A check that must agree with the broker about which connector a handle
    /// names reads this one, not a registry of its own: two sets could differ,
    /// and a connector only one of them carries would mint unchecked.
    #[must_use]
    pub fn connectors(&self) -> &dyn Connectors {
        &*self.connectors
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

mod cache;
mod vendors;

#[cfg(test)]
mod tests;
