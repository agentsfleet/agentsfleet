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
//! compile. [`quiesce`] holds the pin and its volume together as
//! [`Quiesced`], which is what a sandbox's snapshots hand a publication.

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

/// A workspace held still once, then let run on: what a publication reads,
/// after the fleet resumed, and releases once the daemon or the store
/// answered its commit.
#[async_trait::async_trait]
pub trait Quiesced: Send + Sync {
    /// What the hold fixed, as [`WorkspaceVolume::changes`] reads it.
    fn changes(&self) -> BoxStream<'_, Result<Change>>;

    /// The generation the changes are the difference from, when they are.
    fn parent(&self) -> Option<&Uuid7>;

    /// What the pause did.
    fn paused_work(&self) -> PausedWork;

    /// Ends the hold, naming the generation its publication committed.
    ///
    /// # Errors
    /// What it held could not be removed.
    async fn release(self: Box<Self>, committed: Option<&Uuid7>) -> Result<()>;
}

/// A pin of `V`, taken under a pause that has ended, read as [`Quiesced`].
pub struct QuiescedVolume<V: WorkspaceVolume> {
    volume: V,
    pin: Resumed<V::Pin>,
}

impl<V: WorkspaceVolume> std::fmt::Debug for QuiescedVolume<V> {
    /// A pin holds tenant content, so only that it is one is told.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuiescedVolume").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl<V: WorkspaceVolume> Quiesced for QuiescedVolume<V> {
    fn changes(&self) -> BoxStream<'_, Result<Change>> {
        self.volume.changes(&self.pin)
    }

    fn parent(&self) -> Option<&Uuid7> {
        self.volume.parent(&self.pin)
    }

    fn paused_work(&self) -> PausedWork {
        self.volume.paused_work(&self.pin)
    }

    async fn release(self: Box<Self>, committed: Option<&Uuid7>) -> Result<()> {
        let Self { volume, pin } = *self;
        volume.release(pin, committed).await
    }
}

/// Pauses `volume`, pins it against `parent` and resumes it, as
/// [`pin_paused`] does, and hands it back held for its publication.
///
/// # Errors
/// As [`pin_paused`].
pub async fn quiesce<V: WorkspaceVolume>(
    volume: V,
    parent: Option<&Uuid7>,
) -> Result<QuiescedVolume<V>> {
    let pin = pin_paused(&volume, parent).await?;
    Ok(QuiescedVolume { volume, pin })
}

/// A borrowed volume is that volume, so one owner can hold a workspace still
/// for each of several saves.
#[async_trait::async_trait]
impl<V: WorkspaceVolume + ?Sized> WorkspaceVolume for &V {
    type Pin = V::Pin;

    async fn pause(&self) -> Result<()> {
        (**self).pause().await
    }

    async fn pin(&self, paused: &Paused, parent: Option<&Uuid7>) -> Result<Self::Pin> {
        (**self).pin(paused, parent).await
    }

    async fn resume(&self) -> Result<()> {
        (**self).resume().await
    }

    fn changes<'a>(&'a self, pin: &'a Resumed<Self::Pin>) -> BoxStream<'a, Result<Change>> {
        (**self).changes(pin)
    }

    fn paused_work(&self, pin: &Resumed<Self::Pin>) -> PausedWork {
        (**self).paused_work(pin)
    }

    fn parent<'a>(&self, pin: &'a Resumed<Self::Pin>) -> Option<&'a Uuid7> {
        (**self).parent(pin)
    }

    async fn release(&self, pin: Resumed<Self::Pin>, committed: Option<&Uuid7>) -> Result<()> {
        (**self).release(pin, committed).await
    }
}
