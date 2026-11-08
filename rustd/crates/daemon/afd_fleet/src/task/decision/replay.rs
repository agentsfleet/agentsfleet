//! Saved physical calls never reissue their credential or rerender questions.

use af_wire::task::{
    DecisionCallState, DecisionReceipt, DecisionRecorded, DecisionReservationResult,
};
use zeroize::Zeroizing;

use super::{
    ReservedDecision,
    row::{self, Call},
    semantic,
};
use crate::{Result, task::Tasks};

pub(super) async fn response(tasks: &Tasks, call: Call) -> Result<ReservedDecision> {
    let state = call.state()?;
    let recorded = if state == DecisionCallState::Known {
        recorded(tasks, &call).await?
    } else {
        None
    };
    let packet = DecisionReservationResult::Existing {
        call_id: call.id.into(),
        state,
        recorded,
    };
    let body =
        Zeroizing::new(serde_json::to_string(&packet).map_err(af_core::json::Unreadable::from)?);
    Ok(ReservedDecision {
        body,
        issued: false,
    })
}

async fn recorded(tasks: &Tasks, call: &Call) -> Result<Option<Box<DecisionRecorded>>> {
    let payload = row::saved_payload(tasks.leases.pool(), &call.id).await?;
    let Some(payload) = payload else {
        return Ok(None);
    };
    let Some(receipt) = payload.result else {
        return Ok(None);
    };
    let DecisionReceipt::Provider { result } = receipt.0 else {
        return Ok(None);
    };
    let disposition = semantic::disposition(tasks, call, &payload.request.0, &result).await;
    Ok(Some(Box::new(DecisionRecorded {
        request: payload.request.0,
        result,
        disposition,
    })))
}
