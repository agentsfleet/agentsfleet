//! One frame: which runs the statement renews, which are refused before it
//! runs, and what its rows decided.

use std::collections::HashMap;

use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use jiff::Timestamp;

use super::ceilings::Read;
use super::columns::{Bound, Columns};
use super::rows::Found;
use super::verdict::{Answer, Judged, Verdict, log_refused, refusal_of};
use crate::error::{Result, query, renewal_no_credits, run_not_found};
use crate::run::coverage::refuse_unreadable_budget;
use crate::run::pull::RunService;

/// What query failures name the batch statement as.
const CONTEXT_RENEW: &str = "renewal batch";

/// Logged when a run's prices or ceilings moved under its statement.
const EVENT_REPRICED: &str = "renew_batch_repriced";

impl RunService {
    /// The frame's answers by run, or a fault before any run was judged.
    pub(super) async fn renew_frame(
        &self,
        runner_id: &Uuid7,
        runs: &[(Uuid7, RenewRequest)],
        kept: &HashMap<Uuid7, crate::run::Reported>,
        funds_due: bool,
        now: Timestamp,
    ) -> Result<HashMap<Uuid7, Answer>> {
        let found = match self.kept_rows(runs, kept).await {
            Some(found) => found,
            None => self.read_renewals(runner_id, runs).await?,
        };
        let found: HashMap<&Uuid7, &Found> = found.iter().map(|row| (&row.id, row)).collect();

        let mut answers = HashMap::new();
        let batch = triage(runs, &found, &mut answers);
        let bound = self
            .admit(runner_id, batch, funds_due, now, &mut answers)
            .await;
        let judged = self.run_batch(runner_id, &bound, now).await?;
        self.answer_verdicts(runner_id, bound, judged, now, &mut answers)
            .await;
        Ok(answers)
    }

    /// Refuses, before the statement runs, a run whose payer is short of
    /// funds or whose fleet's stored ceiling will not parse, and binds the rest
    /// with the prices and ceilings the statement guards on.
    async fn admit<'a>(
        &self,
        runner_id: &Uuid7,
        batch: Vec<(&'a Found, &'a RenewRequest)>,
        funds_due: bool,
        now: Timestamp,
        answers: &mut HashMap<Uuid7, Answer>,
    ) -> Vec<Bound<'a>> {
        let short = self.refusals(&batch, funds_due, now).await;
        let ceilings = self.ceilings(&batch).await;
        let mut bound = Vec::new();
        for (row, request) in batch {
            let refusal = if short.contains(&(&row.tenant_id, &row.workspace_id)) {
                Some(renewal_no_credits())
            } else if let Some(Read::Unreadable(fault)) = ceilings.get(&row.fleet_id) {
                Some(refuse_unreadable_budget(
                    &row.fleet_id,
                    row.id.as_str(),
                    fault,
                ))
            } else {
                None
            };
            if let Some(refusal) = refusal {
                self.known_lines(runner_id, row, request, now).await;
                answers.insert(row.id.clone(), Answer::Done(Err(refusal)));
                continue;
            }
            let ceilings = match ceilings.get(&row.fleet_id) {
                Some(Read::Ceilings(ceilings)) => Some(*ceilings),
                Some(Read::Unreadable(_) | Read::Unread) | None => None,
            };
            match self.pricing_of(row).await {
                Ok(pricing) => bound.push(Bound {
                    found: row,
                    request,
                    pricing,
                    ceilings,
                }),
                Err(fault) => {
                    answers.insert(row.id.clone(), Answer::Done(Err(fault)));
                }
            }
        }
        bound
    }

    /// The batch statement over every bound run, a verdict for each in order.
    async fn run_batch(
        &self,
        runner_id: &Uuid7,
        bound: &[Bound<'_>],
        now: Timestamp,
    ) -> Result<Vec<Verdict>> {
        if bound.is_empty() {
            return Ok(Vec::new());
        }
        let columns = Columns::of(bound, self.runs.entropy(), now)?;
        let (digest, connection) = self.runs.host_proof(runner_id);
        let rows: Vec<Judged> = columns
            .bind(runner_id, self.runs.clock())
            .bind(digest)
            .bind(connection)
            .bind(self.runs.connected())
            .fetch_all(&mut *self.runs.pool().acquire().await?)
            .await
            .map_err(query(CONTEXT_RENEW))?;
        let mut by_run: HashMap<Uuid7, Judged> = rows
            .into_iter()
            .map(|row| (row.run_id.clone(), row))
            .collect();
        Ok(bound
            .iter()
            .map(|entry| {
                by_run
                    .remove(&entry.found.id)
                    .map_or(Verdict::Lost, |row| row.verdict())
            })
            .collect())
    }

    /// Turns each run's verdict into its answer: an extension, a refusal said
    /// aloud, or the per-run path's to give.
    async fn answer_verdicts(
        &self,
        runner_id: &Uuid7,
        bound: Vec<Bound<'_>>,
        judged: Vec<Verdict>,
        now: Timestamp,
        answers: &mut HashMap<Uuid7, Answer>,
    ) {
        let mut moved = Vec::new();
        for (entry, verdict) in bound.into_iter().zip(judged) {
            let run_id = entry.found.id.clone();
            let answer = match verdict {
                Verdict::Extended(until) => Ok(until),
                Verdict::Moved => {
                    moved.push(entry);
                    continue;
                }
                Verdict::Tasked => {
                    answers.insert(run_id, Answer::PerRun);
                    continue;
                }
                refused => {
                    self.known_lines(runner_id, entry.found, entry.request, now)
                        .await;
                    let refusal = refusal_of(&refused);
                    log_refused(entry.found, &refused, &refusal);
                    Err(refusal)
                }
            };
            answers.insert(run_id, Answer::Done(answer));
        }
        self.reprice(runner_id, &moved, answers).await;
    }

    /// Sends the runs whose prices or ceilings moved to the per-run path,
    /// and has the replica read their fleets' versions again.
    async fn reprice(
        &self,
        runner_id: &Uuid7,
        moved: &[Bound<'_>],
        answers: &mut HashMap<Uuid7, Answer>,
    ) {
        if moved.is_empty() {
            return;
        }
        let runner = runner_id.as_str();
        let runs = moved.len();
        {
            let (runner_id, event, message) = (
                &runner,
                &EVENT_REPRICED,
                format_args!(
                    "prices or ceilings moved under the renewal statement; the runs take the per-run path"
                ),
            );
            tracing::info!(message, runner_id, runs, event);
        };
        for entry in moved {
            self.runs.latest().forget(&entry.found.fleet_id).await;
            answers.insert(entry.found.id.clone(), Answer::PerRun);
        }
    }
}

/// Splits the frame: a run the pre-read did not find is not this host's, a
/// run with a task keeps the per-run path, and the rest go to the
/// statement.
fn triage<'a>(
    runs: &'a [(Uuid7, RenewRequest)],
    found: &HashMap<&Uuid7, &'a Found>,
    answers: &mut HashMap<Uuid7, Answer>,
) -> Vec<(&'a Found, &'a RenewRequest)> {
    let mut batch = Vec::new();
    for (run_id, request) in runs {
        let Some(row) = found.get(run_id) else {
            answers.insert(run_id.clone(), Answer::Done(Err(run_not_found())));
            continue;
        };
        if row.task_id.is_some() {
            answers.insert(run_id.clone(), Answer::PerRun);
            continue;
        }
        batch.push((*row, request));
    }
    batch
}
