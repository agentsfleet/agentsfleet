//! Authenticated objects cannot cross workspace, repository or object identity.

use super::transfer;
use afd_core::id::Uuid7;
use afd_wire::task::{CheckpointObject, CheckpointObjectStatus, CheckpointPlaintext};
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
            sha256: afd_core::etag::sha256_hex(CONTENT),
            size: CONTENT.len() as u64,
            kind: "history".to_owned(),
        },
        status: CheckpointObjectStatus::Missing,
        nonce: Some(base64::prelude::BASE64_STANDARD.encode([4; afd_crypto::NONCE_LEN])),
        data_key: Some(base64::prelude::BASE64_STANDARD.encode([7; afd_crypto::KEY_LEN])),
        ciphertext_sha256: None,
        ciphertext_length: None,
        transfer: None,
    }
}

fn registered(ciphertext: &[u8]) -> CheckpointObject {
    let mut object = object();
    object.ciphertext_sha256 = Some(afd_core::etag::sha256_hex(ciphertext));
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
        "the consumed temporary key is not retained"
    );
    assert_eq!(
        transfer::open(&workspace, &mut registered(&bytes), bytes.clone())?.expose(),
        CONTENT
    );
    assert!(
        transfer::open(
            &Uuid7::parse(OTHER)?,
            &mut registered(&bytes),
            bytes.clone()
        )
        .is_err()
    );
    let mut wrong_repository = registered(&bytes);
    wrong_repository.repository = "another/repository".to_owned();
    assert!(transfer::open(&workspace, &mut wrong_repository, bytes.clone()).is_err());
    let mut wrong_object = registered(&bytes);
    wrong_object.object_id = OTHER.to_owned();
    assert!(transfer::open(&workspace, &mut wrong_object, bytes.clone()).is_err());
    let mut corrupt = bytes;
    let last = corrupt
        .last_mut()
        .ok_or_else(|| crate::error::config("fixture ciphertext is empty"))?;
    *last ^= 1;
    assert!(transfer::open(&workspace, &mut registered(&corrupt), corrupt).is_err());
    Ok(())
}

#[test]
fn plaintext_and_transport_descriptors_cannot_be_substituted() -> crate::Result<()> {
    let workspace = Uuid7::parse(ID)?;
    assert!(
        transfer::seal(
            &workspace,
            &mut object(),
            Zeroizing::new(b"different content".to_vec())
        )
        .is_err()
    );
    let bytes = transfer::seal(&workspace, &mut object(), Zeroizing::new(CONTENT.to_vec()))?;
    let mut truncated = registered(&bytes);
    truncated.ciphertext_length = Some(17);
    assert!(transfer::open(&workspace, &mut truncated, bytes.clone()).is_err());
    let mut wrong_plaintext = registered(&bytes);
    wrong_plaintext.plaintext.sha256 = "0".repeat(64);
    assert!(transfer::open(&workspace, &mut wrong_plaintext, bytes).is_err());
    Ok(())
}
