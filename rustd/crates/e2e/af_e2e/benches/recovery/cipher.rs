//! What sealing costs a save and opening costs a restore, with no datastore:
//! one chunk and a full checkpoint, stored plain and under two AES-256-GCM
//! implementations whose bytes are interchangeable.

use super::Checked;
use af_core::id::Uuid7;
use af_crypto::aad::Aad as Binding;
use af_crypto::checkpoint::{Aes256Gcm, CheckpointScope, ChunkCipher, Plain};
use af_crypto::secret::SecretBytes;
use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
use std::io::Write as _;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// One chunk: the largest plaintext a checkpoint object holds.
const CHUNK_BYTES: usize = 8 * 1024 * 1024;
/// A full checkpoint: the 1 GiB a task increment may upload, in chunks.
const CHECKPOINT_CHUNKS: usize = 128;
/// Runs per figure; the median is reported, so one slow run moves nothing.
const CHUNK_RUNS: usize = 15;
const CHECKPOINT_RUNS: usize = 3;
const KEY: [u8; af_crypto::KEY_LEN] = [0x5a; af_crypto::KEY_LEN];

/// One way to store a chunk, and to read it back.
trait Sealing {
    fn name(&self) -> &'static str;
    fn seal(&self, nonce: &[u8; 12], binding: &Binding, plaintext: Vec<u8>) -> Checked<Vec<u8>>;
    fn open(&self, nonce: &[u8; 12], binding: &Binding, sealed: Vec<u8>) -> Checked<Opened>;
}

/// An opened chunk, held as its cipher returned it: the timed call copies
/// nothing out, and the buffer is dropped after the clock stops.
enum Opened {
    Wiping(SecretBytes),
    Bytes(Vec<u8>),
}

impl Opened {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Wiping(bytes) => bytes.expose(),
            Self::Bytes(bytes) => bytes,
        }
    }
}

/// The daemon's own ciphers, through the trait a save and a restore call.
struct Shipped(&'static str, &'static dyn ChunkCipher);

impl Sealing for Shipped {
    fn name(&self) -> &'static str {
        self.0
    }

    fn seal(&self, nonce: &[u8; 12], binding: &Binding, plaintext: Vec<u8>) -> Checked<Vec<u8>> {
        let key = SecretBytes::new(KEY.to_vec());
        Ok(self
            .1
            .seal(&key, nonce, binding, Zeroizing::new(plaintext))?)
    }

    fn open(&self, nonce: &[u8; 12], binding: &Binding, sealed: Vec<u8>) -> Checked<Opened> {
        let key = SecretBytes::new(KEY.to_vec());
        Ok(Opened::Wiping(self.1.open(&key, nonce, binding, sealed)?))
    }
}

/// AES-256-GCM from `aws-lc-rs`, writing the same ciphertext and tag.
struct AwsLc;

impl AwsLc {
    fn key() -> Checked<LessSafeKey> {
        let key = UnboundKey::new(&AES_256_GCM, &KEY).map_err(|_key| "aws-lc-rs key")?;
        Ok(LessSafeKey::new(key))
    }
}

impl Sealing for AwsLc {
    fn name(&self) -> &'static str {
        "aes_256_gcm_aws_lc_rs"
    }

    fn seal(
        &self,
        nonce: &[u8; 12],
        binding: &Binding,
        mut plaintext: Vec<u8>,
    ) -> Checked<Vec<u8>> {
        Self::key()?
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(*nonce),
                Aad::from(binding.as_bytes()),
                &mut plaintext,
            )
            .map_err(|_seal| "aws-lc-rs seal")?;
        Ok(plaintext)
    }

    fn open(&self, nonce: &[u8; 12], binding: &Binding, mut sealed: Vec<u8>) -> Checked<Opened> {
        let length = Self::key()?
            .open_in_place(
                Nonce::assume_unique_for_key(*nonce),
                Aad::from(binding.as_bytes()),
                &mut sealed,
            )
            .map_err(|_open| "aws-lc-rs open")?
            .len();
        sealed.truncate(length);
        Ok(Opened::Bytes(sealed))
    }
}

