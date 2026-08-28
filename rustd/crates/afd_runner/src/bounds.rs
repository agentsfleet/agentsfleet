//! The bounds on what a host asserts about itself.
//!
//! A runner token authenticates a machine that reports two things nobody can
//! check from here: what its kernel can enforce, and how its own probe went.
//! Both are stored, so both are a persistence-amplification channel if they are
//! unbounded — a mebibyte of JSONB the runner page then re-reads on every load.
//! Ported from `src/lib/contract/protocol_selftest.zig` and
//! `protocol_policy.zig`'s `capabilityReportBounded`, which draw these caps
//! from the probe's fixed vocabulary rather than from a guess.
//!
//! # Both refusals are lenient, and that is deliberate
//!
//! Neither bound fails the beat. A malformed report reads as "nothing reported
//! this beat", the stored value keeps reconciling, and the host is told its
//! liveness landed — because a runner token must not be able to fail a liveness
//! beat by sending nonsense, and a host that cannot beat is a host the fleet
//! reads as dead.
//!
//! # Why the answer is a `Result` and not a three-state enum
//!
//! The Zig returns `Rejection{ none, unbounded, all_ok_disagrees }`, so every
//! caller writes a `switch` whose first arm means "carry on". That arm is the
//! success path wearing an error's clothes, and a caller that forgets it is a
//! caller that stores nothing. Here acceptance is `Ok(())` and the two refusals
//! are the `Err`, so `?` carries them and the success path has no arm at all
//! (`dispatch/write_rust.md` §Functional design).
//!
//! The distinction between the two refusals is kept for the reason the Zig
//! keeps it: the refusing side logs WHICH, and they are different operator
//! problems — a bound is a runner sending too much, a disagreement is a runner
//! claiming health its own checks contradict.

use afd_wire::runner::{CapabilityReport, SelftestReport};

/// Most checks one verdict may carry.
///
/// Sized to the probe's vocabulary plus room for a per-operator-bind check
/// each, not to a guess.
pub const MAX_SELFTEST_CHECKS: usize = 32;

/// Longest a check's name may be.
pub const MAX_CHECK_NAME_LEN: usize = 128;

/// Longest a check's prose cause may be.
pub const MAX_CHECK_DETAIL_LEN: usize = 256;

/// Longest the policy strings travelling with a verdict may be.
pub const MAX_SELFTEST_POLICY_LEN: usize = 64;

/// Why a reported verdict was refused.
///
/// Carries no fragment of what was refused: a runner token can put arbitrary
/// bytes in a check name, and a refusal that quotes them is a refusal that logs
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// Past a cap, or carrying an empty name, detail or policy string.
    #[error("the reported verdict exceeds its bounds")]
    Unbounded,
    /// `all_ok` contradicts the checks it arrived with.
    #[error("all_ok disagrees with the reported checks")]
    AllOkDisagrees,
}

/// Whether `report` may be stored.
///
/// Total-report shaped: the whole verdict is refused on one bad entry, because
/// a partially stored self-test is a verdict nobody reasoned about.
///
/// # Errors
/// [`Rejection::Unbounded`] when any cap is exceeded or any required string is
/// empty; [`Rejection::AllOkDisagrees`] when the summary contradicts the
/// checks. That second one is the incident this exists for: a host reading
/// healthy while every lease dies inside its sandbox.
pub fn accept(report: &SelftestReport<'_>) -> Result<(), Rejection> {
    if report.checks.len() > MAX_SELFTEST_CHECKS
        || !bounded(&report.sandbox_tier, MAX_SELFTEST_POLICY_LEN)
        || !bounded(&report.network_policy, MAX_SELFTEST_POLICY_LEN)
        || !report.checks.iter().all(|check| {
            bounded(&check.name, MAX_CHECK_NAME_LEN)
                // `detail` may be long-ish prose but never empty: an empty
                // cause reads to the dashboard as a leaked internal identifier
                // and is hidden, so the check would arrive explanation-less.
                && bounded(&check.detail, MAX_CHECK_DETAIL_LEN)
        })
    {
        return Err(Rejection::Unbounded);
    }

    // Reported by the host rather than derived on arrival, so without this a
    // runner could claim health its own checks contradict.
    let every_check_passed = report.checks.iter().all(|check| check.ok);
    if report.all_ok == every_check_passed {
        Ok(())
    } else {
        Err(Rejection::AllOkDisagrees)
    }
}

/// Whether a required string is present and within `ceiling` bytes.
fn bounded(value: &str, ceiling: usize) -> bool {
    !value.is_empty() && value.len() <= ceiling
}

/// Most cgroup controllers one capability report may name.
///
/// `protocol_policy.zig`'s `MAX_REPORT_CONTROLLERS`.
pub const MAX_REPORT_CONTROLLERS: usize = 16;

/// Longest a controller name may be.
///
/// `protocol_policy.zig`'s `MAX_CONTROLLER_NAME_LEN`.
pub const MAX_CONTROLLER_NAME_LEN: usize = 64;

