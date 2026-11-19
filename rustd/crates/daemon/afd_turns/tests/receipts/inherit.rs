//! A reply destination is inherited only from an admission in the same
//! workspace.

use af_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_turns::{Admissions, Admitted, Reply as ReplyTo};

use super::{
    Fallible,
    lane::Lane,
    ledger::{admit, steer},
    queue::over_lane,
};

/// The connector a thread's destination belongs to.
const CONNECTOR: &str = "slack";

/// A thread's address, as its connector writes it.
const ADDRESS: &str =
    r#"{"team_id":"T0FIXTURE","channel_id":"C0FIXTURE","thread_ts":"1712345678.000200"}"#;

type Destination = (Option<String>, Option<String>);

async fn destination(lane: &Lane, id: &Uuid7) -> Fallible<Destination> {
    Ok(
        sqlx::query_as("SELECT reply_provider, reply_address FROM fleet.admissions WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *lane.database.acquire().await?)
            .await?,
    )
}

/// A message on `lane`'s fleet that inherits the destination of `source`.
async fn inherit_from(admissions: &Admissions, lane: &Lane, source: &Uuid7) -> Fallible<Admitted> {
    let key = mint_id();
    let inherit = ReplyTo::Inherit { event_id: source };
    admit(admissions, steer(lane, key.as_str(), inherit)).await
}

/// An admission naming an event in the same workspace takes that event's
/// destination; one naming an event of another workspace takes none, and the
/// message is still admitted.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn inherit_copies_a_destination_only_inside_its_own_workspace() -> Fallible {
    let (ours, theirs) = (Lane::seed().await?, Lane::seed().await?);
    let (here, there) = (over_lane(&ours).await?, over_lane(&theirs).await?);
    let at = ReplyTo::To {
        connector: CONNECTOR,
        address: ADDRESS,
    };

    let from_ours = admit(&here, steer(&ours, mint_id().as_str(), at)).await?;
    let from_theirs = admit(&there, steer(&theirs, mint_id().as_str(), at)).await?;
    assert!(
        destination(&theirs, &from_theirs.stored.id)
            .await?
            .0
            .is_some()
    );

    let inherited = inherit_from(&here, &ours, &from_ours.stored.id).await?;
    assert_eq!(
        destination(&ours, &inherited.stored.id).await?,
        (Some(CONNECTOR.to_owned()), Some(ADDRESS.to_owned())),
        "an event of the same workspace lends its destination"
    );

    let crossed = inherit_from(&here, &ours, &from_theirs.stored.id).await?;
    assert!(!crossed.replayed, "the message is admitted");
    assert_eq!(
        destination(&ours, &crossed.stored.id).await?,
        (None, None),
        "an event of another workspace lends nothing"
    );
    ours.cleanup().await;
    theirs.cleanup().await;
    Ok(())
}
