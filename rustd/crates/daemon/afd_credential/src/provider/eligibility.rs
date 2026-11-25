//! What a model's saved facts let it do: run a fleet.
//!
//! The workspace's model catalogue reads this to mark the choices a fleet can
//! run on. The provider rules behind it are in `af_providers::settings`; this
//! is the one place the daemon's fleet path reaches them, so `afd_setup` names
//! no provider crate.

use af_wire::inference::ModelInference;

/// A model's saved facts, as the catalogue row carries them.
#[derive(Debug, Clone, Copy)]
pub struct ModelFacts<'a> {
    /// The saved inference settings; none routes nowhere.
    pub inference: Option<&'a ModelInference>,
    /// The endpoint the model's key allows, if it names one.
    pub endpoint: Option<&'a str>,
    /// The priced context window, in tokens.
    pub context_window_tokens: i32,
    /// Fresh input nanos per million tokens.
    pub input_nanos_per_million: i64,
    /// Output nanos per million tokens.
    pub output_nanos_per_million: i64,
}

/// What a model's facts allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Eligibility {
    /// Its settings route to its endpoint, so a fleet can run on it.
    pub runtime: bool,
}

impl ModelFacts<'_> {
    /// What these facts allow: nothing for a negative price, settings that do
    /// not route, or an endpoint the settings refuse.
    #[must_use]
    pub fn eligibility(&self) -> Eligibility {
        let context = u32::try_from(self.context_window_tokens).unwrap_or_default();
        let route = self
            .inference
            .filter(|_| self.input_nanos_per_million >= 0 && self.output_nanos_per_million >= 0)
            .and_then(|settings| af_providers::settings::Configured::new(settings, context).ok())
            .filter(|route| route.accepts_endpoint(self.endpoint));
        Eligibility {
            runtime: route.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Eligibility, ModelFacts};

    /// A model with no saved settings routes nowhere, so no fleet may run on it.
    #[test]
    fn a_model_without_settings_is_eligible_for_nothing() {
        let facts = ModelFacts {
            inference: None,
            endpoint: None,
            context_window_tokens: 128_000,
            input_nanos_per_million: 1,
            output_nanos_per_million: 1,
        };
        assert_eq!(facts.eligibility(), Eligibility::default());
    }
}
