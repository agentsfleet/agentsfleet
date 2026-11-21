//! Explicit persisted-model fixtures for the three real HTTP APIs.

use super::{CHAT_PROVIDER, Fake};
use af_providers::{Connect, Connector, Provider, Result};
use af_wire::{
    inference::{InferenceApi, ModelInference},
    policy::ExecutionPolicy,
    run::RunPayload,
};

#[derive(Debug)]
pub(crate) struct FixtureConnector {
    inner: Connector,
    base: String,
    dialect: Option<String>,
}

impl FixtureConnector {
    pub(crate) fn new(fake: &Fake, dialect: Option<String>) -> Self {
        Self {
            inner: Connector::with_client(fake.client.clone()),
            base: fake.base.clone(),
            dialect,
        }
    }

    fn policy(&self, policy: &ExecutionPolicy<'_>) -> ModelInference {
        let api = match policy.provider.as_ref() {
            "anthropic" => InferenceApi::Messages,
            "openai" => InferenceApi::Responses,
            CHAT_PROVIDER | "openrouter" => InferenceApi::Chat,
            name => panic!("no fixture for {name}"),
        };
        ModelInference {
            api,
            base_url: format!("{}/v1", self.base),
            dialect: self.dialect.clone(),
            max_output_tokens: 4096,
        }
    }
}

impl Connect for FixtureConnector {
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<()> {
        let mut policy = policy.clone();
        policy.inference = Some(self.policy(&policy));
        self.inner.admit(&policy)
    }
    fn connect(&self, run: &RunPayload<'_>) -> Result<Box<Provider>> {
        let mut run = run.clone();
        run.policy.inference = Some(self.policy(&run.policy));
        self.inner.connect(&run)
    }
}
