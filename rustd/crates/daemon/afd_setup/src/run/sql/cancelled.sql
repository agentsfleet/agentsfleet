SELECT NOT EXISTS (SELECT 1 FROM setup.drafts WHERE id = $1)
    OR EXISTS (SELECT 1 FROM setup.drafts WHERE id = $1 AND status <> $3)
    OR EXISTS (SELECT 1 FROM thread.turns
                WHERE thread_id = $1 AND sequence = $2 AND status = $4)
