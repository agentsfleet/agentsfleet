//! Public checkpoint crypto: one workspace key, two ciphers, ownership and corruption.
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the scenario"
)]

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_crypto::checkpoint::{Aes256Gcm, CheckpointScope, ChunkCipher, Plain, content_identity};
use af_crypto::checkpoint_digest::history_root;
use af_crypto::secret::SecretBytes;
use af_crypto::{KEY_LEN, NONCE_LEN, TAG_LEN};
use zeroize::Zeroizing;

struct Fixture {
    workspace: Uuid7,
    object: Uuid7,
}

impl Fixture {
    fn new() -> Self {
        Self {
            workspace: Uuid7::parse("019f01d0-0000-7000-8000-000000000001").unwrap(),
            object: Uuid7::parse("019f01d0-0000-7000-8000-000000000002").unwrap(),
        }
    }

    const fn scope(&self) -> CheckpointScope<'_> {
        CheckpointScope {
            workspace_id: &self.workspace,
            repository: "github:repo:123",
            object_id: &self.object,
            format_version: 1,
        }
    }

    fn history(&self) -> Aad {
        Aad::turn_recovery(&self.workspace, &self.object, &self.object, 7, 1)
    }
}

fn key(byte: u8) -> SecretBytes {
    SecretBytes::new(vec![byte; KEY_LEN])
}

fn assert_open_failed(fault: &af_crypto::error::Error) {
    assert!(fault.is_open_failed(), "unexpected fault: {fault}");
    assert_eq!(fault.code().as_str(), "UZ-INTERNAL-003");
}

#[test]
fn one_workspace_key_seals_chunks_and_history_and_keys_identity() {
    let fixture = Fixture::new();
    let (workspace_key, other_key) = (key(0x51), key(0x52));
    let chunk = fixture.scope().binding();
    let history = fixture.history();
    let nonce = [0x33; NONCE_LEN];
    for (binding, plaintext) in [(&chunk, b"file chunk".to_vec()), (&history, b"{}".to_vec())] {
        let sealed = Aes256Gcm
            .seal(
                &workspace_key,
                &nonce,
                binding,
                Zeroizing::new(plaintext.clone()),
            )
            .unwrap();
        assert_eq!(sealed.len(), plaintext.len() + TAG_LEN);
        assert_ne!(sealed.get(..plaintext.len()), Some(plaintext.as_slice()));
        let opened = Aes256Gcm
            .open(&workspace_key, &nonce, binding, sealed.clone())
            .unwrap();
        assert_eq!(opened.expose(), plaintext);
        assert_open_failed(
            &Aes256Gcm
                .open(&other_key, &nonce, binding, sealed)
                .unwrap_err(),
        );

        let passed = Plain
            .seal(
                &workspace_key,
                &nonce,
                binding,
                Zeroizing::new(plaintext.clone()),
            )
            .unwrap();
        assert_eq!(passed, plaintext);
        let returned = Plain.open(&workspace_key, &nonce, binding, passed).unwrap();
        assert_eq!(returned.expose(), plaintext);
    }
    // A chunk never opens as a history root.
    let sealed = Aes256Gcm
        .seal(&workspace_key, &nonce, &chunk, Zeroizing::new(vec![1; 8]))
        .unwrap();
    assert_open_failed(
        &Aes256Gcm
            .open(&workspace_key, &nonce, &history, sealed)
            .unwrap_err(),
    );
    // The identity turns on the key and the content, and no cipher enters it.
    let digest = "a".repeat(64);
    let identity = content_identity(&workspace_key, 1, &digest, 10);
    assert_eq!(identity, content_identity(&workspace_key, 1, &digest, 10));
    assert_eq!(identity.len(), 64);
    assert_ne!(identity, content_identity(&other_key, 1, &digest, 10));
    assert_ne!(identity, content_identity(&workspace_key, 2, &digest, 10));
    assert_ne!(identity, content_identity(&workspace_key, 1, &digest, 11));
    assert_ne!(
        identity,
        content_identity(&workspace_key, 1, &"b".repeat(64), 10)
    );
    assert_eq!((Aes256Gcm.overhead(), Plain.overhead()), (TAG_LEN, 0));
}

// Node 24.19.0 / OpenSSL 3.5.7 independently produced this complete format
// vector: zero AES-256 key, zero 96-bit nonce, sixteen zero plaintext bytes.
#[test]
fn object_bytes_match_the_external_vector_and_keep_the_owned_allocation() {
    let fixture = Fixture::new();
    let binding = fixture.scope().binding();
    let mut plaintext = Zeroizing::new(vec![0xa7; 64]);
    let initialized = plaintext.len();
    plaintext.get_mut(..16).unwrap().fill(0);
    plaintext.truncate(16);
    let allocation = plaintext.as_ptr();
    let sealed = Aes256Gcm
        .seal(&key(0), &[0; NONCE_LEN], &binding, plaintext)
        .unwrap();
    assert_eq!(
        hex::encode(&sealed),
        "cea7403d4d606b6e074ec5d3baf39d18820a9e64c67f4f05d3a26f9c5a388670"
    );
    assert_eq!(sealed.as_ptr(), allocation);
    assert!(sealed.capacity() >= initialized);
    // SAFETY: The original Vec initialized this whole range before truncation.
    // Its allocation is unchanged; sealing writes bytes without deinitializing.
    let retained = unsafe { std::slice::from_raw_parts(sealed.as_ptr(), initialized) };
    assert!(
        retained
            .get(32..)
            .is_some_and(|tail| tail.iter().all(|byte| *byte == 0))
    );
    let opened = Aes256Gcm
        .open(&key(0), &[0; NONCE_LEN], &binding, sealed)
        .unwrap();
    assert_eq!(opened.expose(), [0; 16]);
    assert_eq!(opened.expose().as_ptr(), allocation);
}

