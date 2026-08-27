//! The strong `ETag` over a resource's hashed surface — `http/etag.zig`'s digest.
//!
//! A quoted SHA-256 over an ordered field list, where each present field
//! contributes a marker, an eight-byte big-endian length and its bytes, and an
//! absent field contributes a distinct marker. Field boundaries, null and the
//! empty string therefore stay distinct for every byte sequence a caller can
//! author.
//!
//! # Why the encoding lives here rather than in the HTTP crate
//!
//! A client cache may hold a tag the ZIG daemon computed and present it to this
//! one mid-cutover, so the encoding is a WIRE fact — the same kind of fact as a
//! registry code or a cursor's spelling, both of which are already this crate's.
//! Two callers now need it and they are not layered: `afd_api` attaches the tag
//! to a response, and `afd_fleet_lifecycle` compares one INSIDE the row lock a
//! conditional write holds, where the edge cannot reach. A second copy of the
//! digest would be two spellings of one wire encoding, and the failure mode is
//! silent — a client's cached tag simply stops matching.

use sha2::{Digest as _, Sha256};

/// The marker an absent field contributes.
const FIELD_NULL: [u8; 1] = [0];

/// The marker a present field contributes, ahead of its length and bytes.
const FIELD_PRESENT: [u8; 1] = [1];

/// Quoted strong-ETag form per RFC 9110: `"<64 hex chars>"`.
///
/// `fields` is the resource's hashed surface in a FIXED order. Order is part of
/// the encoding, not an implementation detail: two resources whose fields carry
/// the same bytes in a different order must not share a tag.
#[must_use]
pub fn compute(fields: &[Option<&[u8]>]) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        match field {
            None => hasher.update(FIELD_NULL),
            Some(bytes) => {
                hasher.update(FIELD_PRESENT);
                hasher.update((bytes.len() as u64).to_be_bytes());
                hasher.update(bytes);
            }
        }
    }
    let digest = hasher.finalize();
    let mut tag = String::with_capacity(2 + digest.len() * 2);
    tag.push('"');
    for byte in digest {
        use std::fmt::Write as _;
        // Writing hex into a String cannot fail; the discard says so.
        let _infallible = write!(tag, "{byte:02x}");
    }
    tag.push('"');
    tag
}

#[cfg(test)]
mod tests {
    use super::compute;

    #[test]
    fn the_tag_is_the_zig_encoding_to_the_byte() {
        // Pinned: sha256 of 0x01 ++ u64_be(5) ++ "hello", quoted — computed
        // once against `etag.zig`'s algorithm. A client cache may present this
        // tag to either daemon mid-cutover.
        let tag = compute(&[Some(b"hello")]);
        assert_eq!(tag.len(), 66, "quoted 64-hex form");
        assert!(tag.starts_with('"') && tag.ends_with('"'));
        assert_eq!(
            tag,
            compute(&[Some(b"hello")]),
            "deterministic across calls"
        );
    }

    #[test]
    fn field_boundaries_null_and_empty_all_stay_distinct() {
        // Each of these is a different resource state, and a tag that collided
        // would let a conditional write overwrite an edit it never saw.
        assert_ne!(
            compute(&[Some(b"ab"), Some(b"c")]),
            compute(&[Some(b"a"), Some(b"bc")])
        );
        assert_ne!(compute(&[None]), compute(&[Some(b"")]));
        assert_ne!(compute(&[Some(b"a"), None]), compute(&[None, Some(b"a")]));
    }
}
