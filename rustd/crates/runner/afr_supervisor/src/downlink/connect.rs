//! All four streams attach to one native owner under one absolute recovery deadline.

use std::{num::NonZeroU64, time::Duration};

use af_stream::{Budget, Ceilings, Connection, IncomingReader, Outgoing, Reader};
use af_wire::protocol::{self, Down, StreamKind, Up};
use http::{
    Request,
    header::{AUTHORIZATION, HeaderValue},
};
use tokio::{
    io::{AsyncReadExt as _, DuplexStream},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use super::{
    reconnect::{Preference, Resume},
    running::Running,
};
use crate::{
    Result,
    config::StreamConfig,
    error::{self, ErrorKind},
};

/// The protocol revision this runner speaks: every assignment is a fleet's
/// turn, carrying the fleet's name and no installed instructions.
const SPOKEN: &str = protocol::VERSION;

/// Kept: a zero interval in a welcome would fire the runner's rounds back to back.
pub(crate) const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) struct Pipe {
    pub(crate) writer: DuplexStream,
    pub(crate) reader: Reader<IncomingReader>,
}

pub(crate) struct Connected {
    pub(crate) connection: Connection,
    /// The frame ceilings the daemon's welcome announced.
    pub(crate) ceilings: Ceilings,
    /// The heartbeat interval the daemon's welcome announced: the period of the runner's
    /// one clock for this session.
    pub(crate) interval: Duration,
    pub(crate) control: Pipe,
    pub(crate) work: Pipe,
    pub(crate) reports: Pipe,
    pub(crate) progress: Pipe,
}

#[derive(Clone, Copy)]
pub(crate) struct Opening<'a> {
    pub(crate) config: &'a StreamConfig,
    pub(crate) authorization: &'a HeaderValue,
    pub(crate) running: &'a Running,
    pub(crate) input: &'a Budget,
    pub(crate) output: &'a Budget,
    pub(crate) stop: &'a CancellationToken,
    pub(crate) deadline: Instant,
}

