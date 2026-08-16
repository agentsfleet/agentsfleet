-- Disconnect and GitHub drift recovery replace workspace connector bindings.
-- Both paths run as api_runtime, so the role needs the DELETE used by the
-- shared provider/workspace transaction.
GRANT DELETE ON core.connector_installs TO api_runtime;
