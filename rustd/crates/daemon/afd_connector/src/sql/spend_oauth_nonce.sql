DELETE FROM connector.oauth_nonces
WHERE provider = $1
  AND nonce_digest = $2
  AND expires_at > clock_timestamp()
RETURNING id
