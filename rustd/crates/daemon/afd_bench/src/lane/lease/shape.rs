//! Which population a lease run drives.
//!
//! `PROFILE` already answers WHERE a run points and how hard it may push. The
//! shape answers WHAT it drives there, so the two never share one knob: the
//! default pair (the drain, then the contended window), saturation — the
//! production poll with more requesters than the pool has connections — or a
//! paced burst of steers to fleets whose runners are busy (`M241_001` §5).

use core::fmt;

use af_core::env::EnvSource;

use crate::error::{ErrorKind, Result};
use crate::knobs::{SHAPE_VARIABLE, variable};
use crate::profile::Profile;

/// The spelling of [`Shape::Default`].
const DEFAULT: &str = "default";

/// The spelling of [`Shape::Saturation`].
const SATURATION: &str = "saturation";

/// The spelling of [`Shape::Burst`].
const BURST: &str = "burst";

/// Every spelling, for the refusal that names them.
const EXPECTED: &str = "default, saturation, burst";

/// A lease run's population.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The drain, then the contended window: what every baseline measured.
    Default,
    /// The production poll over chat drafts and fleets, beside live-set
    /// readers, with requesters outnumbering the pool's connections.
    Saturation,
    /// Steers paced at a rate per fleet while runners hold each lease for a
    /// turn, so new messages queue behind running ones.
    Burst,
}

impl Shape {
    /// The shape `BENCH_SHAPE` names, or the default when it is unset.
    ///
    /// # Errors
    ///
    /// `UnknownShape` naming what the variable held.
    pub fn read(env: &dyn EnvSource) -> Result<Self> {
        match variable(env, SHAPE_VARIABLE).as_deref().map(str::trim) {
            None | Some(DEFAULT) => Ok(Self::Default),
            Some(SATURATION) => Ok(Self::Saturation),
            Some(BURST) => Ok(Self::Burst),
            Some(other) => Err(ErrorKind::UnknownShape {
                name: other.to_owned(),
                expected: EXPECTED,
            }
            .into()),
        }
    }

    /// Refuse saturation and the burst anywhere but the compose rig: both
    /// push a pool past what a shared deployment should carry.
    ///
    /// # Errors
    ///
    /// `ShapeNeedsRig` naming the profile that was asked for.
    pub fn admit(self, profile: Profile) -> Result<()> {
        if self != Self::Default && profile != Profile::Rig {
            return Err(ErrorKind::ShapeNeedsRig {
                shape: self.name(),
                profile,
            }
            .into());
        }
        Ok(())
    }

    /// The spelling this shape is selected by.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => DEFAULT,
            Self::Saturation => SATURATION,
            Self::Burst => BURST,
        }
    }
}

impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]
mod tests {
    use af_core::env::MapEnv;

    use super::{BURST, Profile, SATURATION, SHAPE_VARIABLE, Shape};

    fn env_of(pairs: &[(&str, &str)]) -> MapEnv {
        MapEnv::from_pairs(pairs.iter().copied())
    }

    #[test]
    fn test_unset_shape_reads_as_default() {
        assert_eq!(Shape::read(&env_of(&[])).expect("default"), Shape::Default);
    }

    #[test]
    fn test_saturation_is_read_by_name() {
        let env = env_of(&[(SHAPE_VARIABLE, SATURATION)]);
        assert_eq!(Shape::read(&env).expect("reads"), Shape::Saturation);
    }

    #[test]
    fn shape_reads_burst_and_refuses_unknown() {
        let env = env_of(&[(SHAPE_VARIABLE, BURST)]);
        assert_eq!(Shape::read(&env).expect("reads"), Shape::Burst);

        let refused = Shape::read(&env_of(&[(SHAPE_VARIABLE, "bursty")]))
            .expect_err("a misspelt shape is refused");
        assert!(
            refused.to_string().contains("default, saturation, burst"),
            "the refusal names every spelling: {refused}"
        );
        Shape::Burst
            .admit(Profile::Dev)
            .expect_err("a burst never runs against a deployment");
    }

    #[test]
    fn test_misspelt_shape_is_refused_not_defaulted() {
        let refused = Shape::read(&env_of(&[(SHAPE_VARIABLE, "saturate")]))
            .expect_err("a misspelt shape is refused");
        assert!(refused.to_string().contains("saturate"), "{refused}");
        assert!(refused.is_pre_flight());
    }

    #[test]
    fn test_saturation_is_refused_off_the_rig() {
        for profile in [Profile::Dev, Profile::Prod] {
            let refused = Shape::Saturation
                .admit(profile)
                .expect_err("a deployed pool is never saturated on purpose");
            assert!(refused.is_pre_flight(), "{refused}");
        }
        Shape::Saturation
            .admit(Profile::Rig)
            .expect("the rig admits it");
        Shape::Default
            .admit(Profile::Prod)
            .expect("the default shape is unchanged");
    }
}
