//! Inbound HTTP adapters for deliveries the sender signs.
//!
//! Every handler here is reached without a bearer token and none of them is
//! unauthenticated: the credential is a signature over the body, checked before
//! the handler runs. Keeping them in one crate makes that a property of the
//! compilation unit rather than a per-route claim — nothing in here answers a
//! caller who proved nothing, and there is no bearer-authenticated surface
//! beside them for a route to be moved onto by accident.
//!
//! The plane depends on the substrate and never on another plane, so the
//! provider-segment parse and the approval vault key both resolve through
//! [`afd_http`] rather than through the tenant crate that also uses them.

pub use afd_http::{admission, auth, client, envelope, etag, request_id, route, services};

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

pub use handler::webhook::BUFFER_CEILING;
pub use handler::webhook::verify_platform::{HEADER_APPROVAL_SIGNATURE, HEADER_APPROVAL_TIMESTAMP};

use std::sync::Arc;

use axum::routing::{MethodRouter, post};
use route::{AuthRoute, ConnectorRoute, WebhookRoute};
use services::Services;

/// Selects the handler for a signed inbound delivery.
pub fn webhook_handler_for<D: Services>(verb: WebhookRoute) -> MethodRouter<Arc<D>> {
    match verb {
        WebhookRoute::Receive => post(handler::webhook::receive_route::receive::<D>),
        WebhookRoute::ReceiveSvix => post(handler::webhook::svix_route::receive::<D>),
        WebhookRoute::Approval => post(handler::webhook::approval_route::receive::<D>),
        WebhookRoute::GitHub => post(handler::webhook::github_route::receive::<D>),
        WebhookRoute::AppIngress => post(handler::webhook::app_route::receive::<D>),
        WebhookRoute::QstashSchedules => post(handler::webhook::qstash_route::receive::<D>),
    }
}

/// Selects the handler for the one auth route proven by a signature.
///
/// `None` for every other verb in the family, so the composition root asks the
/// tenant plane first and falls through to here — the same split the connector
/// family already carries, and for the same reason: whoever proves the caller
/// owns the route.
///
/// The identity provider's signup event is not bearer-proven and never can be.
/// It arrives before the account it opens exists, so there is no credential for
/// the sender to present; what it presents is a signature over the body.
#[must_use]
pub fn auth_handler_for<D: Services>(verb: AuthRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        AuthRoute::IdentityEventClerk => Some(post(handler::webhook::identity_route::receive::<D>)),
        AuthRoute::CreateSession
        | AuthRoute::PollSession
        | AuthRoute::ApproveSession
        | AuthRoute::VerifySession
        | AuthRoute::DeleteSession
        | AuthRoute::DeleteAllSessions => None,
    }
}

/// Selects the handler for the one connector route reached without a bearer.
///
/// The rest of the connector family is the tenant plane's: a workspace
/// connecting a provider proves itself, where a provider delivering an event
/// proves only the signature. Returning `None` for those keeps this crate from
/// answering a surface it does not own.
pub fn connector_handler_for<D: Services>(verb: ConnectorRoute) -> Option<MethodRouter<Arc<D>>> {
    match verb {
        ConnectorRoute::Events => Some(post(handler::events::receive::<D>)),
        ConnectorRoute::Connect
        | ConnectorRoute::Status
        | ConnectorRoute::Callback
        | ConnectorRoute::Complete
        | ConnectorRoute::Catalog => None,
    }
}

/// Unused re-export so the crate's lint set sees every declared dependency.
#[cfg(test)]
use {afd_crypto as _, afd_vault as _};
