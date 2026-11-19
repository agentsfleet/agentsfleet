//! Platform-operator reads over runners and live streams.

pub(crate) mod events;
mod query;
pub(crate) mod runner_delete;
mod runner_notify;
pub(crate) mod runner_patch;
pub(crate) mod runner_register;
pub(crate) mod runners;
pub(crate) mod runs;
pub(crate) mod tenant_runners;
