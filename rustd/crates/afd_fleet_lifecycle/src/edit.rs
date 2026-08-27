//! Changing a fleet: the conditional write, and the status machine.
//!
//! # The whole edit is one transaction holding one row lock
//!
//! `SELECT … FOR UPDATE` → `If-Match` compare → reparse → the FSM-gated
//! `UPDATE`. Exactly one `core.fleets` row is ever locked, which makes a
//! deadlock on this path structurally impossible rather than merely unlikely.
//!
//! The transaction is a [`sqlx::Transaction`] rather than hand-written `BEGIN`
//! and `COMMIT`, and the difference shows on the paths nobody planned: a `?`
//! returning early, a panic, a future dropped when the client hangs up.
//! `patch_txn.zig` covers those with `defer if (tx_open) conn.rollback()`, which
//! is correct and has to be re-derived by every reader; here the rollback is the
//! value's `Drop` and cannot be forgotten.
//!
//! # Two timeouts and one SQLSTATE
//!
//! `lock_timeout`, `statement_timeout` and
//! `idle_in_transaction_session_timeout` are set per transaction, so a request
//! that loses a lock race is refused in seconds instead of holding a pool
//! connection against a stuck writer. Postgres reports the first as `55P03`,
//! and that one code becomes a 503 the caller may retry rather than a 500 that
//! reads as a defect they should stop for.

mod rewrite;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{self, ErrorKind, Result};
use crate::read::surface;
use crate::{FleetStatus, Fleets, sql};

use self::rewrite::{Rewrite, Snapshot};

/// The per-transaction guards, in the order `patch_txn.zig` sets them.
const GUARDS: [&str; 3] = [
    "SET LOCAL lock_timeout = '5s'",
    "SET LOCAL statement_timeout = '10s'",
    "SET LOCAL idle_in_transaction_session_timeout = '5s'",
];

/// Postgres's SQLSTATE for a row-lock timeout.
///
/// The one code this transaction turns into a deterministic outcome: it means
/// another writer holds the row, which is transient and worth retrying, where a
/// generic failure is neither.
const SQLSTATE_LOCK_TIMEOUT: &str = "55P03";

/// The column an unknown stored status is reported against.
const COLUMN_STATUS: &str = "status";

/// The contexts a failed statement on this path reports under.
const CONTEXT_BEGIN: &str = "open fleet patch transaction";
const CONTEXT_LOCK: &str = "lock fleet for update";
const CONTEXT_UPDATE: &str = "apply fleet patch";
const CONTEXT_COMMIT: &str = "commit fleet patch";

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
    /// Absent means last-writer-wins by the caller's own choice, so a stale
    /// refusal would be answering a question they did not ask.
    pub if_match: Option<String>,
}

impl Patch {
    /// Whether this asks for nothing.
    ///
    /// An empty body is a 200 that touches no row — `patch.zig`'s behaviour, and
    /// worth keeping: a dashboard saving an untouched form should not take a row
    /// lock. Answered by the caller before this crate is reached, which is why
    /// it is the only method here that is not a write.
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
    /// Applies one PATCH inside a single row lock.
    ///
    /// # Errors
    /// Refuses an id naming no fleet this workspace holds and one already
    /// killed, a transition the machine does not allow from where the row
    /// stands, an `If-Match` naming a version the source has moved past, and
    /// either document being unusable. Reports a lock this request could not
    /// take, and a datastore that would not answer.
    pub async fn patch(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        request: &Patch,
        now: UnixMillis,
    ) -> Result<Patched> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_BEGIN))?;
        for guard in GUARDS {
            sqlx::query(guard)
                .execute(&mut *transaction)
                .await
                .map_err(classify(CONTEXT_BEGIN))?;
        }

        let current = snapshot(&mut transaction, workspace, fleet).await?;
        stale_check(request.if_match.as_deref(), &current)?;

        let rewrite = Rewrite::read(request, &current)?;
        let revision = apply(&mut transaction, workspace, fleet, request, &rewrite, now).await?;
        // Computed from what the row WILL hold, before the commit that makes it
        // so — the update `COALESCE`s each column, and this mirrors that, or an
        // editor's next save would be refused against a tag nothing ever wrote.
        let etag = afd_core::etag::compute(&surface(
            rewrite.source_after(&current),
            rewrite.trigger_after(&current),
        ));
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_COMMIT))?;
        Ok(Patched { revision, etag })
    }
}