/// Whether a reported capability set may be stored.
///
/// A `bool` rather than a `Result`, unlike [`accept`]: there is exactly one way
/// to be out of bounds here and nothing downstream distinguishes them, so a
/// variant would be a name for a distinction nobody makes
/// (`M-SIMPLE-ABSTRACTIONS`). The self-test verdict earns its enum because its
/// two refusals are different operator problems.
#[must_use]
pub fn capability_within_bounds(report: &CapabilityReport<'_>) -> bool {
    report.cgroup_controllers.len() <= MAX_REPORT_CONTROLLERS
        && report
            .cgroup_controllers
            .iter()
            .all(|controller| bounded(controller, MAX_CONTROLLER_NAME_LEN))
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use afd_wire::runner::SelftestCheck;

    use super::*;

    /// A check that passed, with the prose every check carries.
    fn passing() -> SelftestCheck<'static> {
        SelftestCheck {
            name: Cow::Borrowed("a hostname resolves inside the sandbox"),
            ok: true,
            detail: Cow::Borrowed("no fault detected"),
        }
    }

    /// A check that failed, with its cause.
    fn failing() -> SelftestCheck<'static> {
        SelftestCheck {
            name: Cow::Borrowed("a hostname resolves inside the sandbox"),
            ok: false,
            detail: Cow::Borrowed("the resolver did not answer"),
        }
    }

    /// A verdict over `checks`, summarised as `all_ok`.
    fn report(checks: Vec<SelftestCheck<'static>>, all_ok: bool) -> SelftestReport<'static> {
        SelftestReport {
            checks,
            all_ok,
            sandbox_tier: Cow::Borrowed("landlock_full"),
            network_policy: Cow::Borrowed("allow_all"),
        }
    }

    /// A well-formed verdict is stored.
    #[test]
    fn test_selftest_report_bounds_accept_a_well_formed_verdict() {
        assert_eq!(accept(&report(vec![passing()], true)), Ok(()));
        assert_eq!(accept(&report(vec![passing(), failing()], false)), Ok(()));
        // No checks at all is vacuously all-ok, and is what a probe that ran
        // nothing reports.
        assert_eq!(accept(&report(Vec::new(), true)), Ok(()));
    }

    /// Every bound refuses the whole verdict.
    #[test]
    fn test_selftest_report_bounds_reject_a_malformed_verdict() {
        let unnamed = SelftestCheck {
            name: Cow::Borrowed(""),
            ..passing()
        };
        let unexplained = SelftestCheck {
            detail: Cow::Borrowed(""),
            ..passing()
        };
        let long_name = SelftestCheck {
            name: Cow::Owned("n".repeat(MAX_CHECK_NAME_LEN + 1)),
            ..passing()
        };
        let long_detail = SelftestCheck {
            detail: Cow::Owned("d".repeat(MAX_CHECK_DETAIL_LEN + 1)),
            ..passing()
        };

        for refused in [
            report(vec![unnamed], true),
            report(vec![unexplained], true),
            report(vec![long_name], true),
            report(vec![long_detail], true),
            report(vec![passing(); MAX_SELFTEST_CHECKS + 1], true),
            // An empty policy string cannot be compared against the row for
            // staleness, so a verdict carrying one says nothing.
            SelftestReport {
                sandbox_tier: Cow::Borrowed(""),
                ..report(vec![passing()], true)
            },
            SelftestReport {
                network_policy: Cow::Borrowed(""),
                ..report(vec![passing()], true)
            },
            SelftestReport {
                sandbox_tier: Cow::Owned("t".repeat(MAX_SELFTEST_POLICY_LEN + 1)),
                ..report(vec![passing()], true)
            },
        ] {
            assert_eq!(accept(&refused), Err(Rejection::Unbounded));
        }
    }

    /// A controller set within its caps is stored.
    #[test]
    fn test_capability_report_within_bounds_is_accepted() {
        let report = CapabilityReport {
            landlock: true,
            seccomp: true,
            cgroup_controllers: vec![Cow::Borrowed("cpu"), Cow::Borrowed("memory")],
            bubblewrap: true,
            egress_enforcement: true,
        };

        assert!(capability_within_bounds(&report));
    }

    /// Too many controllers, an unnamed one, or an overlong name all refuse.
    #[test]
    fn test_capability_report_bounds_reject_an_amplifying_report() {
        let with = |controllers: Vec<Cow<'static, str>>| CapabilityReport {
            landlock: false,
            seccomp: false,
            cgroup_controllers: controllers,
            bubblewrap: false,
            egress_enforcement: false,
        };

        assert!(!capability_within_bounds(&with(vec![
            Cow::Borrowed("cpu");
            MAX_REPORT_CONTROLLERS
                + 1
        ])));
        assert!(!capability_within_bounds(&with(vec![Cow::Borrowed("")])));
        assert!(!capability_within_bounds(&with(vec![Cow::Owned(
            "c".repeat(MAX_CONTROLLER_NAME_LEN + 1)
        )])));
    }

    /// The summary must agree with the checks it arrived with, both ways.
    #[test]
    fn test_all_ok_must_agree_with_the_checks_it_arrived_with() {
        // A runner claiming health its own checks contradict — the shape that
        // lets a broken host keep reading healthy, which is the whole incident.
        assert_eq!(
            accept(&report(vec![passing(), failing()], true)),
            Err(Rejection::AllOkDisagrees)
        );
        // And the mirror: claiming failure when every check passed.
        assert_eq!(
            accept(&report(vec![passing()], false)),
            Err(Rejection::AllOkDisagrees)
        );
    }
}
