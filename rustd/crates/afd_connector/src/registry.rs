//! What each provider's connect flow IS: its archetype, its endpoints, and the
//! domain its install state is signed in.
//!
//! # Two archetypes, and a new provider is data rather than code
//!
//! `registry.zig`'s whole design claim, and it is kept: adding a connector is
//! one arm in each match below plus a parse hook, never a new route or a new
//! flow. What changes is where the totality comes from — the Zig proves its
//! table's invariants in a `comptime` block that raises `@compileError`, and
//! here the same facts are the language's: a [`Provider`] variant with no arm
//! does not compile, and two variants cannot share a name.
//!
//! # The state domains must not collide, and that is checked
//!
//! A state's HMAC is domain-separated by [`StateBinding::domain_prefix`] and
//! its single-use nonce namespaced by [`StateBinding::nonce_prefix`]. Two
//! providers sharing either would let one connector's signed state verify — and
//! be consumed — under another's callback. The Zig enforces it at comptime over
//! its table; the suite at the foot of this file enforces it over the enum,
//! which is the same guarantee one build stage later.

use crate::provider::Provider;

/// The authorize parameter asking a provider to re-show its consent screen.
///
/// Two connectors send it, which is why it is named rather than spelled twice:
/// without it Atlassian and Zoho silently reuse an existing authorization, and
/// a person reconnecting after revoking access gets the revoked grant back.
const PARAM_PROMPT: &str = "prompt";

/// See [`PARAM_PROMPT`]. The value both connectors send.
const PROMPT_CONSENT: &str = "consent";

/// How long a connect round-trip may take before its state stops verifying.
///
/// `state.zig`'s `DEFAULT_TTL_SECONDS`. Ten minutes is a browser journey
/// through a provider's consent screen with room for a person to read it, and
/// it bounds how long a leaked state is worth anything.
pub const STATE_TTL_SECONDS: u32 = 600;

/// The domain one provider's install state is signed and remembered in.
///
/// The two prefixes travel together because they answer one question — whose
/// state is this — and a signature checked in one domain against a nonce
/// consumed in another would be two providers sharing a single-use slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateBinding {
    /// What the HMAC is domain-separated by, so one connector's state cannot
    /// cross-verify as another's.
    pub domain_prefix: &'static str,
    /// What the single-use nonce key is namespaced by.
    pub nonce_prefix: &'static str,
}

/// An OAuth 2.0 authorization-code connector's endpoints and scopes.
///
/// Data, not behaviour: [`crate::oauth`] composes the authorize URL and the
/// exchange form from one of these, and knows nothing about which provider it
/// holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Oauth2Flow {
    /// Where the browser is sent to consent.
    pub authorize_endpoint: &'static str,
    /// Where the authorization code is redeemed.
    ///
    /// The single-region answer. Zoho overrides it per callback — see
    /// [`crate::zoho`] on why the code is only redeemable at the data centre
    /// that issued it.
    pub token_endpoint: &'static str,
    /// What access is asked for, in the provider's own delimiter.
    pub scopes: &'static str,
    /// The provider-specific authorize parameters beyond the shared five.
    ///
    /// Pairs rather than a pre-encoded tail, which is where this departs from
    /// `oauth2.zig`'s `authorize_extra_query`: a raw string has to be
    /// concatenated past the encoder, so it is the one part of the URL nothing
    /// checks. As pairs they go through the same `append_pair` as everything
    /// else and cannot carry an unescaped `&` into the query.
    pub extra_query: &'static [(&'static str, &'static str)],
    /// Whether the vendor issues a refresh token the broker re-mints from.
    pub refresh: bool,
}

/// A GitHub-App-shaped connector: an INSTALLATION, not a token exchange.
///
/// The user-authorization leg proves the person can reach the installation
/// before the callback writes its handle, which is why the two are not one
/// flow with a flag — see `github/callback.zig`'s ownership check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppInstall {
    /// Where the browser is sent to authorize the person.
    pub authorize_endpoint: &'static str,
    /// Where that authorization code is redeemed for a user token.
    pub token_endpoint: &'static str,
}

