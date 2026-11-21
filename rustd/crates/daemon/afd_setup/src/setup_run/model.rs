//! A setup turn's model: which one it pinned, whether it may take setup's
//! tool call, and whether the payer's balance funds the run's first slice.
//!
//! Eligibility is read on the workspace billing owner, never the chat
//! creator's account. Nothing here calls a model: the runner does.

use af_core::{id::Uuid7, timing::RunClock};
use afd_billing::RUN_NANOS_PER_SEC;
use afd_credential::provider::{PLATFORM_MODEL_REF, RegisteredModel};
use sqlx::PgConnection;

use crate::{Result, error::ErrorKind};

/// The `tenant_model_id` a model reference is stored as: NULL names the
/// platform default, any other reference is the tenant model's own id.
pub(crate) fn tenant_model(reference: &str) -> Result<Option<Uuid7>> {
    if reference == PLATFORM_MODEL_REF {
        return Ok(None);
    }
    Ok(Some(Uuid7::parse(reference)?))
}

/// The model reference a stored `tenant_model_id` spells.
pub(crate) fn reference(tenant_model: Option<Uuid7>) -> String {
    tenant_model.map_or_else(|| PLATFORM_MODEL_REF.to_owned(), String::from)
}

/// `reference`'s model for `owner`, refused unless its settings route and
/// its provider takes setup's tool call.
pub(crate) async fn selected(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<RegisteredModel> {
    let selected = afd_credential::provider::registered_model(connection, owner, reference)
        .await?
        .ok_or(ErrorKind::Model)?;
    if !selected.facts().eligibility().tools {
        return Err(ErrorKind::Model.into());
    }
    Ok(selected)
}

/// What a setup run must be able to fund before it starts: one run lifetime
/// of runtime, the slice its first renewal asks for again. A fleet's run is
/// gated the same way, and neither prices the whole run: the balance is
/// checked again at every renewal, which is what ends a runaway.
pub(crate) fn first_slice(clock: RunClock) -> Result<i64> {
    let seconds = i64::try_from(clock.ttl().as_secs()).map_err(|_overflow| ErrorKind::Balance)?;
    RUN_NANOS_PER_SEC
        .checked_mul(seconds)
        .ok_or_else(|| ErrorKind::Balance.into())
}

/// The assignment gate: whether `tenant`'s balance, less every line it logged
/// and the relay has not yet charged, covers `required`.
///
/// Read on the assignment's own transaction from the ledger's tables, never
/// through a second pooled connection, which would starve the pool under
/// concurrent assignments. Nothing is set aside: usage is charged at report.
/// A tenant with no wallet is refused.
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

#[cfg(test)]
mod tests {
    use af_core::{id::Uuid7, timing::RunClock};

    use super::{first_slice, reference, tenant_model};
    use afd_credential::provider::PLATFORM_MODEL_REF;

    // Value: protects=an empty wallet cannot start a setup run once the
    // whole-run estimate is gone; fails_when=the first slice prices to zero,
    // so a zero balance covers it; why_new=the 300-second estimate was what
    // priced the gate; seam=the live gate's balance read, covered by
    // `a_setup_run_on_an_empty_wallet_is_refused`; no_claim=the renewal gate.
    #[test]
    fn a_setup_run_must_fund_one_run_lifetime_to_start() -> crate::Result<()> {
        let required = first_slice(RunClock::DEFAULT)?;
        let seconds = i64::try_from(RunClock::DEFAULT.ttl().as_secs()).ok();
        assert_eq!(
            Some(required),
            seconds.and_then(|seconds| afd_billing::RUN_NANOS_PER_SEC.checked_mul(seconds))
        );
        assert!(required > 0, "an empty wallet must not cover it");
        Ok(())
    }

    /// The platform default is stored as NULL and read back as its reference;
    /// any other reference is the tenant model's own id.
    #[test]
    fn a_model_reference_round_trips_through_its_stored_column() -> crate::Result<()> {
        assert_eq!(tenant_model(PLATFORM_MODEL_REF)?, None);
        assert_eq!(reference(None), PLATFORM_MODEL_REF);
        let own = Uuid7::parse("01920000-0000-7000-8000-000000000001")?;
        assert_eq!(tenant_model(own.as_str())?, Some(own.clone()));
        assert_eq!(reference(Some(own.clone())), own.as_str());
        Ok(())
    }
}
