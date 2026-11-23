-- The draft's thread, held by a Stop or a deletion before its own statements run: an
-- assignment holds the thread from its claim to its commit, so waiting here, in a statement
-- of its own, starts the next statement after that commit, and its snapshot sees the run the
-- assignment issued. The mode the assignment and Stop take it in, so neither blocks a turn's
-- key share.
--
-- $1 draft
SELECT id FROM thread.threads WHERE id = $1 FOR NO KEY UPDATE
