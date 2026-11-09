//! Decode inside the charged frame; domain work runs separately from stream reads and pings.

use std::sync::Arc;

use af_wire::protocol::Up;
use garde::Validate as _;

use crate::{Result, error::ErrorKind};
use super::{Message, SessionServices, io::IncomingFrame};

mod heartbeat;
mod progress;
mod renewal;
mod report;

impl SessionServices {
    pub(super) async fn dispatch(self: Arc<Self>, incoming: IncomingFrame,
        requests: &Arc<super::requests::RequestTracker>) -> Result<()> {
        let message: Up<'_> = incoming.frame.decode()?;
        message.validate()?;
        if message.stream_kind() != incoming.kind { return Err(ErrorKind::Protocol.into()); }
        let _registration = requests.register(message.request_id())?;
        match message {
            Up::Ping { request_id } => self.send(&Message::Pong { request_id }).await?,
            Up::Heartbeat { request_id, request } => self.heartbeat(request_id, request).await?,
            Up::RenewBatch { request_id, leases } => self.renew(request_id, leases).await?,
            Up::Report { request_id, request } => self.report(request_id, &request).await?,
            Up::Folds { request_id, lease_id, request } => self.folds(request_id, &lease_id, &request).await?,
            Up::Activity { lease_id, request } => self.progress(&lease_id, &request).await?,
            Up::Ready { credits } => {
                self.handle.snapshot.send_modify(|state| { state.credits = credits; state.ready = true; });
                self.handle.request_offer();
            }
            Up::Hello { .. } | Up::Confirm { .. } => return Err(ErrorKind::Protocol.into()),
        }
        Ok(())
    }
}
