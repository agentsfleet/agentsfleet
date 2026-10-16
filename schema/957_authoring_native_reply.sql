-- Current SDK-native assistant messages are daemon-private replay state, bound
-- to an attempt, dispatch and unchanged conversation digest. They share the
-- result's byte limit, authorization checks and deletion/expiry lifecycle.
-- This contains no provider credentials and is never serialized to a runner.
ALTER TABLE fleet.authoring_dispatches ADD COLUMN IF NOT EXISTS native_reply JSONB;
