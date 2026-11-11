//! A host's renewal frame is one batch (M245 §7): one pre-read and one
//! statement for every live lease the frame names, each answered on its own.
//! A lease id that does not parse, or a request that does not validate, keeps
//! the gateway's refusal and never reaches the batch.

use std::sync::atomic::Ordering;

use super::super::{Message, SessionServices, context::answer};
use crate::{Operation, Result};
use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::{
    protocol::{Answer, Renewal, RenewalAnswer},
    report::{RenewRequest, RenewResponse},
};
use afd_fleet::lease::renew::{Renewals, funds_due};
use garde::Validate as _;
use std::num::NonZeroU64;
use tokio::time::Instant;

/// A frame entry the batch takes: the lease, its request, and the highest
/// request line it acknowledges.
struct Entry {
    index: usize,
    lease_id: Uuid7,
    request: RenewRequest,
    recorded_through: Option<u32>,
}

impl SessionServices {
    /// Answers every lease of one renewal frame from one batch.
    pub(in super::super) async fn renew(
        &self,
        request_id: NonZeroU64,
        leases: Vec<Renewal<'_>>,
    ) -> Result<()> {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let funds_due = funds_due(self.renewal_ticks.fetch_add(1, Ordering::Relaxed));
        let mut ids = Vec::with_capacity(leases.len());
        let mut replies: Vec<Option<Answer<'static, RenewResponse>>> =
            Vec::with_capacity(leases.len());
        let mut entries = Vec::new();
        for (index, Renewal { lease_id, request }) in leases.into_iter().enumerate() {
            match admitted(&lease_id, request) {
                Ok((id, request)) => {
                    entries.push(Entry {
                        index,
                        recorded_through: request.requests.0.iter().map(|line| line.seq).max(),
                        lease_id: id,
                        request,
                    });
                    replies.push(None);
                }
                Err(error) => {
                    crate::Gateway::observe(&error, Operation::Renewal);
                    replies.push(Some(answer(Err(error))));
                }
            }
            ids.push(lease_id);
        }
        if !entries.is_empty() {
            self.answer_batch(entries, funds_due, deadline, &mut replies)
                .await;
        }
        let leases = ids
            .into_iter()
            .zip(replies)
            .filter_map(|(lease_id, answer)| {
                answer.map(|answer| RenewalAnswer { lease_id, answer })
            })
            .collect();
        self.send(&Message::Renewed { request_id, leases }).await
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
            .map(|entry| (entry.index, entry.lease_id.clone(), entry.recorded_through))
            .collect();
        let kept = entries
            .iter()
            .filter_map(|entry| {
                self.cache
                    .reported(&entry.lease_id)
                    .map(|facts| (entry.lease_id.clone(), facts))
            })
            .collect();
        let leases = entries
            .into_iter()
            .map(|entry| (entry.lease_id, entry.request))
            .collect();
        let batch = self.operation_until(Operation::Renewal, deadline, async {
            Ok(self
                .leases
                .renew_batch(Renewals {
                    runner_id: &self.handle.runner,
                    leases,
                    funds_due,
                    now: self.gateway.inner.dependencies.wall_clock.now(),
                    kept,
                })
                .await)
        });
        match batch.await {
            Ok(answers) => {
                for ((index, lease_id, recorded_through), outcome) in
                    positions.into_iter().zip(answers)
                {
                    let outcome: Result<RenewResponse> = outcome
                        .map(|(lease_expires_at, _charged)| RenewResponse {
                            lease_expires_at,
                            recorded_through,
                        })
                        .map_err(Into::into);
                    match &outcome {
                        Ok(response) => self.cache.renew(&lease_id, response.lease_expires_at),
                        Err(error) => {
                            crate::Gateway::observe(error, Operation::Renewal);
                            if !error.retryable() {
                                self.cache.forget(&lease_id);
                            }
                        }
                    }
                    if let Some(reply) = replies.get_mut(index) {
                        *reply = Some(answer(outcome));
                    }
                }
            }
            // The frame missed its deadline or the session is closing: every
            // lease it named answers the same refusal, which the runner retries.
            Err(error) => {
                let refused = answer::<RenewResponse>(Err(error));
                for (index, _lease_id, _recorded_through) in positions {
                    if let Some(reply) = replies.get_mut(index) {
                        *reply = Some(refused.clone());
                    }
                }
            }
        }
    }
}

/// A frame entry the gateway accepts: a lease id that parses and a request
/// that validates.
fn admitted(lease_id: &str, request: RenewRequest) -> Result<(Uuid7, RenewRequest)> {
    let id = Uuid7::parse(lease_id)?;
    request.validate()?;
    Ok((id, request))
}
