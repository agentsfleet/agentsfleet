//! Refresh races, repeated outages, and revocation through the public resolver.

use afd_auth::capability::CapabilitySource;
use afd_auth::error::Unavailable;
use afd_auth::scope::{ScopeSet, parse_claim};
use afd_identity::capability::{DEFAULT_STALE_CEILING_MS, DEFAULT_TTL_MS};
use afd_identity::error::ClaimUnavailable;

use crate::support::capability::{CLAIM, resolver, subject, warm, wave};

#[tokio::test]
async fn concurrent_cold_requests_share_one_provider_answer() {
    let (resolver, _) = resolver();
    for answer in wave(&resolver, false, 1).await {
        assert_eq!(answer, Ok(parse_claim(CLAIM)));
    }
}

#[tokio::test]
async fn distinct_subjects_can_fetch_while_other_subjects_are_blocked() {
    let (resolver, _) = resolver();
    for answer in wave(&resolver, true, 100).await {
        assert_eq!(answer, Ok(parse_claim(CLAIM)));
    }
}

#[tokio::test]
async fn concurrent_refreshes_all_receive_the_narrowed_claim() {
    let (resolver, clock) = resolver();
    warm(&resolver).await;
    resolver.source().answer(Ok("fleet:read".to_owned()));
    clock.advance_millis(DEFAULT_TTL_MS + 1);
    for answer in wave(&resolver, false, 2).await {
        assert_eq!(answer, Ok(parse_claim("fleet:read")));
    }
}

#[tokio::test]
async fn concurrent_failed_refreshes_all_retain_the_stale_claim() {
    let (resolver, clock) = resolver();
    warm(&resolver).await;
    resolver.source().answer(Err(ClaimUnavailable::Unreachable));
    clock.advance_millis(DEFAULT_TTL_MS + 1);
    for answer in wave(&resolver, false, 2).await {
        assert_eq!(answer, Ok(parse_claim(CLAIM)));
    }
    for answer in wave(&resolver, false, 3).await {
        assert_eq!(answer, Ok(parse_claim(CLAIM)));
    }
}

#[tokio::test]
async fn failed_refreshes_never_extend_the_stale_ceiling() {
    let (resolver, clock) = resolver();
    warm(&resolver).await;
    resolver.source().answer(Err(ClaimUnavailable::Unreachable));
    clock.advance_millis(DEFAULT_TTL_MS + 1);
    resolver.source().release.add_permits(2);
    assert_eq!(
        resolver.capabilities(&subject()).await,
        Ok(parse_claim(CLAIM))
    );
    clock.advance_millis(DEFAULT_STALE_CEILING_MS);
    assert_eq!(resolver.capabilities(&subject()).await, Err(Unavailable));
}

#[tokio::test]
async fn a_deleted_subject_cannot_recover_old_scopes_during_an_outage() {
    let (resolver, clock) = resolver();
    warm(&resolver).await;
    clock.advance_millis(DEFAULT_TTL_MS + 1);
    resolver.source().release.add_permits(2);
    resolver
        .source()
        .answer(Err(ClaimUnavailable::UnknownSubject));
    assert_eq!(resolver.capabilities(&subject()).await, Ok(ScopeSet::EMPTY));
    resolver.source().answer(Err(ClaimUnavailable::Unreachable));
    assert_eq!(resolver.capabilities(&subject()).await, Err(Unavailable));
}

#[tokio::test]
async fn a_cancelled_refresh_preserves_the_stale_claim() {
    let (resolver, clock) = resolver();
    warm(&resolver).await;
    clock.advance_millis(DEFAULT_TTL_MS + 1);
    let who = subject();
    let mut request = Box::pin(resolver.capabilities(&who));
    std::future::poll_fn(|context| {
        assert!(request.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(request);
    resolver.source().answer(Err(ClaimUnavailable::Unreachable));
    resolver.source().release.add_permits(1);
    assert_eq!(resolver.capabilities(&who).await, Ok(parse_claim(CLAIM)));
}
