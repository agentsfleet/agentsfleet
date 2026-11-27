//! An object another capture is uploading is reported busy without its key, frozen
//! descriptors refuse a second spelling, and a commit verifies every object it names.

use af_core::{error_code, etag::sha256_hex};
use af_crypto::{checkpoint::CheckpointScope, checkpoint_digest, secret::SecretBytes};
use af_wire::durable_task::{
    CheckpointAuthorizationInput, CheckpointCommitInput, CheckpointEntry, CheckpointFile,
    CheckpointObject, CheckpointObjectStatus, CheckpointObjectsInput, CheckpointPlaintext,
    CheckpointPlan, CheckpointTransferPurpose, CheckpointUpload, CheckpointUploadsInput,
    DecisionDurableTaskVersion,
};
use afd_fleet::durable_task::storage::chunk_cipher;
use afd_fleet::durable_task::{
    DurableTaskLifecycle as _, DurableTasks,
    storage::{CheckpointStorageConfig, R2CheckpointStore},
};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use zeroize::Zeroizing;

use super::{
    super::{
        super::fixture::{Case, exercise, id},
        checkpoint::valid,
        checkpoint_remote::configured,
        checkpoint_server::{HOST, Server},
        fixture::Decisions,
    },
    api_only, incremental, lapse, page, planned, refused, write,
};

const RESERVED: &[u8] = b"reserved by an abandoned capture";
const HISTORY: &[u8] = b"conversation history chunk";
const UNSENT: &[u8] = b"history never uploaded";
const FILE: &[u8] = b"a file never uploaded";
const DECLARED: &[u8] = b"declared plaintext";
const REPLACED: &[u8] = b"replaced plaintext";

// Value: protects=a capture never receives the key of an object another capture
// reserved and never uploaded,
// frozen ciphertext descriptors accept only their own spelling, and a commit publishes
// history only when every chunk it names is verified and digested as claimed;
// fails_when=the busy projection leaks key material, the descriptor digest check is
// dropped, a remote miss stops leaving the object missing, storage moved to another
// bucket opens an old object, a decrypted plaintext is not compared with its
// registration, a stored nonce of the wrong width is used, or the history-root,
// verified-history or complete-object refusal is dropped;
// why_new=these object arms had no case; seam=the production task verbs over live
// Postgres and real SDK requests to the private HTTPS fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_busy_object_carries_no_key_and_a_commit_names_only_verified_objects() {
    exercise(objects).await;
}

async fn objects(case: &Case) {
    let server = Server::start().await;
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    api_only(case, &task).await;
    let tasks = configured(&fixture, &server).await;
    let earlier = capture(&tasks, case, &task).await;
    let reserved = register(&tasks, case, &task, &earlier, RESERVED).await;
    assert!(matches!(reserved.status, CheckpointObjectStatus::Missing));
    assert!(
        reserved.data_key.is_some(),
        "the uploading owner holds the key"
    );
    lapse(case, &earlier).await;

    let owner = capture(&tasks, case, &task).await;
    let busy = register(&tasks, case, &task, &owner, RESERVED).await;
    assert_eq!(busy.object_id, reserved.object_id, "one object per content");
    assert!(matches!(busy.status, CheckpointObjectStatus::Busy));
    assert_eq!(
        (
            &busy.nonce,
            &busy.data_key,
            &busy.ciphertext_sha256,
            busy.ciphertext_length
        ),
        (&None, &None, &None, None),
        "a busy object carries no key material and no descriptor"
    );
    assert!(busy.transfer.is_none());
    assert_eq!(busy.plaintext.size, RESERVED.len() as u64);

    let chunk = register(&tasks, case, &task, &owner, HISTORY).await;
    assert!(matches!(chunk.status, CheckpointObjectStatus::Missing));

    let ciphertext = encrypt(case, &chunk, HISTORY);
    descriptors(&tasks, case, &task, &owner, &chunk, &ciphertext, &server).await;
    moved(&fixture, &server, case, &task, &owner, &chunk).await;
    history(&tasks, case, &task, &owner, &chunk).await;
    unverified(&tasks, case, &task).await;
    substituted(&tasks, case, &task, &server).await;
    fixture.model.finish().await.expect("fixture cleanup");
    server.finish().await;
}

