INSERT INTO runner.checkpoint_objects (id, workspace_id, repository, content_identity, plaintext_digest,
    plaintext_size, kind, format_version, bucket_reference, cipher, accounted_bytes, owner_checkpoint_id,
    owner_run_id, owner_fence, object_nonce, reclaim_after_at, created_at, updated_at)
SELECT n.id, $2, n.repository, n.content_identity, n.plaintext_digest,
    n.plaintext_size, n.kind, n.format_version, $6, n.cipher, n.accounted_bytes, $3, $4, $5,
    n.object_nonce, clock_timestamp() + make_interval(secs => $7), now(), now()
FROM jsonb_to_recordset($1::jsonb) AS n(id uuid, repository text, content_identity text,
    plaintext_digest text, plaintext_size bigint, kind text, format_version integer, cipher text,
    accounted_bytes bigint, object_nonce bytea)
