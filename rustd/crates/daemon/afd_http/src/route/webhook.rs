//! Inbound deliveries authenticated by signature rather than bearer.
//!
//! Every route here is `Guard::Open` in the bearer sense and none of them is
//! unauthenticated: the credential is a signature over the body, checked
//! before the handler runs. They carry no capability scope because there is no
//! principal to hold one — the sender proves itself, not a person.

use super::{Guard, NONE, RouteClass, RouteMeta, Scopes, Verb};

/// Signed inbound deliveries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebhookRoute {
    /// A fleet's inbound webhook from the source its URL names, so two
    /// webhook triggers on one fleet each have an address of their own.
    ReceiveSource,
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
        Self::ReceiveSource,
        Self::Approval,
        Self::GitHub,
        Self::AppIngress,
        Self::QstashSchedules,
    ];

    /// The verbs this route identity serves.
    ///
    /// `POST` throughout, and it could not be otherwise: the credential is a
    /// signature over the body, so a delivery that carries no body carries no
    /// proof and there is nothing for a `GET` to authenticate.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        &[Verb::Post]
    }

    /// The guard IS the authentication here, which is why it varies per route
    /// while the scope never does.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (guard, template) = match self {
            // Beside the literal `/github` and `/approval` templates, which the
            // router matches first: a static segment outranks a parameter.
            Self::ReceiveSource => (Guard::WebhookSignature, "/v1/webhooks/{fleet_id}/{source}"),
            Self::GitHub => (Guard::WebhookSignature, "/v1/webhooks/{fleet_id}/github"),
            Self::Approval => (Guard::WebhookHmac, "/v1/webhooks/{fleet_id}/approval"),
            Self::AppIngress => (Guard::PayloadSigned, "/v1/ingress/{provider}"),
            Self::QstashSchedules => (Guard::PayloadSigned, "/v1/ingress/qstash/schedules"),
        };
        RouteMeta::new(guard, RouteClass::Api, template, Scopes::Always(NONE))
    }
}
