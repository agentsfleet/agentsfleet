//! The one billing lifecycle every product path runs through.
//!
//! Work checks that its payer can fund what it is about to spend
//! ([`Biller::check`]) and logs its priced lines in its own transaction; the
//! billing relay charges them ([`Biller::settle`]). No money is set aside: a
//! payer that runs out is stopped at its next check, and what parallel work
//! spent before then is a debt the next grant repays. The built-in
//! implementation is [`LedgerBiller`](crate::ledger::LedgerBiller); a billing
//! service in its own process or a third-party metering system is another
//! implementation of the same trait, held as `Arc<dyn Biller>` (billing §11).
//!
//! `billing.usage_lines` records what happened (always ours); a biller moves
//! money from it. `settle` is idempotent on each line's `(work, seq)`, so a
//! retried or repeated relay pass never charges twice.

use af_core::id::Uuid7;
use jiff::Timestamp;

use crate::price::RatedLine;
use crate::{Nanos, Posture, Result, sql};

pub(crate) mod observed;
mod outage;

pub use self::observed::Observed;
pub use self::outage::{OutagePolicy, WithOutagePolicy};

#[cfg(feature = "test-util")]
pub mod conformance;
#[cfg(feature = "test-util")]
mod recording;
#[cfg(feature = "test-util")]
pub use self::recording::RecordingBiller;

/// The piece of work money is charged for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorkKey {
    /// A fleet run's lease.
    FleetLease(Uuid7),
    /// A setup-chat attempt.
    AuthoringAttempt(Uuid7),
}

impl WorkKey {
    /// The stored spelling of this work's kind (`billing.*.work_type`).
    #[must_use]
    pub const fn work_type(&self) -> &'static str {
        match self {
            Self::FleetLease(_) => sql::work::FLEET_LEASE,
            Self::AuthoringAttempt(_) => sql::work::AUTHORING_ATTEMPT,
        }
    }

    /// The work's own identifier.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        match self {
            Self::FleetLease(id) | Self::AuthoringAttempt(id) => id,
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

/// What a charge names beside its work, for the charges page and budgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribution {
    /// A fleet run: the fleet, its name at charge time, and the event it ran.
    Fleet {
        /// The fleet.
        fleet: Uuid7,
        /// The fleet's name when the line was written.
        fleet_name: String,
        /// The event the run answered.
        event_id: Uuid7,
    },
    /// A setup-chat attempt; the attempt is the work itself.
    SetupChat,
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
    /// The balance, less what is logged and not yet charged, covers the ask.
    Covered,
    /// It cannot cover the ask.
    Short {
        /// What the payer has left once its logged lines are charged.
        available: Nanos,
        /// What was asked for.
        asked: Nanos,
    },
    /// The biller could not answer and the outage policy admitted the work.
    Unconfirmed,
}

impl Funds {
    /// The answer for a payer with `left` once its unsettled lines are
    /// charged: covered only when `left` reaches `asked`, so a payer in debt is
    /// short even of nothing.
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

/// Priced lines of one piece of work, ready to charge.
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

/// What one [`Biller::settle`] charged.
///
/// `charged` is what the newly settled lines cost; `taken` is what left the
/// payer's balance. A balance takes the whole charge even past zero, so the two
/// differ only for a payer with no wallet to debit, where nothing is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Receipt {
    /// Lines this call charged; lines charged before are not counted again.
    pub settled_lines: usize,
    /// What the newly settled lines cost.
    pub charged: Nanos,
    /// What this call took from the balance: all of `charged`, or nothing
    /// when the payer has no wallet.
    pub taken: Nanos,
}

/// A payer's balance as a biller sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance {
    /// Credit left, negative while the payer owes a debt a settlement ran past
    /// zero; a grant repays the debt before any of it is spendable.
    pub balance: Nanos,
    /// What the payer's logged lines cost and the relay has not yet charged.
    pub unsettled: Nanos,
    /// When the balance last reached zero or went below it; `None` while it
    /// holds credit.
    pub exhausted_at: Option<Timestamp>,
}

impl Balance {
    /// What the payer has left once its unsettled lines are charged: the one
    /// number every funds check and gate reads, below zero while it owes.
    #[must_use]
    pub const fn left(&self) -> Nanos {
        Nanos::from_i64(
            self.balance
                .as_i64()
                .saturating_sub(self.unsettled.as_i64()),
        )
    }
}

/// Checks funds and charges money for work (billing §11).
///
/// `check` writes nothing; `settle` is idempotent on each line's
/// `(work, seq)`.
#[async_trait::async_trait]
pub trait Biller: Send + Sync + std::fmt::Debug {
    /// Whether `ask.payer` can fund `ask.next` on top of every line it has
    /// logged and not yet been charged for.
    ///
    /// # Errors
    /// The biller could not answer. [`WithOutagePolicy`] decides what the work
    /// does then.
    async fn check(&self, ask: &Ask) -> Result<Funds>;

    /// Charges `settlement`'s lines that were not charged before.
    ///
    /// # Errors
    /// The biller could not answer; the lines stay unsettled for the next
    /// relay pass.
    async fn settle(&self, settlement: &Settlement) -> Result<Receipt>;

    /// `payer`'s balance and what its logged lines still owe.
    ///
    /// # Errors
    /// The biller could not answer, or the payer has no balance at all.
    async fn balance(&self, payer: &Account) -> Result<Balance>;
}