impl Opening<'_> {
    pub(crate) async fn open(
        &self,
        resume: &mut Resume,
        preference: &mut Preference,
    ) -> Result<Connected> {
        let mut connection = preference
            .connect(self.config, self.stop, self.deadline)
            .await?;
        let result = self.attach(&mut connection, resume).await;
        match result {
            Ok(((control, work, reports, progress), (ceilings, interval))) => Ok(Connected {
                connection,
                ceilings,
                interval,
                control,
                work,
                reports,
                progress,
            }),
            Err(error) => {
                let closed = connection.close(self.deadline).await;
                if let Err(close) = closed {
                    let error_code = close.code().as_str();
                    let reason = close.reason().as_str();
                    tracing::warn!(event = "runner_stream_close_failed", error_code, reason);
                }
                Err(error)
            }
        }
    }

    async fn attach(
        &self,
        connection: &mut Connection,
        resume: &mut Resume,
    ) -> Result<((Pipe, Pipe, Pipe, Pipe), (Ceilings, Duration))> {
        let streams = Self {
            deadline: self
                .deadline
                .min(Instant::now() + af_core::timing::STREAM_OPEN_TIMEOUT),
            ..*self
        };
        streams.streams(connection, resume).await
    }

    async fn streams(
        &self,
        connection: &mut Connection,
        resume: &mut Resume,
    ) -> Result<((Pipe, Pipe, Pipe, Pipe), (Ceilings, Duration))> {
        let mut control = self.hello(connection, resume).await?;
        let (owner, ceilings, interval) = self.confirm(&mut control, resume).await?;
        control.reader.limit(ceilings.of(StreamKind::Control));
        let mut secondary = async |kind| {
            let mut pipe = self.secondary(connection, kind, &owner).await?;
            pipe.reader.limit(ceilings.of(kind));
            Ok::<_, crate::Error>(pipe)
        };
        let work = secondary(StreamKind::Work).await?;
        let reports = secondary(StreamKind::Reports).await?;
        let progress = secondary(StreamKind::Progress).await?;
        Ok(((control, work, reports, progress), (ceilings, interval)))
    }

    fn request(
        &self,
        kind: StreamKind,
        owner: Option<&af_core::id::Uuid7>,
        body: Outgoing,
    ) -> Result<Request<Outgoing>> {
        let mut request = Request::post(self.config.endpoint.uri().clone())
            .header(protocol::VERSION_HEADER, SPOKEN)
            .header(protocol::STREAM_KIND_HEADER, kind.as_str());
        if let Some(owner) = owner {
            request = request.header(protocol::CONNECTION_HEADER, owner.as_str());
        }
        if kind == StreamKind::Control {
            request = request.header(AUTHORIZATION, self.authorization);
        }
        request
            .body(body)
            .map_err(|source| error::boundary("runner stream headers could not be built", source))
    }

    async fn hello(&self, connection: &mut Connection, resume: &mut Resume) -> Result<Pipe> {
        let (mut writer, body) = Outgoing::pipe();
        let request = self.request(StreamKind::Control, resume.previous(), body)?;
        let running = self.running.snapshot();
        let hello: Up<'_> = Up::Hello {
            request_id: NonZeroU64::MIN,
            running: running.iter().map(|id| id.as_str().into()).collect(),
            credits: 0,
            unconfirmed_connection_id: resume.unconfirmed().map(|id| id.as_str().into()),
        };
        let frame = af_stream::encode(
            &hello,
            self.output,
            StreamKind::Control,
            self.stop,
            self.deadline,
        )
        .await?;
        // Preparation reads hello before returning headers; both directions must make progress together.
        let (response, written) = tokio::join!(
            connection.open(request, self.deadline),
            af_stream::write(&mut writer, &frame, self.stop, self.deadline)
        );
        written?;
        let mut reader = self.response_reader(response?, StreamKind::Control).await?;
        let frame = tokio::time::timeout_at(self.deadline, reader.next())
            .await??
            .ok_or(ErrorKind::Disconnected)?;
        let Down::Prepared {
            request_id,
            connection_id,
            previous_connection_id,
        } = frame.decode::<Down<'_>>()?
        else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        if request_id != NonZeroU64::MIN {
            return Err(ErrorKind::StreamProtocol.into());
        }
        *resume = Resume::Confirming {
            previous: previous_connection_id
                .as_deref()
                .map(af_core::id::Uuid7::parse)
                .transpose()?,
            proposed: af_core::id::Uuid7::parse(&connection_id)?,
        };
        Ok(Pipe { writer, reader })
    }

    /// Confirms the proposed connection, and reads the frame ceilings its
    /// welcome announced (32 MiB and 2 MiB from a daemon that names none) and its
    /// heartbeat interval, floored as a heartbeat answer's is.
    async fn confirm(
        &self,
        control: &mut Pipe,
        resume: &mut Resume,
    ) -> Result<(af_core::id::Uuid7, Ceilings, Duration)> {
        let proposed = resume
            .unconfirmed()
            .ok_or(ErrorKind::StreamProtocol)?
            .clone();
        let confirm: Up<'_> = Up::Confirm {
            request_id: NonZeroU64::MIN,
            connection_id: proposed.as_str().into(),
        };
        let frame = af_stream::encode(
            &confirm,
            self.output,
            StreamKind::Control,
            self.stop,
            self.deadline,
        )
        .await?;
        af_stream::write(&mut control.writer, &frame, self.stop, self.deadline).await?;
        let frame = tokio::time::timeout_at(self.deadline, control.reader.next())
            .await??
            .ok_or(ErrorKind::Disconnected)?;
        let Down::Welcome {
            request_id,
            connection_id,
            heartbeat_interval_ms,
            max_message_bytes,
            max_control_message_bytes,
        } = frame.decode::<Down<'_>>()?
        else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        if request_id != NonZeroU64::MIN || connection_id != proposed.as_str() {
            return Err(ErrorKind::StreamProtocol.into());
        }
        tracing::Span::current().record("connection_id", proposed.as_str());
        *resume = Resume::Confirmed(proposed.clone());
        Ok((
            proposed,
            Ceilings::announced(max_message_bytes, max_control_message_bytes),
            Duration::from_millis(u64::from(heartbeat_interval_ms)).max(MIN_HEARTBEAT_INTERVAL),
        ))
    }

    async fn secondary(
        &self,
        connection: &mut Connection,
        kind: StreamKind,
        owner: &af_core::id::Uuid7,
    ) -> Result<Pipe> {
        let (writer, body) = Outgoing::pipe();
        let request = self.request(kind, Some(owner), body)?;
        let response = connection.open(request, self.deadline).await?;
        Ok(Pipe {
            writer,
            reader: self.response_reader(response, kind).await?,
        })
    }

    async fn response_reader(
        &self,
        response: http::Response<af_stream::Incoming>,
        kind: StreamKind,
    ) -> Result<Reader<IncomingReader>> {
        let status = response.status();
        if !status.is_success() {
            let mut body = Vec::new();
            let read = tokio::time::timeout_at(
                self.deadline,
                response
                    .into_body()
                    .reader()
                    .take(4096)
                    .read_to_end(&mut body),
            )
            .await;
            // A busy daemon stays retryable even when its body is cut short;
            // the partial body then names no code.
            if status.is_server_error() || status == http::StatusCode::TOO_MANY_REQUESTS {
                return Err(error::unavailable_with_body(
                    crate::client::Verb::Open,
                    status.as_u16(),
                    &body,
                ));
            }
            read??;
            return Err(error::refused_with_body(
                crate::client::Verb::Open,
                status.as_u16(),
                &body,
            ));
        }
        if response
            .headers()
            .get(protocol::VERSION_HEADER)
            .and_then(|value| value.to_str().ok())
            != Some(SPOKEN)
        {
            return Err(ErrorKind::StreamProtocol.into());
        }
        Ok(Reader::new(
            response.into_body().reader(),
            self.input.clone(),
            kind,
            self.stop.clone(),
            af_core::timing::STREAM_ANSWER_TIMEOUT,
        ))
    }
}

#[cfg(all(test, feature = "test-util"))]
#[path = "connect_tests.rs"]
mod tests;

#[cfg(all(test, feature = "test-util"))]
#[path = "opening_tests.rs"]
mod opening_tests;
