//! A minted token as the broker caches it, how long it stays, and why a mint
//! produced none.
//!
//! Split from the broker by concern: these decide what an entry holds and when
//! it expires, and none of them resolves a connector or performs an exchange.

use std::time::Duration;

use afd_core::clock::before;
use jiff::Timestamp;
use moka::Expiry;
use zeroize::Zeroizing;

use super::Key;
use crate::credential::outcome::{Minted, Outcome, Retry};

/// Re-mint this long BEFORE the upstream expiry.
///
/// A token handed to a tool call has to outlive the call, and the daemon cannot
/// know how long that is. Erring early is the only safe direction: too much
/// slack costs a re-mint, too little costs a 401 mid-run.
const EXPIRY_SKEW: Duration = Duration::from_mins(1);

/// A token this process is holding on to.
#[derive(Clone)]
pub(super) struct Cached {
    /// The credential.
    pub(super) token: Zeroizing<String>,
    /// When it stops working upstream.
    pub(super) expires_at: Timestamp,
    /// A refresh token the exchange rotated, held only until the caller that
    /// performed that exchange has taken it — see [`Broker::mint`].
    pub(super) rotated_refresh_token: Option<Zeroizing<String>>,
    /// How long this entry may stay cached, skew already subtracted.
    pub(super) live_for: Duration,
}

// Hand-written for [`Minted`]'s reason: a derived one prints the token, and
// `Debug` is what a `tracing` field renders through.
impl std::fmt::Debug for Cached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cached")
            .field("expires_at", &self.expires_at)
            .field("live_for", &self.live_for)
            .finish_non_exhaustive()
    }
}

impl Cached {
    /// The cache entry for a freshly minted credential.
    pub(super) fn new(minted: Minted, now: Timestamp) -> Self {
        // Clamped at zero by the conversion: a provider that returned a token
        // expiring inside the skew is cached for no time at all rather than for
        // a negative one. The CALLER still receives it — what it does not get
        // is a second caller a minute later.
        let live_for = Duration::try_from(now.duration_until(stale_at(minted.expires_at)))
            .unwrap_or(Duration::ZERO);
        Self {
            token: minted.token,
            expires_at: minted.expires_at,
            rotated_refresh_token: minted.rotated_refresh_token,
            live_for,
        }
    }

    /// Whether this entry is still worth handing over at `now`.
    ///
    /// Checked against the INJECTED clock rather than left to `moka`'s own
    /// expiry alone, which is what keeps the decision provable without waiting
    /// for wall-clock time to pass. The two agree; this one is the earlier of
    /// them and the one a test can drive.
    pub(super) fn is_live(&self, now: Timestamp) -> bool {
        now < stale_at(self.expires_at)
    }

    /// This entry with the rotation dropped.
    ///
    /// Written back over the fresh entry as soon as the minting caller has the
    /// replacement, so a refresh token does not sit in a process-wide cache for
    /// the life of the access token beside it.
    pub(super) fn without_rotation(&self) -> Self {
        Self {
            token: self.token.clone(),
            expires_at: self.expires_at,
            rotated_refresh_token: None,
            live_for: self.live_for,
        }
    }
}

/// The instant a token expiring at `expires_at` stops being handed over.
fn stale_at(expires_at: Timestamp) -> Timestamp {
    before(expires_at, EXPIRY_SKEW)
}

/// Expires each entry by its OWN remaining life.
///
/// A single `time_to_live` cannot: a GitHub installation token lasts an hour, a
/// Zoho access token whatever Zoho said, and a `static` handle forever. Left to
/// an LRU alone, a dead token would sit in memory until something else needed
/// the slot.
pub(super) struct PerToken;

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
pub(super) enum Refused {
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

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use zeroize::Zeroizing;

    use super::Cached;
    use crate::credential::outcome::Minted;

    /// The token a fixture mints; it must never reach a log line.
    const TOKEN: &str = "ghs_cached_fixture_token";

    /// A cached entry renders its expiry and nothing a reader could use.
    #[test]
    fn a_cached_entry_renders_without_its_token() {
        let cached = Cached::new(
            Minted {
                token: Zeroizing::new(TOKEN.to_owned()),
                expires_at: Timestamp::constant(180, 0),
                rotated_refresh_token: Some(Zeroizing::new(TOKEN.to_owned())),
            },
            Timestamp::UNIX_EPOCH,
        );
        let rendered = format!("{cached:?}");
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(rendered.contains("expires_at"), "{rendered}");
        assert!(
            rendered.contains("120s"),
            "cached for the life minus the skew: {rendered}"
        );
    }
}
