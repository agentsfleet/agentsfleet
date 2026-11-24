//! What a fleet and the workspace's model choices read as.

mod models;
mod output;

pub use models::{ModelEligibility, ModelPricing, ModelPurpose, ModelsQuery, WorkspaceModel};
pub use output::{CreatedFleet, Page, SavedFleet};

pub(crate) const MAX_CURSOR_BYTES: usize = 4096;

pub(crate) fn present<'de, T: serde::Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
