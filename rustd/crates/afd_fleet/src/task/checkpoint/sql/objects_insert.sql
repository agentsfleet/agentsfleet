INSERT INTO fleet.task_objects (id, workspace_id, task_identity_key_id, content_identity, plaintext_digest,
    plaintext_size, kind, format_version, bucket_reference, accounted_bytes, owner_checkpoint_id,
    owner_lease_id, owner_fence, dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext,
    secret_tag, kek_version, object_nonce, reclaim_after_at, created_at, updated_at)
SELECT n.id, $2::uuid, n.task_identity_key_id, n.content_identity, n.plaintext_digest,
    n.plaintext_size, n.kind, n.format_version, $6, n.accounted_bytes, $3::uuid, $4::uuid, $5,
    n.dek_ciphertext, n.dek_nonce, n.dek_tag, n.secret_nonce, n.secret_ciphertext,
    n.secret_tag, n.kek_version, n.object_nonce, clock_timestamp() + make_interval(secs => $7), now(), now()
FROM jsonb_to_recordset($1::jsonb) AS n(id uuid, task_identity_key_id uuid, content_identity text,
    plaintext_digest text, plaintext_size bigint, kind text, format_version integer, accounted_bytes bigint,
    dek_ciphertext bytea, dek_nonce bytea, dek_tag bytea, secret_nonce bytea, secret_ciphertext bytea,
    secret_tag bytea, kek_version integer, object_nonce bytea)
