//! The one transaction a terminal report commits, and what it answers.
//!
//! Usage, admission settlement, result, checkpoint, slot release, and outbound
//! obligation commit together. Metering locks the slot and lease before testing
//! their fences, so a reclaim cannot interleave with the remaining writes.
//! Dropping the transaction on any error rolls every write back.

use af_core::id::Uuid7;
use afd_billing::biller::Settlement;
use afd_events::Closed;
use jiff::Timestamp;
use sqlx::{Acquire as _, PgConnection};

use crate::error::{Result, query};
use crate::lease::fold::Folds;
use crate::lease::obligation::Owing;
use crate::lease::settle::{Reported, Settled};
use crate::lease::sql::meter::Metering;
use crate::lease::store::Leases;
use crate::lease::usage::Pricing;
use crate::lease::verdict::Terminal;

/// Statement name, for the context a transaction failure carries.
const CONTEXT_COMMIT: &str = "report commit";

/// Everything one terminal report commits, by name.
///
/// A struct rather than eight positional parameters for the reason
/// [`Metering`](crate::lease::sql::meter::Metering) is one: two of these are
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
    /// The lease's identifier as the work its usage lines name.
    pub lease_key: &'a Uuid7,
    /// One line per provider request no renewal acknowledged.
    pub requests: &'a [af_wire::usage::RequestUsage],
    /// What the final slice is priced against.
    pub pricing: &'a Pricing,
    /// The run's ending, as the event row will carry it.
    pub outcome: Terminal<'a>,
    /// The event the fleet's next run resumes after.
    pub last_event_id: &'a str,
    /// The answer that run resumes from.
    pub last_response: &'a str,
    /// When the sandbox the run left held for the fleet's next event lapses;
    /// none when the runner holds nothing.
    pub held_until: Option<Timestamp>,
    /// Native task participant, prepared before taking report locks.
    pub task_settlement: Option<crate::task::TaskSettlement<'a>>,
    /// The last folded message a completed model call read; its messages
    /// settle with this answer when the run succeeded.
    pub folded_through_sequence: Option<i64>,
    /// The instant every row this transaction writes is stamped with.
    pub now: Timestamp,
}

/// What the report's transaction decided.
///
/// Three arms, and the usage line lives on exactly one of them — the same
/// property [`Settled`] holds, carried up so a caller cannot settle a line off
/// a report that committed nothing. The closed row rides [`Committed::Settled`] rather
/// than being read again afterwards: the terminal write already returned it,
/// and re-reading would answer about a row a concurrent reclaim may have moved
/// on from.
#[derive(Debug)]
pub enum Committed {
    /// The report won its fence and all its writes committed, the final
    /// slice's usage line among them; `closed` is the row the terminal write
    /// ended, absent
    /// when the event was already terminal and there was no new ending to
    /// announce.
    Settled {
        /// The final slice's usage line, logged and still to be settled; `None`
        /// when the last window measured nothing.
        settlement: Option<Box<Settlement>>,
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
        /// Native replacement for a stale or unverified completion claim.
        notice: Option<&'static str>,
        /// The messages the run folded and read, settled with its answer.
        folds: Folds,
    },
    /// This runner already settled this lease and the earlier report committed
    /// everything below. Nothing was written and nothing logged; what the
    /// runner is owed is the acknowledgement its lost response never carried.
    AlreadySettled,
    /// A newer holder has execution. Known original usage can still be recorded.
    Fenced {
        /// Original known requests commit independently of execution authority.
        settlement: Option<Box<Settlement>>,
    },
}

