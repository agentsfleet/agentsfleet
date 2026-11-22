-- One priced provider request or runtime slice, charged by the statement that writes it (below,
-- and the daily rollups' files). No foreign keys: a line outlives what it names, so references are
-- opaque and `fleet_name` is the name at write time. Amounts are nanodollars.
-- By `work_type`: a fleet line sets `fleet_id` and `turn_id`; a setup line sets `run_id`, and
-- `turn_id` to the draft message its run answered.
CREATE TABLE billing.usage_lines (
    id                          UUID   PRIMARY KEY,
    work_type                   TEXT   NOT NULL,
    work_id                     UUID   NOT NULL,
    kind                        TEXT   NOT NULL,
    seq                         BIGINT NOT NULL,
    tenant_id                   UUID   NOT NULL,
    workspace_id                UUID   NOT NULL,
    fleet_id                    UUID,
    fleet_name                  TEXT,
    turn_id                     UUID,
    run_id                      UUID,
    provider                    TEXT   NOT NULL,
    model                       TEXT   NOT NULL,
    posture                     TEXT   NOT NULL,
    requested_at                TIMESTAMPTZ(3) NOT NULL,
    input_token_count           BIGINT NOT NULL,
    cached_input_token_count    BIGINT NOT NULL,
    cache_write_token_count     BIGINT NOT NULL,
    cache_write_5m_token_count  BIGINT NOT NULL,
    cache_write_1h_token_count  BIGINT NOT NULL,
    output_token_count          BIGINT NOT NULL,
    runtime_ms                  BIGINT NOT NULL,
    -- NULL when no catalogue price covered the request; `unpriced_token_count` keeps those
    -- tokens visible rather than free.
    platform_model_price_id     UUID,
    unpriced_token_count        BIGINT NOT NULL,
    charged_nanos               BIGINT NOT NULL,
    created_at                  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_usage_lines_work_type_work_id_kind_seq UNIQUE (work_type, work_id, kind, seq),
    CONSTRAINT ck_usage_lines_non_negative
        CHECK (seq >= 0 AND input_token_count >= 0 AND cached_input_token_count >= 0
            AND cache_write_token_count >= 0 AND cache_write_5m_token_count >= 0
            AND cache_write_1h_token_count >= 0 AND output_token_count >= 0
            AND runtime_ms >= 0 AND unpriced_token_count >= 0 AND charged_nanos >= 0)
);

CREATE INDEX idx_usage_lines_tenant_id_created_at_id
    ON billing.usage_lines (tenant_id, created_at DESC, id DESC);

CREATE INDEX idx_usage_lines_fleet_id_created_at
    ON billing.usage_lines (fleet_id, created_at) WHERE fleet_id IS NOT NULL;

-- INCLUDE lets a turn's cost be summed without visiting the table.
CREATE INDEX idx_usage_lines_fleet_id_turn_id
    ON billing.usage_lines (fleet_id, turn_id) INCLUDE (charged_nanos)
    WHERE fleet_id IS NOT NULL;

-- Only an insert moves the budget, because a line's price never changes. A setup-chat line and a
-- purged fleet write none.
CREATE FUNCTION billing.usage_lines_bump_budget() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO pg_catalog, fleet, billing
AS $$
BEGIN
    INSERT INTO fleet.activity_counters
        (fleet_id, event_count, budget_used_nanos, created_at, updated_at)
    SELECT f.id, 0, NEW.charged_nanos, f.created_at, NEW.created_at
      FROM fleet.fleets f WHERE f.id = NEW.fleet_id
    ON CONFLICT (fleet_id) DO UPDATE
       SET budget_used_nanos = fleet.activity_counters.budget_used_nanos
                             + EXCLUDED.budget_used_nanos,
           updated_at = GREATEST(fleet.activity_counters.updated_at, EXCLUDED.updated_at);
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_usage_lines_bump_budget
    AFTER INSERT ON billing.usage_lines
    FOR EACH ROW WHEN (NEW.fleet_id IS NOT NULL)
    EXECUTE FUNCTION billing.usage_lines_bump_budget();

-- A statement's new lines debit their payers' wallets, past zero if they must: a debt the next
-- grant repays. A line the unique key turned away is not in `inserted`, so a retry charges nothing.
-- Wallets lock in tenant order, last: this trigger's name sorts after the rollups'. `exhausted_at`
-- keeps the first crossing to zero or below until a debit leaves credit. No wallet, no debit.
CREATE FUNCTION billing.usage_lines_debit_wallets() RETURNS trigger
    LANGUAGE plpgsql SET search_path TO pg_catalog, billing
AS $$
BEGIN
    PERFORM 1 FROM billing.wallets w
      WHERE w.tenant_id IN (SELECT line.tenant_id FROM inserted line WHERE line.charged_nanos > 0)
      ORDER BY w.tenant_id FOR NO KEY UPDATE OF w;
    UPDATE billing.wallets w
       SET balance_nanos = w.balance_nanos - spent.nanos, updated_at = now(),
           exhausted_at = CASE WHEN w.balance_nanos - spent.nanos <= 0
                               THEN COALESCE(w.exhausted_at, now()) END
      FROM (SELECT line.tenant_id, sum(line.charged_nanos)::bigint AS nanos FROM inserted line
             WHERE line.charged_nanos > 0 GROUP BY line.tenant_id) spent
     WHERE w.tenant_id = spent.tenant_id;
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_usage_lines_debit_wallets
    AFTER INSERT ON billing.usage_lines REFERENCING NEW TABLE AS inserted
    FOR EACH STATEMENT EXECUTE FUNCTION billing.usage_lines_debit_wallets();

CREATE FUNCTION billing.usage_lines_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'usage_lines is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_usage_lines_refuse_update
    BEFORE UPDATE ON billing.usage_lines
    FOR EACH ROW EXECUTE FUNCTION billing.usage_lines_refuse_update();

GRANT SELECT, INSERT ON billing.usage_lines TO api_runtime;
