//! A stored authoring document, turned once into the policy every gate reads.
//!
//! # The shape of the read
//!
//! One `serde_json::from_str` builds [`raw::Document`] — presence, types,
//! collections, tagged unions and defaults, all from the schema. One
//! `TryFrom` then applies the rules serde cannot know. Nothing walks a map,
//! nothing frees anything, and the two halves cannot interleave, because they
//! are separate types.
//!
//! # Parse once, share the result
//!
//! [`FleetConfig`] is `Send + Sync` and holds no borrow of the document it was
//! read from, so a claim can parse it once and hand every downstream gate an
//! `Arc` of the same value. A gate that needs one field reads it off that
//! value rather than parsing the whole document again per request.

mod anomaly;
mod attach;
mod binding_match;
pub mod budget;
mod condition;
mod gates;
mod policy;
pub(crate) mod raw;
mod repository;
mod trigger;

use std::sync::Arc;

use garde::Validate as _;

use crate::error::{Error, ErrorKind, Result, missing};
use crate::name::{CredentialName, FleetName};

pub use self::anomaly::{AnomalyRule, Pattern};
pub use self::budget::{Budget, BudgetCeiling};
pub use self::condition::Condition;
pub use self::gates::{Behavior, DEFAULT_TIMEOUT_MS, GatePolicy, GateRule};
pub use self::policy::{ContextBudget, Network};
pub use self::repository::{Access, Reach, Recorded, RepositoryBinding};
pub use self::trigger::{
    ChannelId, Cron, KIND_CRON, KIND_MENTION, KIND_WEBHOOK, Mention, TRIGGER_KINDS, Trigger,
    VERIFIED_WEBHOOK_SOURCES, WEBHOOK_SOURCE_APPROVAL, WEBHOOK_SOURCE_GITHUB,
    WEBHOOK_SOURCE_LINEAR, WEBHOOK_SOURCE_SLACK, Webhook, WebhookSignature,
};

/// The key naming the fleet.
const NAME: &str = "name";
/// The key naming what may wake it.
const TRIGGERS: &str = "triggers";
/// The key naming what it may dispatch.
const TOOLS: &str = "tools";
/// The key naming what it may spend.
const BUDGET: &str = "budget";

/// The refusal a key outside the schema earns, wherever it is found.
const REASON_UNSUPPORTED_KEY: &str = "unsupported configuration key";

/// Every key the runtime block accepts.
///
/// One list, read for one purpose: naming a runtime key found at the TOP level,
/// which is an author who forgot to indent. The accepted set itself is the
/// [`raw::Runtime`] struct, so there is no second copy of these eleven strings
/// to drift into disagreement with it.
/// `every_runtime_key_is_accepted_by_the_schema` proves this list still agrees.
const RUNTIME_KEYS: [&str; 11] = [
    TRIGGERS,
    TOOLS,
    "credentials",
    "network",
    BUDGET,
    "gates",
    "model",
    "context",
    "repositories",
    "repository_access",
    "repository_base",
];

/// A fleet's resolved policy.
///
/// Every field is already checked. A holder of this type does not re-validate,
/// because there is no way to construct one that would need it.
#[derive(Debug, Clone, PartialEq)]
pub struct FleetConfig {
    /// The fleet's name.
    name: FleetName,
    /// What may wake it. Never empty.
    triggers: Box<[Trigger]>,
    /// Which tools it may dispatch.
    tools: Box<[Box<str>]>,
    /// Which secrets it may read.
    credentials: Box<[CredentialName]>,
    /// Where it may reach, when it declares a policy.
    network: Option<Network>,
    /// What it may spend.
    budget: Budget,
    /// Which actions need a human, when it declares any.
    gates: Option<GatePolicy>,
    /// Which repositories its credentials may reach.
    repository_binding: Option<RepositoryBinding>,
    /// An opaque model identifier, passed through untouched.
    model: Option<Box<str>>,
    /// Context-budget overrides, when it declares any.
    context: Option<ContextBudget>,
}

impl FleetConfig {
    /// Reads a document being authored now.
    ///
    /// # Errors
    /// Whichever rule the document broke.
    pub fn authored(document: &str) -> Result<Self> {
        Self::parse(document)
    }

    /// Reads a document out of the datastore.
    ///
    /// Uses exactly the same validation as [`authored`](Self::authored).
    ///
    /// # Errors
    /// Whichever rule the document broke.
    pub fn stored(document: &str) -> Result<Self> {
        Self::parse(document)
    }

    /// The one parse both entry points share.
    fn parse(document: &str) -> Result<Self> {
        // Three stages, each owned by whoever does it best: serde reads the
        // shape, garde proves the bounds the schema declares, and `resolve`
        // applies the rules only this product knows.
        let mut ignored = false;
        let mut decoder = serde_json::Deserializer::from_str(document);
        let parsed: raw::Document = serde_ignored::deserialize(&mut decoder, |_path| {
            ignored = true;
        })?;
        decoder.end()?;
        if ignored {
            return Err(ErrorKind::InvalidDefinition {
                reason: REASON_UNSUPPORTED_KEY,
            }
            .into());
        }
        parsed.validate()?;
        Self::resolve(parsed)
    }

