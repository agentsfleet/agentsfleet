//! Where a failure becomes an [`Error`]: the reply codes read off a refused
//! command, and one constructor per way a command or a dial can end.
//!
//! Split from [`super`] by concern (RULE FLL): that module declares WHAT can go
//! wrong and how a caller asks about it; this one is the seam every raise site
//! in the crate goes through.

use super::{Error, ErrorKind};

/// The reply codes pulled out of a failed command by name (RULE UFS).
///
/// `NOGROUP` is the one recoverable failure: the group vanished (deleted out
/// of band, a restart without persistence, a failover to an empty replica)
/// and recreating it is a defined repair. `OOM` is the one that is not a
/// fault at all but a limit. Dragonfly reports both as ordinary error replies, so
/// nothing else would tell them apart from a genuine command failure.
const CODE_NO_GROUP: &str = "NOGROUP";
const CODE_BUSY_GROUP: &str = "BUSYGROUP";
pub(super) const CODE_OUT_OF_MEMORY: &str = "OOM";

/// Classifies a failed command — see the three codes above.
pub(crate) fn classify(command: &'static str, stream: &str, source: redis::RedisError) -> Error {
    match source.code() {
        Some(CODE_NO_GROUP) => {
            return Error::new(ErrorKind::GroupMissing {
                stream: stream.to_owned(),
            });
        }
        Some(CODE_BUSY_GROUP) => {
            return Error::new(ErrorKind::GroupExists {
                stream: stream.to_owned(),
            });
        }
        Some(CODE_OUT_OF_MEMORY) => return Error::new(ErrorKind::Full { command }),
        _ => {}
    }
    if source.is_connection_dropped() || source.is_io_error() {
        return Error::new(ErrorKind::Unreachable {
            role: "default",
            source: Box::new(source),
        });
    }
    Error::new(ErrorKind::Command {
        command,
        source: Box::new(source),
    })
}

/// The `WRONGTYPE` a working server owes, raised by the caller instead.
///
/// `FleetStreams` and `OutboundQueue` ask `TYPE` before any `MKSTREAM` create
/// and call this when the answer rules the command out — see
/// [`ErrorKind::WrongType`] for why the command cannot simply be sent and let
/// the server refuse it.
pub(crate) fn wrong_type(command: &'static str, stream: &str, holds: &str) -> Error {
    Error::new(ErrorKind::WrongType {
        command,
        stream: stream.to_owned(),
        holds: holds.to_owned(),
    })
}

/// A command that never answered inside its deadline.
pub(crate) fn timed_out(command: &'static str, waited_ms: u128) -> Error {
    Error::new(ErrorKind::Timeout { command, waited_ms })
}

/// A datastore that could not be reached at all.
///
/// Here rather than at the call site so the three ways a dial can end —
/// unreachable, refused for trust, refused for configuration — are raised
/// through one seam instead of two constructors and a hand-rolled
/// `Error::new`.
pub(crate) fn unreachable(role: &'static str, source: redis::RedisError) -> Error {
    Error::new(ErrorKind::Unreachable {
        role,
        source: Box::new(source),
    })
}

/// A seed or certificate the driver refused before opening any socket.
///
/// Deliberately not [`ErrorKind::InvalidUrl`]: the client is built from
/// the seed AND the certificate bytes, and a build that fails has not said
/// which. Naming the URL would be a guess, and a guess in an error message is
/// how an operator ends up reading the wrong file. The driver's own reason
/// stays on the source.
pub(crate) fn config_rejected(role: &'static str, source: redis::RedisError) -> Error {
    Error::new(ErrorKind::ConfigRejected {
        role,
        source: Box::new(source),
    })
}

/// A TLS endpoint whose certificate the configured authority does not trust.
///
/// Separate from [`ErrorKind::Unreachable`] because the two send an operator
/// to opposite places: unreachable is a port, a firewall or a dead process,
/// and this is the authority they configured. Both are still
/// [`Error::is_unavailable`] — the datastore is equally unusable either way,
/// and a caller branching on availability should not have to learn a new kind
/// to keep working.
pub(crate) fn certificate_rejected(role: &'static str, source: redis::RedisError) -> Error {
    Error::new(ErrorKind::CertificateRejected {
        role,
        source: Box::new(source),
    })
}

/// A connection that did not finish inside its whole-operation budget.
pub(crate) fn connect_timed_out(role: &'static str, waited_ms: u128) -> Error {
    Error::new(ErrorKind::ConnectTimeout { role, waited_ms })
}

/// A reply whose shape the client does not recognise.
pub(crate) fn unexpected_reply(what: &'static str) -> Error {
    Error::new(ErrorKind::UnexpectedReply { what })
}
