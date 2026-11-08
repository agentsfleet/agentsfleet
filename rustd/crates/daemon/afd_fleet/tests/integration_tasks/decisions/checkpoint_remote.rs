//! Real S3 SDK requests authenticate ciphertext before restoring a manifest.

use af_core::etag::sha256_hex;
use af_crypto::checkpoint_digest;
use af_wire::task::{
    CheckpointAuthorizationInput, CheckpointCommitInput, CheckpointEntry, CheckpointFile,
    CheckpointMode, CheckpointObject, CheckpointObjectStatus, CheckpointObjectsInput,
    CheckpointPage, CheckpointPageInput, CheckpointPlaintext, CheckpointPlan, CheckpointReadQuery,
    CheckpointTransfer, CheckpointTransferPurpose, CheckpointUpload, CheckpointUploadsInput,
    DecisionTaskVersion,
};
use afd_fleet::task::{TaskLifecycle as _, Tasks};

use super::{
    super::fixture::{Case, exercise, id},
    checkpoint::{input, valid},
    checkpoint_server::Server,
    fixture::Decisions,
};

mod config;
mod encryption;
mod reuse;
mod signature;
mod tamper;
pub(super) use config::configured;
use encryption::encrypt;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn checkpoint_real_sdk_verifies_create_only_ciphertext_and_restore_custody() {
    exercise(roundtrip).await;
}

async fn roundtrip(case: &Case) {
    let server = Server::start().await;
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = configured(&fixture, &server).await;
    let plaintext = b"private repair state";
    let (plan, object) = allocate(case, &tasks, &task, plaintext).await;
    let ciphertext = encrypt(case, &object, plaintext);
    upload(case, &tasks, &task, &plan, &object, &ciphertext, &server).await;
    tamper::refuse(case, &tasks, &task, &plan, &object, &server).await;
    confirm(case, &tasks, &task, &plan, &object).await;
    publish(
        case,
        &tasks,
        &task,
        &plan,
        &object.object_id,
        plaintext.len() as u64,
    )
    .await;
    reuse::verify(case, &tasks, &task, &plan, &object).await;
    restore(case, &tasks, &task, &plan, &ciphertext, &server).await;
    fixture.model.finish().await.expect("model cleanup");
    server.finish().await;
}

async fn allocate(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    plaintext: &[u8],
) -> (CheckpointPlan, CheckpointObject) {
    let mut request = input(
        task,
        case,
        CheckpointMode::RetainBase,
        None,
        &"0".repeat(40),
    );
    request.upload_budget_bytes = (plaintext.len() + af_crypto::TAG_LEN) as u64;
    request.logical_bytes = plaintext.len() as u64;
    request.entry_count = 1;
    let plan = tasks
        .plan_checkpoint(case.lease(), &valid(request))
        .await
        .expect("capture plan");
    let checkpoint = id(&plan.checkpoint_id);
    let objects = tasks
        .register_checkpoint_objects(
            case.lease(),
            &checkpoint,
            &valid(CheckpointObjectsInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                pin_id: plan.pin.id.clone(),
                objects: vec![CheckpointPlaintext {
                    sha256: sha256_hex(plaintext),
                    size: plaintext.len() as u64,
                    kind: "file".into(),
                }],
            }),
        )
        .await
        .expect("native encrypted allocation");
    (plan, objects.items.into_iter().next().expect("one object"))
}

async fn upload(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
    ciphertext: &[u8],
    server: &Server,
) {
    let checkpoint = id(&plan.checkpoint_id);
    let captured = af_core::test_util::trace::Capture::install_filtered(
        tracing_subscriber::filter::Targets::new()
            .with_default(tracing::level_filters::LevelFilter::OFF)
            .with_target("aws_sigv4", tracing::level_filters::LevelFilter::TRACE)
            .with_target(
                "afd_fleet::task::storage",
                tracing::level_filters::LevelFilter::TRACE,
            ),
    );
    let uploads = tasks
        .checkpoint_uploads(
            case.lease(),
            &checkpoint,
            &valid(CheckpointUploadsInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                pin_id: plan.pin.id.clone(),
                objects: vec![CheckpointUpload {
                    object_id: object.object_id.clone(),
                    ciphertext_sha256: sha256_hex(ciphertext),
                    ciphertext_length: ciphertext.len() as u64,
                }],
            }),
        )
        .await
        .expect("SDK proves create-only and signs bounded upload");
    assert!(
        captured.events().is_empty(),
        "private signing diagnostics stay off even when SDK tracing is enabled"
    );
    assert!(captured.spans().iter().any(|span| {
        span.name == "checkpoint_storage" && span.field("operation") == Some("authorize_upload")
    }));
    drop(captured);
    let upload = uploads
        .items
        .first()
        .expect("registered upload")
        .transfer
        .as_ref()
        .expect("upload authorization");
    send_upload(server, upload, &object.object_id, ciphertext).await;
}

