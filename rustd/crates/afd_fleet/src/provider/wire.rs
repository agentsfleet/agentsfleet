//! What a resolved provider tells the engine to dial.
//!
//! Separate from [`Resolved`](super::Resolved) because it answers a different
//! question. That type is what the resolution DECIDED — a posture, a model, a
//! key, an endpoint. This is how the decision is spelled on the wire, and the
//! spelling has one rule that is not obvious from the value.
//!
//! # A custom endpoint is never announced under its raw provider name
//!
//! The engine classifies on the provider STRING. `openai-compatible` paired
//! with a URL maps to no documented provider at all — an undefined route — so a
//! custom endpoint is announced as `custom:<url>`, which classifies as a
//! compatible provider and honours the override.
//!
//! `service_endpoint.zig` needs a fourth outcome to hold that: an allocation
//! failure building the prefixed name, degrading to the named-provider shape so
//! the undefined pairing cannot escape. `format!` does not fail, so the branch
//! and its reasoning are gone — the pairing is unrepresentable here rather than
//! defended against.

use std::borrow::Cow;

use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;

use super::resolved::Resolved;

/// What the lease tells the engine to dial.
///
/// Three fields that must agree, so they are produced together by one function
/// rather than read off three call sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wire<'a> {
    /// The provider name the engine classifies on.
    pub provider: Cow<'a, str>,
    /// The URL override, for a custom endpoint.
    pub base_url: Option<&'a str>,
    /// The host the egress allowlist admits, empty for a named provider.
    pub inference_host: &'a str,
}

impl Resolved {
    /// The provider triple this resolution puts on the wire.
    ///
    /// All three come out together, which is what makes the module note's rule
    /// structural: a caller that set `base_url` and forgot the prefix would
    /// produce the undefined pairing, and there is no such caller.
    #[must_use]
    pub fn wire(&self) -> Wire<'_> {
        self.endpoint.as_ref().map_or_else(
            || Wire {
                provider: Cow::Borrowed(&self.provider),
                base_url: None,
                // A named provider dials a built-in host, so the allowlist has
                // nothing extra to admit. Empty, exactly as the Zig leaves it.
                inference_host: "",
            },
            |dialled| Wire {
                provider: Cow::Owned(format!(
                    "{CUSTOM_PROVIDER_PREFIX}{base}",
                    base = dialled.base_url
                )),
                base_url: Some(&dialled.base_url),
                inference_host: &dialled.inference_host,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Resolved;
    use crate::money::rates::Posture;
    use crate::provider::resolved::{Dialled, SecretString};

    fn resolved(endpoint: Option<Dialled>) -> Resolved {
        Resolved::new(
            Posture::SelfManaged,
            "openai-compatible".into(),
            "some-model".into(),
            0,
            endpoint,
            SecretString::new("k".to_owned()),
        )
    }

    #[test]
    fn a_named_provider_passes_through_with_nothing_added() {
        let named = Resolved::new(
            Posture::Platform,
            "anthropic".into(),
            "some-model".into(),
            0,
            None,
            SecretString::new("k".to_owned()),
        );
        let wire = named.wire();

        assert_eq!(wire.provider, "anthropic");
        assert_eq!(wire.base_url, None);
        // Empty rather than the built-in host: the allowlist has nothing extra
        // to admit, and inventing one here would be a second source of truth
        // about where a named provider lives.
        assert_eq!(wire.inference_host, "");
    }

    #[test]
    fn a_custom_endpoint_is_announced_under_the_custom_prefix() {
        // The undefined route this exists to prevent: `openai-compatible`
        // paired with a URL classifies as no documented provider at all.
        let custom = resolved(Some(Dialled {
            base_url: "https://vllm.corp/v1".into(),
            inference_host: "vllm.corp".into(),
        }));
        let wire = custom.wire();

        assert_eq!(wire.provider, "custom:https://vllm.corp/v1");
        assert_eq!(wire.base_url, Some("https://vllm.corp/v1"));
        assert_eq!(wire.inference_host, "vllm.corp");
    }

    #[test]
    fn the_url_and_the_prefixed_name_can_never_disagree() {
        // One function produces all three, so the pairing the Zig has to defend
        // with a degradation branch is unrepresentable here. Asserted as the
        // relationship rather than the literal.
        for url in [
            "https://vllm.corp/v1",
            "https://gw.example.com:8443/openai/v1",
            "https://[2606:4700:4700::1111]/v1",
        ] {
            let wire = resolved(Some(Dialled {
                base_url: url.into(),
                inference_host: "irrelevant".into(),
            }));
            let wire = wire.wire();

            assert_eq!(wire.base_url, Some(url));
            assert!(wire.provider.ends_with(url), "{}", wire.provider);
            assert!(wire.provider.starts_with("custom:"), "{}", wire.provider);
        }
    }

    #[test]
    fn a_host_is_announced_only_alongside_the_url_it_was_validated_from() {
        // The allowlist admits `inference_host`; the run dials `base_url`. If
        // one could appear without the other the run would reach a host nothing
        // cleared, or be refused a host it was cleared for.
        assert_eq!(resolved(None).wire().inference_host, "");
        let paired = resolved(Some(Dialled {
            base_url: "https://vllm.corp/v1".into(),
            inference_host: "vllm.corp".into(),
        }));
        let wire = paired.wire();
        assert!(wire.base_url.is_some() && !wire.inference_host.is_empty());
    }
}
