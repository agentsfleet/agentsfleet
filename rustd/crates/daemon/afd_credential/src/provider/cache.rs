//! The platform default's resolution, held in daemon memory by the versions
//! that change it (M245 §3, blocker 1's default).
//!
//! # What the key is, and why it is enough
//!
//! A platform resolution is the active default's provider, its catalogued
//! model and endpoint, and the decrypted key of the vault row it names. Each of
//! those moves a version a statement can read:
//!
//! - the catalogue: every model, price and platform-default write advances
//!   `model.catalogue_version` in its own transaction;
//! - the key: `vault.secrets.resource_version` moves on every update, by
//!   trigger, including a replace under the same KEK that keeps the row's id;
//! - a delete and recreate: the new row has a new id.
//!
//! So an entry keyed by the catalogue version and the secret's id and version
//! answers for exactly one resolution, and the claim statement reads all three
//! beside the slot it takes. No lookup by provider or tenant compiles: a
//! [`PlatformKeyKey`] is built only from a statement's row.
//!
//! A price window that closes with no successor tier is the one change no
//! write announces. It still resolves from the cache until a version moves,
//! where the read would refuse; the admin edit that would fix it bumps the
//! catalogue.
//!
//! # The key stays a secret
//!
//! The value is a [`Resolved`], whose key is a `SecretString` over
//! `Zeroizing<String>`: it renders as redacted and is zeroed when the last copy
//! drops, eviction included.

use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::{CacheOutcome, LeaseCache};
use af_observability::producers::fleet::cache::looked_up;
use moka::future::Cache;

use super::Resolved;

/// How many platform resolutions one replica holds.
///
/// A performance parameter: a deployment has one active default, so the cap
/// only bounds the versions a busy catalogue leaves behind.
pub const PLATFORM_KEY_ENTRIES: u64 = 1_024;

/// The catalogue version and the platform secret's id and version, exactly as
/// a statement returned them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlatformKeyKey {
    catalogue_version: i64,
    secret_id: Uuid7,
    secret_version: i64,
}

impl PlatformKeyKey {
    /// The key for the versions the caller's statement returned in this pass.
    ///
    /// Only a statement row's decoder calls this: the lease claim and the
    /// platform resolution's own read.
    #[must_use]
    pub const fn observed(catalogue_version: i64, secret_id: Uuid7, secret_version: i64) -> Self {
        Self {
            catalogue_version,
            secret_id,
            secret_version,
        }
    }
}

/// Platform resolutions by the versions they were read at.
#[derive(Debug, Clone)]
pub struct PlatformKeys {
    entries: Cache<PlatformKeyKey, Resolved>,
}

impl Default for PlatformKeys {
    fn default() -> Self {
        Self {
            entries: Cache::new(PLATFORM_KEY_ENTRIES),
        }
    }
}

impl PlatformKeys {
    /// The resolution `key` holds, counted as a hit or a miss.
    pub async fn get(&self, key: &PlatformKeyKey) -> Option<Resolved> {
        let held = self.entries.get(key).await;
        let outcome = if held.is_some() {
            CacheOutcome::Hit
        } else {
            CacheOutcome::Miss
        };
        looked_up(LeaseCache::PlatformKey, outcome);
        held
    }

    /// Holds `resolved` under the versions its own read returned.
    pub async fn insert(&self, key: PlatformKeyKey, resolved: Resolved) {
        self.entries.insert(key, resolved).await;
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the library"
    )]

    use af_crypto::secret::SecretString;
    use afd_billing::Posture;

    use super::{PlatformKeyKey, PlatformKeys, Resolved};

    /// The platform key a fixture caches; it must never render.
    const KEY: &str = "sk-platform-cached-fixture";

    fn key() -> PlatformKeyKey {
        PlatformKeyKey::observed(
            7,
            af_core::id::Uuid7::parse("01900000-0000-7000-8000-000000000001")
                .expect("a fixture identifier"),
            2,
        )
    }

    fn resolved() -> Resolved {
        Resolved::new(
            Posture::Platform,
            "anthropic".into(),
            "claude-fixture".into(),
            200_000,
            None,
            SecretString::new(KEY.to_owned()),
        )
    }

    /// A cached resolution, and the cache holding it, render no byte of the key.
    #[tokio::test]
    async fn cached_platform_key_never_renders() {
        let cache = PlatformKeys::default();
        cache.insert(key(), resolved()).await;
        let held = cache.get(&key()).await.expect("the entry is held");
        assert_eq!(held.api_key().expose(), KEY, "the entry holds the key");
        for rendered in [format!("{held:?}"), format!("{cache:?}")] {
            assert!(!rendered.contains(KEY), "{rendered}");
            assert!(!rendered.contains("sk-platform"), "{rendered}");
        }
    }

    /// Another version of the same secret misses: the entry answers for one.
    #[tokio::test]
    async fn another_secret_version_misses() {
        let cache = PlatformKeys::default();
        cache.insert(key(), resolved()).await;
        let moved = PlatformKeyKey::observed(
            7,
            af_core::id::Uuid7::parse("01900000-0000-7000-8000-000000000001")
                .expect("a fixture identifier"),
            3,
        );
        assert!(cache.get(&moved).await.is_none());
    }
}
