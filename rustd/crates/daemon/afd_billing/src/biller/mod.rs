//! The one billing lifecycle every product path runs through.
//!
//! Work checks that its payer can fund what it is about to spend
//! ([`Biller::check`]) and logs its priced lines in its own transaction
//! ([`crate::ledger::log_lines`]); writing a line debits its payer's wallet in
//! the same statement. No money is set aside: a payer that runs out is stopped
//! at its next check, and what parallel work spent before then is a debt the
//! next grant repays. The built-in implementation is
//! [`LedgerBiller`](crate::ledger::LedgerBiller); a billing service in its own
//! process or a third-party metering system is another implementation of the
//! same trait, held as `Arc<dyn Biller>`, per the billing design.
//!
//! `billing.usage_lines` records what happened (always ours), and each line is
//! charged once, on its `(work, kind, seq)`, so a retried write never charges
//! twice.

use af_core::id::Uuid7;
use jiff::Timestamp;

use crate::price::RatedLine;
use crate::{Nanos, Posture, Result, sql};

pub(crate) mod observed;
mod outage;

pub use self::observed::Observed;
pub use self::outage::FailOpen;

#[cfg(feature = "test-util")]
pub mod conformance;
#[cfg(feature = "test-util")]
mod recording;
#[cfg(feature = "test-util")]
pub use self::recording::RecordingBiller;

/// The piece of work money is charged for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkKey {
    /// A fleet run's run.
    FleetRun(Uuid7),
}

impl WorkKey {
    /// The stored spelling of this work's kind (`billing.*.work_type`).
    #[must_use]
    pub const fn work_type(&self) -> &'static str {
        match self {
            Self::FleetRun(_) => sql::work::FLEET_RUN,
        }
    }

    /// The work's own identifier.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        match self {
            Self::FleetRun(id) => id,
        }
    }
}

/// Who pays, and where the work ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The paying tenant: the workspace owner's account.
    pub tenant: Uuid7,
    /// The workspace the work ran in.
    pub workspace: Uuid7,
}

/// What a charge names beside its work, for the charges page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribution {
    /// A fleet run: the fleet, its name at charge time, and the turn it ran.
    Fleet {
        /// The fleet.
        fleet: Uuid7,
        /// The fleet's name when the line was written.
        fleet_name: String,
        /// The turn the run answered.
        turn_id: Uuid7,
    },
}

/// A request to check that `payer` can fund `next` more for `work`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// The work about to spend.
    pub work: WorkKey,
    /// Who pays.
    pub payer: Account,
    /// What the work is about to spend before its next check.
    pub next: Nanos,
}

/// A biller's answer to an [`Ask`].
///
/// An ordinary answer rather than an error: a short balance is a decision the
/// caller acts on, not a failure (FN-RS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Funds {
    /// The balance covers the ask.
    Covered,
    /// It cannot cover the ask.
    Short {
        /// What the payer has left.
        available: Nanos,
        /// What was asked for.
        asked: Nanos,
    },
    /// The biller could not answer and the outage policy admitted the work.
    Unconfirmed,
}

impl Funds {
    /// The answer for a payer with `left`: covered only when `left` reaches
    /// `asked`, so a payer in debt is short even of nothing.
    #[must_use]
    pub fn of(left: Nanos, asked: Nanos) -> Self {
        if left.as_i64() >= asked.as_i64() {
            Self::Covered
        } else {
            Self::Short {
                available: Nanos::from_i64(left.as_i64().max(0)),
                asked,
            }
        }
    }
}

/// Priced lines of one piece of work, as [`crate::ledger::log_lines`] writes
/// and charges them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    /// The work the lines belong to.
    pub work: WorkKey,
    /// Who pays.
    pub payer: Account,
    /// What the charge names.
    pub attribution: Attribution,
    /// The model's provider key.
    pub provider: String,
    /// The provider's model identifier.
    pub model: String,
    /// Whose key paid for the tokens.
    pub posture: Posture,
    /// Each line's usage and its price.
    pub lines: Vec<RatedLine>,
}

/// A payer's balance as a biller sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    /// Credit left, every logged line already debited: negative while the
    /// payer owes a debt its usage ran past zero; a grant repays the debt
    /// before any of it is spendable.
    pub balance: Nanos,
    /// When the balance last reached zero or went below it; `None` while it
    /// holds credit.
    pub exhausted_at: Option<Timestamp>,
}

/// Checks funds for work, per the billing design.
///
/// Neither method writes: logging a line is what charges it.
#[async_trait::async_trait]
pub trait Biller: Send + Sync + std::fmt::Debug {
    /// Whether `ask.payer`'s balance can fund `ask.next`.
    ///
    /// # Errors
    /// The biller could not answer. [`FailOpen`] admits the work then.
    async fn check(&self, ask: &Ask) -> Result<Funds>;

    /// `payer`'s balance.
    ///
    /// # Errors
    /// The biller could not answer, or the payer has no balance at all.
    async fn balance(&self, payer: &Account) -> Result<Balance>;
}
