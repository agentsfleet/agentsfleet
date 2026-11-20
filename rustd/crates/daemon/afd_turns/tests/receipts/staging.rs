//! A turn follows the surrounding transaction and rings no notification before commit.

use super::{
    Fallible,
    lane::Lane,
    queue::{confirm, over_bus, over_lane},
};
use af_wire::turn::TurnKind;
use afd_turns::{IncomingTurn, Key, Producer, Reply as ReplyTo};
use sqlx::Connection as _;

fn request(lane: &Lane) -> IncomingTurn<'_> {
    IncomingTurn {
        producer: Producer::Steer,
        key: Key::Repeated(lane.fleet.as_str()),
        fleet: &lane.fleet,
        workspace: &lane.workspace,
        actor: "fixture:staged",
        event_type: TurnKind::Chat,
        request_json: r#"{"message":"staged initial run"}"#,
        reply: ReplyTo::None,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn rollback_removes_staged_turn_without_a_notification() -> Fallible {
    let lane = Lane::seed().await?;
    let (mut rings, turns) = over_bus(&lane).await?;
    let prepared = turns.prepare(request(&lane)).await?;
    let mut connection = lane.database.acquire().await?;
    let mut tx = connection.begin().await?;
    turns.stage(&mut tx, prepared).await?;
    tx.rollback().await?;
    drop(connection);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE producer IS NOT NULL AND thread_id = $1",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.database.acquire().await?)
    .await?;
    assert_eq!(count, 0);
    assert_eq!(rings.taken().await?, Vec::<String>::new());
    assert_eq!(
        turns.confirm(Producer::Steer, lane.fleet.as_str()).await?,
        None
    );
    assert_eq!(rings.taken().await?, Vec::<String>::new());
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_committed_stage_confirms_without_a_second_queue_record() -> Fallible {
    let lane = Lane::seed().await?;
    let (mut rings, turns) = over_bus(&lane).await?;
    let prepared = turns.prepare(request(&lane)).await?;
    let mut connection = lane.database.acquire().await?;
    let mut tx = connection.begin().await?;
    let first = turns.stage(&mut tx, prepared).await?;
    tx.commit().await?;
    drop(connection);
    assert_eq!(
        rings.taken().await?,
        Vec::<String>::new(),
        "a stage rings nothing; its caller's commit does not either"
    );
    let accepted = confirm(&turns, &lane).await?;
    assert_eq!(accepted.turn_id, first.stored.id);
    assert_eq!(confirm(&turns, &lane).await?, accepted);
    assert_eq!(
        rings.taken().await?,
        [lane.fleet.as_str(), lane.fleet.as_str()],
        "each confirmation rings the fleet once"
    );
    lane.cleanup().await;
    Ok(())
}

/// An unrepeatable turn keys its row by the row's own id: two stages of
/// the same request are two rows, neither read as a retry of the other.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unrepeatable_stage_is_keyed_by_its_own_row() -> Fallible {
    let lane = Lane::seed().await?;
    let turns = over_lane(&lane).await?;
    let mut staged = Vec::new();
    for _ in 0..2 {
        let prepared = turns
            .prepare(IncomingTurn {
                key: Key::Unrepeatable,
                ..request(&lane)
            })
            .await?;
        let mut connection = lane.database.acquire().await?;
        let mut tx = connection.begin().await?;
        let admitted = turns.stage(&mut tx, prepared).await?;
        tx.commit().await?;
        assert!(!admitted.replayed, "an unrepeatable key is never a retry");
        staged.push(admitted.stored.id);
    }

    assert_ne!(staged.first(), staged.last(), "two stages, two rows");
    for id in &staged {
        let key: String = sqlx::query_scalar("SELECT producer_key FROM thread.turns WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *lane.database.acquire().await?)
            .await?;
        assert_eq!(&key, id, "the row's key is its own id");
    }
    lane.cleanup().await;
    Ok(())
}
