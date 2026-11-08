//! Predictable capability, endpoint and input failures cost no provider call.

use af_wire::decision::{DecisionProfile, DecisionRequest};
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
    if af_egress::blocked_literal(&endpoint)
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
    let supported = request
        .questions
        .iter()
        .all(|named| profile.capabilities.supports(&named.question));
    if !supported {
        return Err(ErrorKind::DecisionPreflight {
            reason: "the decision profile lacks a required question capability",
        }
        .into());
    }
    Ok(endpoint)
}
