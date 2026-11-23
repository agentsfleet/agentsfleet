//! The router mounts bearer authentication on ordinary guarded routes and the initial runner opening.
//! Tenant API keys, browser sessions, CLI credentials and runner tokens use the existing registry.
//! Secondary native runner streams use connection authentication instead; scope checking stays common.

use std::sync::Arc;

use afd_auth::{Plane, Principal};
use http::{header::AUTHORIZATION, request::Parts};

use super::{Authenticator as _, guard::reject};
use crate::{route::RouteMeta, services::Shared};

/// The proved principal, or a complete, typed HTTP refusal.
///
/// An unguarded route mounts no strategy, so every strategy proves a principal.
pub type AuthenticationResult = Result<Principal, crate::handler::Refusal>;

/// A route chooses its strategy once at mount time; no request path selects policy.
pub trait RequestAuthentication: Clone + Send + Sync + std::fmt::Debug + 'static {
    /// Prove request metadata, leaving body bytes to the bounded handler.
    fn authenticate(
        &self,
        parts: &mut Parts,
        route: RouteMeta,
    ) -> impl Future<Output = AuthenticationResult> + Send;
}

/// Used by the API router and by `RunnerStreamAuthentication` before a native session is bound.
/// It proves the credential class and identity; the common middleware checks route permissions.
#[derive(Debug)]
pub struct BearerAuthentication<D> {
    services: Arc<D>,
    plane: Plane,
}

impl<D> BearerAuthentication<D> {
    /// Share the same authenticator and directory as the rest of the API,
    /// proving credentials of the plane the route's guard names.
    #[must_use]
    pub const fn new(services: Arc<D>, plane: Plane) -> Self {
        Self { services, plane }
    }
}

impl<D> Clone for BearerAuthentication<D> {
    fn clone(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            plane: self.plane,
        }
    }
}

impl<D: Shared> RequestAuthentication for BearerAuthentication<D> {
    async fn authenticate(&self, parts: &mut Parts, route: RouteMeta) -> AuthenticationResult {
        let plane = self.plane;
        let header = match parts
            .headers
            .get(AUTHORIZATION)
            .map(http::HeaderValue::to_str)
            .transpose()
        {
            Ok(header) => header,
            Err(source) => {
                // The header failure is observed at the HTTP boundary; no token bytes enter the refusal.
                tracing::debug!(event = "credential_header_unreadable", reason = %source);
                let refusal = plane.refusal();
                return Err(reject(
                    route,
                    refusal.code(),
                    refusal.detail(),
                    "credential_refused",
                )
                .into());
            }
        };
        self.services
            .authenticator()
            .authenticate(plane, header)
            .await
            .map_err(|error| {
                reject(route, error.code(), error.detail(), "credential_refused").into()
            })
    }
}
