//! Where telemetry goes, read one knob at a time into a type that names its
//! own refusal.
//!
//! # The standard names are the configuration
//!
//! Both binaries read the OpenTelemetry specification's own variable names,
//! so a deployment says WHERE to send and nothing about who receives. A
//! binary that also accepts a vendor alias (the daemon, through its cutover)
//! resolves the alias itself and hands the value here; this module knows the
//! four standard knobs and nothing else.
//!
//! # The endpoint is never a value this module prints
//!
//! It is read from the same place as a credential beside it. So a refusal
//! names the knob and what it accepts, [`OtlpConfig`]'s `Debug` renders the
//! knob's name and the header NAMES, and the value itself reaches exactly one
//! reader: the exporter.

use std::str::FromStr;
use std::time::Duration;

use afd_core::env::EnvSource;
use opentelemetry_otlp::Protocol;

use crate::error::{Refused, Result};

#[cfg(test)]
mod tests;

/// Where signals are sent, as the specification spells it.
pub const OTEL_ENDPOINT_KNOB: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// Headers every export carries, as `key=value` pairs joined by commas.
///
/// Declared here so a binary that REFUSES it can name it by the same
/// constant the binary that reads it uses.
pub const OTEL_HEADERS_KNOB: &str = "OTEL_EXPORTER_OTLP_HEADERS";

/// Every header knob the exporter reads from the process environment itself.
///
/// The general knob and each signal's own, which it prefers: `opentelemetry-otlp`
/// 0.32's `build_client` (`exporter/http/mod.rs`) reads them whatever the
/// programmatic configuration says and merges the result into every request.
/// A binary that refuses a header has to refuse all four, or a credential
/// rides the knob it did not check.
pub const HEADER_KNOBS: [&str; 4] = [
    OTEL_HEADERS_KNOB,
    opentelemetry_otlp::OTEL_EXPORTER_OTLP_TRACES_HEADERS,
    opentelemetry_otlp::OTEL_EXPORTER_OTLP_METRICS_HEADERS,
    opentelemetry_otlp::OTEL_EXPORTER_OTLP_LOGS_HEADERS,
];

/// Every compression knob the exporter reads from the process environment
/// itself, when the programmatic configuration names none.
///
/// This build compiles no compression in, so the exporter refuses to build on
/// any of them — with an error that names no knob. A binary that grades them
/// first names the one an operator has to clear.
pub const COMPRESSION_KNOBS: [&str; 4] = [
    opentelemetry_otlp::OTEL_EXPORTER_OTLP_COMPRESSION,
    opentelemetry_otlp::OTEL_EXPORTER_OTLP_TRACES_COMPRESSION,
    opentelemetry_otlp::OTEL_EXPORTER_OTLP_METRICS_COMPRESSION,
    opentelemetry_otlp::OTEL_EXPORTER_OTLP_LOGS_COMPRESSION,
];

/// Which encoding goes on the wire.
pub const OTEL_PROTOCOL_KNOB: &str = "OTEL_EXPORTER_OTLP_PROTOCOL";

/// How long one export may take, in whole milliseconds.
pub const OTEL_TIMEOUT_KNOB: &str = "OTEL_EXPORTER_OTLP_TIMEOUT";

/// What an export waits before giving up, when nothing says otherwise.
///
/// The specification's own default. Stated rather than inherited so a reader
/// of this file knows the number without going to the exporter's source.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The protocol this build sends, and the only other one it accepts.
const PROTOCOL_PROTOBUF: &str = "http/protobuf";

/// The JSON encoding, accepted because a collector may prefer it.
const PROTOCOL_JSON: &str = "http/json";

/// Why a protocol this build does not carry is refused.
const WHY_PROTOCOL: &str = "http/protobuf or http/json; this build carries no gRPC transport, and a \
     deployment asking for one would export nothing at all";

/// Why a timeout that will not parse is refused.
const WHY_TIMEOUT: &str = "how long one export may take, in whole milliseconds";

/// Why an endpoint no exporter could build is refused.
///
/// Graded HERE so the refusal names the knob. Left to the exporter it comes
/// back as `invalid URI <the whole value>`, and that value is read from the
/// same place as the credential beside it — a rejection that echoed it would
/// print to stderr and to whatever ships stderr.
const WHY_ENDPOINT: &str = "an http or https URL with a host and no query, such as \
                            https://collector.example:4318";

