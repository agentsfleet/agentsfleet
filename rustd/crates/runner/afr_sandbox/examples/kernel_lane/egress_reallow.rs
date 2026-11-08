//! A held sandbox whose allowlisted name now resolves to other addresses takes
//! them in place, on a real kernel, with no sandbox rebuilt: the name follows
//! the set, an address the set left is closed, and one both sets hold stays
//! open through the refill.

use afr_sandbox::egress_testing::{
    FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_SPARE, FAR_UNLISTED, Far,
};
use afr_sandbox::{Allowlist, Engine, Limits, Network, SandboxRequest};
use libtest_mimic::Failed;

use crate::egress::{ERRNO, FAR_NAME, connect};
use crate::lane::Lane;
use crate::run::{expect, run, runtime, shell};

/// The lease each trial's sandbox is built and refilled under.
const LEASE: &str = "egress-reallow";
const OVERLAP_LEASE: &str = "egress-reallow-overlap";

/// Built to reach the far host's name at one address, then frozen and
/// refilled to the other, as a held sandbox is when its next lease's hosts
/// resolved anew: the name follows the set, and the address it left is closed.
pub(crate) fn reallow_swaps_the_set_in_place(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let first = Allowlist::new(vec![(FAR_NAME.to_owned(), FAR_UNLISTED)])?;
    let moved = Far::allowlist(FAR_NAME)?;
    let scripts = [
        connect(FAR_NAME, FAR_PORT),
        connect(&FAR_UNLISTED.to_string(), FAR_PORT),
        connect(&FAR_LISTED.to_string(), FAR_PORT),
    ];

    let (before, after) = across_a_refill(lane, LEASE, &first, &moved, &scripts)?;

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
    let scripts = [FAR_LISTED, FAR_UNLISTED, FAR_SPARE]
        .map(|address| connect(&address.to_string(), FAR_PORT));

    let (before, after) = across_a_refill(lane, OVERLAP_LEASE, &first, &moved, &scripts)?;

    expect(
        reached(&before) == [Some(true), Some(true), Some(false)],
        format!("before, the kept and the dropped address are open: {before:?}"),
    )?;
    expect(
        reached(&after) == [Some(true), Some(false), Some(true)],
        format!("after, the kept address stays, the dropped closes, the added opens: {after:?}"),
    )
}

/// What each of `scripts` printed in a sandbox built for `lease` to reach
/// `first`, then again after it was frozen, refilled to `moved` and thawed.
fn across_a_refill(
    lane: &Lane,
    lease: &str,
    first: &Allowlist,
    moved: &Allowlist,
    scripts: &[String],
) -> Result<(Vec<String>, Vec<String>), Failed> {
    runtime().block_on(async {
        let request =
            SandboxRequest::new(lease, Limits::default()).with_network(Network::Allowed(first));
        let mut sandbox = lane.engine().prepare(request).await?;
        let mut said = Vec::with_capacity(2 * scripts.len());
        for script in scripts {
            said.push(run(sandbox.executor(), shell(script)).await);
        }
        sandbox.freeze().await?;
        let refilled = sandbox.reallow(moved).await;
        sandbox.thaw().await?;
        for script in scripts {
            said.push(run(sandbox.executor(), shell(script)).await);
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
