//! The types one model call reads and writes, provider-neutral: each wire
//! maps them onto its own, so the loop above never knows which provider it
//! drives.

mod message;
mod model;
mod tool;

pub use self::message::{Call, Message, Replay};
pub use self::model::{Chunk, End, Hosted, Model, Request, TextKind, Usage};
pub use self::tool::{Image, ImageKind, ToolSpec};

#[cfg(test)]
mod tests;
