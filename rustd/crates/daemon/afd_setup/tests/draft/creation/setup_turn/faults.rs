//! A datastore that stops answering mid-way is not a turn's fault: the
//! assignment, the report and the review each hand the failure back, so the
//! caller rolls back, and the message stays as it was for the next pass.
//!
//! The lane's owner holds one table in `ACCESS EXCLUSIVE` mode, so the one
//! statement that reads it waits out the API login's lock bound. Each case
//! is a private database: the lock would stall every other proof.

use super::{
    Fixture, assigned, finished, funded, hint, mint, queued, reviewed, send, take, try_assign,
};
use af_core::error_code;
use afd_setup::setup_run::{SetupAssignment, SetupFinish, SetupHint, SetupOwed};
use sqlx::{Connection as _, PgConnection};

/// The table the creator's person read joins and nothing before it reads.
const LOCK_WORKSPACES: &str = "LOCK TABLE identity.workspaces IN ACCESS EXCLUSIVE MODE";
/// The table the head candidate's read takes, and nothing before it reads.
const LOCK_CANDIDATES: &str = "LOCK TABLE fleet.revisions IN ACCESS EXCLUSIVE MODE";
/// The table the payer's balance reads.
const LOCK_WALLETS: &str = "LOCK TABLE billing.wallets IN ACCESS EXCLUSIVE MODE";
/// The table the assignment's first read, its creator's, takes.
const LOCK_USERS: &str = "LOCK TABLE identity.users IN ACCESS EXCLUSIVE MODE";
/// The table a saved fleet's row is written to.
const LOCK_FLEETS: &str = "LOCK TABLE fleet.fleets IN EXCLUSIVE MODE";

/// Runs `work` while the lane's owner holds `lock`, then releases it.
async fn while_held<T>(f: &Fixture, lock: &'static str, work: impl Future<Output = T>) -> T {
    let mut owner = PgConnection::connect(&f.owner_url())
        .await
        .expect("the lane owner connects");
    let mut held = owner.begin().await.expect("the holder opens");
    sqlx::query(lock)
        .execute(&mut *held)
        .await
        .expect("the owner takes the table");
    let answered = work.await;
    held.rollback().await.expect("the owner releases the table");
    answered
}

/// One assignment of `hint` on a transaction bounded by `bounds`, rolled back.
async fn assign_bounded(
    f: &Fixture,
    hint: &SetupHint,
    bounds: &[(&str, &str)],
) -> afd_setup::Result<SetupAssignment> {
    let mut connection = f.connection().await;
    let mut tx = connection.begin().await.expect("the assignment opens");
    for (setting, value) in bounds {
        sqlx::query("SELECT set_config($1, $2, true)")
            .bind(setting)
            .bind(value)
            .execute(&mut *tx)
            .await
            .expect("the assignment's bound");
    }
    let mut owed = SetupOwed::default();
    let found = f
        .drafts
        .assign_setup_turn(&mut tx, hint, false, &mut owed)
        .await;
    tx.rollback().await.expect("the assignment rolls back");
    found
}

