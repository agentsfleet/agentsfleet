//! Native ciphertext readback, rather than a physical response, owns publication.

use super::*;
use crate::recovery::{Capture, Restore};
use crate::test_support::drain;
use base64::Engine as _;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

fn missing_registration(call: &Call, archive: &mut Archive) -> Answer {
    let Answer::Reply(bytes) = archive.respond(call) else {
        return Answer::Fail(crate::error::config("registration fixture refused"));
    };
    let mut page: CheckpointObjectsPage = serde_json::from_slice(&bytes).unwrap();
    for object in &mut page.items {
        object.status = CheckpointObjectStatus::Missing;
        object.nonce = Some(base64::prelude::BASE64_STANDARD.encode([4; afd_crypto::NONCE_LEN]));
        object.data_key = Some(base64::prelude::BASE64_STANDARD.encode([7; afd_crypto::KEY_LEN]));
        archive.objects.insert(
            object.object_id.clone(),
            serde_json::to_value(&object).unwrap(),
        );
    }
    json(&page)
}

#[tokio::test]
async fn already_received_ciphertext_is_verified_without_a_second_physical_upload()
-> crate::Result<()> {
    let (plane, mut calls, state) = remote(Archive::empty(), |call, archive| {
        (call.verb == Verb::TaskCheckpointObjectsRegister)
            .then(|| missing_registration(call, archive))
    });
    let identity = id();
    let task = task();
    let saved = Capture {
        plane: &plane,
        lease: &identity,
        workspace: &identity,
        fence: FENCE,
        task: &task,
        toolbox: &toolbox(),
        mode: CheckpointMode::Incremental,
        base: None,
        stop: &CancellationToken::new(),
    }
    .run(None, Zeroizing::new(b"private history".to_vec()), vec![])
    .await?;
    assert!(saved.manifest_root.is_some());
    let requests = drain(&mut calls);
    assert_verified_upload(&requests, &state);
    Ok(())
}

#[tokio::test]
async fn upload_response_cannot_replace_registered_ciphertext_or_skip_readback() -> crate::Result<()>
{
    for (fault, expected) in [
        ("missing", "checkpoint upload batch is incomplete"),
        ("identity", "checkpoint upload names another object"),
        ("repository", "checkpoint ciphertext authorization differs"),
        ("digest", "checkpoint ciphertext authorization differs"),
        ("length", "checkpoint ciphertext authorization differs"),
        (
            "unverified",
            "checkpoint upload batch was not fully verified",
        ),
        ("transfer", "checkpoint upload transfer is absent"),
    ] {
        let (plane, mut calls, state) = remote(Archive::empty(), move |call, archive| {
            upload_reply(call, archive, fault)
        });
        let identity = id();
        let task = task();
        let error = Capture {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            mode: CheckpointMode::Incremental,
            base: None,
            stop: &CancellationToken::new(),
        }
        .run(None, Zeroizing::new(b"private history".to_vec()), vec![])
        .await
        .unwrap_err();
        config_failure(&error, expected);
        assert!(state.lock().unwrap().plan.manifest_root.is_none());
        let requests = drain(&mut calls);
        assert!(
            !requests
                .iter()
                .any(|call| call.verb == Verb::TaskCheckpointCommit)
        );
        assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    }
    Ok(())
}

fn upload_reply(call: &Call, archive: &mut Archive, fault: &str) -> Option<Answer> {
    if call.verb == Verb::TaskCheckpointObjectsRegister {
        return Some(missing_registration(call, archive));
    }
    if call.verb == Verb::TaskCheckpointUploads {
        let Answer::Reply(bytes) = archive.respond(call) else {
            return Some(Answer::Fail(crate::error::config("upload fixture refused")));
        };
        let mut page: CheckpointObjectsPage = serde_json::from_slice(&bytes).unwrap();
        if fault == "missing" {
            page.items.clear();
        }
        for object in &mut page.items {
            match fault {
                "identity" => object.object_id = PAGE.to_owned(),
                "repository" => object.repository = "other/repository".to_owned(),
                "digest" => object.ciphertext_sha256 = Some("f".repeat(64)),
                "length" => object.ciphertext_length = Some(17),
                "transfer" => object.status = CheckpointObjectStatus::Missing,
                _ => {}
            }
        }
        return Some(json(&page));
    }
    if call.verb == Verb::TaskCheckpointAuthorizations && matches!(fault, "unverified" | "transfer")
    {
        return Some(json(&CheckpointAuthorizations {
            plan: archive.plan.clone(),
            objects: vec![],
        }));
    }
    None
}

