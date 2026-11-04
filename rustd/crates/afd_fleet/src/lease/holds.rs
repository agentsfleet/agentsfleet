//! One snapshot covers logged charges and every unresolved physical call.

use afd_billing::Nanos;
use afd_core::id::Uuid7;
use sqlx::PgConnection;

use super::Leases;
use crate::{Result, error::query};

pub(crate) async fn ceiling_on(
    connection: &mut PgConnection,
    lease: &Uuid7,
    coding_headroom: Nanos,
) -> Result<Nanos> {
    let charged_and_reserved: i64 = sqlx::query_scalar(include_str!("sql/reservation_ceiling.sql"))
        .bind(afd_billing::sql::work::FLEET_LEASE)
        .bind(lease.as_str())
        .fetch_one(connection)
        .await
        .map_err(query("read complete lease reservation ceiling"))?;
    Ok(Nanos::from_i64(charged_and_reserved).saturating_add(coding_headroom))
}

impl Leases {
    pub(crate) async fn decisions_pending(&self, lease: &Uuid7) -> Result<bool> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_scalar(include_str!("sql/decisions_pending.sql"))
            .bind(lease.as_str())
            .fetch_one(&mut *connection)
            .await
            .map_err(query(
                "check unresolved physical decisions before hold release",
            ))
    }
}
