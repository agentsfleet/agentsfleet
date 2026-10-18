import type { SQL } from "bun";
import { expect } from "bun:test";
import { z } from "zod";
import { openCatalogueDatabase } from "./seed";

export const CATALOGUE_DATABASE_URL = Bun.env.TEST_CATALOGUE_DATABASE_URL;
const UNUSED_DATABASE_URL = "postgres://unused@127.0.0.1:1/unused";
const CLOSE_CHECK_ATTEMPTS = 100;
const CLOSE_CHECK_DELAY_MS = 10;
const COMMIT_OPERATION = "commit";

export function openIntegrationDatabase(): SQL {
  return openCatalogueDatabase(CATALOGUE_DATABASE_URL ?? UNUSED_DATABASE_URL);
}

/** Each suite runs in the database owned and migrated by its Rust parent test. */
export async function removeFixtureModels(database: SQL): Promise<void> {
  await database`DELETE FROM model.platform_models`;
}

export async function snapshot(database: SQL): Promise<unknown> {
  return database`SELECT
    (SELECT jsonb_agg(m ORDER BY id) FROM model.platform_models m) AS models,
    (SELECT jsonb_agg(p ORDER BY id) FROM model.platform_model_prices p) AS prices,
    (SELECT resource_version::text FROM model.catalogue_version WHERE id = 1) AS version`;
}

export async function version(database: SQL): Promise<string> {
  const result: unknown = await database`SELECT resource_version::text AS version FROM model.catalogue_version WHERE id = 1`;
  return z.tuple([z.object({ version: z.string() })]).parse(result)[0].version;
}

/** Every backend in the suite's own database except the observer's: none once clients close. */
export const NO_OTHER_CLIENTS = [{ clients: 0 }];

/**
 * PostgreSQL observes the client's socket close after Bun resolves close(). The
 * observer's own backend is excluded, so a baseline taken while an earlier
 * connection was still closing can never be the target.
 */
export async function waitForClientCount(database: SQL, expected: unknown): Promise<void> {
  const clients = () => database`SELECT count(*)::integer AS clients FROM pg_stat_activity
    WHERE datname = current_database() AND pid <> pg_backend_pid()`;
  for (let attempt = 0; attempt < CLOSE_CHECK_ATTEMPTS; attempt++) {
    if (Bun.deepEquals(await clients(), expected)) return;
    await Bun.sleep(CLOSE_CHECK_DELAY_MS);
  }
  expect(await clients()).toEqual(expected);
}

type Target = "model.platform_models" | "model.platform_model_prices" | "model.catalogue_version";
type Operation = "insert" | "delete" | "update" | typeof COMMIT_OPERATION;

/** Real PostgreSQL triggers fail a chosen write, including deferred COMMIT. */
export async function withWriteRefusal(database: SQL, target: Target, operation: Operation, work: () => Promise<void>): Promise<void> {
  const stages: Record<Exclude<Operation, typeof COMMIT_OPERATION>, SQL.Query<unknown>> = {
    insert: database`BEFORE INSERT`, delete: database`BEFORE DELETE`, update: database`BEFORE UPDATE`,
  };
  await database`CREATE FUNCTION model.refuse_catalogue_test_write() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN RAISE EXCEPTION 'catalogue_test_refusal' USING ERRCODE = 'P0001'; END $$`;
  try {
    if (operation === COMMIT_OPERATION) {
      await database`CREATE CONSTRAINT TRIGGER catalogue_test_refusal
        AFTER INSERT ON ${database(target)} DEFERRABLE INITIALLY DEFERRED
        FOR EACH ROW EXECUTE FUNCTION model.refuse_catalogue_test_write()`;
    } else {
      await database`CREATE TRIGGER catalogue_test_refusal ${stages[operation]} ON ${database(target)}
        FOR EACH ROW EXECUTE FUNCTION model.refuse_catalogue_test_write()`;
    }
    await work();
  } finally {
    await database`DROP TRIGGER IF EXISTS catalogue_test_refusal ON ${database(target)}`;
    await database`DROP FUNCTION model.refuse_catalogue_test_write()`;
  }
}
