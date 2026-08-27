//! Installing a fleet: the row, the stream, and the promise that both exist.
//!
//! # The guarantee
//!
//! When [`Fleets::install`] answers `Ok`, the `core.fleets` row exists AND the
//! per-fleet event stream and its consumer group exist. An event published a
//! millisecond later finds the group the lease `XREADGROUP` needs. Redis gets
//! four attempts across [`STREAM_BACKOFF`], jittered so concurrent installs do
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

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_fleet_runtime::FleetName;
use afd_redis::Backoff;

use crate::error::{self, ErrorKind, Result};
use crate::{FleetStatus, Fleets, sql};

/// What the stream setup waits between attempts.
///
/// `afd_redis::Backoff` rather than a fixed table, and rather than a retry
/// crate: it is the workspace's own schedule, it is already the one the pub/sub
/// pump reconnects on, and it JITTERS. The Zig's fixed `[100, 500, 1500]` means
/// every install racing the same struggling Redis retries in the same
/// millisecond — the reconnect storm that keeps it down, which is the reason
/// the hub's schedule spreads its delays in the first place.
///
/// Doubling from 200ms, capped at 1500ms: three sleeps come to 1.4s before
/// jitter and about 1.75s with it, inside the 2.1s wall the Zig documents. The
/// first retry lands sooner, so a Redis that blips for 150ms is caught on the
/// second try instead of after 600ms of waiting.
const STREAM_BACKOFF: Backoff =
    Backoff::new(Duration::from_millis(200), Duration::from_millis(1500));

/// How many times the stream setup is tried before the install gives up.
///
/// Four attempts means three sleeps. The count is the constant and the sleeps
/// are derived from it, which is the direction that cannot go wrong: the Zig
/// wrote the sleeps down and derived the count with `attempt + 1 >= len`,
/// leaving its last entry unreachable while the comment beside it promised four
/// tries — and it shipped that way until a reviewer caught it.
const STREAM_ATTEMPTS: u32 = 4;

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

        let id = self.mint_id(now)?;
        let name = self
            .insert_with_retry(&mut connection, workspace, &id, &authored, request, now)
            .await?;
        // Released before Redis is touched. The stream setup can spend two
        // seconds, and holding a pool connection across it is how a slow queue
        // becomes a Postgres outage.
        drop(connection);

        match self.finish(workspace, &id, now).await {
            Ok(()) => Ok(Installed {
                id,
                name,
                status: FleetStatus::Active,
                webhook_sources: authored.webhook_sources(),
            }),
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
    /// Only a TRANSPORT failure is retried. A Redis that answered and refused
    /// the command will refuse the identical command three more times, and a
    /// deployment whose Redis is misconfigured will still be misconfigured in
    /// 1.75 seconds — spending the budget on either makes a person wait out a
    /// foregone conclusion. That classification is the difference between a
    /// retry that means something and a retry that is ceremony.
    ///
    /// The final attempt does not sleep, which is what makes four attempts
    /// three sleeps: waiting after the last try spends 1.5 seconds answering
    /// nothing.
    async fn ensure_stream(&self, fleet: &str) -> Result<()> {
        let mut last = None;
        for attempt in 0..STREAM_ATTEMPTS {
            let failure = match self.streams.ensure_group(fleet).await {
                Ok(()) => return Ok(()),
                Err(failure) => failure,
            };
            if !failure.is_unavailable() {
                // Answered and refused, or misconfigured. Neither improves by
                // being asked again, and the caller is better served by hearing
                // so now.
                return Err(failure.into());
            }
            if attempt + 1 < STREAM_ATTEMPTS {
                let delay = STREAM_BACKOFF.delay(attempt, self.jitter());
                let sleep_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
                let reason = failure.to_string();
                tracing::warn!(
                    error_code = error_code::INTERNAL_OPERATION_FAILED.as_str(),
                    fleet,
                    attempt = attempt + 1,
                    of = STREAM_ATTEMPTS,
                    sleep_ms,
                    reason,
                    event = "install_stream_retry",
                );
                tokio::time::sleep(delay).await;
            }
            last = Some(failure);
        }
        Err(last.map_or_else(|| ErrorKind::InstallRolledBack.into(), Into::into))
    }

    /// Spread for one backoff delay, so concurrent installs do not retry in step.
    ///
    /// Zero when the host cannot draw entropy, which degrades to the Zig's
    /// lockstep schedule rather than failing an install over a jitter value —
    /// the delay is still correct, it is just no longer spread.
    fn jitter(&self) -> u64 {
        let mut bytes = [0u8; 8];
        self.entropy
            .fill(&mut bytes)
            .map_or(0, |()| u64::from_be_bytes(bytes))
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

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]
    use super::{STREAM_ATTEMPTS, STREAM_BACKOFF};
    use std::time::Duration;

    /// The wall budget `create_stream.zig` documents, which this stays inside.
    const ZIG_WALL_BUDGET: Duration = Duration::from_millis(2100);

    /// The most the jitter may add, as `Backoff::delay` spreads it.
    const JITTER_SHARE: u32 = 4;

    #[test]
    fn the_whole_schedule_fits_inside_the_documented_wall_budget() {
        // A person is waiting on this. Three sleeps, jitter at its worst, still
        // under the 2.1s the Zig spends — otherwise the "robust" retry is just
        // a slower failure.
        let worst: Duration = (0..STREAM_ATTEMPTS - 1)
            .map(|attempt| STREAM_BACKOFF.delay(attempt, u64::MAX))
            .sum();

        assert!(
            worst <= ZIG_WALL_BUDGET,
            "worst case {worst:?} must stay inside {ZIG_WALL_BUDGET:?}"
        );
    }

    #[test]
    fn the_first_retry_lands_sooner_than_the_zig_schedule_did() {
        // The point of doubling from 200ms: a Redis that blips is caught on the
        // second attempt instead of after the Zig's first 100ms plus a 500ms
        // second wait.
        let first = STREAM_BACKOFF.delay(0, 0);

        assert!(
            first <= Duration::from_millis(250),
            "first wait is {first:?}"
        );
    }

    #[test]
    fn concurrent_installs_do_not_retry_in_the_same_millisecond() {
        // Lockstep retries against a struggling Redis are the reconnect storm
        // that keeps it down. Two callers drawing different jitter must wait
        // different amounts.
        let one = STREAM_BACKOFF.delay(1, 0);
        let other = STREAM_BACKOFF.delay(1, u64::MAX);

        assert_ne!(one, other, "the schedule must spread");
        // Bounded as well as non-zero: jitter that could double a delay would
        // make the wall budget above unprovable.
        let spread = STREAM_BACKOFF.delay(1, 0) / JITTER_SHARE;
        let widened = other.checked_sub(one).expect("jitter only ever adds");
        assert!(widened <= spread + Duration::from_millis(1));
    }

    #[test]
    fn four_attempts_means_three_sleeps() {
        // Waiting after the final try spends 1.5s answering nothing, and the
        // loop is written so the count is the constant and the sleeps derive.
        assert_eq!(STREAM_ATTEMPTS, 4);
        assert_eq!((0..STREAM_ATTEMPTS - 1).count(), 3);
    }
}
