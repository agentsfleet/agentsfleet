//! Ordered manifest and conversation identities shared by native custody and recovery.

use sha2::{Digest as _, Sha256};

/// Binds each native page identity to its exact canonical bytes in order.
#[must_use]
pub fn manifest_root<'a>(pages: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"\0agentsfleet.manifest.v1\0");
    for (id, page) in pages { field(&mut digest, id); field(&mut digest, page); }
    hex::encode(digest.finalize())
}

/// Binds complete ordered history chunks without copying their plaintext again.
#[must_use]
pub fn history_root<'a>(chunks: impl IntoIterator<Item = (&'a str, &'a str, u64)>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"\0agentsfleet.history.v1\0");
    for (id, plaintext, size) in chunks {
        field(&mut digest, id); field(&mut digest, plaintext); digest.update(size.to_be_bytes());
    }
    hex::encode(digest.finalize())
}

fn field(digest: &mut Sha256, field: &str) { digest.update(field.as_bytes()); digest.update([0]); }
