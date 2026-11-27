//! A sandbox's life between commands: a kill leaves no tenant process, and
//! each park mode keeps what it says it keeps, or is refused as undeclared.

use bytes::Bytes;
use libtest_mimic::Failed;

use super::Context;
use super::run::{ALIVE, BACKGROUND_ALIVE, GONE, build, expect, said, start_background, within};
use crate::engine::{Engine, Killed, Limits, ParkMode, Quiescence};

/// The note written before a park, and the workspace file it is written to.
const NOTE: &[u8] = b"written before the park";
const NOTE_FILE: &str = "note";
/// How much of the note a read takes.
const NOTE_BYTES: u64 = 64;
/// What a refused park names.
const PARK: &str = "park";

/// With a process running in a session of its own, the sandbox reads busy;
/// the kill settles, and it reads quiet.
pub(super) async fn kill_leaves_nothing(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let (before, killed, after) = within(build(engine, Limits::DEFAULT).await?, async |sandbox| {
        start_background(sandbox).await?;
        let before = sandbox.quiesce().await?;
        let killed = sandbox.kill().await;
        Ok((before, killed, sandbox.quiesce().await?))
    })
    .await?;
    expect(
        before == Quiescence::Busy,
        format!("a running process reads busy, got {before:?}"),
    )?;
    expect(
        matches!(killed, Killed::Settled),
        format!("the kill proves the tree empty, got {killed:?}"),
    )?;
    expect(
        after == Quiescence::Quiet,
        format!("nothing runs after the kill, got {after:?}"),
    )
}

/// A paused sandbox resumes with the note it was written and the process it
/// ran still running; an engine that does not declare `Pause` refuses it.
pub(super) async fn paused(engine: &dyn Engine, _context: &Context<'_>) -> Result<(), Failed> {
    parked(engine, ParkMode::Pause, ALIVE).await
}

/// A stopped sandbox resumes on its disk alone: the note reads back and no
/// earlier process remains; an engine that does not declare `Stop` refuses
/// it.
pub(super) async fn stopped(engine: &dyn Engine, _context: &Context<'_>) -> Result<(), Failed> {
    parked(engine, ParkMode::Stop, GONE).await
}

/// Writes the note and starts the background process, parks in `mode` and
/// resumes; the note must read back and the process read `process`. When
/// `engine` does not declare `mode`, the park must answer `Unsupported`.
async fn parked(engine: &dyn Engine, mode: ParkMode, process: &str) -> Result<(), Failed> {
    let declared = engine.capabilities().parks.contains(&mode);
    let sandbox = build(engine, Limits::DEFAULT).await?;
    let prepared = async {
        sandbox
            .agent()
            .write_file(NOTE_FILE, Bytes::from_static(NOTE))
            .await?;
        start_background(&*sandbox).await
    }
    .await;
    if let Err(failed) = prepared {
        sandbox.destroy().await?;
        return Err(failed);
    }
    let parked = match (declared, sandbox.park(mode).await) {
        (true, Ok(parked)) => parked,
        (true, Err(refused)) => {
            return Err(format!("it declares {mode:?} and refused it: {refused}").into());
        }
        (false, Ok(parked)) => {
            parked.destroy().await?;
            return Err(format!("it parked in {mode:?}, which it does not declare").into());
        }
        (false, Err(refused)) => {
            return expect(
                refused.unsupported_operation() == Some(PARK),
                format!("an undeclared {mode:?} answers Unsupported, got {refused}"),
            );
        }
    };
    let (note, after) = within(parked.resume().await?, async |sandbox| {
        let note = sandbox.agent().read_file(NOTE_FILE, NOTE_BYTES).await?;
        Ok((note.data, said(sandbox, BACKGROUND_ALIVE).await?))
    })
    .await?;
    expect(
        note == NOTE,
        format!("the note reads back after {mode:?}, got {note:?}"),
    )?;
    expect(
        after == process,
        format!("after {mode:?} the earlier process reads {process}, got {after:?}"),
    )
}
