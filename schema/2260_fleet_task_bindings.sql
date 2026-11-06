-- The association is assigned in the same binding transaction as the owner.
ALTER TABLE fleet.task_items ADD CONSTRAINT fk_task_items_task_scope
    FOREIGN KEY (task_id, fleet_id, workspace_id)
    REFERENCES fleet.tasks (id, fleet_id, workspace_id)
    DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE fleet.admissions
    ADD COLUMN task_id UUID,
    ADD COLUMN task_item_id UUID,
    ADD COLUMN input_claim_lease_id UUID,
    ADD COLUMN input_claim_fence BIGINT,
    ADD CONSTRAINT fk_admissions_task_scope FOREIGN KEY (task_id, fleet_id, workspace_id)
        REFERENCES fleet.tasks (id, fleet_id, workspace_id) ON DELETE SET NULL (task_id),
    ADD CONSTRAINT fk_admissions_task_item_scope FOREIGN KEY (task_item_id, fleet_id, workspace_id)
        REFERENCES fleet.task_items (id, fleet_id, workspace_id) ON DELETE SET NULL (task_item_id);

-- Settled leases outlive a fleet purge; retain only the opaque task identity.
-- Active lease/task ownership is proven against the current fleet slot and
-- task row inside the native mutation transaction, as for existing lease data.
ALTER TABLE runner.leases ADD COLUMN task_id UUID;
