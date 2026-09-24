use super::{Archetype, Provider};

/// No two providers share a state domain or a nonce namespace.
///
/// The invariant `registry.zig` spends a comptime double loop on, and it is
/// worth the same care here: a shared pair lets one connector's signed
/// state verify AND consume under another's callback, which is a
/// cross-connector grant with nothing in the types to notice.
#[test]
fn no_two_providers_share_a_state_domain_or_nonce_namespace() {
    for (index, one) in Provider::ALL.iter().copied().enumerate() {
        for other in Provider::ALL.iter().copied().skip(index + 1) {
            let (a, b) = (one.state_binding(), other.state_binding());
            assert_ne!(
                a.domain_prefix, b.domain_prefix,
                "`{one}` and `{other}` sign in one domain",
            );
            assert_ne!(
                a.nonce_prefix, b.nonce_prefix,
                "`{one}` and `{other}` consume one nonce namespace",
            );
        }
    }
}

/// Every state binding names both halves.
///
/// An empty domain prefix is a degenerate binding rather than a typo: it
/// removes the separation the HMAC gets from being domain-scoped, and an
/// empty nonce prefix puts every provider's slots in one namespace.
#[test]
fn every_state_binding_names_both_of_its_halves() {
    for provider in Provider::ALL.iter().copied() {
        let binding = provider.state_binding();
        assert!(!binding.domain_prefix.is_empty(), "`{provider}` domain");
        assert!(!binding.nonce_prefix.is_empty(), "`{provider}` nonce");
    }
}

/// Every OAuth 2.0 connector asks for something.
///
/// `registry.zig` raises `@compileError` on an oauth2 entry with no scopes,
/// because an authorize URL carrying none is a consent screen that grants
/// nothing and a token that opens nothing. The App-install archetype is
/// exempt by construction — a GitHub App carries its own permissions.
#[test]
fn every_oauth_connector_asks_for_at_least_one_scope() {
    for provider in Provider::ALL.iter().copied() {
        if let Archetype::Oauth2(flow) = provider.archetype() {
            assert!(!flow.scopes.is_empty(), "`{provider}` asks for nothing");
            assert!(flow.authorize_endpoint.starts_with("https://"));
            assert!(flow.token_endpoint.starts_with("https://"));
        }
    }
}

/// Each connector's declared delimiter is the one its scope list uses.
///
/// The drift guard, and the reason the delimiter is a field at all. A
/// declaration nothing checks is a comment: someone adding a provider can
/// write comma-joined scopes beside `SCOPE_SPACE` and the request would ask
/// for one scope named after all of them, which most vendors answer by
/// granting nothing and reporting success. Here the two cannot disagree —
/// a list carrying the OTHER delimiter and none of its own fails this.
#[test]
fn every_connectors_scope_list_uses_the_delimiter_it_declares() {
    for provider in Provider::ALL.iter().copied() {
        let Archetype::Oauth2(flow) = provider.archetype() else {
            continue;
        };
        let other = if flow.scope_delimiter == super::SCOPE_COMMA {
            super::SCOPE_SPACE
        } else {
            super::SCOPE_COMMA
        };

        assert!(
            flow.scopes.contains(flow.scope_delimiter),
            "`{provider}` declares `{}` and its scope list carries none",
            flow.scope_delimiter,
        );
        assert!(
            !flow.scopes.contains(other),
            "`{provider}` declares `{}` and its scope list also carries `{other}`",
            flow.scope_delimiter,
        );
    }
}

/// Three of the four OAuth connectors deviate from the standard.
///
/// Pinned as a PRODUCT fact, not an implementation detail: it is the reason
/// this workspace carries its own authorize-URL and scope parse rather than
/// the `oauth2` crate, whose delimiter is a hard-coded space in both
/// directions. A future reader wondering why should find the count here.
#[test]
fn only_atlassian_among_the_connectors_follows_the_standard_delimiter() {
    let deviating = Provider::ALL
        .iter()
        .copied()
        .filter_map(|provider| match provider.archetype() {
            Archetype::Oauth2(flow) => Some(flow.scope_delimiter),
            Archetype::AppInstall(_) => None,
        })
        .filter(|delimiter| *delimiter != super::SCOPE_SPACE)
        .count();

    assert_eq!(deviating, 3, "Slack, Zoho and Linear delimit with a comma");
}

/// Slack's token is long-lived; the other three OAuth connectors refresh.
///
/// Pinned because the flag decides whether the credential broker holds a
/// re-mint entry for the provider — the drift `registry.zig` guards with a
/// comptime check against `credentials/integration.zig`.
#[test]
fn only_slack_among_the_oauth_connectors_holds_a_long_lived_token() {
    for provider in Provider::ALL.iter().copied() {
        let Archetype::Oauth2(flow) = provider.archetype() else {
            continue;
        };
        assert_eq!(
            flow.refresh,
            provider != Provider::Slack,
            "`{provider}` refresh flag",
        );
    }
}
