//! Typed commands share request correlation, decoding and domain refusal handling.

use std::num::NonZeroU64;

use af_core::{error_code::ErrorCode, id::Uuid7};
use af_wire::{
    activity::ActivityRequest,
    fold::FoldRequest,
    protocol::{Answer, Down, Renewal, Up},
    report::{RenewRequest, RenewResponse, ReportRequest},
    runner::HeartbeatRequest,
};
use serde::Serialize;
use zeroize::Zeroizing;

use super::{Body, ControlPlane, Verb};
use crate::{
    Result,
    downlink::pending::Expected,
    error::{self, ErrorKind},
    uplink::StreamHandle,
};

/// A command determines both its correlated reply and the owned value it returns.
pub(crate) trait Command: Send + Sync {
    const VERB: Option<Verb> = None;
    type Output: Send;
    fn expected(&self) -> Expected;
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_>;
    fn decode(reply: Down<'_>) -> Result<Self::Output>;
    fn timeout(&self) -> std::time::Duration {
        af_core::timing::STREAM_ANSWER_TIMEOUT
    }
}

impl StreamHandle {
    pub(crate) async fn compute<C: Command>(&self, command: C) -> Result<C::Output> {
        let deadline = tokio::time::Instant::now() + command.timeout();
        self.compute_until(command, deadline).await
    }

    /// Partitioned host work shares its original absolute answer deadline.
    pub(crate) async fn compute_until<C: Command>(
        &self,
        command: C,
        deadline: tokio::time::Instant,
    ) -> Result<C::Output> {
        let verb = C::VERB.map_or("ping", Verb::as_str);
        tracing::debug!(event = "runner_command_started", verb);
        let deadline = deadline.min(tokio::time::Instant::now() + command.timeout());
        let result = async {
            let frame = self
                .request(command.expected(), |id| command.outgoing(id), deadline)
                .await?;
            C::decode(frame.decode()?)
        }
        .await;
        match &result {
            Ok(_) => tracing::debug!(event = "runner_command_completed", verb),
            Err(error) => {
                let error_code = error.code().as_str();
                tracing::warn!(event = "runner_command_failed", verb, error_code);
            }
        }
        result
    }
}

/// Resolve the registry code once, before a reply leaves the frame's lifetime.
pub(crate) enum Outcome<T> {
    Accepted(T),
    Refused { code: ErrorCode, retryable: bool },
}

impl<T> TryFrom<Answer<'_, T>> for Outcome<T> {
    type Error = crate::Error;
    fn try_from(answer: Answer<'_, T>) -> Result<Self> {
        match answer {
            Answer::Ok { response } => Ok(Self::Accepted(response)),
            Answer::Refused {
                error_code,
                retryable,
                ..
            } => {
                let code = ErrorCode::lookup(&error_code).ok_or(ErrorKind::StreamProtocol)?;
                Ok(Self::Refused { code, retryable })
            }
        }
    }
}

impl<T> Outcome<T> {
    pub(super) fn accepted(self) -> Result<T> {
        match self {
            Self::Accepted(response) => Ok(response),
            Self::Refused { code, retryable } => {
                Err(ErrorKind::StreamRefused { code, retryable }.into())
            }
        }
    }
}

impl ControlPlane {
    pub(crate) async fn heartbeat(&self, request: &HeartbeatRequest<'_>) -> Result<Body> {
        self.stream.compute(Heartbeat(request.clone())).await
    }

    pub(crate) async fn renew(
        &self,
        lease_id: &Uuid7,
        spent: &RenewRequest,
    ) -> Result<RenewResponse> {
        let batch = RenewalBatch {
            ids: vec![lease_id.clone()],
            leases: vec![Renewal {
                lease_id: lease_id.as_str().into(),
                request: spent.clone(),
            }],
        };
        let mut replies = self.stream.compute(batch).await?;
        replies.pop().ok_or(ErrorKind::StreamProtocol)?.1.accepted()
    }

    pub(crate) async fn activity(
        &self,
        lease_id: &Uuid7,
        request: &ActivityRequest<'_>,
    ) -> Result<()> {
        let verb = Verb::Activity.as_str();
        tracing::debug!(event = "runner_activity_queued", verb);
        self.stream
            .send(&Up::Activity {
                lease_id: lease_id.as_str().into(),
                request: request.clone(),
            })
            .await
    }

