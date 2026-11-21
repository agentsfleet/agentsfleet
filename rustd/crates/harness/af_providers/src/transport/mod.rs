//! The guarded physical send every run's turns go through: the runner's one
//! pooled client under `guffbolt_ai`'s default transport, its guard's
//! refusal read as one no retry repeats. `guffbolt_ai::wire` reads the
//! replies and retries the sends.

use af_wire::policy::CUSTOM_PROVIDER_PREFIX;
use guffbolt_ai::wire::transport::Reqwest;
use reqwest::Url;

/// The bytes one model reply may carry: the runner frame, since a reply
/// larger than one frame cannot reach the daemon.
pub(crate) const REPLY_MAX_BYTES: usize = af_wire::protocol::MESSAGE_BYTES_MAX;

/// `client`, a destination its egress guard refused answered as refused.
pub(crate) fn guarded(client: reqwest::Client) -> Reqwest {
    Reqwest::with_client(client).refusing(af_egress::blocked_address)
}

/// What a log line names for `provider`: a registry name as it is, a `custom:`
/// endpoint by its host alone, so no path, query or userinfo reaches the
/// journal.
pub(crate) fn logged(provider: &str) -> Box<str> {
    provider
        .strip_prefix(CUSTOM_PROVIDER_PREFIX)
        .and_then(|base| Url::parse(base).ok())
        .and_then(|base| {
            base.host_str()
                .map(|host| format!("{CUSTOM_PROVIDER_PREFIX}{host}"))
        })
        .map_or_else(|| provider.into(), String::into_boxed_str)
}

#[cfg(test)]
mod tests;
