//! Privileged platform administration, kept out of the runner control plane.
//!
//! This crate owns only the priced-model catalogue and the reveal-free
//! platform-default metadata. Fleet Bundle catalogue administration remains in
//! `afd_library`, where its manifest parser and importer already live.

mod error;
mod model;
mod platform_key;

pub use error::{Error, Result};
pub use model::{CreateModel, DeleteModel, Model, ModelInput, ModelRates, Models};
pub use platform_key::{PlatformKey, PlatformKeyInput, PlatformKeys, SetPlatformKey};