    pub(crate) async fn report(&self, bytes: &[u8]) -> Result<()> {
        self.stream
            .compute(Report::try_from(bytes)?)
            .await
            .map(drop)
    }

    pub(crate) async fn folds(
        &self,
        lease_id: &Uuid7,
        fencing_token: u64,
        after_sequence: i64,
    ) -> Result<Body> {
        self.stream
            .compute(Folds {
                lease: lease_id,
                request: FoldRequest {
                    fencing_token,
                    after_sequence,
                },
            })
            .await
    }
}

struct Heartbeat<'a>(HeartbeatRequest<'a>);

impl Command for Heartbeat<'_> {
    const VERB: Option<Verb> = Some(Verb::Heartbeat);
    type Output = Body;
    fn expected(&self) -> Expected {
        Expected::Heartbeat
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::Heartbeat {
            request_id,
            request: self.0.clone(),
        }
    }
    fn decode(reply: Down<'_>) -> Result<Body> {
        let Down::Heartbeat { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Body::encoded(Verb::Heartbeat, &Outcome::try_from(answer)?.accepted()?)
    }
}

pub(crate) struct RenewalBatch<'a> {
    pub(crate) ids: Vec<Uuid7>,
    pub(crate) leases: Vec<Renewal<'a>>,
}

impl Command for RenewalBatch<'_> {
    const VERB: Option<Verb> = Some(Verb::Renew);
    type Output = Vec<(Uuid7, Outcome<RenewResponse>)>;
    fn expected(&self) -> Expected {
        Expected::Renewed(self.ids.clone())
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::RenewBatch {
            request_id,
            leases: self.leases.clone(),
        }
    }
    fn decode(reply: Down<'_>) -> Result<Self::Output> {
        let Down::Renewed { leases, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        leases
            .into_iter()
            .map(|lease| {
                Ok((
                    Uuid7::parse(&lease.lease_id)?,
                    Outcome::try_from(lease.answer)?,
                ))
            })
            .collect()
    }
}

pub(crate) struct Report<'a> {
    lease: Uuid7,
    request: Box<ReportRequest<'a>>,
}

impl<'a> TryFrom<&'a [u8]> for Report<'a> {
    type Error = crate::Error;
    fn try_from(bytes: &'a [u8]) -> Result<Self> {
        let request: ReportRequest<'_> =
            af_core::json::object_from_slice(bytes).map_err(error::malformed(Verb::Report))?;
        Ok(Self {
            lease: Uuid7::parse(&request.lease_id)?,
            request: Box::new(request),
        })
    }
}

impl Command for Report<'_> {
    const VERB: Option<Verb> = Some(Verb::Report);
    type Output = af_wire::report::ReportResponse;
    fn expected(&self) -> Expected {
        Expected::Report(self.lease.clone())
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::Report {
            request_id,
            request: Box::new(std::borrow::Cow::Borrowed(&self.request)),
        }
    }
    fn decode(reply: Down<'_>) -> Result<Self::Output> {
        let Down::ReportAck { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        let response = Outcome::try_from(answer)?.accepted()?;
        if !response.ok {
            return Err(ErrorKind::StreamProtocol.into());
        }
        Ok(response)
    }
}

struct Folds<'a> {
    lease: &'a Uuid7,
    request: FoldRequest,
}

impl Command for Folds<'_> {
    const VERB: Option<Verb> = Some(Verb::Folds);
    type Output = Body;
    fn expected(&self) -> Expected {
        Expected::Folds(self.lease.clone())
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::Folds {
            request_id,
            lease_id: self.lease.as_str().into(),
            request: self.request,
        }
    }
    fn decode(reply: Down<'_>) -> Result<Body> {
        let Down::Folded { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Body::encoded(Verb::Folds, &Outcome::try_from(answer)?.accepted()?)
    }
}

pub(crate) struct Ping;

impl Command for Ping {
    type Output = ();
    fn expected(&self) -> Expected {
        Expected::Pong
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::Ping { request_id }
    }
    fn decode(reply: Down<'_>) -> Result<()> {
        match reply {
            Down::Pong { .. } => Ok(()),
            _ => Err(ErrorKind::StreamProtocol.into()),
        }
    }
}

impl Body {
    pub(crate) fn encoded<T: Serialize>(verb: Verb, response: &T) -> Result<Self> {
        Ok(Self {
            verb,
            bytes: Zeroizing::new(serde_json::to_vec(response).map_err(error::encode)?),
        })
    }
}
