//! The scheduler's credentials as `QStash` reads them from the admin
//! workspace's vault: absent with no admin workspace, retryable when the vault
//! will not answer. Both are injected with no datastore behind them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code::{INTERNAL_DB_UNAVAILABLE, INTERNAL_OPERATION_FAILED};
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_cron::verifier::MAX_TOKEN_BYTES;
use afd_cron::{Attempts, QStash, Scheduler as _};
use afd_vault::Vault;
use afd_vault::bag::BagFault;

const DESTINATION: &str = "https://daemon.example.test/v1/ingress/qstash/schedules";
const ADMIN: &str = "01a1c513-9ab7-7ed5-8ad3-2cf8e2d02ef4";
const SCHEDULE: &str = "01a1c92d-311b-7bd8-8957-aeeb7963b2c2";

/// A fire token's shape, signed by nobody: it passes the shape check and so
/// reaches the read of the keys, which is what these cases are about.
const SHAPED: &str = "aaa.bbb.ccc";

/// A vault over a pool that never connects.
fn unreachable_vault() -> Vault {
    let kek = Kek::from_hex(&"ab".repeat(32)).expect("a 32-byte hex key");
    Vault::new(
        afd_db::test_util::unreachable_db(),
        std::sync::Arc::new(kek),
        Entropy::new(),
    )
}

#[expect(
    clippy::disallowed_methods,
    reason = "the client is never sent: every case fails before a request exists"
)]
fn qstash(admin: Option<Uuid7>) -> QStash {
    QStash::new(
        reqwest::Client::new(),
        unreachable_vault(),
        admin,
        DESTINATION.to_owned(),
    )
}

#[tokio::test]
async fn no_admin_workspace_is_an_absent_bag_for_every_call() {
    let scheduler = qstash(None);
    let schedule = Uuid7::parse(SCHEDULE).expect("a canonical id");

    let read = scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect_err("no workspace holds no bag");
    assert_eq!(read.bag_fault(), Some(BagFault::Absent));
    assert!(
        !read.is_retryable(),
        "nothing changes until an operator syncs the bag"
    );
    assert_eq!(read.refusal_word(), "scheduler_bag_unusable");
    assert_eq!(read.detail(), afd_cron::error::detail::UPSTREAM_UNAVAILABLE);

    let pushed = scheduler
        .upsert(&schedule, "0 9 * * 1", "UTC", "{}", Attempts::Retried)
        .await
        .expect_err("no credentials, no push");
    assert_eq!(pushed.bag_fault(), Some(BagFault::Absent));
    let fire = afd_cron::Scheduler::verify(&scheduler, SHAPED, b"{}")
        .await
        .expect_err("no keys, no believed fire");
    assert_eq!(fire.bag_fault(), Some(BagFault::Absent));
}

#[tokio::test]
async fn a_vault_that_will_not_answer_is_retryable_and_names_no_field() {
    let scheduler = qstash(Some(Uuid7::parse(ADMIN).expect("a canonical id")));

    let read = scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect_err("an unreachable vault answers nothing");
    assert!(read.is_retryable(), "a datastore outage heals on its own");
    assert_eq!(
        read.bag_fault(),
        None,
        "no bag was read, so no field is at fault"
    );
    assert_eq!(read.code(), af_core::error_code::INTERNAL_DB_UNAVAILABLE);
    if let Some(source) = std::error::Error::source(&read) {
        assert_ne!(source.to_string(), read.to_string());
    }
}

/// The token's bound and shape are checked ahead of the credentials read: a
/// fire with no token, an oversized one or a string that is no token costs the
/// datastore nothing.
#[tokio::test]
async fn a_fire_with_no_usable_token_is_refused_before_any_bag_is_read() {
    let scheduler = qstash(Some(Uuid7::parse(ADMIN).expect("a canonical id")));

    for token in [
        String::new(),
        "x".repeat(MAX_TOKEN_BYTES + 1),
        "garbage".to_owned(),
        "two.segments".to_owned(),
        "four.dotted.segments.here".to_owned(),
        "a..c".to_owned(),
        "has space.in.it".to_owned(),
    ] {
        let refused = scheduler
            .verify(&token, b"{}")
            .await
            .expect_err("a token outside the bound proves nothing");

        assert_eq!(refused.refusal_word(), "token_malformed", "{token:.20}");
        assert_eq!(refused.bag_fault(), None, "no bag was read to find out");
    }
}

/// The other side of the same check: a token of the right shape is passed on to
/// the keys, which here cannot be read.
#[tokio::test]
async fn a_token_of_the_right_shape_reaches_the_credentials_read() {
    let scheduler = qstash(Some(Uuid7::parse(ADMIN).expect("a canonical id")));

    let refused = scheduler
        .verify(SHAPED, b"{}")
        .await
        .expect_err("the vault is unreachable, so the keys cannot be read");

    assert_ne!(refused.refusal_word(), "token_malformed");
    assert!(refused.is_retryable(), "the read, not the token, failed");
}

/// An unusable read leaves one line naming the bag and the fault, so an
/// operator can tell a bag nobody stored from one stored wrong. It names no
/// value.
#[tokio::test]
async fn an_unusable_bag_leaves_one_line_naming_the_bag_and_the_fault() {
    let capture = Capture::install();
    let scheduler = qstash(None);

    scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect_err("no workspace holds no bag");

    let logged = capture.only("scheduler_bag_unusable");
    assert_eq!(logged.field("bag"), Some("qstash"));
    assert_eq!(logged.field("fault"), Some("absent"));
    assert_eq!(
        logged.field("error_code"),
        Some(INTERNAL_OPERATION_FAILED.as_str())
    );
}

/// A vault that would not answer names no fault, because no field was read,
/// and carries the vault's own code.
#[tokio::test]
async fn a_vault_that_would_not_answer_leaves_a_line_with_its_code_and_no_fault() {
    let capture = Capture::install();
    let scheduler = qstash(Some(Uuid7::parse(ADMIN).expect("a canonical id")));

    scheduler
        .remove("scd_fixture", Attempts::Once)
        .await
        .expect_err("an unreachable vault answers nothing");

    let logged = capture.only("scheduler_bag_unusable");
    assert_eq!(logged.field("fault"), None, "no field was read");
    assert_eq!(
        logged.field("error_code"),
        Some(INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert_eq!(logged.field("reason"), Some("operational_fault"));
}
