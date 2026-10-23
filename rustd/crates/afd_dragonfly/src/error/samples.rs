//! One error of every kind, for the suites that walk the whole surface.
//!
//! Split from [`super`] by concern (RULE FLL): a fixture builder compiled only
//! under `test-util`, with nothing a production caller reaches.

use std::time::Duration;

use super::raise::{
    CODE_OUT_OF_MEMORY, certificate_rejected, classify, config_rejected, connect_timed_out,
    timed_out, unexpected_reply, wrong_type,
};
use super::{Error, ErrorKind};

/// The knob every URL sample names.
const KNOB: &str = "DRAGONFLY_URL";
/// The connection role every connect-side sample names.
const ROLE: &str = "default";
/// The detail every server refusal carries.
const REFUSED: &str = "refused";
/// The command the write-side samples ran.
const APPEND: &str = "XADD";
/// The stream every stream-side sample names.
const STREAM: &str = "connector:outbound";

/// One error of every kind, for tests that walk the whole surface.
///
/// Same seam and same argument as `afd_db::error::one_of_each_kind`: these are
/// the renderings a human reads while something is already wrong, and a Dragonfly
/// that refuses a command on demand is not something a test can arrange for
/// every kind.
#[must_use]
pub fn one_of_each_kind() -> Vec<(&'static str, Error)> {
    vec![
        (
            "missing url",
            Error::new(ErrorKind::MissingUrl { knob: KNOB }),
        ),
        (
            "invalid url",
            Error::new(ErrorKind::InvalidUrl { knob: KNOB }),
        ),
        (
            "invalid namespace",
            Error::new(ErrorKind::InvalidNamespace {
                source: garde::Report::new(),
            }),
        ),
        (
            "ca cert unreadable",
            Error::new(ErrorKind::CaCertUnreadable {
                path: "/tls/ca.crt".to_owned(),
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            }),
        ),
        (
            "unreachable",
            classify(
                APPEND,
                STREAM,
                redis::RedisError::from((
                    redis::ErrorKind::ClusterConnectionNotFound,
                    "No connections found",
                )),
            ),
        ),
        (
            "certificate rejected",
            certificate_rejected(ROLE, refusal(REFUSED)),
        ),
        ("config rejected", config_rejected(ROLE, refusal(REFUSED))),
        (
            "connect timeout",
            connect_timed_out(ROLE, Duration::from_secs(5)),
        ),
        ("timeout", timed_out(APPEND, Duration::from_secs(5))),
        ("command", classify(APPEND, STREAM, refusal(REFUSED))),
        ("wrong type", wrong_type("XGROUP", STREAM, "string")),
        (
            "group missing",
            Error::new(ErrorKind::GroupMissing {
                stream: STREAM.to_owned(),
            }),
        ),
        (
            "group exists",
            Error::new(ErrorKind::GroupExists {
                stream: STREAM.to_owned(),
            }),
        ),
        (
            "full",
            classify(APPEND, STREAM, refusal(CODE_OUT_OF_MEMORY)),
        ),
        (
            "unsafe eviction",
            Error::new(ErrorKind::UnsafeEviction {
                node: 0,
                setting: "maxmemory_policy=allkeys-lru".to_owned(),
            }),
        ),
        (
            "not a cluster",
            Error::new(ErrorKind::NotACluster {
                reported: "0".to_owned(),
            }),
        ),
        (
            "missing capability",
            Error::new(ErrorKind::MissingCapability {
                command: "SSUBSCRIBE",
            }),
        ),
        ("unexpected reply", unexpected_reply("PING")),
        ("hub closed", Error::new(ErrorKind::HubClosed)),
    ]
}

/// One server-side refusal, built the way the wire builds one, because the
/// code is the whole difference between a command the server would not run
/// and a server that would not grow.
///
/// Parsed from a RESP error reply rather than assembled from
/// `RedisError::from((ErrorKind::Extension, code, detail))`. That constructor
/// LOOKS like it carries the code and does not: it produces a `General` repr,
/// whose `code()` is always `None`. Every sample routed through `classify`
/// therefore fell past all three code arms and came back a plain command
/// failure — including the one labelled `full`, so the sample set promised one
/// of each kind and held no `Full` at all, and `afd_admission`'s error suite
/// panicked looking for it. Parse-then-extract is the route the driver itself
/// takes — the parser answers an error reply as `Value::ServerError` so a
/// caller can find one nested in an array, and `extract_error` is what turns
/// it into the `Err` a command sees — so the code survives and `classify` is
/// exercised instead of bypassed.
#[expect(
    clippy::expect_used,
    reason = "a sample builder whose own preconditions fail should stop the suite"
)]
fn refusal(code: &str) -> redis::RedisError {
    let reply = format!("-{code} the server said no\r\n");
    redis::parse_redis_value(reply.as_bytes())
        .and_then(redis::Value::extract_error)
        .expect_err("a RESP error reply extracts as an error, not a value")
}
