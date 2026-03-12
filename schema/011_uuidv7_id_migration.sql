-- UseZombie M6_007 hard cutover to UUIDv7-typed IDs.
-- Pre-production contract: fail fast if legacy prefixed IDs are present.

CREATE OR REPLACE FUNCTION assert_uuidv7_text_column_clean(
    p_table_name TEXT,
    p_column_name TEXT
)
RETURNS VOID LANGUAGE plpgsql AS $$
DECLARE
    invalid_count BIGINT;
BEGIN
    EXECUTE format(
        'SELECT COUNT(*) FROM %I WHERE %I IS NOT NULL AND NOT (%I ~* %L)',
        p_table_name,
        p_column_name,
        p_column_name,
        '^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
    )
    INTO invalid_count;

    IF invalid_count > 0 THEN
        RAISE EXCEPTION 'UZ-UUIDV7-010: % has % invalid rows in %.%', invalid_count, p_table_name, p_column_name, p_table_name, p_column_name;
    END IF;
END;
$$;

SELECT assert_uuidv7_text_column_clean('runs', 'run_id');
SELECT assert_uuidv7_text_column_clean('run_transitions', 'run_id');
SELECT assert_uuidv7_text_column_clean('artifacts', 'run_id');
SELECT assert_uuidv7_text_column_clean('usage_ledger', 'run_id');
SELECT assert_uuidv7_text_column_clean('run_side_effects', 'run_id');
SELECT assert_uuidv7_text_column_clean('run_side_effect_outbox', 'run_id');
SELECT assert_uuidv7_text_column_clean('workspace_memories', 'run_id');
SELECT assert_uuidv7_text_column_clean('policy_events', 'run_id');
SELECT assert_uuidv7_text_column_clean('agent_profile_versions', 'profile_version_id');
SELECT assert_uuidv7_text_column_clean('workspace_active_profile', 'profile_version_id');
SELECT assert_uuidv7_text_column_clean('profile_compile_jobs', 'compile_job_id');
SELECT assert_uuidv7_text_column_clean('profile_linkage_audit_artifacts', 'profile_version_id');
SELECT assert_uuidv7_text_column_clean('profile_linkage_audit_artifacts', 'compile_job_id');
SELECT assert_uuidv7_text_column_clean('profile_linkage_audit_artifacts', 'run_id');
SELECT assert_uuidv7_text_column_clean('runs', 'run_snapshot_version');

ALTER TABLE IF EXISTS run_transitions DROP CONSTRAINT IF EXISTS run_transitions_run_id_fkey;
ALTER TABLE IF EXISTS artifacts DROP CONSTRAINT IF EXISTS artifacts_run_id_fkey;
ALTER TABLE IF EXISTS usage_ledger DROP CONSTRAINT IF EXISTS usage_ledger_run_id_fkey;
ALTER TABLE IF EXISTS run_side_effects DROP CONSTRAINT IF EXISTS run_side_effects_run_id_fkey;
ALTER TABLE IF EXISTS run_side_effect_outbox DROP CONSTRAINT IF EXISTS run_side_effect_outbox_run_id_fkey;
ALTER TABLE IF EXISTS workspace_active_profile DROP CONSTRAINT IF EXISTS workspace_active_profile_profile_version_id_fkey;
ALTER TABLE IF EXISTS profile_linkage_audit_artifacts DROP CONSTRAINT IF EXISTS profile_linkage_audit_artifacts_profile_version_id_fkey;
ALTER TABLE IF EXISTS profile_linkage_audit_artifacts DROP CONSTRAINT IF EXISTS profile_linkage_audit_artifacts_compile_job_id_fkey;
ALTER TABLE IF EXISTS profile_linkage_audit_artifacts DROP CONSTRAINT IF EXISTS profile_linkage_audit_artifacts_run_id_fkey;

ALTER TABLE runs ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE run_transitions ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE artifacts ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE usage_ledger ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE run_side_effects ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE run_side_effect_outbox ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE workspace_memories ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE policy_events ALTER COLUMN run_id TYPE UUID USING run_id::uuid;

