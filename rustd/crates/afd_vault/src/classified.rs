//! What a stored credential IS — the dashboard's union, as a Rust sum type.
//!
//! The dashboard narrows on a tagged union (`ui/packages/app/lib/api/secrets.ts`)
//! where the fields a kind carries are exactly the fields it CAN carry: a
//! `provider_key` names its provider, a `custom_endpoint` names its provider
//! and may name a URL, and a `custom_secret` names nothing. The flat struct
//! this replaces mirrored that contract by CONVENTION — two hand-written
//! degrade arms, one in the list and one in the batch describe, each
//! remembering to shed the descriptors a degraded row must not carry. The Zig
//! peer forgot, and could emit `kind: custom_secret` beside a provider label —
//! a shape the union does not admit.
//!
//! Here the shed is structural: [`Classified::CustomSecret`] has no field to
//! carry a provider in, so no future edit can reintroduce the stitch without
//! changing this type — at which point the dashboard's union is the review
//! comment. Invariants live in the type, not in every caller
//! (M-STRONG-TYPES-GUARD), and the one fallible decision is made once, at
//! construction, in [`Classified::classify`].
//!
//! # Two degrades, not one
//!
//! A spelling this build cannot place was always a degrade. The union adds a
//! second the flat struct could not express: a `provider_key` or
//! `custom_endpoint` row whose `meta_provider` is NULL has a kind but not the
//! field that kind REQUIRES — the wire union spells `provider: string`, not
//! optional — so it degrades the same way. Both are logged at `debug` for the
//! reason the original decision recorded: an un-backfilled row is expected on
//! an older database, and a page of them must not be a wall of warnings.

use crate::projection::Kind;

/// The column the degrade logs name, spelled once.
const COLUMN_KIND: &str = "meta_kind";

/// The classified half of a credential's projection.
///
/// Everything here is a fact about WHAT the credential is; the independent
/// facts that ride beside it — key presence, the name, the timestamps — stay
/// on the projection that carries this. See the module note for why this is a
/// sum type rather than a struct of options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classified {
    /// A named provider's key — anthropic, openai, and the rest.
    ProviderKey {
        /// The provider label, as the write path projected it.
        provider: Box<str>,
    },
    /// An OpenAI-compatible endpoint the operator supplied a URL for.
    CustomEndpoint {
        /// The provider label, as the write path projected it.
        provider: Box<str>,
        /// The endpoint, where the row stored one.
        base_url: Option<Box<str>>,
    },
    /// Anything else — including every row this build cannot describe.
    CustomSecret,
}