/// A one-page overlay capture with room for one small upload.
async fn capture(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
) -> CheckpointPlan {
    let mut request = incremental(case, task, 1);
    request.upload_budget_bytes = 1024;
    planned(tasks, case, request).await
}

async fn register(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    plaintext: &[u8],
) -> CheckpointObject {
    tasks
        .register_checkpoint_objects(
            case.run(),
            &id(&plan.checkpoint_id),
            &valid(CheckpointObjectsInput {
                fencing_token: case.run().fencing_token,
                durable_task: task.clone(),
                pin_id: plan.pin.id.clone(),
                objects: vec![CheckpointPlaintext {
                    sha256: sha256_hex(plaintext),
                    size: plaintext.len() as u64,
                    kind: "history".into(),
                }],
            }),
        )
        .await
        .expect("registration")
        .items
        .into_iter()
        .next()
        .expect("one object")
}

/// The ciphertext a trusted runner uploads for `object`.
fn encrypt(case: &Case, object: &CheckpointObject, plaintext: &[u8]) -> Vec<u8> {
    let nonce: [u8; af_crypto::NONCE_LEN] = BASE64_STANDARD
        .decode(object.nonce.as_ref().expect("nonce"))
        .expect("nonce encoding")
        .try_into()
        .expect("nonce size");
    let key = SecretBytes::new(
        BASE64_STANDARD
            .decode(object.data_key.as_ref().expect("key"))
            .expect("key encoding"),
    );
    let scope = CheckpointScope {
        workspace_id: &case.scope.workspace_id,
        repository: &object.repository,
        object_id: &id(&object.object_id),
        format_version: 1,
    };
    chunk_cipher(object.cipher)
        .seal(
            &key,
            &nonce,
            &scope.binding(),
            Zeroizing::new(plaintext.to_vec()),
        )
        .expect("capture encryption")
}

fn uploads(
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
    digest: String,
    length: u64,
) -> garde::Valid<CheckpointUploadsInput> {
    valid(CheckpointUploadsInput {
        fencing_token: case.run().fencing_token,
        durable_task: task.clone(),
        pin_id: plan.pin.id.clone(),
        objects: vec![CheckpointUpload {
            object_id: object.object_id.clone(),
            ciphertext_sha256: digest,
            ciphertext_length: length,
        }],
    })
}

async fn descriptors(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
    ciphertext: &[u8],
    server: &Server,
) {
    let checkpoint = id(&plan.checkpoint_id);
    let digest = sha256_hex(ciphertext);
    let length = ciphertext.len() as u64;
    let first = tasks
        .checkpoint_uploads(
            case.run(),
            &checkpoint,
            &uploads(case, task, plan, object, digest.clone(), length),
        )
        .await
        .expect("the descriptor freezes");
    let again = tasks
        .checkpoint_uploads(
            case.run(),
            &checkpoint,
            &uploads(case, task, plan, object, digest.clone(), length),
        )
        .await
        .expect("the same descriptor replays");
    let first = first.items.first().expect("one upload").clone();
    let again = again.items.first().expect("one upload").clone();
    assert_eq!(again.ciphertext_sha256, first.ciphertext_sha256);
    assert_eq!(again.ciphertext_length, Some(length));
    refused(
        tasks
            .checkpoint_uploads(
                case.run(),
                &checkpoint,
                &uploads(case, task, plan, object, "f".repeat(64), length),
            )
            .await,
        error_code::DURABLE_TASK_REPLAY_CONFLICT,
        "a frozen descriptor refuses another ciphertext digest",
    );

    let missing = tasks
        .checkpoint_authorizations(case.run(), &checkpoint, &confirm(case, task, plan, object))
        .await
        .expect("an object not yet uploaded stays missing");
    let missing = missing.objects.first().expect("one object");
    assert!(matches!(missing.status, CheckpointObjectStatus::Missing));
    assert!(
        missing.transfer.is_some(),
        "and is offered its upload again"
    );

    let transfer = first.transfer.expect("an upload URL");
    let mut put = server.client.put(&transfer.url).body(ciphertext.to_vec());
    for (name, value) in &transfer.headers {
        put = put.header(name, value);
    }
    assert_eq!(put.send().await.expect("authorized PUT").status(), 200);
    let confirmed = tasks
        .checkpoint_authorizations(case.run(), &checkpoint, &confirm(case, task, plan, object))
        .await
        .expect("the uploaded ciphertext verifies");
    assert!(matches!(
        confirmed.objects.first().expect("one object").status,
        CheckpointObjectStatus::Reused
    ));
}

