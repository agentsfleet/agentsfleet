//! Ordered manifest and conversation identities shared by native custody and recovery.

use aws_lc_rs::digest::{Context, SHA256};

/// Binds each native page identity to its exact canonical bytes in order.
#[must_use]
pub fn manifest_root<'a>(pages: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut digest = Context::new(&SHA256);
    digest.update(b"\0agentsfleet.manifest.v1\0");
    for (id, page) in pages {
        field(&mut digest, id);
        field(&mut digest, page);
    }
    hex::encode(digest.finish())
}

/// Binds complete ordered history chunks without copying their plaintext again.
#[must_use]
pub fn history_root<'a>(chunks: impl IntoIterator<Item = (&'a str, &'a str, u64)>) -> String {
    let mut digest = Context::new(&SHA256);
    digest.update(b"\0agentsfleet.history.v1\0");
    for (id, plaintext, size) in chunks {
        field(&mut digest, id);
        field(&mut digest, plaintext);
        digest.update(&size.to_be_bytes());
    }
    hex::encode(digest.finish())
}

fn field(digest: &mut Context, field: &str) {
    digest.update(field.as_bytes());
    digest.update(&[0]);
}
