//! All postcommit effects consume the same committed result and report identity.

use af_core::id::Uuid7;
use af_wire::report::ReportRequest;
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
                let mut result = reconciled(lease, true, RunTokens::default(), None);
                result.hold = self.replayed_hold(runner_id, lease_id).await?;
                Ok(result)
            }
            Committed::Settled {
                accepted_hold,
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
                let mut result = reconciled(lease, false, tokens, completed_at);
                result.hold = accepted_hold.map(|expires_at| af_wire::report::AcceptedHold {
                    lease_id: lease_id.to_string(),
                    fencing_token: request.fencing_token,
                    expires_at,
                });
                Ok(result)
            }
        }
    }
    async fn replayed_hold(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
    ) -> Result<Option<af_wire::report::AcceptedHold>> {
        let mut connection = self.leases.pool().acquire().await?;
        let row: Option<(i64, jiff_sqlx::Timestamp)> =
            sqlx::query_as(include_str!("../sql/lease/report_hold.sql"))
                .bind(lease)
                .bind(runner)
                .bind(crate::lease::sql::LEASE_STATUS_REPORTED)
                .fetch_optional(&mut *connection)
                .await
                .map_err(crate::error::query("read current report hold receipt"))?;
        row.map(|(fence, until)| {
            Ok(af_wire::report::AcceptedHold {
                lease_id: lease.to_string(),
                fencing_token: u64::try_from(fence)
                    .map_err(|_negative| crate::error::stale_fence())?,
                expires_at: until.into(),
            })
        })
        .transpose()
    }
}
