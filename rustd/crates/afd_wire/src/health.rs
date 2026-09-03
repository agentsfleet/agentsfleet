//! `GET /healthz` and `GET /readyz`, and what each one answers.
//!
//! # Why the probes have wire types at all
//!
//! Nothing about either answer is the handler's to improvise. An orchestrator's
//! liveness check reads the status word and nothing else; its readiness check
//! reads three booleans by name; the dashboard's status page reads all of it.
//! `health.zig` fixed both shapes and every one of those readers parses them,
//! which makes them contracts in exactly the sense the rest of this crate is:
//! a field renamed inside a handler-local `json!` breaks a reader no build can
//! see, and a field renamed here fails the build.
//!
//! # Borrowed, like the rest of this crate
//!
//! [`Liveness`]'s four fields are `'static` in practice: the service name and
//! the version are compile-time constants. `Cow<'a, str>` lets them be written
//! without a copy while keeping the type shaped like its siblings.

use std::borrow::Cow;

use serde::Serialize;

/// The process is up and answering HTTP.
///
/// Reports the build and nothing about its dependencies. A liveness answer
/// that went red when the database blinked would get the process killed and
/// restarted. That does nothing about the database, and it drops every request
/// the instance was serving.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Liveness<'a> {
    /// Always `ok` from a live process. One word to switch on, so a reader
    /// never has to treat an empty document as a signal.
    pub status: Cow<'a, str>,
    /// The service answering: `agentsfleetd`.
    pub service: Cow<'a, str>,
    /// The version this binary was cut from.
    pub version: Cow<'a, str>,
    /// The commit this binary was built from, or `unknown` when the build was
    /// not told.
    pub commit: Cow<'a, str>,
}

/// Whether this instance should take traffic, and if not, why.
///
/// One shape for the 200 and the 503: only the booleans differ. The fields
/// stay separate all the way to the wire because an operator's next action
/// does. A red database and a red queue are different incidents.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Readiness {
    /// `true` only when every dependency answered. Mirrors the status code:
    /// `true` with a 200, `false` with a 503.
    pub ready: bool,
    /// Whether the database answered.
    pub database: bool,
    /// Whether the event queue answered.
    pub queue: bool,
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{Liveness, Readiness};

    /// The liveness document is the four fields in the order `health.zig`
    /// writes them.
    ///
    /// Asserted as bytes: a probe is read by tools that were pointed at the
    /// Zig daemon, and a reordered key is a diff in every one of their logs.
    #[test]
    fn test_liveness_is_the_four_fields_in_the_zig_order() {
        let alive = Liveness {
            status: Cow::Borrowed("ok"),
            service: Cow::Borrowed("agentsfleetd"),
            version: Cow::Borrowed("2.0.0"),
            commit: Cow::Borrowed("unknown"),
        };

        assert_eq!(
            serde_json::to_string(&alive).ok().as_deref(),
            Some(
                r#"{"status":"ok","service":"agentsfleetd","version":"2.0.0","commit":"unknown"}"#
            ),
        );
    }

    /// The readiness document is the same three keys whether or not the
    /// instance is ready; only the values move.
    #[test]
    fn test_readiness_carries_the_same_keys_either_way() {
        let ready = Readiness {
            ready: true,
            database: true,
            queue: true,
        };
        let degraded = Readiness {
            ready: false,
            database: true,
            queue: false,
        };

        assert_eq!(
            serde_json::to_string(&ready).ok().as_deref(),
            Some(r#"{"ready":true,"database":true,"queue":true}"#),
        );
        assert_eq!(
            serde_json::to_string(&degraded).ok().as_deref(),
            Some(r#"{"ready":false,"database":true,"queue":false}"#),
            "an operator reads WHICH dependency is red, not just that one is"
        );
    }
}
