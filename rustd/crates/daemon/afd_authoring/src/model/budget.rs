//! Checked integer bounds use per-dispatch upward rounding, independent of runtime billing.

use std::time::Duration;

use af_core::id::Uuid7;
use afd_billing::biller::{Account, Ask, Biller, Funds, WorkKey};
use afd_billing::{Nanos, RUN_NANOS_PER_SEC};
use afd_credential::provider::RegisteredModel;
use sqlx::PgConnection;

use crate::{
    Drafts, Result,
    error::ErrorKind,
    lease::guard::{self, Attempt},
};

pub(crate) const MAX_CALLS: i64 = 8;
pub(crate) const PROVIDER_TIMEOUT: Duration = af_inference::stream::READ_TIMEOUT;
/// How long a provider's private result outlives its dispatch.
pub(crate) const PRIVATE_TTL: Duration = af_core::timing::DAY;
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

/// The claim gate: whether `tenant`'s balance, less every line it logged and
/// the relay has not yet charged, covers `required` before an attempt is
/// issued.
///
/// Read on the claim's own transaction from the ledger's tables, never through
/// a second pooled connection, which would starve the pool under concurrent
/// claims. Nothing is set aside: each model call checks again, and refuses if
/// the balance can no longer fund the attempt. A tenant with no wallet is
/// refused.
pub(crate) async fn gate(
    connection: &mut PgConnection,
    tenant: &Uuid7,
    required: i64,
) -> Result<()> {
    match afd_billing::ledger::balance(&mut *connection, tenant).await {
        Ok(balance) => {
            if balance.left().as_i64() < required {
                return Err(ErrorKind::Balance.into());
            }
            Ok(())
        }
        Err(fault) if fault.is_wallet_missing() => Err(ErrorKind::Balance.into()),
        Err(fault) => Err(fault.into()),
    }
}

/// Whether the attempt behind `lease` is still funded, checked before a
/// renewal and outside its transaction.
pub(crate) async fn funded(drafts: &Drafts, runner: &Uuid7, lease: &Uuid7) -> Result<()> {
    let attempt = {
        let mut connection = drafts.database.acquire().await?;
        guard::peek(&mut connection, runner, lease).await
    };
    match attempt {
        Ok(attempt) => covers(&drafts.database, drafts.biller.as_ref(), &attempt).await,
        Err(_unreadable) => Ok(()),
    }
}

/// Whether the payer can still fund the attempt's whole bound.
///
/// Called at renewal and before every model call. Coding spend is logged at
/// report time; edit-decision spend is logged immediately. Subtract those
/// already logged costs from the bound because the ledger already subtracts
/// them from available credit, whether settled or still owed to the relay.
pub(crate) async fn covers(
    database: &afd_db::Db,
    biller: &dyn Biller,
    attempt: &Attempt,
) -> Result<()> {
    if attempt.reserved_nanos < bound(&attempt.model_snapshot)? {
        return Err(ErrorKind::Balance.into());
    }
    let logged: i64 = sqlx::query_scalar(include_str!("budget_logged.sql"))
        .bind(afd_billing::sql::work::AUTHORING_ATTEMPT)
        .bind(&attempt.id)
        .fetch_one(&mut *database.acquire().await?)
        .await
        .map_err(crate::error::raise::query("read logged authoring budget"))?;
    let ask = Ask {
        work: WorkKey::AuthoringAttempt(attempt.id.clone()),
        payer: Account {
            tenant: attempt.tenant_id.clone(),
            workspace: attempt.workspace_id.clone(),
        },
        next: Nanos::from_i64(attempt.reserved_nanos.saturating_sub(logged).max(0)),
    };
    match biller.check(&ask).await {
        Ok(Funds::Covered | Funds::Unconfirmed) => Ok(()),
        Ok(Funds::Short { .. }) => Err(ErrorKind::Balance.into()),
        Err(fault) if fault.is_wallet_missing() => Err(ErrorKind::Balance.into()),
        Err(fault) => Err(fault.into()),
    }
}
