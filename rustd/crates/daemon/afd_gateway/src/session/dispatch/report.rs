//! The terminal acknowledgement follows saved settlement and preserves replay suppression.

use std::num::NonZeroU64;

use af_core::id::Uuid7;
use af_wire::report::{ReportRequest, ReportResponse};
use garde::Validate as _;

use crate::{Operation, Result};
use super::super::{Message, SessionServices, context::answer};

impl SessionServices {
    pub(in super::super) async fn report(&self, request_id: NonZeroU64, request: &ReportRequest<'_>) -> Result<()> {
        let result = self.operation(Operation::Report, async {
            request.requests.validate()?;
            let id = Uuid7::parse(&request.lease_id)?;
            let received = self.gateway.0.dependencies.wall_clock.now();
            let settled = self.leases.report(&self.handle.runner, request, received).await?;
            if let Err(error) = afd_fleet::lease::report::observe(&self.gateway.0.dependencies.analytics,
                &settled, request, self.handle.runner.as_str(), received) {
                self.gateway.observe(&error.into(), Operation::Report);
            }
            self.cache.forget(&id);
            Ok(ReportResponse { ok: true })
        }).await;
        self.send(&Message::ReportAck { request_id, lease_id: request.lease_id.as_ref().into(), answer: answer(result) }).await
    }
}
