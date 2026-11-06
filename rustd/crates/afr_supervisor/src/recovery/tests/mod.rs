//! Recovery scenarios drive native requests and actual workspace files.
#![expect(clippy::unwrap_used, reason = "test fixtures must fail loudly")]

mod capture;
mod restore;
mod session;

use crate::client::{Call, ControlPlane, Verb};
use crate::test_support::{Answer, Calls, json, plane};
use afd_core::{clock, id::Uuid7};
use afd_wire::task::{
    CHECKPOINT_FORMAT, CheckpointAuthorizationInput, CheckpointAuthorizations,
    CheckpointCommitInput, CheckpointEntry, CheckpointMode, CheckpointObject,
    CheckpointObjectStatus, CheckpointObjectsInput, CheckpointObjectsPage, CheckpointPage,
    CheckpointPageInput, CheckpointPin, CheckpointPlan, CheckpointPlanInput,
    CheckpointUploadsInput, TaskContext, TaskLimits, TaskState, TaskTarget, TaskUsage,
};
use serde::de::DeserializeOwned;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

const ID: &str = "019f0000-0000-7000-8000-000000000001";
const PAGE: &str = "019f0000-0000-7000-8000-000000000002";
const BASE_PAGE: &str = "019f0000-0000-7000-8000-000000000003";
const FENCE: u64 = 31;

fn toolbox() -> String {
    "a".repeat(64)
}

fn task() -> TaskContext<'static> {
    TaskContext {
        task_id: ID.into(),
        originating_item_id: ID.into(),
        target: TaskTarget::ApiOnly,
        generation: 7,
        stop_generation: 0,
        accepted_revision_id: ID.into(),
        objective: "Repair the admitted defect".into(),
        goal_digest: "b".repeat(64).into(),
        questions: vec![],
        state: TaskState::Running,
        covered_input_sequence: 3,
        input_sequence: 3,
        effect_cursor: 2,
        confirmed_effect_watermark: 2,
        checkpoint_id: Some(ID.into()),
        checkpoint_generation: Some(1),
        expires_at: clock::after(clock::now(), Duration::from_hours(1)),
        limits: TaskLimits::default(),
        usage: TaskUsage::default(),
    }
}

fn plan() -> CheckpointPlan {
    let deadline = clock::after(clock::now(), Duration::from_mins(15));
    CheckpointPlan {
        checkpoint_id: ID.to_owned(),
        generation: 1,
        base_id: None,
        mode: CheckpointMode::Incremental,
        repository: "owner/repository".to_owned(),
        format_version: CHECKPOINT_FORMAT,
        toolbox_digest: toolbox(),
        page_ids: vec![PAGE.to_owned()],
        base_page_ids: vec![],
        base_manifest_root: None,
        base_commit_oid: None,
        manifest_root: None,
        history_digest: None,
        covered_input_sequence: 3,
        effect_watermark: 2,
        expires_at: deadline,
        pin: CheckpointPin {
            id: ID.to_owned(),
            resource_version: 1,
            operation_deadline: deadline,
            expires_at: deadline,
        },
        replayed: false,
    }
}

fn page(entries: Vec<CheckpointEntry>) -> CheckpointPage {
    CheckpointPage {
        page_number: 0,
        entries,
        history_objects: vec![],
    }
}

fn root(id: &str, page: &CheckpointPage) -> String {
    let digest = afd_core::etag::sha256_hex(&serde_json::to_vec(page).unwrap());
    afd_crypto::checkpoint_digest::manifest_root([(id, digest.as_str())])
}

fn body<T: DeserializeOwned>(call: &Call) -> T {
    serde_json::from_slice(call.body.as_ref().unwrap()).unwrap()
}

/// The existing daemon seam owns only remote responses; capture, hashing,
/// encryption, page assembly, validation and filesystem writes remain real.
#[derive(Debug)]
struct Archive {
    plan: CheckpointPlan,
    pages: BTreeMap<String, CheckpointPage>,
    objects: BTreeMap<String, serde_json::Value>,
}

impl Archive {
    fn empty() -> Self {
        Self {
            plan: plan(),
            pages: BTreeMap::new(),
            objects: BTreeMap::new(),
        }
    }

    fn saved(entries: Vec<CheckpointEntry>) -> Self {
        let page = page(entries);
        let mut archive = Self::empty();
        archive.plan.manifest_root = Some(root(PAGE, &page));
        archive.plan.history_digest = Some(afd_crypto::checkpoint_digest::history_root(
            std::iter::empty::<(&str, &str, u64)>(),
        ));
        archive.pages.insert(PAGE.to_owned(), page);
        archive
    }

    fn respond(&mut self, call: &Call) -> Answer {
        match call.verb {
            Verb::Activity => json(&serde_json::json!({})),
            Verb::TaskCheckpointPlan => self.planned(call),
            Verb::TaskCheckpointObjectsRegister => self.registered(call),
            Verb::TaskCheckpointPageWrite => self.written(call),
            Verb::TaskCheckpointCommit => self.committed(call),
            Verb::TaskCheckpointUploads => self.uploaded(call),
            Verb::TaskCheckpointAuthorizations => self.authorized(call),
            Verb::TaskCheckpointPage => self.read_page(call),
            Verb::TaskCheckpointPin => self.renewed(call),
            _ => Answer::Fail(crate::error::config("unexpected recovery request")),
        }
    }
    fn planned(&mut self, call: &Call) -> Answer {
        let input: CheckpointPlanInput = body(call);
        assert_eq!(input.fencing_token, FENCE);
        assert_eq!(input.task.generation, 7);
        assert_eq!(input.task.task_id, ID);
        assert_eq!(input.page_count, 1);
        json(&self.plan)
    }

