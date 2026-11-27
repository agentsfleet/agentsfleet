//! A run's workspace as a save reads it: held still for an instant, pinned,
//! then read back after tenant code runs on.
//!
//! ```text
//!   pause ─► pin(parent) ─► resume        tenant code held only for the pin
//!                │
//!                └─► changes(pin) ─► …publication… ─► release(pin, committed)
//! ```
//!
//! [`pin_paused`] is the only way to a [`Resumed`] pin, and it resumes before
//! it returns, so whatever reads a pin's changes, registers or uploads them
//! runs after tenant code is free again: publishing under the pause does not
//! compile.

use af_chunk_store::Change;
use af_core::id::Uuid7;
use futures_util::stream::BoxStream;

use crate::error::Result;

/// Proof that tenant code is held still: only [`pin_paused`] makes one, and
/// only [`WorkspaceVolume::pin`] takes one.
#[derive(Debug, Clone, Copy)]
pub struct Paused(());

/// A pin taken under a pause that has since ended.
#[derive(Debug)]
pub struct Resumed<P>(P);

impl<P> Resumed<P> {
    /// What the pin holds.
    pub const fn pin(&self) -> &P {
        &self.0
    }

    /// The pin itself, for the volume that releases it.
    pub fn into_pin(self) -> P {
        self.0
    }
}

/// What a pause did to take one pin: the work that grows with the workspace
/// and the work that grows only with what changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PausedWork {
    /// The entries whose identity the pin read.
    pub entries: u64,
    /// The bytes of what changed that the pin copied.
    pub changed_bytes: u64,
}

/// A workspace a save reads in two steps: what changed is fixed under a pause,
/// and read only after the pause ends.
#[async_trait::async_trait]
pub trait WorkspaceVolume: Send + Sync {
    /// What one pin holds until it is released.
    type Pin: Send + Sync;

    /// Holds tenant code still; returns once nothing of the tenant's can
    /// write the workspace until [`WorkspaceVolume::resume`].
    ///
    /// # Errors
    /// The hold could not be taken; nothing is held.
    async fn pause(&self) -> Result<()>;

    /// Fixes what changed since `parent`, the generation the last committed
    /// pin published, or since nothing; returns once no later write can
    /// change what the pin reads.
    ///
    /// # Errors
    /// The pin could not be taken; nothing of it is kept.
    async fn pin(&self, paused: &Paused, parent: Option<&Uuid7>) -> Result<Self::Pin>;

    /// Lets tenant code run on.
    ///
    /// # Errors
    /// The hold could not be ended: tenant code may still be held.
    async fn resume(&self) -> Result<()>;

    /// What `pin` fixed: each changed entry, each followed by its changed
    /// chunks, and a removal for each path `parent` held that is gone. Read
    /// as often as a publication needs; every read yields the same changes.
    fn changes<'a>(&'a self, pin: &'a Resumed<Self::Pin>) -> BoxStream<'a, Result<Change>>;

    /// What the pause did to take `pin`.
    fn paused_work(&self, pin: &Resumed<Self::Pin>) -> PausedWork;

    /// The generation `pin`'s changes are the difference from: the `parent`
    /// it was taken against, or none when its changes are the whole
    /// workspace, as a pin with no record of `parent` reads it.
    fn parent<'a>(&self, pin: &'a Resumed<Self::Pin>) -> Option<&'a Uuid7>;

    /// Ends `pin`. `committed` names the generation the daemon answered the
    /// commit of, so the next pin against it reads only what changed since
    /// this one; none keeps the last committed pin's record.
    ///
    /// # Errors
    /// What the pin held could not be removed.
    async fn release(&self, pin: Resumed<Self::Pin>, committed: Option<&Uuid7>) -> Result<()>;
}

/// Pauses tenant code, pins `volume` against `parent` and resumes it, whether
/// or not the pin was taken.
///
/// # Errors
/// The pause, the pin or the resume failed. A failed resume drops the pin: a
/// workspace tenant code may still be held over is never published.
pub async fn pin_paused<V: WorkspaceVolume + ?Sized>(
    volume: &V,
    parent: Option<&Uuid7>,
) -> Result<Resumed<V::Pin>> {
    volume.pause().await?;
    let pinned = volume.pin(&Paused(()), parent).await;
    volume.resume().await?;
    pinned.map(Resumed)
}
