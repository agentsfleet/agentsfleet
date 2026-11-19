//! Real encrypted file and root fixtures for the production supervisor entry path.
use af_core::{etag::sha256_hex, id::Uuid7};
use af_crypto::{
    checkpoint::{CheckpointScope, EncryptedCheckpointObject},
    checkpoint_digest,
    secret::SecretBytes,
};
use af_wire::{
    run::{
        RunPayload, TurnCheckpointAction as Action, TurnCheckpointCommand, TurnCheckpointRead,
        TurnCheckpointResponse as Response, TurnId, TurnPrivateRoot, TurnResume, TurnScope,
    },
    task::{
        CheckpointObject, CheckpointObjectStatus, CheckpointPage, CheckpointPin,
        CheckpointPlaintext,
    },
};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde_json::json;
use zeroize::Zeroizing;

use crate::{
    client::{Call, Verb},
    test_support::{self, Answer},
};

pub(super) const ID: &str = "019f0000-0000-7000-8000-000000000011";
const PAGE: &str = "019f0000-0000-7000-8000-000000000012";
const OBJECT: &str = "019f0000-0000-7000-8000-000000000013";
pub(super) const FILE: &str = "progress.txt";
pub(super) const CONTENT: &[u8] = b"saved workspace contents\n";
pub(super) const PREFIX: &str = "conversation saved before the interruption";
pub(super) type Checked<T = ()> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub(super) fn checkpoint(
    run: &RunPayload<'_>,
    scope: TurnScope,
    expiry: jiff::Timestamp,
    page: &CheckpointPage,
) -> Checked<TurnCheckpointRead> {
    let private = json!({
        "format":1,"scope":scope,"model":run.policy.context.model,"provider":run.policy.provider,
        "root":{"messages":[{"User":PREFIX}],"driver":{"answers":[],"completed_turns":0},
        "children":{"next":0,"started":0,"children":[]},
        "memory":{"pending":[],"superseded":[],"misses":0},"pinned":0,"folds":[]}
    })
    .to_string();
    let root = TurnPrivateRoot { history: private };
    let page_digest = sha256_hex(&serde_json::to_vec(page)?);
    Ok(TurnCheckpointRead {
        descriptor: TurnResume {
            checkpoint_id: TurnId(ID.to_owned()),
            generation: 1,
            format_version: 1,
            scope,
            manifest_root: checkpoint_digest::manifest_root([(PAGE, page_digest.as_str())]),
            history_digest: sha256_hex(root.history.as_bytes()),
        },
        pin: CheckpointPin {
            id: ID.to_owned(),
            expires_at: expiry,
            operation_deadline: expiry,
            resource_version: 1,
        },
        page_ids: vec![TurnId(PAGE.to_owned())],
        root,
        folds: Vec::new(),
    })
}

pub(super) fn answer(
    call: &Call,
    saved: &TurnCheckpointRead,
    page: &CheckpointPage,
    object: &CheckpointObject,
) -> Option<Answer> {
    if call.verb != Verb::TurnCheckpoint {
        return None;
    }
    let Some(body) = call.body.as_ref() else {
        return Some(Answer::Fail(crate::error::config(
            "checkpoint fixture command has no body",
        )));
    };
    let Ok(command) = serde_json::from_slice::<TurnCheckpointCommand>(body) else {
        return Some(Answer::Fail(crate::error::config(
            "checkpoint fixture command is malformed",
        )));
    };
    if matches!(command.action, Action::Begin(_)) {
        return Some(Answer::Fail(crate::error::refused(
            Verb::TurnCheckpoint,
            409,
            Some(af_core::error_code::RUN_CHECKPOINT_LIMIT),
        )));
    }
    let response = match command.action {
        Action::Read { .. } => Response::Read(Box::new(saved.clone())),
        Action::PageRead { .. } => Response::PageRead(page.clone()),
        Action::Authorize { .. } => Response::Authorized(vec![object.clone()]),
        Action::Adopt { .. } => Response::Adopted(Vec::new()),
        Action::PinRelease { .. } => Response::PinReleased,
        _ => {
            return Some(Answer::Fail(crate::error::config(
                "unexpected checkpoint fixture action",
            )));
        }
    };
    Some(test_support::json(&response))
}

pub(super) fn encrypted(scope: &TurnScope) -> Checked<(CheckpointObject, Vec<u8>)> {
    let namespace = format!("turn:{}", scope.event_id.0);
    let nonce = [7; af_crypto::NONCE_LEN];
    let key = [17; af_crypto::KEY_LEN];
    let encrypted = EncryptedCheckpointObject::seal(
        SecretBytes::new(key.to_vec()),
        &nonce,
        CheckpointScope {
            workspace_id: &Uuid7::parse(&scope.workspace_id.0)?,
            repository: &namespace,
            object_id: &Uuid7::parse(OBJECT)?,
            format_version: 1,
        },
        Zeroizing::new(CONTENT.to_vec()),
    )?;
    let (mut bytes, tag) = encrypted.into_parts();
    bytes.extend(tag);
    Ok((
        CheckpointObject {
            object_id: OBJECT.to_owned(),
            repository: namespace,
            status: CheckpointObjectStatus::Reused,
            plaintext: CheckpointPlaintext {
                size: crate::byte_count(CONTENT.len()),
                sha256: sha256_hex(CONTENT),
                kind: "file".to_owned(),
            },
            data_key: Some(BASE64_STANDARD.encode(key)),
            nonce: Some(BASE64_STANDARD.encode(nonce)),
            ciphertext_sha256: Some(sha256_hex(&bytes)),
            ciphertext_length: Some(crate::byte_count(bytes.len())),
            transfer: None,
        },
        bytes,
    ))
}