impl Leases {
    /// Commit a terminal report and settle its admission on one pooled connection.
    ///
    /// The host's capacity lock comes first, as it does for a renewal
    /// ([`Leases::extend`]): host row, then lease and slot rows. The metering
    /// statement numbers its runtime line from the lines it counts, and under
    /// READ COMMITTED a statement that waits on the lease row keeps its
    /// starting snapshot for that count. Waiting on the host row in its own
    /// statement means the metering statement starts after a racing renewal
    /// committed, so it counts the renewal's line. The capacity read itself
    /// gates nothing: a report from a cordoned, revoked or removed host still
    /// settles, and the fence alone decides whether it wins.
    ///
    /// # Errors
    /// Reports an entropy source that could not produce the usage line's
    /// identifier, an instant that cannot be encoded, and a datastore that
    /// would not answer at any statement — in which case nothing at all was
    /// written, including the usage line and the obligation.
    pub async fn commit_report(&self, report: TerminalReport<'_>) -> Result<Committed> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_COMMIT))?;
        crate::task::lease::report_parents(
            &mut transaction,
            &report.lease.fleet_id,
            &report.lease.workspace_id,
        )
        .await?;
        let _held_until_commit =
            afd_runner::capacity::lock(&mut transaction, report.runner_id).await?;

        let metering = Metering {
            lease_id: report.lease_id,
            runner_id: report.runner_id,
            requests: report.requests,
        };
        let settlement = match self
            .claim_and_settle(
                &mut transaction,
                metering,
                report.lease_key,
                report.pricing,
                report.outcome.verdict.succeeded(),
                report.now,
            )
            .await?
        {
            Settled::Claimed(settlement) => settlement.map(Box::new),
            Settled::AlreadySettled => return Ok(Committed::AlreadySettled),
            Settled::Fenced => {
                let settlement = report
                    .pricing
                    .original_requests(
                        &mut transaction,
                        report.lease_key,
                        report.runner_id,
                        report.requests,
                    )
                    .await?;
                super::usage::log_measured(
                    &mut transaction,
                    self.entropy(),
                    report.now,
                    settlement.as_ref(),
                )
                .await?;
                transaction.commit().await.map_err(query(CONTEXT_COMMIT))?;
                return Ok(Committed::Fenced {
                    settlement: settlement.map(Box::new),
                });
            }
        };
        let (closed, owed, notice, folds) = self.record_settled(&mut transaction, report).await?;

        transaction.commit().await.map_err(query(CONTEXT_COMMIT))?;
        Ok(Committed::Settled {
            settlement,
            closed,
            owed,
            notice,
            folds,
        })
    }

    /// The writes a settled report owes after its money, inside its
    /// transaction: the result, the session cursor, the freed slot, and the
    /// delivery the answer is owed. Answers the row the result closed and the
    /// obligation newly owed.
    async fn record_settled(
        &self,
        connection: &mut PgConnection,
        report: TerminalReport<'_>,
    ) -> Result<(
        Option<Box<Closed>>,
        Option<Owing>,
        Option<&'static str>,
        Folds,
    )> {
        let TerminalReport {
            lease_id,
            lease,
            mut outcome,
            last_event_id,
            last_response,
            held_until,
            task_settlement,
            folded_through_sequence,
            lease_key,
            runner_id,
            now,
            ..
        } = report;

        // Read before `outcome` moves into the terminal write below. This is
        // the run's OUTPUT, which is what a destination receives — never
        // `last_response`, which is the session checkpoint and is truncated to
        // fit one, so delivering it would silently cut a long answer off at the
        // byte cap.
        let notice =
            crate::task::settle_report(connection, lease_key, runner_id, task_settlement).await?;
        if let Some(notice) = notice {
            outcome.response_text = notice;
        }
        let answer = outcome.response_text;
        let succeeded = outcome.verdict.succeeded();
        let last_response = notice.unwrap_or(last_response);

        let closed = self
            .mark_terminal(connection, &lease.fleet_id, &lease.event_id, outcome)
            .await?
            .map(Box::new);
        // The settling fence's records describe the run whose answer stands;
        // a reclaimed lease's are of a run that did not, so they go with it.
        self.drop_other_fences(connection, lease).await?;
        self.checkpoint(connection, &lease.fleet_id, last_event_id, last_response)
            .await?;
        self.release_through(connection, &lease.fleet_id, lease.fence, held_until)
            .await?;
        let owed = self.owe_answer(connection, lease, answer, now).await?;
        // A failed or stopped run's folds settle nothing: each message stays
        // for the fleet's next run, in order.
        let read_through = folded_through_sequence.filter(|_| succeeded);
        let folds = self
            .settle_folds(connection, lease, lease_id, read_through, answer)
            .await?;
        Ok((closed, owed, notice, folds))
    }

    /// Owes the run's answer to where its question came from.
    ///
    /// The fifth write, and the one that closes the window between the charge
    /// and the send: the answer is owed before anything tries to send it, so a
    /// process that dies in between leaves a record rather than a charged run
    /// whose answer exists nowhere. The append itself is NOT here — see
    /// `obligation` for why it cannot be.
    ///
    /// The lease's own `provider` is the MODEL provider billing resolved, and
    /// it has no path in here: `Delivery` takes a connector type, which only
    /// the event's recorded destination supplies. Answers the obligation
    /// newly owed to it. A folded message is in the run's conversation, so
    /// this one reply reaches it too (`claim_inputs.sql`).
    async fn owe_answer(
        &self,
        connection: &mut PgConnection,
        lease: &Reported,
        answer: &str,
        now: Timestamp,
    ) -> Result<Option<Owing>> {
        let destination =
            Self::reply_destination(connection, lease.fleet_id.as_str(), &lease.event_id).await?;
        let owed = match &destination {
            Some(reply) => self
                .owe_delivery(connection, reply.delivery(lease, answer), now)
                .await?
                .map(|obligation| Owing {
                    obligation,
                    reply: reply.clone(),
                }),
            None => None,
        };
        Ok(owed)
    }
}
