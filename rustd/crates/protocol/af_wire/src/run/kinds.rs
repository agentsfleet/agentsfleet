//! A poll explicitly selects the kinds its worker can execute.

use serde::{Deserialize, Serialize};

/// The two independent work domains sharing a physical host's workers.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = RunWorkKind))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    /// Execute an installed fleet's event.
    Fleet,
    /// Answer a setup draft's message with `propose_setup`.
    Setup,
}

impl RunKind {
    /// The durable choice spelling; the database has no second vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fleet => "fleet",
            Self::Setup => "setup",
        }
    }

    /// The other kind, used after a successful mixed placement.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Fleet => Self::Setup,
            Self::Setup => Self::Fleet,
        }
    }
}

/// A nonempty, duplicate-free set parsed once at the polling boundary. Every
/// runner runs fleets; setup is the second kind it may also take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<RunKind>", into = "Vec<RunKind>")]
pub struct AcceptedKinds(u8);

impl AcceptedKinds {
    /// A worker which only executes installed fleets.
    pub const FLEET: Self = Self(1);
    /// A worker which executes fleets and setup.
    pub const ALL: Self = Self(3);

    /// Whether this set permits a particular placement.
    #[must_use]
    pub const fn contains(self, kind: RunKind) -> bool {
        let bit = match kind {
            RunKind::Fleet => 1,
            RunKind::Setup => 2,
        };
        self.0 & bit != 0
    }
}

impl TryFrom<Vec<RunKind>> for AcceptedKinds {
    type Error = &'static str;

    fn try_from(kinds: Vec<RunKind>) -> Result<Self, Self::Error> {
        use RunKind::{Fleet, Setup};
        match kinds.as_slice() {
            [Fleet] => Ok(Self::FLEET),
            [Fleet, Setup] | [Setup, Fleet] => Ok(Self::ALL),
            _ => Err("accepted_kinds must be fleet, or fleet and setup"),
        }
    }
}

impl From<AcceptedKinds> for Vec<RunKind> {
    fn from(kinds: AcceptedKinds) -> Self {
        [RunKind::Fleet, RunKind::Setup]
            .into_iter()
            .filter(|&kind| kinds.contains(kind))
            .collect()
    }
}
