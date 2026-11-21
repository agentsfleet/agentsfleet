//! A host's renewal frame is one batch: one pre-read and one
//! statement for every live run the frame names, each answered on its own.
//! A run id that does not parse, or a request that does not validate, keeps
//! the gateway's refusal and never reaches the batch.

use std::sync::atomic::Ordering;

use super::super::{ConnectionServices, Message, context::answer};
use crate::{Operation, Result};
use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::{
    protocol::{Answer, Renewal, RenewalAnswer},
    report::{RenewRequest, RenewResponse},
};
use afd_fleet::run::renew::{Renewals, funds_due};
use garde::Validate as _;
use std::num::NonZeroU64;
use tokio::time::Instant;

/// A frame entry the batch takes: the run, its request, and the highest
/// request line it acknowledges.
struct Entry {
    index: usize,
    run_id: Uuid7,
    request: RenewRequest,
    recorded_through: Option<u32>,
}

impl ConnectionServices {
    /// Answers every run of one renewal frame from one batch.
    pub(in super::super) async fn renew(
        &self,
        request_id: NonZeroU64,
        runs: Vec<Renewal<'_>>,
    ) -> Result<()> {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let funds_due = funds_due(self.renewal_ticks.fetch_add(1, Ordering::Relaxed));
        let mut ids = Vec::with_capacity(runs.len());
        let mut replies: Vec<Option<Answer<'static, RenewResponse>>> =
            Vec::with_capacity(runs.len());
        let mut entries = Vec::new();
        for (index, Renewal { run_id, request }) in runs.into_iter().enumerate() {
            match admitted(&run_id, request) {
                Ok((id, request)) => {
                    entries.push(Entry {
                        index,
                        recorded_through: request.requests.0.iter().map(|line| line.seq).max(),
                        run_id: id,
                        request,
                    });
                    replies.push(None);
                }
                Err(error) => {
                    crate::Gateway::observe(&error, Operation::Renewal);
                    replies.push(Some(answer(Err(error))));
                }
            }
            ids.push(run_id);
        }
        // A draft's run renews on its own path; the fleet batch reads fleet rows.
        let (drafts, entries): (Vec<Entry>, Vec<Entry>) = entries
            .into_iter()
            .partition(|entry| self.cache.draft(&entry.run_id).is_some());
        if !drafts.is_empty() {
            self.answer_drafts(drafts, deadline, &mut replies).await;
        }
        if !entries.is_empty() {
            self.answer_batch(entries, funds_due, deadline, &mut replies)
                .await;
        }
        let runs = ids
            .into_iter()
            .zip(replies)
            .filter_map(|(run_id, answer)| answer.map(|answer| RenewalAnswer { run_id, answer }))
            .collect();
        self.send(&Message::Renewed { request_id, runs }).await
    }

    /// Runs the batch under the frame's deadline and fills each entry's reply.
    async fn answer_batch(
        &self,
        entries: Vec<Entry>,
        funds_due: bool,
        deadline: Instant,
        replies: &mut [Option<Answer<'static, RenewResponse>>],
    ) {
        let positions: Vec<(usize, Uuid7, Option<u32>)> = entries
            .iter()
            .map(|entry| (entry.index, entry.run_id.clone(), entry.recorded_through))
            .collect();
        let kept = entries
            .iter()
            .filter_map(|entry| {
                self.cache
                    .reported(&entry.run_id)
                    .map(|facts| (entry.run_id.clone(), facts))
            })
            .collect();
        let runs = entries
            .into_iter()
            .map(|entry| (entry.run_id, entry.request))
            .collect();
        let batch = self.operation_until(Operation::Renewal, deadline, async {
            Ok(self
                .runs
                .renew_batch(Renewals {
                    runner_id: &self.handle.runner,
                    runs,
                    funds_due,
                    now: self.gateway.inner.dependencies.wall_clock.now(),
                    kept,
                })
                .await)
        });
        match batch.await {
            Ok(answers) => {
                for ((index, run_id, recorded_through), outcome) in
                    positions.into_iter().zip(answers)
                {
                    let outcome: Result<RenewResponse> = outcome
                        .map(|run_expires_at| RenewResponse {
                            run_expires_at,
                            recorded_through,
                        })
                        .map_err(Into::into);
                    match &outcome {
                        Ok(response) => self.cache.renew(&run_id, response.run_expires_at),
                        Err(error) => {
                            crate::Gateway::observe(error, Operation::Renewal);
                            if !error.retryable() {
                                self.cache.forget(&run_id);
                            }
                        }
                    }
                    if let Some(reply) = replies.get_mut(index) {
                        *reply = Some(answer(outcome));
                    }
                }
            }
            // The frame missed its deadline or the connection is closing: every
            // run it named answers the same refusal, which the runner retries.
            Err(error) => {
                let refused = answer::<RenewResponse>(Err(error));
                for (index, _run_id, _recorded_through) in positions {
                    if let Some(reply) = replies.get_mut(index) {
                        *reply = Some(refused.clone());
                    }
                }
            }
        }
    }
}

impl ConnectionServices {
    /// Renews each draft-owned run of the frame on the draft path, under the
    /// frame's deadline, and fills its reply.
    async fn answer_drafts(
        &self,
        entries: Vec<Entry>,
        deadline: Instant,
        replies: &mut [Option<Answer<'static, RenewResponse>>],
    ) {
        for entry in entries {
            let now = self.gateway.inner.dependencies.wall_clock.now();
            let outcome: Result<RenewResponse> = self
                .operation_until(Operation::Renewal, deadline, async {
                    Ok(self
                        .runs
                        .renew_draft(&self.handle.runner, &entry.run_id, &entry.request, now)
                        .await?)
                })
                .await
                .map(|run_expires_at| RenewResponse {
                    run_expires_at,
                    recorded_through: entry.recorded_through,
                });
            match &outcome {
                Ok(response) => self.cache.renew(&entry.run_id, response.run_expires_at),
                Err(error) => {
                    crate::Gateway::observe(error, Operation::Renewal);
                    if !error.retryable() {
                        self.cache.forget(&entry.run_id);
                    }
                }
            }
            if let Some(reply) = replies.get_mut(entry.index) {
                *reply = Some(answer(outcome));
            }
        }
    }
}

/// A frame entry the gateway accepts: a run id that parses and a request
/// that validates.
fn admitted(run_id: &str, request: RenewRequest) -> Result<(Uuid7, RenewRequest)> {
    let id = Uuid7::parse(run_id)?;
    request.validate()?;
    Ok((id, request))
}
