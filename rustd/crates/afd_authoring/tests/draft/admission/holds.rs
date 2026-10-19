//! A claim's hold is placed after its attempt commits: a biller that cannot
//! place it leaves the chat running unheld and says why, and a ledger that
//! cannot be read refuses the claim before any attempt exists.

use super::super::biller::{Faults, Faulty, Reserve};
use super::fixture::{attempts, funded, queue};
use super::{Fixture, mint};
use afd_authoring::{Claimed, Drafts, Setup};
use afd_core::{error_code, test_util::trace::Capture};
use afd_crypto::{entropy::Entropy, secret::Kek};
use std::sync::Arc;

/// The event a claimed attempt with no hold is logged under.
const HOLD_UNPLACED: &str = "setup_chat_hold_unplaced";

/// Two open holds whose remainders no `BIGINT` can sum.
const OVERFLOWING_HOLD: &str = "INSERT INTO billing.holds \
     (id,work_type,work_id,tenant_id,ceiling_nanos,captured_nanos,created_at,updated_at) \
     VALUES($1::uuid,'fixture',$1::uuid,$2::uuid,9223372036854775807,0,now(),now())";

/// The fixture's queued work, claimed through a biller that fails `faults`.
async fn claim_with(f: &Fixture, faults: Faults) -> Claimed {
    let draft = queue(f, "Hold the claim").await;
    Drafts::new(f.db.clone(), Entropy::new(), Faulty::over(&f.db, faults))
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect("admission answers")
        .expect("the gate admits; only the hold is missed")
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_hold_the_balance_no_longer_covers_leaves_the_chat_running_unheld() {
    let f = funded(1).await;
    let capture = Capture::install();
    let claimed = claim_with(
        &f,
        Faults {
            reserve: Reserve::Short,
            ..Faults::NONE
        },
    )
    .await;
    let unplaced = capture.only(HOLD_UNPLACED);
    drop(capture);
    assert_eq!(unplaced.field("work_id"), Some(claimed.attempt_id.as_str()));
    assert!(
        unplaced
            .field("hold")
            .is_some_and(|hold| hold.starts_with("Short")),
        "the short answer is named: {unplaced:?}"
    );
    assert_eq!(attempts(&f).await, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_biller_that_cannot_answer_leaves_the_chat_running_unheld_under_its_code() {
    let f = funded(1).await;
    let capture = Capture::install();
    let claimed = claim_with(
        &f,
        Faults {
            reserve: Reserve::Down,
            ..Faults::NONE
        },
    )
    .await;
    let unplaced = capture.only(HOLD_UNPLACED);
    drop(capture);
    assert_eq!(unplaced.field("work_id"), Some(claimed.attempt_id.as_str()));
    assert_eq!(
        unplaced.field("error_code"),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    assert_eq!(attempts(&f).await, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_ledger_that_cannot_sum_its_holds_refuses_the_claim_and_the_review() {
    let f = funded(1).await;
    let mut connection = f.connection().await;
    for _ in 0..2 {
        sqlx::query(OVERFLOWING_HOLD)
            .bind(mint().as_str())
            .bind(f.tenant.as_str())
            .execute(&mut *connection)
            .await
            .expect("a corrupt open hold");
    }
    drop(connection);
    let draft = queue(&f, "Claim against a broken ledger").await;
    let refused = f
        .drafts
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect_err("an unreadable balance is no refusal to record");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(attempts(&f).await, 0, "no attempt was issued");
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    let review = Setup::new(f.drafts.clone(), Arc::new(key))
        .read_draft(f.access(), &draft.id)
        .await
        .expect_err("the wallet check cannot read the balance");
    assert_eq!(review.code(), error_code::INTERNAL_OPERATION_FAILED);
    f.cleanup().await;
}
