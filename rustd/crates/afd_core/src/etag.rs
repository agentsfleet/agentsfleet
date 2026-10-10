//! The strong `ETag` over a resource's hashed surface.
//!
//! A quoted SHA-256 over an ordered field list, where each present field
//! contributes a marker, an eight-byte big-endian length and its bytes, and an
//! absent field contributes a distinct marker. Field boundaries, null and the
//! empty string therefore stay distinct for every byte sequence a caller can
//! author.
//!
//! # Why the encoding lives here rather than in the HTTP crate
//!
//! A client cache holds a tag across deployments and presents it back, so the
//! encoding is a WIRE fact — the same kind of fact as a
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
    let mut tag = String::with_capacity(2 + QUOTED_TAG_LEN);
    tag.push('"');
    tag.push_str(&hex::encode(hasher.finalize()));
    tag.push('"');
    tag
}

/// The quoted tag's hex half — SHA-256 is thirty-two bytes, two characters each.
const QUOTED_TAG_LEN: usize = 64;

/// A plain SHA-256 over `bytes`, lower-case hex.
///
/// Deliberately NOT [`compute`]'s encoding: no markers, no length prefixes, no
/// quotes. This one has to equal what Postgres computes for the same value —
/// `encode(sha256(convert_to(col, 'UTF8')), 'hex')` — because a conditional
/// write compares the two inside the `UPDATE`'s own predicate. A caller wanting
/// a validator to hand a client wants [`compute`] instead.
///
/// Both renderings here are `hex::encode`, lowercase, so [`compute`]'s quoted
/// form and this bare one cannot drift apart in width or case — either of
/// which would silently stop a client's cached validator from matching again.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{compute, sha256_hex};

    #[test]
    fn the_tag_is_a_quoted_sha256_hex_and_deterministic() {
        // sha256 of 0x01 ++ u64_be(5) ++ "hello", quoted. A client cache holds
        // this tag across deployments and presents it back, so the encoding
        // must not drift.
        let tag = compute(&[Some(b"hello")]);
        assert_eq!(tag.len(), 66, "quoted 64-hex form");
        assert!(tag.starts_with('"') && tag.ends_with('"'));
        assert_eq!(
            tag,
            compute(&[Some(b"hello")]),
            "deterministic across calls"
        );
    }

    /// A tag a client cached from an earlier build must still match, so the
    /// encoding is pinned to its bytes: markers, length prefixes, the absent
    /// field's marker, lowercase hex and the quotes.
    #[test]
    fn the_tag_is_the_zig_encoding_to_the_byte() {
        // pin test: literal is the contract
        assert_eq!(
            compute(&[Some(b"hello")]),
            "\"fdc54b20f1d3a02028207d0439220bea6c7f6807349996ee723d26bd52bbf8b5\""
        );
        // pin test: literal is the contract
        assert_eq!(
            compute(&[None, Some(b"")]),
            "\"13c6a7b85fcb0443c1d31dafe22561aac714fbaa99d3b9a56474d8dda0c9aee0\""
        );
    }

    #[test]
    fn the_column_digest_is_the_bare_hash_postgres_computes() {
        // Pinned against `encode(sha256(convert_to('hello','UTF8')),'hex')`. A
        // conditional write compares these two values inside the UPDATE, so a
        // change on either side has to fail HERE rather than by silently
        // matching no row and reading as a lost race.
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(sha256_hex(b"").len(), 64, "bare hex, no quotes or markers");
        assert_ne!(sha256_hex(b"hello"), compute(&[Some(b"hello")]));
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
