//! The run time one metering pass measured, and where its lines are logged.

use af_core::id::Uuid7;
use afd_billing::biller::Settlement;
use jiff::Timestamp;

use crate::error::Result;

/// The run time one metering pass measured, as its trailing row carries it.
#[derive(Debug)]
pub(crate) struct Slice {
    /// The runtime line's number: milliseconds from the lease's start to the
    /// slice's start.
    pub runtime_seq: i64,
    /// Where the measured time ends, which is the runtime line's `at`.
    pub checked_at: Timestamp,
    /// When the lease began, which opens the lines' pricing window.
    pub started_at: Timestamp,
    /// Runtime since the lease's cursor, in milliseconds.
    pub runtime_ms: i64,
    /// Who pays.
    pub tenant_id: Uuid7,
    /// Where the work ran.
    pub workspace_id: Uuid7,
    /// The fleet the lines name.
    pub fleet_id: Uuid7,
    /// The fleet's name when the lines were written; `None` once purged.
    pub fleet_name: Option<String>,
    /// The event the run answered.
    pub event_id: Uuid7,
}

/// The slice columns of a metering statement's answer.
///
/// Every column is nullable because the row comes from a `LEFT JOIN` on the
/// fence: a pass that lost it measured nothing.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SliceColumns {
    checked_at: Option<jiff_sqlx::Timestamp>,
    started_at: Option<jiff_sqlx::Timestamp>,
    d_ms: Option<i64>,
    tenant_id: Option<Uuid7>,
    workspace_id: Option<Uuid7>,
    fleet_id: Option<Uuid7>,
    event_id: Option<Uuid7>,
    fleet_name: Option<String>,
    runtime_seq: Option<i64>,
}

impl SliceColumns {
    /// The measured slice, when the pass held the fence.
    pub(crate) fn into_slice(self) -> Option<Slice> {
        Some(Slice {
            runtime_seq: self.runtime_seq?,
            checked_at: self.checked_at?.to_jiff(),
            started_at: self.started_at?.to_jiff(),
            runtime_ms: self.d_ms?,
            tenant_id: self.tenant_id?,
            workspace_id: self.workspace_id?,
            fleet_id: self.fleet_id?,
            fleet_name: self.fleet_name,
            event_id: self.event_id?,
        })
    }
}

/// Logs a measured window's lines inside the pass's own transaction; a window
/// that measured nothing logs nothing. The renewal, the report's settle and
/// the expiry sweep each log this way.
///
/// # Errors
/// As [`afd_billing::ledger::log_lines`].
pub(crate) async fn log_measured(
    connection: &mut sqlx::PgConnection,
    entropy: &af_crypto::entropy::Entropy,
    now: Timestamp,
    settlement: Option<&Settlement>,
) -> Result<bool> {
    if let Some(settled) = settlement {
        let fresh = afd_billing::ledger::log_lines(&mut *connection, entropy, now, settled).await?;
        return crate::task::usage::record(connection, settled.work.id(), fresh).await;
    }
    Ok(true)
}
