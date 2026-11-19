//! The provider seam: `guffbolt`'s [`Model`](guffbolt::Model), failing with
//! this crate's [`Error`].
//!
//! The turn's vocabulary, its request, chunks, usage and transcript, is
//! `guffbolt`'s and provider-neutral. Each wire maps it onto its own
//! function-calling shape, so the loop never knows which provider it drives.
//! The one provider-owned value the loop carries is a turn's
//! [`Replay`](guffbolt::Replay): the turn's own assistant message as rig
//! records it, which this crate writes at a turn's end and reads back when
//! the next request carries it.

use crate::error::Error;

/// A model provider a run drives: what [`Connect`](crate::Connect) hands
/// back, and what the loop streams each turn from.
pub type Provider = dyn guffbolt::Model<Error = Error>;
