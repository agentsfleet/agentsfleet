//! Which third parties a workspace can connect to, as a closed set.
//!
//! # An enum where the Zig has a table scan
//!
//! `registry.zig` holds a `[_]ConnectorSpec` and answers `lookup(provider)` by
//! walking it with `std.mem.eql`, so every consumer holds a `[]const u8` that
//! MIGHT name a connector. Here the route segment is parsed into a [`Provider`]
//! once, at the edge, and everything inward takes the enum — which is what
//! makes the archetype dispatch a total match rather than a scan that can
//! answer null. `dispatch/write_rust.md`'s "parse, don't validate" is the rule,
//! and the Zig's comptime uniqueness checks become the language's own: two
//! variants cannot share a name.
//!
//! # The ids are a stored-data contract, not a spelling
//!
//! Each id is the `{provider}` route segment, the `provider` column value in
//! `core.connector_installs`, and the stem of two vault keys — `<id>-app` for
//! the platform app credentials and `<id>` for the workspace's own grant. A
//! deployment mid-cutover has both daemons reading the same rows, so these
//! strings are byte-identical to `common`'s `PROVIDER_*` constants and are not
//! this crate's to improve.

use std::fmt;

/// Declares the catalogue once: the variants, [`Provider::ALL`], and the two
/// strings each variant carries.
///
/// A macro because `ALL` was a SECOND hand-written list of the same variants,
/// and the compiler cannot check a list it did not write. A variant left out of
/// it does not fail to build — every total `match` still compiles, the registry
/// arm is there, the vault keys are right — and the only symptom is
/// [`Provider::parse`] answering `None`, so the route 404s while the connector
/// looks correctly wired everywhere a reader would go looking. Enumerating an
/// enum's variants otherwise needs a derive (`strum`, `enum-iterator`) that
/// this workspace does not carry, and one list expanded three ways is cheaper
/// than the dependency.
///
/// The `id` is the route segment, the `provider` column value in
/// `core.connector_installs`, and the stem of two vault keys — see the module
/// note on why these are not this crate's to improve.
macro_rules! catalogue {
    ($(
        $(#[doc = $doc:literal])+
        $variant:ident => id: $id:literal, display: $display:literal;
    )+) => {
        /// A third party this deployment can connect a workspace to.
        ///
        /// Five, and the count is deliberate rather than incidental: api-key
        /// providers (Datadog, Grafana, Fly) are workspace secrets referenced
        /// as `${secrets.<name>.<field>}` and were never connectors —
        /// `registry.zig` records the same decision where it dropped its
        /// `api_key` archetype.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Provider {
            $($(#[doc = $doc])+ $variant,)+
        }

        impl Provider {
            /// Every provider this deployment ships, in the order the
            /// catalogue lists — the same list the variants come from, so it
            /// cannot fall behind them.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// The route segment, column value and vault-key stem — see the
            /// module note.
            #[must_use]
            pub const fn id(self) -> &'static str {
                match self { $(Self::$variant => $id,)+ }
            }

            /// The name an operator-facing sentence calls this provider.
            ///
            /// "Slack connect is not configured on this deployment" — the
            /// display name is what makes that sentence name the thing the
            /// operator has to go and configure, rather than saying "a
            /// connector".
            #[must_use]
            pub const fn display_name(self) -> &'static str {
                match self { $(Self::$variant => $display,)+ }
            }
        }
    };
}

catalogue! {
    /// Slack, whose bot token answers mentions in a channel.
    Slack => id: "slack", display: "Slack";
    /// GitHub, connected as an App INSTALLATION rather than a token exchange.
    GitHub => id: "github", display: "GitHub";
    /// Zoho Desk, whose authorization code is redeemable at one data centre.
    Zoho => id: "zoho", display: "Zoho Desk";
    /// Jira Cloud, over Atlassian's three-legged OAuth.
    Jira => id: "jira", display: "Jira";
    /// Linear.
    Linear => id: "linear", display: "Linear";
}

/// What a `<provider>-app` vault key ends in.
///
/// The one site that spells it (RULE UFS), the way `oauth2.zig`'s
/// `APP_VAULT_KEY_SUFFIX` is the one site on the other daemon.
const APP_KEY_SUFFIX: &str = "-app";

