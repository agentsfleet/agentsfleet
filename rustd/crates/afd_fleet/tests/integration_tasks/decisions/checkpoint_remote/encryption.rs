//! Exercise the same authenticated capture envelope as the trusted runner.

use super::super::super::fixture::{Case, id};
use afd_crypto::{
    checkpoint::{CheckpointScope, EncryptedCheckpointObject},
    secret::SecretBytes,
};
use afd_wire::task::CheckpointObject;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use zeroize::Zeroizing;

pub(super) fn encrypt(case: &Case, object: &CheckpointObject, plaintext: &[u8]) -> Vec<u8> {
    let nonce: [u8; afd_crypto::NONCE_LEN] = BASE64_STANDARD
        .decode(object.nonce.as_ref().expect("nonce"))
        .expect("nonce encoding")
        .try_into()
        .expect("nonce size");
    let key = SecretBytes::new(
        BASE64_STANDARD
            .decode(object.data_key.as_ref().expect("key"))
            .expect("key encoding"),
    );
    let encrypted = EncryptedCheckpointObject::seal(
        key,
        &nonce,
        CheckpointScope {
            workspace_id: &case.scope.workspace_id,
            repository: &object.repository,
            object_id: &id(&object.object_id),
            format_version: 1,
        },
        Zeroizing::new(plaintext.to_vec()),
    )
    .expect("trusted capture encryption");
    let (mut ciphertext, tag) = encrypted.into_parts();
    ciphertext.extend_from_slice(&tag);
    ciphertext
}
