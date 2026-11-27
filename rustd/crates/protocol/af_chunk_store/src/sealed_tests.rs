//! Authenticated objects cannot cross workspace, repository or object
//! identity, and a restore reads only what was registered.

use std::collections::BTreeMap;

use af_core::id::Uuid7;
use af_crypto::checkpoint::CheckpointScope;
use af_crypto::secret::SecretBytes;
use af_wire::durable_task::{
    CheckpointCipher, CheckpointObject, CheckpointObjectStatus, CheckpointPlaintext,
};
use base64::Engine as _;
use zeroize::Zeroizing;

use super::{open, seal, stored_length};

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const OTHER: &str = "019f0000-0000-7000-8000-000000000002";
const CONTENT: &[u8] = b"private conversation and files";
const NONCE: [u8; af_crypto::NONCE_LEN] = [4; af_crypto::NONCE_LEN];
const KEY: [u8; af_crypto::KEY_LEN] = [7; af_crypto::KEY_LEN];

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
        cipher: CheckpointCipher::default(),
        nonce: Some(base64::prelude::BASE64_STANDARD.encode(NONCE)),
        data_key: Some(base64::prelude::BASE64_STANDARD.encode(KEY)),
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

/// `result` refused; the test fails naming `what` when it was accepted.
fn refused<T>(result: crate::Result<T>, what: &str) -> Checked<crate::Error> {
    result
        .err()
        .ok_or_else(|| format!("{what} was accepted").into())
}

#[test]
fn ciphertext_authenticates_scope_even_when_its_transport_digest_matches() -> Checked {
    let workspace = Uuid7::parse(ID)?;
    let mut source = object();
    let bytes = seal(&workspace, &mut source, Zeroizing::new(CONTENT.to_vec()))?;
    assert!(
        source.data_key.is_none(),
        "the consumed workspace key is not retained"
    );
    assert_eq!(
        open(&workspace, &mut registered(&bytes), bytes.clone())?.expose(),
        CONTENT
    );
    let other = Uuid7::parse(OTHER)?;
    refused(
        open(&other, &mut registered(&bytes), bytes.clone()),
        "another workspace",
    )?;
    let mut wrong_repository = registered(&bytes);
    wrong_repository.repository = "another/repository".to_owned();
    refused(
        open(&workspace, &mut wrong_repository, bytes.clone()),
        "another repository",
    )?;
    let mut wrong_object = registered(&bytes);
    wrong_object.object_id = OTHER.to_owned();
    refused(
        open(&workspace, &mut wrong_object, bytes.clone()),
        "another object",
    )?;
    let mut corrupt = bytes;
    let last = corrupt.last_mut().ok_or("fixture ciphertext is empty")?;
    *last ^= 1;
    refused(
        open(&workspace, &mut registered(&corrupt), corrupt),
        "a flipped tag",
    )?;
    Ok(())
}

#[test]
fn plaintext_and_transport_descriptors_cannot_be_substituted() -> Checked {
    let workspace = Uuid7::parse(ID)?;
    refused(
        seal(
            &workspace,
            &mut object(),
            Zeroizing::new(b"different content".to_vec()),
        ),
        "unregistered plaintext",
    )?;
    let bytes = seal(&workspace, &mut object(), Zeroizing::new(CONTENT.to_vec()))?;
    let mut truncated = registered(&bytes);
    truncated.ciphertext_length = Some(17);
    refused(
        open(&workspace, &mut truncated, bytes.clone()),
        "a changed length",
    )?;
    let mut wrong_plaintext = registered(&bytes);
    wrong_plaintext.plaintext.sha256 = "0".repeat(64);
    refused(
        open(&workspace, &mut wrong_plaintext, bytes),
        "a changed digest",
    )?;
    Ok(())
}

/// What a malformed descriptor names, and how it is malformed.
type Mutation = (&'static str, fn(&mut CheckpointObject));

#[test]
fn a_short_ciphertext_or_missing_or_malformed_key_material_never_opens() -> Checked {
    let workspace = Uuid7::parse(ID)?;
    let short = vec![0; af_crypto::TAG_LEN - 1];
    let failure = refused(
        open(&workspace, &mut registered(&short), short.clone()),
        "a tagless ciphertext",
    )?;
    assert_eq!(
        failure.code(),
        af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE,
        "{failure}"
    );

    let bytes = seal(&workspace, &mut object(), Zeroizing::new(CONTENT.to_vec()))?;
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
        let failure = refused(
            open(&workspace, &mut descriptor, bytes.clone()),
            "unusable key material",
        )?;
        assert!(failure.to_string().contains(reason), "{reason}: {failure}");
    }
    Ok(())
}

