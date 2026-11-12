//! What ends a cached platform resolution besides a moved witness: the price
//! window it matched, and the time limit no entry outlives.

use std::time::Duration;

use afd_credential::provider::PLATFORM_MODEL_REF;
use afd_credential::provider::cache::PLATFORM_KEY_TTL;
use afd_db::test_util::statements::Window;

use super::{Lane, secret_reads};

// Value: protects=a price window that closes with no successor ends the cached
// resolution; fails_when=the window's remaining span is not read or not applied;
// why_new=no write announces a window closing; seam=the cache's clock;
// no_claim=a successor tier (its write moves the catalogue).
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_key_cache_ends_with_its_price_window() {
    /// Shorter than the time limit, so only the window can end the entry.
    const WINDOW: Duration = Duration::from_secs(240);
    let lane = Lane::create().await;
    lane.key("sk-key-a", false).await;
    lane.activate_default().await;

    let open = lane.observed().await;
    assert_eq!(
        open.window, None,
        "an open-ended window has no end to count"
    );

    lane.close_price_window_in(WINDOW).await;
    let left = lane
        .observed()
        .await
        .window
        .expect("a closing window is observed");
    assert!(
        left > WINDOW / 2 && left <= WINDOW,
        "the window has most of its span left, measured on the database's clock: {left:?}"
    );

    let window = Window::open(&lane.owner).await;
    assert_eq!(lane.lease(PLATFORM_MODEL_REF).await, "sk-key-a");
    lane.clock.advance(Duration::from_secs(60));
    assert_eq!(
        lane.lease(PLATFORM_MODEL_REF).await,
        "sk-key-a",
        "inside the window the held entry answers"
    );
    lane.clock.advance(Duration::from_secs(190));
    assert_eq!(
        lane.lease(PLATFORM_MODEL_REF).await,
        "sk-key-a",
        "past the window, though inside the time limit, the entry is read again"
    );
    let ran = window.close().await;

    assert_eq!(
        secret_reads(&ran),
        2,
        "read for lease one and again once the window had closed: {:?}",
        ran.statements()
    );
    lane.cleanup().await;
}

// Value: protects=a replaced key does not stay decrypted past the time limit;
// fails_when=an entry outlives PLATFORM_KEY_TTL; why_new=the entry cap alone
// leaves a rotated key's plaintext in memory indefinitely; seam=the cache's
// clock; no_claim=eviction timing inside moka.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_key_cache_ends_at_the_time_limit() {
    let lane = Lane::create().await;
    lane.key("sk-key-a", false).await;
    lane.activate_default().await;

    let window = Window::open(&lane.owner).await;
    assert_eq!(lane.lease(PLATFORM_MODEL_REF).await, "sk-key-a");
    lane.clock
        .advance(PLATFORM_KEY_TTL.saturating_sub(Duration::from_millis(1)));
    assert_eq!(lane.lease(PLATFORM_MODEL_REF).await, "sk-key-a");
    lane.clock.advance(Duration::from_millis(1));
    assert_eq!(lane.lease(PLATFORM_MODEL_REF).await, "sk-key-a");
    let ran = window.close().await;

    assert_eq!(
        secret_reads(&ran),
        2,
        "read for lease one and again at the limit: {:?}",
        ran.statements()
    );
    lane.cleanup().await;
}