/// The writer's authorization of `object`, which verifies an uploaded ciphertext.
fn confirm(
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
) -> garde::Valid<CheckpointAuthorizationInput> {
    valid(CheckpointAuthorizationInput {
        fencing_token: case.run().fencing_token,
        durable_task: task.clone(),
        purpose: CheckpointTransferPurpose::Upload,
        pin_id: Some(plan.pin.id.clone()),
        object_ids: vec![object.object_id.clone()],
    })
}

/// Uploads `ciphertext` for `object` through its own signed URL.
async fn put(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
    ciphertext: &[u8],
    server: &Server,
) {
    let signed = tasks
        .checkpoint_uploads(
            case.run(),
            &id(&plan.checkpoint_id),
            &uploads(
                case,
                task,
                plan,
                object,
                sha256_hex(ciphertext),
                ciphertext.len() as u64,
            ),
        )
        .await
        .expect("the descriptor freezes");
    let transfer = signed
        .items
        .first()
        .and_then(|item| item.transfer.clone())
        .expect("an upload URL");
    let mut put = server.client.put(&transfer.url).body(ciphertext.to_vec());
    for (name, value) in &transfer.headers {
        put = put.header(name, value);
    }
    assert_eq!(put.send().await.expect("authorized PUT").status(), 200);
}

fn commit(
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    pages: &[(String, String)],
    history: String,
) -> garde::Valid<CheckpointCommitInput> {
    valid(CheckpointCommitInput {
        fencing_token: case.run().fencing_token,
        durable_task: task.clone(),
        pin_id: plan.pin.id.clone(),
        manifest_root: checkpoint_digest::manifest_root(
            pages
                .iter()
                .map(|(page, digest)| (page.as_str(), digest.as_str())),
        ),
        history_digest: history,
        covered_input_sequence: 0,
        effect_watermark: 0,
    })
}

