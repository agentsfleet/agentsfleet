//! Any biller, counted: the one choke point every reservation and settlement
//! passes through, whichever product path or sweeper made it.

use afd_observability::metrics::label::billing::{ReserveOutcome, SettleOutcome, WorkType};
use afd_observability::metrics::label::cost::ChargeClass;
use afd_observability::producers::{billing as metrics, cost};

use super::{Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey};
use crate::Result;

/// `inner`, with every reservation's answer and every settlement's outcome,
/// spend and unpriced tokens recorded.
///
/// Wraps outside [`super::WithOutagePolicy`] so a reservation the policy
/// admitted is counted as `unconfirmed` rather than vanishing as an error.
#[derive(Debug)]
pub struct Observed<B> {
    inner: B,
}

impl<B> Observed<B> {
    /// `inner`, counted.
    #[must_use]
    pub const fn new(inner: B) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl<B: Biller> Biller for Observed<B> {
    async fn reserve(&self, ask: &Reservation) -> Result<Hold> {
        let answer = self.inner.reserve(ask).await;
        if let Ok(hold) = &answer {
            let outcome = match hold {
                Hold::Held { .. } => ReserveOutcome::Held,
                Hold::Short { .. } => ReserveOutcome::Short,
                Hold::Closed => ReserveOutcome::Closed,
                Hold::Unconfirmed => ReserveOutcome::Unconfirmed,
            };
            metrics::reserved(work_type(&ask.work), outcome);
        }
        answer
    }

    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        let answer = self.inner.settle(settlement).await;
        let work = work_type(&settlement.work);
        match &answer {
            Ok(receipt) => {
                metrics::settled(work, SettleOutcome::Ok);
                record_spend(settlement, receipt);
            }
            Err(fault) if fault.is_unanswered() => {
                metrics::settled(work, SettleOutcome::DatastoreUnavailable);
            }
            Err(_defect) => metrics::settled(work, SettleOutcome::Defect),
        }
        answer
    }

    async fn release(&self, work: &WorkKey, why: Release) -> Result<()> {
        self.inner.release(work, why).await
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        self.inner.balance(payer).await
    }
}

/// What one landed settlement spent and left unpriced.
///
/// Spend is what left the balance (`taken`): the whole charge, or nothing for a
/// payer with no wallet to debit. Unpriced tokens count only when this
/// call charged lines, so a sweeper's retry of lines already settled adds
/// nothing.
fn record_spend(settlement: &Settlement, receipt: &Receipt) {
    if receipt.settled_lines == 0 {
        return;
    }
    let posture = settlement.posture.as_str();
    if let Ok(taken) = u64::try_from(receipt.taken.as_i64()) {
        cost::credits_consumed(&settlement.model, posture, ChargeClass::Settle, taken);
    }
    let unpriced = settlement
        .lines
        .iter()
        .fold(0_u64, |sum, line| sum.saturating_add(line.unpriced_tokens));
    metrics::unpriced_tokens(&settlement.provider, &settlement.model, unpriced);
}

/// The label a work's kind is counted under.
pub(crate) const fn work_type(work: &WorkKey) -> WorkType {
    match work {
        WorkKey::FleetLease(_) => WorkType::FleetLease,
        WorkKey::AuthoringAttempt(_) => WorkType::AuthoringAttempt,
    }
}

#[cfg(test)]
mod tests {
    use super::work_type;
    use crate::biller::WorkKey;
    use crate::sql;
    use afd_core::id::{ENTROPY_LEN, Uuid7};

    /// The metric labels spell the stored values byte for byte.
    ///
    /// `afd_billing` depends on the metrics crate, so the two cannot share a
    /// type; a dashboard joining a work type to its rows needs one spelling.
    #[test]
    fn the_labels_spell_the_stored_values() {
        let id = Uuid7::encode(jiff::Timestamp::UNIX_EPOCH, [1; ENTROPY_LEN])
            .unwrap_or_else(|_unencodable| unreachable!("the epoch encodes"));
        assert_eq!(
            work_type(&WorkKey::FleetLease(id.clone())).as_str(),
            sql::work::FLEET_LEASE
        );
        assert_eq!(
            work_type(&WorkKey::AuthoringAttempt(id)).as_str(),
            sql::work::AUTHORING_ATTEMPT
        );
    }
}
