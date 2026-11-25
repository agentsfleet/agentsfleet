//! A run adding, listing and removing what may wake its fleet: the domain
//! half of `POST`, `GET` and `DELETE L/triggers`.
//!
//! The fleet is the run's, never named by the request. An add is checked
//! against what the workspace holds — the provider an App event or a mention
//! needs connected, the signing secret a webhook names in the vault — and its
//! shape is read by the one trigger parser, then written under the run's write
//! fence. A source takes one row per kind, so a second webhook for one source
//! is refused. The row takes the call's item as its id, so a retried add whose
//! write committed answers the row it wrote.

use af_core::id::Uuid7;
use af_fleet_runtime::config::{WEBHOOK_SOURCE_APPROVAL, WEBHOOK_SOURCE_GITHUB, parse_triggers};
use af_wire::trigger_verb::{TriggerAddRequest, TriggerKind};
use afd_db::Precondition as _;
use afd_setup::trigger::{self as store, NewTrigger, TriggerRow};
use serde_json::{Map, Value, json};
use sqlx::Connection as _;

use crate::error::{Result, query, rejected, stale_fence};
use crate::run::access::provider_connected;
use crate::run::pull::RunService;
use crate::run::refusal::FleetRefusal;
use crate::run::standing::Standing;
use crate::run::write_fence::WriteFence;

const CONTEXT: &str = "change a fleet's triggers";

/// The operator line a trigger row's add or remove writes.
const EVENT_TRIGGER_CHANGED: &str = store::EVENT_TRIGGER_CHANGED;

/// Who changed a trigger, as [`EVENT_TRIGGER_CHANGED`] names it.
const CHANGED_BY_RUN: &str = "run";

/// The source an App event and a mention take, and the provider each needs.
const SOURCE_APP_EVENT: &str = "github";
const SOURCE_MENTION: &str = "slack";

/// Why an add was refused for its shape.
const DETAIL_RESERVED_SOURCE: &str =
    "a webhook source may not be `approval` or `github`; GitHub events are an app_event trigger";
/// See [`DETAIL_RESERVED_SOURCE`].
const DETAIL_SOURCE_NEEDED: &str = "a webhook trigger names its source";
/// See [`DETAIL_RESERVED_SOURCE`].
const DETAIL_WRONG_SOURCE: &str =
    "an app_event trigger's source is `github`, and a mention trigger's `slack`";
/// See [`DETAIL_RESERVED_SOURCE`].
const DETAIL_CHANNEL_NEEDED: &str = "a mention trigger names its channel";
/// See [`DETAIL_RESERVED_SOURCE`].
const DETAIL_SHAPE: &str = "the trigger's source, events, repositories, verifier or channel is not one the fleet can be woken by";

/// An add, checked and ready to write.
struct Checked<'a> {
    kind: TriggerKind,
    source: &'a str,
}