    /// Reads a document and shares it.
    ///
    /// The claim path's entry point: one parse, then an `Arc` every gate reads
    /// concurrently.
    ///
    /// # Errors
    /// Whichever rule the document broke.
    pub fn stored_shared(document: &str) -> Result<Arc<Self>> {
        Self::stored(document).map(Arc::new)
    }

    /// Applies the rules serde cannot know.
    fn resolve(parsed: raw::Document) -> Result<Self> {
        if let Some(misplaced) = parsed
            .extra
            .keys()
            .find(|key| RUNTIME_KEYS.contains(&key.as_str()))
        {
            return Err(Error::from(ErrorKind::RuntimeKeyOutsideBlock {
                field: misplaced.as_str().into(),
            }));
        }

        if !parsed.extra.is_empty() {
            return Err(ErrorKind::InvalidDefinition {
                reason: REASON_UNSUPPORTED_KEY,
            }
            .into());
        }

        let mut runtime = parsed
            .runtime
            .ok_or(Error::from(ErrorKind::RuntimeBlockRequired))?;

        if let Some(unknown) = runtime.extra.keys().next() {
            return Err(Error::from(ErrorKind::UnknownRuntimeKey {
                field: unknown.as_str().into(),
            }));
        }

        // Taken before the struct is consumed field-by-field: the binding
        // spans three keys and reads them together, which is what makes a
        // half-declared one detectable at all.
        let repository_binding = RepositoryBinding::parse(&mut runtime)?;

        Ok(Self {
            name: parsed
                .name
                .ok_or_else(|| missing(NAME))
                .and_then(|authored| FleetName::parse(&authored))?,
            // Absent is the empty set: a fleet woken only by messages.
            triggers: trigger::parse_set(runtime.triggers.unwrap_or_default())?,
            // Already bounded by the schema, so this is ownership only.
            tools: runtime
                .tools
                .ok_or_else(|| missing(TOOLS))?
                .into_iter()
                .map(Into::into)
                .collect(),
            // The charset is stronger than a bound and belongs to the type, so
            // it stays a constructor rather than an annotation.
            credentials: runtime
                .credentials
                .unwrap_or_default()
                .iter()
                .map(String::as_str)
                .map(CredentialName::parse)
                .collect::<Result<_>>()?,
            network: runtime.network.map(Network::try_from).transpose()?,
            budget: runtime
                .budget
                .ok_or_else(|| missing(BUDGET))
                .and_then(Budget::try_from)?,
            gates: runtime.gates.map(GatePolicy::try_from).transpose()?,
            repository_binding,
            // Empty reads as absent: the runner treats an unset model as
            // "resolve from the tenant selection", and an empty string is how a
            // template that filled in nothing spells the same thing.
            model: runtime
                .model
                .filter(|value| !value.is_empty())
                .map(Into::into),
            context: runtime.context.map(ContextBudget::try_from).transpose()?,
        })
    }

    /// The fleet's name.
    #[must_use]
    pub const fn name(&self) -> &FleetName {
        &self.name
    }

    /// What may wake it. Never empty.
    #[must_use]
    pub fn triggers(&self) -> &[Trigger] {
        &self.triggers
    }

    /// Which tools it may dispatch.
    #[must_use]
    pub fn tools(&self) -> &[Box<str>] {
        &self.tools
    }

    /// Which secrets it may read.
    #[must_use]
    pub fn credentials(&self) -> &[CredentialName] {
        &self.credentials
    }

    /// Where it may reach, when it declares a policy.
    #[must_use]
    pub const fn network(&self) -> Option<&Network> {
        self.network.as_ref()
    }

    /// What it may spend.
    #[must_use]
    pub const fn budget(&self) -> Budget {
        self.budget
    }

    /// Which actions need a human, when it declares any.
    #[must_use]
    pub const fn gates(&self) -> Option<&GatePolicy> {
        self.gates.as_ref()
    }

    /// Which repositories its credentials may reach.
    #[must_use]
    pub const fn repository_binding(&self) -> Option<&RepositoryBinding> {
        self.repository_binding.as_ref()
    }

    /// Its opaque model identifier.
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Its context-budget overrides.
    #[must_use]
    pub const fn context(&self) -> Option<ContextBudget> {
        self.context
    }
}

#[cfg(test)]
mod tests {
    use super::{FleetConfig, RUNTIME_KEYS, raw};

    #[test]
    fn every_runtime_key_is_accepted_by_the_schema() {
        for key in RUNTIME_KEYS {
            let block = format!(r#"{{"{key}": null}}"#);
            // A key the schema reads may still refuse `null` (`triggers` does:
            // it is a list or absent). That refusal is proof enough the key is
            // read, because an unknown key lands in `extra` and never errors.
            let Ok(runtime) = serde_json::from_str::<raw::Runtime>(&block) else {
                continue;
            };

            assert!(
                !runtime.extra.contains_key(key),
                "`{key}` is listed as a runtime key but the schema does not accept it, \
                 so a document authoring it at the top level would be misreported"
            );
        }
    }

    #[test]
    fn a_config_is_shareable_across_threads() {
        const fn assert_shareable<T: Send + Sync>() {}
        assert_shareable::<FleetConfig>();
    }
}
