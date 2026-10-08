//! A held sandbox whose allowlisted name now resolves to another address takes
//! the new address in place, on a real kernel: the name reaches the new
//! address, and the old one is closed, with no sandbox rebuilt.

use afr_sandbox::egress_testing::{FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_UNLISTED, Far};
use afr_sandbox::{Allowlist, Engine, Limits, Network, SandboxRequest};
use libtest_mimic::Failed;

use crate::egress::{ERRNO, FAR_NAME, connect};
use crate::lane::Lane;
use crate::run::{expect, run, runtime, shell};

/// The lease the trial's sandbox is built and refilled under.
const LEASE: &str = "egress-reallow";

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

    let (before, after) = runtime().block_on(async {
        let request =
            SandboxRequest::new(LEASE, Limits::default()).with_network(Network::Allowed(&first));
        let mut sandbox = lane.engine().prepare(request).await?;
        let mut said = Vec::with_capacity(2 * scripts.len());
        for script in &scripts {
            said.push(run(sandbox.executor(), shell(script)).await);
        }
        sandbox.freeze().await?;
        let refilled = sandbox.reallow(&moved).await;
        sandbox.thaw().await?;
        for script in &scripts {
            said.push(run(sandbox.executor(), shell(script)).await);
        }
        sandbox.destroy().await?;
        refilled?;
        let said = said
            .into_iter()
            .map(|outcome| outcome.map(|outcome| outcome.output.trim().to_owned()))
            .collect::<Result<Vec<_>, Failed>>()?;
        let (before, after) = said.split_at(scripts.len());
        Ok::<_, Failed>((before.to_vec(), after.to_vec()))
    })?;

    let refused = |answer: &String| answer.starts_with(ERRNO);
    expect(
        before.first().is_some_and(|answer| answer == FAR_GREETING)
            && before.get(1).is_some_and(|answer| answer == FAR_GREETING)
            && before.get(2).is_some_and(refused),
        format!("before, the name reaches its first address alone: {before:?}"),
    )?;
    expect(
        after.first().is_some_and(|answer| answer == FAR_GREETING)
            && after.get(1).is_some_and(refused)
            && after.get(2).is_some_and(|answer| answer == FAR_GREETING),
        format!("after, the name follows the set and the old address is closed: {after:?}"),
    )
}
