//! What may wake a fleet besides its schedules: its `fleet.triggers` rows.
//!
//! A run adds, lists and removes them through its fenced verbs (`afd_fleet`),
//! which prove the run and check the request before they call [`insert`],
//! [`list`] and [`remove_on`] on their own transaction. A person lists and
//! removes them through [`Creations`]. A row is never edited: a change is a
//! remove and an add.

mod sql;

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::trigger_verb::TriggerKind;
use afd_api_wire::fleet::Trigger;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

use crate::{
    Access, Creations, Result, access,
    error::raise::{query, stored},
};

const CONTEXT: &str = "read or change a fleet's triggers";

/// The operator log line a trigger row's add or remove writes.
pub const EVENT_TRIGGER_CHANGED: &str = "trigger_changed";

/// Who changed a trigger, as [`EVENT_TRIGGER_CHANGED`] names it.
pub const CHANGED_BY_PERSON: &str = "person";

/// One trigger as its row holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerRow {
    /// The row's identity: what a remove names.
    pub id: Uuid7,
    /// What wakes the fleet.
    pub kind: TriggerKind,
    /// A webhook's source slug, `github` for an App event, `slack` for a mention.
    pub source: String,
    /// The event names that wake the fleet; `None` wakes it on every event.
    pub events: Option<Vec<String>>,
    /// The repositories whose deliveries wake the fleet, as `owner/name`.
    pub repositories: Option<Vec<String>>,
    /// A webhook's signing recipe, for a source outside the built-in registry.
    pub verifier_scheme: Option<String>,
    /// The header that recipe reads its signature from.
    pub verifier_header: Option<String>,
    /// The workspace vault key of a webhook's signing secret.
    pub secret_name: Option<String>,
    /// A mention's channel identifier.
    pub channel: Option<String>,
    /// When the fleet added it.
    pub created_at: Timestamp,
}

impl TriggerRow {
    /// The row as a person and a fleet read it. A webhook carries the address
    /// its deliveries are sent to on `deployment`; no kind carries its
    /// signing secret's name or its recipe.
    #[must_use]
    pub fn shown(&self, deployment: &str, fleet: &Uuid7) -> Trigger<'_> {
        let url = (self.kind == TriggerKind::Webhook)
            .then(|| Cow::Owned(format!("{deployment}/v1/webhooks/{fleet}/{}", self.source)));
        Trigger {
            id: Cow::Owned(self.id.to_string()),
            kind: self.kind,
            source: Cow::Borrowed(&self.source),
            events: self.events.as_deref().map(borrowed),
            repositories: self.repositories.as_deref().map(borrowed),
            channel: self.channel.as_deref().map(Cow::Borrowed),
            url,
            created_at: self.created_at,
        }
    }
}

fn borrowed(items: &[String]) -> Vec<Cow<'_, str>> {
    items
        .iter()
        .map(|item| Cow::Borrowed(item.as_str()))
        .collect()
}

/// One trigger to write, already checked by its caller.
#[derive(Debug, Clone, Copy)]
pub struct NewTrigger<'a> {
    /// The row's freshly minted identity.
    pub id: &'a Uuid7,
    /// The fleet it wakes.
    pub fleet: &'a Uuid7,
    /// The fleet's workspace, which every delivery route matches on.
    pub workspace: &'a Uuid7,
    /// What wakes the fleet.
    pub kind: TriggerKind,
    /// See [`TriggerRow::source`].
    pub source: &'a str,
    /// See [`TriggerRow::events`].
    pub events: Option<&'a [String]>,
    /// See [`TriggerRow::repositories`].
    pub repositories: Option<&'a [String]>,
    /// See [`TriggerRow::verifier_scheme`].
    pub verifier_scheme: Option<&'a str>,
    /// See [`TriggerRow::verifier_header`].
    pub verifier_header: Option<&'a str>,
    /// See [`TriggerRow::secret_name`].
    pub secret_name: Option<&'a str>,
    /// See [`TriggerRow::channel`].
    pub channel: Option<&'a str>,
}

