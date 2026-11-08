SELECT NOT EXISTS (SELECT 1 FROM authoring.drafts WHERE id = $1::uuid)
    OR EXISTS (SELECT 1 FROM authoring.drafts
                WHERE id = $1::uuid
                  AND (cancelled_through_sequence >= $2 OR status <> $3))
