//! Installing a fleet: the row, the stream, and the promise that both exist.
//!
//! # The guarantee
//!
//! When [`Fleets::install`] answers `Ok`, the `core.fleets` row exists AND the
//! per-fleet event stream and its consumer group exist. An event published a
//! millisecond later finds the group the lease `XREADGROUP` needs. Redis gets
//! four attempts across [`stream_backoff`], jittered so concurrent installs do
//! not retry in step; if it never answers, the Postgres row is
//! deleted and the caller is told nothing was created — a promise they can act
//! on, because retrying is then safe.
//!
//! # The flip happens here, not on a detached worker
//!
//! `create.zig` spawns a thread that sleeps, publishes two cosmetic frames,
//! flips `installing` → `active`, then publishes a third. Its own comment says
//! a failed spawn is survivable because "a later list/detail reconcile flips
//! it". That reconcile does not exist — `list.zig` and `get.zig` are pure
//! reads. So a spawn failure, or a restart between the 201 and the flip, strands
//! the fleet in `installing`, where the runner's candidate query
//! (`status = 'active'`) can never see it, permanently.
//!
//! Here the flip is part of the pipeline, under the same rollback as the
//! stream: a fleet is installed or it is not. Two consequences, both declared in
//! the milestone's Discovery log. The 201 reports `active` rather than
//! `installing`, and the cosmetic `install:*` frames are not emitted — they are
//! an event-surface concern and land with §5, which owns the stream they would
//! be published on.

mod authored;
mod row;

use std::collections::BTreeSet;
use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_fleet_runtime::FleetName;

use backon::{ExponentialBuilder, Retryable as _};

use crate::error::{self, ErrorKind, Result};
use crate::{FleetStatus, Fleets, sql};

/// What the stream setup waits between attempts, and how many it gets.
///
/// `backon`'s builder rather than a fixed table: the Zig's `[100, 500, 1500]`
/// means every install racing the same struggling Redis retries in the same
/// millisecond — the reconnect storm that keeps it down — and `with_jitter`
/// spreads them.
///
/// Doubling from 200ms, capped at 1500ms: three sleeps come to 1.4s before
/// jitter, inside the 2.1s wall the Zig documents. The first retry lands
/// sooner, so a Redis that blips for 150ms is caught on the second try instead
/// of after 600ms of waiting.
///
/// `max_times` is RETRIES, one fewer than the attempts, and it is derived from
/// [`STREAM_ATTEMPTS`] rather than written as a number. That direction is the
/// one that cannot go wrong: the Zig wrote its sleeps down and derived the
/// count with `attempt + 1 >= len`, leaving its last entry unreachable while
/// the comment beside it promised four tries.
fn stream_backoff() -> ExponentialBuilder {
    ExponentialBuilder::new()
        .with_min_delay(Duration::from_millis(200))
        .with_max_delay(Duration::from_millis(1500))
        .with_max_times(STREAM_ATTEMPTS - 1)
        .with_jitter()
}

/// How many times the stream setup is tried before the install gives up.
///
/// Four attempts means three sleeps. The count is the constant and the sleeps
/// are derived from it, which is the direction that cannot go wrong: the Zig
/// wrote the sleeps down and derived the count with `attempt + 1 >= len`,
/// leaving its last entry unreachable while the comment beside it promised four
/// tries — and it shipped that way until a reviewer caught it.
const STREAM_ATTEMPTS: usize = 4;

/// The context the activation flip reports a failed statement under.
const CONTEXT_ACTIVATE: &str = "activate installed fleet";

/// The context the rollback reports a failed statement under.
const CONTEXT_ROLLBACK: &str = "roll back installed fleet";

/// The hint a rollback that itself failed leaves for an operator to grep.
const HINT_ORPHANED: &str = "row_orphaned_manual_recovery";

/// Which library tier an install draws its bundle from.
///
/// An enum, so "exactly one of `platform_library_id` and `tenant_library_id`"
/// is a property of the type rather than two optional fields and two runtime
/// checks. Both the neither-set and the both-set case are refused once, where
/// the body is read, and nothing downstream can be handed an ambiguous pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibrarySource<'a> {
    /// A published platform entry, by slug.
    Platform(&'a str),
    /// This workspace's own entry, by identifier.
    Tenant(Uuid7),
}

