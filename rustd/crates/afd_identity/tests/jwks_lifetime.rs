//! Outage tolerance and key removal through one live verifier instance.
#![expect(clippy::expect_used, reason = "test prerequisites must fail loudly")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use afd_auth::credential::Presented;
use afd_auth::verifier::{TokenVerifier, VerifyError};
use afd_core::clock::{Clock, FixedClock};
use afd_identity::jwks::cache::{DEFAULT_TTL, REFRESH_MIN_INTERVAL, STALE_GRACE};
use afd_identity::jwks::source::KeySetSource;
use afd_identity::{JwksVerifier, VerifierConfig};

use crate::support::{ONE_MS, signing};

#[derive(Debug)]
struct Source {
    offline: AtomicBool,
    document: Mutex<Vec<u8>>,
}

impl KeySetSource for Source {
    fn fetch(&self) -> impl Future<Output = Result<Vec<u8>, VerifyError>> + Send {
        std::future::ready(if self.offline.load(Ordering::SeqCst) {
            Err(VerifyError::KeySetUnavailable)
        } else {
            Ok(self
                .document
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone())
        })
    }
}

fn verifier() -> (JwksVerifier<Source>, Arc<FixedClock>, Presented) {
    let clock = Arc::new(FixedClock::at(signing::NOW));
    let source = Source {
        offline: AtomicBool::new(false),
        document: Mutex::new(signing::key_set().into_bytes()),
    };
    let config = VerifierConfig::new(signing::ISSUER, signing::AUDIENCE);
    let verifier = JwksVerifier::new(source, config, Arc::clone(&clock) as Arc<dyn Clock>);
    let payload = serde_json::json!({
        "sub": "user_key_lifetime", "iss": signing::ISSUER,
        "aud": signing::AUDIENCE, "exp": signing::NOT_EXPIRED,
    });
    (verifier, clock, signing::sign(&payload.to_string()))
}

#[tokio::test]
async fn repeated_outages_do_not_extend_the_last_confirmed_keys_lifetime() {
    let (verifier, clock, token) = verifier();
    verifier
        .verify(&token)
        .await
        .expect("the provider publishes the key");
    verifier.source().offline.store(true, Ordering::SeqCst);
    clock.advance(DEFAULT_TTL + ONE_MS);
    verifier
        .verify(&token)
        .await
        .expect("the outage is within the grace period");
    clock.advance(STALE_GRACE.saturating_sub(ONE_MS));
    verifier
        .verify(&token)
        .await
        .expect("the ceiling itself remains usable");
    clock.advance(ONE_MS);
    assert_eq!(
        verifier
            .verify(&token)
            .await
            .expect_err("the stale ceiling was exceeded"),
        VerifyError::KeySetUnavailable
    );
}

#[tokio::test]
async fn a_successful_refresh_restores_verification_after_the_ceiling() {
    let (verifier, clock, token) = verifier();
    verifier.verify(&token).await.expect("warm the cache");
    verifier.source().offline.store(true, Ordering::SeqCst);
    clock.advance(DEFAULT_TTL + STALE_GRACE + ONE_MS);
    assert_eq!(
        verifier
            .verify(&token)
            .await
            .expect_err("unconfirmed keys are too old"),
        VerifyError::KeySetUnavailable
    );
    verifier.source().offline.store(false, Ordering::SeqCst);
    clock.advance(REFRESH_MIN_INTERVAL + ONE_MS);
    verifier
        .verify(&token)
        .await
        .expect("the same verifier recovers after confirmation");
}

#[tokio::test]
async fn removed_keys_stop_verifying_after_refresh_of_the_same_cache() {
    let (verifier, clock, token) = verifier();
    verifier.verify(&token).await.expect("warm the cache");
    *verifier
        .source()
        .document
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = signing::key_set()
        .replace(signing::KID, "replacement-key")
        .into_bytes();
    clock.advance(DEFAULT_TTL + ONE_MS);
    assert_eq!(
        verifier
            .verify(&token)
            .await
            .expect_err("the issuer removed the old identifier"),
        VerifyError::KeyNotFound
    );
    *verifier
        .source()
        .document
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = signing::key_set().into_bytes();
    clock.advance(REFRESH_MIN_INTERVAL + ONE_MS);
    verifier
        .verify(&token)
        .await
        .expect("the original key is published again");
}

#[tokio::test]
async fn duplicated_scope_claims_are_refused_after_signature_verification() {
    let (verifier, _clock, _token) = verifier();
    let payload = format!(
        r#"{{"sub":"user_duplicates","iss":"{}","aud":"{}","exp":{},"scopes":"fleet:read","scopes":"fleet:admin"}}"#,
        signing::ISSUER,
        signing::AUDIENCE,
        signing::NOT_EXPIRED
    );
    let token = signing::sign(&payload);
    assert_eq!(
        verifier
            .verify(&token)
            .await
            .expect_err("duplicate known claims are ambiguous"),
        VerifyError::Malformed
    );
}

#[tokio::test]
async fn malformed_signature_encoding_is_distinct_from_an_invalid_signature() {
    let (verifier, _clock, valid) = verifier();
    let (message, _signature) = valid.expose().rsplit_once('.').expect("a signed token");
    for (signature, expected) in [
        ("!invalid-base64", VerifyError::Malformed),
        ("AQAB", VerifyError::SignatureInvalid),
    ] {
        let token = Presented::new(&format!("{message}.{signature}")).expect("nonblank");
        assert_eq!(
            verifier
                .verify(&token)
                .await
                .expect_err("invalid signature"),
            expected
        );
    }
}