impl RunService {
    /// Adds the trigger `request` names to the run's fleet, as the row
    /// `item_id`.
    ///
    /// # Errors
    /// A provider the workspace has not connected (`UZ-AGT-023`), a signing
    /// secret the vault does not hold (`UZ-AGT-025`), a source or shape the
    /// parser refuses, a kind and source the fleet already holds
    /// (`UZ-AGT-024`), a run superseded before the write, and datastore
    /// failures. A refusal writes no row.
    pub async fn add_trigger(
        &self,
        standing: &Standing,
        item_id: &Uuid7,
        request: &TriggerAddRequest<'_>,
        guard: &WriteFence,
    ) -> Result<TriggerRow> {
        let checked = check(request)?;
        let mut connection = self.runs.pool().acquire().await?;
        self.connected(&mut connection, standing, &checked, request)
            .await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        if !guard.holds(&mut tx).await.map_err(query(CONTEXT))? {
            return Err(stale_fence());
        }
        let events: Option<Vec<String>> = request
            .events
            .as_ref()
            .map(|events| events.iter().map(ToString::to_string).collect());
        let repositories: Option<Vec<String>> = request
            .repositories
            .as_ref()
            .map(|repositories| repositories.iter().map(ToString::to_string).collect());
        let verifier = request.verifier.as_ref();
        let written = store::insert(
            &mut tx,
            NewTrigger {
                id: item_id,
                fleet: &standing.fleet_id,
                workspace: &standing.workspace_id,
                kind: checked.kind,
                source: checked.source,
                events: events.as_deref(),
                repositories: repositories.as_deref(),
                verifier_scheme: verifier.map(|verifier| verifier.scheme.as_ref()),
                verifier_header: verifier.and_then(|verifier| verifier.header.as_deref()),
                secret_name: request.secret_name.as_deref(),
                channel: request.channel.as_deref(),
            },
        )
        .await?;
        let row = match written {
            Some(row) => row,
            None => replayed(&mut tx, standing, item_id).await?,
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        log_changed(standing, row.kind, "added");
        Ok(row)
    }

    /// The run's fleet's triggers, oldest first.
    ///
    /// # Errors
    /// Datastore failures.
    pub async fn triggers(&self, standing: &Standing) -> Result<Vec<TriggerRow>> {
        let mut connection = self.runs.pool().acquire().await?;
        Ok(store::list(&mut connection, &standing.fleet_id).await?)
    }

    /// Removes one of the run's fleet's triggers; `false` when it holds no
    /// such trigger.
    ///
    /// # Errors
    /// A run superseded before the write, and datastore failures.
    pub async fn remove_trigger(
        &self,
        standing: &Standing,
        trigger: &Uuid7,
        guard: &WriteFence,
    ) -> Result<bool> {
        let mut connection = self.runs.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        if !guard.holds(&mut tx).await.map_err(query(CONTEXT))? {
            return Err(stale_fence());
        }
        let removed =
            store::remove_on(&mut tx, &standing.workspace_id, &standing.fleet_id, trigger).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        if let Some(kind) = removed {
            log_changed(standing, kind, "removed");
        }
        Ok(removed.is_some())
    }

    /// Refuses an add whose provider the workspace has not connected, or
    /// whose signing secret the vault does not hold.
    async fn connected(
        &self,
        connection: &mut sqlx::PgConnection,
        standing: &Standing,
        checked: &Checked<'_>,
        request: &TriggerAddRequest<'_>,
    ) -> Result<()> {
        match checked.kind {
            TriggerKind::Webhook => {
                let secret = request.secret_name.as_deref().unwrap_or(checked.source);
                self.vault
                    .find_on(connection, &standing.workspace_id, secret)
                    .await?
                    .map(drop)
                    .ok_or_else(|| FleetRefusal::TriggerSecretNotFound.into())
            }
            TriggerKind::AppEvent | TriggerKind::Mention => {
                if provider_connected(connection, &standing.workspace_id, checked.source).await? {
                    Ok(())
                } else {
                    Err(FleetRefusal::ProviderNotConnected.into())
                }
            }
        }
    }
}

/// The add's kind and source, its shape read by the trigger parser.
fn check<'a>(request: &'a TriggerAddRequest<'a>) -> Result<Checked<'a>> {
    let source = match request.kind {
        TriggerKind::Webhook => {
            let source = request
                .source
                .as_deref()
                .ok_or_else(|| rejected(DETAIL_SOURCE_NEEDED))?;
            if source == WEBHOOK_SOURCE_APPROVAL || source == WEBHOOK_SOURCE_GITHUB {
                return Err(rejected(DETAIL_RESERVED_SOURCE));
            }
            source
        }
        TriggerKind::AppEvent => fixed(request, SOURCE_APP_EVENT)?,
        TriggerKind::Mention => fixed(request, SOURCE_MENTION)?,
    };
    parse_triggers(&document(request, source)?.to_string())
        .map_err(|_refused| rejected(DETAIL_SHAPE))?;
    Ok(Checked {
        kind: request.kind,
        source,
    })
}

/// The one source a kind takes, refusing any other.
fn fixed<'a>(request: &'a TriggerAddRequest<'a>, source: &'static str) -> Result<&'a str> {
    match request.source.as_deref() {
        None => Ok(source),
        Some(named) if named == source => Ok(named),
        Some(_) => Err(rejected(DETAIL_WRONG_SOURCE)),
    }
}

/// The request as the trigger parser reads a trigger list of one.
fn document(request: &TriggerAddRequest<'_>, source: &str) -> Result<Value> {
    let mut trigger = Map::new();
    if request.kind == TriggerKind::Mention {
        let channel = request
            .channel
            .as_deref()
            .ok_or_else(|| rejected(DETAIL_CHANNEL_NEEDED))?;
        trigger.insert("type".to_owned(), json!("mention"));
        trigger.insert("source".to_owned(), json!(source));
        trigger.insert("channels".to_owned(), json!([channel]));
        return Ok(json!([trigger]));
    }
    trigger.insert("type".to_owned(), json!("webhook"));
    trigger.insert("source".to_owned(), json!(source));
    if let Some(events) = &request.events {
        trigger.insert("events".to_owned(), json!(events));
    }
    if let Some(repositories) = &request.repositories {
        trigger.insert("repositories".to_owned(), json!(repositories));
    }
    if let Some(secret) = &request.secret_name {
        trigger.insert("credential_name".to_owned(), json!(secret));
    }
    if let Some(verifier) = &request.verifier {
        let mut recipe = Map::new();
        recipe.insert("scheme".to_owned(), json!(verifier.scheme));
        if let Some(header) = &verifier.header {
            recipe.insert("header".to_owned(), json!(header));
        }
        trigger.insert("verifier".to_owned(), Value::Object(recipe));
    }
    Ok(json!([trigger]))
}

/// The row a retried add already wrote, or the refusal for a source the
/// fleet holds under another add.
async fn replayed(
    connection: &mut sqlx::PgConnection,
    standing: &Standing,
    item_id: &Uuid7,
) -> Result<TriggerRow> {
    store::list(connection, &standing.fleet_id)
        .await?
        .into_iter()
        .find(|row| row.id == *item_id)
        .ok_or_else(|| FleetRefusal::TriggerExists.into())
}

fn log_changed(standing: &Standing, kind: TriggerKind, change: &'static str) {
    let (event, fleet_id, kind, by) = (
        EVENT_TRIGGER_CHANGED,
        standing.fleet_id.as_str(),
        kind.as_str(),
        CHANGED_BY_RUN,
    );
    tracing::info!(event, fleet_id, kind, by, change);
}

#[cfg(test)]
#[path = "trigger_tests.rs"]
mod tests;
