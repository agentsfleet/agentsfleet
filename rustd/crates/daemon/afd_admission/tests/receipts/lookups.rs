//! The lookups the lease path makes back into the ledger, keyed on the row id
//! an event is named by.

use afd_admission::{Reply as ReplyTo, sql};
use afd_db::test_util::mint_id;

use super::ledger::{CONNECTOR, THREAD, admit, steer};
use super::{Fallible, lane::Lane, queue::over_lane};

/// The lease path's two lookups key on the fleet and the row id: another
/// fleet's id answers nothing, and a second delivery stamps nothing.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn destination_lookup_is_scoped_to_the_admission_and_fleet() -> Fallible {
    let lane = Lane::seed().await?;
    let admissions = over_lane(&lane).await?;
    let stated = ReplyTo::To {
        connector: CONNECTOR,
        address: THREAD,
    };
    let admitted = admit(&admissions, steer(&lane, mint_id().as_str(), stated)).await?;
    let id = &admitted.stored.id;
    let mut connection = lane.database.acquire().await?;

    let found: Option<(String, String)> = sqlx::query_as(sql::SELECT_REPLY_DESTINATION)
        .bind(&lane.fleet)
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;
    assert_eq!(found, Some((CONNECTOR.to_owned(), THREAD.to_owned())));
    let elsewhere: Option<(String, String)> = sqlx::query_as(sql::SELECT_REPLY_DESTINATION)
        .bind(mint_id())
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;
    assert_eq!(
        elsewhere, None,
        "another fleet cannot read this row's destination"
    );

    drop(connection);
    lane.cleanup().await;
    Ok(())
}

/// A continuation that inherits names the earlier event by its id, and the
/// row it writes copies that event's destination in the same statement.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_inherited_destination_is_copied_from_the_named_row() -> Fallible {
    let lane = Lane::seed().await?;
    let admissions = over_lane(&lane).await?;
    let stated = ReplyTo::To {
        connector: CONNECTOR,
        address: THREAD,
    };
    let earlier = admit(&admissions, steer(&lane, mint_id().as_str(), stated)).await?;
    let inherited = ReplyTo::Inherit {
        event_id: &earlier.stored.id,
    };
    let later = admit(&admissions, steer(&lane, mint_id().as_str(), inherited)).await?;

    let copied: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT reply_provider, reply_address FROM fleet.admissions WHERE id = $1")
            .bind(&later.stored.id)
            .fetch_one(&mut *lane.database.acquire().await?)
            .await?;
    assert_eq!(
        copied,
        (Some(CONNECTOR.to_owned()), Some(THREAD.to_owned()))
    );
    lane.cleanup().await;
    Ok(())
}
