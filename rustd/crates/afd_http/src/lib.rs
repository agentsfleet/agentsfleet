//! Shared HTTP policy used by independent API handler planes.
//!
//! This crate owns transport concepts that do not belong to tenant, runner, or
//! operator domain services. It stays below every handler crate so those crates
//! remain siblings and `afd_api` can compose them without a dependency cycle.

pub mod admission;
pub mod auth;
pub mod client;
pub mod envelope;
pub mod etag;
pub mod handler;
pub mod request_id;
pub mod route;
pub mod services;
