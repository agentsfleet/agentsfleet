-- One schema per product area. `audit` is absent: the migrator creates it before any file runs,
-- because its own ledger lives there.
CREATE SCHEMA identity;

CREATE SCHEMA setup;

CREATE SCHEMA fleet;

CREATE SCHEMA runner;

CREATE SCHEMA thread;

CREATE SCHEMA connector;

CREATE SCHEMA model;

CREATE SCHEMA billing;

CREATE SCHEMA vault;

CREATE SCHEMA memory;
