//! Each trial is one kernel-tier row of the sandbox's proof.

use std::sync::Arc;

use afr_engine::Engine as _;
use afr_engine::conformance::{self, Context, FarHost};
use afr_firewall::far::{FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_UNLISTED, Far};
use libtest_mimic::{Arguments, Conclusion, Failed, Trial};

use crate::git::LaneCheckout;
use crate::lane::{Lane, TOOLBOX_VARIABLE, missing};
use crate::run::{expect, runtime};
use crate::verify::{adoption, path_swap};

/// The kind the lane reports a conformance trial under.
const CONFORMANCE: &str = "conformance";
/// The kind of a trial only Firecracker answers.
const FIRECRACKER: &str = "firecracker";
/// The kind of a trial on the toolbox or the lane itself, whichever engine
/// runs.
const LANE: &str = "lane";
/// The name the far host answers to inside a sandbox.
const FAR_NAME: &str = "far.test";
/// The far host the verdict matrix dials.
const FAR: FarHost = FarHost {
    name: FAR_NAME,
    listed: FAR_LISTED,
    unlisted: FAR_UNLISTED,
    port: FAR_PORT,
    greeting: FAR_GREETING,
};

pub(crate) type Body = fn(&Lane) -> Result<(), Failed>;

/// Every trial on the toolbox or the lane itself, by the name the lane
/// reports it under.
const TRIALS: &[(&str, Body)] = &[
    ("test_toolbox_verify_survives_path_swap", path_swap),
    ("test_toolbox_adoption_checks_identity", adoption),
    ("test_kernel_lane_refuses_to_skip", refuses_to_skip),
];

/// Runs the conformance suite against Firecracker, then the trials only it
/// answers, then the toolbox's and the lane's own, one at a time.
pub(crate) fn run(arguments: &Arguments, lane: &Arc<Lane>) -> Conclusion {
    let suite = conformance::TRIALS.iter().map(|trial| {
        let lane = Arc::clone(lane);
        Trial::test(trial.name(), move || conformed(&lane, trial)).with_kind(CONFORMANCE)
    });
    let microvms = crate::firecracker::TRIALS
        .iter()
        .chain(crate::microvm_pool::TRIALS)
        .map(|trial| (FIRECRACKER, trial));
    let own = TRIALS
        .iter()
        .chain(crate::toolbox_trials::TRIALS)
        .map(|trial| (LANE, trial));
    let own = microvms.chain(own).map(|(kind, &(name, body))| {
        let lane = Arc::clone(lane);
        Trial::test(name, move || body(&lane)).with_kind(kind)
    });
    libtest_mimic::run(arguments, suite.chain(own).collect())
}

/// Holds the lane's engine to `trial`, with the far host answering and the
/// supervisor's checkout serving a fixture repository.
fn conformed(lane: &Lane, trial: &conformance::Trial) -> Result<(), Failed> {
    let _far = Far::start()?;
    let checkout = LaneCheckout::serve()?;
    let context = Context {
        far: FAR,
        checkout: &checkout,
    };
    let engine = lane.microvms()?;
    runtime().block_on(trial.run(&engine, &context))
}

/// Prints what may differ on the lane's engine, before any trial runs.
pub(crate) fn differences(lane: &Lane) -> Result<(), Failed> {
    let engine = lane.microvms()?;
    let differences = runtime().block_on(conformance::differences(&engine))?;
    println!(
        "{}: {differences}",
        <&str>::from(engine.capabilities().engine)
    );
    Ok(())
}

/// A host that is not root and names no toolbox is refused with each gap
/// named, rather than skipped.
fn refuses_to_skip(_lane: &Lane) -> Result<(), Failed> {
    let gaps = missing(None, false);
    expect(
        gaps.iter().any(|gap| gap.starts_with("root"))
            && gaps.iter().any(|gap| gap.starts_with(TOOLBOX_VARIABLE)),
        format!("root and the toolbox are each named, got {gaps:?}"),
    )
}
