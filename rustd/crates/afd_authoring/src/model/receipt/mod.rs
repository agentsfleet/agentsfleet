//! Replay acknowledgements and accounting survive independently from private result text.

mod finish;
mod replay;

pub(in crate::model) use finish::finish;
pub(in crate::model) use replay::prior;