impl Provider {
    /// The provider a route segment names, or nothing.
    ///
    /// Exact and lower-case: `SLACK` resolves to nothing, because the segment
    /// is also a column value and a vault-key stem, and a case-insensitive
    /// parse here would let one workspace store a grant the other daemon
    /// cannot find.
    #[must_use]
    pub fn parse(segment: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|provider| provider.id() == segment)
    }

    /// The admin-workspace vault key holding this provider's platform app.
    ///
    /// One OAuth app per connector serving every tenant, which is why the key
    /// is read from the deployment's admin workspace rather than from the
    /// workspace doing the connecting.
    #[must_use]
    pub fn app_key(self) -> String {
        format!("{}{APP_KEY_SUFFIX}", self.id())
    }

    /// The workspace vault key this provider's grant is sealed under.
    ///
    /// The bare id: `crypto_store.zig` stores the connector handle under the
    /// provider name, and the runner plane's `afd_credential::vault` opens it
    /// by that name when a fleet declares the integration.
    #[must_use]
    pub const fn grant_key(self) -> &'static str {
        self.id()
    }
}

/// Renders the id, which is what every log line and error sentence names.
impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::{APP_KEY_SUFFIX, Provider};

    /// Every id round-trips through the parse it is matched by.
    ///
    /// The property `registry.zig` gets from a comptime duplicate scan: no two
    /// entries answer one segment. Here the enum gives it, and this pins that
    /// the parse actually reads the same string the id renders.
    #[test]
    fn every_provider_parses_from_the_id_it_renders() {
        for provider in Provider::ALL.iter().copied() {
            assert_eq!(
                Provider::parse(provider.id()),
                Some(provider),
                "`{provider}` must parse from its own id",
            );
        }
    }

    /// Every provider the catalogue lists is reachable through it.
    ///
    /// The property the `catalogue!` macro exists for, stated where a reader
    /// looking for it will find it: `ALL` and the variants are one list, so
    /// there is no sixth provider sitting outside this loop. Before the macro
    /// they were two hand-written lists, and a variant missing from `ALL`
    /// parsed as nothing — a 404 on a route whose registry arm, vault keys and
    /// display name all looked correct.
    #[test]
    fn the_catalogue_reaches_every_provider_it_declares() {
        for provider in Provider::ALL.iter().copied() {
            assert_eq!(Provider::parse(provider.id()), Some(provider));
            assert!(
                !provider.display_name().is_empty(),
                "`{provider}` must name itself in an operator's sentence",
            );
        }
    }

    /// No two providers answer to one display name.
    ///
    /// The operator-facing half of the id uniqueness the parse test pins: two
    /// connectors called "Jira" would make "Jira connect is not configured"
    /// name neither of them.
    #[test]
    fn no_two_providers_share_a_display_name() {
        for (index, provider) in Provider::ALL.iter().copied().enumerate() {
            for other in Provider::ALL.iter().copied().skip(index + 1) {
                assert_ne!(
                    provider.display_name(),
                    other.display_name(),
                    "`{provider}` and `{other}` share a display name",
                );
            }
        }
    }

    /// The catalogue is exactly the five, and a sixth updates this pin.
    ///
    /// `registry.zig` carries the same test for the same reason: the registry
    /// IS the provider catalogue, so its size is a product fact rather than an
    /// implementation detail.
    #[test]
    fn the_catalogue_is_the_five_shipped_connectors() {
        assert_eq!(Provider::ALL.len(), 5);
    }

    /// A segment nothing ships resolves to nothing, and so does the empty one.
    ///
    /// Both are the 404 path. The upper-case case is the one worth pinning:
    /// the id is also a stored column value, so a case-insensitive parse would
    /// let a grant be written under a spelling no reader looks for.
    #[test]
    fn an_unknown_or_miscased_segment_names_no_provider() {
        for segment in ["", "nope", "SLACK", "Slack", " slack"] {
            assert_eq!(Provider::parse(segment), None, "`{segment}` names nothing");
        }
    }

    /// The two vault keys a provider owns are distinct and derived from its id.
    ///
    /// They must not collide: the app key holds the DEPLOYMENT's client secret
    /// in the admin workspace, and the grant key holds a TENANT's token in
    /// theirs. One spelling for both would put a tenant's grant where the
    /// platform credentials are read from.
    #[test]
    fn the_app_key_and_the_grant_key_are_not_the_same_name() {
        for provider in Provider::ALL.iter().copied() {
            let app = provider.app_key();
            assert_eq!(app, format!("{provider}{APP_KEY_SUFFIX}"));
            assert_ne!(app, provider.grant_key());
        }
    }
}
