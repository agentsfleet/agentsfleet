//! Shared endpoint proof for persisted coding and decision settings.

use afd_wire::decision::DecisionConfiguration;
use garde::Validate as _;
use url::Url;

use crate::{Result, error::ErrorKind};

/// Parse a credential-free HTTPS provider endpoint without dispatching a request.
/// DNS enforcement remains the guarded transport's responsibility.
///
/// # Errors
/// Rejects malformed URLs and private literals without including their bytes.
pub fn provider_endpoint(value: &str) -> Result<Url> {
    let url = Url::parse(value)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ErrorKind::Endpoint.into());
    }
    let blocked = match url.host() {
        Some(url::Host::Ipv4(address)) => afd_core::net::is_blocked(address.into()),
        Some(url::Host::Ipv6(address)) => afd_core::net::is_blocked(address.into()),
        _ => false,
    };
    if blocked {
        return Err(ErrorKind::Endpoint.into());
    }
    Ok(url)
}

/// Prove a saved decision definition before activating it for a runner.
///
/// # Errors
/// Rejects invalid settings, endpoints and missing mandatory task primitives.
pub fn validate_decision(configuration: &DecisionConfiguration) -> Result<()> {
    configuration.validate()?;
    provider_endpoint(&configuration.profile.endpoint)?;
    provider_endpoint(&configuration.profile.evaluation_source_url)?;
    let capabilities = configuration.profile.capabilities;
    if configuration.enabled && !(capabilities.noul && capabilities.choice) {
        return Err(ErrorKind::DecisionCapabilities.into());
    }
    Ok(())
}
