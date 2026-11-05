//! A setup question can produce an answer without changing the candidate.

mod prepare;
mod record;
mod reserve;
mod verdict;

use afd_core::id::Uuid7;
use afd_wire::{
    authoring::{Proposal, ProposalCall},
    decision::{DecisionAssignment, DecisionRequest},
};
use serde::{Deserialize, Serialize};

use super::{ModelCallInput, ModelCalls, provider::Decoded};
use crate::{Result, lease::guard};

const CONTEXT: &str = "check setup edit intent";
const QUESTION: &str = "Does this message ask to change the fleet's setup (instructions, configuration, name, schedule or access)?";
const QUESTION_NAME: &str = "setup_edit";

#[derive(Debug, Clone)]
pub(in crate::model) struct Configuration {
    pub admin: Uuid7,
    pub client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    assignment: DecisionAssignment,
    provider: String,
    book: afd_billing::price::PriceBook,
    at: jiff::Timestamp,
}

struct Prepared {
    snapshot: Snapshot,
    request: garde::Valid<DecisionRequest>,
    attempt: guard::Attempt,
    candidate_revision: i64,
    reserved_nanos: i64,
}

impl ModelCalls {
    pub(in crate::model) async fn check_edit_intent(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
        decoded: &mut Decoded,
    ) -> Result<()> {
        let has_edit = decoded.result.as_ref().is_some_and(|result| {
            result.proposal_calls.iter().any(|call| {
                matches!(
                    call,
                    ProposalCall::Proposed {
                        proposal: Proposal::Edit { .. },
                        ..
                    }
                )
            })
        });
        if !has_edit {
            return Ok(());
        }
        let Some(prepared) = self.prepare_edit(runner, lease, request, counters).await? else {
            return Ok(());
        };
        let verdict = match self
            .reserve_edit(runner, lease, request, counters, &prepared)
            .await
        {
            Ok(true) => self.evaluate_edit(&prepared).await,
            Ok(false) => return Ok(()),
            Err(error) => return Err(error),
        };
        let accepted = self
            .record_edit(runner, lease, counters, &prepared, verdict)
            .await?;
        if accepted == verdict::Disposition::No {
            verdict::reject(decoded);
            if let Some(result) = &decoded.result {
                super::continuation::refresh_result(&mut decoded.continuation, request, result)?;
            }
        }
        Ok(())
    }

    async fn evaluate_edit(&self, prepared: &Prepared) -> verdict::Evaluation {
        use afr_providers::decision::DecisionEvaluator as _;
        let outcome = async {
            let config = self
                .decisions
                .as_ref()
                .ok_or(crate::error::ErrorKind::Model)?;
            let profile =
                garde::Unvalidated::new(prepared.snapshot.assignment.profile.clone()).validate()?;
            let key = self
                .providers
                .resolve_decision_key(&config.admin, profile.api)
                .await?;
            let evaluator = afr_providers::decision::DecisionClient::with_client(
                config.client.clone(),
                profile,
                key,
            );
            Ok(evaluator.evaluate(&prepared.request).await?)
        }
        .await;
        verdict::Evaluation::from_result(outcome, &prepared.snapshot.assignment.profile)
    }
}
