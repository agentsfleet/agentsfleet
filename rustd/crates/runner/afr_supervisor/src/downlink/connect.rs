//! All four streams attach to one native owner under one absolute recovery deadline.

use std::num::NonZeroU64;

use af_stream::{Budget, Connection, IncomingReader, Outgoing, Reader};
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

pub(crate) struct Pipe {
    pub(crate) writer: DuplexStream,
    pub(crate) reader: Reader<IncomingReader>,
}

pub(crate) struct Connected {
    pub(crate) connection: Connection,
    pub(crate) control: Pipe,
    pub(crate) work: Pipe,
    pub(crate) reports: Pipe,
    pub(crate) activity: Pipe,
}

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
        let result = self.streams(&mut connection, resume).await;
        match result {
            Ok((control, work, reports, activity)) => Ok(Connected {
                connection,
                control,
                work,
                reports,
                activity,
            }),
            Err(error) => {
                let closed = connection
                    .close(Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT)
                    .await;
                if let Err(close) = closed {
                    let error_code = close.code().as_str();
                    let reason = close.reason().as_str();
                    tracing::warn!(event = "runner_stream_close_failed", error_code, reason);
                }
                Err(error)
            }
        }
    }

    async fn streams(
        &self,
        connection: &mut Connection,
        resume: &mut Resume,
    ) -> Result<(Pipe, Pipe, Pipe, Pipe)> {
        let mut control = self.hello(connection, resume).await?;
        let owner = self.confirm(&mut control, resume).await?;
        let work = self.secondary(connection, StreamKind::Work, &owner).await?;
        let reports = self
            .secondary(connection, StreamKind::Reports, &owner)
            .await?;
        let activity = self
            .secondary(connection, StreamKind::Activity, &owner)
            .await?;
        Ok((control, work, reports, activity))
    }

    fn request(
        &self,
        kind: StreamKind,
        owner: Option<&af_core::id::Uuid7>,
        body: Outgoing,
    ) -> Result<Request<Outgoing>> {
        let mut request = Request::post(self.config.endpoint.uri().clone())
            .header(protocol::VERSION_HEADER, protocol::VERSION)
            .header(protocol::STREAM_KIND_HEADER, kind.as_str());
        if let Some(owner) = owner {
            request = request.header(protocol::SESSION_HEADER, owner.as_str());
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
            accepted_kinds: af_wire::lease::AcceptedKinds::ALL,
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

    async fn confirm(&self, control: &mut Pipe, resume: &mut Resume) -> Result<af_core::id::Uuid7> {
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
            ..
        } = frame.decode::<Down<'_>>()?
        else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        if request_id != NonZeroU64::MIN || connection_id != proposed.as_str() {
            return Err(ErrorKind::StreamProtocol.into());
        }
        *resume = Resume::Confirmed(proposed.clone());
        Ok(proposed)
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
            if status.is_server_error() || status == http::StatusCode::TOO_MANY_REQUESTS {
                return Err(ErrorKind::Disconnected.into());
            }
            let mut body = Vec::new();
            tokio::time::timeout_at(
                self.deadline,
                response
                    .into_body()
                    .reader()
                    .take(4096)
                    .read_to_end(&mut body),
            )
            .await??;
            return Err(error::refused_with_body(
                crate::client::Verb::Heartbeat,
                status.as_u16(),
                &body,
            ));
        }
        if response
            .headers()
            .get(protocol::VERSION_HEADER)
            .and_then(|value| value.to_str().ok())
            != Some(protocol::VERSION)
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
