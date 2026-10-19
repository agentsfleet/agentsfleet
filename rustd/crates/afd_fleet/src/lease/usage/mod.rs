//! A metering pass's lines priced, and the settlement they ride in.
//!
//! A renewal and a report each measure the lease's run time under its row lock
//! ([`super::sql::meter::METER_LEASE`]) and carry the runner's lines, one per
//! provider request since the last acknowledged renewal. This module prices
//! every request line by its own whole-prompt tier and its own request time,
//! adds one runtime-only line for the time the daemon measured, and wraps them
//! as the [`Settlement`] the lease's transaction logs and its biller settles
//! after commit (billing §11).

use afd_billing::Nanos;
use afd_billing::biller::{Account, Attribution, Settlement, WorkKey};
use afd_billing::price::{PriceBook, Rater, Window};
use afd_billing::rates::Posture;
use afd_core::error_code::{Coded as _, Logged};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_wire::usage::{DAEMON_SEQ_BASE, RequestUsage, UsageLine};
use jiff::Timestamp;

use crate::error::{Result, row_malformed};

/// Logged when a pass could not read its prices and logged its tokens
/// unpriced.
const EVENT_TOKENS_UNPRICED: &str = "lease_tokens_unpriced";

/// The table a malformed slice identifier is reported against.
const TABLE_LEASES: &str = "runner.leases";

/// What one lease's lines are priced against.
///
/// Read BEFORE the lease's transaction opens, so no catalogue read waits while
/// the lease and its slot are locked.
#[derive(Debug)]
pub struct Pricing {
    book: PriceBook,
    posture: Posture,
    provider: String,
    model: String,
}

impl Pricing {
    /// The prices `provider`'s `model` is published at, for a run under
    /// `posture`.
    ///
    /// Fails OPEN: a catalogue that will not answer must not kill a run in
    /// flight. The pass then charges its runtime and logs its tokens unpriced:
    /// counted on the line, visible on the charges page, never silently free.
    pub async fn load(
        database: &Db,
        posture: Posture,
        provider: &str,
        model: &str,
        lease_id: &str,
    ) -> Self {
        let read = async {
            if posture == Posture::SelfManaged {
                return Ok(PriceBook::default());
            }
            let mut connection = database.acquire().await?;
            PriceBook::load(&mut *connection, provider, model)
                .await
                .map_err(crate::Error::from)
        }
        .await;
        let book = read.unwrap_or_else(|fault| {
            let Logged { error_code, reason } = fault.logged();
            tracing::warn!(
                error_code,
                lease_id,
                reason,
                event = EVENT_TOKENS_UNPRICED,
                "the catalogue could not be read; the run fee is charged and the tokens are logged unpriced"
            );
            PriceBook::default()
        });
        Self {
            book,
            posture,
            provider: provider.to_owned(),
            model: model.to_owned(),
        }
    }

    /// Prices for a pass that logs run time only, as an expired lease's last
    /// slice does: run time is charged under both postures and reads no
    /// catalogue.
    pub(crate) fn runtime_only(posture: Posture, provider: String, model: String) -> Self {
        Self {
            book: PriceBook::default(),
            posture,
            provider,
            model,
        }
    }

    /// `requests` and the measured `slice` priced as lines of `lease_id`,
    /// wrapped as the settlement its biller charges.
    ///
    /// One line per request, at its own tier and time, plus a runtime-only line
    /// numbered in the daemon's range when the slice measured run time. `None`
    /// when there is neither: logging nothing spends no line number.
    ///
    /// # Errors
    /// A stored identifier is not one, or a charge does not fit in a signed
    /// 64-bit nano count.
    pub(crate) fn settlement(
        &self,
        lease_id: &Uuid7,
        slice: Slice,
        requests: &[RequestUsage],
    ) -> Result<Option<Settlement>> {
        let runtime_ms = count(slice.runtime_ms);
        let runtime = (runtime_ms > 0).then(|| {
            UsageLine::runtime(
                DAEMON_SEQ_BASE.saturating_add(count(slice.daemon_lines)),
                slice.checked_at,
                runtime_ms,
            )
        });
        let lines: Vec<UsageLine> = requests
            .iter()
            .map(|request| request.line())
            .chain(runtime)
            .collect();
        if lines.is_empty() {
            return Ok(None);
        }
        let window = Window {
            from: slice.started_at,
            until: slice.checked_at,
        };
        let rater = Rater::new(&self.book, self.posture, window);
        let priced = lines
            .iter()
            .map(|line| rater.rate(line))
            .collect::<afd_billing::Result<Vec<_>>>()?;
        let id = |text: &str, column: &'static str| {
            Uuid7::parse(text).map_err(row_malformed(TABLE_LEASES, column))
        };
        Ok(Some(Settlement {
            work: WorkKey::FleetLease(lease_id.clone()),
            payer: Account {
                tenant: id(&slice.tenant_id, "tenant_id")?,
                workspace: id(&slice.workspace_id, "workspace_id")?,
            },
            attribution: Attribution::Fleet {
                fleet: id(&slice.fleet_id, "fleet_id")?,
                fleet_name: slice.fleet_name.unwrap_or_else(|| slice.fleet_id.clone()),
                event_id: slice.event_id,
            },
            provider: self.provider.clone(),
            model: self.model.clone(),
            posture: self.posture,
            lines: priced,
        }))
    }
}

/// What one settlement's lines cost, for the caller that reports a charge.
pub(crate) fn charged(settlement: Option<&Settlement>) -> Nanos {
    settlement.map_or(Nanos::ZERO, |settled| {
        settled
            .lines
            .iter()
            .fold(Nanos::ZERO, |sum, line| sum.saturating_add(line.charged))
    })
}

/// The run time one metering pass measured, as its trailing row carries it.
#[derive(Debug)]
pub(crate) struct Slice {
    /// Runtime lines the lease already logged; the next one takes the number
    /// after them in the daemon's range.
    pub daemon_lines: i64,
    /// Where the measured time ends, which is the runtime line's `at`.
    pub checked_at: Timestamp,
    /// When the lease began, which opens the lines' pricing window.
    pub started_at: Timestamp,
    /// Runtime since the lease's cursor, in milliseconds.
    pub runtime_ms: i64,
    /// Who pays.
    pub tenant_id: String,
    /// Where the work ran.
    pub workspace_id: String,
    /// The fleet the lines name.
    pub fleet_id: String,
    /// The fleet's name when the lines were written; `None` once purged.
    pub fleet_name: Option<String>,
    /// The event the run answered.
    pub event_id: String,
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
    tenant_id: Option<String>,
    workspace_id: Option<String>,
    fleet_id: Option<String>,
    event_id: Option<String>,
    fleet_name: Option<String>,
    daemon_lines: Option<i64>,
}

impl SliceColumns {
    /// The measured slice, when the pass held the fence.
    pub(crate) fn into_slice(self) -> Option<Slice> {
        Some(Slice {
            daemon_lines: self.daemon_lines?,
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
    entropy: &afd_crypto::entropy::Entropy,
    now: Timestamp,
    settlement: Option<&Settlement>,
) -> Result<()> {
    if let Some(settled) = settlement {
        afd_billing::ledger::log_lines(connection, entropy, now, settled).await?;
    }
    Ok(())
}

/// A non-negative count as the line type holds it, saturating rather than
/// wrapping: an elapsed span the lease's hard cap bounds, or a line count, so
/// the saturation never fires.
fn count(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
