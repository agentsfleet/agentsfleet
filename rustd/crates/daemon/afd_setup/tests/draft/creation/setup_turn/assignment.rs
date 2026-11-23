//! An assignment reads a draft's head message without a lock: a draft another
//! transaction holds is still assigned, and one whose turn, creator or draft
//! moved since its hint is passed.

use super::{ANSWER, funded, hint, queued, report, send, take, try_assign};
use afd_setup::setup_run::SetupAssignment;
use sqlx::Connection as _;

// Value: protects=a taken turn carries the earlier answered turns, and its
// Debug names the draft and message, never the person's text; fails_when=the
// history drops the answered turn, or Debug prints private text;
// seam=Drafts::assign_setup_turn over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_taken_turn_carries_its_history_and_debug_names_no_text() {
    let c = funded().await;
    let draft = queued(&c.base, c.base.access(), "Watch checkout").await;
    let first = take(&c.base, &draft).await;
    assert!(first.history.is_empty());
    report(&c, &c.creations, &first)
        .await
        .expect("the first turn reports");
    send(&c.base, &draft, "Now watch billing", false).await;
    let second = take(&c.base, &draft).await;
    assert_eq!(second.sequence, 2);
    assert_eq!(second.user_text, "Now watch billing");
    let history: Vec<_> = second
        .history
        .iter()
        .map(|turn| (turn.sequence, turn.message.as_str(), turn.answer.as_str()))
        .collect();
    assert_eq!(history, [(1, "Watch checkout", ANSWER)]);
    assert_eq!(format!("{:?}", second.history), "[SetupTurnText { .. }]");
    let shown = format!("{second:?}");
    assert!(shown.starts_with("SetupTurn { draft_id: "), "{shown}");
    assert!(shown.contains(draft.as_str()), "{shown}");
    assert!(shown.contains(second.message_id.as_str()), "{shown}");
    assert!(
        shown.ends_with("sequence: 2, reassigned: false, .. }"),
        "{shown}"
    );
    assert!(!shown.contains("billing"), "{shown}");
    c.cleanup().await;
}

// Value: protects=a draft whose row another transaction holds, as every draft
// write does, is assigned and claimed without waiting on it, so a page read or
// a writer can no longer make a pass skip the draft (the setup Save that sat
// queued); fails_when=the assignment locks the draft or its creator again, and
// waits out the login's lock bound or answers busy; seam=Drafts::assign_setup_turn
// and Drafts::claim_setup_turn over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_draft_another_transaction_holds_is_still_assigned() {
    let c = funded().await;
    let draft = queued(&c.base, c.base.access(), "Watch checkout").await;
    let mut held = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the holder opens");
    sqlx::query("SELECT id FROM setup.drafts WHERE id=$1 FOR UPDATE")
        .bind(&draft)
        .fetch_one(&mut *held)
        .await
        .expect("the holder locks the draft");

    let taken = take(&c.base, &draft).await;

    assert_eq!(taken.draft_id, draft);
    assert_eq!(taken.sequence, 1);
    held.rollback().await.expect("the holder releases");
    let status: String =
        sqlx::query_scalar("SELECT status FROM thread.turns WHERE thread_id=$1 AND sequence=1")
            .bind(&draft)
            .fetch_one(&mut *c.base.connection().await)
            .await
            .expect("the message");
    assert_eq!(
        status, "running",
        "the claim took the message past the holder"
    );
    c.cleanup().await;
}

// Value: protects=a hint carries no authority: a turn another assignment took
// or a person stopped since the hint, a deleted draft and a deleted creator
// are each passed; fails_when=any recheck of the hint is dropped and the
// stale hint is assigned; seam=Drafts::assign_setup_turn over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_hint_overtaken_since_it_was_read_is_passed() {
    let c = funded().await;
    let passed = |found: SetupAssignment| matches!(found, SetupAssignment::Passed);

    let draft = queued(&c.base, c.base.access(), "Watch checkout").await;
    let stale = hint(&c.base, &draft).await;
    sqlx::query("UPDATE thread.threads SET run_expires_at = now() + interval '1 hour' WHERE id=$1")
        .bind(&draft)
        .execute(&mut *c.base.connection().await)
        .await
        .expect("another assignment's run holds the thread");
    assert!(passed(try_assign(&c.base, &stale).await.expect("answers")));
    sqlx::query(
        "UPDATE thread.threads SET run_expires_at = now() - interval '1 second' WHERE id=$1",
    )
    .bind(&draft)
    .execute(&mut *c.base.connection().await)
    .await
    .expect("the run lapsed");
    c.base
        .stop(&draft, 1)
        .await
        .expect("the person stops the message");
    assert!(passed(try_assign(&c.base, &stale).await.expect("answers")));

    let deleted = queued(&c.base, c.base.access(), "Watch billing").await;
    let stale = hint(&c.base, &deleted).await;
    c.base
        .drafts
        .delete(c.base.access(), &deleted)
        .await
        .expect("the creator deletes the draft");
    assert!(passed(try_assign(&c.base, &stale).await.expect("answers")));

    let member = queued(&c.base, c.base.member_access(), "Watch refunds").await;
    let stale = hint(&c.base, &member).await;
    let mut owner = sqlx::PgConnection::connect(&c.base.owner_url())
        .await
        .expect("the lane owner connects");
    sqlx::query("DELETE FROM identity.users WHERE id=$1")
        .bind(&c.base.member)
        .execute(&mut owner)
        .await
        .expect("the member's account is deleted");
    assert!(passed(try_assign(&c.base, &stale).await.expect("answers")));
    c.cleanup().await;
}

