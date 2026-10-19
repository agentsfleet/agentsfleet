UPDATE vault.secrets
   SET dek_ciphertext = $3,
       dek_nonce = $4,
       dek_tag = $5,
       secret_nonce = $6,
       secret_ciphertext = $7,
       secret_tag = $8,
       kek_version = $9,
       updated_at = now()
 WHERE workspace_id = $1::uuid AND key_name = $2
