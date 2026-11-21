//! Stop: a person ends one turn of any thread, a fleet's or a setup draft's.
//!
//! One statement ([`STOP_TURN`]) decides everything in the caller's
//! transaction: it cancels the turn, marks the live run serving it, cancels
//! every other turn that run serves, and closes the turn's open approval. The
//! durable mark commits first; [`announce`] then sends each marked run's
//! runner the lossy `Command::Cancel` hint, and a lost hint is caught by the
//! run's next renewal, which reads the same mark
//! (`afd_bus/src/command.rs`).
//!
//! A fleet reaches this through [`History::stop`](crate::view::History::stop);
//! a setup draft runs [`stop_on`] inside the transaction that already holds
//! the draft, because its creator-private access is checked there.

use af_core::id::Uuid7;
use sqlx::PgConnection;

/// The statement Stop runs. Public so the plan suite can `EXPLAIN` it.
pub const STOP_TURN: &str = include_str!("stop_turn.sql");

/// Who a closed approval records as its resolver: the person's Stop, not a
/// reviewer's decision.
const CLOSED_BY: &str = "stop";

/// What a closed approval records as its detail.
const CLOSED_DETAIL: &str = "The turn this approval waited on was stopped";

/// One live run Stop marked, and the runner holding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    /// The runner to send the Cancel hint to.
    pub runner: Uuid7,
    /// The run it ends.
    pub run: Uuid7,
}

/// What one Stop decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// This Stop cancelled the turn.
    Cancelled {
        /// Whether a live run served the turn, so a runner ends it.
        running: bool,
        /// The runs this Stop marked, each owed a Cancel hint after commit.
        runs: Vec<Marked>,
        /// How many turns it cancelled: the turn, and every other its run serves.
        turns: i64,
        /// How many open approvals it closed, the waiting turn's.
        approvals: i64,
    },
    /// The turn was already cancelled, so nothing changed.
    AlreadyCancelled,
    /// The turn finished before Stop reached it; its stored status.
    Finished(String),
}

/// One Stop, and the sequence of the turn it named, which a setup draft's
/// conversation names its turns by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    /// The turn's sequence on its thread.
    pub sequence: i64,
    /// What Stop decided.
    pub stop: Stop,
}

/// [`STOP_TURN`]'s one row, by column name.
#[derive(sqlx::FromRow)]
struct Decided {
    sequence: i64,
    status: String,
    settled: bool,
    running: bool,
    runners: Vec<Uuid7>,
    runs: Vec<Uuid7>,
    cancelled: i64,
    closed: i64,
}

/// Stops `turn` of `thread` in `workspace`, inside the caller's transaction:
/// `None` when the workspace holds no such turn on that thread.
///
/// # Errors
/// The statement's own failure, for the caller to name in its own error.
pub async fn stop_on(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    thread: &Uuid7,
    turn: &Uuid7,
) -> Result<Option<Stopped>, sqlx::Error> {
    let decided: Option<Decided> = sqlx::query_as(STOP_TURN)
        .bind(workspace)
        .bind(thread)
        .bind(turn)
        .bind(af_core::turn::status::CANCELLED)
        .bind(afd_state::sql::RUN_STATUS_ACTIVE)
        .bind(afd_api_wire::approval::status::ARCHIVED)
        .bind(CLOSED_BY)
        .bind(CLOSED_DETAIL)
        .fetch_optional(connection)
        .await?;
    Ok(decided.map(|decided| Stopped {
        sequence: decided.sequence,
        stop: decided.stop(),
    }))
}

impl Decided {
    fn stop(self) -> Stop {
        if !self.settled {
            let runs = self
                .runners
                .into_iter()
                .zip(self.runs)
                .map(|(runner, run)| Marked { runner, run })
                .collect();
            return Stop::Cancelled {
                running: self.running,
                runs,
                turns: self.cancelled,
                approvals: self.closed,
            };
        }
        if self.status == af_core::turn::status::CANCELLED {
            Stop::AlreadyCancelled
        } else {
            Stop::Finished(self.status)
        }
    }
}

/// Sends each marked run's runner the Cancel hint, after the commit that
/// marked it. Lossy by design: the durable mark refuses the run's next
/// renewal whether or not the hint arrives.
pub async fn announce(bus: &afd_bus::Handle, runs: impl IntoIterator<Item = Marked>) {
    for Marked { runner, run } in runs {
        let subject = afd_bus::Subject::RunnerCommand(runner.clone());
        let publication = afd_bus::Command::Cancel { run_id: run }
            .publish(bus, &runner)
            .await;
        afd_bus::observe_notification(&subject, publication);
    }
}

#[cfg(test)]
mod tests {
    use super::{Decided, Marked, Stop};
    use af_core::id::Uuid7;

    type Outcome = Result<(), Box<dyn std::error::Error>>;

    fn id(last: u8) -> Result<Uuid7, af_core::error::Error> {
        Uuid7::parse(&format!("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb0{last:02x}"))
    }

    fn decided(status: &str, settled: bool) -> Result<Decided, af_core::error::Error> {
        Ok(Decided {
            sequence: 3,
            status: status.to_owned(),
            settled,
            running: true,
            runners: vec![id(1)?],
            runs: vec![id(2)?],
            cancelled: 2,
            closed: 0,
        })
    }

    #[test]
    fn an_unsettled_turn_is_cancelled_with_the_runs_it_marked() -> Outcome {
        assert_eq!(
            decided("received", false)?.stop(),
            Stop::Cancelled {
                running: true,
                runs: vec![Marked {
                    runner: id(1)?,
                    run: id(2)?,
                }],
                turns: 2,
                approvals: 0,
            }
        );
        Ok(())
    }

    #[test]
    fn a_settled_turn_answers_its_state() -> Outcome {
        assert_eq!(decided("cancelled", true)?.stop(), Stop::AlreadyCancelled);
        for finished in [
            "processed",
            "fleet_error",
            "completed",
            "failed",
            "recorded",
        ] {
            assert_eq!(
                decided(finished, true)?.stop(),
                Stop::Finished(finished.to_owned())
            );
        }
        Ok(())
    }
}
