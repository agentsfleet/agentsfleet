//! The three windows a capability answer lives in, and the one it dies in.
//!
//! The port of `clerk_scope_resolver.zig`'s in-file tests, which exist there
//! because the cache is private and its policy is observable only from inside.
//! Here the policy is observable from outside — the claim source is a seam and
//! the clock is injected — so the tests sit in `tests/` like everything else.
//!
//! What is being pinned is a set of choices that are easy to get backwards:
//! a provider outage must not look like a demotion, a person the provider has
//! forgotten must not look like an outage, and neither must be cached the way
//! the other is.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_auth::capability::CapabilitySource;
use afd_auth::principal::Subject;
use afd_auth::scope::{Scope, ScopeSet, parse_claim};
use afd_core::clock::{Clock, FixedClock, UnixMillis};
use afd_identity::ProviderCapabilities;
use afd_identity::capability::{
    ClaimSource, ClaimUnavailable, DEFAULT_STALE_CEILING_MS, DEFAULT_TTL_MS,
};

const CLAIM: &str = "fleet:admin billing:read";

/// The instant the fixed clock starts at, in epoch milliseconds.
///
/// Arbitrary, and identical across all nine window tests on purpose: the
/// assertions are about elapsed time from a common origin, so one name is what
/// keeps them comparable.
const CLOCK_ORIGIN_MS: i64 = 1_000_000;