/// The two schemes an HTTP exporter posts over.
const SCHEME_HTTP: &str = "http";
const SCHEME_HTTPS: &str = "https";

/// One optional knob, absent when it is unset or blank.
///
/// Blank is treated as absent rather than as a value, because an environment
/// that exports a variable to the empty string is an environment that meant
/// to unset it.
#[must_use]
pub fn optional<E: EnvSource + ?Sized>(env: &E, knob: &str) -> Option<Box<str>> {
    let value = env.get(knob)?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.into())
}

/// The encoding a signal is posted in.
///
/// An enum rather than the knob's string: the two spellings the knob accepts
/// and the two encodings the exporter speaks meet in exactly one place, and
/// a value this type holds cannot be a third spelling nobody mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// `http/protobuf`, the default and what the retired daemon posted.
    #[default]
    HttpProtobuf,
    /// `http/json`, accepted because a collector may prefer it.
    HttpJson,
}

impl Encoding {
    /// The knob's spelling, for the line that reports it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HttpProtobuf => PROTOCOL_PROTOBUF,
            Self::HttpJson => PROTOCOL_JSON,
        }
    }

    /// The same encoding, as the exporter's own vocabulary spells it.
    pub(crate) const fn wire(self) -> Protocol {
        match self {
            Self::HttpProtobuf => Protocol::HttpBinary,
            Self::HttpJson => Protocol::HttpJson,
        }
    }
}

impl FromStr for Encoding {
    type Err = Refused;

    /// Every other spelling, `grpc` included, is refused HERE rather than at
    /// the first export: a deployment that asked for gRPC and got a process
    /// exporting nothing would look like a collector fault for as long as
    /// nobody checked.
    fn from_str(requested: &str) -> core::result::Result<Self, Refused> {
        match requested {
            PROTOCOL_PROTOBUF => Ok(Self::HttpProtobuf),
            PROTOCOL_JSON => Ok(Self::HttpJson),
            _unsupported => Err(Refused {
                knob: OTEL_PROTOCOL_KNOB,
                why: WHY_PROTOCOL,
            }),
        }
    }
}

/// How long one export may take, read from the knob's digits.
///
/// Zero and unreadable alike are refused. A zero timeout is not "no limit",
/// it is an export that is over before it starts, which is indistinguishable
/// from a collector refusing everything.
///
/// # Errors
/// [`Refused`] naming the timeout knob.
pub fn timeout_from(raw: &str) -> core::result::Result<Duration, Refused> {
    match raw.parse::<u64>() {
        Ok(millis) if millis > 0 => Ok(Duration::from_millis(millis)),
        _unusable => Err(Refused {
            knob: OTEL_TIMEOUT_KNOB,
            why: WHY_TIMEOUT,
        }),
    }
}

/// What a process exports to, when it exports at all.
///
/// Built through [`OtlpConfig::new`], which is the one place the endpoint is
/// graded, so a value of this type is an endpoint the exporter will accept.
#[derive(Clone, PartialEq, Eq)]
pub struct OtlpConfig {
    /// The base URL every signal is posted under.
    endpoint: Box<str>,
    /// The knob the endpoint came from, for the line that reports it.
    source: &'static str,
    /// Every header an export carries, credential included.
    headers: Vec<(String, String)>,
    /// The encoding on the wire.
    encoding: Encoding,
    /// How long one export may take.
    timeout: Duration,
}