async fn send_upload(
    server: &Server,
    upload: &CheckpointTransfer,
    object: &str,
    ciphertext: &[u8],
) {
    assert_eq!(
        upload.headers.get("if-none-match").map(String::as_str),
        Some("*")
    );
    assert!(upload.url.contains(object));
    signature::assert_bound(upload);
    let mut put = server.client.put(&upload.url).body(ciphertext.to_vec());
    for (name, value) in &upload.headers {
        put = put.header(name, value);
    }
    assert_eq!(put.send().await.expect("authorized PUT").status(), 200);
    let mut replacement = server.client.put(&upload.url).body(ciphertext.to_vec());
    for (name, value) in &upload.headers {
        replacement = replacement.header(name, value);
    }
    assert_eq!(
        replacement.send().await.expect("second PUT").status(),
        412,
        "immutable ciphertext cannot be replaced"
    );
}

async fn confirm(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    plan: &CheckpointPlan,
    object: &CheckpointObject,
) {
    let checkpoint = id(&plan.checkpoint_id);
    let confirmed = tasks
        .checkpoint_authorizations(
            case.lease(),
            &checkpoint,
            &valid(CheckpointAuthorizationInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                purpose: CheckpointTransferPurpose::Upload,
                pin_id: Some(plan.pin.id.clone()),
                object_ids: vec![object.object_id.clone()],
            }),
        )
        .await
        .expect("native authenticated remote readback");
    assert!(matches!(
        confirmed.objects.first().expect("confirmed object").status,
        CheckpointObjectStatus::Reused
    ));
    assert!(
        confirmed
            .objects
            .first()
            .expect("confirmed object")
            .transfer
            .is_none(),
        "verified ciphertext needs no second PUT"
    );
}

async fn restore(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    plan: &CheckpointPlan,
    ciphertext: &[u8],
    server: &Server,
) {
    let checkpoint = id(&plan.checkpoint_id);
    let restore = tasks
        .checkpoint_authorizations(
            case.lease(),
            &checkpoint,
            &valid(CheckpointAuthorizationInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                purpose: CheckpointTransferPurpose::Restore,
                pin_id: None,
                object_ids: vec![],
            }),
        )
        .await
        .expect("restore bootstrap");
    let objects = tasks
        .checkpoint_objects(
            case.lease(),
            &checkpoint,
            &valid(CheckpointReadQuery {
                fencing_token: case.lease().fencing_token,
                pin_id: restore.plan.pin.id,
                starting_after: None,
            }),
        )
        .await
        .expect("bounded restore object inventory");
    let download = objects
        .items
        .first()
        .expect("registered object")
        .transfer
        .as_ref()
        .expect("read-only restore URL");
    assert_download(server, download, ciphertext).await;
}

async fn assert_download(server: &Server, download: &CheckpointTransfer, ciphertext: &[u8]) {
    assert!(download.headers.is_empty());
    let restored = server
        .client
        .get(&download.url)
        .send()
        .await
        .expect("restore GET")
        .bytes()
        .await
        .expect("restored ciphertext");
    assert_eq!(restored.as_ref(), ciphertext);
    assert_eq!(
        server.objects.lock().expect("remote state").len(),
        1,
        "capability probe was deleted"
    );
}

async fn publish(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    plan: &CheckpointPlan,
    object: &str,
    size: u64,
) {
    let digest = write_page(case, tasks, task, plan, object, size).await;
    let page = plan.page_ids.first().expect("checkpoint manifest page");
    tasks
        .commit_checkpoint(
            case.lease(),
            &id(&plan.checkpoint_id),
            &valid(CheckpointCommitInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                pin_id: plan.pin.id.clone(),
                manifest_root: checkpoint_digest::manifest_root([(page.as_str(), digest.as_str())]),
                history_digest: checkpoint_digest::history_root([]),
                covered_input_sequence: 0,
                effect_watermark: 0,
            }),
        )
        .await
        .expect("verified manifest commit");
}

async fn write_page(
    case: &Case,
    tasks: &Tasks,
    task: &DecisionTaskVersion,
    plan: &CheckpointPlan,
    object: &str,
    size: u64,
) -> String {
    let contents = CheckpointPage {
        page_number: 0,
        entries: vec![CheckpointEntry::File(CheckpointFile {
            path: "repair.txt".into(),
            executable: false,
            size,
            objects: vec![object.to_owned()],
        })],
        history_objects: vec![],
    };
    let digest = sha256_hex(&serde_json::to_vec(&contents).expect("manifest bytes"));
    let page = plan.page_ids.first().expect("checkpoint manifest page");
    tasks
        .write_checkpoint_page(
            case.lease(),
            &id(&plan.checkpoint_id),
            &id(page),
            &valid(CheckpointPageInput {
                fencing_token: case.lease().fencing_token,
                task: task.clone(),
                pin_id: plan.pin.id.clone(),
                digest: digest.clone(),
                contents,
            }),
        )
        .await
        .expect("complete file mapping");
    digest
}
