//! The validated setup shared by browser chat, terminal chat and reviewed revisions.
//!
//! Construction and edits validate the entire replacement before returning it.
//! A Definition carries no grants, credentials, publication or runtime authority.
//!
//! The fleet's name is `config.name` and nowhere else. The `description`
//! beside it is display text: required, so every listing has a sentence to
//! show, and read by nothing that decides what a fleet may do.
//!
//! Every text bound counts characters, the unit a published `maxLength`
//! counts, so a client validating against the schema never builds a document
//! the parser refuses. Byte bounds remain only where bytes are what is
//! measured: instruction-edit offsets and the envelope read before parsing.

mod digest;
mod document;
mod edit;
#[cfg(feature = "openapi")]
pub(crate) mod schema;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::FleetConfig;
use crate::error::{ErrorKind, Result, missing};

pub use self::digest::canonical_digest;
pub use self::document::ConfigDocument;
#[cfg(feature = "openapi")]
pub use self::edit::narrow_edit_batch;
pub use self::edit::{ConfigOp, EditBatch, InstructionEdit, MAX_CONFIG_OPS, MAX_INSTRUCTION_EDITS};
#[cfg(feature = "openapi")]
pub use self::schema::DefinitionSchema;

/// Supported Definition wire version.
pub const DEFINITION_SCHEMA_VERSION: u8 = 1;
/// Explicit policy meaning included in every review and content digest.
pub const POLICY_SEMANTICS: &str = "fleet-setup-v1";
/// Maximum description, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 32 * 1024;
/// Maximum edited instructions, in characters.
pub const MAX_INSTRUCTIONS_CHARS: usize = 32 * 1024;
/// First generated instructions are deliberately smaller than later edits.
pub const MAX_INITIAL_INSTRUCTIONS_CHARS: usize = 12 * 1024;
/// The widest character in UTF-8, which is how stored text is addressed.
const UTF8_MAX_CHAR_BYTES: usize = 4;
/// The widest one character serializes to in JSON: a `\u00XX` control escape.
const JSON_MAX_CHAR_BYTES: usize = 6;
/// Room the configuration takes in an envelope beside the two texts.
const CONFIG_ALLOWANCE_BYTES: usize = 64 * 1024;
/// The largest UTF-8 byte offset an instruction edit can name: instructions
/// at their character cap, every character at its widest.
pub const MAX_INSTRUCTION_OFFSET: usize = UTF8_MAX_CHAR_BYTES * MAX_INSTRUCTIONS_CHARS;
/// Maximum Definition or edit envelope before allocating its parsed tree.
///
/// Both texts at their character caps in the widest JSON encoding, plus the
/// configuration, so the envelope never refuses what the text bounds admit.
pub const MAX_DEFINITION_BYTES: usize =
    JSON_MAX_CHAR_BYTES * (MAX_DESCRIPTION_CHARS + MAX_INSTRUCTIONS_CHARS) + CONFIG_ALLOWANCE_BYTES;
/// The `description` key, named once for the refusal that cites it.
const FIELD_DESCRIPTION: &str = "description";

/// A normalized complete setup. Empty instructions are valid only during editing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Definition {
    schema_version: u8,
    description: Box<str>,
    instructions_md: Box<str>,
    config: ConfigDocument,
}

impl<'de> Deserialize<'de> for Definition {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value).map_err(serde::de::Error::custom)
    }
}

// `description` is `Option` so serde never raises "missing field" for it: an
// absent or empty description refuses as missing, a wrong-typed or null one
// as a shape, the split `crate::error` keeps for every required key.
#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[garde(range(min = DEFINITION_SCHEMA_VERSION, max = DEFINITION_SCHEMA_VERSION))]
    schema_version: u8,
    #[serde(default, deserialize_with = "crate::optional::present")]
    #[garde(inner(length(chars, max = MAX_DESCRIPTION_CHARS), custom(afd_validate::rules::nul_free)))]
    description: Option<String>,
    #[garde(length(chars, max = MAX_INSTRUCTIONS_CHARS), custom(afd_validate::rules::nul_free))]
    instructions_md: String,
    #[garde(skip)]
    config: Value,
}

#[derive(garde::Validate)]
struct InitialInstructions<'a> {
    #[garde(length(chars, max = MAX_INITIAL_INSTRUCTIONS_CHARS))]
    text: &'a str,
}

impl Definition {
    /// Parses strict typed input and proves the shared runtime rules.
    ///
    /// # Errors
    /// Rejects unsupported versions and keys, a missing or empty description,
    /// and malformed policy.
    pub fn parse(input: &str) -> Result<Self> {
        document::bounded(input, MAX_DEFINITION_BYTES)?;
        Self::from_value(serde_json::from_str(input)?)
    }

    pub(crate) fn from_value(value: Value) -> Result<Self> {
        document::bounded(&serde_json::to_string(&value)?, MAX_DEFINITION_BYTES)?;
        let raw: Raw = serde_json::from_value(value)?;
        let raw = garde::Unvalidated::new(raw).validate()?.into_inner();
        let description = raw
            .description
            .filter(|found| !found.is_empty())
            .ok_or_else(|| missing(FIELD_DESCRIPTION))?;
        let config = ConfigDocument::parse(raw.config)?;
        Ok(Self {
            schema_version: DEFINITION_SCHEMA_VERSION,
            description: normalize_text(&description).into(),
            instructions_md: normalize_text(&raw.instructions_md).into(),
            config,
        })
    }

    /// The description every listing shows, with normalized line endings.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Instructions with normalized line endings.
    #[must_use]
    pub fn instructions_md(&self) -> &str {
        &self.instructions_md
    }

    /// The validated configuration document, preserving all supported policy.
    #[must_use]
    pub const fn config(&self) -> &ConfigDocument {
        &self.config
    }

    /// Runtime policy parsed once from this Definition.
    #[must_use]
    pub const fn runtime(&self) -> &FleetConfig {
        self.config.runtime()
    }

    /// Checks the stronger boundary for Save and explicit Library publication.
    ///
    /// # Errors
    /// Editing may be empty; publication and runnable setup may not.
    pub fn validate_for_save(&self) -> Result<()> {
        if self.instructions_md.trim().is_empty() {
            return Err(invalid(
                "instructions are required before Save or publication",
            ));
        }
        Ok(())
    }

    /// Proves the smaller text bound for the model's first generated candidate.
    ///
    /// # Errors
    /// Refuses initial instructions beyond the shared generation allowance.
    pub fn validate_initial_generation(&self) -> Result<()> {
        garde::Unvalidated::new(InitialInstructions {
            text: self.instructions_md(),
        })
        .validate()?;
        Ok(())
    }
}

pub(crate) fn invalid(reason: &'static str) -> crate::Error {
    ErrorKind::InvalidDefinition { reason }.into()
}

/// The one line ending stored text keeps.
const LINE_FEED: &str = "\n";

/// `text` with every CRLF and lone CR rewritten to LF.
///
/// Public because authored conversation text is normalized the same way, so a
/// definition and the messages that produced it hash and diff alike.
#[must_use]
pub fn normalize_text(text: &str) -> String {
    text.replace("\r\n", LINE_FEED).replace('\r', LINE_FEED)
}
