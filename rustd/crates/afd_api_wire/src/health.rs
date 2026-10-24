//! `GET /healthz` and `GET /readyz`, and what each one answers.
//!
//! # Why the probes have wire types at all
//!
//! Nothing about either answer is the handler's to improvise. An orchestrator's
//! liveness check reads the status word and nothing else; its readiness check
//! reads three booleans by name; the dashboard's status page reads all of it.
//! Every one of those readers parses the shapes declared here, which makes them
//! wire commitments in exactly the sense the rest of this crate is: a field
//! renamed inside a handler-local `json!` breaks a reader no build can see, and
//! a field renamed here fails the build.
//!
//! # Borrowed, like the rest of this crate
//!
//! [`Liveness`]'s build fields are `'static` in practice: the service name and
//! the version are compile-time constants. `Cow<'a, str>` lets them be written
//! without a copy while keeping the type shaped like its siblings.

use std::borrow::Cow;

use serde::Serialize;

/// Server availability and build information. Does not check the database or event queue; use
/// the readiness endpoint for those.
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
    /// Source digest shared with the app, runner and terminal client, when bound.
    pub build_id: Option<Cow<'a, str>>,
}

/// Whether this server can accept traffic. Both 200 and 503 responses include separate
/// database and event-queue checks.
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

    /// The liveness document is the build fields in the order [`Liveness`]
    /// declares them.
    ///
    /// Asserted as bytes: a probe is read by tools that log the body verbatim,
    /// and a reordered key is a diff in every one of their logs.
    #[test]
    fn test_liveness_is_the_build_fields_in_declaration_order() {
        let alive = Liveness {
            status: Cow::Borrowed("ok"),
            service: Cow::Borrowed("agentsfleetd"),
            version: Cow::Borrowed("2.0.0"),
            commit: Cow::Borrowed("unknown"),
            build_id: None,
        };

        assert_eq!(
            serde_json::to_string(&alive).ok().as_deref(),
            Some(
                r#"{"status":"ok","service":"agentsfleetd","version":"2.0.0","commit":"unknown","build_id":null}"#
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