/// Refuses a conditional write against a version the row has moved past.
///
/// Strong comparison, which is `If-Match`'s rule and the opposite of the
/// conditional GET's: a WRITE may only proceed against the exact representation
/// the caller read, where a revalidating cache may accept a weak match.
fn stale_check(presented: Option<&str>, current: &Snapshot) -> Result<()> {
    let Some(presented) = presented else {
        return Ok(());
    };
    let held = afd_core::etag::compute(&surface(
        &current.source_markdown,
        current.trigger_markdown.as_deref(),
    ));
    if presented == held {
        return Ok(());
    }
    Err(error::source_stale(held))
}

/// Takes the row lock and reads what the update needs in order to decide.
async fn snapshot(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<Snapshot> {
    let found = sqlx::query(sql::SELECT_FLEET_FOR_UPDATE)
        .bind(fleet.as_str())
        .bind(workspace.as_str())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(classify(CONTEXT_LOCK))?;
    let row = found.ok_or_else(|| crate::Error::from(ErrorKind::NotFound))?;

    let unreadable = error::query(CONTEXT_LOCK);
    let stored: String = row.try_get(1).map_err(&unreadable)?;
    Ok(Snapshot {
        name: row.try_get(0).map_err(&unreadable)?,
        status: FleetStatus::parse(&stored)
            .ok_or_else(|| error::row_malformed(COLUMN_STATUS, &stored))?,
        source_markdown: row.try_get(2).map_err(&unreadable)?,
        trigger_markdown: row.try_get(3).map_err(&unreadable)?,
    })
}

/// Runs the FSM-gated update, and says what zero rows meant.
async fn apply(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: &Uuid7,
    fleet: &Uuid7,
    request: &Patch,
    rewrite: &Rewrite,
    now: UnixMillis,
) -> Result<i64> {
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
        .fetch_optional(&mut **transaction)
        .await
        .map_err(classify(CONTEXT_UPDATE))?;

    match updated {
        Some(row) => row.try_get(0).map_err(error::query(CONTEXT_UPDATE)),
        // Zero rows, and the snapshot already says which of the two it was. No
        // second read is needed, because the lock is still held: a killed row is
        // a tombstone and answers 404, anything else refused the transition.
        None if rewrite.was_killed => Err(ErrorKind::NotFound.into()),
        None => Err(ErrorKind::TransitionRefused.into()),
    }
}

/// The statuses `stopped` may be reached FROM.
///
/// `&[&str]` and not a built `Vec<String>`: these are two constant sets bound
/// on every PATCH, and `sqlx` encodes a string slice as `TEXT[]` directly. The
/// built form cost two heap vectors and four owned strings per request to say
/// something that never changes.
const REACHABLE_STOPPED: [&str; 2] = [FleetStatus::Active.as_str(), FleetStatus::Paused.as_str()];

/// The statuses `active` may be reached FROM.
///
/// `paused` is in both sets because the anomaly gate parks a fleet there, and an
/// operator has to be able to bring it back either way — a gate that could only
/// be left in one direction would be a trap.
const REACHABLE_ACTIVE: [&str; 2] = [FleetStatus::Stopped.as_str(), FleetStatus::Paused.as_str()];

/// Turns a lock timeout into a transient refusal, passing everything else on.
fn classify(context: &'static str) -> impl Fn(sqlx::Error) -> crate::Error {
    move |source| {
        let contended = source
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .is_some_and(|code| code == SQLSTATE_LOCK_TIMEOUT);
        if contended {
            ErrorKind::LockContended.into()
        } else {
            error::query(context)(source)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FleetStatus, Patch, REACHABLE_ACTIVE, REACHABLE_STOPPED, Requested};

    #[test]
    fn an_empty_patch_asks_for_nothing_and_takes_no_lock() {
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
        // The anomaly gate parks a fleet in `paused`, and an operator has to be
        // able to reach both targets from there or the gate is a trap.
        let paused = FleetStatus::Paused.as_str();
        assert!(REACHABLE_ACTIVE.contains(&paused));
        assert!(REACHABLE_STOPPED.contains(&paused));
    }

    #[test]
    fn no_transition_may_be_reached_from_itself() {
        // A no-op transition is refused by the SQL guard's `status != $6` and
        // the reachable sets, together — a resume of an already-active fleet is
        // a 409, not a silent success.
        assert!(!REACHABLE_ACTIVE.contains(&FleetStatus::Active.as_str()));
        assert!(!REACHABLE_STOPPED.contains(&FleetStatus::Stopped.as_str()));
    }

    #[test]
    fn the_requested_statuses_map_to_exactly_three_stored_ones() {
        // `paused` and `installing` are absent by construction, which is the
        // whole point of the type being smaller than `FleetStatus`.
        assert_eq!(Requested::Active.status(), FleetStatus::Active);
        assert_eq!(Requested::Stopped.status(), FleetStatus::Stopped);
        assert_eq!(Requested::Killed.status(), FleetStatus::Killed);
    }
}
