//! Public checkpoint crypto: interoperability, ownership, corruption and failure.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    reason = "test target: an unmet precondition should fail the scenario"
)]

use af_core::id::Uuid7;
use af_crypto::checkpoint::{
    CheckpointScope, EncryptedCheckpointObject, PreparedCheckpointKey, WrappedCheckpointKey,
};
use af_crypto::entropy::Entropy;
use af_crypto::envelope::Sealer;
use af_crypto::secret::{Kek, SecretBytes};
use af_crypto::{KEY_LEN, NONCE_LEN};
use zeroize::Zeroizing;

struct Fixture {
    workspace: Uuid7,
    object: Uuid7,
    master: Kek,
}

impl Fixture {
    fn new() -> Self {
        Self {
            workspace: Uuid7::parse("019f01d0-0000-7000-8000-000000000001").unwrap(),
            object: Uuid7::parse("019f01d0-0000-7000-8000-000000000002").unwrap(),
            master: Kek::from_hex(&hex::encode([0x71; KEY_LEN])).unwrap(),
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

    fn key(&self) -> PreparedCheckpointKey {
        PreparedCheckpointKey::create(&Entropy::new(), &Sealer::new(), &self.master, self.scope())
            .unwrap()
    }
}

// Node 24.19.0 / OpenSSL 3.5.7 independently produced this complete format
// vector: zero AES-256 key, zero 96-bit nonce, sixteen zero plaintext bytes.
#[test]
fn object_bytes_match_the_external_vector_and_keep_the_owned_allocation() {
    let fixture = Fixture::new();
    let mut plaintext = Zeroizing::new(vec![0xa7; 64]);
    let initialized = plaintext.len();
    plaintext.get_mut(..16).unwrap().fill(0);
    plaintext.truncate(16);
    let allocation = plaintext.as_ptr();
    let encrypted = EncryptedCheckpointObject::seal(
        SecretBytes::new(vec![0; KEY_LEN]),
        &[0; NONCE_LEN],
        fixture.scope(),
        plaintext,
    )
    .unwrap();
    assert!(format!("{encrypted:?}").contains("ciphertext_bytes: 16"));
    let (bytes, tag) = encrypted.into_parts();
    assert_eq!(hex::encode(&bytes), "cea7403d4d606b6e074ec5d3baf39d18");
    assert_eq!(hex::encode(tag), "820a9e64c67f4f05d3a26f9c5a388670");
    assert_eq!(bytes.as_ptr(), allocation);
    assert!(bytes.capacity() >= initialized);
    // SAFETY: The original Vec initialized this whole range before truncation.
    // Its allocation is unchanged; sealing writes bytes without deinitializing.
    let retained = unsafe { std::slice::from_raw_parts(bytes.as_ptr(), initialized) };
    assert!(retained.get(16..).unwrap().iter().all(|byte| *byte == 0));
    let opened = EncryptedCheckpointObject::from_parts(bytes, tag)
        .open(
            SecretBytes::new(vec![0; KEY_LEN]),
            &[0; NONCE_LEN],
            fixture.scope(),
        )
        .unwrap();
    assert_eq!(opened.expose(), [0; 16]);
    assert_eq!(opened.expose().as_ptr(), allocation);
}

#[test]
fn scoped_keys_recover_empty_small_and_full_chunks_without_copying_plaintext() {
    let fixture = Fixture::new();
    for length in [0, 17, 8 * 1024 * 1024] {
        let prepared = fixture.key();
        assert_eq!(format!("{prepared:?}"), "PreparedCheckpointKey(redacted)");
        let (wrapped, key) = prepared.into_parts();
        let restored = wrapped.open(&fixture.master, fixture.scope()).unwrap();
        assert_eq!(restored.expose(), key.expose());
        let plaintext = Zeroizing::new(vec![0xa7; length]);
        let allocation = plaintext.as_ptr();
        let sealed =
            EncryptedCheckpointObject::seal(key, wrapped.nonce(), fixture.scope(), plaintext)
                .unwrap();
        let (ciphertext, tag) = sealed.into_parts();
        assert_eq!(ciphertext.len(), length);
        assert_eq!(ciphertext.as_ptr(), allocation);
        let reopened = EncryptedCheckpointObject::from_parts(ciphertext, tag)
            .open(restored, wrapped.nonce(), fixture.scope())
            .unwrap();
        assert_eq!(reopened.len(), length);
        assert!(reopened.expose().iter().all(|byte| *byte == 0xa7));
        assert_eq!(reopened.expose().as_ptr(), allocation);
        let rebuilt =
            WrappedCheckpointKey::from_parts(wrapped.envelope().clone(), *wrapped.nonce());
        assert_eq!(rebuilt, wrapped);
    }
    let first = fixture.key().into_parts();
    let second = fixture.key().into_parts();
    assert_ne!(first.0.nonce(), second.0.nonce());
    assert_ne!(first.1.expose(), second.1.expose());
}

fn assert_open_failed(fault: &af_crypto::error::Error) {
    assert!(fault.is_open_failed(), "unexpected fault: {fault}");
    assert_eq!(fault.code().as_str(), "UZ-INTERNAL-003");
}

#[test]
fn ownership_changes_and_ciphertext_corruption_never_return_plaintext() {
    let fixture = Fixture::new();
    let (wrapped, key) = fixture.key().into_parts();
    let scope = fixture.scope();
    let altered = Uuid7::parse("019f01d0-0000-7000-8000-000000000003").unwrap();
    let encrypted = EncryptedCheckpointObject::seal(
        key,
        wrapped.nonce(),
        scope,
        Zeroizing::new(b"private recovery history".to_vec()),
    )
    .unwrap();
    let (bytes, tag) = encrypted.into_parts();
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
        assert_open_failed(&wrapped.open(&fixture.master, changed).unwrap_err());
        let key = wrapped.open(&fixture.master, scope).unwrap();
        let result = EncryptedCheckpointObject::from_parts(bytes.clone(), tag).open(
            key,
            wrapped.nonce(),
            changed,
        );
        assert_open_failed(&result.unwrap_err());
    }
    assert_corruption(&fixture, &wrapped, &bytes, tag);
}

fn assert_corruption(
    fixture: &Fixture,
    wrapped: &WrappedCheckpointKey,
    bytes: &[u8],
    tag: [u8; 16],
) {
    let mut nonce = *wrapped.nonce();
    nonce[0] ^= 1;
    let altered = WrappedCheckpointKey::from_parts(wrapped.envelope().clone(), nonce);
    assert_open_failed(&altered.open(&fixture.master, fixture.scope()).unwrap_err());
    for (offset, damaged_tag, damaged_nonce) in [
        (Some(0), false, false),
        (None, true, false),
        (None, false, true),
    ] {
        let mut damaged = bytes.to_vec();
        let mut authentication = tag;
        if let Some(offset) = offset {
            *damaged.get_mut(offset).unwrap() ^= 1;
        }
        if damaged_tag {
            authentication[0] ^= 1;
        }
        let key = wrapped.open(&fixture.master, fixture.scope()).unwrap();
        let result = EncryptedCheckpointObject::from_parts(damaged, authentication).open(
            key,
            if damaged_nonce {
                &nonce
            } else {
                wrapped.nonce()
            },
            fixture.scope(),
        );
        assert_open_failed(&result.unwrap_err());
    }
    let vault = af_crypto::aad::Aad::new(fixture.workspace.as_str(), "checkpoint");
    assert_open_failed(
        &wrapped
            .envelope()
            .open(&fixture.master, &vault)
            .unwrap_err(),
    );
    let wrong = Kek::from_hex(&hex::encode([0x72; KEY_LEN])).unwrap();
    assert_open_failed(&wrapped.open(&wrong, fixture.scope()).unwrap_err());
}

#[test]
fn entropy_failures_and_malformed_transfer_keys_refuse_the_complete_operation() {
    let fixture = Fixture::new();
    let (entropy, control) = Entropy::new_mocked();
    control.fail_next();
    let result =
        PreparedCheckpointKey::create(&entropy, &Sealer::new(), &fixture.master, fixture.scope());
    let fault = result.unwrap_err();
    assert!(fault.is_entropy());
    assert_eq!(fault.code().as_str(), "UZ-INTERNAL-003");
    control.push_bytes(&[0x55; KEY_LEN]);
    control.push_bytes(&[0; NONCE_LEN - 1]);
    let result =
        PreparedCheckpointKey::create(&entropy, &Sealer::new(), &fixture.master, fixture.scope());
    let fault = result.unwrap_err();
    assert!(fault.is_malformed_envelope());
    assert_eq!(fault.code(), af_core::error_code::VAULT_DATA_INVALID);
    let (sealer, wrap_control) = Sealer::new_mocked();
    wrap_control.fail_next();
    let result =
        PreparedCheckpointKey::create(&Entropy::new(), &sealer, &fixture.master, fixture.scope());
    let fault = result.unwrap_err();
    assert!(fault.is_entropy());
    assert_eq!(fault.code().as_str(), "UZ-INTERNAL-003");
    assert_bad_transfer_keys(&fixture);
    assert_bad_wrapped_length(&fixture);
}

fn assert_bad_transfer_keys(fixture: &Fixture) {
    for length in [0, 31, 33] {
        let result = EncryptedCheckpointObject::seal(
            SecretBytes::new(vec![0; length]),
            &[0; NONCE_LEN],
            fixture.scope(),
            Zeroizing::new(vec![1; 16]),
        );
        let fault = result.unwrap_err();
        assert!(fault.is_malformed_envelope());
        assert_eq!(fault.code(), af_core::error_code::VAULT_DATA_INVALID);
        let result = EncryptedCheckpointObject::from_parts(vec![0; 16], [0; 16]).open(
            SecretBytes::new(vec![0; length]),
            &[0; NONCE_LEN],
            fixture.scope(),
        );
        assert!(result.unwrap_err().is_malformed_envelope());
    }
}

fn assert_bad_wrapped_length(fixture: &Fixture) {
    let bytes = concat!(
        "\0agentsfleet.checkpoint.v1\0key\0",
        "019f01d0-0000-7000-8000-000000000001\0",
        "6769746875623a7265706f3a313233\0",
        "019f01d0-0000-7000-8000-000000000002\0",
        "1\0",
        "2\0",
        "000000000000000000000000\0",
    );
    let binding = af_crypto::aad::Aad::from_bytes(bytes.as_bytes().to_vec());
    let envelope = Sealer::new()
        .seal(&fixture.master, &binding, &[0x91; 31])
        .unwrap();
    let wrapped = WrappedCheckpointKey::from_parts(envelope, [0; NONCE_LEN]);
    let fault = wrapped.open(&fixture.master, fixture.scope()).unwrap_err();
    assert!(fault.is_malformed_envelope());
    assert_eq!(fault.code(), af_core::error_code::VAULT_DATA_INVALID);
}
