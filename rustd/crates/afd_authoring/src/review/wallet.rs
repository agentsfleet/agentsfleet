//! Readiness is a current observation; wallet balances never enter review identity.

use afd_core::timing::LEASE_TTL;
use sqlx::PgConnection;

use super::{Context, Observed};
use crate::{
    Result,
    types::{Action, ActionKind, CheckKind, CheckState},
};

/// Ready when the owner's balance, less every open hold, still funds one
/// lease interval of runtime: the least a setup chat can run before its first
/// renewal asks again. Read on the caller's transaction, never a second pooled
/// connection.
pub(super) async fn observe(
    connection: &mut PgConnection,
    draft: &Context,
    observed: &mut Observed,
) -> Result<()> {
    let ready = match afd_billing::ledger::balance(&mut *connection, &draft.tenant_id).await {
        Ok(balance) => {
            let interval = i64::try_from(LEASE_TTL.as_secs())
                .ok()
                .and_then(|seconds| afd_billing::RUN_NANOS_PER_SEC.checked_mul(seconds));
            interval.is_some_and(|interval| {
                balance
                    .balance
                    .as_i64()
                    .saturating_sub(balance.held.as_i64())
                    >= interval
            })
        }
        Err(fault) if fault.is_wallet_missing() => false,
        Err(fault) => return Err(fault.into()),
    };
    observed.check(
        CheckKind::Wallet,
        "owner_wallet",
        if ready {
            CheckState::Ready
        } else {
            CheckState::Missing
        },
        Some(Action {
            kind: ActionKind::Billing,
            label: "Open owner billing".to_owned(),
            href: "/settings/billing".to_owned(),
        }),
    );
    Ok(())
}
