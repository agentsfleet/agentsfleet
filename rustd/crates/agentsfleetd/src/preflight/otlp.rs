//! Where telemetry goes, and the two spellings that say so.
//!
//! # The standard names are the configuration; the vendor names are a bridge
//!
//! The daemon this replaces is configured with `GRAFANA_OTLP_*` — a vendor's
//! identity spelled into the daemon's own environment. That works while there
//! is one backend and makes moving to a second one a code change.
//!
//! This build reads the OpenTelemetry specification's own names, so the
//! deployment says WHERE to send and nothing about who receives. The vendor
//! spellings are still accepted, because a rollback to the Zig binary during
//! the cutover has to keep exporting from an environment nobody re-wrote. They
//! retire with that binary, and where both are set the standard name wins —
//! otherwise the alias would silently outrank the thing it is an alias for.
//!
//! # The credential is never a value this module prints
//!
//! The vendor pair becomes an `Authorization` header, and the endpoint is
//! logged as its SOURCE — the variable's name — because the header beside it
//! carries a secret and the two are read from the same place.

use std::time::Duration;

use afd_core::env::EnvSource;
use afd_otlp::{
    DEFAULT_TIMEOUT, Encoding, OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB,
    OTEL_TIMEOUT_KNOB, OtlpConfig, Refused,
};

use crate::error::Fault;

/// The vendor endpoint, accepted through the cutover.
pub const GRAFANA_ENDPOINT_KNOB: &str = "GRAFANA_OTLP_ENDPOINT";

/// The vendor's account identifier, which is the basic-auth user.
pub const GRAFANA_INSTANCE_KNOB: &str = "GRAFANA_OTLP_INSTANCE_ID";

/// The vendor's token, which is the basic-auth password.
pub const GRAFANA_API_KEY_KNOB: &str = "GRAFANA_OTLP_API_KEY";

/// The header a basic credential is presented in.
const AUTHORIZATION: &str = "Authorization";

/// Why a malformed header list refuses boot.
const WHY_HEADERS: &str = "comma-joined `key=value` pairs; a pair with no `=` \
                           would be sent as a header with no name";

/// Resolves where telemetry goes, or nothing when this deployment sends none.
///
/// Absent is the ordinary case — every developer's environment, every test —
/// and it is not a fault: a daemon that refused to boot without a collector
/// would make a telemetry backend a prerequisite for running the product.
///
/// Every knob is read even when an earlier one faulted, so one restart can fix
/// all of them; the endpoint itself is graded by `afd_otlp`, the one place an
/// endpoint becomes a configuration.
pub(super) fn otlp<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Option<OtlpConfig> {
    let (endpoint, source) = endpoint(env)?;
    let headers = headers(env, faults);
    let encoding = encoding(env, faults);
    let timeout = timeout(env, faults);
    match OtlpConfig::new(&endpoint, source) {
        Ok(config) => Some(
            config
                .with_headers(headers)
                .with_encoding(encoding)
                .with_timeout(timeout),
        ),
        Err(refused) => {
            faults.push(fault(refused));
            None
        }
    }
}

/// A knob `afd_otlp` refused, as the fault preflight reports beside the rest.
fn fault(refused: Refused) -> Fault {
    Fault::Invalid {
        knob: refused.knob,
        why: refused.why.to_owned(),
    }
}

/// The endpoint and the knob it came from.
///
/// The standard name first, and the alias only when it is unset: an alias that
/// could outrank the name it stands in for is not an alias, it is a second
/// configuration surface with an undefined winner.
fn endpoint<E: EnvSource + ?Sized>(env: &E) -> Option<(Box<str>, &'static str)> {
    for knob in [OTEL_ENDPOINT_KNOB, GRAFANA_ENDPOINT_KNOB] {
        if let Some(value) = super::optional(env, knob) {
            return Some((value, knob));
        }
    }
    None
}

/// Every header an export carries.
///
/// The vendor's credential pair becomes an `Authorization` header, and a
/// standard `OTEL_EXPORTER_OTLP_HEADERS` entry of the same name REPLACES it —
/// the same precedence the endpoint has, for the same reason.
fn headers<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(credential) = vendor_credential(env) {
        headers.push((AUTHORIZATION.to_owned(), credential));
    }
    let Some(raw) = super::optional(env, OTEL_HEADERS_KNOB) else {
        return headers;
    };
    for pair in raw
        .split(',')
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
    {
        let Some((name, value)) = pair.split_once('=') else {
            faults.push(Fault::Invalid {
                knob: OTEL_HEADERS_KNOB,
                why: WHY_HEADERS.to_owned(),
            });
            continue;
        };
        let name = name.trim().to_owned();
        headers.retain(|(existing, _value)| !existing.eq_ignore_ascii_case(&name));
        headers.push((name, value.trim().to_owned()));
    }
    headers
}

/// The vendor pair as a basic credential, when both halves are configured.
///
/// Both or neither: an instance id with no key authenticates nothing, and
/// sending half a credential produces a 401 whose message names nothing an
/// operator can act on.
fn vendor_credential<E: EnvSource + ?Sized>(env: &E) -> Option<String> {
    let instance = super::optional(env, GRAFANA_INSTANCE_KNOB)?;
    let key = super::optional(env, GRAFANA_API_KEY_KNOB)?;
    let encoded = base64_standard(&format!("{instance}:{key}"));
    Some(format!("Basic {encoded}"))
}

/// The wire encoding, defaulting to the one the retired daemon posted.
fn encoding<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Encoding {
    let Some(requested) = super::optional(env, OTEL_PROTOCOL_KNOB) else {
        return Encoding::default();
    };
    requested.parse().unwrap_or_else(|refused| {
        faults.push(fault(refused));
        Encoding::default()
    })
}

/// How long one export may take.
fn timeout<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Duration {
    let Some(raw) = super::optional(env, OTEL_TIMEOUT_KNOB) else {
        return DEFAULT_TIMEOUT;
    };
    afd_otlp::config::timeout_from(&raw).unwrap_or_else(|refused| {
        faults.push(fault(refused));
        DEFAULT_TIMEOUT
    })
}

/// `input` in standard base64, which is what a basic credential is.
fn base64_standard(input: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(input)
}

#[cfg(test)]
mod tests;
