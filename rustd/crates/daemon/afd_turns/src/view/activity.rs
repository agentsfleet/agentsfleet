//! Durable receipts join the fleet history without admitting or executing work.

use af_core::{id::Uuid7, turn::status::RECORDED};
use afd_api_wire::activity::FleetActivity;
use afd_bus::Handle;
use futures_util::StreamExt as _;
use serde::Serialize;
use sqlx::{PgConnection, Postgres, Transaction};

use crate::view::Result;

/// The history-only event type, distinct from every runnable trigger type.
pub const EVENT_TYPE_ACTIVITY: &str = "activity";
/// Receipts describe system facts rather than messages from a person or runner.
pub const ACTOR_ACTIVITY: &str = "system";
/// Fleets told about one commit's receipts at once, when
/// `[server] max_live_activity_notices` names nothing else.
pub const DEFAULT_ACTIVITY_NOTICES: usize = 16;

/// Where receipts are announced, and how many fleets are told at once.
#[derive(Debug, Clone)]
pub struct Notices {
    streams: Handle,
    fanout: usize,
}

impl Notices {
    /// Announces on `streams`, telling at most `fanout` fleets at once.
    #[must_use]
    pub const fn new(streams: Handle, fanout: usize) -> Self {
        Self { streams, fanout }
    }

    /// The bus the announcements go out on.
    #[must_use]
    pub const fn streams(&self) -> &Handle {
        &self.streams
    }

    /// How many fleets are told at once.
    #[must_use]
    pub const fn fanout(&self) -> usize {
        self.fanout
    }
}

/// Unpublished recipients produced by a receipt write; consumed by [`commit`].
#[derive(Debug, Default)]
#[must_use = "commit the receipt transaction with its announcement"]
pub struct Announcement {
    fleets: Vec<Uuid7>,
    /// Workspaces whose set of fleets this transaction changed.
    workspaces: std::collections::BTreeSet<Uuid7>,
    /// Setup drafts whose durable state this transaction changed.
    drafts: std::collections::BTreeSet<Uuid7>,
}

impl Announcement {
    /// Combines receipt writes owned by the same transaction.
    pub fn append(&mut self, other: Self) {
        self.fleets.extend(other.fleets);
        self.workspaces.extend(other.workspaces);
        self.drafts.extend(other.drafts);
    }

    /// Announces after the commit that `draft`'s durable state changed, so its open
    /// setup page reads it again.
    pub fn draft_changed(&mut self, draft: &Uuid7) {
        self.drafts.insert(draft.clone());
    }

    /// Announces after the commit that `workspace` gained or lost a fleet, so its
    /// open walls re-read the set.
    pub fn fleet_set(&mut self, workspace: &Uuid7) {
        self.workspaces.insert(workspace.clone());
    }
}

/// Commits receipt writes, releases their owned connection, then notifies open tails.
///
/// # Errors
/// A refused commit publishes nothing. Notification failures leave history durable.
pub async fn commit(
    transaction: Transaction<'static, Postgres>,
    mut announcement: Announcement,
    notices: Option<&Notices>,
) -> Result<(), sqlx::Error> {
    transaction.commit().await?;
    announcement.fleets.sort_unstable();
    announcement.fleets.dedup();
    if let Some(notices) = notices {
        for workspace in &announcement.workspaces {
            fleets_changed(notices.streams(), workspace).await;
        }
        for draft in &announcement.drafts {
            draft_changed(notices.streams(), draft).await;
        }
        let streams = notices.streams();
        futures_util::stream::iter(announcement.fleets)
            .for_each_concurrent(notices.fanout(), async move |fleet| {
                announce(streams, &fleet).await;
            })
            .await;
    }
    Ok(())
}

/// Adds a receipt on the mutation's connection, never on a queue.
///
/// Postgres mints the receipt's identity, a fresh `UUIDv7`, so a repeated write
/// records again (owner's call, Nov 08, 2026). The caller adds its operation
/// context to a statement refusal.
///
/// # Errors
/// Returns the database refusal; the caller's transaction must roll back with it.
pub async fn record_on(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    activity: FleetActivity<'_>,
) -> Result<Announcement, sqlx::Error> {
    sqlx::query(crate::view::sql::INSERT_TURN)
        .bind(fleet)
        .bind(None::<&Uuid7>)
        .bind(workspace)
        .bind(ACTOR_ACTIVITY)
        .bind(EVENT_TYPE_ACTIVITY)
        .bind(sqlx::types::Json(activity))
        .bind(None::<&Uuid7>)
        .bind(RECORDED)
        .execute(connection)
        .await?;
    // A fresh identity never meets a conflict, so every write is a new receipt.
    Ok(Announcement {
        fleets: vec![fleet.clone()],
        workspaces: std::collections::BTreeSet::new(),
        drafts: std::collections::BTreeSet::new(),
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
    let fleets = sqlx::query_scalar(crate::view::sql::INSERT_ACCESS_ACTIVITY)
        .bind(workspace)
        .bind(service)
        .bind(ACTOR_ACTIVITY)
        .bind(EVENT_TYPE_ACTIVITY)
        .bind(RECORDED)
        .bind(sqlx::types::Json(FleetActivity::AccessSaved {
            service: service.into(),
        }))
        .fetch_all(connection)
        .await?;
    Ok(Announcement {
        fleets,
        workspaces: std::collections::BTreeSet::new(),
        drafts: std::collections::BTreeSet::new(),
    })
}

/// Tells `workspace`'s open walls, after the commit that changed it, that its set of
/// fleets changed. A lost notice is owed as a gap, which re-reads every wall.
pub async fn fleets_changed(streams: &Handle, workspace: &Uuid7) {
    let subject = afd_bus::Subject::Fleets(workspace.clone());
    let publication = streams.publish(&subject, "").await;
    afd_bus::observe_notification(&subject, publication);
}

/// A setup draft's durable state changed: its open page reads it again. Carries the
/// draft's id and nothing else, on the draft's own items subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftChanged {
    /// The draft whose state changed.
    pub changed: Uuid7,
}

/// Tells `draft`'s open setup page, after the commit that changed it, to read it
/// again. A lost notice is owed as a gap, which re-reads every page.
pub async fn draft_changed(streams: &Handle, draft: &Uuid7) {
    let subject = afd_bus::Subject::ThreadItems(draft.clone());
    let notice = DraftChanged {
        changed: draft.clone(),
    };
    let publication = streams.publish_frame(&subject, &notice).await;
    afd_bus::observe_notification(&subject, publication);
}

/// Notify open tails after durable history commits; failure preserves reload.
pub async fn announce(streams: &Handle, fleet: &Uuid7) {
    let subject = afd_bus::Subject::ThreadItems(fleet.clone());
    let publication = streams
        .publish_frame(&subject, &Notice::CatchingUp { dropped: 0 })
        .await;
    afd_bus::observe_notification(&subject, publication);
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Notice {
    CatchingUp { dropped: u64 },
}
