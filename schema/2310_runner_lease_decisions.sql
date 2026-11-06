-- Non-secret delivery snapshot of the selected immutable decision revision.
-- This is the profile delivered by this lease, not a second current selection.
-- No foreign key: settled runner history must survive catalogue removal.
-- Physical dispatch must separately prove a live lease, enabled owned profile,
-- current authority and a native cost reservation before resolving a key.
ALTER TABLE runner.leases ADD COLUMN decision_snapshot JSONB;
