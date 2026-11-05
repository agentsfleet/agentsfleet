//! What a lease poll spent reaching Postgres and Dragonfly.
//!
//! A suite proving a poll's cost has to assert on something the poll did NOT
//! do, and "no query was issued" leaves no row, no lease and no error behind.
//! The tally the poll already keeps for the operator gauges is the evidence,
//! so this hands the same numbers back to the caller instead of only to
//! `producers::fleet::lease_polled`.
//!
//! Compiled only under `test-util`: the daemon reads these numbers off the
//! meter, never off a return value, and a production caller that wanted them
//! here would be measuring the wrong seam.

use afd_core::id::Uuid7;

use super::PollCost;
use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

/// One poll's cost, in the numbers whose ratios an operator reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollMeasurement {
    /// Postgres statements; an empty poll sends the one claim statement.
    pub database_roundtrips: u64,
    /// Dragonfly commands; an empty poll sends none.
    pub dragonfly_commands: u64,
}

impl From<PollCost> for PollMeasurement {
    fn from(cost: PollCost) -> Self {
        Self {
            database_roundtrips: cost.database_roundtrips,
            dragonfly_commands: cost.dragonfly_commands,
        }
    }
}

impl Leases {
    /// [`Leases::select`], with the cost the same poll recorded.
    ///
    /// The outcome is returned whole rather than unwrapped, because the
    /// failing poll is the interesting one: a claim that reached Postgres and
    /// could not acquire a connection proves the reach.
    pub async fn select_measured(
        &self,
        runner_id: &Uuid7,
        held: &[Uuid7],
    ) -> (Result<Option<Acquired>>, PollMeasurement) {
        let (selected, cost) = self.select_recorded(runner_id, held).await;
        (selected, cost.into())
    }
}

#[cfg(test)]
mod tests {
    use super::PollMeasurement;
    use crate::lease::assign::Tally;

    /// Every statement and command the poll records reaches the measurement:
    /// one claim, seven pass reads, the approval gate's read and the received
    /// frame's publish.
    #[test]
    fn test_lease_polled_counts_every_statement() {
        let tally = Tally::default();
        tally.statements(1);
        for _read in 0..7 {
            tally.statements(1);
        }
        tally.commands(1);
        tally.commands(1);

        assert_eq!(
            PollMeasurement::from(tally.cost()),
            PollMeasurement {
                database_roundtrips: 8,
                dragonfly_commands: 2,
            }
        );
    }
}
