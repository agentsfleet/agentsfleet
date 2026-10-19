//! An admission follows the surrounding transaction and sends no queue command before commit.

use super::{
    lane::Lane,
    queue::{confirm, over_fake},
};
use crate::fake_redis::Reply;
use afd_admission::{Admission, Key, Producer, Reply as ReplyTo};
use afd_wire::event::EventType;
use sqlx::Connection as _;

fn request(lane: &Lane) -> Admission<'_> {
    Admission {
        producer: Producer::Steer,
        key: Key::Repeated(&lane.fleet),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: "fixture:staged",
        event_type: EventType::Chat,
        request_json: r#"{"message":"staged initial run"}"#,
        reply: ReplyTo::None,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rollback_removes_staged_admission_and_no_append_escapes() {
    let lane = Lane::seed().await;
    let (server, admissions) = over_fake(&lane, &[]).await;
    let prepared = admissions
        .prepare(request(&lane))
        .await
        .expect("budget observation");
    let mut connection = lane.database.acquire().await.expect("transaction");
    let mut tx = connection.begin().await.expect("begin");
    admissions
        .stage(&mut tx, prepared)
        .await
        .expect("stage locally");
    tx.rollback()
        .await
        .expect("rollback surrounding activation");
    drop(connection);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1::uuid")
            .bind(&lane.fleet)
            .fetch_one(&mut *lane.database.acquire().await.expect("observer"))
            .await
            .expect("rolled-back state");
    assert_eq!(count, 0);
    assert!(!server.seen().iter().any(|line| line.starts_with("XADD")));
    assert_eq!(
        admissions
            .confirm(Producer::Steer, &lane.fleet)
            .await
            .expect("no pending row"),
        None
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn committed_stage_is_pending_until_one_receipt_commits() {
    let lane = Lane::seed().await;
    let (server, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk("1790000000000-0"))]).await;
    let prepared = admissions
        .prepare(request(&lane))
        .await
        .expect("budget observation");
    let mut connection = lane.database.acquire().await.expect("transaction");
    let mut tx = connection.begin().await.expect("begin");
    let first = admissions
        .stage(&mut tx, prepared)
        .await
        .expect("durable stage");
    tx.commit()
        .await
        .expect("activation and initial admission commit");
    drop(connection);
    assert_eq!(lane.receipt().await, None);
    assert!(!server.seen().iter().any(|line| line.starts_with("XADD")));
    let accepted = confirm(&admissions, &lane)
        .await
        .expect("same event confirmed");
    assert!(accepted.accepted);
    assert_eq!(accepted.event_id, first.stored.id);
    assert_eq!(
        confirm(&admissions, &lane)
            .await
            .expect("confirmation replay"),
        accepted
    );
    assert_eq!(
        server
            .seen()
            .iter()
            .filter(|line| line.starts_with("XADD"))
            .count(),
        1
    );
    lane.cleanup().await;
}

/// An unrepeatable admission keys its row by the row's own id: two stages of
/// the same request are two rows, neither read as a retry of the other.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_unrepeatable_stage_is_keyed_by_its_own_row() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let mut staged = Vec::new();
    for _ in 0..2 {
        let prepared = admissions
            .prepare(Admission {
                key: Key::Unrepeatable,
                ..request(&lane)
            })
            .await
            .expect("budget observation");
        let mut connection = lane.database.acquire().await.expect("transaction");
        let mut tx = connection.begin().await.expect("begin");
        let admitted = admissions
            .stage(&mut tx, prepared)
            .await
            .expect("durable stage");
        tx.commit().await.expect("the stage commits");
        assert!(!admitted.replayed, "an unrepeatable key is never a retry");
        staged.push(admitted.stored.id);
    }

    assert_ne!(staged.first(), staged.last(), "two stages, two rows");
    for id in &staged {
        let key: String =
            sqlx::query_scalar("SELECT producer_key FROM fleet.admissions WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&mut *lane.database.acquire().await.expect("a pooled connection"))
                .await
                .expect("the staged row reads back");
        assert_eq!(&key, id, "the row's key is its own id");
    }
    lane.cleanup().await;
}