/// Appends a candidate no build can read to draft `$1`, id `$2`, as its head.
const UNREADABLE_HEAD: &str = "WITH appended AS ( \
         INSERT INTO fleet.revisions \
             (id,draft_id,revision,definition,canonical_digest,policy_semantics,installed,created_at,updated_at) \
         SELECT $2,d.id,c.revision+1,'{\"schema_version\":99}'::jsonb, \
             c.canonical_digest,c.policy_semantics,false,now(),now() \
           FROM fleet.revisions c JOIN setup.drafts d ON d.head_revision_id=c.id \
          WHERE d.id=$1 RETURNING id) \
     UPDATE setup.drafts SET head_revision_id=(SELECT id FROM appended) WHERE id=$1";

// Value: protects=a head candidate this build cannot read fails its message,
// with a retry offered, and passes the draft so the walk goes on;
// fails_when=the unreadable head stalls the walk, or is assigned;
// seam=Drafts::assign_setup_turn over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_head_this_build_cannot_read_fails_its_message_and_passes() {
    let c = funded().await;
    let draft = queued(&c.base, c.base.access(), "Watch checkout").await;
    let stale = hint(&c.base, &draft).await;
    sqlx::query(UNREADABLE_HEAD)
        .bind(&draft)
        .bind(super::mint())
        .execute(&mut *c.base.connection().await)
        .await
        .expect("an unreadable head");
    let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the assignment opens");
    let mut owed = afd_setup::setup_run::SetupOwed::default();
    let found = c
        .base
        .drafts
        .assign_setup_turn(&mut tx, &stale, false, &mut owed)
        .await
        .expect("the assignment answers");
    assert!(matches!(found, SetupAssignment::Passed), "{found:?}");
    tx.commit().await.expect("the assignment commits");
    owed.settle(&c.base.drafts).await;
    let (status, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT status, failure_reason FROM thread.turns WHERE thread_id=$1 AND sequence=1",
    )
    .bind(&draft)
    .fetch_one(&mut *c.base.connection().await)
    .await
    .expect("the message");
    assert_eq!(
        (status.as_str(), reason.as_deref()),
        ("failed", Some("provider_failed"))
    );
    c.cleanup().await;
}

// Value: protects=a payer with no wallet fails the message for its balance and
// passes the draft; fails_when=a missing wallet is read as a datastore fault
// and stalls the walk; seam=Drafts::assign_setup_turn over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_payer_with_no_wallet_fails_the_message_for_its_balance() {
    let c = super::Case::isolated().await;
    super::seed_runner(&c.base).await;
    let draft = queued(&c.base, c.base.access(), "Watch checkout").await;
    let stale = hint(&c.base, &draft).await;
    let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the assignment opens");
    let mut owed = afd_setup::setup_run::SetupOwed::default();
    let found = c
        .base
        .drafts
        .assign_setup_turn(&mut tx, &stale, false, &mut owed)
        .await
        .expect("the assignment answers");
    assert!(matches!(found, SetupAssignment::Passed), "{found:?}");
    tx.commit().await.expect("the assignment commits");
    owed.settle(&c.base.drafts).await;
    let (status, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT status, failure_reason FROM thread.turns WHERE thread_id=$1 AND sequence=1",
    )
    .bind(&draft)
    .fetch_one(&mut *c.base.connection().await)
    .await
    .expect("the message");
    assert_eq!(
        (status.as_str(), reason.as_deref()),
        ("failed", Some("insufficient_balance"))
    );
    c.cleanup().await;
}