/// One install request, already parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install<'a> {
    /// Where the bundle comes from.
    pub source: LibrarySource<'a>,
    /// The operator's name for this instance, when they chose one.
    ///
    /// Already a [`FleetName`], so the slug rules were checked at the edge and
    /// this path carries no validation arm. Absent means the bundle's own name,
    /// re-drawn with a suffix if it collides.
    pub name: Option<FleetName>,
}

/// What an install answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The new fleet's identifier.
    pub id: Uuid7,
    /// The name it was stored under, chosen or drawn.
    pub name: String,
    /// Where it stands — [`FleetStatus::Active`]; see the module note.
    pub status: FleetStatus,
    /// The webhook providers it declared, for the caller to build URLs from.
    ///
    /// Sources rather than URLs: the deployment's own base is configuration the
    /// HTTP edge holds, and a store formatting a URL would have to be told what
    /// host this daemon answers on.
    pub webhook_sources: Vec<Box<str>>,
}

impl Fleets {
    /// Refuses a write whose configuration names credentials this workspace
    /// lacks.
    ///
    /// # Why before the row and not after
    ///
    /// A fleet whose configuration names a credential nobody stored is a row
    /// that exists and cannot run: the first lease reaches for the secret, and
    /// the operator meets the failure at run time with the fleet already in
    /// their list. `create_fleet_bundle.zig` decides the same thing in the same
    /// place.
    ///
    /// # Why both writes ask
    ///
    /// The install is not the only door. `PATCH` takes a replacement
    /// `TRIGGER.md`, and that document declares credentials too, so a check on
    /// the install alone leaves the same unrunnable fleet one `fleet update`
    /// away. This is `pub(crate)` for exactly that reason.
    ///
    /// # Why the names and not the values
    ///
    /// The vault is asked what it HOLDS, never what those secrets are. The
    /// answer this needs is a set difference over names, and reading a value to
    /// compute it would put plaintext in a path that has no use for it.
    pub(crate) async fn require_the_stored_credentials(
        &self,
        workspace: &Uuid7,
        declared: &[String],
    ) -> Result<()> {
        if declared.is_empty() {
            return Ok(());
        }
        let held = self.secrets.list(workspace).await?;
        let stored: BTreeSet<&str> = held.iter().map(|secret| secret.name.as_str()).collect();
        // A set on both sides: a document is free to declare one credential
        // twice, and a refusal listing it twice reads as two missing secrets.
        let wanted: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
        let missing: Vec<String> = wanted
            .into_iter()
            .filter(|name| !stored.contains(name))
            .map(ToOwned::to_owned)
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        Err(ErrorKind::BundleSecretsMissing { missing }.into())
    }

    /// Installs one fleet into `workspace`, stream and all.
    ///
    /// # Errors
    /// Refuses a library id naming nothing installable here, either document
    /// being unusable, the two naming different fleets, placement tags outside
    /// their bounds, and a CHOSEN name this workspace already holds. Reports an
    /// install whose stream could not be created — with the row removed — and a
    /// datastore that would not answer.
    pub async fn install(
        &self,
        workspace: &Uuid7,
        request: &Install<'_>,
        now: UnixMillis,
    ) -> Result<Installed> {
        let mut connection = self.database.acquire().await?;
        let entry = self
            .resolve(&mut connection, workspace, &request.source)
            .await?;
        let authored = authored::read(entry)?;
        // Released BEFORE the vault read, and reacquired for the write. The
        // pre-flight takes a pool connection of its own, so holding this one
        // across it would let N concurrent installs each hold one and wait for
        // another — the pool exhausting itself on a check that needs no
        // transaction, which is the trade the stream setup below already makes
        // for the same reason.
        drop(connection);
        let declared: Vec<String> = authored
            .trigger
            .config()
            .credentials()
            .iter()
            .map(ToString::to_string)
            .collect();
        self.require_the_stored_credentials(workspace, &declared)
            .await?;
        let mut connection = self.database.acquire().await?;

        let id = self.mint_id(now)?;
        let name = self
            .insert_with_retry(&mut connection, workspace, &id, &authored, request, now)
            .await?;
        // Released before Redis is touched. The stream setup can spend two
        // seconds, and holding a pool connection across it is how a slow queue
        // becomes a Postgres outage.
        drop(connection);

        match self.finish(workspace, &id, now).await {
            Ok(()) => {
                // A workspace wall may already hold the pre-install set. Drop
                // it only after both datastores and the active row agree, so
                // the next refresh observes the committed fleet immediately.
                self.live_sets.invalidate(workspace.as_str()).await;
                Ok(Installed {
                    id,
                    name,
                    status: FleetStatus::Active,
                    webhook_sources: authored.webhook_sources(),
                })
            }
            Err(unfinished) => Err(self.roll_back(workspace, &id, &unfinished).await),
        }
    }

