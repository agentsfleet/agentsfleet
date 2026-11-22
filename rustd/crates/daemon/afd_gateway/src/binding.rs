//! Binding belongs to a physical connection, never to a client-supplied identifier alone.

use std::sync::{
    OnceLock,
    atomic::{AtomicU8, Ordering},
};

use af_stream::ConnectionInfo;
use af_wire::protocol::StreamKind;
use afd_auth::principal::Runner;
use afd_http::auth::{
    AuthenticatedRunnerStream, ConnectionAuthenticationResult, ConnectionCredentials,
    RunnerConnectionAuthentication,
};
use afd_runner::capacity::authority::Authority;
use http::HeaderMap;

use crate::{
    Result, connection::StreamAttachmentSender, error::ErrorKind, stream_headers::StreamHeaders,
};

pub(crate) struct Binding {
    pub(crate) runner: Runner,
    pub(crate) authority: Authority,
    pub(crate) attachments: StreamAttachmentSender,
}

/// A server-created context shared only by requests on the same native connection.
pub struct ConnectionContext {
    pub(crate) native: ConnectionInfo,
    binding: OnceLock<Binding>,
    streams: AtomicU8,
}

impl std::fmt::Debug for ConnectionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionContext")
            .field("transport", &self.native.transport)
            .field("bound", &self.binding.get().is_some())
            .finish_non_exhaustive()
    }
}

impl ConnectionContext {
    /// Constructed by the listener's per-connection service factory.
    #[must_use]
    pub const fn new(native: ConnectionInfo) -> Self {
        Self {
            native,
            binding: OnceLock::new(),
            streams: AtomicU8::new(0),
        }
    }

    /// Prove a secondary request before the HTTP guard accepts its identity.
    /// The caller inserts this proof as a request extension, never a header.
    ///
    /// # Errors
    /// Refuses missing, duplicate or foreign metadata and control stream reuse.
    fn authorize_secondary(
        &self,
        headers: &HeaderMap,
    ) -> Result<(Runner, AuthenticatedRunnerStream)> {
        let opening = StreamHeaders::try_from(headers)?;
        if opening.stream_kind() == StreamKind::Control {
            return Err(ErrorKind::Binding.into());
        }
        let binding = self.binding(&opening)?;
        Ok((
            binding.runner.clone(),
            AuthenticatedRunnerStream::new(
                opening.stream_kind(),
                binding.authority.connection().clone(),
            ),
        ))
    }

    pub(crate) fn binding(&self, opening: &StreamHeaders) -> Result<&Binding> {
        self.binding
            .get()
            .filter(|binding| opening.connection() == Some(binding.authority.connection()))
            .ok_or_else(|| ErrorKind::Binding.into())
    }

    pub(crate) fn bind(&self, binding: Binding) -> Result<()> {
        self.binding
            .set(binding)
            .map_err(|_already_bound| ErrorKind::Binding.into())
    }

    pub(crate) fn reserve(&self, kind: StreamKind) -> Result<()> {
        let bit = match kind {
            StreamKind::Control => 1,
            StreamKind::Work => 2,
            StreamKind::Reports => 4,
            StreamKind::Progress => 8,
        };
        if self.streams.fetch_or(bit, Ordering::AcqRel) & bit != 0 {
            return Err(ErrorKind::DuplicateStream { kind }.into());
        }
        Ok(())
    }
}

impl RunnerConnectionAuthentication for ConnectionContext {
    fn credentials(&self, headers: &HeaderMap) -> ConnectionAuthenticationResult {
        if self.native.stop.is_cancelled() {
            return Err(crate::Error::from(ErrorKind::Closed).response().into());
        }
        if self.binding.get().is_none() {
            return Ok(ConnectionCredentials::Bearer);
        }
        self.authorize_secondary(headers)
            .map(|(runner, kind)| ConnectionCredentials::Bound { runner, kind })
            .map_err(|error| error.response().into())
    }
}

#[cfg(test)]
mod tests {
    //! Only the physical connection that authenticated may attach its other kinds.

    use super::{Binding, ConnectionContext};
    use crate::error::ErrorKind;
    use af_core::{clock, error_code, id::Uuid7};
    use af_stream::{ConnectionInfo, Transport};
    use af_wire::protocol::{self, StreamKind};
    use afd_http::auth::{ConnectionCredentials, RunnerConnectionAuthentication as _};
    use http::{HeaderMap, HeaderValue};
    use http_body_util::BodyExt as _;
    use tokio_util::sync::CancellationToken;

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

    fn context(stop: &CancellationToken) -> ConnectionContext {
        ConnectionContext::new(ConnectionInfo {
            transport: Transport::H2,
            peer: ([127, 0, 0, 1], 0).into(),
            stop: stop.clone(),
            native_stats: None,
        })
    }

