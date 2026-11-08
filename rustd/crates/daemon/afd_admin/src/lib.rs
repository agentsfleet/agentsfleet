//! Privileged platform administration, kept out of the runner control plane.
//!
//! This crate owns only the priced-model catalogue and the reveal-free
//! platform-default metadata. Platform Library curation lives in
//! `afd_authoring`, beside the native candidates it publishes.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod catalogue;
mod decision_revision;
mod error;
mod model;
mod model_page;
mod model_validation;
mod platform_key;
mod sql;
#[cfg(feature = "test-util")]
pub mod test_util;

pub use error::{Error, Result};
pub use model::{CreateModel, DecisionModel, DeleteModel, Model, ModelInput, ModelUpdate, Models};
pub use model_page::ModelPage;
pub use platform_key::{PlatformKey, PlatformKeyInput, PlatformKeys, SetPlatformKey};
