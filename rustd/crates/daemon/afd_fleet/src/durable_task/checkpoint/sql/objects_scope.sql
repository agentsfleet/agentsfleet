SELECT o.id, o.checkpoint_identity_key_id, o.content_identity, o.plaintext_digest, o.plaintext_size, o.kind,
       o.format_version, o.owner_checkpoint_id, o.owner_run_id, o.owner_fence, o.bucket_reference,
       o.ciphertext_digest, o.ciphertext_length, o.accounted_bytes,
       o.verified_at IS NOT NULL AS verified, o.tombstoned_at IS NOT NULL AS tombstoned,
       o.dek_ciphertext, o.dek_nonce, o.dek_tag, o.secret_nonce, o.secret_ciphertext, o.secret_tag, o.kek_version, o.object_nonce
FROM runner.checkpoint_objects o JOIN runner.checkpoint_identity_keys k ON k.id = o.checkpoint_identity_key_id AND k.workspace_id = o.workspace_id
WHERE o.id = ANY($1) AND o.workspace_id = $2 AND k.repository = $5
  AND o.tombstoned_at IS NULL
  AND EXISTS (SELECT 1 FROM runner.checkpoint_object_refs r WHERE r.checkpoint_object_id = o.id AND r.workspace_id = o.workspace_id
      AND (r.checkpoint_id = $3 OR r.checkpoint_base_id = $4))
ORDER BY o.id FOR UPDATE OF o
