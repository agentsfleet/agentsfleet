-- Objects $1 that checkpoint $3, or a generation its pages are a delta through, names.
WITH RECURSIVE chain (id) AS (
    SELECT $3::uuid
    UNION ALL
    SELECT c.parent_checkpoint_id
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT o.id, o.content_identity, o.plaintext_digest, o.plaintext_size, o.kind, o.format_version,
       o.owner_checkpoint_id, o.owner_run_id, o.owner_fence, o.bucket_reference, o.cipher, o.ciphertext_digest, o.ciphertext_length,
       o.accounted_bytes, o.verified_at IS NOT NULL AS verified, o.tombstoned_at IS NOT NULL AS tombstoned, o.object_nonce
FROM runner.checkpoint_objects o
WHERE o.id=ANY($1::uuid[]) AND o.workspace_id=$2 AND o.repository=$4
  AND o.tombstoned_at IS NULL
  AND EXISTS (SELECT 1 FROM runner.checkpoint_object_refs r
      WHERE r.checkpoint_object_id=o.id AND r.workspace_id=o.workspace_id
        AND r.checkpoint_id IN (SELECT id FROM chain))
ORDER BY o.id FOR UPDATE OF o
