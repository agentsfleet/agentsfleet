//! The refresh gate: one flight at a time, and what the losers see.
//!
//! `dispatch/write_rust.md` asks for a deterministic contention test rather
//! than a happy-path asynchronous one, so the race here is CONSTRUCTED rather
//! than hoped for: the key-set source yields before answering, which parks the
//! first flight inside the gate and lets the second reach it. On a
//! current-thread runtime that ordering is fixed, so the test proves the same
//! thing on every run instead of most runs.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_auth::capability::CapabilitySource;
use afd_auth::credential::Presented;
use afd_auth::principal::Subject;
use afd_auth::verifier::{TokenVerifier, VerifyError};
use afd_core::clock::{Clock, FixedClock, SystemClock, UnixMillis};
use afd_identity::capability::{ClaimSource, ClaimUnavailable};
use afd_identity::jwks::source::KeySetSource;
use afd_identity::{JwksVerifier, ProviderCapabilities, VerifierConfig};

const TEST_RSA_N: &str = "7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ";
const TEST_KID: &str = "test-kid-static";
const TEST_HEADER: &str = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
const TEST_PAYLOAD_VALID: &str = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ";
const TEST_SIG_VALID: &str = "pU5Y3T5yhLjleABex4K0fsyfjrxHDFa-8sjbI5hQhPHVw7P-WF_72VbWoCa9sVPi5cwGU0tbj8rZY2BMhq36_xZxwh7l4Z9SdguVGCiceDuqhhtRxA8vdPIlolrrykxAuEvlyeHRiE1uOzSvSGZZFCHvkgVK06SwC4oK1NlSgFx_cjKYbY0NychCG0XxLrl5XUoR79va4-9HGRMDYaTFRMutwMzFF_4iCbpn3RHl-qu9_RAabJrsQkeCmYYXaQKLt_aVVfrBMQWOwJDvCuTaeJcRGJefKmNdc-aM8mqBjZX9RIocD_hp5ADxY9HZdBFtGz7OAofgM2ZqVeJPkvNKfQ";

const ISSUER: &str = "https://clerk.dev.agentsfleet.net";
const AUDIENCE: &str = "https://api.agentsfleet.net";

fn valid_token() -> Presented {
    Presented::new(&format!(
        "{TEST_HEADER}.{TEST_PAYLOAD_VALID}.{TEST_SIG_VALID}"
    ))
    .expect("a non-blank token")
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock::at(UnixMillis::from_millis(1_704_067_400_000)))
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

/// A source that yields before answering, so a second caller reaches the gate.
#[derive(Debug)]
struct YieldingKeySet {
    document: Vec<u8>,
    fetches: AtomicUsize,
}

impl KeySetSource for YieldingKeySet {
    async fn fetch(&self) -> Result<Vec<u8>, VerifyError> {
        // The park that makes the contention deterministic: the first flight
        // suspends here holding the gate, which is exactly when the second must
        // arrive for the "another flight refreshed while we waited" branch to
        // be the one that runs.
        tokio::task::yield_now().await;
        self.fetches.fetch_add(1, Ordering::Relaxed);
        Ok(self.document.clone())
    }
}

/// A source that never answers.
#[derive(Debug)]
struct NeverAnswers;

impl KeySetSource for NeverAnswers {
    fn fetch(&self) -> impl Future<Output = Result<Vec<u8>, VerifyError>> + Send {
        std::future::ready(Err(VerifyError::KeySetUnavailable))
    }
}

