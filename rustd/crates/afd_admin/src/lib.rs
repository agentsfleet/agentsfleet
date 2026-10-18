//! Privileged platform administration, kept out of the runner control plane.
//!
//! This crate owns only the priced-model catalogue and the reveal-free
//! platform-default metadata. Platform Library curation lives in
//! `afd_authoring`, beside the native candidates it publishes.

#![deny(clippy::self_named_module_files)]

mod error;
mod model;
mod platform_key;
mod sql;

pub use error::{Error, Result};
pub use model::{CreateModel, DeleteModel, Model, ModelInput, ModelUpdate, Models};
pub use platform_key::{PlatformKey, PlatformKeyInput, PlatformKeys, SetPlatformKey};
