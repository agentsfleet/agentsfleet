//! Binding belongs to a physical connection, never to a client-supplied identifier alone.

use std::sync::{OnceLock, atomic::{AtomicU8, Ordering}};

use af_stream::ConnectionInfo;
use af_wire::protocol::StreamKind;
use afd_auth::principal::Runner;
use afd_http::auth::{AuthenticatedRunnerStream, ConnectionCredentials,
    ConnectionAuthenticationResult, RunnerConnectionAuthentication};
use afd_runner::capacity::authority::Authority;
use http::HeaderMap;

use crate::{Result, error::ErrorKind, stream_headers::StreamHeaders, session::StreamAttachmentSender};

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
        f.debug_struct("ConnectionContext").field("transport", &self.native.transport)
            .field("bound", &self.binding.get().is_some()).finish_non_exhaustive()
    }
}

impl ConnectionContext {
    /// Constructed by the listener's per-connection service factory.
    #[must_use]
    pub const fn new(native: ConnectionInfo) -> Self {
        Self { native, binding: OnceLock::new(), streams: AtomicU8::new(0) }
    }

    /// Prove a secondary request before the HTTP guard accepts its identity.
    /// The caller inserts this proof as a request extension, never a header.
    ///
    /// # Errors
    /// Refuses missing, duplicate or foreign metadata and control stream reuse.
    fn authorize_secondary(&self, headers: &HeaderMap) -> Result<(Runner, AuthenticatedRunnerStream)> {
        let opening = StreamHeaders::try_from(headers)?;
        if opening.stream_kind() == StreamKind::Control { return Err(ErrorKind::Binding.into()); }
        let binding = self.binding(&opening)?;
        Ok((binding.runner.clone(), AuthenticatedRunnerStream::new(
            opening.stream_kind(), binding.authority.connection().clone())))
    }

    pub(crate) fn binding(&self, opening: &StreamHeaders) -> Result<&Binding> {
        self.binding.get()
            .filter(|binding| opening.session() == Some(binding.authority.connection()))
            .ok_or_else(|| ErrorKind::Binding.into())
    }

    pub(crate) fn bind(&self, binding: Binding) -> Result<()> {
        self.binding.set(binding).map_err(|_| ErrorKind::Binding.into())
    }

    pub(crate) fn reserve(&self, kind: StreamKind) -> Result<()> {
        let bit = match kind {
            StreamKind::Control => 1, StreamKind::Work => 2, StreamKind::Reports => 4, StreamKind::Activity => 8,
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
        if self.binding.get().is_none() { return Ok(ConnectionCredentials::Bearer); }
        self.authorize_secondary(headers)
            .map(|(runner, kind)| ConnectionCredentials::Session { runner, kind })
            .map_err(|error| error.response().into())
    }
}
