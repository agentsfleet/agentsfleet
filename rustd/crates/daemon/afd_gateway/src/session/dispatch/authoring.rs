//! Authoring shares the confirmed connection and the existing accounting owners.

use super::super::{Message, SessionServices, context::answer};
use crate::{Operation, Result};
use af_core::id::Uuid7;
use af_wire::authoring::{Renew, Report};
use std::num::NonZeroU64;
use tokio::time::Instant;

impl SessionServices {
    pub(in super::super) async fn authoring_model(
        &self,
        request_id: NonZeroU64,
        lease: &str,
        request: afd_authoring::ModelCallInput,
    ) -> Result<()> {
        let result = self
            .operation_until(
                Operation::Model,
                Instant::now() + af_core::timing::AUTHORING_MODEL_ANSWER_TIMEOUT,
                async {
                    let lease = Uuid7::parse(lease)?;
                    Ok(self
                        .models
                        .call(&self.handle.runner, &lease, request)
                        .await?)
                },
            )
            .await;
        self.send(&Message::AuthoringModelResult {
            request_id,
            lease_id: lease.into(),
            answer: answer(result),
        })
        .await
    }

    pub(in super::super) async fn authoring_activity(
        &self,
        request_id: NonZeroU64,
        lease: &str,
        request: afd_authoring::ActivityInput,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Activity, async {
                let lease = Uuid7::parse(lease)?;
                Ok(self
                    .drafts
                    .activity(&self.handle.runner, &lease, request)
                    .await?)
            })
            .await;
        self.send(&Message::AuthoringActivityAck {
            request_id,
            lease_id: lease.into(),
            answer: answer(result),
        })
        .await
    }

    pub(in super::super) async fn authoring_renew(
        &self,
        request_id: NonZeroU64,
        lease: &str,
        request: Renew,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Renewal, async {
                let lease = Uuid7::parse(lease)?;
                let renewed = self
                    .drafts
                    .renew(&self.handle.runner, &lease, request)
                    .await?;
                self.cache.renew(&lease, renewed.expires_at);
                Ok(renewed)
            })
            .await;
        self.send(&Message::AuthoringRenewed {
            request_id,
            lease_id: lease.into(),
            answer: answer(result),
        })
        .await
    }

    pub(in super::super) async fn authoring_report(
        &self,
        request_id: NonZeroU64,
        lease: &str,
        request: Report,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Report, async {
                let lease = Uuid7::parse(lease)?;
                let reported = self
                    .creations
                    .report(&self.handle.runner, &lease, request)
                    .await?;
                self.cache.forget(&lease);
                Ok(reported)
            })
            .await;
        self.send(&Message::AuthoringReportAck {
            request_id,
            lease_id: lease.into(),
            answer: answer(result),
        })
        .await
    }
}
