//! A pin or capture whose time ran out authorizes nothing, and a restore reads only
//! what was frozen.

use af_core::error_code;
use af_wire::durable_task::{
    CheckpointAuthorizationInput, CheckpointMode, CheckpointPinInput, CheckpointReadQuery,
    CheckpointTransferPurpose, DecisionDurableTaskVersion,
};
use afd_db::test_util::mint_id;
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTasks};

use super::{
    super::{
        super::fixture::{Case, exercise, id},
        checkpoint::{complete, input, valid},
        checkpoint_remote::configured,
        checkpoint_server::Server,
        fixture::Decisions,
    },
    Pages, Stored, connection, directory, page, planned, refused,
};

// Value: protects=a lapsed writer pin or capture window writes nothing, an unpinned
// authorization is only a restore bootstrap, a reader pin renews and lapses on its own
// clock, and a restore reads only bytes that still match their frozen digest;
// fails_when=the pin or transfer-deadline guard, the bootstrap-purpose or
// duplicate-object refusal, the reader renewal arm, the unknown-kind refusal, the
// restore cursor, the page digest check or the lapsed-reader refusal is dropped;
// why_new=these custody arms had no case; seam=the production task verbs over live
// Postgres, deadlines and rows moved in the real tables.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn lapsed_pins_and_capture_windows_authorize_nothing_and_a_reader_pin_lapses_alone() {
    exercise(custody).await;
}

async fn custody(case: &Case) {
    let server = Server::start().await;
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let tasks = configured(&fixture, &server).await;
    let plan = planned(
        &tasks,
        case,
        input(
            &task,
            case,
            CheckpointMode::RetainBase,
            None,
            &"0".repeat(40),
        ),
    )
    .await;
    lapsed_writes(&tasks, case, &task, &plan).await;
    let plan = complete(&tasks, case, &task, &plan).await;
    unpinned(&tasks, case, &task, &plan).await;
    let reader = reader(&tasks, case, &task, &plan).await;
    restore_reads(&tasks, case, &task, &plan, &reader).await;
    fixture.model.finish().await.expect("fixture cleanup");
    server.finish().await;
}

/// Moves one deadline column of one row back or forward a day.
async fn shift(case: &Case, statement: &'static str, row: &str) {
    let changed = sqlx::query(statement)
        .bind(id(row))
        .execute(&mut *connection(case).await)
        .await
        .expect("the deadline moves");
    assert_eq!(changed.rows_affected(), 1, "{statement}");
}

const PIN_LAPSES: &str =
    "UPDATE runner.checkpoint_pins SET expires_at = expires_at - interval '1 day' WHERE id = $1";
const PIN_RESTORED: &str =
    "UPDATE runner.checkpoint_pins SET expires_at = expires_at + interval '1 day' WHERE id = $1";
const WINDOW_LAPSES: &str =
    "UPDATE runner.checkpoints SET expires_at = expires_at - interval '1 day' WHERE id = $1";
const WINDOW_RESTORED: &str =
    "UPDATE runner.checkpoints SET expires_at = expires_at + interval '1 day' WHERE id = $1";

async fn try_write(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &af_wire::durable_task::CheckpointPlan,
) -> Result<af_wire::durable_task::CheckpointPage, afd_fleet::Error> {
    let contents = page(0, vec![directory("a")], vec![]);
    tasks
        .write_checkpoint_page(
            case.run(),
            &id(&plan.checkpoint_id),
            &id(plan.page_ids.first().expect("allocated page")),
            &valid(af_wire::durable_task::CheckpointPageInput {
                fencing_token: case.run().fencing_token,
                durable_task: task.clone(),
                pin_id: plan.pin.id.clone(),
                digest: super::digest_of(&contents),
                contents,
            }),
        )
        .await
}