/// Prints one line per cipher and object: the median time to seal it and to
/// open it, and the rate each is.
pub(super) fn report() -> Checked {
    let workspace = Uuid7::parse("019f0000-0000-7000-8000-000000000001")?;
    let object = Uuid7::parse("019f0000-0000-7000-8000-000000000002")?;
    let scope = CheckpointScope {
        workspace_id: &workspace,
        repository: "bench:recovery",
        object_id: &object,
        format_version: 1,
    };
    let binding = &scope.binding();
    interchangeable(binding)?;
    let ciphers: [&dyn Sealing; 3] = [
        &Shipped("plain", &Plain),
        &Shipped("aes_256_gcm_rustcrypto", &Aes256Gcm),
        &AwsLc,
    ];
    for cipher in ciphers {
        line(cipher, "chunk", 1, CHUNK_RUNS, binding)?;
        line(
            cipher,
            "checkpoint",
            CHECKPOINT_CHUNKS,
            CHECKPOINT_RUNS,
            binding,
        )?;
    }
    Ok(())
}

/// The two AES implementations write one stored form: either opens the other's.
fn interchangeable(binding: &Binding) -> Checked {
    let plaintext = content(0, 4096);
    let nonce = [9; 12];
    let rust = Shipped("aes_256_gcm_rustcrypto", &Aes256Gcm);
    let ours = rust.seal(&nonce, binding, plaintext.clone())?;
    let theirs = AwsLc.seal(&nonce, binding, plaintext.clone())?;
    if ours != theirs
        || AwsLc.open(&nonce, binding, ours)?.bytes() != plaintext
        || rust.open(&nonce, binding, theirs)?.bytes() != plaintext
    {
        return Err("the two AES-256-GCM implementations disagree".into());
    }
    Ok(())
}

fn line(
    cipher: &dyn Sealing,
    object: &str,
    chunks: usize,
    runs: usize,
    binding: &Binding,
) -> Checked {
    let (mut seals, mut opens) = (Vec::new(), Vec::new());
    for _run in 0..runs {
        let (seal, open) = once(cipher, chunks, binding)?;
        seals.push(seal);
        opens.push(open);
    }
    let bytes = chunks * CHUNK_BYTES;
    let (seal, open) = (median(&mut seals), median(&mut opens));
    writeln!(
        std::io::stdout(),
        "{}",
        serde_json::json!({
            "shape": "recovery_cipher", "cipher": cipher.name(), "object": object,
            "bytes": bytes, "runs": runs,
            "seal_ms": millis(seal), "open_ms": millis(open),
            "seal_mib_per_s": rate(bytes, seal), "open_mib_per_s": rate(bytes, open),
        })
    )?;
    Ok(())
}

/// Seals then opens `chunks` chunks one at a time, as a save and a restore
/// do, each under its own nonce; only the cipher's own work is timed.
fn once(cipher: &dyn Sealing, chunks: usize, binding: &Binding) -> Checked<(Duration, Duration)> {
    let (mut seal, mut open) = (Duration::ZERO, Duration::ZERO);
    for index in 0..chunks {
        let plaintext = content(index, CHUNK_BYTES);
        let mut nonce = [0_u8; 12];
        nonce
            .iter_mut()
            .zip(index.to_be_bytes())
            .for_each(|(byte, value)| *byte = value);
        let started = Instant::now();
        let sealed = cipher.seal(&nonce, binding, plaintext)?;
        seal += started.elapsed();
        let started = Instant::now();
        let opened = cipher.open(&nonce, binding, sealed)?;
        open += started.elapsed();
        let opened = opened.bytes();
        if opened.len() != CHUNK_BYTES || opened.first() != Some(&byte_of(index)) {
            return Err("a chunk did not open to what was sealed".into());
        }
    }
    Ok((seal, open))
}

/// A chunk with spare room for a tag, so no cipher is charged a reallocation.
fn content(index: usize, bytes: usize) -> Vec<u8> {
    let mut content = Vec::with_capacity(bytes + af_crypto::TAG_LEN);
    content.resize(bytes, byte_of(index));
    content
}

fn byte_of(index: usize) -> u8 {
    index.to_le_bytes()[0] ^ 0xa7
}

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples
        .get(samples.len() / 2)
        .copied()
        .unwrap_or(Duration::ZERO)
}

fn millis(elapsed: Duration) -> f64 {
    (elapsed.as_secs_f64() * 1_000_000.0).round() / 1000.0
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a byte count far below 2^52 reported as a rate"
)]
fn rate(bytes: usize, elapsed: Duration) -> f64 {
    let mib = bytes as f64 / (1024.0 * 1024.0);
    (mib / elapsed.as_secs_f64().max(f64::EPSILON)).round()
}
