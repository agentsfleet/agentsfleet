//! What the frame prices from the caches: the funds floor, each lease's book,
//! and the known request lines of a lease the batch refused.

use std::collections::HashSet;
use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use afd_billing::rates::Estimate;
use jiff::Timestamp;

use super::EVENT_FRAME_FAILED;
use super::rows::Found;
use crate::error::Result;
use crate::lease::pull::LeaseService;
use crate::lease::renew::Renewing;
use crate::lease::usage::Pricing;

impl LeaseService {
    /// The payers this tick's funds check found short, checked once each, against
    /// the floor of the payer's dearest lease.
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
        for (payer, rows) in by_payer(batch) {
            let mut floors = Vec::with_capacity(rows.len());
            for &row in &rows {
                floors.push((row, self.cached_floor(&row.renewing(), row, now).await));
            }
            let Some((row, estimate)) = dearest(floors) else {
                continue;
            };
            if self
                .funds_cover_at(&row.renewing(), &row.id, row.id.as_str(), estimate)
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

/// The batch's leases grouped by the payer that funds them, in the order each
/// payer first appears.
fn by_payer<'a>(
    batch: &[(&'a Found, &RenewRequest)],
) -> Vec<((&'a Uuid7, &'a Uuid7), Vec<&'a Found>)> {
    let mut groups: Vec<((&Uuid7, &Uuid7), Vec<&Found>)> = Vec::new();
    for &(row, _request) in batch {
        let payer = (&row.tenant_id, &row.workspace_id);
        match groups.iter_mut().find(|(known, _rows)| *known == payer) {
            Some((_payer, rows)) => rows.push(row),
            None => groups.push((payer, vec![row])),
        }
    }
    groups
}

/// The lease whose floor a payer's funds must cover: the dearest, whichever
/// order the frame listed them in.
///
/// A lease whose floor could not be priced never stands in for one that could,
/// so a payer is admitted unchecked only when none of its leases can be priced,
/// and then with the first fault, as an unreadable gate is on the per-lease path.
fn dearest<T>(
    floors: Vec<(T, core::result::Result<Estimate, Arc<crate::Error>>)>,
) -> Option<(T, core::result::Result<Estimate, Arc<crate::Error>>)> {
    let mut chosen: Option<(T, core::result::Result<Estimate, Arc<crate::Error>>)> = None;
    for (row, floor) in floors {
        chosen = Some(match (chosen.take(), floor) {
            (None, floor) => (row, floor),
            (Some((kept_row, Ok(kept))), Ok(floor)) => {
                if floor.floor() > kept.floor() {
                    (row, Ok(floor))
                } else {
                    (kept_row, Ok(kept))
                }
            }
            (Some((_faulted, Err(_fault))), Ok(floor)) => (row, Ok(floor)),
            (Some(kept), Err(_fault)) => kept,
        });
    }
    chosen
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use afd_billing::Nanos;
    use afd_billing::rates::Estimate;

    use super::dearest;

    fn priced(nanos: i64) -> Estimate {
        Estimate::Priced(Nanos::from_i64(nanos))
    }

    fn fault() -> Result<Estimate, Arc<crate::Error>> {
        Err(Arc::new(crate::error::lease_lost()))
    }

    /// A cheap lease listed first never hides the dearer one's floor.
    #[test]
    fn the_dearest_floor_wins_whichever_lease_is_listed_first() {
        let cheap_first = dearest(vec![("cheap", Ok(priced(10))), ("dear", Ok(priced(900)))]);
        let dear_first = dearest(vec![("dear", Ok(priced(900))), ("cheap", Ok(priced(10)))]);
        assert_eq!(cheap_first.map(|(lease, _floor)| lease), Some("dear"));
        assert_eq!(dear_first.map(|(lease, _floor)| lease), Some("dear"));
    }

    /// An unpriceable model costs nothing to admit against, so it never beats a priced one.
    #[test]
    fn an_unpriceable_lease_does_not_hide_a_priced_one() {
        let chosen = dearest(vec![
            ("free", Ok(Estimate::Unpriceable)),
            ("priced", Ok(priced(5))),
        ]);
        assert_eq!(chosen.map(|(lease, _floor)| lease), Some("priced"));
    }

    /// A lease that cannot be priced is passed over for the next that can, so a
    /// permanent fault on one lease does not exempt the payer from the check.
    #[test]
    fn a_fault_never_stands_in_for_a_priced_floor() {
        let chosen = dearest(vec![("broken", fault()), ("priced", Ok(priced(5)))]);
        assert!(matches!(chosen, Some(("priced", Ok(_)))));
    }

    /// Only a payer none of whose leases can be priced is admitted unchecked.
    #[test]
    fn a_payer_with_no_priceable_lease_keeps_the_first_fault() {
        let chosen = dearest(vec![("first", fault()), ("second", fault())]);
        assert!(matches!(chosen, Some(("first", Err(_)))));
        assert!(dearest::<&str>(Vec::new()).is_none());
    }
}
