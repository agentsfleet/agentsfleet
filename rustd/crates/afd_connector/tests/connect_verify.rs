//! `Connectors::verify` — the step that decides WHO may finish a connect.
//!
//! # Why this is tested through `Connectors` and not through `state`
//!
//! `state::verify` proves a state is genuine and unexpired, and `state`'s own
//! suite covers that thoroughly. What it cannot cover is the composition here:
//! `Connectors::verify` asks `state::verify` first and THEN asks whether the
//! person presenting the state is the person who started the round-trip, and
//! turns a mismatch into [`Rejected::ForeignSubject`].
//!
//! Drop that second question and every case in `state`'s suite still passes,
//! while any authenticated person can complete somebody else's connect. That is
//! the failure this file exists to catch, and it is only visible one level up.
//!
//! # No datastore, and that is a property of the function
//!
//! `verify` touches no store — deliberately, so a state that is not going to be
//! acted on costs a hash rather than a round trip, and a replay attempt is not
//! a way to make this daemon do work. The `Connectors` below is built over
//! stores that are not there, which would fail loudly if that ever stopped
//! being true.
//!
//! The cases are `async` for a reason that is not about `verify`: constructing
//! the unreachable POOL needs a reactor, because `sqlx` registers its idle
//! reaper on build. Nothing below awaits anything.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::SecretBytes;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use afd_vault::Vault;

use afd_connector::state::{self, Rejected};
use afd_connector::{Connectors, Exchange, Grants, PlatformApp, Provider};

/// An address that answers nothing. Port 1 is reserved and unbound.
const NOWHERE: &str = "postgres://nobody@127.0.0.1:1/nothing";

/// The same, for the queue.
const NOWHERE_QUEUE: &str = "redis://127.0.0.1:1";

/// The workspace a round-trip is started in.
const WORKSPACE: &str = "01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8e9f";

/// The person who started it.
const STARTER: &str = "user_started_the_connect";

/// Somebody else, authenticated, who did not.
const BYSTANDER: &str = "user_did_not_start_it";

/// This round-trip's single-use slot.
const NONCE: &str = "nonce-for-this-round-trip";

/// The instant every case signs and verifies at.
const NOW_MS: i64 = 1_760_000_000_000;

/// A day in milliseconds — well past any connect window this suite asserts on.
const ONE_DAY_MS: i64 = 1_000 * 60 * 60 * 24;

/// The state signing secret this deployment holds.
fn secret() -> SecretBytes {
    SecretBytes::new(b"fixture-connector-state-secret".to_vec())
}

/// A secret this deployment does not hold.
fn foreign_secret() -> SecretBytes {
    SecretBytes::new(b"fixture-connector-other-secret".to_vec())
}

fn now() -> UnixMillis {
    UnixMillis::from_millis(NOW_MS)
}

/// The production connect flow over stores that are not there.
fn connectors() -> Connectors {
    let environment = MapEnv::from_pairs([(DbRole::Api.url_knob(), NOWHERE)]);
    let pool = PoolConfig::resolve(&environment, DbRole::Api).expect("the fixture URL resolves");
    let database = Db::unreachable(&pool);
    let queue = Redis::unreachable(&RedisConfig::from_url(
        RedisRole::Default,
        NOWHERE_QUEUE.to_owned(),
    ))
    .expect("a lazy manager opens no socket, so it cannot fail to open one");
    let kek = std::sync::Arc::new(afd_crypto::secret::Kek::from_bytes([7_u8; 32]));

    Connectors::new(
        PlatformApp::new(Vault::new(
            database.clone(),
            std::sync::Arc::clone(&kek),
            Entropy::new(),
        )),
        Grants::new(
            Vault::new(database.clone(), kek, Entropy::new()),
            database,
            Entropy::new(),
        ),
        Exchange::new(reqwest::Client::new()),
        reqwest::Client::new(),
        queue,
        Entropy::new(),
    )
}

/// A state this deployment signed for `subject`, on `provider`.
fn state_for(provider: Provider, subject: &str) -> String {
    state::sign(
        provider.state_binding(),
        &secret(),
        WORKSPACE,
        subject,
        NONCE,
        now(),
    )
}

