//! Fleet task reads share native workspace ownership and lease lifecycle state.

mod input;
pub(crate) mod read;

pub(crate) use read::{list, one};
