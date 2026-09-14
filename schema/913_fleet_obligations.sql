-- The delivery ledger: one row per answer this deployment owes a destination,
-- committed in the SAME transaction as the result that produced it.
--
-- `core.fleet_admissions` is this table pointed the other way. There the row is
-- the acceptance and the stream entry is a receipt recorded after the fact;
-- here the row is the OBLIGATION and the queue entry is a receipt recorded
-- after the fact. Both exist for the same reason: a stream entry is not a
-- durable record of intent, so anything whose loss would strand work has to be
-- a row first and an entry second.
--
-- Before this table the answer went straight onto the queue, and the window
-- between the result committing and the append landing had no record in it. A
-- process that died there left a run committed, charged and answered, with
-- nothing anywhere saying a delivery was owed — not in the queue, which never
-- got the entry, and not in PostgreSQL, which recorded only that the run
-- finished. The tenant had paid for an answer nothing would ever send.
--
--   provider                which connector carries the answer back, as opaque
--                           text. Never matched against a literal here: the
--                           poster registry is the application's vocabulary and
--                           a spelling pinned in schema would drift from it
--                           silently (RULE STS).
--   event_id                the event the question arrived on, which is where
--                           the answer is threaded. TEXT rather than UUID
--                           because it is the logical `<created_at>-<seq>`
--                           spelling `core.fleet_admissions` mints, not a row
--                           identity.
--   answer                  what to say, verbatim. TEXT and not JSONB for the
--                           reason the admission's `request_json` is: a
--                           redelivery must present the same bytes the first
--                           attempt did, and JSONB normalises key order and
--                           whitespace.
--   receipt                 the queue entry id the append answered with, or
--                           NULL until it has. "Owed and not yet queued" is
--                           `receipt IS NULL`, which is exactly the set the
--                           producer re-appends after the datastore loses its
--                           contents.
--   delivered_at            when a poster reported the destination accepted
--                           this answer, or NULL until one did. The queue
--                           entry being acknowledged is NOT this: an ack says
--                           the worker is done with the entry, and the entry
--                           can be dropped for reasons that delivered nothing.
--
--                           These two timestamps are the whole status
--                           vocabulary, both NULL tests rather than a `status`
--                           text column — which would be a free-text
--                           restatement of them with nothing validating the
--                           spelling (RULE STS).
--   attempt_count           how many times a poster has carried this row. Read,
--                           not merely written: a row whose count climbs while
--                           `delivered_at` stays NULL is a destination refusing
--                           an answer it will never take, and that is visible
--                           here as something other than steady throughput.

CREATE TABLE IF NOT EXISTS core.fleet_obligations (
    id             UUID   PRIMARY KEY,
    CONSTRAINT ck_fleet_obligations_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    seq            BIGINT GENERATED ALWAYS AS IDENTITY,
    fleet_id       UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    workspace_id   UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    provider       TEXT   NOT NULL,
    event_id       TEXT   NOT NULL,
    answer         TEXT   NOT NULL,
    receipt        TEXT,
    delivered_at   BIGINT,
    attempt_count  BIGINT NOT NULL,
    created_at     BIGINT NOT NULL,
    updated_at     BIGINT NOT NULL,
    -- One answer per event, so the terminal report's insert is idempotent under
    -- replay. A runner that never saw its first response re-sends the same
    -- report; the settle already answers that with `AlreadySettled` and charges
    -- nothing, and this constraint is what makes the obligation half agree —
    -- the repeat conflicts here instead of owing a second delivery of the same
    -- answer to the same thread.
    CONSTRAINT uq_fleet_obligations_event UNIQUE (fleet_id, event_id)
);

-- Reader: the producer, which walks obligations that never got a receipt,
-- oldest first, and appends them to the queue.
--
-- Partial on the NULL test, so a healthy deployment's index holds almost
-- nothing — the steady state is an obligation receipted microseconds after it
-- is committed. It fills exactly when the queue is unreachable or was lost,
-- which is when this index is the recovery set. The predicate is a NULL test,
-- so no application constant is mirrored here (RULE STS).
CREATE INDEX IF NOT EXISTS idx_fleet_obligations_unreceipted
    ON core.fleet_obligations (created_at, seq)
    WHERE receipt IS NULL;

-- Reader: the recovery pass that finds answers queued but never delivered —
-- the set a lost consumer group, a lost stream, or a worker replaced mid-flight
-- leaves behind.
--
-- Leads on `fleet_id` because delivery order is promised PER DESTINATION, so
-- the pass groups a batch by fleet and re-offers each fleet's owed answers in
-- `(created_at, seq)` order. A global ordering would be a stronger promise than
-- the design makes and a slower index to maintain.
--
-- Both predicates are NULL tests (RULE STS).
CREATE INDEX IF NOT EXISTS idx_fleet_obligations_undelivered
    ON core.fleet_obligations (fleet_id, created_at, seq)
    WHERE receipt IS NOT NULL AND delivered_at IS NULL;

-- Reader: the `ON DELETE CASCADE` from `core.fleets`, which without an index on
-- the referencing side scans this whole table per deleted fleet.
CREATE INDEX IF NOT EXISTS idx_fleet_obligations_fleet_id
    ON core.fleet_obligations (fleet_id);

-- api_runtime commits the obligation on the report path, appends receipts from
-- the producer, and stamps delivery from the outbound worker — all three run in
-- the daemon under this one role, as the replay sweeper does for admissions.
-- Nothing deletes: rows go with their fleet through the cascade.
GRANT SELECT, INSERT, UPDATE ON core.fleet_obligations TO api_runtime;