#[tokio::test]
async fn the_person_who_started_the_round_trip_may_finish_it() {
    let presented = state_for(Provider::Slack, STARTER);

    let verified = connectors()
        .verify(Provider::Slack, &secret(), &presented, STARTER, now())
        .expect("the starter's own state verifies for them");

    assert_eq!(verified.workspace(), WORKSPACE);
    assert_eq!(
        verified.nonce(),
        NONCE,
        "the slot to spend travels in the state, so the spend cannot be aimed \
         at a different round-trip"
    );
}

/// The case this file exists for.
///
/// A GENUINE state, unexpired, presented by an authenticated person who did not
/// start the round-trip. Nothing about the signature is wrong, so a verify that
/// stopped at `state::verify` would accept it — and that person would complete
/// somebody else's connect, landing a grant in a workspace on the strength of a
/// state they found in a browser log or a shared screen.
#[tokio::test]
async fn an_authenticated_bystander_cannot_finish_somebody_elses_connect() {
    let presented = state_for(Provider::Slack, STARTER);

    let rejected = connectors()
        .verify(Provider::Slack, &secret(), &presented, BYSTANDER, now())
        .expect_err("a state is not a bearer token");

    assert_eq!(
        rejected,
        Rejected::ForeignSubject,
        "and NOT BadSignature: the two send an operator to different places — \
         a forged state is somebody probing, this is an authenticated person \
         completing a round-trip that was not theirs"
    );
}

#[tokio::test]
async fn a_state_signed_under_a_secret_this_deployment_does_not_hold_is_a_bad_signature() {
    let presented = state::sign(
        Provider::Slack.state_binding(),
        &foreign_secret(),
        WORKSPACE,
        STARTER,
        NONCE,
        now(),
    );

    assert_eq!(
        connectors()
            .verify(Provider::Slack, &secret(), &presented, STARTER, now())
            .expect_err("a foreign secret does not sign for this deployment"),
        Rejected::BadSignature
    );
}

/// A state is bound to the connector it was minted for.
///
/// Without the binding, a state started against one provider would complete
/// against another — landing a grant under the wrong provider's key name.
#[tokio::test]
async fn a_state_minted_for_one_connector_does_not_verify_as_another() {
    let presented = state_for(Provider::Slack, STARTER);

    assert_eq!(
        connectors()
            .verify(Provider::Jira, &secret(), &presented, STARTER, now())
            .expect_err("the binding separates the connectors"),
        Rejected::BadSignature
    );
}

#[tokio::test]
async fn a_state_past_its_window_is_expired_rather_than_forged() {
    let presented = state_for(Provider::Slack, STARTER);
    let long_after = UnixMillis::from_millis(NOW_MS + ONE_DAY_MS);

    assert_eq!(
        connectors()
            .verify(Provider::Slack, &secret(), &presented, STARTER, long_after)
            .expect_err("a state does not last a day"),
        Rejected::Expired,
        "an operator reading the log needs to tell a person who took too long \
         from somebody probing"
    );
}

#[tokio::test]
async fn nothing_that_is_not_a_state_verifies_as_one() {
    for candidate in ["", "not-a-state", "a.b", "....", "%%%"] {
        let rejected = connectors()
            .verify(Provider::Slack, &secret(), candidate, STARTER, now())
            .expect_err("`{candidate}` is not a state");
        assert!(
            matches!(rejected, Rejected::Malformed | Rejected::BadSignature),
            "`{candidate}` earned {rejected:?}"
        );
    }
}

/// Every rejection reaches a log under its own word.
///
/// One code answers the caller — telling them WHICH way their state was
/// refused would help somebody probing — while the log has to be able to
/// distinguish them, because the four mean four different things to an operator.
#[tokio::test]
async fn the_four_rejections_are_four_different_words_in_the_log() {
    let words = [
        Rejected::Malformed.reason(),
        Rejected::BadSignature.reason(),
        Rejected::Expired.reason(),
        Rejected::ForeignSubject.reason(),
    ];
    let mut unique = words.to_vec();
    unique.sort_unstable();
    unique.dedup();

    assert_eq!(
        unique.len(),
        words.len(),
        "two rejections share a word, so a log cannot tell them apart: {words:?}"
    );
}
