SELECT id::text, task_identity_key_id::text, content_identity, plaintext_digest, plaintext_size, kind, format_version,
       owner_checkpoint_id::text, owner_lease_id::text, owner_fence, bucket_reference, ciphertext_digest, ciphertext_length,
       accounted_bytes, verified_at IS NOT NULL AS verified, tombstoned_at IS NOT NULL AS tombstoned,
       dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, secret_tag, kek_version, object_nonce
FROM fleet.task_objects
WHERE workspace_id = $1::uuid AND task_identity_key_id = $2::uuid
  AND content_identity = ANY($3::text[]) AND tombstoned_at IS NULL
ORDER BY id FOR UPDATE