#[test]
fn both_ciphers_recover_empty_small_and_full_chunks_in_their_own_allocation() {
    let fixture = Fixture::new();
    let binding = fixture.scope().binding();
    let ciphers: [&dyn ChunkCipher; 2] = [&Aes256Gcm, &Plain];
    for cipher in ciphers {
        for length in [0, 17, 8 * 1024 * 1024] {
            let mut plaintext = Zeroizing::new(Vec::with_capacity(length + TAG_LEN));
            plaintext.resize(length, 0xa7);
            let allocation = plaintext.as_ptr();
            let sealed = cipher
                .seal(&key(0x51), &[9; NONCE_LEN], &binding, plaintext)
                .unwrap();
            assert_eq!(sealed.len(), length + cipher.overhead());
            assert_eq!(sealed.as_ptr(), allocation);
            let reopened = cipher
                .open(&key(0x51), &[9; NONCE_LEN], &binding, sealed)
                .unwrap();
            assert_eq!(reopened.len(), length);
            assert!(reopened.expose().iter().all(|byte| *byte == 0xa7));
            assert_eq!(reopened.expose().as_ptr(), allocation);
        }
    }
}

#[test]
fn ownership_changes_and_ciphertext_corruption_never_return_plaintext() {
    let fixture = Fixture::new();
    let scope = fixture.scope();
    let altered = Uuid7::parse("019f01d0-0000-7000-8000-000000000003").unwrap();
    let nonce = [4; NONCE_LEN];
    let sealed = Aes256Gcm
        .seal(
            &key(0x51),
            &nonce,
            &scope.binding(),
            Zeroizing::new(b"private recovery history".to_vec()),
        )
        .unwrap();
    let changes = [
        CheckpointScope {
            workspace_id: &altered,
            ..scope
        },
        CheckpointScope {
            object_id: &altered,
            ..scope
        },
        CheckpointScope {
            repository: "github:repo:123\0key",
            ..scope
        },
        CheckpointScope {
            repository: "github:répo:123",
            ..scope
        },
        CheckpointScope {
            format_version: 2,
            ..scope
        },
    ];
    for changed in changes {
        let result = Aes256Gcm.open(&key(0x51), &nonce, &changed.binding(), sealed.clone());
        assert_open_failed(&result.unwrap_err());
    }
    let vault = Aad::new(fixture.workspace.as_str(), "checkpoint");
    assert_open_failed(
        &Aes256Gcm
            .open(&key(0x51), &nonce, &vault, sealed.clone())
            .unwrap_err(),
    );
    let last = sealed.len() - 1;
    for (flipped, damaged_nonce) in [(Some(0), false), (Some(last), false), (None, true)] {
        let mut damaged = sealed.clone();
        // An offset past the end flips nothing, and the open below then succeeds and fails the test.
        if let Some(byte) = flipped.and_then(|offset| damaged.get_mut(offset)) {
            *byte ^= 1;
        }
        let mut used = nonce;
        if damaged_nonce {
            used[0] ^= 1;
        }
        let result = Aes256Gcm.open(&key(0x51), &used, &scope.binding(), damaged);
        assert_open_failed(&result.unwrap_err());
    }
    for short in [0, TAG_LEN - 1] {
        let result = Aes256Gcm.open(&key(0x51), &nonce, &scope.binding(), vec![0; short]);
        assert_open_failed(&result.unwrap_err());
    }
}

#[test]
fn a_key_of_another_length_is_refused_by_both_ciphers() {
    let fixture = Fixture::new();
    let binding = fixture.scope().binding();
    let ciphers: [&dyn ChunkCipher; 2] = [&Aes256Gcm, &Plain];
    for cipher in ciphers {
        for length in [0, 31, 33] {
            let short = SecretBytes::new(vec![0; length]);
            let fault = cipher
                .seal(
                    &short,
                    &[0; NONCE_LEN],
                    &binding,
                    Zeroizing::new(vec![1; 16]),
                )
                .unwrap_err();
            assert!(fault.is_malformed_envelope());
            assert_eq!(fault.code(), af_core::error_code::VAULT_DATA_INVALID);
            let fault = cipher
                .open(&short, &[0; NONCE_LEN], &binding, vec![0; 32])
                .unwrap_err();
            assert!(fault.is_malformed_envelope());
        }
    }
}

#[test]
fn conversation_identity_detects_reordering_truncation_and_boundary_changes() {
    let original = history_root([("chunk-a", "digest-a", 10), ("chunk-b", "digest-b", 20)]);
    for changed in [
        history_root([("chunk-b", "digest-b", 20), ("chunk-a", "digest-a", 10)]),
        history_root([("chunk-a", "digest-a", 10)]),
        history_root([("chunk-a", "digest-a", 11), ("chunk-b", "digest-b", 20)]),
        history_root([("chunk-a", "digest-z", 10), ("chunk-b", "digest-b", 20)]),
        history_root([("chunk-z", "digest-a", 10), ("chunk-b", "digest-b", 20)]),
    ] {
        assert_ne!(original, changed);
    }
    assert_ne!(
        history_root([("ab", "c", 1)]),
        history_root([("a", "bc", 1)])
    );
}
