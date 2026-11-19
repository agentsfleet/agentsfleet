SELECT NOT EXISTS (SELECT 1 FROM setup.drafts WHERE id = $1)
    OR EXISTS (SELECT 1 FROM setup.drafts
                WHERE id = $1
                  AND (cancelled_through_sequence >= $2 OR status <> $3))
