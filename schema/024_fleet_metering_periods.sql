-- fleet.metering_periods — the per-renewal billing breakdown. One row per
-- /renew (and one final row per report-settle) for a metered run: the slice's
-- token deltas, its run milliseconds, and how the charge split between the
-- run-time fee and the per-token cost. agentsfleetd owns it; the runner never sees it.
--
-- Why a separate table from the telemetry `stage` row: the per-EVENT total is
-- the single accumulated `core.agent_execution_telemetry` stage row the Usage
-- tab already renders (UNIQUE (event_id, charge_type) means it is updated in
-- place, never multiplied). This table is the slice-by-slice drill-down behind
-- that one number — auditable as "how did this run's debit accrue, renewal by
-- renewal" without bloating the headline telemetry row. The read API exposes an
-- event's periods; the React drill-down is a later spec.
--
-- (event_id, slice_seq) is the natural key: slice_seq is the 1-based ordinal of
-- the renewal/settle within the run, so a re-applied write conflicts rather than
-- duplicating. run_fee_nanos + token_cost_nanos is the slice METERED; charged_nanos
-- is what was actually DEBITED = LEAST(metered, balance_before), so on the slice
-- that exhausts the wallet charged_nanos < run_fee_nanos + token_cost_nanos (the
-- remainder is forgiven by the GREATEST(0,…) wallet clamp). charged_nanos always
-- equals the wallet drain + the accumulated telemetry ledger row, for audit.

CREATE TABLE IF NOT EXISTS fleet.metering_periods (
    uid               UUID   NOT NULL,
    event_id          TEXT   NOT NULL,
    slice_seq         BIGINT NOT NULL,
    d_input_tokens    BIGINT NOT NULL,
    d_cached_tokens   BIGINT NOT NULL,
    d_output_tokens   BIGINT NOT NULL,
    run_ms            BIGINT NOT NULL,
    run_fee_nanos     BIGINT NOT NULL,
    token_cost_nanos  BIGINT NOT NULL,
    charged_nanos     BIGINT NOT NULL,
    created_at        BIGINT NOT NULL,
    CONSTRAINT pk_metering_periods PRIMARY KEY (uid),
    CONSTRAINT ck_metering_periods_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    CONSTRAINT uq_metering_periods_event_slice UNIQUE (event_id, slice_seq)
);

-- No separate index: uq_metering_periods_event_slice is a unique B-tree on
-- (event_id, slice_seq), so it already serves the INSERT conflict check and
-- the per-event drill-down read (GET /v1/.../telemetry, WHERE event_id = $1
-- ORDER BY slice_seq). A second index on the same columns would only waste
-- storage and slow every INSERT.

-- api_runtime: the renew/report path INSERTs a slice; the read endpoint SELECTs.
GRANT SELECT, INSERT ON fleet.metering_periods TO api_runtime;
