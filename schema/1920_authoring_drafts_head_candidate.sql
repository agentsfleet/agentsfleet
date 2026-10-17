-- Closes the draft ↔ candidate cycle. Deferred, so a turn inserts its candidate and
-- moves the pointer in one transaction, checked at commit. Candidates go only with
-- their draft, so the pointer never dangles.

ALTER TABLE authoring.drafts
    ADD CONSTRAINT fk_drafts_head_candidate
    FOREIGN KEY (head_candidate_id) REFERENCES authoring.candidates(id)
    DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX idx_drafts_head_candidate_id
    ON authoring.drafts (head_candidate_id) WHERE head_candidate_id IS NOT NULL;
