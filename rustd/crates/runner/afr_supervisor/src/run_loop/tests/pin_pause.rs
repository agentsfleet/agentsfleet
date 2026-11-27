//! The pin pause over the workspace's own volume: a running tenant process
//! is frozen for the pin alone, the thaw runs whether or not the pin was
//! taken, and a call in flight skips the save before anything is frozen.

use super::{Pinning, pin_sandbox, release};
use crate::staging::Staging;
use crate::test_support::{FakeEngine, Parking};
use af_chunk_store::Change;
use af_core::id::Uuid7;
use afr_agent::RecoverySkip;
use afr_engine::{
    GuestPin, GuestVolume, Paused, PausedWork, Quiesced, Resumed, Sandbox, WorkspaceVolume, quiesce,
};
use futures_util::stream::BoxStream;
use tokio::sync::mpsc;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// How one save's pause ended: the skip, if it skipped; each step in order;
/// the pin files held while it was pinned, and those left once released.
#[derive(Debug, PartialEq, Eq)]
struct Saved {
    skipped: Option<RecoverySkip>,
    order: Vec<&'static str>,
    held: usize,
    left: usize,
}

/// The workspace's own volume, reporting its pin on the channel the fake
/// sandbox reports its quiescence, freeze and thaw on.
struct Ordered<'a> {
    inner: GuestVolume<'a>,
    steps: mpsc::UnboundedSender<&'static str>,
}

#[async_trait::async_trait]
impl WorkspaceVolume for Ordered<'_> {
    type Pin = GuestPin;

    async fn pause(&self) -> afr_engine::Result<()> {
        self.inner.pause().await
    }

    async fn pin(&self, paused: &Paused, parent: Option<&Uuid7>) -> afr_engine::Result<GuestPin> {
        let pin = self.inner.pin(paused, parent).await;
        let _reader_gone = self
            .steps
            .send(if pin.is_ok() { "pin" } else { "pin refused" });
        pin
    }

    async fn resume(&self) -> afr_engine::Result<()> {
        self.inner.resume().await
    }

    fn changes<'a>(
        &'a self,
        pin: &'a Resumed<GuestPin>,
    ) -> BoxStream<'a, afr_engine::Result<Change>> {
        self.inner.changes(pin)
    }

    fn paused_work(&self, pin: &Resumed<GuestPin>) -> PausedWork {
        self.inner.paused_work(pin)
    }

    fn parent<'a>(&self, pin: &'a Resumed<GuestPin>) -> Option<&'a Uuid7> {
        self.inner.parent(pin)
    }

    async fn release(
        &self,
        pin: Resumed<GuestPin>,
        committed: Option<&Uuid7>,
    ) -> afr_engine::Result<()> {
        self.inner.release(pin, committed).await
    }
}

/// One save's pause over a sandbox `parking` as told, whose guest agent
/// freezes when `freezes` and serves `workspace` when it has one.
async fn paused(
    parking: Parking,
    freezes: bool,
    workspace: Option<&std::path::Path>,
) -> Checked<Saved> {
    let (steps, mut recorded) = mpsc::unbounded_channel();
    let engine = FakeEngine {
        workspace: workspace.map(ToOwned::to_owned),
        toolbox: Some("b".repeat(64)),
        steps: Some(steps.clone()),
        parking,
        freezes,
        ..FakeEngine::default()
    };
    let sandbox = engine.sandbox().await?;
    let staged = tempfile::tempdir()?;
    let staging = Staging::new(staged.path().to_owned());
    let inner = GuestVolume::new(sandbox.as_ref(), staging.root());
    let volume = Ordered { inner, steps };
    let quiesced = async {
        let held = quiesce(&volume, None).await?;
        Ok(Box::new(held) as Box<dyn Quiesced + '_>)
    };
    let (skipped, held) = match pin_sandbox(sandbox.as_ref() as &dyn Sandbox, quiesced).await? {
        Pinning::Skipped(reason) => (Some(reason), pinned(staged.path())?),
        Pinning::Pinned(pin) => {
            let held = pinned(staged.path())?;
            release(pin, None).await;
            (None, held)
        }
    };
    let mut order = Vec::new();
    while let Ok(step) = recorded.try_recv() {
        order.push(step);
    }
    let left = pinned(staged.path())?;
    Ok(Saved {
        skipped,
        order,
        held,
        left,
    })
}

/// The pin files under `staged`.
fn pinned(staged: &std::path::Path) -> Checked<usize> {
    let mut found = 0;
    for entry in std::fs::read_dir(staged)? {
        found += usize::from(entry?.path().extension().is_some_and(|ext| ext == "pin"));
    }
    Ok(found)
}

#[tokio::test]
async fn a_running_tenant_is_frozen_for_the_pin_and_thawed_after_it() -> Checked {
    let workspace = tempfile::tempdir()?;
    std::fs::write(workspace.path().join("kept"), b"kept")?;

    let saved = paused(Parking::BusyTenant, true, Some(workspace.path())).await?;

    assert_eq!(
        saved,
        Saved {
            skipped: None,
            order: vec!["quiesce", "freeze", "pin", "thaw"],
            held: 1,
            left: 0,
        }
    );
    Ok(())
}

/// The pin fails while the tenant is frozen, and the thaw still runs: the
/// save skips, and no tenant process is left held.
#[tokio::test]
async fn a_pin_that_fails_still_thaws_the_tenant() -> Checked {
    let saved = paused(Parking::BusyTenant, true, None).await?;

    assert_eq!(
        saved,
        Saved {
            skipped: Some(RecoverySkip::Unavailable),
            order: vec!["quiesce", "freeze", "pin refused", "thaw"],
            held: 0,
            left: 0,
        }
    );
    Ok(())
}

/// A call still in flight skips the save before the tenant is read, frozen
/// or pinned.
#[tokio::test]
async fn a_call_in_flight_skips_the_save_before_any_freeze_or_pin() -> Checked {
    let workspace = tempfile::tempdir()?;

    let saved = paused(Parking::BusyAgent, true, Some(workspace.path())).await?;

    assert_eq!(
        saved,
        Saved {
            skipped: Some(RecoverySkip::BusyAgent),
            order: vec![],
            held: 0,
            left: 0,
        }
    );
    Ok(())
}

/// Where nothing can freeze it, a running tenant process still skips the
/// save, and no pin is taken.
#[tokio::test]
async fn a_tenant_no_leaf_can_freeze_skips_the_save_unpinned() -> Checked {
    let workspace = tempfile::tempdir()?;

    let saved = paused(Parking::BusyTenant, false, Some(workspace.path())).await?;

    assert_eq!(saved.skipped, Some(RecoverySkip::BusyProcesses));
    assert!(
        !saved.order.iter().any(|step| step.starts_with("pin")),
        "{:?}",
        saved.order
    );
    assert_eq!((saved.held, saved.left), (0, 0));
    Ok(())
}
