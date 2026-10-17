//! Persisted catalogue values, independent of database operations.

use afd_core::id::Uuid7;
use afd_wire::inference::ModelInference;

/// Mutable priced-model fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelRates {
    context_cap_tokens: i32,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
}

impl ModelRates {
    /// Builds already-validated rates for persistence.
    #[must_use]
    pub const fn new(
        context_cap_tokens: i32,
        input_nanos_per_mtok: i64,
        cached_input_nanos_per_mtok: i64,
        output_nanos_per_mtok: i64,
    ) -> Self {
        Self {
            context_cap_tokens,
            input_nanos_per_mtok,
            cached_input_nanos_per_mtok,
            output_nanos_per_mtok,
        }
    }

    /// Maximum model context.
    #[must_use]
    pub const fn context_cap_tokens(self) -> i32 {
        self.context_cap_tokens
    }

    /// Input-token nanos per million tokens.
    #[must_use]
    pub const fn input_nanos_per_mtok(self) -> i64 {
        self.input_nanos_per_mtok
    }

    /// Cached-input nanos per million tokens.
    #[must_use]
    pub const fn cached_input_nanos_per_mtok(self) -> i64 {
        self.cached_input_nanos_per_mtok
    }

    /// Output-token nanos per million tokens.
    #[must_use]
    pub const fn output_nanos_per_mtok(self) -> i64 {
        self.output_nanos_per_mtok
    }
}

/// Create input whose provider/model identity is immutable afterward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInput {
    pub(super) provider: String,
    pub(super) identifier: String,
    pub(super) rates: ModelRates,
    pub(super) inference: Option<ModelInference>,
}

impl ModelInput {
    /// Builds an already-validated model input.
    #[must_use]
    pub const fn new(provider: String, model_id: String, rates: ModelRates) -> Self {
        Self {
            provider,
            identifier: model_id,
            rates,
            inference: None,
        }
    }

    /// Adds the model's non-secret inference configuration.
    #[must_use]
    pub fn with_inference(mut self, inference: Option<ModelInference>) -> Self {
        self.inference = inference;
        self
    }
}

/// One priced model row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub(super) id: Uuid7,
    pub(super) provider: String,
    pub(super) identifier: String,
    pub(super) rates: ModelRates,
    pub(super) inference: Option<ModelInference>,
}

impl Model {
    /// Opaque row identifier.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.id
    }

    /// Provider identifier.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Provider-native model identifier.
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.identifier
    }

    /// Mutable rates and cap.
    #[must_use]
    pub const fn rates(&self) -> ModelRates {
        self.rates
    }

    /// Persisted inference settings, absent until the administrator configures them.
    #[must_use]
    pub const fn inference(&self) -> Option<&ModelInference> {
        self.inference.as_ref()
    }
}

/// Create outcome kept distinct from datastore failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateModel {
    /// The new row, including its minted identifier.
    Created(Model),
    /// A row already has the provider/model identity.
    Duplicate,
}

/// Delete outcome kept distinct from datastore failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteModel {
    /// Row was removed and the revision advanced.
    Deleted,
    /// No row matches the identifier.
    NotFound,
    /// The active platform default still references this row.
    InUse,
}