    fn registered(&mut self, call: &Call) -> Answer {
        let input: CheckpointObjectsInput = body(call);
        assert_eq!(input.fencing_token, FENCE);
        assert_eq!(input.pin_id, ID);
        let items: Vec<_> = input
            .objects
            .into_iter()
            .enumerate()
            .map(|(index, plaintext)| CheckpointObject {
                object_id: format!("019f0000-0000-7000-8000-{:012}", index + 10),
                repository: self.plan.repository.clone(),
                plaintext,
                status: CheckpointObjectStatus::Reused,
                nonce: None,
                data_key: None,
                ciphertext_sha256: None,
                ciphertext_length: None,
                transfer: None,
            })
            .collect();
        for object in &items {
            self.objects.insert(
                object.object_id.clone(),
                serde_json::to_value(object).unwrap(),
            );
        }
        json(&CheckpointObjectsPage {
            items,
            next_cursor: None,
        })
    }

    fn written(&mut self, call: &Call) -> Answer {
        let input: CheckpointPageInput = body(call);
        assert_eq!(input.fencing_token, FENCE);
        assert_eq!(input.pin_id, ID);
        assert_eq!(
            input.digest,
            afd_core::etag::sha256_hex(&serde_json::to_vec(&input.contents).unwrap())
        );
        self.pages.insert(PAGE.to_owned(), input.contents.clone());
        json(&input.contents)
    }

    fn committed(&mut self, call: &Call) -> Answer {
        let input: CheckpointCommitInput = body(call);
        assert_eq!(input.fencing_token, FENCE);
        assert_eq!(input.pin_id, ID);
        assert_eq!(input.manifest_root, root(PAGE, &self.pages[PAGE]));
        self.plan.manifest_root = Some(input.manifest_root);
        self.plan.history_digest = Some(input.history_digest);
        self.plan.covered_input_sequence = input.covered_input_sequence;
        self.plan.effect_watermark = input.effect_watermark;
        json(&self.plan)
    }

    fn uploaded(&mut self, call: &Call) -> Answer {
        let input: CheckpointUploadsInput = body(call);
        assert_eq!(input.fencing_token, FENCE);
        assert_eq!(input.pin_id, ID);
        let mut items = Vec::new();
        for descriptor in input.objects {
            let value = self.objects.get_mut(&descriptor.object_id).unwrap();
            value["ciphertext_sha256"] = descriptor.ciphertext_sha256.into();
            value["ciphertext_length"] = descriptor.ciphertext_length.into();
            value["status"] = "reused".into();
            items.push(serde_json::from_value::<CheckpointObject>(value.clone()).unwrap());
        }
        json(&CheckpointObjectsPage {
            items,
            next_cursor: None,
        })
    }

    fn authorized(&mut self, call: &Call) -> Answer {
        let input: CheckpointAuthorizationInput = body(call);
        assert_eq!(input.fencing_token, FENCE);
        let objects = input
            .object_ids
            .iter()
            .map(|id| serde_json::from_value(self.objects[id].clone()).unwrap())
            .collect();
        json(&CheckpointAuthorizations {
            plan: self.plan.clone(),
            objects,
        })
    }

    fn read_page(&mut self, call: &Call) -> Answer {
        let id = call
            .path
            .split('?')
            .next()
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap();
        assert!(call.path.contains("fencing_token=31"));
        json(&self.pages[id])
    }

    fn renewed(&mut self, _call: &Call) -> Answer {
        self.plan.pin.resource_version += 1;
        self.plan.pin.expires_at = self.plan.pin.operation_deadline;
        json(&self.plan.pin)
    }
}

fn remote(
    archive: Archive,
    intercept: impl Fn(&Call, &mut Archive) -> Option<Answer> + Send + Sync + 'static,
) -> (ControlPlane, Calls, Arc<Mutex<Archive>>) {
    let saved = Arc::new(Mutex::new(archive));
    let remote = Arc::clone(&saved);
    let (plane, calls) = plane(move |call| {
        let mut state = remote.lock().unwrap();
        intercept(call, &mut state).unwrap_or_else(|| state.respond(call))
    });
    (plane, calls, saved)
}

fn config_failure(error: &crate::Error, expected: &str) {
    assert!(
        matches!(error.kind(), crate::error::ErrorKind::Config { detail } if *detail == expected),
        "{error}"
    );
    assert!(!error.is_retryable());
}

fn phases(calls: &[Call]) -> Vec<String> {
    calls
        .iter()
        .filter(|call| call.verb == Verb::Activity)
        .map(|call| {
            let value: serde_json::Value = body(call);
            value["frames"][0]["task_transfer_progress"]["phase"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn id() -> Uuid7 {
    Uuid7::parse(ID).unwrap()
}
