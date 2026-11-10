SELECT EXISTS (SELECT 1 FROM fleet.fleets
    WHERE id = $1 AND workspace_id = $2 AND head_revision_id = $3)
