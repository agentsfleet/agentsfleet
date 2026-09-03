//! Changing a fleet: the conditional write, and the status machine.
//!
//! # No row lock, and no transaction
//!
//! `patch_txn.zig` opens a transaction, takes `SELECT … FOR UPDATE`, sets three
//! `SET LOCAL` timeouts, and classifies `55P03` so a lock race reads as a 503.
//! All of that exists to make a read-modify-write safe. None of it is needed
//! once the compare-and-set is expressed as a PREDICATE: the read runs
//! unlocked, and the `UPDATE` proceeds only while the columns still hold what
//! the caller read. A concurrent write makes it match no row — the same answer
//! the lock would have produced, without holding one across a YAML reparse.
//!
//! # What the guard is over, and why not the obvious things
//!
//! The digests of the two markdown columns: exactly what the `ETag` hashes.
//!
//! Not `updated_at`. It is epoch MILLISECONDS, so two commits inside one
//! millisecond return it to a value a third reader is still holding, and its
//! guard then passes against a version that moved. `TIMESTAMPTZ` would not
//! close that either — `now()` is transaction-START time, and two concurrent
//! transactions read it identical.
//!
//! Not the whole row (`xmin`). That is exact, and wrong here: any column change
//! bumps it, so somebody stopping the fleet would refuse an editor whose source
//! nobody touched — the precise case a source-only tag exists to allow.

mod conditional;
mod rewrite;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{self, ErrorKind, Result};
use crate::read::surface;
use crate::{FleetStatus, Fleets, sql};

use self::conditional::{Guard, stale_check};
use self::rewrite::{Rewrite, Snapshot};

/// The column an unknown stored status is reported against.
const COLUMN_STATUS: &str = "status";

/// The contexts a failed statement on this path reports under.
const CONTEXT_READ: &str = "read fleet for patch";
const CONTEXT_UPDATE: &str = "apply fleet patch";

/// The statuses `stopped` may be reached FROM.
///
/// `&[&str]` and not a built `Vec<String>`: two constant sets bound on every
/// PATCH, and `sqlx` encodes a string slice as `TEXT[]` directly.
const REACHABLE_STOPPED: [&str; 2] = [FleetStatus::Active.as_str(), FleetStatus::Paused.as_str()];

/// The statuses `active` may be reached FROM.
///
/// `paused` is in both sets because the anomaly gate parks a fleet there, and an
/// operator has to be able to bring it back either way — a gate that could only
/// be left in one direction would be a trap.
const REACHABLE_ACTIVE: [&str; 2] = [FleetStatus::Stopped.as_str(), FleetStatus::Paused.as_str()];

/// Where a PATCH's new configuration comes from — never both at once.
///
/// The Zig carries `config_json` and `trigger_markdown` as two optional fields
/// and refuses the both-set case at the door, at runtime. Here the ambiguity
/// cannot be constructed: both drive `core.fleets.config_json`, so they are one
/// choice, and the edge resolves it once while reading the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// A configuration document, replacing the stored one directly.
    Json(String),
    /// An authored `TRIGGER.md`, reparsed into the configuration AND the name.
    Trigger(String),
}

/// A status an API caller may ask a fleet to move to.
///
/// Deliberately smaller than [`FleetStatus`]: `paused` belongs to the platform's
/// anomaly gate and `installing` to the install, so neither is spellable here. A
/// caller cannot forge a system-halt provenance, and `patch_body.validateBody`'s
/// hand-written allow-list has nothing left to check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requested {
    /// Resume, or finish an install by hand.
    Active,
    /// Stop, keeping the fleet resumable.
    Stopped,
    /// Terminal. Nothing edits a killed fleet, and only a killed fleet purges.
    Killed,
}

impl Requested {
    /// The status this asks for.
    #[must_use]
    pub const fn status(self) -> FleetStatus {
        match self {
            Self::Active => FleetStatus::Active,
            Self::Stopped => FleetStatus::Stopped,
            Self::Killed => FleetStatus::Killed,
        }
    }
}

