-- The admission ledger: one row per unit of work a producer asked a fleet to
-- run, committed BEFORE the producer is told yes.
--
-- Acceptance used to be a Redis stream entry, and identity was the entry id
-- Redis minted. Lose the stream and both were gone: a retry after loss was
-- indistinguishable from a second request, and a delivery a provider was told
-- "received" for could vanish with nothing recording that it ever arrived.
-- This table inverts that. The row is the acceptance and the row's logical id
-- is the event's identity; the stream entry is a RECEIPT recorded back after
-- the fact, and a row that never got one is re-appended by the replay sweeper.
--
--   producer, producer_key  who asked, and the value that producer REPEATS
--                           when it retries: a webhook's delivery id, a
--                           scheduler's message id, a gate's action, a repair
--                           verification's row. Unique together, so a retry is
--                           a conflict on this index and never a second run.
--                           This is what the Redis `append_once` claim used to
--                           be, now with no expiry and no second key.
--   seq                     a global sequence, read only to spell the logical
--                           event id `<created_at>-<seq>` — the same numeric
--                           shape a stream entry id has, so nothing that
--                           renders, sorts or pages on event ids changed. Not
--                           an identity column: `id` is the row's identity and
--                           this is a tiebreak inside one millisecond.
--   payload_digest          SHA-256 of the entry's actor, type and body. A
--                           producer key reused with a DIFFERENT payload is
--                           LOGGED at warn and answered with the first
--                           admission's event. Not refused: a 4xx is what
--                           stops a provider retrying, and drift is far more
--                           often our bug than theirs.
--   actor, event_type,      the entry as the producer wrote it, kept verbatim
--   request_json,           so a replay appends the same bytes. TEXT rather
--   event_created_at        than JSONB for the body: JSONB normalises key
--                           order and whitespace, and a replayed entry must be
--                           byte-identical to the one the producer was told
--                           about.
--   receipt                 the stream entry id the append answered with, or
--                           NULL until it has: "admitted" is `receipt IS NULL`
--                           and "queued" is its negation.
--   delivered_at            when a runner was handed this event, stamped by the
--                           same lease write that opens the narrative log in
--                           `core.fleet_events`, or NULL until one was.
--
--                           These two timestamps are the table's whole status
--                           vocabulary, and both are NULL TESTS rather than a
--                           `status` text column, which would be a free-text
--                           restatement of them with nothing validating the
--                           spelling.
--
--                           This one duplicates a fact `core.fleet_events`
--                           already holds, and that is the point. "Receipted
--                           but never delivered" is the recovery set after the
--                           queue loses its data — work a producer was told yes
--                           about, whose entry is gone, and which the replay
--                           sweeper's `receipt IS NULL` scan does not see.
--                           Asking that across two tables is a join no index
--                           can bound; here it is one partial index, below.
--   replay_count            how many times the sweeper re-appended this row.
--                           Read, not just written: the sweeper logs it when a
--                           row is re-appended more than once, which is how a
--                           POISON row — one that is appended, never
--                           receipted, and comes back every pass — is visible
--                           as something other than steady throughput.

CREATE TABLE IF NOT EXISTS core.fleet_admissions (
    id               UUID   PRIMARY KEY,
    CONSTRAINT ck_fleet_admissions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    seq              BIGINT GENERATED ALWAYS AS IDENTITY,
    fleet_id         UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    workspace_id     UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    producer         TEXT   NOT NULL,
    producer_key     TEXT   NOT NULL,
    payload_digest   TEXT   NOT NULL,
    actor            TEXT   NOT NULL,
    event_type       TEXT   NOT NULL,
    request_json     TEXT   NOT NULL,
    event_created_at BIGINT NOT NULL,
    receipt          TEXT,
    delivered_at     BIGINT,
    replay_count     BIGINT NOT NULL,
    created_at       BIGINT NOT NULL,
    updated_at       BIGINT NOT NULL,
    CONSTRAINT uq_fleet_admissions_producer_key UNIQUE (producer, producer_key)
);

-- Reader: the replay sweeper, which walks rows that never got a receipt oldest
-- first. Partial on the NULL test so a healthy deployment's index holds almost
-- nothing; the predicate is a NULL test rather than a value literal, so no
-- application constant is mirrored here (RULE STS).
CREATE INDEX IF NOT EXISTS idx_fleet_admissions_unreceipted
    ON core.fleet_admissions (created_at, seq)
    WHERE receipt IS NULL;

-- Readers: the lease path's delivery stamp, and the reconciliation pass that
-- finds accepted work whose queue entry is gone.
--
-- Both ask the same question — which admissions are receipted and not yet
-- delivered — so both ride one partial index, and in a healthy deployment it
-- holds only the work currently in flight.
--
-- The stamp is a point lookup on the logical event id's two integers under one
-- fleet. It keys on `(created_at, seq)` and NOT on `receipt`, because a
-- replayed admission put one logical event on two stream entries while the
-- ledger recorded only the first receipt: a receipt-keyed stamp would miss the
-- delivery of the second entry, leave the row unstamped forever, and have the
-- reconciliation pass re-append work that already ran.
--
-- The pass walks the same index, and leading on `fleet_id` is what lets it
-- group a batch by fleet and ask each stream once where its oldest surviving
-- entry is.
--
-- Both predicates are NULL tests, so no application constant is mirrored here
-- (RULE STS).
CREATE INDEX IF NOT EXISTS idx_fleet_admissions_undelivered
    ON core.fleet_admissions (fleet_id, created_at, seq)
    WHERE receipt IS NOT NULL AND delivered_at IS NULL;

-- Reader: the `ON DELETE CASCADE` from `core.fleets`, which without an index
-- on the referencing side scans this whole table per deleted fleet.
--
-- `(fleet_id)` and not `(fleet_id, created_at)`: no query orders or ranges by
-- a fleet's admissions, so the second column would be bytes written on every
-- insert to serve a read nothing makes.
CREATE INDEX IF NOT EXISTS idx_fleet_admissions_fleet_id
    ON core.fleet_admissions (fleet_id);

-- api_runtime admits on every ingress path and records receipts; the replay
-- sweeper runs in the same process under the same role. Nothing deletes: rows
-- go with their fleet through the cascade.
GRANT SELECT, INSERT, UPDATE ON core.fleet_admissions TO api_runtime;
