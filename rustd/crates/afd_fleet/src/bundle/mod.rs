//! Fleet Bundle snapshots: the daemon's proxy in front of object storage.
//!
//! A runner holds no datastore credentials — that is the whole point of the
//! runner plane — so it cannot reach R2 itself. It learns a bundle's content
//! hash from its lease payload and asks the daemon, which does hold the keys,
//! for the immutable canonical tar under that hash.
//!
//! # The hash IS the access control, so the hash is a type
//!
//! The snapshot is content-addressed by SHA-256 and carries no secrets:
//! resolved secret values never enter the archive, because credentials ride the
//! lease's `secret_delivery` instead. An authenticated runner holding an
//! unguessable 256-bit digest is therefore the boundary, and the only way that
//! boundary fails is if a caller-supplied string reaches the storage key.
//!
//! [`ContentHash`] is what makes that impossible rather than merely unlikely.
//! The key is REBUILT server-side from a validated digest, so there is no path
//! from request bytes to a key — `bundles.zig` gets the same property from an
//! `isContentHash` guard the handler must remember to call, and this gets it
//! from a value the key builder cannot be handed without.
//!
//! # Why the object store is a `dyn`, and why that is not a trait of ours
//!
//! [`Bundles`] holds `Arc<dyn ObjectStore>` rather than being generic over one,
//! and it does NOT define a store trait of its own. `object_store` already ships
//! the seam: production hands it an `AmazonS3` pointed at an R2 account
//! endpoint, and a test hands it `object_store::memory::InMemory`. A trait here
//! would be a second seam over the first, and the suite would then be proving
//! this crate's mock rather than the client that runs in production.

use std::sync::Arc;

use bytes::Bytes;
// `ObjectStoreExt` carries `get`; `ObjectStore` itself is object-safe and
// carries only `get_opts`. The `dyn` is spelled against the first and the call
// goes through the second's blanket implementation.
use object_store::path::Path as StorePath;
use object_store::{ObjectStore, ObjectStoreExt as _};

use crate::error::{
    Error, Result, bundle_missing, bundle_oversized, bundle_storage, bundle_unconfigured, rejected,
};

/// A content hash is the lowercase hex of a SHA-256 digest: 32 bytes, 64 chars.
const SHA256_HEX_LEN: usize = 64;

/// The key layout `fleet_library/importer.zig` writes a snapshot under.
///
/// Restated here rather than derived, because the two implementations write and
/// read the same bucket: `prepare` puts under this prefix and this reads from
/// it, so a divergence would be a runner that fetches nothing forever while
/// every import reports success. RULE UFS single-sources it per implementation;
/// what pins the two together across implementations is
/// `test_snapshot_key_matches_the_zig_layout`.
const SNAPSHOT_KEY_PREFIX: &str = "fleet-bundles/sha256/";

/// The extension every snapshot key ends in.
const SNAPSHOT_KEY_SUFFIX: &str = ".tar";

/// The largest snapshot this daemon will hold in memory to answer one request.
///
/// `importer.zig` caps a bundle's support files at 256 KiB in total, so a
/// snapshot written by this product cannot approach this. The ceiling is not
/// about those: it is about the fact that the SIZE of a stored object is not a
/// thing this daemon validated, and reading an object of unknown size into a
/// buffer is how one misfiled upload becomes a memory fault. A megabyte is the
/// importer's own limit with room for tar framing and then some.
const MAX_SNAPSHOT_BYTES: u64 = 1024 * 1024;

/// The refusal a path segment that is not a digest earns.
///
/// Says what a correct value looks like, because unlike every other refusal on
/// this plane the caller CAN act on it — a runner sending this has a bug in how
/// it read its own lease payload.
const DETAIL_NOT_A_CONTENT_HASH: &str =
    "bundle ref must be a 64-character lowercase sha256 hex digest";

/// A validated SHA-256 content hash, and the only thing a snapshot key is built
/// from.
///
/// Borrowed rather than owned: every caller parses one out of a path segment
/// that outlives the fetch, and owning it would allocate on the request path to
/// hold bytes the request already holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentHash<'a>(&'a str);

impl<'a> ContentHash<'a> {
    /// Reads `raw` as a content hash, or refuses it.
    ///
    /// Exactly 64 lowercase hex characters. Uppercase is refused rather than
    /// folded, and that is deliberate: the digest is written lowercase by the
    /// importer, so accepting `A-F` would mean two spellings of one key and a
    /// cache that answers for one of them. `bundles.zig` refuses it too.
    ///
    /// # Errors
    /// Refuses anything that is not that, including — and this is the case the
    /// check exists for — a segment carrying path characters.
    pub fn parse(raw: &'a str) -> Result<Self> {
        // `is_ascii_hexdigit` is the obvious call and is the wrong one: it
        // accepts `A-F`, which is the single spelling this refuses.
        let is_digest = raw.len() == SHA256_HEX_LEN
            && raw
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
        if is_digest {
            Ok(Self(raw))
        } else {
            Err(rejected(DETAIL_NOT_A_CONTENT_HASH))
        }
    }

