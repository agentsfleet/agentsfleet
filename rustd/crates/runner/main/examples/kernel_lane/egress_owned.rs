//! An egress table belongs to the runner that built it: no other process on
//! the host may delete it or flush it away. And a host forward chain that
//! would drop the sandbox's traffic keeps the probe from reporting
//! enforcement at all.

use std::io;

use afr_sandbox::egress_testing::{
    FAR_GREETING, FAR_PORT, FAR_UNLISTED, Far, delete_from_elsewhere, objects,
    probe_beside_a_dropping_forward_chain,
};
use afr_sandbox::{Engine, Limits, Network, SandboxRequest};
use libtest_mimic::Failed;

use crate::egress::{ERRNO, FAR_NAME, connect};
use crate::lane::Lane;
use crate::run::{expect, run, runtime, shell};

/// The prefix every scope's table name carries.
const TABLE_PREFIX: &str = "afegress";
/// What the kernel answers a socket that does not own a table (`EPERM`).
const NOT_OWNER: i32 = libc::EPERM;

/// A live scope's table survives what another process tries on it — deleting
/// it by name is refused, a flush passes it by — and goes on holding the
/// sandbox to its allowlist.
pub(crate) fn host_cannot_delete_a_live_table(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let allowlist = Far::allowlist(FAR_NAME)?;
    let before = objects()?;
    let scripts = [
        connect(&FAR_UNLISTED.to_string(), FAR_PORT),
        connect(FAR_NAME, FAR_PORT),
    ];

    let (answers, survived, said) = runtime().block_on(async {
        let engine = lane.engine();
        let request = SandboxRequest::new("egress-owned", Limits::default())
            .with_network(Network::Allowed(&allowlist));
        let sandbox = engine.prepare(request).await?;
        let table = objects()?
            .into_iter()
            .find(|name| name.starts_with(TABLE_PREFIX) && !before.contains(name))
            .ok_or("the live scope's table is listed")?;
        let answers = delete_from_elsewhere(&table)?;
        let survived = objects()?.contains(&table);
        let mut said = Vec::with_capacity(scripts.len());
        for script in &scripts {
            let outcome = run(sandbox.executor(), shell(script)).await;
            said.push(outcome.map(|outcome| outcome.output.trim().to_owned()));
        }
        sandbox.destroy().await?;
        let said = said.into_iter().collect::<Result<Vec<_>, Failed>>()?;
        Ok::<_, Failed>((answers, survived, said))
    })?;

    let [by_name, flushed] = answers;
    expect(
        by_name.as_ref().err().and_then(io::Error::raw_os_error) == Some(NOT_OWNER),
        format!("deleting the table by name is refused: {by_name:?}"),
    )?;
    expect(
        flushed.is_ok(),
        format!("a flush succeeds and passes the table by: {flushed:?}"),
    )?;
    expect(survived, "the table outlives both")?;
    expect(
        said.first().is_some_and(|answer| answer.starts_with(ERRNO))
            && said.get(1).is_some_and(|answer| answer == FAR_GREETING),
        format!("the allowlist still holds: {said:?}"),
    )
}

/// Beside a forward chain that drops by policy the probe reports no
/// enforcement; with the chain gone it reports enforcement again.
pub(crate) fn probe_refuses_a_dropping_forward_chain(_lane: &Lane) -> Result<(), Failed> {
    let [beside, without] = probe_beside_a_dropping_forward_chain()?;

    expect(
        !beside,
        "a host forward chain that drops keeps the probe from reporting enforcement",
    )?;
    expect(
        without,
        "with that chain gone, the probe reports enforcement",
    )
}
