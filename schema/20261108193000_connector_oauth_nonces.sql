-- One connect's single-use OAuth state nonce, kept as its SHA-256 digest.
-- A table rather than a column: a nonce exists before any install row does and
-- lives only STATE_TTL_SECONDS (afd_connector::registry), ten minutes.
-- Readers: spend_oauth_nonce.sql (provider, nonce_digest, expires_at) and the
-- expiry sweep (expires_at). Spending is DELETE … RETURNING, so exactly one
-- callback sees the row; the row is never updated.
CREATE TABLE connector.oauth_nonces (
    id UUID PRIMARY KEY CONSTRAINT ck_oauth_nonces_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    provider TEXT NOT NULL,
    nonce_digest BYTEA NOT NULL,
    created_at TIMESTAMPTZ(3) NOT NULL,
    expires_at TIMESTAMPTZ(3) NOT NULL
);

CREATE INDEX idx_oauth_nonces_provider_nonce_digest ON connector.oauth_nonces (provider, nonce_digest);

CREATE INDEX idx_oauth_nonces_expires_at ON connector.oauth_nonces (expires_at);

CREATE FUNCTION connector.oauth_nonces_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'oauth_nonces is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_oauth_nonces_refuse_update
    BEFORE UPDATE ON connector.oauth_nonces
    FOR EACH ROW EXECUTE FUNCTION connector.oauth_nonces_refuse_update();

GRANT SELECT, INSERT, DELETE ON connector.oauth_nonces TO api_runtime;
