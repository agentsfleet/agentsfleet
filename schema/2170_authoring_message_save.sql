-- The report writer reads save_requested to finish the person's accepted save
-- in the same transaction as the terminal answer. Existing messages retain
-- their previous behavior. The message API returns save_error_code when a
-- checked save is refused, while retaining the completed conversation.
ALTER TABLE authoring.messages
    ADD COLUMN save_requested BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN save_error_code TEXT;