    /// The headers a secondary `kind` opens with, naming `connection`.
    fn secondary(kind: &'static str, connection: &str) -> TestResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            protocol::VERSION_HEADER,
            HeaderValue::from_static(protocol::VERSION),
        );
        headers.insert(protocol::STREAM_KIND_HEADER, HeaderValue::from_static(kind));
        headers.insert(
            protocol::CONNECTION_HEADER,
            HeaderValue::from_str(connection)?,
        );
        Ok(headers)
    }

    /// The status and code of a refused opening's problem response.
    async fn problem(
        refused: Option<afd_http::handler::Refusal>,
    ) -> TestResult<(u16, serde_json::Value)> {
        use axum::response::IntoResponse as _;
        let response = refused.ok_or("the opening was accepted")?.into_response();
        let status = response.status().as_u16();
        let body = response.into_body().collect().await?.to_bytes();
        let problem: serde_json::Value = serde_json::from_slice(&body)?;
        Ok((
            status,
            problem.get("error_code").cloned().unwrap_or_default(),
        ))
    }

    #[tokio::test]
    async fn an_unbound_connection_asks_for_a_bearer_until_it_closes() -> TestResult {
        let stop = CancellationToken::new();
        let context = context(&stop);
        assert_eq!(
            format!("{context:?}"),
            "ConnectionContext { transport: H2, bound: false, .. }"
        );
        assert!(matches!(
            context.credentials(&HeaderMap::new()),
            Ok(ConnectionCredentials::Bearer)
        ));
        stop.cancel();
        assert_eq!(
            problem(context.credentials(&HeaderMap::new()).err()).await?,
            (500, error_code::INTERNAL_OPERATION_FAILED.as_str().into())
        );
        Ok(())
    }

    #[test]
    fn each_kind_attaches_once() -> TestResult {
        let context = context(&CancellationToken::new());
        for kind in StreamKind::ALL {
            context.reserve(kind)?;
        }
        let refused = context.reserve(StreamKind::Work).err();
        assert!(
            refused.as_ref().is_some_and(|error| matches!(
                error.kind(),
                ErrorKind::DuplicateStream {
                    kind: StreamKind::Work
                }
            )),
            "{refused:?}"
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "needs live datastores: make test-integration-rustd"]
    async fn a_bound_connection_attaches_only_its_own_secondary_kinds() -> TestResult {
        let lane = afd_db::test_util::TestDatabase::shared();
        let database = lane.open(afd_db::config::DbRole::Api, &[]).await;
        let owner = lane.open(afd_db::config::DbRole::Migrator, &[]).await;
        let runners =
            afd_runner::Runners::new(database.clone(), af_crypto::entropy::Entropy::new());
        let enrolled = runners
            .register(&registration(), "fixture:platform-operator", clock::now())
            .await?;
        let runner = enrolled.runner_id.clone();
        let authority = crate::connection::opening::allowed(
            runners
                .acquire_connection(&runner, enrolled.token.digest().clone(), None, clock::now())
                .await?,
        )?;
        let connection = authority.connection().as_str().to_owned();
        let principal = afd_auth::principal::Runner::new(runner.clone(), false);
        let (attachments, _attached) = tokio::sync::mpsc::channel(3);
        let context = context(&CancellationToken::new());
        context.bind(Binding {
            runner: principal.clone(),
            authority: authority.clone(),
            attachments: attachments.clone(),
        })?;
        assert_eq!(
            format!("{context:?}"),
            "ConnectionContext { transport: H2, bound: true, .. }"
        );
        let rebound = context
            .bind(Binding {
                runner: principal.clone(),
                authority,
                attachments,
            })
            .err();
        assert!(
            rebound
                .as_ref()
                .is_some_and(|error| matches!(error.kind(), ErrorKind::Binding)),
            "{rebound:?}"
        );
        let credentials = context.credentials(&secondary("work", &connection)?);
        assert!(
            matches!(&credentials, Ok(ConnectionCredentials::Bound { runner: bound, kind })
                if *bound == principal
                    && kind.kind() == StreamKind::Work
                    && kind.session().as_str() == connection),
            "a bound secondary kind was refused"
        );
        let forbidden = (403, error_code::AUTH_FORBIDDEN.as_str().into());
        let foreign = Uuid7::parse("019f0000-0000-7000-8000-00000000f00d")?;
        assert_eq!(
            problem(
                context
                    .credentials(&secondary("reports", foreign.as_str())?)
                    .err()
            )
            .await?,
            forbidden,
            "another connection's identifier"
        );
        let mut control = secondary("control", &connection)?;
        control.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer agt_r_fixture"),
        );
        assert_eq!(
            problem(context.credentials(&control).err()).await?,
            forbidden,
            "a second control on a bound connection"
        );
        sqlx::query("DELETE FROM runner.runners WHERE id = $1")
            .bind(&runner)
            .execute(&mut *owner.acquire().await?)
            .await?;
        database.close().await;
        owner.close().await;
        lane.cleanup().await;
        Ok(())
    }

    fn registration() -> af_wire::runner::RegisterRequest<'static> {
        use af_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
        af_wire::runner::RegisterRequest {
            host_id: format!("binding-{}.test", afd_db::test_util::mint_id()).into(),
            labels: Vec::new(),
            assigned_policy: AssignedPolicy {
                durable_tasks_enabled: true,
                decision_model_id: None,
                sandbox_tier: SandboxTier::DevNone,
                network_policy: NetworkPolicy::AllowAll,
                registry_allowlist: Vec::new(),
                worker_count: 1,
                extra_binds: Vec::new(),
            },
        }
    }
}
