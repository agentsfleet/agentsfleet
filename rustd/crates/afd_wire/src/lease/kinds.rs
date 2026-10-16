//! A poll explicitly selects the kinds its worker can execute.

use serde::{Deserialize, Serialize};

/// The two independent work domains sharing a physical host's workers.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = LeaseWorkKind))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseKind {
    /// Execute an installed fleet's event.
    Fleet,
    /// Produce a private setup proposal through scoped daemon model access.
    Authoring,
}

impl LeaseKind {
    /// The durable choice spelling; the database has no second vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fleet => "fleet",
            Self::Authoring => "authoring",
        }
    }

    /// The other kind, used after a successful mixed placement.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Fleet => Self::Authoring,
            Self::Authoring => Self::Fleet,
        }
    }
}

/// A nonempty, duplicate-free set parsed once at the polling boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<LeaseKind>", into = "Vec<LeaseKind>")]
pub struct AcceptedKinds(u8);

impl AcceptedKinds {
    /// A worker which only executes installed fleets.
    pub const FLEET: Self = Self(1);
    /// A worker which only executes private authoring.
    pub const AUTHORING: Self = Self(2);
    /// A worker implementing both protocols.
    pub const ALL: Self = Self(3);

    /// Whether this set permits a particular placement.
    #[must_use]
    pub const fn contains(self, kind: LeaseKind) -> bool {
        let bit = match kind {
            LeaseKind::Fleet => Self::FLEET.0,
            LeaseKind::Authoring => Self::AUTHORING.0,
        };
        self.0 & bit != 0
    }
}

impl TryFrom<Vec<LeaseKind>> for AcceptedKinds {
    type Error = &'static str;

    fn try_from(kinds: Vec<LeaseKind>) -> Result<Self, Self::Error> {
        use LeaseKind::{Authoring, Fleet};
        match kinds.as_slice() {
            [Fleet] => Ok(Self::FLEET),
            [Authoring] => Ok(Self::AUTHORING),
            [Fleet, Authoring] | [Authoring, Fleet] => Ok(Self::ALL),
            _ => Err("accepted_kinds must contain one or two distinct supported kinds"),
        }
    }
}

impl From<AcceptedKinds> for Vec<LeaseKind> {
    fn from(kinds: AcceptedKinds) -> Self {
        [LeaseKind::Fleet, LeaseKind::Authoring]
            .into_iter()
            .filter(|&kind| kinds.contains(kind))
            .collect()
    }
}
