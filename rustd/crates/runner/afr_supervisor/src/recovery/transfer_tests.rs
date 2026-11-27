//! Authenticated objects cannot cross workspace, repository or object identity.

use super::transfer;
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointObject, CheckpointObjectStatus, CheckpointPlaintext};
use base64::Engine as _;
use zeroize::Zeroizing;

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const OTHER: &str = "019f0000-0000-7000-8000-000000000002";
const CONTENT: &[u8] = b"private conversation and files";

fn object() -> CheckpointObject {
    CheckpointObject {
        object_id: ID.to_owned(),
        repository: "owner/repository".to_owned(),
        plaintext: CheckpointPlaintext {
            sha256: af_core::etag::sha256_hex(CONTENT),
            size: CONTENT.len() as u64,
            kind: "history".to_owned(),
        },
        status: CheckpointObjectStatus::Missing,
        cipher: af_wire::durable_task::CheckpointCipher::default(),
        nonce: Some(base64::prelude::BASE64_STANDARD.encode([4; af_crypto::NONCE_LEN])),
        data_key: Some(base64::prelude::BASE64_STANDARD.encode([7; af_crypto::KEY_LEN])),
        ciphertext_sha256: None,
        ciphertext_length: None,
        transfer: None,
    }
}

fn registered(ciphertext: &[u8]) -> CheckpointObject {
    let mut object = object();
    object.ciphertext_sha256 = Some(af_core::etag::sha256_hex(ciphertext));
    object.ciphertext_length = Some(ciphertext.len() as u64);
    object
}

#[test]
fn ciphertext_authenticates_scope_even_when_its_transport_digest_matches() -> crate::Result<()> {
    let workspace = Uuid7::parse(ID)?;
    let mut source = object();
    let bytes = transfer::seal(&workspace, &mut source, Zeroizing::new(CONTENT.to_vec()))?;
    assert!(
        source.data_key.is_none(),
        "the consumed workspace key is not retained"
    );
    assert_eq!(
        transfer::open(&workspace, &mut registered(&bytes), bytes.clone())?.expose(),
        CONTENT
    );
    transfer::open(
        &Uuid7::parse(OTHER)?,
        &mut registered(&bytes),
        bytes.clone(),
    )
    .err()
    .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    let mut wrong_repository = registered(&bytes);
    wrong_repository.repository = "another/repository".to_owned();
    transfer::open(&workspace, &mut wrong_repository, bytes.clone())
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    let mut wrong_object = registered(&bytes);
    wrong_object.object_id = OTHER.to_owned();
    transfer::open(&workspace, &mut wrong_object, bytes.clone())
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    let mut corrupt = bytes;
    let last = corrupt
        .last_mut()
        .ok_or_else(|| crate::error::config("fixture ciphertext is empty"))?;
    *last ^= 1;
    transfer::open(&workspace, &mut registered(&corrupt), corrupt)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    Ok(())
}

#[test]
fn plaintext_and_transport_descriptors_cannot_be_substituted() -> crate::Result<()> {
    let workspace = Uuid7::parse(ID)?;
    transfer::seal(
        &workspace,
        &mut object(),
        Zeroizing::new(b"different content".to_vec()),
    )
    .err()
    .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    let bytes = transfer::seal(&workspace, &mut object(), Zeroizing::new(CONTENT.to_vec()))?;
    let mut truncated = registered(&bytes);
    truncated.ciphertext_length = Some(17);
    transfer::open(&workspace, &mut truncated, bytes.clone())
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    let mut wrong_plaintext = registered(&bytes);
    wrong_plaintext.plaintext.sha256 = "0".repeat(64);
    transfer::open(&workspace, &mut wrong_plaintext, bytes)
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    Ok(())
}

/// What a malformed descriptor names, and how it is malformed.
type Mutation = (&'static str, fn(&mut CheckpointObject));

#[test]
fn a_short_ciphertext_or_missing_or_malformed_key_material_never_opens() -> crate::Result<()> {
    let workspace = Uuid7::parse(ID)?;
    let short = vec![0; af_crypto::TAG_LEN - 1];
    let refused = transfer::open(&workspace, &mut registered(&short), short.clone())
        .err()
        .ok_or_else(|| crate::error::config("a tagless ciphertext was accepted"))?;
    assert_eq!(
        refused.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED,
        "{refused}"
    );

    let bytes = transfer::seal(&workspace, &mut object(), Zeroizing::new(CONTENT.to_vec()))?;
    let mutations: [Mutation; 5] = [
        ("checkpoint key is absent", |object| object.data_key = None),
        ("checkpoint key encoding is invalid", |object| {
            object.data_key = Some("not base64!".to_owned());
        }),
        ("nonce is absent", |object| object.nonce = None),
        ("nonce encoding is invalid", |object| {
            object.nonce = Some("not base64!".to_owned());
        }),
        ("nonce length is invalid", |object| {
            object.nonce = Some(base64::prelude::BASE64_STANDARD.encode([4; 3]));
        }),
    ];
    for (reason, mutate) in mutations {
        let mut descriptor = registered(&bytes);
        mutate(&mut descriptor);
        let refused = transfer::open(&workspace, &mut descriptor, bytes.clone())
            .err()
            .ok_or_else(|| crate::error::config("unusable key material was accepted"))?;
        assert!(refused.to_string().contains(reason), "{reason}: {refused}");
    }
    Ok(())
}

// A plain object is stored as its bytes with no tag, and its digests are
// checked exactly as an encrypted one's are.
#[test]
fn a_plain_object_is_stored_as_its_bytes_and_keeps_its_digest_checks() -> crate::Result<()> {
    let workspace = Uuid7::parse(ID)?;
    let plain = || {
        let mut object = object();
        object.cipher = af_wire::durable_task::CheckpointCipher::Plain;
        object
    };
    let stored = transfer::seal(&workspace, &mut plain(), Zeroizing::new(CONTENT.to_vec()))?;
    assert_eq!(stored, CONTENT);
    let mut registered = plain();
    registered.ciphertext_sha256 = Some(af_core::etag::sha256_hex(&stored));
    registered.ciphertext_length = Some(stored.len() as u64);
    assert_eq!(transfer::stored_length(&registered)?, stored.len() as u64);
    assert_eq!(
        transfer::open(&workspace, &mut registered.clone(), stored.clone())?.expose(),
        CONTENT
    );

    let mut tampered = stored.clone();
    let first = tampered
        .first_mut()
        .ok_or_else(|| crate::error::config("fixture content is empty"))?;
    *first ^= 1;
    transfer::open(&workspace, &mut registered.clone(), tampered.clone())
        .err()
        .ok_or_else(|| crate::error::config("bytes unlike their stored digest opened"))?;
    let mut redescribed = registered.clone();
    redescribed.ciphertext_sha256 = Some(af_core::etag::sha256_hex(&tampered));
    transfer::open(&workspace, &mut redescribed, tampered)
        .err()
        .ok_or_else(|| crate::error::config("bytes unlike their plaintext digest opened"))?;
    let mut keyless = registered.clone();
    keyless.data_key = None;
    transfer::open(&workspace, &mut keyless, stored)
        .err()
        .ok_or_else(|| crate::error::config("a plain object opened with no key handed over"))?;

    // An encrypted object is never shorter than a byte and its tag.
    let mut short = registered;
    short.cipher = af_wire::durable_task::CheckpointCipher::Aes256Gcm;
    short.ciphertext_length = Some(af_crypto::TAG_LEN as u64);
    transfer::stored_length(&short)
        .err()
        .ok_or_else(|| crate::error::config("a tag with no chunk has a valid length"))?;
    Ok(())
}
