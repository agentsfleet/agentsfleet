//! Platform-operator reads over runners and live streams.

pub(crate) mod events;
pub(crate) mod leases;
mod query;
pub(crate) mod runner_delete;
mod runner_notify;
pub(crate) mod runner_patch;
pub(crate) mod runner_register;
pub(crate) mod runners;
pub(crate) mod tenant_runners;
