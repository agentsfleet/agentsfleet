//! Authoring shares the confirmed connection and the existing accounting owners.

use super::super::{ConnectionServices, Message, context::answer};
use crate::{Operation, Result, error::ErrorKind};
use af_core::id::Uuid7;
use af_wire::authoring::{Renew, Report};
use std::num::NonZeroU64;

impl ConnectionServices {
    pub(in super::super) async fn authoring_model(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: afd_setup::ModelCallInput,
    ) -> Result<()> {
        // The runner bounds its answer wait. This tracked work still owns its frame,
        // slot and registration until known provider usage reaches the receipt.
        let result = tokio::select! {
            biased;
            () = self.gateway.inner.accounting_stop.cancelled() => Err(ErrorKind::Closed.into()),
            result = async {
                let run = Uuid7::parse(run)?;
                Ok(self.models.call(&self.handle.runner, &run, request).await?)
            } => result,
        };
        if let Err(error) = &result {
            crate::Gateway::observe(error, Operation::Model);
        }
        self.send(&Message::AuthoringModelResult {
            request_id,
            run_id: run.into(),
            answer: answer(result),
        })
        .await
    }

    pub(in super::super) async fn authoring_activity(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: afd_setup::ActivityInput,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Activity, async {
                let run = Uuid7::parse(run)?;
                Ok(self
                    .drafts
                    .activity(&self.handle.runner, &run, request)
                    .await?)
            })
            .await;
        self.send(&Message::AuthoringActivityAck {
            request_id,
            run_id: run.into(),
            answer: answer(result),
        })
        .await
    }

    pub(in super::super) async fn authoring_renew(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: Renew,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Renewal, async {
                let run = Uuid7::parse(run)?;
                let renewed = self
                    .drafts
                    .renew(&self.handle.runner, &run, request)
                    .await?;
                self.cache.renew(&run, renewed.expires_at);
                Ok(renewed)
            })
            .await;
        self.send(&Message::AuthoringRenewed {
            request_id,
            run_id: run.into(),
            answer: answer(result),
        })
        .await
    }

    pub(in super::super) async fn authoring_report(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: Report,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Report, async {
                let run = Uuid7::parse(run)?;
                let reported = self
                    .creations
                    .report(&self.handle.runner, &run, request)
                    .await?;
                if reported.settled {
                    self.cache.forget(&run);
                }
                Ok(reported)
            })
            .await;
        self.send(&Message::AuthoringReportAck {
            request_id,
            run_id: run.into(),
            answer: answer(result),
        })
        .await
    }
}
