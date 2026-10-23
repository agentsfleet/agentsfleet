//! Replay acknowledgements and accounting survive independently from private result text.

mod finish;
mod replay;

pub(in crate::model) use finish::finish;
pub(in crate::model) use replay::prior;

pub(in crate::model) struct Received {
    pub(in crate::model) decoded: crate::Result<super::provider::Decoded>,
    pub(in crate::model) cause: Option<crate::types::FailureCause>,
}
