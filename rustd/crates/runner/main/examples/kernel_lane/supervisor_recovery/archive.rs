//! External service returns verified reused objects and authenticates complete manifests.
use super::{Config, TASK, wire};
use crate::run::expect;
use af_wire::task::{
    CheckpointAuthorizationInput, CheckpointAuthorizations, CheckpointCommitInput, CheckpointMode,
    CheckpointObject, CheckpointObjectStatus, CheckpointObjectsInput, CheckpointObjectsPage,
    CheckpointPage, CheckpointPageInput, CheckpointPin, CheckpointPlaintext, CheckpointPlan,
    CheckpointPlanInput, CheckpointTransferPurpose,
};
use afr_supervisor::test_util::FENCING;
use axum::http::{Method, Uri};
use garde::Validate as _;
use libtest_mimic::Failed;
use serde_json::{Value, json};
use std::collections::BTreeMap;
mod plans;
#[derive(Debug)]
pub(crate) struct Archive {
    config: Config,
    plans: BTreeMap<String, CheckpointPlan>,
    pages: BTreeMap<String, CheckpointPage>,
    objects: BTreeMap<String, CheckpointPlaintext>,
    current: Option<String>,
    base: Option<String>,
}
impl Archive {
    pub(crate) fn new(config: Config) -> Self {
        Self {
            config,
            plans: BTreeMap::new(),
            pages: BTreeMap::new(),
            objects: BTreeMap::new(),
            current: None,
            base: None,
        }
    }
    pub(crate) fn reply(
        &mut self,
        method: &Method,
        uri: &Uri,
        body: Value,
    ) -> Result<Value, Failed> {
        let path = uri.path();
        if method == Method::GET {
            expect(
                uri.query().is_some_and(|query| {
                    query
                        .split('&')
                        .any(|field| field == format!("fencing_token={FENCING}"))
                }),
                "native read fence",
            )?;
            if path.ends_with("/inputs") {
                return wire::inputs(&self.config);
            }
            if path.ends_with("/effects") {
                return Ok(json!({"effects":[],"next_cursor":null}));
            }
            return self.read_page(uri);
        }
        expect(
            body.get("fencing_token") == Some(&json!(FENCING)),
            "native mutation fence",
        )?;
        if path.ends_with("/decisions") {
            expect(method == Method::POST, "goal reservation method")?;
            return wire::decision(body);
        }
        if path.ends_with("/checkpoints") {
            expect(method == Method::POST, "plan method")?;
            return self.plan(body);
        }
        if path.ends_with("/objects") {
            expect(method == Method::POST, "object registration method")?;
            return self.objects(body);
        }
        if path.ends_with("/authorizations") {
            expect(method == Method::POST, "restore authorization method")?;
            return self.authorize(path, body);
        }
        if method == Method::PUT {
            return self.page(path, body);
        }
        expect(method == Method::PATCH, "commit method")?;
        expect(
            path.ends_with(&format!("/checkpoints/{}", self.selected()?.checkpoint_id)),
            "exact committed checkpoint path",
        )?;
        self.commit(body)
    }
    fn read_page(&self, uri: &Uri) -> Result<Value, Failed> {
        let id = uri.path().rsplit('/').next().ok_or("page identifier")?;
        let plan = self.selected()?;
        expect(
            plan.page_ids.iter().any(|page| page == id),
            "read page belongs to retained base",
        )?;
        expect(
            uri.path()
                .ends_with(&format!("/checkpoints/{}/pages/{id}", plan.checkpoint_id)),
            "exact retained page path",
        )?;
        expect(
            uri.query() == Some(format!("fencing_token={FENCING}&pin_id={}", plan.pin.id).as_str()),
            "exact retained page fence and custody",
        )?;
        Ok(serde_json::to_value(
            self.pages.get(id).ok_or("unknown native page")?,
        )?)
    }
    fn objects(&mut self, body: Value) -> Result<Value, Failed> {
        let input: CheckpointObjectsInput = serde_json::from_value(body)?;
        input.validate()?;
        let plan = self.selected()?;
        expect(
            input.pin_id == plan.pin.id && input.task.task_id == TASK && input.task.generation == 1,
            "object custody",
        )?;
        let repository = plan.repository.clone();
        let mut items = Vec::new();
        for plaintext in input.objects {
            let id = identity(500 + self.objects.len());
            self.objects.insert(id.clone(), plaintext.clone());
            items.push(CheckpointObject {
                object_id: id,
                repository: repository.clone(),
                plaintext,
                status: CheckpointObjectStatus::Reused,
                nonce: None,
                data_key: None,
                ciphertext_sha256: None,
                ciphertext_length: None,
                transfer: None,
            });
        }
        Ok(serde_json::to_value(CheckpointObjectsPage {
            items,
            next_cursor: None,
        })?)
    }
    fn page(&mut self, path: &str, body: Value) -> Result<Value, Failed> {
        let input: CheckpointPageInput = serde_json::from_value(body)?;
        input.validate()?;
        let id = path.rsplit('/').next().ok_or("page identifier")?;
        expect(
            self.selected()?.page_ids.iter().any(|page| page == id),
            "page belongs to current plan",
        )?;
        expect(
            input.pin_id == self.selected()?.pin.id,
            "page writer owns native pin",
        )?;
        expect(
            input.task.task_id == TASK && input.task.generation == 1,
            "page accepted task version",
        )?;
        expect(
            input.digest == af_core::etag::sha256_hex(&serde_json::to_vec(&input.contents)?),
            "page digest matches content",
        )?;
        self.pages.insert(id.to_owned(), input.contents.clone());
        Ok(serde_json::to_value(input.contents)?)
    }
    fn authorize(&self, path: &str, body: Value) -> Result<Value, Failed> {
        let input: CheckpointAuthorizationInput = serde_json::from_value(body)?;
        input.validate()?;
        expect(
            input.purpose == CheckpointTransferPurpose::Restore
                && input.object_ids.is_empty()
                && input.pin_id.is_none(),
            "retained base inspection only",
        )?;
        let plan = self
            .plans
            .get(self.base.as_ref().ok_or("retained base identity")?)
            .ok_or("committed base")?;
        expect(
            input.task.task_id == TASK && input.task.generation == 1,
            "restore accepted task version",
        )?;
        expect(
            path.ends_with(&format!(
                "/checkpoints/{}/authorizations",
                plan.checkpoint_id
            )),
            "exact base authorization path",
        )?;
        expect(
            plan.manifest_root.is_some(),
            "base committed before inspection",
        )?;
        Ok(serde_json::to_value(CheckpointAuthorizations {
            plan: plan.clone(),
            objects: vec![],
        })?)
    }
    fn selected(&self) -> Result<&CheckpointPlan, Failed> {
        self.plans
            .get(self.current.as_ref().ok_or("current plan")?)
            .ok_or_else(|| "stored current plan".into())
    }
}
fn identity(index: usize) -> String {
    format!("019f0000-0000-7000-8000-{index:012}")
}
