//! How a session's answers become lines: decoded parameters in, results
//! and refusals out.

use bytes::Bytes;
use jsonrpsee_types::{ErrorObject, Id, Request, Response, ResponsePayload};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use af_guest_wire::error::{self, Result};
use af_guest_wire::protocol::{decoded, line};

/// A call that needs parameters and sent none.
const DETAIL_NO_PARAMS: &str = "the method takes parameters";
/// The guest agent refused a call; the code says why.
pub(super) const EVENT_CALL_REFUSED: &str = "agent_call_refused";

/// A request's parameters, decoded through the object-only gate: whatever is
/// on the other end of the socket is not trusted.
pub(super) fn params<T: DeserializeOwned>(request: &Request<'_>) -> Result<T> {
    let raw = request
        .params
        .as_deref()
        .ok_or_else(|| error::invalid_params(DETAIL_NO_PARAMS))?;
    Ok(decoded(raw)?)
}

/// The line answering `id` with `outcome`.
///
/// A refusal is logged by its code alone: its sentence can quote what the
/// call carried — an environment value, a path — and the sandbox's error
/// stream reaches the host's journal.
pub(super) fn reply<T: Serialize + Clone>(id: Id<'static>, outcome: Result<T>) -> Bytes {
    match outcome {
        Ok(value) => line(&Response::new(ResponsePayload::success(value), id)),
        Err(failure) => {
            let event = EVENT_CALL_REFUSED;
            let code = failure.rpc_code();
            tracing::debug!(event, code, "the guest agent refused a call");
            refusal(id, code, failure.wire_message())
        }
    }
}

/// The line refusing `id` with `code` and `message`.
pub(super) fn refusal(id: Id<'static>, code: i32, message: impl Into<String>) -> Bytes {
    let refusal = ErrorObject::owned(code, message, None::<()>);
    line(&Response::<()>::new(ResponsePayload::error(refusal), id))
}

/// Queues a line for the writer.
///
/// A send fails only once the writer has ended, which means the supervisor
/// is gone and there is no one left to tell.
pub(super) fn post(outbound: &mpsc::UnboundedSender<Bytes>, line: Bytes) {
    let _writer_gone = outbound.send(line);
}
