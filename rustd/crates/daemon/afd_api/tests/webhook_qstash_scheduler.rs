//! A scheduler that cannot verify a fire, and what the route tells the sender.
//!
//! The scheduler's keys live in a vault bag read on each fire, so the route has
//! two ways to be unable to verify that a forged token never produces: the bag
//! is unusable, which answers as a forgery does, and the vault will not answer,
//! which answers as the outage it is so the scheduler sends the fire again.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness::{Fleet, problem, refusal_code};
use crate::webhook_qstash_route::{BODY, CURRENT_KEY, FireClaims, HEADER_SIGNATURE, fire, mint};
use af_core::error_code;
use http::StatusCode;

/// A scheduler whose every call fails as `afd_cron`'s own sample of `kind` does.
#[derive(Debug)]
struct Failing(&'static str);

impl Failing {
    fn error(&self) -> afd_cron::Error {
        afd_cron::error::one_of_each_kind()
            .into_iter()
            .find(|(label, _)| *label == self.0)
            .map(|(_, error)| error)
            .expect("afd_cron declares this kind")
    }
}

#[async_trait::async_trait]
impl afd_cron::Scheduler for Failing {
    async fn upsert(
        &self,
        _schedule: &af_core::id::Uuid7,
        _cron: &str,
        _timezone: &str,
        _message: &str,
        _attempts: afd_cron::Attempts,
    ) -> afd_cron::Result<afd_cron::qstash::Registered> {
        Err(self.error())
    }

    async fn remove(&self, _id: &str, _attempts: afd_cron::Attempts) -> afd_cron::Result<()> {
        Err(self.error())
    }

    async fn verify(
        &self,
        _signature: &str,
        _body: &[u8],
    ) -> afd_cron::Result<afd_cron::VerifiedFire> {
        Err(self.error())
    }
}

#[tokio::test]
async fn an_unusable_scheduler_bag_is_refused_as_a_forged_fire_is() {
    let token = mint(&FireClaims::good(BODY), CURRENT_KEY);
    let router = Fleet::new()
        .with_scheduler(std::sync::Arc::new(Failing("bag unusable")))
        .router();

    let refused = fire(&router, BODY, &[(HEADER_SIGNATURE, token.as_str())]).await;

    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        refusal_code(refused).await,
        error_code::WEBHOOK_SIGNATURE_INVALID.as_str(),
        "telling a prober the bag is missing would say this daemon is misconfigured"
    );
}

#[tokio::test]
async fn a_vault_that_will_not_answer_is_an_outage_the_scheduler_retries() {
    let token = mint(&FireClaims::good(BODY), CURRENT_KEY);
    let router = Fleet::new()
        .with_scheduler(std::sync::Arc::new(Failing("bag unreadable")))
        .router();

    let answered = fire(&router, BODY, &[(HEADER_SIGNATURE, token.as_str())]).await;

    let (status, code, _detail) = problem(answered).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "a vault outage is this daemon failing; a 2xx or a 401 would end the scheduler's retries"
    );
    assert_eq!(code, error_code::INTERNAL_DB_UNAVAILABLE.as_str());
}
