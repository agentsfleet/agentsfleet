//! No key or link reaches the guest across a take and a restore. The store a
//! microVM's snapshots publish into seals every chunk on the host under a key
//! and a nonce of its own, and holds the signed link its objects are read
//! through, as the supervisor's store over the bucket does. After a take and a
//! restore, the planted-token search finds none of them in the guest's
//! files, environment or `/proc`, and finds a canary planted on purpose; the
//! monitor that ran before the take runs after the restore.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use af_chunk_store::{
    Change, ChunkSource, ChunkStore, MemoryStore, Publication, Result as StoreResult, Stored,
};
use af_core::id::Uuid7;
use af_crypto::NONCE_LEN;
use af_crypto::aad::Aad;
use af_crypto::checkpoint::{Aes256Gcm, ChunkCipher as _};
use af_crypto::secret::SecretBytes;
use afr_engine::{Engine as _, Sandbox, fresh_sandbox_id};
use base64::Engine as _;
use bytes::Bytes;
use libtest_mimic::Failed;
use zeroize::Zeroizing;

use crate::lane::{Lane, id_of, isolated};
use crate::run::{expect, run, runtime, shell, within};
use crate::trials::Body;

/// The key every chunk is sealed under: 32 bytes, as AES-256 takes.
const KEY: &[u8; 32] = b"laneSnapshotKeyNeverInsideGuest!";
/// The signed link the store holds for reading its objects back.
const LINK: &str = "https://lane-bucket.test/objects?X-Amz-Signature=laneSnapshotLinkNeverInside";
/// Where the canary is planted on purpose, and the path the search names it by.
const PLANTED: &str = "planted";
const PLANTED_PATH: &str = "/workspace/planted";

/// The trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[(
    "test_no_key_or_link_reaches_the_guest_across_take_and_restore",
    no_key_or_link,
)];

/// After a take and a restore, the guest holds no key, nonce or link of the
/// store's; the canary planted on purpose is found; the monitor is the same.
fn no_key_or_link(lane: &Lane) -> Result<(), Failed> {
    let id = id_of("snapshot-keys");
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let procs = engine
            .config()
            .cgroup_root
            .join(id.as_str())
            .join(afr_cgroup::SANDBOX_LEAF)
            .join(af_cgroup::CGROUP_PROCS);
        let sealing = Arc::new(Sealing::default());
        let store: Arc<dyn ChunkStore> = Arc::<Sealing>::clone(&sealing);
        let sandbox = engine
            .prepare(isolated(&engine, &id)?.with_store(&store))
            .await?;
        let found = within(sandbox, async |sandbox| {
            across(sandbox, &sealing, &procs).await
        })
        .await?;
        expect(
            found.after_restore.trim().is_empty(),
            format!(
                "no key, nonce or link reaches the guest, got {:?}",
                found.after_restore
            ),
        )?;
        expect(
            found.after_planting.trim() == PLANTED_PATH,
            format!(
                "the search finds the planted canary alone, got {:?}",
                found.after_planting
            ),
        )?;
        expect(
            sealing.held().fetched > 0,
            "the restore read its chunks back through the store",
        )?;
        expect(
            found.same_monitor,
            "the same monitor runs: the VM never stopped",
        )
    })
}

/// What the search found after the restore and after the planting, and
/// whether the monitor after is the one from before.
struct Found {
    after_restore: String,
    after_planting: String,
    same_monitor: bool,
}

/// Takes and restores a snapshot of `sandbox`, its chunks sealed by
/// `sealing`, searches the guest for what the store holds, then plants the
/// link and searches again; the monitor is read from `procs` before and after.
async fn across(sandbox: &dyn Sandbox, sealing: &Sealing, procs: &Path) -> Result<Found, Failed> {
    let monitor = fs::read_to_string(procs)?;
    let agent = sandbox.agent();
    let snapshots = sandbox
        .snapshots()
        .ok_or("the sandbox offers no snapshots")?;
    agent
        .write_file("kept", Bytes::from_static(b"kept across the restore"))
        .await?;
    let snapshot = snapshots.take(fresh_sandbox_id()).await?;
    snapshots.restore(&snapshot).await?;
    let search = search(&sealing.tokens());
    let after_restore = run(agent, shell(&search)).await?.output;
    agent
        .write_file(PLANTED, Bytes::from_static(LINK.as_bytes()))
        .await?;
    let after_planting = run(agent, shell(&search)).await?.output;
    snapshots.forget(&snapshot).await?;
    Ok(Found {
        after_restore,
        after_planting,
        same_monitor: fs::read_to_string(procs)? == monitor,
    })
}

