//! Readiness is a current observation; wallet balances never enter review identity.

use afd_core::{id::Uuid7, timing::LEASE_TTL_MS};
use sqlx::PgConnection;

use super::Observed;
use crate::{
    Result,
    error::raise::query,
    types::{Action, ActionKind, CheckKind, CheckState},
};

const CONTEXT: &str = "observe setup wallet readiness";
const COVERAGE: &str = "\
SELECT w.balance_nanos,count(a.id)::bigint,coalesce(max(a.reserved_nanos),0)::bigint
FROM billing.tenant_wallet w LEFT JOIN fleet.authoring_attempts a
ON a.tenant_id=w.tenant_id AND (a.status IN ('active','awaiting_accounting') OR a.usage_unknown)
WHERE w.tenant_id=$1::uuid GROUP BY w.balance_nanos";

pub(super) async fn observe(
    connection: &mut PgConnection,
    owner: &Uuid7,
    observed: &mut Observed,
) -> Result<()> {
    let wallet: Option<(i64, i64, i64)> = sqlx::query_as(COVERAGE)
        .bind(owner.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let ready = wallet.is_some_and(|(balance, count, maximum)| {
        let interval = afd_billing::RUN_NANOS_PER_SEC
            .checked_mul(LEASE_TTL_MS / afd_billing::nanos::MS_PER_SEC);
        count
            .checked_mul(maximum)
            .and_then(|reserved| interval.and_then(|interval| reserved.checked_add(interval)))
            .is_some_and(|required| balance >= required)
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
