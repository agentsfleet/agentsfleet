//! Each request names the one response variant that answers it, so a caller
//! receives that variant's value rather than matching the whole response.

use super::{
    FoldRef, TurnCheckpointAbandon, TurnCheckpointAction, TurnCheckpointAdopt,
    TurnCheckpointAuthorize, TurnCheckpointBegin, TurnCheckpointCommit, TurnCheckpointObjects,
    TurnCheckpointPageRead, TurnCheckpointPageWrite, TurnCheckpointPinRelease,
    TurnCheckpointPinRenew, TurnCheckpointPlan, TurnCheckpointRead, TurnCheckpointResponse,
    TurnCheckpointSaved, TurnCheckpointUploads, TurnResume,
};
use crate::durable_task::{CheckpointObject, CheckpointPage, CheckpointPin};

/// One checkpoint action, typed by the response variant that answers it.
pub trait TurnCheckpointRequest: Into<TurnCheckpointAction> {
    /// What the answering [`TurnCheckpointResponse`] variant carries.
    type Response: Send;

    /// The answering variant's value; any other variant is `None`.
    fn answer(response: TurnCheckpointResponse) -> Option<Self::Response>;
}

/// Wraps `$request` as its action variant and reads its answer from
/// `$answer`, whose value, when it carries one, is `$value`.
macro_rules! answered_by {
    ($request:ident, $action:ident => $answer:ident($value:ty)) => {
        impl From<$request> for TurnCheckpointAction {
            fn from(request: $request) -> Self {
                Self::$action(request)
            }
        }

        impl TurnCheckpointRequest for $request {
            type Response = $value;

            fn answer(response: TurnCheckpointResponse) -> Option<$value> {
                match response {
                    TurnCheckpointResponse::$answer(value) => Some(value),
                    _ => None,
                }
            }
        }
    };
    ($request:ident, $action:ident => $answer:ident) => {
        impl From<$request> for TurnCheckpointAction {
            fn from(request: $request) -> Self {
                Self::$action(request)
            }
        }

        impl TurnCheckpointRequest for $request {
            type Response = ();

            fn answer(response: TurnCheckpointResponse) -> Option<()> {
                matches!(response, TurnCheckpointResponse::$answer).then_some(())
            }
        }
    };
}

answered_by!(TurnCheckpointBegin, Begin => Begun(Box<TurnCheckpointPlan>));
answered_by!(TurnCheckpointObjects, Objects => Objects(Vec<CheckpointObject>));
answered_by!(TurnCheckpointUploads, Uploads => Uploads(Vec<CheckpointObject>));
answered_by!(TurnCheckpointAuthorize, Authorize => Authorized(Vec<CheckpointObject>));
answered_by!(TurnCheckpointPageWrite, PageWrite => PageWritten(CheckpointPage));
answered_by!(TurnCheckpointPageRead, PageRead => PageRead(CheckpointPage));
answered_by!(TurnCheckpointPinRenew, PinRenew => PinRenewed(CheckpointPin));
answered_by!(TurnCheckpointPinRelease, PinRelease => PinReleased);
answered_by!(TurnCheckpointCommit, Commit => Committed(Box<TurnResume>));
answered_by!(TurnCheckpointRead, Read => Read(Box<TurnCheckpointSaved>));
answered_by!(TurnCheckpointAdopt, Adopt => Adopted(Vec<FoldRef>));
answered_by!(TurnCheckpointAbandon, Abandon => Abandoned);
