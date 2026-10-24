//! Checked integer bounds use per-dispatch upward rounding, independent of runtime billing.

use std::time::Duration;

use afd_billing::biller::{Account, Biller, Hold, Reservation, WorkKey};
use afd_billing::{Nanos, RUN_NANOS_PER_SEC};
use afd_core::id::Uuid7;
use afd_credential::provider::RegisteredModel;
use sqlx::PgConnection;

use crate::{
    Drafts, Result,
    error::ErrorKind,
    lease::guard::{self, Attempt},
};

pub(crate) const MAX_CALLS: i64 = 8;
pub(crate) const PROVIDER_TIMEOUT: Duration = afd_inference::stream::READ_TIMEOUT;
/// How long a provider's private result outlives its dispatch.
pub(crate) const PRIVATE_TTL: Duration = afd_core::timing::DAY;
/// The longest an authoring attempt runs, renewal included.
pub(crate) const WALL: Duration = Duration::from_secs(300);
const MILLION: i128 = 1_000_000;

pub(crate) fn output_limit(model: &RegisteredModel) -> i64 {
    model
        .inference
        .as_ref()
        .map_or(0, |settings| i64::from(settings.max_output_tokens))
}

pub(crate) fn cost(model: &RegisteredModel, input: i64, output: i64) -> Result<i64> {
    ceil(input, model.input_nanos_per_million)?
        .checked_add(ceil(output, model.output_nanos_per_million)?)
        .ok_or_else(|| ErrorKind::Balance.into())
}

pub(crate) fn bound(model: &RegisteredModel) -> Result<i64> {
    let seconds = i64::try_from(WALL.as_secs()).map_err(|_overflow| ErrorKind::Balance)?;
    let runtime = RUN_NANOS_PER_SEC
        .checked_mul(seconds)
        .ok_or(ErrorKind::Balance)?;
    if !model.platform_managed {
        return Ok(runtime);
    }
    let output = ceil(output_limit(model), model.output_nanos_per_million)?
        .checked_add(MAX_CALLS - 1)
        .ok_or(ErrorKind::Balance)?;
    runtime
        .checked_add(output)
        .ok_or_else(|| ErrorKind::Balance.into())
}

fn ceil(tokens: i64, rate: i64) -> Result<i64> {
    let numerator = i128::from(tokens) * i128::from(rate);
    i64::try_from((numerator + MILLION - 1) / MILLION)
        .map_err(|_overflow| ErrorKind::Balance.into())
}

/// The claim gate: whether `tenant`'s balance, less every open hold, covers
/// `required` before an attempt is issued.
///
/// Read on the claim's own transaction from the ledger's tables, never through
/// a second pooled connection, which would starve the pool under concurrent
/// claims. The gate only refuses early; the hold placed after commit is what
/// the biller decides, and the first model call refuses if that hold is short.
/// A tenant with no wallet is refused, as before the biller.
pub(crate) async fn gate(
    connection: &mut PgConnection,
    tenant: &Uuid7,
    required: i64,
) -> Result<()> {
    match afd_billing::ledger::balance(&mut *connection, tenant).await {
        Ok(balance) => {
            let available = balance
                .balance
                .as_i64()
                .saturating_sub(balance.held.as_i64());
            if available < required {
                return Err(ErrorKind::Balance.into());
            }
            Ok(())
        }
        Err(fault) if fault.is_wallet_missing() => Err(ErrorKind::Balance.into()),
        Err(fault) => Err(fault.into()),
    }
}

/// Re-establish the current reservation before a renewal, outside its transaction.
pub(crate) async fn held(drafts: &Drafts, runner: &Uuid7, lease: &Uuid7) -> Result<()> {
    let attempt = {
        let mut connection = drafts.database.acquire().await?;
        guard::peek(&mut connection, runner, lease).await
    };
    match attempt {
        Ok(attempt) => hold(drafts.biller.as_ref(), &attempt).await,
        Err(_unreadable) => Ok(()),
    }
}

/// The attempt's hold, raised to its reserved ceiling before more is spent.
///
/// Called at renewal and before every model call. The ceiling is the bound the
/// claim reserved, so a repeat is a no-op unless the hold was never placed;
/// [`Hold::Short`] means the balance can no longer fund the attempt, and
/// [`Hold::Closed`] means a report already released it.
pub(crate) async fn hold(biller: &dyn Biller, attempt: &Attempt) -> Result<()> {
    if attempt.reserved_nanos < bound(&attempt.model_snapshot)? {
        return Err(ErrorKind::Balance.into());
    }
    let ask = reservation(
        attempt.id.clone(),
        &attempt.tenant_id,
        &attempt.workspace_id,
        attempt.reserved_nanos,
    );
    match biller.reserve(&ask).await {
        Ok(Hold::Held { .. } | Hold::Unconfirmed) => Ok(()),
        Ok(Hold::Short { .. } | Hold::Closed) => Err(ErrorKind::Balance.into()),
        Err(fault) if fault.is_wallet_missing() => Err(ErrorKind::Balance.into()),
        Err(fault) => Err(fault.into()),
    }
}

/// The reservation an attempt holds: up to `ceiling` for its whole run.
pub(crate) fn reservation(
    attempt: Uuid7,
    tenant: &Uuid7,
    workspace: &Uuid7,
    ceiling: i64,
) -> Reservation {
    Reservation {
        work: WorkKey::AuthoringAttempt(attempt),
        payer: Account {
            tenant: tenant.clone(),
            workspace: workspace.clone(),
        },
        ceiling: Nanos::from_i64(ceiling),
    }
}
