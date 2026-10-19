//! The lookups the lease path makes back into the ledger, keyed on the row id
//! an event is named by.

use afd_admission::{Reply as ReplyTo, sql};
use afd_db::test_util::mint_id;
use afd_dragonfly::{EventId, GroupCursor};

use super::ledger::{CONNECTOR, FAKE_RECEIPT, THREAD, admit, steer};
use super::{lane::Lane, queue::over_fake};
use crate::fake_redis::Reply;

/// The lease path's two lookups key on the fleet and the row id: another
/// fleet's id answers nothing, and a second delivery stamps nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn delivery_and_destination_lookups_key_on_the_row_id() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;
    let stated = ReplyTo::To {
        connector: CONNECTOR,
        address: THREAD,
    };
    let admitted = admit(&admissions, steer(&lane, &mint_id(), stated)).await;
    let id = admitted.stored.id.as_str();
    let mut connection = lane.database.acquire().await.expect("a pooled connection");

    let found: Option<(String, String)> = sqlx::query_as(sql::SELECT_REPLY_DESTINATION)
        .bind(&lane.fleet)
        .bind(id)
        .fetch_optional(&mut *connection)
        .await
        .expect("the destination reads");
    assert_eq!(found, Some((CONNECTOR.to_owned(), THREAD.to_owned())));
    let elsewhere: Option<(String, String)> = sqlx::query_as(sql::SELECT_REPLY_DESTINATION)
        .bind(mint_id())
        .bind(id)
        .fetch_optional(&mut *connection)
        .await
        .expect("the destination reads");
    assert_eq!(
        elsewhere, None,
        "another fleet cannot read this row's destination"
    );

    for expected in [1, 0] {
        let stamped = sqlx::query(sql::MARK_DELIVERED)
            .bind(&lane.fleet)
            .bind(id)
            .execute(&mut *connection)
            .await
            .expect("the stamp runs")
            .rows_affected();
        assert_eq!(stamped, expected, "only the first delivery stamps");
    }
    drop(connection);
    lane.cleanup().await;
}

/// A continuation that inherits names the earlier event by its id, and the
/// row it writes copies that event's destination in the same statement.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_inherited_destination_is_copied_from_the_named_row() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;
    let stated = ReplyTo::To {
        connector: CONNECTOR,
        address: THREAD,
    };
    let earlier = admit(&admissions, steer(&lane, &mint_id(), stated)).await;
    let inherited = ReplyTo::Inherit {
        event_id: &earlier.stored.id,
    };
    let later = admit(&admissions, steer(&lane, &mint_id(), inherited)).await;

    let copied: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT reply_provider, reply_address FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(&later.stored.id)
    .fetch_one(&mut *lane.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the continuation's row reads");
    assert_eq!(
        copied,
        (Some(CONNECTOR.to_owned()), Some(THREAD.to_owned()))
    );
    lane.cleanup().await;
}

/// A delivered admission is found through `fleet.events.event_id`, which holds
/// the row id as text, and its receipt is where a recreated group resumes.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_delivered_cursor_joins_events_on_the_row_id() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[("XADD", Reply::Bulk(FAKE_RECEIPT))]).await;
    assert_eq!(
        admissions
            .delivered_cursor(&lane.fleet)
            .await
            .expect("reads"),
        GroupCursor::Beginning,
        "nothing delivered yet"
    );
    let admitted = admit(&admissions, steer(&lane, &mint_id(), ReplyTo::None)).await;

    sqlx::query(
        "INSERT INTO fleet.events (id, fleet_id, workspace_id, event_id, actor, event_type, \
         status, request_json, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'fixture:ledger', 'chat', 'received', \
         '{}'::jsonb, now(), now())",
    )
    .bind(mint_id())
    .bind(&lane.fleet)
    .bind(&lane.workspace)
    .bind(&admitted.stored.id)
    .execute(&mut *lane.database.acquire().await.expect("a pooled connection"))
    .await
    .expect("the delivery's event row inserts");

    assert_eq!(
        admissions
            .delivered_cursor(&lane.fleet)
            .await
            .expect("reads"),
        GroupCursor::After(EventId::of(FAKE_RECEIPT))
    );
    lane.cleanup().await;
}
