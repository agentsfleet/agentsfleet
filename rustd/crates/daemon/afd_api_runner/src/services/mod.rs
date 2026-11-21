//! What a runner-control handler acts through: [`RunnerPlane`], and the run
//! verbs' seam only this plane reaches.
//!
//! The seams another plane also reaches live in [`afd_http::services`]; the
//! private import below lets a handler name them beside these as
//! `crate::services::…`.

mod plane;
mod run_plane;

pub use self::plane::RunnerPlane;
pub use self::run_plane::RunPlane;

pub(crate) use afd_http::services::{
    FleetSchedules, FleetSteering, RunnerStreams, WorkspaceFleets, WorkspaceTurns,
};
