//! A native connection selects bearer authentication or a bound secondary-kind identity.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::protocol::StreamKind;
use afd_auth::{Principal, principal::Runner};
use afd_auth::{credential::Presented, directory::Digest};
use http::{HeaderMap, request::Parts};

use super::{AuthenticationResult, BearerAuthentication, RequestAuthentication};
use crate::{handler::Refusal, route::RouteMeta, services::Services};

/// The initial middleware proves the bearer; session transactions retain only its digest.
#[derive(Clone)]
pub struct AuthenticatedRunnerCredential(Digest);

impl std::fmt::Debug for AuthenticatedRunnerCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthenticatedRunnerCredential")
            .finish_non_exhaustive()
    }
}

impl AuthenticatedRunnerCredential {
    /// Recheck this digest at durable grant boundaries without retaining the bearer.
    #[must_use]
    pub const fn digest(&self) -> &Digest {
        &self.0
    }
}

/// Metadata already checked against one authenticated physical connection.
#[derive(Debug, Clone)]
pub struct AuthenticatedRunnerStream {
    kind: StreamKind,
    session: Uuid7,
}

impl AuthenticatedRunnerStream {
    /// Constructed only by a native connection after its stream kind and owner checks.
    #[must_use]
    pub const fn new(kind: StreamKind, session: Uuid7) -> Self {
        Self { kind, session }
    }

    /// The validated independently flow-controlled stream.
    #[must_use]
    pub const fn kind(&self) -> StreamKind {
        self.kind
    }

    /// The owner bound to this exact physical connection.
    #[must_use]
    pub const fn session(&self) -> &Uuid7 {
        &self.session
    }
}

/// A native connection proves continuity; the common registry proves a new bearer.
#[derive(Debug)]
pub enum ConnectionCredentials {
    /// No session has authenticated on this connection yet.
    Bearer,
    /// A secondary stream belongs to the connection's already proved runner.
    Session {
        runner: Runner,
        kind: AuthenticatedRunnerStream,
    },
}

/// A refused stream already carries its HTTP problem response.
pub type ConnectionAuthenticationResult = core::result::Result<ConnectionCredentials, Refusal>;

/// Implemented by the native connection owner, independent of HTTP route dispatch.
pub trait RunnerConnectionAuthentication: Send + Sync + std::fmt::Debug + 'static {
    /// Prove a secondary stream or request initial bearer authentication.
    fn credentials(&self, headers: &HeaderMap) -> ConnectionAuthenticationResult;
}

/// Only the listener factory inserts this server-owned request extension.
#[derive(Debug, Clone)]
pub struct NativeRunnerConnection(Arc<dyn RunnerConnectionAuthentication>);

impl NativeRunnerConnection {
    /// Share the one connection owner across its native request streams.
    #[must_use]
    pub fn new<C: RunnerConnectionAuthentication>(connection: Arc<C>) -> Self {
        Self(connection)
    }
}

/// Native stream authentication strategy selected when the stream route is mounted.
#[derive(Debug)]
pub struct RunnerStreamAuthentication<D> {
    bearer: BearerAuthentication<D>,
}

impl<D> RunnerStreamAuthentication<D> {
    /// Retain the common bearer registry for the initial opening.
    #[must_use]
    pub fn new(services: Arc<D>) -> Self {
        Self {
            bearer: BearerAuthentication::new(services),
        }
    }
}

impl<D> Clone for RunnerStreamAuthentication<D> {
    fn clone(&self) -> Self {
        Self {
            bearer: self.bearer.clone(),
        }
    }
}

impl<D: Services> RequestAuthentication for RunnerStreamAuthentication<D> {
    async fn authenticate(&self, parts: &mut Parts, route: RouteMeta) -> AuthenticationResult {
        let Some(native) = parts.extensions.get::<NativeRunnerConnection>().cloned() else {
            return self.initial(parts, route).await;
        };
        match native.0.credentials(&parts.headers) {
            Ok(ConnectionCredentials::Bearer) => self.initial(parts, route).await,
            Ok(ConnectionCredentials::Session { runner, kind }) => {
                parts.extensions.insert(kind);
                Ok(Some(Principal::Runner(runner)))
            }
            Err(refusal) => Err(refusal),
        }
    }
}

impl<D: Services> RunnerStreamAuthentication<D> {
    async fn initial(&self, parts: &mut Parts, route: RouteMeta) -> AuthenticationResult {
        let principal = self.bearer.authenticate(parts, route).await?;
        if matches!(&principal, Some(Principal::Runner(_))) {
            let refuse = || {
                super::guard::reject(
                    route,
                    af_core::error_code::RUN_INVALID_RUNNER_TOKEN,
                    "Present the enrolled runner credential on the control stream.",
                    "runner_stream_credential_missing",
                )
            };
            let header = parts
                .headers
                .get(http::header::AUTHORIZATION)
                .ok_or_else(refuse)?;
            let header = header.to_str().map_err(|source| {
                tracing::debug!(event = "runner_credential_header_unreadable", reason = %source);
                refuse()
            })?;
            let presented = Presented::from_authorization(header).map_err(|_| refuse())?;
            parts
                .extensions
                .insert(AuthenticatedRunnerCredential(Digest::of(&presented)));
        }
        Ok(principal)
    }
}
