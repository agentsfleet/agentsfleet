//! The one transaction a terminal report commits, and what it answers.
//!
//! Five writes with one fate: the money, the result, the session cursor, the
//! freed slot, and the delivery the answer is owed. Before §7 the first four
//! were a settle that committed on its own
//! followed by four best-effort writes that each logged their own failure, and
//! the gap between them is where a run's answer could be lost for good — the
//! wallet drawn down, the lease flipped to `reported`, and `core.fleet_events`
//! still holding the row as `received` with no response in it. Nothing
//! recovers that: the runner's retry is refused because the lease is no longer
//! active, and the tenant has paid for an answer the platform cannot produce.
//!
//! # Why the money still goes first INSIDE the transaction
//!
//! [`super::report`] explains why the settle precedes everything else, and the
//! reason survives the change: a run that reaches `MAX_RUNTIME_MS` races the
//! reclaim sweep, and `CLAIM_AND_SETTLE` takes `FOR UPDATE OF l, a` — the
//! affinity row lock that holds the sweep off until this commits. Running it
//! first now holds that lock for the whole transaction rather than for one
//! statement, which is strictly stronger: the sweep that would have bumped the
//! fence cannot interleave anywhere in here, not merely between two statements.
//!
//! The cost is that a fleet's affinity row is locked across three more
//! same-fleet writes. They are keyed writes on rows this transaction already
//! holds or has no contender for, which is why the lock is affordable; a
//! statement added here that reaches a table under general contention would
//! not be.
//!
//! # Rollback is the language's, not a compensating write
//!
//! `sqlx::Transaction` rolls back when it is DROPPED, so every `?` below
//! unwinds all five writes with no rollback path of its own — the argument
//! `afd_connector`'s grant install already makes, and the reason RULE TXN's
//! "every failure branch must ROLLBACK" is satisfied here without a `match`
//! per statement.

use afd_billing::{Meter, Nanos};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_events::Closed;
use sqlx::Acquire as _;

use crate::error::{Result, query};
use crate::lease::obligation::Owing;
use crate::lease::settle::{Reported, Settled};
use crate::lease::store::Leases;
use crate::lease::verdict::Terminal;
use afd_outbound::obligation::Delivery;

/// Statement name, for the context a transaction failure carries.
const CONTEXT_COMMIT: &str = "report commit";

/// Everything one terminal report commits, by name.
///
/// A struct rather than eight positional parameters for the reason
/// [`SettleRow`](crate::lease::sql::report::SettleRow) is one: two of these are
/// `&str` and two are instants, and a transposition between them writes a
/// checkpoint under a response and compiles clean.
#[derive(Debug)]
pub struct TerminalReport<'a> {
    /// The lease being reported on.
    pub lease_id: &'a str,
    /// The runner presenting the report — the ownership scope of every
    /// statement below.
    pub runner_id: &'a Uuid7,
    /// The lease as the row holds it: the fleet, the money identities, the
    /// event, and the token the settle is fenced on.
    pub lease: &'a Reported,
    /// What the final slice is priced from.
    pub meter: Meter,
    /// The run's ending, as the event row will carry it.
    pub outcome: Terminal<'a>,
    /// The event the fleet's next run resumes after.
    pub last_event_id: &'a str,
    /// The answer that run resumes from.
    pub last_response: &'a str,
    /// The instant every row this transaction writes is stamped with.
    pub now: UnixMillis,
}

