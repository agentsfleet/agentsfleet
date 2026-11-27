-- Each committed increment permits one upstream request. A daemon that dies
-- between commit and send leaves an unused reservation counted here.
CREATE TABLE thread.run_web_calls (
    run_id     UUID NOT NULL REFERENCES thread.runs(id) ON DELETE CASCADE,
    vendor     TEXT NOT NULL,
    operation  TEXT NOT NULL,
    call_count BIGINT NOT NULL CHECK (call_count > 0),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    PRIMARY KEY (run_id, vendor, operation)
);

GRANT SELECT, INSERT, UPDATE ON thread.run_web_calls TO api_runtime;