async fn lapsed_writes(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &af_wire::durable_task::CheckpointPlan,
) {
    shift(case, PIN_LAPSES, &plan.pin.id).await;
    refused(
        try_write(tasks, case, task, plan).await,
        error_code::DURABLE_TASK_STALE_STATE,
        "a lapsed writer pin writes no page",
    );
    shift(case, PIN_RESTORED, &plan.pin.id).await;
    shift(case, WINDOW_LAPSES, &plan.checkpoint_id).await;
    refused(
        try_write(tasks, case, task, plan).await,
        error_code::DURABLE_TASK_STALE_STATE,
        "a capture past its transfer window writes no page",
    );
    shift(case, WINDOW_RESTORED, &plan.checkpoint_id).await;
    let unwritten: bool =
        sqlx::query_scalar("SELECT body IS NULL FROM runner.checkpoint_pages WHERE id = $1")
            .bind(id(plan.page_ids.first().expect("allocated page")))
            .fetch_one(&mut *connection(case).await)
            .await
            .expect("the base page");
    assert!(unwritten, "neither refused write froze the page");
}

fn authorization(
    case: &Case,
    task: &DecisionDurableTaskVersion,
    purpose: CheckpointTransferPurpose,
    pin: Option<String>,
    objects: Vec<String>,
) -> CheckpointAuthorizationInput {
    CheckpointAuthorizationInput {
        fencing_token: case.run().fencing_token,
        durable_task: task.clone(),
        purpose,
        pin_id: pin,
        object_ids: objects,
    }
}

async fn unpinned(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &af_wire::durable_task::CheckpointPlan,
) {
    let checkpoint = id(&plan.checkpoint_id);
    refused(
        tasks
            .checkpoint_authorizations(
                case.run(),
                &checkpoint,
                &valid(authorization(
                    case,
                    task,
                    CheckpointTransferPurpose::Upload,
                    None,
                    vec![],
                )),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "an upload authorization is refused: only a restore authorizes",
    );
    let twice = String::from(mint_id());
    refused(
        tasks
            .checkpoint_authorizations(
                case.run(),
                &checkpoint,
                &valid(authorization(
                    case,
                    task,
                    CheckpointTransferPurpose::Restore,
                    None,
                    vec![twice.clone(), twice],
                )),
            )
            .await,
        error_code::DURABLE_TASK_REPLAY_CONFLICT,
        "one object named twice is a conflicting request",
    );
    let readers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runner.checkpoint_pins WHERE checkpoint_id = $1 AND kind = 'reader'",
    )
    .bind(&checkpoint)
    .fetch_one(&mut *connection(case).await)
    .await
    .expect("reader pins");
    assert_eq!(readers, 0, "a refused bootstrap pins nothing");
}

async fn reader(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &af_wire::durable_task::CheckpointPlan,
) -> af_wire::durable_task::CheckpointPin {
    let checkpoint = id(&plan.checkpoint_id);
    let bootstrap = tasks
        .checkpoint_authorizations(
            case.run(),
            &checkpoint,
            &valid(authorization(
                case,
                task,
                CheckpointTransferPurpose::Restore,
                None,
                vec![],
            )),
        )
        .await
        .expect("a ready base bootstraps a restore");
    let pin = bootstrap.plan.pin;
    let renewed = tasks
        .renew_checkpoint_pin(
            case.run(),
            &checkpoint,
            &id(&pin.id),
            &valid(CheckpointPinInput {
                fencing_token: case.run().fencing_token,
                durable_task: task.clone(),
                resource_version: pin.resource_version,
            }),
        )
        .await
        .expect("a reader pin renews");
    assert_eq!(renewed.id, pin.id);
    assert_eq!(renewed.resource_version, pin.resource_version + 1);
    assert_eq!(
        renewed.operation_deadline, pin.operation_deadline,
        "a renewed reader keeps its restore deadline"
    );
    relabel(case, &pin.id, "auditor").await;
    refused(
        tasks
            .renew_checkpoint_pin(
                case.run(),
                &checkpoint,
                &id(&pin.id),
                &valid(CheckpointPinInput {
                    fencing_token: case.run().fencing_token,
                    durable_task: task.clone(),
                    resource_version: renewed.resource_version,
                }),
            )
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "a pin of a kind no transfer names renews nothing",
    );
    relabel(case, &pin.id, "reader").await;
    renewed
}

