//! External checkpoint-service fixture models pre-existing content-addressed objects.
//! It proves orchestration and manifest acknowledgement, not ciphertext retention.
use super::{CHECKPOINT, ID, PAGE, fixture};
use af_wire::task::*;
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    None,
    Plan,
    Commit,
}
pub(super) struct Archive {
    pub(super) refusal: Refusal,
    pub(super) plan: Option<CheckpointPlan>,
    pub(super) page: Option<CheckpointPage>,
    pub(super) descriptors: Vec<CheckpointPlaintext>,
    pub(super) decision: Option<String>,
    pub(super) calls: Vec<(String, Value)>,
}
impl Archive {
    pub(super) fn new(refusal: Refusal) -> Self {
        Self {
            refusal,
            plan: None,
            page: None,
            descriptors: vec![],
            decision: None,
            calls: vec![],
        }
    }
    pub(super) fn task(&mut self, path: &str, body: Value) -> (axum::http::StatusCode, Value) {
        use axum::http::StatusCode;
        self.calls.push((path.to_owned(), body.clone()));
        if path.ends_with("/inputs") {
            return (StatusCode::OK, fixture::inputs());
        }
        if path.ends_with("/effects") {
            return (StatusCode::OK, json!({"effects":[],"next_cursor":null}));
        }
        assert_eq!(body.get("fencing_token"), Some(&json!(super::FENCING)));
        if path.ends_with("/decisions") {
            let input: DecisionReservationInput = serde_json::from_value(body).unwrap();
            self.decision = Some(input.operation_id.clone());
            return (StatusCode::OK, fixture::decision(&input));
        }
        if (path.ends_with("/checkpoints") && self.refusal == Refusal::Plan)
            || (path.ends_with(CHECKPOINT) && self.refusal == Refusal::Commit)
        {
            return (StatusCode::CONFLICT, json!({"error_code":"UZ-RUN-011"}));
        }
        let response = if path.ends_with("/checkpoints") {
            self.planned(body)
        } else if path.ends_with("/objects") {
            self.registered(body)
        } else if path.ends_with(PAGE) {
            self.written(body)
        } else if path.ends_with(CHECKPOINT) {
            self.committed(body)
        } else {
            unreachable!("unexpected checkpoint route {path}")
        };
        (StatusCode::OK, response)
    }
    fn planned(&mut self, body: Value) -> Value {
        let input: CheckpointPlanInput = serde_json::from_value(body).unwrap();
        assert_eq!(input.mode, CheckpointMode::Incremental);
        assert_eq!(input.task.task_id, ID);
        assert_eq!(input.task.generation, 7);
        assert_eq!(input.page_count, 1);
        assert_eq!(input.entry_count, 0);
        assert!(input.upload_budget_bytes > 16);
        let plan = fixture::plan(&input);
        let response = serde_json::to_value(&plan).unwrap();
        self.plan = Some(plan);
        response
    }
    fn registered(&mut self, body: Value) -> Value {
        let input: CheckpointObjectsInput = serde_json::from_value(body).unwrap();
        assert_eq!(input.pin_id, ID);
        assert_eq!(input.objects.len(), 1);
        assert_eq!(input.objects.first().unwrap().kind, "history");
        assert!(input.objects.first().unwrap().size > 0);
        self.descriptors = input.objects.clone();
        let items = input
            .objects
            .into_iter()
            .map(|plaintext| CheckpointObject {
                object_id: ID.to_owned(),
                repository: self.plan.as_ref().unwrap().repository.clone(),
                plaintext,
                status: CheckpointObjectStatus::Reused,
                nonce: None,
                data_key: None,
                ciphertext_sha256: None,
                ciphertext_length: None,
                transfer: None,
            })
            .collect();
        serde_json::to_value(CheckpointObjectsPage {
            items,
            next_cursor: None,
        })
        .unwrap()
    }
    fn written(&mut self, body: Value) -> Value {
        let input: CheckpointPageInput = serde_json::from_value(body).unwrap();
        assert_eq!(input.pin_id, ID);
        assert_eq!(
            input.digest,
            af_core::etag::sha256_hex(&serde_json::to_vec(&input.contents).unwrap())
        );
        assert!(input.contents.entries.is_empty());
        assert_eq!(input.contents.history_objects, vec![ID.to_owned()]);
        let response = serde_json::to_value(&input.contents).unwrap();
        self.page = Some(input.contents);
        response
    }
    fn committed(&mut self, body: Value) -> Value {
        let input: CheckpointCommitInput = serde_json::from_value(body).unwrap();
        assert_eq!(input.pin_id, ID);
        let page_digest =
            af_core::etag::sha256_hex(&serde_json::to_vec(self.page.as_ref().unwrap()).unwrap());
        assert_eq!(
            input.manifest_root,
            af_crypto::checkpoint_digest::manifest_root([(PAGE, page_digest.as_str())])
        );
        let history = self.descriptors.first().unwrap();
        assert_eq!(
            input.history_digest,
            af_crypto::checkpoint_digest::history_root([(
                ID,
                history.sha256.as_str(),
                history.size
            )])
        );
        let plan = self.plan.as_mut().unwrap();
        plan.manifest_root = Some(input.manifest_root);
        plan.history_digest = Some(input.history_digest);
        plan.covered_input_sequence = input.covered_input_sequence;
        plan.effect_watermark = input.effect_watermark;
        serde_json::to_value(plan).unwrap()
    }
}
