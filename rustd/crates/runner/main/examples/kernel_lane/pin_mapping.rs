//! A write through a shared mapping, on a real microVM, is saved by the next
//! pin (Dimension 3.4).
//!
//! The tenant writes a page through `MAP_SHARED` before one pin and writes
//! the same page again after it. A page still dirty takes no fault on the
//! second write, and whether the kernel then moves the file's change time is
//! the open question D23: the walk trusts an entry whose stat fields did not
//! move. The trial passes when the next pin's export carries the new bytes,
//! and says either way whether the change time moved.

use std::time::{Duration, Instant};

use af_guest_wire::protocol::PinWalked;
use afr_engine::Engine as _;
use afr_guest_client::GuestAgent;
use bytes::Bytes;
use libtest_mimic::Failed;

use crate::lane::{Lane, id_of, isolated};
use crate::run::{expect, run, runtime, shell};
use crate::trials::Body;

/// A detached tenant that maps a page of `mapped`, writes it, says `ready`,
/// waits for `go`, writes the same page through the same mapping, says
/// `done` and holds the mapping open.
const START_MAPPER: &str = "setsid python3 -c '
import mmap, os, time
with open(\"/workspace/mapped\", \"wb\") as f:
    f.write(b\"a\" * 4096)
f = open(\"/workspace/mapped\", \"r+b\")
m = mmap.mmap(f.fileno(), 4096, mmap.MAP_SHARED)
m[0:6] = b\"before\"
open(\"/workspace/ready\", \"w\").close()
while not os.path.exists(\"/workspace/go\"):
    time.sleep(0.01)
m[0:6] = b\"after!\"
open(\"/workspace/done\", \"w\").close()
time.sleep(600)
' >/dev/null 2>&1 </dev/null &";
/// The mapped file's change time, as the guest's own `stat` prints it.
const CHANGE_TIME: &str = "stat -c %z /workspace/mapped";
/// The bytes the second write puts through the mapping.
const AFTER: &[u8] = b"after!";
/// Past a tick of the coarse clock file times come from, so the first pin's
/// tick falls after the first write and the racy-clean guard cannot be what
/// saves the second.
const PAST_A_TICK: Duration = Duration::from_millis(50);
/// How long the tenant has to say each step.
const STEP_WITHIN: Duration = Duration::from_secs(10);

/// The trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[(
    "test_a_shared_mapping_write_is_saved_by_the_next_save",
    a_shared_mapping_write_is_saved_by_the_next_save,
)];

/// The second pin, against the first's committed generation, exports the
/// page the tenant wrote through its mapping after the first.
fn a_shared_mapping_write_is_saved_by_the_next_save(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("pin-mapping");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let sandbox = engine.prepare(isolated(&engine, &id)?).await?;
        let agent = sandbox.agent();
        let observed = mapped_twice(agent).await;
        sandbox.destroy().await?;
        let Observed {
            second,
            stream,
            before,
            after,
        } = observed?;
        let moved = if before == after {
            "did not move"
        } else {
            "moved"
        };
        expect(second.delta, "the second pin reads against the first")?;
        expect(
            stream.windows(AFTER.len()).any(|window| window == AFTER),
            format!(
                "the next save holds the mapped write; its change time {moved} \
                 ({before} then {after}), which D23 asks"
            ),
        )
    })
}

/// What the trial saw around the second mapped write.
struct Observed {
    /// The second pin's answer.
    second: PinWalked,
    /// Its export.
    stream: Vec<u8>,
    /// The mapped file's change time before the second write.
    before: String,
    /// And after it.
    after: String,
}

/// Pins once after the first mapped write and once after the second.
async fn mapped_twice(agent: &dyn GuestAgent) -> Result<Observed, Failed> {
    run(agent, shell(START_MAPPER)).await?;
    said(agent, "ready").await?;
    tokio::time::sleep(PAST_A_TICK).await;
    let (first, _whole) = pin(agent, None).await?;
    agent.pin_release(first.pin, Some("g1")).await?;
    let before = run(agent, shell(CHANGE_TIME)).await?.output;
    agent.write_file("go", Bytes::new()).await?;
    said(agent, "done").await?;
    let after = run(agent, shell(CHANGE_TIME)).await?.output;
    let (second, stream) = pin(agent, Some("g1")).await?;
    Ok(Observed {
        second,
        stream,
        before: before.trim().to_owned(),
        after: after.trim().to_owned(),
    })
}

/// One pin as a save takes it: the tenant frozen for the walk alone.
async fn pin(agent: &dyn GuestAgent, parent: Option<&str>) -> Result<(PinWalked, Vec<u8>), Failed> {
    agent.freeze_tenant().await?;
    let mut stream = Vec::new();
    let walked = agent.pin_walk(parent, &mut stream).await;
    agent.thaw_tenant().await?;
    Ok((walked?, stream))
}

/// Waits until the tenant has made the file `name`.
async fn said(agent: &dyn GuestAgent, name: &str) -> Result<(), Failed> {
    let deadline = Instant::now() + STEP_WITHIN;
    while Instant::now() < deadline {
        if agent.read_file(name, 0).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("the tenant never said {name}").into())
}