/// One PATCH request, already parsed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patch {
    /// Where the new configuration comes from, when one was sent.
    pub config: Option<ConfigSource>,
    /// The transition asked for, when one was.
    pub status: Option<Requested>,
    /// A replacement `SKILL.md`, reparsed and cross-checked against the name.
    pub source_markdown: Option<String>,
    /// The version the caller believes they are editing.
    ///
    /// Absent means last-writer-wins by the caller's own choice, so no guard is
    /// bound and a concurrent write is not a refusal.
    pub if_match: Option<String>,
}

impl Patch {
    /// Whether this asks for nothing.
    ///
    /// An empty body is a 200 that touches no row — `patch.zig`'s behaviour, and
    /// worth keeping: a dashboard saving an untouched form should not make this
    /// daemon read one. Answered by the caller before this crate is reached.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.config.is_none() && self.status.is_none() && self.source_markdown.is_none()
    }
}

/// What a committed PATCH answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patched {
    /// The new `updated_at`, which IS the config revision a caller echoes back.
    pub revision: i64,
    /// The tag over the post-update source, for the editor's next save.
    pub etag: String,
}

impl Fleets {
    /// Applies one PATCH, refusing a write the caller's version cannot support.
    ///
    /// # Errors
    /// Refuses an id naming no fleet this workspace holds and one already
    /// killed, a transition the machine does not allow from where the row
    /// stands, an `If-Match` naming a version the source has moved past, and
    /// either document being unusable. Reports a datastore that would not
    /// answer.
    pub async fn patch(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        request: &Patch,
        now: UnixMillis,
    ) -> Result<Patched> {
        let current = self.editable(workspace, fleet).await?;
        // Compared here AND in the predicate, and both earn their place: this
        // answers a stale editor in one round trip with the tag it needs, while
        // the predicate is what makes the write atomic against a race that
        // starts after this read returns.
        let guard = stale_check(request.if_match.as_deref(), &current)?;

        let rewrite = Rewrite::read(request, &current)?;
        // The second door. A replacement `TRIGGER.md` declares credentials just
        // as a bundle does, so an edit that skipped this check would land the
        // same unrunnable fleet the install refuses — a row whose first lease
        // reaches for a secret nobody stored.
        if let Some(declared) = &rewrite.declared_credentials {
            self.require_the_stored_credentials(workspace, declared)
                .await?;
        }
        let updated = self
            .apply(workspace, fleet, request, &rewrite, guard.as_ref(), now)
            .await?;
        let Some(revision) = updated else {
            return Err(self.explain(workspace, fleet, &current).await);
        };
        Ok(Patched {
            revision,
            // What the row WILL hold: the update `COALESCE`s each column and
            // this mirrors that, or an editor's next save would be refused
            // against a tag nothing ever wrote.
            etag: afd_core::etag::compute(&surface(
                rewrite.source_after(&current),
                rewrite.trigger_after(&current),
            )),
        })
    }

    /// Reads the editable surface, unlocked.
    async fn editable(&self, workspace: &Uuid7, fleet: &Uuid7) -> Result<Snapshot> {
        let mut connection = self.database.acquire().await?;
        let found = sqlx::query(sql::SELECT_FLEET_EDITABLE)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;
        let row = found.ok_or_else(|| crate::Error::from(ErrorKind::NotFound))?;

        let unreadable = error::query(CONTEXT_READ);
        let stored: String = row.try_get(1).map_err(&unreadable)?;
        Ok(Snapshot {
            name: row.try_get(0).map_err(&unreadable)?,
            status: FleetStatus::parse(&stored)
                .ok_or_else(|| error::row_malformed(COLUMN_STATUS, &stored))?,
            source_markdown: row.try_get(2).map_err(&unreadable)?,
            trigger_markdown: row.try_get(3).map_err(&unreadable)?,
        })
    }

