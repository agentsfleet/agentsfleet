//! Public provider identities from the compiled catalogue and SDK registry.
//!
//! OpenTelemetry permits custom provider values when no well-known name
//! applies. The accepted set here is still finite: public identities owned
//! by the shipped catalogue or SDK, plus standard names and their aliases.
//! Arbitrary configured names and endpoint URLs never enter this registry.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde::Deserialize;

/// Well-known names at the pinned OpenTelemetry convention revision.
pub const WELL_KNOWN: &[&str] = &[
    "anthropic",
    "aws.bedrock",
    "azure.ai.inference",
    "azure.ai.openai",
    "cohere",
    "deepseek",
    "gcp.gemini",
    "gcp.gen_ai",
    "gcp.vertex_ai",
    "groq",
    "ibm.watsonx.ai",
    "mistral_ai",
    "moonshot_ai",
    "openai",
    "perplexity",
    "x_ai",
];

/// Vendor identifiers whose standard spelling differs from the product's.
const ALIASES: &[(&str, &str)] = &[
    ("azure.openai", "azure.ai.openai"),
    ("gemini", "gcp.gemini"),
    ("mistral", "mistral_ai"),
    ("moonshot", "moonshot_ai"),
    ("xai", "x_ai"),
];

const CATALOGUE: &str = include_str!("../../../../../scripts/model-library-allowlist.json");

#[derive(Deserialize)]
struct Catalogue<'a> {
    #[serde(borrow)]
    models: Vec<Model<'a>>,
    // Ollama lists instance-owned models rather than entries in `models`.
    #[serde(rename = "ollama")]
    _ollama: serde::de::IgnoredAny,
}

#[derive(Deserialize)]
struct Model<'a> {
    #[serde(borrow)]
    provider: &'a str,
}

struct Names {
    inputs: Vec<(&'static str, &'static str)>,
    outputs: usize,
}

static NAMES: LazyLock<Names> = LazyLock::new(|| {
    #[expect(
        clippy::expect_used,
        reason = "immutable compiled catalogue, validated by catalogue and provider tests; malformed source is a build defect"
    )]
    let catalogue: Catalogue<'static> =
        serde_json::from_str(CATALOGUE).expect("compiled model catalogue must be valid");
    let mut inputs: BTreeMap<_, _> = WELL_KNOWN.iter().map(|name| (*name, *name)).collect();
    for name in rig_core::providers::registry::ProviderId::all()
        .map(|provider| provider.vendor())
        .chain(catalogue.models.into_iter().map(|model| model.provider))
        .chain(std::iter::once("ollama"))
    {
        inputs.insert(name, name);
    }
    // Standard names win over SDK/catalogue spellings. The hosting provider
    // is never inferred from the model name or the API protocol.
    inputs.extend(ALIASES.iter().copied());
    let outputs = inputs.values().copied().collect::<BTreeSet<_>>().len();
    Names {
        inputs: inputs.into_iter().collect(),
        outputs,
    }
});

/// Canonical public provider identity, or none for a private/unrecognized name.
#[must_use]
pub fn normalize(stored: &str) -> Option<&'static str> {
    NAMES
        .inputs
        .binary_search_by(|(name, _)| compare_ignoring_ascii_case(name, stored))
        .ok()
        .and_then(|index| NAMES.inputs.get(index).map(|(_, output)| *output))
}

/// Distinct emitted provider values; aliases consume no additional series.
#[must_use]
pub fn count() -> usize {
    NAMES.outputs
}

fn compare_ignoring_ascii_case(known: &str, stored: &str) -> core::cmp::Ordering {
    let lowered = stored.bytes().map(|byte| byte.to_ascii_lowercase());
    known.bytes().cmp(lowered)
}

#[cfg(test)]
mod tests {
    use super::{WELL_KNOWN, normalize};

    #[test]
    fn supported_providers_keep_their_serving_identity() {
        for (configured, expected) in [
            ("fireworks", "fireworks"),
            ("MINIMAX", "minimax"),
            ("openrouter", "openrouter"),
            ("gemini", "gcp.gemini"),
            ("mistral", "mistral_ai"),
            ("moonshot", "moonshot_ai"),
            ("ollama", "ollama"),
            ("azure.openai", "azure.ai.openai"),
        ] {
            assert_eq!(normalize(configured), Some(expected), "{configured}");
        }
    }

    /// Every declared member maps to itself, in the table's own spelling.
    #[test]
    fn every_well_known_provider_maps_to_itself() {
        for known in WELL_KNOWN {
            assert_eq!(normalize(known), Some(*known));
        }
    }

    /// Case is tolerated on the way in and never on the way out.
    #[test]
    fn a_differently_cased_provider_maps_to_the_canonical_spelling() {
        assert_eq!(normalize("Anthropic"), Some("anthropic"));
        assert_eq!(normalize("OPENAI"), Some("openai"));
        assert_eq!(normalize("Gcp.Vertex_AI"), Some("gcp.vertex_ai"));
    }

    /// An arbitrary private provider stays outside the compiled public registry.
    ///
    /// Custom endpoint URLs and private identifiers are not public vendor names.
    #[test]
    fn a_private_provider_is_not_exported() {
        assert_eq!(normalize("our-internal-gateway"), None);
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("anthropic-beta"), None);
        for private in [
            "custom:https://models.example.com/v1",
            "https://models.example.com/v1",
            "minimax/openai",
            "accounts/fireworks/models/minimax-m3p1-flash",
            "Fireworks Inc",
            "模型供应商",
            "_other",
        ] {
            assert_eq!(normalize(private), None, "{private}");
        }
        assert_eq!(
            normalize("anthropi"),
            None,
            "a prefix of a known name is not that name"
        );
    }
}
