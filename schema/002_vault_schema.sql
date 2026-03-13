-- UseZombie clean-state vault schema and role separation

CREATE SCHEMA vault;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vault_accessor') THEN
        CREATE ROLE vault_accessor;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'api_accessor') THEN
        CREATE ROLE api_accessor;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'worker_accessor') THEN
        CREATE ROLE worker_accessor;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'callback_accessor') THEN
        CREATE ROLE callback_accessor;
    END IF;
END
$$;

GRANT USAGE ON SCHEMA public TO api_accessor;
GRANT SELECT, INSERT, UPDATE ON
    runs, specs, workspaces, tenants, policy_events,
    run_transitions, artifacts, usage_ledger, workspace_memories
TO api_accessor;

GRANT USAGE ON SCHEMA vault TO vault_accessor;
GRANT api_accessor TO worker_accessor;
GRANT vault_accessor TO worker_accessor;
GRANT vault_accessor TO callback_accessor;

DROP TABLE public.secrets;

CREATE TABLE vault.secrets (
    id            BIGSERIAL PRIMARY KEY,
    workspace_id  UUID    NOT NULL REFERENCES public.workspaces(workspace_id),
    key_name      TEXT    NOT NULL,
    kek_version   INTEGER NOT NULL DEFAULT 1,
    encrypted_dek BYTEA   NOT NULL,
    dek_nonce     BYTEA   NOT NULL,
    dek_tag       BYTEA   NOT NULL,
    nonce         BYTEA   NOT NULL,
    ciphertext    BYTEA   NOT NULL,
    tag           BYTEA   NOT NULL,
    created_at    BIGINT  NOT NULL,
    updated_at    BIGINT  NOT NULL,
    UNIQUE (workspace_id, key_name)
);

CREATE INDEX idx_vault_secrets_workspace
    ON vault.secrets(workspace_id, key_name);

ALTER TABLE vault.secrets ENABLE ROW LEVEL SECURITY;

GRANT SELECT, INSERT, UPDATE ON vault.secrets TO worker_accessor;
GRANT SELECT, INSERT, UPDATE ON vault.secrets TO callback_accessor;
GRANT USAGE, SELECT ON SEQUENCE vault.secrets_id_seq TO worker_accessor;
GRANT USAGE, SELECT ON SEQUENCE vault.secrets_id_seq TO callback_accessor;
