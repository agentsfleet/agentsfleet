-- Who decided, in words, captured when the decision was made.
--
-- `resolved_by` is an OIDC subject. It is the right thing to store and the
-- wrong thing to show, and the read has to bridge that somehow. Joining
-- `core.users` at read time works and was measured: on 50,075 gates the page
-- read went from 7 shared buffers and 0.169ms to 157 buffers and 0.410ms,
-- because `uq_users_oidc_subject` is searched once per row -- fifty index
-- searches per page, on every page load, forever. The fleet-name join beside it
-- memoizes (49 hits of 50) because a page is usually one fleet; deciders vary,
-- so that one does not.
--
-- A column pays once, at the decision, which is the rarer event by orders of
-- magnitude. The read returns to 7 buffers because the answer is already in the
-- row.
--
-- It is also the more correct record. A joined name shows who the account is
-- NOW: rename someone and every historical row silently reads differently,
-- delete them and every row they ever decided goes blank. An audit trail says
-- who acted, at the time they acted. That is what a capture stores and what a
-- join cannot.
--
-- Empty is the ordinary absence and the reason there is no NOT NULL constraint
-- to argue about: `resolved_by` is `''` while a gate is pending, the daemon's
-- own sentinels are not people, and a subject that never signed up on this
-- deployment has no row to name. The DEFAULT is the empty string as a
-- STRUCTURAL absence, not a vocabulary value -- there is no application
-- constant it could drift from (RULE STS carve-out).
ALTER TABLE core.fleet_approval_gates
    ADD COLUMN IF NOT EXISTS resolved_by_name TEXT NOT NULL DEFAULT '';

-- Rows decided before this slot existed. One pass, matched on the same pair the
-- write path will use from here: the subject, and the tenant that owns the
-- gate's fleet. A decider this deployment has no user row for keeps the DEFAULT
-- and renders as the shortened subject, exactly as it did before the column.
UPDATE core.fleet_approval_gates g
   SET resolved_by_name = u.display_name
  FROM core.fleets z, core.users u
 WHERE z.id = g.fleet_id
   AND u.oidc_subject = g.resolved_by
   AND u.tenant_id = z.tenant_id
   AND u.display_name IS NOT NULL
   AND g.resolved_by_name = '';