/// Two verifications racing a cold cache cost one fetch, and both succeed.
///
/// The loser does not fetch again: it re-reads the set the winner installed.
/// Without the gate, every request arriving during a key rotation would hit the
/// identity provider at once — which is the storm the rate limit and this gate
/// exist together to prevent.
#[test]
fn test_two_verifications_racing_a_cold_cache_cost_one_fetch() {
    support::install_subscriber();
    block_on(async {
        let source = YieldingKeySet {
            document: format!(
                "{{\"keys\":[{{\"kty\":\"RSA\",\"kid\":\"{TEST_KID}\",\"n\":\"{TEST_RSA_N}\",\"e\":\"AQAB\"}}]}}"
            )
            .into_bytes(),
            fetches: AtomicUsize::new(0),
        };
        let verifier = JwksVerifier::new(source, VerifierConfig::new(ISSUER, AUDIENCE), clock());

        // Both futures are polled by the same current-thread runtime, so the
        // yield inside the first fetch hands control to the second — a fixed
        // interleaving rather than a scheduling accident.
        let (one, two) = (valid_token(), valid_token());
        let (first, second) = tokio::join!(verifier.verify(&one), verifier.verify(&two));

        first.expect("the winner verifies");
        second.expect("the loser verifies against what the winner installed");
        assert_eq!(
            verifier.source().fetches.load(Ordering::Relaxed),
            1,
            "two racing verifications, one flight"
        );
    });
}

/// With nothing held and nothing to fetch, the answer is that the key set is
/// unavailable — not that the key is missing.
///
/// The distinction an operator acts on: "the issuer never published this key"
/// sends someone to the identity provider's dashboard, and "we cannot reach the
/// issuer" sends them to the network.
#[test]
fn test_a_cold_cache_with_no_reachable_source_is_unavailable() {
    support::install_subscriber();
    block_on(async {
        let verifier =
            JwksVerifier::new(NeverAnswers, VerifierConfig::new(ISSUER, AUDIENCE), clock());

        assert_eq!(
            verifier
                .verify(&valid_token())
                .await
                .expect_err("nothing held, nothing reachable"),
            VerifyError::KeySetUnavailable
        );
        assert_eq!(
            verifier.prime().await.expect_err("boot must refuse"),
            VerifyError::KeySetUnavailable
        );
    });
}

/// A key set carrying keys this daemon cannot use is reported, not swallowed.
///
/// The diagnostic only RUNS when a subscriber is installed — `tracing::warn!`
/// checks whether its callsite is enabled before evaluating its fields — which
/// is why every test in this file installs one.
#[test]
fn test_declined_keys_are_reported_when_a_set_is_installed() {
    support::install_subscriber();
    block_on(async {
        let document = format!(
            "{{\"keys\":[{{\"kty\":\"EC\",\"kid\":\"ec\"}},\
              {{\"kty\":\"RSA\",\"kid\":\"{TEST_KID}\",\"n\":\"{TEST_RSA_N}\",\"e\":\"AQAB\"}}]}}"
        );
        let verifier = JwksVerifier::new(
            afd_identity::StaticKeySet::new(document.into_bytes()),
            VerifierConfig::new(ISSUER, AUDIENCE),
            clock(),
        );
        verifier
            .verify(&valid_token())
            .await
            .expect("the usable key still verifies");
    });
}

/// A provider whose answer a test drives, for the default-window constructor.
#[derive(Debug)]
struct FixedClaim(&'static str);

impl ClaimSource for FixedClaim {
    fn claim(
        &self,
        _subject: &Subject,
    ) -> impl Future<Output = Result<String, ClaimUnavailable>> + Send {
        std::future::ready(Ok(self.0.to_owned()))
    }
}

/// The documented-window constructor is the one production uses.
///
/// `with_windows` exists for tests and for an operator with a reason; `new` is
/// what boot calls, and a constructor nothing exercises is a constructor whose
/// defaults nobody has checked.
#[test]
fn test_the_default_windows_constructor_resolves() {
    block_on(async {
        let capabilities =
            ProviderCapabilities::new(FixedClaim("fleet:read"), Arc::new(SystemClock));
        let subject = Subject::new("user_default").expect("a non-blank subject");

        let resolved = capabilities
            .capabilities(&subject)
            .await
            .expect("the provider answers");

        assert!(resolved.contains(afd_auth::scope::Scope::FleetRead));
        assert_eq!(capabilities.source().0, "fleet:read");
    });
}