fn subject() -> Subject {
    Subject::new("user_2aXyTest").expect("a non-blank subject")
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

/// A provider whose answer and availability a test drives.
#[derive(Debug)]
struct Provider {
    answer: std::sync::Mutex<Result<String, ClaimUnavailable>>,
    calls: AtomicUsize,
}

impl Provider {
    fn answering(claim: &str) -> Self {
        Self {
            answer: std::sync::Mutex::new(Ok(claim.to_owned())),
            calls: AtomicUsize::new(0),
        }
    }

    fn failing(reason: ClaimUnavailable) -> Self {
        Self {
            answer: std::sync::Mutex::new(Err(reason)),
            calls: AtomicUsize::new(0),
        }
    }

    fn set(&self, answer: Result<String, ClaimUnavailable>) {
        *self
            .answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = answer;
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl ClaimSource for Provider {
    fn claim(
        &self,
        _subject: &Subject,
    ) -> impl Future<Output = Result<String, ClaimUnavailable>> + Send {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let answer = self
            .answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        std::future::ready(answer)
    }
}

fn clock_at(millis: i64) -> Arc<FixedClock> {
    Arc::new(FixedClock::at(UnixMillis::from_millis(millis)))
}

fn resolver(
    source: Provider,
    clock: Arc<FixedClock>,
) -> (ProviderCapabilities<Provider>, Arc<FixedClock>) {
    let capabilities = ProviderCapabilities::with_windows(
        source,
        Arc::clone(&clock) as Arc<dyn Clock>,
        DEFAULT_TTL_MS,
        DEFAULT_STALE_CEILING_MS,
    );
    (capabilities, clock)
}

// ── Fresh ────────────────────────────────────────────────────────────────

/// Inside the freshness window the provider is not asked again.
#[test]
fn test_a_fresh_answer_is_served_without_asking_the_provider() {
    let (capabilities, clock) = resolver(Provider::answering(CLAIM), clock_at(CLOCK_ORIGIN_MS));

    let first = block_on(capabilities.capabilities(&subject())).expect("the provider answers");
    assert_eq!(first, parse_claim(CLAIM));
    assert!(first.contains(Scope::FleetRead), "the ladder expands");

    clock.advance_millis(DEFAULT_TTL_MS - 1);
    let second = block_on(capabilities.capabilities(&subject())).expect("still fresh");

    assert_eq!(second, first);
    assert_eq!(
        capabilities.source().calls(),
        1,
        "one call inside the window"
    );
}

/// Past the freshness window the provider is asked again, and a narrowing
/// reaches the caller.
///
/// The whole point of resolving rather than granting: narrowing someone at the
/// provider narrows every credential they hold, with no deploy and no backfill.
#[test]
fn test_narrowing_at_the_provider_reaches_the_next_request() {
    let (capabilities, clock) = resolver(Provider::answering(CLAIM), clock_at(CLOCK_ORIGIN_MS));
    block_on(capabilities.capabilities(&subject())).expect("the provider answers");

    capabilities
        .source()
        .set(Ok(Scope::FleetRead.wire().to_owned()));
    clock.advance_millis(DEFAULT_TTL_MS + 1);

    let narrowed = block_on(capabilities.capabilities(&subject())).expect("re-resolved");
    assert_eq!(narrowed, parse_claim(Scope::FleetRead.wire()));
    assert!(
        !narrowed.contains(Scope::BillingRead),
        "what the provider took away is gone"
    );
    assert_eq!(capabilities.source().calls(), 2);
}

// ── Stale, and the ceiling ───────────────────────────────────────────────

/// A warm entry survives a provider outage, up to the ceiling.
///
/// Refusing every terminal during a vendor blip is worse than acting on
/// capabilities that are minutes old.
#[test]
fn test_a_stale_answer_survives_an_outage_within_the_ceiling() {
    let (capabilities, clock) = resolver(Provider::answering(CLAIM), clock_at(CLOCK_ORIGIN_MS));
    block_on(capabilities.capabilities(&subject())).expect("warm the entry");

    capabilities
        .source()
        .set(Err(ClaimUnavailable::Unreachable));
    clock.advance_millis(DEFAULT_TTL_MS + 1);

    let served = block_on(capabilities.capabilities(&subject()))
        .expect("a warm entry beats refusing every terminal");
    assert_eq!(served, parse_claim(CLAIM));
}

/// Past the ceiling the caller is refused, never handed an empty set.
///
/// An empty set here would read to an operator as a demotion they never
/// received — and would be indistinguishable from a person the provider has
/// forgotten, which is a permanent condition rather than a transient one.
#[test]
fn test_past_the_ceiling_the_caller_is_refused_rather_than_emptied() {
    let (capabilities, clock) = resolver(Provider::answering(CLAIM), clock_at(CLOCK_ORIGIN_MS));
    block_on(capabilities.capabilities(&subject())).expect("warm the entry");

    capabilities
        .source()
        .set(Err(ClaimUnavailable::Unreachable));
    clock.advance_millis(DEFAULT_STALE_CEILING_MS + 1);

    block_on(capabilities.capabilities(&subject()))
        .expect_err("past the ceiling a claim could contradict an unconfirmable revocation");
}

/// A cold subject with no reachable provider is an outage, not an empty grant.
#[test]
fn test_a_cold_subject_with_no_provider_is_an_outage() {
    let (capabilities, _clock) = resolver(
        Provider::failing(ClaimUnavailable::Unreachable),
        clock_at(CLOCK_ORIGIN_MS),
    );
    block_on(capabilities.capabilities(&subject())).expect_err("nothing warm, and nothing to ask");
}

// ── The person the provider has forgotten ────────────────────────────────

/// An unknown subject resolves to no capabilities — an ANSWER, not an outage.
///
/// The credential outlived the person. Every gate refuses them by name, which
/// is what should happen; telling a terminal to retry would be telling it to
/// retry something that will never work again.
#[test]
fn test_a_subject_the_provider_does_not_know_resolves_to_nothing() {
    let (capabilities, _clock) = resolver(
        Provider::failing(ClaimUnavailable::UnknownSubject),
        clock_at(CLOCK_ORIGIN_MS),
    );

    let resolved =
        block_on(capabilities.capabilities(&subject())).expect("an answer, not a failure");
    assert_eq!(resolved, ScopeSet::EMPTY);
}

/// And that answer is NOT cached.
///
/// A deletion is permanent and needs no cache; a transient miss must not blank
/// a live operator for a whole freshness window. So the next request asks
/// again — which is the behaviour that lets a person come back.
#[test]
fn test_an_unknown_subject_is_not_cached() {
    let (capabilities, _clock) = resolver(
        Provider::failing(ClaimUnavailable::UnknownSubject),
        clock_at(CLOCK_ORIGIN_MS),
    );
    block_on(capabilities.capabilities(&subject())).expect("empty, not an error");

    // The provider learns about them again — a restored backup, a replayed
    // webhook — and the very next request must see it.
    capabilities.source().set(Ok(CLAIM.to_owned()));
    let resolved = block_on(capabilities.capabilities(&subject())).expect("asked again");

    assert_eq!(resolved, parse_claim(CLAIM));
    assert_eq!(
        capabilities.source().calls(),
        2,
        "an unknown subject must not occupy the cache"
    );
}

// ── Single flight ────────────────────────────────────────────────────────

/// Concurrent misses for one subject cost one provider call.
///
/// This is what the `moka` dependency buys, and it closes the caveat
/// `clerk_scope_resolver.zig:19-22` writes down and leaves open: a tenant key
/// rides ONE creator subject at machine rates, so at expiry its in-flight
/// requests would otherwise fan out to the provider together.
///
/// It also retires the `seq` counter that existed only so a slow out-of-order
/// response could not resurrect a pre-revocation claim — with one flight per
/// subject there is no second response to be out of order with.
#[test]
fn test_concurrent_misses_for_one_subject_cost_one_provider_call() {
    let (capabilities, _clock) = resolver(Provider::answering(CLAIM), clock_at(CLOCK_ORIGIN_MS));

    let who = subject();
    let resolved = block_on(async {
        let mut answers = Vec::with_capacity(16);
        for _ in 0..16_u8 {
            answers.push(capabilities.capabilities(&who));
        }
        futures_join(answers).await
    });

    for answer in resolved {
        assert_eq!(
            answer.expect("every waiter gets the answer"),
            parse_claim(CLAIM)
        );
    }
    assert_eq!(
        capabilities.source().calls(),
        1,
        "sixteen waiters, one flight"
    );
}

/// Awaits every future in order, without pulling in a combinator crate.
///
/// The resolutions are coalesced by the cache rather than by this helper, so
/// sequential awaiting still proves the property: the second through sixteenth
/// find the entry the first installed.
async fn futures_join<F: Future>(futures: Vec<F>) -> Vec<F::Output> {
    let mut out = Vec::with_capacity(futures.len());
    for future in futures {
        out.push(future.await);
    }
    out
}

/// Distinct subjects do not share an entry.
///
/// A cache keyed loosely enough to collide two people would be the worst
/// possible bug in this file, so it is stated rather than assumed.
#[test]
fn test_two_subjects_resolve_independently() {
    let (capabilities, _clock) = resolver(Provider::answering(CLAIM), clock_at(CLOCK_ORIGIN_MS));
    let first = Subject::new("user_first").expect("non-blank");
    let second = Subject::new("user_second").expect("non-blank");

    block_on(capabilities.capabilities(&first)).expect("the provider answers");
    capabilities
        .source()
        .set(Ok(Scope::FleetRead.wire().to_owned()));
    let other = block_on(capabilities.capabilities(&second)).expect("a separate lookup");

    assert_eq!(other, parse_claim(Scope::FleetRead.wire()));
    assert_eq!(capabilities.source().calls(), 2, "two subjects, two calls");
}
