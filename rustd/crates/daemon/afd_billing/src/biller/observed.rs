//! Any biller, counted: the one choke point every funds check passes through,
//! whichever product path made it.

use af_observability::metrics::label::billing::{FundsOutcome, WorkType};
use af_observability::producers::billing as metrics;

use super::{Account, Ask, Balance, Biller, Funds, WorkKey};
use crate::Result;

/// `inner`, with every funds check's answer counted.
///
/// Wraps outside [`super::FailOpen`] so a check it admitted is counted as
/// `unconfirmed` rather than vanishing as an error.
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
    async fn check(&self, ask: &Ask) -> Result<Funds> {
        let answer = self.inner.check(ask).await;
        if let Ok(funds) = &answer {
            let outcome = match funds {
                Funds::Covered => FundsOutcome::Covered,
                Funds::Short { .. } => FundsOutcome::Short,
                Funds::Unconfirmed => FundsOutcome::Unconfirmed,
            };
            metrics::funds_checked(work_type(&ask.work), outcome);
        }
        answer
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        self.inner.balance(payer).await
    }
}

/// The label a work's kind is counted under.
pub(crate) const fn work_type(work: &WorkKey) -> WorkType {
    match work {
        WorkKey::FleetRun(_) => WorkType::FleetRun,
        WorkKey::SetupRun(_) => WorkType::SetupRun,
    }
}

#[cfg(test)]
mod tests {
    use super::work_type;
    use crate::biller::WorkKey;
    use crate::sql;
    use af_core::id::{ENTROPY_LEN, Uuid7};

    /// The metric labels spell the stored values byte for byte.
    ///
    /// `afd_billing` depends on the metrics crate, so the two cannot share a
    /// type; a dashboard joining a work type to its rows needs one spelling.
    #[test]
    fn the_labels_spell_the_stored_values() -> Result<(), Box<dyn std::error::Error>> {
        let id = Uuid7::encode(jiff::Timestamp::UNIX_EPOCH, [1; ENTROPY_LEN])?;
        assert_eq!(
            work_type(&WorkKey::FleetRun(id.clone())).as_str(),
            sql::work::FLEET_RUN
        );
        assert_eq!(
            work_type(&WorkKey::SetupRun(id)).as_str(),
            sql::work::SETUP_RUN
        );
        Ok(())
    }
}
