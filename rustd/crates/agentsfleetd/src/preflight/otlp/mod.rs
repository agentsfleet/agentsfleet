//! Where telemetry goes, read from the OpenTelemetry specification's own names.
//!
//! The deployment says WHERE to send and nothing about who receives: a backend
//! change is an environment change, never a code change.
//!
//! # The credential is never a value this module prints
//!
//! An `OTEL_EXPORTER_OTLP_HEADERS` entry may carry a credential, so the
//! endpoint is logged as its SOURCE — the variable's name — rather than beside
//! headers read from the same place.

use std::time::Duration;

use afd_core::env::EnvSource;
use afd_otlp::{
    DEFAULT_TIMEOUT, Encoding, OTEL_ENDPOINT_KNOB, OTEL_HEADERS_KNOB, OTEL_PROTOCOL_KNOB,
    OTEL_TIMEOUT_KNOB, OtlpConfig, Refused,
};

use crate::error::Fault;

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
fn endpoint<E: EnvSource + ?Sized>(env: &E) -> Option<(Box<str>, &'static str)> {
    super::optional(env, OTEL_ENDPOINT_KNOB).map(|value| (value, OTEL_ENDPOINT_KNOB))
}

/// Every header an export carries; a name written twice keeps its last value.
fn headers<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Vec<(String, String)> {
    let mut headers = Vec::new();
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

/// The wire encoding, defaulting to the specification's `http/protobuf`.
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

#[cfg(test)]
mod tests;
