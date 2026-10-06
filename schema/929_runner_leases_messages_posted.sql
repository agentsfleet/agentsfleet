-- How many interim messages one lease has posted to its thread.
--
-- A run may speak before it answers, through the runner verb that posts to
-- the event's origin channel, and a looping or injected run must not flood
-- that thread. The count lives on the lease so the fence, the count and the
-- cap are one guarded statement: a reclaimed lease starts a new count, and two
-- concurrent posts cannot both take the last slot.
--
-- Zero is every existing lease's true count, because the verb that increments
-- it did not exist before this slot. The cap itself is an application constant
-- (`afd_wire::message_verb::MESSAGES_PER_RUN_MAX`), never a CHECK here.
--
-- Forward migration: schema/610 is frozen history. Idempotent.

ALTER TABLE fleet.runner_leases
    ADD COLUMN IF NOT EXISTS messages_posted INTEGER NOT NULL DEFAULT 0;
