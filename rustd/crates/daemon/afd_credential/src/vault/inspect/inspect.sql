SELECT key_name,
       dek_ciphertext, dek_nonce, dek_tag, secret_nonce, secret_ciphertext, secret_tag, kek_version,
       encode(sha256(convert_to(jsonb_build_array(
           id, kek_version, encode(sha256(secret_ciphertext), 'hex')
       )::text, 'UTF8')), 'hex') AS resolution_version
  FROM vault.secrets
 WHERE workspace_id = $1::uuid AND key_name = ANY($2::text[])
 ORDER BY key_name
   FOR SHARE
