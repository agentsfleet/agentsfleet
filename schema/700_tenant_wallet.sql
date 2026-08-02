-- The tenant's credit-pool balance: one row per tenant, and the only row in the
-- system that money moves on.
--
-- Renamed from `billing.tenant_billing`. The retired name said the schema twice
-- and the table not at all — `billing.tenant_billing` reads as "billing
-- information about a tenant", which is what a reader assumes a table of plans,
-- invoices, or payment methods holds. It holds a balance.
--
-- Two-rate metering, both rates in nanos (1 nano = 1/1,000,000,000 United States
-- Dollar): events are free under both postures; stages cost $0.001 under
-- platform-managed and $0.0001 under self-managed. No plan tiers. The rate
-- constants live in state/tenant_billing.zig.
--
-- PRIVILEGE: the grants below land on `billing_runtime`, not on
-- `api_runtime`. Every Hypertext Transfer Protocol handler runs as the latter,
-- so moving a balance requires deliberately assuming the former for the span of
-- one transaction. Before this rebuild `api_runtime` held SELECT, INSERT, UPDATE
-- and DELETE here directly, which meant any handler — and any bug inside one —
-- could move any tenant's money.
--
-- Keyed by its parent, per the pattern stated in
-- `schema/430_tenant_model_selection.sql`: at most one wallet exists per tenant,
-- every statement addresses it by `tenant_id`, and the starter-grant insert
-- arbitrates on that column. The retired shape carried a generated identity
-- key alongside `tenant_id UUID NOT NULL UNIQUE` — two unique indexes over the
-- same sixteen bytes, on the table where a first-touch upsert race would have
-- been a billing incident rather than an inconvenience.

CREATE TABLE IF NOT EXISTS billing.tenant_wallet (
    tenant_id            UUID   PRIMARY KEY REFERENCES core.tenants(id) ON DELETE CASCADE,
    balance_nanos        BIGINT NOT NULL,
    grant_source         TEXT   NOT NULL,
    balance_exhausted_at BIGINT,
    -- When this tenant's promotional free trial ends, in epoch milliseconds.
    -- NULL means open-ended: the trial has no end yet and stage charges stay
    -- zero for this tenant until one is set. Per-tenant rather than a build-time
    -- constant so a trial can end for one account without a release, and so a
    -- date passing can never change pricing for everyone at once.
    free_trial_ends_at   BIGINT,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    -- A numeric floor, not a value set: RULE STS bans frozen string vocabularies
    -- and this is not one. It is the last line of defence for the wallet clamp —
    -- the debit path already applies GREATEST(0, …), so a negative balance here
    -- would mean the clamp was bypassed, and failing the write is better than
    -- recording a debt the product has no concept of.
    CONSTRAINT ck_tenant_wallet_balance_nanos_non_negative CHECK (balance_nanos >= 0)
);

-- `balance_exhausted_at` keeps its domain name: it is the instant the pool hit
-- zero, which is not when the row was last written — a top-up updates the row
-- and leaves the exhaustion instant standing as history.

-- No index. Every reader and every writer addresses this table by `tenant_id`,
-- which is the primary key. The retired `idx_tenant_billing_updated` on
-- (updated_at DESC) had no reader at all: nothing orders wallets by change time,
-- so it was a btree maintained on every debit — the hottest write in the money
-- path — to serve no query.

-- billing_runtime owns the table. api_runtime is a member (schema/110) and must
-- assume this role to reach it; it holds nothing here directly. DELETE is for
-- account erasure, which elevates for the same reason.
GRANT SELECT, INSERT, UPDATE, DELETE ON billing.tenant_wallet TO billing_runtime;

-- Read-only operator principals never see a balance. Stated explicitly rather
-- than relied upon: these roles hold no grant here, and saying so makes
-- re-widening them a visible edit to this line.
REVOKE ALL ON billing.tenant_wallet FROM ops_readonly_human, ops_readonly_fleet;
