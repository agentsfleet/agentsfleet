-- One object the recovery head $1, or a generation its pages are a delta through, names.
WITH RECURSIVE chain (id) AS (
    SELECT $1::uuid
    UNION ALL
    SELECT c.parent_checkpoint_id
    FROM runner.checkpoints c JOIN chain ON c.id = chain.id
    WHERE c.parent_checkpoint_id IS NOT NULL
)
SELECT o.id, o.content_identity, o.plaintext_digest, o.plaintext_size, o.kind, o.format_version,
       o.owner_checkpoint_id, o.owner_run_id, o.owner_fence, o.bucket_reference, o.cipher, o.ciphertext_digest, o.ciphertext_length,
       o.accounted_bytes, o.verified_at IS NOT NULL AS verified, o.tombstoned_at IS NOT NULL AS tombstoned, o.object_nonce
FROM runner.checkpoint_objects o
JOIN runner.checkpoint_object_refs r ON r.checkpoint_object_id = o.id AND r.workspace_id = o.workspace_id
WHERE r.checkpoint_id IN (SELECT id FROM chain) AND o.workspace_id = $2
ORDER BY o.id LIMIT 1
