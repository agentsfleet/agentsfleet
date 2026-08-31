//! Third-party connector authorization flows.

use afd_auth::Scope;

use super::path::workspace_path;
use super::{Guard, NONE, RouteClass, RouteMeta, Scopes};

const CONNECTOR_READ: &[Scope] = &[Scope::ConnectorRead];
const CONNECTOR_WRITE: &[Scope] = &[Scope::ConnectorWrite];

/// Connecting a workspace to something outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectorRoute {
    /// Start a connect flow for one provider.
    Connect,
    /// One provider's connection status.
    Status,
    /// The provider's redirect back. Unauthenticated by necessity — the
    /// caller is a browser arriving from the provider, carrying the flow's own
    /// state rather than a credential of ours.
    Callback,
    /// Completing that flow from the dashboard, which IS bearer-authenticated.
    Complete,
    /// Everything connectable from this workspace.
    Catalog,
    /// A connector's inbound event delivery.
    ///
    /// Proven by a signature over the body against this deployment's own app
    /// secret for that connector — no bearer, and no workspace in the path to
    /// read one for. Which connectors answer here is
    /// [`afd_connector::Provider::event_ingress`]'s to say; Slack is the only
    /// one today.
    Events,
}

impl ConnectorRoute {
    /// Every connector route.
    pub const ALL: &'static [Self] = &[
        Self::Connect,
        Self::Status,
        Self::Callback,
        Self::Complete,
        Self::Catalog,
        Self::Events,
    ];

    /// `Callback` and `Complete` share a template and differ in guard, which
    /// is why a template is not an identity: the browser's redirect and the
    /// dashboard's follow-up land on one path and are not one route.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (guard, template, scopes) = match self {
            Self::Connect => (
                Guard::Bearer,
                workspace_path!("/connectors/{provider}/connect"),
                Scopes::Always(CONNECTOR_WRITE),
            ),
            Self::Status => (
                Guard::Bearer,
                workspace_path!("/connectors/{provider}"),
                Scopes::rw(CONNECTOR_READ, CONNECTOR_WRITE),
            ),
            Self::Catalog => (
                Guard::Bearer,
                workspace_path!("/connectors"),
                Scopes::Always(CONNECTOR_READ),
            ),
            Self::Complete => (
                Guard::Bearer,
                "/v1/connectors/{provider}/callback",
                Scopes::Always(CONNECTOR_WRITE),
            ),
            Self::Callback => (
                Guard::Open,
                "/v1/connectors/{provider}/callback",
                Scopes::Always(NONE),
            ),
            // `WebhookSignature` rather than `Open`, and the difference is a
            // claim rather than a layer: `plane_of` answers `None` for both,
            // because a signed delivery carries no principal to resolve. What
            // the guard records is that this route IS proven — Invariant 2
            // reads the route metadata to say nothing reaches a stream write
            // unverified, and an `Open` row here would make that read wrong
            // about the one connector route that has a wall in front of it.
            Self::Events => (
                Guard::WebhookSignature,
                "/v1/connectors/{provider}/events",
                Scopes::Always(NONE),
            ),
        };
        RouteMeta::new(guard, RouteClass::Api, template, scopes)
    }
}