/// Lists every file under the guest's writable trees, and every process's
/// environment and command line, that holds one of `tokens`. Each token is
/// spelled in two halves joined in an unexported variable, so no command
/// line or environment of the search itself holds it.
fn search(tokens: &[String]) -> String {
    let mut script = String::new();
    let mut joined = String::new();
    for (index, token) in tokens.iter().enumerate() {
        let (head, tail) = token.split_at(token.len() / 2);
        script.push_str(&format!("a{index}='{head}'; b{index}='{tail}'; "));
        joined.push_str(&format!(" \"$a{index}$b{index}\""));
    }
    format!(
        "{script}printf '%s\\n'{joined} | grep -rlsF -D skip --exclude-dir=lost+found -f - \
         /workspace /tmp /run /etc /home /root /var /proc/[0-9]*/environ /proc/[0-9]*/cmdline; true"
    )
}

/// What the sealing store has used, and how many objects it read back.
#[derive(Debug, Default)]
struct Used {
    nonces: Vec<[u8; NONCE_LEN]>,
    fetched: u64,
}

/// An in-memory store that holds only ciphertext: each chunk sealed on the
/// host under [`KEY`] and a nonce of its own, beside [`LINK`].
#[derive(Debug, Default)]
struct Sealing {
    inner: MemoryStore,
    used: Mutex<Used>,
}

impl Sealing {
    fn held(&self) -> MutexGuard<'_, Used> {
        self.used.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Everything a guest must never hold: the key, raw and in hex and
    /// base64, each nonce in hex, and the link.
    fn tokens(&self) -> Vec<String> {
        let key = std::str::from_utf8(KEY).unwrap_or_default().to_owned();
        let mut tokens = vec![
            key,
            hex::encode(KEY),
            base64::prelude::BASE64_STANDARD.encode(KEY),
            LINK.to_owned(),
        ];
        tokens.extend(self.held().nonces.iter().map(hex::encode));
        tokens
    }

    /// `plaintext` sealed under a fresh nonce, the nonce first.
    fn seal(&self, plaintext: Zeroizing<Vec<u8>>) -> StoreResult<Vec<u8>> {
        let mut used = self.held();
        let counter = u64::try_from(used.nonces.len()).unwrap_or(u64::MAX) + 1;
        // A prefix of its own, then a count, so no file a guest ships holds a
        // nonce's hex by chance and the search reads it as a leak.
        let mut nonce = *b"lane\0\0\0\0\0\0\0\0";
        let count = nonce.iter_mut().skip(4).zip(counter.to_be_bytes());
        count.for_each(|(slot, byte)| *slot = byte);
        let sealed = Aes256Gcm.seal(&key(), &nonce, &binding(), plaintext)?;
        used.nonces.push(nonce);
        Ok([nonce.as_slice(), &sealed].concat())
    }
}

fn key() -> SecretBytes {
    SecretBytes::new(KEY.to_vec())
}

fn binding() -> Aad {
    Aad::new("kernel-lane", "snapshot")
}

#[async_trait::async_trait]
impl ChunkSource for Sealing {
    async fn tree(&self, id: &Uuid7) -> StoreResult<Stored> {
        self.inner.tree(id).await
    }

    /// Opens `object`: a short or altered one fails its tag, and no byte of
    /// it is used.
    async fn read(&self, object: &str) -> StoreResult<Zeroizing<Vec<u8>>> {
        self.held().fetched += 1;
        let stored = self.inner.read(object).await?;
        let nonce = stored
            .first_chunk::<NONCE_LEN>()
            .copied()
            .unwrap_or_default();
        let sealed = stored.get(NONCE_LEN..).unwrap_or_default().to_vec();
        let opened = Aes256Gcm.open(&key(), &nonce, &binding(), sealed)?;
        Ok(opened.expose().to_vec().into())
    }
}

#[async_trait::async_trait]
impl ChunkStore for Sealing {
    async fn publish(
        &self,
        id: &Uuid7,
        parent: Option<&Uuid7>,
    ) -> StoreResult<Box<dyn Publication + '_>> {
        Ok(Box::new(Sealed {
            store: self,
            inner: self.inner.publish(id, parent).await?,
        }))
    }

    async fn forget(&self, id: &Uuid7) -> StoreResult<()> {
        self.inner.forget(id).await
    }
}

/// A publication whose chunks are sealed before they are stored.
struct Sealed<'a> {
    store: &'a Sealing,
    inner: Box<dyn Publication + 'a>,
}

#[async_trait::async_trait]
impl Publication for Sealed<'_> {
    async fn add(&mut self, change: Change) -> StoreResult<()> {
        let change = match change {
            Change::Chunk { key, bytes } => Change::Chunk {
                key,
                bytes: self.store.seal(bytes)?.into(),
            },
            entry @ Change::Entry(_) => entry,
        };
        self.inner.add(change).await
    }

    async fn commit(self: Box<Self>) -> StoreResult<u64> {
        self.inner.commit().await
    }
}
