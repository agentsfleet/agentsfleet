//! Zoho's multi-data-centre resolution: which accounts server issued the code.
//!
//! Zoho's authorize step always starts at the US accounts server whatever
//! region the person is in, and the callback appends `location` naming the data
//! centre that actually issued the `code`. That code is redeemable ONLY at that
//! centre's accounts server — exchanging or later refreshing at the wrong one
//! fails `invalid_grant` — so the token endpoint is a per-callback fact rather
//! than a per-provider one.
//!
//! Every centre's accounts server is `accounts.zoho.<tld>` except Canada, whose
//! server lives on a different apex domain entirely. That is Zoho's own
//! irregularity, documented in their multi-DC table, not a suffix guessed here.

/// The path every accounts server serves the token exchange on.
const TOKEN_PATH: &str = "/oauth/v2/token";

/// The default token endpoint, for a callback that named no data centre.
///
/// Named here rather than in [`crate::registry`] so the US answer has one
/// spelling across the default and the resolver (RULE UFS).
pub const US_TOKEN_ENDPOINT: &str = "https://accounts.zoho.com/oauth/v2/token";

/// One of Zoho's data centres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataCentre {
    /// The default, and the answer for a `location` this build does not know.
    UnitedStates,
    Europe,
    India,
    Australia,
    China,
    Japan,
    Canada,
}

impl DataCentre {
    /// The centre a callback's `location` names.
    ///
    /// An absent or unrecognised value is the United States, which is the
    /// fail-safe answer rather than a guess: it is where every authorize starts,
    /// so it is the centre a single-region tenant's code came from, and a wrong
    /// guess costs a refused exchange rather than a misplaced token.
    fn named(location: Option<&str>) -> Self {
        match location {
            Some("eu") => Self::Europe,
            Some("in") => Self::India,
            Some("au") => Self::Australia,
            Some("cn") => Self::China,
            Some("jp") => Self::Japan,
            Some("ca") => Self::Canada,
            _ => Self::UnitedStates,
        }
    }

    /// This centre's accounts server, which is also where a refresh is minted.
    const fn accounts_base(self) -> &'static str {
        match self {
            Self::UnitedStates => "https://accounts.zoho.com",
            Self::Europe => "https://accounts.zoho.eu",
            Self::India => "https://accounts.zoho.in",
            Self::Australia => "https://accounts.zoho.com.au",
            Self::China => "https://accounts.zoho.com.cn",
            Self::Japan => "https://accounts.zoho.jp",
            // The one irregular entry — a different apex domain, not a `.ca`
            // suffix under `zoho`. See the module note.
            Self::Canada => "https://accounts.zohocloud.ca",
        }
    }
}

/// The accounts server a callback's `location` names.
///
/// Persisted on the grant so a later refresh mints at the same centre the
/// original code was issued by.
#[must_use]
pub fn accounts_base(location: Option<&str>) -> &'static str {
    DataCentre::named(location).accounts_base()
}

/// The token endpoint a callback's `location` names.
#[must_use]
pub fn token_endpoint(location: Option<&str>) -> String {
    format!("{}{TOKEN_PATH}", accounts_base(location))
}

#[cfg(test)]
mod tests {
    use super::{US_TOKEN_ENDPOINT, accounts_base, token_endpoint};

    /// Each documented centre resolves to its own accounts server.
    #[test]
    fn every_named_data_centre_resolves_to_its_own_accounts_server() {
        let expected = [
            ("us", "https://accounts.zoho.com"),
            ("eu", "https://accounts.zoho.eu"),
            ("in", "https://accounts.zoho.in"),
            ("au", "https://accounts.zoho.com.au"),
            ("cn", "https://accounts.zoho.com.cn"),
            ("jp", "https://accounts.zoho.jp"),
        ];
        for (location, base) in expected {
            assert_eq!(accounts_base(Some(location)), base, "`{location}`");
        }
    }

    /// Canada is the irregular one — a different apex domain, not `zoho.ca`.
    ///
    /// Pinned on its own because it is the entry a reader would "correct" into
    /// the pattern its six neighbours follow, and the correction would send
    /// every Canadian exchange to a host that refuses it.
    #[test]
    fn canada_lives_on_zohocloud_rather_than_under_the_zoho_apex() {
        assert_eq!(accounts_base(Some("ca")), "https://accounts.zohocloud.ca");
    }

    /// An absent or unrecognised location is the United States.
    #[test]
    fn an_unknown_or_absent_location_falls_back_to_the_united_states() {
        for location in [None, Some(""), Some("mars"), Some("EU")] {
            assert_eq!(accounts_base(location), "https://accounts.zoho.com");
        }
    }

    /// The endpoint is the base with the one token path appended.
    ///
    /// And the default the registry carries is the same string this resolver
    /// answers for a callback that named nothing — one spelling, two readers.
    #[test]
    fn the_token_endpoint_is_the_accounts_base_and_the_one_path() {
        assert_eq!(token_endpoint(None), US_TOKEN_ENDPOINT);
        assert_eq!(
            token_endpoint(Some("ca")),
            "https://accounts.zohocloud.ca/oauth/v2/token",
        );
    }
}
