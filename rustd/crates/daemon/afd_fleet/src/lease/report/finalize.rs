//! All postcommit effects consume the same committed result and report identity.

use af_core::id::Uuid7;
use af_wire::report::ReportRequest;
use afd_billing::Nanos;
use jiff::Timestamp;

use super::{Reconciled, RunTokens, assembly::reconciled, steps, timing};
use crate::lease::{commit::Committed, pull::LeaseService, settle::Reported};
use crate::task::PreparedReport;
use crate::{Result, error::stale_fence};

pub(super) struct Context<'a> {
    pub(super) runner_id: &'a Uuid7,
    pub(super) request: &'a ReportRequest<'a>,
    pub(super) lease_id: &'a Uuid7,
    pub(super) now: Timestamp,
}

impl LeaseService {
    pub(super) async fn finalize_report(
        &self,
        lease: Reported,
        committed: Committed,
        task_plan: Option<&PreparedReport>,
        context: Context<'_>,
    ) -> Result<Reconciled> {
        let Context {
            runner_id,
            request,
            lease_id,
            now,
        } = context;
        match committed {
            Committed::Fenced { .. } => Err(stale_fence()),
            Committed::AlreadySettled => {
                Self::record_repeat(&lease, lease_id.as_str());
                Ok(reconciled(
                    lease,
                    Nanos::ZERO,
                    true,
                    RunTokens::default(),
                    None,
                ))
            }
            Committed::Settled {
                charged,
                tokens,
                closed,
                owed,
                notice,
                folds,
            } => {
                if let Some(plan) = task_plan {
                    plan.announce().await;
                }
                let answer = notice.unwrap_or_else(|| request.response_text.as_ref());
                let completed_at = self
                    .announce(
                        runner_id,
                        request,
                        &lease,
                        steps::Completion { closed, answer },
                        charged,
                    )
                    .await;
                timing::completed(&lease, now, completed_at);
                if let Some(owing) = owed {
                    self.wake_owed_answer(&owing).await;
                }
                self.announce_folds(&lease, lease_id.as_str(), folds, answer)
                    .await;
                Ok(reconciled(lease, charged, false, tokens, completed_at))
            }
        }
    }
}
