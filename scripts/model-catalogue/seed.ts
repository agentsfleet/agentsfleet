import type { Catalogue } from "./catalogue";
import { SQL } from "bun";
import { z } from "zod";
import { catalogueDiff, desiredRows, hasChanges, modelRowsSchema, type CatalogueDiff, type ModelRow } from "./rows";

type QueryClient = Pick<SQL, "file">;
export type SeedResult = { diff: CatalogueDiff; applied: boolean };
const SQL_DIRECTORY = `${import.meta.dir}/sql`;
const VERSION_ROWS = z.array(z.object({ resource_version: z.string().regex(/^\d+$/) })).length(1);
const TIMEOUT_SECONDS = 5;
const POOL_SIZE = 1;

export function openCatalogueDatabase(url: string): SQL {
  return new SQL(url, { max: POOL_SIZE, connectionTimeout: TIMEOUT_SECONDS, idleTimeout: TIMEOUT_SECONDS });
}

/** The singleton lock, catalogue writes and generation increment commit together. */
export async function seedCatalogue(database: SQL, catalogue: Catalogue, apply = false): Promise<SeedResult> {
  const wanted = desiredRows(catalogue);
  if (!apply) return { diff: catalogueDiff(wanted, await readRows(database)), applied: false };
  return database.begin(async (transaction) => {
    VERSION_ROWS.parse(await transaction.file(`${SQL_DIRECTORY}/lock_version.sql`));
    const diff = catalogueDiff(wanted, await readRows(transaction));
    if (!hasChanges(diff)) return { diff, applied: false };
    await writeRows(transaction, wanted);
    return { diff, applied: true };
  });
}

export function seedReport(result: SeedResult): string[] {
  const { diff } = result;
  const lines = [
    ...diff.added.map((key) => `  + ${key}`),
    ...diff.changed.map((key) => `  ~ ${key}`),
    ...diff.deleted.map((key) => `  - ${key}`),
    `${diff.added.length} added; ${diff.changed.length} changed; ${diff.deleted.length} deleted`,
  ];
  return [...lines, result.applied ? "✓ catalogue and generation committed" : "✓ diff only; no rows written"];
}

async function readRows(database: QueryClient): Promise<ModelRow[]> {
  return modelRowsSchema.parse(await database.file(`${SQL_DIRECTORY}/select_rows.sql`));
}

async function writeRows(database: QueryClient, wanted: ModelRow[]): Promise<void> {
  const identities = wanted.map(({ prices: _, ...identity }) => ({ id: Bun.randomUUIDv7(), ...identity }));
  const prices = wanted.flatMap((model) => model.prices.map((price) => ({
    id: Bun.randomUUIDv7(), provider: model.provider, model_name: model.model_name, ...price,
  })));
  await database.file(`${SQL_DIRECTORY}/upsert_models.sql`, [identities]);
  await database.file(`${SQL_DIRECTORY}/upsert_prices.sql`, [prices]);
  await database.file(`${SQL_DIRECTORY}/delete_prices.sql`, [prices]);
  await database.file(`${SQL_DIRECTORY}/delete_models.sql`, [identities]);
  await database.file(`${SQL_DIRECTORY}/bump_version.sql`);
}