// Value: protects=a datastore that stops answering at any read of an
// assignment rolls it back instead of failing the person's message, and the
// message is assigned once the store answers; fails_when=any of those failure
// arms is read as the turn's own fault or as a pass; seam=
// Drafts::assign_setup_turn over a private live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_assignment_whose_store_stops_answering_hands_the_failure_back() {
    let c = funded().await;
    let draft = queued(&c.base, c.base.access(), "Watch checkout").await;
    let hint = hint(&c.base, &draft).await;
    // With no lock bound, the statement bound ends the creator's read, which
    // waits on the table, never on a row another assignment holds.
    let bounded = [("lock_timeout", "0"), ("statement_timeout", "300ms")];
    let held = while_held(
        &c.base,
        LOCK_USERS,
        assign_bounded(&c.base, &hint, &bounded),
    )
    .await;
    assert_eq!(
        held.expect_err("the creator's row never answers").code(),
        error_code::INTERNAL_DB_QUERY
    );
    // The ledger names a lock it gave up on an outage, as every billing read does.
    for (lock, code) in [
        (LOCK_WORKSPACES, error_code::INTERNAL_DB_QUERY),
        (LOCK_CANDIDATES, error_code::INTERNAL_DB_QUERY),
        (LOCK_WALLETS, error_code::INTERNAL_DB_UNAVAILABLE),
    ] {
        let refused = while_held(&c.base, lock, try_assign(&c.base, &hint)).await;
        assert_eq!(refused.expect_err(lock).code(), code, "{lock}");
    }
    let messages: Vec<String> =
        sqlx::query_scalar("SELECT status FROM thread.turns WHERE thread_id=$1 ORDER BY sequence")
            .bind(&draft)
            .fetch_all(&mut *c.base.connection().await)
            .await
            .expect("the draft's messages");
    assert_eq!(messages, ["queued"], "no fault failed the message");
    let taken = assigned(try_assign(&c.base, &hint).await.expect("the store answers"));
    assert!(
        taken.is_some(),
        "the message is assigned once the store answers"
    );
    c.cleanup().await;
}

// Value: protects=a report whose store stops answering, at the creator's
// read or at the save's fleet write, commits nothing, so its retry finds the
// message still running; fails_when=a save's datastore fault is kept on the
// message as a refusal, or the report commits a half; seam=the report half of
// afd_setup::Creations over a private live Postgres and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_whose_store_stops_answering_commits_nothing() {
    let c = funded().await;
    let draft = reviewed(&c, "fault-save").await;
    send(&c.base, &draft, "Save it", true).await;
    let turn = take(&c.base, &draft).await;
    let run = mint();

    let locked = while_held(&c.base, LOCK_WORKSPACES, async {
        let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
            .await
            .expect("the report opens");
        let locked = c
            .creations
            .lock_setup_report(&mut tx, &draft, &turn.message_id)
            .await
            .map(|scope| scope.is_some());
        tx.rollback().await.expect("the report rolls back");
        locked
    })
    .await;
    assert_eq!(
        locked.expect_err("the creator's read never answers").code(),
        error_code::INTERNAL_DB_QUERY
    );

    let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the report opens");
    let scope = c
        .creations
        .lock_setup_report(&mut tx, &draft, &turn.message_id)
        .await
        .expect("the report locks")
        .expect("the draft is there");
    let finish = SetupFinish {
        run_id: &run,
        failure: None,
        answer: Some("Saving."),
        proposal: None,
        now: super::now(),
    };
    let mut owed = SetupOwed::default();
    let saved = while_held(
        &c.base,
        LOCK_FLEETS,
        c.creations
            .finish_setup_turn(&mut tx, &scope, finish, &mut owed),
    )
    .await;
    assert_eq!(
        saved.expect_err("the fleet write never answers").code(),
        error_code::INTERNAL_DB_QUERY
    );
    tx.rollback().await.expect("the report rolls back");
    let (state, answer, refused) = finished(&c.base, &turn.message_id).await;
    assert_eq!((state.as_str(), answer, refused), ("running", None, None));
    assert_eq!(c.fleet_count().await, 0);
    c.cleanup().await;
}

// Value: protects=a review whose billing read stops answering fails the read
// rather than showing the wallet as missing; fails_when=the wallet check reads
// any billing fault as an empty wallet; seam=Setup::read_draft over a private
// live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_review_whose_billing_read_stops_answering_fails_the_read() {
    let c = funded().await;
    let draft = reviewed(&c, "fault-review").await;
    let read = while_held(
        &c.base,
        LOCK_WALLETS,
        c.setup.read_draft(c.base.access(), &draft),
    )
    .await;
    assert_eq!(
        read.expect_err("the wallet never answers").code(),
        error_code::INTERNAL_DB_UNAVAILABLE
    );
    let review = c
        .setup
        .read_draft(c.base.access(), &draft)
        .await
        .expect("the review reads once billing answers");
    assert!(review.review.is_some());
    c.cleanup().await;
}
