//! A fleet's HTTP destination scope, independent of sandbox networking.

use serde::{Deserialize, Serialize};

/// Which public HTTPS destinations a fleet's HTTP tools may reach: the
/// stored `fleet.fleets.http_egress`, spelled as that column holds it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpEgress {
    /// Only the hosts of the fleet's approved secrets, its approved GitHub
    /// grant and, when the fleet allows them, the package registries. A new
    /// fleet starts here.
    #[default]
    Restricted,
    /// Public destinations, while retaining address, credential and request
    /// restrictions.
    Public,
}

impl HttpEgress {
    /// Every stored spelling, in declaration order.
    pub const ALL: [Self; 2] = [Self::Restricted, Self::Public];

    /// The spelling `fleet.fleets.http_egress` stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Restricted => "restricted",
            Self::Public => "public",
        }
    }

    /// The scope a stored spelling names; `None` for any other text.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.as_str() == stored)
    }
}

#[cfg(test)]
mod tests {
    use super::HttpEgress;

    #[test]
    fn a_stored_spelling_reads_back_as_its_scope() {
        for scope in HttpEgress::ALL {
            assert_eq!(HttpEgress::parse(scope.as_str()), Some(scope));
            assert_eq!(
                serde_json::to_value(scope).ok(),
                Some(serde_json::Value::from(scope.as_str())),
                "serde and the stored spelling agree"
            );
        }
    }

    #[test]
    fn an_unknown_or_cased_spelling_is_no_scope() {
        for stored in [
            "",
            "Public",
            "RESTRICTED",
            "private",
            "public ",
            "allow_all",
        ] {
            assert_eq!(HttpEgress::parse(stored), None, "{stored:?}");
        }
    }

    #[test]
    fn a_new_fleet_starts_restricted() {
        assert_eq!(HttpEgress::default(), HttpEgress::Restricted);
    }
}