impl Classified {
    /// The kind this classification answers on the wire.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        match self {
            Self::ProviderKey { .. } => Kind::ProviderKey,
            Self::CustomEndpoint { .. } => Kind::CustomEndpoint,
            Self::CustomSecret => Kind::CustomSecret,
        }
    }

    /// The provider label, for the kinds that carry one.
    #[must_use]
    pub fn provider(&self) -> Option<&str> {
        match self {
            Self::ProviderKey { provider } | Self::CustomEndpoint { provider, .. } => {
                Some(provider)
            }
            Self::CustomSecret => None,
        }
    }

    /// The custom endpoint, where one may be displayed.
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        match self {
            Self::CustomEndpoint { base_url, .. } => base_url.as_deref(),
            Self::ProviderKey { .. } | Self::CustomSecret => None,
        }
    }

    /// Classifies one stored projection — the single place the decision lives.
    ///
    /// One decision, two readers: the workspace list and the registry page's
    /// batch describe both call this, and degrade identically because there is
    /// one function rather than two written to agree. A spelling this build
    /// cannot place, a row that has none, and a labelled kind missing the
    /// provider its variant requires all answer [`Classified::CustomSecret`] —
    /// which cannot carry the descriptors a degraded row must not present.
    pub(crate) fn classify(
        stored: Option<&str>,
        name: &str,
        provider: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        let Some(kind) = stored.and_then(Kind::parse) else {
            // `debug`, not `warn`: an un-backfilled row is expected on a
            // database older than the projection columns. The stored spelling
            // is carried so a peer daemon's vocabulary is visible when it
            // appears, and `backfilled` tells a spelling this build cannot
            // place apart from a row that never got one.
            let spelling = stored.unwrap_or_default();
            let backfilled = stored.is_some();
            tracing::debug!(
                column = COLUMN_KIND,
                stored = spelling,
                backfilled,
                name,
                event = "secret_kind_degraded",
            );
            return Self::CustomSecret;
        };
        match (kind, provider) {
            (Kind::ProviderKey, Some(provider)) => Self::ProviderKey {
                provider: provider.into_boxed_str(),
            },
            (Kind::CustomEndpoint, Some(provider)) => Self::CustomEndpoint {
                provider: provider.into_boxed_str(),
                base_url: base_url.map(String::into_boxed_str),
            },
            (Kind::CustomSecret, _) => Self::CustomSecret,
            (Kind::ProviderKey | Kind::CustomEndpoint, None) => {
                // The kind is real but the field its wire shape REQUIRES is
                // not there — `provider: string` in the union, no optional.
                // Presenting the kind without it would hand the dashboard a
                // row outside its own type.
                let stored_kind = kind.as_str();
                tracing::debug!(
                    column = COLUMN_KIND,
                    stored = stored_kind,
                    name,
                    event = "secret_provider_missing",
                );
                Self::CustomSecret
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each stored spelling classifies to the variant carrying its fields.
    #[test]
    fn should_classify_each_spelling_with_the_fields_its_kind_carries() {
        assert_eq!(
            Classified::classify(
                Some("provider_key"),
                "a-key",
                Some("anthropic".to_owned()),
                None
            ),
            Classified::ProviderKey {
                provider: "anthropic".into()
            }
        );
        assert_eq!(
            Classified::classify(
                Some("custom_endpoint"),
                "gw",
                Some("openai-compatible".to_owned()),
                Some("https://gw.example.com/v1".to_owned())
            ),
            Classified::CustomEndpoint {
                provider: "openai-compatible".into(),
                base_url: Some("https://gw.example.com/v1".into())
            }
        );
        assert_eq!(
            Classified::classify(Some("custom_secret"), "note", None, None),
            Classified::CustomSecret
        );
    }

    /// A spelling this build cannot place degrades, and the degrade cannot
    /// carry a provider — there is no field for one.
    ///
    /// The populated provider and base URL are the point: the Zig peer kept
    /// them beside a degraded kind, a shape the dashboard's union does not
    /// admit, and this is the case that pins the difference.
    #[test]
    fn should_degrade_an_unplaceable_spelling_and_structurally_shed_its_descriptors() {
        let classified = Classified::classify(
            Some("a-kind-no-build-spells"),
            "foreign",
            Some("anthropic".to_owned()),
            Some("https://kept.example.com".to_owned()),
        );

        assert_eq!(classified, Classified::CustomSecret);
        assert_eq!(classified.kind(), Kind::CustomSecret);
        assert_eq!(classified.provider(), None);
        assert_eq!(classified.base_url(), None);
    }

    /// Every history that leaves a row unplaceable degrades the same way.
    ///
    /// Ported from `labelled`'s suite when that function folded in here: `None`
    /// is a row from before the projection columns; the versioned spelling is a
    /// peer daemon's newer vocabulary; the empty string and the upper-cased
    /// spelling are corruptions. The stored spelling is exact-match on purpose —
    /// a case-insensitive parse would quietly accept a value no writer of this
    /// column ever produced.
    #[test]
    fn should_degrade_every_history_that_leaves_a_row_unplaceable() {
        for stored in [
            None,
            Some("provider_key_v2"),
            Some(""),
            Some("PROVIDER_KEY"),
        ] {
            assert_eq!(
                Classified::classify(stored, "pre-backfill", None, None),
                Classified::CustomSecret,
                "stored {stored:?}"
            );
        }
    }

    /// A labelled kind missing the provider its wire shape requires degrades
    /// rather than presenting a `provider_key` with no provider.
    ///
    /// The flat struct could not express this rule — it emitted the kind and
    /// omitted the field, a row outside the dashboard's union. The sum type
    /// makes the repair the same as every other degrade.
    #[test]
    fn should_degrade_a_labelled_kind_missing_its_required_provider() {
        assert_eq!(
            Classified::classify(Some("provider_key"), "half-projected", None, None),
            Classified::CustomSecret
        );
        assert_eq!(
            Classified::classify(
                Some("custom_endpoint"),
                "half-projected",
                None,
                Some("https://orphaned.example.com".to_owned())
            ),
            Classified::CustomSecret
        );
    }

    /// A provider key drops a stray base URL by construction — its variant has
    /// no field for one, matching the union where only `custom_endpoint` does.
    #[test]
    fn should_not_carry_a_base_url_on_a_provider_key() {
        let classified = Classified::classify(
            Some("provider_key"),
            "a-key",
            Some("anthropic".to_owned()),
            Some("https://stray.example.com".to_owned()),
        );

        assert_eq!(classified.base_url(), None);
        assert_eq!(classified.provider(), Some("anthropic"));
    }
}
