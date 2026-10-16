//! Private authoring only uses the four scoped machine routes.

use super::{ControlPlane, Verb};
use crate::Result;
use afd_core::id::Uuid7;
use afd_wire::{
    authoring::{
        ActivityAccepted, ActivityRequest, ModelCall, ModelResult, Renew, Renewed, Report, Reported,
    },
    paths,
};
use std::borrow::Cow;

impl ControlPlane {
    pub(crate) async fn authoring_model(
        &self,
        lease: &Uuid7,
        request: &ModelCall,
    ) -> Result<ModelResult> {
        let result: ModelResult = self
            .send_json(
                Verb::AuthoringModel,
                path(lease, paths::AUTHORING_MODEL_CALLS),
                request,
            )
            .await?
            .decode()?;
        Ok(garde::Unvalidated::new(result).validate()?.into_inner())
    }

    pub(crate) async fn authoring_activity(
        &self,
        lease: &Uuid7,
        request: &ActivityRequest,
    ) -> Result<ActivityAccepted> {
        self.send_json(
            Verb::AuthoringActivity,
            path(lease, paths::AUTHORING_ACTIVITY),
            request,
        )
        .await?
        .decode()
    }

    pub(crate) async fn authoring_renew(&self, lease: &Uuid7, request: &Renew) -> Result<Renewed> {
        self.send_json(
            Verb::AuthoringRenew,
            path(lease, paths::AUTHORING_RENEW),
            request,
        )
        .await?
        .decode()
    }

    pub(crate) async fn authoring_report(
        &self,
        lease: &Uuid7,
        request: &Report,
    ) -> Result<Reported> {
        self.send_json(
            Verb::AuthoringReport,
            path(lease, paths::AUTHORING_REPORTS),
            request,
        )
        .await?
        .decode()
    }
}

fn path(lease: &Uuid7, template: &str) -> Cow<'static, str> {
    Cow::Owned(template.replace("{lease_id}", lease.as_str()))
}