    /// The digest as it was written.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// The object-storage key this snapshot is stored under.
    ///
    /// Infallible, and it is the type that makes it so: `StorePath::from` would
    /// normalise or reject a key holding path characters, and there are none to
    /// hold — [`Self::parse`] admitted 64 characters of `[0-9a-f]` and nothing
    /// else, so the only variable part of this string is a hex digest.
    #[must_use]
    pub fn snapshot_key(self) -> StorePath {
        StorePath::from(format!(
            "{SNAPSHOT_KEY_PREFIX}{}{SNAPSHOT_KEY_SUFFIX}",
            self.0
        ))
    }
}

/// The canonical tar for one bundle, whole.
///
/// Buffered rather than streamed, and the bound above is why that is safe. It
/// is also why it is BETTER: a stream that fails after its first chunk has
/// already sent a 200, so a truncated tar reaches the runner as a successful
/// fetch and fails later as a corrupt archive. Reading the object before
/// answering means a storage fault is still a 503 the runner can act on.
pub type Snapshot = Bytes;

/// The Fleet Bundle snapshot store, which a deployment may not have.
///
/// The absence is held HERE rather than as an `Option` on the services trait,
/// and that is the whole design decision in this file. An unconfigured store is
/// a 503 with a registry code and a sentence, exactly like a store that will not
/// answer — so it belongs in the same classification table as every other
/// refusal this crate decides, not in an `if` at the top of a handler. Handing
/// the HTTP layer an `Option` would make the one refusal that has no error type
/// the handler's to invent, which is how two call sites end up describing one
/// failure differently.
///
/// Cheap to clone — the store handle behind it is an `Arc` and the client's
/// connection pool is shared — so the composition root builds one and every
/// request borrows it.
#[derive(Debug, Clone)]
pub struct Bundles {
    store: Option<Arc<dyn ObjectStore>>,
}

impl Bundles {
    /// Wraps an already-built object store.
    ///
    /// Takes the store CONSTRUCTED rather than taking credentials, for the
    /// reason `ServingPlane` takes connected pools: boot has already decided
    /// whether this deployment has snapshot storage at all, and a second place
    /// that can build one is a second place for the bucket to be wrong.
    #[must_use]
    pub const fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store: Some(store) }
    }

    /// A deployment with no snapshot storage configured.
    ///
    /// Not a failure at boot, and `serve_r2.zig` agrees: the daemon builds an
    /// R2 client only when all four knobs are present and serves everything
    /// else regardless. A fleet with no support files never asks, so refusing
    /// to start would take the whole product down for a verb most deployments
    /// never reach.
    #[must_use]
    pub const fn unconfigured() -> Self {
        Self { store: None }
    }

    /// Reads the snapshot stored under `hash`.
    ///
    /// # Errors
    /// Reports a hash with nothing stored under it — which is an ORDINARY
    /// outcome, not a fault: a skill-only bundle stores no snapshot, and the
    /// runner proceeds with no support files. Also reports a store that would
    /// not answer, and an object too large to buffer.
    pub async fn fetch(&self, hash: ContentHash<'_>) -> Result<Snapshot> {
        let Some(store) = self.store.as_ref() else {
            return Err(bundle_unconfigured());
        };
        let key = hash.snapshot_key();
        let got = store.get(&key).await.map_err(classify_store)?;

        // Checked BEFORE the body is read, which is the only moment it can be:
        // `object_store` reports the object's size from the response head, so
        // an oversized object is refused having transferred nothing.
        if got.meta.size > MAX_SNAPSHOT_BYTES {
            return Err(bundle_oversized(got.meta.size));
        }

        got.bytes().await.map_err(classify_store)
    }
}

/// Sorts a store failure into the one that is expected and the one that is not.
///
/// A missing object is not an error condition in this product — see
/// [`Bundles::fetch`] — so it gets its own kind and its own registry code,
/// while everything else the client can fail with (a refused signature, a
/// timeout, a bucket that does not exist) collapses into one: a runner acts
/// identically on all of them, and the distinction an OPERATOR needs rides
/// through as the source.
fn classify_store(error: object_store::Error) -> Error {
    match error {
        object_store::Error::NotFound { .. } => bundle_missing(),
        other => bundle_storage(other),
    }
}

#[cfg(test)]
mod tests;
