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

use crate::error::Fault;

/// Where signals are sent, as the specification spells it.
pub const OTEL_ENDPOINT_KNOB: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// Headers every export carries, as `key=value` pairs joined by commas.
pub const OTEL_HEADERS_KNOB: &str = "OTEL_EXPORTER_OTLP_HEADERS";

/// Which encoding goes on the wire.
pub const OTEL_PROTOCOL_KNOB: &str = "OTEL_EXPORTER_OTLP_PROTOCOL";

/// How long one export may take.
pub const OTEL_TIMEOUT_KNOB: &str = "OTEL_EXPORTER_OTLP_TIMEOUT";

/// The vendor endpoint, accepted through the cutover.
pub const GRAFANA_ENDPOINT_KNOB: &str = "GRAFANA_OTLP_ENDPOINT";

/// The vendor's account identifier, which is the basic-auth user.
pub const GRAFANA_INSTANCE_KNOB: &str = "GRAFANA_OTLP_INSTANCE_ID";

/// The vendor's token, which is the basic-auth password.
pub const GRAFANA_API_KEY_KNOB: &str = "GRAFANA_OTLP_API_KEY";

/// The protocol this build sends, and the only other one it accepts.
const PROTOCOL_PROTOBUF: &str = "http/protobuf";

/// The JSON encoding, accepted because a collector may prefer it.
const PROTOCOL_JSON: &str = "http/json";

/// What an export waits before giving up, when nothing says otherwise.
///
/// The specification's own default. Stated rather than inherited so a reader
/// of this file knows the number without going to the exporter's source.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The header a basic credential is presented in.
const AUTHORIZATION: &str = "Authorization";

/// Why a protocol this build does not carry refuses boot.
const WHY_PROTOCOL: &str = "http/protobuf or http/json; this build carries no gRPC transport, and a \
     deployment asking for one would export nothing at all";

/// Why a timeout that will not parse refuses boot.
const WHY_TIMEOUT: &str = "how long one export may take, in whole milliseconds";

/// Why a malformed header list refuses boot.
const WHY_HEADERS: &str = "comma-joined `key=value` pairs; a pair with no `=` \
                           would be sent as a header with no name";

/// What this deployment exports to, when it exports at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtlpConfig {
    /// The base URL every signal is posted under.
    pub endpoint: Box<str>,
    /// The knob the endpoint came from, for the line that reports it.
    ///
    /// The NAME, never the value: an endpoint is read from the same place as
    /// the credential beside it, and a log line carrying one is a log line a
    /// reader will assume carries neither.
    pub source: &'static str,
    /// Every header an export carries, credential included.
    pub headers: Vec<(String, String)>,
    /// The encoding, as the exporter's own vocabulary spells it.
    pub protocol: Box<str>,
    /// How long one export may take.
    pub timeout: Duration,
}

/// Resolves where telemetry goes, or nothing when this deployment sends none.
///
/// Absent is the ordinary case — every developer's environment, every test —
/// and it is not a fault: a daemon that refused to boot without a collector
/// would make a telemetry backend a prerequisite for running the product.
pub(super) fn otlp<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Option<OtlpConfig> {
    let (endpoint, source) = endpoint(env)?;
    Some(OtlpConfig {
        endpoint,
        source,
        headers: headers(env, faults),
        protocol: protocol(env, faults),
        timeout: timeout(env, faults),
    })
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

/// The wire encoding, defaulting to the one the Zig daemon already posts.
fn protocol<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Box<str> {
    let Some(requested) = super::optional(env, OTEL_PROTOCOL_KNOB) else {
        return PROTOCOL_PROTOBUF.into();
    };
    match &*requested {
        PROTOCOL_PROTOBUF | PROTOCOL_JSON => requested,
        // Every other spelling, `grpc` included. Refused HERE rather than at
        // the first export, which is the whole point of reading knobs before
        // anything opens: a deployment that asked for gRPC and got a daemon
        // exporting nothing would look like a collector fault for as long as
        // nobody checked.
        _unsupported => {
            faults.push(Fault::Invalid {
                knob: OTEL_PROTOCOL_KNOB,
                why: WHY_PROTOCOL.to_owned(),
            });
            PROTOCOL_PROTOBUF.into()
        }
    }
}

/// How long one export may take.
fn timeout<E: EnvSource + ?Sized>(env: &E, faults: &mut Vec<Fault>) -> Duration {
    let Some(raw) = super::optional(env, OTEL_TIMEOUT_KNOB) else {
        return DEFAULT_TIMEOUT;
    };
    match raw.parse::<u64>() {
        Ok(millis) if millis > 0 => Duration::from_millis(millis),
        // Zero and unreadable alike. A zero timeout is not "no limit", it is
        // an export that is over before it starts, which is indistinguishable
        // from a collector refusing everything.
        _unusable => {
            faults.push(Fault::Invalid {
                knob: OTEL_TIMEOUT_KNOB,
                why: WHY_TIMEOUT.to_owned(),
            });
            DEFAULT_TIMEOUT
        }
    }
}

/// `input` in standard base64, which is what a basic credential is.
fn base64_standard(input: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(input)
}

#[cfg(test)]
mod tests;
