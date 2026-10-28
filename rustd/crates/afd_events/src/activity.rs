//! Durable receipts join the fleet history without admitting or executing work.

use afd_core::{event::status::RECORDED, id::Uuid7};
use afd_dragonfly::FleetStreams;
use futures_util::StreamExt as _;
use serde::Serialize;
use sqlx::{PgConnection, Postgres, Transaction};

use crate::Result;

/// The history-only event type, distinct from every runnable trigger type.
pub const EVENT_TYPE_ACTIVITY: &str = "activity";
/// Receipts describe system facts rather than messages from a person or runner.
pub const ACTOR_ACTIVITY: &str = "system";
const MAX_CONCURRENT_ACTIVITY_NOTICES: usize = 16;

/// Unpublished recipients produced by a receipt write; consumed by [`commit`].
#[derive(Debug, Default)]
#[must_use = "commit the receipt transaction with its announcement"]
pub struct Announcement {
    fleets: Vec<String>,
}

impl Announcement {
    /// Combines receipt writes owned by the same transaction.
    pub fn append(&mut self, other: Self) {
        self.fleets.extend(other.fleets);
    }
}

/// Commits receipt writes, releases their owned connection, then notifies open tails.
///
/// # Errors
/// A refused commit publishes nothing. Notification failures leave history durable.
pub async fn commit(
    transaction: Transaction<'static, Postgres>,
    mut announcement: Announcement,
    streams: Option<&FleetStreams>,
) -> Result<(), sqlx::Error> {
    transaction.commit().await?;
    announcement.fleets.sort_unstable();
    announcement.fleets.dedup();
    if let Some(streams) = streams {
        futures_util::stream::iter(announcement.fleets)
            .for_each_concurrent(MAX_CONCURRENT_ACTIVITY_NOTICES, |fleet| async move {
                announce(streams, &fleet).await;
            })
            .await;
    }
    Ok(())
}

/// Metadata permitted in history. No credential body or instruction text fits.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Activity<'a> {
    /// An immutable setup revision committed.
    SetupSaved {
        /// The saved decimal revision.
        revision: &'a str,
    },
    /// The setup's checked activation committed.
    SetupReady {
        /// The checked decimal revision.
        revision: &'a str,
    },
    /// Access was stored; this does not claim the provider accepted it.
    AccessSaved {
        /// The required credential name, never its value.
        service: &'a str,
    },
    /// The fleet's two workspace memory permissions changed.
    MemorySharingChanged {
        /// Whether the fleet can read workspace memories.
        read: bool,
        /// Whether it can publish workspace memories.
        publish: bool,
    },
}

/// Adds a receipt on the mutation's connection, never on a queue.
///
/// `identity` is stable for the mutation, so a repeated write converges.
/// The caller adds its operation context to a statement refusal.
///
/// # Errors
/// Returns the database refusal; the caller's transaction must roll back with it.
pub async fn record_on(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    identity: &str,
    activity: Activity<'_>,
) -> Result<Announcement, sqlx::Error> {
    let (inserted, _id): (bool, String) = sqlx::query_as(crate::sql::INSERT_FLEET_EVENT)
        .bind(fleet.as_str())
        .bind(identity)
        .bind(workspace.as_str())
        .bind(ACTOR_ACTIVITY)
        .bind(EVENT_TYPE_ACTIVITY)
        .bind(sqlx::types::Json(activity))
        .bind(None::<&str>)
        .bind(RECORDED)
        .fetch_one(connection)
        .await?;
    Ok(Announcement {
        fleets: if inserted {
            vec![fleet.to_string()]
        } else {
            Vec::new()
        },
    })
}

/// Records access storage for the workspace's saved fleets that require it.
///
/// # Errors
/// Returns the statement refusal so storing access and its receipt roll back together.
pub async fn record_access_on(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    service: &str,
) -> Result<Announcement, sqlx::Error> {
    let fleets = sqlx::query_scalar(crate::sql::INSERT_ACCESS_ACTIVITY)
        .bind(workspace.as_str())
        .bind(service)
        .bind(ACTOR_ACTIVITY)
        .bind(EVENT_TYPE_ACTIVITY)
        .bind(RECORDED)
        .bind(sqlx::types::Json(Activity::AccessSaved { service }))
        .fetch_all(connection)
        .await?;
    Ok(Announcement { fleets })
}

/// Open tails recover committed history through their existing catch-up path.
async fn announce(streams: &FleetStreams, fleet: &str) {
    streams
        .publish_frame(fleet, &Notice::CatchingUp { dropped: 0 })
        .await;
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Notice {
    CatchingUp { dropped: u64 },
}
