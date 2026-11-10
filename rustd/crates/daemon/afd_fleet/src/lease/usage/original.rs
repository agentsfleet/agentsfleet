//! Known request usage remains the original lease's after execution stops.

use super::{Pricing, SliceColumns};
use crate::{Result, error::query};
use af_core::id::Uuid7;
use af_wire::usage::RequestUsage;
use afd_billing::biller::Settlement;
use sqlx::PgConnection;

impl Pricing {
    /// Does not advance runtime, extend ownership or accept task/model semantics.
    pub(crate) async fn original_requests(
        &self,
        connection: &mut PgConnection,
        lease: &Uuid7,
        runner: &Uuid7,
        requests: &[RequestUsage],
    ) -> Result<Option<Settlement>> {
        if requests.is_empty() {
            return Ok(None);
        }
        let slice: Option<SliceColumns> =
            sqlx::query_as(include_str!("../sql/original_request_owner.sql"))
                .bind(lease)
                .bind(runner)
                .fetch_optional(connection)
                .await
                .map_err(query("read original known request accounting scope"))?;
        slice
            .and_then(SliceColumns::into_slice)
            .map_or(Ok(None), |slice| self.settlement(lease, slice, requests))
    }
}

impl crate::lease::Leases {
    /// Financial acknowledgement for a renewal refused before extension admission.
    ///
    /// Under the lease's own row lock and no host lock (M245 R8): the lines are
    /// model lines numbered by the runner, so there is no number to race for.
    pub(crate) async fn record_known_requests(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        pricing: &Pricing,
        requests: &[RequestUsage],
        now: jiff::Timestamp,
    ) -> Result<Option<Settlement>> {
        use sqlx::Connection as _;
        if requests.is_empty() {
            return Ok(None);
        }
        let mut connection = self.pool().acquire().await?;
        let mut tx = connection
            .begin()
            .await
            .map_err(query("begin original request accounting"))?;
        let settlement = pricing
            .original_requests(&mut tx, lease, runner, requests)
            .await?;
        super::log_measured(&mut tx, self.entropy(), now, settlement.as_ref()).await?;
        tx.commit()
            .await
            .map_err(query("commit original request accounting"))?;
        Ok(settlement)
    }
}