#[tokio::test]
async fn history_download_requires_exact_object_scope_length_and_read_authority()
-> crate::Result<()> {
    for (fault, expected) in [
        (
            "missing",
            "checkpoint restore authorization differs from requested objects",
        ),
        (
            "repository",
            "checkpoint restore authorization differs from requested objects",
        ),
        ("length", "checkpoint ciphertext length is invalid"),
        ("authority", "checkpoint read authority is absent"),
    ] {
        let object_id = "019f0000-0000-7000-8000-000000000010";
        let mut archive = Archive::saved(vec![]);
        let page = archive.pages.get_mut(PAGE).unwrap();
        page.history_objects = vec![object_id.to_owned(), object_id.to_owned()];
        archive.plan.manifest_root = Some(root(PAGE, page));
        let (plane, mut calls, _) = remote(archive, move |call, archive| {
            history_reply(call, archive, fault, object_id)
        });
        let identity = id();
        let task = task();
        let error = Restore {
            plane: &plane,
            lease: &identity,
            workspace: &identity,
            fence: FENCE,
            task: &task,
            toolbox: &toolbox(),
            stop: &CancellationToken::new(),
        }
        .run(None)
        .await
        .unwrap_err();
        config_failure(&error, expected);
        let requests = drain(&mut calls);
        assert_eq!(
            requests
                .iter()
                .filter(|call| call.verb == Verb::TaskCheckpointAuthorizations)
                .count(),
            2
        );
        assert_eq!(phases(&requests).last().map(String::as_str), Some("failed"));
    }
    Ok(())
}

fn assert_verified_upload(requests: &[Call], state: &Arc<Mutex<Archive>>) {
    let upload: CheckpointUploadsInput = body(
        requests
            .iter()
            .find(|call| call.verb == Verb::TaskCheckpointUploads)
            .unwrap(),
    );
    assert_eq!(upload.objects.len(), 1);
    assert_eq!(
        upload.objects[0].ciphertext_length,
        15 + afd_crypto::TAG_LEN as u64
    );
    assert_eq!(upload.objects[0].ciphertext_sha256.len(), 64);
    let verified: CheckpointAuthorizationInput = body(
        requests
            .iter()
            .find(|call| call.verb == Verb::TaskCheckpointAuthorizations)
            .unwrap(),
    );
    assert_eq!(verified.object_ids, [upload.objects[0].object_id.clone()]);
    let object = state.lock().unwrap().objects[&upload.objects[0].object_id].clone();
    assert_eq!(
        object["transfer"],
        serde_json::Value::Null,
        "verified reuse needs no physical URL"
    );
    assert_eq!(phases(requests).last().map(String::as_str), Some("saved"));
}

fn history_reply(
    call: &Call,
    archive: &mut Archive,
    fault: &str,
    object_id: &str,
) -> Option<Answer> {
    if call.verb != Verb::TaskCheckpointAuthorizations {
        return None;
    }
    let input: CheckpointAuthorizationInput = body(call);
    if input.object_ids.is_empty() {
        return None;
    }
    assert_eq!(
        input.object_ids,
        [object_id],
        "repeated manifest chunks share one authorization"
    );
    let objects = if fault == "missing" {
        vec![]
    } else {
        vec![CheckpointObject {
            object_id: object_id.to_owned(),
            repository: if fault == "repository" {
                "other/repository".to_owned()
            } else {
                archive.plan.repository.clone()
            },
            plaintext: afd_wire::task::CheckpointPlaintext {
                sha256: "a".repeat(64),
                size: 1,
                kind: "history".to_owned(),
            },
            status: CheckpointObjectStatus::Reused,
            nonce: None,
            data_key: None,
            ciphertext_sha256: Some("b".repeat(64)),
            ciphertext_length: if fault == "length" {
                Some(16)
            } else {
                Some(17)
            },
            transfer: None,
        }]
    };
    Some(json(&CheckpointAuthorizations {
        plan: archive.plan.clone(),
        objects,
    }))
}