/// Stores `kind` as the pin's kind.
async fn relabel(case: &Case, pin: &str, kind: &str) {
    let changed = sqlx::query("UPDATE runner.checkpoint_pins SET kind = $2 WHERE id = $1")
        .bind(id(pin))
        .bind(kind)
        .execute(&mut *connection(case).await)
        .await
        .expect("the pin kind is stored");
    assert_eq!(changed.rows_affected(), 1);
}

async fn restore_reads(
    tasks: &DurableTasks,
    case: &Case,
    task: &DecisionDurableTaskVersion,
    plan: &af_wire::durable_task::CheckpointPlan,
    reader: &af_wire::durable_task::CheckpointPin,
) {
    let checkpoint = id(&plan.checkpoint_id);
    refused(
        tasks
            .checkpoint_authorizations(
                case.run(),
                &checkpoint,
                &valid(authorization(
                    case,
                    task,
                    CheckpointTransferPurpose::Restore,
                    Some(reader.id.clone()),
                    vec![mint_id().into()],
                )),
            )
            .await,
        error_code::DURABLE_TASK_NOT_FOUND,
        "an object outside the checkpoint is not found",
    );

    let page_id = plan.page_ids.first().expect("base page").clone();
    let query = valid(CheckpointReadQuery {
        fencing_token: case.run().fencing_token,
        pin_id: reader.id.clone(),
        starting_after: None,
    });
    let after = tasks
        .checkpoint_objects(
            case.run(),
            &checkpoint,
            &valid(CheckpointReadQuery {
                fencing_token: case.run().fencing_token,
                pin_id: reader.id.clone(),
                starting_after: Some(mint_id().into()),
            }),
        )
        .await
        .expect("a page past a cursor");
    assert!(
        after.items.is_empty(),
        "an empty base has nothing past any cursor"
    );
    assert_eq!(after.next_cursor, None);
    let frozen = tasks
        .checkpoint_page(case.run(), &checkpoint, &id(&page_id), &query)
        .await
        .expect("a frozen page reads back");
    assert!(frozen.entries.is_empty());
    let original = Stored::read(case, Pages::Base, &page_id).await;
    let mut altered = original.clone();
    altered.digest = "f".repeat(64);
    altered.replace(case, Pages::Base).await;
    refused(
        tasks
            .checkpoint_page(case.run(), &checkpoint, &id(&page_id), &query)
            .await,
        error_code::DURABLE_TASK_CHECKPOINT_INCOMPATIBLE,
        "a page whose bytes no longer match its digest is never restored",
    );
    original.replace(case, Pages::Base).await;
    let restored = tasks
        .checkpoint_page(case.run(), &checkpoint, &id(&page_id), &query)
        .await
        .expect("the original row reads back again");
    assert_eq!(restored.page_number, 0);

    shift(case, PIN_LAPSES, &reader.id).await;
    refused(
        tasks
            .checkpoint_authorizations(
                case.run(),
                &checkpoint,
                &valid(authorization(
                    case,
                    task,
                    CheckpointTransferPurpose::Restore,
                    None,
                    vec![],
                )),
            )
            .await,
        error_code::DURABLE_TASK_STALE_STATE,
        "a lapsed live reader is not handed out again",
    );
    let readers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runner.checkpoint_pins WHERE checkpoint_id = $1 AND kind = 'reader'",
    )
    .bind(&checkpoint)
    .fetch_one(&mut *connection(case).await)
    .await
    .expect("reader pins");
    assert_eq!(readers, 1, "and no second reader is minted beside it");
}