    /// The stream, its group, and the flip that makes the fleet leasable.
    ///
    /// One function because they share one failure policy: either both happen
    /// or the row goes back. Splitting them would invite a caller to do the
    /// first and skip the second, which is the stranding the module note
    /// describes.
    async fn finish(&self, workspace: &Uuid7, id: &Uuid7, now: UnixMillis) -> Result<()> {
        self.ensure_stream(id.as_str()).await?;
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::UPDATE_FLEET_STATUS)
            .bind(FleetStatus::Active.as_str())
            .bind(now.as_millis())
            .bind(id.as_str())
            .bind(workspace.as_str())
            .bind(FleetStatus::Installing.as_str())
            .execute(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_ACTIVATE))?;
        Ok(())
    }

    /// Creates the stream and its consumer group, or spends the whole schedule.
    ///
    /// The loop AND the schedule are `backon`'s. The half most worth handing
    /// over is the loop guard: the Zig's shipped bug was `attempt + 1 >= len`,
    /// which left the final delay unreachable while the comment beside it
    /// promised four tries.
    ///
    /// `when` is what makes the retry mean something. Only a TRANSPORT failure
    /// is retried; a Redis that answered and refused the command will refuse it
    /// three more times, and a misconfigured deployment will still be
    /// misconfigured in 1.75 seconds. Spending the budget on either makes a
    /// person wait out a foregone conclusion.
    async fn ensure_stream(&self, fleet: &str) -> Result<()> {
        (|| async { self.streams.ensure_group(fleet).await })
            .retry(stream_backoff())
            .when(afd_redis::Error::is_unavailable)
            .notify(|failure: &afd_redis::Error, delay: Duration| {
                let sleep_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
                let reason = failure.to_string();
                tracing::warn!(
                    error_code = error_code::INTERNAL_OPERATION_FAILED.as_str(),
                    fleet,
                    sleep_ms,
                    reason,
                    event = "install_stream_retry",
                );
            })
            .await
            .map_err(Into::into)
    }

    /// Deletes the row an install could not finish, on a FRESH connection.
    ///
    /// Fresh because the request's was released before Redis was reached, and
    /// because a rollback queued behind the same exhausted pool would fail for
    /// the reason it is running.
    ///
    /// Answers [`ErrorKind::InstallRolledBack`] either way. A rollback that
    /// itself fails leaves an orphan row and logs a hint an operator can grep,
    /// but the CALLER is in the same position regardless: the fleet is unusable
    /// and retrying is the only move they have.
    async fn roll_back(&self, workspace: &Uuid7, id: &Uuid7, cause: &crate::Error) -> crate::Error {
        let fleet = id.as_str();
        let reason = cause.to_string();
        let removed = async {
            let mut connection = self.database.acquire().await?;
            sqlx::query(sql::DELETE_FLEET)
                .bind(fleet)
                .bind(workspace.as_str())
                .execute(connection.as_mut())
                .await
                .map_err(error::query(CONTEXT_ROLLBACK))?;
            Ok::<(), crate::Error>(())
        }
        .await;

        match removed {
            Ok(()) => tracing::warn!(
                error_code = error_code::AGENTSFLEET_INSTALL_ROLLED_BACK.as_str(),
                fleet,
                reason,
                event = "install_rolled_back",
            ),
            Err(double_fault) => {
                let rollback_error = double_fault.to_string();
                tracing::error!(
                    error_code = error_code::AGENTSFLEET_INSTALL_ROLLED_BACK.as_str(),
                    fleet,
                    reason,
                    rollback_error,
                    hint = HINT_ORPHANED,
                    event = "install_rollback_failed",
                );
            }
        }
        ErrorKind::InstallRolledBack.into()
    }

    /// Draws a fresh fleet identifier.
    fn mint_id(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}