    /// Runs the update, answering the new revision or nothing.
    ///
    /// Nothing means one of three things and this deliberately does not guess
    /// which — [`Fleets::explain`] re-reads to say, on the error path only.
    async fn apply(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        request: &Patch,
        rewrite: &Rewrite,
        guard: Option<&Guard>,
        now: UnixMillis,
    ) -> Result<Option<i64>> {
        let mut connection = self.database.acquire().await?;
        let updated = sqlx::query(sql::PATCH_FLEET)
            .bind(rewrite.config_json.as_deref())
            .bind(request.status.map(|asked| asked.status().as_str()))
            .bind(now.as_millis())
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .bind(FleetStatus::Killed.as_str())
            .bind(FleetStatus::Stopped.as_str())
            .bind(FleetStatus::Active.as_str())
            .bind(REACHABLE_STOPPED.as_slice())
            .bind(REACHABLE_ACTIVE.as_slice())
            .bind(rewrite.trigger_markdown.as_deref())
            .bind(rewrite.source_markdown.as_deref())
            .bind(rewrite.name.as_deref())
            .bind(rewrite.required_tags.as_deref())
            .bind(guard.map(|held| held.source.as_str()))
            .bind(guard.and_then(|held| held.trigger.as_deref()))
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_UPDATE))?;

        updated
            .map(|row| row.try_get(0).map_err(error::query(CONTEXT_UPDATE)))
            .transpose()
    }

    /// Says what zero updated rows meant, by reading the row again.
    ///
    /// The one extra statement this design costs, and it is spent only on a path
    /// that is already an error. What the row holds NOW decides: gone means
    /// somebody purged it mid-request, a killed row is a tombstone, a source
    /// that no longer matches what the caller read is a lost race, and anything
    /// else is the status machine refusing the transition.
    async fn explain(&self, workspace: &Uuid7, fleet: &Uuid7, seen: &Snapshot) -> crate::Error {
        let read = match self.editable(workspace, fleet).await {
            Ok(read) => read,
            // Including the not-found this raises when the row has gone, which
            // is the honest answer for a fleet purged mid-request.
            Err(vanished) => return vanished,
        };
        if read.status == FleetStatus::Killed {
            return ErrorKind::NotFound.into();
        }
        if read.source_markdown != seen.source_markdown
            || read.trigger_markdown != seen.trigger_markdown
        {
            return error::source_stale(afd_core::etag::compute(&surface(
                &read.source_markdown,
                read.trigger_markdown.as_deref(),
            )));
        }
        ErrorKind::TransitionRefused.into()
    }
}

#[cfg(test)]
mod tests {
    use super::{FleetStatus, Patch, REACHABLE_ACTIVE, REACHABLE_STOPPED, Requested};

    #[test]
    fn an_empty_patch_asks_for_nothing_and_reads_no_row() {
        assert!(Patch::default().is_empty());
        assert!(
            !Patch {
                status: Some(Requested::Killed),
                ..Patch::default()
            }
            .is_empty()
        );
        // An `If-Match` alone is still empty — there is nothing to write
        // conditionally, which is what the edge answers 400 for.
        assert!(
            Patch {
                if_match: Some("\"abc\"".to_owned()),
                ..Patch::default()
            }
            .is_empty()
        );
    }

    #[test]
    fn a_paused_fleet_can_be_resumed_and_stopped_from_where_it_parks() {
        let paused = FleetStatus::Paused.as_str();
        assert!(REACHABLE_ACTIVE.contains(&paused));
        assert!(REACHABLE_STOPPED.contains(&paused));
    }

    #[test]
    fn no_transition_may_be_reached_from_itself() {
        assert!(!REACHABLE_ACTIVE.contains(&FleetStatus::Active.as_str()));
        assert!(!REACHABLE_STOPPED.contains(&FleetStatus::Stopped.as_str()));
    }

    #[test]
    fn the_requested_statuses_map_to_exactly_three_stored_ones() {
        assert_eq!(Requested::Active.status(), FleetStatus::Active);
        assert_eq!(Requested::Stopped.status(), FleetStatus::Stopped);
        assert_eq!(Requested::Killed.status(), FleetStatus::Killed);
    }
}