ALTER TABLE agent_profile_versions ALTER COLUMN profile_version_id TYPE UUID USING profile_version_id::uuid;
ALTER TABLE workspace_active_profile ALTER COLUMN profile_version_id TYPE UUID USING profile_version_id::uuid;
ALTER TABLE profile_compile_jobs ALTER COLUMN compile_job_id TYPE UUID USING compile_job_id::uuid;
ALTER TABLE profile_linkage_audit_artifacts ALTER COLUMN profile_version_id TYPE UUID USING profile_version_id::uuid;
ALTER TABLE profile_linkage_audit_artifacts ALTER COLUMN compile_job_id TYPE UUID USING compile_job_id::uuid;
ALTER TABLE profile_linkage_audit_artifacts ALTER COLUMN run_id TYPE UUID USING run_id::uuid;
ALTER TABLE runs ALTER COLUMN run_snapshot_version TYPE UUID USING run_snapshot_version::uuid;

ALTER TABLE run_transitions
    ADD CONSTRAINT fk_run_transitions_run_id
    FOREIGN KEY (run_id) REFERENCES runs(run_id);
ALTER TABLE artifacts
    ADD CONSTRAINT fk_artifacts_run_id
    FOREIGN KEY (run_id) REFERENCES runs(run_id);
ALTER TABLE usage_ledger
    ADD CONSTRAINT fk_usage_ledger_run_id
    FOREIGN KEY (run_id) REFERENCES runs(run_id);
ALTER TABLE run_side_effects
    ADD CONSTRAINT fk_run_side_effects_run_id
    FOREIGN KEY (run_id) REFERENCES runs(run_id);
ALTER TABLE run_side_effect_outbox
    ADD CONSTRAINT fk_run_side_effect_outbox_run_id
    FOREIGN KEY (run_id) REFERENCES runs(run_id);
ALTER TABLE workspace_active_profile
    ADD CONSTRAINT fk_workspace_active_profile_profile_version
    FOREIGN KEY (profile_version_id) REFERENCES agent_profile_versions(profile_version_id);
ALTER TABLE profile_linkage_audit_artifacts
    ADD CONSTRAINT fk_profile_linkage_profile_version
    FOREIGN KEY (profile_version_id) REFERENCES agent_profile_versions(profile_version_id) ON DELETE RESTRICT;
ALTER TABLE profile_linkage_audit_artifacts
    ADD CONSTRAINT fk_profile_linkage_compile_job
    FOREIGN KEY (compile_job_id) REFERENCES profile_compile_jobs(compile_job_id) ON DELETE RESTRICT;
ALTER TABLE profile_linkage_audit_artifacts
    ADD CONSTRAINT fk_profile_linkage_run_id
    FOREIGN KEY (run_id) REFERENCES runs(run_id) ON DELETE CASCADE;

ALTER TABLE runs DROP CONSTRAINT IF EXISTS ck_runs_run_id_uuidv7;
ALTER TABLE runs ADD CONSTRAINT ck_runs_run_id_uuidv7 CHECK (substring(run_id::text from 15 for 1) = '7');

ALTER TABLE agent_profile_versions DROP CONSTRAINT IF EXISTS ck_agent_profile_versions_uuidv7;
ALTER TABLE agent_profile_versions ADD CONSTRAINT ck_agent_profile_versions_uuidv7 CHECK (substring(profile_version_id::text from 15 for 1) = '7');

ALTER TABLE profile_compile_jobs DROP CONSTRAINT IF EXISTS ck_profile_compile_jobs_uuidv7;
ALTER TABLE profile_compile_jobs ADD CONSTRAINT ck_profile_compile_jobs_uuidv7 CHECK (substring(compile_job_id::text from 15 for 1) = '7');

ALTER TABLE runs DROP CONSTRAINT IF EXISTS ck_runs_snapshot_uuidv7;
ALTER TABLE runs ADD CONSTRAINT ck_runs_snapshot_uuidv7 CHECK (run_snapshot_version IS NULL OR substring(run_snapshot_version::text from 15 for 1) = '7');

CREATE OR REPLACE FUNCTION assert_uuidv7_rollback_allowed()
RETURNS VOID LANGUAGE plpgsql AS $$
DECLARE
    run_rows BIGINT;
    profile_rows BIGINT;
    compile_rows BIGINT;
BEGIN
    SELECT COUNT(*) INTO run_rows FROM runs;
    SELECT COUNT(*) INTO profile_rows FROM agent_profile_versions;
    SELECT COUNT(*) INTO compile_rows FROM profile_compile_jobs;

    IF run_rows > 0 OR profile_rows > 0 OR compile_rows > 0 THEN
        RAISE EXCEPTION 'UZ-UUIDV7-011: rollback blocked once UUIDv7 IDs are persisted';
    END IF;
END;
$$;

DROP FUNCTION IF EXISTS assert_uuidv7_text_column_clean(TEXT, TEXT);
