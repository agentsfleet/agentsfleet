SELECT id, content_identity, plaintext_digest, plaintext_size, kind, format_version,
       owner_checkpoint_id, owner_run_id, owner_fence, bucket_reference, cipher, ciphertext_digest, ciphertext_length,
       accounted_bytes, verified_at IS NOT NULL AS verified, tombstoned_at IS NOT NULL AS tombstoned, object_nonce
FROM runner.checkpoint_objects
WHERE workspace_id = $1 AND repository = $2
  AND content_identity = ANY($3::text[]) AND tombstoned_at IS NULL
ORDER BY id FOR UPDATE
