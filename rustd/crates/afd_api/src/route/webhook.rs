//! Inbound deliveries authenticated by signature rather than by bearer.
//!
//! Every route here is `Guard::Open` in the bearer sense and none of them is
//! unauthenticated: the credential is a signature over the body, checked
//! before the handler runs. They carry no capability scope because there is no
//! principal to hold one — the sender proves itself, not a person.

use super::{Guard, NONE, RouteClass, RouteMeta, Scopes};

/// Signed inbound deliveries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebhookRoute {
    /// A fleet's own inbound webhook.
    Receive,
    /// The Svix-signed variant of the same delivery.
    ReceiveSvix,
    /// An approval decision arriving from outside.
    Approval,
    /// A GitHub delivery for a fleet.
    GitHub,
    /// A provider's application ingress.
    AppIngress,
    /// `QStash`'s scheduled-delivery ingress.
    QstashSchedules,
}

impl WebhookRoute {
    /// Every webhook route.
    pub const ALL: &'static [Self] = &[
        Self::Receive,
        Self::ReceiveSvix,
        Self::Approval,
        Self::GitHub,
        Self::AppIngress,
        Self::QstashSchedules,
    ];

    /// The guard IS the authentication here, which is why it varies per route
    /// while the scope never does.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (guard, template) = match self {
            Self::Receive => (Guard::WebhookSignature, "/v1/webhooks/{fleet_id}"),
            Self::GitHub => (Guard::WebhookSignature, "/v1/webhooks/{fleet_id}/github"),
            Self::Approval => (Guard::WebhookHmac, "/v1/webhooks/{fleet_id}/approval"),
            Self::ReceiveSvix => (Guard::Svix, "/v1/webhooks/svix/{fleet_id}"),
            Self::AppIngress => (Guard::Open, "/v1/ingress/{provider}"),
            Self::QstashSchedules => (Guard::Open, "/v1/ingress/qstash/schedules"),
        };
        RouteMeta::new(guard, RouteClass::Api, template, Scopes::Always(NONE))
    }
}