impl core::fmt::Debug for OtlpConfig {
    /// Header NAMES only, and the endpoint by its knob rather than its value.
    ///
    /// Derived, this renders a live credential: a daemon's first header is a
    /// vendor pair as `Basic <base64>`, and base64 is an encoding rather than
    /// a protection. A `{:?}` somebody adds later must not be the thing that
    /// ships a token to a log.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OtlpConfig")
            .field("source", &self.source)
            .field("encoding", &self.encoding)
            .field("timeout", &self.timeout)
            .field(
                "headers",
                &self
                    .headers
                    .iter()
                    .map(|(name, _value)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl OtlpConfig {
    /// A configuration posting to `endpoint`, read from the knob `source`,
    /// with no headers and the default encoding and timeout.
    ///
    /// # Errors
    /// [`Refused`] naming `source`, when `endpoint` is not a base every
    /// signal path can be appended to — see [`postable`].
    pub fn new(endpoint: &str, source: &'static str) -> core::result::Result<Self, Refused> {
        if !postable(endpoint) {
            return Err(Refused {
                knob: source,
                why: WHY_ENDPOINT,
            });
        }
        Ok(Self {
            endpoint: endpoint.into(),
            source,
            headers: Vec::new(),
            encoding: Encoding::default(),
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Resolves the three standard knobs this crate reads, or nothing when
    /// the endpoint is unset.
    ///
    /// The headers knob is NOT read here. Whether a header may be configured
    /// is the binary's policy — the daemon merges one with a vendor
    /// credential, the runner refuses any — so each reads or refuses it
    /// itself and this resolution stays the part the two agree on.
    ///
    /// # Errors
    /// The first knob that will not read, naming itself. Absent is the
    /// ordinary case and not an error: a process that refused to start
    /// without a collector would make a telemetry backend a prerequisite for
    /// running the product.
    pub fn from_env<E: EnvSource + ?Sized>(env: &E) -> Result<Option<Self>> {
        let Some(endpoint) = optional(env, OTEL_ENDPOINT_KNOB) else {
            return Ok(None);
        };
        let mut config = Self::new(&endpoint, OTEL_ENDPOINT_KNOB)?;
        if let Some(requested) = optional(env, OTEL_PROTOCOL_KNOB) {
            config.encoding = requested.parse()?;
        }
        if let Some(raw) = optional(env, OTEL_TIMEOUT_KNOB) {
            config.timeout = timeout_from(&raw)?;
        }
        Ok(Some(config))
    }

    /// The same configuration carrying `headers` on every export.
    #[must_use]
    pub fn with_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers = headers;
        self
    }

    /// The same configuration posting in `encoding`.
    #[must_use]
    pub const fn with_encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// The same configuration giving each export `timeout`.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The knob the endpoint came from. The NAME, never the value.
    #[must_use]
    pub const fn source(&self) -> &'static str {
        self.source
    }

    /// The encoding on the wire.
    #[must_use]
    pub const fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// How long one export may take.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Every header an export carries.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Whether the endpoint names a user, as `scheme://user:secret@host` does.
    ///
    /// A binary that holds no credential refuses such an endpoint; the one
    /// that may hold one passes it to the exporter unchanged. Answered from
    /// the parsed authority rather than by searching the string for `@`,
    /// which a path or a query may legitimately carry.
    #[must_use]
    pub fn endpoint_names_a_user(&self) -> bool {
        self.endpoint
            .parse::<http::Uri>()
            .ok()
            .and_then(|uri| {
                uri.authority()
                    .map(|authority| authority.as_str().contains('@'))
            })
            .unwrap_or(false)
    }

    /// Where one signal is posted: the endpoint with `path` appended.
    ///
    /// Appended here because a programmatic endpoint is used verbatim — see
    /// [`crate::pipelines`] for the exporter's docs-versus-code disagreement.
    #[must_use]
    pub fn signal_endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.endpoint.trim_end_matches('/'))
    }
}

/// Whether `endpoint` is a base every signal path can be appended to.
///
/// Parsing as a URI is not enough. `http::Uri` also takes the authority form
/// (`collector:4318`) and the origin form (`/v1/traces`): the first builds no
/// exporter once a path is appended, and the exporter's refusal echoes the
/// whole value; the second builds one that fails every export. A query or a
/// fragment would swallow the appended path, so every signal posts to `/`.
/// What is left is an `http` or `https` URL with a host, which is exactly
/// what the exporter can post to.
fn postable(endpoint: &str) -> bool {
    let Ok(uri) = endpoint.parse::<http::Uri>() else {
        return false;
    };
    matches!(uri.scheme_str(), Some(SCHEME_HTTP | SCHEME_HTTPS))
        && uri.authority().is_some()
        && uri.query().is_none()
        && !endpoint.contains('#')
}