async fn history(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
) {
    let checkpoint = id(&plan.checkpoint_id);
    let written = write(
        tasks,
        case,
        task,
        plan,
        page(0, vec![], vec![object.object_id.clone()]),
    )
    .await;
    let plaintext = sha256_hex(HISTORY);
    let root = checkpoint_digest::history_root([(
        object.object_id.as_str(),
        plaintext.as_str(),
        HISTORY.len() as u64,
    )]);
    refused(
        tasks
            .commit_checkpoint(
                case.run(),
                &checkpoint,
                &commit(
                    case,
                    task,
                    plan,
                    std::slice::from_ref(&written),
                    checkpoint_digest::history_root([]),
                ),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "a history root that omits a named chunk is refused",
    );
    let saved = tasks
        .commit_checkpoint(
            case.run(),
            &checkpoint,
            &commit(
                case,
                task,
                plan,
                std::slice::from_ref(&written),
                root.clone(),
            ),
        )
        .await
        .expect("verified history publishes");
    assert_eq!(saved.history_digest.as_deref(), Some(root.as_str()));
    assert!(!saved.replayed);
}

async fn unverified(tasks: &DurableTasks, case: &Case, task: &DecisionDurableTaskVersion) {
    let history_plan = capture(tasks, case, task).await;
    let chunk = register(tasks, case, task, &history_plan, UNSENT).await;
    let written = write(
        tasks,
        case,
        task,
        &history_plan,
        page(0, vec![], vec![chunk.object_id.clone()]),
    )
    .await;
    let plaintext = sha256_hex(UNSENT);
    refused(
        tasks
            .commit_checkpoint(
                case.run(),
                &id(&history_plan.checkpoint_id),
                &commit(
                    case,
                    task,
                    &history_plan,
                    std::slice::from_ref(&written),
                    checkpoint_digest::history_root([(
                        chunk.object_id.as_str(),
                        plaintext.as_str(),
                        UNSENT.len() as u64,
                    )]),
                ),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "history whose chunk never uploaded is not published",
    );

    lapse(case, &history_plan).await;
    let mut request = incremental(case, task, 1);
    request.upload_budget_bytes = 1024;
    request.entry_count = 1;
    request.logical_bytes = FILE.len() as u64;
    let file_plan = planned(tasks, case, request).await;
    let file = register(tasks, case, task, &file_plan, FILE).await;
    let written = write(
        tasks,
        case,
        task,
        &file_plan,
        page(
            0,
            vec![CheckpointEntry::File(CheckpointFile {
                path: "notes.txt".into(),
                executable: false,
                size: FILE.len() as u64,
                objects: vec![file.object_id.clone()],
            })],
            vec![],
        ),
    )
    .await;
    refused(
        tasks
            .commit_checkpoint(
                case.run(),
                &id(&file_plan.checkpoint_id),
                &commit(
                    case,
                    task,
                    &file_plan,
                    std::slice::from_ref(&written),
                    checkpoint_digest::history_root([]),
                ),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "a file whose object never uploaded is not published",
    );
    lapse(case, &file_plan).await;
}

/// Storage reconfigured to another bucket opens no object recorded under the first.
async fn moved(
    fixture: &Decisions<'_>,
    server: &Server,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
) {
    let mut dependencies = fixture.dependencies.clone();
    dependencies.storage = Some(R2CheckpointStore::new(
        valid(CheckpointStorageConfig {
            endpoint: format!("https://{HOST}"),
            bucket: "checkpoint-moved".into(),
            credential_name: "checkpoint-test".into(),
        }),
        case.scope.workspace_id.clone(),
        fixture.keys(),
        server.client.clone(),
    ));
    let elsewhere = DurableTasks::new(case.held.runs.clone(), case.turns.clone(), dependencies);
    refused(
        elsewhere
            .checkpoint_authorizations(
                case.run(),
                &id(&plan.checkpoint_id),
                &confirm(case, task, plan, object),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "an object in the old bucket is never signed for the new one",
    );
}

/// A ciphertext that authenticates but decrypts to bytes other than those registered.
async fn substituted(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    server: &Server,
) {
    let plan = capture(tasks, case, task).await;
    let object = register(tasks, case, task, &plan, DECLARED).await;
    let ciphertext = encrypt(case, &object, REPLACED);
    put(tasks, case, task, &plan, &object, &ciphertext, server).await;
    refused(
        tasks
            .checkpoint_authorizations(
                case.run(),
                &id(&plan.checkpoint_id),
                &confirm(case, task, &plan, &object),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "plaintext other than the registered bytes never verifies",
    );
    let verified: bool = sqlx::query_scalar(
        "SELECT verified_at IS NOT NULL FROM runner.checkpoint_objects WHERE id = $1",
    )
    .bind(id(&object.object_id))
    .fetch_one(&mut *super::connection(case).await)
    .await
    .expect("the substituted object");
    assert!(!verified, "the substituted object stays unverified");

    let narrowed =
        sqlx::query("UPDATE runner.checkpoint_objects SET object_nonce = $2 WHERE id = $1")
            .bind(id(&object.object_id))
            .bind(vec![0_u8; 3])
            .execute(&mut *super::connection(case).await)
            .await
            .expect("the stored nonce is narrowed");
    assert_eq!(narrowed.rows_affected(), 1);
    refused(
        tasks
            .checkpoint_authorizations(
                case.run(),
                &id(&plan.checkpoint_id),
                &confirm(case, task, &plan, &object),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "a stored nonce of the wrong width never opens its key",
    );
}
