//! A running guest whose allowlisted name now resolves to other addresses
//! takes them in place, on a real kernel, with no microVM rebooted: the name
//! follows the set, an address the set left is closed, and one both sets hold
//! stays open through the refill.

use afr_engine::{Allowlist, Engine as _, Limits, Network};
use afr_firewall::far::{FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_SPARE, FAR_UNLISTED, Far};
use libtest_mimic::Failed;

use crate::egress::{ERRNO, connect};
use crate::lane::{Lane, id_of, plan, spec};
use crate::run::{expect, run, runtime, shell};
use crate::trials::FAR_NAME;

/// The run each trial's sandbox is built and refilled under.
const RUN: &str = "egress-refill";
const OVERLAP_RUN: &str = "egress-refill-overlap";

/// Built to reach the far host's name at one address, then refilled to the
/// other, as a warm or parked microVM is when its next run's hosts resolved
/// anew: the name follows the set, and the address it left is closed.
pub(crate) fn reallow_swaps_the_set_in_place(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let first = Allowlist::new(vec![(FAR_NAME.to_owned(), FAR_UNLISTED)])?;
    let moved = Far::allowlist(FAR_NAME)?;
    let scripts = [
        connect(FAR_NAME, FAR_PORT),
        connect(FAR_UNLISTED, FAR_PORT),
        connect(FAR_LISTED, FAR_PORT),
    ];

    let (before, after) = across_a_refill(lane, RUN, &first, &moved, &scripts)?;

    expect(
        reached(&before) == [Some(true), Some(true), Some(false)],
        format!("before, the name reaches its first address alone: {before:?}"),
    )?;
    expect(
        reached(&after) == [Some(true), Some(false), Some(true)],
        format!("after, the name follows the set and the old address is closed: {after:?}"),
    )
}

/// Built to reach two addresses, then refilled to a set that keeps one of
/// them and swaps the other. The refill empties the set and re-adds the kept
/// address in the same batch, as an exclusive create, so the kernel must apply
/// the flush before the re-add: the kept address stays open, the dropped one
/// closes and the added one opens.
pub(crate) fn reallow_keeps_an_address_both_sets_hold(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let at = |address| (FAR_NAME.to_owned(), address);
    let first = Allowlist::new(vec![at(FAR_LISTED), at(FAR_UNLISTED)])?;
    let moved = Allowlist::new(vec![at(FAR_LISTED), at(FAR_SPARE)])?;
    let scripts = [FAR_LISTED, FAR_UNLISTED, FAR_SPARE].map(|address| connect(address, FAR_PORT));

    let (before, after) = across_a_refill(lane, OVERLAP_RUN, &first, &moved, &scripts)?;

    expect(
        reached(&before) == [Some(true), Some(true), Some(false)],
        format!("before, the kept and the dropped address are open: {before:?}"),
    )?;
    expect(
        reached(&after) == [Some(true), Some(false), Some(true)],
        format!("after, the kept address stays, the dropped closes, the added opens: {after:?}"),
    )
}

/// What each of `scripts` printed in a guest built for `run_name` to reach
/// `first`, then again once its running microVM was refilled to `moved`.
fn across_a_refill(
    lane: &Lane,
    run_name: &str,
    first: &Allowlist,
    moved: &Allowlist,
    scripts: &[String],
) -> Result<(Vec<String>, Vec<String>), Failed> {
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of(run_name);
        let request = spec(&engine, &id, Limits::default(), Network::Allowed(first))?;
        let mut sandbox = engine.prepare(request).await?;
        let mut said = Vec::with_capacity(2 * scripts.len());
        for script in scripts {
            said.push(run(sandbox.agent(), shell(script)).await);
        }
        let refilled = sandbox.egress(plan(2, Network::Allowed(moved))).await;
        for script in scripts {
            said.push(run(sandbox.agent(), shell(script)).await);
        }
        sandbox.destroy().await?;
        refilled?;
        let said = said
            .into_iter()
            .map(|outcome| outcome.map(|outcome| outcome.output.trim().to_owned()))
            .collect::<Result<Vec<_>, Failed>>()?;
        let (before, after) = said.split_at(scripts.len());
        Ok((before.to_vec(), after.to_vec()))
    })
}

/// Whether each answer reached the far host, was refused, or neither.
fn reached(said: &[String]) -> Vec<Option<bool>> {
    said.iter()
        .map(|answer| {
            if answer == FAR_GREETING {
                Some(true)
            } else if answer.starts_with(ERRNO) {
                Some(false)
            } else {
                None
            }
        })
        .collect()
}
