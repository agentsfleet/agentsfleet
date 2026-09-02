//! Which vendor spellings may appear on the wire, and what happens to the rest.
//!
//! # Why an unmapped provider is omitted rather than exported
//!
//! `gen_ai.provider.name` has a closed set of well-known values at the pinned
//! commit. A configured provider that does not map to one of them is not a new
//! member of that set — it is a private string, and exporting it under a
//! standard key tells every consumer downstream that a standard vocabulary
//! contains a word it does not. Omitting the attribute says less and says it
//! truthfully, and the omission is COUNTED, so an operator can see a vendor
//! this daemon does not know rather than wondering where the attribution went.
//!
//! The alternative — a private key, `agentsfleet.provider.name` — was not
//! taken: it would double the vocabulary for a fact the standard already names
//! whenever it is nameable at all.

/// The exact well-known `gen_ai.provider.name` values at the pinned commit.
///
/// Sorted, and the order is not decorative: [`normalize`] binary-searches it,
/// and `the_well_known_table_is_sorted` fails the build's own test if an
/// insertion breaks the order a search depends on.
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
    "openai",
    "perplexity",
    "x_ai",
];

/// The canonical spelling of `stored`, or nothing when it maps to none.
///
/// Case-insensitive because the identifier reaches this daemon unvalidated
/// from the command-line provider option, where `Anthropic` and `anthropic`
/// name the same vendor. What comes back is always the table's own spelling,
/// so tolerating case removes a false omission without ever putting a
/// non-standard spelling on the wire. ASCII-only is correct: every well-known
/// name is ASCII.
#[must_use]
pub fn normalize(stored: &str) -> Option<&'static str> {
    WELL_KNOWN
        .binary_search_by(|known| compare_ignoring_ascii_case(known, stored))
        .ok()
        .and_then(|index| WELL_KNOWN.get(index).copied())
}

/// Orders two names the way the table is sorted, ignoring ASCII case.
///
/// Hand-written rather than `to_lowercase().cmp()`, and not for speed: the
/// allocation-free version is what lets this run inside a search over a
/// `&'static` table without touching the allocator on a request path. The
/// table itself is lower-case, so lowering the needle alone would be enough
/// for equality but not for ORDER — `binary_search_by` needs both sides
/// compared under the same rule or it can walk past a match.
fn compare_ignoring_ascii_case(known: &str, stored: &str) -> core::cmp::Ordering {
    let lowered = stored.bytes().map(|byte| byte.to_ascii_lowercase());
    known.bytes().cmp(lowered)
}

#[cfg(test)]
mod tests {
    use super::{WELL_KNOWN, normalize};

    /// The table is sorted, which is what the search rests on.
    ///
    /// Not ground truth: `binary_search_by` on an unsorted slice does not fail
    /// loudly, it silently misses members — so a provider inserted in the wrong
    /// place would stop being attributed with nothing to say so.
    #[test]
    fn the_well_known_table_is_sorted() {
        assert!(
            WELL_KNOWN.is_sorted(),
            "the table is binary-searched; an unsorted entry silently stops matching"
        );
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

    /// A vendor the standard does not name maps to nothing at all.
    ///
    /// The case the whole module exists for: the answer is an omission, never
    /// a private spelling wearing a standard key.
    #[test]
    fn a_provider_outside_the_standard_maps_to_nothing() {
        assert_eq!(normalize("our-internal-gateway"), None);
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("anthropic-beta"), None);
        assert_eq!(
            normalize("anthropi"),
            None,
            "a prefix of a known name is not that name"
        );
    }
}