/// What the report's transaction decided.
///
/// Three arms, and the charge lives on exactly one of them — the same property
/// [`Settled`] holds, carried up so a caller cannot read an amount off a report
/// that committed nothing. The closed row rides [`Committed::Settled`] rather
/// than being read again afterwards: the terminal write already returned it,
/// and re-reading would answer about a row a concurrent reclaim may have moved
/// on from.
#[derive(Debug)]
pub enum Committed {
    /// The report won its fence and all five writes committed. The final slice
    /// drained `charged`; `closed` is the row the terminal write ended, absent
    /// when the event was already terminal and there was no new ending to
    /// announce.
    Settled {
        /// What the final slice drained.
        charged: Nanos,
        /// The row this report ended, for the frame the caller announces.
        ///
        /// Boxed because a closing carries the whole event row and the fleet's
        /// counters beside it, and an enum sized to its largest arm would make
        /// every fenced answer that size too. `Option<Box<_>>` rather than
        /// `Box<Option<_>>`: a report that closed nothing allocates nothing.
        closed: Option<Box<Closed>>,
        /// The delivery obligation this report newly owed, and where, for the
        /// caller to append and receipt.
        ///
        /// `None` when the settled event was admitted with no destination, when
        /// the run answered nothing, and on a repeat that conflicted — so a
        /// caller cannot append an entry for an answer nobody asked for, or one
        /// already owed and in flight. Carried up rather than appended in here
        /// because the append is not part of the transaction and must not be
        /// able to fail it.
        owed: Option<Owing>,
    },
    /// This runner already settled this lease and the earlier report committed
    /// everything below. Nothing was written and nothing charged; what the
    /// runner is owed is the acknowledgement its lost response never carried.
    AlreadySettled,
    /// A newer holder has the fleet. Nothing was written and nothing charged.
    Fenced,
}

impl Leases {
    /// Commit one terminal report: the money, the result, the cursor, the slot.
    ///
    /// One transaction, in that order, on one pooled connection. The queue is
    /// NOT acknowledged here and cannot be — see [`super::finalize`] — so a
    /// caller that returns success without acknowledging afterwards leaves the
    /// entry pending and redelivered, which is the safe direction.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the ledger row's
    /// identifier, an instant that cannot be encoded, and a datastore that
    /// would not answer at any of the five statements — in which case nothing
    /// at all was written, including the charge and the obligation.
    pub async fn commit_report(&self, report: TerminalReport<'_>) -> Result<Committed> {
        let TerminalReport {
            lease_id,
            runner_id,
            lease,
            meter,
            outcome,
            last_event_id,
            last_response,
            now,
        } = report;

        // Read before `outcome` moves into the terminal write below. This is
        // the run's OUTPUT, which is what a destination receives — never
        // `last_response`, which is the session checkpoint and is truncated to
        // fit one, so delivering it would silently cut a long answer off at the
        // byte cap.
        let answer = outcome.response_text;

        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_COMMIT))?;

        let charged = match self
            .claim_and_settle(
                &mut transaction,
                lease_id,
                runner_id,
                meter,
                outcome.verdict.succeeded(),
                now,
            )
            .await?
        {
            Settled::Claimed(nanos) => nanos,
            Settled::AlreadySettled => return Ok(Committed::AlreadySettled),
            Settled::Fenced => return Ok(Committed::Fenced),
        };

        let closed = self
            .mark_terminal(
                &mut transaction,
                &lease.fleet_id,
                &lease.event_id,
                outcome,
                now,
            )
            .await?
            .map(Box::new);
        self.checkpoint(
            &mut transaction,
            &lease.fleet_id,
            last_event_id,
            last_response,
            now,
        )
        .await?;
        self.release_through(&mut transaction, &lease.fleet_id, lease.fence, now)
            .await?;
        // The fifth write, and the one that closes 7.6's window: the answer is
        // owed before anything tries to send it, so a process that dies between
        // here and the queue append leaves a record rather than a charged run
        // whose answer exists nowhere. The append itself is NOT here — see
        // `obligation` for why it cannot be.
        //
        // Owed only to where the question came from. The lease's own
        // `provider` is the MODEL provider billing resolved, and it has no path
        // in here: `Delivery` takes a connector type, which only the event's
        // recorded destination supplies.
        let destination =
            Leases::reply_destination(&mut transaction, lease.fleet_id.as_str(), &lease.event_id)
                .await?;
        let owed = match destination {
            Some(reply) => self
                .owe_delivery(
                    &mut transaction,
                    Delivery {
                        fleet_id: lease.fleet_id.as_str(),
                        workspace_id: lease.workspace_id.as_str(),
                        provider: reply.provider,
                        destination: &reply.address,
                        event_id: &lease.event_id,
                        answer,
                    },
                    now,
                )
                .await?
                .map(|obligation| Owing { obligation, reply }),
            None => None,
        };

        transaction.commit().await.map_err(query(CONTEXT_COMMIT))?;
        Ok(Committed::Settled {
            charged,
            closed,
            owed,
        })
    }
}
