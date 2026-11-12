#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the library"
)]

use std::sync::Arc;
use std::time::Duration;

use af_core::clock::FixedClock;
use af_crypto::secret::SecretString;
use afd_billing::Posture;
use jiff::Timestamp;
use moka::Expiry;

use super::{Held, Observed, PLATFORM_KEY_TTL, PerEntry, PlatformKeyKey, PlatformKeys, Resolved};

/// The platform key a fixture caches; it must never render.
const KEY: &str = "sk-platform-cached-fixture";

/// When the fixture's read began.
const STARTED: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The digest a fixture's statement returned.
const DIGEST: &str = "5e3f1a";

fn key_with(digest: &str) -> PlatformKeyKey {
    PlatformKeyKey::observed(
        7,
        af_core::id::Uuid7::parse("01900000-0000-7000-8000-000000000001")
            .expect("a fixture identifier"),
        digest,
    )
}

fn key() -> PlatformKeyKey {
    key_with(DIGEST)
}

fn observed(window: Option<Duration>) -> Observed {
    Observed { key: key(), window }
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

fn cache_at(clock: &FixedClock) -> PlatformKeys {
    PlatformKeys::new(Arc::new(clock.clone()))
}

/// A cached resolution, and the cache holding it, render no byte of the key.
#[tokio::test]
async fn cached_platform_key_never_renders() {
    let clock = FixedClock::at(STARTED);
    let cache = cache_at(&clock);
    cache.insert(observed(None), STARTED, resolved()).await;
    let held = cache.get(&key()).await.expect("the entry is held");
    assert_eq!(held.api_key().expose(), KEY, "the entry holds the key");
    for rendered in [format!("{held:?}"), format!("{cache:?}")] {
        assert!(!rendered.contains(KEY), "{rendered}");
        assert!(!rendered.contains("sk-platform"), "{rendered}");
    }
}

/// Another digest of the same secret misses: the entry answers for one.
#[tokio::test]
async fn another_secret_digest_misses() {
    let clock = FixedClock::at(STARTED);
    let cache = cache_at(&clock);
    cache.insert(observed(None), STARTED, resolved()).await;
    assert!(cache.get(&key_with("a-replaced-key")).await.is_none());
}

/// A price window that closes with no successor stops the entry answering
/// at that instant, though no version moved.
#[tokio::test]
async fn an_entry_ends_with_its_price_window() {
    let window = Duration::from_secs(90);
    let clock = FixedClock::at(STARTED);
    let cache = cache_at(&clock);
    cache
        .insert(observed(Some(window)), STARTED, resolved())
        .await;

    clock.advance(window.saturating_sub(Duration::from_millis(1)));
    assert!(
        cache.get(&key()).await.is_some(),
        "a millisecond before the window closes it still answers"
    );
    clock.advance(Duration::from_millis(1));
    assert!(
        cache.get(&key()).await.is_none(),
        "at the instant the window closes it does not"
    );
}

/// The window was measured after the read began, so the entry's end is
/// counted from when the read began, never from when it was stored.
#[tokio::test]
async fn the_span_is_counted_from_when_the_read_began() {
    let window = Duration::from_secs(90);
    let read_took = Duration::from_secs(5);
    let clock = FixedClock::at(STARTED);
    let cache = cache_at(&clock);
    clock.advance(read_took);
    cache
        .insert(observed(Some(window)), STARTED, resolved())
        .await;

    clock.advance(
        window
            .saturating_sub(read_took)
            .saturating_sub(Duration::from_millis(1)),
    );
    assert!(cache.get(&key()).await.is_some());
    clock.advance(Duration::from_millis(1));
    assert!(
        cache.get(&key()).await.is_none(),
        "the window ends {window:?} after the read began"
    );
}

/// An open-ended window is bounded by the time limit alone, so a key an
/// operator replaced does not stay decrypted past it.
#[tokio::test]
async fn an_open_window_is_bounded_by_the_time_limit() {
    let clock = FixedClock::at(STARTED);
    let cache = cache_at(&clock);
    cache.insert(observed(None), STARTED, resolved()).await;

    clock.advance(PLATFORM_KEY_TTL.saturating_sub(Duration::from_millis(1)));
    assert!(cache.get(&key()).await.is_some());
    clock.advance(Duration::from_millis(1));
    assert!(cache.get(&key()).await.is_none());
}

/// A window longer than the time limit does not extend it.
#[tokio::test]
async fn a_long_window_does_not_outlive_the_time_limit() {
    let clock = FixedClock::at(STARTED);
    let cache = cache_at(&clock);
    cache
        .insert(observed(Some(PLATFORM_KEY_TTL * 12)), STARTED, resolved())
        .await;

    clock.advance(PLATFORM_KEY_TTL);
    assert!(cache.get(&key()).await.is_none());
}

/// The cache is told each entry's own span, so an entry nobody asks for
/// again (a replaced key) is still dropped when the span ends.
#[test]
fn the_cache_is_told_each_entrys_own_span() {
    let span = Duration::from_secs(9);
    let held = Held {
        resolved: resolved(),
        open_until: STARTED,
        lifetime: span,
    };
    assert_eq!(
        PerEntry.expire_after_create(&key(), &held, std::time::Instant::now()),
        Some(span)
    );
}

/// An entry past its span leaves the cache on a sweep with no lookup, which is
/// what frees a replaced key's plaintext on a replica that stopped asking for it.
///
/// Real time, because the cache's own expiry runs on its monotonic clock; the
/// span is short and the wait is the cache's timer-wheel granularity.
#[tokio::test]
async fn a_sweep_frees_an_entry_past_its_span_without_a_lookup() {
    let cache = PlatformKeys::default();
    let span = Duration::from_millis(20);
    cache
        .insert(observed(Some(span)), cache.now(), resolved())
        .await;
    cache.entries.run_pending_tasks().await;
    assert_eq!(cache.entries.entry_count(), 1, "the entry is held at first");

    tokio::time::sleep(Duration::from_millis(1_600)).await;
    cache.sweep().await;

    assert_eq!(
        cache.entries.entry_count(),
        0,
        "a sweep frees the entry once its span has passed, with no lookup"
    );
}