// A plain object is stored as its bytes with no tag, and its digests are
// checked exactly as an encrypted one's are.
#[test]
fn a_plain_object_is_stored_as_its_bytes_and_keeps_its_digest_checks() -> Checked {
    let workspace = Uuid7::parse(ID)?;
    let plain = || {
        let mut object = object();
        object.cipher = CheckpointCipher::Plain;
        object
    };
    let stored = seal(&workspace, &mut plain(), Zeroizing::new(CONTENT.to_vec()))?;
    assert_eq!(stored, CONTENT);
    let mut registered = plain();
    registered.ciphertext_sha256 = Some(af_core::etag::sha256_hex(&stored));
    registered.ciphertext_length = Some(stored.len() as u64);
    assert_eq!(stored_length(&registered)?, stored.len() as u64);
    assert_eq!(
        open(&workspace, &mut registered.clone(), stored.clone())?.expose(),
        CONTENT
    );

    let mut tampered = stored.clone();
    let first = tampered.first_mut().ok_or("fixture content is empty")?;
    *first ^= 1;
    refused(
        open(&workspace, &mut registered.clone(), tampered.clone()),
        "bytes unlike their stored digest",
    )?;
    let mut redescribed = registered.clone();
    redescribed.ciphertext_sha256 = Some(af_core::etag::sha256_hex(&tampered));
    refused(
        open(&workspace, &mut redescribed, tampered),
        "bytes unlike their plaintext digest",
    )?;
    let mut keyless = registered.clone();
    keyless.data_key = None;
    refused(
        open(&workspace, &mut keyless, stored),
        "a plain object with no key handed over",
    )?;

    // An encrypted object is never shorter than a byte and its tag.
    let mut short = registered;
    short.cipher = CheckpointCipher::Aes256Gcm;
    short.ciphertext_length = Some(af_crypto::TAG_LEN as u64);
    refused(stored_length(&short), "a tag with no chunk")?;
    Ok(())
}

/// A restore's view of one workspace: the files its last checkpoint wrote,
/// and the objects a bucket holds by identifier.
struct Restore {
    files: BTreeMap<String, Vec<u8>>,
    bucket: BTreeMap<String, Vec<u8>>,
}

impl Restore {
    /// Writes `path` from the object `registration` names, through the read
    /// check; a refused object writes nothing.
    fn write(
        &mut self,
        workspace: &Uuid7,
        path: &str,
        mut registration: CheckpointObject,
    ) -> Checked {
        let stored = self
            .bucket
            .get(&registration.object_id)
            .cloned()
            .ok_or("the bucket holds no such object")?;
        let plaintext = open(workspace, &mut registration, stored)?;
        self.files
            .insert(path.to_owned(), plaintext.expose().to_vec());
        Ok(())
    }
}

#[test]
fn a_restore_refuses_an_object_unlike_its_registration() -> Checked {
    let workspace = Uuid7::parse(ID)?;
    let previous = b"the previous checkpoint's notes".to_vec();
    // The runner registered CONTENT, then sealed and uploaded other bytes
    // under the same key and nonce: the tag authenticates what it stored.
    let uploaded = b"bytes the runner never registered".to_vec();
    let id = Uuid7::parse(ID)?;
    let binding = CheckpointScope {
        workspace_id: &workspace,
        repository: "owner/repository",
        object_id: &id,
        format_version: super::FORMAT_VERSION,
    }
    .binding();
    let cipher = super::cipher(CheckpointCipher::Aes256Gcm);
    let key = SecretBytes::new(KEY.to_vec());
    let stored = cipher.seal(&key, &NONCE, &binding, Zeroizing::new(uploaded.clone()))?;
    let mut restore = Restore {
        files: BTreeMap::from([("notes.md".to_owned(), previous.clone())]),
        bucket: BTreeMap::from([(ID.to_owned(), stored.clone())]),
    };

    // A check of the tag alone accepts it.
    let opened = cipher.open(&key, &NONCE, &binding, stored.clone())?;
    assert_eq!(opened.expose(), uploaded.as_slice());
    let failure = restore
        .write(&workspace, "notes.md", registered(&stored))
        .err()
        .ok_or("an object unlike its registration was written")?;

    assert!(
        failure
            .to_string()
            .contains("plaintext authentication mismatch"),
        "{failure}"
    );
    assert_eq!(
        restore.files,
        BTreeMap::from([("notes.md".to_owned(), previous)]),
        "the previous checkpoint's file stays as it was"
    );
    Ok(())
}
