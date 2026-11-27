//! A turn save holds the run only for its pin: every daemon call follows the
//! resume, a publication that fails or dies keeps the previous generation, and
//! the pin's staging file goes with it.

use self::fixture::{Log, Recording, owner, pin_files, pinned, recorded_plane, sandbox, save};
use super::capture_tests::{Checked, Remote, snapshot, workspace};
use super::{Encrypted, ID, encrypted, scope};
use crate::recovery::NativeTurn;
use crate::recovery::transfer::http_tests::server::Server;
use crate::run_loop::pin::Pinned as PinSource;
use crate::staging::Staging;
use af_core::{id::Uuid7, test_util::trace::Capture};
use afr_engine::Sandbox;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[path = "pause_fixture.rs"]
mod fixture;

#[tokio::test]
async fn a_save_resumes_before_it_registers_or_uploads() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    let mut server = Server::simultaneous(payloads).await?;
    let (root, staged) = (workspace()?, tempfile::tempdir()?);
    let (sandbox, steps) = sandbox(root.path()).await?;
    let (log, stop) = (Log::default(), CancellationToken::new());
    let staging = Staging::new(staged.path().to_owned());
    let volume = Recording::over(sandbox.as_ref(), &staging, steps, &log, &stop);
    let remote = Arc::new(Mutex::new(Remote::new(objects, &server.url, None)?));
    let (plane, _calls) = recorded_plane(remote, log.clone());
    let (run, transfers) = (Uuid7::parse(ID)?, server.transfers());
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);

    save(&mut native, sandbox.as_ref(), &volume, &stop).await??;

    let steps = log.steps();
    assert_eq!(
        steps,
        [
            "quiesce", "pause", "pin", "resume", "begin", "register", "seal", "pages", "commit",
            "release"
        ]
    );
    for _ in 0..4 {
        assert_eq!(server.seen.try_recv()?.method, http::Method::PUT);
    }
    assert_eq!(pin_files(staged.path())?, 0, "the pin's file went with it");
    server.finish().await
}

#[tokio::test]
async fn a_publication_failing_after_resume_keeps_the_previous_generation() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    // The second save's third PUT is refused: requests five to eight are its.
    let server = Server::refusing(payloads, Some(7)).await?;
    let (root, staged) = (workspace()?, tempfile::tempdir()?);
    let (sandbox, steps) = sandbox(root.path()).await?;
    let (log, stop) = (Log::default(), CancellationToken::new());
    let staging = Staging::new(staged.path().to_owned());
    let volume = Recording::over(sandbox.as_ref(), &staging, steps, &log, &stop);
    let remote = Arc::new(Mutex::new(Remote::new(objects, &server.url, None)?));
    let (plane, _calls) = recorded_plane(Arc::clone(&remote), log.clone());
    let (run, transfers) = (Uuid7::parse(ID)?, server.transfers());
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let previous = save(&mut native, sandbox.as_ref(), &volume, &stop).await??;
    // Every chunk is new to the owner again, so the next save uploads all four.
    owner(&remote)?.uploaded = false;
    let logs = Capture::install();

    let failed = save(&mut native, sandbox.as_ref(), &volume, &stop).await?;

    let failure = failed
        .err()
        .ok_or("a save whose PUT was refused committed")?;
    assert_eq!(
        failure.code(),
        af_core::error_code::RUN_CHECKPOINT_UNAVAILABLE
    );
    let head = owner(&remote)?.head.clone();
    assert_eq!(
        head.as_ref(),
        Some(&previous),
        "the head names the previous generation"
    );
    assert_eq!(
        native.head(),
        Some(&previous),
        "the next pin reads against it"
    );
    assert_eq!(
        pin_files(staged.path())?,
        0,
        "the failed pin's file is gone"
    );
    assert!(log.steps().ends_with(&["release uncommitted"]));
    let failed = logs.only("checkpoint_publication_failed");
    assert_eq!(failed.field("owner_kind"), Some("turn"));
    Ok(())
}

#[tokio::test]
async fn a_runner_killed_between_resume_and_commit_keeps_the_previous_generation() -> Checked {
    let Encrypted {
        objects,
        mut payloads,
    } = encrypted()?;
    let first = payloads.clone();
    // One more than the save sends: the second save's PUTs wait at the
    // fixture's barrier until the runner is killed under them.
    payloads.insert("/never-sent".to_owned(), Vec::new());
    let committed = Server::simultaneous(first).await?;
    let holding = Server::simultaneous(payloads).await?;
    let (root, staged) = (workspace()?, tempfile::tempdir()?);
    let (sandbox, steps) = sandbox(root.path()).await?;
    let (log, stop) = (Log::default(), CancellationToken::new());
    let staging = Staging::new(staged.path().to_owned());
    let volume = Recording::over(sandbox.as_ref(), &staging, steps, &log, &stop);
    let remote = Arc::new(Mutex::new(Remote::new(
        objects.clone(),
        &committed.url,
        None,
    )?));
    let (plane, _calls) = recorded_plane(Arc::clone(&remote), log.clone());
    let run = Uuid7::parse(ID)?;
    let transfers = committed.transfers();
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let previous = save(&mut native, sandbox.as_ref(), &volume, &stop).await??;
    *owner(&remote)? = Remote::new(objects, &holding.url, None)?;
    owner(&remote)?.head = Some(previous.clone());

    killed_mid_publication(&mut native, sandbox.as_ref(), &volume, &log, &stop).await?;

    let head = owner(&remote)?.head.clone();
    assert_eq!(
        head,
        Some(previous),
        "the head names the previous generation"
    );
    assert_eq!(
        pin_files(staged.path())?,
        1,
        "the dead runner left its pin's file"
    );
    Staging::sweep(staged.path())?;
    assert_eq!(
        std::fs::read_dir(staged.path())?.count(),
        0,
        "the next boot leaves no staging file"
    );
    Ok(())
}

/// Starts a save and kills it once its uploads are authorized: the
/// publication is dropped mid-flight and its pin is never released, as a
/// `SIGKILL` leaves both.
async fn killed_mid_publication(
    native: &mut NativeTurn<'_>,
    sandbox: &dyn Sandbox,
    volume: &Recording<'_>,
    log: &Log,
    stop: &CancellationToken,
) -> Checked {
    let pin = pinned(sandbox, volume, None).await?;
    {
        let source = PinSource { volume, pin: &pin };
        let publication = native.capture(snapshot(scope()), Some(&source), stop);
        tokio::pin!(publication);
        while !log.steps().contains(&"seal") {
            tokio::select! {
                biased;
                _published = &mut publication => return Err("the save ended before the kill".into()),
                () = tokio::time::sleep(Duration::from_millis(1)) => {}
            }
        }
    }
    // A killed process runs no destructor: its pin's file stays on disk.
    let _killed = std::mem::ManuallyDrop::new(pin);
    Ok(())
}
