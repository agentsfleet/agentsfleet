//! Authoring uses the same correlated session as fleet work and never redispatches a model call.

use std::{borrow::Cow, num::NonZeroU64};

use af_core::id::Uuid7;
use af_wire::{
    authoring::{
        ActivityAccepted, ActivityRequest, ModelCall, ModelResult, Renew, Renewed, Report, Reported,
    },
    protocol::{Down, Up},
};

use super::{
    ControlPlane, Verb,
    stream::{Command, Outcome},
};
use crate::{Result, downlink::pending::Expected, error::ErrorKind};

impl ControlPlane {
    pub(crate) async fn authoring_model(
        &self,
        run: &Uuid7,
        request: &ModelCall,
    ) -> Result<ModelResult> {
        self.authoring_model_until(
            run,
            request,
            tokio::time::Instant::now() + af_core::timing::AUTHORING_MODEL_ANSWER_TIMEOUT,
        )
        .await
    }

    pub(crate) async fn authoring_model_until(
        &self,
        run: &Uuid7,
        request: &ModelCall,
        deadline: tokio::time::Instant,
    ) -> Result<ModelResult> {
        let reply = self
            .stream
            .compute_until(Model { run, request }, deadline)
            .await?;
        Ok(garde::Unvalidated::new(reply).validate()?.into_inner())
    }

    pub(crate) async fn authoring_activity(
        &self,
        run: &Uuid7,
        request: &ActivityRequest,
    ) -> Result<ActivityAccepted> {
        self.stream.compute(Activity { run, request }).await
    }

    pub(crate) async fn authoring_renew(&self, run: &Uuid7, request: &Renew) -> Result<Renewed> {
        self.stream
            .compute(Renewal {
                run,
                request: request.clone(),
            })
            .await
    }

    pub(crate) async fn authoring_report(&self, run: &Uuid7, request: &Report) -> Result<Reported> {
        self.stream.compute(Completion { run, request }).await
    }
}

struct Model<'a> {
    run: &'a Uuid7,
    request: &'a ModelCall,
}

impl Command for Model<'_> {
    const VERB: Option<Verb> = Some(Verb::AuthoringModel);
    type Output = ModelResult;
    fn expected(&self) -> Expected {
        Expected::AuthoringModel {
            run: self.run.clone(),
            dispatch: self.request.dispatch_id.clone(),
        }
    }
    fn timeout(&self) -> std::time::Duration {
        af_core::timing::AUTHORING_MODEL_ANSWER_TIMEOUT
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::AuthoringModel {
            request_id,
            run_id: self.run.as_str().into(),
            request: Cow::Borrowed(self.request),
        }
    }
    fn decode(reply: Down<'_>) -> Result<ModelResult> {
        let Down::AuthoringModelResult { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Outcome::try_from(answer)?.accepted()
    }
}

struct Activity<'a> {
    run: &'a Uuid7,
    request: &'a ActivityRequest,
}

impl Command for Activity<'_> {
    const VERB: Option<Verb> = Some(Verb::AuthoringActivity);
    type Output = ActivityAccepted;
    fn expected(&self) -> Expected {
        Expected::AuthoringActivity(self.run.clone())
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::AuthoringActivity {
            request_id,
            run_id: self.run.as_str().into(),
            request: Cow::Borrowed(self.request),
        }
    }
    fn decode(reply: Down<'_>) -> Result<ActivityAccepted> {
        let Down::AuthoringActivityAck { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Outcome::try_from(answer)?.accepted()
    }
}

struct Renewal<'a> {
    run: &'a Uuid7,
    request: Renew,
}

impl Command for Renewal<'_> {
    const VERB: Option<Verb> = Some(Verb::AuthoringRenew);
    type Output = Renewed;
    fn expected(&self) -> Expected {
        Expected::AuthoringRenew(self.run.clone())
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::AuthoringRenew {
            request_id,
            run_id: self.run.as_str().into(),
            request: self.request.clone(),
        }
    }
    fn decode(reply: Down<'_>) -> Result<Renewed> {
        let Down::AuthoringRenewed { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Outcome::try_from(answer)?.accepted()
    }
}

struct Completion<'a> {
    run: &'a Uuid7,
    request: &'a Report,
}

impl Command for Completion<'_> {
    const VERB: Option<Verb> = Some(Verb::AuthoringReport);
    type Output = Reported;
    fn expected(&self) -> Expected {
        Expected::AuthoringReport(self.run.clone())
    }
    fn outgoing(&self, request_id: NonZeroU64) -> Up<'_> {
        Up::AuthoringReport {
            request_id,
            run_id: self.run.as_str().into(),
            request: Cow::Borrowed(self.request),
        }
    }
    fn decode(reply: Down<'_>) -> Result<Reported> {
        let Down::AuthoringReportAck { answer, .. } = reply else {
            return Err(ErrorKind::StreamProtocol.into());
        };
        Outcome::try_from(answer)?.accepted()
    }
}
