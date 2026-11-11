//! What the frame prices from the caches: the funds floor, each lease's book,
//! and the known request lines of a lease the batch refused.

use std::collections::HashSet;
use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use jiff::Timestamp;

use super::EVENT_FRAME_FAILED;
use super::rows::Found;
use crate::error::Result;
use crate::lease::pull::LeaseService;
use crate::lease::renew::Renewing;
use crate::lease::usage::Pricing;

impl LeaseService {
    /// The payers this tick's funds check found short, checked once each.
    ///
    /// Only every [`FUNDS_CHECK_TICKS`](super::FUNDS_CHECK_TICKS)th tick checks:
    /// the cadence is the gateway session's, so a reconnect checks on its first
    /// tick.
    pub(super) async fn refusals<'a>(
        &self,
        batch: &[(&'a Found, &RenewRequest)],
        funds_due: bool,
        now: Timestamp,
    ) -> HashSet<(&'a Uuid7, &'a Uuid7)> {
        let mut short = HashSet::new();
        if !funds_due {
            return short;
        }
        let mut checked = HashSet::new();
        for (row, _request) in batch {
            let payer = (&row.tenant_id, &row.workspace_id);
            if !checked.insert(payer) {
                continue;
            }
            let renewing = row.renewing();
            let estimate = self.cached_floor(&renewing, row, now).await;
            if self
                .funds_cover_at(&renewing, &row.id, row.id.as_str(), estimate)
                .await
                .is_err()
            {
                short.insert(payer);
            }
        }
        short
    }

    /// The funds check's floor, priced from the book cached at the row's
    /// catalogue version: a warm replica reads no price.
    async fn cached_floor(
        &self,
        renewing: &Renewing,
        row: &Found,
        now: Timestamp,
    ) -> Result<afd_billing::rates::Estimate, Arc<crate::Error>> {
        let posture = Pricing::posture_of(&renewing.posture).map_err(Arc::new)?;
        let pricing = self
            .leases
            .prices()
            .try_get_or_load(
                self.leases.pool(),
                posture,
                &row.provider,
                &row.model,
                Some(row.catalogue_version),
            )
            .await?;
        afd_billing::Accounts::estimate_from(pricing.book(), posture, now)
            .map_err(|fault| Arc::new(crate::Error::from(fault)))
    }

    /// The lease's pricing, from the price books by catalogue version.
    ///
    /// # Errors
    /// The lease's stored posture is none this daemon writes.
    pub(super) async fn pricing_of(&self, row: &Found) -> Result<Arc<Pricing>> {
        let posture = Pricing::posture_of(&row.posture)?;
        Ok(self
            .leases
            .prices()
            .get_or_load(
                self.leases.pool(),
                posture,
                &row.provider,
                &row.model,
                Some(row.catalogue_version),
                crate::lease::cache::LABEL_BATCH,
            )
            .await)
    }

    /// Logs a refused lease's known request lines, as the per-lease path does.
    /// A failure is the line's to retry with the next frame.
    pub(super) async fn known_lines(
        &self,
        runner_id: &Uuid7,
        row: &Found,
        request: &RenewRequest,
        now: Timestamp,
    ) {
        if request.requests.0.is_empty() {
            return;
        }
        if let Err(fault) = self.record_known(runner_id, row, request, now).await {
            let lease = row.id.as_str();
            let code = fault.code().as_str();
            tracing::warn!(
                lease_id = lease,
                error_code = code,
                event = EVENT_FRAME_FAILED,
                "a refused lease's known request lines were not logged"
            );
        }
    }

    async fn record_known(
        &self,
        runner_id: &Uuid7,
        row: &Found,
        request: &RenewRequest,
        now: Timestamp,
    ) -> Result<()> {
        let pricing = self.pricing_of(row).await?;
        self.leases
            .record_known_requests(runner_id, &row.id, &pricing, &request.requests.0, now)
            .await?;
        Ok(())
    }
}
