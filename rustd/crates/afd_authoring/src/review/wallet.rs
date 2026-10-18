//! Readiness is a current observation; wallet balances never enter review identity.

use afd_core::{id::Uuid7, timing::LEASE_TTL};
use sqlx::PgConnection;

use super::Observed;
use crate::{
    Result,
    model::budget,
    types::{Action, ActionKind, CheckKind, CheckState},
};

const CONTEXT: &str = "observe setup wallet readiness";

pub(super) async fn observe(
    connection: &mut PgConnection,
    owner: &Uuid7,
    observed: &mut Observed,
) -> Result<()> {
    let wallet = budget::reserved(connection, owner, CONTEXT).await?;
    let ready = wallet.is_some_and(|wallet| {
        let interval = i64::try_from(LEASE_TTL.as_secs())
            .ok()
            .and_then(|seconds| afd_billing::RUN_NANOS_PER_SEC.checked_mul(seconds));
        wallet
            .open_count
            .checked_mul(wallet.largest_reserved)
            .and_then(|reserved| interval.and_then(|interval| reserved.checked_add(interval)))
            .is_some_and(|required| wallet.balance_nanos >= required)
    });
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
