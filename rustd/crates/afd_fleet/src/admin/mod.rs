//! Privileged catalogue and platform-default repositories.

mod platform_key;
mod model;

pub use model::{DeleteModel, Model, ModelInput, ModelRates, Models};
pub use platform_key::{PlatformKey, PlatformKeyInput, PlatformKeys};
