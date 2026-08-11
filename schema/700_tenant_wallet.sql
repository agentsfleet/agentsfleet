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
-- PRIVILEGE: `billing_runtime` owns the grants. `api_runtime`, the role every
-- Hypertext Transfer Protocol handler runs as, holds nothing here — a malformed
-- query in an unelevated handler cannot move a balance; the paths that may
-- (starter grant, metered debit, balance read, erasure) assume `billing_runtime`
-- for the span of one transaction (schema/110, `WITH INHERIT FALSE, SET TRUE`).
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

-- billing_runtime reads and writes the wallet: the starter grant at signup
-- (`state/signup_bootstrap.zig`), the balance reads, and the exhausted-marker
-- transitions — each elevating for one transaction. api_runtime holds no
-- direct grant; the catalogue test asserts zero rows for it here.
--
-- No DELETE, to anyone. A wallet leaves only with the tenant that owns it,
-- through the `core.tenants` cascade — referential actions run with the table
-- owner's authority, so the purge needs no billing elevation for it
-- (`state/account_teardown.zig` says so at its statement list). The grant was
-- carried over from a draft where erasure deleted the row explicitly; nothing
-- in the tree issues that statement, so it was reach with no caller.
GRANT SELECT, INSERT, UPDATE ON billing.tenant_wallet TO billing_runtime;

-- metering_runtime reaches the wallet directly, composed to exactly what the
-- fenced renew/settle statement issues against it: it reads the balance and
-- updates it. Never INSERT (the starter grant above is the only creator) and
-- never DELETE (the cascade is). See schema/120 for why this is a direct grant
-- rather than a membership.
GRANT SELECT, UPDATE ON billing.tenant_wallet TO metering_runtime;

-- Read-only operator principals never see a balance. Stated explicitly rather
-- than relied upon: these roles hold no grant here, and saying so makes
-- re-widening them a visible edit to this line.
REVOKE ALL ON billing.tenant_wallet FROM ops_readonly_human, ops_readonly_fleet;