/// Which flow a provider runs.
///
/// A closed enum so every consumer's `match` is exhaustive: a third archetype
/// cannot land half-wired, because every site that dispatches on this one fails
/// to compile until it says what the new one does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archetype {
    /// Consent, then a code redeemed for a token.
    Oauth2(Oauth2Flow),
    /// An App installed on an account, proven by a user authorization.
    AppInstall(AppInstall),
}

impl Provider {
    /// The flow this provider runs.
    #[must_use]
    pub const fn archetype(self) -> Archetype {
        match self {
            Self::Slack => Archetype::Oauth2(Oauth2Flow {
                authorize_endpoint: "https://slack.com/oauth/v2/authorize",
                token_endpoint: "https://slack.com/api/oauth.v2.access",
                // Receive mentions, post replies, and read the recent thread on
                // a mention. Whole-channel history is deliberately absent.
                scopes: "app_mentions:read,chat:write,channels:history",
                extra_query: &[],
                // A Slack bot token is long-lived, so there is nothing to
                // re-mint from and no refresh entry for the broker to hold.
                refresh: false,
            }),
            Self::GitHub => Archetype::AppInstall(AppInstall {
                // A GitHub App uses its own permissions rather than OAuth
                // scopes, which is why this carries none.
                authorize_endpoint: "https://github.com/login/oauth/authorize",
                token_endpoint: "https://github.com/login/oauth/access_token",
            }),
            Self::Zoho => Archetype::Oauth2(Oauth2Flow {
                authorize_endpoint: "https://accounts.zoho.com/oauth/v2/auth",
                token_endpoint: crate::zoho::US_TOKEN_ENDPOINT,
                scopes: "Desk.organization.READ,Desk.basic.READ",
                extra_query: &[("access_type", "offline"), (PARAM_PROMPT, PROMPT_CONSENT)],
                refresh: true,
            }),
            Self::Jira => Archetype::Oauth2(Oauth2Flow {
                authorize_endpoint: "https://auth.atlassian.com/authorize",
                token_endpoint: "https://auth.atlassian.com/oauth/token",
                // Space-delimited, which is Atlassian's spelling rather than a
                // mistake: the exchange form percent-encodes whichever the
                // provider uses, so the delimiter is provider data.
                scopes: "read:jira-work read:jira-user write:jira-work \
                         read:servicedesk-request write:servicedesk-request offline_access",
                extra_query: &[
                    ("audience", "api.atlassian.com"),
                    (PARAM_PROMPT, PROMPT_CONSENT),
                ],
                refresh: true,
            }),
            Self::Linear => Archetype::Oauth2(Oauth2Flow {
                authorize_endpoint: "https://linear.app/oauth/authorize",
                token_endpoint: "https://api.linear.app/oauth/token",
                scopes: "read,comments:create",
                extra_query: &[],
                refresh: true,
            }),
        }
    }

    /// The domain this provider's install state is signed and remembered in.
    ///
    /// Byte-identical to the Zig's prefixes: a connect started on one daemon
    /// and completed on the other during a cutover must verify, and the domain
    /// is what the signature binds.
    #[must_use]
    pub const fn state_binding(self) -> StateBinding {
        match self {
            Self::Slack => StateBinding {
                domain_prefix: "slackconnect:v1:",
                nonce_prefix: "connect:slack:nonce:",
            },
            Self::GitHub => StateBinding {
                domain_prefix: "ghconnect:v1:",
                nonce_prefix: "connect:gh:nonce:",
            },
            Self::Zoho => StateBinding {
                domain_prefix: "zoho:v1:",
                nonce_prefix: "connect:zoho:nonce:",
            },
            Self::Jira => StateBinding {
                domain_prefix: "jira:v1:",
                nonce_prefix: "connect:jira:nonce:",
            },
            Self::Linear => StateBinding {
                domain_prefix: "linear:v1:",
                nonce_prefix: "connect:linear:nonce:",
            },
        }
    }
}

#[cfg(test)]
mod tests {
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
}
