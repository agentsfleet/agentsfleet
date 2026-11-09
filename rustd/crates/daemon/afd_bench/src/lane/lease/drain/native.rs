//! Typed native answers retain domain refusal codes instead of substituting success.

use crate::error::{ErrorKind, Result};
use af_wire::protocol::{Answer, Down};

pub(in crate::lane::lease) fn decode(frame: &af_stream::OwnedFrame) -> Result<Down<'_>> {
    serde_json::from_slice(frame.as_bytes())
        .map_err(|source| ErrorKind::LeaseUnreadable { source }.into())
}

pub(in crate::lane::lease) fn accepted<T>(answer: Answer<'_, T>) -> Result<T> {
    match answer {
        Answer::Ok { response } => Ok(response),
        Answer::Refused { error_code, .. } => Err(ErrorKind::StreamRefused {
            error_code: error_code.into_owned(),
        }
        .into()),
    }
}
