SELECT o.id, o.content_identity, o.plaintext_digest, o.plaintext_size, o.kind, o.format_version,
       o.owner_checkpoint_id, o.owner_run_id, o.owner_fence, o.bucket_reference, o.cipher, o.ciphertext_digest, o.ciphertext_length,
       o.accounted_bytes, o.verified_at IS NOT NULL AS verified, o.tombstoned_at IS NOT NULL AS tombstoned, o.object_nonce
FROM runner.checkpoint_objects o
JOIN runner.checkpoint_object_refs r ON r.checkpoint_object_id=o.id AND r.workspace_id=o.workspace_id
WHERE o.id=ANY($1::uuid[]) AND o.workspace_id=$2 AND r.checkpoint_id=$3 AND o.repository=$4
  AND o.tombstoned_at IS NULL
ORDER BY o.id FOR UPDATE OF o
