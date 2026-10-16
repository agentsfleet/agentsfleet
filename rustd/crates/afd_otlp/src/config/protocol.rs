//! Which encoding goes on the wire: the protocol knob's two spellings, and the
//! exporter's two encodings they map to.
//!
//! Split from [`super`] for length; the knob's name lives there with the
//! other three.

use std::str::FromStr;

use opentelemetry_otlp::Protocol;

use super::OTEL_PROTOCOL_KNOB;
use crate::error::Refused;

/// The protocol this build sends, and the only other one it accepts.
const PROTOCOL_PROTOBUF: &str = "http/protobuf";

/// The JSON encoding, accepted because a collector may prefer it.
const PROTOCOL_JSON: &str = "http/json";

/// Why a protocol this build does not carry is refused.
const WHY_PROTOCOL: &str = "http/protobuf or http/json; this build carries no gRPC transport, and a \
     deployment asking for one would export nothing at all";

/// The encoding a signal is posted in.
///
/// An enum rather than the knob's string: the two spellings the knob accepts
/// and the two encodings the exporter speaks meet in exactly one place, and
/// a value this type holds cannot be a third spelling nobody mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// `http/protobuf`, the default the OpenTelemetry specification sets for
    /// the protocol knob.
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
