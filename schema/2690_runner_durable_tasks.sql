-- Canonical default: afd_wire::runner::DURABLE_TASKS_DEFAULT_ENABLED.
-- Existing hosts start enabled; software capability still fences old workers.
-- This controls new lease eligibility, never authority of an issued lease.
ALTER TABLE runner.runners
    ADD COLUMN durable_tasks_enabled BOOLEAN NOT NULL DEFAULT true;