/// [`sql::LIST`]'s and [`sql::INSERT`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Stored {
    id: Uuid7,
    kind: String,
    source: String,
    events: Option<Vec<String>>,
    repositories: Option<Vec<String>>,
    verifier_scheme: Option<String>,
    verifier_header: Option<String>,
    secret_name: Option<String>,
    channel: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

impl Stored {
    fn decode(self) -> Result<TriggerRow> {
        Ok(TriggerRow {
            kind: stored_kind(&self.kind)?,
            id: self.id,
            source: self.source,
            events: self.events,
            repositories: self.repositories,
            verifier_scheme: self.verifier_scheme,
            verifier_header: self.verifier_header,
            secret_name: self.secret_name,
            channel: self.channel,
            created_at: self.created_at,
        })
    }
}

/// The kind a stored spelling names; any other text is a stored fault.
fn stored_kind(text: &str) -> Result<TriggerKind> {
    serde_json::from_value(serde_json::Value::String(text.to_owned())).map_err(stored)
}

/// `fleet`'s triggers, oldest first, on the caller's connection.
///
/// # Errors
/// A statement that would not run, or a stored kind this build cannot read.
pub async fn list(connection: &mut PgConnection, fleet: &Uuid7) -> Result<Vec<TriggerRow>> {
    let rows: Vec<Stored> = sqlx::query_as(sql::LIST)
        .bind(fleet)
        .fetch_all(connection)
        .await
        .map_err(query(CONTEXT))?;
    rows.into_iter().map(Stored::decode).collect()
}

/// Writes `new`, or answers `None` when the fleet already holds a trigger of
/// its kind and source.
///
/// # Errors
/// A statement that would not run.
pub async fn insert(
    connection: &mut PgConnection,
    new: NewTrigger<'_>,
) -> Result<Option<TriggerRow>> {
    let row: Option<Stored> = sqlx::query_as(sql::INSERT)
        .bind(new.id)
        .bind(new.fleet)
        .bind(new.workspace)
        .bind(new.kind.as_str())
        .bind(new.source)
        .bind(new.events)
        .bind(new.repositories)
        .bind(new.verifier_scheme)
        .bind(new.verifier_header)
        .bind(new.secret_name)
        .bind(new.channel)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.map(Stored::decode).transpose()
}

/// Removes `trigger` from `fleet` in `workspace`, answering the removed kind,
/// or `None` when the fleet holds no such trigger.
///
/// # Errors
/// A statement that would not run, or a stored kind this build cannot read.
pub async fn remove_on(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    trigger: &Uuid7,
) -> Result<Option<TriggerKind>> {
    let kind: Option<String> = sqlx::query_scalar(sql::REMOVE)
        .bind(trigger)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    kind.as_deref().map(stored_kind).transpose()
}

impl Creations {
    /// `fleet`'s triggers as a member of its workspace reads them.
    ///
    /// # Errors
    /// A caller who is not a current member, a fleet outside the workspace,
    /// and datastore failures.
    pub async fn triggers(&self, access: Access<'_>, fleet: &Uuid7) -> Result<Vec<TriggerRow>> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::unlocked(&mut tx, access).await?;
        crate::creation::exists(&mut tx, access.workspace, fleet).await?;
        let rows = list(&mut tx, fleet).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(rows)
    }

    /// Removes one of `fleet`'s triggers for a member of its workspace;
    /// `false` when the fleet holds no such trigger.
    ///
    /// # Errors
    /// A caller who is not a current member, a fleet outside the workspace,
    /// and datastore failures.
    pub async fn remove_trigger(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        trigger: &Uuid7,
    ) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::current(&mut tx, access).await?;
        crate::creation::exists(&mut tx, access.workspace, fleet).await?;
        let removed = remove_on(&mut tx, access.workspace, fleet, trigger).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        if let Some(kind) = removed {
            let (event, fleet_id, kind, by) = (
                EVENT_TRIGGER_CHANGED,
                fleet.as_str(),
                kind.as_str(),
                CHANGED_BY_PERSON,
            );
            tracing::info!(event, fleet_id, kind, by, change = "removed");
        }
        Ok(removed.is_some())
    }
}

#[cfg(test)]
mod tests;
