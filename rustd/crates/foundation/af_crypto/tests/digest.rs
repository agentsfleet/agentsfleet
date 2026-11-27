//! SHA-256 is standard SHA-256, whole, in parts and streamed, and a streamed
//! hash covers exactly what its writer accepted.
//!
//! The expected digests are the FIPS 180-2 examples, transcribed rather than
//! computed, so a wrong primitive fails against a constant.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::io::{self, Write};

use af_crypto::constant_time;
use af_crypto::digest::{Sha256, Sha256Writer, sha256};

/// FIPS 180-2's one-block example.
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
/// FIPS 180-2's two-block example, and its message.
const TWO_BLOCK: &str = "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";
const TWO_BLOCK_MESSAGE: &[u8] = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
/// The empty message's digest.
const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn test_sha256_matches_the_published_examples() {
    assert_eq!(hex::encode(sha256(b"abc")), ABC);
    assert_eq!(hex::encode(sha256(TWO_BLOCK_MESSAGE)), TWO_BLOCK);
    assert_eq!(hex::encode(sha256(b"")), EMPTY);
}

#[test]
fn test_parts_hash_as_their_concatenation() {
    let mut hash = Sha256::new();
    for part in TWO_BLOCK_MESSAGE.chunks(7) {
        hash.update(part);
    }
    assert_eq!(hex::encode(hash.finish()), TWO_BLOCK);
    assert_eq!(hex::encode(Sha256::default().finish()), EMPTY);
}

#[test]
fn test_a_writer_over_a_sink_hashes_everything_copied() {
    let mut writer = Sha256Writer::new(io::sink());
    io::copy(&mut &*TWO_BLOCK_MESSAGE, &mut writer).unwrap();
    assert_eq!(hex::encode(writer.finish().0), TWO_BLOCK);
}

/// Accepts at most `per_write` bytes a call, and refuses everything once it
/// holds `capacity`.
struct Narrow {
    held: Vec<u8>,
    per_write: usize,
    capacity: usize,
}

impl Write for Narrow {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let room = self.capacity - self.held.len();
        if room == 0 {
            return Err(io::Error::other("full"));
        }
        let taken = buf.len().min(self.per_write).min(room);
        self.held.extend_from_slice(&buf[..taken]);
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_short_writes_hash_each_accepted_byte_once() {
    let mut writer = Sha256Writer::new(Narrow {
        held: Vec::new(),
        per_write: 2,
        capacity: usize::MAX,
    });
    writer.write_all(b"abc").unwrap();
    let (digest, inner) = writer.finish();
    assert_eq!(inner.held, b"abc");
    assert_eq!(hex::encode(digest), ABC);
}

#[test]
fn test_a_refused_write_hashes_only_what_reached_the_writer() {
    let mut writer = Sha256Writer::new(Narrow {
        held: Vec::new(),
        per_write: 5,
        capacity: 12,
    });
    assert!(writer.write_all(TWO_BLOCK_MESSAGE).is_err());
    let (digest, inner) = writer.finish();
    assert_eq!(inner.held, &TWO_BLOCK_MESSAGE[..12]);
    assert_eq!(digest, sha256(&TWO_BLOCK_MESSAGE[..12]));
}

#[test]
fn test_constant_time_equality_answers_equality() {
    assert!(constant_time::eq(b"digest", b"digest"));
    assert!(!constant_time::eq(b"digest", b"digesT"));
    assert!(!constant_time::eq(b"digest", b"diges"), "a short value");
    assert!(!constant_time::eq(b"digest", b"digest="), "a long value");
    assert!(constant_time::eq(b"", b""));
}
