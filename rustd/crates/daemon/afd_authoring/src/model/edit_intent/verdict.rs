//! Only a complete, current yes/no assessment affects proposals; usage survives refusals.

use afd_wire::{
    authoring::{Proposal, ProposalCall, ProposalRejection},
    decision::{DecisionAnswer, DecisionOutcome, DecisionProfile, DecisionResult, DecisionUsage},
};

use super::{super::provider::Decoded, QUESTION_NAME};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Disposition {
    Yes,
    No,
    Unavailable,
    Malformed,
    Stale,
}

impl Disposition {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Unavailable => "unavailable",
            Self::Malformed => "malformed",
            Self::Stale => "stale",
        }
    }
}

pub(super) struct Evaluation {
    pub disposition: Disposition,
    pub usage: Option<DecisionUsage>,
    pub sent: bool,
}

impl Evaluation {
    pub(super) fn from_result(
        result: crate::Result<DecisionResult>,
        profile: &DecisionProfile,
    ) -> Self {
        match result {
            Ok(result) => Self {
                disposition: classify(&result, profile),
                usage: result
                    .usage
                    .filter(|usage| usage.uncached_input().is_some()),
                sent: true,
            },
            Err(error) => Self {
                disposition: Disposition::Unavailable,
                usage: None,
                sent: matches!(error.kind(), crate::error::ErrorKind::Decision { source } if !source.decision_not_sent()),
            },
        }
    }
}

fn classify(result: &DecisionResult, profile: &DecisionProfile) -> Disposition {
    if result
        .usage
        .is_none_or(|usage| usage.uncached_input().is_none())
    {
        return Disposition::Malformed;
    }
    if !result
        .reported_model
        .as_ref()
        .is_some_and(|model| profile.accepted_models.contains(model))
    {
        return Disposition::Stale;
    }
    let DecisionOutcome::Answered { answers } = &result.result else {
        return Disposition::Malformed;
    };
    match answers.get(QUESTION_NAME).filter(|_| answers.len() == 1) {
        Some(DecisionAnswer::Noul { noul }) if (0.0..0.5).contains(noul) => Disposition::No,
        Some(DecisionAnswer::Noul { noul }) if *noul > 0.5 && *noul <= 1.0 => Disposition::Yes,
        _ => Disposition::Malformed,
    }
}

pub(super) fn reject(decoded: &mut Decoded) {
    if let Some(result) = &mut decoded.result {
        for call in &mut result.proposal_calls {
            if let ProposalCall::Proposed {
                id,
                proposal: Proposal::Edit { .. },
            } = call
            {
                *call = ProposalCall::Refused {
                    id: id.clone(),
                    reason: ProposalRejection::EditNotRequested,
                };
            }
        }
    }
}
