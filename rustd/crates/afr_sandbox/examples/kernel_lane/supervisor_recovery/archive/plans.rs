//! Native plan identities and complete captured boundary acknowledgements.
use super::*;
impl Archive {
    pub(super) fn plan(&mut self, body: Value) -> Result<Value, Failed> {
        let input: CheckpointPlanInput = serde_json::from_value(body)?;
        input.validate()?;
        expect(
            input.toolbox_digest == self.config.toolbox,
            "verified real sandbox toolbox",
        )?;
        expect(
            input.task.task_id == TASK && input.task.generation == 1,
            "accepted task generation",
        )?;
        let id = identity(200 + self.plans.len());
        let pages = (0..usize::try_from(input.page_count)?)
            .map(|number| identity(300 + self.pages.len() + number))
            .collect();
        let base = self.base.as_ref().and_then(|base| self.plans.get(base));
        let deadline = wire::deadline();
        let plan = CheckpointPlan {
            checkpoint_id: id.clone(),
            generation: u64::try_from(self.plans.len())? + 1,
            base_id: if input.mode == CheckpointMode::RetainBase {
                Some(id.clone())
            } else {
                self.base.clone()
            },
            mode: input.mode,
            repository: if self.config.mode == super::super::Mode::Api {
                "api-only"
            } else {
                "acme/widget"
            }
            .to_owned(),
            format_version: input.format_version,
            toolbox_digest: input.toolbox_digest,
            page_ids: pages,
            base_page_ids: base.map_or_else(Vec::new, |base| base.page_ids.clone()),
            base_manifest_root: base.and_then(|base| base.manifest_root.clone()),
            base_commit_oid: input.commit_oid,
            manifest_root: None,
            history_digest: None,
            covered_input_sequence: 0,
            effect_watermark: 0,
            expires_at: deadline,
            pin: CheckpointPin {
                id: identity(400 + self.plans.len()),
                resource_version: 1,
                operation_deadline: deadline,
                expires_at: deadline,
            },
            replayed: false,
        };
        if input.mode == CheckpointMode::RetainBase {
            self.base = Some(id.clone());
        } else {
            expect(
                input.base_id == self.base,
                "incremental capture retains original base",
            )?;
        }
        let response = serde_json::to_value(&plan)?;
        self.current = Some(id.clone());
        self.plans.insert(id, plan);
        Ok(response)
    }
    pub(super) fn commit(&mut self, body: Value) -> Result<Value, Failed> {
        let input: CheckpointCommitInput = serde_json::from_value(body)?;
        input.validate()?;
        let plan = self.selected()?;
        expect(input.pin_id == plan.pin.id, "commit native writer custody")?;
        expect(
            input.task.task_id == TASK && input.task.generation == 1,
            "commit accepted task version",
        )?;
        let mut digests = Vec::new();
        let mut histories = Vec::new();
        for id in &plan.page_ids {
            let page = self
                .pages
                .get(id)
                .ok_or("all planned pages must be written")?;
            digests.push((
                id.clone(),
                afd_core::etag::sha256_hex(&serde_json::to_vec(page)?),
            ));
            for object in &page.history_objects {
                let descriptor = self
                    .objects
                    .get(object)
                    .ok_or("registered history object")?;
                histories.push((object.clone(), descriptor.sha256.clone(), descriptor.size));
            }
        }
        expect(
            input.manifest_root
                == afd_crypto::checkpoint_digest::manifest_root(
                    digests
                        .iter()
                        .map(|(id, digest)| (id.as_str(), digest.as_str())),
                ),
            "complete ordered manifest",
        )?;
        expect(
            input.history_digest
                == afd_crypto::checkpoint_digest::history_root(
                    histories
                        .iter()
                        .map(|(id, digest, size)| (id.as_str(), digest.as_str(), *size)),
                ),
            "complete ordered history",
        )?;
        expect(
            (input.covered_input_sequence, input.effect_watermark) == (0, 0),
            "accepted native cursors",
        )?;
        let id = self.current.as_ref().ok_or("current plan identity")?;
        let plan = self.plans.get_mut(id).ok_or("current plan")?;
        plan.manifest_root = Some(input.manifest_root);
        plan.history_digest = Some(input.history_digest);
        plan.covered_input_sequence = input.covered_input_sequence;
        plan.effect_watermark = input.effect_watermark;
        Ok(serde_json::to_value(plan)?)
    }
}
