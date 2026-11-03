//! Predictable capability, endpoint and input failures cost no provider call.

use afd_wire::decision::{DecisionProfile, DecisionQuestion, DecisionRequest};
use garde::Valid;
use url::Url;

use crate::{
    Result,
    error::{Error, ErrorKind},
};

pub(super) fn check(
    profile: &Valid<DecisionProfile>,
    request: &Valid<DecisionRequest>,
) -> Result<Url> {
    let endpoint = Url::parse(&profile.endpoint)
        .map_err(|source| Error::from(ErrorKind::DecisionEndpoint { source }))?;
    if afr_egress::blocked_literal(&endpoint)
        || endpoint.scheme() != "https"
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(ErrorKind::DecisionPreflight {
            reason: "the decision endpoint must be a credential-free HTTPS URL",
        }
        .into());
    }
    let supported = request.questions.iter().all(|named| match &named.question {
        DecisionQuestion::Noul { .. } => profile.capabilities.noul,
        DecisionQuestion::Choice { .. } => profile.capabilities.choice,
        DecisionQuestion::Score { .. } => profile.capabilities.score,
    });
    if !supported {
        return Err(ErrorKind::DecisionPreflight {
            reason: "the decision profile lacks a required question capability",
        }
        .into());
    }
    Ok(endpoint)
}
